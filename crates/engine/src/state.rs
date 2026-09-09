use crate::character::{Character, PlayerStatus};
use crate::command::Command;
use crate::error::GameError;
use crate::event::DomainEvent;
use crate::player::{Faction, Player, PlayerId};
use crate::round::Round;
use std::collections::BTreeMap;

/// The single canonical game state. Owned exclusively by the actor task in
/// the `app` crate at runtime (see the plan's Networking section); never
/// cloned wholesale to a client. Every field is only ever mutated through
/// [`apply_command`].
#[derive(Debug, Clone)]
pub struct GameState {
    players: BTreeMap<PlayerId, Player>,
    next_player_id: u32,
    event_log: Vec<DomainEvent>,

    current_round: Round,

    king_queen: Option<PlayerId>,
    king_queen_transfer_used: bool,
    /// Persistent: true forever once *any* King/Queen has been Denounced
    /// while unconverted, regardless of whether a later King/Queen was
    /// installed afterward. Feeds Cult Path B. See `resolve_cast_out`.
    king_queen_ever_denounced_unconverted: bool,
    /// Persistent: true forever once *any* King/Queen has been converted,
    /// independent of who currently holds the title or whether that person
    /// was later Cast Out. Feeds Cult Path D (martyrdom) -- conversion is
    /// permanent and irreversible, so this never resets. See `convert`.
    king_queen_ever_converted: bool,

    prince_princess: Option<PlayerId>,

    revolutionary_leader: Option<PlayerId>,
    revolutionary_leader_successor: Option<PlayerId>,
    /// Persistent, same reasoning as `king_queen_ever_denounced_unconverted`
    /// but for the Revolutionary Leader. Feeds Cult Path C.
    revolutionary_leader_ever_denounced_unconverted: bool,
    /// Persistent, same reasoning as `king_queen_ever_converted`. Feeds
    /// Cult Path D.
    revolutionary_leader_ever_converted: bool,

    cult_leader: Option<PlayerId>,

    oracle_disabled: bool,
    /// Locked in at the moment the Cult Leader is Cast Out -- see the
    /// module doc comment on `win_condition::evaluate` for why this can't
    /// be a live-recomputed check.
    martyrdom_triggered: bool,
}

impl Default for GameState {
    fn default() -> Self {
        GameState {
            players: BTreeMap::new(),
            next_player_id: 0,
            event_log: Vec::new(),
            current_round: Round::One,
            king_queen: None,
            king_queen_transfer_used: false,
            king_queen_ever_denounced_unconverted: false,
            king_queen_ever_converted: false,
            prince_princess: None,
            revolutionary_leader: None,
            revolutionary_leader_successor: None,
            revolutionary_leader_ever_denounced_unconverted: false,
            revolutionary_leader_ever_converted: false,
            cult_leader: None,
            oracle_disabled: false,
            martyrdom_triggered: false,
        }
    }
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

    pub fn current_round(&self) -> Round {
        self.current_round
    }

    pub(crate) fn king_queen(&self) -> Option<PlayerId> {
        self.king_queen
    }

    // Not read anywhere in win_condition.rs (Prince/Princess doesn't gate
    // any win condition) -- exercised by the cascade tests below, and
    // expected to gain a real caller once view_for/the host UI surfaces
    // title-holder info in a later phase.
    #[allow(dead_code)]
    pub(crate) fn prince_princess(&self) -> Option<PlayerId> {
        self.prince_princess
    }

    pub(crate) fn revolutionary_leader(&self) -> Option<PlayerId> {
        self.revolutionary_leader
    }

    pub fn cult_leader(&self) -> Option<PlayerId> {
        self.cult_leader
    }

    pub fn king_queen_ever_denounced_unconverted(&self) -> bool {
        self.king_queen_ever_denounced_unconverted
    }

    pub fn revolutionary_leader_ever_denounced_unconverted(&self) -> bool {
        self.revolutionary_leader_ever_denounced_unconverted
    }

    pub fn oracle_disabled(&self) -> bool {
        self.oracle_disabled
    }

    pub fn martyrdom_triggered(&self) -> bool {
        self.martyrdom_triggered
    }

    /// The faction a title's holder must belong to. Used to validate
    /// [`Command::AssignCharacter`] -- e.g. rejects assigning `CultLeader`
    /// to a Ton player.
    fn required_faction(character: Character) -> Option<Faction> {
        match character {
            Character::KingQueen | Character::PrincePrincess | Character::NormalTon => {
                Some(Faction::Ton)
            }
            Character::RevolutionaryLeader | Character::NormalUprising => Some(Faction::Uprising),
            Character::CultLeader | Character::Cultist => Some(Faction::Cult),
        }
    }

    /// The lowest-`PlayerId` active, *untitled* player of `faction`,
    /// excluding `exclude` -- the deterministic fallback used when no
    /// explicit replacement is supplied or the supplied one isn't eligible.
    /// "Untitled" (a `Normal*`/`Cultist`/no character yet) matters: without
    /// it, this could hand the King/Queen's crown to whoever's currently
    /// the Prince/Princess, since they're Ton-faction too -- double-titling
    /// someone was never intended. Lowest ID (rather than e.g. highest, or
    /// first-inserted) is an arbitrary but fixed choice, picked so tests
    /// are reproducible without needing to inject a fake RNG.
    fn first_eligible(&self, faction: Faction, exclude: PlayerId) -> Option<PlayerId> {
        self.players
            .values()
            .find(|p| {
                p.faction == faction
                    && p.status == PlayerStatus::Active
                    && p.id != exclude
                    && self.is_untitled(p.id)
            })
            .map(|p| p.id)
    }

