//! Real Round 2/4 mini-game mechanics for the three contest categories
//! (rules.md §4: "Strength, Creativity, and Intelligence") -- `contest.rs`'s
//! own doc comment used to say these were "explicitly out of scope for
//! this engine pass... to be designed later." Dalton's own words starting
//! this module: "it is time to start the creation of those tasks within
//! the game," later expanded into four distinct kinds each for Creativity
//! and Intelligence.
//!
//! **Type structure, Dalton's own explicit correction**: every categorization
//! type here mirrors `ContestCategory`'s own 3-way split exactly --
//! `Strength`, `Creativity`, and `Intelligence` are the only top-level
//! variants anywhere (`MinigamePayload`, `OpenMinigameDetail`), with
//! Creativity's four kinds living inside `CreativityPayload`/
//! `CreativityKind` and Intelligence's four kinds nested inside one
//! `IntelligencePayload`/`IntelligenceKind` wrapper -- never as separate
//! top-level slots.
//!
//! **Design, resolved with Dalton directly rather than guessed** (see this
//! project's memory/plan file for the full history):
//! - Creativity: Drawing (a real freehand canvas), Joke, Dictionarium,
//!   Smut-acular -- one write phase, then one-at-a-time rating (self-rating
//!   allowed), all real, server-enforced, auto-advancing timers (see
//!   `game_server`'s minigame-deadline sweep). An entry's score is the
//!   median of its received stars, mean breaks a tie (`creativity::
//!   CreativityPayload::scores`).
//! - Intelligence: Trivia and Math (identical shared-quiz mechanic, see
//!   `quiz`), Memory (a client-side color-sequence game, see `memory`),
//!   and Wordle (see `wordle`, ported from `/home/drc/game-changer`).
//! - Strength: unchanged from the original build -- self-reported
//!   placement, never a "Ton vs the room" vote (players aren't supposed to
//!   know who's on which side).
//! - All four scoring functions (Creativity, Quiz, Memory, Wordle) feed
//!   the *same* team-result rule: take the top `DEFAULT_TOP_N_SCORERS`
//!   individual scores, sum them by true faction (Ton vs. everyone else),
//!   higher total wins, ties favor the room (`resolve_ton_won`).
//!
//! Every category now ships with real, authored content, not a
//! placeholder: Trivia and Math have real 10-question banks (`quiz::
//! trivia_questions`/`math_questions`), Wordle reuses game-changer's own
//! real, audited word list, and Drawing/Strength draw randomly from
//! `game_server::DRAWING_PROMPTS`/`PHYSICAL_CHALLENGES` (the same
//! `LOCATION_TASKS`-style curated-bank pattern, server-side since the draw
//! needs a real RNG) rather than being Host-typed. Joke/Dictionarium/Smut
//! have no content bank of their own -- each player invents their own
//! joke/word/scene live, so there's nothing to draw from a bank; the
//! Host's optional prompt there is just flavor text.

mod creativity;
mod memory;
mod quiz;
mod sequence;
mod wordle;

pub use creativity::{
    CreativeEntry, CreativityKind, CreativityPayload, CreativityPhase, RatingStep,
    DRAWING_DATA_URL_PREFIX, MAX_DRAWING_DATA_URL_LEN,
};
pub use memory::{MemoryPayload, MAX_MEMORY_SEQUENCE_LENGTH};
pub use quiz::{math_questions, trivia_questions, QuizKind, QuizPayload, QuizQuestion};
pub use sequence::{majority_ton_won, ContestSequence};
pub use wordle::{LetterFeedback, WordleGuess, WordlePayload, MAX_GUESSES, WORD_LIST};

// Crate-internal only -- `state.rs`'s command handlers need these, but
// they're not part of this module's public (app-facing) surface.
pub(crate) use creativity::check_entry as check_creative_entry;
pub(crate) use memory::check_sequence_length as check_memory_sequence_length;
pub(crate) use wordle::{
    normalize_guess as normalize_wordle_guess, score_guess as score_wordle_guess,
};

use crate::contest::ContestCategory;
use crate::error::GameError;
use crate::player::{Faction, PlayerId};
use crate::state::GameState;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

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

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PhysicalPayload {
    pub placements: BTreeMap<PlayerId, u32>,
}

