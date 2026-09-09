use serde::{Deserialize, Serialize};

/// The fixed round sequence from rules.md §4: Round 1 (intro) -> Round 2
/// (contest) -> Round 3 (task + Denouncement) -> Round 4 (contest) ->
/// Intermission -> Round 5 (task + Denouncement) -> Finale (the Last
/// Denouncement). Intermission isn't a variant here since nothing in the
/// win-condition/cast-out logic keys off it; it's a pure content phase.
///
/// `Ord` is derived from declaration order specifically so
/// `current_round < Round::Five` can express "before Round 5" (the
/// King/Queen transfer ability's deadline) and `current_round ==
/// Round::Three` can express the Round-3-specific Prince/Princess cascade,
/// both direct quotes from rules.md.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Round {
    One,
    Two,
    Three,
    Four,
    Five,
    Finale,
}

impl Round {
    /// The round immediately after this one, or `None` if this is already
    /// the Finale. Used by `Command::AdvanceRound` to move forward one step
    /// at a time -- see `state.rs`.
    pub fn next(self) -> Option<Round> {
        match self {
            Round::One => Some(Round::Two),
            Round::Two => Some(Round::Three),
            Round::Three => Some(Round::Four),
            Round::Four => Some(Round::Five),
            Round::Five => Some(Round::Finale),
            Round::Finale => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ordering_matches_the_declared_sequence() {
        assert!(Round::One < Round::Two);
        assert!(Round::Two < Round::Three);
        assert!(Round::Three < Round::Five);
        assert!(Round::Four < Round::Five);
        assert!(Round::Five < Round::Finale);
    }

    #[test]
    fn before_round_five_check_matches_rules_md_wording() {
        // "before round 5" -- the King/Queen transfer ability's deadline.
        assert!(Round::One < Round::Five);
        assert!(Round::Four < Round::Five);
        assert!(!(Round::Five < Round::Five));
        assert!(!(Round::Finale < Round::Five));
    }

    #[test]
    fn next_advances_sequentially_and_stops_at_finale() {
        assert_eq!(Round::One.next(), Some(Round::Two));
        assert_eq!(Round::Two.next(), Some(Round::Three));
        assert_eq!(Round::Three.next(), Some(Round::Four));
        assert_eq!(Round::Four.next(), Some(Round::Five));
        assert_eq!(Round::Five.next(), Some(Round::Finale));
        assert_eq!(Round::Finale.next(), None);
    }
}
