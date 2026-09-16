//! Build one champion's overlay archive somewhere of its own, ready to be
//! dropped into a running patcher's overlay directory.
//!
//!     cargo run --release -p ltk-manager-core --example stage_overlay_wad -- \
//!         "C:\Riot Games\League of Legends\Game" Kayle C:\staging
//!
//! It exists to answer the question the whole design rests on and that nothing
//! has tested: **does an overlay archive that appears after the patcher
//! injected reach the game?** If the DLL resolves the overlay path when the
//! game opens a file, it does. If the DLL indexed the directory once at
//! injection, it does not, and a champion select swap is impossible as
//! designed.
//!
//! Staging it in advance is what makes the test sharp. The copy into the live
//! overlay is then a file move rather than a build, so the moment the archive
//! appears is known to the millisecond and is unambiguously after injection.
//!
//! The content is synthetic, generated here, at paths of this file's own. No
//! game assets are copied and nothing resembles purchasable content.

use std::path::PathBuf;

use camino::Utf8PathBuf;
use fs_err as fs;
use ltk_manager_core::overlay::{OverlayBuildInputs, build_overlay};

const CHUNKS: usize = 4;
const CHUNK_BYTES: usize = 32 * 1024;

fn main() {
    let mut args = std::env::args().skip(1);
    let (Some(game), Some(champion), Some(out)) = (args.next(), args.next(), args.next()) else {
        eprintln!("usage: stage_overlay_wad <Game directory> <Champion> <staging directory>");
        std::process::exit(2);
    };

    let game = Utf8PathBuf::from_path_buf(PathBuf::from(&game)).expect("a UTF-8 path");
    let staging = Utf8PathBuf::from_path_buf(PathBuf::from(&out)).expect("a UTF-8 path");
    let wad_name = format!("{champion}.wad.client");

    let scratch = tempfile::tempdir().expect("a scratch directory");
    let mod_dir = Utf8PathBuf::from_path_buf(scratch.path().join("mod")).expect("a UTF-8 path");
    let dir = mod_dir
        .join("content")
        .join("base")
        .join(&wad_name)
        .join("assets")
        .join("ltk_probe")
        .join(champion.to_lowercase());
    fs::create_dir_all(dir.as_std_path()).expect("the content directory");
    for i in 0..CHUNKS {
        let mut bytes = vec![0u8; CHUNK_BYTES];
        let mut x = (i as u32 + 7) | 1;
        for b in bytes.iter_mut() {
            x ^= x << 13;
            x ^= x >> 17;
            x ^= x << 5;
            *b = x as u8;
        }
        fs::write(dir.join(format!("staged_{i}.bin")).as_std_path(), &bytes).expect("a chunk");
    }

    let project = ltk_mod_project::ModProject {
        name: "staged-probe".to_string(),
        display_name: "Staged probe".to_string(),
        version: "1.0.0".to_string(),
        description: "Synthetic, staged for a live overlay test.".to_string(),
        authors: Vec::new(),
        license: None,
        tags: Vec::new(),
        champions: vec![champion.clone()],
        maps: Vec::new(),
        transformers: Vec::new(),
        layers: ltk_mod_project::ModProjectLayer::default_table(),
        thumbnail: None,
        hashtables: Vec::new(),
    };
    fs::write(
        mod_dir.join("mod.config.json").as_std_path(),
        serde_json::to_string_pretty(&project).unwrap(),
    )
    .expect("the config");

    let started = std::time::Instant::now();
    build_overlay(
        OverlayBuildInputs {
            game_dir: game,
            overlay_root: staging.clone(),
            state_dir: staging.join(".state"),
            blocked_wads: Vec::new(),
            string_override_mode: ltk_overlay::StringOverrideMode::Disabled,
            mods: vec![ltk_overlay::EnabledMod {
                id: "staged-probe".to_string(),
                content: Box::new(ltk_overlay::FsModContent::new(mod_dir)),
                enabled_layers: None,
            }],
        },
        |_| {},
    )
    .expect("the build");

    let built = staging
        .join("DATA")
        .join("FINAL")
        .join("Champions")
        .join(&wad_name);
    let size = fs::metadata(built.as_std_path())
        .map(|m| m.len())
        .unwrap_or(0);
    println!(
        "staged {} ({:.1} MB) in {:.0} ms",
        built,
        size as f64 / (1024.0 * 1024.0),
        started.elapsed().as_secs_f64() * 1000.0
    );
    println!("copy it over the same relative path under the running profile's overlay");
}
