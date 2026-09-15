//! The gameflow phase, `/lol-gameflow/v1/gameflow-phase`.
//!
//! One string the client publishes on every transition. The variants below are
//! the spellings observed from the client; a new one lands in
//! [`GameflowPhase::Other`] verbatim rather than under a neighbour's name.

/// Where the client is between the lobby and the end of a game.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GameflowPhase {
    /// The client's `None`: nothing going on.
    Nothing,
    Lobby,
    Matchmaking,
    ReadyCheck,
    ChampSelect,
    /// The client is starting the game process. The last moment an overlay
    /// may still change.
    GameStart,
    InProgress,
    Reconnect,
    WaitingForStats,
    PreEndOfGame,
    EndOfGame,
    FailedToLaunch,
    TerminatedInError,
    /// A spelling this build does not know, carried verbatim.
    Other(String),
}

impl GameflowPhase {
    /// The client's own spelling, which round-trips through [`From`].
    pub fn as_str(&self) -> &str {
        match self {
            Self::Nothing => "None",
            Self::Lobby => "Lobby",
            Self::Matchmaking => "Matchmaking",
            Self::ReadyCheck => "ReadyCheck",
            Self::ChampSelect => "ChampSelect",
            Self::GameStart => "GameStart",
            Self::InProgress => "InProgress",
            Self::Reconnect => "Reconnect",
            Self::WaitingForStats => "WaitingForStats",
            Self::PreEndOfGame => "PreEndOfGame",
            Self::EndOfGame => "EndOfGame",
            Self::FailedToLaunch => "FailedToLaunch",
            Self::TerminatedInError => "TerminatedInError",
            Self::Other(raw) => raw,
        }
    }

    /// Whether the game process exists or is about to: the overlay is frozen.
    pub fn game_is_starting_or_running(&self) -> bool {
        matches!(self, Self::GameStart | Self::InProgress | Self::Reconnect)
    }
}

impl From<&str> for GameflowPhase {
    fn from(raw: &str) -> Self {
        match raw {
            "None" => Self::Nothing,
            "Lobby" => Self::Lobby,
            "Matchmaking" => Self::Matchmaking,
            "ReadyCheck" => Self::ReadyCheck,
            "ChampSelect" => Self::ChampSelect,
            "GameStart" => Self::GameStart,
            "InProgress" => Self::InProgress,
            "Reconnect" => Self::Reconnect,
            "WaitingForStats" => Self::WaitingForStats,
            "PreEndOfGame" => Self::PreEndOfGame,
            "EndOfGame" => Self::EndOfGame,
            "FailedToLaunch" => Self::FailedToLaunch,
            "TerminatedInError" => Self::TerminatedInError,
            other => Self::Other(other.to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_spellings_round_trip() {
        for raw in [
            "None",
            "Lobby",
            "Matchmaking",
            "ReadyCheck",
            "ChampSelect",
            "GameStart",
            "InProgress",
            "Reconnect",
            "WaitingForStats",
            "PreEndOfGame",
            "EndOfGame",
            "FailedToLaunch",
            "TerminatedInError",
        ] {
            let phase = GameflowPhase::from(raw);
            assert!(!matches!(phase, GameflowPhase::Other(_)), "{raw} is known");
            assert_eq!(phase.as_str(), raw);
        }
    }

    #[test]
    fn an_unknown_spelling_is_carried_verbatim() {
        let phase = GameflowPhase::from("Rehearsal");
        assert_eq!(phase, GameflowPhase::Other("Rehearsal".to_string()));
        assert_eq!(phase.as_str(), "Rehearsal");
        assert!(!phase.game_is_starting_or_running());
    }

    #[test]
    fn the_overlay_is_frozen_from_game_start_on() {
        assert!(GameflowPhase::GameStart.game_is_starting_or_running());
        assert!(GameflowPhase::InProgress.game_is_starting_or_running());
        assert!(!GameflowPhase::ChampSelect.game_is_starting_or_running());
        assert!(!GameflowPhase::Lobby.game_is_starting_or_running());
    }
}
