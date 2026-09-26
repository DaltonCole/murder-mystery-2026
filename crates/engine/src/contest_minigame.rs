//! Real Round 2/4 mini-game mechanics for the three contest categories
//! (rules.md §4: "Strength, Creativity, and Intelligence") -- `contest.rs`'s
//! own doc comment used to say these were "explicitly out of scope for
//! this engine pass... to be designed later." Dalton's own words starting
//! this module: "it is time to start the creation of those tasks within
//! the game."
//!
//! *** PLACEHOLDER CONTENT WARNING ***: nothing in this module authors
//! real Creativity prompts or Intelligence questions -- that's the Host's
//! own job each time they call `Command::OpenContestMinigame`, typing in
//! whatever prompt/question they want live. There is no equivalent of
//! `game_server::LOCATION_TASKS`' pre-authored content table for these
//! categories yet.
//! TODO(dalton): flesh out real Creativity/Intelligence/Physical round
//! content (a bank of drawing/creative-writing prompts, trivia questions,
//! and physical challenge descriptions) before game night -- what's here
//! is deliberately rough, for testing the mechanic itself.
//!
//! Design, resolved with Dalton directly rather than guessed:
//! - Creativity is a free-text creative-writing style prompt (not an
//!   actual drawing canvas -- see the module-level scope note below) that
//!   every participant submits one entry for, then every OTHER active
//!   player rates 1-5 stars; an entry's score is the total stars it
//!   received (game-changer's own "most stars wins" framing).
//! - Intelligence is a shared question with one correct answer (checked
//!   case-insensitively, trimmed, one attempt per player -- the same
//!   "one shot" shape `AttemptLocationTask` already uses); a player's
//!   score is based on how early they solved it (first correct answer
//!   scores highest).
//! - Strength is judged live, in person -- the app never sees who actually
//!   won. Instead, Dalton's own explicit instruction: each participant
//!   self-reports their own finishing placement (1st, 2nd, ...) rather
//!   than a "Ton vs the room" vote, since players aren't supposed to know
//!   who's on which side. A placement converts to a score the same way
//!   Intelligence's solve-order does (better placement -> higher score).
//! - All three categories funnel into the *same* team-result rule: take
//!   the top `top_n` individual scores (regardless of category), sum them
//!   by true faction (Ton vs. everyone else), and whichever total is
//!   higher wins that category for the round. Dalton's own explicit
//!   instruction, replacing an earlier "whoever wins individually decides
//!   it" draft: "aggregate score by faction of the N highest scoring
//!   participants." A tie favors the room, not Ton -- an arbitrary but
//!   fixed choice (Ton needs to actually outscore the room, not just tie
//!   it), the same shape as this engine's other tie-break defaults.
//!
//! Scope note: Creativity is text, not an actual drawing canvas. Building
//! real freehand drawing (canvas capture, image storage/serialization)
//! would be a much larger, riskier lift than the mechanic itself and
//! wasn't asked for explicitly -- game-changer's `DrawingGame` was cited
//! as *inspiration* for the shape (shared prompt, peer-rated, most stars
//! wins), not a literal spec to port pixel-for-pixel. Documented here
//! rather than silently substituted.

use crate::error::GameError;
use crate::player::{Faction, PlayerId};
use crate::round::Round;
use crate::state::GameState;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

/// A generous cap for a Creativity entry or an Intelligence answer -- both
/// free text with no inherent size limit of their own. Matches the
/// reasoning behind `task::MAX_TASK_PROMPT_LEN`: a reliability guard
/// against a multi-megabyte string hitting the one process running the
/// whole live event, not a rules.md quote.
pub const MAX_CONTEST_ENTRY_LEN: usize = 500;

/// How many of the top individual scorers (by raw score, any faction)
/// actually decide a mini-game-backed contest category -- Dalton's own
/// instruction: "Set N to 5 for now, but make it configurable." Kept as
/// one named constant (the same "*** EDIT THIS ***" shape as
/// `game_server::auto_task_counts`) rather than threaded through every
/// command, so retuning it before game night is a one-line change.
pub const DEFAULT_TOP_N_SCORERS: usize = 5;

