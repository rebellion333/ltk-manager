use std::sync::Arc;
use std::time::Duration;

use parking_lot::Mutex;
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};
use ts_rs::TS;

use crate::error::IpcResult;
use crate::mods::ModLibraryState;
use crate::patcher::PatcherState;
use crate::state::SettingsState;
use ltk_manager_core::champ_select::{
    Budget, ChampionPreference, Desired, Refusal, Report, Scheduler, Swapper,
};
use ltk_manager_core::lcu::champions::{ChampionRoster, ChampionSummary};
use ltk_manager_core::lcu::{ChampSelectView, LcuEvent};
use ltk_manager_core::mods::ModLibrary;
use ltk_manager_core::patcher::PatcherPhase;

/// What the scheduler concluded, for the interface to draw.
///
/// A code and typed fields rather than a sentence, per ADR-0017.
#[derive(Debug, Clone, Serialize, TS, specta::Type)]
#[ts(export)]
#[serde(
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    tag = "status"
)]
pub enum SwapReport {
    /// The overlay now carries this champion's mod.
    Applied {
        champion: String,
        #[ts(optional = nullable)]
        mod_id: Option<String>,
        #[ts(type = "number")]
        took_ms: u64,
    },
    /// The swap was not attempted, and the reader keeps whatever was applied.
    Refused {
        champion: String,
        #[ts(optional = nullable)]
        mod_id: Option<String>,
        why: Refusal,
    },
    /// The rebuild failed, so the overlay is whatever it was before.
    Failed {
        champion: String,
        #[ts(optional = nullable)]
        mod_id: Option<String>,
        detail: String,
    },
}

impl From<Report> for SwapReport {
    fn from(report: Report) -> Self {
        match report {
            Report::Applied { desired, took } => Self::Applied {
                champion: desired.alias,
                mod_id: desired.mod_id,
                took_ms: took.as_millis() as u64,
            },
            Report::Refused { desired, why } => Self::Refused {
                champion: desired.alias,
                mod_id: desired.mod_id,
                why,
            },
            Report::Failed { desired, error } => Self::Failed {
                champion: desired.alias,
                mod_id: desired.mod_id,
                detail: error,
            },
        }
    }
}

/// The scheduler's view of the world, for a frontend that has just mounted.
#[derive(Debug, Clone, Serialize, TS, specta::Type)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct ChampSelectStatus {
    /// The champion and mod the overlay should be carrying, when there is one.
    #[ts(optional = nullable)]
    pub wanted: Option<DesiredMod>,
    /// What it was last made to carry.
    #[ts(optional = nullable)]
    pub applied: Option<DesiredMod>,
    /// What a rebuild is currently expected to cost on this machine.
    #[ts(type = "number")]
    pub rebuild_ms: u64,
    /// How long there is after champion select ends, on this machine.
    #[ts(type = "number")]
    pub tail_ms: u64,
}

#[derive(Debug, Clone, Serialize, TS, specta::Type)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct DesiredMod {
    pub champion: String,
    #[ts(optional = nullable)]
    pub mod_id: Option<String>,
}

impl From<Desired> for DesiredMod {
    fn from(desired: Desired) -> Self {
        Self {
            champion: desired.alias,
            mod_id: desired.mod_id,
        }
    }
}

/// The scheduler's hands: the real library, the real patcher, the real webview.
struct ShellSwapper {
    app: AppHandle,
    library: ModLibrary,
}

impl Swapper for ShellSwapper {
    fn patcher_status(&self) -> (PatcherPhase, bool) {
        match self.app.try_state::<PatcherState>() {
            Some(patcher) => (patcher.with(|inner| inner.phase), patcher.game_attached()),
            // No managed state means the app is shutting down, and an idle
            // patcher is the answer that refuses everything.
            None => (PatcherPhase::Idle, false),
        }
    }

    /* The rebuild is unconditional, and the profile not moving is not a reason
    to skip it. `PreferenceChange::changed` describes the profile; what has to
    be true here is about the overlay, and the two come apart the moment
    anything else writes the preference first - which `set_champion_preference`
    does on every click of the panel. Trusting the profile there left an overlay
    with nothing in it while the scheduler reported the mod applied, caught in a
    live Practice Tool on 2026-09-16. The scheduler only asks when its own
    record of the overlay differs from what is wanted, so an ask is always a
    rebuild worth doing. */
    fn apply(&self, desired: &Desired) -> ltk_manager_core::error::AppResult<Duration> {
        let config = self.app.state::<SettingsState>().config();
        self.library.apply_champion_preference(
            &config,
            &desired.alias,
            desired.mod_id.as_deref(),
        )?;

        let outcome = self.library.rebuild_for_swap(&config)?;
        Ok(outcome.rebuilt_in)
    }

    fn report(&self, report: Report) {
        let payload = SwapReport::from(report);
        let _ = self.app.emit("champ-select-swap", &payload);
    }
}

/// The one scheduler the app runs.
pub struct ChampSelectState {
    scheduler: Mutex<Scheduler>,
    library: ModLibrary,
    /// The champion the window was last raised for, cleared with each select.
    raised_for: Mutex<Option<String>>,
}

