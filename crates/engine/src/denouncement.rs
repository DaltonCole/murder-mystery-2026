use crate::player::PlayerId;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// A single vote during a Ballot or Runoff. Voting `For` someone who isn't
/// currently a valid candidate is rejected at the command level (see
/// `state::cast_ballot`) -- this type itself doesn't enforce that, since it
/// has no access to what's currently a valid target.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Ballot {
    For(PlayerId),
    Abstain,
}

/// The Denouncement's current phase (rules.md §5). One `GameState` holds at
/// most one of these at a time -- `Command::OpenDenouncement` is rejected if
/// one is already in progress. Voter -> choice maps (rather than a `Vec` of
/// votes) so re-nominating or re-voting before the phase closes naturally
/// replaces a player's earlier choice instead of adding a second one.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DenouncementPhase {
    Nomination {
        submitted: BTreeMap<PlayerId, PlayerId>,
    },
    Discussion {
        surfaced: Vec<PlayerId>,
    },
    Ballot {
        surfaced: Vec<PlayerId>,
        ballots: BTreeMap<PlayerId, Ballot>,
    },
    /// Entered only when the ballot ties for the last available slot(s).
    /// `already_locked_in` carries forward anyone the original ballot
    /// already resolved cleanly (e.g. one clear winner plus two tied for a
    /// second slot) -- they don't get re-voted on, and are still Cast Out
    /// once the runoff closes, even if the runoff itself ties again.
    Runoff {
        candidates: Vec<PlayerId>,
        slots_remaining: usize,
        already_locked_in: Vec<PlayerId>,
        ballots: BTreeMap<PlayerId, Ballot>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Denouncement {
    pub phase: DenouncementPhase,
}

/// How many players are Cast Out in one Denouncement round, per rules.md
/// §5's scaling formula: 1 at ≤20 competing players, 2 at 21-30, `ceil(n /
/// 15)` beyond that (this is what makes the 21-30 tier's "2" consistent
/// with the general formula: `ceil(30/15) == 2`). `n` only counts the three
/// competing factions (Ton/Uprising/Cult) -- Servants are excluded, per
/// Dalton's resolution of that ambiguity during planning.
pub fn execution_count(competing_players: usize) -> usize {
    if competing_players == 0 {
        return 0;
    }
    if competing_players <= 20 {
        1
    } else if competing_players <= 30 {
        2
    } else {
        competing_players.div_ceil(15)
    }
}

/// Which candidates surface for discussion out of everyone nominated, per
/// rules.md §5: normally the top 3 by nomination count, but "everyone tied
/// for the last spot surfaces" -- so this can return more than 3 if there's
/// a tie at the boundary. Never fewer than 3 unless fewer than 3 distinct
/// candidates were nominated at all. Deterministic tie-break within equal
/// counts (by `PlayerId`) only affects iteration order, never *whether*
/// someone surfaces -- ties always all surface together.
pub fn surfaced_nominees(tally: &BTreeMap<PlayerId, u32>, target_slots: usize) -> Vec<PlayerId> {
    let mut counts: Vec<(PlayerId, u32)> = tally.iter().map(|(&id, &c)| (id, c)).collect();
    counts.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));

    let mut surfaced = Vec::new();
    let mut i = 0;
    while i < counts.len() && surfaced.len() < target_slots {
        let current_count = counts[i].1;
        let group_start = i;
        while i < counts.len() && counts[i].1 == current_count {
            i += 1;
        }
        surfaced.extend(counts[group_start..i].iter().map(|(id, _)| *id));
    }
    surfaced
}

/// The result of tallying one ballot (or runoff ballot) against a slot
/// count: everyone in `locked_in` is unambiguously Cast Out; if
/// `tied_for_last_slot` is non-empty, those candidates are tied for the
/// remaining slot(s) and the caller must run a runoff among just them (or,
/// if this *was* the runoff, treat it as an unfillable slot per rules.md's
/// "a repeat tie means no one is Denounced for that slot").
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BallotResolution {
    pub locked_in: Vec<PlayerId>,
    pub tied_for_last_slot: Vec<PlayerId>,
}

