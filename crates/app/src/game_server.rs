//! The process-wide game server: the single [`engine::GameState`] every
//! route reads and writes through, plus a broadcast channel that lets
//! connected clients learn "something changed" without polling.
//!
//! Server-only (only compiled under the `server` feature -- see `main.rs`).
//! This is a deliberately simpler stand-in for the implementation plan's
//! recommended mpsc-actor-task architecture: a `Mutex` around `GameState`
//! gives the same core guarantee an actor would (every mutation is
//! serialized through [`engine::apply_command`], nothing ever mutates the
//! state directly) with far less code, which fits this phase's "basic
//! shells" scope. Swap in a real actor if/when lock contention or
//! back-pressure ever becomes a real concern -- neither is likely at a
//! 20-30 player, single-process, one-event-at-a-time party game.

use engine::{
    apply_command, raffle_priority, raffle_winners, task_candidates, ticket_count, ticket_slots,
    view_for, Command, DomainEvent, Faction, GameError, GameState, PlayerId, PlayerStatus,
    PlayerView, Round, TaskTier, Viewer,
};
use rand::seq::SliceRandom;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Mutex, OnceLock};
use tokio::sync::broadcast;

struct GameServer {
    state: Mutex<GameState>,
    changed: broadcast::Sender<()>,
    // Tracks which odd rounds' task phases have already been auto-pushed
    // (see `auto_push_odd_round_tasks`) -- process-local, not persisted
    // `GameState`, since it's purely an app-level "don't repeat this side
    // effect" guard, not a fact about the game itself.
    auto_tasks_pushed: Mutex<BTreeSet<Round>>,
}

fn server() -> &'static GameServer {
    static SERVER: OnceLock<GameServer> = OnceLock::new();
    SERVER.get_or_init(|| {
        let (changed, _receiver) = broadcast::channel(32);
        GameServer {
            state: Mutex::new(GameState::new()),
            changed,
            auto_tasks_pushed: Mutex::new(BTreeSet::new()),
        }
    })
}

/// How many tasks to automatically push per tier -- `(easy, medium, hard)`
/// -- when each odd round's task phase begins (rules.md §4: Rounds 1, 3, 5
/// have tasks; 2 and 4 are contest rounds; see `auto_push_odd_round_tasks`).
/// Round 1's count is fixed by rules.md itself ("exactly 2 fixed tasks, 1
/// easy 1 medium"); Rounds 3 and 5 ramp up per Dalton's own game-night
/// pacing call, not a rules.md quote.
///
/// *** EDIT THIS to retune pacing before game night -- no other code
/// changes needed. ***
fn auto_task_counts(round: Round) -> (usize, usize, usize) {
    match round {
        Round::One => (1, 1, 0),
        Round::Three => (1, 1, 1),
        Round::Five => (0, 2, 2),
        _ => (0, 0, 0),
    }
}

