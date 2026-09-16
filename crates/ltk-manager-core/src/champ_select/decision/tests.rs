//! The views below are the shapes live captures produced on 2026-09-15, so a
//! case here is a moment that happened rather than one imagined.

use super::*;

fn view(phase: &str, time_left_ms: i64) -> ChampSelectView {
    ChampSelectView {
        game_id: 1624397377,
        locked_champion_id: Some(141),
        hovered_champion_id: None,
        timer_phase: phase.to_string(),
        time_left_ms,
        can_still_change: false,
        bench_champion_ids: Vec::new(),
    }
}

/// A patching session waiting for a game, mid champion select.
fn waiting<'a>(champ_select: Option<&'a ChampSelectView>) -> SwapContext<'a> {
    SwapContext {
        patcher: PatcherPhase::Patching,
        game_attached: false,
        gameflow: GameflowPhase::ChampSelect,
        champ_select,
        build_in_flight: false,
    }
}

#[test]
fn a_swap_early_in_finalization_is_allowed() {
    // Draft, straight after the lock.
    let v = view("FINALIZATION", 30000);
    let verdict = decide(&waiting(Some(&v)), &Budget::default());
    assert_eq!(
        verdict,
        Verdict::Allow {
            available: Duration::from_millis(30000) + Budget::default().tail
        }
    );
}

/// The ARAM case that looked like the tight one: a bench swap with eight
/// seconds left. Measured, the tail alone is four seconds and a rebuild is
/// half of one.
#[test]
fn a_late_aram_bench_swap_still_fits() {
    let v = view("FINALIZATION", 8284);
    assert!(decide(&waiting(Some(&v)), &Budget::default()).is_allowed());
}

/// The worst moment champion select offers, and it still fits on this hardware.
#[test]
fn the_last_instant_of_finalization_fits_on_a_measured_machine() {
    let v = view("FINALIZATION", 0);
    let verdict = decide(&waiting(Some(&v)), &Budget::default());
    assert_eq!(
        verdict,
        Verdict::Allow {
            available: Budget::default().tail
        }
    );
}

/// Blind pick and Practice Tool open in `BAN_PICK` with a clock that is not a
/// countdown to the game, and an ARAM opens there with no bans at all. The
/// floor is the tail, which is enough.
#[test]
fn an_earlier_phase_is_judged_on_the_tail_alone() {
    for phase in ["PLANNING", "BAN_PICK"] {
        let v = view(phase, 500);
        let verdict = decide(&waiting(Some(&v)), &Budget::default());
        assert_eq!(
            verdict,
            Verdict::Allow {
                available: Budget::default().tail
            },
            "{phase} must not be judged on its own clock"
        );
    }
}

#[test]
fn game_starting_is_refused_though_it_would_fit() {
    let v = view("GAME_STARTING", 0);
    assert_eq!(
        decide(&waiting(Some(&v)), &Budget::default()),
        Verdict::Refuse(Refusal::ChampSelectOver)
    );
}

#[test]
fn with_no_champion_select_the_lobby_is_the_easy_case() {
    let mut context = waiting(None);
    context.gameflow = GameflowPhase::Lobby;
    assert!(decide(&context, &Budget::default()).is_allowed());
}

#[test]
fn a_patcher_that_is_not_waiting_refuses() {
    let v = view("FINALIZATION", 30000);
    for (phase, expected) in [
        (PatcherPhase::Idle, Refusal::PatcherIdle),
        (PatcherPhase::Building, Refusal::PatcherBuilding),
    ] {
        let mut context = waiting(Some(&v));
        context.patcher = phase;
        assert_eq!(
            decide(&context, &Budget::default()),
            Verdict::Refuse(expected)
        );
    }
}

/// The session already served a game, so its archives are being read.
#[test]
fn a_session_that_already_attached_refuses() {
    let v = view("FINALIZATION", 30000);
    let mut context = waiting(Some(&v));
    context.game_attached = true;
    assert_eq!(
        decide(&context, &Budget::default()),
        Verdict::Refuse(Refusal::GameAlreadyRunning)
    );
}

/// `GameStart` and the game process are the same instant to within 100 ms, so
/// the gameflow phase refuses whatever champion select still says.
#[test]
fn the_gameflow_phase_refuses_once_the_game_is_starting() {
    let v = view("FINALIZATION", 30000);
    for phase in [
        GameflowPhase::GameStart,
        GameflowPhase::InProgress,
        GameflowPhase::Reconnect,
    ] {
        let mut context = waiting(Some(&v));
        context.gameflow = phase.clone();
        assert_eq!(
            decide(&context, &Budget::default()),
            Verdict::Refuse(Refusal::GameStarting),
            "{} must refuse",
            phase.as_str()
        );
    }
}

#[test]
fn one_build_at_a_time() {
    let v = view("FINALIZATION", 30000);
    let mut context = waiting(Some(&v));
    context.build_in_flight = true;
    assert_eq!(
        decide(&context, &Budget::default()),
        Verdict::Refuse(Refusal::BuildInFlight)
    );
}

/// A machine whose rebuilds do not fit declines rather than gambling, and says
/// both numbers so the interface can explain itself.
#[test]
fn a_slow_machine_refuses_with_both_numbers() {
    let mut budget = Budget::default();
    budget.observe_rebuild(Duration::from_millis(2800));
    let v = view("FINALIZATION", 0);

    let verdict = decide(&waiting(Some(&v)), &budget);
    assert_eq!(
        verdict,
        Verdict::Refuse(Refusal::NotEnoughTime {
            needed: Duration::from_millis(5600),
            available: budget.tail,
        })
    );

    // The same machine in a draft, where thirty seconds is plenty.
    let draft = view("FINALIZATION", 30000);
    assert!(decide(&waiting(Some(&draft)), &budget).is_allowed());
}
