use crate::character::Character;
use crate::event::DomainEvent;
use crate::player::{Faction, PlayerId};
use crate::round::Round;
use crate::state::GameState;
use crate::win_condition::GameOutcome;
use serde::{Deserialize, Serialize};

/// One player's true identity, for the finale reveal only -- everywhere
/// else in the engine, `PlayerView` never exposes another player's
/// faction/conversion status to anyone (see `view::view_for`'s "no
/// ambient god-view" design note). rules.md §4 deliberately breaks that
/// rule at exactly this one moment: "Dalton walks through all three win
/// conditions and reveals everything that happened privately all game."
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlayerReveal {
    pub id: PlayerId,
    pub name: String,
    pub true_faction: Faction,
    pub character: Option<Character>,
    pub converted: bool,
}

/// The full post-Finale walkthrough (rules.md §4's "Dalton walks through
/// all three win conditions and reveals everything"). `key_events` is
/// filtered straight from `GameState::event_log` -- the implementation
/// plan's own "Core Domain Model" section names the event log as "the
/// finale's 'reveal everything' data source," so this doesn't reconstruct
/// a separate narrative, it just picks out the events that actually matter
/// for the walkthrough (conversions, successions, martyrdom) from
/// everything else logged over the whole game.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FinaleReveal {
    pub winner: GameOutcome,
    pub everyone: Vec<PlayerReveal>,
    /// Full identities of whoever was actually Cast Out at the Last
    /// Denouncement specifically -- the subset of `everyone` rules.md says
    /// gets shown "publicly on a shared screen" the moment the Finale's
    /// ballot/runoff closes, distinct from the full walkthrough that
    /// follows it. See `PlayerView::finale_cast_out_reveal`'s doc comment
    /// for why this is exposed separately, and more broadly, than the rest
    /// of this struct.
    pub cast_out_this_denouncement: Vec<PlayerId>,
    pub key_events: Vec<DomainEvent>,
}

fn is_key_event(event: &DomainEvent) -> bool {
    matches!(
        event,
        DomainEvent::Converted { .. }
            | DomainEvent::KingQueenConversionCascade { .. }
            | DomainEvent::KingQueenCastOutCascade { .. }
            | DomainEvent::RevolutionaryLeaderSucceeded { .. }
            | DomainEvent::MartyrdomTriggered { .. }
    )
}

/// `None` until the Last Denouncement has actually closed (rules.md: the
/// reveal happens "immediately after" it, not before -- the real outcome
/// isn't knowable any earlier, the same timing rule
/// `GameState::gallery_resolved`'s gating already follows).
pub fn reveal(state: &GameState) -> Option<FinaleReveal> {
    if state.current_round() != Round::Finale || state.denouncement_phase().is_some() {
        return None;
    }
    // `denouncement_phase().is_some()` alone can't distinguish "never
    // opened" from "opened, then closed" -- `close_denouncement` resets the
    // field to `None` rather than leaving a terminal "Closed" phase behind,
    // so both states look identical from here. A security/reliability
    // review found that gap meant `reveal` returned the full walkthrough
    // (everyone's true faction/character/conversion) the instant
    // `Round::Finale` began, *before* `OpenDenouncement` for the Last
    // Denouncement was ever called -- reachable through the real intended
    // driving pattern (advancing to the Finale round is a separate step
    // from opening its Denouncement). Track "opened this round" the same
    // way `whistledown::posts` already does: scan for `DenouncementOpened`
    // since the last `RoundAdvanced`.
    let mut opened_this_round = false;
    for event in state.event_log() {
        match event {
            DomainEvent::RoundAdvanced { .. } => opened_this_round = false,
            DomainEvent::DenouncementOpened => opened_this_round = true,
            _ => {}
        }
    }
    if !opened_this_round {
        return None;
    }

    let everyone = state
        .players()
        .map(|p| PlayerReveal {
            id: p.id,
            name: p.name.clone(),
            true_faction: p.true_faction(),
            character: p.character,
            converted: p.converted,
        })
        .collect();

    // Whoever was Cast Out since the *last* round advance -- since we've
    // already confirmed `current_round == Finale`, that's exactly the
    // Finale's own Denouncement, not an earlier round's.
    let mut cast_out_this_denouncement = Vec::new();
    for event in state.event_log() {
        match event {
            DomainEvent::RoundAdvanced { .. } => cast_out_this_denouncement.clear(),
            DomainEvent::PlayerCastOut { player } => cast_out_this_denouncement.push(*player),
            _ => {}
        }
    }

    let key_events = state
        .event_log()
        .iter()
        .filter(|e| is_key_event(e))
        .cloned()
        .collect();

    Some(FinaleReveal {
        winner: crate::win_condition::evaluate(state),
        everyone,
        cast_out_this_denouncement,
        key_events,
    })
}

