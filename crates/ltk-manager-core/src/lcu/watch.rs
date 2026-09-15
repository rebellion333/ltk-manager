//! Following one League client for as long as it lives.
//!
//! The watch owns a thread. With no live lockfile it sleeps and looks again
//! every ten seconds. With one, it connects the event socket, subscribes to
//! the gameflow phase and the champion select session, reads each once over
//! HTTP so the observer never waits for a first event, and then blocks on the
//! socket. A lost socket is retried, and a client that went away is announced
//! as such and looked for again.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use super::champ_select::{ChampSelectSession, ChampSelectView};
use super::client::LcuClient;
use super::gameflow::GameflowPhase;
use super::lockfile::LeagueLockfile;
use super::socket::{LcuSocket, SocketStopper, event_name_for};

const GAMEFLOW_URI: &str = "/lol-gameflow/v1/gameflow-phase";
const CHAMP_SELECT_URI: &str = "/lol-champ-select/v1/session";

/// How often a closed client is looked for.
const LOOK_INTERVAL: Duration = Duration::from_secs(10);
/// How long a lost socket waits before reconnecting to a live client.
const RECONNECT_DELAY: Duration = Duration::from_secs(2);

/// What the watch reports.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LcuEvent {
    /// A client answered on this port.
    ClientUp { port: u16 },
    /// The client went away, or stopped answering.
    ClientDown,
    /// The gameflow phase, at connect and on every change.
    Gameflow(GameflowPhase),
    /// Champion select began, with the local player's part of it.
    ChampSelectStarted(ChampSelectView),
    /// The local player's part of champion select changed.
    ChampSelectChanged(ChampSelectView),
    /// Champion select is over, whichever way.
    ChampSelectEnded,
}

/// Receives the watch's events, on the watching thread.
pub trait LcuObserver: Send + Sync {
    fn on_event(&self, event: LcuEvent);
}

/// Where the lockfile is looked for. `None` sleeps the watch.
type LeagueRoot = Option<PathBuf>;

struct Shared {
    root: Mutex<LeagueRoot>,
    /// Woken by `stop` and `reconfigure`, so a sleeping watch reacts at once.
    wake: Condvar,
    stopped: AtomicBool,
    stopper: Mutex<Option<SocketStopper>>,
}

/// A running watch. Dropping it stops the thread.
pub struct LcuWatch {
    shared: Arc<Shared>,
    thread: Option<JoinHandle<()>>,
}

impl std::fmt::Debug for LcuWatch {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LcuWatch")
            .field("root", &self.shared.root.lock().unwrap())
            .field("stopped", &self.shared.stopped.load(Ordering::Relaxed))
            .finish()
    }
}

impl LcuWatch {
    /// Start watching the client under `root`.
    pub fn start(root: LeagueRoot, observer: Arc<dyn LcuObserver>) -> Self {
        let shared = Arc::new(Shared {
            root: Mutex::new(root),
            wake: Condvar::new(),
            stopped: AtomicBool::new(false),
            stopper: Mutex::new(None),
        });
        let worker = Arc::clone(&shared);
        let thread = thread::Builder::new()
            .name("lcu-watch".to_string())
            .spawn(move || run(worker, observer))
            .expect("spawn the LCU watch");
        Self {
            shared,
            thread: Some(thread),
        }
    }

    /// Point the watch at another install root, dropping the current client.
    pub fn reconfigure(&self, root: LeagueRoot) {
        let changed = {
            let mut current = self.shared.root.lock().unwrap();
            let changed = *current != root;
            *current = root;
            changed
        };
        if changed {
            self.shared.interrupt();
        }
    }

