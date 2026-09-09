use crate::character::Character;
use crate::player::{Faction, PlayerId};
use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum GameError {
    #[error("no player with id {0:?}")]
    UnknownPlayer(PlayerId),

    #[error("player {0:?} already has an assigned faction")]
    AlreadyAssigned(PlayerId),

    #[error("player {player:?} has faction {actual:?}, which cannot hold {character:?}")]
    WrongFactionForCharacter {
        player: PlayerId,
        character: Character,
        actual: Faction,
    },

    #[error("{character:?} is already held by {holder:?}")]
    TitleAlreadyHeld {
        character: Character,
        holder: PlayerId,
    },

    #[error("player {0:?} is not currently active")]
    NotActive(PlayerId),

    #[error("player {0:?} is not the current Revolutionary Leader")]
    NotCurrentLeader(PlayerId),

    #[error("player {0:?} is not an eligible Revolutionary Leader successor")]
    IneligibleSuccessor(PlayerId),

    #[error("the King/Queen transfer ability has already been used")]
    KingQueenTransferAlreadyUsed,

    #[error("the King/Queen transfer ability can only be used before Round 5")]
    KingQueenTransferTooLate,

    #[error("player {0:?} is not an eligible King/Queen replacement")]
    IneligibleKingQueenReplacement(PlayerId),

    #[error("the game is already at the Finale")]
    AlreadyAtFinale,
}
