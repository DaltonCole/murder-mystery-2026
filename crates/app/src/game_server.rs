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
    apply_command, contest_submission_count, math_questions, raffle_priority, raffle_winners,
    task_candidates, ticket_count, ticket_slots, trivia_questions, view_for, Command,
    ContestCategory, CreativityKind, CreativityPhase, DenouncementPhase, DomainEvent, Faction,
    GameError, GameState, IntelligenceKind, MinigamePayload, OpenMinigameDetail, PlayerId,
    PlayerStatus, PlayerView, RatingStep, Round, TaskTier, Viewer, WORD_LIST,
};
use rand::seq::{IndexedRandom, SliceRandom};
use rand::RngExt;
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
    // Wall-clock deadlines for the currently-open Creativity mini-game
    // sessions' auto-advancing phase timers -- see `track_minigame_timers`.
    // Process-local, same as every other field here: lost on a restart, a
    // mid-phase session just sits until the Host's manual "Force next"
    // fast-forwards it a few times.
    minigame_deadlines: Mutex<BTreeMap<(Round, ContestCategory), Instant>>,
    // When each currently-open Intelligence session was opened -- lets
    // `record_minigame_elapsed_time` compute a real, server-measured
    // elapsed time for Trivia/Math/Wordle instead of trusting a
    // client-supplied one. See `Command::RecordQuizElapsedTime`'s doc
    // comment.
    minigame_started_at: Mutex<BTreeMap<(Round, ContestCategory), Instant>>,
    // Round 4's "declare your category" window deadline, if one is
    // currently open -- keyed by round so a stale deadline from an
    // earlier round can never be mistaken for the current one. Separate
    // from `minigame_deadlines`: this isn't any one category's own
    // session, it's the room-wide choice phase that precedes all three
    // categories' sequences starting at once. See
    // `category_choice_window_seconds`'s doc comment for the duration.
    category_choice_deadline: Mutex<Option<(Round, Instant)>>,
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
            minigame_deadlines: Mutex::new(BTreeMap::new()),
            minigame_started_at: Mutex::new(BTreeMap::new()),
            category_choice_deadline: Mutex::new(None),
        }
    })
}

