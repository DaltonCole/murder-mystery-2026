use crate::ability::InfoQueryKind;
use crate::character::Character;
use crate::contest::ContestCategory;
use crate::denouncement::Ballot;
use crate::player::{Faction, PlayerId};
use crate::round::Round;
use crate::servant::GalleryPrediction;
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

    /// Assigns a named character to a player -- the four major titles
    /// (King/Queen, Prince/Princess, Revolutionary Leader, Cult Leader)
    /// during setup, but also every Phase 2 named role (Oracle, Deceiver,
    /// ...), including *mid-game*: rules.md §3.3 has the Cult Leader
    /// designate which recruited Cultist holds the Deceiver title "at the
    /// moment of recruitment or any point after," so this command must
    /// stay usable after setup too, not just during it. Rejects a player
    /// whose faction doesn't match the character (checked against
    /// `true_faction()` for a Cult-required character, so a secretly
    /// recruited Cultist qualifies even though their apparent faction
    /// never changes), and rejects a character that's already held by
    /// someone else. A generic catch-all (`NormalTon`/`NormalUprising`/
    /// `Cultist`) already assigned to the player -- whether by
    /// [`Command::FinalizeSetup`] or by `Convert`'s own auto-stamp -- can
    /// always be upgraded to a specific named role; any other existing
    /// character is a hard rejection. Everyone who ends setup with no
    /// character at all gets a catch-all from
    /// [`Command::FinalizeSetup`], not this.
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

    // --- Phase 2: info-check family + the Deceiver's falsify pipeline
    // (rules.md §3.1-3.3) ---
    /// Oracle: views `target`'s full history, snapshotted now. `player`
    /// must currently be the Oracle, with a check available (after every
    /// odd round) and the Oracle not permanently disabled (rules.md §5:
    /// tripped if the King/Queen is Cast Out unconverted).
    UseOracle { player: PlayerId, target: PlayerId },

    /// Almanac: learns 3 players who are definitely not the Revolutionary
    /// Leader. Once per game; no target -- the 3 names are chosen
    /// deterministically by the engine (see `state::pick_non_leaders`),
    /// not something the caller picks.
    UseAlmanac { player: PlayerId },

    /// Spymaster: views `target`'s apparent faction only (never reveals
    /// conversion). Once per game.
    UseSpymaster { player: PlayerId, target: PlayerId },

    /// The Cult Leader's own query (rules.md §3.3): "is this person
    /// Ton-aligned?" or "is this person the Revolutionary Leader?" --
    /// `kind` must be `IsTonAligned` or `IsTheLeader`; anything else is
    /// rejected. One use per open recruitment window (does not itself
    /// consume a recruitment slot).
    CultLeaderQuery {
        player: PlayerId,
        target: PlayerId,
        kind: InfoQueryKind,
    },

    /// The Deceiver arms or disarms their once-per-game falsify power
    /// (rules.md §3.3) -- a standing choice, changeable at any time, the
    /// same shape as `DesignateSuccessor`. See `GameState::deceiver_armed`'s
    /// doc comment for why this replaces a real-time "falsify now?" prompt.
    SetDeceiverArmed { player: PlayerId, armed: bool },

    // --- Phase 2: protect family (rules.md §3.1/§3.2) ---
    /// Priest/Priestess: protects `target` from conversion this
    /// recruitment window (rules.md §3.1). Rejected if `target` has ever
    /// been protected by this ability before, even in an earlier window.
    PriestProtect { player: PlayerId, target: PlayerId },

    /// Doctor/Medic: shields `target` from this round's Cast-Out
    /// resolution (rules.md §3.2) -- if `target` would otherwise be
    /// Cast Out, their name is removed from the resolved list before slots
    /// are filled, letting the next-highest vote-getter backfill the freed
    /// slot (Dalton's resolution of that ambiguity during Phase 2
    /// planning). Rejected if `target` was also protected last round
    /// (can't repeat a target on consecutive rounds).
    MedicProtect { player: PlayerId, target: PlayerId },

    /// Bartender targets `target` with a 50% chance of making them drunk
    /// this round (rules.md §3.2) -- `lands` is the pre-rolled outcome,
    /// supplied by the caller rather than rolled inside the engine (see
    /// the plan's "keep randomness at the boundary" principle, the same
    /// reasoning behind `CastOut`'s caller-supplied `fallback_replacement`).
    /// A drunk player can't `Nominate` or `CastBallot` for the rest of the
    /// round (Dalton's resolution of what "drunk" mechanically restricts).
    /// Once per round.
    BartenderTarget {
        player: PlayerId,
        target: PlayerId,
        lands: bool,
    },

    /// Potion Maker activates round-wide execution-immunity (rules.md
    /// §3.1) -- whoever the vote selects this Denouncement survives
    /// instead of being Cast Out (Dalton's resolution of the
    /// named-target-vs-blanket ambiguity: this is blanket, no target
    /// choice). Once per game; must be armed before the ballot/runoff
    /// that it protects actually closes.
    ActivatePotionImmunity { player: PlayerId },

    // --- Phase 2: vote-weight pair (rules.md §3.1/§3.2) ---
    /// The Magistrate or the Firebrand arms their once-per-game double
    /// vote (rules.md §5: "adds one extra vote to whichever single nominee
    /// that player supported") for the ballot/runoff they're about to
    /// vote in. `player`'s own character determines which of the two this
    /// is -- there's only ever one of each.
    ActivateDoubleVote { player: PlayerId },

    // --- Phase 3: Denouncement procedural modifiers (rules.md §3.1/§3.2)
    // ---
    /// The Duelist "challenges" `target`, once per game, guaranteeing them
    /// a spot on the ballot regardless of verbal support (rules.md §3.1).
    /// `target` is added on top of whoever naturally surfaced from
    /// nominations, not swapped in for them (Dalton's resolution of that
    /// ambiguity during Phase 3 planning). Must be issued while a
    /// Denouncement's Nomination phase is currently open -- see
    /// `state::duelist_challenge`.
    DuelistChallenge { player: PlayerId, target: PlayerId },

    /// The Agitator "redirects" the room's attention to `target`, once per
    /// game, during Discussion (rules.md §3.2: "the mirror to the
    /// Duelist"). Per Dalton's resolution during Phase 3 planning, this is
    /// mechanically identical to the Duelist's effect -- `target` is added
    /// to the candidate list too, not merely discussed -- just triggered
    /// from the Discussion phase instead of pre-Nomination-close. Must be
    /// issued while a Denouncement's Discussion phase is currently open --
    /// see `state::agitator_redirect`.
    AgitatorRedirect { player: PlayerId, target: PlayerId },

    /// The Grand Inquisitor invokes their office, once per game, before a
    /// ballot/runoff closes: forces exactly 2 Cast-Outs (the top two
    /// vote-getters) regardless of the standard headcount-scaled execution
    /// count (rules.md §5: "a one-time override of a single Denouncement's
    /// outcome"). Applies to whichever tally is open when armed -- the
    /// original ballot, or a runoff if one is already underway -- the same
    /// "arm before it closes" convention as Potion Maker/the double
    /// vote/the vote-shield.
    ActivateGrandInquisitor { player: PlayerId },

    // --- Phase 2: Normal Uprising's reactive safety-net (rules.md §3.2)
    // ---
    /// Arms a standing shield negating one vote cast against `player` at
    /// the current Denouncement (declared proactively, before the ballot
    /// closes -- Dalton's resolution of that ambiguity during the original
    /// implementation planning). Once per game.
    ArmVoteShield { player: PlayerId },

    // --- Phase 3: contest rounds + the Leader's Confidants (rules.md
    // §3.2/§4) ---
    /// Records one contest category's outcome for Round 2 or Round 4 --
    /// however that category was actually run (a live judged activity, a
    /// self-reported physical challenge, a digital mini-game); this engine
    /// only owns the recorded result and what it triggers, not the
    /// activity itself (Dalton's own scoping during Phase 3 planning: the
    /// actual mini-games are still being designed). No actor -- this is an
    /// objective fact the host records, not a player ability, the same
    /// shape as `PushTask`/`CastOut`. `round` must be `Two` or `Four`;
    /// rejected if this exact (round, category) pair was already recorded.
    /// Deliberately never exposed back through `view_for` to any viewer --
    /// Dalton's explicit instruction: players learn nothing about the
    /// running standings, and not even the breakdown once the round ends.
    RecordContestResult {
        round: Round,
        category: ContestCategory,
        ton_won: bool,
    },

    // --- Phase 3: the Intermission lottery (rules.md §4) ---
    /// A player opts into "Who is Lorel's number one love?" -- rejected if
    /// they're already Cast Out ("anyone Cast Out earlier is ineligible to
    /// enter"). Idempotent: opting in twice is a harmless no-op.
    OptIntoIntermission { player: PlayerId },

    /// Draws the Intermission's entrants from the opted-in pool.
    /// `selected` is the caller-supplied random draw (up to 5 -- see the
    /// plan's "keep randomness at the boundary" principle, the same
    /// reasoning behind `CastOut`'s `fallback_replacement`); every name in
    /// it must have actually opted in and still be active. Once per game.
    DrawIntermissionEntrants { selected: Vec<PlayerId> },

    // --- Phase 3: Servant leaderboard + Gallery (rules.md §7) ---
    /// Awards Servant leaderboard points to `player` -- host-recorded, the
    /// same "objective fact the host records" shape as
    /// `RecordContestResult`. Eligible only for someone currently
    /// operating as a Servant: either literally `Faction::Servant`, or any
    /// already-Cast-Out player (rules.md §5: "operationally, for the rest
    /// of the game they participate alongside the Servants"). What
    /// specifically earns points (zone scorekeeping, trivia, a minigame)
    /// isn't specified by rules.md and isn't this engine's concern -- it
    /// just tracks the running total.
    AwardServantPoints { player: PlayerId, points: u32 },

    /// A Cast-Out player's private Gallery prediction (rules.md §7: "who
    /// gets Cast Out, or which faction ultimately wins"), submitted while
    /// the Last Denouncement (the Finale) has an open Denouncement --
    /// "before the Last Denouncement's ballot closes." Rejected for anyone
    /// not currently Cast Out -- a late-arrival Servant doesn't get a
    /// Gallery prediction, only someone who was actually voted out.
    SubmitGalleryPrediction {
        player: PlayerId,
        prediction: GalleryPrediction,
    },

    /// Scores every submitted Gallery prediction against the actual
    /// finale outcome, awarding one Servant leaderboard point per correct
    /// guess (rules.md §7: "scored against the Servant leaderboard" --
    /// the point value itself isn't specified, so this engine uses a
    /// simple flat award). `actual_cast_out` is whoever the Last
    /// Denouncement actually resolved; a `CastOutIs` prediction is correct
    /// if its name is anywhere in that set (a multi-slot Finale can Cast
    /// Out more than one person). Once per game.
    ResolveGalleryPredictions {
        actual_cast_out: Vec<PlayerId>,
        actual_winner: Faction,
    },
}
