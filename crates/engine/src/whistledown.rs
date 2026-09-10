use crate::event::DomainEvent;
use crate::round::Round;
use crate::state::GameState;
use serde::{Deserialize, Serialize};

/// One Lady Whistledown post, tied to the round it narrates (rules.md §6:
/// "Publishes after every round, not just Denouncement rounds"). A pure
/// projection over `GameState::event_log` -- cheap to recompute on every
/// read, the same way `view_for` is, and needing no new `Command`/mutation
/// of its own.
///
/// Deliberately does NOT cover the Finale: rules.md's Last Round procedure
/// calls for Dalton to "walk through all three win conditions and reveal
/// everything" immediately after it -- the opposite of Whistledown's whole
/// premise (every result gets the *same* deliberately uninformative
/// treatment). The finale's reveal is its own, separate narrative piece.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WhistledownPost {
    pub round: Round,
    pub text: String,
}

/// rules.md §6's one given "every-round teaser," used verbatim for every
/// round that has no Denouncement to report (Rounds 1, 2, 4). rules.md
/// gives exactly this one line, not a rotating set the way the Cast-Out
/// templates are -- inventing additional variants would mean writing new
/// creative copy nobody asked for, not implementing what's specified.
const TEASER: &str =
    "The Ton dances on, unaware -- or is it unwilling? -- of what stirs beneath the ballroom floor.";

/// rules.md §6's three given Cast-Out templates, verbatim, rotated by
/// player ID (a stable, already-committed value -- keeps this function
/// pure with no caller-supplied randomness, per the engine's "randomness
/// at the boundary" convention, while still varying across different
/// victims). Note the second template names no one at all, by design --
/// rules.md is explicit that "no template ever correlates with who was
/// actually Cast Out," so the absence of a name here is not a bug.
const CAST_OUT_TEMPLATES: [&str; 3] = [
    "Dearest reader, the Ton awoke to shocking news -- Lord {name} was cast from society's good graces at last night's gathering, denounced by the very peers who once toasted his health. Whether justice was served or a grave error made, this author cannot say.",
    "Society lost one of its own last night, and this author confesses genuine sorrow at the loss -- though whether the room's judgment was righteous or rash, only time (and perhaps a guilty conscience) will tell.",
    "A most curious turn at last night's gathering -- {name}, cast out before the assembled Ton, protested their innocence to the last. This author has heard such protests before. Sometimes they are even true.",
];

/// Not covered by any rules.md template: a repeat tie (rules.md §5) can
/// leave a Denouncement's slot completely unfilled, with nobody actually
/// Cast Out that round. Rather than inventing new prose rules.md never
/// specified for this narrow case, this is a single plain, deliberately
/// uninformative fallback -- consistent in *spirit* (never confirms
/// anything) but flagged here as a gap worth real copy later if it matters.
const NO_ONE_CAST_OUT: &str =
    "Dearest reader, the assembled Ton could not agree last night, and so the evening closed with every mask still in place. This author suspects the debate is far from over.";

/// Every completed round's post, in round order. A round counts as
/// "complete" once the game has moved past it (`state.current_round() >
/// round`) or, for a Denouncement round the game hasn't advanced past yet,
/// once its Denouncement has actually closed (`denouncement_phase().is_none()`)
/// -- matching rules.md's "auto-publishes" happening right after
/// resolution, not only once the *next* round officially begins.
pub fn posts(state: &GameState) -> Vec<WhistledownPost> {
    let mut posts = Vec::new();
    let mut round = Round::One;
    let mut cast_out_this_round: Vec<crate::player::PlayerId> = Vec::new();
    let mut denouncement_opened_this_round = false;

    for event in state.event_log() {
        match event {
            DomainEvent::PlayerCastOut { player } => cast_out_this_round.push(*player),
            DomainEvent::DenouncementOpened => denouncement_opened_this_round = true,
            DomainEvent::RoundAdvanced { round: next } => {
                posts.push(round_post(
                    state,
                    round,
                    denouncement_opened_this_round,
                    &cast_out_this_round,
                ));
                round = *next;
                cast_out_this_round.clear();
                denouncement_opened_this_round = false;
            }
            _ => {}
        }
    }

    // The *current* round's own post, but only once there's actually
    // something to report: rules.md frames "Whistledown auto-publishes" as
    // the LAST step of the Round 3/5 procedure, happening right after
    // resolution -- before the round officially advances -- so a
    // Denouncement round gets its post the instant it closes. A round with
    // no Denouncement at all (1, 2, 4) has no equivalent early signal, so
    // it only gets its post once the game has actually moved past it (via
    // the loop above). Never the Finale (see `WhistledownPost`'s doc
    // comment).
    if round != Round::Finale
        && denouncement_opened_this_round
        && state.denouncement_phase().is_none()
    {
        posts.push(round_post(
            state,
            round,
            denouncement_opened_this_round,
            &cast_out_this_round,
        ));
    }

    posts
}

