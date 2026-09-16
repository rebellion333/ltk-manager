use super::*;

use std::sync::mpsc;

use crate::lcu::ChampionSummary;

const GAREN: i32 = 86;
const WUKONG: i32 = 62;

fn roster() -> ChampionRoster {
    ChampionRoster::from_champions(vec![
        ChampionSummary {
            id: GAREN,
            name: "Garen".into(),
            alias: "Garen".into(),
        },
        ChampionSummary {
            id: WUKONG,
            // Localized, as a live client really answers.
            name: "Wukong".into(),
            alias: "MonkeyKing".into(),
        },
    ])
}

fn view(locked: Option<i32>, hovered: Option<i32>) -> ChampSelectView {
    ChampSelectView {
        game_id: 1,
        locked_champion_id: locked,
        hovered_champion_id: hovered,
        timer_phase: "FINALIZATION".into(),
        time_left_ms: 30000,
        can_still_change: false,
        bench_champion_ids: Vec::new(),
    }
}

fn preferences(pairs: &[(&str, &str)]) -> HashMap<String, ChampionPreference> {
    pairs
        .iter()
        .map(|(alias, mod_id)| {
            (
                (*alias).to_string(),
                ChampionPreference {
                    preferred: Some((*mod_id).to_string()),
                    favorites: Vec::new(),
                },
            )
        })
        .collect()
}

#[test]
fn a_lock_outranks_a_hover() {
    let wanted = desired_for(
        &view(Some(GAREN), Some(WUKONG)),
        &roster(),
        &preferences(&[("Garen", "garen-mod"), ("MonkeyKing", "wukong-mod")]),
    )
    .unwrap();
    assert_eq!(wanted.alias, "Garen");
    assert_eq!(wanted.mod_id, Some("garen-mod".to_string()));
}

/// Building on the hover rather than waiting for the lock, because the lock
/// was measured not to be the commitment it looks like.
#[test]
fn a_hover_is_acted_on_before_any_lock() {
    let wanted = desired_for(
        &view(None, Some(WUKONG)),
        &roster(),
        &preferences(&[("MonkeyKing", "wukong-mod")]),
    )
    .unwrap();
    assert_eq!(
        wanted.alias, "MonkeyKing",
        "the alias, never the display name"
    );
    assert_eq!(wanted.mod_id, Some("wukong-mod".to_string()));
}

#[test]
fn a_champion_with_no_preference_wants_nothing_applied() {
    let wanted = desired_for(&view(Some(GAREN), None), &roster(), &HashMap::new()).unwrap();
    assert_eq!(wanted.alias, "Garen");
    assert_eq!(
        wanted.mod_id, None,
        "which is a swap to unmodded, not an absence of one"
    );
}

#[test]
fn an_empty_champion_select_wants_nothing() {
    assert!(desired_for(&view(None, None), &roster(), &HashMap::new()).is_none());
}

/// A champion released after the cached roster was fetched is one no mod can be
/// categorized for either, so there is nothing to apply and nothing to say.
#[test]
fn a_champion_the_roster_does_not_know_wants_nothing() {
    assert!(desired_for(&view(Some(9999), None), &roster(), &HashMap::new()).is_none());
}

/// Records what the scheduler asked of it, and answers with a patcher that is
/// waiting for a game unless a test says otherwise.
struct FakeSwapper {
    status: Mutex<(PatcherPhase, bool)>,
    applied: Mutex<Vec<Desired>>,
    reports: Mutex<Vec<Report>>,
    done: mpsc::Sender<()>,
    /// How long a rebuild claims to have taken.
    took: Duration,
}

impl FakeSwapper {
    fn new(done: mpsc::Sender<()>) -> Self {
        Self {
            status: Mutex::new((PatcherPhase::Patching, false)),
            applied: Mutex::new(Vec::new()),
            reports: Mutex::new(Vec::new()),
            done,
            took: Duration::from_millis(400),
        }
    }
}

impl Swapper for FakeSwapper {
    fn patcher_status(&self) -> (PatcherPhase, bool) {
        *self.status.lock().unwrap()
    }

    fn apply(&self, desired: &Desired) -> crate::error::AppResult<Duration> {
        self.applied.lock().unwrap().push(desired.clone());
        Ok(self.took)
    }

    fn report(&self, report: Report) {
        self.reports.lock().unwrap().push(report);
        let _ = self.done.send(());
    }
}

/// Wait for the scheduler to report something, or give up.
fn wait_for_report(rx: &mpsc::Receiver<()>) -> bool {
    rx.recv_timeout(Duration::from_secs(5)).is_ok()
}

#[test]
fn a_hover_with_a_preference_is_applied() {
    let (tx, rx) = mpsc::channel();
    let swapper = Arc::new(FakeSwapper::new(tx));
    let mut scheduler = Scheduler::start(roster(), swapper.clone());
    scheduler.set_preferences(preferences(&[("Garen", "garen-mod")]));

    scheduler.observe(&LcuEvent::ChampSelectStarted(view(None, Some(GAREN))));
    assert!(wait_for_report(&rx));

    assert_eq!(
        swapper.applied.lock().unwrap().as_slice(),
        [Desired {
            alias: "Garen".into(),
            mod_id: Some("garen-mod".into())
        }]
    );
    assert!(matches!(
        swapper.reports.lock().unwrap().first(),
        Some(Report::Applied { .. })
    ));
    scheduler.stop();
}

