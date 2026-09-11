//! rules.md §1's weighted setup raffle: "every player rates their desired
//! involvement 1-10 at signup. A rating of 6+ enters a weighted raffle for
//! major roles... A rating of 5 or below can't receive a major role unless
//! the pool is otherwise underfilled."
//!
//! Like `bio::task_candidates`/`whistledown::posts`/`finale_reveal::reveal`,
//! this module only *computes* -- nothing here mutates `GameState`. The one
//! genuinely random step (whose ticket comes up first) can't live in the
//! engine at all (see "randomness at the boundary" in `DrawIntermissionEntrants`/
//! `CastOut`'s own doc comments): the caller shuffles a flat ticket list with
//! a real RNG and hands the result to `raffle_priority`, which turns it into
//! a fair, deterministic priority ranking; `raffle_winners` then just walks
//! that ranking. The caller commits each winner via the existing
//! `Command::AssignCharacter` (which -- see its own doc comment -- now
//! auto-assigns the matching faction too, since this raffle assigns roles
//! *before* factions exist at all).
use crate::character::Character;
use crate::player::PlayerId;
use std::collections::{BTreeMap, BTreeSet};

/// rules.md §1's bounds on a signup interest rating.
pub const MIN_INTEREST_LEVEL: u8 = 1;
pub const MAX_INTEREST_LEVEL: u8 = 10;

/// rules.md §1's exact ticket table. A rating of 5 or below gets zero
/// tickets here -- see the module doc comment for how the "unless the pool
/// is otherwise underfilled" fallback still gives them a chance.
pub fn ticket_count(interest_level: u8) -> u32 {
    match interest_level {
        6 => 1,
        7 => 5,
        8 => 20,
        9 => 50,
        10 => 100,
        _ => 0,
    }
}

/// Every named role the setup raffle assigns, in the order roles are
/// filled -- the four major titles first (rules.md's own framing: "a
/// weighted raffle for major roles"), then the rest of the Phase 2/3
/// roster. Deliberately excludes `Deceiver`: rules.md §3.3 has the Cult
/// Leader designate it *after* a Convert recruits a second Cult member, not
/// at setup (the Cult starts seeded with just the Cult Leader -- raffling
/// Deceiver here too would force a second starting Cult member). Also
/// excludes the three generic catch-alls (`NormalTon`/`NormalUprising`/
/// `Cultist`), which `Command::FinalizeSetup` fills in for whoever doesn't
/// win a named role at all.
pub const RAFFLED_ROLES: [Character; 17] = [
    Character::KingQueen,
    Character::PrincePrincess,
    Character::RevolutionaryLeader,
    Character::CultLeader,
    Character::Oracle,
    Character::Almanac,
    Character::Spymaster,
    Character::PriestPriestess,
    Character::PotionMaker,
    Character::Magistrate,
    Character::Bartender,
    Character::DoctorMedic,
    Character::Firebrand,
    Character::CellLeader,
    Character::Duelist,
    Character::Agitator,
    Character::GrandInquisitor,
];

/// Expands `tickets` into one entry per ticket -- a player with 20 tickets
/// appears 20 times. The caller shuffles the result with a real RNG (see
/// the module doc comment) and passes it to `raffle_priority`.
pub fn ticket_slots(tickets: &BTreeMap<PlayerId, u32>) -> Vec<PlayerId> {
    let mut slots = Vec::new();
    for (&id, &count) in tickets {
        for _ in 0..count {
            slots.push(id);
        }
    }
    slots
}

/// Turns a caller-shuffled flat ticket list into a priority-ordered,
/// deduplicated ranking (first occurrence wins each player's rank) --
/// whoever's first ticket lands earliest in the shuffle effectively "wins"
/// that priority slot, proportional to how many tickets they held.
/// `shuffled_fallback` (also caller-shuffled) is appended afterward, lowest
/// priority, for rules.md's "unless the pool is otherwise underfilled"
/// clause: a player who appears in both lists keeps their higher (ticketed)
/// rank, since `raffle_winners` only ever consumes this list front-to-back.
pub fn raffle_priority(
    shuffled_slots: &[PlayerId],
    shuffled_fallback: &[PlayerId],
) -> Vec<PlayerId> {
    let mut seen = BTreeSet::new();
    let mut order = Vec::new();
    for &id in shuffled_slots.iter().chain(shuffled_fallback) {
        if seen.insert(id) {
            order.push(id);
        }
    }
    order
}