/// rules.md §6's martyrdom message, quoted verbatim with the Cult Leader's
/// name filled in -- delivered privately and immediately to whichever
/// currently-converted title-holder(s) triggered it. Unlike `reveal`
/// above, this isn't gated to the Finale: martyrdom can trigger at any
/// Denouncement the Cult Leader is personally Cast Out at, per rules.md
/// §2's Path D. Scoped to whoever holds a converted King/Queen or
/// Revolutionary Leader *character* -- not the current title *slot*
/// (`state.king_queen()`/`state.revolutionary_leader()`): a reliability
/// review found those diverge for a converted King/Queen specifically,
/// since `convert()`'s own cascade reassigns the slot to a fresh player
/// while the original convert keeps `character == Some(KingQueen)` forever
/// (rules.md: "a converted player keeps their original character"). Once
/// that cascade fires in a realistic game (any other eligible Ton player
/// exists), checking the slot meant *neither* player ever got the
/// message: the new holder isn't converted, and the real convert no
/// longer equals `state.king_queen()`. `character` has no such split --
/// it's permanent on the player it was assigned to.
pub fn martyrdom_message_for(state: &GameState, viewer: PlayerId) -> Option<String> {
    if !state.martyrdom_triggered() {
        return None;
    }
    let player = state.player(viewer)?;
    if !player.converted {
        return None;
    }
    let holds_a_convertible_title = matches!(
        player.character,
        Some(Character::KingQueen) | Some(Character::RevolutionaryLeader)
    );
    if !holds_a_convertible_title {
        return None;
    }
    let cult_leader_name = state
        .event_log()
        .iter()
        .find_map(|e| match e {
            DomainEvent::MartyrdomTriggered { cult_leader } => {
                state.player(*cult_leader).map(|p| p.name.clone())
            }
            _ => None,
        })
        .unwrap_or_else(|| "your Cult Leader".to_string());
    Some(format!(
        "Your hands are still trembling from the vote. You did not know -- could not have \
         known -- the weight {cult_leader_name} carried, or the promise you made them in \
         confidence. And yet their final words to you echo louder now than any denouncement: \
         'Should I fall, you will finish what we began.'"
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::command::Command;
    use crate::state::apply_command;

    fn add_player(state: &mut GameState, name: &str, faction: Faction) -> PlayerId {
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

    #[test]
    fn no_reveal_before_the_finale_is_reached() {
        let mut state = GameState::new();
        add_player(&mut state, "Alice", Faction::Ton);
        apply_command(&mut state, Command::FinalizeSetup).unwrap();
        assert!(reveal(&state).is_none());
    }

    #[test]
    fn no_reveal_while_the_finales_own_denouncement_is_still_open() {
        let mut state = GameState::new();
        add_player(&mut state, "Alice", Faction::Ton);
        apply_command(&mut state, Command::FinalizeSetup).unwrap();
        for _ in 0..5 {
            apply_command(&mut state, Command::AdvanceRound).unwrap();
        }
        assert_eq!(state.current_round(), Round::Finale);
        apply_command(&mut state, Command::OpenDenouncement).unwrap();
        assert!(reveal(&state).is_none());
    }

    #[test]
    fn no_reveal_at_the_finale_before_its_denouncement_has_even_opened() {
        // Security regression: `denouncement_phase().is_some()` alone
        // can't tell "never opened" apart from "opened, then closed" --
        // both are `None`. Without the fix, `reveal` returned the full
        // walkthrough (everyone's true faction/character/conversion) the
        // instant `Round::Finale` began, before `OpenDenouncement` was
        // ever called -- reachable through the real driving pattern, since
        // advancing to the Finale round is a separate step from opening
        // its Denouncement.
        let mut state = GameState::new();
        add_player(&mut state, "Alice", Faction::Ton);
        apply_command(&mut state, Command::FinalizeSetup).unwrap();
        for _ in 0..5 {
            apply_command(&mut state, Command::AdvanceRound).unwrap();
        }
        assert_eq!(state.current_round(), Round::Finale);
        assert!(
            reveal(&state).is_none(),
            "must not reveal before the Last Denouncement has even opened"
        );
    }

    #[test]
    fn reveal_lists_everyones_true_identity_and_the_finales_own_cast_out() {
        let mut state = GameState::new();
        let cult_leader = add_player(&mut state, "CultLeader", Faction::Cult);
        apply_command(
            &mut state,
            Command::AssignCharacter {
                player: cult_leader,
                character: Character::CultLeader,
            },
        )
        .unwrap();
        let king_queen = add_player(&mut state, "King", Faction::Ton);
        apply_command(
            &mut state,
            Command::AssignCharacter {
                player: king_queen,
                character: Character::KingQueen,
            },
        )
        .unwrap();
        apply_command(&mut state, Command::FinalizeSetup).unwrap();
        apply_command(&mut state, Command::AdvanceRound).unwrap(); // -> Two, slot
        apply_command(
            &mut state,
            Command::Convert {
                converter: cult_leader,
                target: king_queen,
            },
        )
        .unwrap();
        for _ in 0..4 {
            apply_command(&mut state, Command::AdvanceRound).unwrap();
        }
        assert_eq!(state.current_round(), Round::Finale);

        apply_command(&mut state, Command::OpenDenouncement).unwrap();
        apply_command(
            &mut state,
            Command::Nominate {
                voter: king_queen,
                nominee: cult_leader,
            },
        )
        .unwrap();
        apply_command(&mut state, Command::CloseNomination).unwrap();
        apply_command(&mut state, Command::OpenBallot).unwrap();
        apply_command(
            &mut state,
            Command::CastBallot {
                voter: king_queen,
                ballot: crate::denouncement::Ballot::For(cult_leader),
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

        let r = reveal(&state).expect("the Finale's Denouncement has closed");
        assert_eq!(r.cast_out_this_denouncement, vec![cult_leader]);
        let king_queen_reveal = r.everyone.iter().find(|p| p.id == king_queen).unwrap();
        assert!(king_queen_reveal.converted);
        assert_eq!(king_queen_reveal.true_faction, Faction::Cult);
        assert!(r
            .key_events
            .iter()
            .any(|e| matches!(e, DomainEvent::Converted { .. })));
        assert!(r
            .key_events
            .iter()
            .any(|e| matches!(e, DomainEvent::MartyrdomTriggered { .. })));

        let message = martyrdom_message_for(&state, king_queen)
            .expect("the converted, still-titled King/Queen should get the message");
        assert!(message.contains("CultLeader"));
        assert!(message.contains("Should I fall, you will finish what we began."));
    }

    #[test]
    fn martyrdom_message_is_none_without_martyrdom_and_none_for_an_uninvolved_player() {
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
        let bystander = add_player(&mut state, "Bystander", Faction::Ton);
        apply_command(&mut state, Command::FinalizeSetup).unwrap();

        assert_eq!(martyrdom_message_for(&state, king_queen), None);
        assert_eq!(martyrdom_message_for(&state, bystander), None);
    }

    #[test]
    fn martyrdom_message_still_reaches_the_original_convert_after_the_king_queen_cascade_reassigns_the_crown(
    ) {
        // Regression: `convert()`'s King/Queen cascade reassigns
        // `state.king_queen()` to a fresh player while the original
        // convert keeps `character == Some(KingQueen)` forever (rules.md:
        // "a converted player keeps their original character"). Checking
        // the title *slot* here used to mean neither player ever got the
        // martyrdom message once a real replacement existed -- the
        // original test above never caught this because its 2-player
        // setup has no eligible replacement, so the cascade never actually
        // moves the crown.
        let mut state = GameState::new();
        let cult_leader = add_player(&mut state, "CultLeader", Faction::Cult);
        apply_command(
            &mut state,
            Command::AssignCharacter {
                player: cult_leader,
                character: Character::CultLeader,
            },
        )
        .unwrap();
        let king_queen = add_player(&mut state, "King", Faction::Ton);
        apply_command(
            &mut state,
            Command::AssignCharacter {
                player: king_queen,
                character: Character::KingQueen,
            },
        )
        .unwrap();
        // A second, untitled Ton player -- the cascade's actual replacement.
        let extra_ton = add_player(&mut state, "ExtraTon", Faction::Ton);
        apply_command(&mut state, Command::FinalizeSetup).unwrap();
        apply_command(&mut state, Command::AdvanceRound).unwrap(); // -> Two, opens a recruitment slot

        apply_command(
            &mut state,
            Command::Convert {
                converter: cult_leader,
                target: king_queen,
            },
        )
        .unwrap();
        // Confirm the cascade actually reassigned the crown -- this test
        // is meaningless if it didn't.
        assert_eq!(state.king_queen(), Some(extra_ton));

        apply_command(&mut state, Command::OpenDenouncement).unwrap();
        apply_command(
            &mut state,
            Command::Nominate {
                voter: extra_ton,
                nominee: cult_leader,
            },
        )
        .unwrap();
        apply_command(&mut state, Command::CloseNomination).unwrap();
        apply_command(&mut state, Command::OpenBallot).unwrap();
        apply_command(
            &mut state,
            Command::CastBallot {
                voter: extra_ton,
                ballot: crate::denouncement::Ballot::For(cult_leader),
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
        assert!(state.martyrdom_triggered());

        let message = martyrdom_message_for(&state, king_queen).expect(
            "the original convert should still get the message despite losing the title slot",
        );
        assert!(message.contains("CultLeader"));
        assert_eq!(
            martyrdom_message_for(&state, extra_ton),
            None,
            "the new, unconverted title-holder must not get the message"
        );
    }
}
