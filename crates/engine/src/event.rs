use crate::ability::{InfoCheckAnswer, InfoQueryKind};
use crate::character::Character;
use crate::contest::ContestCategory;
use crate::denouncement::Ballot;
use crate::player::{Faction, PlayerId};
use crate::round::Round;
use crate::task::{TaskId, TaskTier};
use crate::win_condition::CultPath;
use serde::{Deserialize, Serialize};

// `GalleryPrediction` is deliberately NOT imported/used here -- the actual
// prediction content is private and never enters the event log, only
// `state::gallery_predictions`. See `GalleryPredictionSubmitted` below.

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
    PlayerAdded {
        id: PlayerId,
        name: String,
    },
    FactionAssigned {
        player: PlayerId,
        faction: Faction,
    },
    CharacterAssigned {
        player: PlayerId,
        character: Character,
    },
    SetupFinalized,
    Converted {
        converter: PlayerId,
        target: PlayerId,
    },
    /// Emitted alongside `Converted` specifically when converting the
    /// current King/Queen triggers the auto-transfer + Prince/Princess
    /// cascade from rules.md §4.3 -- kept as a distinct event (rather than
    /// folded into `Converted`) so a Whistledown-style narrator or a test
    /// assertion can tell "a plain conversion happened" apart from
    /// "the monarchy's whole line just turned over," without having to
    /// re-derive it from state.
    KingQueenConversionCascade {
        old_king_queen: PlayerId,
        new_king_queen: Option<PlayerId>,
        prince_princess_converted: Option<PlayerId>,
    },
    SuccessorDesignated {
        leader: PlayerId,
        successor: PlayerId,
    },
    KingQueenTransferred {
        old_holder: PlayerId,
        new_holder: PlayerId,
    },
    PlayerCastOut {
        player: PlayerId,
    },
    /// Emitted alongside `PlayerCastOut` specifically for the Round-3
    /// King/Queen cascade (rules.md §5) -- see the doc comment on
    /// `KingQueenConversionCascade` for why this is a distinct event
    /// rather than folded in.
    KingQueenCastOutCascade {
        old_king_queen: PlayerId,
        prince_princess_cast_out: Option<PlayerId>,
        new_king_queen: Option<PlayerId>,
    },
    RevolutionaryLeaderSucceeded {
        old_leader: PlayerId,
        new_leader: Option<PlayerId>,
    },
    OracleDisabled,
    MartyrdomTriggered {
        cult_leader: PlayerId,
    },
    RoundAdvanced {
        round: Round,
    },
    /// Not itself a state mutation -- `apply_command` never emits this.
    /// Reserved for `app`/`sim` callers who want to log a win-condition
    /// check (e.g. after every Denouncement resolution) through the same
    /// event-log mechanism everything else uses, rather than inventing a
    /// second logging path. See `win_condition::evaluate`.
    WinConditionChecked {
        ton_wins: bool,
        uprising_wins: bool,
        cult_wins: bool,
        cult_paths: Vec<CultPath>,
    },

    DenouncementOpened,
    NominationCast {
        voter: PlayerId,
        nominee: PlayerId,
    },
    /// The `surfaced` list can hold more than 3 names -- see the doc
    /// comment on `denouncement::surfaced_nominees` for why.
    NominationClosed {
        surfaced: Vec<PlayerId>,
    },
    BallotOpened {
        candidates: Vec<PlayerId>,
    },
    BallotCast {
        voter: PlayerId,
        ballot: Ballot,
    },
    /// The Denouncement closed with no runoff needed -- `cast_out` lists
    /// everyone Denounced this round (each also carries its own
    /// `PlayerCastOut`/cascade events, appended separately).
    BallotClosed {
        cast_out: Vec<PlayerId>,
    },
    RunoffOpened {
        candidates: Vec<PlayerId>,
        slots_remaining: usize,
    },
    /// The Denouncement closed after a runoff. `cast_out` covers everyone
    /// Denounced this round, including anyone already locked in before the
    /// runoff started. `unfilled_slot` is `true` if the runoff itself tied
    /// again and rules.md's "no one is Denounced for that slot" applied.
    RunoffClosed {
        cast_out: Vec<PlayerId>,
        unfilled_slot: bool,
    },

    /// Deliberately omits `qualifying_players` -- that set is the ground
    /// truth `attempt_task` checks against and must never appear anywhere
    /// a client-facing consumer of the event log (e.g. a future Whistledown
    /// generator) could read it back out.
    TaskPushed {
        id: TaskId,
        prompt: String,
        tier: TaskTier,
    },
    TasksClosed {
        closed: Vec<TaskId>,
    },
    /// Never carries `named` -- the whole point of rules.md's "talk to 3,
    /// credit on 1 match, don't learn which" mechanic is that neither the
    /// player nor anyone reading the log afterward learns which (if any) of
    /// their 3 claims actually matched, only whether they were credited.
    TaskAttempted {
        player: PlayerId,
        task: TaskId,
        credited: bool,
    },

    /// Emitted alongside `RoundAdvanced` -- every round advance opens a
    /// new Cult recruitment window (rules.md §3.3), which also grants the
    /// Cult Leader's query, the Priest/Priestess's protect, and (on the
    /// rounds following an odd one) the Oracle's check. `slots` is how
    /// many recruitment slots this specific window granted (1, or 2 once
    /// ramped -- see `recruitment::recruitment_window_size`).
    RecruitmentWindowOpened {
        round: Round,
        slots: usize,
    },

    // --- Phase 2: info-check family + the Deceiver ---
    /// Carries the *delivered* answer -- already falsified if the Deceiver
    /// intervened. This engine never separately logs the true answer next
    /// to a falsified one; from the moment of delivery onward, a lie and
    /// the truth are the same shape (see `ability::InfoCheckAnswer`).
    InfoCheckDelivered {
        querier: PlayerId,
        /// `None` only for the Almanac -- see `ability::InfoCheckDelivery`.
        target: Option<PlayerId>,
        kind: InfoQueryKind,
        answer: InfoCheckAnswer,
    },
    DeceiverArmedChanged {
        player: PlayerId,
        armed: bool,
    },
    /// Emitted alongside `InfoCheckDelivered` specifically when the
    /// Deceiver's standing arm fired -- lets a test (or a future
    /// Whistledown-style narrator) tell "a genuine check" apart from "a
    /// falsified one" without inspecting the answer's plausibility, same
    /// reasoning as `KingQueenConversionCascade` being its own event.
    CheckFalsifiedByDeceiver {
        deceiver: PlayerId,
    },

    // --- Phase 2: protect family ---
    PriestProtected {
        player: PlayerId,
        target: PlayerId,
    },
    MedicProtected {
        player: PlayerId,
        target: PlayerId,
    },
    /// `landed` is the same pre-rolled outcome the caller supplied --
    /// logged so a Whistledown-style narrator or audit trail doesn't need
    /// to re-derive it from `Player::drunk`-style state this engine
    /// deliberately doesn't keep as a queryable per-player flag (see
    /// `GameState::drunk_this_round`).
    BartenderTargeted {
        player: PlayerId,
        target: PlayerId,
        landed: bool,
    },
    PotionImmunityActivated {
        player: PlayerId,
    },

    // --- Phase 2: vote-weight pair ---
    DoubleVoteActivated {
        player: PlayerId,
        character: Character,
    },

    // --- Phase 2: Normal Uprising's reactive safety-net ---
    VoteShieldArmed {
        player: PlayerId,
    },

    // --- Phase 3: Denouncement procedural modifiers ---
    DuelistChallengeIssued {
        player: PlayerId,
        target: PlayerId,
    },
    AgitatorRedirectIssued {
        player: PlayerId,
        target: PlayerId,
    },
    GrandInquisitorInvoked {
        player: PlayerId,
    },

    // --- Phase 3: contest rounds + the Leader's Confidants ---
    ContestResultRecorded {
        round: Round,
        category: ContestCategory,
        ton_won: bool,
    },
    /// A bidirectional identity reveal (rules.md §3.2): `confidant` learns
    /// who `leader` is, and `leader` learns `confidant`'s identity in
    /// return (though `leader` already knew who every active Uprising
    /// member was publicly -- what's new for them is specifically that
    /// this person now knows about *them*).
    LeaderConfidantRevealed {
        leader: PlayerId,
        confidant: PlayerId,
    },

    // --- Phase 3: the Intermission lottery ---
    IntermissionOptedIn {
        player: PlayerId,
    },
    IntermissionEntrantsDrawn {
        entrants: Vec<PlayerId>,
    },

    // --- Phase 3: Servant leaderboard + Gallery ---
    ServantPointsAwarded {
        player: PlayerId,
        points: u32,
        total: u32,
    },
    /// Deliberately never carries the prediction itself -- see the module
    /// doc note above `Command`'s Gallery variants.
    GalleryPredictionSubmitted {
        player: PlayerId,
    },
}
