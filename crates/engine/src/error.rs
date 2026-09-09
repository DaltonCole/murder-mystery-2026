use crate::player::PlayerId;
use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum GameError {
    #[error("no player with id {0:?}")]
    UnknownPlayer(PlayerId),

    #[error("player {0:?} already has an assigned faction")]
    AlreadyAssigned(PlayerId),
}