    fn is_active(&self, id: PlayerId) -> bool {
        self.players
            .get(&id)
            .is_some_and(|p| p.status == PlayerStatus::Active)
    }

    /// True if `id` doesn't already hold one of the four special titles.
    /// Shared by every "is this candidate a legal replacement/successor"
    /// check -- `first_eligible`'s own search and every caller-supplied
    /// `fallback_replacement`/successor -- so a host mistake (e.g. handing
    /// the crown to the sitting Prince/Princess) is rejected the same way
    /// regardless of which path picked the candidate.
    fn is_untitled(&self, id: PlayerId) -> bool {
        self.players.get(&id).is_some_and(|p| {
            !matches!(
                p.character,
                Some(Character::KingQueen)
                    | Some(Character::PrincePrincess)
                    | Some(Character::RevolutionaryLeader)
                    | Some(Character::CultLeader)
            )
        })
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

        Command::AssignCharacter { player, character } => {
            assign_character(state, player, character)?
        }

        Command::FinalizeSetup => finalize_setup(state),

        Command::Convert { converter, target } => convert(state, converter, target)?,

        Command::DesignateSuccessor { leader, successor } => {
            designate_successor(state, leader, successor)?
        }

        Command::TransferKingQueen { new_holder } => transfer_king_queen(state, new_holder)?,

        Command::CastOut {
            player,
            fallback_replacement,
        } => resolve_cast_out(state, player, fallback_replacement)?,

        Command::AdvanceRound => {
            let next = state
                .current_round
                .next()
                .ok_or(GameError::AlreadyAtFinale)?;
            state.current_round = next;
            vec![DomainEvent::RoundAdvanced { round: next }]
        }
    };

    state.event_log.extend(events.clone());
    Ok(events)
}

fn assign_character(
    state: &mut GameState,
    player: PlayerId,
    character: Character,
) -> Result<Vec<DomainEvent>, GameError> {
    let p = state
        .players
        .get(&player)
        .ok_or(GameError::UnknownPlayer(player))?;
    if let Some(required) = GameState::required_faction(character) {
        if p.faction != required {
            return Err(GameError::WrongFactionForCharacter {
                player,
                character,
                actual: p.faction,
            });
        }
    }

    let title_slot = match character {
        Character::KingQueen => Some(&mut state.king_queen),
        Character::PrincePrincess => Some(&mut state.prince_princess),
        Character::RevolutionaryLeader => Some(&mut state.revolutionary_leader),
        Character::CultLeader => Some(&mut state.cult_leader),
        Character::NormalTon | Character::NormalUprising | Character::Cultist => None,
    };
    if let Some(slot) = title_slot {
        if let Some(holder) = *slot {
            if holder != player {
                return Err(GameError::TitleAlreadyHeld { character, holder });
            }
        }
        *slot = Some(player);
    }

    state.players.get_mut(&player).unwrap().character = Some(character);
    Ok(vec![DomainEvent::CharacterAssigned { player, character }])
}

fn finalize_setup(state: &mut GameState) -> Vec<DomainEvent> {
    for p in state.players.values_mut() {
        if p.character.is_some() {
            continue;
        }
        p.character = match p.faction {
            Faction::Ton => Some(Character::NormalTon),
            Faction::Uprising => Some(Character::NormalUprising),
            Faction::Cult => Some(Character::Cultist),
            Faction::Servant | Faction::Unassigned => None,
        };
    }
    vec![DomainEvent::SetupFinalized]
}

/// Converts `target` to secretly serve the Cult. Growing the Cult's ranks
/// (a generic Ton/Uprising member) and flipping a titled royal are the same
/// underlying operation -- see the doc comment on `Character::Cultist`.
/// Converting the current King/Queen before they've used their transfer
/// ability triggers the auto-cascade from rules.md §4.3: the title
/// auto-transfers to a random remaining Ton player, and the Prince/Princess
/// (if any, and still active) is converted too, staying in play as a secret
/// cultist -- distinct from the *Cast-Out* cascade in `resolve_cast_out`,
/// which removes the Prince/Princess from the game instead.
fn convert(
    state: &mut GameState,
    converter: PlayerId,
    target: PlayerId,
) -> Result<Vec<DomainEvent>, GameError> {
    if state.cult_leader != Some(converter) {
        return Err(GameError::NotCurrentLeader(converter));
    }
    if !state.is_active(converter) {
        return Err(GameError::NotActive(converter));
    }
    let target_player = state
        .players
        .get(&target)
        .ok_or(GameError::UnknownPlayer(target))?;
    if target_player.status != PlayerStatus::Active {
        return Err(GameError::NotActive(target));
    }
    if !matches!(target_player.faction, Faction::Ton | Faction::Uprising) {
        return Err(GameError::WrongFactionForCharacter {
            player: target,
            character: Character::Cultist,
            actual: target_player.faction,
        });
    }

    let mut events = Vec::new();
    let is_king_queen = state.king_queen == Some(target);
    let is_leader = state.revolutionary_leader == Some(target);

    {
        let p = state.players.get_mut(&target).unwrap();
        p.converted = true;
        if matches!(
            p.character,
            None | Some(Character::NormalTon) | Some(Character::NormalUprising)
        ) {
            p.character = Some(Character::Cultist);
        }
    }
    events.push(DomainEvent::Converted { converter, target });

    if is_king_queen {
        state.king_queen_ever_converted = true;
        if !state.king_queen_transfer_used {
            state.king_queen_transfer_used = true;
            let replacement = state.first_eligible(Faction::Ton, target);
            // If nobody else is available, the crown has nowhere to go --
            // the now-converted King/Queen simply keeps it (they're still
            // physically, publicly the King/Queen; conversion doesn't
            // remove them from play the way a Cast-Out does). Only
            // overwrite `state.king_queen` when a real replacement exists;
            // leaving it alone when `replacement` is `None` is what keeps
            // it pointing at `target` rather than going vacant.
            if let Some(new_holder) = replacement {
                state.players.get_mut(&new_holder).unwrap().character = Some(Character::KingQueen);
                state.king_queen = replacement;
            }

            let mut prince_princess_converted = None;
            if let Some(pp) = state.prince_princess {
                if state.is_active(pp) {
                    state.players.get_mut(&pp).unwrap().converted = true;
                    prince_princess_converted = Some(pp);
                }
            }

            events.push(DomainEvent::KingQueenConversionCascade {
                old_king_queen: target,
                new_king_queen: state.king_queen,
                prince_princess_converted,
            });
        }
    }
    if is_leader {
        state.revolutionary_leader_ever_converted = true;
    }

    Ok(events)
}

