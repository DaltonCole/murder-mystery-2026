use crate::player::PlayerId;
use crate::state::GameState;
use serde::{Deserialize, Serialize};

/// Which of the Cult's four win paths (rules.md §2) is satisfied, if any.
/// `evaluate` can report more than one simultaneously true condition (see
/// `GameOutcome`) -- this identifies *which* Cult path, since unlike
/// Ton/Uprising the Cult has more than one route to a win.
///
/// Serializable so `DomainEvent::WinConditionChecked` can carry a
/// `Vec<CultPath>` -- `evaluate` itself has no I/O, but callers logging its
/// result through the event log need this to round-trip.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CultPath {
    /// Both the King/Queen and the Revolutionary Leader are converted and
    /// remain uncaught.
    A,
    /// The Revolutionary Leader is converted (and remains uncaught) and the
    /// King/Queen has been Denounced.
    B,
    /// The King/Queen is converted (and remains uncaught) and the
    /// Revolutionary Leader has been Denounced.
    C,
    /// The martyrdom path: the Cult Leader was personally Cast Out while at
    /// least one of the two royals was already converted. Locked in at the
    /// moment it happened -- see `resolve_cast_out` in `state.rs`.
    D,
}

/// The result of checking every faction's win/loss condition against the
/// current state. Deliberately independent booleans, not a single
/// `enum Winner` -- see the module-level doc comment on why more than one
/// can be true at once, and why that's a real property of the ruleset this
/// engine faithfully reports rather than silently resolving.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GameOutcome {
    pub ton_wins: bool,
    pub uprising_wins: bool,
    pub cult_wins: bool,
    /// Every Cult path currently satisfied (usually zero or one, but see
    /// the module doc comment -- A and D, or B and D, etc. can coincide).
    pub cult_paths: Vec<CultPath>,
}

impl GameOutcome {
    fn none() -> Self {
        GameOutcome {
            ton_wins: false,
            uprising_wins: false,
            cult_wins: false,
            cult_paths: Vec::new(),
        }
    }
}

/// Checks every faction's win/loss condition against `state`, exactly as
/// worded in rules.md §2. Safe to call at any point in the game (each
/// Denouncement resolution, not just the finale) -- it's a pure read, never
/// mutates anything.
///
/// # Why `ton_wins`/`cult_wins` and `uprising_wins` behave differently mid-game
///
/// Ton's and the Cult's conditions are all phrased as something *already
/// having permanently happened* (a successor line exhausted, a conversion
/// that's irreversible, a martyrdom that locked in) -- once true, they stay
/// true for the rest of the game, so reporting them the moment they become
/// true is correct at any point, not just at the end. The Uprising's
/// condition, by contrast, is phrased as *surviving to the end* -- that can
/// only be honestly confirmed once there IS no more game left, so
/// `uprising_wins` is only ever `true` when `state.current_round() ==
/// Round::Finale`. Checking it earlier would report "currently safe" as if
/// it meant "has won," which isn't the same claim.
///
/// # A known, real overlap in the ruleset (not an engine bug)
///
/// Once Revolutionary Leader succession is in play, it's possible for
/// `uprising_wins` and `cult_wins` (via [`CultPath::C`]) to both be true at
/// once: if an *earlier* Leader was correctly Denounced while unconverted
/// (setting the persistent "ever denounced" flag Path C reads) and
/// succession later installed a *different* Leader who survives to the end
/// unconverted (satisfying the Uprising's own win condition), both
/// conditions hold simultaneously. This engine reports it faithfully rather
/// than silently picking a winner -- it's a genuine open question in the
/// ruleset (does a successor's safe survival retroactively protect against
/// Path C, or not?) that needs a ruling from the game's designer, not an
/// engineering guess. See `win_condition::tests::
/// path_c_can_overlap_with_uprising_survival_after_a_succession` for a
/// worked example, and flag this prominently rather than resolving it here.
pub fn evaluate(state: &GameState) -> GameOutcome {
    let mut outcome = GameOutcome::none();

    let king_queen_converted_and_active = is_converted_and_active(state, state.king_queen());
    let leader_converted_and_active = is_converted_and_active(state, state.revolutionary_leader());

    // --- The Ton ---
    // "Wins if: the Revolutionary Leader is correctly identified and Cast
    // Out by game's end, and the King/Queen has not been converted."
    // Succession means "correctly Cast Out by game's end" cashes out as "no
    // successor remains" -- see the doc comment on `evaluate_win_conditions`
    // in the implementation plan and `state.rs::resolve_cast_out`.
    let king_queen_not_converted = !king_queen_converted_and_active;
    if state.revolutionary_leader().is_none() && king_queen_not_converted {
        outcome.ton_wins = true;
    }

    // --- The Uprising ---
    // "Wins if: their Leader survives to the end, uncaught and unconverted."
    // Only checked at the Finale -- see the doc comment on `evaluate` for
    // why this can't be a live "currently true" check the way Ton's and
    // the Cult's conditions can.
    if state.current_round() == crate::round::Round::Finale {
        if let Some(leader) = state.revolutionary_leader() {
            let player = state
                .player(leader)
                .expect("revolutionary_leader always names a real player");
            if player.status == crate::character::PlayerStatus::Active && !player.converted {
                outcome.uprising_wins = true;
            }
        }
    }

    // --- The Cult ---
    if leader_converted_and_active && king_queen_converted_and_active {
        outcome.cult_paths.push(CultPath::A);
    }
    if leader_converted_and_active && state.king_queen_ever_denounced_unconverted() {
        outcome.cult_paths.push(CultPath::B);
    }
    if king_queen_converted_and_active && state.revolutionary_leader_ever_denounced_unconverted() {
        outcome.cult_paths.push(CultPath::C);
    }
    if state.martyrdom_triggered() {
        outcome.cult_paths.push(CultPath::D);
    }
    outcome.cult_wins = !outcome.cult_paths.is_empty();

    outcome
}

