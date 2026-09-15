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

/// The game's own process, which is what champion select is a countdown to.
///
/// `GameStart` is the client saying it is starting the game, not the game
/// existing: a live draft on 2026-09-15 showed the two are not the same moment,
/// and the one a mod has to be in place for is this one.
const GAME_EXE: &str = "league of legends.exe";

/// Print when the game process appears and when it goes, on the same clock as
/// the client's events, so the two can be subtracted.
fn watch_the_game_process(started: Instant) {
    std::thread::spawn(move || {
        let mut was_running = false;
        loop {
            let running = ritoclient::processes::is_running(GAME_EXE);
            if running != was_running {
                let at = started.elapsed().as_secs_f64();
                let what = if running {
                    "GAME PROCESS up"
                } else {
                    "game process gone"
                };
                println!("{at:10.3}  {what}");
                was_running = running;
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
    });
}

fn main() {
    let root = std::env::args().nth(1).map(PathBuf::from);
    if root.is_none() {
        eprintln!("usage: lcu_watch <League install root>");
        std::process::exit(2);
    }
    println!("watching {}", root.as_ref().unwrap().display());

    let started = Instant::now();
    watch_the_game_process(started);
    let _watch = LcuWatch::start(root, Arc::new(Printer(started)));
    loop {
        std::thread::park();
    }
}
