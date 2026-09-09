use crate::character::{Character, PlayerStatus};
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
///
/// This is a player's *apparent* faction, not necessarily their true
/// loyalty: a Ton or Uprising player can be secretly converted (see
/// `Player::converted`) without their `faction` ever changing -- that's the
/// entire point of conversion as a hidden-role mechanic. Use
/// `Player::true_faction()` when win-condition logic needs the real answer.
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
    pub character: Option<Character>,
    /// True if this player is secretly Cult-aligned despite an apparent
    /// `faction` of `Ton` or `Uprising`. Always `false` for a player whose
    /// `faction` is already `Cult` or `Servant` -- see `is_consistent`.
    pub converted: bool,
    pub status: PlayerStatus,
}

impl Player {
    pub fn new(id: PlayerId, name: impl Into<String>) -> Self {
        Player {
            id,
            name: name.into(),
            faction: Faction::Unassigned,
            character: None,
            converted: false,
            status: PlayerStatus::Active,
        }
    }

    /// The player's real loyalty for win-condition purposes, as opposed to
    /// the `faction` they appear to be. A converted Ton/Uprising player's
    /// true faction is `Cult`; everyone else's true faction is just their
    /// apparent one.
    pub fn true_faction(&self) -> Faction {
        if self.converted {
            Faction::Cult
        } else {
            self.faction
        }
    }

    /// Debug/test invariant: `character == Cultist` should always imply
    /// `converted == true` (see the doc comment on `Character::Cultist`),
    /// and a converted player's apparent faction should always be `Ton` or
    /// `Uprising` (conversion only makes sense as *hiding inside* a public
    /// faction). Not called from production code paths -- exercised by
    /// tests that construct edge-case states, so a future change that
    /// breaks this invariant fails loudly instead of silently.
    #[cfg(test)]
    fn is_consistent(&self) -> bool {
        if self.character == Some(Character::Cultist) && !self.converted {
            return false;
        }
        if self.converted && !matches!(self.faction, Faction::Ton | Faction::Uprising) {
            return false;
        }
        true
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
        assert_eq!(p.character, None);
        assert!(!p.converted);
        assert_eq!(p.status, PlayerStatus::Active);
        assert!(p.is_consistent());
    }

    #[test]
    fn true_faction_is_apparent_faction_when_not_converted() {
        let mut p = Player::new(PlayerId(1), "Alice");
        p.faction = Faction::Ton;
        assert_eq!(p.true_faction(), Faction::Ton);
    }

    #[test]
    fn true_faction_is_cult_when_converted() {
        let mut p = Player::new(PlayerId(1), "Alice");
        p.faction = Faction::Uprising;
        p.converted = true;
        assert_eq!(p.true_faction(), Faction::Cult);
        assert!(p.is_consistent());
    }

    #[test]
    fn cultist_character_without_converted_is_inconsistent() {
        let mut p = Player::new(PlayerId(1), "Alice");
        p.faction = Faction::Ton;
        p.character = Some(Character::Cultist);
        // Deliberately NOT setting converted = true.
        assert!(!p.is_consistent());
    }

    #[test]
    fn converted_non_ton_or_uprising_is_inconsistent() {
        let mut p = Player::new(PlayerId(1), "Alice");
        p.faction = Faction::Servant;
        p.converted = true;
        assert!(!p.is_consistent());
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