/// Bans `prompt` from ever being auto-selected for Round 1/3/5's task push
/// (see `push_tasks_for_round`) -- the Host console's "Ban" button next to
/// a bio-derived candidate. Doesn't affect pushing the same prompt by hand
/// through the free-text "Manual entry" fallback -- banning only scopes the
/// automatic pool, matching the actual feature request ("tasks should be
/// assigned automatically... allow the admin to ban a certain task"), not
/// a blanket block on that exact text ever being used at all.
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
///
/// Also drives `sweep_minigame_deadlines` every tick, unconditionally --
/// deliberately NOT gated by `timer_active` below. The display `GameTimer`
/// (Host-started/stopped) and a Creativity session's own phase deadlines
/// are independent concerns: a session must keep auto-advancing whether or
/// not the Host has separately started the visible round timer. Called
/// unconditionally from `apply` (not just from the Host's timer buttons)
/// so the sweep is guaranteed running before any contest mini-game could
/// possibly be opened.
fn ensure_ticker_running() {
    static TICKER: OnceLock<()> = OnceLock::new();
    TICKER.get_or_init(|| {
        tokio::spawn(async {
            loop {
                tokio::time::sleep(Duration::from_secs(1)).await;
                sweep_minigame_deadlines();
                sweep_category_choice_window();
                let timer_active = server()
                    .timer
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner())
                    .is_some();
                // Also broadcast on every tick a Creativity phase deadline or
                // Round 4's category-choice window is running, even if the
                // sweep didn't just fire an advance -- otherwise a player's
                // progress bar would only visibly move at the moments
                // something actually happens instead of draining smoothly
                // every second.
                let minigame_timer_active = !server()
                    .minigame_deadlines
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner())
                    .is_empty();
                let choice_window_active = server()
                    .category_choice_deadline
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner())
                    .is_some();
                if timer_active || minigame_timer_active || choice_window_active {
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
pub fn timer_remaining_secs() -> Option<crate::TimerState> {
    server()
        .timer
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .as_ref()
        .map(|t| crate::TimerState {
            remaining_secs: t.duration.as_secs() as i64 - t.started_at.elapsed().as_secs() as i64,
            total_secs: t.duration.as_secs() as u32,
        })
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

/// One candidate task the automatic per-round draw (`push_tasks_for_round`)
/// can pick from -- either a bio-derived "talk to 3, credit on 1 match"
/// prompt (`expected_code: None`) or a pre-authored `LOCATION_TASKS` entry
/// (`qualifying_players` empty, `expected_code: Some`). Unifying the two
/// into one pool is what lets the draw shuffle them together per tier --
/// see `push_tasks_for_round`'s doc comment on why location tasks were
/// folded in here rather than kept as their own separate Host-picked pool.
struct AutoTaskCandidate {
    prompt: String,
    qualifying_players: BTreeSet<PlayerId>,
    expected_code: Option<String>,
}

/// Pushes `round`'s configured tasks (see `auto_task_counts`), unless
/// they've already been pushed (`pushed`, this process's own idempotency
/// guard -- see `GameServer::auto_tasks_pushed`'s doc comment). Shared by
/// every way a round's tasks get pushed: automatically for Round 3/5 (see
/// `auto_push_on_round_advance` below) and automatically for Round 1, the
/// moment setup finalizes (see `run_raffle`).
///
/// For each tier, pools together every bio-derived `task_candidates`
/// prompt with every `LOCATION_TASKS` entry of that same tier (Dalton's
/// own explicit instruction: the Host shouldn't hand-pick location tasks
/// either -- see the Host console's removed "push" picker), excludes any
/// banned prompt (`banned_task_prompts` -- "all tasks should be available
/// by default," so this is normally a no-op filter), then shuffles the
/// combined pool with real, OS-backed entropy (the same "randomness at the
/// boundary" shape as `run_raffle`/`draw_intermission_entrants`) and
/// pushes `auto_task_counts`'s configured count from the front -- capped
/// at however many distinct candidates actually exist, so a too-small pool
/// (a tiny playtest game, or a tier nobody's bio happens to fill, or every
/// candidate in a tier happening to be banned) just pushes fewer tasks
/// rather than erroring.
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
        let mut candidates: Vec<AutoTaskCandidate> = task_candidates(state, tier)
            .into_iter()
            .map(|c| AutoTaskCandidate {
                prompt: c.prompt,
                qualifying_players: c.qualifying_players.into_iter().collect(),
                expected_code: None,
            })
            .chain(
                LOCATION_TASKS
                    .iter()
                    .filter(|&&(location_tier, ..)| location_tier == tier)
                    .map(|&(_, prompt, code)| AutoTaskCandidate {
                        prompt: prompt.to_string(),
                        qualifying_players: BTreeSet::new(),
                        expected_code: Some(code.to_string()),
                    }),
            )
            .filter(|c| !banned.contains(&c.prompt))
            .collect();
        candidates.shuffle(&mut rng);
        for candidate in candidates.into_iter().take(count) {
            if let Ok(events) = apply_command(
                state,
                Command::PushTask {
                    prompt: candidate.prompt,
                    tier,
                    qualifying_players: candidate.qualifying_players,
                    expected_code: candidate.expected_code,
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
/// which bio-derived or location candidate to push -- a
/// reliability/automation review found this was one of the last remaining
/// points of necessary Host involvement in an otherwise-automated round
/// flow, and Dalton's own later instruction removed the Host's push
/// pickers entirely (see `push_tasks_for_round`'s doc comment). The
/// free-text "Manual entry" fallback stays in the Host console regardless,
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

/// Flips the Bartender's coin server-side (Dalton's own explicit
/// instruction: for any character involving randomness, the engine decides
/// it, never the player) and applies the real result -- the same
/// "randomness at the boundary" shape as `transfer_king_queen_randomly`
/// above. `Command::BartenderTarget`'s own `lands` field is unchanged; this
/// is just the one caller that supplies a real coin flip instead of a
/// player-reported one.
pub fn bartender_target_randomly(
    player: PlayerId,
    target: PlayerId,
) -> Result<Vec<DomainEvent>, String> {
    let events;
    {
        let mut state = lock_state();
        let lands = rand::rng().random_bool(0.5);
        events = apply_command(
            &mut state,
            Command::BartenderTarget {
                player,
                target,
                lands,
            },
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
/// `PushTask` calls (the Host console's free-text "Manual entry" fallback
/// is built specifically for pushing one at a time, for a live-event
/// fix-up after the automatic per-round draw) -- a fast group finishing
/// the *first* pushed task auto-closed the whole phase
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

/// Closes every currently-open task (via `Command::CloseTasks`) the
/// instant a Denouncement opens, or the instant the round advances into
/// Round 2 or Round 4 -- Dalton's own explicit instructions: "once
/// denouncement has started, remove the tasks from the player screen and
/// close the tasks. Any incomplete tasks are considered failed tasks," and
/// later, "for even rounds, players should not have the option to
/// complete odd round tasks." The Denouncement-open trigger alone left a
/// gap: Round 1 has no Denouncement at all, so a task still open when the
/// game advances straight from Round 1 into Round 2 would otherwise linger
/// on every player's screen throughout the whole contest round.
/// `CloseTasks` already makes closed tasks disappear from every player's
/// `open_tasks` (see `engine::view`'s own tests), and a never-attempted
/// task was already treated identically to an explicitly-failed one
/// everywhere that matters (e.g. `ton_met_task_threshold`'s Leader's
/// Confidants trigger) -- so this is purely automation, not a new engine
/// rule. `CloseTasks` is already a safe no-op when nothing is open (see
/// its own doc comment), so this never needs to check first.
///
/// Triggered by scanning `events` (whatever command was just applied) for
/// `DomainEvent::DenouncementOpened` or a `RoundAdvanced` into Round 2/4 --
/// same shape as `auto_push_on_round_advance`.
fn auto_close_tasks_on_denouncement_open_or_even_round(
    state: &mut GameState,
    events: &[DomainEvent],
) -> Vec<DomainEvent> {
    let should_close = events.iter().any(|e| {
        matches!(e, DomainEvent::DenouncementOpened)
            || matches!(
                e,
                DomainEvent::RoundAdvanced {
                    round: Round::Two | Round::Four
                }
            )
    });
    if !should_close {
        return Vec::new();
    }
    apply_command(state, Command::CloseTasks).unwrap_or_default()
}

/// Every player currently expected to submit to `(round, category)`'s
/// open session. Round 4: every active player whose stored choice for
/// this round equals `category` -- `Command::ChooseContestCategory`'s own
/// enforcement guarantees nobody *else* could have submitted to it
/// anyway. Everything else (Round 2, or an ad-hoc Host-opened session
/// outside either automated flow): every active player, unchanged from
/// today's "no participation gating" model.
fn expected_contest_participants(
    state: &GameState,
    round: Round,
    category: ContestCategory,
) -> BTreeSet<PlayerId> {
    state
        .players()
        .filter(|p| p.status == PlayerStatus::Active)
        .filter(|p| {
            round != Round::Four || state.contest_category_choice(round, p.id) == Some(category)
        })
        .map(|p| p.id)
        .collect()
}

/// Auto-closes any currently-open Intelligence(Trivia/Math/Wordle/Memory)
/// or Strength session the instant every expected participant has
/// finished -- an automation review found these always needed a Host's
/// manual "Close & resolve" click, unlike Nomination/Ballot/Runoff, which
/// already auto-close the same way (`auto_close_denouncement_phase`).
/// Never Creativity, which already has its own real-timer auto-advance.
/// This is what actually drives Round 2/4's non-Creativity steps forward
/// with no Host click (and, as a bonus, speeds up any ad-hoc Host-opened
/// session the same way).
///
/// Never fires on an empty required set -- same guard
/// `auto_close_denouncement_phase` already documents, so a Round 4
/// category literally nobody chose just sits open for the Host's manual
/// fallback rather than auto-closing vacuously. Checked unconditionally
/// on every `apply` call (not scoped to a specific triggering event, the
/// way most other `auto_*` functions here are) since a session can become
/// fully-submitted as a side effect of several different submit commands.
fn auto_close_contest_session_on_full_participation(state: &mut GameState) -> Vec<DomainEvent> {
    let to_close: Vec<(Round, ContestCategory)> = state
        .contest_minigames()
        .filter(|(_, session)| !matches!(session.payload, MinigamePayload::Creativity(_)))
        .filter_map(|(&(round, category), session)| {
            let expected = expected_contest_participants(state, round, category);
            if expected.is_empty() {
                return None;
            }
            (contest_submission_count(session) >= expected.len()).then_some((round, category))
        })
        .collect();

    let mut events = Vec::new();
    for (round, category) in to_close {
        if let Ok(close_events) =
            apply_command(state, Command::CloseContestMinigame { round, category })
        {
            events.extend(close_events);
        }
    }
    events
}

/// The category that follows `category` in Round 2's fixed
/// Creativity -> Intelligence -> Strength order (Dalton's own explicit
/// spec), or `None` once Strength -- the last step -- closes.
fn round_two_next_category(category: ContestCategory) -> Option<ContestCategory> {
    match category {
        ContestCategory::Creativity => Some(ContestCategory::Intelligence),
        ContestCategory::Intelligence => Some(ContestCategory::Strength),
        ContestCategory::Strength => None,
    }
}

fn start_round_two_category(state: &mut GameState, category: ContestCategory) -> Vec<DomainEvent> {
    let (prompt, detail) = round_two_step(category);
    apply_command(
        state,
        Command::StartContestSequence {
            round: Round::Two,
            steps: vec![(prompt, detail)],
        },
    )
    .unwrap_or_default()
}

/// Round 2's fully-automated sequence (Dalton's own explicit "this should
/// be automated" instruction): Drawing, then Trivia, then a push-up
/// contest, strictly one after another with no Host click. Kicked off the
/// instant the round is reached (`RoundAdvanced{Two}`, the same "no admin
/// action needed" precedent Round 3/5's task auto-push already
/// established), then chained forward by each category's own
/// `ContestMinigameClosed` -- same "scan just-applied events" shape as
/// `auto_push_on_round_advance`.
fn auto_advance_round_two_contests(
    state: &mut GameState,
    events: &[DomainEvent],
) -> Vec<DomainEvent> {
    let mut new_events = Vec::new();
    for event in events {
        match *event {
            DomainEvent::RoundAdvanced { round: Round::Two } => {
                new_events.extend(start_round_two_category(state, ContestCategory::Creativity));
            }
            DomainEvent::ContestMinigameClosed {
                round: Round::Two,
                category,
            } => {
                if let Some(next) = round_two_next_category(category) {
                    new_events.extend(start_round_two_category(state, next));
                }
            }
            _ => {}
        }
    }
    new_events
}

/// How long Round 4's category-choice window stays open before all three
/// tracks auto-start regardless of who's chosen -- Dalton's own
/// "everything should be automated" instruction, the same real-timer
/// treatment Creativity's own phases already get.
///
/// *** EDIT THIS to retune pacing before game night -- no other code
/// changes needed. ***
const CATEGORY_CHOICE_WINDOW_SECS: u64 = 45;

/// Opens Round 4's category-choice window -- a real, fixed-duration timer,
/// not a second manual Host click, per Dalton's own "everything should be
/// automated" instruction. Separate from `minigame_deadlines`: this is a
/// room-wide phase that precedes all three categories' sequences starting
/// at once, not any one category's own session.
fn open_category_choice_window(round: Round) {
    *server()
        .category_choice_deadline
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some((
        round,
        Instant::now() + Duration::from_secs(CATEGORY_CHOICE_WINDOW_SECS),
    ));
}

fn auto_open_round_four_choice_window(events: &[DomainEvent]) {
    if events
        .iter()
        .any(|e| matches!(e, DomainEvent::RoundAdvanced { round: Round::Four }))
    {
        open_category_choice_window(Round::Four);
    }
}

/// Locks in Round 4's category choices and starts all three categories'
/// 3-game sequences simultaneously ("3 simultaneous zones," this
/// project's own established framing for Round 4) -- called once the
/// choice window's deadline passes (`sweep_category_choice_window`) or
/// the Host force-closes it early (`force_close_category_choice_window`).
/// Each category's sequence then runs itself to completion independently
/// via `close_contest_minigame`'s own sequence-awareness -- no further
/// cross-category orchestration needed, unlike Round 2.
fn begin_round_four_contests(round: Round) {
    *server()
        .category_choice_deadline
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner()) = None;
    for category in [
        ContestCategory::Creativity,
        ContestCategory::Intelligence,
        ContestCategory::Strength,
    ] {
        let steps = round_four_steps(category);
        let _ = apply(Command::StartContestSequence { round, steps });
    }
}

/// Checks Round 4's category-choice window and, once its deadline has
/// passed, locks in choices and starts all three tracks -- called from
/// the same once-a-second ticker as `sweep_minigame_deadlines`.
fn sweep_category_choice_window() {
    let due = server()
        .category_choice_deadline
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .filter(|&(_, deadline)| Instant::now() >= deadline)
        .map(|(round, _)| round);
    if let Some(round) = due {
        begin_round_four_contests(round);
    }
}

/// The Host console's "force-close the choice window now" override --
/// mirrors `force_advance_creative_writing`'s own "always keep a manual
/// escape hatch" reasoning.
pub fn force_close_category_choice_window() -> Result<(), String> {
    let round = server()
        .category_choice_deadline
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .map(|(round, _)| round);
    let Some(round) = round else {
        return Err("no category-choice window is currently open".to_string());
    };
    begin_round_four_contests(round);
    Ok(())
}

/// Per-`CreativityKind` `(writing, rating-per-item)` durations -- Dalton's
/// own explicit spec for each of the four games. Lives here, not the
/// engine: these are wall-clock facts, not game rules -- the same
/// "randomness/time is an app-layer concern" boundary this module already
/// draws for `GameTimer` and every `_randomly` helper above.
///
/// *** EDIT THIS to retune pacing before game night -- no other code
/// changes needed. ***
fn creativity_durations(kind: CreativityKind) -> (Duration, Duration) {
    match kind {
        CreativityKind::Drawing => (Duration::from_secs(60), Duration::from_secs(10)),
        CreativityKind::Joke => (Duration::from_secs(120), Duration::from_secs(15)),
        CreativityKind::Dictionarium => (Duration::from_secs(180), Duration::from_secs(20)),
        CreativityKind::Smut => (Duration::from_secs(150), Duration::from_secs(30)),
    }
}

/// The currently-open Creativity session's kind and phase for `round`, if
/// any -- `game_server`'s own read of `GameState::contest_minigames`
/// (`pub` specifically for this), used to decide which duration to arm
/// next and which command the deadline sweep should fire.
fn creativity_kind_and_phase(
    state: &GameState,
    round: Round,
) -> Option<(CreativityKind, CreativityPhase)> {
    state
        .contest_minigames()
        .find_map(|(&(r, category), session)| {
            if r != round || category != ContestCategory::Creativity {
                return None;
            }
            match &session.payload {
                MinigamePayload::Creativity(payload) => Some((payload.kind, payload.phase)),
                _ => None,
            }
        })
}

fn set_minigame_deadline(round: Round, category: ContestCategory, duration: Duration) {
    server()
        .minigame_deadlines
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .insert((round, category), Instant::now() + duration);
}

/// Clears both this module's own deadline and started-at bookkeeping for
/// `(round, category)` -- called once a session closes, so a stale entry
/// never lingers to confuse a later session opened under the same key.
fn clear_minigame_timer(round: Round, category: ContestCategory) {
    server()
        .minigame_deadlines
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .remove(&(round, category));
    server()
        .minigame_started_at
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .remove(&(round, category));
}

fn record_minigame_started(round: Round, category: ContestCategory) {
    server()
        .minigame_started_at
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .insert((round, category), Instant::now());
}

fn minigame_elapsed_ms(round: Round, category: ContestCategory) -> Option<u64> {
    server()
        .minigame_started_at
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .get(&(round, category))
        .map(|start| start.elapsed().as_millis() as u64)
}

/// Every currently-armed Creativity phase deadline, as a countdown the
/// client can render a progress bar from -- `ServerMsg::MinigameTimers`'s
/// data source. The engine itself can't supply this (`ContestMinigameView`
/// has no notion of wall-clock time at all, by design), so this reads
/// `minigame_deadlines` directly and re-derives each entry's total
/// duration from its session's current kind/phase, the same lookup
/// `track_minigame_timers` uses to arm the deadline in the first place.
/// `saturating_duration_since` (never negative) rather than signed
/// subtraction: a deadline that's already passed just reads as "0
/// remaining" for the brief instant before the next sweep tick actually
/// advances the phase, rather than a confusing negative countdown (unlike
/// the Host's own `GameTimer`, a Creativity deadline always resolves
/// itself, so there's no real "overtime" state to show here).
pub fn minigame_timer_states() -> Vec<crate::MinigameTimer> {
    let now = Instant::now();
    let deadlines: Vec<((Round, ContestCategory), Instant)> = server()
        .minigame_deadlines
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .iter()
        .map(|(&key, &deadline)| (key, deadline))
        .collect();
    let state = lock_state();
    deadlines
        .into_iter()
        .filter_map(|((round, category), deadline)| {
            let (kind, phase) = creativity_kind_and_phase(&state, round)?;
            let total_secs = match phase {
                CreativityPhase::Writing => creativity_durations(kind).0,
                CreativityPhase::Rating { .. } => creativity_durations(kind).1,
            }
            .as_secs() as u32;
            Some(crate::MinigameTimer {
                round,
                category,
                state: crate::TimerState {
                    remaining_secs: deadline.saturating_duration_since(now).as_secs() as i64,
                    total_secs,
                },
            })
        })
        .collect()
}

/// Round 4's category-choice window countdown, if one is currently open
/// -- `ServerMsg::CategoryChoiceTimer`'s data source, the same shape as
/// `minigame_timer_states` but for the one room-wide window rather than a
/// per-category deadline.
pub fn category_choice_timer_state() -> Option<(Round, crate::TimerState)> {
    let (round, deadline) = server()
        .category_choice_deadline
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .as_ref()
        .copied()?;
    Some((
        round,
        crate::TimerState {
            remaining_secs: deadline.saturating_duration_since(Instant::now()).as_secs() as i64,
            total_secs: CATEGORY_CHOICE_WINDOW_SECS as u32,
        },
    ))
}

/// Arms/clears this module's own wall-clock bookkeeping in response to
/// whatever `events` a just-applied command produced -- same "scan the
/// just-applied events" shape as `auto_push_on_round_advance`, but writes
/// to `minigame_deadlines`/`minigame_started_at` instead of firing a
/// follow-up command. Opening a Creativity session arms its Writing
/// deadline; ending Writing or advancing Rating re-arms the per-item
/// rating deadline; opening an Intelligence session records its start time
/// (used by `record_minigame_elapsed_time` for Trivia/Math/Wordle); a
/// session closing clears both maps for that key.
fn track_minigame_timers(state: &GameState, events: &[DomainEvent]) {
    for event in events {
        match *event {
            DomainEvent::ContestMinigameOpened {
                round, category, ..
            }
            // A sequence's own step-to-step transition (`close_contest_minigame`'s
            // internal "more steps remain" branch, see `contest_minigame::
            // ContestSequence`'s doc comment) opens a fresh session exactly
            // like `OpenContestMinigame` does, just under a different event
            // -- same arming logic applies.
            | DomainEvent::ContestSequenceStepAdvanced {
                round, category, ..
            } => match category {
                ContestCategory::Creativity => {
                    if let Some((kind, _)) = creativity_kind_and_phase(state, round) {
                        set_minigame_deadline(round, category, creativity_durations(kind).0);
                    }
                }
                ContestCategory::Intelligence => record_minigame_started(round, category),
                ContestCategory::Strength => {}
            },
            DomainEvent::CreativeWritingPhaseEnded { round }
            | DomainEvent::CreativeRatingAdvanced { round } => {
                if let Some((kind, _)) = creativity_kind_and_phase(state, round) {
                    set_minigame_deadline(
                        round,
                        ContestCategory::Creativity,
                        creativity_durations(kind).1,
                    );
                }
            }
            DomainEvent::ContestMinigameClosed { round, category } => {
                clear_minigame_timer(round, category);
            }
            _ => {}
        }
    }
}

/// Catches `QuizCompleted`/`WordleAttemptFinished` (see
/// `Command::RecordQuizElapsedTime`'s doc comment) and immediately records
/// a real, server-measured elapsed time -- never a client-supplied one.
/// Same "scan just-applied events, fire a follow-up command" shape as
/// `auto_push_on_round_advance`.
fn record_minigame_elapsed_time(state: &mut GameState, events: &[DomainEvent]) -> Vec<DomainEvent> {
    let mut follow_up = Vec::new();
    for event in events {
        let command = match *event {
            DomainEvent::QuizCompleted { player, round } => {
                let Some(elapsed_ms) = minigame_elapsed_ms(round, ContestCategory::Intelligence)
                else {
                    continue;
                };
                Command::RecordQuizElapsedTime {
                    player,
                    round,
                    elapsed_ms,
                }
            }
            DomainEvent::WordleAttemptFinished { player, round } => {
                let Some(elapsed_ms) = minigame_elapsed_ms(round, ContestCategory::Intelligence)
                else {
                    continue;
                };
                Command::RecordWordleElapsedTime {
                    player,
                    round,
                    elapsed_ms,
                }
            }
            _ => continue,
        };
        if let Ok(more) = apply_command(state, command) {
            follow_up.extend(more);
        }
    }
    follow_up
}

/// Every currently-active player, server-shuffled -- the real, OS-backed
/// "randomness at the boundary" source for `Command::AdvanceCreativeWriting`'s
/// `order` field, both from the automatic sweep below and from the Host's
/// manual "Force next" override during Writing (`force_advance_creative_writing`).
fn active_players_shuffled(state: &GameState) -> Vec<PlayerId> {
    let mut ids: Vec<PlayerId> = state
        .players()
        .filter(|p| p.status == PlayerStatus::Active)
        .map(|p| p.id)
        .collect();
    ids.shuffle(&mut rand::rng());
    ids
}

/// Checks every open Creativity session's deadline and, for any that has
/// passed, fires the matching advance command through `apply` itself --
/// letting `track_minigame_timers` (run again from inside that `apply`
/// call) re-derive and store the *next* deadline, exactly as a Host's
/// manual override would. Only Creativity sessions ever get a deadline
/// (see `track_minigame_timers`), so nothing else needs filtering here.
fn sweep_minigame_deadlines() {
    let due: Vec<(Round, ContestCategory)> = {
        let now = Instant::now();
        server()
            .minigame_deadlines
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .iter()
            .filter(|&(_, &deadline)| now >= deadline)
            .map(|(&key, _)| key)
            .collect()
    };
    for (round, category) in due {
        let phase = creativity_kind_and_phase(&lock_state(), round).map(|(_, phase)| phase);
        let Some(phase) = phase else {
            // The session closed or changed shape some other way since the
            // deadline was armed (e.g. a Host manual override raced this
            // sweep) -- drop the stale entry instead of looping forever.
            clear_minigame_timer(round, category);
            continue;
        };
        let cmd = match phase {
            CreativityPhase::Writing => Command::AdvanceCreativeWriting {
                round,
                order: active_players_shuffled(&lock_state()),
            },
            CreativityPhase::Rating { .. } => Command::AdvanceCreativeRating {
                round,
                direction: RatingStep::Forward,
            },
        };
        let _ = apply(cmd);
    }
}

/// Forces a Creativity session's Writing phase to end right now, ahead of
/// its own auto-timer -- the Host console's "Force next" button while a
/// session is still in Writing. Not a plain `Command::AdvanceCreativeWriting`
/// sent directly from the client: that command's `order` field needs a
/// real, server-shuffled active-participant list (randomness at the
/// boundary), which the Host browser has no principled way to produce
/// itself. Going back to Writing is not offered -- as with every other
/// direction here, only Rating has a Backward step.
pub fn force_advance_creative_writing(round: Round) -> Result<Vec<DomainEvent>, String> {
    let order = active_players_shuffled(&lock_state());
    apply(Command::AdvanceCreativeWriting { round, order }).map_err(|e| e.to_string())
}

/// Applies one command against the single canonical `GameState`, holding
/// the lock only for the mutation itself. The only place in the whole app
/// allowed to call `engine::apply_command` -- every route-driven mutation
/// funnels through here.
pub fn apply(cmd: Command) -> Result<Vec<DomainEvent>, GameError> {
    ensure_ticker_running();
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
        events.extend(auto_close_tasks_on_denouncement_open_or_even_round(
            &mut state, &events,
        ));
        events.extend(auto_close_denouncement_phase(&mut state));
        events.extend(auto_close_contest_session_on_full_participation(&mut state));
        events.extend(auto_advance_round_two_contests(&mut state, &events));
        auto_open_round_four_choice_window(&events);
        track_minigame_timers(&state, &events);
        let elapsed_time_events = record_minigame_elapsed_time(&mut state, &events);
        events.extend(elapsed_time_events);
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
/// `String` error (not `GameError`, matching every other app-layer
/// randomness-at-the-boundary function's precedent) since the one way this
/// can fail -- a double-click re-running
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
        // See `Command::SetPlayerPriorityOrder`'s doc comment: a real,
        // app-shuffled permutation of the whole roster, set exactly once
        // right here, that the engine consults for every "pick a random
        // remaining player" need for the rest of the game (King/Queen
        // replacement on Convert/Cast-Out, Revolutionary Leader succession,
        // the Leader's Confidants) instead of a deterministic stand-in.
        let mut priority_order = roster.clone();
        priority_order.shuffle(&mut rng);
        events.extend(
            apply_command(
                &mut state,
                Command::SetPlayerPriorityOrder {
                    order: priority_order,
                },
            )
            .map_err(|e| e.to_string())?,
        );
    }
    // Same reasoning as `apply` above: nobody subscribed is a fine outcome
    // to ignore, there's just nobody waiting to be told.
    let _ = server().changed.send(());
    Ok(events)
}

/// Pushes Round 1's tasks (rules.md §4) -- the Host console's own "Start
/// Round 1" button. Dalton's own explicit instruction reversing an earlier
/// choice: Round 1 should wait for an explicit Host click, exactly like
/// every other round's task phase waits for `AdvanceRound` (see
/// `auto_push_on_round_advance`), rather than firing automatically the
/// instant setup finalizes -- a live host needs room to give a scripted
/// intro before tasks appear. `push_tasks_for_round`'s own idempotency
/// guard (`auto_tasks_pushed`) makes a double-click here harmless.
pub fn start_round_one() -> Result<Vec<DomainEvent>, String> {
    let events;
    {
        let mut state = lock_state();
        let mut pushed = server()
            .auto_tasks_pushed
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        events = push_tasks_for_round(&mut state, Round::One, &mut pushed);
    }
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
/// learns the safe subset (tier + prompt, via `location_task_templates`,
/// for reference only -- see its own doc comment); the code itself never
/// leaves this server process. `push_tasks_for_round` is the only thing
/// that ever actually turns one of these into a real open task.
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

/// The safe subset of `LOCATION_TASKS` for the Host console's read-only
/// reference list -- tier and prompt, never the code (see
/// `location_task_templates`'s doc comment on why the code itself never
/// reaches this list). Dalton's own explicit instruction removed the
/// Host's old per-template "push" button entirely -- location tasks now
/// only ever enter play through the automatic per-round draw (see
/// `push_tasks_for_round`), same as bio-derived ones.
pub fn location_task_templates() -> Vec<(usize, TaskTier, String)> {
    LOCATION_TASKS
        .iter()
        .enumerate()
        .map(|(i, &(tier, prompt, _code))| (i, tier, prompt.to_string()))
        .collect()
}

/// Bridgerton-themed drawing prompts for the Creativity round's Drawing
/// game -- Dalton's own explicit instruction: "take heavy inspiration from
/// [game-changer's `DRAWING_PROMPTS`] but keep the prompts more bridgerton
/// themed." Same short "a X doing Y" shape as that list. Drawn randomly by
/// `open_drawing_session` (real, OS-backed randomness, the same
/// "randomness at the boundary" shape as `LOCATION_TASKS`'s own draw), not
/// Host-typed -- consistent with this project's own recent shift away from
/// Host-picked content (bio/location tasks are auto-drawn too).
///
/// *** EDIT THIS before game night *** to retheme/expand -- these are a
/// first-draft bank, not a rules.md quote.
const DRAWING_PROMPTS: &[&str] = &[
    "a duke tripping over his own cravat",
    "a debutante fainting into the punch bowl",
    "a chaperone falling asleep at the ball",
    "a carriage stuck in the mud on the way to Almack's",
    "a viscount losing a duel to a crumpet",
    "a lady scandalized by a shockingly modern waltz",
    "a footman spilling tea on a duchess",
    "a marquess proposing to the wrong sister",
    "a gossip columnist spying through a hedge",
    "a rake charming his way out of trouble",
    "a bonnet blown clean off in the wind",
    "an earl fencing in his nightshirt",
    "a lady's fan hiding a mischievous grin",
    "a wallflower secretly winning at cards",
    "a suitor serenading the wrong window",
    "a corset laced far too tight",
    "a duel at dawn interrupted by a goose",
    "a lady riding sidesaddle at full gallop",
    "a butler eavesdropping at the parlor door",
    "a matchmaking mama plotting in the corner",
    "a prince tangled in his own cape",
    "a garden party ruined by an escaped pig",
    "a love letter delivered to the wrong house",
    "a masquerade mask that won't come off",
    "a scandal sheet blowing through the streets of Mayfair",
    "a debutante curtsying a little too low",
    "anything Bridgerton-themed you like!",
    "anything Bridgerton-themed you like!",
];

fn random_drawing_prompt() -> String {
    DRAWING_PROMPTS
        .choose(&mut rand::rng())
        .copied()
        .unwrap_or("anything Bridgerton-themed you like!")
        .to_string()
}

/// Opens a Drawing Creativity session for `round` with a randomly-drawn
/// prompt from `DRAWING_PROMPTS` -- the Host console's "Open" button for
/// Drawing specifically. Not a plain `Command::OpenContestMinigame` sent
/// directly from the client: the prompt draw needs a real RNG, which only
/// exists server-side (same "randomness at the boundary" shape as
/// `run_raffle`).
pub fn open_drawing_session(round: Round) -> Result<Vec<DomainEvent>, String> {
    apply(Command::OpenContestMinigame {
        round,
        prompt: random_drawing_prompt(),
        detail: OpenMinigameDetail::Creativity(CreativityKind::Drawing),
    })
    .map_err(|e| e.to_string())
}

/// A curated bank of physically-judged Strength challenges -- Dalton's own
/// explicit instruction: "take inspiration from [game-changer's physical
/// games]... include additional, more physically demanding games, like
/// push-ups." Unlike game-changer's own mutual-agreement pair games, this
/// project's Strength mechanic is a single live challenge with a
/// self-reported finishing *placement* (see `SubmitPhysicalPlacement`), so
/// these are phrased as one shared, simultaneous contest rather than a
/// paired win/lose game -- the inspiration is the playful, low-prop party
/// energy of Ninja/Item Hunt/Carrot, not their exact mechanic. Same
/// `LOCATION_TASKS`-style shape and random draw as `DRAWING_PROMPTS` above.
///
/// *** EDIT THIS before game night *** against your actual venue/space --
/// these are a first-draft bank, not a rules.md quote.
const PHYSICAL_CHALLENGES: &[&str] = &[
    "Push-ups: as many as you can manage in 60 seconds. Most reps wins.",
    "Plank hold: last one still holding wins.",
    "Wall sit: last one still sitting wins.",
    "Sock-footed sprint down the hall and back -- fastest wins.",
    "Balance on one foot with your eyes closed -- last one standing wins.",
    "Stack 10 cups into a pyramid and back down -- fastest wins.",
    "Carry an egg across the room on a spoon without dropping it -- fastest wins.",
    "Burpees: as many as you can manage in 60 seconds. Most reps wins.",
    "Hop on one foot across the room and back -- fastest wins.",
    "Keep a balloon off the floor using only one hand -- longest streak wins.",
    "Sit-ups: as many as you can manage in 60 seconds. Most reps wins.",
    "Bridgerton-ballroom musical chairs -- last one seated wins.",
    "Wheelbarrow race the length of the room with a partner -- fastest pair wins.",
];

/// Opens a Strength session for `round` with a randomly-drawn challenge
/// description from `PHYSICAL_CHALLENGES` -- see `open_drawing_session`'s
/// doc comment for why this needs to be server-side, not a plain
/// `Command::OpenContestMinigame`.
pub fn open_physical_session(round: Round) -> Result<Vec<DomainEvent>, String> {
    let prompt = PHYSICAL_CHALLENGES
        .choose(&mut rand::rng())
        .copied()
        .unwrap_or("Host's choice -- judge live and record placements as they finish.")
        .to_string();
    apply(Command::OpenContestMinigame {
        round,
        prompt,
        detail: OpenMinigameDetail::Strength,
    })
    .map_err(|e| e.to_string())
}

/// Round 2's fixed Creativity/Intelligence/Strength step -- Dalton's own
/// explicit spec: "First... drawing... second... trivia... finally... a
/// pushup contest." Drawing draws its prompt the same random way the
/// Host's own manual "Open Drawing" button does; Trivia uses the real
/// 10-question bank; Strength is fixed to `PHYSICAL_CHALLENGES[0]`
/// specifically (the exact push-up prompt Dalton names), not a random
/// draw -- unlike Round 4's Strength track below.
fn round_two_step(category: ContestCategory) -> (String, OpenMinigameDetail) {
    match category {
        ContestCategory::Creativity => (
            random_drawing_prompt(),
            OpenMinigameDetail::Creativity(CreativityKind::Drawing),
        ),
        ContestCategory::Intelligence => (
            "Trivia!".to_string(),
            OpenMinigameDetail::Intelligence(IntelligenceKind::Trivia {
                questions: trivia_questions(),
            }),
        ),
        ContestCategory::Strength => (
            PHYSICAL_CHALLENGES[0].to_string(),
            OpenMinigameDetail::Strength,
        ),
    }
}

fn random_wordle_secret() -> String {
    WORD_LIST
        .choose(&mut rand::rng())
        .copied()
        .unwrap_or("HOUSE")
        .to_string()
}

/// Round 4's three-game track for `category` -- whichever games Round 2
/// didn't already use for it. Creativity/Intelligence's remaining kinds
/// are fixed (all 4 kinds are named outright, nothing to draw from a
/// bank); Strength draws 3 *distinct* random challenges excluding
/// `PHYSICAL_CHALLENGES[0]` (Round 2's exact push-up prompt), so it's
/// never an immediate repeat.
fn round_four_steps(category: ContestCategory) -> Vec<(String, OpenMinigameDetail)> {
    match category {
        ContestCategory::Creativity => [
            (CreativityKind::Joke, "Tell a Bridgerton-themed joke!"),
            (
                CreativityKind::Dictionarium,
                "Invent a Bridgerton-themed word!",
            ),
            (CreativityKind::Smut, "Write a short smut scene!"),
        ]
        .into_iter()
        .map(|(kind, prompt)| (prompt.to_string(), OpenMinigameDetail::Creativity(kind)))
        .collect(),
        ContestCategory::Intelligence => vec![
            (
                "Math!".to_string(),
                OpenMinigameDetail::Intelligence(IntelligenceKind::Math {
                    questions: math_questions(),
                }),
            ),
            (
                "Memory!".to_string(),
                OpenMinigameDetail::Intelligence(IntelligenceKind::Memory),
            ),
            (
                "Wordle!".to_string(),
                OpenMinigameDetail::Intelligence(IntelligenceKind::Wordle {
                    secret: random_wordle_secret(),
                }),
            ),
        ],
        ContestCategory::Strength => PHYSICAL_CHALLENGES[1..]
            .sample(&mut rand::rng(), 3)
            .into_iter()
            .map(|&prompt| (prompt.to_string(), OpenMinigameDetail::Strength))
            .collect(),
    }
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
