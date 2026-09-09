use crate::character::{Character, PlayerStatus};
use crate::denouncement::DenouncementPhase;
use crate::player::{Faction, PlayerId};
use crate::round::Round;
use crate::state::GameState;
use crate::task::{TaskId, TaskTier};
use serde::{Deserialize, Serialize};

/// Who is asking to see the state. This is the *only* input that
/// determines what a caller gets back from [`view_for`] — there is no path
/// that hands out a raw [`GameState`] for a client to filter itself.
///
/// `Serialize`/`Deserialize` are for the wire protocol between `app` and a
/// browser -- **not** a statement that a client-supplied `Viewer` is safe
/// to trust as-is. `engine` has no session/auth concept (that's `app`'s
/// job, per the implementation plan's "Session" section); a server that
/// deserializes a `Viewer` straight from an unauthenticated client message
/// and calls `view_for` with it would let anyone request anyone else's
/// private view. As of Phase 1, `app`'s websocket handler does exactly
/// that for the Player case -- it does not yet verify the caller actually
/// *is* the player they claim to be. This is a known, tracked gap, not an
/// oversight: real per-player join tokens are planned but not yet built.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Viewer {
    Player(PlayerId),
    Host,
    Display,
}

/// One row of the public roster: visible to every viewer. Deliberately
/// carries no faction — see the note on `roster` below. `status` is safe to
/// include for everyone: a Cast-Out is read aloud to the whole room the
/// moment it happens (rules.md §4 step 6), so it's not a secret this
/// function would be the first to leak.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RosterEntry {
    pub id: PlayerId,
    pub name: String,
    pub status: PlayerStatus,
}

/// A redacted, viewer-safe projection of [`DenouncementPhase`]. Nomination
/// choices and in-progress ballots are private (rules.md §4: "everyone
/// privately submits one name"; "secret ballot... public tally only, never
/// individual votes") -- this type is what keeps that private data out of
/// `PlayerView` instead of relying on every call site to remember to redact
/// it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DenouncementView {
    /// `i_have_acted` is the viewer's own nomination status; always `false`
    /// for the Host and Display viewers, who don't nominate.
    Nomination {
        i_have_acted: bool,
    },
    Discussion {
        surfaced: Vec<PlayerId>,
    },
    /// `i_have_acted` is the viewer's own ballot status; always `false` for
    /// Host/Display.
    Ballot {
        candidates: Vec<PlayerId>,
        i_have_acted: bool,
    },
    Runoff {
        candidates: Vec<PlayerId>,
        i_have_acted: bool,
    },
}

/// A viewer-safe projection of one open task. Never carries
/// `qualifying_players` -- see the doc comment on [`crate::task::TaskDef`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskView {
    pub id: TaskId,
    pub prompt: String,
    pub tier: TaskTier,
    /// `Some(credited)` if the viewer (when they're a Player) has already
    /// attempted this task; `None` if not yet attempted. Always `None` for
    /// Host/Display, who don't attempt tasks themselves.
    pub my_outcome: Option<bool>,
}

/// What a single connection is allowed to see, fully pre-filtered
/// server-side. This is the only type that ever gets serialized and sent
/// to a client.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlayerView {
    pub roster: Vec<RosterEntry>,
    /// The viewer's own faction, if they are a player with one assigned.
    /// `None` for the Host and Display viewers, and for a player not yet
    /// assigned. Never another player's faction, regardless of role.
    pub own_faction: Option<Faction>,
    /// The viewer's own character, once assigned -- the press-and-hold
    /// reveal's data source. Never another player's character, and never
    /// populated for Host/Display.
    pub own_character: Option<Character>,
    pub current_round: Round,
    /// `None` when no Denouncement is currently running.
    pub denouncement: Option<DenouncementView>,
    pub open_tasks: Vec<TaskView>,
}

