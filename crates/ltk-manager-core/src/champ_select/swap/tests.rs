use super::*;

/// An `overlay.json` the way the builder writes one, with a layout record per
/// archive and the fingerprints that decide what gets skipped.
///
/// The record's shape is copied from a file the builder really wrote, not from
/// the struct's field names: an earlier version of this fixture guessed
/// `regionOffset` where the format says `dataRegionOffset`, and a state that
/// will not load makes every assertion here pass for the wrong reason.
fn state_with(layouts: &[&str], fingerprints: &[&str]) -> serde_json::Value {
    let layout = |path: &str| {
        (
            path.to_string(),
            serde_json::json!({
                "source": {
                    "len": 205462288u64,
                    "mtime": 1789045025643113100u64,
                    "tocHash": 995723348469021839u64
                },
                "layout": {
                    "dataRegionOffset": 185456,
                    "offsetDelta": 96,
                    "tailOffset": 205462384u64,
                    "tocCapacity": 5787
                },
                "overrides": {}
            }),
        )
    };
    serde_json::json!({
        "version": 6,
        "enabledMods": ["a-mod"],
        "modFingerprints": { "a-mod": 1122334455u64 },
        "gameFingerprint": 1234567890u64,
        "blockedWads": [],
        "stringOverrideLocales": [],
        "wadFingerprints": fingerprints
            .iter()
            .map(|p| (p.to_string(), serde_json::json!(9876543210u64)))
            .collect::<serde_json::Map<String, serde_json::Value>>(),
        "linkedBinOffenders": [],
        "wadLayouts": layouts
            .iter()
            .map(|p| layout(p))
            .collect::<serde_json::Map<String, serde_json::Value>>(),
        "dirtyWads": []
    })
}

fn write_state(dir: &Path, value: &serde_json::Value) {
    fs_err::write(
        dir.join("overlay.json"),
        serde_json::to_vec_pretty(value).unwrap(),
    )
    .unwrap();
}

fn read_state(dir: &Path) -> serde_json::Value {
    serde_json::from_slice(&fs_err::read(dir.join("overlay.json")).unwrap()).unwrap()
}

const GAREN: &str = "DATA/FINAL/Champions/Garen.wad.client";
const KAYLE: &str = "DATA/FINAL/Champions/Kayle.wad.client";

/// The whole point: with no record the builder cannot take the in-place path.
#[test]
fn every_layout_record_goes() {
    let dir = tempfile::tempdir().unwrap();
    write_state(dir.path(), &state_with(&[GAREN, KAYLE], &[GAREN, KAYLE]));

    assert_eq!(forget_wad_layouts(dir.path()).unwrap(), 2);

    let after = read_state(dir.path());
    assert!(
        after["wadLayouts"].as_object().unwrap().is_empty(),
        "no record may survive, or that archive can still be rewritten in place"
    );
}

/// Only the layouts. Dropping the fingerprints too would rebuild every archive
/// in the profile instead of the one the swap changed.
#[test]
fn the_fingerprints_stay_so_untouched_archives_are_still_skipped() {
    let dir = tempfile::tempdir().unwrap();
    write_state(dir.path(), &state_with(&[GAREN, KAYLE], &[GAREN, KAYLE]));

    forget_wad_layouts(dir.path()).unwrap();

    let after = read_state(dir.path());
    let fingerprints = after["wadFingerprints"].as_object().unwrap();
    assert_eq!(fingerprints.len(), 2);
    assert!(fingerprints.contains_key(GAREN));
    assert_eq!(after["gameFingerprint"], 1234567890u64);
    assert_eq!(after["enabledMods"][0], "a-mod");
}

#[test]
fn a_profile_with_nothing_to_forget_is_left_alone() {
    let dir = tempfile::tempdir().unwrap();
    write_state(dir.path(), &state_with(&[], &[GAREN]));
    let before = fs_err::read(dir.path().join("overlay.json")).unwrap();

    assert_eq!(forget_wad_layouts(dir.path()).unwrap(), 0);
    assert_eq!(
        fs_err::read(dir.path().join("overlay.json")).unwrap(),
        before
    );
}

/// A profile that has never been built has no state file, and a swap into it
/// is a first build rather than an error.
#[test]
fn a_missing_state_file_is_not_a_failure() {
    let dir = tempfile::tempdir().unwrap();
    assert_eq!(forget_wad_layouts(dir.path()).unwrap(), 0);
}

/// An unreadable state is reported as nothing forgotten, because the builder
/// already treats one as no state at all and rebuilds whole. The safe side is
/// where this lands without any help.
#[test]
fn an_unreadable_state_reports_nothing_and_does_not_fail() {
    let dir = tempfile::tempdir().unwrap();
    fs_err::write(dir.path().join("overlay.json"), b"{ this is not json").unwrap();
    assert_eq!(forget_wad_layouts(dir.path()).unwrap(), 0);
}
