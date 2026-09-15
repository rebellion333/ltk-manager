//! Follow a League client from the terminal and print what it says, with a
//! monotonic timestamp per line, so a champion select can be timed end to end.
//!
//!     cargo run -p ltk-manager-core --example lcu_watch -- "C:\Riot Games\League of Legends"
//!
//! Reads only. Stop with Ctrl+C.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use ltk_manager_core::lcu::{LcuEvent, LcuObserver, LcuWatch};

struct Printer(Instant);

impl LcuObserver for Printer {
    fn on_event(&self, event: LcuEvent) {
        let at = self.0.elapsed().as_secs_f64();
        match event {
            LcuEvent::ClientUp { port } => println!("{at:10.3}  client up on port {port}"),
            LcuEvent::ClientDown => println!("{at:10.3}  client down"),
            LcuEvent::Gameflow(phase) => println!("{at:10.3}  gameflow {}", phase.as_str()),
            LcuEvent::ChampSelectStarted(view) => {
                println!("{at:10.3}  champ select started {view:?}")
            }
            LcuEvent::ChampSelectChanged(view) => {
                println!("{at:10.3}  champ select changed {view:?}")
            }
            LcuEvent::ChampSelectEnded => println!("{at:10.3}  champ select ended"),
        }
    }
}

fn main() {
    let root = std::env::args().nth(1).map(PathBuf::from);
    if root.is_none() {
        eprintln!("usage: lcu_watch <League install root>");
        std::process::exit(2);
    }
    println!("watching {}", root.as_ref().unwrap().display());
    let _watch = LcuWatch::start(root, Arc::new(Printer(Instant::now())));
    loop {
        std::thread::park();
    }
}
