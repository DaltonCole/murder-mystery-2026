use crate::denouncement::Ballot;
use crate::event::DomainEvent;
use crate::player::PlayerId;
use crate::round::Round;
use crate::state::GameState;
use crate::task::TaskTier;
use serde::{Deserialize, Serialize};

/// One closed task's outcome for a specific player -- Dalton's own
/// explicit instruction: a player's history tab should show every task's
/// outcome, including one they never attempted at all before it closed
/// (rules.md/Dalton's own framing: "any incomplete tasks are considered
/// failed tasks," matching how `ton_met_task_threshold` already treats a
/// never-attempted task the same as an explicitly-failed one).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TaskOutcome {
    /// Attempted, and credited (one of the 3 named players was in the
    /// task's real qualifying set).
    Completed,
    /// Attempted, but not credited (named 3 real people, none of whom
    /// actually qualified -- or a wrong location code).
    NoMatch,
    /// The task closed before this player ever attempted it.
    Failed,
}

/// One closed task's outcome, as it belongs on a specific player's own
/// history tab -- see `task_history`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskHistoryEntry {
    pub prompt: String,
    pub tier: TaskTier,
    pub outcome: TaskOutcome,
}

/// `player`'s own outcome on every task that has ever *closed* -- a
/// currently-open task isn't history yet, it's still in `PlayerView::
/// open_tasks`. A pure projection over `GameState`'s task/attempt records,
/// the same "cheap to recompute on every read" shape as `whistledown::
/// posts`.
pub fn task_history(state: &GameState, player: PlayerId) -> Vec<TaskHistoryEntry> {
    state
        .tasks()
        .filter(|def| !state.is_task_open(def.id))
        .map(|def| {
            let outcome = match state.task_attempt(player, def.id) {
                Some(true) => TaskOutcome::Completed,
                Some(false) => TaskOutcome::NoMatch,
                None => TaskOutcome::Failed,
            };
            TaskHistoryEntry {
                prompt: def.prompt.clone(),
                tier: def.tier,
                outcome,
            }
        })
        .collect()
}

/// One round's Denouncement activity for a specific player -- `nominated`
/// and `ballot` are `None` if the player didn't nominate/vote that round
/// (drunk, Cast Out before it opened, or simply chose not to), not if the
/// round had no Denouncement at all (Rounds 1/2/4 never produce an entry
/// here in the first place -- see `round_history`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoundHistoryEntry {
    pub round: Round,
    pub nominated: Option<PlayerId>,
    pub ballot: Option<Ballot>,
}