/// One category's live mini-game session for one contest round --
/// `GameState` keeps one of these per currently-open `(Round,
/// ContestCategory)` pair, since Round 4 runs all three simultaneously in
/// separate zones (confirmed live).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContestMinigameSession {
    pub prompt: String,
    pub payload: MinigamePayload,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum MinigamePayload {
    Strength(PhysicalPayload),
    Creativity(CreativityPayload),
    Intelligence(IntelligencePayload),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum IntelligencePayload {
    Trivia(QuizPayload),
    Math(QuizPayload),
    Memory(MemoryPayload),
    Wordle(WordlePayload),
}

/// What the Host supplies when opening a session -- everything needed to
/// build the right `MinigamePayload`. `category()` derives the
/// `ContestCategory` this belongs under, so a `detail`/category mismatch
/// (the flat original command's own risk) is structurally impossible.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum OpenMinigameDetail {
    Strength,
    Creativity(CreativityKind),
    Intelligence(IntelligenceKind),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum IntelligenceKind {
    /// Exactly 10 questions -- validated by `quiz::check_question_count`.
    Trivia {
        questions: Vec<QuizQuestion>,
    },
    Math {
        questions: Vec<QuizQuestion>,
    },
    Memory,
    /// Host-only-visible ground truth -- see
    /// `crate::view::ContestMinigameView::answer`'s doc comment.
    Wordle {
        secret: String,
    },
}

impl OpenMinigameDetail {
    pub fn category(&self) -> ContestCategory {
        match self {
            OpenMinigameDetail::Strength => ContestCategory::Strength,
            OpenMinigameDetail::Creativity(_) => ContestCategory::Creativity,
            OpenMinigameDetail::Intelligence(_) => ContestCategory::Intelligence,
        }
    }
}

/// Builds a fresh, empty session from `detail` -- validating whatever
/// `detail` itself requires (exactly 10 quiz questions, a real 5-letter
/// Wordle secret) before constructing anything.
pub(crate) fn new_session(
    prompt: String,
    detail: OpenMinigameDetail,
) -> Result<ContestMinigameSession, GameError> {
    let payload = match detail {
        OpenMinigameDetail::Strength => MinigamePayload::Strength(PhysicalPayload::default()),
        OpenMinigameDetail::Creativity(kind) => {
            MinigamePayload::Creativity(CreativityPayload::new(kind))
        }
        OpenMinigameDetail::Intelligence(IntelligenceKind::Trivia { questions }) => {
            quiz::check_question_count(&questions)?;
            MinigamePayload::Intelligence(IntelligencePayload::Trivia(QuizPayload {
                questions,
                progress: BTreeMap::new(),
            }))
        }
        OpenMinigameDetail::Intelligence(IntelligenceKind::Math { questions }) => {
            quiz::check_question_count(&questions)?;
            MinigamePayload::Intelligence(IntelligencePayload::Math(QuizPayload {
                questions,
                progress: BTreeMap::new(),
            }))
        }
        OpenMinigameDetail::Intelligence(IntelligenceKind::Memory) => {
            MinigamePayload::Intelligence(IntelligencePayload::Memory(MemoryPayload::new()))
        }
        OpenMinigameDetail::Intelligence(IntelligenceKind::Wordle { secret }) => {
            let secret = wordle::normalize_guess(&secret)
                .map_err(|_| GameError::WordleSecretMustBeAFiveLetterWord)?;
            MinigamePayload::Intelligence(IntelligencePayload::Wordle(WordlePayload {
                secret,
                progress: BTreeMap::new(),
            }))
        }
    };
    Ok(ContestMinigameSession { prompt, payload })
}

/// Every participant's score for whichever mechanic this session actually
/// is -- the one place all four scoring functions are dispatched to.
pub(crate) fn scores(session: &ContestMinigameSession) -> Vec<(PlayerId, i64)> {
    match &session.payload {
        MinigamePayload::Strength(payload) => physical_scores(payload),
        MinigamePayload::Creativity(payload) => payload.scores(),
        MinigamePayload::Intelligence(IntelligencePayload::Trivia(payload))
        | MinigamePayload::Intelligence(IntelligencePayload::Math(payload)) => {
            quiz::quiz_scores(payload)
        }
        MinigamePayload::Intelligence(IntelligencePayload::Memory(payload)) => {
            memory::memory_scores(payload)
        }
        MinigamePayload::Intelligence(IntelligencePayload::Wordle(payload)) => {
            wordle::wordle_scores(payload)
        }
    }
}

/// How many participants have *finished* submitting to this session -- an
/// entry (Creativity), a completed quiz/Wordle attempt (not just started),
/// a memory score, or a placement, whichever this session's mechanic uses.
/// `pub` (not `pub(crate)`): both `view::contest_minigame_view` (the
/// player-facing `ContestMinigameView::submission_count`) and
/// `game_server`'s auto-close-on-full-participation sweep need this same
/// count -- the latter compares it against how many players are actually
/// expected to submit, to decide when a session (never Creativity, which
/// has its own real-timer auto-advance) can close itself with no Host
/// click.
pub fn submission_count(session: &ContestMinigameSession) -> usize {
    match &session.payload {
        MinigamePayload::Strength(payload) => payload.placements.len(),
        MinigamePayload::Creativity(payload) => payload.entries.len(),
        MinigamePayload::Intelligence(IntelligencePayload::Trivia(quiz))
        | MinigamePayload::Intelligence(IntelligencePayload::Math(quiz)) => quiz
            .progress
            .values()
            .filter(|p| p.answers.len() == quiz.questions.len())
            .count(),
        MinigamePayload::Intelligence(IntelligencePayload::Memory(scores)) => scores.len(),
        MinigamePayload::Intelligence(IntelligencePayload::Wordle(payload)) => payload
            .progress
            .values()
            .filter(|p| p.solved || p.guesses.len() >= MAX_GUESSES)
            .count(),
    }
}

fn physical_scores(payload: &PhysicalPayload) -> Vec<(PlayerId, i64)> {
    let n = payload.placements.len() as i64;
    payload
        .placements
        .iter()
        .map(|(&id, &placement)| (id, (n - i64::from(placement) + 1).max(0)))
        .collect()
}

/// Resolves a mini-game-backed contest category into `RecordContestResult`'s
/// `ton_won` -- see this module's doc comment for the "top N scorers,
/// summed by true faction, higher total wins, ties favor the room" rule
/// Dalton specified directly. Relies on `scores` handing back a stably
/// pre-ordered `Vec` for exact-score ties (Creativity's median/mean,
/// Quiz's/Wordle's speed tiebreaks) -- `Vec::sort_by` is a stable sort, so
/// this function's own re-sort-by-score-alone never disturbs that order.
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

/// Shared length cap check for a session's `prompt` -- see
/// `MAX_CONTEST_ENTRY_LEN`'s doc comment.
pub(crate) fn check_prompt_len(prompt: &str) -> Result<(), GameError> {
    let len = prompt.chars().count();
    if len > MAX_CONTEST_ENTRY_LEN {
        return Err(GameError::FieldTooLong {
            field: "contest mini-game prompt",
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
pub(crate) fn check_is_contest_round(round: crate::round::Round) -> Result<(), GameError> {
    if !matches!(round, crate::round::Round::Two | crate::round::Round::Four) {
        return Err(GameError::NotAContestRound(round));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::command::Command;
    use crate::event::DomainEvent;
    use crate::player::Faction;
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
    fn open_minigame_detail_category_matches_each_variant() {
        assert_eq!(
            OpenMinigameDetail::Strength.category(),
            ContestCategory::Strength
        );
        assert_eq!(
            OpenMinigameDetail::Creativity(CreativityKind::Joke).category(),
            ContestCategory::Creativity
        );
        assert_eq!(
            OpenMinigameDetail::Intelligence(IntelligenceKind::Memory).category(),
            ContestCategory::Intelligence
        );
    }

    #[test]
    fn new_session_rejects_a_miscounted_quiz_bank() {
        let mut questions = trivia_questions();
        questions.pop();
        let result = new_session(
            "Trivia!".into(),
            OpenMinigameDetail::Intelligence(IntelligenceKind::Trivia { questions }),
        );
        assert_eq!(result, Err(GameError::QuizMustHaveExactlyTenQuestions(9)));
    }

    #[test]
    fn new_session_rejects_a_wordle_secret_that_isnt_five_letters() {
        let result = new_session(
            "Wordle!".into(),
            OpenMinigameDetail::Intelligence(IntelligenceKind::Wordle {
                secret: "TOOLONG".into(),
            }),
        );
        assert_eq!(result, Err(GameError::WordleSecretMustBeAFiveLetterWord));
    }

    #[test]
    fn resolve_ton_won_sums_the_top_n_scores_by_true_faction() {
        let mut state = GameState::new();
        let ton_a = add_player(&mut state, "TonA", Faction::Ton);
        let ton_b = add_player(&mut state, "TonB", Faction::Ton);
        let room_a = add_player(&mut state, "RoomA", Faction::Uprising);
        let room_b = add_player(&mut state, "RoomB", Faction::Cult);
        apply_command(&mut state, Command::FinalizeSetup).unwrap();

        let scores = vec![(ton_a, 10), (room_a, 8), (ton_b, 1), (room_b, 1)];
        assert!(resolve_ton_won(&state, &scores, 2));
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
}