/// Automatically pushes each odd round's task phase (rules.md §4) the
/// moment it begins, so the Host never has to hand-pick which bio-derived
/// candidate to push from the "From player bios" panel -- a reliability/
/// automation review found this was one of the last remaining points of
/// necessary Host involvement in an otherwise-automated round flow. The
/// manual per-candidate buttons and the free-text "Manual entry" fallback
/// stay in the Host console regardless, for a live-event fix-up.
///
/// Triggered by scanning `events` (whatever command was just applied) for
/// `DomainEvent::SetupFinalized` (Round 1) or `DomainEvent::RoundAdvanced`
/// reaching `Round::Three`/`Round::Five` -- covers both ways
/// `Command::FinalizeSetup` can be reached (`run_raffle`'s own internal
/// call, or the Host's manual "Finalize setup" button) since both pass
/// their resulting events through here. `pushed` guards against a
/// legitimate repeat `FinalizeSetup` call (see that command's own doc
/// comment) double-pushing Round 1's tasks.
///
/// For each tier, shuffles `bio::task_candidates`'s pool with real,
/// OS-backed entropy (the same "randomness at the boundary" shape as
/// `run_raffle`/`draw_intermission_entrants`) and pushes
/// `auto_task_counts`'s configured count from the front -- capped at
/// however many distinct candidates actually exist, so a too-small bio
/// pool (a tiny playtest game, or a tier nobody's bio happens to fill)
/// just pushes fewer tasks rather than erroring.
fn auto_push_odd_round_tasks(
    state: &mut GameState,
    events: &[DomainEvent],
    pushed: &mut BTreeSet<Round>,
) -> Vec<DomainEvent> {
    let round = events.iter().find_map(|e| match e {
        DomainEvent::SetupFinalized => Some(Round::One),
        DomainEvent::RoundAdvanced {
            round: round @ (Round::Three | Round::Five),
        } => Some(*round),
        _ => None,
    });
    let Some(round) = round else {
        return Vec::new();
    };
    if !pushed.insert(round) {
        return Vec::new();
    }

    let (easy, medium, hard) = auto_task_counts(round);
    let mut rng = rand::rng();
    let mut new_events = Vec::new();
    for (tier, count) in [
        (TaskTier::Easy, easy),
        (TaskTier::Medium, medium),
        (TaskTier::Hard, hard),
    ] {
        let mut candidates = task_candidates(state, tier);
        candidates.shuffle(&mut rng);
        for candidate in candidates.into_iter().take(count) {
            if let Ok(events) = apply_command(
                state,
                Command::PushTask {
                    prompt: candidate.prompt,
                    tier,
                    qualifying_players: candidate.qualifying_players.into_iter().collect(),
                    expected_code: None,
                },
            ) {
                new_events.extend(events);
            }
        }
    }
    new_events
}

/// Applies one command against the single canonical `GameState`, holding
/// the lock only for the mutation itself. The only place in the whole app
/// allowed to call `engine::apply_command` -- every route-driven mutation
/// funnels through here.
pub fn apply(cmd: Command) -> Result<Vec<DomainEvent>, GameError> {
    let mut events;
    {
        let mut state = lock_state();
        events = apply_command(&mut state, cmd)?;
        let mut pushed = server()
            .auto_tasks_pushed
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let auto_events = auto_push_odd_round_tasks(&mut state, &events, &mut pushed);
        events.extend(auto_events);
    }
    // Errors here just mean nobody's subscribed right now -- fine to
    // ignore, there's nobody waiting to be told.
    let _ = server().changed.send(());
    Ok(events)
}