    /// Stop the thread and wait for it.
    pub fn stop(&mut self) {
        self.shared.stopped.store(true, Ordering::SeqCst);
        self.shared.interrupt();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

impl Drop for LcuWatch {
    fn drop(&mut self) {
        self.stop();
    }
}

impl Shared {
    /// Wake a sleeping loop and end a blocking read.
    fn interrupt(&self) {
        self.wake.notify_all();
        if let Some(stopper) = self.stopper.lock().unwrap().as_ref() {
            stopper.stop();
        }
    }

    fn is_stopped(&self) -> bool {
        self.stopped.load(Ordering::SeqCst)
    }

    /// Sleep for `duration` unless interrupted first.
    fn nap(&self, duration: Duration) {
        let guard = self.root.lock().unwrap();
        let _ = self.wake.wait_timeout(guard, duration).unwrap();
    }

    fn root(&self) -> LeagueRoot {
        self.root.lock().unwrap().clone()
    }
}

/// Where a live lockfile might be, configured install first.
///
/// A player with several installs runs whichever one they queued on, and the
/// lockfile belongs to the client that is running rather than to the install
/// the manager is set up for. Following only the configured root leaves a PBE
/// game followed by nobody, and silent while it happens, which is worse than
/// being wrong out loud: the watch looks healthy and reports nothing.
///
/// The configured root leads, so the ordinary case is one `stat` and no
/// question asked of the Riot Client.
fn candidate_roots(configured: &LeagueRoot, discovered: Vec<PathBuf>) -> Vec<PathBuf> {
    let mut roots: Vec<PathBuf> = configured.clone().into_iter().collect();
    for root in discovered {
        if !roots.contains(&root) {
            roots.push(root);
        }
    }
    roots
}

/// The first candidate whose lockfile names a live client.
///
/// With nothing configured the watch hunts for nothing. No League path means
/// the manager is not set up for League at all, and following a client it was
/// never pointed at would be reaching past what the reader asked for.
fn find_live_client(configured: &LeagueRoot) -> Option<(PathBuf, LeagueLockfile)> {
    configured.as_ref()?;
    let discovered = crate::launcher::install::installed_patchlines()
        .into_iter()
        .map(|patchline| patchline.root)
        .collect();
    candidate_roots(configured, discovered)
        .into_iter()
        .find_map(|root| {
            let lockfile = LeagueLockfile::read(&root).filter(LeagueLockfile::is_live)?;
            Some((root, lockfile))
        })
}

fn run(shared: Arc<Shared>, observer: Arc<dyn LcuObserver>) {
    while !shared.is_stopped() {
        let configured = shared.root();
        let Some((root, lockfile)) = find_live_client(&configured) else {
            shared.nap(LOOK_INTERVAL);
            continue;
        };
        if Some(&root) != configured.as_ref() {
            tracing::info!(
                "The running client is {}, which is not the configured install",
                root.display()
            );
        }

        let Some(client) = LcuClient::new(&lockfile) else {
            shared.nap(LOOK_INTERVAL);
            continue;
        };

        tracing::info!(port = lockfile.port, "Following the League client");
        observer.on_event(LcuEvent::ClientUp {
            port: lockfile.port,
        });

        // The configured root is re-read every pass, so a settings change ends
        // the follow even when the client it names is a different install.
        while !shared.is_stopped()
            && shared.root() == configured
            && lockfile_still_live(&Some(root.clone()))
        {
            match follow_socket(&shared, &lockfile, &client, &observer) {
                FollowEnd::Stopped => break,
                FollowEnd::Lost => shared.nap(RECONNECT_DELAY),
            }
        }

        tracing::info!("The League client went away");
        observer.on_event(LcuEvent::ClientDown);
    }
}

fn lockfile_still_live(root: &LeagueRoot) -> bool {
    root.as_deref()
        .and_then(LeagueLockfile::read)
        .is_some_and(|lf| lf.is_live())
}

enum FollowEnd {
    Stopped,
    Lost,
}

/// One socket's lifetime: connect, subscribe, seed, then relay until it ends.
fn follow_socket(
    shared: &Shared,
    lockfile: &LeagueLockfile,
    client: &LcuClient,
    observer: &Arc<dyn LcuObserver>,
) -> FollowEnd {
    let mut socket = match LcuSocket::connect(lockfile) {
        Ok(socket) => socket,
        Err(e) => {
            tracing::debug!("LCU socket did not connect: {e}");
            return FollowEnd::Lost;
        }
    };
    for uri in [GAMEFLOW_URI, CHAMP_SELECT_URI] {
        if let Err(e) = socket.subscribe(&event_name_for(uri)) {
            tracing::debug!("LCU subscription to {uri} failed: {e}");
            return FollowEnd::Lost;
        }
    }
    *shared.stopper.lock().unwrap() = socket.stopper();
    if shared.is_stopped() {
        return FollowEnd::Stopped;
    }

    let mut tracker = Tracker::default();

    // The socket only says what changes from here on, so the current state is
    // read once over HTTP and reported as if it had just arrived.
    if let Some(phase) = client.get_json::<String>(GAMEFLOW_URI) {
        tracker.gameflow(GameflowPhase::from(phase.as_str()), observer);
    }
    match client.get_json::<ChampSelectSession>(CHAMP_SELECT_URI) {
        Some(session) => tracker.champ_select(Some(session.view()), observer),
        None => tracker.champ_select(None, observer),
    }

    while let Some(frame) = socket.next_event() {
        match frame.uri.as_str() {
            GAMEFLOW_URI => {
                if let Some(phase) = frame.data.as_str() {
                    tracker.gameflow(GameflowPhase::from(phase), observer);
                }
            }
            CHAMP_SELECT_URI => {
                let view = if frame.event_type == "Delete" {
                    None
                } else {
                    serde_json::from_value::<ChampSelectSession>(frame.data)
                        .ok()
                        .map(|s| s.view())
                };
                tracker.champ_select(view, observer);
            }
            _ => {}
        }
    }

    *shared.stopper.lock().unwrap() = None;
    if shared.is_stopped() {
        FollowEnd::Stopped
    } else {
        tracker.socket_lost(observer);
        FollowEnd::Lost
    }
}

/// Turns raw resource updates into the started / changed / ended shape.
#[derive(Default)]
struct Tracker {
    phase: Option<GameflowPhase>,
    champ_select: Option<ChampSelectView>,
}

impl Tracker {
    fn gameflow(&mut self, phase: GameflowPhase, observer: &Arc<dyn LcuObserver>) {
        if self.phase.as_ref() == Some(&phase) {
            return;
        }
        self.phase = Some(phase.clone());
        observer.on_event(LcuEvent::Gameflow(phase));
    }

    fn champ_select(&mut self, view: Option<ChampSelectView>, observer: &Arc<dyn LcuObserver>) {
        match (self.champ_select.take(), view) {
            (None, None) => {}
            (None, Some(view)) => {
                self.champ_select = Some(view.clone());
                observer.on_event(LcuEvent::ChampSelectStarted(view));
            }
            (Some(_), None) => observer.on_event(LcuEvent::ChampSelectEnded),
            (Some(previous), Some(view)) => {
                let worth_saying = view.differs_meaningfully_from(&previous);
                self.champ_select = Some(view.clone());
                if worth_saying {
                    observer.on_event(LcuEvent::ChampSelectChanged(view));
                }
            }
        }
    }

    /// A lost socket means champion select, if any, is no longer being followed.
    fn socket_lost(&mut self, observer: &Arc<dyn LcuObserver>) {
        if self.champ_select.take().is_some() {
            observer.on_event(LcuEvent::ChampSelectEnded);
        }
    }
}

#[cfg(test)]
mod tests;
