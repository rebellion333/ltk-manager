//! The champion roster the client itself publishes.
//!
//! `/lol-game-data/assets/v1/champion-summary.json` lists every champion with
//! the numeric id champion select speaks in and the `alias` the game's own WAD
//! files are named after (`MonkeyKing`, not `Wukong`). Cached beside the app's
//! data so the join works with the client closed, and refreshed whenever a
//! client answers, since a patch adds champions.

use std::path::Path;

use fs_err as fs;
use serde::{Deserialize, Serialize};

use super::client::LcuClient;

const SUMMARY_PATH: &str = "/lol-game-data/assets/v1/champion-summary.json";
const CACHE_FILE: &str = "champion-summary.json";

/// One champion as the client lists it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export))]
#[serde(rename_all = "camelCase")]
pub struct ChampionSummary {
    pub id: i32,
    pub name: String,
    /// The internal name, which is the stem of the champion's WAD.
    pub alias: String,
}

/// The roster, keyed both ways.
#[derive(Debug, Clone, Default)]
pub struct ChampionRoster {
    champions: Vec<ChampionSummary>,
}

impl ChampionRoster {
    /// Ask a live client, and keep the answer under `cache_dir`.
    pub fn fetch(client: &LcuClient, cache_dir: &Path) -> Option<Self> {
        let mut champions: Vec<ChampionSummary> = client.get_json(SUMMARY_PATH)?;
        // The client lists a placeholder with id -1 ("None") ahead of the roster.
        champions.retain(|c| c.id > 0);
        let roster = Self { champions };
        if let Err(e) = roster.save(cache_dir) {
            tracing::debug!("Could not cache the champion roster: {e}");
        }
        Some(roster)
    }

    /// The last roster a client answered with, from `cache_dir`.
    pub fn load(cache_dir: &Path) -> Option<Self> {
        let raw = fs::read_to_string(cache_dir.join(CACHE_FILE)).ok()?;
        let champions = serde_json::from_str(&raw).ok()?;
        Some(Self { champions })
    }

    fn save(&self, cache_dir: &Path) -> std::io::Result<()> {
        fs::create_dir_all(cache_dir)?;
        let raw = serde_json::to_string(&self.champions)?;
        crate::utils::fs::atomic_write(&cache_dir.join(CACHE_FILE), raw.as_bytes())
    }

    pub fn from_champions(champions: Vec<ChampionSummary>) -> Self {
        Self { champions }
    }

    pub fn by_id(&self, id: i32) -> Option<&ChampionSummary> {
        self.champions.iter().find(|c| c.id == id)
    }

    /// Case-insensitive on the alias, which is how a WAD stem compares.
    pub fn by_alias(&self, alias: &str) -> Option<&ChampionSummary> {
        self.champions
            .iter()
            .find(|c| c.alias.eq_ignore_ascii_case(alias))
    }

    pub fn len(&self) -> usize {
        self.champions.len()
    }

    pub fn is_empty(&self) -> bool {
        self.champions.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn roster() -> ChampionRoster {
        ChampionRoster::from_champions(vec![
            ChampionSummary {
                id: 62,
                name: "Wukong".into(),
                alias: "MonkeyKing".into(),
            },
            ChampionSummary {
                id: 157,
                name: "Yasuo".into(),
                alias: "Yasuo".into(),
            },
        ])
    }

    #[test]
    fn joins_by_id_and_by_wad_stem() {
        let r = roster();
        assert_eq!(r.by_id(62).unwrap().alias, "MonkeyKing");
        assert_eq!(r.by_alias("monkeyking").unwrap().name, "Wukong");
        assert!(r.by_id(1).is_none());
    }

    #[test]
    fn a_saved_roster_loads_back() {
        let dir = tempfile::tempdir().unwrap();
        roster().save(dir.path()).unwrap();
        let loaded = ChampionRoster::load(dir.path()).unwrap();
        assert_eq!(loaded.len(), 2);
        assert_eq!(loaded.by_id(157).unwrap().alias, "Yasuo");
    }
}