fn designate_successor(
    state: &mut GameState,
    leader: PlayerId,
    successor: PlayerId,
) -> Result<Vec<DomainEvent>, GameError> {
    if state.revolutionary_leader != Some(leader) {
        return Err(GameError::NotCurrentLeader(leader));
    }
    let s = state
        .players
        .get(&successor)
        .ok_or(GameError::UnknownPlayer(successor))?;
    if s.status != PlayerStatus::Active
        || s.faction != Faction::Uprising
        || successor == leader
        || !state.is_untitled(successor)
    {
        return Err(GameError::IneligibleSuccessor(successor));
    }
    state.revolutionary_leader_successor = Some(successor);
    Ok(vec![DomainEvent::SuccessorDesignated { leader, successor }])
}

fn transfer_king_queen(
    state: &mut GameState,
    new_holder: PlayerId,
) -> Result<Vec<DomainEvent>, GameError> {
    if state.king_queen_transfer_used {
        return Err(GameError::KingQueenTransferAlreadyUsed);
    }
    if state.current_round >= Round::Five {
        return Err(GameError::KingQueenTransferTooLate);
    }
    let old_holder = state
        .king_queen
        .ok_or(GameError::IneligibleKingQueenReplacement(new_holder))?;
    let np = state
        .players
        .get(&new_holder)
        .ok_or(GameError::UnknownPlayer(new_holder))?;
    if np.status != PlayerStatus::Active
        || np.faction != Faction::Ton
        || new_holder == old_holder
        || !state.is_untitled(new_holder)
    {
        return Err(GameError::IneligibleKingQueenReplacement(new_holder));
    }

    state.king_queen_transfer_used = true;
    // The outgoing King/Queen willingly gave up the crown -- they weren't
    // converted or Cast Out, so they simply revert to an ordinary Ton
    // member rather than keeping a title they no longer hold. This specific
    // "what do they become" detail isn't spelled out in rules.md; it's this
    // engine's own reasonable-default judgment call, flagged here rather
    // than left silent.
    state.players.get_mut(&old_holder).unwrap().character = Some(Character::NormalTon);
    state.players.get_mut(&new_holder).unwrap().character = Some(Character::KingQueen);
    state.king_queen = Some(new_holder);

    Ok(vec![DomainEvent::KingQueenTransferred {
        old_holder,
        new_holder,
    }])
}

