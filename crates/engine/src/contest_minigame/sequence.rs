//! Automated multi-game contest sequences (TODO.md's Contest Rounds
//! rewrite): Round 2 runs Drawing, then Trivia, then a push-up contest,
//! strictly one after another, with no Host click between them; Round 4
//! runs three such sequences in parallel, one per category, each three
//! games deep (whichever three a player's chosen category didn't already
//! use in Round 2). Both are the exact same mechanism here -- a queue of
//! not-yet-opened steps plus each finished step's raw result -- just
//! Round 2's happens to be one step long per category.
//!
//! `close_contest_minigame` (`state.rs`) is the only thing that reads
//! this: it's sequence-aware, so every existing way a session ever closes
//! (the Host's manual "Close & resolve", Creativity's own "Rating reached
//! the end" auto-close, `game_server`'s new auto-close-on-full-
//! participation) transparently opens the next step or finalizes by
//! majority, with no other call site needing to know a sequence exists at
//! all.

use super::OpenMinigameDetail;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContestSequence {
    /// Not-yet-opened steps, in order -- `(prompt, detail)`. The *live*
    /// step is the real `ContestMinigameSession` itself, not stored here.
    pub upcoming: VecDeque<(String, OpenMinigameDetail)>,
    /// Each finished step's raw `ton_won`, oldest first.
    pub completed: Vec<bool>,
}

/// One level of Dalton's own explicit majority rule ("majority of the
/// game winners win the category, and majority winner of the category
/// wins the round") -- applied identically whether `results` are games
/// deciding a category or categories deciding a round. Ties favor the
/// room, the same tiebreak `resolve_ton_won` itself already uses one
/// level down.
pub fn majority_ton_won(results: &[bool]) -> bool {
    let ton = results.iter().filter(|&&r| r).count();
    ton * 2 > results.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn majority_ton_won_needs_a_real_majority_not_just_a_plurality() {
        assert!(majority_ton_won(&[true, true, false]));
        assert!(!majority_ton_won(&[true, false, false]));
    }

    #[test]
    fn majority_ton_won_ties_favor_the_room() {
        assert!(!majority_ton_won(&[true, false]));
        assert!(!majority_ton_won(&[true, true, false, false]));
    }

    #[test]
    fn majority_ton_won_of_one_is_just_that_one_result() {
        assert!(majority_ton_won(&[true]));
        assert!(!majority_ton_won(&[false]));
    }
}
