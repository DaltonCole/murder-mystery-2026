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
    apply_command, view_for, Command, DomainEvent, GameError, GameState, PlayerView, Viewer,
};
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
