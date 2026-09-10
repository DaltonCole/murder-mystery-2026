//! The info-check family (rules.md §3.1-3.3: Oracle, Almanac, Spymaster,
//! the Cult Leader's query) and the Deceiver's falsify pipeline that can
//! target any of them, per the implementation plan's "Core Domain Model"
//! section: "info-checks funnel through one falsify pipeline."
//!
//! `resolve_info_check` is the single place that computes a query's true
//! answer and decides whether the Deceiver flips it -- every info-check
//! command in `state.rs` calls this instead of duplicating the
//! true-answer/falsify logic per ability.

use crate::character::Character;
use crate::player::{Faction, PlayerId};
use crate::round::Round;
use serde::{Deserialize, Serialize};

/// Which info-check is being performed. `IsTheLeader` is shared by the
/// Cult Leader's own query and the Uprising's round-outcome-reward intel
/// query (rules.md §3.2) -- the latter isn't wired to a live trigger until
/// Phase 3's round-outcome-reward system exists, but the falsify pipeline
/// is already correct for it since it's the same query shape.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum InfoQueryKind {
    /// Oracle: the target's full dossier.
    FullHistory,
    /// Spymaster: the target's apparent faction only (never reveals
    /// conversion -- that's the whole point of it being the *weaker*
    /// check).
    FactionColorOnly,
    /// Cult Leader's query (and, later, the Uprising's intel query):
    /// "is this person the Revolutionary Leader?"
    IsTheLeader,
    /// Cult Leader's query: "is this person Ton-aligned?" -- checks
    /// current effective alignment (a converted target is no longer
    /// Ton-aligned), not just their apparent faction.
    IsTonAligned,
    /// Almanac: "3 players who are definitely not the Revolutionary
    /// Leader" -- has no single `target` the way the other kinds do (see
    /// `state::use_almanac`'s doc comment for why this one deliberately
    /// skips the Deceiver falsify pipeline rather than guessing at rules
    /// text that doesn't actually say what "targeted by Almanac" means for
    /// a check with no single target).
    NotLeaderSet,
}

/// The answer shape for each `InfoQueryKind` -- deliberately one enum
/// covering every kind rather than a generic `bool`/`String`, so a
/// falsified `Bool(true)` can't accidentally get compared against a
/// genuine `Dossier` from a different query kind.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum InfoCheckAnswer {
    Dossier(Dossier),
    Faction(Faction),
    Bool(bool),
    /// Almanac's 3 confirmed non-Leaders. Never actually passed through
    /// `falsified()` in practice -- see `InfoQueryKind::NotLeaderSet`.
    PlayerSet(Vec<PlayerId>),
}

/// Oracle's "full history" result -- a snapshot of everything true about
/// the target *as of this query*, not a live-updating link (rules.md:
/// "locked at that moment").
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Dossier {
    pub apparent_faction: Faction,
    pub converted: bool,
    pub character: Option<Character>,
}

impl InfoCheckAnswer {
    /// Flips a `Bool`, and for the richer answer shapes substitutes a
    /// plausible-looking but wrong value -- "force that check to return a
    /// false result" (rules.md §3.3) means the *delivered* answer is
    /// wrong, not that it becomes visibly garbage.
    fn falsified(&self) -> InfoCheckAnswer {
        match self {
            InfoCheckAnswer::Bool(b) => InfoCheckAnswer::Bool(!b),
            InfoCheckAnswer::Faction(f) => {
                // Any faction other than the true one reads as "false" for
                // a color-only check -- Ton is an arbitrary but fixed
                // stand-in when the true answer already was Ton.
                InfoCheckAnswer::Faction(if *f == Faction::Ton {
                    Faction::Uprising
                } else {
                    Faction::Ton
                })
            }
            InfoCheckAnswer::Dossier(d) => InfoCheckAnswer::Dossier(Dossier {
                // Flipping `converted` is the one lie that actually matters
                // for a dossier (it's the one fact an Oracle-user is
                // hunting for) -- faction/character are left alone since
                // rules.md never suggests those get faked too, and
                // fabricating a whole fake character would need to invent
                // data this engine has no basis for.
                //
                // Known residual limitation: this can only ever falsify a
                // check against the Deceiver themself (see
                // `state::deliver_info_check`), and the Deceiver's own
                // `character` is fixed. If it's ever `Cultist` specifically
                // -- only reachable via `convert`'s defensive `None`
                // fallback, not the normal recruited-and-later-designated
                // path -- flipping `converted` to `false` alongside
                // `character: Cultist` would contradict this engine's own
                // `Player::is_consistent` invariant, a theoretical tell not
                // currently worth a deeper fix given how narrow the path to
                // it is.
                converted: !d.converted,
                ..d.clone()
            }),
            // Never actually reached in practice -- `state::use_almanac`
            // deliberately never calls `resolve_info_check` with
            // `should_falsify: true` for this variant (see
            // `InfoQueryKind::NotLeaderSet`'s doc comment). Implemented
            // anyway so this match stays exhaustive without a panic
            // branch: an identity transform is at least never *wrong*
            // (every name in the set genuinely is a non-Leader), even
            // though it doesn't achieve "force a false result" the way
            // the other variants do.
            InfoCheckAnswer::PlayerSet(set) => InfoCheckAnswer::PlayerSet(set.clone()),
        }
    }
}