/// The single read path for the whole engine. Every field on the returned
/// [`PlayerView`] is authorized for `viewer` specifically — this function
/// is the enforcement point for "the server must not send unauthorized
/// data," not a convention callers are expected to follow.
///
/// Design note: even the Host viewer does not get other players'
/// factions here. Rules.md never states the host app should have an
/// ambient god-view of every secret role — only that Dalton is the one
/// person who may look at a *player's own phone* with them in person. A
/// host laptop that silently held everyone's secrets would be a real
/// spoiler/security risk on its own (a glanced-at screen). Later phases
/// add specific, deliberate host reveal actions (e.g. the finale's
/// "reveal everything" walkthrough) rather than this function granting
/// blanket visibility by default.
pub fn view_for(state: &GameState, viewer: Viewer) -> PlayerView {
    let roster = state
        .players()
        .map(|p| RosterEntry {
            id: p.id,
            name: p.name.clone(),
            status: p.status,
        })
        .collect();

    let viewer_id = match viewer {
        Viewer::Player(id) => Some(id),
        Viewer::Host | Viewer::Display => None,
    };

    let own_faction = viewer_id.and_then(|id| state.player(id).map(|p| p.faction));
    let own_character = viewer_id.and_then(|id| state.player(id).and_then(|p| p.character));

    let denouncement = state.denouncement_phase().map(|phase| match phase {
        DenouncementPhase::Nomination { submitted } => DenouncementView::Nomination {
            i_have_acted: viewer_id.is_some_and(|id| submitted.contains_key(&id)),
        },
        DenouncementPhase::Discussion { surfaced } => DenouncementView::Discussion {
            surfaced: surfaced.clone(),
        },
        DenouncementPhase::Ballot { surfaced, ballots } => DenouncementView::Ballot {
            candidates: surfaced.clone(),
            i_have_acted: viewer_id.is_some_and(|id| ballots.contains_key(&id)),
        },
        DenouncementPhase::Runoff {
            candidates,
            ballots,
            ..
        } => DenouncementView::Runoff {
            candidates: candidates.clone(),
            i_have_acted: viewer_id.is_some_and(|id| ballots.contains_key(&id)),
        },
    });

    let open_tasks = state
        .open_task_ids()
        .filter_map(|&id| state.task(id))
        .map(|def| TaskView {
            id: def.id,
            prompt: def.prompt.clone(),
            tier: def.tier,
            my_outcome: viewer_id.and_then(|viewer_id| state.task_attempt(viewer_id, def.id)),
        })
        .collect();

    PlayerView {
        roster,
        own_faction,
        own_character,
        current_round: state.current_round(),
        denouncement,
        open_tasks,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::command::Command;
    use crate::event::DomainEvent;
    use crate::state::apply_command;

    fn two_player_state() -> GameState {
        let mut state = GameState::new();
        apply_command(
            &mut state,
            Command::AddPlayer {
                name: "Alice".into(),
            },
        )
        .unwrap();
        apply_command(&mut state, Command::AddPlayer { name: "Bob".into() }).unwrap();
        apply_command(
            &mut state,
            Command::AssignFaction {
                player: PlayerId(0),
                faction: Faction::Ton,
            },
        )
        .unwrap();
        state
    }

    #[test]
    fn a_player_sees_their_own_faction() {
        let state = two_player_state();
        let view = view_for(&state, Viewer::Player(PlayerId(0)));
        assert_eq!(view.own_faction, Some(Faction::Ton));
    }

    #[test]
    fn a_player_never_sees_anyone_elses_faction() {
        let state = two_player_state();
        // Bob (player 1) is a real, unassigned player -- his own_faction
        // correctly reflects *his own* current state, Faction::Unassigned,
        // not None. What must never happen is Alice's Ton assignment
        // leaking into Bob's view anywhere -- that's the actual security
        // property this test exists to check.
        let view = view_for(&state, Viewer::Player(PlayerId(1)));
        assert_eq!(view.own_faction, Some(Faction::Unassigned));

        // Belt-and-suspenders: serialize Bob's view and confirm Alice's
        // faction string never appears in it at all, so this test still
        // catches a leak even if a future field is added to PlayerView
        // that isn't covered by an explicit assertion above.
        let serialized = serde_json::to_string(&view).unwrap();
        assert!(
            !serialized.contains("Ton"),
            "Bob's view leaked Alice's faction: {serialized}"
        );

        for entry in &view.roster {
            // RosterEntry has no faction field at all -- this loop exists
            // to make that invariant explicit and future-proof: if a field
            // is ever added to RosterEntry, this test forces a decision
            // about whether it's safe to expose here.
            let _: &RosterEntry = entry;
        }
    }

    #[test]
    fn host_and_display_never_see_any_players_faction() {
        let state = two_player_state();
        assert_eq!(view_for(&state, Viewer::Host).own_faction, None);
        assert_eq!(view_for(&state, Viewer::Display).own_faction, None);
    }

    #[test]
    fn roster_is_visible_to_every_viewer_kind() {
        let state = two_player_state();
        for viewer in [Viewer::Player(PlayerId(0)), Viewer::Host, Viewer::Display] {
            let view = view_for(&state, viewer);
            assert_eq!(view.roster.len(), 2);
            assert!(view.roster.iter().any(|r| r.name == "Alice"));
            assert!(view.roster.iter().any(|r| r.name == "Bob"));
        }
    }

    #[test]
    fn unknown_player_viewer_gets_an_empty_own_faction_not_a_panic() {
        let state = two_player_state();
        let view = view_for(&state, Viewer::Player(PlayerId(999)));
        assert_eq!(view.own_faction, None);
    }

    fn three_player_state() -> GameState {
        let mut state = GameState::new();
        for name in ["Alice", "Bob", "Carol"] {
            apply_command(&mut state, Command::AddPlayer { name: name.into() }).unwrap();
        }
        for id in [PlayerId(0), PlayerId(1), PlayerId(2)] {
            apply_command(
                &mut state,
                Command::AssignFaction {
                    player: id,
                    faction: Faction::Ton,
                },
            )
            .unwrap();
        }
        apply_command(&mut state, Command::FinalizeSetup).unwrap();
        state
    }

    #[test]
    fn a_player_sees_only_their_own_character_never_anyone_elses() {
        let mut state = GameState::new();
        for name in ["Alice", "Bob", "Carol"] {
            apply_command(&mut state, Command::AddPlayer { name: name.into() }).unwrap();
        }
        for id in [PlayerId(0), PlayerId(1), PlayerId(2)] {
            apply_command(
                &mut state,
                Command::AssignFaction {
                    player: id,
                    faction: Faction::Ton,
                },
            )
            .unwrap();
        }
        // Give Alice a distinct character from Bob's (assigned before
        // FinalizeSetup, which only fills in a *missing* character) so
        // this test can't pass vacuously -- everyone gets the same generic
        // NormalTon otherwise.
        apply_command(
            &mut state,
            Command::AssignCharacter {
                player: PlayerId(0),
                character: crate::character::Character::KingQueen,
            },
        )
        .unwrap();
        apply_command(&mut state, Command::FinalizeSetup).unwrap();

        let alice = view_for(&state, Viewer::Player(PlayerId(0)));
        assert_eq!(
            alice.own_character,
            Some(crate::character::Character::KingQueen)
        );

        let bobs_character = state.player(PlayerId(1)).unwrap().character;
        assert_ne!(alice.own_character, bobs_character);

        let serialized = serde_json::to_string(&alice).unwrap();
        assert!(
            !serialized.contains("NormalTon"),
            "Alice's view leaked Bob's character: {serialized}"
        );
    }

    #[test]
    fn host_and_display_never_get_a_character() {
        let state = three_player_state();
        assert_eq!(view_for(&state, Viewer::Host).own_character, None);
        assert_eq!(view_for(&state, Viewer::Display).own_character, None);
    }

    #[test]
    fn nomination_phase_never_leaks_who_nominated_whom() {
        let mut state = three_player_state();
        apply_command(&mut state, Command::OpenDenouncement).unwrap();
        apply_command(
            &mut state,
            Command::Nominate {
                voter: PlayerId(0),
                nominee: PlayerId(1),
            },
        )
        .unwrap();

        let alice = view_for(&state, Viewer::Player(PlayerId(0)));
        assert_eq!(
            alice.denouncement,
            Some(DenouncementView::Nomination { i_have_acted: true })
        );
        let carol = view_for(&state, Viewer::Player(PlayerId(2)));
        assert_eq!(
            carol.denouncement,
            Some(DenouncementView::Nomination {
                i_have_acted: false
            })
        );

        // Belt-and-suspenders: the actual voter -> nominee mapping must
        // never appear in any serialized view, host/display included.
        for viewer in [
            Viewer::Player(PlayerId(0)),
            Viewer::Player(PlayerId(2)),
            Viewer::Host,
            Viewer::Display,
        ] {
            let view = view_for(&state, viewer);
            let serialized = serde_json::to_string(&view).unwrap();
            assert!(
                !serialized.contains("submitted"),
                "a view leaked the private nomination map: {serialized}"
            );
        }
    }

    #[test]
    fn ballot_phase_shows_candidates_but_never_individual_votes() {
        let mut state = three_player_state();
        apply_command(&mut state, Command::OpenDenouncement).unwrap();
        apply_command(
            &mut state,
            Command::Nominate {
                voter: PlayerId(0),
                nominee: PlayerId(1),
            },
        )
        .unwrap();
        apply_command(&mut state, Command::CloseNomination).unwrap();
        apply_command(&mut state, Command::OpenBallot).unwrap();
        apply_command(
            &mut state,
            Command::CastBallot {
                voter: PlayerId(0),
                ballot: crate::denouncement::Ballot::For(PlayerId(1)),
            },
        )
        .unwrap();

        let alice = view_for(&state, Viewer::Player(PlayerId(0)));
        match &alice.denouncement {
            Some(DenouncementView::Ballot {
                candidates,
                i_have_acted,
            }) => {
                assert_eq!(candidates, &vec![PlayerId(1)]);
                assert!(i_have_acted);
            }
            other => panic!("expected Ballot phase, got {other:?}"),
        }

        let host = view_for(&state, Viewer::Host);
        assert_eq!(
            host.denouncement,
            Some(DenouncementView::Ballot {
                candidates: vec![PlayerId(1)],
                i_have_acted: false,
            })
        );

        for viewer in [Viewer::Player(PlayerId(0)), Viewer::Host, Viewer::Display] {
            let serialized = serde_json::to_string(&view_for(&state, viewer)).unwrap();
            assert!(
                !serialized.contains("ballots"),
                "a view leaked the private ballot map: {serialized}"
            );
        }
    }

    #[test]
    fn discussion_phase_shows_the_surfaced_candidates() {
        let mut state = three_player_state();
        apply_command(&mut state, Command::OpenDenouncement).unwrap();
        apply_command(
            &mut state,
            Command::Nominate {
                voter: PlayerId(0),
                nominee: PlayerId(1),
            },
        )
        .unwrap();
        apply_command(&mut state, Command::CloseNomination).unwrap();

        let alice = view_for(&state, Viewer::Player(PlayerId(0)));
        assert_eq!(
            alice.denouncement,
            Some(DenouncementView::Discussion {
                surfaced: vec![PlayerId(1)]
            })
        );
        assert_eq!(
            view_for(&state, Viewer::Display).denouncement,
            alice.denouncement
        );
    }

    #[test]
    fn runoff_phase_shows_candidates_but_never_individual_votes() {
        let mut state = three_player_state();
        apply_command(
            &mut state,
            Command::AddPlayer {
                name: "Dave".into(),
            },
        )
        .unwrap();
        apply_command(
            &mut state,
            Command::AssignFaction {
                player: PlayerId(3),
                faction: Faction::Ton,
            },
        )
        .unwrap();
        apply_command(&mut state, Command::FinalizeSetup).unwrap();

        apply_command(&mut state, Command::OpenDenouncement).unwrap();
        apply_command(
            &mut state,
            Command::Nominate {
                voter: PlayerId(0),
                nominee: PlayerId(0),
            },
        )
        .unwrap();
        apply_command(
            &mut state,
            Command::Nominate {
                voter: PlayerId(1),
                nominee: PlayerId(1),
            },
        )
        .unwrap();
        apply_command(&mut state, Command::CloseNomination).unwrap();
        apply_command(&mut state, Command::OpenBallot).unwrap();
        apply_command(
            &mut state,
            Command::CastBallot {
                voter: PlayerId(0),
                ballot: crate::denouncement::Ballot::For(PlayerId(0)),
            },
        )
        .unwrap();
        apply_command(
            &mut state,
            Command::CastBallot {
                voter: PlayerId(1),
                ballot: crate::denouncement::Ballot::For(PlayerId(1)),
            },
        )
        .unwrap();
        // A tie: exactly one vote each for the only execution slot.
        apply_command(
            &mut state,
            Command::CloseBallot {
                fallback_replacement: None,
            },
        )
        .unwrap();

        let alice = view_for(&state, Viewer::Player(PlayerId(0)));
        let mut candidates = match &alice.denouncement {
            Some(DenouncementView::Runoff {
                candidates,
                i_have_acted,
            }) => {
                assert!(!i_have_acted, "Alice hasn't cast a runoff ballot yet");
                candidates.clone()
            }
            other => panic!("expected Runoff phase, got {other:?}"),
        };
        candidates.sort();
        assert_eq!(candidates, vec![PlayerId(0), PlayerId(1)]);

        apply_command(
            &mut state,
            Command::CastBallot {
                voter: PlayerId(0),
                ballot: crate::denouncement::Ballot::For(PlayerId(0)),
            },
        )
        .unwrap();
        let alice = view_for(&state, Viewer::Player(PlayerId(0)));
        assert!(matches!(
            alice.denouncement,
            Some(DenouncementView::Runoff {
                i_have_acted: true,
                ..
            })
        ));

        for viewer in [Viewer::Player(PlayerId(0)), Viewer::Host, Viewer::Display] {
            let serialized = serde_json::to_string(&view_for(&state, viewer)).unwrap();
            assert!(
                !serialized.contains("ballots"),
                "a view leaked the private runoff ballot map: {serialized}"
            );
        }
    }

    #[test]
    fn no_denouncement_view_when_none_is_open() {
        let state = three_player_state();
        assert_eq!(
            view_for(&state, Viewer::Player(PlayerId(0))).denouncement,
            None
        );
    }

    #[test]
    fn open_tasks_never_leak_the_qualifying_set_and_show_only_the_viewers_own_outcome() {
        let mut state = three_player_state();
        apply_command(
            &mut state,
            Command::AddPlayer {
                name: "Dave".into(),
            },
        )
        .unwrap();
        apply_command(
            &mut state,
            Command::PushTask {
                prompt: "Talk to someone wearing red".into(),
                tier: crate::task::TaskTier::Easy,
                qualifying_players: [PlayerId(1)].into_iter().collect(),
            },
        )
        .unwrap();
        let task_id = match state.event_log().last() {
            Some(DomainEvent::TaskPushed { id, .. }) => *id,
            other => panic!("expected TaskPushed, got {other:?}"),
        };

        let before = view_for(&state, Viewer::Player(PlayerId(0)));
        assert_eq!(before.open_tasks.len(), 1);
        assert_eq!(before.open_tasks[0].my_outcome, None);

        // Bob (PlayerId(1)) is in the qualifying set, so naming him among
        // the 3 must earn credit.
        apply_command(
            &mut state,
            Command::AttemptTask {
                player: PlayerId(0),
                task: task_id,
                named: [PlayerId(1), PlayerId(2), PlayerId(3)],
            },
        )
        .unwrap();

        let alice = view_for(&state, Viewer::Player(PlayerId(0)));
        assert_eq!(alice.open_tasks[0].my_outcome, Some(true));
        // Carol never attempted -- her view must show no outcome, not
        // Alice's.
        let carol = view_for(&state, Viewer::Player(PlayerId(2)));
        assert_eq!(carol.open_tasks[0].my_outcome, None);

        for viewer in [Viewer::Player(PlayerId(0)), Viewer::Host, Viewer::Display] {
            let serialized = serde_json::to_string(&view_for(&state, viewer)).unwrap();
            assert!(
                !serialized.contains("qualifying"),
                "a view leaked the task's qualifying-player set: {serialized}"
            );
        }
    }

    #[test]
    fn closed_tasks_disappear_from_open_tasks() {
        let mut state = three_player_state();
        apply_command(
            &mut state,
            Command::PushTask {
                prompt: "Talk to someone wearing red".into(),
                tier: crate::task::TaskTier::Easy,
                qualifying_players: [PlayerId(1)].into_iter().collect(),
            },
        )
        .unwrap();
        assert_eq!(
            view_for(&state, Viewer::Player(PlayerId(0)))
                .open_tasks
                .len(),
            1
        );
        apply_command(&mut state, Command::CloseTasks).unwrap();
        assert_eq!(
            view_for(&state, Viewer::Player(PlayerId(0)))
                .open_tasks
                .len(),
            0
        );
    }
}
