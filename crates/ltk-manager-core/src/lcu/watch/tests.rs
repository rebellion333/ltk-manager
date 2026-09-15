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

/// Reconfiguring ends the current follow and stopping returns promptly, rather
/// than waiting out the ten-second nap.
///
/// Nothing is asserted about events: a configured root sends the watch looking
/// at every installed patchline, so a machine with a League client open really
/// does have one to follow and this test must not depend on which machine runs
/// it.
#[test]
fn reconfiguring_and_stopping_do_not_wait_out_the_nap() {
    let dir = tempfile::tempdir().unwrap();
    let (_recorder, observer) = recorder();
    let mut watch = LcuWatch::start(Some(dir.path().to_path_buf()), observer);
    thread::sleep(Duration::from_millis(50));
    watch.reconfigure(None);
    let started = std::time::Instant::now();
    watch.stop();
    assert!(started.elapsed() < Duration::from_secs(2));
}

/// The configured root leads, discovered installs follow it, and one that is
/// already configured is not visited twice.
#[test]
fn the_configured_root_leads_the_candidates() {
    let configured = PathBuf::from("C:/Riot Games/League of Legends");
    let pbe = PathBuf::from("C:/Riot Games/League of Legends (PBE)");

    let roots = candidate_roots(&Some(configured.clone()), vec![pbe.clone()]);
    assert_eq!(roots, vec![configured.clone(), pbe.clone()]);

    let deduped = candidate_roots(
        &Some(configured.clone()),
        vec![configured.clone(), pbe.clone()],
    );
    assert_eq!(deduped, vec![configured, pbe]);
}

/// With nothing configured the watch hunts for nothing, whatever is installed.
#[test]
fn no_configured_root_means_no_candidates() {
    assert!(candidate_roots(&None, Vec::new()).is_empty());
    assert!(find_live_client(&None).is_none());
}