fn is_converted_and_active(state: &GameState, holder: Option<PlayerId>) -> bool {
    holder
        .and_then(|id| state.player(id))
        .is_some_and(|p| p.converted && p.status == crate::character::PlayerStatus::Active)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::character::{Character, PlayerStatus};
    use crate::command::Command;
    use crate::state::apply_command;
    use crate::{Faction, Round};

    /// Builds a minimal, fully-assigned game: one King/Queen, one
    /// Prince/Princess, one Revolutionary Leader, one Cult Leader, plus one
    /// plain member of each public faction -- enough surface area to drive
    /// every win-condition branch without needing a full 20-30 player
    /// setup.
    fn base_state() -> (GameState, PlayerId, PlayerId, PlayerId, PlayerId) {
        let mut state = GameState::new();
        let mut add = |name: &str, faction: Faction| -> PlayerId {
            let events =
                apply_command(&mut state, Command::AddPlayer { name: name.into() }).unwrap();
            let id = match events[0] {
                crate::DomainEvent::PlayerAdded { id, .. } => id,
                _ => unreachable!(),
            };
            apply_command(
                &mut state,
                Command::AssignFaction {
                    player: id,
                    faction,
                },
            )
            .unwrap();
            id
        };

        let king_queen = add("King", Faction::Ton);
        let prince = add("Prince", Faction::Ton);
        let leader = add("Leader", Faction::Uprising);
        let cult_leader = add("CultLeader", Faction::Cult);

        apply_command(
            &mut state,
            Command::AssignCharacter {
                player: king_queen,
                character: Character::KingQueen,
            },
        )
        .unwrap();
        apply_command(
            &mut state,
            Command::AssignCharacter {
                player: prince,
                character: Character::PrincePrincess,
            },
        )
        .unwrap();
        apply_command(
            &mut state,
            Command::AssignCharacter {
                player: leader,
                character: Character::RevolutionaryLeader,
            },
        )
        .unwrap();
        apply_command(
            &mut state,
            Command::AssignCharacter {
                player: cult_leader,
                character: Character::CultLeader,
            },
        )
        .unwrap();
        apply_command(&mut state, Command::FinalizeSetup).unwrap();

        (state, king_queen, leader, cult_leader, prince)
    }

    #[test]
    fn nobody_wins_at_the_start() {
        let (state, ..) = base_state();
        let outcome = evaluate(&state);
        assert_eq!(outcome, GameOutcome::none());
    }

    #[test]
    fn ton_wins_when_leader_line_is_exhausted_and_king_queen_is_loyal() {
        let (mut state, _kq, leader, ..) = base_state();
        // No successor exists (only one Uprising player was added), so
        // Casting Out the Leader exhausts the line.
        apply_command(
            &mut state,
            Command::CastOut {
                player: leader,
                fallback_replacement: None,
            },
        )
        .unwrap();

        let outcome = evaluate(&state);
        assert!(outcome.ton_wins);
        assert!(!outcome.uprising_wins);
        assert!(!outcome.cult_wins);
    }

    #[test]
    fn uprising_wins_when_leader_survives_unconverted_to_the_finale() {
        let (mut state, ..) = base_state();
        // Not yet at the Finale: nothing has happened to the Leader, but
        // "survives to the end" genuinely isn't confirmable yet.
        assert!(!evaluate(&state).uprising_wins);

        for _ in 0..5 {
            apply_command(&mut state, Command::AdvanceRound).unwrap();
        }
        assert_eq!(state.current_round(), crate::round::Round::Finale);

        let outcome = evaluate(&state);
        assert!(outcome.uprising_wins);
        assert!(!outcome.ton_wins);
    }

    #[test]
    fn evaluate_at_finale_with_no_leader_left_does_not_panic_or_claim_uprising_wins() {
        let (mut state, _king_queen, leader, ..) = base_state();
        apply_command(
            &mut state,
            Command::CastOut {
                player: leader,
                fallback_replacement: None,
            },
        )
        .unwrap();
        assert_eq!(state.revolutionary_leader(), None);

        for _ in 0..5 {
            apply_command(&mut state, Command::AdvanceRound).unwrap();
        }
        assert_eq!(state.current_round(), Round::Finale);

        let outcome = evaluate(&state);
        assert!(!outcome.uprising_wins);
        assert!(
            outcome.ton_wins,
            "the line was already exhausted, so Ton's win stands at the Finale too"
        );
    }

    #[test]
    fn cult_path_a_both_converted_and_uncaught() {
        let (mut state, king_queen, leader, cult_leader, _prince) = base_state();
        apply_command(&mut state, Command::AdvanceRound).unwrap(); // recruitment slot
        apply_command(&mut state, Command::AdvanceRound).unwrap(); // recruitment slot
        apply_command(
            &mut state,
            Command::Convert {
                converter: cult_leader,
                target: king_queen,
            },
        )
        .unwrap();
        apply_command(
            &mut state,
            Command::Convert {
                converter: cult_leader,
                target: leader,
            },
        )
        .unwrap();

        let outcome = evaluate(&state);
        assert!(outcome.cult_wins);
        assert_eq!(outcome.cult_paths, vec![CultPath::A]);
        // Both remain Active and converted -- the Uprising's own condition
        // (unconverted) is false, so it must not also claim a win.
        assert!(!outcome.uprising_wins);
    }

    #[test]
    fn cult_path_b_leader_converted_king_queen_denounced() {
        let (mut state, king_queen, leader, cult_leader, prince) = base_state();
        // King/Queen falls at Round 3 -- exercises the full cascade, not
        // just the flag, to keep this test honest about what actually
        // happens in play.
        apply_command(&mut state, Command::AdvanceRound).unwrap(); // -> Two
        apply_command(&mut state, Command::AdvanceRound).unwrap(); // -> Three
        apply_command(
            &mut state,
            Command::CastOut {
                player: king_queen,
                fallback_replacement: None,
            },
        )
        .unwrap();
        // A new King/Queen was installed by the cascade -- the *original*
        // seat still counts as "the King/Queen was Denounced" per the
        // persistent flag, regardless of who holds the title now.
        assert!(state.king_queen_ever_denounced_unconverted());
        assert!(state.player(prince).unwrap().status == PlayerStatus::CastOut);

        apply_command(
            &mut state,
            Command::Convert {
                converter: cult_leader,
                target: leader,
            },
        )
        .unwrap();

        let outcome = evaluate(&state);
        assert!(outcome.cult_wins);
        assert!(outcome.cult_paths.contains(&CultPath::B));
    }

    #[test]
    fn cult_path_c_king_queen_converted_leader_denounced() {
        let (mut state, king_queen, leader, cult_leader, _prince) = base_state();
        apply_command(&mut state, Command::AdvanceRound).unwrap(); // recruitment slot
        apply_command(
            &mut state,
            Command::Convert {
                converter: cult_leader,
                target: king_queen,
            },
        )
        .unwrap();
        // No successor exists, so this also exhausts the Uprising's line --
        // deliberately checking that Path C fires even though Ton's own
        // condition is *also* independently true here (king_queen_not_
        // converted is false because king_queen IS converted, so Ton does
        // NOT win this specific case -- only Path C should fire).
        apply_command(
            &mut state,
            Command::CastOut {
                player: leader,
                fallback_replacement: None,
            },
        )
        .unwrap();

        let outcome = evaluate(&state);
        assert!(outcome.cult_paths.contains(&CultPath::C));
        assert!(
            !outcome.ton_wins,
            "king_queen is converted, so Ton must not win here"
        );
    }

    #[test]
    fn cult_path_d_martyrdom_requires_a_prior_conversion() {
        let (mut state, king_queen, _leader, cult_leader, _prince) = base_state();
        apply_command(&mut state, Command::AdvanceRound).unwrap(); // recruitment slot
        apply_command(
            &mut state,
            Command::Convert {
                converter: cult_leader,
                target: king_queen,
            },
        )
        .unwrap();
        apply_command(
            &mut state,
            Command::CastOut {
                player: cult_leader,
                fallback_replacement: None,
            },
        )
        .unwrap();

        let outcome = evaluate(&state);
        assert!(outcome.cult_paths.contains(&CultPath::D));
    }

    #[test]
    fn cult_leader_cast_out_without_any_conversion_does_not_trigger_martyrdom() {
        let (mut state, .., cult_leader, _prince) = base_state();
        apply_command(
            &mut state,
            Command::CastOut {
                player: cult_leader,
                fallback_replacement: None,
            },
        )
        .unwrap();

        let outcome = evaluate(&state);
        assert!(!outcome.cult_paths.contains(&CultPath::D));
    }

    #[test]
    fn martyrdom_locks_in_and_a_later_conversion_does_not_retroactively_trigger_it() {
        let (mut state, king_queen, _leader, cult_leader, _prince) = base_state();
        // Cult Leader falls *before* any conversion has happened.
        apply_command(
            &mut state,
            Command::CastOut {
                player: cult_leader,
                fallback_replacement: None,
            },
        )
        .unwrap();
        assert!(!state.martyrdom_triggered());

        // A conversion can still be attempted narratively-late in a real
        // game via some other path in later phases, but this engine has no
        // way to convert *after* the Cult Leader is gone (Convert takes a
        // `converter` who must still be able to act) -- this test instead
        // documents the locked-in invariant directly against the flag,
        // since that's the property that actually matters for Path D.
        let _ = king_queen; // not converted in this scenario -- see above
        assert!(!evaluate(&state).cult_paths.contains(&CultPath::D));
    }

    #[test]
    fn path_c_can_overlap_with_uprising_survival_after_a_succession() {
        // See the module-level doc comment on `evaluate` for the full
        // explanation -- this is the concrete, worked example of the real
        // ruleset overlap it describes, not a bug.
        let mut state = GameState::new();
        let mut add = |name: &str, faction: Faction| -> PlayerId {
            let events =
                apply_command(&mut state, Command::AddPlayer { name: name.into() }).unwrap();
            let id = match events[0] {
                crate::DomainEvent::PlayerAdded { id, .. } => id,
                _ => unreachable!(),
            };
            apply_command(
                &mut state,
                Command::AssignFaction {
                    player: id,
                    faction,
                },
            )
            .unwrap();
            id
        };

        let king_queen = add("King", Faction::Ton);
        let leader1 = add("Leader1", Faction::Uprising);
        let leader2 = add("Leader2", Faction::Uprising);
        let cult_leader = add("CultLeader", Faction::Cult);

        apply_command(
            &mut state,
            Command::AssignCharacter {
                player: king_queen,
                character: Character::KingQueen,
            },
        )
        .unwrap();
        apply_command(
            &mut state,
            Command::AssignCharacter {
                player: leader1,
                character: Character::RevolutionaryLeader,
            },
        )
        .unwrap();
        apply_command(
            &mut state,
            Command::AssignCharacter {
                player: cult_leader,
                character: Character::CultLeader,
            },
        )
        .unwrap();
        apply_command(&mut state, Command::FinalizeSetup).unwrap();

        // Leader1 is caught while unconverted -- succession promotes
        // Leader2 (the only other eligible Uprising member).
        apply_command(
            &mut state,
            Command::CastOut {
                player: leader1,
                fallback_replacement: Some(leader2),
            },
        )
        .unwrap();
        assert_eq!(state.revolutionary_leader(), Some(leader2));
        assert!(state.revolutionary_leader_ever_denounced_unconverted());

        // The King/Queen is converted and never caught.
        apply_command(&mut state, Command::AdvanceRound).unwrap(); // recruitment slot, -> Two
        apply_command(
            &mut state,
            Command::Convert {
                converter: cult_leader,
                target: king_queen,
            },
        )
        .unwrap();

        // Leader2 survives to the end, uncaught and unconverted -- actually
        // reach the Finale, since `uprising_wins` is only meaningful there.
        for _ in [Round::Three, Round::Four, Round::Five, Round::Finale] {
            apply_command(&mut state, Command::AdvanceRound).unwrap();
        }
        assert_eq!(state.current_round(), Round::Finale);

        let outcome = evaluate(&state);
        assert!(
            outcome.uprising_wins,
            "the current Leader (Leader2) genuinely satisfies the Uprising's own condition"
        );
        assert!(
            outcome.cult_paths.contains(&CultPath::C),
            "Path C's clauses are independently satisfied by King/Queen's conversion \
             and Leader1's earlier, permanent denouncement flag"
        );
    }
}
