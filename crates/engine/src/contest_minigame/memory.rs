//! Memory: a client-side color-sequence (Simon-style) game. The engine
//! never generates or verifies the sequence itself -- the browser has no
//! Rust-side randomness source to draw from anyway (`rand` is a
//! `server`-feature-only dependency), and unlike Wordle/Quiz this
//! category's outcome doesn't need to stay hidden from anyone, so there's
//! nothing here worth being server-authoritative about. The engine only
//! ever receives a player's own final, self-reported score -- the same
//! trust model as Strength's existing self-reported placement.

use crate::error::GameError;
use crate::player::PlayerId;
use std::collections::BTreeMap;

pub type MemoryPayload = BTreeMap<PlayerId, u32>;

/// A generous sanity cap on a self-reported sequence length -- not
/// anti-cheat (nothing here is server-verified), the same reliability
/// guard `MAX_TASK_PROMPT_LEN`/`MAX_CONTEST_ENTRY_LEN` exist for: keeping
/// an absurd number out of the same "clone this into a broadcast to every
/// client" pipeline every other mutation already goes through.
pub const MAX_MEMORY_SEQUENCE_LENGTH: u32 = 100;

pub(crate) fn check_sequence_length(longest_sequence: u32) -> Result<(), GameError> {
    if longest_sequence > MAX_MEMORY_SEQUENCE_LENGTH {
        return Err(GameError::MemorySequenceOutOfRange(longest_sequence));
    }
    Ok(())
}

/// Memory has no stated tiebreak (unlike Trivia/Math/Wordle) -- equal
/// scores tie normally, by design, not by oversight.
pub(crate) fn memory_scores(payload: &MemoryPayload) -> Vec<(PlayerId, i64)> {
    payload
        .iter()
        .map(|(&id, &longest_sequence)| (id, i64::from(longest_sequence)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn memory_scores_uses_the_raw_sequence_length_with_no_tiebreak_ordering() {
        let mut payload: MemoryPayload = BTreeMap::new();
        payload.insert(PlayerId(0), 12);
        payload.insert(PlayerId(1), 12);
        payload.insert(PlayerId(2), 30);
        let mut scores = memory_scores(&payload);
        scores.sort();
        assert_eq!(
            scores,
            vec![(PlayerId(0), 12), (PlayerId(1), 12), (PlayerId(2), 30)]
        );
    }

    #[test]
    fn check_sequence_length_rejects_above_the_cap() {
        assert!(check_sequence_length(MAX_MEMORY_SEQUENCE_LENGTH).is_ok());
        assert_eq!(
            check_sequence_length(MAX_MEMORY_SEQUENCE_LENGTH + 1),
            Err(GameError::MemorySequenceOutOfRange(
                MAX_MEMORY_SEQUENCE_LENGTH + 1
            ))
        );
    }
}
