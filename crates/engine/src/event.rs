use crate::player::{Faction, PlayerId};
use serde::{Deserialize, Serialize};

/// The read-side record of everything that has happened. `apply_command`
/// returns the events a command produced; `GameState` keeps an append-only
/// log of all of them.
///
/// This log is deliberately the single source powering several later
/// features rather than each having its own bespoke tracking: Whistledown's
/// text generation reads it, the finale's "reveal everything" walkthrough
/// reads it, and it doubles as an audit trail for host debugging. Keeping
/// one append-only log now, even while it only has two variants, avoids
/// having to retrofit that unification later.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DomainEvent {
    PlayerAdded { id: PlayerId, name: String },
    FactionAssigned { player: PlayerId, faction: Faction },
}
