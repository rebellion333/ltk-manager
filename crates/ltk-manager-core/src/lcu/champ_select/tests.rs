//! The fixtures below are hand-written from the session's documented shape.
//! Phase 2 replaces them with captures from a live client.

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
fn a_pending_trade_keeps_the_change_open_in_draft() {
    let s = session(serde_json::json!({
        "localPlayerCellId": 0,
        "myTeam": [{"cellId": 0, "championId": 157, "championPickIntent": 0}],
        "actions": [[{"id": 1, "actorCellId": 0, "championId": 157, "completed": true, "type": "pick"}]],
        "trades": [{"id": 9, "cellId": 4, "state": "RECEIVED"}],
        "timer": {"phase": "FINALIZATION"}
    }));
    assert!(s.view().can_still_change);
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

#[test]
fn an_empty_document_still_parses() {
    let v = session(serde_json::json!({})).view();
    assert_eq!(v.locked_champion_id, None);
    assert_eq!(v.hovered_champion_id, None);
    assert_eq!(v.timer_phase, "");
}
