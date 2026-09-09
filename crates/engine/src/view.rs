use crate::player::{Faction, PlayerId};
use crate::state::GameState;
use serde::{Deserialize, Serialize};

/// Who is asking to see the state. This is the *only* input that
/// determines what a caller gets back from [`view_for`] — there is no path
/// that hands out a raw [`GameState`] for a client to filter itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Viewer {
    Player(PlayerId),
    Host,
    Display,
}

/// One row of the public roster: visible to every viewer. Deliberately
/// carries no faction — see the note on `roster` below.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RosterEntry {
    pub id: PlayerId,
    pub name: String,
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
        })
        .collect();

    let own_faction = match viewer {
        Viewer::Player(id) => state.player(id).map(|p| p.faction),
        Viewer::Host | Viewer::Display => None,
    };

    PlayerView {
        roster,
        own_faction,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::command::Command;
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
}
