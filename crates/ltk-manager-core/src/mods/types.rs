//! Types that cross the IPC boundary.
//!
//! These are the shapes the frontend sees: library entries, profiles, folders,
//! and bulk-install results. The on-disk index that backs them lives in
//! [`super::index`].

use crate::mods::index::{HarvestSummary, LibraryIndex, ModArchiveFormat, ModStorage};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Slugified profile name used as the filesystem directory name.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export))]
#[serde(transparent)]
pub struct ProfileSlug(pub String);

impl ProfileSlug {
    /// Create a slug from a profile name. Returns `None` if the name produces an empty slug.
    pub fn from_name(name: &str) -> Option<Self> {
        let s = slug::slugify(name);
        if s.is_empty() { None } else { Some(Self(s)) }
    }

    /// Check whether this slug is unique among profiles in the index.
    pub(crate) fn is_unique_in(&self, index: &LibraryIndex, exclude_id: Option<&str>) -> bool {
        !index
            .profiles
            .iter()
            .any(|p| p.slug == *self && exclude_id.is_none_or(|id| p.id != id))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for ProfileSlug {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl From<String> for ProfileSlug {
    fn from(s: String) -> Self {
        Self(s)
    }
}

/// A mod profile for organizing different mod configurations.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export))]
#[serde(rename_all = "camelCase")]
pub struct Profile {
    /// Unique identifier (UUID)
    pub id: String,
    /// User-friendly name
    pub name: String,
    /// Slugified name used as the filesystem directory name
    #[serde(default)]
    pub slug: ProfileSlug,
    /// List of mod IDs enabled in this profile (maintains overlay priority order)
    pub enabled_mods: Vec<String>,
    /// Display order of all mods (enabled and disabled) in the UI
    pub mod_order: Vec<String>,
    /// Per-mod layer enabled/disabled states: mod_id → (layer_name → enabled).
    #[serde(default)]
    pub layer_states: HashMap<String, HashMap<String, bool>>,
    /// What the reader wants applied to each champion, keyed by the champion's
    /// alias - `MonkeyKing`, never `Wukong` - because the alias is the one
    /// spelling the client does not localize.
    ///
    /// A record rather than a cache: nothing recomputes it, and losing it loses
    /// something the reader chose. It lives on the profile because a preference
    /// is about a set of enabled mods, and that set is what a profile is. See
    /// ADR-0041.
    #[serde(default)]
    pub champion_preferences: HashMap<String, ChampionPreference>,
    /// The mods the reader keeps within reach for each champion, in the order
    /// champion select offers them, keyed by the champion's alias.
    ///
    /// Beside [`champion_preferences`](Self::champion_preferences) rather than
    /// inside it, because the two answer different questions and only one of
    /// them is a decision. An entry in the preferences means the reader settled
    /// what the champion applies, and champion select acts on that; a favourite
    /// says only where a mod sits in a list. Held together, marking a favourite
    /// would write a preference, and a preference with no mod in it is the
    /// reader having chosen *no* mod - so ordering a list would switch mods
    /// off. Apart, neither can be mistaken for the other.
    #[serde(default)]
    pub champion_favorites: HashMap<String, Vec<String>>,
    /// Creation timestamp
    pub created_at: DateTime<Utc>,
    /// Last time this profile was used/switched to
    pub last_used: DateTime<Utc>,
}

/// What the reader settled for one champion.
///
/// **An entry existing is the decision.** A champion with no entry is one the
/// reader never spoke about, and champion select leaves it alone; a champion
/// with an entry is one they settled, and champion select acts on it every
/// game. Nothing that is not a decision belongs in here - see
/// [`Profile::champion_favorites`] for the other half.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS, specta::Type))]
#[cfg_attr(feature = "ts", ts(export))]
#[serde(rename_all = "camelCase", default)]
pub struct ChampionPreference {
    /// The mod applied when this champion is picked, if any.
    ///
    /// `None` means the champion has no mod of its own, which is different from
    /// having one that is currently disabled.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional = nullable))]
    pub preferred: Option<String>,
}

/// A mod layer shown in the UI.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export))]
#[serde(rename_all = "camelCase")]
pub struct ModLayer {
    pub name: String,
    #[serde(default)]
    pub display_name: String,
    pub priority: i32,
    pub enabled: bool,
}

/// What a mod says it is licensed under.
///
/// The name alone, which every mod's config already carries. A license's text
/// reaches disk for neither format and costs one archive mount, so it is read
/// where a reader asks to see it rather than beside every card.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export))]
#[serde(rename_all = "camelCase")]
pub struct ModLicense {
    /// An SPDX id, or the name a custom license gives itself.
    pub name: String,
    /// Where the full terms are, for a license that points anywhere.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional = nullable))]
    pub url: Option<String>,
}

impl From<&ltk_mod_project::ModProjectLicense> for ModLicense {
    fn from(license: &ltk_mod_project::ModProjectLicense) -> Self {
        match license {
            ltk_mod_project::ModProjectLicense::Spdx(id) => Self {
                name: id.clone(),
                url: None,
            },
            ltk_mod_project::ModProjectLicense::Custom { name, url } => Self {
                name: name.clone(),
                url: url.clone(),
            },
        }
    }
}

/// A mod entry shown in the UI Library.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export))]
#[serde(rename_all = "camelCase")]
pub struct InstalledMod {
    pub id: String,
    pub name: String,
    pub display_name: String,
    pub version: String,
    pub description: Option<String>,
    pub authors: Vec<String>,
    pub enabled: bool,
    pub installed_at: DateTime<Utc>,
    pub layers: Vec<ModLayer>,
    pub tags: Vec<String>,
    pub champions: Vec<String>,
    pub maps: Vec<String>,
    /// Directory where the mod is installed
    pub mod_dir: String,
    /// The file this mod arrived as.
    pub format: ModArchiveFormat,
    /// Where this mod's content is read from.
    pub storage: ModStorage,
    /// Whether the archive is still beside the mod, which is what makes
    /// [`storage`](Self::storage) switchable either way.
    pub has_archive: bool,
    /// ID of the containing folder, or None if ungrouped.
    pub folder_id: Option<String>,
    /// What the mod's config declares it is licensed under, if it declares one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional = nullable))]
    pub license: Option<ModLicense>,
    /// The mod's directory name under `mods/`.
    ///
    /// `None` while the mod is still in the legacy layout the migration has
    /// not moved it out of.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional = nullable))]
    pub slug: Option<String>,
    /// What preserving the mod's names at import found. `None` for a modpkg
    /// and for mods installed before the preserve existed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional = nullable))]
    pub harvest: Option<HarvestSummary>,
}

/// Fields to change on a mod's metadata. `None` leaves a field untouched.
#[derive(Debug, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export))]
#[serde(rename_all = "camelCase")]
pub struct EditModMetadataArgs {
    pub display_name: Option<String>,
    pub tags: Option<Vec<String>>,
    pub champions: Option<Vec<String>>,
    pub maps: Option<Vec<String>>,
    #[serde(default)]
    pub set_thumbnail_path: Option<String>,
    #[serde(default)]
    pub remove_thumbnail: Option<bool>,
}

/// A named folder for grouping mods in the library.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export))]
#[serde(rename_all = "camelCase")]
pub struct LibraryFolder {
    pub id: String,
    pub name: String,
    pub mod_ids: Vec<String>,
}

/// Sentinel ID for the implicit root folder that holds ungrouped mods.
pub const ROOT_FOLDER_ID: &str = "root";

/// Result of a bulk mod install operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export))]
#[serde(rename_all = "camelCase")]
pub struct BulkInstallResult {
    pub installed: Vec<InstalledMod>,
    pub failed: Vec<BulkInstallError>,
}

/// Error info for a single file that failed during bulk install.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export))]
#[serde(rename_all = "camelCase")]
pub struct BulkInstallError {
    pub file_path: String,
    pub file_name: String,
    pub message: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn profile_slug_from_name_normal() {
        let slug = ProfileSlug::from_name("My Profile").unwrap();
        assert_eq!(slug.as_str(), "my-profile");
    }

    #[test]
    fn profile_slug_from_name_special_chars() {
        let slug = ProfileSlug::from_name("Test & Profile #1").unwrap();
        assert!(!slug.as_str().is_empty());
        assert!(
            slug.as_str()
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        );
    }

    #[test]
    fn profile_slug_from_name_empty_returns_none() {
        assert!(ProfileSlug::from_name("").is_none());
    }

    #[test]
    fn profile_slug_from_name_whitespace_only_returns_none() {
        assert!(ProfileSlug::from_name("   ").is_none());
    }

    #[test]
    fn profile_slug_from_name_symbols_only_returns_none() {
        assert!(ProfileSlug::from_name("!!!").is_none());
    }

    #[test]
    fn profile_slug_is_unique_in_no_profiles() {
        let index = LibraryIndex {
            version: 0,
            mods: Vec::new(),
            profiles: Vec::new(),
            active_profile_id: String::new(),
            folders: vec![LibraryFolder {
                id: ROOT_FOLDER_ID.to_string(),
                name: String::new(),
                mod_ids: Vec::new(),
            }],
            folder_order: vec![ROOT_FOLDER_ID.to_string()],
        };
        let slug = ProfileSlug("test".to_string());
        assert!(slug.is_unique_in(&index, None));
    }

    #[test]
    fn profile_slug_is_unique_in_with_different_slugs() {
        let index = LibraryIndex {
            version: 0,
            mods: Vec::new(),
            profiles: vec![Profile {
                id: "p1".to_string(),
                name: "Default".to_string(),
                slug: ProfileSlug("default".to_string()),
                enabled_mods: Vec::new(),
                mod_order: Vec::new(),
                layer_states: HashMap::new(),
                champion_preferences: HashMap::new(),
                champion_favorites: Default::default(),
                created_at: Utc::now(),
                last_used: Utc::now(),
            }],
            active_profile_id: "p1".to_string(),
            folders: vec![LibraryFolder {
                id: ROOT_FOLDER_ID.to_string(),
                name: String::new(),
                mod_ids: Vec::new(),
            }],
            folder_order: vec![ROOT_FOLDER_ID.to_string()],
        };
        let slug = ProfileSlug("my-profile".to_string());
        assert!(slug.is_unique_in(&index, None));
    }

    #[test]
    fn profile_slug_is_not_unique_when_duplicate() {
        let index = LibraryIndex {
            version: 0,
            mods: Vec::new(),
            profiles: vec![Profile {
                id: "p1".to_string(),
                name: "Default".to_string(),
                slug: ProfileSlug("default".to_string()),
                enabled_mods: Vec::new(),
                mod_order: Vec::new(),
                layer_states: HashMap::new(),
                champion_preferences: HashMap::new(),
                champion_favorites: Default::default(),
                created_at: Utc::now(),
                last_used: Utc::now(),
            }],
            active_profile_id: "p1".to_string(),
            folders: vec![LibraryFolder {
                id: ROOT_FOLDER_ID.to_string(),
                name: String::new(),
                mod_ids: Vec::new(),
            }],
            folder_order: vec![ROOT_FOLDER_ID.to_string()],
        };
        let slug = ProfileSlug("default".to_string());
        assert!(!slug.is_unique_in(&index, None));
    }

    #[test]
    fn profile_slug_is_unique_when_excluded() {
        let index = LibraryIndex {
            version: 0,
            mods: Vec::new(),
            profiles: vec![Profile {
                id: "p1".to_string(),
                name: "Default".to_string(),
                slug: ProfileSlug("default".to_string()),
                enabled_mods: Vec::new(),
                mod_order: Vec::new(),
                layer_states: HashMap::new(),
                champion_preferences: HashMap::new(),
                champion_favorites: Default::default(),
                created_at: Utc::now(),
                last_used: Utc::now(),
            }],
            active_profile_id: "p1".to_string(),
            folders: vec![LibraryFolder {
                id: ROOT_FOLDER_ID.to_string(),
                name: String::new(),
                mod_ids: Vec::new(),
            }],
            folder_order: vec![ROOT_FOLDER_ID.to_string()],
        };
        let slug = ProfileSlug("default".to_string());
        assert!(slug.is_unique_in(&index, Some("p1")));
    }

    #[test]
    fn create_profile_slug_generation() {
        let slug = ProfileSlug::from_name("My Test Profile").unwrap();
        assert_eq!(slug.as_str(), "my-test-profile");
    }

    #[test]
    fn create_profile_symbols_only_name_rejected() {
        assert!(ProfileSlug::from_name("!!!").is_none());
    }

    #[test]
    fn profile_slug_uniqueness_check() {
        let mut index = LibraryIndex::default();
        index.profiles.push(Profile {
            id: "p2".to_string(),
            name: "My Profile".to_string(),
            slug: ProfileSlug::from_name("My Profile").unwrap(),
            enabled_mods: Vec::new(),
            mod_order: Vec::new(),
            layer_states: HashMap::new(),
            champion_preferences: HashMap::new(),
            champion_favorites: Default::default(),
            created_at: Utc::now(),
            last_used: Utc::now(),
        });

        let slug = ProfileSlug::from_name("My Profile").unwrap();
        assert!(!slug.is_unique_in(&index, None));
        assert!(slug.is_unique_in(&index, Some("p2")));
    }
}