/// Resolves a tally against `slots` available Cast-Out spots. Candidates
/// with zero votes are never eligible for a slot (an unfilled slot from a
/// lack of votes is not the same thing as a tie, and shouldn't force a
/// runoff) -- see the module tests for the distinction.
pub fn resolve_ballot(tally: &BTreeMap<PlayerId, u32>, slots: usize) -> BallotResolution {
    if slots == 0 {
        return BallotResolution {
            locked_in: Vec::new(),
            tied_for_last_slot: Vec::new(),
        };
    }

    let mut viable: Vec<(PlayerId, u32)> = tally
        .iter()
        .map(|(&id, &c)| (id, c))
        .filter(|&(_, c)| c > 0)
        .collect();
    viable.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));

    let mut locked_in = Vec::new();
    let mut i = 0;
    while i < viable.len() && locked_in.len() < slots {
        let current_count = viable[i].1;
        let group_start = i;
        while i < viable.len() && viable[i].1 == current_count {
            i += 1;
        }
        let group = &viable[group_start..i];
        let remaining_slots = slots - locked_in.len();
        if group.len() <= remaining_slots {
            locked_in.extend(group.iter().map(|(id, _)| *id));
        } else {
            return BallotResolution {
                locked_in,
                tied_for_last_slot: group.iter().map(|(id, _)| *id).collect(),
            };
        }
    }
    BallotResolution {
        locked_in,
        tied_for_last_slot: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tally(pairs: &[(u32, u32)]) -> BTreeMap<PlayerId, u32> {
        pairs.iter().map(|&(id, c)| (PlayerId(id), c)).collect()
    }

    // --- execution_count ---

    #[test]
    fn execution_count_matches_every_boundary_in_the_formula() {
        assert_eq!(execution_count(0), 0);
        assert_eq!(execution_count(1), 1);
        assert_eq!(execution_count(20), 1);
        assert_eq!(execution_count(21), 2);
        assert_eq!(execution_count(30), 2);
        assert_eq!(execution_count(31), 3);
        assert_eq!(execution_count(45), 3);
        assert_eq!(execution_count(46), 4);
    }

    // --- surfaced_nominees ---

    #[test]
    fn surfaced_nominees_takes_the_top_three_with_no_ties() {
        let t = tally(&[(0, 5), (1, 4), (2, 3), (3, 1)]);
        let mut surfaced = surfaced_nominees(&t, 3);
        surfaced.sort();
        assert_eq!(surfaced, vec![PlayerId(0), PlayerId(1), PlayerId(2)]);
    }

    #[test]
    fn surfaced_nominees_expands_for_a_tie_at_the_boundary() {
        // 3rd place is a 3-way tie -- all of them surface, not a subset.
        let t = tally(&[(0, 5), (1, 3), (2, 3), (3, 3), (4, 1)]);
        let mut surfaced = surfaced_nominees(&t, 3);
        surfaced.sort();
        assert_eq!(
            surfaced,
            vec![PlayerId(0), PlayerId(1), PlayerId(2), PlayerId(3)]
        );
    }

    #[test]
    fn surfaced_nominees_returns_fewer_than_target_if_not_enough_candidates() {
        let t = tally(&[(0, 5), (1, 2)]);
        let mut surfaced = surfaced_nominees(&t, 3);
        surfaced.sort();
        assert_eq!(surfaced, vec![PlayerId(0), PlayerId(1)]);
    }

    #[test]
    fn surfaced_nominees_handles_an_empty_tally() {
        let t = tally(&[]);
        assert_eq!(surfaced_nominees(&t, 3), Vec::new());
    }

    #[test]
    fn surfaced_nominees_all_tied_still_surfaces_everyone() {
        let t = tally(&[(0, 2), (1, 2), (2, 2), (3, 2), (4, 2)]);
        let mut surfaced = surfaced_nominees(&t, 3);
        surfaced.sort();
        assert_eq!(
            surfaced,
            vec![
                PlayerId(0),
                PlayerId(1),
                PlayerId(2),
                PlayerId(3),
                PlayerId(4)
            ]
        );
    }

    // --- resolve_ballot ---

    #[test]
    fn resolve_ballot_locks_in_a_clean_single_winner() {
        let t = tally(&[(0, 10), (1, 3)]);
        let r = resolve_ballot(&t, 1);
        assert_eq!(r.locked_in, vec![PlayerId(0)]);
        assert!(r.tied_for_last_slot.is_empty());
    }

    #[test]
    fn resolve_ballot_locks_in_two_clean_winners_for_two_slots() {
        let t = tally(&[(0, 10), (1, 8), (2, 3)]);
        let r = resolve_ballot(&t, 2);
        assert_eq!(r.locked_in, vec![PlayerId(0), PlayerId(1)]);
        assert!(r.tied_for_last_slot.is_empty());
    }

    #[test]
    fn resolve_ballot_ties_for_the_last_slot_trigger_a_runoff_group() {
        // A(10) is a clean winner for slot 1; B and C tie at 8 for the
        // single remaining slot.
        let t = tally(&[(0, 10), (1, 8), (2, 8), (3, 3)]);
        let r = resolve_ballot(&t, 2);
        assert_eq!(r.locked_in, vec![PlayerId(0)]);
        let mut tied = r.tied_for_last_slot.clone();
        tied.sort();
        assert_eq!(tied, vec![PlayerId(1), PlayerId(2)]);
    }

    #[test]
    fn resolve_ballot_a_tie_for_the_only_slot_locks_in_nobody() {
        let t = tally(&[(0, 5), (1, 5)]);
        let r = resolve_ballot(&t, 1);
        assert!(r.locked_in.is_empty());
        let mut tied = r.tied_for_last_slot.clone();
        tied.sort();
        assert_eq!(tied, vec![PlayerId(0), PlayerId(1)]);
    }

    #[test]
    fn resolve_ballot_zero_vote_candidates_never_take_a_slot() {
        // Only one candidate got any votes at all -- the second slot must
        // go unfilled, not forced onto (or tied with) a 0-vote candidate.
        let t = tally(&[(0, 5), (1, 0), (2, 0)]);
        let r = resolve_ballot(&t, 2);
        assert_eq!(r.locked_in, vec![PlayerId(0)]);
        assert!(r.tied_for_last_slot.is_empty());
    }

    #[test]
    fn resolve_ballot_with_zero_slots_locks_in_nobody() {
        let t = tally(&[(0, 5)]);
        let r = resolve_ballot(&t, 0);
        assert!(r.locked_in.is_empty());
        assert!(r.tied_for_last_slot.is_empty());
    }

    #[test]
    fn resolve_ballot_with_an_empty_tally_locks_in_nobody() {
        let t = tally(&[]);
        let r = resolve_ballot(&t, 1);
        assert!(r.locked_in.is_empty());
        assert!(r.tied_for_last_slot.is_empty());
    }

    #[test]
    fn resolve_ballot_with_no_votes_at_all_locks_in_nobody() {
        let t = tally(&[(0, 0), (1, 0)]);
        let r = resolve_ballot(&t, 1);
        assert!(r.locked_in.is_empty());
        assert!(r.tied_for_last_slot.is_empty());
    }
}
