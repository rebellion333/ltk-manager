//! Patcher state owned by the Tauri shell.
//!
//! The session thread, its lifecycle state and the line protocol all live in the
//! Tauri-free core crate; what stays here is the managed-state wrappers Tauri
//! registers and the [`thread`] adapter that maps core notifications to UI
//! events.
//!
//! The legacy in-process implementation (`api.rs` / `runner.rs`, which loaded
//! the patcher DLL into the manager process and interfered with Vanguard) was
//! deleted here; it was superseded by the external host in `c0ecc27` and is
//! recoverable from history if the native reimplementation ever wants it.

pub mod thread;

pub use ltk_manager_core::patcher::{
    host, injector, session, PatcherError, PatcherEvents, PatcherPhase, PatcherSession,
    PatcherStateInner, PatcherThread, SessionOrigin, SessionParams, StoredPatcherConfig,
};

use crate::error::{AppError, AppResult};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use ltk_manager_core::patcher::host::PatcherHost;
use parking_lot::Mutex;

/// Tauri-managed patcher lifecycle state.
///
/// The inner `Arc` is private on purpose: every accessor below either takes a
/// closure or does one bounded operation, so no caller can hold the patcher lock
/// across a blocking wait or a second lock. [`Self::handle`] is the one way out,
/// for handing the shared state to the core session thread.
pub struct PatcherState {
    inner: Arc<Mutex<PatcherStateInner>>,
    /// Whether the DLL has entered a game in the session now running.
    ///
    /// The phase does not say this: a session stays `Patching` from before the
    /// first game until it is stopped, and the moment the overlay stops being
    /// safe to change is the attach rather than the phase.
    game_attached: Arc<AtomicBool>,
}

impl PatcherState {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(PatcherStateInner::new())),
            game_attached: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Whether a game attached in the running session.
    pub fn game_attached(&self) -> bool {
        self.game_attached.load(Ordering::SeqCst)
    }

    /// Record that the DLL entered a game, or that a new session cleared it.
    pub fn set_game_attached(&self, attached: bool) {
        self.game_attached.store(attached, Ordering::SeqCst);
    }

    /// The shared state itself, for [`PatcherThread::start`].
    pub fn handle(&self) -> &Arc<Mutex<PatcherStateInner>> {
        &self.inner
    }

    /// Read the state under the lock. The closure bounds the guard's lifetime.
    pub fn with<T>(&self, f: impl FnOnce(&PatcherStateInner) -> T) -> T {
        let inner = self.inner.lock();
        f(&inner)
    }

    /// Mutate the state under the lock.
    pub fn with_mut<T>(&self, f: impl FnOnce(&mut PatcherStateInner) -> T) -> T {
        let mut inner = self.inner.lock();
        f(&mut inner)
    }

    /// Whether a patching session is currently live.
    #[must_use]
    pub fn is_running(&self) -> bool {
        self.with(PatcherStateInner::is_running)
    }

    /// Ask a running session to stop, reporting whether there was one.
    ///
    /// Only signals — the session unwinds on its own thread, so callers that
    /// need it *gone* must still wait for the handle to finish.
    pub fn request_stop(&self) -> bool {
        self.with(|inner| {
            let running = inner.is_running();
            if running {
                inner.stop_flag.store(true, Ordering::SeqCst);
            }
            running
        })
    }

    /// Block until the session thread is gone, for a caller that starts the
    /// next one.
    ///
    /// # Errors
    ///
    /// The thread is still running after five seconds.
    pub fn wait_for_stop(&self) -> AppResult<()> {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        loop {
            if !self.is_running() {
                return Ok(());
            }
            if std::time::Instant::now() > deadline {
                return Err(AppError::Other(
                    "Timed out waiting for patcher to stop".to_string(),
                ));
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
    }
}

impl Default for PatcherState {
    fn default() -> Self {
        Self::new()
    }
}

/// Tauri-managed handle to the persistent host. `None` until the first patcher
/// start spawns it (lazy start); cleared when the host dies so the next start
/// respawns.
pub struct PatcherHostState(Arc<Mutex<Option<PatcherHost>>>);

impl PatcherHostState {
    /// The shared slot itself, for [`PatcherThread::start`] — which spawns into
    /// it lazily and clears it when the host dies.
    pub fn handle(&self) -> &Arc<Mutex<Option<PatcherHost>>> {
        &self.0
    }

    /// Take the host out of managed state, leaving it empty.
    ///
    /// Ownership moves to the caller so the (possibly long) shutdown happens
    /// with the lock released.
    pub fn take(&self) -> Option<PatcherHost> {
        self.0.lock().take()
    }
}

impl Default for PatcherHostState {
    fn default() -> Self {
        Self(Arc::new(Mutex::new(None)))
    }
}

/// Stop a running patching session and shut down the long-lived injection host.
pub fn shutdown_resources(app_handle: &tauri::AppHandle) {
    use tauri::Manager;

    if let Some(patcher_state) = app_handle.try_state::<PatcherState>() {
        let _ = patcher_state.request_stop();
    }

    if let Some(host_state) = app_handle.try_state::<PatcherHostState>() {
        if let Some(mut host) = host_state.take() {
            tracing::info!("Shutting down injection host");
            host.shutdown();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn patcher_host_state_defaults_to_empty() {
        let state = PatcherHostState::default();
        assert!(
            state.take().is_none(),
            "no host until the first patcher start spawns one (lazy start)"
        );
    }

    #[test]
    fn patcher_state_new_creates_valid_state() {
        let state = PatcherState::new();
        assert!(!state.is_running());
        assert_eq!(state.with(|inner| inner.phase), PatcherPhase::Idle);
    }

    /// Stopping an idle patcher is a no-op that says so, rather than setting the
    /// flag for whatever session starts next.
    #[test]
    fn request_stop_reports_no_session_and_leaves_the_flag_clear() {
        let state = PatcherState::new();
        assert!(!state.request_stop());
        assert!(!state.with(|inner| inner.stop_flag.load(Ordering::SeqCst)));
    }
}
