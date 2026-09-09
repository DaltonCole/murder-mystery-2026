use crate::character::Character;
use crate::player::{Faction, PlayerId};
use crate::round::Round;
use crate::win_condition::CultPath;
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
}
