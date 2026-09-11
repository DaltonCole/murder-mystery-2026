use crate::ability::{
    resolve_info_check, Dossier, InfoCheckAnswer, InfoCheckDelivery, InfoQueryKind,
};
use crate::bio::Bio;
use crate::character::{Character, PlayerStatus};
use crate::command::Command;
use crate::contest::ContestCategory;
use crate::denouncement::{
    execution_count, resolve_ballot, surfaced_nominees, Ballot, Denouncement, DenouncementPhase,
};
use crate::error::GameError;
use crate::event::DomainEvent;
use crate::player::{Faction, Player, PlayerId};
use crate::recruitment::recruitment_window_size;
use crate::round::Round;
use crate::servant::GalleryPrediction;
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
    /// rules.md §1's "Character creation" -- submitted independently of
    /// faction/character assignment, so this stays a separate map rather
    /// than a field on `Player`. See `bio::task_candidates` for the
    /// consumer this exists for.
    bios: BTreeMap<PlayerId, Bio>,
    /// rules.md §1's signup interest rating, keyed like `bios` rather than
    /// living on `Player` -- submitted before faction/character exist at
    /// all. See `raffle::ticket_count` for the consumer this exists for.
    interest_levels: BTreeMap<PlayerId, u8>,
    /// True forever once `Command::CloseRaffle` has run -- a distinct,
    /// explicit moment from `FinalizeSetup` (which stays safely repeatable
    /// while players are still trickling in on time). A player
    /// `AddPlayer`'d after this point is a rules.md §1 "late arrival" and
    /// is auto-assigned `Faction::Servant` on the spot rather than landing
    /// `Unassigned`. See `AddPlayer`'s doc comment.
    raffle_closed: bool,

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
    /// Players who have already burned their Normal Ton auto-succeed
    /// (rules.md §3.1) -- a `BTreeSet` rather than a bool-per-player since
    /// most players never need an entry at all.
    normal_ton_auto_succeed_used: BTreeSet<PlayerId>,

    // --- Phase 2: Cult recruitment schedule (rules.md §3.3) ---
    /// Opened by `AdvanceRound`, consumed by `Convert` -- see
    /// `recruitment::recruitment_window_size`. A window's slots persist
    /// until spent; an unused window doesn't expire when the next one
    /// opens (rules.md doesn't say a missed window is lost, and there's no
    /// reason to assume so).
    available_recruitment_slots: usize,
    /// The Cult Leader's "before each recruitment window" query
    /// (rules.md §3.3) -- same non-expiring-window reasoning as
    /// `available_recruitment_slots`.
    cult_leader_queries_available: usize,

    // --- Phase 2: the Deceiver's falsify pipeline (rules.md §3.3) ---
    /// A standing choice the Deceiver arms/disarms at will (mirrors
    /// `revolutionary_leader_successor`'s "standing choice, changeable at
    /// any time" shape) -- real-time "were you just checked, falsify now?"
    /// interactivity isn't possible in this engine's synchronous
    /// command/event model, so "may force a false result" becomes "if
    /// armed, the next check against them auto-falsifies," consumed once.
    /// See `ability::resolve_info_check`.
    deceiver_armed: bool,
    deceiver_falsify_used: bool,

    // --- Phase 2: protect family (rules.md §3.1/§3.2) ---
    /// Priest/Priestess: how many protect-from-conversion uses are
    /// currently available (one per recruitment window, same
    /// non-expiring-window reasoning as the Cult's own counters above),
    /// and who they've already protected (can never repeat a target, for
    /// the whole game).
    priest_protects_available: usize,
    priest_protected_ever: BTreeSet<PlayerId>,
    /// Who's currently shielded from conversion *this round specifically*
    /// (rules.md: "the Cult Leader can't target that person **that
    /// round**") -- cleared every `AdvanceRound`, unlike
    /// `priest_protected_ever`. A target can still be converted in a
    /// *later* round even though the Priest can never protect them again
    /// (their one shot at that target has been spent), which is the actual
    /// balance tradeoff the ability makes.
    priest_protected_this_round: BTreeSet<PlayerId>,
    /// Doctor/Medic: who's currently shielded from the open Denouncement's
    /// Cast-Out resolution, and who they protected at the *last*
    /// Denouncement (can't repeat a target at two consecutive
    /// Denouncements). Rotated in `close_denouncement`, not on
    /// `AdvanceRound` -- see that function's doc comment for why "last
    /// round" has to mean "last Denouncement" rather than "last calendar
    /// round" (Round 4 has no Denouncement at all, so tying the rotation to
    /// round-advancement would let it slip through unblocked between
    /// Rounds 3 and 5). `medic_protected_last_round` is compared by
    /// identity, not by round number -- it's overwritten every time the
    /// Medic protects someone new, so two calls before the *same*
    /// Denouncement closes would incorrectly self-block; callers only get
    /// one protect action per Denouncement in practice, but see
    /// `medic_protect`'s own guard.
    medic_protected_this_round: Option<PlayerId>,
    medic_protected_last_round: Option<PlayerId>,
    /// Bartender: who's currently drunk (cleared every `AdvanceRound`) and
    /// whether this round's single use has already been spent.
    drunk_this_round: BTreeSet<PlayerId>,
    bartender_used_this_round: bool,
    /// Potion Maker: a named target, armed for the *current* Denouncement,
    /// consumed (regardless of whether the target was actually selected)
    /// the moment a ballot/runoff actually closes while armed. Mechanically
    /// the same "protect family" shape as Doctor/Medic (Dalton's follow-up
    /// ruling replacing the original blanket, no-target design) -- see
    /// `activate_potion_immunity`'s doc comment.
    potion_immunity_target: Option<PlayerId>,
    potion_maker_used: bool,

    // --- Phase 2: vote-weight pair (rules.md §3.1/§3.2) ---
    /// Magistrate/Firebrand: armed for the *current* ballot/runoff,
    /// consumed the moment a tally that used it actually resolves.
    magistrate_double_vote_armed: bool,
    magistrate_double_vote_used: bool,
    firebrand_double_vote_armed: bool,
    firebrand_double_vote_used: bool,

    // --- Phase 2: Normal Uprising's reactive safety-net (rules.md §3.2) ---
    /// Everyone currently armed for the *current* Denouncement (declared
    /// proactively, before the ballot closes -- Dalton's resolution of that
    /// ambiguity during the original implementation planning), each
    /// consumed the moment a tally that used it actually resolves. A set,
    /// not a single slot: `NormalUprising` is deliberately *not*
    /// unique (unlike every named Phase 2 character) -- a 20-30 player
    /// game plausibly has several simultaneous holders, and each gets
    /// their own independent once-per-game shield.
    vote_shield_armed: BTreeSet<PlayerId>,
    vote_shield_used: BTreeSet<PlayerId>,

    // --- Phase 3: Denouncement procedural modifiers (rules.md §3.1/§3.2) ---
    /// The Duelist's once-per-game challenge, pending until the currently
    /// open Nomination phase actually closes -- see
    /// `state::duelist_challenge`/`close_nomination`. Always `None`
    /// outside of that window; cleared the moment it's consumed.
    duelist_challenge: Option<PlayerId>,
    duelist_used: bool,
    agitator_used: bool,
    /// The Grand Inquisitor's once-per-game override, armed for whichever
    /// ballot/runoff is open when invoked, consumed the moment a tally
    /// that used it actually resolves -- same lifecycle as
    /// `magistrate_double_vote_armed`.
    grand_inquisitor_armed: bool,
    grand_inquisitor_used: bool,

    // --- Phase 3: contest rounds + the Leader's Confidants (rules.md
    // §3.2/§4) ---
    /// Every recorded contest category result, keyed by (round, category)
    /// so `RecordContestResult` can reject a duplicate. Deliberately never
    /// surfaced through `view_for` to any viewer -- see
    /// `Command::RecordContestResult`'s doc comment.
    contest_results: BTreeMap<(Round, ContestCategory), bool>,
    /// Every active Uprising member who currently knows the Revolutionary
    /// Leader's identity, grown one at a time by `trigger_leader_confidant`
    /// -- never shrinks, and self-limits once it covers every active
    /// Uprising member (rules.md §3.2: "once every Uprising member knows
    /// the Leader, further triggers have nothing left to reveal").
    leader_known_by: BTreeSet<PlayerId>,

    // --- Phase 3: the Intermission lottery (rules.md §4) ---
    intermission_opt_ins: BTreeSet<PlayerId>,
    /// `None` until `DrawIntermissionEntrants` runs -- once per game.
    intermission_entrants: Option<Vec<PlayerId>>,

    // --- Phase 3: Servant leaderboard + Gallery (rules.md §7) ---
    /// A running total per player -- a "leaderboard" in the rules.md sense
    /// is public once it exists, unlike everything faction/character
    /// related, so `view_for` exposes this to every viewer kind.
    servant_points: BTreeMap<PlayerId, u32>,
    /// Never exposed through `view_for` at all -- private until
    /// `resolve_gallery_predictions` scores it, and even then only the
    /// resulting point award is visible, never the prediction itself.
    gallery_predictions: BTreeMap<PlayerId, GalleryPrediction>,
    gallery_resolved: bool,

    // --- Phase 2: Cell Leader passive-knowledge (rules.md §3.2) ---
    /// Computed once, automatically, at `FinalizeSetup` -- starting
    /// knowledge, not something anyone activates.
    cell_leader_knows: Vec<PlayerId>,

    // --- Phase 2: info-check family (rules.md §3.1-3.3) ---
    /// Oracle: how many checks are currently available (one after every
    /// odd round -- Rounds 1, 3, 5 -- so usable during Round 2, Round 4,
    /// and the Finale), separate from `oracle_disabled`, which permanently
    /// zeroes this out for the rest of the game once tripped.
    oracle_checks_available: usize,
    almanac_used: bool,
    spymaster_used: bool,
    /// Every info-check ever delivered, across every ability -- the
    /// privacy-respecting read path `view_for` filters by `querier` (see
    /// `ability::InfoCheckDelivery`). Kept separate from `event_log`
    /// because `event_log` is a general audit trail with no privacy
    /// guarantee of its own; this field exists specifically so `view_for`
    /// has something to filter that only ever needs one predicate
    /// (`querier == viewer`).
    info_check_results: Vec<crate::ability::InfoCheckDelivery>,
}