/// One category's live mini-game session for one contest round --
/// `GameState` keeps one of these per currently-open `(Round,
/// ContestCategory)` pair (see `GameState::contest_minigames`), since
/// Round 4 runs all three simultaneously in separate zones.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContestMinigameSession {
    /// What the Host typed in when opening this session -- the creative
    /// writing prompt, the trivia question, or a plain description of the
    /// physical challenge (informational only for Strength).
    pub prompt: String,
    /// Intelligence only: the correct answer, checked case-insensitively
    /// and trimmed, the same way `AttemptLocationTask`'s code check works.
    /// `None` for Creativity/Strength.
    pub correct_answer: Option<String>,
    /// Creativity: each participant's one free-text entry.
    pub creative_entries: BTreeMap<PlayerId, String>,
    /// Creativity: `rater -> target -> stars (1-5)`. A repeat rating of
    /// the same target replaces the earlier one -- the same "a standing
    /// choice, not a one-shot" shape `Nominate`/`CastBallot` already use,
    /// since there's no reason to lock in a hasty first rating before the
    /// session closes.
    pub creative_ratings: BTreeMap<PlayerId, BTreeMap<PlayerId, u8>>,
    /// Intelligence: every player who has already attempted (right or
    /// wrong) -- gates the one-shot re-attempt the same way
    /// `GameState::task_attempts` gates `AttemptTask`.
    pub intelligence_attempts: BTreeSet<PlayerId>,
    /// Intelligence: correct answers in the order they arrived -- the
    /// first entry solved it fastest.
    pub correct_order: Vec<PlayerId>,
    /// Strength: each participant's self-reported finishing placement
    /// (1 = won outright). One entry per player, one-shot (key presence
    /// gates a repeat submission).
    pub placements: BTreeMap<PlayerId, u32>,
}

impl ContestMinigameSession {
    fn new(prompt: String, correct_answer: Option<String>) -> Self {
        Self {
            prompt,
            correct_answer,
            creative_entries: BTreeMap::new(),
            creative_ratings: BTreeMap::new(),
            intelligence_attempts: BTreeSet::new(),
            correct_order: Vec::new(),
            placements: BTreeMap::new(),
        }
    }

    /// Every Creativity entry's score -- total stars received, "most stars
    /// wins" per game-changer's own framing. An entry nobody rated yet
    /// scores 0, not absent (it still deserves a place in the standings).
    pub(crate) fn creative_scores(&self) -> Vec<(PlayerId, i64)> {
        self.creative_entries
            .keys()
            .map(|&id| {
                let total: i64 = self
                    .creative_ratings
                    .values()
                    .filter_map(|by_target| by_target.get(&id))
                    .map(|&stars| i64::from(stars))
                    .sum();
                (id, total)
            })
            .collect()
    }

    /// Every correct Intelligence solver's score -- the first solver
    /// scores `n` (the total number who ever solved it), the last scores
    /// 1. Anyone who never solved it (or never attempted) simply isn't in
    /// this list, which `resolve_ton_won` already treats as a score of 0.
    pub(crate) fn intelligence_scores(&self) -> Vec<(PlayerId, i64)> {
        let n = self.correct_order.len() as i64;
        self.correct_order
            .iter()
            .enumerate()
            .map(|(i, &id)| (id, n - i as i64))
            .collect()
    }

    /// Every Strength participant's score, derived from their self-reported
    /// placement -- 1st place among `n` participants scores `n`, last
    /// place scores 1.
    pub(crate) fn physical_scores(&self) -> Vec<(PlayerId, i64)> {
        let n = self.placements.len() as i64;
        self.placements
            .iter()
            .map(|(&id, &placement)| (id, (n - i64::from(placement) + 1).max(0)))
            .collect()
    }
}

/// Resolves a mini-game-backed contest category into `RecordContestResult`'s
/// `ton_won` -- see this module's doc comment for the "top N scorers,
/// summed by true faction, higher total wins, ties favor the room" rule
/// Dalton specified directly.
pub(crate) fn resolve_ton_won(state: &GameState, scores: &[(PlayerId, i64)], top_n: usize) -> bool {
    let mut sorted = scores.to_vec();
    sorted.sort_by(|a, b| b.1.cmp(&a.1));

    let mut ton_total: i64 = 0;
    let mut room_total: i64 = 0;
    for &(id, score) in sorted.iter().take(top_n) {
        let is_ton = state
            .player(id)
            .map(|p| p.true_faction())
            .is_some_and(|f| f == Faction::Ton);
        if is_ton {
            ton_total += score;
        } else {
            room_total += score;
        }
    }
    ton_total > room_total
}

/// Shared length cap check for a Creativity entry or an Intelligence
/// answer -- see `MAX_CONTEST_ENTRY_LEN`'s doc comment.
pub(crate) fn check_entry_len(field: &'static str, text: &str) -> Result<(), GameError> {
    let len = text.chars().count();
    if len > MAX_CONTEST_ENTRY_LEN {
        return Err(GameError::FieldTooLong {
            field,
            len,
            max: MAX_CONTEST_ENTRY_LEN,
        });
    }
    Ok(())
}

