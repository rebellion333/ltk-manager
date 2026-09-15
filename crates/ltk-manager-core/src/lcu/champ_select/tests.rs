//! A fixture is hand-written from the session's documented shape unless its
//! test says otherwise. The ones that name a date were recorded off a live
//! client, and each of those exists because the shape it carries contradicted
//! what the documented one implied.

use super::*;

fn session(json: serde_json::Value) -> ChampSelectSession {
    serde_json::from_value(json).unwrap()
}

#[test]
fn a_hover_is_the_pick_intent_until_the_pick_is_in_progress() {
    let s = session(serde_json::json!({
        "gameId": 42,
        "localPlayerCellId": 2,
        "myTeam": [
            {"cellId": 1, "championId": 0, "championPickIntent": 0},
            {"cellId": 2, "championId": 0, "championPickIntent": 157}
        ],
        "actions": [[
            {"id": 1, "actorCellId": 2, "championId": 0, "completed": false, "isInProgress": false, "type": "pick"}
        ]],
        "timer": {"phase": "PLANNING", "adjustedTimeLeftInPhase": 25000, "totalTimeInPhase": 30000, "isInfinite": false}
    }));
    let v = s.view();
    assert_eq!(v.hovered_champion_id, Some(157));
    assert_eq!(v.locked_champion_id, None);
    assert_eq!(v.timer_phase, "PLANNING");
    assert_eq!(v.time_left_ms, 25000);
    assert!(!v.can_still_change);
}

#[test]
fn an_in_progress_pick_names_the_hover_over_the_intent() {
    let s = session(serde_json::json!({
        "localPlayerCellId": 0,
        "myTeam": [{"cellId": 0, "championId": 0, "championPickIntent": 157}],
        "actions": [[
            {"id": 7, "actorCellId": 0, "championId": 62, "completed": false, "isInProgress": true, "type": "pick"}
        ]],
        "timer": {"phase": "BAN_PICK", "adjustedTimeLeftInPhase": 9000}
    }));
    assert_eq!(s.view().hovered_champion_id, Some(62));
}

#[test]
fn a_completed_pick_is_the_lock_and_clears_the_hover() {
    let s = session(serde_json::json!({
        "localPlayerCellId": 0,
        "myTeam": [{"cellId": 0, "championId": 157, "championPickIntent": 157}],
        "actions": [[
            {"id": 7, "actorCellId": 0, "championId": 157, "completed": true, "isInProgress": false, "type": "pick"}
        ]],
        "timer": {"phase": "FINALIZATION", "adjustedTimeLeftInPhase": 30000}
    }));
    let v = s.view();
    assert_eq!(v.locked_champion_id, Some(157));
    assert_eq!(v.hovered_champion_id, None);
}

#[test]
fn a_champion_on_the_cell_without_a_completed_pick_is_not_a_lock() {
    // The client fills `championId` early in some modes; without the action
    // it is not a commitment.
    let s = session(serde_json::json!({
        "localPlayerCellId": 3,
        "myTeam": [{"cellId": 3, "championId": 157, "championPickIntent": 0}],
        "actions": [[
            {"id": 1, "actorCellId": 3, "championId": 157, "completed": false, "isInProgress": true, "type": "pick"}
        ]],
        "timer": {"phase": "BAN_PICK"}
    }));
    let v = s.view();
    assert_eq!(v.locked_champion_id, None);
    assert_eq!(v.hovered_champion_id, Some(157));
}