/// Runs rules.md §1's weighted setup raffle over the current roster and
/// finalizes setup, all under one lock acquisition -- the Host console's
/// "Run the raffle" button is the only caller. This is where the one
/// genuinely random step actually happens (see `engine::raffle`'s module
/// doc comment on "randomness at the boundary": the engine only computes,
/// it never generates its own randomness) -- this function runs natively
/// on the server, so a plain `rand::rng()` is a real OS-backed source,
/// unlike the WASM client half of this same binary.
///
/// Mirrors `bots::HostDriver::setup_game` step for step (ticket-weighted
/// draw, `AssignCharacter` for every winner, ~60/40 Ton/Uprising split for
/// the leftovers, then `CloseRaffle`/`FinalizeSetup`) -- that's the
/// network-driven equivalent of this same sequence for the bot test
/// harness; this is the real one a live host actually presses.
///
/// `String` error (not `GameError`, matching `push_location_task`'s
/// precedent) since the one way this can fail -- a double-click re-running
/// an already-closed raffle -- is an app-level UI mistake, not a domain
/// rejection: a reliability review found this had no guard at all, so a
/// double-click under live-event network latency (the button doesn't
/// disable until a fresh view round-trips back) could draw a *second*,
/// different random shuffle and abort partway through `AssignCharacter`
/// once a winner collides with a character they already hold from the
/// first run, leaving a tangled, half-reassigned roster with no clean way
/// to recover except the Host UI's manual fallback controls.
pub fn run_raffle() -> Result<Vec<DomainEvent>, String> {
    let mut events = Vec::new();
    {
        let mut state = lock_state();
        if state.raffle_closed() {
            return Err("the raffle has already run".to_string());
        }
        let roster: Vec<PlayerId> = state.players().map(|p| p.id).collect();

        let mut tickets = BTreeMap::new();
        let mut low_interest = Vec::new();
        for &id in &roster {
            let level = state.interest_level(id).unwrap_or(0);
            let count = ticket_count(level);
            if count > 0 {
                tickets.insert(id, count);
            } else {
                low_interest.push(id);
            }
        }
        let mut rng = rand::rng();
        let mut slots = ticket_slots(&tickets);
        slots.shuffle(&mut rng);
        low_interest.shuffle(&mut rng);
        let priority = raffle_priority(&slots, &low_interest);
        let winners = raffle_winners(&priority);

        for (character, player) in winners.iter().copied() {
            events.extend(
                apply_command(&mut state, Command::AssignCharacter { player, character })
                    .map_err(|e| e.to_string())?,
            );
        }

        let won_a_role: BTreeSet<PlayerId> = winners.iter().map(|&(_, player)| player).collect();
        let mut remaining: Vec<PlayerId> = roster
            .iter()
            .copied()
            .filter(|id| !won_a_role.contains(id))
            .collect();
        remaining.shuffle(&mut rng);
        let ton_count = (remaining.len() as f64 * 0.6).round() as usize;
        let (ton, uprising) = remaining.split_at(ton_count);
        for (group, faction) in [(ton, Faction::Ton), (uprising, Faction::Uprising)] {
            for &id in group {
                events.extend(
                    apply_command(
                        &mut state,
                        Command::AssignFaction {
                            player: id,
                            faction,
                        },
                    )
                    .map_err(|e| e.to_string())?,
                );
            }
        }

        events.extend(apply_command(&mut state, Command::CloseRaffle).map_err(|e| e.to_string())?);
        events
            .extend(apply_command(&mut state, Command::FinalizeSetup).map_err(|e| e.to_string())?);

        let mut pushed = server()
            .auto_tasks_pushed
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let auto_events = auto_push_odd_round_tasks(&mut state, &events, &mut pushed);
        events.extend(auto_events);
    }
    // Same reasoning as `apply` above: nobody subscribed is a fine outcome
    // to ignore, there's just nobody waiting to be told.
    let _ = server().changed.send(());
    Ok(events)
}

/// Draws up to 5 Intermission entrants (rules.md §4) from whoever's
/// currently opted in and still active -- the Host console's "Draw
/// entrants" button. A reliability review found the Host UI previously
/// asked Dalton to type in entrant player IDs by hand, which is impossible
/// to do correctly: `view_for` deliberately never reveals the opt-in pool
/// to `Viewer::Host` (see `GameState::intermission_opt_ins`'s doc comment
/// on "no ambient god-view"). This runs the actual weighted-nothing (a
/// plain shuffle, every opted-in active player equally likely) draw
/// server-side, reading `GameState` directly rather than the scrubbed
/// view -- the same "randomness at the boundary" shape as `run_raffle`
/// above and `bots::HostDriver::draw_intermission_entrants`.
pub fn draw_intermission_entrants() -> Result<Vec<DomainEvent>, String> {
    let events;
    {
        let mut state = lock_state();
        let active: BTreeSet<PlayerId> = state
            .players()
            .filter(|p| p.status == PlayerStatus::Active)
            .map(|p| p.id)
            .collect();
        let mut candidates: Vec<PlayerId> = state
            .intermission_opt_ins()
            .filter(|id| active.contains(id))
            .collect();
        let mut rng = rand::rng();
        candidates.shuffle(&mut rng);
        candidates.truncate(5);
        events = apply_command(
            &mut state,
            Command::DrawIntermissionEntrants {
                selected: candidates,
            },
        )
        .map_err(|e| e.to_string())?;
    }
    let _ = server().changed.send(());
    Ok(events)
}

