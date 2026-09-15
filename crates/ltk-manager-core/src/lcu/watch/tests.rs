use std::sync::Mutex;

use super::*;

#[derive(Default)]
struct Recorder(Mutex<Vec<LcuEvent>>);

impl Recorder {
    fn events(&self) -> Vec<LcuEvent> {
        self.0.lock().unwrap().clone()
    }
}

impl LcuObserver for Recorder {
    fn on_event(&self, event: LcuEvent) {
        self.0.lock().unwrap().push(event);
    }
}

/// The recorder twice: once to read, once as the observer the code takes.
fn recorder() -> (Arc<Recorder>, Arc<dyn LcuObserver>) {
    let recorder = Arc::new(Recorder::default());
    let observer: Arc<dyn LcuObserver> = recorder.clone();
    (recorder, observer)
}

fn view(locked: Option<i32>, hovered: Option<i32>) -> ChampSelectView {
    ChampSelectView {
        game_id: 1,
        locked_champion_id: locked,
        hovered_champion_id: hovered,
        timer_phase: "BAN_PICK".into(),
        time_left_ms: 1000,
        can_still_change: false,
        bench_champion_ids: Vec::new(),
    }
}

#[test]
fn the_tracker_reports_a_phase_once_per_change() {
    let (recorder, observer) = recorder();
    let mut tracker = Tracker::default();
    tracker.gameflow(GameflowPhase::Lobby, &observer);
    tracker.gameflow(GameflowPhase::Lobby, &observer);
    tracker.gameflow(GameflowPhase::ChampSelect, &observer);

    assert_eq!(
        recorder.events(),
        vec![
            LcuEvent::Gameflow(GameflowPhase::Lobby),
            LcuEvent::Gameflow(GameflowPhase::ChampSelect)
        ]
    );
}

#[test]
fn champ_select_starts_changes_and_ends() {
    let (recorder, observer) = recorder();
    let mut tracker = Tracker::default();
    tracker.champ_select(None, &observer);
    tracker.champ_select(Some(view(None, Some(157))), &observer);
    tracker.champ_select(Some(view(None, Some(157))), &observer);
    tracker.champ_select(Some(view(Some(157), None)), &observer);
    tracker.champ_select(None, &observer);
    tracker.champ_select(None, &observer);

    assert_eq!(
        recorder.events(),
        vec![
            LcuEvent::ChampSelectStarted(view(None, Some(157))),
            LcuEvent::ChampSelectChanged(view(Some(157), None)),
            LcuEvent::ChampSelectEnded,
        ]
    );
}

#[test]
fn a_lost_socket_ends_a_champ_select_it_was_following() {
    let (recorder, observer) = recorder();
    let mut tracker = Tracker::default();
    tracker.socket_lost(&observer);
    tracker.champ_select(Some(view(None, None)), &observer);
    tracker.socket_lost(&observer);

    assert_eq!(
        recorder.events(),
        vec![
            LcuEvent::ChampSelectStarted(view(None, None)),
            LcuEvent::ChampSelectEnded,
        ]
    );
}

/// A watch with nowhere to look sleeps, and stopping it returns promptly
/// rather than waiting out the ten-second nap.
#[test]
fn a_watch_without_a_root_stops_at_once() {
    let (recorder, observer) = recorder();
    let mut watch = LcuWatch::start(None, observer);
    thread::sleep(Duration::from_millis(50));
    let started = std::time::Instant::now();
    watch.stop();
    assert!(started.elapsed() < Duration::from_secs(2));
    assert!(recorder.events().is_empty());
}

/// A root with no lockfile is the same as no root, and can be changed while
/// the watch sleeps.
#[test]
fn reconfiguring_wakes_the_sleeping_watch() {
    let dir = tempfile::tempdir().unwrap();
    let (recorder, observer) = recorder();
    let mut watch = LcuWatch::start(Some(dir.path().to_path_buf()), observer);
    thread::sleep(Duration::from_millis(50));
    watch.reconfigure(None);
    let started = std::time::Instant::now();
    watch.stop();
    assert!(started.elapsed() < Duration::from_secs(2));
    assert!(recorder.events().is_empty());
}