/// The same champion arriving again, which the client republishes constantly,
/// must not rebuild the overlay a second time.
#[test]
fn the_same_champion_is_applied_once() {
    let (tx, rx) = mpsc::channel();
    let swapper = Arc::new(FakeSwapper::new(tx));
    let mut scheduler = Scheduler::start(roster(), swapper.clone());
    scheduler.set_preferences(preferences(&[("Garen", "garen-mod")]));

    scheduler.observe(&LcuEvent::ChampSelectStarted(view(None, Some(GAREN))));
    assert!(wait_for_report(&rx));
    scheduler.observe(&LcuEvent::ChampSelectChanged(view(Some(GAREN), None)));
    scheduler.observe(&LcuEvent::ChampSelectChanged(view(Some(GAREN), None)));
    thread::sleep(Duration::from_millis(200));

    assert_eq!(swapper.applied.lock().unwrap().len(), 1);
    scheduler.stop();
}

/// The trade that took Garen to Wukong: a second champion is a second build.
#[test]
fn a_champion_that_changes_after_the_lock_is_applied_again() {
    let (tx, rx) = mpsc::channel();
    let swapper = Arc::new(FakeSwapper::new(tx));
    let mut scheduler = Scheduler::start(roster(), swapper.clone());
    scheduler.set_preferences(preferences(&[
        ("Garen", "garen-mod"),
        ("MonkeyKing", "wukong-mod"),
    ]));

    scheduler.observe(&LcuEvent::ChampSelectStarted(view(Some(GAREN), None)));
    assert!(wait_for_report(&rx));
    scheduler.observe(&LcuEvent::ChampSelectChanged(view(Some(WUKONG), None)));
    assert!(wait_for_report(&rx));

    let applied = swapper.applied.lock().unwrap();
    assert_eq!(applied.len(), 2);
    assert_eq!(applied[1].alias, "MonkeyKing");
    scheduler.stop();
}

/// The game is already up, so the archives are being read and nothing may move.
/// The reader is told once rather than every tick.
#[test]
fn a_refusal_that_cannot_change_is_reported_once() {
    let (tx, rx) = mpsc::channel();
    let swapper = Arc::new(FakeSwapper::new(tx));
    *swapper.status.lock().unwrap() = (PatcherPhase::Patching, true);
    let mut scheduler = Scheduler::start(roster(), swapper.clone());
    scheduler.set_preferences(preferences(&[("Garen", "garen-mod")]));

    scheduler.observe(&LcuEvent::ChampSelectStarted(view(Some(GAREN), None)));
    assert!(wait_for_report(&rx));
    thread::sleep(Duration::from_millis(300));

    assert!(
        swapper.applied.lock().unwrap().is_empty(),
        "nothing was built"
    );
    let reports = swapper.reports.lock().unwrap();
    assert_eq!(reports.len(), 1, "said once, not on every tick");
    assert!(matches!(
        reports.first(),
        Some(Report::Refused {
            why: Refusal::GameAlreadyRunning,
            ..
        })
    ));
    scheduler.stop();
}

/// A patcher that is still building is a moment, not a state, so the scheduler
/// keeps the swap outstanding and applies it when the patcher is up.
#[test]
fn a_refusal_that_can_change_is_retried() {
    let (tx, rx) = mpsc::channel();
    let swapper = Arc::new(FakeSwapper::new(tx));
    *swapper.status.lock().unwrap() = (PatcherPhase::Building, false);
    let mut scheduler = Scheduler::start(roster(), swapper.clone());
    scheduler.set_preferences(preferences(&[("Garen", "garen-mod")]));

    scheduler.observe(&LcuEvent::ChampSelectStarted(view(Some(GAREN), None)));
    thread::sleep(Duration::from_millis(200));
    assert!(swapper.applied.lock().unwrap().is_empty());
    assert!(
        swapper.reports.lock().unwrap().is_empty(),
        "nothing final to say yet"
    );

    *swapper.status.lock().unwrap() = (PatcherPhase::Patching, false);
    scheduler.poke();
    assert!(wait_for_report(&rx));

    assert_eq!(swapper.applied.lock().unwrap().len(), 1);
    scheduler.stop();
}

/// What a rebuild really took is what the next decision plans around.
#[test]
fn the_budget_learns_from_the_rebuild() {
    let (tx, rx) = mpsc::channel();
    let mut swapper = FakeSwapper::new(tx);
    swapper.took = Duration::from_millis(1800);
    let swapper = Arc::new(swapper);
    let mut scheduler = Scheduler::start(roster(), swapper.clone());
    scheduler.set_preferences(preferences(&[("Garen", "garen-mod")]));

    scheduler.observe(&LcuEvent::ChampSelectStarted(view(Some(GAREN), None)));
    assert!(wait_for_report(&rx));

    let (_, _, budget) = scheduler.snapshot();
    assert_eq!(budget.rebuild, Duration::from_millis(1800));
    scheduler.stop();
}

/// The next champion select is a fresh question about an overlay this game may
/// have left in any state.
#[test]
fn the_end_of_champion_select_forgets_what_was_applied() {
    let (tx, rx) = mpsc::channel();
    let swapper = Arc::new(FakeSwapper::new(tx));
    let mut scheduler = Scheduler::start(roster(), swapper.clone());
    scheduler.set_preferences(preferences(&[("Garen", "garen-mod")]));

    scheduler.observe(&LcuEvent::ChampSelectStarted(view(Some(GAREN), None)));
    assert!(wait_for_report(&rx));
    scheduler.observe(&LcuEvent::ChampSelectEnded);

    let (wanted, applied, _) = scheduler.snapshot();
    assert_eq!(wanted, None);
    assert_eq!(applied, None);
    scheduler.stop();
}
