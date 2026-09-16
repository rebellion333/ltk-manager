use super::*;

use crate::mods::test_support::{
    make_slugged_entry, make_test_library, make_test_profile, place_installed_mod, seed_library,
};
use crate::mods::{ModArchiveFormat, ModWadReport};

/// A library holding `mods`, none of them enabled, and a WAD report per mod
/// naming the champions it was categorized under.
///
/// The mods are placed on disk as well as written into the index. An entry with
/// no content is skipped by `get_installed_mods`, which makes every assertion
/// below pass for the wrong reason: a first version of this helper seeded the
/// index alone, and the two tests that expected an empty answer were the only
/// ones that passed.
fn library_with(
    storage: &std::path::Path,
    mods: &[(&str, &[&str])],
) -> (crate::mods::ModLibrary, Config) {
    let (library, config) = make_test_library(storage);
    let entries: Vec<_> = mods
        .iter()
        .map(|(id, _)| {
            place_installed_mod(storage, id, ModArchiveFormat::Modpkg, true);
            make_slugged_entry(id, id, ModArchiveFormat::Modpkg)
        })
        .collect();
    seed_library(&library, &config, entries);

    // `seed_library` enables everything, and these tests are about what a swap
    // turns on and off, so the profile starts with nothing on.
    library
        .mutate_index(&config, |_storage, index| {
            let ids: Vec<&str> = mods.iter().map(|(id, _)| *id).collect();
            index.profiles = vec![make_test_profile("p1", "Default", ids, Vec::new())];
            index.active_profile_id = "p1".to_string();
            Ok(())
        })
        .unwrap();

    let reports: Vec<ModWadReport> = mods
        .iter()
        .map(|(id, champions)| ModWadReport {
            mod_id: (*id).to_string(),
            affected_wads: Vec::new(),
            wad_count: 0,
            override_count: 0,
            content_fingerprint: None,
            game_index_fingerprint: 0,
            computed_at: String::new(),
            is_stale: false,
            derived: crate::mods::DerivedCategorization {
                champions: champions.iter().map(|c| (*c).to_string()).collect(),
                maps: Vec::new(),
                tags: Vec::new(),
                primary_champion: champions.first().map(|c| (*c).to_string()),
            },
        })
        .collect();
    library.wad_reports().0.lock().upsert_many(reports).unwrap();

    (library, config)
}

fn enabled(library: &crate::mods::ModLibrary, config: &Config) -> Vec<String> {
    library
        .with_index(config, |_storage, index| {
            let active = index.active_profile_id.clone();
            Ok(index
                .profiles
                .iter()
                .find(|p| p.id == active)
                .unwrap()
                .enabled_mods
                .clone())
        })
        .unwrap()
}

#[test]
fn a_mod_is_found_by_the_champions_alias_not_its_name() {
    let dir = tempfile::tempdir().unwrap();
    // Categorized as Wukong, which is what the WAD stem `MonkeyKing` derives to.
    let (library, config) = library_with(dir.path(), &[("wukong-mod", &["Wukong"])]);

    assert_eq!(
        library.mods_for_champion(&config, "MonkeyKing").unwrap(),
        vec!["wukong-mod".to_string()],
        "the client names this champion MonkeyKing and nothing else joins the two"
    );
}

#[test]
fn a_champion_with_no_mods_finds_none() {
    let dir = tempfile::tempdir().unwrap();
    let (library, config) = library_with(dir.path(), &[("garen-mod", &["Garen"])]);
    assert!(
        library
            .mods_for_champion(&config, "Teemo")
            .unwrap()
            .is_empty()
    );
}

#[test]
fn applying_a_preference_turns_one_on_and_the_others_off() {
    let dir = tempfile::tempdir().unwrap();
    let (library, config) = library_with(
        dir.path(),
        &[
            ("garen-a", &["Garen"]),
            ("garen-b", &["Garen"]),
            ("teemo-a", &["Teemo"]),
        ],
    );

    library
        .apply_champion_preference(&config, "Garen", Some("garen-a"))
        .unwrap();
    library
        .apply_champion_preference(&config, "Teemo", Some("teemo-a"))
        .unwrap();
    assert_eq!(enabled(&library, &config), vec!["garen-a", "teemo-a"]);

    let change = library
        .apply_champion_preference(&config, "Garen", Some("garen-b"))
        .unwrap();

    assert_eq!(change.disabled, vec!["garen-a".to_string()]);
    assert_eq!(change.enabled, Some("garen-b".to_string()));
    assert!(change.changed);
    assert_eq!(
        enabled(&library, &config),
        vec!["teemo-a", "garen-b"],
        "the other champion's mod is untouched"
    );
}

