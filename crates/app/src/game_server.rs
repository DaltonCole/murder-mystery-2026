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
    view_for, Command, DenouncementPhase, DomainEvent, Faction, GameError, GameState, PlayerId,
    PlayerStatus, PlayerView, Round, TaskTier, Viewer,
};
use rand::seq::SliceRandom;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};
use tokio::sync::broadcast;

/// A purely advisory, host-controlled round/phase clock (see
/// `start_timer`'s doc comment) -- deliberately NOT part of `GameState`.
/// It never drives any game rule (nothing auto-advances when it hits
/// zero -- see `crates/app/src/main.rs`'s Host copy for why), so it has
/// no business living in the engine's deterministic, replayable state;
/// this is exactly the same "wall-clock time is a caller-supplied,
/// app-layer concept, not an engine one" boundary this session's earlier
/// automation work already established for randomness.
struct GameTimer {
    started_at: Instant,
    duration: Duration,
}

struct GameServer {
    state: Mutex<GameState>,
    changed: broadcast::Sender<()>,
    // Tracks which odd rounds' task phases have already been pushed (see
    // `push_tasks_for_round`) -- process-local, not persisted `GameState`,
    // since it's purely an app-level "don't repeat this side effect"
    // guard, not a fact about the game itself.
    auto_tasks_pushed: Mutex<BTreeSet<Round>>,
    timer: Mutex<Option<GameTimer>>,
    // Bio-derived task prompts (rules.md §4, Rounds 1/3/5's auto-push
    // pool) the Host has banned from ever being auto-selected -- see
    // `push_tasks_for_round`'s doc comment. Empty by default ("all tasks
    // should be available by default," Dalton's own explicit instruction)
    // -- process-local, not persisted `GameState`, the same "host
    // operational preference, not a fact about the game itself" shape as
    // `auto_tasks_pushed`.
    banned_task_prompts: Mutex<BTreeSet<String>>,
}

fn server() -> &'static GameServer {
    static SERVER: OnceLock<GameServer> = OnceLock::new();
    SERVER.get_or_init(|| {
        let (changed, _receiver) = broadcast::channel(32);
        GameServer {
            state: Mutex::new(GameState::new()),
            changed,
            auto_tasks_pushed: Mutex::new(BTreeSet::new()),
            timer: Mutex::new(None),
            banned_task_prompts: Mutex::new(BTreeSet::new()),
        }
    })
}

/// Bans `prompt` from ever being auto-selected for Round 1/3/5's task push
/// (see `push_tasks_for_round`) -- the Host console's "Ban" button next to
/// a bio-derived candidate. Doesn't affect pushing the same prompt
/// manually (the "Manual entry"/location-task pickers) -- banning only
/// scopes the automatic pool, matching the actual feature request ("tasks
/// should be assigned automatically... allow the admin to ban a certain
/// task"), not a blanket block on that exact text ever being used at all.
pub fn ban_task_prompt(prompt: String) {
    server()
        .banned_task_prompts
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .insert(prompt);
    let _ = server().changed.send(());
}

/// Reverses `ban_task_prompt` -- the Host console's "Unban" button.
pub fn unban_task_prompt(prompt: &str) {
    server()
        .banned_task_prompts
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .remove(prompt);
    let _ = server().changed.send(());
}

/// Every currently-banned task prompt, for the Host console's own list of
/// what's excluded right now.
pub fn banned_task_prompts() -> Vec<String> {
    server()
        .banned_task_prompts
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .iter()
        .cloned()
        .collect()
}

/// Spawns the once-per-process background task that keeps the round timer
/// display live for every connected client even when nobody's taking any
/// other action (e.g. everyone's mid-discussion, not clicking anything).
/// Ticks once a second and does nothing but nudge the existing `changed`
/// broadcast -- the same "something changed, go re-fetch" signal every
/// other mutation already fires -- so it reuses 100% of the existing push
/// plumbing rather than inventing a second one. Never touches `GameState`
/// or any game rule; a missed or delayed tick just means the displayed
/// number is briefly stale, matching this feature's own "roughly how much
/// time is remaining" framing. `OnceLock` guarantees this loop is spawned
/// exactly once no matter how many times a timer gets started.
fn ensure_ticker_running() {
    static TICKER: OnceLock<()> = OnceLock::new();
    TICKER.get_or_init(|| {
        tokio::spawn(async {
            loop {
                tokio::time::sleep(Duration::from_secs(1)).await;
                let timer_active = server()
                    .timer
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner())
                    .is_some();
                if timer_active {
                    let _ = server().changed.send(());
                }
            }
        });
    });
}

