//! Keeping the overlay in step with the champion the reader is on.
//!
//! The pieces around this one each answer a question: [`super::decision`]
//! whether a swap may be attempted, [`super::preference`] which mod a champion
//! wants, [`super::swap`] how the overlay is rebuilt. This is what watches, and
//! it holds exactly one idea: **what should be applied**, against **what is**.
//!
//! Champion select moves faster than a rebuild. A reader flicking through
//! champions produces a change every few hundred milliseconds where a rebuild
//! takes about five hundred, so the worker coalesces rather than queues: it
//! reads what is wanted when it starts, and when it finishes it looks again. A
//! flurry of hovers costs one build for the champion the reader landed on, not
//! one per champion they passed.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use crate::lcu::champions::ChampionRoster;
use crate::lcu::{ChampSelectView, GameflowPhase, LcuEvent};
use crate::mods::ChampionPreference;
use crate::patcher::PatcherPhase;

use super::budget::Budget;
use super::decision::{self, Refusal, SwapContext, Verdict};

/// What the overlay should be carrying for the champion in play.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Desired {
    /// The champion's alias, which is the spelling everything else joins on.
    pub alias: String,
    /// The mod to apply, or `None` to leave the champion unmodded.
    pub mod_id: Option<String>,
}

/// The champion a swap is about, once champion select has one.
///
/// The lock when there is one, the hover before that. A hover is worth acting
/// on rather than waiting for the lock: measured across four modes, the
/// champion changed after the lock in half the games, so the lock is not the
/// commitment it looks like, and building early costs nothing that building
/// late would not cost again.
fn champion_in_play(view: &ChampSelectView) -> Option<i32> {
    view.locked_champion_id.or(view.hovered_champion_id)
}

/// What should be applied for the champion select as it stands.
///
/// `None` when there is no champion yet, or when the roster cannot name the one
/// there is - a champion released after this install's cached roster is one the
/// app cannot categorize mods for either, so there is nothing to apply.
pub fn desired_for(
    view: &ChampSelectView,
    roster: &ChampionRoster,
    preferences: &HashMap<String, ChampionPreference>,
) -> Option<Desired> {
    let id = champion_in_play(view)?;
    let alias = roster.by_id(id)?.alias.clone();
    let mod_id = preferences
        .get(&alias)
        .and_then(|preference| preference.preferred.clone());
    Some(Desired { alias, mod_id })
}

/// Applies what the scheduler decides, and reports what the patcher is doing.
///
/// A trait rather than the library itself, so the loop below can be tested
/// without a game, an overlay or a profile on disk.
pub trait Swapper: Send + Sync {
    /// The patcher's phase, and whether a game attached in this session.
    fn patcher_status(&self) -> (PatcherPhase, bool);
    /// Make `desired` true. Returns how long the rebuild took.
    fn apply(&self, desired: &Desired) -> crate::error::AppResult<Duration>;
    /// What the scheduler concluded, for the interface to draw.
    fn report(&self, report: Report);
}

/// What the scheduler did, or why it did not.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Report {
    /// The overlay now carries this champion's mod.
    Applied { desired: Desired, took: Duration },
    /// The swap was not attempted, and this is what the reader loses by it.
    Refused { desired: Desired, why: Refusal },
    /// The rebuild itself failed, so the overlay is whatever it was.
    Failed { desired: Desired, error: String },
}

struct State {
    budget: Budget,
    gameflow: GameflowPhase,
    champ_select: Option<ChampSelectView>,
    roster: ChampionRoster,
    preferences: HashMap<String, ChampionPreference>,
    /// What the overlay should carry, and what it last was made to carry.
    wanted: Option<Desired>,
    applied: Option<Desired>,
    building: bool,
}

impl State {
    /// What is left to do, if anything.
    fn outstanding(&self) -> Option<Desired> {
        let wanted = self.wanted.as_ref()?;
        if self.applied.as_ref() == Some(wanted) {
            return None;
        }
        Some(wanted.clone())
    }

    /// Recompute what is wanted from whatever champion select now says.
    fn refresh_wanted(&mut self) {
        self.wanted = self
            .champ_select
            .as_ref()
            .and_then(|view| desired_for(view, &self.roster, &self.preferences));
    }
}

struct Shared {
    state: Mutex<State>,
    /// Woken when something might have become worth doing.
    wake: Condvar,
    stopped: AtomicBool,
}

/// Watches champion select and keeps the overlay in step with it.
pub struct Scheduler {
    shared: Arc<Shared>,
    thread: Option<JoinHandle<()>>,
}

impl Scheduler {
    /// Start the worker.
    pub fn start(roster: ChampionRoster, swapper: Arc<dyn Swapper>) -> Self {
        let shared = Arc::new(Shared {
            state: Mutex::new(State {
                budget: Budget::default(),
                gameflow: GameflowPhase::Nothing,
                champ_select: None,
                roster,
                preferences: HashMap::new(),
                wanted: None,
                applied: None,
                building: false,
            }),
            wake: Condvar::new(),
            stopped: AtomicBool::new(false),
        });
        let worker = Arc::clone(&shared);
        let thread = thread::Builder::new()
            .name("champ-select-scheduler".to_string())
            .spawn(move || run(worker, swapper))
            .expect("spawn the champion select scheduler");
        Self {
            shared,
            thread: Some(thread),
        }
    }