impl ChampSelectState {
    /// Start the scheduler with whatever roster is already on disk.
    ///
    /// A cached roster is enough to name champions the app has seen before, and
    /// the client refreshes it as soon as one answers. Waiting for a client
    /// would leave the first champion select of a session unable to name
    /// anything.
    pub fn new(app: &AppHandle, library: ModLibrary, cache_dir: Option<&std::path::Path>) -> Self {
        let roster = cache_dir.and_then(ChampionRoster::load).unwrap_or_default();
        let swapper = Arc::new(ShellSwapper {
            app: app.clone(),
            library: library.clone(),
        });
        Self {
            scheduler: Mutex::new(Scheduler::start(roster, swapper)),
            library,
            raised_for: Mutex::new(None),
        }
    }

    /// Feed the scheduler one thing the client said.
    ///
    /// The preferences are re-read as a champion select opens rather than held:
    /// the reader may have changed one between games, and reading a small map
    /// once per select is cheaper than keeping it in step with every write.
    pub fn observe(&self, app: &AppHandle, event: &LcuEvent) {
        let scheduler = self.scheduler.lock();
        if matches!(event, LcuEvent::ChampSelectStarted(_)) {
            let config = app.state::<SettingsState>().config();
            match self.library.champion_preferences(&config) {
                Ok(preferences) => scheduler.set_preferences(preferences),
                Err(e) => tracing::warn!("Could not read the champion preferences: {e}"),
            }
            *self.raised_for.lock() = None;
        }
        let roster = scheduler.roster();
        scheduler.observe(event);
        drop(scheduler);

        match event {
            LcuEvent::ChampSelectStarted(view) | LcuEvent::ChampSelectChanged(view) => {
                self.raise_if_undecided(app, view, &roster)
            }
            _ => {}
        }
    }

    /// Bring the window forward for a pick the reader has to answer.
    ///
    /// Here rather than in the panel, because the panel is in a webview the
    /// reader has minimized and a minimized webview is not a reliable place to
    /// run anything. A frontend attempt at this went unnoticed on 2026-09-16
    /// with no way to tell whether it had run at all; the backend is running
    /// either way and says what it did.
    ///
    /// Silent when the champion's mod is already decided. A swap the scheduler
    /// settles on its own asks the reader for nothing, and taking the screen
    /// for it takes it from a game about to start.
    fn raise_if_undecided(&self, app: &AppHandle, view: &ChampSelectView, roster: &ChampionRoster) {
        let Some(id) = view.locked_champion_id.or(view.hovered_champion_id) else {
            return;
        };
        let Some(alias) = roster.by_id(id).map(|champion| champion.alias.clone()) else {
            return;
        };

        // Once per champion, so finishing a ban does not fight the window.
        {
            let mut raised = self.raised_for.lock();
            if raised.as_deref() == Some(alias.as_str()) {
                return;
            }
            *raised = Some(alias.clone());
        }

        let config = app.state::<SettingsState>().config();
        let decided = self
            .library
            .champion_preferences(&config)
            .map(|preferences| preferences.contains_key(&alias))
            .unwrap_or(false);
        if decided {
            tracing::debug!(%alias, "Pick already decided, leaving the window alone");
            return;
        }

        let offers = self
            .library
            .mods_for_champion(&config, &alias)
            .map(|mods| !mods.is_empty())
            .unwrap_or(false);
        if !offers {
            tracing::debug!(%alias, "No mods for the pick, leaving the window alone");
            return;
        }

        tracing::info!(%alias, "Raising the window for an undecided pick");
        crate::commands::shell::raise_main_window(app);
    }

    /// Hand the scheduler a roster a live client answered with.
    pub fn set_roster(&self, roster: ChampionRoster) {
        self.scheduler.lock().set_roster(roster);
    }

    /// Tell the scheduler something it cannot see changed, such as the patcher
    /// coming up.
    pub fn poke(&self) {
        self.scheduler.lock().poke();
    }

    pub fn shutdown(&self) {
        self.scheduler.lock().stop();
    }

    fn roster(&self) -> Vec<ChampionSummary> {
        self.scheduler.lock().roster().champions().to_vec()
    }

    fn status(&self) -> ChampSelectStatus {
        let (wanted, applied, budget) = self.scheduler.lock().snapshot();
        let Budget { rebuild, tail } = budget;
        ChampSelectStatus {
            wanted: wanted.map(DesiredMod::from),
            applied: applied.map(DesiredMod::from),
            rebuild_ms: rebuild.as_millis() as u64,
            tail_ms: tail.as_millis() as u64,
        }
    }
}

/// What the scheduler is planning for, for a frontend that has just mounted.
#[tauri::command]
#[specta::specta]
pub fn get_champ_select_status(state: State<ChampSelectState>) -> IpcResult<ChampSelectStatus> {
    IpcResult::ok(state.status())
}