/// Starts (or restarts) the round timer at `seconds` from now -- the Host
/// console's "Start timer" button. Purely a display aid: rules.md gives
/// explicit time budgets for each Denouncement phase ("nomination 2 min,
/// discussion 3-4 min, ballot 90 sec, resolution 90 sec") and frames
/// Discussion's specifically as "a shared timer that Dalton starts but
/// doesn't moderate" -- this is that shared, visible clock, generalized to
/// any round/phase rather than hardcoded to one, since Round 1 ("the round
/// ends on the timer") and the contest rounds need the same kind of
/// at-a-glance "how much longer" signal. Deliberately never triggers any
/// command when it reaches zero -- unlike the Denouncement's own
/// auto-close (a condition on completed actions, not a clock), a real
/// countdown-driven auto-advance would need the pause/override safety
/// infrastructure this project has twice now deliberately deferred
/// building; this stays informational, so the Host button remains the
/// only thing that ever actually moves the game forward.
pub fn start_timer(seconds: u32) {
    ensure_ticker_running();
    *server()
        .timer
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(GameTimer {
        started_at: Instant::now(),
        duration: Duration::from_secs(u64::from(seconds)),
    });
    let _ = server().changed.send(());
}

/// Extends the running timer by `seconds`, or starts a fresh one at
/// `seconds` if none is running -- the Host console's "+N min" buttons,
/// for exactly the live-event reality that a real conversation sometimes
/// needs more than the stated budget.
pub fn add_timer_seconds(seconds: u32) {
    ensure_ticker_running();
    let mut timer = server()
        .timer
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let extra = Duration::from_secs(u64::from(seconds));
    match timer.as_mut() {
        Some(t) => t.duration += extra,
        None => {
            *timer = Some(GameTimer {
                started_at: Instant::now(),
                duration: extra,
            })
        }
    }
    drop(timer);
    let _ = server().changed.send(());
}

/// Clears the timer entirely -- the Host console's "Clear" button, for
/// dismissing a finished/no-longer-relevant countdown.
pub fn clear_timer() {
    *server()
        .timer
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner()) = None;
    let _ = server().changed.send(());
}

/// Seconds remaining on the current timer, or `None` if none is running --
/// negative once the timer's run past its duration (rendered as "overtime"
/// rather than clamped to zero, so the Host can see exactly how far past
/// budget a phase has run). Computed fresh from a real `Instant` on every
/// call rather than stored/ticked, so it's never stale by more than however
/// often a caller asks.
pub fn timer_remaining_secs() -> Option<i64> {
    server()
        .timer
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .as_ref()
        .map(|t| t.duration.as_secs() as i64 - t.started_at.elapsed().as_secs() as i64)
}

/// Whether `password` matches the configured Host passphrase, read fresh
/// from the `HOST_PASSWORD` environment variable on every call (so a
/// changed `.env` takes effect on the next login attempt, no restart
/// needed for `dx serve`'s own reload). A UI-level gate only, matching the
/// user's own explicit choice over enforcing this on every host-only
/// `Command` too -- the raw websocket protocol still has no real access
/// control (see `main.rs`'s module doc comment's KNOWN GAP); this only
/// keeps the Host console from being reachable, and Host-privileged data
/// from even being requested, without the passphrase.
///
/// Deliberately open (`true`) when `HOST_PASSWORD` isn't set at all --
/// the Makefile's own `.env` support has said "ready for when the host
/// console needs a password" since before this feature existed, and
/// defaulting to *locked* instead would silently break `make run` for
/// every local dev/test session that's never set one. Set `HOST_PASSWORD`
/// before a real event to actually gate this.
pub fn check_host_password(password: &str) -> bool {
    match std::env::var("HOST_PASSWORD") {
        Ok(configured) => configured == password,
        Err(_) => true,
    }
}

/// How many tasks to push per tier -- `(easy, medium, hard)` -- for each
/// odd round's task phase (rules.md §4: Rounds 1, 3, 5 have tasks; 2 and 4
/// are contest rounds; see `push_tasks_for_round`). Round 1's count is
/// fixed by rules.md itself ("exactly 2 fixed tasks, 1 easy 1 medium");
/// Rounds 3 and 5 ramp up per Dalton's own game-night pacing call, not a
/// rules.md quote.
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

