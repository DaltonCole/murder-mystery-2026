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
    apply_command, raffle_priority, raffle_winners, ticket_count, ticket_slots, view_for, Command,
    DomainEvent, Faction, GameError, GameState, PlayerId, PlayerView, Viewer,
};
use rand::seq::SliceRandom;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Mutex, OnceLock};
use tokio::sync::broadcast;

struct GameServer {
    state: Mutex<GameState>,
    changed: broadcast::Sender<()>,
}

fn server() -> &'static GameServer {
    static SERVER: OnceLock<GameServer> = OnceLock::new();
    SERVER.get_or_init(|| {
        let (changed, _receiver) = broadcast::channel(32);
        GameServer {
            state: Mutex::new(GameState::new()),
            changed,
        }
    })
}

/// Applies one command against the single canonical `GameState`, holding
/// the lock only for the mutation itself. The only place in the whole app
/// allowed to call `engine::apply_command` -- every route-driven mutation
/// funnels through here.
pub fn apply(cmd: Command) -> Result<Vec<DomainEvent>, GameError> {
    let result = {
        let mut state = lock_state();
        apply_command(&mut state, cmd)
    };
    if result.is_ok() {
        // Errors here just mean nobody's subscribed right now -- fine to
        // ignore, there's nobody waiting to be told.
        let _ = server().changed.send(());
    }
    result
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
pub fn run_raffle() -> Result<Vec<DomainEvent>, GameError> {
    let mut events = Vec::new();
    {
        let mut state = lock_state();
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
            events.extend(apply_command(
                &mut state,
                Command::AssignCharacter { player, character },
            )?);
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
                events.extend(apply_command(
                    &mut state,
                    Command::AssignFaction {
                        player: id,
                        faction,
                    },
                )?);
            }
        }

        events.extend(apply_command(&mut state, Command::CloseRaffle)?);
        events.extend(apply_command(&mut state, Command::FinalizeSetup)?);
    }
    // Same reasoning as `apply` above: nobody subscribed is a fine outcome
    // to ignore, there's just nobody waiting to be told.
    let _ = server().changed.send(());
    Ok(events)
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
