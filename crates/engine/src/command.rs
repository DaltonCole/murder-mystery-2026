use crate::player::{Faction, PlayerId};
use serde::{Deserialize, Serialize};

/// The write-side API of the engine. Every state mutation in the whole
/// application — from setup through the finale — is expressed as one of
/// these and goes through [`crate::apply_command`]. Nothing outside this
/// crate ever mutates [`crate::GameState`] directly.
///
/// This is intentionally a small, growing enum rather than many small
/// mutator methods on `GameState`: it gives every mutation a name that
/// shows up in the event log, in test assertions, and (later) on the wire
/// as the client -> server message shape, for free.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Command {
    /// Registers a new player during setup. Assigns the next `PlayerId` in
    /// join order; the caller does not choose the ID.
    AddPlayer { name: String },

    /// Assigns a player's faction. Setup-only for now — Phase 1 will route
    /// this through the raffle-ticket weighting in rules.md §2 rather than
    /// taking the faction directly, but the underlying mutation (one player,
    /// one faction, once) stays the same.
    AssignFaction { player: PlayerId, faction: Faction },
}
