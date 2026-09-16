use std::path::PathBuf;
use std::sync::Arc;

use parking_lot::Mutex;
use tauri::{AppHandle, Manager, State};

use crate::error::IpcResult;
use crate::events::TauriEventSink;
use crate::state::SettingsState;
use ltk_manager_core::config::Config;
use ltk_manager_core::events::EventSink;
use ltk_manager_core::lcu::champions::ChampionRoster;
use ltk_manager_core::lcu::{
    find_live_client, LcuClient, LcuEvent, LcuObserver, LcuSnapshot, LcuWatch, SinkObserver,
    Snapshotting,
};

/// The one League client watch the app runs, and what it last saw.
///
/// Held for the app's lifetime like the launcher: the watch owns a thread that
/// outlives any command, and a settings change reaches it through
/// [`reconfigure`](Self::reconfigure) rather than by rebuilding it.
pub struct LcuState {
    watch: Mutex<LcuWatch>,
    snapshot: Arc<Mutex<LcuSnapshot>>,
}

impl LcuState {
    /// Start watching the install the settings name.
    pub fn new(app: &AppHandle, config: &Config) -> Self {
        let snapshot = Arc::new(Mutex::new(LcuSnapshot::default()));
        let sink: Arc<dyn EventSink> = Arc::new(TauriEventSink::new(app.clone()));
        let observer = Snapshotting {
            snapshot: Arc::clone(&snapshot),
            next: Box::new(SchedulerObserver {
                app: app.clone(),
                next: Box::new(SinkObserver(sink)),
            }),
        };
        let watch = LcuWatch::start(root_of(config), Arc::new(observer));
        Self {
            watch: Mutex::new(watch),
            snapshot,
        }
    }

    /// Point the watch at the install the new settings name.
    pub fn reconfigure(&self, config: &Config) {
        self.watch.lock().reconfigure(root_of(config));
    }

    /// Stop the watch, for an app that is quitting.
    pub fn shutdown(&self) {
        self.watch.lock().stop();
    }

    pub fn snapshot(&self) -> LcuSnapshot {
        self.snapshot.lock().clone()
    }
}

/// The install root the lockfile lives under.
fn root_of(config: &Config) -> Option<PathBuf> {
    config.league_path.clone()
}

/// Hands each event to the champion select scheduler before passing it on.
///
/// It reaches the scheduler through managed state rather than holding it,
/// because the watch is built before the scheduler is managed and a missing
/// one is a startup moment rather than an error.
struct SchedulerObserver {
    app: AppHandle,
    next: Box<dyn LcuObserver>,
}

impl LcuObserver for SchedulerObserver {
    fn on_event(&self, event: LcuEvent) {
        if let Some(champ_select) = self.app.try_state::<crate::commands::ChampSelectState>() {
            champ_select.observe(&self.app, &event);
        }
        if let LcuEvent::ClientUp { .. } = event {
            refresh_roster(&self.app);
        }
        self.next.on_event(event);
    }
}

/// Ask a live client for the champion roster, off the watching thread.
///
/// The cached copy is what the app starts from, and it goes stale the moment a
/// patch adds a champion. A client that has just answered is the cheapest place
/// to get a fresh one, and failing costs nothing: the cache still names every
/// champion the reader has seen.
fn refresh_roster(app: &AppHandle) {
    let app = app.clone();
    std::thread::spawn(move || {
        let config = app.state::<SettingsState>().config();
        let Some((_, lockfile)) = find_live_client(&root_of(&config)) else {
            return;
        };
        let Some(client) = LcuClient::new(&lockfile) else {
            return;
        };
        let Some(cache_dir) = crate::state::get_app_data_dir(&app) else {
            return;
        };
        let Some(roster) = ChampionRoster::fetch(&client, &cache_dir) else {
            tracing::debug!("The client did not answer with a champion roster");
            return;
        };
        tracing::info!("Champion roster refreshed: {} champions", roster.len());
        if let Some(champ_select) = app.try_state::<crate::commands::ChampSelectState>() {
            champ_select.set_roster(roster);
        }
    });
}

/// What the League client watch last saw.
///
/// What a frontend asks on mount: an event that fired before the webview did
/// announced itself to nobody.
#[tauri::command]
#[specta::specta]
pub fn get_lcu_snapshot(lcu: State<LcuState>) -> IpcResult<LcuSnapshot> {
    IpcResult::ok(lcu.snapshot())
}