    /// Feed one thing the League client said.
    pub fn observe(&self, event: &LcuEvent) {
        {
            let mut state = self.shared.state.lock().unwrap();
            match event {
                LcuEvent::Gameflow(phase) => state.gameflow = phase.clone(),
                LcuEvent::ChampSelectStarted(view) | LcuEvent::ChampSelectChanged(view) => {
                    state.champ_select = Some(view.clone());
                    state.refresh_wanted();
                }
                LcuEvent::ChampSelectEnded => {
                    state.champ_select = None;
                    state.wanted = None;
                    // The next champion select is a fresh question, and the
                    // overlay it will be asked about is whatever this game left.
                    state.applied = None;
                }
                LcuEvent::ClientDown => {
                    state.champ_select = None;
                    state.wanted = None;
                    state.applied = None;
                }
                LcuEvent::ClientUp { .. } => {}
            }
        }
        self.shared.wake.notify_all();
    }

    /// Point the scheduler at the preferences the active profile holds.
    pub fn set_preferences(&self, preferences: HashMap<String, ChampionPreference>) {
        {
            let mut state = self.shared.state.lock().unwrap();
            state.preferences = preferences;
            state.refresh_wanted();
        }
        self.shared.wake.notify_all();
    }

    /// Replace the champion roster, after one is fetched or a patch lands.
    pub fn set_roster(&self, roster: ChampionRoster) {
        {
            let mut state = self.shared.state.lock().unwrap();
            state.roster = roster;
            state.refresh_wanted();
        }
        self.shared.wake.notify_all();
    }

    /// What the scheduler is planning for, for a caller that wants to draw it.
    pub fn snapshot(&self) -> (Option<Desired>, Option<Desired>, Budget) {
        let state = self.shared.state.lock().unwrap();
        (
            state.wanted.clone(),
            state.applied.clone(),
            state.budget.clone(),
        )
    }

    /// Nudge the worker to look again, for a caller that changed something the
    /// scheduler cannot see, such as the patcher coming up.
    pub fn poke(&self) {
        self.shared.wake.notify_all();
    }

    /// Stop the worker and wait for it.
    pub fn stop(&mut self) {
        self.shared.stopped.store(true, Ordering::SeqCst);
        self.shared.wake.notify_all();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

impl Drop for Scheduler {
    fn drop(&mut self) {
        self.stop();
    }
}

/// How long the worker sleeps before looking again with nothing to wake it.
///
/// Everything that matters wakes it, so this is only the belt to that braces:
/// the patcher coming up is news the scheduler is told about rather than
/// something it watches, and a poke that goes missing should cost a second
/// rather than a champion select.
const IDLE_TICK: Duration = Duration::from_secs(1);

fn run(shared: Arc<Shared>, swapper: Arc<dyn Swapper>) {
    while !shared.stopped.load(Ordering::SeqCst) {
        let Some(desired) = claim(&shared, &swapper) else {
            let state = shared.state.lock().unwrap();
            let _ = shared.wake.wait_timeout(state, IDLE_TICK).unwrap();
            continue;
        };

        // Outside the lock: a rebuild takes half a second, and champion select
        // keeps moving while it runs.
        let outcome = swapper.apply(&desired);

        let mut state = shared.state.lock().unwrap();
        state.building = false;
        match outcome {
            Ok(took) => {
                state.budget.observe_rebuild(took);
                state.applied = Some(desired.clone());
                drop(state);
                swapper.report(Report::Applied { desired, took });
            }
            Err(error) => {
                drop(state);
                swapper.report(Report::Failed {
                    desired,
                    error: error.to_string(),
                });
            }
        }
        // Champion select may have moved while that ran, so look again rather
        // than waiting to be woken.
        shared.wake.notify_all();
    }
}

/// Take the next swap worth attempting, marking the worker busy.
///
/// `None` when there is nothing to do, or when the rule refuses - and a refusal
/// is reported once per desired state rather than on every tick, by recording
/// it as applied. The reader is told the change did not land; repeating it every
/// second would say the same thing sixty times.
fn claim(shared: &Arc<Shared>, swapper: &Arc<dyn Swapper>) -> Option<Desired> {
    let mut state = shared.state.lock().unwrap();
    if state.building {
        return None;
    }
    let desired = state.outstanding()?;

    let (patcher, game_attached) = swapper.patcher_status();
    let context = SwapContext {
        patcher,
        game_attached,
        gameflow: state.gameflow.clone(),
        champ_select: state.champ_select.as_ref(),
        build_in_flight: false,
    };

    match decision::decide(&context, &state.budget) {
        Verdict::Allow { .. } => {
            state.building = true;
            Some(desired)
        }
        Verdict::Refuse(why) => {
            // A refusal that can never become an allowance is final, and saying
            // so once is the whole of what the reader needs. One that could
            // change - the patcher is still building, say - is left outstanding
            // so the next tick tries again.
            if is_final(&why) {
                state.applied = Some(desired.clone());
                drop(state);
                swapper.report(Report::Refused { desired, why });
            }
            None
        }
    }
}

/// Whether a refusal will still hold however long the scheduler waits.
fn is_final(why: &Refusal) -> bool {
    match why {
        // The game is under way, or champion select is over. Nothing about this
        // champion select can change these back.
        Refusal::GameAlreadyRunning | Refusal::GameStarting | Refusal::ChampSelectOver => true,
        // Time only runs out, so once there is not enough there never will be
        // again in this select.
        Refusal::NotEnoughTime { .. } => true,
        // These are moments rather than states: the patcher can come up, and a
        // build in flight ends.
        Refusal::PatcherIdle | Refusal::PatcherBuilding | Refusal::BuildInFlight => false,
    }
}

#[cfg(test)]
mod tests;
