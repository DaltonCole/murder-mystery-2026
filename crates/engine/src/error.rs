use crate::character::Character;
use crate::contest::ContestCategory;
use crate::player::{Faction, PlayerId};
use crate::round::Round;
use crate::task::TaskId;
use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum GameError {
    #[error("no player with id {0:?}")]
    UnknownPlayer(PlayerId),

    #[error("player {0:?} already has an assigned faction")]
    AlreadyAssigned(PlayerId),

    #[error("player {player:?} has faction {actual:?}, which cannot hold {character:?}")]
    WrongFactionForCharacter {
        player: PlayerId,
        character: Character,
        actual: Faction,
    },

    #[error("{character:?} is already held by {holder:?}")]
    TitleAlreadyHeld {
        character: Character,
        holder: PlayerId,
    },

    #[error("player {0:?} is not currently active")]
    NotActive(PlayerId),

    #[error("player {0:?} is not the current Revolutionary Leader")]
    NotCurrentLeader(PlayerId),

    #[error("player {0:?} is not an eligible Revolutionary Leader successor")]
    IneligibleSuccessor(PlayerId),

    #[error("the King/Queen transfer ability has already been used")]
    KingQueenTransferAlreadyUsed,

    #[error("the King/Queen transfer ability can only be used before Round 5")]
    KingQueenTransferTooLate,

    #[error("player {0:?} is not an eligible King/Queen replacement")]
    IneligibleKingQueenReplacement(PlayerId),

    #[error("the game is already at the Finale")]
    AlreadyAtFinale,

    #[error("a Denouncement is already in progress")]
    DenouncementAlreadyOpen,

    #[error("no Denouncement is currently in progress")]
    NoDenouncementOpen,

    #[error("nomination is not currently open")]
    NominationNotOpen,

    #[error("discussion is not currently open")]
    DiscussionNotOpen,

    #[error("the ballot is not currently open")]
    BallotNotOpen,

    #[error("no runoff is currently in progress")]
    RunoffNotOpen,

    #[error("player {0:?} is not a valid ballot target right now")]
    InvalidBallotTarget(PlayerId),

    #[error("no task with id {0:?}")]
    UnknownTask(TaskId),

    #[error("task {0:?} is not currently open")]
    TaskNotOpen(TaskId),

    #[error("player {player:?} already attempted task {task:?}")]
    AlreadyAttemptedTask { player: PlayerId, task: TaskId },

    #[error("a task attempt cannot name the attempting player")]
    CannotNameSelfForTask,

    #[error("a task attempt must name 3 distinct players")]
    DuplicateNamedPlayerForTask,

    #[error(
        "player {player:?} already has character {existing:?}, cannot also assign {requested:?}"
    )]
    AlreadyHasCharacter {
        player: PlayerId,
        existing: Character,
        requested: Character,
    },

    #[error("player {0:?} is already converted")]
    AlreadyConverted(PlayerId),

    // --- Phase 2: shared ability errors, deliberately generic rather than
    // one bespoke pair per character (11+ named roles would otherwise mean
    // 20+ near-duplicate variants) ---
    #[error("player {player:?} is not the {required:?}")]
    NotCharacter {
        player: PlayerId,
        required: Character,
    },

    #[error("{character:?}'s ability isn't available right now (no uses left, or a precondition isn't met)")]
    AbilityNotAvailable { character: Character },

    #[error("that info-query kind isn't valid for this ability")]
    InvalidInfoQueryKind,

    #[error(
        "player {0:?} has already been protected by the Priest/Priestess and can never be again"
    )]
    AlreadyProtectedByPriest(PlayerId),

    #[error("player {0:?} was protected by the Doctor/Medic last round -- can't repeat the same target on consecutive rounds")]
    CannotProtectSameTargetConsecutively(PlayerId),

    #[error("that player is not currently in a Cast-Out-eligible Denouncement phase to be Medic-protected")]
    NoActiveBallotToProtectAgainst,

    #[error("no recruitment slot is currently available for the Cult Leader to spend")]
    NoRecruitmentSlotAvailable,

    #[error("player {0:?} is drunk this round and can't nominate or vote")]
    PlayerIsDrunk(PlayerId),

    /// Distinct from `AlreadyProtectedByPriest` -- that one blocks the
    /// Priest/Priestess from *re-choosing* a past target; this one blocks
    /// the Cult Leader from converting someone currently shielded (rules.md
    /// §3.1: "the Cult Leader can't target that person that round").
    #[error("player {0:?} is protected from conversion this round by the Priest/Priestess")]
    ProtectedFromConversionThisRound(PlayerId),

    #[error("{0:?} is not a contest round (only Round::Two and Round::Four have one)")]
    NotAContestRound(Round),

    #[error("{0:?} hasn't happened yet -- the game is still at an earlier round")]
    ContestRoundNotYetReached(Round),

    #[error("the {category:?} result for {round:?} was already recorded")]
    ContestResultAlreadyRecorded {
        round: Round,
        category: ContestCategory,
    },

    #[error("the Intermission lottery has already been drawn")]
    IntermissionAlreadyDrawn,

    #[error(
        "player {0:?} can't be an Intermission entrant (didn't opt in, or isn't currently active)"
    )]
    InvalidIntermissionEntrant(PlayerId),

    #[error("at most 5 Intermission entrants can be drawn, got {0}")]
    TooManyIntermissionEntrants(usize),

    #[error("duplicate Intermission entrant {0:?}")]
    DuplicateIntermissionEntrant(PlayerId),

    #[error(
        "player {0:?} isn't currently a Servant (must be Faction::Servant, or already Cast Out)"
    )]
    NotAServant(PlayerId),

    #[error("player {0:?} must be Cast Out to submit a Gallery prediction")]
    MustBeCastOutForGallery(PlayerId),

    #[error("Gallery predictions can only be submitted while the Last Denouncement is open")]
    GalleryPredictionWindowClosed,

    #[error("Gallery predictions have already been resolved")]
    GalleryAlreadyResolved,

    #[error("Gallery predictions can only be resolved once the Last Denouncement has closed")]
    GalleryResolutionTooEarly,

    #[error("bio field {field} is {len} characters, over rules.md's 32-character cap")]
    BioFieldTooLong { field: &'static str, len: usize },
}
