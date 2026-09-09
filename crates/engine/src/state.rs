use crate::character::{Character, PlayerStatus};
use crate::command::Command;
use crate::denouncement::{
    execution_count, resolve_ballot, surfaced_nominees, Ballot, Denouncement, DenouncementPhase,
};
use crate::error::GameError;
use crate::event::DomainEvent;
use crate::player::{Faction, Player, PlayerId};
use crate::round::Round;
use crate::task::{TaskDef, TaskId, TaskTier};
use std::collections::{BTreeMap, BTreeSet};

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

    /// At most one Denouncement runs at a time -- `Command::OpenDenouncement`
    /// is rejected while this is `Some`. See `denouncement.rs`.
    denouncement: Option<Denouncement>,

    /// Every task ever pushed, across every round -- kept even after a
    /// task closes so `task_attempt` can still answer "did this player
    /// complete this task" for round-recap/Whistledown purposes later.
    tasks: BTreeMap<TaskId, TaskDef>,
    next_task_id: u32,
    /// Currently attemptable tasks. `CloseTasks` clears this without
    /// removing anything from `tasks`.
    open_tasks: BTreeSet<TaskId>,
    /// `credited` outcome per (player, task) attempt. Deliberately does not
    /// store the `named` claim itself -- see the doc comment on
    /// `DomainEvent::TaskAttempted` for why.
    task_attempts: BTreeMap<(PlayerId, TaskId), bool>,
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
            denouncement: None,
            tasks: BTreeMap::new(),
            next_task_id: 0,
            open_tasks: BTreeSet::new(),
            task_attempts: BTreeMap::new(),
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

    /// The active Denouncement's current phase, if one is in progress.
    pub fn denouncement_phase(&self) -> Option<&DenouncementPhase> {
        self.denouncement.as_ref().map(|d| &d.phase)
    }

    /// How many Active players belong to one of the three competing
    /// factions (Ton/Uprising/Cult) -- Servants and still-`Unassigned`
    /// players are excluded. This is what the execution-count scaling
    /// formula (rules.md §5) actually scales against, per Dalton's
    /// resolution of that ambiguity during planning: the formula's
    /// "players remaining" means competing players specifically, not
    /// everyone still physically in the game.
    pub fn competing_player_count(&self) -> usize {
        self.players
            .values()
            .filter(|p| {
                p.status == PlayerStatus::Active
                    && matches!(p.faction, Faction::Ton | Faction::Uprising | Faction::Cult)
            })
            .count()
    }

    pub fn task(&self, id: TaskId) -> Option<&TaskDef> {
        self.tasks.get(&id)
    }

    pub fn is_task_open(&self, id: TaskId) -> bool {
        self.open_tasks.contains(&id)
    }

    pub fn open_task_ids(&self) -> impl Iterator<Item = &TaskId> {
        self.open_tasks.iter()
    }

    /// `Some(credited)` if `player` has already attempted `task`, `None` if
    /// they haven't yet.
    pub fn task_attempt(&self, player: PlayerId, task: TaskId) -> Option<bool> {
        self.task_attempts.get(&(player, task)).copied()
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
    /// excluding anyone in `exclude` -- the deterministic fallback used
    /// when no explicit replacement is supplied or the supplied one isn't
    /// eligible. "Untitled" (a `Normal*`/`Cultist`/no character yet)
    /// matters: without it, this could hand the King/Queen's crown to
    /// whoever's currently the Prince/Princess, since they're Ton-faction
    /// too -- double-titling someone was never intended. `exclude` takes a
    /// slice (not a single `PlayerId`) so a caller resolving several
    /// Cast-Outs from the same Denouncement batch can exclude everyone in
    /// that batch, not just the one player currently being processed --
    /// see the doc comment on `resolve_cast_out`'s `also_departing`
    /// parameter for why that matters. Lowest ID (rather than e.g.
    /// highest, or first-inserted) is an arbitrary but fixed choice,
    /// picked so tests are reproducible without needing to inject a fake
    /// RNG.
    fn first_eligible(&self, faction: Faction, exclude: &[PlayerId]) -> Option<PlayerId> {
        self.players
            .values()
            .find(|p| {
                p.faction == faction
                    && p.status == PlayerStatus::Active
                    && !exclude.contains(&p.id)
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
        } => resolve_cast_out(state, player, fallback_replacement, &[player])?,

        Command::AdvanceRound => {
            let next = state
                .current_round
                .next()
                .ok_or(GameError::AlreadyAtFinale)?;
            state.current_round = next;
            vec![DomainEvent::RoundAdvanced { round: next }]
        }

        Command::OpenDenouncement => {
            if state.denouncement.is_some() {
                return Err(GameError::DenouncementAlreadyOpen);
            }
            state.denouncement = Some(Denouncement {
                phase: DenouncementPhase::Nomination {
                    submitted: BTreeMap::new(),
                },
            });
            vec![DomainEvent::DenouncementOpened]
        }

        Command::Nominate { voter, nominee } => nominate(state, voter, nominee)?,

        Command::CloseNomination => close_nomination(state)?,

        Command::OpenBallot => open_ballot(state)?,

        Command::CastBallot { voter, ballot } => cast_ballot(state, voter, ballot)?,

        Command::CloseBallot {
            fallback_replacement,
        } => close_ballot(state, fallback_replacement)?,

        Command::CloseRunoff {
            fallback_replacement,
        } => close_runoff(state, fallback_replacement)?,

        Command::PushTask {
            prompt,
            tier,
            qualifying_players,
        } => push_task(state, prompt, tier, qualifying_players),

        Command::CloseTasks => close_tasks(state),

        Command::AttemptTask {
            player,
            task,
            named,
        } => attempt_task(state, player, task, named)?,
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
    // Re-assigning a player's *current* character to them is an idempotent
    // no-op (see the title_slot handling below), but assigning a
    // *different* character to a player who already has one is always a
    // mistake -- without this check, a player could pick up a second
    // title (e.g. King/Queen after already being Prince/Princess) with the
    // old title slot (`state.prince_princess` here) left dangling, pointing
    // at someone who no longer holds it.
    if let Some(existing) = p.character {
        if existing != character {
            return Err(GameError::AlreadyHasCharacter {
                player,
                existing,
                requested: character,
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
    if target_player.converted {
        return Err(GameError::AlreadyConverted(target));
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
            let replacement = state.first_eligible(Faction::Ton, &[target]);
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
///
/// `also_departing` lists everyone being Cast Out in this same batch
/// (`player` included) -- a multi-slot Denouncement (21+ competing
/// players) resolves several Cast-Outs from one Ballot/Runoff, one at a
/// time, via a caller loop. Without excluding the rest of the batch from
/// replacement/successor eligibility here, an earlier iteration's cascade
/// could crown or elect someone who is *also* independently on the same
/// batch's list -- e.g. the King/Queen and player Z both get a slot; Z
/// gets crowned mid-loop as the King/Queen's replacement; then Z's own,
/// separately-earned Cast-Out is processed and immediately re-triggers the
/// cascade a second time, handing the crown to a third player who was
/// never nominated or voted for at all. Passing the whole batch here means
/// `first_eligible`/the eligibility filters simply never consider anyone
/// who's leaving the game this round, so a replacement only ever comes
/// from outside the batch (or the throne/leadership goes vacant, same as
/// when no one at all is eligible). A direct `Command::CastOut` (not part
/// of a Denouncement) just passes `&[player]`.
fn resolve_cast_out(
    state: &mut GameState,
    player: PlayerId,
    fallback_replacement: Option<PlayerId>,
    also_departing: &[PlayerId],
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
                            && !also_departing.contains(&c)
                            && state.is_active(c)
                            && state.player(c).unwrap().faction == Faction::Ton
                            && state.is_untitled(c)
                    })
                    .or_else(|| state.first_eligible(Faction::Ton, also_departing));
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
                && !also_departing.contains(&id)
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
            .or_else(|| state.first_eligible(Faction::Uprising, also_departing));

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

fn nominate(
    state: &mut GameState,
    voter: PlayerId,
    nominee: PlayerId,
) -> Result<Vec<DomainEvent>, GameError> {
    if state.denouncement.is_none() {
        return Err(GameError::NoDenouncementOpen);
    }
    if !state.is_active(voter) {
        return Err(GameError::NotActive(voter));
    }
    if !state.is_active(nominee) {
        return Err(GameError::NotActive(nominee));
    }
    let denouncement = state.denouncement.as_mut().unwrap();
    let DenouncementPhase::Nomination { submitted } = &mut denouncement.phase else {
        return Err(GameError::NominationNotOpen);
    };
    submitted.insert(voter, nominee);
    Ok(vec![DomainEvent::NominationCast { voter, nominee }])
}

fn close_nomination(state: &mut GameState) -> Result<Vec<DomainEvent>, GameError> {
    let denouncement = state
        .denouncement
        .as_mut()
        .ok_or(GameError::NoDenouncementOpen)?;
    let DenouncementPhase::Nomination { submitted } = &denouncement.phase else {
        return Err(GameError::NominationNotOpen);
    };

    let mut tally: BTreeMap<PlayerId, u32> = BTreeMap::new();
    for &nominee in submitted.values() {
        *tally.entry(nominee).or_insert(0) += 1;
    }
    // Top 3, with everyone tied for the last spot surfacing too -- see
    // `denouncement::surfaced_nominees`.
    let surfaced = surfaced_nominees(&tally, 3);

    denouncement.phase = DenouncementPhase::Discussion {
        surfaced: surfaced.clone(),
    };
    Ok(vec![DomainEvent::NominationClosed { surfaced }])
}

fn open_ballot(state: &mut GameState) -> Result<Vec<DomainEvent>, GameError> {
    let denouncement = state
        .denouncement
        .as_mut()
        .ok_or(GameError::NoDenouncementOpen)?;
    let DenouncementPhase::Discussion { surfaced } = &denouncement.phase else {
        return Err(GameError::DiscussionNotOpen);
    };
    let candidates = surfaced.clone();
    denouncement.phase = DenouncementPhase::Ballot {
        surfaced: candidates.clone(),
        ballots: BTreeMap::new(),
    };
    Ok(vec![DomainEvent::BallotOpened { candidates }])
}

fn cast_ballot(
    state: &mut GameState,
    voter: PlayerId,
    ballot: Ballot,
) -> Result<Vec<DomainEvent>, GameError> {
    if !state.is_active(voter) {
        return Err(GameError::NotActive(voter));
    }
    // Determine the phase (and thus its candidate list) *before* taking a
    // mutable borrow, so an invalid target is rejected without recording
    // anything. Checking the phase first -- rather than validating a
    // `For` target against "whatever phase happens to be active, if any"
    // -- means a ballot cast at the wrong phase reports the actually
    // correct `NoDenouncementOpen`/`BallotNotOpen`, not a misleading
    // `InvalidBallotTarget` for a candidate that was never the real
    // problem.
    let candidates: &[PlayerId] = match state.denouncement.as_ref().map(|d| &d.phase) {
        Some(DenouncementPhase::Ballot { surfaced, .. }) => surfaced,
        Some(DenouncementPhase::Runoff { candidates, .. }) => candidates,
        Some(_) => return Err(GameError::BallotNotOpen),
        None => return Err(GameError::NoDenouncementOpen),
    };
    if let Ballot::For(candidate) = ballot {
        if !candidates.contains(&candidate) {
            return Err(GameError::InvalidBallotTarget(candidate));
        }
    }

    let denouncement = state.denouncement.as_mut().unwrap();
    match &mut denouncement.phase {
        DenouncementPhase::Ballot { ballots, .. } | DenouncementPhase::Runoff { ballots, .. } => {
            ballots.insert(voter, ballot);
        }
        _ => unreachable!("phase was already confirmed to be Ballot or Runoff above"),
    }
    Ok(vec![DomainEvent::BallotCast { voter, ballot }])
}

/// Tallies `ballots` against `candidates`, counting only `Ballot::For`
/// votes -- `Abstain` (and any stray vote for a non-candidate, which
/// `cast_ballot` should already have rejected) simply don't add to anyone's
/// count. Every candidate gets an entry, including 0, so
/// `denouncement::resolve_ballot` can distinguish "nobody voted for them"
/// from "they were never a candidate at all."
fn tally_ballots(
    candidates: &[PlayerId],
    ballots: &BTreeMap<PlayerId, Ballot>,
) -> BTreeMap<PlayerId, u32> {
    let mut tally: BTreeMap<PlayerId, u32> = candidates.iter().map(|&id| (id, 0)).collect();
    for b in ballots.values() {
        if let Ballot::For(candidate) = b {
            if let Some(count) = tally.get_mut(candidate) {
                *count += 1;
            }
        }
    }
    tally
}

fn close_ballot(
    state: &mut GameState,
    fallback_replacement: Option<PlayerId>,
) -> Result<Vec<DomainEvent>, GameError> {
    // Clone what's needed out of the borrowed phase and release it
    // immediately -- `resolve_cast_out` below needs `&mut GameState`, which
    // can't coexist with a live borrow into `state.denouncement`.
    let (surfaced, ballots) = match state.denouncement.as_ref().map(|d| &d.phase) {
        Some(DenouncementPhase::Ballot { surfaced, ballots }) => {
            (surfaced.clone(), ballots.clone())
        }
        _ => return Err(GameError::BallotNotOpen),
    };

    let tally = tally_ballots(&surfaced, &ballots);
    let slots = execution_count(state.competing_player_count());
    let resolution = resolve_ballot(&tally, slots);

    let mut events = Vec::new();
    if resolution.tied_for_last_slot.is_empty() {
        let cast_out = resolution.locked_in;
        events.push(DomainEvent::BallotClosed {
            cast_out: cast_out.clone(),
        });
        for &player in &cast_out {
            // A player who was voted out independently can *also* be swept
            // by another cast-out's own cascade within this same batch --
            // e.g. the King/Queen and Prince/Princess both surfacing and
            // both getting a slot in the same multi-slot Round 3
            // Denouncement: casting out the King/Queen already casts out
            // the Prince/Princess too (rules.md §5). Skip anyone the loop
            // has already resolved this way instead of re-resolving them --
            // `resolve_cast_out` correctly rejects an already-inactive
            // target, and letting that rejection propagate here would
            // abort the whole command partway through, leaving `state`
            // mutated despite returning `Err` (violating this function's
            // own "no mutation on error" contract). Passing `&cast_out` as
            // `also_departing` additionally keeps a title cascade from
            // crowning/electing someone else *also* in this same batch --
            // see `resolve_cast_out`'s doc comment.
            if !state.is_active(player) {
                continue;
            }
            events.extend(resolve_cast_out(
                state,
                player,
                fallback_replacement,
                &cast_out,
            )?);
        }
        state.denouncement = None;
    } else {
        let slots_remaining = slots - resolution.locked_in.len();
        let candidates = resolution.tied_for_last_slot;
        let already_locked_in = resolution.locked_in;
        state.denouncement.as_mut().unwrap().phase = DenouncementPhase::Runoff {
            candidates: candidates.clone(),
            slots_remaining,
            already_locked_in,
            ballots: BTreeMap::new(),
        };
        events.push(DomainEvent::RunoffOpened {
            candidates,
            slots_remaining,
        });
    }
    Ok(events)
}

fn close_runoff(
    state: &mut GameState,
    fallback_replacement: Option<PlayerId>,
) -> Result<Vec<DomainEvent>, GameError> {
    let (candidates, slots_remaining, already_locked_in, ballots) =
        match state.denouncement.as_ref().map(|d| &d.phase) {
            Some(DenouncementPhase::Runoff {
                candidates,
                slots_remaining,
                already_locked_in,
                ballots,
            }) => (
                candidates.clone(),
                *slots_remaining,
                already_locked_in.clone(),
                ballots.clone(),
            ),
            _ => return Err(GameError::RunoffNotOpen),
        };

    let tally = tally_ballots(&candidates, &ballots);
    let resolution = resolve_ballot(&tally, slots_remaining);
    // A repeat tie -- rules.md: "no one is Denounced for that slot." No
    // second runoff; whatever's left in `tied_for_last_slot` is simply
    // dropped, while `already_locked_in` (from the original ballot) and
    // anything the runoff *did* resolve cleanly still go through.
    let unfilled_slot = !resolution.tied_for_last_slot.is_empty();

    let mut cast_out = already_locked_in;
    cast_out.extend(resolution.locked_in);

    let mut events = vec![DomainEvent::RunoffClosed {
        cast_out: cast_out.clone(),
        unfilled_slot,
    }];
    for &player in &cast_out {
        // See the identical guard + `also_departing` argument in
        // `close_ballot` -- a player already swept by another cast-out's
        // own cascade earlier in this same batch must be skipped, not
        // re-resolved, and no cascade in this batch may crown/elect anyone
        // else who's also in it.
        if !state.is_active(player) {
            continue;
        }
        events.extend(resolve_cast_out(
            state,
            player,
            fallback_replacement,
            &cast_out,
        )?);
    }
    state.denouncement = None;
    Ok(events)
}

fn push_task(
    state: &mut GameState,
    prompt: String,
    tier: TaskTier,
    qualifying_players: BTreeSet<PlayerId>,
) -> Vec<DomainEvent> {
    let id = TaskId(state.next_task_id);
    state.next_task_id += 1;
    state.tasks.insert(
        id,
        TaskDef {
            id,
            prompt: prompt.clone(),
            tier,
            qualifying_players,
        },
    );
    state.open_tasks.insert(id);
    vec![DomainEvent::TaskPushed { id, prompt, tier }]
}

fn close_tasks(state: &mut GameState) -> Vec<DomainEvent> {
    let closed: Vec<TaskId> = state.open_tasks.iter().copied().collect();
    state.open_tasks.clear();
    vec![DomainEvent::TasksClosed { closed }]
}

fn attempt_task(
    state: &mut GameState,
    player: PlayerId,
    task: TaskId,
    named: [PlayerId; 3],
) -> Result<Vec<DomainEvent>, GameError> {
    if !state.is_active(player) {
        return Err(GameError::NotActive(player));
    }
    let def = state.tasks.get(&task).ok_or(GameError::UnknownTask(task))?;
    if !state.open_tasks.contains(&task) {
        return Err(GameError::TaskNotOpen(task));
    }
    if state.task_attempts.contains_key(&(player, task)) {
        return Err(GameError::AlreadyAttemptedTask { player, task });
    }
    if named.contains(&player) {
        return Err(GameError::CannotNameSelfForTask);
    }
    let mut distinct = BTreeSet::new();
    if named.iter().any(|id| !distinct.insert(*id)) {
        return Err(GameError::DuplicateNamedPlayerForTask);
    }
    for &id in &named {
        if !state.is_active(id) {
            return Err(GameError::NotActive(id));
        }
    }

    let credited = named.iter().any(|id| def.qualifying_players.contains(id));
    state.task_attempts.insert((player, task), credited);
    Ok(vec![DomainEvent::TaskAttempted {
        player,
        task,
        credited,
    }])
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

    // --- The Denouncement procedure ---

    /// `setup_full_game` plus `extra` additional Ton players with no
    /// title, giving a realistic pool of voters/nominees for exercising
    /// the Denouncement procedure without every test needing its own
    /// bespoke roster.
    fn setup_game_with_extra_voters(extra: usize) -> (GameState, Vec<PlayerId>) {
        let (mut state, king_queen, prince, leader, cult_leader) = setup_full_game();
        let mut everyone = vec![king_queen, prince, leader, cult_leader];
        for i in 0..extra {
            everyone.push(add_player(&mut state, &format!("Extra{i}"), Faction::Ton));
        }
        apply_command(&mut state, Command::FinalizeSetup).unwrap();
        (state, everyone)
    }

    #[test]
    fn open_denouncement_rejects_a_second_one_while_one_is_active() {
        let (mut state, ..) = setup_game_with_extra_voters(2);
        apply_command(&mut state, Command::OpenDenouncement).unwrap();
        let result = apply_command(&mut state, Command::OpenDenouncement);
        assert_eq!(result, Err(GameError::DenouncementAlreadyOpen));
    }

    #[test]
    fn nominate_rejects_when_no_denouncement_is_open() {
        let (mut state, everyone) = setup_game_with_extra_voters(2);
        let result = apply_command(
            &mut state,
            Command::Nominate {
                voter: everyone[0],
                nominee: everyone[1],
            },
        );
        assert_eq!(result, Err(GameError::NoDenouncementOpen));
    }

    #[test]
    fn nominate_rejects_an_inactive_voter_or_nominee() {
        let (mut state, everyone) = setup_game_with_extra_voters(2);
        apply_command(&mut state, Command::OpenDenouncement).unwrap();
        apply_command(
            &mut state,
            Command::CastOut {
                player: everyone[4],
                fallback_replacement: None,
            },
        )
        .unwrap();

        let result = apply_command(
            &mut state,
            Command::Nominate {
                voter: everyone[4],
                nominee: everyone[0],
            },
        );
        assert_eq!(result, Err(GameError::NotActive(everyone[4])));

        let result = apply_command(
            &mut state,
            Command::Nominate {
                voter: everyone[0],
                nominee: everyone[4],
            },
        );
        assert_eq!(result, Err(GameError::NotActive(everyone[4])));
    }

    #[test]
    fn re_nominating_replaces_the_voters_earlier_choice() {
        let (mut state, everyone) = setup_game_with_extra_voters(2);
        apply_command(&mut state, Command::OpenDenouncement).unwrap();
        apply_command(
            &mut state,
            Command::Nominate {
                voter: everyone[0],
                nominee: everyone[1],
            },
        )
        .unwrap();
        apply_command(
            &mut state,
            Command::Nominate {
                voter: everyone[0],
                nominee: everyone[2],
            },
        )
        .unwrap();

        apply_command(&mut state, Command::CloseNomination).unwrap();
        // Only everyone[2] should have a nomination -- everyone[1] never
        // surfaces since the replaced vote for them was overwritten, not
        // added to.
        match state.denouncement_phase() {
            Some(DenouncementPhase::Discussion { surfaced }) => {
                assert!(surfaced.contains(&everyone[2]));
                assert!(!surfaced.contains(&everyone[1]));
            }
            other => panic!("expected Discussion phase, got {other:?}"),
        }
    }

    #[test]
    fn close_nomination_rejects_outside_the_nomination_phase() {
        let (mut state, ..) = setup_game_with_extra_voters(2);
        apply_command(&mut state, Command::OpenDenouncement).unwrap();
        apply_command(&mut state, Command::CloseNomination).unwrap();
        let result = apply_command(&mut state, Command::CloseNomination);
        assert_eq!(result, Err(GameError::NominationNotOpen));
    }

    #[test]
    fn nominate_rejects_outside_the_nomination_phase() {
        let (mut state, everyone) = setup_game_with_extra_voters(2);
        apply_command(&mut state, Command::OpenDenouncement).unwrap();
        apply_command(&mut state, Command::CloseNomination).unwrap();
        let result = apply_command(
            &mut state,
            Command::Nominate {
                voter: everyone[0],
                nominee: everyone[1],
            },
        );
        assert_eq!(result, Err(GameError::NominationNotOpen));
    }

    #[test]
    fn open_ballot_rejects_outside_the_discussion_phase() {
        let (mut state, ..) = setup_game_with_extra_voters(2);
        apply_command(&mut state, Command::OpenDenouncement).unwrap();
        let result = apply_command(&mut state, Command::OpenBallot);
        assert_eq!(result, Err(GameError::DiscussionNotOpen));
    }

    #[test]
    fn cast_ballot_rejects_when_no_denouncement_is_open() {
        let (mut state, everyone) = setup_game_with_extra_voters(2);
        let result = apply_command(
            &mut state,
            Command::CastBallot {
                voter: everyone[0],
                ballot: Ballot::Abstain,
            },
        );
        assert_eq!(result, Err(GameError::NoDenouncementOpen));
    }

    #[test]
    fn cast_ballot_rejects_during_nomination_or_discussion() {
        let (mut state, everyone) = setup_game_with_extra_voters(2);
        apply_command(&mut state, Command::OpenDenouncement).unwrap();
        let result = apply_command(
            &mut state,
            Command::CastBallot {
                voter: everyone[0],
                ballot: Ballot::Abstain,
            },
        );
        assert_eq!(result, Err(GameError::BallotNotOpen));

        apply_command(
            &mut state,
            Command::Nominate {
                voter: everyone[0],
                nominee: everyone[0],
            },
        )
        .unwrap();
        apply_command(&mut state, Command::CloseNomination).unwrap();
        let result = apply_command(
            &mut state,
            Command::CastBallot {
                voter: everyone[0],
                ballot: Ballot::Abstain,
            },
        );
        assert_eq!(result, Err(GameError::BallotNotOpen));
    }

    #[test]
    fn close_ballot_rejects_outside_the_ballot_phase() {
        let (mut state, ..) = setup_game_with_extra_voters(2);
        apply_command(&mut state, Command::OpenDenouncement).unwrap();
        let result = apply_command(
            &mut state,
            Command::CloseBallot {
                fallback_replacement: None,
            },
        );
        assert_eq!(result, Err(GameError::BallotNotOpen));
    }

    #[test]
    fn close_runoff_rejects_outside_the_runoff_phase() {
        let (mut state, ..) = setup_game_with_extra_voters(2);
        apply_command(&mut state, Command::OpenDenouncement).unwrap();
        let result = apply_command(
            &mut state,
            Command::CloseRunoff {
                fallback_replacement: None,
            },
        );
        assert_eq!(result, Err(GameError::RunoffNotOpen));
    }

    #[test]
    fn cast_ballot_rejects_a_target_who_never_surfaced() {
        let (mut state, everyone) = setup_game_with_extra_voters(3);
        apply_command(&mut state, Command::OpenDenouncement).unwrap();
        // Nominate only everyone[0..3] so everyone[3] (a 4th extra) never
        // surfaces.
        for voter in &everyone[0..3] {
            apply_command(
                &mut state,
                Command::Nominate {
                    voter: *voter,
                    nominee: everyone[0],
                },
            )
            .unwrap();
        }
        apply_command(&mut state, Command::CloseNomination).unwrap();
        apply_command(&mut state, Command::OpenBallot).unwrap();

        let never_nominated = everyone[6]; // one of the extras, never nominated
        let result = apply_command(
            &mut state,
            Command::CastBallot {
                voter: everyone[0],
                ballot: Ballot::For(never_nominated),
            },
        );
        assert_eq!(result, Err(GameError::InvalidBallotTarget(never_nominated)));
    }

    #[test]
    fn cast_ballot_rejects_an_inactive_voter() {
        let (mut state, everyone) = setup_game_with_extra_voters(3);
        apply_command(&mut state, Command::OpenDenouncement).unwrap();
        apply_command(
            &mut state,
            Command::Nominate {
                voter: everyone[0],
                nominee: everyone[1],
            },
        )
        .unwrap();
        apply_command(&mut state, Command::CloseNomination).unwrap();
        apply_command(&mut state, Command::OpenBallot).unwrap();

        apply_command(
            &mut state,
            Command::CastOut {
                player: everyone[2],
                fallback_replacement: None,
            },
        )
        .unwrap();
        let result = apply_command(
            &mut state,
            Command::CastBallot {
                voter: everyone[2],
                ballot: Ballot::Abstain,
            },
        );
        assert_eq!(result, Err(GameError::NotActive(everyone[2])));
    }

    #[test]
    fn abstaining_never_counts_toward_any_candidates_tally() {
        let (mut state, everyone) = setup_game_with_extra_voters(2);
        // 6 players total; ≤20 competing players means 1 slot.
        apply_command(&mut state, Command::OpenDenouncement).unwrap();
        for voter in &everyone {
            apply_command(
                &mut state,
                Command::Nominate {
                    voter: *voter,
                    nominee: everyone[0],
                },
            )
            .unwrap();
        }
        apply_command(&mut state, Command::CloseNomination).unwrap();
        apply_command(&mut state, Command::OpenBallot).unwrap();

        for voter in &everyone {
            apply_command(
                &mut state,
                Command::CastBallot {
                    voter: *voter,
                    ballot: Ballot::Abstain,
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
        assert_eq!(
            events.first(),
            Some(&DomainEvent::BallotClosed { cast_out: vec![] }),
            "an all-abstain ballot must Cast Out no one, not default to the only nominee"
        );
        assert_eq!(
            state.denouncement_phase(),
            None,
            "the Denouncement still closes even with no result"
        );
    }

    #[test]
    fn a_full_clean_denouncement_cast_outs_the_clear_winner_and_closes() {
        let (mut state, everyone) = setup_game_with_extra_voters(3);
        let target = everyone[1];

        apply_command(&mut state, Command::OpenDenouncement).unwrap();
        for voter in &everyone {
            apply_command(
                &mut state,
                Command::Nominate {
                    voter: *voter,
                    nominee: target,
                },
            )
            .unwrap();
        }
        apply_command(&mut state, Command::CloseNomination).unwrap();
        apply_command(&mut state, Command::OpenBallot).unwrap();
        for voter in &everyone {
            apply_command(
                &mut state,
                Command::CastBallot {
                    voter: *voter,
                    ballot: Ballot::For(target),
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

        assert_eq!(
            events.first(),
            Some(&DomainEvent::BallotClosed {
                cast_out: vec![target]
            })
        );
        assert!(events.contains(&DomainEvent::PlayerCastOut { player: target }));
        assert_eq!(state.player(target).unwrap().status, PlayerStatus::CastOut);
        assert_eq!(state.denouncement_phase(), None);
    }

    #[test]
    fn a_denouncement_cast_out_still_triggers_the_full_cascade() {
        // Integration check: the procedure hands off to the *same*
        // resolve_cast_out cascades Command::CastOut already exercises --
        // Casting Out the Revolutionary Leader through a real Denouncement
        // must still trigger succession.
        let (mut state, everyone) = setup_game_with_extra_voters(0);
        let leader = state.revolutionary_leader().unwrap();
        let ally = add_player(&mut state, "Ally", Faction::Uprising);
        apply_command(&mut state, Command::FinalizeSetup).unwrap();

        apply_command(&mut state, Command::OpenDenouncement).unwrap();
        for voter in everyone.iter().chain([&ally]) {
            apply_command(
                &mut state,
                Command::Nominate {
                    voter: *voter,
                    nominee: leader,
                },
            )
            .unwrap();
        }
        apply_command(&mut state, Command::CloseNomination).unwrap();
        apply_command(&mut state, Command::OpenBallot).unwrap();
        for voter in everyone.iter().chain([&ally]) {
            apply_command(
                &mut state,
                Command::CastBallot {
                    voter: *voter,
                    ballot: Ballot::For(leader),
                },
            )
            .unwrap();
        }
        apply_command(
            &mut state,
            Command::CloseBallot {
                fallback_replacement: Some(ally),
            },
        )
        .unwrap();

        assert_eq!(state.player(leader).unwrap().status, PlayerStatus::CastOut);
        assert_eq!(state.revolutionary_leader(), Some(ally));
        assert!(state.revolutionary_leader_ever_denounced_unconverted());
    }

    #[test]
    fn a_tie_for_the_only_slot_opens_a_runoff_instead_of_resolving() {
        let (mut state, everyone) = setup_game_with_extra_voters(3);
        let a = everyone[0];
        let b = everyone[1];

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

        // 3 votes each -- an exact tie for the single execution slot.
        for voter in &everyone[0..3] {
            apply_command(
                &mut state,
                Command::CastBallot {
                    voter: *voter,
                    ballot: Ballot::For(a),
                },
            )
            .unwrap();
        }
        for voter in &everyone[3..6] {
            apply_command(
                &mut state,
                Command::CastBallot {
                    voter: *voter,
                    ballot: Ballot::For(b),
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

        match state.denouncement_phase() {
            Some(DenouncementPhase::Runoff {
                candidates,
                slots_remaining,
                already_locked_in,
                ..
            }) => {
                let mut sorted = candidates.clone();
                sorted.sort();
                let mut expected = vec![a, b];
                expected.sort();
                assert_eq!(sorted, expected);
                assert_eq!(*slots_remaining, 1);
                assert!(already_locked_in.is_empty());
            }
            other => panic!("expected Runoff phase, got {other:?}"),
        }
        assert!(matches!(
            events.first(),
            Some(DomainEvent::RunoffOpened { .. })
        ));
        // Nobody is Cast Out yet -- the tie is unresolved.
        assert_eq!(state.player(a).unwrap().status, PlayerStatus::Active);
        assert_eq!(state.player(b).unwrap().status, PlayerStatus::Active);
    }

    #[test]
    fn a_clean_runoff_resolves_and_closes_the_denouncement() {
        let (mut state, everyone) = setup_game_with_extra_voters(3);
        let a = everyone[0];
        let b = everyone[1];

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
        for voter in &everyone[0..3] {
            apply_command(
                &mut state,
                Command::CastBallot {
                    voter: *voter,
                    ballot: Ballot::For(a),
                },
            )
            .unwrap();
        }
        for voter in &everyone[3..6] {
            apply_command(
                &mut state,
                Command::CastBallot {
                    voter: *voter,
                    ballot: Ballot::For(b),
                },
            )
            .unwrap();
        }
        apply_command(
            &mut state,
            Command::CloseBallot {
                fallback_replacement: None,
            },
        )
        .unwrap();

        // Runoff: everyone breaks for `a`.
        for voter in &everyone {
            apply_command(
                &mut state,
                Command::CastBallot {
                    voter: *voter,
                    ballot: Ballot::For(a),
                },
            )
            .unwrap();
        }
        let events = apply_command(
            &mut state,
            Command::CloseRunoff {
                fallback_replacement: None,
            },
        )
        .unwrap();

        assert_eq!(
            events.first(),
            Some(&DomainEvent::RunoffClosed {
                cast_out: vec![a],
                unfilled_slot: false,
            })
        );
        assert_eq!(state.player(a).unwrap().status, PlayerStatus::CastOut);
        assert_eq!(state.player(b).unwrap().status, PlayerStatus::Active);
        assert_eq!(state.denouncement_phase(), None);
    }

    #[test]
    fn a_repeat_tie_in_the_runoff_leaves_the_slot_unfilled_but_still_closes() {
        let (mut state, everyone) = setup_game_with_extra_voters(3);
        let a = everyone[0];
        let b = everyone[1];

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
        for voter in &everyone[0..3] {
            apply_command(
                &mut state,
                Command::CastBallot {
                    voter: *voter,
                    ballot: Ballot::For(a),
                },
            )
            .unwrap();
        }
        for voter in &everyone[3..6] {
            apply_command(
                &mut state,
                Command::CastBallot {
                    voter: *voter,
                    ballot: Ballot::For(b),
                },
            )
            .unwrap();
        }
        apply_command(
            &mut state,
            Command::CloseBallot {
                fallback_replacement: None,
            },
        )
        .unwrap();

        // Runoff ties again: 3 votes each.
        for voter in &everyone[0..3] {
            apply_command(
                &mut state,
                Command::CastBallot {
                    voter: *voter,
                    ballot: Ballot::For(a),
                },
            )
            .unwrap();
        }
        for voter in &everyone[3..6] {
            apply_command(
                &mut state,
                Command::CastBallot {
                    voter: *voter,
                    ballot: Ballot::For(b),
                },
            )
            .unwrap();
        }
        let events = apply_command(
            &mut state,
            Command::CloseRunoff {
                fallback_replacement: None,
            },
        )
        .unwrap();

        assert_eq!(
            events.first(),
            Some(&DomainEvent::RunoffClosed {
                cast_out: vec![],
                unfilled_slot: true,
            })
        );
        assert_eq!(state.player(a).unwrap().status, PlayerStatus::Active);
        assert_eq!(state.player(b).unwrap().status, PlayerStatus::Active);
        assert_eq!(
            state.denouncement_phase(),
            None,
            "a repeat tie still closes the Denouncement -- no second runoff"
        );
    }

    #[test]
    fn a_runoff_still_cast_outs_candidates_already_locked_in_from_the_original_ballot() {
        // 21 competing players -> execution_count() == 2. A gets a clear
        // majority (locked in immediately); B and C tie for the second
        // slot and go to a runoff. Closing the runoff must still Cast Out
        // A even though A was never part of the runoff ballot itself.
        let (mut state, everyone) = setup_game_with_extra_voters(17); // 4 + 17 = 21
        assert_eq!(state.competing_player_count(), 21);
        let a = everyone[0];
        let b = everyone[1];
        let c = everyone[2];

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
        apply_command(
            &mut state,
            Command::Nominate {
                voter: c,
                nominee: c,
            },
        )
        .unwrap();
        apply_command(&mut state, Command::CloseNomination).unwrap();
        apply_command(&mut state, Command::OpenBallot).unwrap();

        // A: 10 votes (clear majority slot). B and C: 5 votes each (tied
        // for the remaining slot). 4 players (index 18,19,20 + one more)
        // abstain to keep totals honest across 21 voters.
        for voter in &everyone[0..10] {
            apply_command(
                &mut state,
                Command::CastBallot {
                    voter: *voter,
                    ballot: Ballot::For(a),
                },
            )
            .unwrap();
        }
        for voter in &everyone[10..15] {
            apply_command(
                &mut state,
                Command::CastBallot {
                    voter: *voter,
                    ballot: Ballot::For(b),
                },
            )
            .unwrap();
        }
        for voter in &everyone[15..20] {
            apply_command(
                &mut state,
                Command::CastBallot {
                    voter: *voter,
                    ballot: Ballot::For(c),
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
        assert!(matches!(
            events.first(),
            Some(DomainEvent::RunoffOpened { .. })
        ));
        // Cast-Outs are deliberately batched: even though A is already
        // locked in, A stays Active until the whole Denouncement closes --
        // everyone Denounced this round is revealed together, not
        // incrementally as each slot resolves.
        assert_eq!(state.player(a).unwrap().status, PlayerStatus::Active);
        match state.denouncement_phase() {
            Some(DenouncementPhase::Runoff {
                already_locked_in,
                slots_remaining,
                ..
            }) => {
                assert_eq!(already_locked_in, &vec![a]);
                assert_eq!(*slots_remaining, 1);
            }
            other => panic!("expected Runoff phase, got {other:?}"),
        }

        // Runoff breaks for B.
        for voter in &everyone[0..21] {
            apply_command(
                &mut state,
                Command::CastBallot {
                    voter: *voter,
                    ballot: Ballot::For(b),
                },
            )
            .unwrap();
        }
        let events = apply_command(
            &mut state,
            Command::CloseRunoff {
                fallback_replacement: None,
            },
        )
        .unwrap();

        let mut cast_out = match &events[0] {
            DomainEvent::RunoffClosed {
                cast_out,
                unfilled_slot,
            } => {
                assert!(!unfilled_slot);
                cast_out.clone()
            }
            other => panic!("expected RunoffClosed, got {other:?}"),
        };
        cast_out.sort();
        let mut expected = vec![a, b];
        expected.sort();
        assert_eq!(cast_out, expected);
        assert_eq!(state.player(a).unwrap().status, PlayerStatus::CastOut);
        assert_eq!(state.player(b).unwrap().status, PlayerStatus::CastOut);
        assert_eq!(state.player(c).unwrap().status, PlayerStatus::Active);
        assert_eq!(state.denouncement_phase(), None);
    }

    #[test]
    fn a_multi_slot_round_three_denouncement_can_cast_out_the_king_queen_and_prince_princess_together(
    ) {
        // Regression test: at 21+ competing players, execution_count() is
        // 2. If the King/Queen AND the Prince/Princess both surface and
        // both win a slot in the *same* Round 3 Denouncement, resolving
        // the King/Queen's own Cast-Out cascade (rules.md §5) already
        // Casts Out the Prince/Princess directly -- so by the time the
        // batch loop in `close_ballot` reaches the Prince/Princess as its
        // own, independently-voted-out slot, they're already inactive.
        // Before the fix, `resolve_cast_out` correctly rejected that as
        // `NotActive`, but that error propagated out of `close_ballot`
        // *after* the King/Queen's cascade had already mutated `state` --
        // silently violating `apply_command`'s "on error, state is left
        // unchanged" contract. This must now resolve cleanly instead.
        let (mut state, everyone) = setup_game_with_extra_voters(17); // 4 + 17 = 21
        assert_eq!(state.competing_player_count(), 21);
        let king_queen = everyone[0];
        let prince_princess = everyone[1];

        apply_command(&mut state, Command::AdvanceRound).unwrap(); // -> Two
        apply_command(&mut state, Command::AdvanceRound).unwrap(); // -> Three

        apply_command(&mut state, Command::OpenDenouncement).unwrap();
        for &voter in &everyone[4..14] {
            apply_command(
                &mut state,
                Command::Nominate {
                    voter,
                    nominee: king_queen,
                },
            )
            .unwrap();
        }
        for &voter in &everyone[14..21] {
            apply_command(
                &mut state,
                Command::Nominate {
                    voter,
                    nominee: prince_princess,
                },
            )
            .unwrap();
        }
        apply_command(&mut state, Command::CloseNomination).unwrap();
        match state.denouncement_phase() {
            Some(DenouncementPhase::Discussion { surfaced }) => {
                let mut sorted = surfaced.clone();
                sorted.sort();
                let mut expected = vec![king_queen, prince_princess];
                expected.sort();
                assert_eq!(sorted, expected);
            }
            other => panic!("expected Discussion phase, got {other:?}"),
        }

        apply_command(&mut state, Command::OpenBallot).unwrap();
        for &voter in &everyone[4..14] {
            apply_command(
                &mut state,
                Command::CastBallot {
                    voter,
                    ballot: Ballot::For(king_queen),
                },
            )
            .unwrap();
        }
        for &voter in &everyone[14..21] {
            apply_command(
                &mut state,
                Command::CastBallot {
                    voter,
                    ballot: Ballot::For(prince_princess),
                },
            )
            .unwrap();
        }

        let result = apply_command(
            &mut state,
            Command::CloseBallot {
                fallback_replacement: None,
            },
        );
        assert!(
            result.is_ok(),
            "closing a multi-slot Denouncement that catches both the King/Queen and Prince/Princess \
             must not error: {result:?}"
        );
        assert_eq!(
            state.player(king_queen).unwrap().status,
            PlayerStatus::CastOut
        );
        assert_eq!(
            state.player(prince_princess).unwrap().status,
            PlayerStatus::CastOut
        );
        // The Round-3 cascade still installs a new King/Queen from the
        // remaining untitled Ton pool.
        assert!(state.king_queen().is_some());
        assert_ne!(state.king_queen(), Some(king_queen));
    }

    #[test]
    fn servants_can_nominate_and_vote_despite_being_excluded_from_headcount_scaling() {
        let (mut state, everyone) = setup_game_with_extra_voters(2);
        let servant = add_player(&mut state, "Servant", Faction::Servant);
        apply_command(&mut state, Command::FinalizeSetup).unwrap();

        // Servants never get a competing-faction character, so
        // competing_player_count must not include them.
        let with_servant = state.competing_player_count();
        assert_eq!(with_servant, everyone.len());

        apply_command(&mut state, Command::OpenDenouncement).unwrap();
        apply_command(
            &mut state,
            Command::Nominate {
                voter: servant,
                nominee: everyone[0],
            },
        )
        .unwrap();
        apply_command(
            &mut state,
            Command::Nominate {
                voter: everyone[0],
                nominee: everyone[0],
            },
        )
        .unwrap();
        apply_command(&mut state, Command::CloseNomination).unwrap();
        match state.denouncement_phase() {
            Some(DenouncementPhase::Discussion { surfaced }) => {
                assert!(surfaced.contains(&everyone[0]));
            }
            other => panic!("expected Discussion phase, got {other:?}"),
        }

        apply_command(&mut state, Command::OpenBallot).unwrap();
        let result = apply_command(
            &mut state,
            Command::CastBallot {
                voter: servant,
                ballot: Ballot::For(everyone[0]),
            },
        );
        assert!(result.is_ok(), "a Servant must be able to vote: {result:?}");
    }

    // --- The task system ---

    fn push_task(
        state: &mut GameState,
        prompt: &str,
        tier: TaskTier,
        qualifying: &[PlayerId],
    ) -> TaskId {
        let events = apply_command(
            state,
            Command::PushTask {
                prompt: prompt.into(),
                tier,
                qualifying_players: qualifying.iter().copied().collect(),
            },
        )
        .unwrap();
        match events.as_slice() {
            [DomainEvent::TaskPushed { id, .. }] => *id,
            other => panic!("expected a single TaskPushed event, got {other:?}"),
        }
    }

    #[test]
    fn push_task_assigns_sequential_ids_and_opens_it() {
        let (mut state, everyone) = setup_game_with_extra_voters(0);
        let first = push_task(
            &mut state,
            "Talk to someone in a mask",
            TaskTier::Easy,
            &everyone,
        );
        let second = push_task(&mut state, "Talk to a dancer", TaskTier::Medium, &everyone);
        assert_ne!(first, second);
        assert!(state.is_task_open(first));
        assert!(state.is_task_open(second));
        assert_eq!(state.task(first).unwrap().tier, TaskTier::Easy);
        assert_eq!(state.task(second).unwrap().tier, TaskTier::Medium);
    }

    #[test]
    fn close_tasks_locks_every_open_task_and_is_a_no_op_when_none_are_open() {
        let (mut state, everyone) = setup_game_with_extra_voters(0);
        let a = push_task(&mut state, "A", TaskTier::Easy, &everyone);
        let b = push_task(&mut state, "B", TaskTier::Medium, &everyone);

        let events = apply_command(&mut state, Command::CloseTasks).unwrap();
        match events.as_slice() {
            [DomainEvent::TasksClosed { closed }] => {
                let mut sorted = closed.clone();
                sorted.sort_by_key(|t| t.0);
                let mut expected = vec![a, b];
                expected.sort_by_key(|t| t.0);
                assert_eq!(sorted, expected);
            }
            other => panic!("expected a single TasksClosed event, got {other:?}"),
        }
        assert!(!state.is_task_open(a));
        assert!(!state.is_task_open(b));
        // Closed tasks are still known (for later completion-rate lookups)
        // -- they just aren't attemptable any more.
        assert!(state.task(a).is_some());

        let events = apply_command(&mut state, Command::CloseTasks).unwrap();
        assert_eq!(events, vec![DomainEvent::TasksClosed { closed: vec![] }]);
    }

    #[test]
    fn attempt_task_credits_when_any_named_player_is_in_the_qualifying_set() {
        let (mut state, everyone) = setup_game_with_extra_voters(2);
        let qualifies = everyone[3]; // one of the extras
        let task = push_task(&mut state, "Talk to someone", TaskTier::Easy, &[qualifies]);

        let attempter = everyone[0];
        let named = [everyone[1], everyone[2], qualifies];
        let events = apply_command(
            &mut state,
            Command::AttemptTask {
                player: attempter,
                task,
                named,
            },
        )
        .unwrap();
        assert_eq!(
            events,
            vec![DomainEvent::TaskAttempted {
                player: attempter,
                task,
                credited: true,
            }]
        );
        assert_eq!(state.task_attempt(attempter, task), Some(true));
    }

    #[test]
    fn attempt_task_does_not_credit_when_no_named_player_qualifies() {
        let (mut state, everyone) = setup_game_with_extra_voters(2);
        let task = push_task(
            &mut state,
            "Talk to someone",
            TaskTier::Easy,
            &[everyone[3]],
        );

        let attempter = everyone[0];
        let named = [everyone[1], everyone[2], everyone[4]];
        let events = apply_command(
            &mut state,
            Command::AttemptTask {
                player: attempter,
                task,
                named,
            },
        )
        .unwrap();
        assert_eq!(
            events,
            vec![DomainEvent::TaskAttempted {
                player: attempter,
                task,
                credited: false,
            }]
        );
        assert_eq!(state.task_attempt(attempter, task), Some(false));
    }

    #[test]
    fn attempt_task_rejects_an_inactive_player() {
        let (mut state, everyone) = setup_game_with_extra_voters(2);
        let task = push_task(&mut state, "Talk to someone", TaskTier::Easy, &everyone);
        apply_command(
            &mut state,
            Command::CastOut {
                player: everyone[0],
                fallback_replacement: None,
            },
        )
        .unwrap();
        let result = apply_command(
            &mut state,
            Command::AttemptTask {
                player: everyone[0],
                task,
                named: [everyone[1], everyone[2], everyone[3]],
            },
        );
        assert_eq!(result, Err(GameError::NotActive(everyone[0])));
    }

    #[test]
    fn attempt_task_rejects_an_unknown_task() {
        let (mut state, everyone) = setup_game_with_extra_voters(2);
        let bogus = TaskId(9999);
        let result = apply_command(
            &mut state,
            Command::AttemptTask {
                player: everyone[0],
                task: bogus,
                named: [everyone[1], everyone[2], everyone[3]],
            },
        );
        assert_eq!(result, Err(GameError::UnknownTask(bogus)));
    }

    #[test]
    fn attempt_task_rejects_a_closed_task() {
        let (mut state, everyone) = setup_game_with_extra_voters(2);
        let task = push_task(&mut state, "Talk to someone", TaskTier::Easy, &everyone);
        apply_command(&mut state, Command::CloseTasks).unwrap();
        let result = apply_command(
            &mut state,
            Command::AttemptTask {
                player: everyone[0],
                task,
                named: [everyone[1], everyone[2], everyone[3]],
            },
        );
        assert_eq!(result, Err(GameError::TaskNotOpen(task)));
    }

    #[test]
    fn attempt_task_rejects_a_second_attempt_at_the_same_task() {
        let (mut state, everyone) = setup_game_with_extra_voters(2);
        let task = push_task(&mut state, "Talk to someone", TaskTier::Easy, &everyone);
        let named = [everyone[1], everyone[2], everyone[3]];
        apply_command(
            &mut state,
            Command::AttemptTask {
                player: everyone[0],
                task,
                named,
            },
        )
        .unwrap();
        let result = apply_command(
            &mut state,
            Command::AttemptTask {
                player: everyone[0],
                task,
                named,
            },
        );
        assert_eq!(
            result,
            Err(GameError::AlreadyAttemptedTask {
                player: everyone[0],
                task
            })
        );
    }

    #[test]
    fn attempt_task_rejects_naming_yourself() {
        let (mut state, everyone) = setup_game_with_extra_voters(2);
        let task = push_task(&mut state, "Talk to someone", TaskTier::Easy, &everyone);
        let result = apply_command(
            &mut state,
            Command::AttemptTask {
                player: everyone[0],
                task,
                named: [everyone[0], everyone[1], everyone[2]],
            },
        );
        assert_eq!(result, Err(GameError::CannotNameSelfForTask));
    }

    #[test]
    fn attempt_task_rejects_duplicate_named_players() {
        let (mut state, everyone) = setup_game_with_extra_voters(2);
        let task = push_task(&mut state, "Talk to someone", TaskTier::Easy, &everyone);
        let result = apply_command(
            &mut state,
            Command::AttemptTask {
                player: everyone[0],
                task,
                named: [everyone[1], everyone[1], everyone[2]],
            },
        );
        assert_eq!(result, Err(GameError::DuplicateNamedPlayerForTask));
    }

    #[test]
    fn a_failed_task_attempt_does_not_consume_the_one_attempt_per_task_limit() {
        let (mut state, everyone) = setup_game_with_extra_voters(2);
        let task = push_task(&mut state, "Talk to someone", TaskTier::Easy, &everyone);
        // First, a rejected attempt (naming self) -- must not count.
        apply_command(
            &mut state,
            Command::AttemptTask {
                player: everyone[0],
                task,
                named: [everyone[0], everyone[1], everyone[2]],
            },
        )
        .unwrap_err();
        assert_eq!(state.task_attempt(everyone[0], task), None);
        // A real attempt afterward must still succeed.
        let result = apply_command(
            &mut state,
            Command::AttemptTask {
                player: everyone[0],
                task,
                named: [everyone[1], everyone[2], everyone[3]],
            },
        );
        assert!(result.is_ok());
    }

    #[test]
    fn attempt_task_rejects_naming_an_inactive_player() {
        let (mut state, everyone) = setup_game_with_extra_voters(2);
        let task = push_task(&mut state, "Talk to someone", TaskTier::Easy, &everyone);
        apply_command(
            &mut state,
            Command::CastOut {
                player: everyone[3],
                fallback_replacement: None,
            },
        )
        .unwrap();
        let result = apply_command(
            &mut state,
            Command::AttemptTask {
                player: everyone[0],
                task,
                named: [everyone[1], everyone[2], everyone[3]],
            },
        );
        assert_eq!(result, Err(GameError::NotActive(everyone[3])));
    }

    // --- Regression: a batch cast-out cascade must never crown/elect
    // someone else who is *also* independently in the same batch. ---

    #[test]
    fn a_batch_cast_out_never_crowns_a_fellow_batch_member_as_king_queen() {
        // 21 competing players -> execution_count() == 2. The King/Queen
        // and an ordinary Ton player (Z) are both voted out in the same
        // Denouncement, with Z explicitly passed as the King/Queen's
        // fallback_replacement -- before the fix, resolving the King/Queen
        // first would crown Z, and then Z's own (independent) cast-out
        // would immediately re-trigger the cascade a second time, handing
        // the crown to a completely uninvolved third player.
        let (mut state, everyone) = setup_game_with_extra_voters(17); // 4 + 17 = 21
        let king_queen = everyone[0];
        let z = everyone[4];

        apply_command(&mut state, Command::AdvanceRound).unwrap(); // -> Two
        apply_command(&mut state, Command::AdvanceRound).unwrap(); // -> Three

        apply_command(&mut state, Command::OpenDenouncement).unwrap();
        apply_command(
            &mut state,
            Command::Nominate {
                voter: king_queen,
                nominee: king_queen,
            },
        )
        .unwrap();
        apply_command(
            &mut state,
            Command::Nominate {
                voter: z,
                nominee: z,
            },
        )
        .unwrap();
        apply_command(&mut state, Command::CloseNomination).unwrap();
        apply_command(&mut state, Command::OpenBallot).unwrap();
        for &voter in &everyone[0..4] {
            apply_command(
                &mut state,
                Command::CastBallot {
                    voter,
                    ballot: Ballot::For(king_queen),
                },
            )
            .unwrap();
        }
        for &voter in &everyone[4..8] {
            apply_command(
                &mut state,
                Command::CastBallot {
                    voter,
                    ballot: Ballot::For(z),
                },
            )
            .unwrap();
        }

        let events = apply_command(
            &mut state,
            Command::CloseBallot {
                fallback_replacement: Some(z),
            },
        )
        .unwrap();

        assert_eq!(
            state.player(king_queen).unwrap().status,
            PlayerStatus::CastOut
        );
        assert_eq!(state.player(z).unwrap().status, PlayerStatus::CastOut);
        // Exactly one cascade -- the double-crowning bug produced two.
        let cascades = events
            .iter()
            .filter(|e| matches!(e, DomainEvent::KingQueenCastOutCascade { .. }))
            .count();
        assert_eq!(cascades, 1, "expected exactly one cascade, got: {events:?}");

        let new_king_queen = state.king_queen();
        assert!(
            new_king_queen.is_some(),
            "the throne should have a new holder"
        );
        assert_ne!(new_king_queen, Some(king_queen));
        assert_ne!(
            new_king_queen,
            Some(z),
            "Z was also cast out this batch and must never end up crowned"
        );
    }

    #[test]
    fn a_batch_cast_out_never_elects_a_fellow_batch_member_as_revolutionary_leader() {
        // Same scenario as the King/Queen version above, for the
        // Revolutionary Leader's succession cascade (which, unlike the
        // King/Queen's, applies at every round, not just Round 3). Z is a
        // second Uprising player -- a plausible, eligible successor --
        // who is *also* independently cast out in the same 2-slot batch.
        let (mut state, king_queen, prince, leader, cult_leader) = setup_full_game();
        let z = add_player(&mut state, "Z", Faction::Uprising);
        let mut everyone = vec![king_queen, prince, leader, cult_leader, z];
        for i in 0..16 {
            everyone.push(add_player(&mut state, &format!("Extra{i}"), Faction::Ton));
        }
        apply_command(&mut state, Command::FinalizeSetup).unwrap();
        assert_eq!(state.competing_player_count(), 21);

        apply_command(&mut state, Command::OpenDenouncement).unwrap();
        apply_command(
            &mut state,
            Command::Nominate {
                voter: leader,
                nominee: leader,
            },
        )
        .unwrap();
        apply_command(
            &mut state,
            Command::Nominate {
                voter: z,
                nominee: z,
            },
        )
        .unwrap();
        apply_command(&mut state, Command::CloseNomination).unwrap();
        apply_command(&mut state, Command::OpenBallot).unwrap();
        for &voter in &everyone[0..4] {
            apply_command(
                &mut state,
                Command::CastBallot {
                    voter,
                    ballot: Ballot::For(leader),
                },
            )
            .unwrap();
        }
        for &voter in &everyone[4..8] {
            apply_command(
                &mut state,
                Command::CastBallot {
                    voter,
                    ballot: Ballot::For(z),
                },
            )
            .unwrap();
        }

        let events = apply_command(
            &mut state,
            Command::CloseBallot {
                fallback_replacement: Some(z),
            },
        )
        .unwrap();

        assert_eq!(state.player(leader).unwrap().status, PlayerStatus::CastOut);
        assert_eq!(state.player(z).unwrap().status, PlayerStatus::CastOut);
        let successions = events
            .iter()
            .filter(|e| matches!(e, DomainEvent::RevolutionaryLeaderSucceeded { .. }))
            .count();
        assert_eq!(
            successions, 1,
            "expected exactly one succession, got: {events:?}"
        );
        assert_ne!(state.revolutionary_leader(), Some(leader));
        assert_ne!(
            state.revolutionary_leader(),
            Some(z),
            "Z was also cast out this batch and must never end up as the new Leader"
        );
    }

    #[test]
    fn assign_character_rejects_giving_a_player_a_second_different_character() {
        let (mut state, ..) = setup_game_with_extra_voters(0);
        let extra = add_player(&mut state, "Extra", Faction::Ton);
        apply_command(
            &mut state,
            Command::AssignCharacter {
                player: extra,
                character: Character::NormalTon,
            },
        )
        .unwrap();
        let result = apply_command(
            &mut state,
            Command::AssignCharacter {
                player: extra,
                character: Character::KingQueen,
            },
        );
        assert_eq!(
            result,
            Err(GameError::AlreadyHasCharacter {
                player: extra,
                existing: Character::NormalTon,
                requested: Character::KingQueen,
            })
        );
        // The old title slot must not have been touched by the rejected
        // attempt.
        assert_ne!(state.king_queen(), Some(extra));
    }

    #[test]
    fn convert_rejects_a_target_who_is_already_converted() {
        let (mut state, king_queen, ..) = setup_full_game();
        let cult_leader = state.cult_leader().unwrap();
        apply_command(
            &mut state,
            Command::Convert {
                converter: cult_leader,
                target: king_queen,
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
        assert_eq!(result, Err(GameError::AlreadyConverted(king_queen)));
    }

    #[test]
    fn close_runoff_also_guards_against_a_cascade_collision_from_the_original_ballot() {
        // The King/Queen locks in cleanly from the *original* ballot
        // (`already_locked_in`), while the Prince/Princess separately wins
        // the runoff for the tied last slot. Closing the runoff resolves
        // the King/Queen first, whose own Round-3 cascade already casts
        // out the Prince/Princess directly -- so by the time the loop
        // reaches the Prince/Princess as their own, independently-won
        // runoff slot, they're already inactive. This is the same
        // "already resolved by another cascade in this batch" collision
        // as `close_ballot`'s version, just reached via the runoff path
        // instead -- exercising `close_runoff`'s own `is_active` guard,
        // not just `close_ballot`'s.
        let (mut state, everyone) = setup_game_with_extra_voters(17); // 4 + 17 = 21
        assert_eq!(state.competing_player_count(), 21);
        let king_queen = everyone[0];
        let prince_princess = everyone[1];
        let y = everyone[17];

        apply_command(&mut state, Command::AdvanceRound).unwrap(); // -> Two
        apply_command(&mut state, Command::AdvanceRound).unwrap(); // -> Three

        apply_command(&mut state, Command::OpenDenouncement).unwrap();
        for &voter in &everyone[4..14] {
            apply_command(
                &mut state,
                Command::Nominate {
                    voter,
                    nominee: king_queen,
                },
            )
            .unwrap();
        }
        for &voter in &everyone[14..17] {
            apply_command(
                &mut state,
                Command::Nominate {
                    voter,
                    nominee: prince_princess,
                },
            )
            .unwrap();
        }
        for &voter in &everyone[17..20] {
            apply_command(&mut state, Command::Nominate { voter, nominee: y }).unwrap();
        }
        apply_command(&mut state, Command::CloseNomination).unwrap();
        apply_command(&mut state, Command::OpenBallot).unwrap();

        for &voter in &everyone[4..14] {
            apply_command(
                &mut state,
                Command::CastBallot {
                    voter,
                    ballot: Ballot::For(king_queen),
                },
            )
            .unwrap();
        }
        for &voter in &everyone[14..17] {
            apply_command(
                &mut state,
                Command::CastBallot {
                    voter,
                    ballot: Ballot::For(prince_princess),
                },
            )
            .unwrap();
        }
        for &voter in &everyone[17..20] {
            apply_command(
                &mut state,
                Command::CastBallot {
                    voter,
                    ballot: Ballot::For(y),
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
        match state.denouncement_phase() {
            Some(DenouncementPhase::Runoff {
                already_locked_in,
                candidates,
                ..
            }) => {
                assert_eq!(already_locked_in, &vec![king_queen]);
                let mut sorted = candidates.clone();
                sorted.sort();
                let mut expected = vec![prince_princess, y];
                expected.sort();
                assert_eq!(sorted, expected);
            }
            other => panic!("expected Runoff phase, got {other:?}: {events:?}"),
        }

        // Everyone breaks for the Prince/Princess in the runoff.
        for &voter in &everyone {
            apply_command(
                &mut state,
                Command::CastBallot {
                    voter,
                    ballot: Ballot::For(prince_princess),
                },
            )
            .unwrap();
        }

        let events = apply_command(
            &mut state,
            Command::CloseRunoff {
                fallback_replacement: None,
            },
        )
        .unwrap();

        assert_eq!(
            state.player(king_queen).unwrap().status,
            PlayerStatus::CastOut
        );
        assert_eq!(
            state.player(prince_princess).unwrap().status,
            PlayerStatus::CastOut
        );
        assert_eq!(state.player(y).unwrap().status, PlayerStatus::Active);
        let cascades = events
            .iter()
            .filter(|e| matches!(e, DomainEvent::KingQueenCastOutCascade { .. }))
            .count();
        assert_eq!(cascades, 1, "expected exactly one cascade, got: {events:?}");
        assert!(state.king_queen().is_some());
        assert_ne!(state.king_queen(), Some(king_queen));
        assert_ne!(state.king_queen(), Some(prince_princess));
    }
}