#[test]
fn the_choice_is_recorded_on_the_profile() {
    let dir = tempfile::tempdir().unwrap();
    let (library, config) = library_with(dir.path(), &[("garen-a", &["Garen"])]);

    library
        .apply_champion_preference(&config, "Garen", Some("garen-a"))
        .unwrap();

    let preferences = library.champion_preferences(&config).unwrap();
    assert_eq!(
        preferences.get("Garen").and_then(|p| p.preferred.clone()),
        Some("garen-a".to_string())
    );
}

#[test]
fn choosing_nothing_leaves_the_champion_unmodded() {
    let dir = tempfile::tempdir().unwrap();
    let (library, config) = library_with(dir.path(), &[("garen-a", &["Garen"])]);
    library
        .apply_champion_preference(&config, "Garen", Some("garen-a"))
        .unwrap();

    let change = library
        .apply_champion_preference(&config, "Garen", None)
        .unwrap();

    assert_eq!(change.disabled, vec!["garen-a".to_string()]);
    assert_eq!(change.enabled, None);
    assert!(enabled(&library, &config).is_empty());
    assert_eq!(
        library
            .champion_preferences(&config)
            .unwrap()
            .get("Garen")
            .and_then(|p| p.preferred.clone()),
        None
    );
}

/// Choosing what is already applied is not a change, and a swap that changes
/// nothing is a rebuild not worth spending champion select on.
#[test]
fn applying_the_same_mod_twice_reports_no_change() {
    let dir = tempfile::tempdir().unwrap();
    let (library, config) = library_with(dir.path(), &[("garen-a", &["Garen"])]);

    assert!(
        library
            .apply_champion_preference(&config, "Garen", Some("garen-a"))
            .unwrap()
            .changed
    );
    assert!(
        !library
            .apply_champion_preference(&config, "Garen", Some("garen-a"))
            .unwrap()
            .changed
    );
}

/// A preference pointing at a mod that cannot serve the champion would refuse
/// silently at build time, so it refuses loudly here instead.
#[test]
fn a_mod_that_is_not_for_this_champion_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let (library, config) = library_with(
        dir.path(),
        &[("garen-a", &["Garen"]), ("teemo-a", &["Teemo"])],
    );

    let error = library
        .apply_champion_preference(&config, "Garen", Some("teemo-a"))
        .unwrap_err();
    assert!(matches!(error, AppError::ModNotFound(_)));
    assert!(enabled(&library, &config).is_empty(), "nothing was written");
}

/// Uninstalling a mod is ordinary, and a reinstall issues a fresh id, so a
/// preference left pointing at the old one would fail every champion select for
/// that champion for good. It reads as silence instead.
#[test]
fn a_preference_for_an_uninstalled_mod_is_not_returned() {
    let dir = tempfile::tempdir().unwrap();
    let (library, config) = library_with(dir.path(), &[("garen-a", &["Garen"])]);
    library
        .apply_champion_preference(&config, "Garen", Some("garen-a"))
        .unwrap();

    // The mod goes, the preference stays behind pointing at nothing.
    library
        .mutate_index(&config, |_storage, index| {
            index.mods.retain(|entry| entry.id != "garen-a");
            Ok(())
        })
        .unwrap();

    /* That the profile still holds it, first. Without this the assertion below
    passes just as well on a fixture that never wrote the preference at all,
    which is how three tests in this module once passed for the wrong
    reason. */
    assert!(
        library
            .recorded_champion_preferences(&config)
            .unwrap()
            .contains_key("Garen"),
        "the dangling preference is on the profile, which is the state under test"
    );

    let preferences = library.champion_preferences(&config).unwrap();
    assert!(
        !preferences.contains_key("Garen"),
        "the whole entry goes: an entry whose mod is None is the reader having \
         chosen no mod, which would switch off mods nobody asked to switch off"
    );
}

/// The other half: an explicit "no mod" is a decision and survives, because
/// nothing about it can dangle.
#[test]
fn a_preference_for_no_mod_at_all_survives() {
    let dir = tempfile::tempdir().unwrap();
    let (library, config) = library_with(dir.path(), &[("garen-a", &["Garen"])]);
    library
        .apply_champion_preference(&config, "Garen", None)
        .unwrap();

    let preferences = library.champion_preferences(&config).unwrap();
    assert!(preferences.contains_key("Garen"));
    assert_eq!(preferences["Garen"].preferred, None);
}

