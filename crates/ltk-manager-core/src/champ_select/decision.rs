//! Whether a swap may be attempted right now, and why not when it may not.
//!
//! Pure: every input arrives as an argument and nothing here reads a clock, a
//! file or the client. That is deliberate. This is the rule ADR-0043 exists
//! for, the one whose failure mode is a corrupted archive in a loading screen,
//! and it is worth being able to state every case as a test.
//!
//! What it does **not** decide is how the swap is written. Rule 0 of that ADR
//! settles that: a swap-triggered build always takes the whole-WAD path, so no
//! decision here can produce the in-place rewrite.

use std::time::Duration;

use crate::lcu::ChampSelectView;
use crate::lcu::GameflowPhase;
use crate::patcher::PatcherPhase;

use super::budget::Budget;

/// Everything the rule reads.
#[derive(Debug, Clone)]
pub struct SwapContext<'a> {
    pub patcher: PatcherPhase,
    /// Whether the DLL has entered a game in this patching session.
    ///
    /// Not whether a game is running anywhere: a session that already served
    /// one game is past the moment its overlay could change.
    pub game_attached: bool,
    pub gameflow: GameflowPhase,
    pub champ_select: Option<&'a ChampSelectView>,
    /// Whether a build of this profile's overlay is already running.
    pub build_in_flight: bool,
}

/// Why a swap was not attempted, in the reader's terms rather than the code's.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Refusal {
    /// The patcher is not up, so there is no overlay a game would read.
    PatcherIdle,
    /// The patcher is still building its overlay.
    PatcherBuilding,
    /// A game already attached in this session, so the archives are being read.
    GameAlreadyRunning,
    /// The game is starting or running, whatever the patcher thinks.
    GameStarting,
    /// Champion select has handed the match over.
    ChampSelectOver,
    /// Another build of this overlay is running.
    BuildInFlight,
    /// There is not enough of champion select left.
    NotEnoughTime {
        needed: Duration,
        available: Duration,
    },
}

/// What the rule concluded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    /// Go ahead, with this much room to spare beyond what the rebuild needs.
    Allow {
        available: Duration,
    },
    Refuse(Refusal),
}

impl Verdict {
    pub fn is_allowed(&self) -> bool {
        matches!(self, Self::Allow { .. })
    }
}

/// How much time a swap decided now has before the game opens the archive.
///
/// Inside `FINALIZATION` the phase clock is a countdown to the game, so what is
/// left of it plus the fixed tail is the answer. In any earlier phase the clock
/// is not: `BAN_PICK` restarts between bans and picks, and an ARAM select opens
/// in `BAN_PICK` with no bans in it at all. What is still true there is that
/// champion select has not ended, so the tail alone is a floor - and since the
/// tail is several rebuilds long, a floor is all the rule needs.
fn available(view: &ChampSelectView, budget: &Budget) -> Duration {
    if view.timer_phase.eq_ignore_ascii_case("FINALIZATION") {
        let left = Duration::from_millis(view.time_left_ms.max(0) as u64);
        return left + budget.tail;
    }
    budget.tail
}

/// Whether a swap may be attempted.
pub fn decide(context: &SwapContext<'_>, budget: &Budget) -> Verdict {
    match context.patcher {
        PatcherPhase::Idle => return Verdict::Refuse(Refusal::PatcherIdle),
        PatcherPhase::Building => return Verdict::Refuse(Refusal::PatcherBuilding),
        PatcherPhase::Patching => {}
    }

    if context.game_attached {
        return Verdict::Refuse(Refusal::GameAlreadyRunning);
    }
    if context.gameflow.game_is_starting_or_running() {
        return Verdict::Refuse(Refusal::GameStarting);
    }
    if context.build_in_flight {
        return Verdict::Refuse(Refusal::BuildInFlight);
    }

    let Some(view) = context.champ_select else {
        // No champion select is not a refusal to swap, it is the ordinary
        // moment to: the lobby is when there is the most time of all.
        return Verdict::Allow {
            available: budget.tail,
        };
    };

    // The champion is settled and the match is being handed over. Measured, a
    // rebuild would still land, and refusing anyway is the conservatism rule 0
    // is built on: the last seconds are where a machine having a bad moment
    // costs a game rather than a refusal.
    if view.timer_phase.eq_ignore_ascii_case("GAME_STARTING") {
        return Verdict::Refuse(Refusal::ChampSelectOver);
    }

    let available = available(view, budget);
    if !budget.fits(available) {
        return Verdict::Refuse(Refusal::NotEnoughTime {
            needed: budget.needed(),
            available,
        });
    }
    Verdict::Allow { available }
}

#[cfg(test)]
mod tests;
