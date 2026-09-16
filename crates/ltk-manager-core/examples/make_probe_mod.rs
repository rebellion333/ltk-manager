//! Pack a synthetic mod that touches one WAD per champion named, so a patched
//! session has something to redirect and the DLL's `redirected wad:` line has a
//! timestamp worth reading.
//!
//!     cargo run --release -p ltk-manager-core --example make_probe_mod -- \
//!         probe.modpkg Garen Kayn Teemo Diana Brand
//!
//! It is a probe, not a skin. Every byte is generated here, at chunk paths of
//! this file's own invention that the game never asks for, so the archive gains
//! entries and changes nothing a player can see. Nothing from Riot is copied
//! and nothing resembles purchasable content.
//!
//! What it is for: an overlay that holds a champion's archive is what makes the
//! patcher redirect that archive, and the moment it does is the last unmeasured
//! point in the champion select budget.

use std::path::PathBuf;

use camino::Utf8PathBuf;
use fs_err as fs;

/// Chunks per champion, and bytes per chunk. Small: the point is that the
/// archive is rebuilt and redirected, not that the mod carries anything.
const CHUNKS: usize = 4;
const CHUNK_BYTES: usize = 32 * 1024;

/// The probe's own name, mixed into its bytes.
///
/// Two probes over the same champions have to differ, or the overlay builder
/// reuses the archive it already wrote and a swap between them changes nothing
/// the game can read. A live test on 2026-09-16 ran against two byte-identical
/// probes and proved less than it looked like it did.
fn seed(name: &str) -> u32 {
    name.bytes()
        .fold(2166136261u32, |h, b| (h ^ b as u32).wrapping_mul(16777619))
}

fn project(name: &str, champions: &[String]) -> ltk_mod_project::ModProject {
    ltk_mod_project::ModProject {
        name: name.to_string(),
        display_name: format!("Probe {name}"),
        version: "1.0.0".to_string(),
        description: "Synthetic probe for measuring when the game opens a champion's archive. \
                      Generated content, no game assets, nothing visible in play."
            .to_string(),
        authors: Vec::new(),
        license: None,
        tags: Vec::new(),
        champions: champions.to_vec(),
        maps: Vec::new(),
        transformers: Vec::new(),
        layers: ltk_mod_project::ModProjectLayer::default_table(),
        thumbnail: None,
        hashtables: Vec::new(),
    }
}

fn main() {
    let mut args = std::env::args().skip(1);
    let Some(out) = args.next() else {
        eprintln!("usage: make_probe_mod <out.modpkg> <Champion> [Champion...]");
        std::process::exit(2);
    };
    let champions: Vec<String> = args.collect();
    if champions.is_empty() {
        eprintln!("name at least one champion, by the stem of its WAD (Garen, MonkeyKing)");
        std::process::exit(2);
    }

    /* From the file name, so a sweep can pack several probes over the same
    champions and tell them apart in the library. */
    let name = std::path::Path::new(&out)
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("champ-select-probe");

    let source = tempfile::tempdir().expect("a scratch directory");
    let root = Utf8PathBuf::from_path_buf(source.path().to_path_buf()).expect("a UTF-8 path");

    for (c, champion) in champions.iter().enumerate() {
        let dir = root
            .join("content")
            .join("base")
            .join(format!("{champion}.wad.client"))
            .join("assets")
            .join("ltk_probe")
            .join(champion.to_lowercase());
        fs::create_dir_all(dir.as_std_path()).expect("the content directory");
        for i in 0..CHUNKS {
            let mut bytes = vec![0u8; CHUNK_BYTES];
            let mut x = seed(name) ^ ((c as u32) << 16 | i as u32) | 1;
            for b in bytes.iter_mut() {
                x ^= x << 13;
                x ^= x >> 17;
                x ^= x << 5;
                *b = x as u8;
            }
            fs::write(dir.join(format!("probe_{i}.bin")).as_std_path(), &bytes)
                .expect("a probe chunk");
        }
    }

    fs::write(
        root.join("mod.config.json").as_std_path(),
        serde_json::to_string_pretty(&project(name, &champions)).expect("the config"),
    )
    .expect("the config file");

    let out = PathBuf::from(&out);
    let writer = std::io::BufWriter::new(fs::File::create(&out).expect("the output file"));
    ltk_mod_project::ProjectPacker::new(project(name, &champions), root)
        .pack(ltk_mod_project::modpkg::ModpkgFormat::new(writer))
        .expect("the pack");

    let size = fs::metadata(&out).map(|m| m.len()).unwrap_or(0);
    println!(
        "wrote {} ({} bytes) touching {} champion archive(s): {}",
        out.display(),
        size,
        champions.len(),
        champions.join(", ")
    );
}
