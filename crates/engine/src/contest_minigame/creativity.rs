//! Creativity's four kinds -- Drawing, Joke, Dictionarium, Smut-acular --
//! share one mechanic end to end (write an entry under a time limit,
//! then everyone rates every entry 1-5 stars one at a time, self-rating
//! included), differing only in the shape of what gets written and how
//! long each phase lasts (the durations themselves are an app-layer
//! concern -- see `game_server`'s per-kind duration table). Modeling this
//! as one concrete `CreativityPayload` holding a `CreativeEntry` enum
//! (rather than four separate generic instantiations) keeps the "it's
//! really one mechanism" story simple: kind-mismatch is one runtime check
//! (`CreativeEntry::kind`), not four monomorphized types to keep in sync.

use crate::error::GameError;
use crate::player::PlayerId;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use super::MAX_CONTEST_ENTRY_LEN;

/// A generous cap for a submitted PNG drawing -- matches game-changer's
/// own `MAX_DRAWING_DATA_URL_BYTES`, the same order of magnitude reasoning:
/// generous for a small canvas doodle, still a real DoS guard given every
/// mutation broadcasts a fresh clone to every connected client.
pub const MAX_DRAWING_DATA_URL_LEN: usize = 1024 * 1024;

/// The exact prefix `canvas.toDataURL('image/png')` always produces (see
/// `game_server`'s `CANVAS_TO_DATA_URL_JS`, ported from game-changer's own
/// `drawing_game.rs`) -- required, not just accepted, since a drawing
/// round-trips straight into `img { src: ... }` on every rater's screen: a
/// real XSS guard, not a formality. Ruling out anything but this literal
/// prefix rules out e.g. `data:image/svg+xml,...` (which could embed
/// markup) ever being accepted.
pub const DRAWING_DATA_URL_PREFIX: &str = "data:image/png;base64,";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CreativityKind {
    Drawing,
    Joke,
    Dictionarium,
    Smut,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum CreativeEntry {
    /// A PNG data URL from the drawing canvas.
    Drawing(String),
    Joke {
        text: String,
    },
    Dictionarium {
        word: String,
        definition: String,
        example: String,
    },
    Smut {
        text: String,
    },
}

impl CreativeEntry {
    pub fn kind(&self) -> CreativityKind {
        match self {
            CreativeEntry::Drawing(_) => CreativityKind::Drawing,
            CreativeEntry::Joke { .. } => CreativityKind::Joke,
            CreativeEntry::Dictionarium { .. } => CreativityKind::Dictionarium,
            CreativeEntry::Smut { .. } => CreativityKind::Smut,
        }
    }

    /// A blank stand-in for a participant who missed the Writing
    /// deadline -- inserted by `AdvanceCreativeWriting` so the rating
    /// rotation always has exactly one entry per active participant.
    pub fn blank(kind: CreativityKind) -> CreativeEntry {
        match kind {
            CreativityKind::Drawing => CreativeEntry::Drawing(String::new()),
            CreativityKind::Joke => CreativeEntry::Joke {
                text: String::new(),
            },
            CreativityKind::Dictionarium => CreativeEntry::Dictionarium {
                word: String::new(),
                definition: String::new(),
                example: String::new(),
            },
            CreativityKind::Smut => CreativeEntry::Smut {
                text: String::new(),
            },
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CreativityPhase {
    Writing,
    Rating { current_index: usize },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreativityPayload {
    pub kind: CreativityKind,
    pub phase: CreativityPhase,
    pub entries: BTreeMap<PlayerId, CreativeEntry>,
    /// Fixed once, the moment Writing ends -- the full active-participant
    /// list in app-shuffled order (randomness at the boundary, same shape
    /// as `CastOut`'s `fallback_replacement`/`run_raffle`'s shuffle). Not
    /// just those who submitted; a missing entry is blank-filled in
    /// `entries` at that same moment.
    pub rating_order: Vec<PlayerId>,
    /// `rater -> target -> stars (1-5)`. Self-rating allowed by design
    /// (Dalton's explicit instruction, matching game-changer's own
    /// DrawingGame precedent). A repeat rating of the same target
    /// replaces the earlier one -- the same "standing choice, not
    /// one-shot" shape `Nominate`/`CastBallot` already use.
    pub ratings: BTreeMap<PlayerId, BTreeMap<PlayerId, u8>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RatingStep {
    Forward,
    Backward,
}

impl CreativityPayload {
    pub fn new(kind: CreativityKind) -> Self {
        Self {
            kind,
            phase: CreativityPhase::Writing,
            entries: BTreeMap::new(),
            rating_order: Vec::new(),
            ratings: BTreeMap::new(),
        }
    }

    /// Writing -> Rating{0}. `order` is supplied by the caller (app-layer
    /// shuffle); any active participant missing an entry is blank-filled
    /// here. A no-op if not currently in the Writing phase (a stale/late
    /// automatic-sweep or Host click racing an already-advanced session).
    pub fn advance_writing(&mut self, order: Vec<PlayerId>) {
        if !matches!(self.phase, CreativityPhase::Writing) {
            return;
        }
        for &id in &order {
            self.entries
                .entry(id)
                .or_insert_with(|| CreativeEntry::blank(self.kind));
        }
        self.rating_order = order;
        self.phase = CreativityPhase::Rating { current_index: 0 };
    }

    /// Rating{i} -> Rating{i +/- 1}, or (Forward past the last index) ->
    /// `true` to signal the session is done and should be closed/resolved
    /// by the caller (`close_contest_minigame` in `state.rs`, which is
    /// what actually computes scores and calls `record_contest_result`).
    /// Backward at index 0, or on an empty rotation, is a harmless no-op.
    /// A no-op (returns `false`) if not currently in the Rating phase.
    pub fn advance_rating(&mut self, direction: RatingStep) -> bool {
        let CreativityPhase::Rating { current_index } = &mut self.phase else {
            return false;
        };
        match direction {
            RatingStep::Forward => {
                if *current_index + 1 >= self.rating_order.len() {
                    return true;
                }
                *current_index += 1;
            }
            RatingStep::Backward => {
                *current_index = current_index.saturating_sub(1);
            }
        }
        false
    }

    pub fn current_target(&self) -> Option<PlayerId> {
        match self.phase {
            CreativityPhase::Rating { current_index } => {
                self.rating_order.get(current_index).copied()
            }
            CreativityPhase::Writing => None,
        }
    }

    /// Every entry's score -- the median of the stars it received, doubled
    /// so it's always a whole number (raw stars are integers 1-5, so an
    /// even-count median's average of two integers is always exact at
    /// this scale). Pre-sorted by `(median x2 desc, mean desc)` -- Dalton's
    /// own explicit rule, "median decides it, mean breaks a tie" -- via
    /// cross-multiplied mean comparison to avoid floats entirely, so
    /// `resolve_ton_won`'s own stable re-sort-by-score-alone preserves
    /// this ordering among any exact-median ties. An entry nobody rated
    /// yet scores 0, not absent.
    pub fn scores(&self) -> Vec<(PlayerId, i64)> {
        let mut standings: Vec<(PlayerId, i64, i64, i64)> = self
            .entries
            .keys()
            .map(|&id| {
                let mut stars: Vec<u8> = self
                    .ratings
                    .values()
                    .filter_map(|by_target| by_target.get(&id))
                    .copied()
                    .collect();
                stars.sort_unstable();
                let count = stars.len() as i64;
                let sum: i64 = stars.iter().map(|&s| i64::from(s)).sum();
                let median_x2 = if stars.is_empty() {
                    0
                } else if stars.len() % 2 == 1 {
                    i64::from(stars[stars.len() / 2]) * 2
                } else {
                    i64::from(stars[stars.len() / 2 - 1]) + i64::from(stars[stars.len() / 2])
                };
                (id, median_x2, sum, count.max(1))
            })
            .collect();
        standings.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| (b.2 * a.3).cmp(&(a.2 * b.3))));
        standings
            .into_iter()
            .map(|(id, median_x2, ..)| (id, median_x2))
            .collect()
    }
}

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

pub(crate) fn check_drawing(data_url: &str) -> Result<(), GameError> {
    if data_url.is_empty() {
        return Ok(()); // a blank-filled entry for a missed deadline
    }
    if data_url.len() > MAX_DRAWING_DATA_URL_LEN {
        return Err(GameError::FieldTooLong {
            field: "drawing",
            len: data_url.len(),
            max: MAX_DRAWING_DATA_URL_LEN,
        });
    }
    if !data_url.starts_with(DRAWING_DATA_URL_PREFIX) {
        return Err(GameError::DrawingMustBeAPngDataUrl);
    }
    Ok(())
}

/// Validates one `CreativeEntry`'s field lengths against
/// `MAX_CONTEST_ENTRY_LEN` (or the drawing-specific checks above).
pub(crate) fn check_entry(entry: &CreativeEntry) -> Result<(), GameError> {
    match entry {
        CreativeEntry::Drawing(data_url) => check_drawing(data_url),
        CreativeEntry::Joke { text } | CreativeEntry::Smut { text } => {
            check_entry_len("creative entry", text)
        }
        CreativeEntry::Dictionarium {
            word,
            definition,
            example,
        } => {
            check_entry_len("dictionarium word", word)?;
            check_entry_len("dictionarium definition", definition)?;
            check_entry_len("dictionarium example", example)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn joke(text: &str) -> CreativeEntry {
        CreativeEntry::Joke {
            text: text.to_string(),
        }
    }

    #[test]
    fn advance_writing_blank_fills_missing_entries_and_moves_to_rating() {
        let mut payload = CreativityPayload::new(CreativityKind::Joke);
        payload.entries.insert(PlayerId(0), joke("a real joke"));
        payload.advance_writing(vec![PlayerId(0), PlayerId(1)]);
        assert_eq!(payload.phase, CreativityPhase::Rating { current_index: 0 });
        assert_eq!(payload.entries.get(&PlayerId(1)), Some(&joke("")));
        assert_eq!(payload.rating_order, vec![PlayerId(0), PlayerId(1)]);
    }

    #[test]
    fn advance_rating_walks_forward_and_signals_done_at_the_end() {
        let mut payload = CreativityPayload::new(CreativityKind::Joke);
        payload.advance_writing(vec![PlayerId(0), PlayerId(1)]);
        assert!(!payload.advance_rating(RatingStep::Forward));
        assert_eq!(payload.phase, CreativityPhase::Rating { current_index: 1 });
        assert!(payload.advance_rating(RatingStep::Forward));
        // Signaling "done" doesn't itself mutate the phase -- the caller
        // (state.rs) is responsible for closing the session.
        assert_eq!(payload.phase, CreativityPhase::Rating { current_index: 1 });
    }

    #[test]
    fn advance_rating_backward_at_zero_is_a_no_op() {
        let mut payload = CreativityPayload::new(CreativityKind::Joke);
        payload.advance_writing(vec![PlayerId(0), PlayerId(1)]);
        assert!(!payload.advance_rating(RatingStep::Backward));
        assert_eq!(payload.phase, CreativityPhase::Rating { current_index: 0 });
    }

    #[test]
    fn scores_use_median_with_mean_as_a_tiebreak() {
        let mut payload = CreativityPayload::new(CreativityKind::Joke);
        payload.entries.insert(PlayerId(0), joke("a"));
        payload.entries.insert(PlayerId(1), joke("b"));
        // Both entries get a median of 3 (ratings [1,3,5] vs [2,3,4]), but
        // player 1's mean (3.0) beats... actually equal means too -- use
        // asymmetric ratings so the medians tie but means differ.
        payload.ratings.insert(
            PlayerId(10),
            BTreeMap::from([(PlayerId(0), 3u8), (PlayerId(1), 3u8)]),
        );
        payload.ratings.insert(
            PlayerId(11),
            BTreeMap::from([(PlayerId(0), 1u8), (PlayerId(1), 4u8)]),
        );
        payload.ratings.insert(
            PlayerId(12),
            BTreeMap::from([(PlayerId(0), 5u8), (PlayerId(1), 3u8)]),
        );
        // Player 0: [1,3,5] median 3, mean 3.0. Player 1: [3,3,4] median 3, mean 10/3 ~ 3.33.
        let scores = payload.scores();
        let ids: Vec<PlayerId> = scores.iter().map(|&(id, _)| id).collect();
        assert_eq!(
            ids,
            vec![PlayerId(1), PlayerId(0)],
            "equal median, higher mean must rank first"
        );
        assert_eq!(scores[0].1, 6); // median 3 * 2
        assert_eq!(scores[1].1, 6);
    }

    #[test]
    fn an_unrated_entry_scores_zero() {
        let mut payload = CreativityPayload::new(CreativityKind::Joke);
        payload.entries.insert(PlayerId(0), joke("a"));
        assert_eq!(payload.scores(), vec![(PlayerId(0), 0)]);
    }

    #[test]
    fn check_drawing_rejects_a_non_png_data_url() {
        assert_eq!(
            check_drawing("data:image/svg+xml,<script>"),
            Err(GameError::DrawingMustBeAPngDataUrl)
        );
        assert!(check_drawing("data:image/png;base64,abc123").is_ok());
        assert!(check_drawing("").is_ok());
    }

    #[test]
    fn entry_kind_matches_its_own_variant() {
        assert_eq!(joke("x").kind(), CreativityKind::Joke);
        assert_eq!(
            CreativeEntry::Drawing("".into()).kind(),
            CreativityKind::Drawing
        );
    }
}