/// Recorded from a live ARAM on 2026-09-15. The bench rotates: the champion
/// swapped away lands on it, and the one taken leaves it.
#[test]
fn an_aram_bench_swap_moves_the_locked_champion() {
    let shen = session(serde_json::json!({
        "gameId": 1624402008,
        "localPlayerCellId": 0,
        "myTeam": [{"cellId": 0, "championId": 98, "championPickIntent": 0}],
        "benchEnabled": true,
        "benchChampions": [{"championId": 523}, {"championId": 203}],
        "timer": {"phase": "FINALIZATION", "adjustedTimeLeftInPhase": 45000}
    }));
    let v = shen.view();
    assert_eq!(v.locked_champion_id, Some(98));
    assert_eq!(v.bench_champion_ids, vec![523, 203]);
    assert!(v.can_still_change);

    let aphelios = session(serde_json::json!({
        "gameId": 1624402008,
        "localPlayerCellId": 0,
        "myTeam": [{"cellId": 0, "championId": 523, "championPickIntent": 0}],
        "benchEnabled": true,
        "benchChampions": [{"championId": 203}, {"championId": 98}],
        "timer": {"phase": "FINALIZATION", "adjustedTimeLeftInPhase": 8284}
    }));
    let v = aphelios.view();
    assert_eq!(
        v.locked_champion_id,
        Some(523),
        "the bench swap is the lock"
    );
    assert_eq!(
        v.bench_champion_ids,
        vec![203, 98],
        "Shen went to the bench"
    );
}

/// The client leaves `benchEnabled` true through `GAME_STARTING`, where no
/// swap is possible any more. Reading it straight says the champion may still
/// change while the game is already being handed the match.
#[test]
fn the_bench_closes_when_the_game_starts() {
    let s = session(serde_json::json!({
        "localPlayerCellId": 0,
        "myTeam": [{"cellId": 0, "championId": 523, "championPickIntent": 0}],
        "benchEnabled": true,
        "benchChampions": [{"championId": 203}, {"championId": 98}],
        "timer": {"phase": "GAME_STARTING", "adjustedTimeLeftInPhase": 0}
    }));
    assert!(!s.view().can_still_change);
}

#[test]
fn aram_reads_the_cell_and_keeps_the_change_open() {
    let s = session(serde_json::json!({
        "localPlayerCellId": 1,
        "myTeam": [{"cellId": 1, "championId": 62, "championPickIntent": 0}],
        "actions": [],
        "benchEnabled": true,
        "benchChampions": [{"championId": 157}, {"championId": 266}],
        "timer": {"phase": "FINALIZATION", "adjustedTimeLeftInPhase": 40000}
    }));
    let v = s.view();
    assert_eq!(v.locked_champion_id, Some(62));
    assert!(v.can_still_change);
    assert_eq!(v.bench_champion_ids, vec![157, 266]);
}

#[test]
fn an_offered_trade_keeps_the_change_open_in_draft() {
    let s = session(serde_json::json!({
        "localPlayerCellId": 0,
        "myTeam": [{"cellId": 0, "championId": 157, "championPickIntent": 0}],
        "actions": [[{"id": 1, "actorCellId": 0, "championId": 157, "completed": true, "type": "pick"}]],
        "trades": [{"id": 9, "cellId": 4, "state": "RECEIVED"}],
        "timer": {"phase": "FINALIZATION"}
    }));
    assert!(s.view().can_still_change);
}

/// Recorded from a live Practice Tool select on 2026-09-15: the client carries
/// a trade slot per teammate with nothing happening in it, and reading those as
/// pending offers reported a champion that could not change as changeable.
#[test]
fn idle_trade_slots_are_not_an_offer() {
    let s = session(serde_json::json!({
        "localPlayerCellId": 0,
        "myTeam": [{"cellId": 0, "championId": 131, "championPickIntent": 0}],
        "actions": [[{"id": 1, "actorCellId": 0, "championId": 131, "completed": true, "type": "pick"}]],
        "trades": [
            {"id": 1, "cellId": 1, "state": "AVAILABLE"},
            {"id": 2, "cellId": 2, "state": "INVALID"},
            {"id": 3, "cellId": 3, "state": "BUSY"}
        ],
        "timer": {"phase": "FINALIZATION", "adjustedTimeLeftInPhase": 10000}
    }));
    assert!(!s.view().can_still_change);
}

