//! What a champion select swap costs: the time to rebuild one overlay WAD.
//!
//!     cargo run --release -p ltk-manager-core --example overlay_bench -- \
//!         "C:\Riot Games\League of Legends\Game" Yasuo.wad.client 5
//!
//! Champion select gives about ten seconds in three modes out of four
//! (`docs/metrics/2026-09-15-presupuesto-por-modo.md`). That is the time
//! available. This measures the time needed, which is the other half of the
//! comparison ADR-0043's precondition makes and the one phase 1 could not
//! reach.
//!
//! Three costs, per repeat:
//!
//! 1. **cold** - no previous state at all, which is what a first build after a
//!    patch pays.
//! 2. **incremental** - the mod's bytes change and the builder picks its own
//!    path, which for an unchanged chunk set is the in-place tail rewrite.
//! 3. **forced whole** - the same edit with the profile's `wadLayouts` cleared
//!    first, so `plan_tail_rewrites` finds no record and the WAD is written to
//!    a temp file and renamed. This is the path ADR-0043 requires of every
//!    swap, and the number a swap is actually budgeted against.
//!
//! Reads the game. Writes only under the output directory. No patcher, no
//! injection, no client, no account.
//!
//! The mod is synthetic: bytes this file generates, at chunk paths of its own.
//! Nothing from Riot is copied, and none of it resembles a purchasable skin.
//! A new chunk rather than a replacement changes the tail and the TOC, not the
//! copy of the source data region, which is where a whole-WAD rebuild spends
//! its time.

use std::path::Path;
use std::time::{Duration, Instant};

use camino::{Utf8Path, Utf8PathBuf};
use ltk_manager_core::overlay::{OverlayBuildInputs, build_overlay};

/// How many override chunks the synthetic mod carries, and how big each is.
///
/// A skin mod ships a handful of textures and a bin or two. The count matters
/// more than the size: each one is a TOC entry and a compression.
const OVERRIDE_COUNT: usize = 8;
const OVERRIDE_BYTES: usize = 64 * 1024;

fn utf8(path: &Path) -> Utf8PathBuf {
    Utf8PathBuf::from_path_buf(path.to_path_buf()).expect("a UTF-8 path")
}

/// Write the synthetic mod, with `seed` deciding its bytes so each repeat is a
/// real content change rather than a no-op the builder would skip.
fn write_mod(mod_dir: &Utf8Path, wad_name: &str, seed: u8) {
    // The crate's own type rather than hand-written JSON, so the shape is
    // whatever the reader accepts rather than whatever this file guessed.
    let config = ltk_mod_project::ModProject {
        name: "overlay-bench".to_string(),
        display_name: "Overlay bench".to_string(),
        version: "1.0.0".to_string(),
        description: "Synthetic content for timing a rebuild. Not a skin.".to_string(),
        authors: Vec::new(),
        license: None,
        tags: Vec::new(),
        champions: Vec::new(),
        maps: Vec::new(),
        transformers: Vec::new(),
        layers: ltk_mod_project::ModProjectLayer::default_table(),
        thumbnail: None,
        hashtables: Vec::new(),
    };
    std::fs::write(
        mod_dir.join("mod.config.json").as_std_path(),
        serde_json::to_vec_pretty(&config).unwrap(),
    )
    .expect("the project config");

    let wad_dir = mod_dir.join("content").join("base").join(wad_name);
    for i in 0..OVERRIDE_COUNT {
        let dir = wad_dir.join("assets").join("overlay_bench");
        std::fs::create_dir_all(dir.as_std_path()).expect("the override directory");
        // Incompressible, so the build pays a realistic compression cost rather
        // than zstd's best case over a run of zeroes.
        let mut bytes = vec![0u8; OVERRIDE_BYTES];
        let mut x = seed as u32 ^ (i as u32) << 8 | 1;
        for b in bytes.iter_mut() {
            x ^= x << 13;
            x ^= x >> 17;
            x ^= x << 5;
            *b = x as u8;
        }
        std::fs::write(dir.join(format!("chunk_{i}.bin")).as_std_path(), &bytes)
            .expect("an override file");
    }
}

