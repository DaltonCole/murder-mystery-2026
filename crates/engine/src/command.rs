use crate::character::Character;
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

    /// Setup-only: assigns one of the four named titles (King/Queen,
    /// Prince/Princess, Revolutionary Leader, Cult Leader) to a player.
    /// Rejects a player whose faction doesn't match the title, and rejects
    /// assigning a title that's already held by someone else. Everyone else
    /// gets a catch-all character from [`Command::FinalizeSetup`], not this.
    AssignCharacter {
        player: PlayerId,
        character: Character,
    },

    /// Setup-only: fills in the catch-all character (`NormalTon`,
    /// `NormalUprising`, or `Cultist`) for every Ton/Uprising/Cult player
    /// who doesn't already have one from `AssignCharacter`. Servants and
    /// still-`Unassigned` players are left alone. Idempotent -- safe to
    /// call again after adding more players.
    FinalizeSetup,

    /// The Cult Leader converts a player (rules.md §3.3/§4.3) -- growing
    /// the Cult's ranks if `target` isn't currently titled, or secretly
    /// flipping the King/Queen or Revolutionary Leader if they are. See
    /// `state::convert` for the full King/Queen-conversion cascade this can
    /// trigger.
    Convert {
        converter: PlayerId,
        target: PlayerId,
    },

    /// The Uprising Leader stands ready to designate who inherits the title
    /// if they're Cast Out -- a standing choice, changeable at any time
    /// (rules.md §3.2). `successor` must currently be an active Uprising
    /// player.
    DesignateSuccessor {
        leader: PlayerId,
        successor: PlayerId,
    },

    /// The King/Queen's once-per-game, self-triggered escape hatch
    /// (rules.md §3.1) -- voluntarily hands the title to `new_holder`
    /// before Round 5. Rejected once already used, rejected from Round 5
    /// onward.
    TransferKingQueen { new_holder: PlayerId },

    /// The Denouncement's outcome for one player: removes them from active
    /// play and, depending on which title (if any) they held, triggers the
    /// cascades described in rules.md §5 -- Oracle disabling and the
    /// Round-3 Prince/Princess cascade for the King/Queen, succession for
    /// the Revolutionary Leader, or the martyrdom check for the Cult
    /// Leader. See `state::resolve_cast_out` for the full dispatch.
    CastOut {
        player: PlayerId,
        /// Consulted only if this Cast-Out requires picking a new
        /// King/Queen, or a Revolutionary Leader successor when no
        /// pre-designated one is set (or the pre-designated one is no
        /// longer eligible). Picking "randomly" is an I/O-adjacent concern
        /// this pure engine deliberately doesn't own -- see the
        /// implementation plan's "Core Domain Model" section on keeping
        /// randomness at the boundary. Ignored, and fine to leave `None`,
        /// when no replacement is needed.
        fallback_replacement: Option<PlayerId>,
    },

    /// Moves the game to the next round in sequence (rules.md §4). Rejected
    /// once already at the Finale.
    AdvanceRound,
}