/// A trade this build does not know the spelling of is not treated as an offer,
/// so an unknown state never claims the champion is still changing.
#[test]
fn an_unknown_trade_state_is_not_an_offer() {
    let s = session(serde_json::json!({
        "localPlayerCellId": 0,
        "myTeam": [{"cellId": 0, "championId": 131, "championPickIntent": 0}],
        "trades": [{"id": 1, "cellId": 1, "state": "REHEARSING"}, {"id": 2, "cellId": 2}],
        "timer": {"phase": "FINALIZATION"}
    }));
    assert!(!s.view().can_still_change);
}

#[test]
fn other_players_do_not_leak_into_the_view() {
    let s = session(serde_json::json!({
        "localPlayerCellId": 4,
        "myTeam": [
            {"cellId": 0, "championId": 1, "championPickIntent": 0},
            {"cellId": 4, "championId": 0, "championPickIntent": 0}
        ],
        "actions": [[
            {"id": 1, "actorCellId": 0, "championId": 1, "completed": true, "type": "pick"},
            {"id": 2, "actorCellId": 4, "championId": 0, "completed": false, "type": "pick"}
        ]],
        "timer": {"phase": "BAN_PICK"}
    }));
    let v = s.view();
    assert_eq!(v.locked_champion_id, None);
    assert_eq!(v.hovered_champion_id, None);
}

/// Recorded from a live draft on 2026-09-15. Twitch was banned while Garen was
/// hovered, and a ban is not a pick: reading the ban as the local player's
/// champion would have swapped the mod to the banned one.
#[test]
fn a_ban_does_not_move_the_hover() {
    let s = session(serde_json::json!({
        "localPlayerCellId": 0,
        "myTeam": [{"cellId": 0, "championId": 0, "championPickIntent": 86}],
        "actions": [
            [{"id": 1, "actorCellId": 0, "championId": 29, "completed": true, "isInProgress": false, "type": "ban"}],
            [{"id": 2, "actorCellId": 0, "championId": 0, "completed": false, "isInProgress": false, "type": "pick"}]
        ],
        "timer": {"phase": "BAN_PICK", "adjustedTimeLeftInPhase": 22901}
    }));
    let v = s.view();
    assert_eq!(
        v.hovered_champion_id,
        Some(86),
        "Garen, not the banned Twitch"
    );
    assert_eq!(v.locked_champion_id, None);
}

/// The clock ticking is not news, and everything else is.
#[test]
fn only_the_clock_moving_is_not_a_change() {
    let base = ChampSelectView {
        game_id: 1624388916,
        locked_champion_id: Some(86),
        hovered_champion_id: None,
        timer_phase: "FINALIZATION".into(),
        time_left_ms: 30000,
        can_still_change: false,
        bench_champion_ids: Vec::new(),
    };

    let ticked = ChampSelectView {
        time_left_ms: 12442,
        ..base.clone()
    };
    assert!(!ticked.differs_meaningfully_from(&base));

    // The trade that took Garen to Anivia, which is the change that matters.
    let traded = ChampSelectView {
        locked_champion_id: Some(34),
        time_left_ms: 27110,
        ..base.clone()
    };
    assert!(traded.differs_meaningfully_from(&base));

    let offered = ChampSelectView {
        can_still_change: true,
        ..base.clone()
    };
    assert!(offered.differs_meaningfully_from(&base));

    let next_phase = ChampSelectView {
        timer_phase: "GAME_STARTING".into(),
        ..base.clone()
    };
    assert!(next_phase.differs_meaningfully_from(&base));
}

#[test]
fn an_empty_document_still_parses() {
    let v = session(serde_json::json!({})).view();
    assert_eq!(v.locked_champion_id, None);
    assert_eq!(v.hovered_champion_id, None);
    assert_eq!(v.timer_phase, "");
}
