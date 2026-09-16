//! Which mod applies to a champion, and changing it.
//!
//! A swap is two writes and a build. This module is the first write: the
//! profile learns which mod the reader wants for a champion, and its enabled
//! set is brought in line with that. The build is [`super::swap`].
//!
//! The guard `reject_if_patcher_running` is untouched. It stops the ordinary
//! library mutations for a good reason, and nothing here relaxes it: this is a
//! separate path whose own preconditions live in [`super::decision`], and the
//! command that calls it asks those first.

use crate::config::Config;
use crate::error::{AppError, AppResult};
use crate::mods::ModLibrary;
use crate::mods::{ChampionPreference, champion_display_name, norm_key};

/// What changing a champion's mod did to the enabled set.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PreferenceChange {
    /// Mods switched off because they belong to the same champion.
    pub disabled: Vec<String>,
    /// The mod switched on, if one was.
    pub enabled: Option<String>,
    /// Whether anything actually moved in the profile.
    ///
    /// **Not a reason to skip a rebuild.** It describes the profile, and
    /// whether the overlay already carries the mod is a different question
    /// with a different answer: anything that wrote the preference a moment
    /// earlier leaves this `false` over an overlay that is stale. Reading it
    /// as "the overlay is fine" once shipped an empty overlay under a swap
    /// reported as applied.
    pub changed: bool,
}

impl ModLibrary {
    /// Every installed mod that applies to `alias`, in library order.
    ///
    /// Matching is on the champion's **alias** rather than its name. A mod is
    /// categorized under a display name derived from the game's own WAD stem,
    /// and the client answers with both an alias and a name - but the name
    /// arrives in the reader's language, so joining on it would work in English
    /// and quietly fail everywhere else.
    pub fn mods_for_champion(&self, config: &Config, alias: &str) -> AppResult<Vec<String>> {
        let wanted = norm_key(&champion_display_name(alias));
        let reports = self.wad_reports().0.lock().get_all();

        Ok(self
            .get_installed_mods(config)?
            .into_iter()
            .filter(|installed| {
                let declared = installed.champions.iter();
                let derived = reports
                    .get(&installed.id)
                    .into_iter()
                    .flat_map(|report| report.derived.champions.iter());
                declared
                    .chain(derived)
                    .any(|champion| norm_key(champion) == wanted)
            })
            .map(|installed| installed.id)
            .collect())
    }

    /// The preferences the active profile holds.
    pub fn champion_preferences(
        &self,
        config: &Config,
    ) -> AppResult<std::collections::HashMap<String, ChampionPreference>> {
        self.with_index(config, |_storage, index| {
            let active = index.active_profile_id.clone();
            Ok(index
                .profiles
                .iter()
                .find(|profile| profile.id == active)
                .map(|profile| profile.champion_preferences.clone())
                .unwrap_or_default())
        })
    }

    /// Apply `mod_id` to `alias`, or `None` to leave the champion unmodded.
    ///
    /// Two things happen together, which is why they are one call: the profile
    /// records what the reader chose, and its enabled set stops carrying any
    /// other mod for that champion. Recording without applying would leave the
    /// preference a lie until the next build, and applying without recording
    /// would lose the choice the moment anything else rewrote the profile.
    ///
    /// Mods for *other* champions are left alone. A profile is a whole set and
    /// a swap is about one champion of it.
    ///
    /// # Errors
    ///
    /// [`AppError::ModNotFound`] when `mod_id` names a mod the library does not
    /// hold, or one that does not apply to `alias`. A preference pointing at a
    /// mod that cannot serve it would refuse silently at build time, which is
    /// the sort of thing that gets debugged twice.
    pub fn apply_champion_preference(
        &self,
        config: &Config,
        alias: &str,
        mod_id: Option<&str>,
    ) -> AppResult<PreferenceChange> {
        let candidates = self.mods_for_champion(config, alias)?;
        if let Some(wanted) = mod_id
            && !candidates.iter().any(|id| id == wanted)
        {
            return Err(AppError::ModNotFound(format!(
                "{wanted} is not a mod for {alias}"
            )));
        }

        self.mutate_index(config, |_storage, index| {
            let active = index.active_profile_id.clone();
            let profile = index
                .profiles
                .iter_mut()
                .find(|profile| profile.id == active)
                .ok_or_else(|| AppError::Other("Active profile not found".to_string()))?;

            let mut change = PreferenceChange::default();

            for candidate in &candidates {
                if Some(candidate.as_str()) == mod_id {
                    continue;
                }
                if profile.enabled_mods.iter().any(|id| id == candidate) {
                    profile.enabled_mods.retain(|id| id != candidate);
                    change.disabled.push(candidate.clone());
                }
            }

            if let Some(wanted) = mod_id
                && !profile.enabled_mods.iter().any(|id| id == wanted)
            {
                // Appended, so the mod a swap turns on takes the priority a
                // newly enabled mod always takes rather than inheriting the
                // position of the one it replaced.
                profile.enabled_mods.push(wanted.to_string());
                change.enabled = Some(wanted.to_string());
            }

            let entry = profile
                .champion_preferences
                .entry(alias.to_string())
                .or_default();
            let was = entry.preferred.clone();
            entry.preferred = mod_id.map(str::to_string);

            change.changed =
                !change.disabled.is_empty() || change.enabled.is_some() || was != entry.preferred;
            Ok(change)
        })
    }
}

#[cfg(test)]
mod tests;
