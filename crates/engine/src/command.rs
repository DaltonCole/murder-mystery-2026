use crate::character::Character;
use crate::denouncement::Ballot;
use crate::player::{Faction, PlayerId};
use crate::task::{TaskId, TaskTier};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

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

    /// Opens a new Denouncement (rules.md §5-6), entering its Nomination
    /// phase. Rejected if one is already in progress -- only one runs at a
    /// time.
    OpenDenouncement,

    /// Submits (or silently replaces) `voter`'s nomination during the
    /// Nomination phase. Both `voter` and `nominee` must currently be
    /// active players; Servants may nominate and be nominated like anyone
    /// else -- only the *execution-count scaling formula* excludes them
    /// (rules.md's own resolution of that ambiguity was scoped to the
    /// headcount formula specifically, not to participation).
    Nominate { voter: PlayerId, nominee: PlayerId },

    /// Ends nomination: computes who surfaces for discussion (top 3 by
    /// nomination count, with ties at the boundary all surfacing -- see
    /// `denouncement::surfaced_nominees`) and moves to the Discussion
    /// phase. Rejected outside the Nomination phase.
    CloseNomination,

    /// Ends the discussion window and opens the Ballot. Rejected outside
    /// the Discussion phase.
    OpenBallot,

    /// Casts (or silently replaces) `voter`'s ballot during the Ballot or
    /// Runoff phase. `Ballot::For` must name one of that phase's current
    /// candidates; rejected otherwise.
    CastBallot { voter: PlayerId, ballot: Ballot },

    /// Tallies the ballot against the headcount-scaled execution count
    /// (rules.md §5). A clean result resolves every Cast-Out player
    /// immediately (through the same cascades `Command::CastOut` uses) and
    /// closes the Denouncement. A tie for the last slot(s) instead opens a
    /// Runoff among just the tied candidates. Rejected outside the Ballot
    /// phase.
    CloseBallot {
        /// Consulted only for any King/Queen/Revolutionary-Leader
        /// replacement cascades a resulting Cast-Out triggers -- see
        /// `Command::CastOut`.
        fallback_replacement: Option<PlayerId>,
    },

    /// Tallies the runoff ballot. Resolves every already-locked-in
    /// candidate from the original ballot plus whatever the runoff itself
    /// resolves cleanly, then closes the Denouncement regardless of
    /// outcome -- a repeat tie leaves that specific slot unfilled per
    /// rules.md, rather than triggering a second runoff. Rejected outside
    /// the Runoff phase.
    CloseRunoff {
        fallback_replacement: Option<PlayerId>,
    },

    /// Opens one new task for attempts (rules.md §4: Round 1's 2 fixed
    /// tasks, or one of Rounds 3/5's live easy/medium/hard tiers).
    /// `qualifying_players` is the ground truth for who satisfies the
    /// prompt -- never sent to any client, only consulted inside
    /// `attempt_task`. Assigns the next `TaskId` in creation order; the
    /// caller does not choose the ID.
    PushTask {
        prompt: String,
        tier: TaskTier,
        qualifying_players: BTreeSet<PlayerId>,
    },

    /// Locks every currently-open task against further attempts (rules.md
    /// §4 step 2: "the app locks submissions"). A no-op (empty
    /// `closed` list) if nothing is open.
    CloseTasks,

    /// `player`'s claim that they talked to each of `named` -- rules.md
    /// §4's "talk to 3, credit on 1 match" mechanic: `player` gets credit
    /// for the task if *any* of `named` is actually in the task's
    /// qualifying set, but the event this produces never reveals which one
    /// (if any) it was. `named` must be 3 distinct active players, none of
    /// them `player` themself. One attempt per player per task -- a second
    /// attempt at the same task is rejected, not silently replaced (unlike
    /// `Nominate`/`CastBallot`, this isn't a standing choice that should
    /// change if the player has second thoughts).
    AttemptTask {
        player: PlayerId,
        task: TaskId,
        named: [PlayerId; 3],
    },
}
