use serde::{Deserialize, Serialize};

/// Stable identifier for a player, assigned in join order.
///
/// Newtype (rather than a bare `u32`) so `PlayerId` can never be silently
/// mixed up with any other numeric ID as the domain model grows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct PlayerId(pub u32);

/// The three competing factions, plus the non-competing Servant track.
///
/// `Unassigned` exists only during setup, before character assignment (§2 of
/// rules.md) has run — no in-progress game state should ever observe it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Faction {
    Unassigned,
    Ton,
    Uprising,
    Cult,
    Servant,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Player {
    pub id: PlayerId,
    pub name: String,
    pub faction: Faction,
}

impl Player {
    pub fn new(id: PlayerId, name: impl Into<String>) -> Self {
        Player {
            id,
            name: name.into(),
            faction: Faction::Unassigned,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_player_starts_unassigned() {
        let p = Player::new(PlayerId(1), "Alice");
        assert_eq!(p.id, PlayerId(1));
        assert_eq!(p.name, "Alice");
        assert_eq!(p.faction, Faction::Unassigned);
    }

    #[test]
    fn player_ids_are_ordered_and_hashable() {
        use std::collections::HashSet;
        let mut ids: Vec<PlayerId> = vec![PlayerId(3), PlayerId(1), PlayerId(2)];
        ids.sort();
        assert_eq!(ids, vec![PlayerId(1), PlayerId(2), PlayerId(3)]);

        let set: HashSet<PlayerId> = ids.into_iter().collect();
        assert!(set.contains(&PlayerId(1)));
    }
}
