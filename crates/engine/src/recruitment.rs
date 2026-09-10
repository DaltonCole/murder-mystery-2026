use crate::round::Round;

/// How many recruitment slots open up when the game advances into `round`,
/// for a game with `competing_players` (Ton+Uprising+Cult, excluding
/// Servants -- same scoping as the Denouncement's execution-count formula).
///
/// rules.md §3.3: "at 20 players or fewer, the Cult recruits one new
/// member every 2 rounds, flat, for the whole game. At 21-30 players, that
/// same flat cadence holds through Round 3, but from Round 4 onward, each
/// recruitment window brings in 2 new members instead of 1."
///
/// Dalton's resolution of the exact window schedule during Phase 2
/// planning: a window opens every time the game advances to a new round
/// (not just at the two Denouncement rounds), with the batch size doubling
/// once the game has reached Round Four or later, for games with more than
/// 20 competing players. He explicitly flagged this as a first pass to be
/// tuned after a real playtest, not a rule locked in stone -- see the
/// session's own resolution notes.
pub fn recruitment_window_size(round: Round, competing_players: usize) -> usize {
    if competing_players > 20 && round >= Round::Four {
        2
    } else {
        1
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flat_schedule_at_or_below_twenty_competing_players() {
        for round in [
            Round::Two,
            Round::Three,
            Round::Four,
            Round::Five,
            Round::Finale,
        ] {
            assert_eq!(recruitment_window_size(round, 20), 1);
            assert_eq!(recruitment_window_size(round, 5), 1);
        }
    }

    #[test]
    fn ramping_schedule_above_twenty_competing_players_stays_flat_before_round_four() {
        assert_eq!(recruitment_window_size(Round::Two, 25), 1);
        assert_eq!(recruitment_window_size(Round::Three, 25), 1);
    }

    #[test]
    fn ramping_schedule_above_twenty_competing_players_doubles_from_round_four_on() {
        assert_eq!(recruitment_window_size(Round::Four, 25), 2);
        assert_eq!(recruitment_window_size(Round::Five, 25), 2);
        assert_eq!(recruitment_window_size(Round::Finale, 25), 2);
    }

    #[test]
    fn exactly_twenty_one_competing_players_is_already_above_the_threshold() {
        assert_eq!(recruitment_window_size(Round::Four, 21), 2);
    }

    #[test]
    fn round_one_never_asked_for_but_would_be_flat_regardless_of_population() {
        // Round::One is never actually passed in practice (no window opens
        // until the game first advances), but the formula shouldn't panic
        // or misbehave if it ever is.
        assert_eq!(recruitment_window_size(Round::One, 25), 1);
    }
}