/// `player`'s own nomination and ballot for every round that actually had
/// a Denouncement (Round 3, Round 5, and the Finale's Last Denouncement) --
/// one entry per such round, always present once that round's Denouncement
/// has opened, even if `player` never acted in it. A pure projection over
/// `GameState::event_log`, tracking the current round the same
/// `RoundAdvanced`-driven way `whistledown::posts` does; a runoff's
/// `BallotCast` (same event shape as the main Ballot phase -- there's no
/// separate variant) simply overwrites `ballot` with the player's final
/// choice for that round, which is what actually decided their fate.
pub fn round_history(state: &GameState, player: PlayerId) -> Vec<RoundHistoryEntry> {
    let mut entries = Vec::new();
    let mut round = Round::One;
    let mut nominated: Option<PlayerId> = None;
    let mut ballot: Option<Ballot> = None;
    let mut denouncement_this_round = false;

    for event in state.event_log() {
        match event {
            DomainEvent::DenouncementOpened => denouncement_this_round = true,
            DomainEvent::NominationCast { voter, nominee } if *voter == player => {
                nominated = Some(*nominee);
            }
            DomainEvent::BallotCast {
                voter,
                ballot: cast,
            } if *voter == player => {
                ballot = Some(*cast);
            }
            DomainEvent::RoundAdvanced { round: next } => {
                if denouncement_this_round {
                    entries.push(RoundHistoryEntry {
                        round,
                        nominated,
                        ballot,
                    });
                }
                round = *next;
                nominated = None;
                ballot = None;
                denouncement_this_round = false;
            }
            _ => {}
        }
    }
    // The Finale's Last Denouncement never gets a trailing `RoundAdvanced`
    // (there's no round after it) -- flush whatever's pending.
    if denouncement_this_round {
        entries.push(RoundHistoryEntry {
            round,
            nominated,
            ballot,
        });
    }

    entries
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::character::Character;
    use crate::command::Command;
    use crate::player::Faction;
    use crate::state::apply_command;
    use crate::task::TaskTier as Tier;

    fn add_player(state: &mut GameState, name: &str, faction: Faction) -> PlayerId {
        let events = apply_command(
            state,
            Command::AddPlayer {
                name: name.to_string(),
            },
        )
        .unwrap();
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
    fn task_history_covers_completed_no_match_and_never_attempted() {
        let mut state = GameState::new();
        let alice = add_player(&mut state, "Alice", Faction::Ton);
        let bob = add_player(&mut state, "Bob", Faction::Ton);
        let carol = add_player(&mut state, "Carol", Faction::Ton);
        let dave = add_player(&mut state, "Dave", Faction::Ton);
        let eve = add_player(&mut state, "Eve", Faction::Ton);
        // Give Alice a named character so `FinalizeSetup` doesn't stamp her
        // as the catch-all `NormalTon`, whose once-per-game task
        // auto-succeed would otherwise silently credit her "No-match one"
        // attempt below regardless of who she actually names.
        apply_command(
            &mut state,
            Command::AssignCharacter {
                player: alice,
                character: Character::KingQueen,
            },
        )
        .unwrap();
        apply_command(&mut state, Command::FinalizeSetup).unwrap();

        apply_command(
            &mut state,
            Command::PushTask {
                prompt: "Completed one".into(),
                tier: Tier::Easy,
                qualifying_players: [bob].into_iter().collect(),
                expected_code: None,
            },
        )
        .unwrap();
        apply_command(
            &mut state,
            Command::PushTask {
                prompt: "No-match one".into(),
                tier: Tier::Medium,
                qualifying_players: [dave].into_iter().collect(),
                expected_code: None,
            },
        )
        .unwrap();
        apply_command(
            &mut state,
            Command::PushTask {
                prompt: "Never attempted".into(),
                tier: Tier::Hard,
                qualifying_players: [dave].into_iter().collect(),
                expected_code: None,
            },
        )
        .unwrap();

        // Alice completes the first (Bob really qualifies).
        apply_command(
            &mut state,
            Command::AttemptTask {
                player: alice,
                task: crate::task::TaskId(0),
                named: [bob, carol, dave],
            },
        )
        .unwrap();
        // Alice attempts the second but names nobody who actually
        // qualifies (Dave does, but isn't named here).
        apply_command(
            &mut state,
            Command::AttemptTask {
                player: alice,
                task: crate::task::TaskId(1),
                named: [bob, carol, eve],
            },
        )
        .unwrap();
        // Alice never attempts the third at all.

        // Still open: no history entries yet.
        assert_eq!(task_history(&state, alice), Vec::new());

        apply_command(&mut state, Command::CloseTasks).unwrap();

        let history = task_history(&state, alice);
        assert_eq!(history.len(), 3);
        assert_eq!(history[0].prompt, "Completed one");
        assert_eq!(history[0].outcome, TaskOutcome::Completed);
        assert_eq!(history[1].prompt, "No-match one");
        assert_eq!(history[1].outcome, TaskOutcome::NoMatch);
        assert_eq!(history[2].prompt, "Never attempted");
        assert_eq!(history[2].outcome, TaskOutcome::Failed);
    }

    fn setup_denouncement_game() -> (GameState, PlayerId, PlayerId, PlayerId) {
        let mut state = GameState::new();
        let alice = add_player(&mut state, "Alice", Faction::Ton);
        let bob = add_player(&mut state, "Bob", Faction::Ton);
        let carol = add_player(&mut state, "Carol", Faction::Ton);
        apply_command(&mut state, Command::FinalizeSetup).unwrap();
        (state, alice, bob, carol)
    }

    #[test]
    fn round_history_records_only_denouncement_rounds_and_only_this_players_own_choices() {
        let (mut state, alice, bob, _carol) = setup_denouncement_game();

        // Round 1 has no Denouncement -- advancing past it must not create
        // an entry at all.
        apply_command(&mut state, Command::AdvanceRound).unwrap(); // -> Two
        apply_command(&mut state, Command::AdvanceRound).unwrap(); // -> Three

        apply_command(&mut state, Command::OpenDenouncement).unwrap();
        apply_command(
            &mut state,
            Command::Nominate {
                voter: alice,
                nominee: bob,
            },
        )
        .unwrap();
        apply_command(&mut state, Command::CloseNomination).unwrap();
        apply_command(&mut state, Command::OpenBallot).unwrap();
        apply_command(
            &mut state,
            Command::CastBallot {
                voter: alice,
                ballot: Ballot::For(bob),
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

        apply_command(&mut state, Command::AdvanceRound).unwrap(); // -> Four

        let history = round_history(&state, alice);
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].round, Round::Three);
        assert_eq!(history[0].nominated, Some(bob));
        assert_eq!(history[0].ballot, Some(Ballot::For(bob)));

        // Someone who neither nominated nor voted still gets a row (a
        // Denouncement really did happen that round), just an empty one.
        let carol_history = round_history(&state, _carol);
        assert_eq!(carol_history.len(), 1);
        assert_eq!(carol_history[0].round, Round::Three);
        assert_eq!(carol_history[0].nominated, None);
        assert_eq!(carol_history[0].ballot, None);
    }

    #[test]
    fn round_history_includes_the_finale_with_no_trailing_round_advanced() {
        let (mut state, alice, bob, _carol) = setup_denouncement_game();
        for _ in 0..5 {
            apply_command(&mut state, Command::AdvanceRound).unwrap();
        }
        assert_eq!(state.current_round(), Round::Finale);

        apply_command(&mut state, Command::OpenDenouncement).unwrap();
        apply_command(
            &mut state,
            Command::Nominate {
                voter: alice,
                nominee: bob,
            },
        )
        .unwrap();

        let history = round_history(&state, alice);
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].round, Round::Finale);
        assert_eq!(history[0].nominated, Some(bob));
        assert_eq!(history[0].ballot, None);
    }

    #[test]
    fn round_history_uses_the_final_ballot_when_a_runoff_overwrites_the_first_one() {
        // Force a 3-way tie at the top so a runoff actually happens: 5
        // players each nominate a distinct target so nobody surfaces
        // outright, then arrange a tied ballot. Simpler: directly drive
        // Nomination -> Ballot -> tie -> Runoff and cast two different
        // ballots for the same voter across the two phases, confirming the
        // later one wins in the history.
        let mut state = GameState::new();
        let alice = add_player(&mut state, "Alice", Faction::Ton);
        let bob = add_player(&mut state, "Bob", Faction::Ton);
        let carol = add_player(&mut state, "Carol", Faction::Ton);
        let dave = add_player(&mut state, "Dave", Faction::Ton);
        apply_command(&mut state, Command::FinalizeSetup).unwrap();
        apply_command(&mut state, Command::AdvanceRound).unwrap();
        apply_command(&mut state, Command::AdvanceRound).unwrap();

        apply_command(&mut state, Command::OpenDenouncement).unwrap();
        for (voter, nominee) in [(alice, bob), (bob, carol), (carol, dave), (dave, bob)] {
            apply_command(&mut state, Command::Nominate { voter, nominee }).unwrap();
        }
        apply_command(&mut state, Command::CloseNomination).unwrap();
        apply_command(&mut state, Command::OpenBallot).unwrap();
        // A tie between Bob and Carol forces a runoff.
        apply_command(
            &mut state,
            Command::CastBallot {
                voter: alice,
                ballot: Ballot::For(bob),
            },
        )
        .unwrap();
        apply_command(
            &mut state,
            Command::CastBallot {
                voter: bob,
                ballot: Ballot::For(carol),
            },
        )
        .unwrap();
        apply_command(
            &mut state,
            Command::CastBallot {
                voter: carol,
                ballot: Ballot::For(bob),
            },
        )
        .unwrap();
        apply_command(
            &mut state,
            Command::CastBallot {
                voter: dave,
                ballot: Ballot::For(carol),
            },
        )
        .unwrap();
        let close_events = apply_command(
            &mut state,
            Command::CloseBallot {
                fallback_replacement: None,
            },
        )
        .unwrap();
        let runoff_opened = close_events
            .iter()
            .any(|e| matches!(e, DomainEvent::RunoffOpened { .. }));
        assert!(runoff_opened, "expected a tie to force a runoff");

        // Alice voted For(Bob) in the main ballot; now she votes
        // differently in the runoff.
        apply_command(
            &mut state,
            Command::CastBallot {
                voter: alice,
                ballot: Ballot::Abstain,
            },
        )
        .unwrap();

        let history = round_history(&state, alice);
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].ballot, Some(Ballot::Abstain));
    }
}
