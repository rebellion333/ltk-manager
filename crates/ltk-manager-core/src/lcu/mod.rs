//! Reading the League client (LCU), and following champion select through it.
//!
//! The Riot Client has [`crate::launcher`]; this is the other process, the
//! League client itself, which holds the lobby, the queue and champion select.
//! Everything here reads. There is no route that creates a lobby, accepts a
//! queue, picks, bans or speaks, and none is to be added: the manager reads
//! the game and never operates the account.
//!
//! The client is found through its `lockfile` in the League install root,
//! reached over HTTP Basic with the password the lockfile holds, and followed
//! over its WebSocket rather than polled. A closed client costs nothing: no
//! lockfile, no connection, one `stat` every ten seconds.

pub mod champ_select;
pub mod champions;
pub mod client;
pub mod gameflow;
pub mod lockfile;
pub mod socket;
pub mod watch;

use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::events::{BackendEvent, EventSink};

pub use champ_select::{ChampSelectSession, ChampSelectView};
pub use champions::ChampionSummary;
pub use client::LcuClient;
pub use gameflow::GameflowPhase;
pub use lockfile::LeagueLockfile;
pub use watch::{LcuEvent, LcuObserver, LcuWatch, find_live_client};

/// Whether a League client is answering, and on which port.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export))]
#[serde(rename_all = "camelCase")]
pub struct LcuClientState {
    pub connected: bool,
    pub port: Option<u16>,
}

/// The gameflow phase, spelled the way the client spelled it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export))]
#[serde(rename_all = "camelCase")]
pub struct GameflowChanged {
    pub phase: String,
}

/// What the watch last saw, for a caller that arrives after the events did.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS, specta::Type))]
#[cfg_attr(feature = "ts", ts(export))]
#[serde(rename_all = "camelCase")]
pub struct LcuSnapshot {
    pub connected: bool,
    pub port: Option<u16>,
    /// The gameflow phase as the client spells it, once one was read.
    pub phase: Option<String>,
    /// The champion select being followed, while there is one.
    pub champ_select: Option<ChampSelectView>,
}

impl LcuSnapshot {
    /// Fold one event into the snapshot.
    pub fn apply(&mut self, event: &LcuEvent) {
        match event {
            LcuEvent::ClientUp { port } => {
                self.connected = true;
                self.port = Some(*port);
            }
            LcuEvent::ClientDown => *self = Self::default(),
            LcuEvent::Gameflow(phase) => self.phase = Some(phase.as_str().to_string()),
            LcuEvent::ChampSelectStarted(view) | LcuEvent::ChampSelectChanged(view) => {
                self.champ_select = Some(view.clone());
            }
            LcuEvent::ChampSelectEnded => self.champ_select = None,
        }
    }
}

/// Keeps a snapshot current, then hands each event on.
pub struct Snapshotting {
    pub snapshot: Arc<parking_lot::Mutex<LcuSnapshot>>,
    pub next: Box<dyn LcuObserver>,
}

impl LcuObserver for Snapshotting {
    fn on_event(&self, event: LcuEvent) {
        self.snapshot.lock().apply(&event);
        self.next.on_event(event);
    }
}

/// Bridges the watch's observer to the manager's event registry.
pub struct SinkObserver(pub Arc<dyn EventSink>);

impl LcuObserver for SinkObserver {
    fn on_event(&self, event: LcuEvent) {
        let event = match event {
            LcuEvent::ClientUp { port } => BackendEvent::LcuClientChanged(LcuClientState {
                connected: true,
                port: Some(port),
            }),
            LcuEvent::ClientDown => BackendEvent::LcuClientChanged(LcuClientState {
                connected: false,
                port: None,
            }),
            LcuEvent::Gameflow(phase) => BackendEvent::GameflowChanged(GameflowChanged {
                phase: phase.as_str().to_string(),
            }),
            LcuEvent::ChampSelectStarted(view) => BackendEvent::ChampSelectStarted(view),
            LcuEvent::ChampSelectChanged(view) => BackendEvent::ChampSelectChanged(view),
            LcuEvent::ChampSelectEnded => BackendEvent::ChampSelectEnded,
        };
        self.0.emit(event);
    }
}
