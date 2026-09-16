//! How long there is, and how long a rebuild takes.
//!
//! Both halves are measurements rather than constants, and the machine the app
//! runs on supplies them. The values below are what this build starts from
//! before it has seen a rebuild of its own, and every one of them is a figure
//! from one PC with an NVMe. A mechanical disk is five to ten times slower at
//! the part that matters, which is why nothing here is compiled in as a
//! promise: [`Budget::fits`] compares what the machine reported, and a machine
//! that cannot make the time says so instead of trying.

use std::time::Duration;

/// The run from champion select ending to the game opening a champion's
/// archive, which is the margin a swap decided at the very last moment has.
///
/// Measured end to end on 2026-09-16 over two games: 3.689 s and 4.098 s from
/// `GAME_STARTING` to the DLL's `redirected wad:` for the picked champion. The
/// floor of that pair is the starting estimate, so the first decisions a fresh
/// install makes are the pessimistic ones.
///
/// See `docs/metrics/2026-09-16-el-overlay-se-lee-bajo-demanda.md`.
pub const OBSERVED_TAIL: Duration = Duration::from_millis(3689);

/// What a whole-WAD rebuild cost on the machine those measurements come from.
///
/// The 20 largest champion archives, each read cold into a profile whose game
/// index already existed: p50 498 ms, p95 553 ms. The p95 is the starting
/// estimate, replaced by the running machine's own as soon as it has rebuilt
/// anything.
pub const OBSERVED_REBUILD_P95: Duration = Duration::from_millis(553);

/// How much of the margin a rebuild may consume before a swap is refused.
///
/// A rebuild that fits with nothing to spare is a rebuild that misses whenever
/// the machine has a bad moment, and what it costs to be wrong is a game played
/// with the mod the reader did not choose. Half is arbitrary and deliberately
/// generous: the measured ratio is about seven to one, so halving the budget
/// still leaves three.
const USABLE_FRACTION: u32 = 2;

/// What the running machine has learned about its own timings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Budget {
    /// The slowest whole-WAD rebuild worth planning around.
    pub rebuild: Duration,
    /// The run after champion select ends.
    pub tail: Duration,
}

impl Default for Budget {
    fn default() -> Self {
        Self {
            rebuild: OBSERVED_REBUILD_P95,
            tail: OBSERVED_TAIL,
        }
    }
}

impl Budget {
    /// Whether a rebuild fits in `available`, keeping the spare the comment on
    /// [`USABLE_FRACTION`] argues for.
    pub fn fits(&self, available: Duration) -> bool {
        self.rebuild * USABLE_FRACTION <= available
    }

    /// What a swap needs before it will be attempted.
    pub fn needed(&self) -> Duration {
        self.rebuild * USABLE_FRACTION
    }

    /// Fold one observed rebuild into the estimate.
    ///
    /// The estimate only ever rises on its own. A single fast rebuild says the
    /// archive was small or the cache was warm, neither of which will be true
    /// next time, so a faster sample decays the estimate slowly while a slower
    /// one is taken at once. Being wrong upwards costs a refused swap, and
    /// being wrong downwards costs a game.
    pub fn observe_rebuild(&mut self, measured: Duration) {
        self.rebuild = if measured > self.rebuild {
            measured
        } else {
            (self.rebuild * 3 + measured) / 4
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_measured_margin_fits_a_measured_rebuild_many_times_over() {
        let budget = Budget::default();
        assert!(budget.fits(OBSERVED_TAIL));
        assert!(
            OBSERVED_TAIL.as_secs_f64() / budget.needed().as_secs_f64() > 3.0,
            "halving the budget still leaves three times what a rebuild costs"
        );
    }

    #[test]
    fn a_slow_machine_refuses_rather_than_gambles() {
        // A mechanical disk, five times slower on the copy that dominates.
        let mut budget = Budget::default();
        budget.observe_rebuild(Duration::from_millis(2800));
        assert!(
            !budget.fits(OBSERVED_TAIL),
            "no room left in the fixed tail"
        );
        assert!(
            budget.fits(Duration::from_secs(10)),
            "a draft still has room"
        );
    }

    #[test]
    fn a_slower_rebuild_is_believed_at_once_and_a_faster_one_slowly() {
        let mut budget = Budget::default();
        budget.observe_rebuild(Duration::from_millis(2000));
        assert_eq!(budget.rebuild, Duration::from_millis(2000));

        budget.observe_rebuild(Duration::from_millis(100));
        assert!(
            budget.rebuild > Duration::from_millis(1500),
            "one quick rebuild does not undo what a slow one taught"
        );
    }
}