/// Resolves a Denouncement's outcome for one player: removes them from
/// active play, then dispatches to whichever cascade applies depending on
/// which title (if any) they held. See rules.md §5, "Resolution by target."
fn resolve_cast_out(
    state: &mut GameState,
    player: PlayerId,
    fallback_replacement: Option<PlayerId>,
) -> Result<Vec<DomainEvent>, GameError> {
    let p = state
        .players
        .get(&player)
        .ok_or(GameError::UnknownPlayer(player))?;
    if p.status != PlayerStatus::Active {
        return Err(GameError::NotActive(player));
    }
    let was_converted = p.converted;

    state.players.get_mut(&player).unwrap().status = PlayerStatus::CastOut;
    let mut events = vec![DomainEvent::PlayerCastOut { player }];

    if state.king_queen == Some(player) {
        state.king_queen = None;
        if !was_converted {
            state.oracle_disabled = true;
            events.push(DomainEvent::OracleDisabled);
            state.king_queen_ever_denounced_unconverted = true;

            // The Round-3-specific cascade (rules.md §5) -- does NOT apply
            // at Round 5 or the Finale, where the King/Queen simply has no
            // successor at all (the throne stays vacant for the rest of
            // the game, unlike the Revolutionary Leader's succession,
            // which rules.md describes as unconditional).
            if state.current_round == Round::Three {
                let mut prince_princess_cast_out = None;
                if let Some(pp) = state.prince_princess {
                    if state.is_active(pp) {
                        state.players.get_mut(&pp).unwrap().status = PlayerStatus::CastOut;
                        prince_princess_cast_out = Some(pp);
                    }
                    state.prince_princess = None;
                }

                let replacement = fallback_replacement
                    .filter(|&c| {
                        c != player
                            && state.is_active(c)
                            && state.player(c).unwrap().faction == Faction::Ton
                            && state.is_untitled(c)
                    })
                    .or_else(|| state.first_eligible(Faction::Ton, player));
                if let Some(new_holder) = replacement {
                    state.players.get_mut(&new_holder).unwrap().character =
                        Some(Character::KingQueen);
                }
                state.king_queen = replacement;

                events.push(DomainEvent::KingQueenCastOutCascade {
                    old_king_queen: player,
                    prince_princess_cast_out,
                    new_king_queen: state.king_queen,
                });
            }
        }
    } else if state.prince_princess == Some(player) {
        state.prince_princess = None;
    } else if state.revolutionary_leader == Some(player) {
        if !was_converted {
            state.revolutionary_leader_ever_denounced_unconverted = true;
        }

        let eligible_uprising = |id: PlayerId| {
            id != player
                && state.is_active(id)
                && state.player(id).unwrap().faction == Faction::Uprising
                && state.is_untitled(id)
        };
        let designated = state
            .revolutionary_leader_successor
            .filter(|&s| eligible_uprising(s));
        let fallback = fallback_replacement.filter(|&c| eligible_uprising(c));
        let replacement = designated
            .or(fallback)
            .or_else(|| state.first_eligible(Faction::Uprising, player));

        state.revolutionary_leader_successor = None;
        if let Some(new_leader) = replacement {
            state.players.get_mut(&new_leader).unwrap().character =
                Some(Character::RevolutionaryLeader);
        }
        state.revolutionary_leader = replacement;

        events.push(DomainEvent::RevolutionaryLeaderSucceeded {
            old_leader: player,
            new_leader: replacement,
        });
    } else if state.cult_leader == Some(player) {
        if state.king_queen_ever_converted || state.revolutionary_leader_ever_converted {
            state.martyrdom_triggered = true;
            events.push(DomainEvent::MartyrdomTriggered {
                cult_leader: player,
            });
        }
    }

    Ok(events)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Faction;

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

        let events = apply_command(&mut state, Command::AddPlayer { name: "Bob".into() }).unwrap();
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

    // --- AssignCharacter / FinalizeSetup ---

    #[test]
    fn assign_character_sets_the_title_slot() {
        let mut state = GameState::new();
        let king = add_player(&mut state, "King", Faction::Ton);
        apply_command(
            &mut state,
            Command::AssignCharacter {
                player: king,
                character: Character::KingQueen,
            },
        )
        .unwrap();
        assert_eq!(state.king_queen(), Some(king));
        assert_eq!(
            state.player(king).unwrap().character,
            Some(Character::KingQueen)
        );
    }

    #[test]
    fn assign_character_rejects_wrong_faction() {
        let mut state = GameState::new();
        let p = add_player(&mut state, "Bob", Faction::Uprising);
        let result = apply_command(
            &mut state,
            Command::AssignCharacter {
                player: p,
                character: Character::KingQueen,
            },
        );
        assert_eq!(
            result,
            Err(GameError::WrongFactionForCharacter {
                player: p,
                character: Character::KingQueen,
                actual: Faction::Uprising,
            })
        );
    }

    #[test]
    fn assign_character_rejects_a_title_already_held_by_someone_else() {
        let mut state = GameState::new();
        let king1 = add_player(&mut state, "King1", Faction::Ton);
        let king2 = add_player(&mut state, "King2", Faction::Ton);
        apply_command(
            &mut state,
            Command::AssignCharacter {
                player: king1,
                character: Character::KingQueen,
            },
        )
        .unwrap();
        let result = apply_command(
            &mut state,
            Command::AssignCharacter {
                player: king2,
                character: Character::KingQueen,
            },
        );
        assert_eq!(
            result,
            Err(GameError::TitleAlreadyHeld {
                character: Character::KingQueen,
                holder: king1,
            })
        );
    }

    #[test]
    fn assign_character_accepts_a_generic_catch_all_with_no_title_slot() {
        let mut state = GameState::new();
        let p = add_player(&mut state, "Bob", Faction::Ton);
        apply_command(
            &mut state,
            Command::AssignCharacter {
                player: p,
                character: Character::NormalTon,
            },
        )
        .unwrap();
        assert_eq!(
            state.player(p).unwrap().character,
            Some(Character::NormalTon)
        );
        // A generic character never occupies a title slot.
        assert_eq!(state.king_queen(), None);
    }

    #[test]
    fn assign_character_re_assigning_the_same_title_to_its_current_holder_is_a_no_op() {
        let mut state = GameState::new();
        let king = add_player(&mut state, "King", Faction::Ton);
        apply_command(
            &mut state,
            Command::AssignCharacter {
                player: king,
                character: Character::KingQueen,
            },
        )
        .unwrap();
        // Re-assigning the exact same title to its current holder must not
        // be rejected as "already held by someone else" -- `holder == player`
        // is the one case that's allowed through.
        apply_command(
            &mut state,
            Command::AssignCharacter {
                player: king,
                character: Character::KingQueen,
            },
        )
        .unwrap();
        assert_eq!(state.king_queen(), Some(king));
    }

    #[test]
    fn finalize_setup_fills_in_generic_characters_and_skips_servants() {
        let mut state = GameState::new();
        let king = add_player(&mut state, "King", Faction::Ton);
        let ton2 = add_player(&mut state, "Ton2", Faction::Ton);
        let uprising = add_player(&mut state, "Uprising", Faction::Uprising);
        let cult = add_player(&mut state, "Cult", Faction::Cult);
        let servant = add_player(&mut state, "Servant", Faction::Servant);
        apply_command(
            &mut state,
            Command::AssignCharacter {
                player: king,
                character: Character::KingQueen,
            },
        )
        .unwrap();

        apply_command(&mut state, Command::FinalizeSetup).unwrap();

        assert_eq!(
            state.player(king).unwrap().character,
            Some(Character::KingQueen)
        );
        assert_eq!(
            state.player(ton2).unwrap().character,
            Some(Character::NormalTon)
        );
        assert_eq!(
            state.player(uprising).unwrap().character,
            Some(Character::NormalUprising)
        );
        assert_eq!(
            state.player(cult).unwrap().character,
            Some(Character::Cultist)
        );
        assert_eq!(state.player(servant).unwrap().character, None);
    }

    #[test]
    fn finalize_setup_is_idempotent() {
        let mut state = GameState::new();
        add_player(&mut state, "Ton", Faction::Ton);
        apply_command(&mut state, Command::FinalizeSetup).unwrap();
        let before = state.player(PlayerId(0)).unwrap().character;
        apply_command(&mut state, Command::FinalizeSetup).unwrap();
        assert_eq!(state.player(PlayerId(0)).unwrap().character, before);
    }

    fn setup_full_game() -> (GameState, PlayerId, PlayerId, PlayerId, PlayerId) {
        let mut state = GameState::new();
        let king_queen = add_player(&mut state, "King", Faction::Ton);
        let prince = add_player(&mut state, "Prince", Faction::Ton);
        let leader = add_player(&mut state, "Leader", Faction::Uprising);
        let cult_leader = add_player(&mut state, "CultLeader", Faction::Cult);
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
        (state, king_queen, prince, leader, cult_leader)
    }

    // --- Convert ---

    #[test]
    fn convert_flips_a_generic_member_to_cultist() {
        let (mut state, ..) = setup_full_game();
        let cult_leader = state.cult_leader().unwrap();
        let extra = add_player(&mut state, "Extra", Faction::Ton);
        apply_command(&mut state, Command::FinalizeSetup).unwrap();

        apply_command(
            &mut state,
            Command::Convert {
                converter: cult_leader,
                target: extra,
            },
        )
        .unwrap();

        let p = state.player(extra).unwrap();
        assert!(p.converted);
        assert_eq!(p.character, Some(Character::Cultist));
        assert_eq!(
            p.faction,
            Faction::Ton,
            "apparent faction never changes on conversion"
        );
        assert_eq!(p.true_faction(), Faction::Cult);
    }

    #[test]
    fn convert_rejects_a_non_cult_leader_converter() {
        let (mut state, king_queen, _prince, _leader, _cult_leader) = setup_full_game();
        let result = apply_command(
            &mut state,
            Command::Convert {
                converter: king_queen,
                target: king_queen,
            },
        );
        assert_eq!(result, Err(GameError::NotCurrentLeader(king_queen)));
    }

    #[test]
    fn convert_rejects_an_inactive_cult_leader() {
        let (mut state, king_queen, _prince, _leader, cult_leader) = setup_full_game();
        apply_command(
            &mut state,
            Command::CastOut {
                player: cult_leader,
                fallback_replacement: None,
            },
        )
        .unwrap();
        let result = apply_command(
            &mut state,
            Command::Convert {
                converter: cult_leader,
                target: king_queen,
            },
        );
        assert_eq!(result, Err(GameError::NotActive(cult_leader)));
    }

    #[test]
    fn convert_rejects_an_inactive_target() {
        let (mut state, king_queen, _prince, _leader, cult_leader) = setup_full_game();
        apply_command(
            &mut state,
            Command::CastOut {
                player: king_queen,
                fallback_replacement: None,
            },
        )
        .unwrap();
        let result = apply_command(
            &mut state,
            Command::Convert {
                converter: cult_leader,
                target: king_queen,
            },
        );
        assert_eq!(result, Err(GameError::NotActive(king_queen)));
    }

    #[test]
    fn convert_rejects_a_target_who_is_not_ton_or_uprising() {
        let (mut state, .., cult_leader) = setup_full_game();
        let other_cultist = add_player(&mut state, "OtherCultist", Faction::Cult);
        let result = apply_command(
            &mut state,
            Command::Convert {
                converter: cult_leader,
                target: other_cultist,
            },
        );
        assert_eq!(
            result,
            Err(GameError::WrongFactionForCharacter {
                player: other_cultist,
                character: Character::Cultist,
                actual: Faction::Cult,
            })
        );
    }

    #[test]
    fn converting_king_queen_triggers_the_full_cascade() {
        let (mut state, king_queen, prince, _leader, cult_leader) = setup_full_game();

        let events = apply_command(
            &mut state,
            Command::Convert {
                converter: cult_leader,
                target: king_queen,
            },
        )
        .unwrap();

        // The only other Ton player is the Prince/Princess, who gets swept
        // into the cult rather than installed as King/Queen (a titled
        // player is never an eligible replacement for a different title --
        // see `GameState::is_untitled`). With no *other* Ton player
        // available, the crown has nowhere to go, so the now-converted
        // King/Queen simply keeps it rather than the title going vacant.
        assert!(state.player(king_queen).unwrap().converted);
        assert_eq!(state.king_queen(), Some(king_queen));

        // Prince/Princess was converted too, and is still Active (not Cast
        // Out) -- the conversion cascade keeps them in play, unlike the
        // Cast-Out cascade.
        assert!(state.player(prince).unwrap().converted);
        assert_eq!(state.player(prince).unwrap().status, PlayerStatus::Active);

        assert!(events
            .iter()
            .any(|e| matches!(e, DomainEvent::KingQueenConversionCascade { .. })));
    }

    #[test]
    fn converting_king_queen_installs_a_replacement_when_one_exists() {
        let (mut state, king_queen, _prince, _leader, cult_leader) = setup_full_game();
        let extra_ton = add_player(&mut state, "ExtraTon", Faction::Ton);
        apply_command(&mut state, Command::FinalizeSetup).unwrap();

        apply_command(
            &mut state,
            Command::Convert {
                converter: cult_leader,
                target: king_queen,
            },
        )
        .unwrap();

        assert_eq!(state.king_queen(), Some(extra_ton));
        assert_eq!(
            state.player(extra_ton).unwrap().character,
            Some(Character::KingQueen)
        );
    }

    #[test]
    fn converting_king_queen_after_transfer_already_used_does_not_recascade() {
        let (mut state, king_queen, prince, _leader, cult_leader) = setup_full_game();
        let extra_ton = add_player(&mut state, "ExtraTon", Faction::Ton);
        apply_command(&mut state, Command::FinalizeSetup).unwrap();

        apply_command(
            &mut state,
            Command::TransferKingQueen {
                new_holder: extra_ton,
            },
        )
        .unwrap();
        assert_eq!(state.king_queen(), Some(extra_ton));

        // Converting the *new* King/Queen should NOT re-trigger the
        // auto-transfer cascade, since the transfer ability is already
        // spent -- they just quietly become a converted King/Queen.
        let events = apply_command(
            &mut state,
            Command::Convert {
                converter: cult_leader,
                target: extra_ton,
            },
        )
        .unwrap();

        assert_eq!(state.king_queen(), Some(extra_ton));
        assert!(state.player(extra_ton).unwrap().converted);
        assert!(!state.player(prince).unwrap().converted);
        assert!(!events
            .iter()
            .any(|e| matches!(e, DomainEvent::KingQueenConversionCascade { .. })));
        let _ = king_queen;
    }

    #[test]
    fn converting_revolutionary_leader_sets_the_ever_converted_flag_but_no_cascade() {
        let (mut state, _king_queen, _prince, leader, cult_leader) = setup_full_game();
        apply_command(
            &mut state,
            Command::Convert {
                converter: cult_leader,
                target: leader,
            },
        )
        .unwrap();
        // Still holds the title -- conversion doesn't remove them from it.
        assert_eq!(state.revolutionary_leader(), Some(leader));
        assert!(state.player(leader).unwrap().converted);
    }

    // --- DesignateSuccessor ---

    #[test]
    fn designate_successor_records_the_choice() {
        let (mut state, _king_queen, _prince, leader, _cult_leader) = setup_full_game();
        let ally = add_player(&mut state, "Ally", Faction::Uprising);
        apply_command(&mut state, Command::FinalizeSetup).unwrap();

        apply_command(
            &mut state,
            Command::DesignateSuccessor {
                leader,
                successor: ally,
            },
        )
        .unwrap();

        apply_command(
            &mut state,
            Command::CastOut {
                player: leader,
                fallback_replacement: None,
            },
        )
        .unwrap();
        assert_eq!(state.revolutionary_leader(), Some(ally));
    }

    #[test]
    fn designate_successor_rejects_a_non_leader() {
        let (mut state, king_queen, _prince, _leader, _cult_leader) = setup_full_game();
        let result = apply_command(
            &mut state,
            Command::DesignateSuccessor {
                leader: king_queen,
                successor: king_queen,
            },
        );
        assert_eq!(result, Err(GameError::NotCurrentLeader(king_queen)));
    }

    #[test]
    fn designate_successor_rejects_an_ineligible_candidate() {
        let (mut state, king_queen, _prince, leader, _cult_leader) = setup_full_game();
        let result = apply_command(
            &mut state,
            Command::DesignateSuccessor {
                leader,
                successor: king_queen, // wrong faction
            },
        );
        assert_eq!(result, Err(GameError::IneligibleSuccessor(king_queen)));
    }

    // --- TransferKingQueen ---

    #[test]
    fn transfer_king_queen_moves_the_title_and_demotes_the_old_holder() {
        let (mut state, king_queen, ..) = setup_full_game();
        let extra_ton = add_player(&mut state, "ExtraTon", Faction::Ton);
        apply_command(&mut state, Command::FinalizeSetup).unwrap();

        apply_command(
            &mut state,
            Command::TransferKingQueen {
                new_holder: extra_ton,
            },
        )
        .unwrap();

        assert_eq!(state.king_queen(), Some(extra_ton));
        assert_eq!(
            state.player(extra_ton).unwrap().character,
            Some(Character::KingQueen)
        );
        assert_eq!(
            state.player(king_queen).unwrap().character,
            Some(Character::NormalTon)
        );
    }

    #[test]
    fn transfer_king_queen_rejects_reuse() {
        let (mut state, ..) = setup_full_game();
        let extra1 = add_player(&mut state, "Extra1", Faction::Ton);
        let extra2 = add_player(&mut state, "Extra2", Faction::Ton);
        apply_command(&mut state, Command::FinalizeSetup).unwrap();

        apply_command(
            &mut state,
            Command::TransferKingQueen { new_holder: extra1 },
        )
        .unwrap();
        let result = apply_command(
            &mut state,
            Command::TransferKingQueen { new_holder: extra2 },
        );
        assert_eq!(result, Err(GameError::KingQueenTransferAlreadyUsed));
    }

    #[test]
    fn transfer_king_queen_rejects_at_round_five_or_later() {
        let (mut state, ..) = setup_full_game();
        let extra = add_player(&mut state, "Extra", Faction::Ton);
        apply_command(&mut state, Command::FinalizeSetup).unwrap();

        for _ in 0..4 {
            apply_command(&mut state, Command::AdvanceRound).unwrap();
        }
        assert_eq!(state.current_round(), Round::Five);

        let result = apply_command(&mut state, Command::TransferKingQueen { new_holder: extra });
        assert_eq!(result, Err(GameError::KingQueenTransferTooLate));
    }

    #[test]
    fn transfer_king_queen_rejects_an_ineligible_new_holder() {
        let (mut state, king_queen, prince, ..) = setup_full_game();
        // Prince/Princess is Ton-faction and Active, but already titled --
        // handing the crown to them would double-title someone.
        let result = apply_command(
            &mut state,
            Command::TransferKingQueen { new_holder: prince },
        );
        assert_eq!(
            result,
            Err(GameError::IneligibleKingQueenReplacement(prince))
        );
        assert_eq!(
            state.king_queen(),
            Some(king_queen),
            "the rejected transfer must not mutate anything"
        );
    }

    // --- CastOut: King/Queen ---

    #[test]
    fn king_queen_cast_out_disables_oracle_and_sets_the_denounced_flag() {
        let (mut state, king_queen, ..) = setup_full_game();
        apply_command(
            &mut state,
            Command::CastOut {
                player: king_queen,
                fallback_replacement: None,
            },
        )
        .unwrap();
        assert!(state.oracle_disabled());
        assert!(state.king_queen_ever_denounced_unconverted());
        assert_eq!(state.king_queen(), None);
    }

    #[test]
    fn king_queen_cast_out_at_round_three_sweeps_prince_princess_and_installs_a_new_king_queen() {
        let (mut state, king_queen, prince, ..) = setup_full_game();
        let extra_ton = add_player(&mut state, "ExtraTon", Faction::Ton);
        apply_command(&mut state, Command::FinalizeSetup).unwrap();
        apply_command(&mut state, Command::AdvanceRound).unwrap(); // -> Two
        apply_command(&mut state, Command::AdvanceRound).unwrap(); // -> Three
        assert_eq!(state.current_round(), Round::Three);

        apply_command(
            &mut state,
            Command::CastOut {
                player: king_queen,
                fallback_replacement: Some(extra_ton),
            },
        )
        .unwrap();

        assert_eq!(state.player(prince).unwrap().status, PlayerStatus::CastOut);
        assert_eq!(state.prince_princess(), None);
        assert_eq!(state.king_queen(), Some(extra_ton));
        assert_eq!(
            state.player(extra_ton).unwrap().character,
            Some(Character::KingQueen)
        );
    }

    #[test]
    fn king_queen_cast_out_at_round_three_with_no_prince_princess_still_reassigns() {
        // No Prince/Princess was ever assigned -- the cascade's sweep step
        // must handle a vacant slot without panicking, and still reassign
        // the title.
        let mut state = GameState::new();
        let king_queen = add_player(&mut state, "King", Faction::Ton);
        let extra_ton = add_player(&mut state, "ExtraTon", Faction::Ton);
        let cult_leader = add_player(&mut state, "CultLeader", Faction::Cult);
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
                player: cult_leader,
                character: Character::CultLeader,
            },
        )
        .unwrap();
        apply_command(&mut state, Command::FinalizeSetup).unwrap();
        apply_command(&mut state, Command::AdvanceRound).unwrap();
        apply_command(&mut state, Command::AdvanceRound).unwrap();
        assert_eq!(state.current_round(), Round::Three);

        apply_command(
            &mut state,
            Command::CastOut {
                player: king_queen,
                fallback_replacement: Some(extra_ton),
            },
        )
        .unwrap();

        assert_eq!(state.prince_princess(), None);
        assert_eq!(state.king_queen(), Some(extra_ton));
    }

    #[test]
    fn prince_princess_cast_out_directly_vacates_the_slot() {
        let (mut state, _king_queen, prince, ..) = setup_full_game();
        apply_command(
            &mut state,
            Command::CastOut {
                player: prince,
                fallback_replacement: None,
            },
        )
        .unwrap();
        assert_eq!(state.player(prince).unwrap().status, PlayerStatus::CastOut);
        assert_eq!(state.prince_princess(), None);
    }

    #[test]
    fn king_queen_cast_out_at_round_five_does_not_sweep_prince_princess_or_reassign() {
        let (mut state, king_queen, prince, ..) = setup_full_game();
        let extra_ton = add_player(&mut state, "ExtraTon", Faction::Ton);
        apply_command(&mut state, Command::FinalizeSetup).unwrap();
        for _ in 0..4 {
            apply_command(&mut state, Command::AdvanceRound).unwrap();
        }
        assert_eq!(state.current_round(), Round::Five);

        apply_command(
            &mut state,
            Command::CastOut {
                player: king_queen,
                fallback_replacement: Some(extra_ton),
            },
        )
        .unwrap();

        assert_eq!(
            state.player(prince).unwrap().status,
            PlayerStatus::Active,
            "the Round-3-only cascade must not fire outside Round 3"
        );
        assert_eq!(state.prince_princess(), Some(prince));
        assert_eq!(
            state.king_queen(),
            None,
            "no successor mechanic exists for the King/Queen outside the Round 3 cascade"
        );
    }

    #[test]
    fn king_queen_cast_out_while_converted_does_not_disable_oracle() {
        // Deliberately a fresh, minimal setup rather than `setup_full_game`:
        // with no *other* Ton player available, converting the King/Queen
        // has nowhere to reassign the title to, so it stays on them --
        // exactly the scenario this test needs (a *converted* King/Queen
        // later getting Cast Out), without a second state or dead code to
        // get there.
        let mut state = GameState::new();
        let king_queen = add_player(&mut state, "King", Faction::Ton);
        let cult_leader = add_player(&mut state, "CultLeader", Faction::Cult);
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
                player: cult_leader,
                character: Character::CultLeader,
            },
        )
        .unwrap();
        apply_command(&mut state, Command::FinalizeSetup).unwrap();

        apply_command(
            &mut state,
            Command::Convert {
                converter: cult_leader,
                target: king_queen,
            },
        )
        .unwrap();
        assert_eq!(
            state.king_queen(),
            Some(king_queen),
            "no other Ton player exists, so conversion's cascade has nowhere to reassign to"
        );
        assert!(state.player(king_queen).unwrap().converted);

        apply_command(
            &mut state,
            Command::CastOut {
                player: king_queen,
                fallback_replacement: None,
            },
        )
        .unwrap();

        assert!(
            !state.oracle_disabled(),
            "Oracle only goes dark for a *loyal* King/Queen being Denounced, not a converted one"
        );
        assert!(
            !state.king_queen_ever_denounced_unconverted(),
            "this King/Queen was converted, not denounced-while-loyal"
        );
        assert_eq!(state.king_queen(), None);
    }

    // --- CastOut: Revolutionary Leader ---

    #[test]
    fn leader_cast_out_with_no_successor_leaves_the_seat_vacant() {
        let (mut state, _king_queen, _prince, leader, _cult_leader) = setup_full_game();
        apply_command(
            &mut state,
            Command::CastOut {
                player: leader,
                fallback_replacement: None,
            },
        )
        .unwrap();
        assert_eq!(state.revolutionary_leader(), None);
        assert!(state.revolutionary_leader_ever_denounced_unconverted());
    }

    #[test]
    fn leader_cast_out_prefers_the_designated_successor_over_the_fallback() {
        let (mut state, _king_queen, _prince, leader, _cult_leader) = setup_full_game();
        let designated = add_player(&mut state, "Designated", Faction::Uprising);
        let other = add_player(&mut state, "Other", Faction::Uprising);
        apply_command(&mut state, Command::FinalizeSetup).unwrap();

        apply_command(
            &mut state,
            Command::DesignateSuccessor {
                leader,
                successor: designated,
            },
        )
        .unwrap();

        apply_command(
            &mut state,
            Command::CastOut {
                player: leader,
                fallback_replacement: Some(other),
            },
        )
        .unwrap();

        assert_eq!(state.revolutionary_leader(), Some(designated));
    }

    #[test]
    fn leader_cast_out_while_converted_does_not_set_the_unconverted_denounced_flag() {
        let (mut state, _king_queen, _prince, leader, cult_leader) = setup_full_game();
        apply_command(
            &mut state,
            Command::Convert {
                converter: cult_leader,
                target: leader,
            },
        )
        .unwrap();
        apply_command(
            &mut state,
            Command::CastOut {
                player: leader,
                fallback_replacement: None,
            },
        )
        .unwrap();
        assert!(!state.revolutionary_leader_ever_denounced_unconverted());
    }

    // --- CastOut: Cult Leader / martyrdom ---

    #[test]
    fn cult_leader_cast_out_with_a_prior_conversion_triggers_martyrdom() {
        let (mut state, king_queen, _prince, _leader, cult_leader) = setup_full_game();
        apply_command(
            &mut state,
            Command::Convert {
                converter: cult_leader,
                target: king_queen,
            },
        )
        .unwrap();
        let events = apply_command(
            &mut state,
            Command::CastOut {
                player: cult_leader,
                fallback_replacement: None,
            },
        )
        .unwrap();
        assert!(state.martyrdom_triggered());
        assert!(events
            .iter()
            .any(|e| matches!(e, DomainEvent::MartyrdomTriggered { .. })));
    }

    #[test]
    fn cult_leader_cast_out_without_any_conversion_does_not_trigger_martyrdom() {
        let (mut state, .., cult_leader) = setup_full_game();
        apply_command(
            &mut state,
            Command::CastOut {
                player: cult_leader,
                fallback_replacement: None,
            },
        )
        .unwrap();
        assert!(!state.martyrdom_triggered());
    }

    // --- CastOut: everyone else / general rules ---

    #[test]
    fn cast_out_rejects_an_already_inactive_player() {
        let (mut state, king_queen, ..) = setup_full_game();
        apply_command(
            &mut state,
            Command::CastOut {
                player: king_queen,
                fallback_replacement: None,
            },
        )
        .unwrap();
        let result = apply_command(
            &mut state,
            Command::CastOut {
                player: king_queen,
                fallback_replacement: None,
            },
        );
        assert_eq!(result, Err(GameError::NotActive(king_queen)));
    }

    #[test]
    fn cast_out_an_ordinary_player_has_no_special_cascade() {
        let (mut state, ..) = setup_full_game();
        let extra = add_player(&mut state, "Extra", Faction::Ton);
        apply_command(&mut state, Command::FinalizeSetup).unwrap();

        let events = apply_command(
            &mut state,
            Command::CastOut {
                player: extra,
                fallback_replacement: None,
            },
        )
        .unwrap();

        assert_eq!(events, vec![DomainEvent::PlayerCastOut { player: extra }]);
        assert_eq!(state.player(extra).unwrap().status, PlayerStatus::CastOut);
    }

    // --- AdvanceRound ---

    #[test]
    fn advance_round_steps_through_the_sequence_and_rejects_past_finale() {
        let mut state = GameState::new();
        for expected in [
            Round::Two,
            Round::Three,
            Round::Four,
            Round::Five,
            Round::Finale,
        ] {
            let events = apply_command(&mut state, Command::AdvanceRound).unwrap();
            assert_eq!(events, vec![DomainEvent::RoundAdvanced { round: expected }]);
            assert_eq!(state.current_round(), expected);
        }
        let result = apply_command(&mut state, Command::AdvanceRound);
        assert_eq!(result, Err(GameError::AlreadyAtFinale));
    }
}