fn inputs(
    game: &Utf8Path,
    root: &Utf8Path,
    state: &Utf8Path,
    mod_dir: &Utf8Path,
) -> OverlayBuildInputs {
    OverlayBuildInputs {
        game_dir: game.to_owned(),
        overlay_root: root.to_owned(),
        state_dir: state.to_owned(),
        blocked_wads: Vec::new(),
        string_override_mode: ltk_overlay::StringOverrideMode::Disabled,
        mods: vec![ltk_overlay::EnabledMod {
            id: "overlay-bench".to_string(),
            content: Box::new(ltk_overlay::FsModContent::new(mod_dir.to_owned())),
            enabled_layers: None,
        }],
    }
}

fn timed(inputs: OverlayBuildInputs) -> Duration {
    let started = Instant::now();
    build_overlay(inputs, |_| {}).expect("the build");
    started.elapsed()
}

/// Drop every `wadLayouts` record, which is what forces the atomic path.
///
/// ADR-0043's mechanism, exercised here rather than asserted: without a record
/// `plan_tail_rewrites` has nothing to verify and the WAD is rebuilt whole.
fn clear_layouts(state: &Utf8Path) {
    let path = state.join("overlay.json");
    let Some(mut overlay_state) = ltk_overlay::OverlayState::load(&path).expect("the state file")
    else {
        return;
    };
    overlay_state.wad_layouts.clear();
    overlay_state.save(&path).expect("the state file");
}

fn stats(label: &str, mut values: Vec<f64>) {
    values.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let n = values.len();
    let avg = values.iter().sum::<f64>() / n as f64;
    let p95 = values[((n as f64 - 1.0) * 0.95).round() as usize];
    println!(
        "{label:<18} n={n}  avg={avg:8.0} ms  p50={:8.0} ms  p95={p95:8.0} ms  max={:8.0} ms",
        values[n / 2],
        values[n - 1]
    );
}

/// Every champion WAD in the install, largest first, skipping the locale
/// variants that carry only voice lines.
fn champion_wads(game: &Utf8Path) -> Vec<(String, u64)> {
    let dir = game.join("DATA").join("FINAL").join("Champions");
    let mut wads: Vec<(String, u64)> = std::fs::read_dir(dir.as_std_path())
        .expect("the champions directory")
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            if !name.ends_with(".wad.client") || name.matches('.').count() > 2 {
                return None;
            }
            Some((name, entry.metadata().ok()?.len()))
        })
        .collect();
    wads.sort_by_key(|(_, size)| std::cmp::Reverse(*size));
    wads
}

/// One forced whole rebuild per champion, each on a WAD the page cache has not
/// seen.
///
/// Repeating one WAD measures a warm cache: 140 MB stays resident on a machine
/// with 31 GB of RAM, so every round after the first reads from memory. The
/// champion archives together are far larger than that, so sweeping them is
/// how a first read gets measured - which is the read a champion select
/// actually pays, since nothing has opened that archive since boot.
fn sweep(game: &Utf8Path, limit: usize) {
    let wads = champion_wads(game);
    let total: u64 = wads.iter().map(|(_, s)| *s).sum();
    println!(
        "{} champion archives, {:.1} GB in total, taking the {limit} largest\n",
        wads.len(),
        total as f64 / (1024.0 * 1024.0 * 1024.0)
    );

    let scratch = tempfile::tempdir().expect("a scratch directory");
    let scratch = utf8(scratch.path());
    let mut rows: Vec<(String, f64, f64)> = Vec::new();

    for (round, (wad_name, size)) in wads.iter().take(limit).enumerate() {
        // A directory of its own per champion, so no state carries over and
        // every build is the first one for that archive.
        let mod_dir = scratch.join(format!("mod{round}"));
        let overlay_root = scratch.join(format!("overlay{round}"));
        let state = scratch.join(format!("state{round}"));
        std::fs::create_dir_all(mod_dir.as_std_path()).unwrap();
        write_mod(&mod_dir, wad_name, round as u8 + 1);

        let ms = timed(inputs(game, &overlay_root, &state, &mod_dir)).as_secs_f64() * 1000.0;
        let mb = *size as f64 / (1024.0 * 1024.0);
        let built = overlay_root
            .join("DATA")
            .join("FINAL")
            .join("Champions")
            .join(wad_name);
        let built_mb = std::fs::metadata(built.as_std_path())
            .map(|m| m.len() as f64 / (1024.0 * 1024.0))
            .unwrap_or(0.0);
        println!(
            "{wad_name:<32} {mb:7.1} MB source  {built_mb:7.1} MB written  {ms:7.0} ms  {:5.0} MB/s",
            mb / (ms / 1000.0)
        );
        rows.push((wad_name.clone(), mb, ms));
    }

    println!();
    stats(
        "cold whole rebuild",
        rows.iter().map(|(_, _, ms)| *ms).collect(),
    );
    let biggest = rows
        .iter()
        .max_by(|a, b| a.2.partial_cmp(&b.2).unwrap())
        .unwrap();
    println!(
        "slowest: {} at {:.0} ms for {:.1} MB",
        biggest.0, biggest.2, biggest.1
    );
}

