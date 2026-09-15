use std::path::PathBuf;
use std::sync::Arc;

use parking_lot::Mutex;
use tauri::{AppHandle, State};

use crate::error::IpcResult;
use crate::events::TauriEventSink;
use ltk_manager_core::config::Config;
use ltk_manager_core::events::EventSink;
use ltk_manager_core::lcu::{LcuSnapshot, LcuWatch, SinkObserver, Snapshotting};

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
            next: Box::new(SinkObserver(sink)),
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

/// What the League client watch last saw.
///
/// What a frontend asks on mount: an event that fired before the webview did
/// announced itself to nobody.
#[tauri::command]
#[specta::specta]
pub fn get_lcu_snapshot(lcu: State<LcuState>) -> IpcResult<LcuSnapshot> {
    IpcResult::ok(lcu.snapshot())
}
