use crate::command::Command;
use crate::error::GameError;
use crate::event::DomainEvent;
use crate::player::{Faction, Player, PlayerId};
use std::collections::BTreeMap;

/// The single canonical game state. Owned exclusively by the actor task in
/// the `app` crate at runtime (see the plan's Networking section); never
/// cloned wholesale to a client. Every field is only ever mutated through
/// [`apply_command`].
#[derive(Debug, Clone, Default)]
pub struct GameState {
    players: BTreeMap<PlayerId, Player>,
    next_player_id: u32,
    event_log: Vec<DomainEvent>,
}

impl GameState {
    pub fn new() -> Self {
        GameState::default()
    }

    pub fn player(&self, id: PlayerId) -> Option<&Player> {
        self.players.get(&id)
    }

    pub fn players(&self) -> impl Iterator<Item = &Player> {
        self.players.values()
    }

    pub fn event_log(&self) -> &[DomainEvent] {
        &self.event_log
    }
}

/// The single write path for the whole engine. Validates the command
/// against current state, mutates `state` if (and only if) it's valid, and
/// returns the events the mutation produced. On error, `state` is left
/// unchanged.
pub fn apply_command(state: &mut GameState, cmd: Command) -> Result<Vec<DomainEvent>, GameError> {
    let events = match cmd {
        Command::AddPlayer { name } => {
            let id = PlayerId(state.next_player_id);
            state.next_player_id += 1;
            state.players.insert(id, Player::new(id, name.clone()));
            vec![DomainEvent::PlayerAdded { id, name }]
        }
        Command::AssignFaction { player, faction } => {
            let existing = state
                .players
                .get(&player)
                .ok_or(GameError::UnknownPlayer(player))?;
            if existing.faction != Faction::Unassigned {
                return Err(GameError::AlreadyAssigned(player));
            }
            state.players.get_mut(&player).unwrap().faction = faction;
            vec![DomainEvent::FactionAssigned { player, faction }]
        }
    };

    state.event_log.extend(events.clone());
    Ok(events)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_player_assigns_sequential_ids_and_logs_event() {
        let mut state = GameState::new();

        let events = apply_command(
            &mut state,
            Command::AddPlayer {
                name: "Alice".into(),
            },
        )
        .unwrap();
        assert_eq!(
            events,
            vec![DomainEvent::PlayerAdded {
                id: PlayerId(0),
                name: "Alice".into()
            }]
        );

        let events = apply_command(
            &mut state,
            Command::AddPlayer {
                name: "Bob".into(),
            },
        )
        .unwrap();
        assert_eq!(
            events,
            vec![DomainEvent::PlayerAdded {
                id: PlayerId(1),
                name: "Bob".into()
            }]
        );

        assert_eq!(state.players().count(), 2);
        assert_eq!(state.event_log().len(), 2);
        assert_eq!(state.player(PlayerId(0)).unwrap().name, "Alice");
    }

    #[test]
    fn assign_faction_updates_the_player() {
        let mut state = GameState::new();
        apply_command(
            &mut state,
            Command::AddPlayer {
                name: "Alice".into(),
            },
        )
        .unwrap();

        let events = apply_command(
            &mut state,
            Command::AssignFaction {
                player: PlayerId(0),
                faction: Faction::Ton,
            },
        )
        .unwrap();

        assert_eq!(
            events,
            vec![DomainEvent::FactionAssigned {
                player: PlayerId(0),
                faction: Faction::Ton,
            }]
        );
        assert_eq!(state.player(PlayerId(0)).unwrap().faction, Faction::Ton);
        assert_eq!(state.event_log().len(), 2); // PlayerAdded + FactionAssigned
    }

    #[test]
    fn assign_faction_rejects_unknown_player() {
        let mut state = GameState::new();
        let result = apply_command(
            &mut state,
            Command::AssignFaction {
                player: PlayerId(99),
                faction: Faction::Ton,
            },
        );
        assert_eq!(result, Err(GameError::UnknownPlayer(PlayerId(99))));
        assert!(state.event_log().is_empty());
    }

    #[test]
    fn assign_faction_rejects_double_assignment() {
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

        let result = apply_command(
            &mut state,
            Command::AssignFaction {
                player: PlayerId(0),
                faction: Faction::Uprising,
            },
        );
        assert_eq!(result, Err(GameError::AlreadyAssigned(PlayerId(0))));
        // Faction from the first (successful) assignment must be untouched.
        assert_eq!(state.player(PlayerId(0)).unwrap().faction, Faction::Ton);
    }

    #[test]
    fn failed_command_does_not_grow_the_event_log() {
        let mut state = GameState::new();
        let before = state.event_log().len();
        let _ = apply_command(
            &mut state,
            Command::AssignFaction {
                player: PlayerId(0),
                faction: Faction::Ton,
            },
        );
        assert_eq!(state.event_log().len(), before);
    }
}