fn main() {
    let mut args = std::env::args().skip(1);
    let (Some(game), Some(wad_name)) = (args.next(), args.next()) else {
        eprintln!("usage: overlay_bench <Game directory> <Wad.wad.client|sweep> [repeats]");
        std::process::exit(2);
    };
    let repeats: usize = args.next().and_then(|s| s.parse().ok()).unwrap_or(5);

    if wad_name == "sweep" {
        sweep(&utf8(Path::new(&game)), repeats);
        return;
    }

    let game = utf8(Path::new(&game));
    let source = game
        .join("DATA")
        .join("FINAL")
        .join("Champions")
        .join(&wad_name);
    let size_mb = std::fs::metadata(source.as_std_path())
        .map(|m| m.len() as f64 / (1024.0 * 1024.0))
        .unwrap_or(0.0);
    println!("{wad_name} is {size_mb:.1} MB, {repeats} repeats\n");

    let scratch = tempfile::tempdir().expect("a scratch directory");
    let scratch = utf8(scratch.path());
    let mod_dir = scratch.join("mod");
    let overlay_root = scratch.join("overlay");
    let state = scratch.join("state");
    std::fs::create_dir_all(mod_dir.as_std_path()).unwrap();

    let mut cold = Vec::new();
    let mut incremental = Vec::new();
    let mut forced = Vec::new();

    for round in 0..repeats {
        // Cold: nothing carried over but the game index, which a real profile
        // also keeps and which is not what a swap pays for.
        let _ = std::fs::remove_file(state.join("overlay.json").as_std_path());
        let _ = std::fs::remove_dir_all(overlay_root.as_std_path());
        write_mod(&mod_dir, &wad_name, round as u8 * 3 + 1);
        cold.push(timed(inputs(&game, &overlay_root, &state, &mod_dir)).as_secs_f64() * 1000.0);

        write_mod(&mod_dir, &wad_name, round as u8 * 3 + 2);
        incremental
            .push(timed(inputs(&game, &overlay_root, &state, &mod_dir)).as_secs_f64() * 1000.0);

        write_mod(&mod_dir, &wad_name, round as u8 * 3 + 3);
        clear_layouts(&state);
        forced.push(timed(inputs(&game, &overlay_root, &state, &mod_dir)).as_secs_f64() * 1000.0);

        println!(
            "round {}: cold {:.0} ms, incremental {:.0} ms, forced whole {:.0} ms",
            round + 1,
            cold[round],
            incremental[round],
            forced[round]
        );
    }

    println!();
    stats("cold", cold);
    stats("incremental", incremental);
    stats("forced whole", forced.clone());
    println!();
    println!("The forced whole row is what a champion select swap costs under ADR-0043.");
    println!("Champion select gives about 10 s in three modes out of four.");
}