/// A mod that is still installed but no longer categorized under the champion
/// cannot serve the preference either, and `apply` would refuse it.
#[test]
fn a_preference_for_a_mod_that_no_longer_fits_the_champion_is_not_returned() {
    let dir = tempfile::tempdir().unwrap();
    let (library, config) = library_with(dir.path(), &[("garen-a", &["Garen"])]);
    library
        .apply_champion_preference(&config, "Garen", Some("garen-a"))
        .unwrap();

    library
        .wad_reports()
        .0
        .lock()
        .upsert_many(vec![ModWadReport {
            mod_id: "garen-a".to_string(),
            affected_wads: Vec::new(),
            wad_count: 0,
            override_count: 0,
            content_fingerprint: None,
            game_index_fingerprint: 0,
            computed_at: String::new(),
            is_stale: false,
            derived: crate::mods::DerivedCategorization {
                champions: vec!["Kayn".to_string()],
                maps: Vec::new(),
                tags: Vec::new(),
                primary_champion: Some("Kayn".to_string()),
            },
        }])
        .unwrap();

    let preferences = library.champion_preferences(&config).unwrap();
    assert!(!preferences.contains_key("Garen"));
}

/// Marking a favourite must not write a preference. If it did, ordering a list
/// would read as "this champion applies no mod" and switch off what the reader
/// had, which is the reason the two live apart on the profile.
#[test]
fn a_favorite_is_not_a_decision() {
    let dir = tempfile::tempdir().unwrap();
    let (library, config) = library_with(dir.path(), &[("garen-a", &["Garen"])]);
    library
        .mutate_index(&config, |_storage, index| {
            index.profiles[0].enabled_mods = vec!["garen-a".to_string()];
            Ok(())
        })
        .unwrap();

    library
        .set_champion_favorite(&config, "Garen", "garen-a", true)
        .unwrap();

    assert!(
        !library
            .champion_preferences(&config)
            .unwrap()
            .contains_key("Garen"),
        "no decision was made, so champion select has nothing to act on"
    );
    assert_eq!(
        enabled(&library, &config),
        vec!["garen-a"],
        "and nothing was switched off"
    );
}

#[test]
fn favorites_keep_the_order_they_were_marked_in() {
    let dir = tempfile::tempdir().unwrap();
    let (library, config) = library_with(
        dir.path(),
        &[
            ("garen-a", &["Garen"]),
            ("garen-b", &["Garen"]),
            ("garen-c", &["Garen"]),
        ],
    );

    for id in ["garen-c", "garen-a"] {
        library
            .set_champion_favorite(&config, "Garen", id, true)
            .unwrap();
    }

    assert_eq!(
        library.champion_favorites(&config).unwrap()["Garen"],
        vec!["garen-c".to_string(), "garen-a".to_string()],
        "library order is not the reader's order"
    );
}

/// Unmarking the last one takes the champion with it, so an empty list is never
/// a thing the map holds and callers have one shape to read.
#[test]
fn unmarking_the_last_favorite_drops_the_champion() {
    let dir = tempfile::tempdir().unwrap();
    let (library, config) = library_with(dir.path(), &[("garen-a", &["Garen"])]);
    library
        .set_champion_favorite(&config, "Garen", "garen-a", true)
        .unwrap();

    library
        .set_champion_favorite(&config, "Garen", "garen-a", false)
        .unwrap();

    assert!(
        !library
            .champion_favorites(&config)
            .unwrap()
            .contains_key("Garen")
    );
}

/// The same pruning the preferences get: a favourite naming a mod the library
/// no longer holds would draw a row for nothing.
#[test]
fn a_favorite_for_an_uninstalled_mod_is_not_returned() {
    let dir = tempfile::tempdir().unwrap();
    let (library, config) = library_with(
        dir.path(),
        &[("garen-a", &["Garen"]), ("garen-b", &["Garen"])],
    );
    for id in ["garen-a", "garen-b"] {
        library
            .set_champion_favorite(&config, "Garen", id, true)
            .unwrap();
    }

    library
        .mutate_index(&config, |_storage, index| {
            index.mods.retain(|entry| entry.id != "garen-a");
            Ok(())
        })
        .unwrap();

    assert_eq!(
        library.champion_favorites(&config).unwrap()["Garen"],
        vec!["garen-b".to_string()]
    );
}

/// A mod that cannot be offered for the champion is one the reader would never
/// see again, so it is refused while they are still looking.
#[test]
fn a_favorite_for_another_champions_mod_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let (library, config) = library_with(
        dir.path(),
        &[("garen-a", &["Garen"]), ("teemo-a", &["Teemo"])],
    );

    let error = library
        .set_champion_favorite(&config, "Garen", "teemo-a", true)
        .unwrap_err();

    assert!(matches!(error, AppError::ModNotFound(_)));
    assert!(library.champion_favorites(&config).unwrap().is_empty());
}
