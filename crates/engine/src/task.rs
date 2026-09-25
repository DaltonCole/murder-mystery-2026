use crate::player::PlayerId;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

/// A security/reliability review found `PushTask`'s `prompt` had no length
/// cap -- unlike `Bio`'s fields, a Host-only command any raw connection can
/// currently issue (see `main.rs`'s documented auth gap) could push a
/// multi-megabyte prompt, broadcast to every connected client for as long
/// as the task stays open. Generous enough for a real riddle; not a
/// rules.md quote.
pub const MAX_TASK_PROMPT_LEN: usize = 200;

/// Same review, for a location task's code (`PushTask`'s `expected_code`
/// and a player's own guess in `AttemptLocationTask`) -- physical codes are
/// short by nature, so this is already generous.
pub const MAX_LOCATION_CODE_LEN: usize = 64;

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
/// mask") -- it is never serialized into a Player or Display
/// [`crate::view::PlayerView`], only the Host's (see
/// `crate::view::TaskView::answer_qualifying_players`; Dalton's own
/// explicit instruction reversed the original "never serialize this at
/// all" guarantee for the Host specifically). Content authoring (turning a
/// bio field into this set) is a Phase 4 concern; the engine only owns the
/// submit/credit mechanic itself.
///
/// `expected_code` is the ground truth for a different mechanic entirely
/// (rules.md §4's medium/hard *location* tasks: "talk to someone at
/// [a stated location / a riddled-at location]," credited only once the
/// player enters the code physically placed there) -- `Some` for a
/// location task, `None` for an ordinary talk-to-someone one. Exactly one
/// of `qualifying_players`/`expected_code` is meaningful for a given task;
/// which command applies (`AttemptTask` vs `AttemptLocationTask`) is
/// decided by which one is set, not a separate tag, since a `TaskDef` is
/// never constructed with both in play. Same Host-only exception as
/// `qualifying_players` (`crate::view::TaskView::answer_code`) -- see
/// `AttemptLocationTask`'s doc comment for the separate, still-true
/// guarantee that the raw `LOCATION_TASKS` content never reaches the
/// client *bundle* at compile time, which this per-viewer wire scoping
/// doesn't change.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskDef {
    pub id: TaskId,
    pub prompt: String,
    pub tier: TaskTier,
    pub qualifying_players: BTreeSet<PlayerId>,
    pub expected_code: Option<String>,
}