impl Default for GameState {
    fn default() -> Self {
        GameState {
            players: BTreeMap::new(),
            next_player_id: 0,
            event_log: Vec::new(),
            bios: BTreeMap::new(),
            interest_levels: BTreeMap::new(),
            raffle_closed: false,
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
            normal_ton_auto_succeed_used: BTreeSet::new(),
            available_recruitment_slots: 0,
            cult_leader_queries_available: 0,
            deceiver_armed: false,
            deceiver_falsify_used: false,
            priest_protects_available: 0,
            priest_protected_ever: BTreeSet::new(),
            priest_protected_this_round: BTreeSet::new(),
            medic_protected_this_round: None,
            medic_protected_last_round: None,
            drunk_this_round: BTreeSet::new(),
            bartender_used_this_round: false,
            potion_immunity_target: None,
            potion_maker_used: false,
            magistrate_double_vote_armed: false,
            magistrate_double_vote_used: false,
            firebrand_double_vote_armed: false,
            firebrand_double_vote_used: false,
            vote_shield_armed: BTreeSet::new(),
            vote_shield_used: BTreeSet::new(),
            duelist_challenge: None,
            duelist_used: false,
            agitator_used: false,
            grand_inquisitor_armed: false,
            grand_inquisitor_used: false,
            contest_results: BTreeMap::new(),
            leader_known_by: BTreeSet::new(),
            intermission_opt_ins: BTreeSet::new(),
            intermission_entrants: None,
            servant_points: BTreeMap::new(),
            gallery_predictions: BTreeMap::new(),
            gallery_resolved: false,
            cell_leader_knows: Vec::new(),
            oracle_checks_available: 0,
            almanac_used: false,
            spymaster_used: false,
            info_check_results: Vec::new(),
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

    pub fn bio(&self, id: PlayerId) -> Option<&Bio> {
        self.bios.get(&id)
    }

    pub(crate) fn bios(&self) -> impl Iterator<Item = (PlayerId, &Bio)> {
        self.bios.iter().map(|(&id, bio)| (id, bio))
    }

    pub fn interest_level(&self, id: PlayerId) -> Option<u8> {
        self.interest_levels.get(&id).copied()
    }

    pub(crate) fn interest_levels(&self) -> impl Iterator<Item = (PlayerId, u8)> + '_ {
        self.interest_levels.iter().map(|(&id, &level)| (id, level))
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

    /// Every task ever pushed, open or already closed -- unlike
    /// `open_task_ids`, which only covers the currently-open subset. See
    /// `bio::task_candidates`'s doc comment for the one consumer this
    /// exists for (avoiding re-suggesting an already-used bio-derived
    /// prompt).
    pub(crate) fn tasks(&self) -> impl Iterator<Item = &TaskDef> {
        self.tasks.values()
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

    /// `view_for`'s single entry point for "what can this player currently
    /// do" -- keeps every Phase 2 counter/flag private to this module while
    /// still letting the view layer render a per-character ability panel.
    /// See `ability::AbilityStatus`'s doc comment for the field-per-ability
    /// shape.
    pub(crate) fn ability_status_for(&self, viewer: PlayerId) -> crate::ability::AbilityStatus {
        use crate::ability::AbilityStatus;
        let mut status = AbilityStatus::default();
        let Some(character) = self.players.get(&viewer).and_then(|p| p.character) else {
            return status;
        };
        match character {
            Character::Oracle => {
                status.oracle_checks_available = Some(if self.oracle_disabled {
                    0
                } else {
                    self.oracle_checks_available
                });
            }
            Character::Almanac => status.almanac_available = Some(!self.almanac_used),
            Character::Spymaster => status.spymaster_available = Some(!self.spymaster_used),
            Character::CultLeader => {
                status.cult_leader_queries_available = Some(self.cult_leader_queries_available);
                status.recruitment_slots_available = Some(self.available_recruitment_slots);
            }
            Character::Deceiver => {
                status.deceiver_armed = Some(self.deceiver_armed);
                status.deceiver_falsify_used = Some(self.deceiver_falsify_used);
            }
            Character::PriestPriestess => {
                status.priest_protects_available = Some(self.priest_protects_available);
            }
            Character::DoctorMedic => {
                // Mirrors `medic_protect`'s own gate (an open Denouncement
                // is required -- the ability only means anything relative
                // to one) so a client's "can I act right now" reads the
                // same real precondition the command itself enforces.
                status.medic_available = Some(self.denouncement.is_some());
            }
            Character::Bartender => {
                status.bartender_available = Some(!self.bartender_used_this_round);
            }
            Character::PotionMaker => status.potion_maker_available = Some(!self.potion_maker_used),
            Character::Magistrate => {
                status.double_vote_available = Some(!self.magistrate_double_vote_used);
            }
            Character::Firebrand => {
                status.double_vote_available = Some(!self.firebrand_double_vote_used);
            }
            Character::NormalUprising => {
                status.vote_shield_available = Some(!self.vote_shield_used.contains(&viewer));
            }
            Character::Duelist => status.duelist_available = Some(!self.duelist_used),
            Character::Agitator => status.agitator_available = Some(!self.agitator_used),
            Character::GrandInquisitor => {
                status.grand_inquisitor_available = Some(!self.grand_inquisitor_used);
            }
            _ => {}
        }
        status
    }

    /// Every info-check ever delivered to `viewer` specifically -- see
    /// `info_check_results`'s doc comment.
    pub(crate) fn info_checks_for(
        &self,
        viewer: PlayerId,
    ) -> Vec<crate::ability::InfoCheckDelivery> {
        self.info_check_results
            .iter()
            .filter(|r| r.querier == viewer)
            .cloned()
            .collect()
    }

    /// `viewer`'s fellow Cultists, if rules.md actually grants them that
    /// passive knowledge -- every Cult-aligned player except the Cult
    /// Leader (their own row in rules.md's ability table lists no passive
    /// knowledge; they already know who they've personally recruited via
    /// their own `Convert` commands, so this deliberately isn't duplicated
    /// here for them). Keyed off `true_faction()` rather than `character`:
    /// a converted player keeps their *original* character (rules.md
    /// §3.3's "a converted player keeps their original character and
    /// abilities" -- see `convert`'s doc comment), so a converted Oracle or
    /// a converted Normal Ton member is just as much a real Cult member as
    /// someone whose character literally is `Cultist`/`Deceiver`, and
    /// should learn who their fellow cultists are the same way. Computed
    /// live rather than a stored list, since conversion can add new fellow
    /// Cultists mid-game.
    pub(crate) fn fellow_cultists_for(&self, viewer: PlayerId) -> Vec<PlayerId> {
        let Some(viewer_player) = self.players.get(&viewer) else {
            return Vec::new();
        };
        if viewer_player.character == Some(Character::CultLeader)
            || viewer_player.true_faction() != Faction::Cult
        {
            return Vec::new();
        }
        self.players
            .values()
            .filter(|p| p.id != viewer && p.true_faction() == Faction::Cult)
            .map(|p| p.id)
            .collect()
    }

    /// The Cell Leader's starting passive knowledge -- see
    /// `cell_leader_knows`'s doc comment.
    pub(crate) fn cell_leader_knows(&self) -> &[PlayerId] {
        &self.cell_leader_knows
    }

    /// Whether `viewer` is currently drunk (rules.md §3.2: "the target is
    /// told if drunk" -- the Bartender themself is deliberately never told
    /// whether it landed, so this is only ever exposed to the drunk player
    /// about themselves, never to the Bartender or anyone else).
    pub(crate) fn is_drunk(&self, viewer: PlayerId) -> bool {
        self.drunk_this_round.contains(&viewer)
    }

    /// The Revolutionary Leader's own view of who currently knows their
    /// identity -- see `leader_known_by`'s doc comment. Empty for anyone
    /// who isn't currently the Leader.
    pub(crate) fn confidants_known_to_leader(&self, viewer: PlayerId) -> Vec<PlayerId> {
        if self.revolutionary_leader != Some(viewer) {
            return Vec::new();
        }
        self.leader_known_by.iter().copied().collect()
    }

    /// The Revolutionary Leader's identity, if `viewer` has been revealed
    /// to as one of their Confidants -- `None` for everyone else,
    /// including the Leader's own view of themselves (they don't need to
    /// be told their own identity).
    pub(crate) fn leader_known_to(&self, viewer: PlayerId) -> Option<PlayerId> {
        if self.leader_known_by.contains(&viewer) {
            self.revolutionary_leader
        } else {
            None
        }
    }

    pub(crate) fn opted_into_intermission(&self, viewer: PlayerId) -> bool {
        self.intermission_opt_ins.contains(&viewer)
    }

    /// The drawn Intermission entrants, once `DrawIntermissionEntrants` has
    /// run -- a public reveal (rules.md §4 frames the draw itself as a live
    /// party moment), so every viewer gets the same answer, unlike the
    /// opt-in pool itself (which stays private to each opted-in player).
    pub(crate) fn intermission_entrants(&self) -> Option<&[PlayerId]> {
        self.intermission_entrants.as_deref()
    }

    /// The Servant leaderboard (rules.md §5/§7), sorted highest-first --
    /// public to every viewer, unlike anything faction/character related.
    /// Ties break by `PlayerId` for a stable, deterministic order.
    pub(crate) fn servant_leaderboard(&self) -> Vec<(PlayerId, u32)> {
        let mut board: Vec<(PlayerId, u32)> = self
            .servant_points
            .iter()
            .map(|(&id, &pts)| (id, pts))
            .collect();
        board.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
        board
    }

    /// Every contest result recorded so far -- unlike every other Phase 3
    /// accessor, this one is deliberately exposed to `Viewer::Host` only
    /// (never `Viewer::Player`/`Viewer::Display`, and NOT keyed off a
    /// `PlayerId` the way the rest of `view.rs` is), so the host has a
    /// self-audit view: `RecordContestResult` has no correction command
    /// and rules.md says players must never learn the breakdown, so
    /// letting the host see what's already on record before they submit
    /// another one is the only realistic guard against a mis-tagged round
    /// going unnoticed for the rest of a live event.
    pub(crate) fn contest_results_for_host(&self) -> Vec<((Round, ContestCategory), bool)> {
        self.contest_results.iter().map(|(&k, &v)| (k, v)).collect()
    }

    /// The one faction currently winning, if any (`win_condition::evaluate`
    /// guarantees at most one -- see its module doc comment on the Cult's
    /// priority ruling). `Viewer::Host`-only, the same scoping as
    /// `contest_results_for_host`: without this, nothing in the live game
    /// ever tells the Host who actually won, which `ResolveGalleryPredictions`
    /// needs a real answer for. Public finale-reveal sequencing for
    /// `Viewer::Player`/`Viewer::Display` is deferred to a later phase (see
    /// the implementation plan's Phase 4).
    pub(crate) fn winner_for_host(&self) -> Option<Faction> {
        let outcome = crate::win_condition::evaluate(self);
        if outcome.cult_wins {
            Some(Faction::Cult)
        } else if outcome.ton_wins {
            Some(Faction::Ton)
        } else if outcome.uprising_wins {
            Some(Faction::Uprising)
        } else {
            None
        }
    }

    /// Whether `ResolveGalleryPredictions` has already run -- `Viewer::Host`
    /// ONLY, the same scoping as `winner_for_host`/`contest_results_for_host`.
    /// Without this, nothing tells a caller (a live host, or the `bots`
    /// integration harness driving a real game end to end) whether the
    /// once-per-game, irreversible resolution actually took effect, forcing
    /// a guess -- see `HostDriver::resolve_gallery_predictions` in the
    /// `bots` crate.
    pub(crate) fn gallery_resolved(&self) -> bool {
        self.gallery_resolved
    }

    /// The faction a title's holder must belong to. Used to validate
    /// [`Command::AssignCharacter`] -- e.g. rejects assigning `CultLeader`
    /// to a Ton player.
    fn required_faction(character: Character) -> Option<Faction> {
        match character {
            Character::KingQueen
            | Character::PrincePrincess
            | Character::NormalTon
            | Character::Oracle
            | Character::Almanac
            | Character::PriestPriestess
            | Character::PotionMaker
            | Character::Magistrate
            | Character::Duelist
            | Character::GrandInquisitor => Some(Faction::Ton),
            Character::RevolutionaryLeader
            | Character::NormalUprising
            | Character::Spymaster
            | Character::Bartender
            | Character::DoctorMedic
            | Character::Firebrand
            | Character::CellLeader
            | Character::Agitator => Some(Faction::Uprising),
            Character::CultLeader | Character::Cultist | Character::Deceiver => Some(Faction::Cult),
        }
    }

    /// The lowest-`PlayerId` active, *untitled* player whose `true_faction`
    /// is `faction`, excluding anyone in `exclude` -- the deterministic
    /// fallback used when no explicit replacement is supplied or the
    /// supplied one isn't eligible. "Untitled" (a `Normal*`/`Cultist`/no
    /// character yet) matters: without it, this could hand the King/Queen's
    /// crown to whoever's currently the Prince/Princess, since they're
    /// Ton-faction too -- double-titling someone was never intended.
    /// `true_faction` (not the apparent `faction` field) matters just as
    /// much: without it, this could install an already-secretly-converted
    /// Cult member as the new King/Queen or Revolutionary Leader,
    /// contradicting rules.md's "a fresh, unconverted Leader/King-Queen"
    /// framing for succession and silently pre-loading Cult Path A/B/C.
    /// `exclude` takes a slice (not a single `PlayerId`) so a caller
    /// resolving several Cast-Outs from the same Denouncement batch can
    /// exclude everyone in that batch, not just the one player currently
    /// being processed -- see the doc comment on `resolve_cast_out`'s
    /// `also_departing` parameter for why that matters. Lowest ID (rather
    /// than e.g. highest, or first-inserted) is an arbitrary but fixed
    /// choice, picked so tests are reproducible without needing to inject a
    /// fake RNG.
    fn first_eligible(&self, faction: Faction, exclude: &[PlayerId]) -> Option<PlayerId> {
        self.players
            .values()
            .find(|p| {
                p.true_faction() == faction
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

    /// True if `id` holds only a generic catch-all character
    /// (`NormalTon`/`NormalUprising`/`Cultist`) or none at all -- i.e. not
    /// one of the four major titles *and* not any Phase 2 named role
    /// either (Oracle, Priest/Priestess, Deceiver, ...). Shared by every
    /// "is this candidate a legal replacement/successor" check --
    /// `first_eligible`'s own search and every caller-supplied
    /// `fallback_replacement`/successor -- so a host mistake (e.g. handing
    /// the crown to the sitting Prince/Princess, or to an Oracle) is
    /// rejected the same way regardless of which path picked the
    /// candidate. This deliberately means a King/Queen transfer or
    /// Leader succession can never silently clobber a named-role holder's
    /// character, even though rules.md's own transfer wording
    /// ("overwriting any existing role") reads as if it might -- treated
    /// as a safety choice worth keeping, not a rule this engine enforces
    /// literally against its own ability-holders.
    fn is_untitled(&self, id: PlayerId) -> bool {
        self.players.get(&id).is_some_and(|p| {
            matches!(
                p.character,
                None | Some(Character::NormalTon)
                    | Some(Character::NormalUprising)
                    | Some(Character::Cultist)
            )
        })
    }

    /// The current Deceiver, if the Cult has recruited/designated one yet --
    /// `assign_character`'s uniqueness check guarantees at most one player
    /// ever holds this character. Used to name the culprit in
    /// `DomainEvent::CheckFalsifiedByDeceiver` without threading a player id
    /// through every info-check caller.
    fn deceiver_id(&self) -> Option<PlayerId> {
        self.players
            .values()
            .find(|p| p.character == Some(Character::Deceiver))
            .map(|p| p.id)
    }

    /// A ballot's per-voter weight: 2 for the Magistrate/Firebrand while
    /// their once-per-game double vote is armed for *this* tally, 1 for
    /// everyone else. See `tally_ballots`.
    fn double_vote_weight(&self, voter: PlayerId) -> u32 {
        match self.players.get(&voter).and_then(|p| p.character) {
            Some(Character::Magistrate) if self.magistrate_double_vote_armed => 2,
            Some(Character::Firebrand) if self.firebrand_double_vote_armed => 2,
            _ => 1,
        }
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
            let mut events = vec![DomainEvent::PlayerAdded { id, name }];
            // Rules.md §1: "late arrivals become Servants" -- the raffle
            // has already closed, so this player never gets a shot at a
            // named role; see `Command::AddPlayer`'s doc comment.
            if state.raffle_closed {
                state.players.get_mut(&id).unwrap().faction = Faction::Servant;
                events.push(DomainEvent::FactionAssigned {
                    player: id,
                    faction: Faction::Servant,
                });
            }
            events
        }

        Command::SubmitInterestLevel { player, level } => {
            submit_interest_level(state, player, level)?
        }

        Command::CloseRaffle => {
            state.raffle_closed = true;
            vec![DomainEvent::RaffleClosed]
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

        Command::SubmitBio { player, bio } => submit_bio(state, player, bio)?,

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

            // Phase 2: per-round transient ability state resets/rotates.
            // Must happen before opening the new window below, since that
            // window's size depends on `competing_player_count`, which
            // isn't affected by any of this -- ordering here is about
            // correctness of *these* resets, not the window calculation.
            //
            // The Medic's last-protected-target tracking is deliberately
            // NOT reset here -- see `close_denouncement`'s doc comment for
            // why "consecutive rounds" actually means "consecutive
            // Denouncements," which don't land on every calendar round
            // (Round 4 is a contest round with no Denouncement at all).
            state.drunk_this_round.clear();
            state.bartender_used_this_round = false;
            state.priest_protected_this_round.clear();

            // Every round advance opens a new Cult recruitment window
            // (rules.md §3.3) -- see `recruitment::recruitment_window_size`
            // for the exact schedule this implements.
            let slots = recruitment_window_size(next, state.competing_player_count());
            state.available_recruitment_slots += slots;
            state.cult_leader_queries_available += 1;
            state.priest_protects_available += 1;

            // Oracle: "after every odd round" (rules.md §3.1) -- Rounds 1,
            // 3, 5 are odd, so a check becomes available arriving at
            // Round 2, Round 4, or the Finale.
            if matches!(next, Round::Two | Round::Four | Round::Finale) {
                state.oracle_checks_available += 1;
            }

            vec![
                DomainEvent::RoundAdvanced { round: next },
                DomainEvent::RecruitmentWindowOpened { round: next, slots },
            ]
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

        Command::UseOracle { player, target } => use_oracle(state, player, target)?,
        Command::UseAlmanac { player } => use_almanac(state, player)?,
        Command::UseSpymaster { player, target } => use_spymaster(state, player, target)?,
        Command::CultLeaderQuery {
            player,
            target,
            kind,
        } => cult_leader_query(state, player, target, kind)?,
        Command::SetDeceiverArmed { player, armed } => set_deceiver_armed(state, player, armed)?,

        Command::PriestProtect { player, target } => priest_protect(state, player, target)?,
        Command::MedicProtect { player, target } => medic_protect(state, player, target)?,
        Command::BartenderTarget {
            player,
            target,
            lands,
        } => bartender_target(state, player, target, lands)?,
        Command::ActivatePotionImmunity { player, target } => {
            activate_potion_immunity(state, player, target)?
        }

        Command::ActivateDoubleVote { player } => activate_double_vote(state, player)?,

        Command::ArmVoteShield { player } => arm_vote_shield(state, player)?,

        Command::DuelistChallenge { player, target } => duelist_challenge(state, player, target)?,
        Command::AgitatorRedirect { player, target } => agitator_redirect(state, player, target)?,
        Command::ActivateGrandInquisitor { player } => activate_grand_inquisitor(state, player)?,

        Command::RecordContestResult {
            round,
            category,
            ton_won,
        } => record_contest_result(state, round, category, ton_won)?,

        Command::OptIntoIntermission { player } => opt_into_intermission(state, player)?,
        Command::DrawIntermissionEntrants { selected } => {
            draw_intermission_entrants(state, selected)?
        }

        Command::AwardServantPoints { player, points } => {
            award_servant_points(state, player, points)?
        }
        Command::SubmitGalleryPrediction { player, prediction } => {
            submit_gallery_prediction(state, player, prediction)?
        }
        Command::ResolveGalleryPredictions {
            actual_cast_out,
            actual_winner,
        } => resolve_gallery_predictions(state, actual_cast_out, actual_winner)?,
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
    let mut faction_to_assign = None;
    if let Some(required) = GameState::required_faction(character) {
        // A secretly-recruited Cultist's *apparent* `faction` never changes
        // (see `Player::true_faction`'s doc comment) -- only `true_faction`
        // reflects their real Cult membership. Checking apparent faction
        // for a Cult-required character would make it impossible for the
        // Cult Leader to ever designate a recruited Cultist as Deceiver
        // (rules.md §3.3: "as recruitment brings in new Cultists, the Cult
        // Leader personally designates who holds each title... at the
        // moment of recruitment or any point after"). Every other
        // faction's characters are assigned before any conversion could
        // apply to them, so this only changes behavior for the Cult case.
        let actual = if required == Faction::Cult {
            p.true_faction()
        } else {
            p.faction
        };
        if actual == Faction::Unassigned {
            // The setup raffle (`crate::raffle`) assigns roles *before*
            // factions exist -- see `Command::AssignCharacter`'s doc
            // comment. Winning a role determines the faction, rather than
            // requiring one up front.
            faction_to_assign = Some(required);
        } else if actual != required {
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
    // at someone who no longer holds it. The one exception: a generic
    // catch-all (`NormalTon`/`NormalUprising`/`Cultist`) isn't a real title
    // -- `convert()` stamps `Cultist` on every plain recruit automatically,
    // and the Cult Leader must still be able to upgrade that recruit to a
    // specific named role (Deceiver) afterward, the same way `is_untitled`
    // already treats these three as "no title held" everywhere else.
    if let Some(existing) = p.character {
        if existing != character
            && !matches!(
                existing,
                Character::NormalTon | Character::NormalUprising | Character::Cultist
            )
        {
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
        _ => None,
    };
    if let Some(slot) = title_slot {
        if let Some(holder) = *slot {
            if holder != player {
                return Err(GameError::TitleAlreadyHeld { character, holder });
            }
        }
        *slot = Some(player);
    } else if !matches!(
        character,
        Character::NormalTon | Character::NormalUprising | Character::Cultist
    ) {
        // Every Phase 2 named role (Oracle, Almanac, ...) is meant to be
        // unique too, same as the four major titles -- just without a
        // dedicated `GameState` field, since nothing else needs O(1)
        // lookup for "who currently holds this" the way King/Queen,
        // Revolutionary Leader, and Cult Leader constantly do elsewhere. A
        // plain scan is cheap at this player count and keeps `GameState`
        // from growing a bespoke field per character.
        if let Some(holder) = state
            .players
            .values()
            .find(|p| p.id != player && p.character == Some(character))
            .map(|p| p.id)
        {
            return Err(GameError::TitleAlreadyHeld { character, holder });
        }
    }

    let mut events = Vec::new();
    if let Some(faction) = faction_to_assign {
        state.players.get_mut(&player).unwrap().faction = faction;
        events.push(DomainEvent::FactionAssigned { player, faction });
    }
    state.players.get_mut(&player).unwrap().character = Some(character);
    events.push(DomainEvent::CharacterAssigned { player, character });
    Ok(events)
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

    // Cell Leader's passive knowledge (rules.md §3.2): computed once, here,
    // as starting knowledge rather than something activated -- deterministic
    // lowest-PlayerId selection (the `first_eligible` convention) among
    // active Uprising members, excluding the Leader (never revealed) and the
    // Cell Leader themself.
    if let Some(cell_leader) = state
        .players
        .values()
        .find(|p| p.character == Some(Character::CellLeader))
        .map(|p| p.id)
    {
        let leader = state.revolutionary_leader;
        state.cell_leader_knows = state
            .players
            .values()
            .filter(|p| {
                p.faction == Faction::Uprising
                    && p.status == PlayerStatus::Active
                    && p.id != cell_leader
                    && Some(p.id) != leader
            })
            .map(|p| p.id)
            .take(2)
            .collect();
    }

    vec![DomainEvent::SetupFinalized]
}

/// Records or replaces `player`'s signup interest rating (rules.md §1) --
/// see `Command::SubmitInterestLevel`'s doc comment for why re-submission
/// silently replaces rather than being rejected as a duplicate, matching
/// `submit_bio`'s standing-choice shape.
fn submit_interest_level(
    state: &mut GameState,
    player: PlayerId,
    level: u8,
) -> Result<Vec<DomainEvent>, GameError> {
    if !state.players.contains_key(&player) {
        return Err(GameError::UnknownPlayer(player));
    }
    if !(crate::raffle::MIN_INTEREST_LEVEL..=crate::raffle::MAX_INTEREST_LEVEL).contains(&level) {
        return Err(GameError::InterestLevelOutOfRange(level));
    }
    state.interest_levels.insert(player, level);
    Ok(vec![DomainEvent::InterestLevelSubmitted { player, level }])
}

/// Records or replaces `player`'s bio (rules.md §1). A standing choice --
/// see `Command::SubmitBio`'s doc comment for why re-submission silently
/// replaces rather than being rejected as a duplicate.
fn submit_bio(
    state: &mut GameState,
    player: PlayerId,
    bio: Bio,
) -> Result<Vec<DomainEvent>, GameError> {
    if !state.players.contains_key(&player) {
        return Err(GameError::UnknownPlayer(player));
    }
    if let Some((field, len)) = bio.first_oversized_field() {
        return Err(GameError::BioFieldTooLong { field, len });
    }
    state.bios.insert(player, bio.clone());
    Ok(vec![DomainEvent::BioSubmitted { player, bio }])
}

/// Converts `target` to secretly serve the Cult. Growing the Cult's ranks
/// (a generic Ton/Uprising member) and flipping a titled royal are the same
/// underlying operation: both keep their existing `character` (and thus
/// their existing ability) exactly as rules.md §3.3 requires -- only their
/// `converted` flag and true faction change. Converting the current
/// King/Queen before they've used their transfer
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
    if state.available_recruitment_slots == 0 {
        return Err(GameError::NoRecruitmentSlotAvailable);
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
    if state.priest_protected_this_round.contains(&target) {
        return Err(GameError::ProtectedFromConversionThisRound(target));
    }

    state.available_recruitment_slots -= 1;

    let mut events = Vec::new();
    let is_king_queen = state.king_queen == Some(target);
    let is_leader = state.revolutionary_leader == Some(target);

    {
        let p = state.players.get_mut(&target).unwrap();
        p.converted = true;
        // rules.md §3.3: "a converted player keeps their original
        // character and abilities" -- Phase 2 gave `NormalTon`/
        // `NormalUprising` real, tracked abilities of their own (task
        // auto-succeed, the vote-shield), so relabeling them `Cultist`
        // here would silently take those away on conversion, exactly the
        // bug this quote rules out. `Cultist` is now only ever stamped on
        // a target with no character at all -- not a reachable case in
        // practice once `FinalizeSetup` has run, but kept as a defensive
        // fallback rather than leaving `character` as `None`.
        if p.character.is_none() {
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
    // true_faction(), not the apparent faction: a secretly-converted
    // Uprising member is Cult now, not a valid successor -- see
    // `first_eligible`'s doc comment for why this matters.
    if s.status != PlayerStatus::Active
        || s.true_faction() != Faction::Uprising
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
    // true_faction(), not the apparent faction -- a King/Queen voluntarily
    // handing the crown to a secretly-converted Ton member would be an
    // immediate, player-triggered version of the same bug `first_eligible`'s
    // doc comment describes.
    if np.status != PlayerStatus::Active
        || np.true_faction() != Faction::Ton
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
                            && state.player(c).unwrap().true_faction() == Faction::Ton
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
                && state.player(id).unwrap().true_faction() == Faction::Uprising
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
        // The Leader's Confidants (rules.md §3.2) are a reveal about a
        // *specific person*, not a standing permission that should follow
        // whoever happens to hold the title next -- `leader_known_to`
        // resolves dynamically against `state.revolutionary_leader`, so
        // without this, every past Confidant would instantly and silently
        // learn the brand-new successor's identity for free the moment
        // succession happens, with no new trigger ever having fired for
        // them. A fresh Leader starts fully unknown, matching rules.md's
        // "a fresh, unconverted Leader" framing for succession.
        state.leader_known_by.clear();

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
    if state.drunk_this_round.contains(&voter) {
        return Err(GameError::PlayerIsDrunk(voter));
    }
    let denouncement = state.denouncement.as_mut().unwrap();
    let DenouncementPhase::Nomination { submitted } = &mut denouncement.phase else {
        return Err(GameError::NominationNotOpen);
    };
    submitted.insert(voter, nominee);
    Ok(vec![DomainEvent::NominationCast { voter, nominee }])
}

fn close_nomination(state: &mut GameState) -> Result<Vec<DomainEvent>, GameError> {
    // Read out before taking `state.denouncement`'s mutable borrow below --
    // must not be consumed until success is certain, so this is only a
    // peek (`Option<PlayerId>` is `Copy`), cleared explicitly once actually
    // applied further down.
    let challenge = state.duelist_challenge;

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
    let mut surfaced = surfaced_nominees(&tally, 3);

    // The Duelist's challenge (rules.md §3.1): guarantees a ballot spot
    // regardless of verbal support -- added on top of the naturally
    // surfaced candidates, per Dalton's resolution during Phase 3
    // planning. A no-op if the challenged player already surfaced on
    // their own.
    if let Some(challenged) = challenge {
        if !surfaced.contains(&challenged) {
            surfaced.push(challenged);
        }
    }

    denouncement.phase = DenouncementPhase::Discussion {
        surfaced: surfaced.clone(),
    };
    state.duelist_challenge = None;
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
    if state.drunk_this_round.contains(&voter) {
        return Err(GameError::PlayerIsDrunk(voter));
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
///
/// Applies the two Phase 2 vote-weight modifiers while counting rather than
/// as a separate pass: a `Ballot::For` from the Magistrate/Firebrand while
/// their double vote is armed counts as 2 (rules.md §5: "adds one extra
/// vote to whichever single nominee that player supported"), and one vote
/// against each currently-armed `vote_shield_armed` holder is negated
/// (`saturating_sub` so a shield with zero votes against it doesn't
/// underflow) -- every armed shield applies independently, since
/// `NormalUprising` isn't unique and several players can hold one at once.
fn tally_ballots(
    state: &GameState,
    candidates: &[PlayerId],
    ballots: &BTreeMap<PlayerId, Ballot>,
) -> BTreeMap<PlayerId, u32> {
    let mut tally: BTreeMap<PlayerId, u32> = candidates.iter().map(|&id| (id, 0)).collect();
    for (&voter, b) in ballots {
        if let Ballot::For(candidate) = b {
            if let Some(count) = tally.get_mut(candidate) {
                *count += state.double_vote_weight(voter);
            }
        }
    }
    for shielded in &state.vote_shield_armed {
        if let Some(count) = tally.get_mut(shielded) {
            *count = count.saturating_sub(1);
        }
    }
    tally
}

/// Closes out the current Denouncement, whatever its outcome (a clean
/// resolve, a repeat-tie unfilled slot, or a Potion Maker blanket save) --
/// the single place `close_ballot`/`close_runoff` clear `state.denouncement`.
///
/// Also rotates the Medic's "last protected target" here, at the
/// Denouncement's actual close, rather than on every `AdvanceRound`.
/// rules.md: "can't protect the same person in two consecutive rounds" --
/// but the two mid-game Denouncement rounds (3 and 5) aren't consecutive
/// *calendar* rounds, since Round 4 is a contest round with no Denouncement
/// at all. Rotating on `AdvanceRound` would let Round 4's advance wipe
/// Round 3's protected target before Round 5 ever checks it, silently
/// defeating the restriction for the one pair of rounds it's actually meant
/// to cover. Rotating here instead means "last round" really means "the
/// last Denouncement," which is what the rule is actually protecting
/// against; `.take()` still correctly clears the memory to `None` when the
/// Medic didn't act at all this Denouncement.
fn close_denouncement(state: &mut GameState) {
    state.denouncement = None;
    state.medic_protected_last_round = state.medic_protected_this_round.take();
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

    let mut tally = tally_ballots(state, &surfaced, &ballots);
    // The Grand Inquisitor's override (rules.md §5): forces exactly 2
    // Cast-Outs regardless of the headcount-scaled formula. Computed
    // before consuming, since that's what clears the armed flag.
    let slots = if state.grand_inquisitor_armed {
        2
    } else {
        execution_count(state.competing_player_count())
    };
    consume_ballot_modifiers(state);

    // Doctor/Medic's protection (rules.md §3.2) and the Potion Maker's
    // named-target immunity (rules.md §3.1, mechanically the same "protect
    // family" shape since Dalton's follow-up ruling): each protected
    // player is pulled out of the tally entirely -- not merely spared --
    // so the next-highest vote-getter backfills the freed slot (Dalton's
    // "backfill from the next candidate" resolution) rather than the slot
    // going unfilled.
    if let Some(protected) = state.medic_protected_this_round {
        tally.remove(&protected);
    }
    // Cleared here, not in `consume_ballot_modifiers` (which already ran
    // above) -- that function only flips `potion_maker_used`, since
    // clearing the target there would happen *before* this removal ever
    // reads it. Left un-cleared, this same stale target would silently
    // keep getting pulled out of every future Denouncement's tally too,
    // reintroducing the exact carryover bug this whole redesign fixes.
    if let Some(protected) = state.potion_immunity_target.take() {
        tally.remove(&protected);
    }

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
        close_denouncement(state);
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

    let mut tally = tally_ballots(state, &candidates, &ballots);
    // Same override as `close_ballot` -- applies to whichever tally is
    // open when the Grand Inquisitor is invoked, the runoff included. The
    // cap is on the Denouncement's *total* (rules.md: "forcing a 2-for-1
    // Denouncement regardless of headcount"), not the runoff's own slot
    // count in isolation -- `already_locked_in` here is whoever the
    // *original* ballot already resolved cleanly before the tie, so the
    // runoff itself may only fill whatever's left of that 2-person budget.
    let slots_remaining = if state.grand_inquisitor_armed {
        2usize.saturating_sub(already_locked_in.len())
    } else {
        slots_remaining
    };
    consume_ballot_modifiers(state);

    if let Some(protected) = state.medic_protected_this_round {
        tally.remove(&protected);
    }
    // Read once via `.take()` (clearing it), not `consume_ballot_modifiers`
    // (which already ran above and only flips `potion_maker_used`) -- both
    // this removal and the `already_locked_in` filter below need the same
    // value, and leaving it un-cleared would silently keep protecting this
    // same target in every future Denouncement for the rest of the game.
    let potion_immunity_target = state.potion_immunity_target.take();
    if let Some(protected) = potion_immunity_target {
        tally.remove(&protected);
    }
    // The Medic (or the Potion Maker) can also target someone already
    // locked in from the *original* ballot (before the tie) during the
    // runoff window -- no ranked backfill is possible for an
    // already-decided list like this, so the protected player is simply
    // saved and that slot goes unfilled.
    let already_locked_in: Vec<PlayerId> = already_locked_in
        .into_iter()
        .filter(|&id| {
            Some(id) != state.medic_protected_this_round && Some(id) != potion_immunity_target
        })
        .collect();

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
    close_denouncement(state);
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
    let mut events = vec![DomainEvent::TasksClosed {
        closed: closed.clone(),
    }];

    // The Leader's Confidants (rules.md §3.2): a task round (3 or 5) where
    // the Ton failed to hit their talking-task completion threshold
    // triggers one reveal. Skipped entirely when nothing was actually open
    // to close (covers Round 1, which isn't Confidants-eligible anyway).
    if !closed.is_empty()
        && matches!(state.current_round, Round::Three | Round::Five)
        && !ton_met_task_threshold(state, &closed)
    {
        trigger_leader_confidant(state, &mut events);
    }

    events
}

/// Whether the Ton hit "that round's talking-task completion threshold"
/// (rules.md §3.2) -- a number rules.md never actually specifies anywhere.
/// First-pass, documented-as-tunable default (same spirit as
/// `recruitment::recruitment_window_size`'s schedule): at least half of
/// the currently-active Ton players must be credited on at least one of
/// `closed`'s tasks. Vacuously met if there are no active Ton players at
/// all (nobody to fail it).
fn ton_met_task_threshold(state: &GameState, closed: &[TaskId]) -> bool {
    let active_ton: Vec<PlayerId> = state
        .players
        .values()
        .filter(|p| p.status == PlayerStatus::Active && p.faction == Faction::Ton)
        .map(|p| p.id)
        .collect();
    if active_ton.is_empty() {
        return true;
    }
    let credited = active_ton
        .iter()
        .filter(|&&id| {
            closed.iter().any(|task| {
                state
                    .task_attempts
                    .get(&(id, *task))
                    .copied()
                    .unwrap_or(false)
            })
        })
        .count();
    credited * 2 >= active_ton.len()
}

/// The Leader's Confidants effect (rules.md §3.2): reveals identities
/// bidirectionally between the Revolutionary Leader and one Uprising
/// member who doesn't already know them. Deterministic lowest-`PlayerId`
/// selection among eligible candidates -- the same fixed,
/// test-reproducible substitute for "random" used throughout this engine
/// (see `first_eligible`). Silently a no-op if there's no Leader seated
/// yet, or if every active Uprising member already knows them (rules.md's
/// own self-limiting clause).
fn trigger_leader_confidant(state: &mut GameState, events: &mut Vec<DomainEvent>) {
    let Some(leader) = state.revolutionary_leader else {
        return;
    };
    let confidant = state
        .players
        .values()
        .find(|p| {
            p.status == PlayerStatus::Active
                && p.true_faction() == Faction::Uprising
                && p.id != leader
                && !state.leader_known_by.contains(&p.id)
        })
        .map(|p| p.id);
    if let Some(confidant) = confidant {
        state.leader_known_by.insert(confidant);
        events.push(DomainEvent::LeaderConfidantRevealed { leader, confidant });
    }
}

/// Records one contest category's result (rules.md §4) and, on a Ton loss,
/// triggers the Leader's Confidants the same way a missed task threshold
/// does. No actor -- see `Command::RecordContestResult`'s doc comment.
fn record_contest_result(
    state: &mut GameState,
    round: Round,
    category: ContestCategory,
    ton_won: bool,
) -> Result<Vec<DomainEvent>, GameError> {
    if !matches!(round, Round::Two | Round::Four) {
        return Err(GameError::NotAContestRound(round));
    }
    // Guards the specific live-event mistake of a host whose round
    // selector is still sitting on a stale default (e.g. `Round::Two`)
    // recording a result for a round that hasn't actually happened yet --
    // that combination can only ever be a mis-click, never a real result.
    // Deliberately NOT `round != state.current_round`: a host correcting a
    // *past* contest round's category they forgot to tap in earlier is
    // legitimate and must stay possible.
    if round > state.current_round {
        return Err(GameError::ContestRoundNotYetReached(round));
    }
    if state.contest_results.contains_key(&(round, category)) {
        return Err(GameError::ContestResultAlreadyRecorded { round, category });
    }

    state.contest_results.insert((round, category), ton_won);
    let mut events = vec![DomainEvent::ContestResultRecorded {
        round,
        category,
        ton_won,
    }];

    if !ton_won {
        trigger_leader_confidant(state, &mut events);
    }

    Ok(events)
}

/// Opts `player` into the Intermission lottery (rules.md §4). Rejected for
/// an already-Cast-Out player -- "anyone Cast Out earlier is ineligible to
/// enter." Idempotent: opting in again before the draw is a harmless
/// no-op (re-inserting into a `BTreeSet`).
fn opt_into_intermission(
    state: &mut GameState,
    player: PlayerId,
) -> Result<Vec<DomainEvent>, GameError> {
    if !state.is_active(player) {
        return Err(GameError::NotActive(player));
    }
    state.intermission_opt_ins.insert(player);
    Ok(vec![DomainEvent::IntermissionOptedIn { player }])
}

/// Draws the Intermission's entrants (rules.md §4: "5 entrants are then
/// selected at random from that pool") from `selected`, the caller-supplied
/// random draw -- this engine never generates its own randomness (see the
/// plan's "keep randomness at the boundary" principle). Once per game;
/// every name must have actually opted in and still be active.
fn draw_intermission_entrants(
    state: &mut GameState,
    selected: Vec<PlayerId>,
) -> Result<Vec<DomainEvent>, GameError> {
    if state.intermission_entrants.is_some() {
        return Err(GameError::IntermissionAlreadyDrawn);
    }
    if selected.len() > 5 {
        return Err(GameError::TooManyIntermissionEntrants(selected.len()));
    }
    let mut seen = BTreeSet::new();
    for &id in &selected {
        if !seen.insert(id) {
            return Err(GameError::DuplicateIntermissionEntrant(id));
        }
        if !state.intermission_opt_ins.contains(&id) || !state.is_active(id) {
            return Err(GameError::InvalidIntermissionEntrant(id));
        }
    }

    state.intermission_entrants = Some(selected.clone());
    Ok(vec![DomainEvent::IntermissionEntrantsDrawn {
        entrants: selected,
    }])
}

/// True for anyone currently "operating as a Servant" (rules.md §5/§7):
/// a literal late-arrival `Faction::Servant` player, or any already-Cast-Out
/// competing player.
fn is_servant(player: &Player) -> bool {
    player.faction == Faction::Servant || player.status == PlayerStatus::CastOut
}

fn award_servant_points(
    state: &mut GameState,
    player: PlayerId,
    points: u32,
) -> Result<Vec<DomainEvent>, GameError> {
    let p = state
        .players
        .get(&player)
        .ok_or(GameError::UnknownPlayer(player))?;
    if !is_servant(p) {
        return Err(GameError::NotAServant(player));
    }
    let total = state.servant_points.entry(player).or_insert(0);
    *total += points;
    let total = *total;
    Ok(vec![DomainEvent::ServantPointsAwarded {
        player,
        points,
        total,
    }])
}

/// Records `player`'s private Gallery prediction (rules.md §7). Requires
/// the player to be Cast Out specifically (narrower than the general
/// Servant eligibility above -- a late-arrival Servant never got a chance
/// to be voted out, so they don't get a Gallery prediction either) and the
/// Last Denouncement to currently be open. Re-submitting before resolution
/// silently replaces the earlier choice, the same "standing choice"
/// treatment as `Nominate`/`CastBallot`.
fn submit_gallery_prediction(
    state: &mut GameState,
    player: PlayerId,
    prediction: GalleryPrediction,
) -> Result<Vec<DomainEvent>, GameError> {
    let p = state
        .players
        .get(&player)
        .ok_or(GameError::UnknownPlayer(player))?;
    if p.status != PlayerStatus::CastOut {
        return Err(GameError::MustBeCastOutForGallery(player));
    }
    if state.current_round != Round::Finale || state.denouncement.is_none() {
        return Err(GameError::GalleryPredictionWindowClosed);
    }

    state.gallery_predictions.insert(player, prediction);
    Ok(vec![DomainEvent::GalleryPredictionSubmitted { player }])
}

/// Scores every submitted Gallery prediction against the real finale
/// outcome (rules.md §7), awarding one Servant leaderboard point per
/// correct guess. Once per game -- see `Command::ResolveGalleryPredictions`.
fn resolve_gallery_predictions(
    state: &mut GameState,
    actual_cast_out: Vec<PlayerId>,
    actual_winner: Faction,
) -> Result<Vec<DomainEvent>, GameError> {
    if state.gallery_resolved {
        return Err(GameError::GalleryAlreadyResolved);
    }
    // Guards against the single highest-consequence mistake in this whole
    // command: resolution is once-per-game and irreversible, so firing it
    // even one round early would permanently zero out the Gallery for the
    // rest of a live 2-hour event with no way to re-run it once real
    // predictions actually come in. Requires the Last Denouncement to have
    // actually closed (not just be open) -- the real outcome isn't known
    // until then, so resolving any earlier could only ever be a mistake.
    if state.current_round != Round::Finale || state.denouncement.is_some() {
        return Err(GameError::GalleryResolutionTooEarly);
    }
    state.gallery_resolved = true;

    let mut events = Vec::new();
    let mut correct_predictions = 0;
    for (&player, prediction) in state.gallery_predictions.clone().iter() {
        let correct = match prediction {
            GalleryPrediction::CastOutIs(id) => actual_cast_out.contains(id),
            GalleryPrediction::FactionWins(f) => *f == actual_winner,
        };
        if correct {
            correct_predictions += 1;
            let total = state.servant_points.entry(player).or_insert(0);
            *total += 1;
            events.push(DomainEvent::ServantPointsAwarded {
                player,
                points: 1,
                total: *total,
            });
        }
    }
    events.push(DomainEvent::GalleryPredictionsResolved {
        correct_predictions,
    });
    Ok(events)
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

    let mut credited = named.iter().any(|id| def.qualifying_players.contains(id));

    // Normal Ton's reactive safety-net (rules.md §3.1): a failed attempt is
    // silently upgraded to a success once per game, rather than the player
    // having to invoke a separate command -- there's no "declare I'm using
    // my auto-succeed" moment in rules.md, just an automatic backstop.
    if !credited
        && state.players.get(&player).and_then(|p| p.character) == Some(Character::NormalTon)
        && !state.normal_ton_auto_succeed_used.contains(&player)
    {
        credited = true;
        state.normal_ton_auto_succeed_used.insert(player);
    }

    state.task_attempts.insert((player, task), credited);
    Ok(vec![DomainEvent::TaskAttempted {
        player,
        task,
        credited,
    }])
}

/// Shared precondition for every Phase 2 ability command: the actor must be
/// active and currently hold the specific character the ability belongs to.
/// See `error::GameError::NotCharacter`'s doc comment for why this is one
/// generic check rather than a bespoke one per character.
fn require_character(
    state: &GameState,
    player: PlayerId,
    required: Character,
) -> Result<(), GameError> {
    if !state.is_active(player) {
        return Err(GameError::NotActive(player));
    }
    if state.players.get(&player).and_then(|p| p.character) != Some(required) {
        return Err(GameError::NotCharacter { player, required });
    }
    Ok(())
}

/// Computes `kind`'s falsify decision, records the delivered result into
/// `state.info_check_results`, and returns the events -- the one place
/// `use_oracle`, `use_spymaster`, and `cult_leader_query` all funnel through
/// (rules.md names these three, plus Almanac, as the set the Deceiver can
/// target; Almanac is deliberately excluded here -- see
/// `InfoQueryKind::NotLeaderSet`'s doc comment).
fn deliver_info_check(
    state: &mut GameState,
    querier: PlayerId,
    target: PlayerId,
    kind: InfoQueryKind,
    true_answer: InfoCheckAnswer,
) -> Vec<DomainEvent> {
    // rules.md §3.3: "once per game, if targeted by another player's
    // info-check ability... may force that check to return a false
    // result" -- the Deceiver's falsify only fires on a check that
    // actually targets *them*, not just the next check anyone performs
    // against anyone.
    let should_falsify =
        state.deceiver_armed && !state.deceiver_falsify_used && state.deceiver_id() == Some(target);
    let answer = resolve_info_check(true_answer, should_falsify);

    let mut events = Vec::new();
    if should_falsify {
        state.deceiver_falsify_used = true;
        state.deceiver_armed = false;
        if let Some(deceiver) = state.deceiver_id() {
            events.push(DomainEvent::CheckFalsifiedByDeceiver { deceiver });
        }
    }

    events.push(DomainEvent::InfoCheckDelivered {
        querier,
        target: Some(target),
        kind,
        answer: answer.clone(),
    });
    state.info_check_results.push(InfoCheckDelivery {
        querier,
        target: Some(target),
        kind,
        answer,
        round: state.current_round,
    });
    events
}

fn use_oracle(
    state: &mut GameState,
    player: PlayerId,
    target: PlayerId,
) -> Result<Vec<DomainEvent>, GameError> {
    require_character(state, player, Character::Oracle)?;
    if state.oracle_disabled || state.oracle_checks_available == 0 {
        return Err(GameError::AbilityNotAvailable {
            character: Character::Oracle,
        });
    }
    let target_player = state
        .players
        .get(&target)
        .ok_or(GameError::UnknownPlayer(target))?;
    let true_answer = InfoCheckAnswer::Dossier(Dossier {
        apparent_faction: target_player.faction,
        converted: target_player.converted,
        character: target_player.character,
    });

    state.oracle_checks_available -= 1;
    Ok(deliver_info_check(
        state,
        player,
        target,
        InfoQueryKind::FullHistory,
        true_answer,
    ))
}

/// Almanac: once per game, learns 3 players who are definitely not the
/// Revolutionary Leader. Deterministic lowest-PlayerId selection (the same
/// fixed, test-reproducible convention as `first_eligible`) among active
/// players, excluding the true Leader and the Almanac-holder themself --
/// rules.md never says the Almanac's own name would appear in their own
/// result, and excluding it keeps every entry informative.
fn use_almanac(state: &mut GameState, player: PlayerId) -> Result<Vec<DomainEvent>, GameError> {
    require_character(state, player, Character::Almanac)?;
    if state.almanac_used {
        return Err(GameError::AbilityNotAvailable {
            character: Character::Almanac,
        });
    }
    state.almanac_used = true;

    let leader = state.revolutionary_leader;
    let picks: Vec<PlayerId> = state
        .players
        .values()
        .filter(|p| p.status == PlayerStatus::Active && Some(p.id) != leader && p.id != player)
        .map(|p| p.id)
        .take(3)
        .collect();
    let answer = InfoCheckAnswer::PlayerSet(picks);

    // Deliberately bypasses `deliver_info_check`/the Deceiver falsify
    // pipeline -- see `InfoQueryKind::NotLeaderSet`'s doc comment on why
    // "targeted by Almanac" is undefined for a check with no single target.
    state.info_check_results.push(InfoCheckDelivery {
        querier: player,
        target: None,
        kind: InfoQueryKind::NotLeaderSet,
        answer: answer.clone(),
        round: state.current_round,
    });
    Ok(vec![DomainEvent::InfoCheckDelivered {
        querier: player,
        target: None,
        kind: InfoQueryKind::NotLeaderSet,
        answer,
    }])
}

fn use_spymaster(
    state: &mut GameState,
    player: PlayerId,
    target: PlayerId,
) -> Result<Vec<DomainEvent>, GameError> {
    require_character(state, player, Character::Spymaster)?;
    if state.spymaster_used {
        return Err(GameError::AbilityNotAvailable {
            character: Character::Spymaster,
        });
    }
    let target_player = state
        .players
        .get(&target)
        .ok_or(GameError::UnknownPlayer(target))?;
    let true_answer = InfoCheckAnswer::Faction(target_player.faction);

    state.spymaster_used = true;
    Ok(deliver_info_check(
        state,
        player,
        target,
        InfoQueryKind::FactionColorOnly,
        true_answer,
    ))
}

fn cult_leader_query(
    state: &mut GameState,
    player: PlayerId,
    target: PlayerId,
    kind: InfoQueryKind,
) -> Result<Vec<DomainEvent>, GameError> {
    require_character(state, player, Character::CultLeader)?;
    if !matches!(
        kind,
        InfoQueryKind::IsTonAligned | InfoQueryKind::IsTheLeader
    ) {
        return Err(GameError::InvalidInfoQueryKind);
    }
    if state.cult_leader_queries_available == 0 {
        return Err(GameError::AbilityNotAvailable {
            character: Character::CultLeader,
        });
    }
    let target_player = state
        .players
        .get(&target)
        .ok_or(GameError::UnknownPlayer(target))?;
    let true_answer = match kind {
        InfoQueryKind::IsTonAligned => {
            InfoCheckAnswer::Bool(target_player.true_faction() == Faction::Ton)
        }
        InfoQueryKind::IsTheLeader => {
            InfoCheckAnswer::Bool(state.revolutionary_leader == Some(target))
        }
        _ => unreachable!("kind validated above"),
    };

    state.cult_leader_queries_available -= 1;
    Ok(deliver_info_check(state, player, target, kind, true_answer))
}

fn set_deceiver_armed(
    state: &mut GameState,
    player: PlayerId,
    armed: bool,
) -> Result<Vec<DomainEvent>, GameError> {
    require_character(state, player, Character::Deceiver)?;
    if state.deceiver_falsify_used {
        return Err(GameError::AbilityNotAvailable {
            character: Character::Deceiver,
        });
    }
    state.deceiver_armed = armed;
    Ok(vec![DomainEvent::DeceiverArmedChanged { player, armed }])
}

fn priest_protect(
    state: &mut GameState,
    player: PlayerId,
    target: PlayerId,
) -> Result<Vec<DomainEvent>, GameError> {
    require_character(state, player, Character::PriestPriestess)?;
    if state.priest_protects_available == 0 {
        return Err(GameError::AbilityNotAvailable {
            character: Character::PriestPriestess,
        });
    }
    if !state.is_active(target) {
        return Err(GameError::NotActive(target));
    }
    if state.priest_protected_ever.contains(&target) {
        return Err(GameError::AlreadyProtectedByPriest(target));
    }

    state.priest_protects_available -= 1;
    state.priest_protected_ever.insert(target);
    state.priest_protected_this_round.insert(target);
    Ok(vec![DomainEvent::PriestProtected { player, target }])
}

/// A standing choice, changeable at any time for the rest of the round
/// (same shape as `DesignateSuccessor`) rather than a counted one-shot --
/// rules.md gives the Medic no explicit "how many times per round" cap
/// beyond "once per round," and re-declaring a new target before any
/// Cast-Out resolution actually consumes it is indistinguishable from having
/// only picked once. Requires an open Denouncement since the ability only
/// means anything relative to one ("if that person is selected for
/// Cast-Out").
fn medic_protect(
    state: &mut GameState,
    player: PlayerId,
    target: PlayerId,
) -> Result<Vec<DomainEvent>, GameError> {
    require_character(state, player, Character::DoctorMedic)?;
    if state.denouncement.is_none() {
        return Err(GameError::NoActiveBallotToProtectAgainst);
    }
    if !state.is_active(target) {
        return Err(GameError::NotActive(target));
    }
    if state.medic_protected_last_round == Some(target) {
        return Err(GameError::CannotProtectSameTargetConsecutively(target));
    }

    state.medic_protected_this_round = Some(target);
    Ok(vec![DomainEvent::MedicProtected { player, target }])
}

fn bartender_target(
    state: &mut GameState,
    player: PlayerId,
    target: PlayerId,
    lands: bool,
) -> Result<Vec<DomainEvent>, GameError> {
    require_character(state, player, Character::Bartender)?;
    if state.bartender_used_this_round {
        return Err(GameError::AbilityNotAvailable {
            character: Character::Bartender,
        });
    }
    if !state.is_active(target) {
        return Err(GameError::NotActive(target));
    }

    state.bartender_used_this_round = true;
    if lands {
        state.drunk_this_round.insert(target);
    }
    Ok(vec![DomainEvent::BartenderTargeted {
        player,
        target,
        landed: lands,
    }])
}

/// Arms the Potion Maker's once-per-game blanket execution-immunity.
/// Arms the Potion Maker's once-per-game named-target immunity (rules.md
/// §3.1, "protect family" -- Dalton's follow-up ruling replacing the
/// original blanket, no-target design so it can coexist with the Grand
/// Inquisitor's forced-2-slots override within the same round instead of
/// discarding the whole tally). A standing choice, changeable at any time
/// before it's consumed -- the same shape as `MedicProtect`. `target` must
/// currently be active; `potion_maker_used` is deliberately NOT set here --
/// see its doc comment on `GameState`: it's consumed only when a
/// ballot/runoff actually closes while armed (`close_ballot`/`close_runoff`
/// via `consume_ballot_modifiers`), not at activation, so re-arming (even
/// with a different target) before anything has closed is a harmless
/// replace rather than a wasted use.
fn activate_potion_immunity(
    state: &mut GameState,
    player: PlayerId,
    target: PlayerId,
) -> Result<Vec<DomainEvent>, GameError> {
    require_character(state, player, Character::PotionMaker)?;
    if state.potion_maker_used {
        return Err(GameError::AbilityNotAvailable {
            character: Character::PotionMaker,
        });
    }
    if !state.is_active(target) {
        return Err(GameError::NotActive(target));
    }
    state.potion_immunity_target = Some(target);
    Ok(vec![DomainEvent::PotionImmunityActivated {
        player,
        target,
    }])
}

/// The Magistrate and Firebrand share one command since `player`'s own
/// character determines which of the two this is (rules.md: there's only
/// ever one of each). `_used` is consumed only once a tally that applied the
/// weight actually runs -- see `consume_ballot_modifiers`.
fn activate_double_vote(
    state: &mut GameState,
    player: PlayerId,
) -> Result<Vec<DomainEvent>, GameError> {
    if !state.is_active(player) {
        return Err(GameError::NotActive(player));
    }
    let character = state.players.get(&player).and_then(|p| p.character);
    match character {
        Some(Character::Magistrate) => {
            if state.magistrate_double_vote_used {
                return Err(GameError::AbilityNotAvailable {
                    character: Character::Magistrate,
                });
            }
            state.magistrate_double_vote_armed = true;
        }
        Some(Character::Firebrand) => {
            if state.firebrand_double_vote_used {
                return Err(GameError::AbilityNotAvailable {
                    character: Character::Firebrand,
                });
            }
            state.firebrand_double_vote_armed = true;
        }
        _ => {
            // Reports `Magistrate` even for a player who's neither --
            // `NotCharacter` only carries one `required` character, and
            // this command legitimately accepts two. Cosmetically
            // incomplete (doesn't mention Firebrand as the other valid
            // option) but not misleading: the player genuinely holds
            // neither.
            return Err(GameError::NotCharacter {
                player,
                required: Character::Magistrate,
            });
        }
    }
    Ok(vec![DomainEvent::DoubleVoteActivated {
        player,
        character: character.unwrap(),
    }])
}

fn arm_vote_shield(state: &mut GameState, player: PlayerId) -> Result<Vec<DomainEvent>, GameError> {
    require_character(state, player, Character::NormalUprising)?;
    if state.vote_shield_used.contains(&player) {
        return Err(GameError::AbilityNotAvailable {
            character: Character::NormalUprising,
        });
    }
    state.vote_shield_armed.insert(player);
    Ok(vec![DomainEvent::VoteShieldArmed { player }])
}

/// The Duelist's once-per-game "challenge" (rules.md §3.1). Only records
/// the choice -- `close_nomination` is where it actually reaches the
/// candidate list, since `surfaced` doesn't exist until that tally runs.
/// Requires an open Nomination phase specifically ("before nomination
/// closes"): arming it any earlier would risk a *later*, unrelated
/// Denouncement's `close_nomination` consuming it instead, since only one
/// Denouncement runs at a time and this field has no round/Denouncement
/// identity of its own.
fn duelist_challenge(
    state: &mut GameState,
    player: PlayerId,
    target: PlayerId,
) -> Result<Vec<DomainEvent>, GameError> {
    require_character(state, player, Character::Duelist)?;
    if state.duelist_used {
        return Err(GameError::AbilityNotAvailable {
            character: Character::Duelist,
        });
    }
    if !state.is_active(target) {
        return Err(GameError::NotActive(target));
    }
    match state.denouncement.as_ref().map(|d| &d.phase) {
        Some(DenouncementPhase::Nomination { .. }) => {}
        Some(_) => return Err(GameError::NominationNotOpen),
        None => return Err(GameError::NoDenouncementOpen),
    }

    state.duelist_used = true;
    state.duelist_challenge = Some(target);
    Ok(vec![DomainEvent::DuelistChallengeIssued { player, target }])
}

/// The Agitator's once-per-game redirect (rules.md §3.2) -- mechanically
/// identical to the Duelist's challenge (Dalton's resolution during Phase 3
/// planning), but applied immediately: unlike Nomination's `surfaced` (only
/// computed when it closes), Discussion's `surfaced` already exists the
/// moment this fires, so there's no need for a separate pending field --
/// `target` is spliced straight into the live candidate list.
fn agitator_redirect(
    state: &mut GameState,
    player: PlayerId,
    target: PlayerId,
) -> Result<Vec<DomainEvent>, GameError> {
    require_character(state, player, Character::Agitator)?;
    if state.agitator_used {
        return Err(GameError::AbilityNotAvailable {
            character: Character::Agitator,
        });
    }
    if !state.is_active(target) {
        return Err(GameError::NotActive(target));
    }
    let denouncement = state
        .denouncement
        .as_mut()
        .ok_or(GameError::NoDenouncementOpen)?;
    let DenouncementPhase::Discussion { surfaced } = &mut denouncement.phase else {
        return Err(GameError::DiscussionNotOpen);
    };
    if !surfaced.contains(&target) {
        surfaced.push(target);
    }

    state.agitator_used = true;
    Ok(vec![DomainEvent::AgitatorRedirectIssued { player, target }])
}

/// Arms the Grand Inquisitor's once-per-game override (rules.md §5) for
/// whichever ballot/runoff is currently open. `grand_inquisitor_used` is
/// deliberately NOT set here -- consumed only when a tally that used it
/// actually resolves, see `consume_ballot_modifiers`, the same lifecycle as
/// the Magistrate/Firebrand's double vote.
fn activate_grand_inquisitor(
    state: &mut GameState,
    player: PlayerId,
) -> Result<Vec<DomainEvent>, GameError> {
    require_character(state, player, Character::GrandInquisitor)?;
    if state.grand_inquisitor_used {
        return Err(GameError::AbilityNotAvailable {
            character: Character::GrandInquisitor,
        });
    }
    state.grand_inquisitor_armed = true;
    Ok(vec![DomainEvent::GrandInquisitorInvoked { player }])
}

/// Consumes whichever Phase 2/3 ballot-modifying abilities actually applied
/// to a tally that just ran (the double vote, the vote-shield, the Grand
/// Inquisitor's forced-2-slots override, and the Potion Maker's named-target
/// immunity) -- called once per real tally-and-resolve (both `close_ballot`
/// and `close_runoff`), *before* `resolve_ballot` runs. This is also what
/// keeps an armed-but-never-triggered ability from silently carrying into a
/// later, unrelated Denouncement: every one of these flags is unconditionally
/// cleared here the moment a tally it was armed for actually closes,
/// regardless of whether it ended up mattering to that tally's outcome.
fn consume_ballot_modifiers(state: &mut GameState) {
    if state.magistrate_double_vote_armed {
        state.magistrate_double_vote_armed = false;
        state.magistrate_double_vote_used = true;
    }
    if state.firebrand_double_vote_armed {
        state.firebrand_double_vote_armed = false;
        state.firebrand_double_vote_used = true;
    }
    for shielded in std::mem::take(&mut state.vote_shield_armed) {
        state.vote_shield_used.insert(shielded);
    }
    if state.grand_inquisitor_armed {
        state.grand_inquisitor_armed = false;
        state.grand_inquisitor_used = true;
    }
    if state.potion_immunity_target.is_some() {
        state.potion_maker_used = true;
    }
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

    /// Test-only convenience: directly grants recruitment slots without
    /// going through `AdvanceRound`, for tests that only care about
    /// `Convert`'s own behavior, not round progression. Legal since this
    /// helper lives in `state.rs`'s own test submodule, which has the same
    /// private-field access as the rest of the file.
    fn grant_recruitment_slots(state: &mut GameState, slots: usize) {
        state.available_recruitment_slots += slots;
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
    fn assign_character_auto_assigns_the_required_faction_when_unassigned() {
        // The setup raffle (rules.md §1) hands out roles *before* factions
        // exist -- winning a role determines the faction, not the other
        // way around. See `Command::AssignCharacter`'s doc comment.
        let mut state = GameState::new();
        let p = add_player(&mut state, "Winner", Faction::Unassigned);
        let events = apply_command(
            &mut state,
            Command::AssignCharacter {
                player: p,
                character: Character::Oracle,
            },
        )
        .unwrap();
        assert_eq!(state.player(p).unwrap().faction, Faction::Ton);
        assert_eq!(state.player(p).unwrap().character, Some(Character::Oracle));
        assert_eq!(
            events,
            vec![
                DomainEvent::FactionAssigned {
                    player: p,
                    faction: Faction::Ton,
                },
                DomainEvent::CharacterAssigned {
                    player: p,
                    character: Character::Oracle,
                },
            ]
        );
    }

    #[test]
    fn assign_character_auto_assigns_cult_faction_for_the_cult_leader_raffle_winner() {
        let mut state = GameState::new();
        let p = add_player(&mut state, "Winner", Faction::Unassigned);
        apply_command(
            &mut state,
            Command::AssignCharacter {
                player: p,
                character: Character::CultLeader,
            },
        )
        .unwrap();
        assert_eq!(state.player(p).unwrap().faction, Faction::Cult);
        assert_eq!(state.cult_leader(), Some(p));
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

    #[test]
    fn a_player_added_before_the_raffle_closes_starts_unassigned() {
        let mut state = GameState::new();
        let events = apply_command(
            &mut state,
            Command::AddPlayer {
                name: "OnTime".into(),
            },
        )
        .unwrap();
        let id = match events[0] {
            DomainEvent::PlayerAdded { id, .. } => id,
            _ => unreachable!(),
        };
        assert_eq!(events.len(), 1);
        assert_eq!(state.player(id).unwrap().faction, Faction::Unassigned);
    }

    #[test]
    fn adding_more_players_after_finalize_setup_alone_does_not_servant_them() {
        // `FinalizeSetup` alone is NOT the "raffle closed" signal -- its own
        // doc comment promises it stays safe to call again while the
        // roster is still growing, and plenty of existing tests build a
        // small scenario via `setup_full_game()` (which calls
        // `FinalizeSetup`) and then add more on-time players afterward.
        // Only `CloseRaffle` should trigger late-arrival auto-Servanting.
        let mut state = GameState::new();
        add_player(&mut state, "OnTime", Faction::Ton);
        apply_command(&mut state, Command::FinalizeSetup).unwrap();
        let still_on_time = add_player(&mut state, "AlsoOnTime", Faction::Uprising);
        assert_eq!(
            state.player(still_on_time).unwrap().faction,
            Faction::Uprising
        );
    }

    #[test]
    fn a_player_added_after_close_raffle_becomes_a_servant_automatically() {
        // Rules.md §1: "late arrivals become Servants" -- once the raffle
        // has closed, a newly-joined player shouldn't sit `Unassigned`
        // forever with no way to participate; they're auto-servanted on
        // the spot.
        let mut state = GameState::new();
        add_player(&mut state, "OnTime", Faction::Ton);
        apply_command(&mut state, Command::CloseRaffle).unwrap();

        let events = apply_command(
            &mut state,
            Command::AddPlayer {
                name: "Late".into(),
            },
        )
        .unwrap();
        let late = match events[0] {
            DomainEvent::PlayerAdded { id, .. } => id,
            _ => unreachable!(),
        };
        assert_eq!(
            events,
            vec![
                DomainEvent::PlayerAdded {
                    id: late,
                    name: "Late".into(),
                },
                DomainEvent::FactionAssigned {
                    player: late,
                    faction: Faction::Servant,
                },
            ]
        );
        assert_eq!(state.player(late).unwrap().faction, Faction::Servant);
        // A late arrival gets no character at all (matching how
        // `finalize_setup` already treats every other Servant).
        apply_command(&mut state, Command::FinalizeSetup).unwrap();
        assert_eq!(state.player(late).unwrap().character, None);
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

    // --- SubmitBio ---

    fn sample_bio(character_name: &str) -> crate::bio::Bio {
        crate::bio::Bio {
            character_name: character_name.into(),
            real_name: "Alex".into(),
            occupation: "Duke".into(),
            hobbies: [
                "chess".into(),
                "fencing".into(),
                "".into(),
                "".into(),
                "".into(),
            ],
            clothing_features: [
                "a silver mask".into(),
                "".into(),
                "".into(),
                "".into(),
                "".into(),
            ],
            skills: [
                "sword fighting".into(),
                "".into(),
                "".into(),
                "".into(),
                "".into(),
            ],
        }
    }

    #[test]
    fn submit_bio_records_it_and_is_readable_back() {
        let (mut state, king_queen, ..) = setup_full_game();
        apply_command(
            &mut state,
            Command::SubmitBio {
                player: king_queen,
                bio: sample_bio("Lord Ashworth"),
            },
        )
        .unwrap();
        assert_eq!(
            state.bio(king_queen).unwrap().character_name,
            "Lord Ashworth"
        );
    }

    #[test]
    fn submit_bio_rejects_an_unknown_player() {
        let mut state = GameState::new();
        let result = apply_command(
            &mut state,
            Command::SubmitBio {
                player: PlayerId(0),
                bio: sample_bio("Nobody"),
            },
        );
        assert_eq!(result, Err(GameError::UnknownPlayer(PlayerId(0))));
    }

    #[test]
    fn submit_bio_rejects_an_over_length_field() {
        let (mut state, king_queen, ..) = setup_full_game();
        let mut bio = sample_bio("Lord Ashworth");
        bio.occupation = "a".repeat(33);
        let result = apply_command(
            &mut state,
            Command::SubmitBio {
                player: king_queen,
                bio,
            },
        );
        assert_eq!(
            result,
            Err(GameError::BioFieldTooLong {
                field: "occupation",
                len: 33
            })
        );
        assert!(state.bio(king_queen).is_none());
    }

    #[test]
    fn submit_interest_level_records_it_and_is_readable_back() {
        let mut state = GameState::new();
        let alice = add_player(&mut state, "Alice", Faction::Unassigned);
        apply_command(
            &mut state,
            Command::SubmitInterestLevel {
                player: alice,
                level: 7,
            },
        )
        .unwrap();
        assert_eq!(state.interest_level(alice), Some(7));
    }

    #[test]
    fn submit_interest_level_rejects_an_unknown_player() {
        let mut state = GameState::new();
        let result = apply_command(
            &mut state,
            Command::SubmitInterestLevel {
                player: PlayerId(0),
                level: 7,
            },
        );
        assert_eq!(result, Err(GameError::UnknownPlayer(PlayerId(0))));
    }

    #[test]
    fn submit_interest_level_rejects_zero() {
        let mut state = GameState::new();
        let alice = add_player(&mut state, "Alice", Faction::Unassigned);
        let result = apply_command(
            &mut state,
            Command::SubmitInterestLevel {
                player: alice,
                level: 0,
            },
        );
        assert_eq!(result, Err(GameError::InterestLevelOutOfRange(0)));
        assert_eq!(state.interest_level(alice), None);
    }

    #[test]
    fn submit_interest_level_rejects_above_ten() {
        let mut state = GameState::new();
        let alice = add_player(&mut state, "Alice", Faction::Unassigned);
        let result = apply_command(
            &mut state,
            Command::SubmitInterestLevel {
                player: alice,
                level: 11,
            },
        );
        assert_eq!(result, Err(GameError::InterestLevelOutOfRange(11)));
    }

    #[test]
    fn resubmitting_an_interest_level_silently_replaces_the_earlier_one() {
        let mut state = GameState::new();
        let alice = add_player(&mut state, "Alice", Faction::Unassigned);
        apply_command(
            &mut state,
            Command::SubmitInterestLevel {
                player: alice,
                level: 3,
            },
        )
        .unwrap();
        apply_command(
            &mut state,
            Command::SubmitInterestLevel {
                player: alice,
                level: 9,
            },
        )
        .unwrap();
        assert_eq!(state.interest_level(alice), Some(9));
    }

    #[test]
    fn resubmitting_a_bio_silently_replaces_the_earlier_one() {
        let (mut state, king_queen, ..) = setup_full_game();
        apply_command(
            &mut state,
            Command::SubmitBio {
                player: king_queen,
                bio: sample_bio("Lord Ashworth"),
            },
        )
        .unwrap();
        apply_command(
            &mut state,
            Command::SubmitBio {
                player: king_queen,
                bio: sample_bio("Lord Pemberton"),
            },
        )
        .unwrap();
        assert_eq!(
            state.bio(king_queen).unwrap().character_name,
            "Lord Pemberton"
        );
    }

    // --- Convert ---

    #[test]
    fn convert_keeps_a_generic_members_original_character_and_ability() {
        let (mut state, ..) = setup_full_game();
        let cult_leader = state.cult_leader().unwrap();
        let extra = add_player(&mut state, "Extra", Faction::Ton);
        apply_command(&mut state, Command::FinalizeSetup).unwrap();
        grant_recruitment_slots(&mut state, 1);

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
        // rules.md §3.3: "a converted player keeps their original
        // character and abilities" -- a plain Normal Ton member keeps
        // being a Normal Ton member (and keeps their real auto-succeed
        // ability), rather than being relabeled `Cultist`.
        assert_eq!(p.character, Some(Character::NormalTon));
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
        grant_recruitment_slots(&mut state, 1);
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
        grant_recruitment_slots(&mut state, 1);
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
        grant_recruitment_slots(&mut state, 1);

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
        grant_recruitment_slots(&mut state, 1);

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
        grant_recruitment_slots(&mut state, 1);

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
        grant_recruitment_slots(&mut state, 1);
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

    #[test]
    fn designate_successor_rejects_an_already_converted_candidate() {
        // Regression test: eligibility used to check the apparent `faction`
        // field, not `true_faction()`, so a secretly-converted Uprising
        // member could be designated -- installing a Cult asset as the
        // Leader's chosen heir.
        let (mut state, _king_queen, _prince, leader, cult_leader) = setup_full_game();
        let turncoat = add_player(&mut state, "Turncoat", Faction::Uprising);
        apply_command(&mut state, Command::AdvanceRound).unwrap(); // -> Two, slot
        apply_command(
            &mut state,
            Command::Convert {
                converter: cult_leader,
                target: turncoat,
            },
        )
        .unwrap();

        let result = apply_command(
            &mut state,
            Command::DesignateSuccessor {
                leader,
                successor: turncoat,
            },
        );
        assert_eq!(result, Err(GameError::IneligibleSuccessor(turncoat)));
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

    #[test]
    fn transfer_king_queen_rejects_an_already_converted_candidate() {
        // Regression test: eligibility used to check the apparent `faction`
        // field, not `true_faction()`, so a King/Queen could voluntarily
        // hand the crown to a secretly-converted Ton member.
        let (mut state, king_queen, _prince, _leader, cult_leader) = setup_full_game();
        let turncoat = add_player(&mut state, "Turncoat", Faction::Ton);
        apply_command(&mut state, Command::AdvanceRound).unwrap(); // -> Two, slot
        apply_command(
            &mut state,
            Command::Convert {
                converter: cult_leader,
                target: turncoat,
            },
        )
        .unwrap();

        let result = apply_command(
            &mut state,
            Command::TransferKingQueen {
                new_holder: turncoat,
            },
        );
        assert_eq!(
            result,
            Err(GameError::IneligibleKingQueenReplacement(turncoat))
        );
        assert_eq!(state.king_queen(), Some(king_queen));
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
    fn king_queen_round_three_cascade_fallback_skips_an_already_converted_candidate() {
        // Regression test: the Round-3 cascade's own fallback filter (and
        // `first_eligible` beneath it) used to check the apparent `faction`
        // field, not `true_faction()`, so an already-secretly-converted Ton
        // member could inherit the crown. `turncoat` has a lower PlayerId
        // than `loyal` (added first), so the old, faction-only check would
        // have picked them.
        let (mut state, king_queen, _prince, _leader, cult_leader) = setup_full_game();
        let turncoat = add_player(&mut state, "Turncoat", Faction::Ton);
        let loyal = add_player(&mut state, "Loyal", Faction::Ton);
        apply_command(&mut state, Command::AdvanceRound).unwrap(); // -> Two, slot
        apply_command(
            &mut state,
            Command::Convert {
                converter: cult_leader,
                target: turncoat,
            },
        )
        .unwrap();
        apply_command(&mut state, Command::AdvanceRound).unwrap(); // -> Three
        assert_eq!(state.current_round(), Round::Three);

        apply_command(
            &mut state,
            Command::CastOut {
                player: king_queen,
                fallback_replacement: None,
            },
        )
        .unwrap();

        assert_eq!(state.king_queen(), Some(loyal));
        assert_ne!(
            state.player(turncoat).unwrap().character,
            Some(Character::KingQueen)
        );
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
        grant_recruitment_slots(&mut state, 1);

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
    fn leader_succession_fallback_skips_an_already_converted_candidate() {
        // Regression test: `first_eligible`/`eligible_uprising` used to
        // check the apparent `faction` field, not `true_faction()`, so the
        // deterministic lowest-PlayerId fallback could install an
        // already-secretly-converted Cult asset as the new Revolutionary
        // Leader -- directly contradicting "a fresh Leader starts fully
        // unknown" and silently pre-loading Cult Path A/B. `turncoat` has a
        // lower PlayerId than `loyal` (added first), so the old,
        // faction-only check would have picked them.
        let (mut state, _king_queen, _prince, leader, cult_leader) = setup_full_game();
        let turncoat = add_player(&mut state, "Turncoat", Faction::Uprising);
        let loyal = add_player(&mut state, "Loyal", Faction::Uprising);
        apply_command(&mut state, Command::AdvanceRound).unwrap(); // -> Two, slot
        apply_command(
            &mut state,
            Command::Convert {
                converter: cult_leader,
                target: turncoat,
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

        assert_eq!(state.revolutionary_leader(), Some(loyal));
        assert_ne!(
            state.player(turncoat).unwrap().character,
            Some(Character::RevolutionaryLeader)
        );
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
        grant_recruitment_slots(&mut state, 1);
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
        grant_recruitment_slots(&mut state, 1);
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
            // A recruitment window opens on every round advance too (rules.md
            // §3.3) -- with 0 competing players in this fixture, every window
            // is a flat 1-slot window regardless of round.
            assert_eq!(
                events,
                vec![
                    DomainEvent::RoundAdvanced { round: expected },
                    DomainEvent::RecruitmentWindowOpened {
                        round: expected,
                        slots: 1
                    },
                ]
            );
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
        // A genuine named role (not a generic catch-all) -- those are
        // exempt from this check specifically so a converted Cultist can
        // later be upgraded to the Deceiver, see `assign_character`'s
        // catch-all doc comment.
        apply_command(
            &mut state,
            Command::AssignCharacter {
                player: extra,
                character: Character::Oracle,
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
                existing: Character::Oracle,
                requested: Character::KingQueen,
            })
        );
        // The old title slot must not have been touched by the rejected
        // attempt.
        assert_ne!(state.king_queen(), Some(extra));
    }

    #[test]
    fn assign_character_allows_upgrading_a_generic_catch_all_to_a_named_role() {
        // The Cult Leader designates the Deceiver among already-recruited
        // Cultists "at the moment of recruitment or any point after"
        // (rules.md §3.3) -- `convert()` stamps a plain recruit's
        // character as `Cultist` (or leaves a named role's character
        // alone), so `AssignCharacter` must still be able to upgrade a
        // `Cultist`-labeled player to `Deceiver` afterward.
        let (mut state, ..) = setup_full_game();
        let cult_leader = state.cult_leader().unwrap();
        let recruit = add_player(&mut state, "Recruit", Faction::Uprising);
        apply_command(&mut state, Command::FinalizeSetup).unwrap();
        grant_recruitment_slots(&mut state, 1);
        apply_command(
            &mut state,
            Command::Convert {
                converter: cult_leader,
                target: recruit,
            },
        )
        .unwrap();
        assert_eq!(
            state.player(recruit).unwrap().character,
            Some(Character::NormalUprising),
            "a converted Normal Uprising member keeps their own character"
        );

        // A plain (no prior character) recruit is the case that actually
        // gets stamped `Cultist` -- exercise that path directly too.
        let plain = add_player(&mut state, "Plain", Faction::Uprising);
        grant_recruitment_slots(&mut state, 1);
        apply_command(
            &mut state,
            Command::Convert {
                converter: cult_leader,
                target: plain,
            },
        )
        .unwrap();
        assert_eq!(
            state.player(plain).unwrap().character,
            Some(Character::Cultist)
        );

        apply_command(
            &mut state,
            Command::AssignCharacter {
                player: plain,
                character: Character::Deceiver,
            },
        )
        .unwrap();
        assert_eq!(
            state.player(plain).unwrap().character,
            Some(Character::Deceiver)
        );
    }

    #[test]
    fn convert_rejects_a_target_who_is_already_converted() {
        let (mut state, king_queen, ..) = setup_full_game();
        let cult_leader = state.cult_leader().unwrap();
        grant_recruitment_slots(&mut state, 2);
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

    // --- Phase 2 ---

    struct Phase2Players {
        king_queen: PlayerId,
        prince_princess: PlayerId,
        leader: PlayerId,
        cult_leader: PlayerId,
        oracle: PlayerId,
        almanac: PlayerId,
        priest: PlayerId,
        potion_maker: PlayerId,
        magistrate: PlayerId,
        spymaster: PlayerId,
        bartender: PlayerId,
        medic: PlayerId,
        firebrand: PlayerId,
        cell_leader: PlayerId,
        deceiver: PlayerId,
        normal_ton: PlayerId,
        normal_uprising: PlayerId,
        duelist: PlayerId,
        agitator: PlayerId,
        grand_inquisitor: PlayerId,
    }

    fn assign_new(
        state: &mut GameState,
        name: &str,
        faction: Faction,
        character: Character,
    ) -> PlayerId {
        let id = add_player(state, name, faction);
        apply_command(
            state,
            Command::AssignCharacter {
                player: id,
                character,
            },
        )
        .unwrap();
        id
    }

    /// A game with one player holding every Phase 2 character, plus one
    /// spare generic Ton and Uprising member (for abilities like
    /// `ArmVoteShield` and Normal Ton's auto-succeed, which key off the
    /// generic catch-all characters `FinalizeSetup` assigns). Every Phase 2
    /// counter/flag starts at 0/unused -- tests that need an ability
    /// available seed it directly (same convention as `grant_recruitment_slots`).
    fn setup_phase2_game() -> (GameState, Phase2Players) {
        let mut state = GameState::new();
        let king_queen = assign_new(&mut state, "King", Faction::Ton, Character::KingQueen);
        let prince_princess = assign_new(
            &mut state,
            "Prince",
            Faction::Ton,
            Character::PrincePrincess,
        );
        let oracle = assign_new(&mut state, "Oracle", Faction::Ton, Character::Oracle);
        let almanac = assign_new(&mut state, "Almanac", Faction::Ton, Character::Almanac);
        let priest = assign_new(
            &mut state,
            "Priest",
            Faction::Ton,
            Character::PriestPriestess,
        );
        let potion_maker = assign_new(&mut state, "Potion", Faction::Ton, Character::PotionMaker);
        let magistrate = assign_new(
            &mut state,
            "Magistrate",
            Faction::Ton,
            Character::Magistrate,
        );
        let duelist = assign_new(&mut state, "Duelist", Faction::Ton, Character::Duelist);
        let grand_inquisitor = assign_new(
            &mut state,
            "GrandInquisitor",
            Faction::Ton,
            Character::GrandInquisitor,
        );
        let normal_ton = add_player(&mut state, "NormalTon", Faction::Ton);

        let leader = assign_new(
            &mut state,
            "Leader",
            Faction::Uprising,
            Character::RevolutionaryLeader,
        );
        let spymaster = assign_new(
            &mut state,
            "Spymaster",
            Faction::Uprising,
            Character::Spymaster,
        );
        let bartender = assign_new(
            &mut state,
            "Bartender",
            Faction::Uprising,
            Character::Bartender,
        );
        let medic = assign_new(
            &mut state,
            "Medic",
            Faction::Uprising,
            Character::DoctorMedic,
        );
        let firebrand = assign_new(
            &mut state,
            "Firebrand",
            Faction::Uprising,
            Character::Firebrand,
        );
        let cell_leader = assign_new(
            &mut state,
            "CellLeader",
            Faction::Uprising,
            Character::CellLeader,
        );
        let agitator = assign_new(
            &mut state,
            "Agitator",
            Faction::Uprising,
            Character::Agitator,
        );
        let normal_uprising = add_player(&mut state, "NormalUprising", Faction::Uprising);

        let cult_leader = assign_new(
            &mut state,
            "CultLeader",
            Faction::Cult,
            Character::CultLeader,
        );
        let deceiver = assign_new(&mut state, "Deceiver", Faction::Cult, Character::Deceiver);

        apply_command(&mut state, Command::FinalizeSetup).unwrap();

        (
            state,
            Phase2Players {
                king_queen,
                prince_princess,
                leader,
                cult_leader,
                oracle,
                almanac,
                priest,
                potion_maker,
                magistrate,
                spymaster,
                bartender,
                medic,
                firebrand,
                cell_leader,
                deceiver,
                normal_ton,
                normal_uprising,
                duelist,
                agitator,
                grand_inquisitor,
            },
        )
    }

    // --- Info-check family ---

    #[test]
    fn use_oracle_delivers_the_targets_dossier() {
        let (mut state, p) = setup_phase2_game();
        state.oracle_checks_available = 1;
        let events = apply_command(
            &mut state,
            Command::UseOracle {
                player: p.oracle,
                target: p.cult_leader,
            },
        )
        .unwrap();
        assert_eq!(state.oracle_checks_available, 0);
        match &events[0] {
            DomainEvent::InfoCheckDelivered {
                querier,
                target,
                kind,
                answer,
            } => {
                assert_eq!(*querier, p.oracle);
                assert_eq!(*target, Some(p.cult_leader));
                assert_eq!(*kind, InfoQueryKind::FullHistory);
                match answer {
                    InfoCheckAnswer::Dossier(d) => {
                        assert_eq!(d.apparent_faction, Faction::Cult);
                        assert!(!d.converted);
                        assert_eq!(d.character, Some(Character::CultLeader));
                    }
                    other => panic!("expected a Dossier, got {other:?}"),
                }
            }
            other => panic!("expected InfoCheckDelivered, got {other:?}"),
        }
    }

    #[test]
    fn use_oracle_rejects_a_non_oracle() {
        let (mut state, p) = setup_phase2_game();
        state.oracle_checks_available = 1;
        let result = apply_command(
            &mut state,
            Command::UseOracle {
                player: p.almanac,
                target: p.cult_leader,
            },
        );
        assert_eq!(
            result,
            Err(GameError::NotCharacter {
                player: p.almanac,
                required: Character::Oracle,
            })
        );
    }

    #[test]
    fn use_oracle_rejects_an_inactive_oracle() {
        let (mut state, p) = setup_phase2_game();
        apply_command(
            &mut state,
            Command::CastOut {
                player: p.oracle,
                fallback_replacement: None,
            },
        )
        .unwrap();
        state.oracle_checks_available = 1;
        let result = apply_command(
            &mut state,
            Command::UseOracle {
                player: p.oracle,
                target: p.king_queen,
            },
        );
        assert_eq!(result, Err(GameError::NotActive(p.oracle)));
    }

    #[test]
    fn use_oracle_rejects_when_no_checks_are_available() {
        let (mut state, p) = setup_phase2_game();
        let result = apply_command(
            &mut state,
            Command::UseOracle {
                player: p.oracle,
                target: p.cult_leader,
            },
        );
        assert_eq!(
            result,
            Err(GameError::AbilityNotAvailable {
                character: Character::Oracle,
            })
        );
    }

    #[test]
    fn use_oracle_rejects_once_permanently_disabled() {
        let (mut state, p) = setup_phase2_game();
        state.oracle_checks_available = 1;
        apply_command(
            &mut state,
            Command::CastOut {
                player: p.king_queen,
                fallback_replacement: None,
            },
        )
        .unwrap();
        assert!(state.oracle_disabled());
        let result = apply_command(
            &mut state,
            Command::UseOracle {
                player: p.oracle,
                target: p.cult_leader,
            },
        );
        assert_eq!(
            result,
            Err(GameError::AbilityNotAvailable {
                character: Character::Oracle,
            })
        );
    }

    #[test]
    fn use_oracle_rejects_an_unknown_target() {
        let (mut state, p) = setup_phase2_game();
        state.oracle_checks_available = 1;
        let bogus = PlayerId(9999);
        let result = apply_command(
            &mut state,
            Command::UseOracle {
                player: p.oracle,
                target: bogus,
            },
        );
        assert_eq!(result, Err(GameError::UnknownPlayer(bogus)));
    }

    #[test]
    fn use_almanac_picks_three_non_leader_active_players_excluding_self() {
        let (mut state, p) = setup_phase2_game();
        let events = apply_command(&mut state, Command::UseAlmanac { player: p.almanac }).unwrap();
        match &events[0] {
            DomainEvent::InfoCheckDelivered {
                querier,
                target,
                kind,
                answer,
            } => {
                assert_eq!(*querier, p.almanac);
                assert_eq!(*target, None);
                assert_eq!(*kind, InfoQueryKind::NotLeaderSet);
                match answer {
                    InfoCheckAnswer::PlayerSet(set) => {
                        assert_eq!(set.len(), 3);
                        assert!(!set.contains(&p.leader));
                        assert!(!set.contains(&p.almanac));
                    }
                    other => panic!("expected a PlayerSet, got {other:?}"),
                }
            }
            other => panic!("expected InfoCheckDelivered, got {other:?}"),
        }
        assert!(state.almanac_used);
    }

    #[test]
    fn use_almanac_rejects_a_second_use() {
        let (mut state, p) = setup_phase2_game();
        apply_command(&mut state, Command::UseAlmanac { player: p.almanac }).unwrap();
        let result = apply_command(&mut state, Command::UseAlmanac { player: p.almanac });
        assert_eq!(
            result,
            Err(GameError::AbilityNotAvailable {
                character: Character::Almanac,
            })
        );
    }

    #[test]
    fn deceiver_armed_does_not_affect_almanac() {
        let (mut state, p) = setup_phase2_game();
        apply_command(
            &mut state,
            Command::SetDeceiverArmed {
                player: p.deceiver,
                armed: true,
            },
        )
        .unwrap();
        let events = apply_command(&mut state, Command::UseAlmanac { player: p.almanac }).unwrap();
        assert!(!events
            .iter()
            .any(|e| matches!(e, DomainEvent::CheckFalsifiedByDeceiver { .. })));
        assert!(
            state.deceiver_armed,
            "Almanac deliberately bypasses the falsify pipeline, so arming stays unconsumed"
        );
    }

    #[test]
    fn use_spymaster_delivers_faction_color_only_once_per_game() {
        let (mut state, p) = setup_phase2_game();
        let events = apply_command(
            &mut state,
            Command::UseSpymaster {
                player: p.spymaster,
                target: p.king_queen,
            },
        )
        .unwrap();
        match &events[0] {
            DomainEvent::InfoCheckDelivered { answer, .. } => {
                assert_eq!(*answer, InfoCheckAnswer::Faction(Faction::Ton));
            }
            other => panic!("expected InfoCheckDelivered, got {other:?}"),
        }
        let result = apply_command(
            &mut state,
            Command::UseSpymaster {
                player: p.spymaster,
                target: p.king_queen,
            },
        );
        assert_eq!(
            result,
            Err(GameError::AbilityNotAvailable {
                character: Character::Spymaster,
            })
        );
    }

    #[test]
    fn cult_leader_query_is_ton_aligned_reflects_true_faction_not_apparent() {
        // Converting an *Uprising* target here would pass even if the
        // implementation buggily read apparent `faction` instead of
        // `true_faction()` -- Uprising is non-Ton either way. Converting a
        // *Ton* target is the actually-discriminating case: apparent
        // faction stays Ton, but true faction becomes Cult, so only a
        // correct `true_faction()` read produces `false` here.
        let (mut state, p) = setup_phase2_game();
        state.cult_leader_queries_available = 1;
        grant_recruitment_slots(&mut state, 1);
        apply_command(
            &mut state,
            Command::Convert {
                converter: p.cult_leader,
                target: p.oracle,
            },
        )
        .unwrap();
        assert_eq!(
            state.player(p.oracle).unwrap().faction,
            Faction::Ton,
            "apparent faction must still read Ton for this test to be discriminating"
        );

        let events = apply_command(
            &mut state,
            Command::CultLeaderQuery {
                player: p.cult_leader,
                target: p.oracle,
                kind: InfoQueryKind::IsTonAligned,
            },
        )
        .unwrap();
        match &events[0] {
            DomainEvent::InfoCheckDelivered { answer, .. } => {
                assert_eq!(*answer, InfoCheckAnswer::Bool(false));
            }
            other => panic!("expected InfoCheckDelivered, got {other:?}"),
        }
    }

    #[test]
    fn cult_leader_query_is_the_leader() {
        let (mut state, p) = setup_phase2_game();
        state.cult_leader_queries_available = 1;
        let events = apply_command(
            &mut state,
            Command::CultLeaderQuery {
                player: p.cult_leader,
                target: p.leader,
                kind: InfoQueryKind::IsTheLeader,
            },
        )
        .unwrap();
        match &events[0] {
            DomainEvent::InfoCheckDelivered { answer, .. } => {
                assert_eq!(*answer, InfoCheckAnswer::Bool(true));
            }
            other => panic!("expected InfoCheckDelivered, got {other:?}"),
        }
    }

    #[test]
    fn cult_leader_query_rejects_an_invalid_kind() {
        let (mut state, p) = setup_phase2_game();
        state.cult_leader_queries_available = 1;
        let result = apply_command(
            &mut state,
            Command::CultLeaderQuery {
                player: p.cult_leader,
                target: p.leader,
                kind: InfoQueryKind::FullHistory,
            },
        );
        assert_eq!(result, Err(GameError::InvalidInfoQueryKind));
    }

    #[test]
    fn cult_leader_query_rejects_when_none_is_available() {
        let (mut state, p) = setup_phase2_game();
        let result = apply_command(
            &mut state,
            Command::CultLeaderQuery {
                player: p.cult_leader,
                target: p.leader,
                kind: InfoQueryKind::IsTheLeader,
            },
        );
        assert_eq!(
            result,
            Err(GameError::AbilityNotAvailable {
                character: Character::CultLeader,
            })
        );
    }

    #[test]
    fn deceiver_falsifies_a_check_that_targets_them_once_armed_then_stops() {
        let (mut state, p) = setup_phase2_game();
        state.oracle_checks_available = 1;
        apply_command(
            &mut state,
            Command::SetDeceiverArmed {
                player: p.deceiver,
                armed: true,
            },
        )
        .unwrap();

        // rules.md §3.3: "if targeted by another player's info-check
        // ability... may force that check to return a false result" -- the
        // Oracle must actually target the Deceiver for this to fire.
        let events = apply_command(
            &mut state,
            Command::UseOracle {
                player: p.oracle,
                target: p.deceiver,
            },
        )
        .unwrap();

        assert!(events.iter().any(
            |e| matches!(e, DomainEvent::CheckFalsifiedByDeceiver { deceiver } if *deceiver == p.deceiver)
        ));
        let answer = events
            .iter()
            .find_map(|e| match e {
                DomainEvent::InfoCheckDelivered { answer, .. } => Some(answer),
                _ => None,
            })
            .unwrap();
        match answer {
            InfoCheckAnswer::Dossier(d) => {
                // The Deceiver in this fixture is Cult-faction from setup
                // (never run through `Convert`), so their true `converted`
                // is `false` -- falsifying should flip it to `true`.
                assert!(d.converted, "falsified dossier should lie about conversion")
            }
            other => panic!("expected a Dossier, got {other:?}"),
        }
        assert!(!state.deceiver_armed);
        assert!(state.deceiver_falsify_used);

        state.oracle_checks_available = 1;
        let events2 = apply_command(
            &mut state,
            Command::UseOracle {
                player: p.oracle,
                target: p.deceiver,
            },
        )
        .unwrap();
        assert!(!events2
            .iter()
            .any(|e| matches!(e, DomainEvent::CheckFalsifiedByDeceiver { .. })));
    }

    #[test]
    fn deceiver_armed_does_not_falsify_a_check_against_an_unrelated_player() {
        let (mut state, p) = setup_phase2_game();
        state.oracle_checks_available = 1;
        apply_command(
            &mut state,
            Command::SetDeceiverArmed {
                player: p.deceiver,
                armed: true,
            },
        )
        .unwrap();

        // The Deceiver is armed, but this check targets someone else
        // entirely -- it must come back genuine, and the arm must stay
        // unconsumed for a check that actually does target the Deceiver
        // later.
        let events = apply_command(
            &mut state,
            Command::UseOracle {
                player: p.oracle,
                target: p.king_queen,
            },
        )
        .unwrap();
        assert!(!events
            .iter()
            .any(|e| matches!(e, DomainEvent::CheckFalsifiedByDeceiver { .. })));
        match events
            .iter()
            .find_map(|e| match e {
                DomainEvent::InfoCheckDelivered { answer, .. } => Some(answer),
                _ => None,
            })
            .unwrap()
        {
            InfoCheckAnswer::Dossier(d) => assert!(!d.converted),
            other => panic!("expected a Dossier, got {other:?}"),
        }
        assert!(state.deceiver_armed, "arming must stay unconsumed");
        assert!(!state.deceiver_falsify_used);
    }

    #[test]
    fn set_deceiver_armed_rejects_a_non_deceiver() {
        let (mut state, p) = setup_phase2_game();
        let result = apply_command(
            &mut state,
            Command::SetDeceiverArmed {
                player: p.cult_leader,
                armed: true,
            },
        );
        assert_eq!(
            result,
            Err(GameError::NotCharacter {
                player: p.cult_leader,
                required: Character::Deceiver,
            })
        );
    }

    #[test]
    fn set_deceiver_armed_rejects_reuse_after_a_falsify_fires() {
        let (mut state, p) = setup_phase2_game();
        state.oracle_checks_available = 1;
        apply_command(
            &mut state,
            Command::SetDeceiverArmed {
                player: p.deceiver,
                armed: true,
            },
        )
        .unwrap();
        apply_command(
            &mut state,
            Command::UseOracle {
                player: p.oracle,
                target: p.deceiver,
            },
        )
        .unwrap();
        let result = apply_command(
            &mut state,
            Command::SetDeceiverArmed {
                player: p.deceiver,
                armed: true,
            },
        );
        assert_eq!(
            result,
            Err(GameError::AbilityNotAvailable {
                character: Character::Deceiver,
            })
        );
    }

    // --- Protect family ---

    #[test]
    fn priest_protect_succeeds_and_blocks_conversion_this_round() {
        let (mut state, p) = setup_phase2_game();
        state.priest_protects_available = 1;
        grant_recruitment_slots(&mut state, 1);

        apply_command(
            &mut state,
            Command::PriestProtect {
                player: p.priest,
                target: p.leader,
            },
        )
        .unwrap();

        let result = apply_command(
            &mut state,
            Command::Convert {
                converter: p.cult_leader,
                target: p.leader,
            },
        );
        assert_eq!(
            result,
            Err(GameError::ProtectedFromConversionThisRound(p.leader))
        );
    }

    #[test]
    fn priest_protection_expires_after_the_round_advances() {
        let (mut state, p) = setup_phase2_game();
        state.priest_protects_available = 1;
        apply_command(
            &mut state,
            Command::PriestProtect {
                player: p.priest,
                target: p.leader,
            },
        )
        .unwrap();

        apply_command(&mut state, Command::AdvanceRound).unwrap();

        apply_command(
            &mut state,
            Command::Convert {
                converter: p.cult_leader,
                target: p.leader,
            },
        )
        .unwrap();
        assert!(state.player(p.leader).unwrap().converted);
    }

    #[test]
    fn priest_protect_rejects_repeating_a_past_target() {
        let (mut state, p) = setup_phase2_game();
        state.priest_protects_available = 2;
        apply_command(
            &mut state,
            Command::PriestProtect {
                player: p.priest,
                target: p.leader,
            },
        )
        .unwrap();
        let result = apply_command(
            &mut state,
            Command::PriestProtect {
                player: p.priest,
                target: p.leader,
            },
        );
        assert_eq!(result, Err(GameError::AlreadyProtectedByPriest(p.leader)));
    }

    #[test]
    fn priest_protect_rejects_when_none_are_available() {
        let (mut state, p) = setup_phase2_game();
        let result = apply_command(
            &mut state,
            Command::PriestProtect {
                player: p.priest,
                target: p.leader,
            },
        );
        assert_eq!(
            result,
            Err(GameError::AbilityNotAvailable {
                character: Character::PriestPriestess,
            })
        );
    }

    #[test]
    fn priest_protect_rejects_an_inactive_target() {
        let (mut state, p) = setup_phase2_game();
        state.priest_protects_available = 1;
        apply_command(
            &mut state,
            Command::CastOut {
                player: p.leader,
                fallback_replacement: None,
            },
        )
        .unwrap();
        let result = apply_command(
            &mut state,
            Command::PriestProtect {
                player: p.priest,
                target: p.leader,
            },
        );
        assert_eq!(result, Err(GameError::NotActive(p.leader)));
    }

    #[test]
    fn medic_protect_requires_an_open_denouncement() {
        let (mut state, p) = setup_phase2_game();
        let result = apply_command(
            &mut state,
            Command::MedicProtect {
                player: p.medic,
                target: p.king_queen,
            },
        );
        assert_eq!(result, Err(GameError::NoActiveBallotToProtectAgainst));
    }

    #[test]
    fn medic_protect_rejects_repeating_last_rounds_target() {
        let (mut state, p) = setup_phase2_game();
        apply_command(&mut state, Command::OpenDenouncement).unwrap();
        state.medic_protected_last_round = Some(p.king_queen);
        let result = apply_command(
            &mut state,
            Command::MedicProtect {
                player: p.medic,
                target: p.king_queen,
            },
        );
        assert_eq!(
            result,
            Err(GameError::CannotProtectSameTargetConsecutively(
                p.king_queen
            ))
        );
    }

    #[test]
    fn medic_protect_rejects_an_inactive_target() {
        let (mut state, p) = setup_phase2_game();
        apply_command(
            &mut state,
            Command::CastOut {
                player: p.leader,
                fallback_replacement: None,
            },
        )
        .unwrap();
        apply_command(&mut state, Command::OpenDenouncement).unwrap();
        let result = apply_command(
            &mut state,
            Command::MedicProtect {
                player: p.medic,
                target: p.leader,
            },
        );
        assert_eq!(result, Err(GameError::NotActive(p.leader)));
    }

    #[test]
    fn medic_protect_is_a_standing_choice_changeable_within_the_round() {
        let (mut state, p) = setup_phase2_game();
        apply_command(&mut state, Command::OpenDenouncement).unwrap();
        apply_command(
            &mut state,
            Command::MedicProtect {
                player: p.medic,
                target: p.king_queen,
            },
        )
        .unwrap();
        apply_command(
            &mut state,
            Command::MedicProtect {
                player: p.medic,
                target: p.prince_princess,
            },
        )
        .unwrap();
        assert_eq!(state.medic_protected_this_round, Some(p.prince_princess));
    }

    #[test]
    fn medic_protection_backfills_from_the_next_candidate_in_close_ballot() {
        let (mut state, p) = setup_phase2_game();
        apply_command(&mut state, Command::OpenDenouncement).unwrap();
        apply_command(
            &mut state,
            Command::Nominate {
                voter: p.oracle,
                nominee: p.king_queen,
            },
        )
        .unwrap();
        apply_command(
            &mut state,
            Command::Nominate {
                voter: p.almanac,
                nominee: p.prince_princess,
            },
        )
        .unwrap();
        apply_command(&mut state, Command::CloseNomination).unwrap();
        apply_command(&mut state, Command::OpenBallot).unwrap();

        apply_command(
            &mut state,
            Command::MedicProtect {
                player: p.medic,
                target: p.king_queen,
            },
        )
        .unwrap();

        for voter in [p.oracle, p.almanac, p.priest] {
            apply_command(
                &mut state,
                Command::CastBallot {
                    voter,
                    ballot: Ballot::For(p.king_queen),
                },
            )
            .unwrap();
        }
        apply_command(
            &mut state,
            Command::CastBallot {
                voter: p.potion_maker,
                ballot: Ballot::For(p.prince_princess),
            },
        )
        .unwrap();

        let events = apply_command(
            &mut state,
            Command::CloseBallot {
                fallback_replacement: None,
            },
        )
        .unwrap();

        assert_eq!(
            state.player(p.king_queen).unwrap().status,
            PlayerStatus::Active,
            "medic-protected target should survive"
        );
        assert_eq!(
            state.player(p.prince_princess).unwrap().status,
            PlayerStatus::CastOut,
            "the next-highest candidate should backfill the freed slot"
        );
        assert!(events.iter().any(
            |e| matches!(e, DomainEvent::BallotClosed { cast_out } if cast_out == &vec![p.prince_princess])
        ));
    }

    #[test]
    fn medic_protection_saves_an_already_locked_in_candidate_during_a_runoff_with_no_backfill() {
        let (mut state, p) = setup_phase2_game();
        add_player(&mut state, "Extra", Faction::Ton);
        apply_command(&mut state, Command::FinalizeSetup).unwrap();
        assert_eq!(state.competing_player_count(), 21);

        apply_command(&mut state, Command::OpenDenouncement).unwrap();
        apply_command(
            &mut state,
            Command::Nominate {
                voter: p.oracle,
                nominee: p.king_queen,
            },
        )
        .unwrap();
        apply_command(
            &mut state,
            Command::Nominate {
                voter: p.almanac,
                nominee: p.prince_princess,
            },
        )
        .unwrap();
        apply_command(
            &mut state,
            Command::Nominate {
                voter: p.priest,
                nominee: p.spymaster,
            },
        )
        .unwrap();
        apply_command(&mut state, Command::CloseNomination).unwrap();
        apply_command(&mut state, Command::OpenBallot).unwrap();

        for voter in [p.oracle, p.almanac, p.priest, p.potion_maker, p.magistrate] {
            apply_command(
                &mut state,
                Command::CastBallot {
                    voter,
                    ballot: Ballot::For(p.king_queen),
                },
            )
            .unwrap();
        }
        for voter in [p.spymaster, p.bartender, p.medic] {
            apply_command(
                &mut state,
                Command::CastBallot {
                    voter,
                    ballot: Ballot::For(p.prince_princess),
                },
            )
            .unwrap();
        }
        for voter in [p.firebrand, p.cell_leader, p.leader] {
            apply_command(
                &mut state,
                Command::CastBallot {
                    voter,
                    ballot: Ballot::For(p.spymaster),
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
        assert!(
            events
                .iter()
                .any(|e| matches!(e, DomainEvent::RunoffOpened { .. })),
            "expected a runoff for the tied 2nd slot: {events:?}"
        );

        apply_command(
            &mut state,
            Command::MedicProtect {
                player: p.medic,
                target: p.king_queen,
            },
        )
        .unwrap();

        apply_command(
            &mut state,
            Command::CastBallot {
                voter: p.oracle,
                ballot: Ballot::For(p.prince_princess),
            },
        )
        .unwrap();
        let runoff_events = apply_command(
            &mut state,
            Command::CloseRunoff {
                fallback_replacement: None,
            },
        )
        .unwrap();

        assert_eq!(
            state.player(p.king_queen).unwrap().status,
            PlayerStatus::Active,
            "the medic-protected already-locked-in candidate should survive"
        );
        assert_eq!(
            state.player(p.prince_princess).unwrap().status,
            PlayerStatus::CastOut
        );
        match &runoff_events[0] {
            DomainEvent::RunoffClosed { cast_out, .. } => {
                assert!(!cast_out.contains(&p.king_queen));
                assert!(cast_out.contains(&p.prince_princess));
            }
            other => panic!("expected RunoffClosed, got {other:?}"),
        }
    }

    #[test]
    fn potion_immunity_saves_an_already_locked_in_candidate_during_a_runoff_with_no_backfill() {
        // Same shape as the Medic's equivalent test above -- Potion
        // Maker's redesigned named-target immunity is mechanically the
        // Medic's protect family, so it needs the exact same two-removal
        // treatment during a runoff: pulled from the runoff's own tally,
        // *and* filtered out of `already_locked_in` (the original ballot's
        // clean winner from before the tie), since no ranked backfill is
        // possible for an already-decided list.
        let (mut state, p) = setup_phase2_game();
        add_player(&mut state, "Extra", Faction::Ton);
        apply_command(&mut state, Command::FinalizeSetup).unwrap();
        assert_eq!(state.competing_player_count(), 21);

        apply_command(&mut state, Command::OpenDenouncement).unwrap();
        apply_command(
            &mut state,
            Command::Nominate {
                voter: p.oracle,
                nominee: p.king_queen,
            },
        )
        .unwrap();
        apply_command(
            &mut state,
            Command::Nominate {
                voter: p.almanac,
                nominee: p.prince_princess,
            },
        )
        .unwrap();
        apply_command(
            &mut state,
            Command::Nominate {
                voter: p.priest,
                nominee: p.spymaster,
            },
        )
        .unwrap();
        apply_command(&mut state, Command::CloseNomination).unwrap();
        apply_command(&mut state, Command::OpenBallot).unwrap();

        for voter in [p.oracle, p.almanac, p.priest, p.potion_maker, p.magistrate] {
            apply_command(
                &mut state,
                Command::CastBallot {
                    voter,
                    ballot: Ballot::For(p.king_queen),
                },
            )
            .unwrap();
        }
        for voter in [p.spymaster, p.bartender, p.medic] {
            apply_command(
                &mut state,
                Command::CastBallot {
                    voter,
                    ballot: Ballot::For(p.prince_princess),
                },
            )
            .unwrap();
        }
        for voter in [p.firebrand, p.cell_leader, p.leader] {
            apply_command(
                &mut state,
                Command::CastBallot {
                    voter,
                    ballot: Ballot::For(p.spymaster),
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
        assert!(
            events
                .iter()
                .any(|e| matches!(e, DomainEvent::RunoffOpened { .. })),
            "expected a runoff for the tied 2nd slot: {events:?}"
        );

        // Armed *during* the runoff window, targeting King/Queen -- who's
        // already locked in from the *original* ballot, before the tie.
        apply_command(
            &mut state,
            Command::ActivatePotionImmunity {
                player: p.potion_maker,
                target: p.king_queen,
            },
        )
        .unwrap();

        apply_command(
            &mut state,
            Command::CastBallot {
                voter: p.oracle,
                ballot: Ballot::For(p.prince_princess),
            },
        )
        .unwrap();
        let runoff_events = apply_command(
            &mut state,
            Command::CloseRunoff {
                fallback_replacement: None,
            },
        )
        .unwrap();

        assert_eq!(
            state.player(p.king_queen).unwrap().status,
            PlayerStatus::Active,
            "the potion-immunity-protected already-locked-in candidate should survive"
        );
        assert_eq!(
            state.player(p.prince_princess).unwrap().status,
            PlayerStatus::CastOut
        );
        match &runoff_events[0] {
            DomainEvent::RunoffClosed { cast_out, .. } => {
                assert!(!cast_out.contains(&p.king_queen));
                assert!(cast_out.contains(&p.prince_princess));
            }
            other => panic!("expected RunoffClosed, got {other:?}"),
        }
        assert!(state.potion_immunity_target.is_none());
        assert!(state.potion_maker_used);
    }

    #[test]
    fn bartender_makes_the_target_drunk_when_it_lands() {
        let (mut state, p) = setup_phase2_game();
        let events = apply_command(
            &mut state,
            Command::BartenderTarget {
                player: p.bartender,
                target: p.king_queen,
                lands: true,
            },
        )
        .unwrap();
        assert!(state.drunk_this_round.contains(&p.king_queen));
        assert!(matches!(
            events[0],
            DomainEvent::BartenderTargeted { landed: true, .. }
        ));
    }

    #[test]
    fn bartender_ability_fails_silently_when_it_does_not_land() {
        let (mut state, p) = setup_phase2_game();
        apply_command(
            &mut state,
            Command::BartenderTarget {
                player: p.bartender,
                target: p.king_queen,
                lands: false,
            },
        )
        .unwrap();
        assert!(!state.drunk_this_round.contains(&p.king_queen));
    }

    #[test]
    fn bartender_target_rejects_a_second_use_this_round() {
        let (mut state, p) = setup_phase2_game();
        apply_command(
            &mut state,
            Command::BartenderTarget {
                player: p.bartender,
                target: p.king_queen,
                lands: false,
            },
        )
        .unwrap();
        let result = apply_command(
            &mut state,
            Command::BartenderTarget {
                player: p.bartender,
                target: p.prince_princess,
                lands: true,
            },
        );
        assert_eq!(
            result,
            Err(GameError::AbilityNotAvailable {
                character: Character::Bartender,
            })
        );
    }

    #[test]
    fn bartender_target_rejects_an_inactive_target() {
        let (mut state, p) = setup_phase2_game();
        apply_command(
            &mut state,
            Command::CastOut {
                player: p.leader,
                fallback_replacement: None,
            },
        )
        .unwrap();
        let result = apply_command(
            &mut state,
            Command::BartenderTarget {
                player: p.bartender,
                target: p.leader,
                lands: true,
            },
        );
        assert_eq!(result, Err(GameError::NotActive(p.leader)));
    }

    #[test]
    fn a_drunk_player_cannot_nominate_or_vote_this_round() {
        let (mut state, p) = setup_phase2_game();
        apply_command(
            &mut state,
            Command::BartenderTarget {
                player: p.bartender,
                target: p.king_queen,
                lands: true,
            },
        )
        .unwrap();
        apply_command(&mut state, Command::OpenDenouncement).unwrap();
        let result = apply_command(
            &mut state,
            Command::Nominate {
                voter: p.king_queen,
                nominee: p.leader,
            },
        );
        assert_eq!(result, Err(GameError::PlayerIsDrunk(p.king_queen)));

        apply_command(
            &mut state,
            Command::Nominate {
                voter: p.oracle,
                nominee: p.leader,
            },
        )
        .unwrap();
        apply_command(&mut state, Command::CloseNomination).unwrap();
        apply_command(&mut state, Command::OpenBallot).unwrap();
        let result2 = apply_command(
            &mut state,
            Command::CastBallot {
                voter: p.king_queen,
                ballot: Ballot::For(p.leader),
            },
        );
        assert_eq!(result2, Err(GameError::PlayerIsDrunk(p.king_queen)));
    }

    #[test]
    fn drunk_status_and_bartender_use_clear_on_advance_round() {
        let (mut state, p) = setup_phase2_game();
        apply_command(
            &mut state,
            Command::BartenderTarget {
                player: p.bartender,
                target: p.king_queen,
                lands: true,
            },
        )
        .unwrap();
        apply_command(&mut state, Command::AdvanceRound).unwrap();
        assert!(!state.drunk_this_round.contains(&p.king_queen));
        assert!(!state.bartender_used_this_round);
    }

    #[test]
    fn activate_potion_immunity_rejects_a_non_potion_maker() {
        let (mut state, p) = setup_phase2_game();
        let result = apply_command(
            &mut state,
            Command::ActivatePotionImmunity {
                player: p.oracle,
                target: p.king_queen,
            },
        );
        assert_eq!(
            result,
            Err(GameError::NotCharacter {
                player: p.oracle,
                required: Character::PotionMaker,
            })
        );
    }

    #[test]
    fn activate_potion_immunity_rejects_an_inactive_target() {
        let (mut state, p) = setup_phase2_game();
        apply_command(
            &mut state,
            Command::CastOut {
                player: p.normal_ton,
                fallback_replacement: None,
            },
        )
        .unwrap();
        let result = apply_command(
            &mut state,
            Command::ActivatePotionImmunity {
                player: p.potion_maker,
                target: p.normal_ton,
            },
        );
        assert_eq!(result, Err(GameError::NotActive(p.normal_ton)));
    }

    #[test]
    fn potion_immunity_saves_its_named_target_from_the_ballot() {
        let (mut state, p) = setup_phase2_game();
        apply_command(
            &mut state,
            Command::ActivatePotionImmunity {
                player: p.potion_maker,
                target: p.king_queen,
            },
        )
        .unwrap();

        apply_command(&mut state, Command::OpenDenouncement).unwrap();
        apply_command(
            &mut state,
            Command::Nominate {
                voter: p.oracle,
                nominee: p.king_queen,
            },
        )
        .unwrap();
        apply_command(&mut state, Command::CloseNomination).unwrap();
        apply_command(&mut state, Command::OpenBallot).unwrap();
        apply_command(
            &mut state,
            Command::CastBallot {
                voter: p.oracle,
                ballot: Ballot::For(p.king_queen),
            },
        )
        .unwrap();

        let events = apply_command(
            &mut state,
            Command::CloseBallot {
                fallback_replacement: None,
            },
        )
        .unwrap();

        assert_eq!(
            state.player(p.king_queen).unwrap().status,
            PlayerStatus::Active
        );
        assert!(
            matches!(&events[0], DomainEvent::BallotClosed { cast_out } if cast_out.is_empty())
        );
        assert!(state.potion_immunity_target.is_none());
        assert!(state.potion_maker_used);
    }

    #[test]
    fn potion_immunity_cannot_be_activated_a_second_time() {
        let (mut state, p) = setup_phase2_game();
        apply_command(
            &mut state,
            Command::ActivatePotionImmunity {
                player: p.potion_maker,
                target: p.king_queen,
            },
        )
        .unwrap();
        apply_command(&mut state, Command::OpenDenouncement).unwrap();
        apply_command(
            &mut state,
            Command::Nominate {
                voter: p.oracle,
                nominee: p.king_queen,
            },
        )
        .unwrap();
        apply_command(&mut state, Command::CloseNomination).unwrap();
        apply_command(&mut state, Command::OpenBallot).unwrap();
        apply_command(
            &mut state,
            Command::CastBallot {
                voter: p.oracle,
                ballot: Ballot::For(p.king_queen),
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

        let result = apply_command(
            &mut state,
            Command::ActivatePotionImmunity {
                player: p.potion_maker,
                target: p.king_queen,
            },
        );
        assert_eq!(
            result,
            Err(GameError::AbilityNotAvailable {
                character: Character::PotionMaker,
            })
        );
    }

    // --- Vote-weight pair ---

    #[test]
    fn activate_double_vote_rejects_a_non_magistrate_non_firebrand() {
        let (mut state, p) = setup_phase2_game();
        let result = apply_command(&mut state, Command::ActivateDoubleVote { player: p.oracle });
        assert_eq!(
            result,
            Err(GameError::NotCharacter {
                player: p.oracle,
                required: Character::Magistrate,
            })
        );
    }

    #[test]
    fn magistrates_double_vote_counts_twice_in_the_tally() {
        let (mut state, p) = setup_phase2_game();
        apply_command(
            &mut state,
            Command::ActivateDoubleVote {
                player: p.magistrate,
            },
        )
        .unwrap();

        apply_command(&mut state, Command::OpenDenouncement).unwrap();
        apply_command(
            &mut state,
            Command::Nominate {
                voter: p.oracle,
                nominee: p.king_queen,
            },
        )
        .unwrap();
        apply_command(
            &mut state,
            Command::Nominate {
                voter: p.almanac,
                nominee: p.prince_princess,
            },
        )
        .unwrap();
        apply_command(&mut state, Command::CloseNomination).unwrap();
        apply_command(&mut state, Command::OpenBallot).unwrap();

        // Without the double vote this would tie 1-1; the Magistrate's
        // single ballot should decide it outright.
        apply_command(
            &mut state,
            Command::CastBallot {
                voter: p.magistrate,
                ballot: Ballot::For(p.king_queen),
            },
        )
        .unwrap();
        apply_command(
            &mut state,
            Command::CastBallot {
                voter: p.almanac,
                ballot: Ballot::For(p.prince_princess),
            },
        )
        .unwrap();

        let events = apply_command(
            &mut state,
            Command::CloseBallot {
                fallback_replacement: None,
            },
        )
        .unwrap();
        assert!(
            matches!(&events[0], DomainEvent::BallotClosed { cast_out } if cast_out == &vec![p.king_queen])
        );
        assert!(state.magistrate_double_vote_used);
        assert!(!state.magistrate_double_vote_armed);
    }

    #[test]
    fn firebrands_double_vote_counts_twice_in_the_tally() {
        let (mut state, p) = setup_phase2_game();
        apply_command(
            &mut state,
            Command::ActivateDoubleVote {
                player: p.firebrand,
            },
        )
        .unwrap();

        apply_command(&mut state, Command::OpenDenouncement).unwrap();
        apply_command(
            &mut state,
            Command::Nominate {
                voter: p.oracle,
                nominee: p.king_queen,
            },
        )
        .unwrap();
        apply_command(
            &mut state,
            Command::Nominate {
                voter: p.almanac,
                nominee: p.prince_princess,
            },
        )
        .unwrap();
        apply_command(&mut state, Command::CloseNomination).unwrap();
        apply_command(&mut state, Command::OpenBallot).unwrap();

        apply_command(
            &mut state,
            Command::CastBallot {
                voter: p.firebrand,
                ballot: Ballot::For(p.prince_princess),
            },
        )
        .unwrap();
        apply_command(
            &mut state,
            Command::CastBallot {
                voter: p.oracle,
                ballot: Ballot::For(p.king_queen),
            },
        )
        .unwrap();

        let events = apply_command(
            &mut state,
            Command::CloseBallot {
                fallback_replacement: None,
            },
        )
        .unwrap();
        assert!(
            matches!(&events[0], DomainEvent::BallotClosed { cast_out } if cast_out == &vec![p.prince_princess])
        );
    }

    #[test]
    fn activate_double_vote_rejects_reuse() {
        let (mut state, p) = setup_phase2_game();
        apply_command(
            &mut state,
            Command::ActivateDoubleVote {
                player: p.magistrate,
            },
        )
        .unwrap();
        apply_command(&mut state, Command::OpenDenouncement).unwrap();
        apply_command(
            &mut state,
            Command::Nominate {
                voter: p.oracle,
                nominee: p.king_queen,
            },
        )
        .unwrap();
        apply_command(&mut state, Command::CloseNomination).unwrap();
        apply_command(&mut state, Command::OpenBallot).unwrap();
        apply_command(
            &mut state,
            Command::CastBallot {
                voter: p.magistrate,
                ballot: Ballot::For(p.king_queen),
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

        let result = apply_command(
            &mut state,
            Command::ActivateDoubleVote {
                player: p.magistrate,
            },
        );
        assert_eq!(
            result,
            Err(GameError::AbilityNotAvailable {
                character: Character::Magistrate,
            })
        );
    }

    // --- Normal Uprising's reactive vote-shield ---

    #[test]
    fn arm_vote_shield_rejects_a_non_normal_uprising() {
        let (mut state, p) = setup_phase2_game();
        let result = apply_command(&mut state, Command::ArmVoteShield { player: p.oracle });
        assert_eq!(
            result,
            Err(GameError::NotCharacter {
                player: p.oracle,
                required: Character::NormalUprising,
            })
        );
    }

    #[test]
    fn vote_shield_negates_one_vote_against_its_holder() {
        let (mut state, p) = setup_phase2_game();
        apply_command(
            &mut state,
            Command::ArmVoteShield {
                player: p.normal_uprising,
            },
        )
        .unwrap();

        apply_command(&mut state, Command::OpenDenouncement).unwrap();
        apply_command(
            &mut state,
            Command::Nominate {
                voter: p.oracle,
                nominee: p.normal_uprising,
            },
        )
        .unwrap();
        apply_command(
            &mut state,
            Command::Nominate {
                voter: p.almanac,
                nominee: p.king_queen,
            },
        )
        .unwrap();
        apply_command(&mut state, Command::CloseNomination).unwrap();
        apply_command(&mut state, Command::OpenBallot).unwrap();

        // Two votes against the shielded player would normally beat one
        // vote against king_queen; the shield should negate one of them,
        // tying it instead.
        apply_command(
            &mut state,
            Command::CastBallot {
                voter: p.oracle,
                ballot: Ballot::For(p.normal_uprising),
            },
        )
        .unwrap();
        apply_command(
            &mut state,
            Command::CastBallot {
                voter: p.almanac,
                ballot: Ballot::For(p.normal_uprising),
            },
        )
        .unwrap();
        apply_command(
            &mut state,
            Command::CastBallot {
                voter: p.priest,
                ballot: Ballot::For(p.king_queen),
            },
        )
        .unwrap();

        let events = apply_command(
            &mut state,
            Command::CloseBallot {
                fallback_replacement: None,
            },
        )
        .unwrap();
        assert!(
            matches!(&events[0], DomainEvent::RunoffOpened { .. }),
            "the shield should have reduced the tally to a 1-1 tie: {events:?}"
        );
        assert!(state.vote_shield_used.contains(&p.normal_uprising));
        assert!(state.vote_shield_armed.is_empty());
    }

    #[test]
    fn arm_vote_shield_rejects_reuse() {
        let (mut state, p) = setup_phase2_game();
        apply_command(
            &mut state,
            Command::ArmVoteShield {
                player: p.normal_uprising,
            },
        )
        .unwrap();
        apply_command(&mut state, Command::OpenDenouncement).unwrap();
        apply_command(
            &mut state,
            Command::Nominate {
                voter: p.oracle,
                nominee: p.normal_uprising,
            },
        )
        .unwrap();
        apply_command(&mut state, Command::CloseNomination).unwrap();
        apply_command(&mut state, Command::OpenBallot).unwrap();
        apply_command(
            &mut state,
            Command::CastBallot {
                voter: p.oracle,
                ballot: Ballot::For(p.normal_uprising),
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

        let result = apply_command(
            &mut state,
            Command::ArmVoteShield {
                player: p.normal_uprising,
            },
        );
        assert_eq!(
            result,
            Err(GameError::AbilityNotAvailable {
                character: Character::NormalUprising,
            })
        );
    }

    // --- Cell Leader passive knowledge ---

    #[test]
    fn cell_leader_knows_two_other_uprising_members_excluding_the_leader() {
        let (state, p) = setup_phase2_game();
        assert_eq!(state.cell_leader_knows.len(), 2);
        assert!(!state.cell_leader_knows.contains(&p.leader));
        assert!(!state.cell_leader_knows.contains(&p.cell_leader));

        let mut expected: Vec<PlayerId> = state
            .players()
            .filter(|pl| {
                pl.faction == Faction::Uprising && pl.id != p.cell_leader && pl.id != p.leader
            })
            .map(|pl| pl.id)
            .collect();
        expected.sort();
        expected.truncate(2);
        assert_eq!(state.cell_leader_knows, expected);
    }

    // --- Normal Ton's reactive safety-net ---

    #[test]
    fn normal_ton_auto_succeeds_one_failed_task_attempt_once_per_game() {
        let (mut state, p) = setup_phase2_game();
        let task_id = match apply_command(
            &mut state,
            Command::PushTask {
                prompt: "test".into(),
                tier: TaskTier::Easy,
                qualifying_players: BTreeSet::new(),
            },
        )
        .unwrap()[0]
        {
            DomainEvent::TaskPushed { id, .. } => id,
            _ => unreachable!(),
        };

        let events = apply_command(
            &mut state,
            Command::AttemptTask {
                player: p.normal_ton,
                task: task_id,
                named: [p.oracle, p.almanac, p.priest],
            },
        )
        .unwrap();
        assert!(
            matches!(events[0], DomainEvent::TaskAttempted { credited: true, .. }),
            "the failed attempt should be auto-upgraded"
        );
        assert!(state.normal_ton_auto_succeed_used.contains(&p.normal_ton));
    }

    #[test]
    fn normal_ton_auto_succeed_only_triggers_once() {
        let (mut state, p) = setup_phase2_game();
        let task1 = match apply_command(
            &mut state,
            Command::PushTask {
                prompt: "t1".into(),
                tier: TaskTier::Easy,
                qualifying_players: BTreeSet::new(),
            },
        )
        .unwrap()[0]
        {
            DomainEvent::TaskPushed { id, .. } => id,
            _ => unreachable!(),
        };
        let task2 = match apply_command(
            &mut state,
            Command::PushTask {
                prompt: "t2".into(),
                tier: TaskTier::Easy,
                qualifying_players: BTreeSet::new(),
            },
        )
        .unwrap()[0]
        {
            DomainEvent::TaskPushed { id, .. } => id,
            _ => unreachable!(),
        };
        apply_command(
            &mut state,
            Command::AttemptTask {
                player: p.normal_ton,
                task: task1,
                named: [p.oracle, p.almanac, p.priest],
            },
        )
        .unwrap();
        let events2 = apply_command(
            &mut state,
            Command::AttemptTask {
                player: p.normal_ton,
                task: task2,
                named: [p.oracle, p.almanac, p.priest],
            },
        )
        .unwrap();
        assert!(
            matches!(
                events2[0],
                DomainEvent::TaskAttempted {
                    credited: false,
                    ..
                }
            ),
            "a second failure shouldn't also be auto-upgraded"
        );
    }

    // --- Regression coverage from the Phase 2 code review ---

    #[test]
    fn a_converted_normal_ton_member_keeps_their_auto_succeed_ability() {
        let (mut state, p) = setup_phase2_game();
        grant_recruitment_slots(&mut state, 1);
        apply_command(
            &mut state,
            Command::Convert {
                converter: p.cult_leader,
                target: p.normal_ton,
            },
        )
        .unwrap();

        let task_id = match apply_command(
            &mut state,
            Command::PushTask {
                prompt: "t".into(),
                tier: TaskTier::Easy,
                qualifying_players: BTreeSet::new(),
            },
        )
        .unwrap()[0]
        {
            DomainEvent::TaskPushed { id, .. } => id,
            _ => unreachable!(),
        };
        let events = apply_command(
            &mut state,
            Command::AttemptTask {
                player: p.normal_ton,
                task: task_id,
                named: [p.oracle, p.almanac, p.priest],
            },
        )
        .unwrap();
        assert!(
            matches!(events[0], DomainEvent::TaskAttempted { credited: true, .. }),
            "a converted Normal Ton member must keep their real auto-succeed ability, per rules.md's \
             \"a converted player keeps their original character and abilities\""
        );
    }

    #[test]
    fn a_converted_normal_uprising_member_keeps_their_vote_shield_ability() {
        let (mut state, p) = setup_phase2_game();
        grant_recruitment_slots(&mut state, 1);
        apply_command(
            &mut state,
            Command::Convert {
                converter: p.cult_leader,
                target: p.normal_uprising,
            },
        )
        .unwrap();

        apply_command(
            &mut state,
            Command::ArmVoteShield {
                player: p.normal_uprising,
            },
        )
        .unwrap();
        assert!(state.vote_shield_armed.contains(&p.normal_uprising));
    }

    #[test]
    fn fellow_cultists_includes_a_converted_named_role_member() {
        let (mut state, p) = setup_phase2_game();
        grant_recruitment_slots(&mut state, 2);
        apply_command(
            &mut state,
            Command::Convert {
                converter: p.cult_leader,
                target: p.oracle,
            },
        )
        .unwrap();
        apply_command(
            &mut state,
            Command::Convert {
                converter: p.cult_leader,
                target: p.normal_ton,
            },
        )
        .unwrap();

        // A converted Oracle keeps their `Oracle` character label (see the
        // two tests above) but is now a full member of the Cult's
        // fellow-member network too -- `fellow_cultists_for` must key off
        // true faction, not the (unchanged) character label.
        let fellow = state.fellow_cultists_for(p.oracle);
        assert!(fellow.contains(&p.normal_ton));
        assert!(fellow.contains(&p.cult_leader));
        assert!(!fellow.contains(&p.oracle));
    }

    #[test]
    fn medic_consecutive_round_rule_survives_an_intervening_contest_round() {
        let (mut state, p) = setup_phase2_game();
        apply_command(&mut state, Command::AdvanceRound).unwrap(); // -> Two
        apply_command(&mut state, Command::AdvanceRound).unwrap(); // -> Three
        assert_eq!(state.current_round(), Round::Three);

        apply_command(&mut state, Command::OpenDenouncement).unwrap();
        apply_command(
            &mut state,
            Command::MedicProtect {
                player: p.medic,
                target: p.king_queen,
            },
        )
        .unwrap();
        apply_command(
            &mut state,
            Command::Nominate {
                voter: p.almanac,
                nominee: p.oracle,
            },
        )
        .unwrap();
        apply_command(&mut state, Command::CloseNomination).unwrap();
        apply_command(&mut state, Command::OpenBallot).unwrap();
        apply_command(
            &mut state,
            Command::CloseBallot {
                fallback_replacement: None,
            },
        )
        .unwrap();
        assert_eq!(state.medic_protected_last_round, Some(p.king_queen));

        // Round 4 is a contest round with no Denouncement at all -- the
        // rotation must NOT re-fire here (it's driven by a Denouncement
        // actually closing, not by `AdvanceRound`), or it would wipe the
        // memory of Round 3's protection before Round 5 ever checks it.
        apply_command(&mut state, Command::AdvanceRound).unwrap(); // -> Four
        assert_eq!(state.medic_protected_last_round, Some(p.king_queen));

        apply_command(&mut state, Command::AdvanceRound).unwrap(); // -> Five
        assert_eq!(state.current_round(), Round::Five);
        apply_command(&mut state, Command::OpenDenouncement).unwrap();
        let result = apply_command(
            &mut state,
            Command::MedicProtect {
                player: p.medic,
                target: p.king_queen,
            },
        );
        assert_eq!(
            result,
            Err(GameError::CannotProtectSameTargetConsecutively(
                p.king_queen
            ))
        );
    }

    #[test]
    fn multiple_normal_uprising_players_can_each_arm_their_own_vote_shield() {
        let (mut state, p) = setup_phase2_game();
        let second_uprising = add_player(&mut state, "SecondUprising", Faction::Uprising);
        apply_command(&mut state, Command::FinalizeSetup).unwrap();
        assert_eq!(
            state.player(second_uprising).unwrap().character,
            Some(Character::NormalUprising)
        );

        apply_command(
            &mut state,
            Command::ArmVoteShield {
                player: p.normal_uprising,
            },
        )
        .unwrap();
        apply_command(
            &mut state,
            Command::ArmVoteShield {
                player: second_uprising,
            },
        )
        .unwrap();

        apply_command(&mut state, Command::OpenDenouncement).unwrap();
        apply_command(
            &mut state,
            Command::Nominate {
                voter: p.oracle,
                nominee: p.normal_uprising,
            },
        )
        .unwrap();
        apply_command(
            &mut state,
            Command::Nominate {
                voter: p.almanac,
                nominee: second_uprising,
            },
        )
        .unwrap();
        apply_command(&mut state, Command::CloseNomination).unwrap();
        apply_command(&mut state, Command::OpenBallot).unwrap();
        // One vote against each shielded candidate -- both should be fully
        // negated since each holds their own independent shield, not one
        // shared global slot.
        apply_command(
            &mut state,
            Command::CastBallot {
                voter: p.oracle,
                ballot: Ballot::For(p.normal_uprising),
            },
        )
        .unwrap();
        apply_command(
            &mut state,
            Command::CastBallot {
                voter: p.almanac,
                ballot: Ballot::For(second_uprising),
            },
        )
        .unwrap();

        let events = apply_command(
            &mut state,
            Command::CloseBallot {
                fallback_replacement: None,
            },
        )
        .unwrap();
        assert!(
            matches!(&events[0], DomainEvent::BallotClosed { cast_out } if cast_out.is_empty()),
            "both votes should have been fully shielded: {events:?}"
        );
        assert!(state.vote_shield_used.contains(&p.normal_uprising));
        assert!(state.vote_shield_used.contains(&second_uprising));
        assert!(state.vote_shield_armed.is_empty());
    }

    #[test]
    fn potion_immunity_and_an_armed_double_vote_both_apply_in_the_same_round() {
        // Regression test: under the old blanket-immunity design, Potion
        // Maker short-circuited the whole tally, leaving the double vote
        // (and every other ballot modifier) un-consumed. Now the tally
        // always genuinely runs -- Potion Maker just pulls its one named
        // target out of it, like Medic -- so the double vote is properly
        // consumed even though King/Queen (the only candidate) survives.
        let (mut state, p) = setup_phase2_game();
        apply_command(
            &mut state,
            Command::ActivateDoubleVote {
                player: p.magistrate,
            },
        )
        .unwrap();
        apply_command(
            &mut state,
            Command::ActivatePotionImmunity {
                player: p.potion_maker,
                target: p.king_queen,
            },
        )
        .unwrap();

        apply_command(&mut state, Command::OpenDenouncement).unwrap();
        apply_command(
            &mut state,
            Command::Nominate {
                voter: p.oracle,
                nominee: p.king_queen,
            },
        )
        .unwrap();
        apply_command(&mut state, Command::CloseNomination).unwrap();
        apply_command(&mut state, Command::OpenBallot).unwrap();
        apply_command(
            &mut state,
            Command::CastBallot {
                voter: p.magistrate,
                ballot: Ballot::For(p.king_queen),
            },
        )
        .unwrap();
        let events = apply_command(
            &mut state,
            Command::CloseBallot {
                fallback_replacement: None,
            },
        )
        .unwrap();

        assert!(
            matches!(&events[0], DomainEvent::BallotClosed { cast_out } if cast_out.is_empty())
        );
        assert!(!state.magistrate_double_vote_armed);
        assert!(state.magistrate_double_vote_used);
        assert!(state.potion_immunity_target.is_none());
        assert!(state.potion_maker_used);
    }

    #[test]
    fn potion_immunity_and_an_armed_vote_shield_both_apply_in_the_same_round() {
        let (mut state, p) = setup_phase2_game();
        apply_command(
            &mut state,
            Command::ArmVoteShield {
                player: p.normal_uprising,
            },
        )
        .unwrap();
        apply_command(
            &mut state,
            Command::ActivatePotionImmunity {
                player: p.potion_maker,
                target: p.normal_uprising,
            },
        )
        .unwrap();

        apply_command(&mut state, Command::OpenDenouncement).unwrap();
        apply_command(
            &mut state,
            Command::Nominate {
                voter: p.oracle,
                nominee: p.normal_uprising,
            },
        )
        .unwrap();
        apply_command(&mut state, Command::CloseNomination).unwrap();
        apply_command(&mut state, Command::OpenBallot).unwrap();
        apply_command(
            &mut state,
            Command::CastBallot {
                voter: p.oracle,
                ballot: Ballot::For(p.normal_uprising),
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

        assert!(state.vote_shield_used.contains(&p.normal_uprising));
        assert!(state.vote_shield_armed.is_empty());
        assert!(state.potion_immunity_target.is_none());
        assert!(state.potion_maker_used);
    }

    #[test]
    fn info_checks_for_returns_every_check_delivered_to_that_querier_without_crosstalk() {
        let (mut state, p) = setup_phase2_game();
        apply_command(
            &mut state,
            Command::UseSpymaster {
                player: p.spymaster,
                target: p.king_queen,
            },
        )
        .unwrap();
        state.oracle_checks_available = 1;
        apply_command(
            &mut state,
            Command::UseOracle {
                player: p.oracle,
                target: p.king_queen,
            },
        )
        .unwrap();

        let spymaster_checks = state.info_checks_for(p.spymaster);
        assert_eq!(spymaster_checks.len(), 1);
        assert_eq!(spymaster_checks[0].kind, InfoQueryKind::FactionColorOnly);

        let oracle_checks = state.info_checks_for(p.oracle);
        assert_eq!(oracle_checks.len(), 1);
        assert_eq!(oracle_checks[0].kind, InfoQueryKind::FullHistory);
    }

    // --- Phase 3: Denouncement procedural modifiers ---

    #[test]
    fn duelist_challenge_adds_to_the_surfaced_list_on_top_of_natural_nominees() {
        let (mut state, p) = setup_phase2_game();
        apply_command(&mut state, Command::OpenDenouncement).unwrap();
        apply_command(
            &mut state,
            Command::Nominate {
                voter: p.oracle,
                nominee: p.king_queen,
            },
        )
        .unwrap();

        apply_command(
            &mut state,
            Command::DuelistChallenge {
                player: p.duelist,
                target: p.medic,
            },
        )
        .unwrap();

        let events = apply_command(&mut state, Command::CloseNomination).unwrap();
        let surfaced = match &events[0] {
            DomainEvent::NominationClosed { surfaced } => surfaced.clone(),
            other => panic!("expected NominationClosed, got {other:?}"),
        };
        assert!(surfaced.contains(&p.king_queen));
        assert!(surfaced.contains(&p.medic));
    }

    #[test]
    fn duelist_challenge_is_a_no_op_if_the_target_already_surfaced() {
        let (mut state, p) = setup_phase2_game();
        apply_command(&mut state, Command::OpenDenouncement).unwrap();
        apply_command(
            &mut state,
            Command::Nominate {
                voter: p.oracle,
                nominee: p.king_queen,
            },
        )
        .unwrap();
        apply_command(
            &mut state,
            Command::DuelistChallenge {
                player: p.duelist,
                target: p.king_queen,
            },
        )
        .unwrap();

        let events = apply_command(&mut state, Command::CloseNomination).unwrap();
        let surfaced = match &events[0] {
            DomainEvent::NominationClosed { surfaced } => surfaced.clone(),
            other => panic!("expected NominationClosed, got {other:?}"),
        };
        assert_eq!(surfaced.iter().filter(|&&id| id == p.king_queen).count(), 1);
    }

    #[test]
    fn duelist_challenge_requires_an_open_nomination_phase() {
        let (mut state, p) = setup_phase2_game();
        let result = apply_command(
            &mut state,
            Command::DuelistChallenge {
                player: p.duelist,
                target: p.king_queen,
            },
        );
        assert_eq!(result, Err(GameError::NoDenouncementOpen));

        apply_command(&mut state, Command::OpenDenouncement).unwrap();
        apply_command(
            &mut state,
            Command::Nominate {
                voter: p.oracle,
                nominee: p.king_queen,
            },
        )
        .unwrap();
        apply_command(&mut state, Command::CloseNomination).unwrap();
        let result2 = apply_command(
            &mut state,
            Command::DuelistChallenge {
                player: p.duelist,
                target: p.medic,
            },
        );
        assert_eq!(result2, Err(GameError::NominationNotOpen));
    }

    #[test]
    fn duelist_challenge_rejects_a_second_use() {
        let (mut state, p) = setup_phase2_game();
        apply_command(&mut state, Command::OpenDenouncement).unwrap();
        apply_command(
            &mut state,
            Command::DuelistChallenge {
                player: p.duelist,
                target: p.king_queen,
            },
        )
        .unwrap();
        let result = apply_command(
            &mut state,
            Command::DuelistChallenge {
                player: p.duelist,
                target: p.medic,
            },
        );
        assert_eq!(
            result,
            Err(GameError::AbilityNotAvailable {
                character: Character::Duelist,
            })
        );
    }

    #[test]
    fn duelist_challenge_rejects_a_non_duelist() {
        let (mut state, p) = setup_phase2_game();
        apply_command(&mut state, Command::OpenDenouncement).unwrap();
        let result = apply_command(
            &mut state,
            Command::DuelistChallenge {
                player: p.oracle,
                target: p.king_queen,
            },
        );
        assert_eq!(
            result,
            Err(GameError::NotCharacter {
                player: p.oracle,
                required: Character::Duelist,
            })
        );
    }

    #[test]
    fn duelist_challenge_rejects_an_inactive_target() {
        let (mut state, p) = setup_phase2_game();
        apply_command(
            &mut state,
            Command::CastOut {
                player: p.king_queen,
                fallback_replacement: None,
            },
        )
        .unwrap();
        apply_command(&mut state, Command::OpenDenouncement).unwrap();
        let result = apply_command(
            &mut state,
            Command::DuelistChallenge {
                player: p.duelist,
                target: p.king_queen,
            },
        );
        assert_eq!(result, Err(GameError::NotActive(p.king_queen)));
    }

    #[test]
    fn agitator_redirect_adds_the_target_to_the_live_discussion_candidates() {
        let (mut state, p) = setup_phase2_game();
        apply_command(&mut state, Command::OpenDenouncement).unwrap();
        apply_command(
            &mut state,
            Command::Nominate {
                voter: p.oracle,
                nominee: p.king_queen,
            },
        )
        .unwrap();
        apply_command(&mut state, Command::CloseNomination).unwrap();

        apply_command(
            &mut state,
            Command::AgitatorRedirect {
                player: p.agitator,
                target: p.medic,
            },
        )
        .unwrap();

        match state.denouncement_phase() {
            Some(DenouncementPhase::Discussion { surfaced }) => {
                assert!(surfaced.contains(&p.king_queen));
                assert!(surfaced.contains(&p.medic));
            }
            other => panic!("expected Discussion phase, got {other:?}"),
        }

        // Carries forward into the Ballot too.
        apply_command(&mut state, Command::OpenBallot).unwrap();
        match state.denouncement_phase() {
            Some(DenouncementPhase::Ballot { surfaced, .. }) => {
                assert!(surfaced.contains(&p.medic));
            }
            other => panic!("expected Ballot phase, got {other:?}"),
        }
    }

    #[test]
    fn agitator_redirect_is_a_no_op_if_the_target_already_surfaced() {
        let (mut state, p) = setup_phase2_game();
        apply_command(&mut state, Command::OpenDenouncement).unwrap();
        apply_command(
            &mut state,
            Command::Nominate {
                voter: p.oracle,
                nominee: p.king_queen,
            },
        )
        .unwrap();
        apply_command(&mut state, Command::CloseNomination).unwrap();
        apply_command(
            &mut state,
            Command::AgitatorRedirect {
                player: p.agitator,
                target: p.king_queen,
            },
        )
        .unwrap();

        match state.denouncement_phase() {
            Some(DenouncementPhase::Discussion { surfaced }) => {
                assert_eq!(surfaced.iter().filter(|&&id| id == p.king_queen).count(), 1);
            }
            other => panic!("expected Discussion phase, got {other:?}"),
        }
    }

    #[test]
    fn agitator_redirect_requires_an_open_discussion_phase() {
        let (mut state, p) = setup_phase2_game();
        let result = apply_command(
            &mut state,
            Command::AgitatorRedirect {
                player: p.agitator,
                target: p.king_queen,
            },
        );
        assert_eq!(result, Err(GameError::NoDenouncementOpen));

        apply_command(&mut state, Command::OpenDenouncement).unwrap();
        let result2 = apply_command(
            &mut state,
            Command::AgitatorRedirect {
                player: p.agitator,
                target: p.king_queen,
            },
        );
        assert_eq!(result2, Err(GameError::DiscussionNotOpen));
    }

    #[test]
    fn agitator_redirect_rejects_a_second_use() {
        let (mut state, p) = setup_phase2_game();
        apply_command(&mut state, Command::OpenDenouncement).unwrap();
        apply_command(
            &mut state,
            Command::Nominate {
                voter: p.oracle,
                nominee: p.king_queen,
            },
        )
        .unwrap();
        apply_command(&mut state, Command::CloseNomination).unwrap();
        apply_command(
            &mut state,
            Command::AgitatorRedirect {
                player: p.agitator,
                target: p.medic,
            },
        )
        .unwrap();
        let result = apply_command(
            &mut state,
            Command::AgitatorRedirect {
                player: p.agitator,
                target: p.priest,
            },
        );
        assert_eq!(
            result,
            Err(GameError::AbilityNotAvailable {
                character: Character::Agitator,
            })
        );
    }

    #[test]
    fn agitator_redirect_rejects_a_non_agitator() {
        let (mut state, p) = setup_phase2_game();
        apply_command(&mut state, Command::OpenDenouncement).unwrap();
        apply_command(
            &mut state,
            Command::Nominate {
                voter: p.oracle,
                nominee: p.king_queen,
            },
        )
        .unwrap();
        apply_command(&mut state, Command::CloseNomination).unwrap();
        let result = apply_command(
            &mut state,
            Command::AgitatorRedirect {
                player: p.medic,
                target: p.king_queen,
            },
        );
        assert_eq!(
            result,
            Err(GameError::NotCharacter {
                player: p.medic,
                required: Character::Agitator,
            })
        );
    }

    #[test]
    fn agitator_redirect_rejects_an_inactive_target() {
        let (mut state, p) = setup_phase2_game();
        apply_command(
            &mut state,
            Command::CastOut {
                player: p.king_queen,
                fallback_replacement: None,
            },
        )
        .unwrap();
        apply_command(&mut state, Command::OpenDenouncement).unwrap();
        apply_command(
            &mut state,
            Command::Nominate {
                voter: p.oracle,
                nominee: p.medic,
            },
        )
        .unwrap();
        apply_command(&mut state, Command::CloseNomination).unwrap();
        let result = apply_command(
            &mut state,
            Command::AgitatorRedirect {
                player: p.agitator,
                target: p.king_queen,
            },
        );
        assert_eq!(result, Err(GameError::NotActive(p.king_queen)));
    }

    #[test]
    fn grand_inquisitor_forces_two_cast_outs_regardless_of_headcount() {
        let (mut state, p) = setup_phase2_game();
        assert!(
            state.competing_player_count() <= 20,
            "this fixture must stay in the flat 1-slot population tier for the test to be meaningful"
        );
        apply_command(
            &mut state,
            Command::ActivateGrandInquisitor {
                player: p.grand_inquisitor,
            },
        )
        .unwrap();

        apply_command(&mut state, Command::OpenDenouncement).unwrap();
        apply_command(
            &mut state,
            Command::Nominate {
                voter: p.oracle,
                nominee: p.king_queen,
            },
        )
        .unwrap();
        apply_command(
            &mut state,
            Command::Nominate {
                voter: p.almanac,
                nominee: p.medic,
            },
        )
        .unwrap();
        apply_command(&mut state, Command::CloseNomination).unwrap();
        apply_command(&mut state, Command::OpenBallot).unwrap();
        apply_command(
            &mut state,
            Command::CastBallot {
                voter: p.oracle,
                ballot: Ballot::For(p.king_queen),
            },
        )
        .unwrap();
        apply_command(
            &mut state,
            Command::CastBallot {
                voter: p.almanac,
                ballot: Ballot::For(p.medic),
            },
        )
        .unwrap();

        let events = apply_command(
            &mut state,
            Command::CloseBallot {
                fallback_replacement: None,
            },
        )
        .unwrap();
        assert!(
            matches!(&events[0], DomainEvent::BallotClosed { cast_out }
                if cast_out.len() == 2 && cast_out.contains(&p.king_queen) && cast_out.contains(&p.medic)),
            "expected both candidates cast out despite the flat 1-slot headcount: {events:?}"
        );
        assert!(state.grand_inquisitor_used);
        assert!(!state.grand_inquisitor_armed);
    }

    #[test]
    fn grand_inquisitor_override_also_applies_during_a_runoff() {
        let (mut state, p) = setup_phase2_game();
        apply_command(&mut state, Command::OpenDenouncement).unwrap();
        apply_command(
            &mut state,
            Command::Nominate {
                voter: p.oracle,
                nominee: p.king_queen,
            },
        )
        .unwrap();
        apply_command(
            &mut state,
            Command::Nominate {
                voter: p.almanac,
                nominee: p.medic,
            },
        )
        .unwrap();
        apply_command(&mut state, Command::CloseNomination).unwrap();
        apply_command(&mut state, Command::OpenBallot).unwrap();
        // Both candidates tie at 1 vote each for the single normal slot.
        apply_command(
            &mut state,
            Command::CastBallot {
                voter: p.oracle,
                ballot: Ballot::For(p.king_queen),
            },
        )
        .unwrap();
        apply_command(
            &mut state,
            Command::CastBallot {
                voter: p.almanac,
                ballot: Ballot::For(p.medic),
            },
        )
        .unwrap();
        let events = apply_command(
            &mut state,
            Command::CloseBallot {
                fallback_replacement: None,
            },
        )
        .unwrap();
        assert!(
            matches!(&events[0], DomainEvent::RunoffOpened { .. }),
            "expected a tie into a runoff: {events:?}"
        );

        // The Grand Inquisitor invokes their office during the runoff
        // window, not before the original ballot -- see `close_runoff`'s
        // own override check.
        apply_command(
            &mut state,
            Command::ActivateGrandInquisitor {
                player: p.grand_inquisitor,
            },
        )
        .unwrap();
        apply_command(
            &mut state,
            Command::CastBallot {
                voter: p.priest,
                ballot: Ballot::For(p.king_queen),
            },
        )
        .unwrap();
        apply_command(
            &mut state,
            Command::CastBallot {
                voter: p.potion_maker,
                ballot: Ballot::For(p.medic),
            },
        )
        .unwrap();

        let runoff_events = apply_command(
            &mut state,
            Command::CloseRunoff {
                fallback_replacement: None,
            },
        )
        .unwrap();
        match &runoff_events[0] {
            DomainEvent::RunoffClosed {
                cast_out,
                unfilled_slot,
            } => {
                assert_eq!(cast_out.len(), 2);
                assert!(cast_out.contains(&p.king_queen));
                assert!(cast_out.contains(&p.medic));
                assert!(!unfilled_slot);
            }
            other => panic!("expected RunoffClosed, got {other:?}"),
        }
    }

    #[test]
    fn grand_inquisitor_during_a_runoff_still_caps_the_whole_denouncement_at_two() {
        // Regression test: at a population where the *original* ballot
        // already locks in a clean winner before a tie sends the second
        // slot to a runoff, arming the Grand Inquisitor during that runoff
        // must still cap the Denouncement's total at 2 -- not add 2 more
        // on top of whoever the original ballot already resolved.
        let (mut state, p) = setup_phase2_game();
        add_player(&mut state, "Extra", Faction::Ton);
        apply_command(&mut state, Command::FinalizeSetup).unwrap();
        assert_eq!(
            state.competing_player_count(),
            21,
            "need 21-30 for a 2-slot execution count"
        );

        apply_command(&mut state, Command::OpenDenouncement).unwrap();
        for (voter, nominee) in [
            (p.oracle, p.king_queen),
            (p.almanac, p.medic),
            (p.priest, p.priest),
        ] {
            apply_command(&mut state, Command::Nominate { voter, nominee }).unwrap();
        }
        apply_command(&mut state, Command::CloseNomination).unwrap();
        apply_command(&mut state, Command::OpenBallot).unwrap();
        // king_queen: a clean 5-vote winner. medic and priest: tied at 2
        // votes each for the second slot.
        for voter in [
            p.oracle,
            p.almanac,
            p.potion_maker,
            p.magistrate,
            p.spymaster,
        ] {
            apply_command(
                &mut state,
                Command::CastBallot {
                    voter,
                    ballot: Ballot::For(p.king_queen),
                },
            )
            .unwrap();
        }
        for voter in [p.bartender, p.firebrand] {
            apply_command(
                &mut state,
                Command::CastBallot {
                    voter,
                    ballot: Ballot::For(p.medic),
                },
            )
            .unwrap();
        }
        for voter in [p.cell_leader, p.deceiver] {
            apply_command(
                &mut state,
                Command::CastBallot {
                    voter,
                    ballot: Ballot::For(p.priest),
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
        assert!(
            matches!(&events[0], DomainEvent::RunoffOpened { .. }),
            "expected king_queen locked in cleanly, medic/priest tied into a runoff: {events:?}"
        );

        apply_command(
            &mut state,
            Command::ActivateGrandInquisitor {
                player: p.grand_inquisitor,
            },
        )
        .unwrap();
        // medic outpolls priest in the runoff -- with the override
        // correctly capped, only medic should take the one slot left.
        for voter in [p.normal_ton, p.normal_uprising] {
            apply_command(
                &mut state,
                Command::CastBallot {
                    voter,
                    ballot: Ballot::For(p.medic),
                },
            )
            .unwrap();
        }
        apply_command(
            &mut state,
            Command::CastBallot {
                voter: p.duelist,
                ballot: Ballot::For(p.priest),
            },
        )
        .unwrap();

        let runoff_events = apply_command(
            &mut state,
            Command::CloseRunoff {
                fallback_replacement: None,
            },
        )
        .unwrap();
        match &runoff_events[0] {
            DomainEvent::RunoffClosed { cast_out, .. } => {
                assert_eq!(
                    cast_out.len(),
                    2,
                    "Grand Inquisitor promises exactly 2 total, not 3: {cast_out:?}"
                );
                assert!(cast_out.contains(&p.king_queen));
                assert!(cast_out.contains(&p.medic));
                assert!(
                    !cast_out.contains(&p.priest),
                    "priest lost the runoff and must not also be swept in"
                );
            }
            other => panic!("expected RunoffClosed, got {other:?}"),
        }
    }

    #[test]
    fn activate_grand_inquisitor_rejects_a_non_grand_inquisitor() {
        let (mut state, p) = setup_phase2_game();
        let result = apply_command(
            &mut state,
            Command::ActivateGrandInquisitor { player: p.oracle },
        );
        assert_eq!(
            result,
            Err(GameError::NotCharacter {
                player: p.oracle,
                required: Character::GrandInquisitor,
            })
        );
    }

    #[test]
    fn activate_grand_inquisitor_rejects_reuse() {
        let (mut state, p) = setup_phase2_game();
        apply_command(
            &mut state,
            Command::ActivateGrandInquisitor {
                player: p.grand_inquisitor,
            },
        )
        .unwrap();
        apply_command(&mut state, Command::OpenDenouncement).unwrap();
        apply_command(
            &mut state,
            Command::Nominate {
                voter: p.oracle,
                nominee: p.king_queen,
            },
        )
        .unwrap();
        apply_command(&mut state, Command::CloseNomination).unwrap();
        apply_command(&mut state, Command::OpenBallot).unwrap();
        apply_command(
            &mut state,
            Command::CastBallot {
                voter: p.oracle,
                ballot: Ballot::For(p.king_queen),
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

        let result = apply_command(
            &mut state,
            Command::ActivateGrandInquisitor {
                player: p.grand_inquisitor,
            },
        );
        assert_eq!(
            result,
            Err(GameError::AbilityNotAvailable {
                character: Character::GrandInquisitor,
            })
        );
    }

    #[test]
    fn grand_inquisitor_and_potion_immunity_both_apply_in_the_same_round() {
        // Regression test for the exact carryover bug a 3-agent review
        // flagged: under the old blanket-immunity design, Potion Maker's
        // activation short-circuited the whole tally, leaving Grand
        // Inquisitor's forced-2-slots override armed but unconsumed -- so
        // it would silently carry into a *later*, unrelated Denouncement
        // instead of applying to the round it was actually armed for. Now
        // the tally always genuinely runs: Grand Inquisitor forces 2 slots
        // this round, and Potion Maker independently protects its one
        // named target from among whoever gets selected.
        let (mut state, p) = setup_phase2_game();
        apply_command(
            &mut state,
            Command::ActivateGrandInquisitor {
                player: p.grand_inquisitor,
            },
        )
        .unwrap();
        apply_command(
            &mut state,
            Command::ActivatePotionImmunity {
                player: p.potion_maker,
                target: p.king_queen,
            },
        )
        .unwrap();

        apply_command(&mut state, Command::OpenDenouncement).unwrap();
        apply_command(
            &mut state,
            Command::Nominate {
                voter: p.oracle,
                nominee: p.king_queen,
            },
        )
        .unwrap();
        apply_command(
            &mut state,
            Command::Nominate {
                voter: p.spymaster,
                nominee: p.prince_princess,
            },
        )
        .unwrap();
        apply_command(&mut state, Command::CloseNomination).unwrap();
        apply_command(&mut state, Command::OpenBallot).unwrap();
        apply_command(
            &mut state,
            Command::CastBallot {
                voter: p.oracle,
                ballot: Ballot::For(p.king_queen),
            },
        )
        .unwrap();
        apply_command(
            &mut state,
            Command::CastBallot {
                voter: p.spymaster,
                ballot: Ballot::For(p.prince_princess),
            },
        )
        .unwrap();
        let events = apply_command(
            &mut state,
            Command::CloseBallot {
                fallback_replacement: None,
            },
        )
        .unwrap();

        // Grand Inquisitor forced 2 slots this round, but Potion Maker
        // pulled King/Queen out of the tally entirely -- only
        // Prince/Princess (the one remaining candidate with any votes)
        // actually gets Cast Out; the second forced slot has nobody left
        // to fill it.
        assert!(matches!(
            &events[0],
            DomainEvent::BallotClosed { cast_out } if cast_out == &vec![p.prince_princess]
        ));
        assert_eq!(
            state.player(p.king_queen).unwrap().status,
            PlayerStatus::Active
        );
        assert!(!state.grand_inquisitor_armed);
        assert!(state.grand_inquisitor_used);
        assert!(state.potion_immunity_target.is_none());
        assert!(state.potion_maker_used);
    }

    // --- Phase 3: contest rounds + the Leader's Confidants ---

    #[test]
    fn record_contest_result_rejects_a_non_contest_round() {
        let mut state = GameState::new();
        let result = apply_command(
            &mut state,
            Command::RecordContestResult {
                round: Round::Three,
                category: ContestCategory::Strength,
                ton_won: true,
            },
        );
        assert_eq!(result, Err(GameError::NotAContestRound(Round::Three)));
    }

    #[test]
    fn record_contest_result_rejects_a_duplicate() {
        let mut state = GameState::new();
        apply_command(&mut state, Command::AdvanceRound).unwrap(); // -> Two
        apply_command(
            &mut state,
            Command::RecordContestResult {
                round: Round::Two,
                category: ContestCategory::Strength,
                ton_won: true,
            },
        )
        .unwrap();
        let result = apply_command(
            &mut state,
            Command::RecordContestResult {
                round: Round::Two,
                category: ContestCategory::Strength,
                ton_won: false,
            },
        );
        assert_eq!(
            result,
            Err(GameError::ContestResultAlreadyRecorded {
                round: Round::Two,
                category: ContestCategory::Strength,
            })
        );
    }

    #[test]
    fn record_contest_result_tracks_distinct_categories_independently() {
        let mut state = GameState::new();
        apply_command(&mut state, Command::AdvanceRound).unwrap(); // -> Two
        apply_command(
            &mut state,
            Command::RecordContestResult {
                round: Round::Two,
                category: ContestCategory::Strength,
                ton_won: true,
            },
        )
        .unwrap();
        // A different category in the same round is unaffected by the
        // first one already being recorded.
        apply_command(
            &mut state,
            Command::RecordContestResult {
                round: Round::Two,
                category: ContestCategory::Creativity,
                ton_won: false,
            },
        )
        .unwrap();
        // And the same category in a *different* contest round is its own
        // independent slot too.
        apply_command(&mut state, Command::AdvanceRound).unwrap(); // -> Three
        apply_command(&mut state, Command::AdvanceRound).unwrap(); // -> Four
        apply_command(
            &mut state,
            Command::RecordContestResult {
                round: Round::Four,
                category: ContestCategory::Strength,
                ton_won: false,
            },
        )
        .unwrap();
    }

    #[test]
    fn record_contest_result_rejects_a_round_that_has_not_happened_yet() {
        let mut state = GameState::new();
        // A fresh game starts at Round::One -- Round::Two hasn't happened
        // yet, so this must be rejected even though it's a valid contest
        // round in the abstract (the exact host mistake this check exists
        // to catch: a round selector left on a stale default).
        let result = apply_command(
            &mut state,
            Command::RecordContestResult {
                round: Round::Two,
                category: ContestCategory::Strength,
                ton_won: true,
            },
        );
        assert_eq!(
            result,
            Err(GameError::ContestRoundNotYetReached(Round::Two))
        );
    }

    #[test]
    fn record_contest_result_allows_correcting_a_past_round() {
        let mut state = GameState::new();
        apply_command(&mut state, Command::AdvanceRound).unwrap(); // -> Two
        apply_command(&mut state, Command::AdvanceRound).unwrap(); // -> Three
        apply_command(&mut state, Command::AdvanceRound).unwrap(); // -> Four
                                                                   // Round Two already happened -- a host filling in a category they
                                                                   // forgot to tap in earlier must still be able to.
        apply_command(
            &mut state,
            Command::RecordContestResult {
                round: Round::Two,
                category: ContestCategory::Strength,
                ton_won: true,
            },
        )
        .unwrap();
    }

    #[test]
    fn a_ton_contest_loss_triggers_a_leader_confidant() {
        let mut state = GameState::new();
        let leader = assign_new(
            &mut state,
            "Leader",
            Faction::Uprising,
            Character::RevolutionaryLeader,
        );
        let member = add_player(&mut state, "Member", Faction::Uprising);
        apply_command(&mut state, Command::FinalizeSetup).unwrap();
        apply_command(&mut state, Command::AdvanceRound).unwrap(); // -> Two

        let events = apply_command(
            &mut state,
            Command::RecordContestResult {
                round: Round::Two,
                category: ContestCategory::Strength,
                ton_won: false,
            },
        )
        .unwrap();

        assert!(events.iter().any(|e| matches!(
            e,
            DomainEvent::LeaderConfidantRevealed { leader: l, confidant }
                if *l == leader && *confidant == member
        )));
    }

    #[test]
    fn a_ton_contest_win_does_not_trigger_a_confidant() {
        let mut state = GameState::new();
        assign_new(
            &mut state,
            "Leader",
            Faction::Uprising,
            Character::RevolutionaryLeader,
        );
        add_player(&mut state, "Member", Faction::Uprising);
        apply_command(&mut state, Command::FinalizeSetup).unwrap();
        apply_command(&mut state, Command::AdvanceRound).unwrap(); // -> Two

        let events = apply_command(
            &mut state,
            Command::RecordContestResult {
                round: Round::Two,
                category: ContestCategory::Strength,
                ton_won: true,
            },
        )
        .unwrap();

        assert!(!events
            .iter()
            .any(|e| matches!(e, DomainEvent::LeaderConfidantRevealed { .. })));
    }

    #[test]
    fn leader_confidant_selection_is_deterministic_and_never_repeats_the_same_person() {
        let mut state = GameState::new();
        let leader = assign_new(
            &mut state,
            "Leader",
            Faction::Uprising,
            Character::RevolutionaryLeader,
        );
        let member_a = add_player(&mut state, "A", Faction::Uprising);
        let member_b = add_player(&mut state, "B", Faction::Uprising);
        apply_command(&mut state, Command::FinalizeSetup).unwrap();
        apply_command(&mut state, Command::AdvanceRound).unwrap(); // -> Two
        let _ = leader;

        let events1 = apply_command(
            &mut state,
            Command::RecordContestResult {
                round: Round::Two,
                category: ContestCategory::Strength,
                ton_won: false,
            },
        )
        .unwrap();
        let first = events1
            .iter()
            .find_map(|e| match e {
                DomainEvent::LeaderConfidantRevealed { confidant, .. } => Some(*confidant),
                _ => None,
            })
            .unwrap();
        assert_eq!(first, member_a.min(member_b));

        let events2 = apply_command(
            &mut state,
            Command::RecordContestResult {
                round: Round::Two,
                category: ContestCategory::Creativity,
                ton_won: false,
            },
        )
        .unwrap();
        let second = events2
            .iter()
            .find_map(|e| match e {
                DomainEvent::LeaderConfidantRevealed { confidant, .. } => Some(*confidant),
                _ => None,
            })
            .unwrap();
        assert_eq!(second, member_a.max(member_b));
        assert_ne!(first, second);

        // Both known Uprising members already know the Leader -- a third
        // loss is a harmless no-op (rules.md's self-limiting clause).
        let events3 = apply_command(
            &mut state,
            Command::RecordContestResult {
                round: Round::Two,
                category: ContestCategory::Intelligence,
                ton_won: false,
            },
        )
        .unwrap();
        assert!(!events3
            .iter()
            .any(|e| matches!(e, DomainEvent::LeaderConfidantRevealed { .. })));
    }

    #[test]
    fn leader_confidant_knowledge_is_cleared_on_leader_succession() {
        // Regression test: `leader_known_to` resolves dynamically against
        // whoever `state.revolutionary_leader` currently is. Without
        // clearing `leader_known_by` on succession, a past Confidant would
        // instantly and silently learn the brand-new successor's identity
        // for free the moment the old Leader is Cast Out, despite the
        // Confidants mechanic never having fired for the successor at all.
        let mut state = GameState::new();
        let leader = assign_new(
            &mut state,
            "Leader",
            Faction::Uprising,
            Character::RevolutionaryLeader,
        );
        let confidant = add_player(&mut state, "Confidant", Faction::Uprising);
        let successor = add_player(&mut state, "Successor", Faction::Uprising);
        apply_command(&mut state, Command::FinalizeSetup).unwrap();
        apply_command(&mut state, Command::AdvanceRound).unwrap(); // -> Two

        apply_command(
            &mut state,
            Command::RecordContestResult {
                round: Round::Two,
                category: ContestCategory::Strength,
                ton_won: false,
            },
        )
        .unwrap();
        assert_eq!(state.leader_known_to(confidant), Some(leader));

        apply_command(
            &mut state,
            Command::CastOut {
                player: leader,
                fallback_replacement: Some(successor),
            },
        )
        .unwrap();
        assert_eq!(state.revolutionary_leader(), Some(successor));

        // The old Confidant must NOT automatically know the new Leader --
        // and the new Leader must start with nobody knowing them.
        assert_eq!(state.leader_known_to(confidant), None);
        assert!(state.confidants_known_to_leader(successor).is_empty());
    }

    #[test]
    fn leader_confidant_never_selects_the_leader_themselves() {
        let mut state = GameState::new();
        assign_new(
            &mut state,
            "Leader",
            Faction::Uprising,
            Character::RevolutionaryLeader,
        );
        apply_command(&mut state, Command::FinalizeSetup).unwrap();
        apply_command(&mut state, Command::AdvanceRound).unwrap(); // -> Two

        // No other Uprising member exists -- nothing eligible to reveal.
        let events = apply_command(
            &mut state,
            Command::RecordContestResult {
                round: Round::Two,
                category: ContestCategory::Strength,
                ton_won: false,
            },
        )
        .unwrap();
        assert!(!events
            .iter()
            .any(|e| matches!(e, DomainEvent::LeaderConfidantRevealed { .. })));
    }

    #[test]
    fn leader_confidant_never_selects_an_already_converted_uprising_member() {
        // Regression test: `trigger_leader_confidant` used to filter
        // candidates on the apparent `faction` field instead of
        // `true_faction()`, so a secretly-converted Uprising member could
        // be picked as a Confidant -- handing the Cult exactly the intel
        // it wants most (the real Leader's identity) via its own asset.
        let mut state = GameState::new();
        let leader = assign_new(
            &mut state,
            "Leader",
            Faction::Uprising,
            Character::RevolutionaryLeader,
        );
        let cult_leader = assign_new(
            &mut state,
            "CultLeader",
            Faction::Cult,
            Character::CultLeader,
        );
        let turncoat = add_player(&mut state, "Turncoat", Faction::Uprising);
        let loyal = add_player(&mut state, "Loyal", Faction::Uprising);
        apply_command(&mut state, Command::FinalizeSetup).unwrap();
        apply_command(&mut state, Command::AdvanceRound).unwrap(); // -> Two, slot
        apply_command(
            &mut state,
            Command::Convert {
                converter: cult_leader,
                target: turncoat,
            },
        )
        .unwrap();

        let events = apply_command(
            &mut state,
            Command::RecordContestResult {
                round: Round::Two,
                category: ContestCategory::Strength,
                ton_won: false,
            },
        )
        .unwrap();
        let revealed_to = events
            .iter()
            .find_map(|e| match e {
                DomainEvent::LeaderConfidantRevealed { confidant, .. } => Some(*confidant),
                _ => None,
            })
            .unwrap();
        assert_eq!(revealed_to, loyal);

        // The next loss finds nobody else genuinely eligible -- the
        // already-converted Turncoat must never be selected, even as a
        // fallback once the one loyal candidate is used up.
        let events2 = apply_command(
            &mut state,
            Command::RecordContestResult {
                round: Round::Two,
                category: ContestCategory::Creativity,
                ton_won: false,
            },
        )
        .unwrap();
        assert!(!events2
            .iter()
            .any(|e| matches!(e, DomainEvent::LeaderConfidantRevealed { .. })));
        assert!(!state.confidants_known_to_leader(leader).contains(&turncoat));
    }

    #[test]
    fn missing_the_task_threshold_in_a_task_round_triggers_a_confidant() {
        let mut state = GameState::new();
        let leader = assign_new(
            &mut state,
            "Leader",
            Faction::Uprising,
            Character::RevolutionaryLeader,
        );
        let member = add_player(&mut state, "Member", Faction::Uprising);
        add_player(&mut state, "Ton1", Faction::Ton);
        add_player(&mut state, "Ton2", Faction::Ton);
        apply_command(&mut state, Command::FinalizeSetup).unwrap();

        apply_command(&mut state, Command::AdvanceRound).unwrap(); // -> Two
        apply_command(&mut state, Command::AdvanceRound).unwrap(); // -> Three
        assert_eq!(state.current_round(), Round::Three);

        apply_command(
            &mut state,
            Command::PushTask {
                prompt: "t".into(),
                tier: TaskTier::Easy,
                qualifying_players: BTreeSet::new(),
            },
        )
        .unwrap();
        // Neither Ton player attempted anything -- 0% completion.
        let events = apply_command(&mut state, Command::CloseTasks).unwrap();
        assert!(events.iter().any(|e| matches!(
            e,
            DomainEvent::LeaderConfidantRevealed { leader: l, confidant }
                if *l == leader && *confidant == member
        )));
    }

    #[test]
    fn meeting_the_task_threshold_does_not_trigger_a_confidant() {
        let mut state = GameState::new();
        let leader = assign_new(
            &mut state,
            "Leader",
            Faction::Uprising,
            Character::RevolutionaryLeader,
        );
        let member = add_player(&mut state, "Member", Faction::Uprising);
        let ton1 = add_player(&mut state, "Ton1", Faction::Ton);
        let ton2 = add_player(&mut state, "Ton2", Faction::Ton);
        apply_command(&mut state, Command::FinalizeSetup).unwrap();

        apply_command(&mut state, Command::AdvanceRound).unwrap();
        apply_command(&mut state, Command::AdvanceRound).unwrap();
        assert_eq!(state.current_round(), Round::Three);

        let task_id = match apply_command(
            &mut state,
            Command::PushTask {
                prompt: "t".into(),
                tier: TaskTier::Easy,
                qualifying_players: [member].into_iter().collect(),
            },
        )
        .unwrap()[0]
        {
            DomainEvent::TaskPushed { id, .. } => id,
            _ => unreachable!(),
        };
        // Both active Ton players get credited -- 100% completion, well
        // above the 50% first-pass threshold.
        apply_command(
            &mut state,
            Command::AttemptTask {
                player: ton1,
                task: task_id,
                named: [leader, member, ton2],
            },
        )
        .unwrap();
        apply_command(
            &mut state,
            Command::AttemptTask {
                player: ton2,
                task: task_id,
                named: [leader, member, ton1],
            },
        )
        .unwrap();

        let events = apply_command(&mut state, Command::CloseTasks).unwrap();
        assert!(!events
            .iter()
            .any(|e| matches!(e, DomainEvent::LeaderConfidantRevealed { .. })));
    }

    #[test]
    fn round_one_task_closure_never_triggers_a_confidant() {
        let mut state = GameState::new();
        assign_new(
            &mut state,
            "Leader",
            Faction::Uprising,
            Character::RevolutionaryLeader,
        );
        add_player(&mut state, "Member", Faction::Uprising);
        add_player(&mut state, "Ton1", Faction::Ton);
        apply_command(&mut state, Command::FinalizeSetup).unwrap();
        assert_eq!(state.current_round(), Round::One);

        apply_command(
            &mut state,
            Command::PushTask {
                prompt: "t".into(),
                tier: TaskTier::Easy,
                qualifying_players: BTreeSet::new(),
            },
        )
        .unwrap();
        let events = apply_command(&mut state, Command::CloseTasks).unwrap();
        assert!(!events
            .iter()
            .any(|e| matches!(e, DomainEvent::LeaderConfidantRevealed { .. })));
    }

    #[test]
    fn closing_tasks_with_nothing_open_never_triggers_a_confidant() {
        let mut state = GameState::new();
        assign_new(
            &mut state,
            "Leader",
            Faction::Uprising,
            Character::RevolutionaryLeader,
        );
        apply_command(&mut state, Command::FinalizeSetup).unwrap();
        apply_command(&mut state, Command::AdvanceRound).unwrap();
        apply_command(&mut state, Command::AdvanceRound).unwrap();
        assert_eq!(state.current_round(), Round::Three);

        let events = apply_command(&mut state, Command::CloseTasks).unwrap();
        assert!(!events
            .iter()
            .any(|e| matches!(e, DomainEvent::LeaderConfidantRevealed { .. })));
    }

    // --- Phase 3: the Intermission lottery ---

    fn setup_game_with_voters(count: usize) -> (GameState, Vec<PlayerId>) {
        let mut state = GameState::new();
        let players: Vec<PlayerId> = (0..count)
            .map(|i| add_player(&mut state, &format!("Player{i}"), Faction::Ton))
            .collect();
        apply_command(&mut state, Command::FinalizeSetup).unwrap();
        (state, players)
    }

    #[test]
    fn opt_into_intermission_rejects_a_cast_out_player() {
        let (mut state, players) = setup_game_with_voters(2);
        apply_command(
            &mut state,
            Command::CastOut {
                player: players[0],
                fallback_replacement: None,
            },
        )
        .unwrap();
        let result = apply_command(
            &mut state,
            Command::OptIntoIntermission { player: players[0] },
        );
        assert_eq!(result, Err(GameError::NotActive(players[0])));
    }

    #[test]
    fn opt_into_intermission_is_idempotent() {
        let (mut state, players) = setup_game_with_voters(1);
        apply_command(
            &mut state,
            Command::OptIntoIntermission { player: players[0] },
        )
        .unwrap();
        apply_command(
            &mut state,
            Command::OptIntoIntermission { player: players[0] },
        )
        .unwrap();
        assert!(state.intermission_opt_ins.contains(&players[0]));
    }

    #[test]
    fn draw_intermission_entrants_succeeds_with_a_valid_pool() {
        let (mut state, players) = setup_game_with_voters(3);
        for &p in &players {
            apply_command(&mut state, Command::OptIntoIntermission { player: p }).unwrap();
        }
        let events = apply_command(
            &mut state,
            Command::DrawIntermissionEntrants {
                selected: players.clone(),
            },
        )
        .unwrap();
        assert!(matches!(
            &events[0],
            DomainEvent::IntermissionEntrantsDrawn { entrants } if entrants == &players
        ));
        assert_eq!(state.intermission_entrants(), Some(players.as_slice()));
    }

    #[test]
    fn draw_intermission_entrants_rejects_someone_who_never_opted_in() {
        let (mut state, players) = setup_game_with_voters(1);
        let result = apply_command(
            &mut state,
            Command::DrawIntermissionEntrants {
                selected: vec![players[0]],
            },
        );
        assert_eq!(
            result,
            Err(GameError::InvalidIntermissionEntrant(players[0]))
        );
    }

    #[test]
    fn draw_intermission_entrants_rejects_a_cast_out_opted_in_player() {
        let (mut state, players) = setup_game_with_voters(1);
        apply_command(
            &mut state,
            Command::OptIntoIntermission { player: players[0] },
        )
        .unwrap();
        apply_command(
            &mut state,
            Command::CastOut {
                player: players[0],
                fallback_replacement: None,
            },
        )
        .unwrap();
        let result = apply_command(
            &mut state,
            Command::DrawIntermissionEntrants {
                selected: vec![players[0]],
            },
        );
        assert_eq!(
            result,
            Err(GameError::InvalidIntermissionEntrant(players[0]))
        );
    }

    #[test]
    fn draw_intermission_entrants_rejects_a_duplicate() {
        let (mut state, players) = setup_game_with_voters(1);
        apply_command(
            &mut state,
            Command::OptIntoIntermission { player: players[0] },
        )
        .unwrap();
        let result = apply_command(
            &mut state,
            Command::DrawIntermissionEntrants {
                selected: vec![players[0], players[0]],
            },
        );
        assert_eq!(
            result,
            Err(GameError::DuplicateIntermissionEntrant(players[0]))
        );
    }

    #[test]
    fn draw_intermission_entrants_rejects_more_than_five() {
        let (mut state, players) = setup_game_with_voters(6);
        for &p in &players {
            apply_command(&mut state, Command::OptIntoIntermission { player: p }).unwrap();
        }
        let result = apply_command(
            &mut state,
            Command::DrawIntermissionEntrants { selected: players },
        );
        assert_eq!(result, Err(GameError::TooManyIntermissionEntrants(6)));
    }

    #[test]
    fn draw_intermission_entrants_rejects_reuse() {
        let (mut state, players) = setup_game_with_voters(1);
        apply_command(
            &mut state,
            Command::OptIntoIntermission { player: players[0] },
        )
        .unwrap();
        apply_command(
            &mut state,
            Command::DrawIntermissionEntrants {
                selected: vec![players[0]],
            },
        )
        .unwrap();
        let result = apply_command(
            &mut state,
            Command::DrawIntermissionEntrants {
                selected: vec![players[0]],
            },
        );
        assert_eq!(result, Err(GameError::IntermissionAlreadyDrawn));
    }

    // --- Phase 3: Servant leaderboard + Gallery ---

    #[test]
    fn award_servant_points_rejects_a_non_servant() {
        let mut state = GameState::new();
        let player = add_player(&mut state, "Player", Faction::Ton);
        apply_command(&mut state, Command::FinalizeSetup).unwrap();
        let result = apply_command(
            &mut state,
            Command::AwardServantPoints { player, points: 5 },
        );
        assert_eq!(result, Err(GameError::NotAServant(player)));
    }

    #[test]
    fn award_servant_points_succeeds_for_a_late_arrival_servant() {
        let mut state = GameState::new();
        let player = add_player(&mut state, "Servant", Faction::Servant);
        apply_command(&mut state, Command::FinalizeSetup).unwrap();
        let events = apply_command(
            &mut state,
            Command::AwardServantPoints { player, points: 3 },
        )
        .unwrap();
        assert!(matches!(
            &events[0],
            DomainEvent::ServantPointsAwarded { player: p, points: 3, total: 3 } if *p == player
        ));
    }

    #[test]
    fn award_servant_points_succeeds_for_a_cast_out_player() {
        let mut state = GameState::new();
        let player = add_player(&mut state, "Player", Faction::Ton);
        apply_command(&mut state, Command::FinalizeSetup).unwrap();
        apply_command(
            &mut state,
            Command::CastOut {
                player,
                fallback_replacement: None,
            },
        )
        .unwrap();
        let events = apply_command(
            &mut state,
            Command::AwardServantPoints { player, points: 2 },
        )
        .unwrap();
        assert!(matches!(
            &events[0],
            DomainEvent::ServantPointsAwarded { .. }
        ));
    }

    #[test]
    fn award_servant_points_accumulates_across_multiple_awards() {
        let mut state = GameState::new();
        let player = add_player(&mut state, "Servant", Faction::Servant);
        apply_command(&mut state, Command::FinalizeSetup).unwrap();
        apply_command(
            &mut state,
            Command::AwardServantPoints { player, points: 2 },
        )
        .unwrap();
        let events = apply_command(
            &mut state,
            Command::AwardServantPoints { player, points: 3 },
        )
        .unwrap();
        assert!(matches!(
            &events[0],
            DomainEvent::ServantPointsAwarded { total: 5, .. }
        ));
        assert_eq!(state.servant_leaderboard(), vec![(player, 5)]);
    }

    fn setup_at_finale_with_open_denouncement() -> (GameState, PlayerId) {
        let mut state = GameState::new();
        let cast_out_player = add_player(&mut state, "CastOut", Faction::Ton);
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
        (state, cast_out_player)
    }

    #[test]
    fn submit_gallery_prediction_rejects_a_non_cast_out_player() {
        let (mut state, _) = setup_at_finale_with_open_denouncement();
        let active_player = add_player(&mut state, "Active", Faction::Ton);
        apply_command(&mut state, Command::FinalizeSetup).unwrap();
        let result = apply_command(
            &mut state,
            Command::SubmitGalleryPrediction {
                player: active_player,
                prediction: GalleryPrediction::FactionWins(Faction::Ton),
            },
        );
        assert_eq!(
            result,
            Err(GameError::MustBeCastOutForGallery(active_player))
        );
    }

    #[test]
    fn submit_gallery_prediction_rejects_outside_the_last_denouncement() {
        let mut state = GameState::new();
        let player = add_player(&mut state, "Player", Faction::Ton);
        apply_command(&mut state, Command::FinalizeSetup).unwrap();
        apply_command(
            &mut state,
            Command::CastOut {
                player,
                fallback_replacement: None,
            },
        )
        .unwrap();
        let result = apply_command(
            &mut state,
            Command::SubmitGalleryPrediction {
                player,
                prediction: GalleryPrediction::FactionWins(Faction::Ton),
            },
        );
        assert_eq!(result, Err(GameError::GalleryPredictionWindowClosed));
    }

    #[test]
    fn submit_gallery_prediction_succeeds_and_can_be_replaced() {
        let (mut state, cast_out_player) = setup_at_finale_with_open_denouncement();
        apply_command(
            &mut state,
            Command::SubmitGalleryPrediction {
                player: cast_out_player,
                prediction: GalleryPrediction::FactionWins(Faction::Ton),
            },
        )
        .unwrap();
        apply_command(
            &mut state,
            Command::SubmitGalleryPrediction {
                player: cast_out_player,
                prediction: GalleryPrediction::FactionWins(Faction::Uprising),
            },
        )
        .unwrap();
        assert_eq!(
            state.gallery_predictions.get(&cast_out_player),
            Some(&GalleryPrediction::FactionWins(Faction::Uprising))
        );
    }

    #[test]
    fn resolve_gallery_predictions_awards_correct_predictions_only() {
        let (mut state, cast_out_a) = setup_at_finale_with_open_denouncement();
        let cast_out_b = add_player(&mut state, "CastOutB", Faction::Uprising);
        apply_command(&mut state, Command::FinalizeSetup).unwrap();
        apply_command(
            &mut state,
            Command::CastOut {
                player: cast_out_b,
                fallback_replacement: None,
            },
        )
        .unwrap();
        let target = add_player(&mut state, "Target", Faction::Ton);
        apply_command(&mut state, Command::FinalizeSetup).unwrap();

        apply_command(
            &mut state,
            Command::SubmitGalleryPrediction {
                player: cast_out_a,
                prediction: GalleryPrediction::CastOutIs(target),
            },
        )
        .unwrap();
        apply_command(
            &mut state,
            Command::SubmitGalleryPrediction {
                player: cast_out_b,
                prediction: GalleryPrediction::FactionWins(Faction::Uprising),
            },
        )
        .unwrap();

        // Predictions must be submitted before the ballot closes; resolving
        // requires it to have actually closed.
        apply_command(&mut state, Command::CloseNomination).unwrap();
        apply_command(&mut state, Command::OpenBallot).unwrap();
        apply_command(
            &mut state,
            Command::CloseBallot {
                fallback_replacement: None,
            },
        )
        .unwrap();

        let events = apply_command(
            &mut state,
            Command::ResolveGalleryPredictions {
                actual_cast_out: vec![target],
                actual_winner: Faction::Ton,
            },
        )
        .unwrap();

        // cast_out_a correctly predicted the actual cast-out; cast_out_b
        // incorrectly predicted Uprising when Ton actually won.
        assert!(events.iter().any(
            |e| matches!(e, DomainEvent::ServantPointsAwarded { player, .. } if *player == cast_out_a)
        ));
        assert!(!events.iter().any(
            |e| matches!(e, DomainEvent::ServantPointsAwarded { player, .. } if *player == cast_out_b)
        ));
        assert_eq!(state.servant_leaderboard(), vec![(cast_out_a, 1)]);
    }

    #[test]
    fn resolve_gallery_predictions_rejects_reuse() {
        let (mut state, _) = setup_at_finale_with_open_denouncement();
        apply_command(&mut state, Command::CloseNomination).unwrap();
        apply_command(&mut state, Command::OpenBallot).unwrap();
        apply_command(
            &mut state,
            Command::CloseBallot {
                fallback_replacement: None,
            },
        )
        .unwrap();
        apply_command(
            &mut state,
            Command::ResolveGalleryPredictions {
                actual_cast_out: vec![],
                actual_winner: Faction::Ton,
            },
        )
        .unwrap();
        let result = apply_command(
            &mut state,
            Command::ResolveGalleryPredictions {
                actual_cast_out: vec![],
                actual_winner: Faction::Ton,
            },
        );
        assert_eq!(result, Err(GameError::GalleryAlreadyResolved));
    }

    #[test]
    fn resolve_gallery_predictions_rejects_before_the_finale() {
        let mut state = GameState::new();
        let result = apply_command(
            &mut state,
            Command::ResolveGalleryPredictions {
                actual_cast_out: vec![],
                actual_winner: Faction::Ton,
            },
        );
        assert_eq!(result, Err(GameError::GalleryResolutionTooEarly));
    }

    #[test]
    fn resolve_gallery_predictions_rejects_while_the_last_denouncement_is_still_open() {
        // Regression test: resolution is once-per-game and irreversible --
        // firing it before the real outcome is even known must be rejected,
        // not silently accepted with a caller-guessed "actual" outcome.
        let (mut state, _) = setup_at_finale_with_open_denouncement();
        let result = apply_command(
            &mut state,
            Command::ResolveGalleryPredictions {
                actual_cast_out: vec![],
                actual_winner: Faction::Ton,
            },
        );
        assert_eq!(result, Err(GameError::GalleryResolutionTooEarly));
    }

    #[test]
    fn resolve_gallery_predictions_scores_a_correct_faction_prediction() {
        // Dalton's ruling (Phase 3 review): only one faction ever wins, and
        // the Cult has priority over any overlap -- win_condition::evaluate
        // enforces that directly, so this command only ever needs a single
        // actual_winner, not a set.
        let (mut state, cast_out_player) = setup_at_finale_with_open_denouncement();
        apply_command(
            &mut state,
            Command::SubmitGalleryPrediction {
                player: cast_out_player,
                prediction: GalleryPrediction::FactionWins(Faction::Uprising),
            },
        )
        .unwrap();
        apply_command(&mut state, Command::CloseNomination).unwrap();
        apply_command(&mut state, Command::OpenBallot).unwrap();
        apply_command(
            &mut state,
            Command::CloseBallot {
                fallback_replacement: None,
            },
        )
        .unwrap();

        let events = apply_command(
            &mut state,
            Command::ResolveGalleryPredictions {
                actual_cast_out: vec![],
                actual_winner: Faction::Uprising,
            },
        )
        .unwrap();
        assert!(events.iter().any(
            |e| matches!(e, DomainEvent::ServantPointsAwarded { player, .. } if *player == cast_out_player)
        ));
    }

    #[test]
    fn resolve_gallery_predictions_emits_an_event_even_with_no_correct_predictions() {
        let (mut state, _) = setup_at_finale_with_open_denouncement();
        apply_command(&mut state, Command::CloseNomination).unwrap();
        apply_command(&mut state, Command::OpenBallot).unwrap();
        apply_command(
            &mut state,
            Command::CloseBallot {
                fallback_replacement: None,
            },
        )
        .unwrap();

        // Nobody submitted a prediction at all -- resolution must still
        // leave an event-log trace, the same as every other Phase 2/3
        // command that always emits its own canonical event.
        let events = apply_command(
            &mut state,
            Command::ResolveGalleryPredictions {
                actual_cast_out: vec![],
                actual_winner: Faction::Ton,
            },
        )
        .unwrap();
        assert_eq!(
            events,
            vec![DomainEvent::GalleryPredictionsResolved {
                correct_predictions: 0,
            }]
        );
    }
}