fn round_post(
    state: &GameState,
    round: Round,
    had_denouncement: bool,
    cast_out: &[crate::player::PlayerId],
) -> WhistledownPost {
    let text = if !had_denouncement {
        TEASER.to_string()
    } else if cast_out.is_empty() {
        NO_ONE_CAST_OUT.to_string()
    } else {
        cast_out
            .iter()
            .map(|&id| {
                let name = state
                    .player(id)
                    .map(|p| p.name.as_str())
                    .unwrap_or("a departed guest");
                let template = CAST_OUT_TEMPLATES[id.0 as usize % CAST_OUT_TEMPLATES.len()];
                template.replace("{name}", name)
            })
            .collect::<Vec<_>>()
            .join(" ")
    };
    WhistledownPost { round, text }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::character::Character;
    use crate::command::Command;
    use crate::player::Faction;
    use crate::state::apply_command;

    fn add_player(state: &mut GameState, name: &str, faction: Faction) -> crate::player::PlayerId {
        let events = apply_command(state, Command::AddPlayer { name: name.into() }).unwrap();
        let id = match events[0] {
            DomainEvent::PlayerAdded { id, .. } => id,
            _ => unreachable!(),
        };
        apply_command(
            state,
            Command::AssignFaction {
                player: id,
                faction,
            },
        )
        .unwrap();
        id
    }

    fn base_state() -> (GameState, crate::player::PlayerId, crate::player::PlayerId) {
        let mut state = GameState::new();
        let king_queen = add_player(&mut state, "King", Faction::Ton);
        apply_command(
            &mut state,
            Command::AssignCharacter {
                player: king_queen,
                character: Character::KingQueen,
            },
        )
        .unwrap();
        let leader = add_player(&mut state, "Leader", Faction::Uprising);
        apply_command(
            &mut state,
            Command::AssignCharacter {
                player: leader,
                character: Character::RevolutionaryLeader,
            },
        )
        .unwrap();
        apply_command(&mut state, Command::FinalizeSetup).unwrap();
        (state, king_queen, leader)
    }

    #[test]
    fn no_posts_before_round_one_has_actually_ended() {
        let (state, ..) = base_state();
        assert!(posts(&state).is_empty());
    }

    #[test]
    fn round_one_gets_the_teaser_once_round_two_begins() {
        let (mut state, ..) = base_state();
        apply_command(&mut state, Command::AdvanceRound).unwrap(); // -> Two
        let p = posts(&state);
        assert_eq!(p.len(), 1);
        assert_eq!(p[0].round, Round::One);
        assert_eq!(p[0].text, TEASER);
    }

    #[test]
    fn a_denouncement_round_publishes_the_instant_it_closes_not_only_on_the_next_advance() {
        let (mut state, _king_queen, leader) = base_state();
        for _ in 0..2 {
            apply_command(&mut state, Command::AdvanceRound).unwrap();
        }
        assert_eq!(state.current_round(), Round::Three);
        apply_command(&mut state, Command::OpenDenouncement).unwrap();
        apply_command(
            &mut state,
            Command::Nominate {
                voter: leader,
                nominee: leader,
            },
        )
        .unwrap();
        apply_command(&mut state, Command::CloseNomination).unwrap();
        apply_command(&mut state, Command::OpenBallot).unwrap();
        apply_command(
            &mut state,
            Command::CastBallot {
                voter: leader,
                ballot: crate::denouncement::Ballot::For(leader),
            },
        )
        .unwrap();
        apply_command(
            &mut state,
            Command::CloseBallot {
                fallback_replacement: None,
            },
        )
        .unwrap();

        let p = posts(&state);
        let round_three = p.iter().find(|post| post.round == Round::Three).unwrap();
        // Not asserting the name appears: PlayerId(1) (leader, added second
        // in `base_state`) happens to land on the one template that never
        // names anyone at all (`CAST_OUT_TEMPLATES[1]`), by design -- see
        // that constant's doc comment. The point here is just that a real,
        // non-teaser post exists the instant the ballot closes.
        assert_ne!(round_three.text, TEASER);
        assert!(!round_three.text.is_empty());
    }

    #[test]
    fn a_repeat_tie_leaving_nobody_cast_out_still_gets_a_post() {
        let mut state = GameState::new();
        let mut everyone = Vec::new();
        for i in 0..6 {
            everyone.push(add_player(&mut state, &format!("P{i}"), Faction::Ton));
        }
        apply_command(&mut state, Command::FinalizeSetup).unwrap();
        for _ in 0..2 {
            apply_command(&mut state, Command::AdvanceRound).unwrap();
        }
        let (a, b) = (everyone[0], everyone[1]);

        apply_command(&mut state, Command::OpenDenouncement).unwrap();
        apply_command(
            &mut state,
            Command::Nominate {
                voter: a,
                nominee: a,
            },
        )
        .unwrap();
        apply_command(
            &mut state,
            Command::Nominate {
                voter: b,
                nominee: b,
            },
        )
        .unwrap();
        apply_command(&mut state, Command::CloseNomination).unwrap();
        apply_command(&mut state, Command::OpenBallot).unwrap();
        for &voter in &everyone[0..3] {
            apply_command(
                &mut state,
                Command::CastBallot {
                    voter,
                    ballot: crate::denouncement::Ballot::For(a),
                },
            )
            .unwrap();
        }
        for &voter in &everyone[3..6] {
            apply_command(
                &mut state,
                Command::CastBallot {
                    voter,
                    ballot: crate::denouncement::Ballot::For(b),
                },
            )
            .unwrap();
        }
        let events = apply_command(
            &mut state,
            Command::CloseBallot {
                fallback_replacement: None,
            },
        )
        .unwrap();
        assert!(events
            .iter()
            .any(|e| matches!(e, DomainEvent::RunoffOpened { .. })));

        // Runoff ties again: same 3-vs-3 split.
        for &voter in &everyone[0..3] {
            apply_command(
                &mut state,
                Command::CastBallot {
                    voter,
                    ballot: crate::denouncement::Ballot::For(a),
                },
            )
            .unwrap();
        }
        for &voter in &everyone[3..6] {
            apply_command(
                &mut state,
                Command::CastBallot {
                    voter,
                    ballot: crate::denouncement::Ballot::For(b),
                },
            )
            .unwrap();
        }
        let runoff_events = apply_command(
            &mut state,
            Command::CloseRunoff {
                fallback_replacement: None,
            },
        )
        .unwrap();
        assert!(matches!(
            &runoff_events[0],
            DomainEvent::RunoffClosed { cast_out, unfilled_slot }
            if cast_out.is_empty() && *unfilled_slot
        ));

        let p = posts(&state);
        let round_three = p.iter().find(|post| post.round == Round::Three).unwrap();
        assert_eq!(round_three.text, NO_ONE_CAST_OUT);
    }

    #[test]
    fn the_finale_never_gets_an_ordinary_post() {
        let (mut state, ..) = base_state();
        for _ in 0..5 {
            apply_command(&mut state, Command::AdvanceRound).unwrap();
        }
        assert_eq!(state.current_round(), Round::Finale);
        assert!(posts(&state).iter().all(|p| p.round != Round::Finale));
    }

    #[test]
    fn cast_out_templates_rotate_across_different_victims_in_the_same_round() {
        // Two Cast-Outs in the same multi-slot Denouncement, at IDs chosen
        // specifically to land on two different templates (0 and 1 mod 3)
        // -- proves `round_post` actually varies the template per victim
        // rather than always picking the same one.
        let mut state = GameState::new();
        let mut ids = Vec::new();
        for i in 0..21 {
            let id = add_player(&mut state, &format!("P{i}"), Faction::Ton);
            ids.push(id);
        }
        apply_command(&mut state, Command::FinalizeSetup).unwrap();
        assert_eq!(state.competing_player_count(), 21);
        let victim_a = ids[0]; // PlayerId(0) -> template index 0
        let victim_b = ids[1]; // PlayerId(1) -> template index 1

        for _ in 0..2 {
            apply_command(&mut state, Command::AdvanceRound).unwrap();
        }
        apply_command(&mut state, Command::OpenDenouncement).unwrap();
        apply_command(
            &mut state,
            Command::Nominate {
                voter: ids[2],
                nominee: victim_a,
            },
        )
        .unwrap();
        apply_command(
            &mut state,
            Command::Nominate {
                voter: ids[3],
                nominee: victim_b,
            },
        )
        .unwrap();
        apply_command(&mut state, Command::CloseNomination).unwrap();
        apply_command(&mut state, Command::OpenBallot).unwrap();
        apply_command(
            &mut state,
            Command::CastBallot {
                voter: ids[2],
                ballot: crate::denouncement::Ballot::For(victim_a),
            },
        )
        .unwrap();
        apply_command(
            &mut state,
            Command::CastBallot {
                voter: ids[3],
                ballot: crate::denouncement::Ballot::For(victim_b),
            },
        )
        .unwrap();
        apply_command(
            &mut state,
            Command::CloseBallot {
                fallback_replacement: None,
            },
        )
        .unwrap();

        let p = posts(&state);
        let round_three = p.iter().find(|post| post.round == Round::Three).unwrap();
        // Order isn't asserted (depends on `resolve_ballot`'s own tiebreak,
        // which isn't this test's concern) -- just that both victims'
        // distinct templates both appear.
        assert!(round_three
            .text
            .contains(&CAST_OUT_TEMPLATES[0].replace("{name}", "P0")));
        assert!(round_three
            .text
            .contains(&CAST_OUT_TEMPLATES[1].replace("{name}", "P1")));
    }
}
