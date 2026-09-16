//! Performing the swap, once [`super::decide`] has allowed one.
//!
//! Rule 0 of ADR-0043 in code: a build a champion select triggers always writes
//! whole archives through a temp file and a rename, never the in-place tail
//! rewrite. The builder picks the in-place path only for an archive it holds a
//! layout record for, so dropping those records is what makes the atomic path
//! the only one available.
//!
//! Why it has to be the only one: `rewrite_one_wad_tail` truncates the overlay
//! archive and writes over the same bytes, with no temp file to discard. A game
//! that opens that archive mid-write reads a truncated file, which is a mount
//! the client dies on. Nothing can be done about that after the fact - by the
//! time a swap could be cancelled, `set_len` has already run - so the rule is a
//! property of the write rather than a reaction to it.
//!
//! The cost of the guarantee, measured on 2026-09-16: 296 ms against 272 ms for
//! the builder's own choice, on the same archive. Nine percent.

use std::path::Path;
use std::time::{Duration, Instant};

use camino::Utf8PathBuf;

use crate::config::Config;
use crate::error::{AppResult, Utf8PathExt};
use crate::mods::ModLibrary;

use super::budget::Budget;

/// What one swap did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SwapOutcome {
    /// How long the rebuild took, which is what the budget learns from.
    pub rebuilt_in: Duration,
    /// Layout records dropped so the build could not take the in-place path.
    pub layouts_forgotten: usize,
}

/// Drop every `wadLayouts` record the profile holds.
///
/// Only that map. `wadFingerprints` stays, so archives the swap does not touch
/// are still skipped and the build costs one whole archive rather than the
/// profile. A record is documented upstream as "a hint, never a fact", which is
/// what makes removing one a supported thing to do rather than a trick.
///
/// A state file that will not load is left alone and reported as zero: the
/// builder treats an unreadable state as no state, which forces a full rebuild
/// anyway, so the failure mode already lands on the safe side.
pub fn forget_wad_layouts(state_dir: &Path) -> AppResult<usize> {
    let path: Utf8PathBuf = state_dir
        .join("overlay.json")
        .try_into_utf8("overlay state file")?;

    let Some(mut state) = ltk_overlay::OverlayState::load(&path).unwrap_or_else(|e| {
        tracing::warn!(
            "Overlay state at {path} could not be read ({e}), so the build rebuilds whole anyway"
        );
        None
    }) else {
        return Ok(0);
    };

    let forgotten = state.wad_layouts.len();
    if forgotten == 0 {
        return Ok(0);
    }
    state.wad_layouts.clear();
    state.save(&path)?;
    tracing::info!(
        "Dropped {forgotten} overlay layout record(s), so the rebuild writes whole archives"
    );
    Ok(forgotten)
}

impl ModLibrary {
    /// The directory a profile keeps its overlay and its build state in.
    pub fn profile_dir(&self, config: &Config) -> AppResult<std::path::PathBuf> {
        let storage_dir = self.storage_dir(config)?;
        let (slug, _) = self.get_enabled_mods_for_overlay(config)?;
        Ok(storage_dir.join("profiles").join(slug.as_str()))
    }

    /// Rebuild the active profile's overlay the way a champion select swap
    /// must: whole archives only, timed, with what it learned handed back.
    ///
    /// The caller has already asked [`super::decide`] and been allowed. This
    /// does not ask again: the decision reads a champion select that may have
    /// moved by the time the build starts, and re-deciding here would be a
    /// second answer to a question that was already answered, from worse
    /// information.
    ///
    /// `budget` is updated with how long the rebuild actually took, so the
    /// machine's own timing is what the next decision uses.
    pub fn rebuild_for_swap(&self, config: &Config, budget: &mut Budget) -> AppResult<SwapOutcome> {
        let profile_dir = self.profile_dir(config)?;
        let layouts_forgotten = forget_wad_layouts(&profile_dir)?;

        let started = Instant::now();
        let build = self.ensure_overlay(config, &[], false)?;
        let rebuilt_in = started.elapsed();

        self.record_overlay_build(build.outcome);
        budget.observe_rebuild(rebuilt_in);

        tracing::info!(
            "Swap rebuilt the overlay in {} ms, and the budget now plans for {} ms",
            rebuilt_in.as_millis(),
            budget.rebuild.as_millis()
        );
        Ok(SwapOutcome {
            rebuilt_in,
            layouts_forgotten,
        })
    }
}

#[cfg(test)]
mod tests;
