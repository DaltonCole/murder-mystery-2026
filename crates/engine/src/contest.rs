use serde::{Deserialize, Serialize};

/// One of the three contest categories rules.md §4 names for Rounds 2 and 4
/// ("Strength, Creativity, and Intelligence"). The actual mini-games behind
/// each category (a drawing-game-style Creativity challenge, a physical
/// self-reported Strength challenge, etc.) are explicitly out of scope for
/// this engine pass -- Dalton's own words during Phase 3 planning: "the
/// activities are primarily app activities that need to be designed
/// later." What this engine owns is just the *result* of each category,
/// however it was actually run, and what that result triggers mechanically
/// (see `state::trigger_leader_confidant`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum ContestCategory {
    Strength,
    Creativity,
    Intelligence,
}