/// The champion roster, so the interface can name the ids champion select speaks in.
///
/// The scheduler's copy rather than a second fetch: whatever it cannot name, the
/// interface cannot name either, and one roster is what keeps the two saying the
/// same thing about the same pick.
#[tauri::command]
#[specta::specta]
pub fn champion_roster(state: State<ChampSelectState>) -> IpcResult<Vec<ChampionSummary>> {
    IpcResult::ok(state.roster())
}

/// Every installed mod that applies to a champion, by the champion's alias.
#[tauri::command]
#[specta::specta]
pub fn mods_for_champion(
    champion: String,
    library: State<ModLibraryState>,
    settings: State<SettingsState>,
) -> IpcResult<Vec<String>> {
    let config = settings.config();
    library.0.mods_for_champion(&config, &champion).into()
}

/// What the active profile wants for each champion.
#[tauri::command]
#[specta::specta]
pub fn get_champion_preferences(
    library: State<ModLibraryState>,
    settings: State<SettingsState>,
) -> IpcResult<std::collections::HashMap<String, ChampionPreference>> {
    let config = settings.config();
    library.0.champion_preferences(&config).into()
}

/// A champion and the mods installed for it.
#[derive(Debug, Clone, Serialize, TS, specta::Type)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct ChampionMods {
    pub champion: ChampionSummary,
    pub mod_ids: Vec<String>,
}

/// Every champion the library holds a mod for, by the champion's name.
///
/// The roster is long and most of it is not worth a row: a reader configuring
/// their champions wants the ones they have something to configure. Sorted
/// here rather than in the interface, because the answer is a list and a list
/// has an order.
#[tauri::command]
#[specta::specta]
pub fn champions_with_mods(
    state: State<ChampSelectState>,
    library: State<ModLibraryState>,
    settings: State<SettingsState>,
) -> IpcResult<Vec<ChampionMods>> {
    let config = settings.config();
    let roster = state.roster();
    let aliases: Vec<String> = roster.iter().map(|c| c.alias.clone()).collect();

    let by_alias = match library.0.mods_by_champion(&config, &aliases) {
        Ok(found) => found,
        Err(e) => return IpcResult::from(Err::<Vec<ChampionMods>, _>(e)),
    };

    let mut found: Vec<ChampionMods> = roster
        .into_iter()
        .filter_map(|champion| {
            by_alias.get(&champion.alias).map(|mod_ids| ChampionMods {
                champion,
                mod_ids: mod_ids.clone(),
            })
        })
        .collect();
    found.sort_by(|a, b| a.champion.name.cmp(&b.champion.name));
    IpcResult::ok(found)
}

/// The mods the active profile keeps within reach, per champion.
#[tauri::command]
#[specta::specta]
pub fn get_champion_favorites(
    library: State<ModLibraryState>,
    settings: State<SettingsState>,
) -> IpcResult<std::collections::HashMap<String, Vec<String>>> {
    let config = settings.config();
    library.0.champion_favorites(&config).into()
}

/// Mark a mod as one of a champion's favourites, or unmark it.
///
/// Not a preference: nothing about the enabled set moves, no rebuild follows,
/// and the scheduler is not told. This only changes where a mod sits in the
/// list champion select offers.
#[tauri::command]
#[specta::specta]
pub async fn set_champion_favorite(
    champion: String,
    mod_id: String,
    favorite: bool,
    app_handle: AppHandle,
) -> IpcResult<()> {
    super::off_thread(move || {
        let config = app_handle.state::<SettingsState>().config();
        let library = app_handle.state::<ModLibraryState>().0.clone();
        library.set_champion_favorite(&config, &champion, &mod_id, favorite)
    })
    .await
}

/// Choose the mod a champion applies, and rebuild the overlay for it.
///
/// The scheduler does this on its own for the champion in play. This is the
/// same thing asked for by hand, from the library or from champion select, and
/// it goes through the same path so both cannot drift.
#[tauri::command]
#[specta::specta]
pub async fn set_champion_preference(
    champion: String,
    mod_id: Option<String>,
    app_handle: AppHandle,
) -> IpcResult<()> {
    super::off_thread(move || {
        /* Both ends, because the panel spinner is this call and a spinner that
        outlives it is a different fault from one that does not. */
        let started = std::time::Instant::now();
        tracing::debug!(%champion, ?mod_id, "Champion preference requested");

        let config = app_handle.state::<SettingsState>().config();
        let library = app_handle.state::<ModLibraryState>().0.clone();
        library.apply_champion_preference(&config, &champion, mod_id.as_deref())?;

        // The scheduler holds what it last applied, and a preference changed by
        // hand makes that stale.
        let champ_select = app_handle.state::<ChampSelectState>();
        let config_again = app_handle.state::<SettingsState>().config();
        if let Ok(preferences) = library.champion_preferences(&config_again) {
            champ_select.scheduler.lock().set_preferences(preferences);
        }
        champ_select.poke();

        tracing::debug!(
            took_ms = started.elapsed().as_millis() as u64,
            "Champion preference recorded"
        );
        Ok(())
    })
    .await
}
