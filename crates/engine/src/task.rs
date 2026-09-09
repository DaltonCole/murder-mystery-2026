use crate::player::PlayerId;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

/// Stable identifier for a task, assigned in creation order across the
/// whole game (not per-round) -- see `PlayerId` for the same rationale.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct TaskId(pub u32);

/// Difficulty tier (rules.md §4: "Task phase, easy/medium/hard tiers
/// live" for Rounds 3 & 5; Round 1 pushes exactly one Easy and one
/// Medium). Ordered easy-to-hard so a future host UI can sort tasks by
/// tier without a lookup table.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum TaskTier {
    Easy,
    Medium,
    Hard,
}

/// A single pushed task. `qualifying_players` is the *ground truth* set of
/// players who actually satisfy the task's prompt (e.g. "wearing a red
/// mask") -- it is never serialized into any [`crate::view::PlayerView`].
/// Content authoring (turning a bio field into this set) is a Phase 4
/// concern; the engine only owns the submit/credit mechanic itself.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskDef {
    pub id: TaskId,
    pub prompt: String,
    pub tier: TaskTier,
    pub qualifying_players: BTreeSet<PlayerId>,
}