/// rules.md §4's pre-authored location tasks (medium: the location stated
/// plainly; hard: a riddle as to where it is) -- `(tier, prompt, code)`.
///
/// *** EDIT THIS before game night *** with the real venue's locations,
/// riddles, and the codes physically placed there -- these are placeholder
/// examples, not real content.
///
/// Deliberately lives *only* here, server-only (`game_server` is compiled
/// under the `server` feature alone -- see `main.rs`'s module doc
/// comment), never in `engine` or anywhere `main.rs`'s `web` feature build
/// reaches: `engine` is a dependency of the WASM client bundle served to
/// every player's browser, so any code stored there would ship straight
/// into that bundle, trivially extractable via devtools -- defeating the
/// entire point of a *physical* location task. The Host browser only ever
/// learns the safe subset (tier + prompt, via `location_task_templates`)
/// and pushes by index (`push_location_task`); the code itself never
/// leaves this server process.
const LOCATION_TASKS: &[(TaskTier, &str, &str)] = &[
    (
        TaskTier::Medium,
        "Head to the coat check and find the code taped underneath the counter.",
        "CHANGE_ME_COATCHECK",
    ),
    (
        TaskTier::Hard,
        "Where the night's first drink was poured, but the bottles never empty -- what's written on the inside of the cabinet door?",
        "CHANGE_ME_BAR",
    ),
];

/// The safe subset of `LOCATION_TASKS` for the Host browser to render a
/// picker from -- index (to push by) plus tier and prompt, never the code.
pub fn location_task_templates() -> Vec<(usize, TaskTier, String)> {
    LOCATION_TASKS
        .iter()
        .enumerate()
        .map(|(i, &(tier, prompt, _code))| (i, tier, prompt.to_string()))
        .collect()
}

/// Pushes `LOCATION_TASKS[index]` as a real, open `TaskDef` (via the
/// ordinary `Command::PushTask`, same as every other task) -- the Host
/// console's per-template "push" button. `String` error (not `GameError`)
/// since an out-of-range index is an app-level mistake, not a domain one.
pub fn push_location_task(index: usize) -> Result<Vec<DomainEvent>, String> {
    let &(tier, prompt, code) = LOCATION_TASKS
        .get(index)
        .ok_or_else(|| format!("no location task at index {index}"))?;
    apply(Command::PushTask {
        prompt: prompt.to_string(),
        tier,
        qualifying_players: BTreeSet::new(),
        expected_code: Some(code.to_string()),
    })
    .map_err(|e| e.to_string())
}

/// The single read path every route uses -- never hands out a raw
/// `GameState`. See `engine::view_for`'s own doc comment for why that
/// matters.
pub fn view(viewer: Viewer) -> PlayerView {
    let state = lock_state();
    view_for(&state, viewer)
}

/// Locks the game state, recovering from mutex poisoning instead of
/// panicking. A panic anywhere inside `apply_command`/`view_for` (a bug,
/// not something expected to happen) would otherwise poison the mutex
/// permanently -- since `SERVER` is a process-wide singleton, that turns
/// one panic into every future request from every connection panicking
/// too, bricking the whole live event until someone restarts the process.
/// For a one-shot, unattended party game, staying up with whatever state
/// existed at the moment of the panic is a better failure mode than a
/// total, permanent outage.
fn lock_state() -> std::sync::MutexGuard<'static, GameState> {
    server()
        .state
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Subscribes to "something changed" notifications. Callers re-fetch their
/// own `view` after each tick rather than being sent state directly --
/// `view_for`'s per-viewer authorization stays the only path data reaches
/// a client through, even for push updates.
pub fn subscribe() -> broadcast::Receiver<()> {
    server().changed.subscribe()
}