/// rules.md §4: contests only ever run in Round 2 or Round 4 -- the same
/// check `record_contest_result` already makes, reused here so every
/// mini-game command rejects an out-of-round mistake with the same
/// familiar error rather than a bespoke one.
pub(crate) fn check_is_contest_round(round: Round) -> Result<(), GameError> {
    if !matches!(round, Round::Two | Round::Four) {
        return Err(GameError::NotAContestRound(round));
    }
    Ok(())
}

pub(crate) fn new_session(
    prompt: String,
    correct_answer: Option<String>,
) -> ContestMinigameSession {
    ContestMinigameSession::new(prompt, correct_answer)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::command::Command;
    use crate::event::DomainEvent;
    use crate::state::apply_command;

    fn add_player(state: &mut GameState, name: &str, faction: Faction) -> PlayerId {
        let events = apply_command(
            state,
            Command::AddPlayer {
                name: name.to_string(),
            },
        )
        .unwrap();
        let id = match events[0] {
            DomainEvent::PlayerAdded { id, .. } => id,
            _ => unreachable!(),
        };
        apply_command(
            state,
            Command::AssignFaction {
                player: id,
                faction,
            },
        )
        .unwrap();
        id
    }

    #[test]
    fn resolve_ton_won_sums_the_top_n_scores_by_true_faction() {
        let mut state = GameState::new();
        let ton_a = add_player(&mut state, "TonA", Faction::Ton);
        let ton_b = add_player(&mut state, "TonB", Faction::Ton);
        let room_a = add_player(&mut state, "RoomA", Faction::Uprising);
        let room_b = add_player(&mut state, "RoomB", Faction::Cult);
        apply_command(&mut state, Command::FinalizeSetup).unwrap();

        // Top 2 by score: ton_a (10) + room_a (8) = 18 vs room combined --
        // only the top 2 count, so ton_b/room_b (both scoring low) never
        // enter the calculation at all.
        let scores = vec![(ton_a, 10), (room_a, 8), (ton_b, 1), (room_b, 1)];
        assert!(resolve_ton_won(&state, &scores, 2));

        // With top_n covering everyone, Room's total (8 + 1 + 1 = 10)
        // still loses to Ton's (10 + 1 = 11).
        assert!(resolve_ton_won(&state, &scores, 4));
    }

    #[test]
    fn resolve_ton_won_ties_favor_the_room() {
        let mut state = GameState::new();
        let ton_a = add_player(&mut state, "TonA", Faction::Ton);
        let room_a = add_player(&mut state, "RoomA", Faction::Uprising);
        apply_command(&mut state, Command::FinalizeSetup).unwrap();

        assert!(!resolve_ton_won(&state, &[(ton_a, 5), (room_a, 5)], 2));
    }

    // `resolve_ton_won_treats_a_converted_player_as_room_not_ton` lives in
    // `state.rs`'s own test module instead -- producing a real converted
    // player needs `Command::Convert`'s full setup (a Cult Leader, a
    // recruitment slot), which relies on private test helpers already
    // defined there.

    #[test]
    fn creative_scores_sum_stars_received_across_every_rater() {
        let mut session = ContestMinigameSession::new("Draw a cat".into(), None);
        session.creative_entries.insert(PlayerId(0), "a cat".into());
        session.creative_entries.insert(PlayerId(1), "a dog".into());
        session
            .creative_ratings
            .entry(PlayerId(2))
            .or_default()
            .insert(PlayerId(0), 5);
        session
            .creative_ratings
            .entry(PlayerId(3))
            .or_default()
            .insert(PlayerId(0), 4);
        session
            .creative_ratings
            .entry(PlayerId(2))
            .or_default()
            .insert(PlayerId(1), 1);

        let scores = session.creative_scores();
        assert!(scores.contains(&(PlayerId(0), 9)));
        assert!(scores.contains(&(PlayerId(1), 1)));
    }

    #[test]
    fn intelligence_scores_reward_earlier_solvers() {
        let mut session = ContestMinigameSession::new("2+2?".into(), Some("4".into()));
        session.correct_order = vec![PlayerId(0), PlayerId(1), PlayerId(2)];
        assert_eq!(
            session.intelligence_scores(),
            vec![(PlayerId(0), 3), (PlayerId(1), 2), (PlayerId(2), 1)]
        );
    }

    #[test]
    fn physical_scores_reward_better_placement() {
        let mut session = ContestMinigameSession::new("Tug of war".into(), None);
        session.placements.insert(PlayerId(0), 1);
        session.placements.insert(PlayerId(1), 3);
        session.placements.insert(PlayerId(2), 2);
        let scores = session.physical_scores();
        assert!(scores.contains(&(PlayerId(0), 3)));
        assert!(scores.contains(&(PlayerId(1), 1)));
        assert!(scores.contains(&(PlayerId(2), 2)));
    }
}