/// Per-player snapshot of "what can I do right now," derived from
/// `GameState::ability_status_for` -- every field is `None` unless it
/// applies to the viewer's own current character, so a client can render
/// "my abilities" purely by checking which fields are `Some`, without
/// needing to know the whole character roster itself.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AbilityStatus {
    /// Oracle only. Already accounts for `oracle_disabled` -- `Some(0)`
    /// either way, so a client can't tell "disabled" apart from "no checks
    /// banked yet" from this field alone, which matches rules.md: a
    /// disabled Oracle just permanently has nothing to spend.
    pub oracle_checks_available: Option<usize>,
    pub almanac_available: Option<bool>,
    pub spymaster_available: Option<bool>,
    pub cult_leader_queries_available: Option<usize>,
    /// Cult Leader only. Without this, the Cult Leader has no way to know
    /// whether a recruitment window is currently open at all -- `Convert`
    /// would otherwise be pure trial and error, unlike every other
    /// ability's own `_available` field.
    pub recruitment_slots_available: Option<usize>,
    pub deceiver_armed: Option<bool>,
    pub deceiver_falsify_used: Option<bool>,
    pub priest_protects_available: Option<usize>,
    pub medic_available: Option<bool>,
    pub bartender_available: Option<bool>,
    pub potion_maker_available: Option<bool>,
    /// Magistrate or Firebrand -- whichever the viewer actually is.
    pub double_vote_available: Option<bool>,
    pub vote_shield_available: Option<bool>,
    pub duelist_available: Option<bool>,
    pub agitator_available: Option<bool>,
    pub grand_inquisitor_available: Option<bool>,
}

/// One delivered info-check result, kept so `view_for` can show a querier
/// their own past results without needing to re-derive them from the raw
/// event log (which stores the same fact, but isn't privacy-filtered
/// itself -- see `state.rs`'s `info_check_results` field and
/// `view::view_for`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InfoCheckDelivery {
    pub querier: PlayerId,
    /// `None` only for the Almanac (rules.md gives it no single target) --
    /// every other info-check names a real player here.
    pub target: Option<PlayerId>,
    pub kind: InfoQueryKind,
    /// What the querier actually received -- already falsified if the
    /// Deceiver intervened. This engine never exposes the *true* answer
    /// separately from what was delivered; a falsified check is
    /// indistinguishable from a genuine one to the querier, by design.
    pub answer: InfoCheckAnswer,
    pub round: Round,
}

/// Computes `kind`'s true answer about `target`, then decides whether to
/// falsify it. `is_deceiver_and_armed_and_unused` is supplied by the
/// caller (`state.rs`) rather than looked up here, since "is this player
/// currently able to falsify" depends on `GameState` fields this pure
/// function has no access to by design -- keeping the falsify *decision*
/// (a `GameState` mutation: consuming the Deceiver's one use) in the
/// caller, and the falsify *transformation* (turning a true answer into a
/// false-looking one) here, where it's shared by every query kind.
pub fn resolve_info_check(true_answer: InfoCheckAnswer, should_falsify: bool) -> InfoCheckAnswer {
    if should_falsify {
        true_answer.falsified()
    } else {
        true_answer
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bool_answers_flip_cleanly() {
        assert_eq!(
            resolve_info_check(InfoCheckAnswer::Bool(true), true),
            InfoCheckAnswer::Bool(false)
        );
        assert_eq!(
            resolve_info_check(InfoCheckAnswer::Bool(false), true),
            InfoCheckAnswer::Bool(true)
        );
    }

    #[test]
    fn an_unfalsified_answer_passes_through_unchanged() {
        let truth = InfoCheckAnswer::Bool(true);
        assert_eq!(resolve_info_check(truth.clone(), false), truth);
    }

    #[test]
    fn faction_answers_flip_to_a_different_faction() {
        let truth = InfoCheckAnswer::Faction(Faction::Ton);
        let lie = resolve_info_check(truth.clone(), true);
        assert_ne!(lie, truth);
        assert_eq!(lie, InfoCheckAnswer::Faction(Faction::Uprising));

        let truth2 = InfoCheckAnswer::Faction(Faction::Uprising);
        let lie2 = resolve_info_check(truth2.clone(), true);
        assert_ne!(lie2, truth2);
    }

    #[test]
    fn dossier_falsification_flips_only_the_converted_flag() {
        let truth = InfoCheckAnswer::Dossier(Dossier {
            apparent_faction: Faction::Ton,
            converted: false,
            character: Some(Character::NormalTon),
        });
        let lie = resolve_info_check(truth.clone(), true);
        match lie {
            InfoCheckAnswer::Dossier(d) => {
                assert!(d.converted);
                assert_eq!(d.apparent_faction, Faction::Ton);
                assert_eq!(d.character, Some(Character::NormalTon));
            }
            other => panic!("expected a Dossier, got {other:?}"),
        }
    }
}
