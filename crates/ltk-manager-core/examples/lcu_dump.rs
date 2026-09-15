//! Dump the League client's raw champion select and gameflow frames to disk,
//! so a field's real shape is read rather than assumed.
//!
//!     cargo run -p ltk-manager-core --example lcu_dump -- "C:\Riot Games\League of Legends" out_dir
//!
//! Writes one `<seq>-<resource>.json` per frame, plus the HTTP seed of each
//! resource at connect. Reads only, and prints nothing but a line per file.
//! Stop with Ctrl+C.

use std::path::{Path, PathBuf};

use ltk_manager_core::lcu::client::LcuClient;
use ltk_manager_core::lcu::lockfile::LeagueLockfile;
use ltk_manager_core::lcu::socket::{LcuSocket, event_name_for};

const GAMEFLOW_URI: &str = "/lol-gameflow/v1/gameflow-phase";
const CHAMP_SELECT_URI: &str = "/lol-champ-select/v1/session";

fn write(dir: &Path, seq: &mut u32, resource: &str, body: &str) {
    let name = resource.trim_start_matches('/').replace('/', "_");
    let path = dir.join(format!("{seq:04}-{name}.json"));
    if let Err(e) = std::fs::write(&path, body) {
        eprintln!("could not write {}: {e}", path.display());
        return;
    }
    println!("{} ({} bytes)", path.display(), body.len());
    *seq += 1;
}

fn main() {
    let mut args = std::env::args().skip(1);
    let (Some(root), Some(out)) = (args.next(), args.next()) else {
        eprintln!("usage: lcu_dump <League install root> <output directory>");
        std::process::exit(2);
    };
    let out = PathBuf::from(out);
    std::fs::create_dir_all(&out).expect("output directory");

    let lockfile = LeagueLockfile::read(Path::new(&root))
        .filter(LeagueLockfile::is_live)
        .expect("a live lockfile under that root");
    println!("client on port {}", lockfile.port);

    let mut seq = 0;

    // The seed, so a select already in progress is captured rather than waited for.
    if let Some(client) = LcuClient::new(&lockfile) {
        for uri in [GAMEFLOW_URI, CHAMP_SELECT_URI] {
            if let Some(value) = client.get_json::<serde_json::Value>(uri) {
                let body = serde_json::to_string_pretty(&value).unwrap_or_default();
                write(&out, &mut seq, &format!("{uri}-seed"), &body);
            }
        }
    }

    let mut socket = LcuSocket::connect(&lockfile).expect("the event socket");
    for uri in [GAMEFLOW_URI, CHAMP_SELECT_URI] {
        socket
            .subscribe(&event_name_for(uri))
            .expect("a subscription");
    }

    while let Some(frame) = socket.next_event() {
        let body = serde_json::to_string_pretty(&frame.data).unwrap_or_default();
        write(
            &out,
            &mut seq,
            &format!("{}-{}", frame.uri, frame.event_type),
            &body,
        );
    }
    println!("the socket closed");
}
