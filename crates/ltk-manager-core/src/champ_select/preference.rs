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
            .filter(|installed| applies_to(installed, &reports, &wanted))
            .map(|installed| installed.id)
            .collect())
    }

    /// The mods for each of `aliases`, in library order, skipping the ones with
    /// none.
    ///
    /// One read of the index for the whole roster.
    /// [`mods_for_champion`](Self::mods_for_champion) reads it once per call,
    /// so asking it about two hundred and forty champions would read the
    /// library two hundred and forty times.
    pub fn mods_by_champion(
        &self,
        config: &Config,
        aliases: &[String],
    ) -> AppResult<std::collections::HashMap<String, Vec<String>>> {
        let installed = self.get_installed_mods(config)?;
        let reports = self.wad_reports().0.lock().get_all();

        Ok(aliases
            .iter()
            .filter_map(|alias| {
                let wanted = norm_key(&champion_display_name(alias));
                let ids: Vec<String> = installed
                    .iter()
                    .filter(|m| applies_to(m, &reports, &wanted))
                    .map(|m| m.id.clone())
                    .collect();
                (!ids.is_empty()).then(|| (alias.clone(), ids))
            })
            .collect())
    }

    /// What the active profile wants per champion, minus what nothing can serve.
    ///
    /// A preference names a mod by id, and a mod the reader uninstalled leaves
    /// that id naming nothing. Left in, every champion select for that champion
    /// asks for a mod the library does not hold, the swap fails, and the panel
    /// reports a failure the reader can do nothing about - for good, since a
    /// reinstall issues a fresh id. Uninstalling a mod is an ordinary thing to
    /// do, so this is a state the app has to absorb rather than report.
    ///
    /// **The whole entry goes, not just its mod.** An entry whose `preferred`
    /// is `None` is the reader having chosen no mod, which disables the ones
    /// they have; clearing the field would turn a dangling preference into that
    /// choice and switch off mods nobody asked to switch off. An absent entry
    /// is silence, and silence leaves the champion alone.
    ///
    /// Nothing is written back. The next preference for that champion
    /// overwrites the dangling entry, and until then it is a few dead bytes in
    /// `library.json` that nothing reads. A read that quietly rewrites the
    /// library is a worse trade than that.
    ///
    /// Favourites are pruned separately, in
    /// [`champion_favorites`](Self::champion_favorites), and losing a
    /// preference never costs the reader the list they built.
    pub fn champion_preferences(
        &self,
        config: &Config,
    ) -> AppResult<std::collections::HashMap<String, ChampionPreference>> {
        let recorded = self.recorded_champion_preferences(config)?;
        if recorded.is_empty() {
            return Ok(recorded);
        }

        /* One read of the index for every champion rather than one each:
        `mods_for_champion` reads the whole library, and a reader with twenty
        champions set would pay for it twenty times. */
        let installed = self.get_installed_mods(config)?;
        let reports = self.wad_reports().0.lock().get_all();

        Ok(recorded
            .into_iter()
            .filter(|(alias, preference)| {
                let Some(wanted_mod) = preference.preferred.as_deref() else {
                    return true;
                };
                let wanted_champion = norm_key(&champion_display_name(alias));
                installed
                    .iter()
                    .any(|m| m.id == wanted_mod && applies_to(m, &reports, &wanted_champion))
            })
            .collect())
    }

    /// The mods the reader keeps within reach, per champion, in their order.
    ///
    /// Pruned like the preferences are, and for the same reason: a favourite
    /// naming an uninstalled mod would draw a row for something the library
    /// does not hold. A champion whose favourites all go is dropped rather than
    /// left with an empty list, so the map holds only what it can offer.
    pub fn champion_favorites(
        &self,
        config: &Config,
    ) -> AppResult<std::collections::HashMap<String, Vec<String>>> {
        let recorded = self.with_index(config, |_storage, index| {
            let active = index.active_profile_id.clone();
            Ok(index
                .profiles
                .iter()
                .find(|profile| profile.id == active)
                .map(|profile| profile.champion_favorites.clone())
                .unwrap_or_default())
        })?;
        if recorded.is_empty() {
            return Ok(recorded);
        }

        let installed = self.get_installed_mods(config)?;
        let reports = self.wad_reports().0.lock().get_all();

        Ok(recorded
            .into_iter()
            .filter_map(|(alias, favorites)| {
                let wanted = norm_key(&champion_display_name(&alias));
                let kept: Vec<String> = favorites
                    .into_iter()
                    .filter(|id| {
                        installed
                            .iter()
                            .any(|m| &m.id == id && applies_to(m, &reports, &wanted))
                    })
                    .collect();
                (!kept.is_empty()).then_some((alias, kept))
            })
            .collect())
    }

    /// Put `mod_id` among `alias`'s favourites, or take it out.
    ///
    /// Appended rather than inserted, so the order is the one the reader built
    /// by marking them. Nothing about the enabled set moves and no rebuild
    /// follows: this is where a mod sits in a list, not what the champion
    /// applies.
    ///
    /// # Errors
    ///
    /// [`AppError::ModNotFound`] when `mod_id` is not a mod for `alias`. A
    /// favourite that cannot be offered is one the reader would never see
    /// again, and failing here says so while they are still looking.
    pub fn set_champion_favorite(
        &self,
        config: &Config,
        alias: &str,
        mod_id: &str,
        favorite: bool,
    ) -> AppResult<()> {
        if favorite
            && !self
                .mods_for_champion(config, alias)?
                .iter()
                .any(|id| id == mod_id)
        {
            return Err(AppError::ModNotFound(format!(
                "{mod_id} is not a mod for {alias}"
            )));
        }

        self.mutate_index(config, |_storage, index| {
            let active = index.active_profile_id.clone();
            let profile = index
                .profiles
                .iter_mut()
                .find(|profile| profile.id == active)
                .ok_or_else(|| AppError::Other("Active profile not found".to_string()))?;

            if !favorite {
                /* The champion's entry goes with its last favourite, so an
                empty list is never a thing the map holds. */
                if let Some(favorites) = profile.champion_favorites.get_mut(alias) {
                    favorites.retain(|id| id != mod_id);
                    if favorites.is_empty() {
                        profile.champion_favorites.remove(alias);
                    }
                }
                return Ok(());
            }

            let favorites = profile
                .champion_favorites
                .entry(alias.to_string())
                .or_default();
            if !favorites.iter().any(|id| id == mod_id) {
                favorites.push(mod_id.to_string());
            }
            Ok(())
        })
    }

    /// The preferences as the profile holds them, dangling ones and all.
    fn recorded_champion_preferences(
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

/// Whether `installed` is categorized under `wanted`, a normalized champion key.
///
/// A mod declares its champions and the WAD scan derives more from what it
/// actually writes. Either is enough, which is what makes a mod that declares
/// nothing still findable.
fn applies_to(
    installed: &crate::mods::InstalledMod,
    reports: &std::collections::HashMap<String, crate::mods::ModWadReport>,
    wanted: &str,
) -> bool {
    let declared = installed.champions.iter();
    let derived = reports
        .get(&installed.id)
        .into_iter()
        .flat_map(|report| report.derived.champions.iter());
    declared
        .chain(derived)
        .any(|champion| norm_key(champion) == wanted)
}
