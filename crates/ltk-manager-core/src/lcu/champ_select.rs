//! Champion select, `/lol-champ-select/v1/session`, reduced to what a mod
//! swap needs to know about the local player.
//!
//! The session is a large document about ten players. What matters here is
//! one cell: which champion the local player is hovering, which they have
//! locked, whether a late change is still possible (a trade, an ARAM bench
//! swap), and how long the current timer phase has left. Every field is
//! tolerant, so a client that adds or drops one keeps parsing.

use serde::{Deserialize, Serialize};

/// The session as the client sends it. Only the parts read here are named.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ChampSelectSession {
    pub game_id: u64,
    pub local_player_cell_id: i64,
    pub my_team: Vec<TeamCell>,
    /// Groups of actions, in phase order. Each inner list is one turn.
    pub actions: Vec<Vec<Action>>,
    pub timer: Timer,
    pub bench_enabled: bool,
    pub bench_champions: Vec<BenchChampion>,
    pub trades: Vec<Trade>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct TeamCell {
    pub cell_id: i64,
    /// Locked champion, or 0.
    pub champion_id: i32,
    /// Hovered champion, or 0.
    pub champion_pick_intent: i32,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Action {
    pub id: i64,
    pub actor_cell_id: i64,
    pub champion_id: i32,
    pub completed: bool,
    pub is_in_progress: bool,
    /// `pick`, `ban`, `ten_bans_reveal` and others.
    #[serde(rename = "type")]
    pub kind: String,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Timer {
    /// `PLANNING`, `BAN_PICK`, `FINALIZATION`, `GAME_STARTING`.
    pub phase: String,
    pub adjusted_time_left_in_phase: i64,
    pub total_time_in_phase: i64,
    pub is_infinite: bool,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct BenchChampion {
    pub champion_id: i32,
}

/// One champion-trade slot with a teammate.
///
/// The client publishes a slot per teammate whether or not anything is
/// happening in it, so the presence of a trade is not the presence of an offer:
/// a Practice Tool select, where no trade is possible at all, still carries
/// them. Only [`Trade::is_in_flight`] says something is actually pending.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Trade {
    pub id: i64,
    pub cell_id: i64,
    /// `AVAILABLE`, `BUSY`, `INVALID`, `SENT`, `RECEIVED` and others.
    pub state: String,
}

impl Trade {
    /// Whether an offer is open in this slot, either way round.
    pub fn is_in_flight(&self) -> bool {
        self.state.eq_ignore_ascii_case("SENT") || self.state.eq_ignore_ascii_case("RECEIVED")
    }
}

/// What crosses to the frontend: the local player's part of the session.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS, specta::Type))]
#[cfg_attr(feature = "ts", ts(export))]
#[serde(rename_all = "camelCase")]
pub struct ChampSelectView {
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub game_id: u64,
    /// The champion the local player has locked, once they have.
    pub locked_champion_id: Option<i32>,
    /// The champion the local player is hovering, while they have not locked.
    pub hovered_champion_id: Option<i32>,
    /// The timer phase, as the client spells it.
    pub timer_phase: String,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub time_left_ms: i64,
    /// Whether the champion can still change after a lock: an offered trade is
    /// open, or the ARAM bench is.
    ///
    /// A trade *slot* does not count. The client lists one per teammate in
    /// every mode, so counting those reports every Practice Tool select as
    /// still changeable, which a live capture on 2026-09-15 is what caught.
    pub can_still_change: bool,
    /// Champions on the ARAM bench, for a reroll or a swap.
    pub bench_champion_ids: Vec<i32>,
}

impl ChampSelectSession {
    /// The local player's part of the session.
    pub fn view(&self) -> ChampSelectView {
        let cell = self
            .my_team
            .iter()
            .find(|c| c.cell_id == self.local_player_cell_id);

        // A completed pick action is the lock. `championId` on the cell also
        // reads as the lock, but it is what a bench swap rewrites, so the cell
        // is the one that keeps up with a late change.
        let locked_by_action = self.actions.iter().flatten().any(|a| {
            a.kind == "pick" && a.actor_cell_id == self.local_player_cell_id && a.completed
        });
        let locked_champion_id = cell
            .map(|c| c.champion_id)
            .filter(|id| *id > 0)
            .filter(|_| locked_by_action || self.bench_enabled);

        let in_progress_pick = self
            .actions
            .iter()
            .flatten()
            .find(|a| {
                a.kind == "pick" && a.actor_cell_id == self.local_player_cell_id && a.is_in_progress
            })
            .map(|a| a.champion_id)
            .filter(|id| *id > 0);
        let hovered_champion_id = match locked_champion_id {
            Some(_) => None,
            None => in_progress_pick
                .or_else(|| cell.map(|c| c.champion_pick_intent).filter(|id| *id > 0)),
        };

        ChampSelectView {
            game_id: self.game_id,
            locked_champion_id,
            hovered_champion_id,
            timer_phase: self.timer.phase.clone(),
            time_left_ms: self.timer.adjusted_time_left_in_phase,
            can_still_change: self.bench_enabled || self.trades.iter().any(Trade::is_in_flight),
            bench_champion_ids: self.bench_champions.iter().map(|b| b.champion_id).collect(),
        }
    }
}

#[cfg(test)]
mod tests;
