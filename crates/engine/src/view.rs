use crate::ability::{AbilityStatus, InfoCheckDelivery};
use crate::bio::{Bio, TaskCandidate};
use crate::character::{Character, PlayerStatus};
use crate::contest::ContestCategory;
use crate::denouncement::DenouncementPhase;
use crate::finale_reveal::{self, FinaleReveal, PlayerReveal};
use crate::player::{Faction, PlayerId};
use crate::round::Round;
use crate::state::GameState;
use crate::task::{TaskId, TaskTier};
use crate::whistledown::WhistledownPost;
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

    /// What the viewer's own current character can do right now -- every
    /// field empty/default for Host/Display and for a player with no
    /// ability-bearing character. See `ability::AbilityStatus`.
    pub my_abilities: AbilityStatus,
    /// Every info-check ever delivered *to this viewer specifically* --
    /// never another player's results, falsified or not (see
    /// `state::GameState::info_checks_for`).
    pub my_info_checks: Vec<InfoCheckDelivery>,
    /// The Cultist/Deceiver passive-knowledge feed -- empty unless the
    /// viewer's own character actually grants it (see
    /// `state::GameState::fellow_cultists_for`'s doc comment on why the
    /// Cult Leader is deliberately excluded).
    pub fellow_cultists: Vec<PlayerId>,
    /// The Cell Leader's passive knowledge -- empty for every other viewer.
    pub known_uprising_members: Vec<PlayerId>,
    /// Whether the viewer is currently drunk (rules.md §3.2: "the target
    /// is told if drunk"). Always `false` for Host/Display, and never
    /// exposed for anyone but the viewer themselves -- the Bartender isn't
    /// told whether their target actually landed drunk.
    pub i_am_drunk: bool,
    /// The Revolutionary Leader's own view of who currently knows their
    /// identity (rules.md §3.2, the Leader's Confidants) -- empty for
    /// every other viewer, including a Confidant themselves (they see
    /// `revealed_leader` instead, not this list).
    pub my_confidants: Vec<PlayerId>,
    /// The Revolutionary Leader's identity, if the viewer has been
    /// revealed to as one of their Confidants -- `None` for everyone else,
    /// including the Leader's own view of themselves.
    pub revealed_leader: Option<PlayerId>,
    /// Whether the viewer has opted into the Intermission lottery -- only
    /// ever the viewer's own status, never anyone else's. Always `false`
    /// for Host/Display.
    pub i_opted_into_intermission: bool,
    /// The drawn Intermission entrants, once drawn -- `None` until then.
    /// Unlike the opt-in pool, this is public once it exists (rules.md §4
    /// frames the draw as a live, shared party moment), so every viewer
    /// kind gets the same answer.
    pub intermission_entrants: Option<Vec<PlayerId>>,
    /// The Servant leaderboard (rules.md §5/§7), highest-first -- public
    /// to every viewer kind, unlike anything faction/character related.
    /// Gallery predictions themselves are never exposed here or anywhere
    /// else -- only the resulting point awards, once resolved.
    pub servant_leaderboard: Vec<(PlayerId, u32)>,
    /// Every contest result recorded so far -- `Viewer::Host` ONLY, always
    /// empty for `Viewer::Player`/`Viewer::Display` (rules.md: players
    /// never learn the standings or the breakdown, even after the round
    /// ends). Gives the host a self-audit view before recording another
    /// result, since `RecordContestResult` has no correction command.
    pub contest_results: Vec<((Round, ContestCategory), bool)>,
    /// The one faction currently winning, if any -- `Viewer::Host` ONLY,
    /// always `None` for `Viewer::Player`/`Viewer::Display` (see
    /// `GameState::winner_for_host`'s doc comment: public finale-reveal
    /// sequencing is deferred to a later phase). `ResolveGalleryPredictions`
    /// needs this to score `FactionWins` predictions against a real answer.
    pub winner: Option<Faction>,
    /// Whether `ResolveGalleryPredictions` has already run -- `Viewer::Host`
    /// ONLY, always `false` for `Viewer::Player`/`Viewer::Display`. See
    /// `GameState::gallery_resolved`'s doc comment.
    pub gallery_resolved: bool,
    /// Lady Whistledown's posts so far (rules.md §6), one per completed
    /// round -- public to every viewer kind, unlike anything faction- or
    /// character-related. See `whistledown::posts`'s doc comment for why
    /// this deliberately never covers the Finale.
    pub whistledown: Vec<WhistledownPost>,
    /// The viewer's own bio (rules.md §1), once submitted -- never another
    /// player's, and never populated for Host/Display. A bio isn't a
    /// secret (it feeds the public task pool), but there's no reason to
    /// hand a client anyone else's raw bio when the task pool -- see
    /// `task_candidates` below -- is the only thing that actually needs
    /// to surface bio content to other people.
    pub own_bio: Option<Bio>,
    /// The viewer's own signup interest rating (rules.md §1), once
    /// submitted -- never another player's, and never populated for
    /// Host/Display. Lets `/play` show a player what they rated themselves
    /// before the raffle runs.
    pub own_interest_level: Option<u8>,
    /// Every player's signup interest rating submitted so far, as
    /// `(PlayerId, level)` pairs -- `Viewer::Host` ONLY, always empty for
    /// `Viewer::Player`/`Viewer::Display`. This is the raw input the Host
    /// needs to actually run the setup raffle (`crate::raffle`): compute
    /// tickets, draw winners, then commit them via `AssignCharacter`. Not
    /// exposed to players -- rules.md never asks for interest ratings to be
    /// public, and revealing them would tip off who's angling for which
    /// role before the raffle even runs.
    pub interest_levels: Vec<(PlayerId, u8)>,
    /// The bio-derived task pool (`bio::task_candidates`), one list per
    /// tier -- `Viewer::Host` ONLY. Round 1's "exactly 2 fixed tasks"
    /// (rules.md §4) don't come from here; this is for Rounds 3/5's
    /// "easy/medium/hard tiers live." Always empty for
    /// `Viewer::Player`/`Viewer::Display`, the same scoping as
    /// `contest_results` -- a client-visible pool would spoil the
    /// mingling the task itself is supposed to require.
    pub task_candidates: Vec<(TaskTier, Vec<TaskCandidate>)>,
    /// The full post-Finale walkthrough (rules.md §4) -- `Viewer::Host`
    /// ONLY, and `None` until the Last Denouncement has actually closed.
    /// See `finale_reveal::reveal`'s doc comment: this is the one
    /// deliberate exception to "no ambient god-view," not a relaxation of
    /// it generally.
    pub finale_reveal: Option<FinaleReveal>,
    /// The full identities of whoever was actually Cast Out at the Last
    /// Denouncement -- unlike `finale_reveal` above, this is public to
    /// every viewer kind (rules.md §4: "revealed publicly on a shared
    /// screen"), and only ever this specific subset, not everyone's.
    /// Empty until the Finale's own Denouncement closes.
    pub finale_cast_out_reveal: Vec<PlayerReveal>,
    /// rules.md §6's martyrdom message -- the viewer's own only, `None`
    /// unless they're the currently-converted title-holder who triggered
    /// it. See `finale_reveal::martyrdom_message_for`'s doc comment: unlike
    /// `finale_reveal` above, this isn't gated to the Finale at all.
    pub martyrdom_message: Option<String>,
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

    let my_abilities = viewer_id
        .map(|id| state.ability_status_for(id))
        .unwrap_or_default();
    let my_info_checks = viewer_id
        .map(|id| state.info_checks_for(id))
        .unwrap_or_default();
    let fellow_cultists = viewer_id
        .map(|id| state.fellow_cultists_for(id))
        .unwrap_or_default();
    let known_uprising_members = viewer_id
        .filter(|&id| {
            state
                .player(id)
                .is_some_and(|p| p.character == Some(Character::CellLeader))
        })
        .map(|_| state.cell_leader_knows().to_vec())
        .unwrap_or_default();
    let i_am_drunk = viewer_id.is_some_and(|id| state.is_drunk(id));
    let my_confidants = viewer_id
        .map(|id| state.confidants_known_to_leader(id))
        .unwrap_or_default();
    let revealed_leader = viewer_id.and_then(|id| state.leader_known_to(id));
    let i_opted_into_intermission = viewer_id.is_some_and(|id| state.opted_into_intermission(id));
    let intermission_entrants = state.intermission_entrants().map(|e| e.to_vec());
    let contest_results = if matches!(viewer, Viewer::Host) {
        state.contest_results_for_host()
    } else {
        Vec::new()
    };
    let winner = if matches!(viewer, Viewer::Host) {
        state.winner_for_host()
    } else {
        None
    };
    let gallery_resolved = matches!(viewer, Viewer::Host) && state.gallery_resolved();
    let own_bio = viewer_id.and_then(|id| state.bio(id)).cloned();
    let own_interest_level = viewer_id.and_then(|id| state.interest_level(id));
    let interest_levels = if matches!(viewer, Viewer::Host) {
        state.interest_levels().collect()
    } else {
        Vec::new()
    };
    let task_candidates = if matches!(viewer, Viewer::Host) {
        [TaskTier::Easy, TaskTier::Medium, TaskTier::Hard]
            .into_iter()
            .map(|tier| (tier, crate::bio::task_candidates(state, tier)))
            .collect()
    } else {
        Vec::new()
    };
    let finale_reveal_data = finale_reveal::reveal(state);
    let finale_reveal_for_host = if matches!(viewer, Viewer::Host) {
        finale_reveal_data.clone()
    } else {
        None
    };
    let finale_cast_out_reveal = finale_reveal_data
        .map(|r| {
            r.everyone
                .into_iter()
                .filter(|p| r.cast_out_this_denouncement.contains(&p.id))
                .collect()
        })
        .unwrap_or_default();
    let martyrdom_message =
        viewer_id.and_then(|id| finale_reveal::martyrdom_message_for(state, id));

    PlayerView {
        roster,
        own_faction,
        own_character,
        current_round: state.current_round(),
        denouncement,
        open_tasks,
        my_abilities,
        my_info_checks,
        fellow_cultists,
        known_uprising_members,
        i_am_drunk,
        my_confidants,
        revealed_leader,
        i_opted_into_intermission,
        intermission_entrants,
        servant_leaderboard: state.servant_leaderboard(),
        contest_results,
        winner,
        gallery_resolved,
        whistledown: crate::whistledown::posts(state),
        own_bio,
        own_interest_level,
        interest_levels,
        task_candidates,
        finale_reveal: finale_reveal_for_host,
        finale_cast_out_reveal,
        martyrdom_message,
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

    // --- Phase 2 ---

    fn phase2_state() -> (GameState, PlayerId, PlayerId, PlayerId, PlayerId, PlayerId) {
        let mut state = GameState::new();
        let new_player = |state: &mut GameState, name: &str, faction: Faction| -> PlayerId {
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
        };

        let oracle = new_player(&mut state, "Oracle", Faction::Ton);
        apply_command(
            &mut state,
            Command::AssignCharacter {
                player: oracle,
                character: Character::Oracle,
            },
        )
        .unwrap();
        let king_queen = new_player(&mut state, "King", Faction::Ton);
        apply_command(
            &mut state,
            Command::AssignCharacter {
                player: king_queen,
                character: Character::KingQueen,
            },
        )
        .unwrap();
        let leader = new_player(&mut state, "Leader", Faction::Uprising);
        apply_command(
            &mut state,
            Command::AssignCharacter {
                player: leader,
                character: Character::RevolutionaryLeader,
            },
        )
        .unwrap();
        let cell_leader = new_player(&mut state, "CellLeader", Faction::Uprising);
        apply_command(
            &mut state,
            Command::AssignCharacter {
                player: cell_leader,
                character: Character::CellLeader,
            },
        )
        .unwrap();
        let cult_leader = new_player(&mut state, "CultLeader", Faction::Cult);
        apply_command(
            &mut state,
            Command::AssignCharacter {
                player: cult_leader,
                character: Character::CultLeader,
            },
        )
        .unwrap();
        let cultist = new_player(&mut state, "Cultist", Faction::Cult);
        // A spare, plain Uprising member with no assigned character so
        // `cell_leader_knows` has someone besides the Leader (excluded)
        // and the Cell Leader themself (also excluded) to actually learn.
        new_player(&mut state, "NormalUprising", Faction::Uprising);
        apply_command(&mut state, Command::FinalizeSetup).unwrap();

        (state, oracle, king_queen, leader, cell_leader, cultist)
    }

    #[test]
    fn recruitment_slots_available_reflects_open_windows_for_the_cult_leader_only() {
        let mut state = GameState::new();
        let new_player = |state: &mut GameState, name: &str, faction: Faction| -> PlayerId {
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
        };
        let cult_leader = new_player(&mut state, "CultLeader", Faction::Cult);
        apply_command(
            &mut state,
            Command::AssignCharacter {
                player: cult_leader,
                character: Character::CultLeader,
            },
        )
        .unwrap();
        let bystander = new_player(&mut state, "Bystander", Faction::Ton);
        apply_command(&mut state, Command::FinalizeSetup).unwrap();

        assert_eq!(
            view_for(&state, Viewer::Player(cult_leader))
                .my_abilities
                .recruitment_slots_available,
            Some(0)
        );

        apply_command(&mut state, Command::AdvanceRound).unwrap(); // -> Two, opens a window

        assert_eq!(
            view_for(&state, Viewer::Player(cult_leader))
                .my_abilities
                .recruitment_slots_available,
            Some(1)
        );
        // Nobody else ever sees this -- not even the Host.
        assert_eq!(
            view_for(&state, Viewer::Player(bystander))
                .my_abilities
                .recruitment_slots_available,
            None
        );
        assert_eq!(
            view_for(&state, Viewer::Host)
                .my_abilities
                .recruitment_slots_available,
            None
        );
    }

    #[test]
    fn own_ability_status_reflects_only_the_viewers_own_character() {
        let (mut state, oracle, king_queen, ..) = phase2_state();
        apply_command(&mut state, Command::AdvanceRound).unwrap();

        let oracle_view = view_for(&state, Viewer::Player(oracle));
        assert!(oracle_view.my_abilities.oracle_checks_available.is_some());

        // King/Queen has no ability-bearing character -- every field stays
        // empty, including `oracle_checks_available`, even though a real
        // Oracle exists elsewhere in the game.
        let king_view = view_for(&state, Viewer::Player(king_queen));
        assert_eq!(king_view.my_abilities, AbilityStatus::default());

        assert_eq!(
            view_for(&state, Viewer::Host).my_abilities,
            AbilityStatus::default()
        );
        assert_eq!(
            view_for(&state, Viewer::Display).my_abilities,
            AbilityStatus::default()
        );
    }

    #[test]
    fn phase_3_procedural_modifier_ability_status_is_also_scoped_to_the_holder() {
        let mut state = GameState::new();
        let new_player = |state: &mut GameState, name: &str, faction: Faction| -> PlayerId {
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
        };

        let duelist = new_player(&mut state, "Duelist", Faction::Ton);
        apply_command(
            &mut state,
            Command::AssignCharacter {
                player: duelist,
                character: Character::Duelist,
            },
        )
        .unwrap();
        let bystander = new_player(&mut state, "Bystander", Faction::Ton);
        apply_command(&mut state, Command::FinalizeSetup).unwrap();

        let duelist_view = view_for(&state, Viewer::Player(duelist));
        assert_eq!(duelist_view.my_abilities.duelist_available, Some(true));

        // Nobody else -- not another player, not Host/Display -- ever sees
        // this reflected in their own view.
        let bystander_view = view_for(&state, Viewer::Player(bystander));
        assert_eq!(bystander_view.my_abilities, AbilityStatus::default());
        assert_eq!(
            view_for(&state, Viewer::Host).my_abilities,
            AbilityStatus::default()
        );
    }

    #[test]
    fn my_info_checks_never_leaks_into_another_players_view() {
        let (mut state, oracle, king_queen, ..) = phase2_state();
        apply_command(&mut state, Command::AdvanceRound).unwrap();
        apply_command(
            &mut state,
            Command::UseOracle {
                player: oracle,
                target: king_queen,
            },
        )
        .unwrap();

        let oracle_view = view_for(&state, Viewer::Player(oracle));
        assert_eq!(oracle_view.my_info_checks.len(), 1);
        assert_eq!(oracle_view.my_info_checks[0].querier, oracle);

        // Nobody else -- including the target themself -- ever sees this
        // result in their own view.
        let king_view = view_for(&state, Viewer::Player(king_queen));
        assert!(king_view.my_info_checks.is_empty());

        for viewer in [Viewer::Player(king_queen), Viewer::Host, Viewer::Display] {
            let serialized = serde_json::to_string(&view_for(&state, viewer)).unwrap();
            assert!(
                !serialized.contains("FullHistory"),
                "a non-querier's view leaked the Oracle's result: {serialized}"
            );
        }
    }

    #[test]
    fn fellow_cultists_only_visible_to_cult_members_who_get_that_passive() {
        let (state, _oracle, king_queen, _leader, _cell_leader, cultist) = phase2_state();
        let cult_leader = state
            .players()
            .find(|p| p.character == Some(Character::CultLeader))
            .unwrap()
            .id;

        let cultist_view = view_for(&state, Viewer::Player(cultist));
        assert!(cultist_view.fellow_cultists.contains(&cult_leader));
        assert!(!cultist_view.fellow_cultists.contains(&cultist));

        // The Cult Leader's own row in rules.md's ability table lists no
        // such passive -- see `state::GameState::fellow_cultists_for`.
        let cult_leader_view = view_for(&state, Viewer::Player(cult_leader));
        assert!(cult_leader_view.fellow_cultists.is_empty());

        let king_view = view_for(&state, Viewer::Player(king_queen));
        assert!(king_view.fellow_cultists.is_empty());
    }

    #[test]
    fn cell_leader_sees_known_uprising_members_nobody_else_does() {
        let (state, _oracle, king_queen, leader, cell_leader, _cultist) = phase2_state();

        let cell_leader_view = view_for(&state, Viewer::Player(cell_leader));
        assert_eq!(cell_leader_view.known_uprising_members.len(), 1);
        assert!(!cell_leader_view.known_uprising_members.contains(&leader));

        let leader_view = view_for(&state, Viewer::Player(leader));
        assert!(leader_view.known_uprising_members.is_empty());

        let king_view = view_for(&state, Viewer::Player(king_queen));
        assert!(king_view.known_uprising_members.is_empty());
    }

    #[test]
    fn drunk_status_is_told_only_to_the_drunk_player_themself() {
        let mut state = GameState::new();
        let new_player = |state: &mut GameState, name: &str, faction: Faction| -> PlayerId {
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
        };

        let bartender = new_player(&mut state, "Bartender", Faction::Uprising);
        apply_command(
            &mut state,
            Command::AssignCharacter {
                player: bartender,
                character: Character::Bartender,
            },
        )
        .unwrap();
        let target = new_player(&mut state, "Target", Faction::Ton);
        apply_command(&mut state, Command::FinalizeSetup).unwrap();

        apply_command(
            &mut state,
            Command::BartenderTarget {
                player: bartender,
                target,
                lands: true,
            },
        )
        .unwrap();

        assert!(view_for(&state, Viewer::Player(target)).i_am_drunk);
        // Nobody else -- not the Bartender, not another player, not
        // Host/Display -- ever sees this in their own view.
        assert!(!view_for(&state, Viewer::Player(bartender)).i_am_drunk);
        assert!(!view_for(&state, Viewer::Host).i_am_drunk);
        assert!(!view_for(&state, Viewer::Display).i_am_drunk);
    }

    #[test]
    fn leader_confidant_reveal_is_scoped_to_the_leader_and_the_confidant_only() {
        let mut state = GameState::new();
        let new_player = |state: &mut GameState, name: &str, faction: Faction| -> PlayerId {
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
        };

        let leader = new_player(&mut state, "Leader", Faction::Uprising);
        apply_command(
            &mut state,
            Command::AssignCharacter {
                player: leader,
                character: Character::RevolutionaryLeader,
            },
        )
        .unwrap();
        let confidant = new_player(&mut state, "Confidant", Faction::Uprising);
        let bystander = new_player(&mut state, "Bystander", Faction::Uprising);
        apply_command(&mut state, Command::FinalizeSetup).unwrap();
        apply_command(&mut state, Command::AdvanceRound).unwrap(); // -> Two

        apply_command(
            &mut state,
            Command::RecordContestResult {
                round: Round::Two,
                category: crate::ContestCategory::Strength,
                ton_won: false,
            },
        )
        .unwrap();

        // The Leader sees who now knows them.
        let leader_view = view_for(&state, Viewer::Player(leader));
        assert_eq!(leader_view.my_confidants, vec![confidant]);
        assert_eq!(leader_view.revealed_leader, None);

        // The Confidant learns the Leader's identity -- but doesn't get
        // the Leader's own "who knows me" list.
        let confidant_view = view_for(&state, Viewer::Player(confidant));
        assert_eq!(confidant_view.revealed_leader, Some(leader));
        assert!(confidant_view.my_confidants.is_empty());

        // Nobody else -- not an uninvolved Uprising member, not Host,
        // not Display -- learns anything from either field.
        let bystander_view = view_for(&state, Viewer::Player(bystander));
        assert_eq!(bystander_view.revealed_leader, None);
        assert!(bystander_view.my_confidants.is_empty());
        assert_eq!(view_for(&state, Viewer::Host).revealed_leader, None);
        assert_eq!(view_for(&state, Viewer::Display).revealed_leader, None);
    }

    #[test]
    fn intermission_opt_in_is_private_but_the_drawn_entrants_are_public() {
        let mut state = GameState::new();
        apply_command(
            &mut state,
            Command::AddPlayer {
                name: "Alice".into(),
            },
        )
        .unwrap();
        apply_command(
            &mut state,
            Command::AssignFaction {
                player: PlayerId(0),
                faction: Faction::Ton,
            },
        )
        .unwrap();
        apply_command(&mut state, Command::FinalizeSetup).unwrap();

        apply_command(
            &mut state,
            Command::OptIntoIntermission {
                player: PlayerId(0),
            },
        )
        .unwrap();

        // Opt-in status is private to the opted-in player themselves.
        assert!(view_for(&state, Viewer::Player(PlayerId(0))).i_opted_into_intermission);
        assert!(!view_for(&state, Viewer::Host).i_opted_into_intermission);
        assert_eq!(
            view_for(&state, Viewer::Player(PlayerId(0))).intermission_entrants,
            None
        );

        apply_command(
            &mut state,
            Command::DrawIntermissionEntrants {
                selected: vec![PlayerId(0)],
            },
        )
        .unwrap();

        // Once drawn, the entrant list is the same for every viewer kind.
        let expected = Some(vec![PlayerId(0)]);
        assert_eq!(
            view_for(&state, Viewer::Player(PlayerId(0))).intermission_entrants,
            expected
        );
        assert_eq!(
            view_for(&state, Viewer::Host).intermission_entrants,
            expected
        );
        assert_eq!(
            view_for(&state, Viewer::Display).intermission_entrants,
            expected
        );
    }

    #[test]
    fn servant_leaderboard_is_public_but_never_leaks_gallery_predictions() {
        let mut state = GameState::new();
        apply_command(
            &mut state,
            Command::AddPlayer {
                name: "Servant".into(),
            },
        )
        .unwrap();
        apply_command(
            &mut state,
            Command::AssignFaction {
                player: PlayerId(0),
                faction: Faction::Servant,
            },
        )
        .unwrap();
        apply_command(&mut state, Command::FinalizeSetup).unwrap();
        apply_command(
            &mut state,
            Command::AwardServantPoints {
                player: PlayerId(0),
                points: 4,
            },
        )
        .unwrap();

        let expected = vec![(PlayerId(0), 4)];
        // The leaderboard is the same for every viewer kind -- it's a
        // public party-game score, not a faction/character secret.
        assert_eq!(
            view_for(&state, Viewer::Player(PlayerId(0))).servant_leaderboard,
            expected
        );
        assert_eq!(view_for(&state, Viewer::Host).servant_leaderboard, expected);
        assert_eq!(
            view_for(&state, Viewer::Display).servant_leaderboard,
            expected
        );

        // No `PlayerView` field anywhere exposes a Gallery prediction's
        // actual content -- belt-and-suspenders check across every field
        // name on the type via serialization.
        let serialized = serde_json::to_string(&view_for(&state, Viewer::Host)).unwrap();
        assert!(!serialized.contains("CastOutIs"));
        assert!(!serialized.contains("FactionWins"));
    }

    #[test]
    fn contest_results_are_visible_to_the_host_only() {
        let mut state = GameState::new();
        apply_command(
            &mut state,
            Command::AddPlayer {
                name: "Alice".into(),
            },
        )
        .unwrap();
        apply_command(
            &mut state,
            Command::AssignFaction {
                player: PlayerId(0),
                faction: Faction::Ton,
            },
        )
        .unwrap();
        apply_command(&mut state, Command::FinalizeSetup).unwrap();
        apply_command(&mut state, Command::AdvanceRound).unwrap(); // -> Two
        apply_command(
            &mut state,
            Command::RecordContestResult {
                round: Round::Two,
                category: crate::ContestCategory::Strength,
                ton_won: true,
            },
        )
        .unwrap();

        // The host gets a self-audit view of what's already recorded...
        let host_view = view_for(&state, Viewer::Host);
        assert_eq!(
            host_view.contest_results,
            vec![((Round::Two, crate::ContestCategory::Strength), true)]
        );

        // ...but no player, and not Display either, ever sees this --
        // rules.md is explicit that players never learn the standings or
        // the breakdown, even after the round ends.
        assert!(view_for(&state, Viewer::Player(PlayerId(0)))
            .contest_results
            .is_empty());
        assert!(view_for(&state, Viewer::Display).contest_results.is_empty());
    }

    #[test]
    fn winner_is_computed_for_the_host_only() {
        let mut state = GameState::new();
        let new_player = |state: &mut GameState, name: &str, faction: Faction| -> PlayerId {
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
        };
        let king_queen = new_player(&mut state, "King", Faction::Ton);
        apply_command(
            &mut state,
            Command::AssignCharacter {
                player: king_queen,
                character: Character::KingQueen,
            },
        )
        .unwrap();
        let leader = new_player(&mut state, "Leader", Faction::Uprising);
        apply_command(
            &mut state,
            Command::AssignCharacter {
                player: leader,
                character: Character::RevolutionaryLeader,
            },
        )
        .unwrap();
        apply_command(&mut state, Command::FinalizeSetup).unwrap();

        // Nobody's won yet.
        assert_eq!(view_for(&state, Viewer::Host).winner, None);

        // Casting Out the sole Leader with no successor available exhausts
        // the Uprising's line -- Ton wins (see win_condition::evaluate).
        apply_command(
            &mut state,
            Command::CastOut {
                player: leader,
                fallback_replacement: None,
            },
        )
        .unwrap();

        assert_eq!(view_for(&state, Viewer::Host).winner, Some(Faction::Ton));
        // Never leaked to a player or Display -- see winner_for_host's doc
        // comment on the deferred public finale-reveal sequencing.
        assert_eq!(view_for(&state, Viewer::Player(king_queen)).winner, None);
        assert_eq!(view_for(&state, Viewer::Display).winner, None);
    }

    #[test]
    fn whistledown_posts_are_visible_to_every_viewer_kind() {
        let mut state = GameState::new();
        let alice = {
            let events = apply_command(
                &mut state,
                Command::AddPlayer {
                    name: "Alice".into(),
                },
            )
            .unwrap();
            match events[0] {
                DomainEvent::PlayerAdded { id, .. } => id,
                _ => unreachable!(),
            }
        };
        apply_command(
            &mut state,
            Command::AssignFaction {
                player: alice,
                faction: Faction::Ton,
            },
        )
        .unwrap();
        apply_command(&mut state, Command::FinalizeSetup).unwrap();
        apply_command(&mut state, Command::AdvanceRound).unwrap(); // -> Two, closes Round One's post

        for viewer in [Viewer::Player(alice), Viewer::Host, Viewer::Display] {
            let view = view_for(&state, viewer);
            assert_eq!(view.whistledown.len(), 1);
            assert_eq!(view.whistledown[0].round, Round::One);
        }
    }

    #[test]
    fn gallery_resolved_is_tracked_for_the_host_only() {
        let mut state = GameState::new();
        let cast_out_player = {
            let events = apply_command(
                &mut state,
                Command::AddPlayer {
                    name: "CastOut".into(),
                },
            )
            .unwrap();
            match events[0] {
                DomainEvent::PlayerAdded { id, .. } => id,
                _ => unreachable!(),
            }
        };
        apply_command(
            &mut state,
            Command::AssignFaction {
                player: cast_out_player,
                faction: Faction::Ton,
            },
        )
        .unwrap();
        apply_command(&mut state, Command::FinalizeSetup).unwrap();
        apply_command(
            &mut state,
            Command::CastOut {
                player: cast_out_player,
                fallback_replacement: None,
            },
        )
        .unwrap();
        for _ in 0..5 {
            apply_command(&mut state, Command::AdvanceRound).unwrap();
        }
        assert_eq!(state.current_round(), Round::Finale);
        apply_command(&mut state, Command::OpenDenouncement).unwrap();
        apply_command(&mut state, Command::CloseNomination).unwrap();
        apply_command(&mut state, Command::OpenBallot).unwrap();
        apply_command(
            &mut state,
            Command::CloseBallot {
                fallback_replacement: None,
            },
        )
        .unwrap();

        assert!(!view_for(&state, Viewer::Host).gallery_resolved);

        apply_command(
            &mut state,
            Command::ResolveGalleryPredictions {
                actual_cast_out: vec![],
                actual_winner: Faction::Ton,
            },
        )
        .unwrap();

        assert!(view_for(&state, Viewer::Host).gallery_resolved);
        assert!(!view_for(&state, Viewer::Player(cast_out_player)).gallery_resolved);
        assert!(!view_for(&state, Viewer::Display).gallery_resolved);
    }

    fn sample_bio() -> crate::bio::Bio {
        crate::bio::Bio {
            character_name: "Lord Ashworth".into(),
            real_name: "Alex".into(),
            occupation: "Duke".into(),
            hobbies: ["chess".into(), "".into(), "".into(), "".into(), "".into()],
            clothing_features: [
                "a silver mask".into(),
                "".into(),
                "".into(),
                "".into(),
                "".into(),
            ],
            skills: ["".into(), "".into(), "".into(), "".into(), "".into()],
        }
    }

    #[test]
    fn own_bio_is_visible_only_to_its_own_player_never_others_or_host_display() {
        let mut state = GameState::new();
        let alice = {
            let events = apply_command(
                &mut state,
                Command::AddPlayer {
                    name: "Alice".into(),
                },
            )
            .unwrap();
            match events[0] {
                DomainEvent::PlayerAdded { id, .. } => id,
                _ => unreachable!(),
            }
        };
        apply_command(
            &mut state,
            Command::AssignFaction {
                player: alice,
                faction: Faction::Ton,
            },
        )
        .unwrap();
        apply_command(
            &mut state,
            Command::SubmitBio {
                player: alice,
                bio: sample_bio(),
            },
        )
        .unwrap();

        assert_eq!(
            view_for(&state, Viewer::Player(alice)).own_bio,
            Some(sample_bio())
        );
        assert_eq!(view_for(&state, Viewer::Host).own_bio, None);
        assert_eq!(view_for(&state, Viewer::Display).own_bio, None);
    }

    #[test]
    fn own_interest_level_is_visible_only_to_its_own_player() {
        let mut state = GameState::new();
        let alice = {
            let events = apply_command(
                &mut state,
                Command::AddPlayer {
                    name: "Alice".into(),
                },
            )
            .unwrap();
            match events[0] {
                DomainEvent::PlayerAdded { id, .. } => id,
                _ => unreachable!(),
            }
        };
        apply_command(
            &mut state,
            Command::SubmitInterestLevel {
                player: alice,
                level: 8,
            },
        )
        .unwrap();

        assert_eq!(
            view_for(&state, Viewer::Player(alice)).own_interest_level,
            Some(8)
        );
        assert_eq!(view_for(&state, Viewer::Host).own_interest_level, None);
        assert_eq!(view_for(&state, Viewer::Display).own_interest_level, None);
    }

    #[test]
    fn interest_levels_are_visible_to_the_host_only() {
        let mut state = GameState::new();
        let alice = {
            let events = apply_command(
                &mut state,
                Command::AddPlayer {
                    name: "Alice".into(),
                },
            )
            .unwrap();
            match events[0] {
                DomainEvent::PlayerAdded { id, .. } => id,
                _ => unreachable!(),
            }
        };
        apply_command(
            &mut state,
            Command::SubmitInterestLevel {
                player: alice,
                level: 9,
            },
        )
        .unwrap();

        assert_eq!(
            view_for(&state, Viewer::Host).interest_levels,
            vec![(alice, 9)]
        );
        assert!(view_for(&state, Viewer::Player(alice))
            .interest_levels
            .is_empty());
        assert!(view_for(&state, Viewer::Display).interest_levels.is_empty());
    }

    #[test]
    fn task_candidates_are_visible_to_the_host_only() {
        let mut state = GameState::new();
        let alice = {
            let events = apply_command(
                &mut state,
                Command::AddPlayer {
                    name: "Alice".into(),
                },
            )
            .unwrap();
            match events[0] {
                DomainEvent::PlayerAdded { id, .. } => id,
                _ => unreachable!(),
            }
        };
        apply_command(
            &mut state,
            Command::AssignFaction {
                player: alice,
                faction: Faction::Ton,
            },
        )
        .unwrap();
        apply_command(
            &mut state,
            Command::SubmitBio {
                player: alice,
                bio: sample_bio(),
            },
        )
        .unwrap();

        let host_view = view_for(&state, Viewer::Host);
        assert_eq!(host_view.task_candidates.len(), 3);
        let (medium_tier, medium_candidates) = &host_view.task_candidates[1];
        assert_eq!(*medium_tier, crate::task::TaskTier::Medium);
        assert!(medium_candidates.iter().any(|c| c.prompt.contains("Chess")));

        assert!(view_for(&state, Viewer::Player(alice))
            .task_candidates
            .is_empty());
        assert!(view_for(&state, Viewer::Display).task_candidates.is_empty());
    }

    #[test]
    fn finale_reveal_fields_are_scoped_correctly_across_viewer_kinds() {
        let mut state = GameState::new();
        let cult_leader = {
            let events = apply_command(
                &mut state,
                Command::AddPlayer {
                    name: "CultLeader".into(),
                },
            )
            .unwrap();
            match events[0] {
                DomainEvent::PlayerAdded { id, .. } => id,
                _ => unreachable!(),
            }
        };
        apply_command(
            &mut state,
            Command::AssignFaction {
                player: cult_leader,
                faction: Faction::Cult,
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
        let king_queen = {
            let events = apply_command(
                &mut state,
                Command::AddPlayer {
                    name: "King".into(),
                },
            )
            .unwrap();
            match events[0] {
                DomainEvent::PlayerAdded { id, .. } => id,
                _ => unreachable!(),
            }
        };
        apply_command(
            &mut state,
            Command::AssignFaction {
                player: king_queen,
                faction: Faction::Ton,
            },
        )
        .unwrap();
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

        // Martyrdom hasn't triggered yet -- no message for anyone.
        assert_eq!(
            view_for(&state, Viewer::Player(king_queen)).martyrdom_message,
            None
        );

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

        // Still open -- finale_reveal/finale_cast_out_reveal stay empty
        // for everyone, Host included.
        assert!(view_for(&state, Viewer::Host).finale_reveal.is_none());
        assert!(view_for(&state, Viewer::Host)
            .finale_cast_out_reveal
            .is_empty());

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

        // finale_reveal: Host only.
        assert!(view_for(&state, Viewer::Host).finale_reveal.is_some());
        assert!(view_for(&state, Viewer::Player(king_queen))
            .finale_reveal
            .is_none());
        assert!(view_for(&state, Viewer::Display).finale_reveal.is_none());

        // finale_cast_out_reveal: public to every viewer kind, and it's
        // specifically the Cult Leader (converted status doesn't apply to
        // them, but their true faction/character are now visible).
        for viewer in [Viewer::Host, Viewer::Player(king_queen), Viewer::Display] {
            let reveal = view_for(&state, viewer).finale_cast_out_reveal;
            assert_eq!(reveal.len(), 1);
            assert_eq!(reveal[0].id, cult_leader);
            assert_eq!(reveal[0].true_faction, Faction::Cult);
            assert_eq!(reveal[0].character, Some(Character::CultLeader));
        }

        // martyrdom_message: the converted, still-titled King/Queen only.
        assert!(view_for(&state, Viewer::Player(king_queen))
            .martyrdom_message
            .is_some());
        assert_eq!(view_for(&state, Viewer::Host).martyrdom_message, None);
        assert_eq!(view_for(&state, Viewer::Display).martyrdom_message, None);
    }
}