/// Pushes `round`'s configured tasks (see `auto_task_counts`), unless
/// they've already been pushed (`pushed`, this process's own idempotency
/// guard -- see `GameServer::auto_tasks_pushed`'s doc comment). Shared by
/// every way a round's tasks get pushed: automatically for Round 3/5 (see
/// `auto_push_on_round_advance` below) and automatically for Round 1, the
/// moment setup finalizes (see `run_raffle`).
///
/// For each tier, excludes any banned prompt (`banned_task_prompts` --
/// "all tasks should be available by default," so this is normally a
/// no-op filter), then shuffles the remaining `bio::task_candidates` pool
/// with real, OS-backed entropy (the same "randomness at the boundary"
/// shape as `run_raffle`/`draw_intermission_entrants`) and pushes
/// `auto_task_counts`'s configured count from the front -- capped at
/// however many distinct candidates actually exist, so a too-small bio
/// pool (a tiny playtest game, or a tier nobody's bio happens to fill, or
/// every candidate in a tier happening to be banned) just pushes fewer
/// tasks rather than erroring.
fn push_tasks_for_round(
    state: &mut GameState,
    round: Round,
    pushed: &mut BTreeSet<Round>,
) -> Vec<DomainEvent> {
    if !pushed.insert(round) {
        return Vec::new();
    }

    let banned = server()
        .banned_task_prompts
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .clone();
    let (easy, medium, hard) = auto_task_counts(round);
    let mut rng = rand::rng();
    let mut new_events = Vec::new();
    for (tier, count) in [
        (TaskTier::Easy, easy),
        (TaskTier::Medium, medium),
        (TaskTier::Hard, hard),
    ] {
        let mut candidates: Vec<_> = task_candidates(state, tier)
            .into_iter()
            .filter(|c| !banned.contains(&c.prompt))
            .collect();
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

/// Automatically pushes Round 3 or Round 5's task phase (rules.md §4) the
/// moment `AdvanceRound` reaches it, so the Host never has to hand-pick
/// which bio-derived candidate to push from the "From player bios" panel
/// -- a reliability/automation review found this was one of the last
/// remaining points of necessary Host involvement in an otherwise-
/// automated round flow. The manual per-candidate buttons and the
/// free-text "Manual entry" fallback stay in the Host console regardless,
/// for a live-event fix-up.
///
/// Round 1 is deliberately NOT triggered from here -- it's pushed at the
/// end of `run_raffle` instead, the moment setup itself finalizes (Dalton's
/// own explicit instruction: no admin action should be needed to get any
/// round's tasks moving, Round 1 included).
///
/// Triggered by scanning `events` (whatever command was just applied) for
/// `DomainEvent::RoundAdvanced` reaching `Round::Three`/`Round::Five`.
fn auto_push_on_round_advance(
    state: &mut GameState,
    events: &[DomainEvent],
    pushed: &mut BTreeSet<Round>,
) -> Vec<DomainEvent> {
    let round = events.iter().find_map(|e| match e {
        DomainEvent::RoundAdvanced {
            round: round @ (Round::Three | Round::Five),
        } => Some(*round),
        _ => None,
    });
    let Some(round) = round else {
        return Vec::new();
    };
    push_tasks_for_round(state, round, pushed)
}

/// Transfers the crown from `player` (the current King/Queen) to a
/// randomly-chosen eligible Ton player -- rules.md §3.1: "the title
/// passes to a random remaining Ton player." Not a plain `Command` --
/// unlike the rest of `AbilityPanel`, this ability has no target picker at
/// all on `/play`: rules.md gives the King/Queen no say in who receives
/// it, so there's nothing for the player to choose, and this is where the
/// one genuinely random step actually happens (the engine's own
/// `Command::TransferKingQueen` still takes a concrete `new_holder` and
/// validates it -- `GameState::eligible_king_queen_successors` is the
/// public pool this picks from, so the choice can't drift from what the
/// engine would actually accept).
pub fn transfer_king_queen_randomly(player: PlayerId) -> Result<Vec<DomainEvent>, String> {
    let events;
    {
        let mut state = lock_state();
        let mut candidates = state.eligible_king_queen_successors();
        if candidates.is_empty() {
            return Err("no eligible Ton player to receive the crown".to_string());
        }
        candidates.shuffle(&mut rand::rng());
        let new_holder = candidates[0];
        events = apply_command(
            &mut state,
            Command::TransferKingQueen { player, new_holder },
        )
        .map_err(|e| e.to_string())?;
    }
    let _ = server().changed.send(());
    Ok(events)
}

/// Every currently-active player who could still nominate/vote this round
/// -- excludes anyone drunk this round (rules.md: a drunk player "can't
/// nominate or vote" at all, so waiting on one would mean the phase could
/// never close early). Used only by `auto_close_denouncement_phase` below;
/// never exposed to any client view.
fn must_still_act(state: &GameState) -> BTreeSet<PlayerId> {
    state
        .players()
        .filter(|p| p.status == PlayerStatus::Active && !state.is_drunk(p.id))
        .map(|p| p.id)
        .collect()
}

/// Auto-closes a Denouncement's Nomination, Ballot, or Runoff phase the
/// instant every player who could still act has -- an automation review
/// found the Host previously had to watch the room and guess when it was
/// safe to click "Close nomination"/"Close ballot"/"Close runoff" (five
/// buttons already correctly gated by phase, see the Host console's
/// disabled-by-phase fix, but none of them fired themselves). rules.md's
/// "nomination 2 min, ballot 90 sec" time budgets are an upper bound, not
/// a requirement to always wait that long -- there's nothing left to wait
/// for once nobody has anything left to submit.
///
/// Deliberately does NOT do this for Discussion: rules.md frames that
/// phase as "a shared timer that Dalton starts but doesn't moderate," with
/// no per-player action to detect completion from -- only Dalton, watching
/// the actual conversation, can judge when it's genuinely run its course.
/// That phase stays a manual "Open ballot" click, same as every contest
/// round and `AdvanceRound` itself (this app automates *content* and
/// *bookkeeping*, never a judgment call rules.md gives to a human).
///
/// Never fires while the required set is empty (nobody active, or
/// everybody drunk) -- an empty set trivially satisfies "everyone's
/// acted," which would otherwise auto-close a phase nobody could have
/// participated in at all.
///
/// A same-shaped auto-close for the *task* phase (once every active
/// player has attempted every open task) was tried and deliberately
/// reverted: unlike Nomination/Ballot/Runoff, which each open as one
/// atomic action, tasks can be added incrementally over several separate
/// `PushTask` calls (the Host console's "From player bios"/"Manual entry"
/// panels are built specifically for pushing one at a time) -- a fast
/// group finishing the *first* pushed task auto-closed the whole phase
/// before the Host had pushed the rest they intended, confirmed by a real
/// bot-test regression. Nomination/Ballot/Runoff don't have that failure
/// mode: nothing adds *more* candidates/ballots to an already-open phase
/// the way `PushTask` does.
fn auto_close_denouncement_phase(state: &mut GameState) -> Vec<DomainEvent> {
    let required = must_still_act(state);
    if required.is_empty() {
        return Vec::new();
    }
    let command = match state.denouncement_phase() {
        Some(DenouncementPhase::Nomination { submitted }) => required
            .iter()
            .all(|id| submitted.contains_key(id))
            .then_some(Command::CloseNomination),
        Some(DenouncementPhase::Ballot { ballots, .. }) => required
            .iter()
            .all(|id| ballots.contains_key(id))
            .then_some(Command::CloseBallot {
                fallback_replacement: None,
            }),
        Some(DenouncementPhase::Runoff { ballots, .. }) => required
            .iter()
            .all(|id| ballots.contains_key(id))
            .then_some(Command::CloseRunoff {
                fallback_replacement: None,
            }),
        _ => None,
    };
    match command {
        Some(cmd) => apply_command(state, cmd).unwrap_or_default(),
        None => Vec::new(),
    }
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
        let auto_events = auto_push_on_round_advance(&mut state, &events, &mut pushed);
        events.extend(auto_events);
        events.extend(auto_close_denouncement_phase(&mut state));
    }
    // Errors here just mean nobody's subscribed right now -- fine to
    // ignore, there's nobody waiting to be told.
    let _ = server().changed.send(());
    Ok(events)
}

/// Runs rules.md §1's weighted setup raffle over the current roster,
/// finalizes setup, and pushes Round 1's tasks, all under one lock
/// acquisition -- the Host console's "Run the raffle" button is the only
/// caller. This is where the one genuinely random step actually happens
/// (see `engine::raffle`'s module doc comment on "randomness at the
/// boundary": the engine only computes, it never generates its own
/// randomness) -- this function runs natively on the server, so a plain
/// `rand::rng()` is a real OS-backed source, unlike the WASM client half
/// of this same binary.
///
/// Mirrors `bots::HostDriver::setup_game` (ticket-weighted draw,
/// `AssignCharacter` for every winner, ~60/40 Ton/Uprising split for the
/// leftovers, then `CloseRaffle`/`FinalizeSetup`) -- that's the
/// network-driven equivalent of this same sequence for the bot test
/// harness; this is the real one a live host actually presses. Round 1's
/// task push at the end is NOT mirrored there -- `HostDriver` pushes its
/// own separate hardcoded test content instead, via the standalone
/// `run_round_one_tasks`, deliberately independent of this function's
/// real bio-derived random selection.
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
        // Round 1's tasks push automatically right here, the instant setup
        // finalizes -- Dalton's own explicit instruction: no admin action
        // should be needed to get any round's tasks moving, Round 1
        // included, matching how Round 3/5 already auto-push on
        // `AdvanceRound` (see `auto_push_on_round_advance`). An earlier
        // version deliberately deferred this to a separate "Start Round 1"
        // Host button, reasoning Dalton needs to give a live scripted
        // intro before tasks appear -- since removed: that intro now
        // happens before this button is pressed at all, not after.
        let mut pushed = server()
            .auto_tasks_pushed
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        events.extend(push_tasks_for_round(&mut state, Round::One, &mut pushed));
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