/// Walks `RAFFLED_ROLES` in order, awarding each to the highest-priority
/// still-unassigned player in `priority`. Pure data -- no mutation; the
/// caller commits each pair via `Command::AssignCharacter`. Fewer winners
/// than roles if `priority` runs out first (a very small game).
pub fn raffle_winners(priority: &[PlayerId]) -> Vec<(Character, PlayerId)> {
    priority
        .iter()
        .copied()
        .zip(RAFFLED_ROLES.iter().copied())
        .map(|(player, character)| (character, player))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ticket_counts_match_rules_md_exactly() {
        assert_eq!(ticket_count(1), 0);
        assert_eq!(ticket_count(5), 0);
        assert_eq!(ticket_count(6), 1);
        assert_eq!(ticket_count(7), 5);
        assert_eq!(ticket_count(8), 20);
        assert_eq!(ticket_count(9), 50);
        assert_eq!(ticket_count(10), 100);
    }

    #[test]
    fn ticket_count_is_zero_out_of_range() {
        assert_eq!(ticket_count(0), 0);
        assert_eq!(ticket_count(11), 0);
    }

    #[test]
    fn raffled_roles_excludes_deceiver_and_catch_alls() {
        assert!(!RAFFLED_ROLES.contains(&Character::Deceiver));
        assert!(!RAFFLED_ROLES.contains(&Character::NormalTon));
        assert!(!RAFFLED_ROLES.contains(&Character::NormalUprising));
        assert!(!RAFFLED_ROLES.contains(&Character::Cultist));
    }

    #[test]
    fn raffled_roles_leads_with_the_four_major_titles() {
        assert_eq!(
            &RAFFLED_ROLES[0..4],
            &[
                Character::KingQueen,
                Character::PrincePrincess,
                Character::RevolutionaryLeader,
                Character::CultLeader,
            ]
        );
    }

    #[test]
    fn ticket_slots_expands_proportionally_to_ticket_count() {
        let mut tickets = BTreeMap::new();
        tickets.insert(PlayerId(1), 3);
        tickets.insert(PlayerId(2), 1);
        let slots = ticket_slots(&tickets);
        assert_eq!(slots.iter().filter(|&&id| id == PlayerId(1)).count(), 3);
        assert_eq!(slots.iter().filter(|&&id| id == PlayerId(2)).count(), 1);
        assert_eq!(slots.len(), 4);
    }

    #[test]
    fn ticket_slots_omits_zero_ticket_players() {
        let mut tickets = BTreeMap::new();
        tickets.insert(PlayerId(1), 0);
        assert!(ticket_slots(&tickets).is_empty());
    }

    #[test]
    fn raffle_priority_dedupes_by_first_occurrence() {
        let shuffled = vec![PlayerId(2), PlayerId(1), PlayerId(2), PlayerId(1)];
        let priority = raffle_priority(&shuffled, &[]);
        assert_eq!(priority, vec![PlayerId(2), PlayerId(1)]);
    }

    #[test]
    fn raffle_priority_appends_fallback_after_ticketed_players() {
        let shuffled = vec![PlayerId(1)];
        let fallback = vec![PlayerId(2), PlayerId(3)];
        let priority = raffle_priority(&shuffled, &fallback);
        assert_eq!(priority, vec![PlayerId(1), PlayerId(2), PlayerId(3)]);
    }

    #[test]
    fn raffle_priority_prefers_ticketed_rank_over_fallback_rank_for_the_same_player() {
        // PlayerId(9) shows up ticketed AND (redundantly) in the fallback
        // list -- their higher, ticketed-tier rank should win, not get
        // overwritten by their fallback-tier appearance.
        let shuffled = vec![PlayerId(9), PlayerId(1)];
        let fallback = vec![PlayerId(9), PlayerId(2)];
        let priority = raffle_priority(&shuffled, &fallback);
        assert_eq!(priority, vec![PlayerId(9), PlayerId(1), PlayerId(2)]);
    }

    #[test]
    fn raffle_winners_pairs_priority_order_with_raffled_roles_in_order() {
        let priority: Vec<PlayerId> = (1..=3).map(PlayerId).collect();
        let winners = raffle_winners(&priority);
        assert_eq!(
            winners,
            vec![
                (Character::KingQueen, PlayerId(1)),
                (Character::PrincePrincess, PlayerId(2)),
                (Character::RevolutionaryLeader, PlayerId(3)),
            ]
        );
    }

    #[test]
    fn raffle_winners_stops_short_if_priority_runs_out_before_roles_do() {
        let winners = raffle_winners(&[PlayerId(1)]);
        assert_eq!(winners, vec![(Character::KingQueen, PlayerId(1))]);
    }

    #[test]
    fn raffle_winners_fills_every_role_when_priority_is_long_enough() {
        let priority: Vec<PlayerId> = (1..=RAFFLED_ROLES.len() as u32).map(PlayerId).collect();
        let winners = raffle_winners(&priority);
        assert_eq!(winners.len(), RAFFLED_ROLES.len());
    }

    #[test]
    fn a_low_interest_player_can_still_win_a_role_when_the_ticketed_pool_is_underfilled() {
        // Nobody rated 6+ at all -- the entire raffle has to fall back to
        // the low-interest pool, which is exactly rules.md's "unless the
        // pool is otherwise underfilled" clause.
        let priority = raffle_priority(&[], &[PlayerId(1)]);
        let winners = raffle_winners(&priority);
        assert_eq!(winners, vec![(Character::KingQueen, PlayerId(1))]);
    }
}
