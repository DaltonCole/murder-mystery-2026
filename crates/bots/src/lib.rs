//! Bot players that drive a real, running Murder Mystery 2026 server over
//! its actual websocket protocol -- not the `engine` crate directly
//! (that's `sim`'s job, headless and networking-free). This is the
//! network-level equivalent, modeled directly on how
//! `/home/drc/game-changer` does it:
//! - `examples/simulate_party.rs` there is an interactive dev tool that
//!   fills an already-running server with N bots while a real human joins
//!   alongside them from their own phone -- `src/main.rs` here is the
//!   same idea (see the `bots` Makefile target).
//! - `tests/api_integration.rs`'s `TestServer` + its
//!   `twelve_participants_play_a_random_simulation_of_the_whole_game`/
//!   `two_dozen_participants_play_concurrently_...` tests spawn the real
//!   compiled server binary as a child process and drive it with bots,
//!   asserting the whole thing stays healthy -- `tests/full_game_with_bots.rs`
//!   in this crate is the same idea, at up to 30 players.
//!
//! Division of labor: `PlayerBot` (`player_bot.rs`) is a simulated guest --
//! join, then react to whatever the current phase asks of it. `HostDriver`
//! (`host.rs`) is *not* a bot; it's the authoritative game-runner issuing
//! direct setup/round/Denouncement commands, exactly the role a real human
//! host plays and exactly why game-changer's own admin actions in
//! `TestServer` aren't modeled as a bot thread either.

pub mod host;
pub mod player_bot;
pub mod protocol;

pub use host::{HostDriver, Roles};
pub use player_bot::PlayerBot;
pub use protocol::{ClientMsg, Conn, ConnError, ServerMsg};

use engine::{Faction, PlayerId, PlayerStatus, RosterEntry, Round};
use std::collections::BTreeSet;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

pub struct GameSummary {
    pub final_round: Round,
    pub final_roster: Vec<RosterEntry>,
    pub cast_out_count: usize,
    /// The one faction `win_condition::evaluate` found winning once the
    /// Finale's Denouncement closed -- `None` covers both "the Host's view
    /// hadn't caught up yet" and the rare legitimate case rules.md's own
    /// conditions allow where nobody's condition was actually met (see
    /// `HostDriver::resolve_gallery_predictions`'s doc comment).
    pub winner: Option<Faction>,
}

/// Runs one entire game end to end against `url` (a real, already-running
/// server) with `player_count` bots total: joins them, runs the setup
/// raffle, Round 1's tasks, then Denouncements at Round 3, Round 5, and
/// the Finale, advancing rounds in between -- the full Rounds 1+3+5+finale
/// playable slice per the implementation plan's Phase 1 scope (rounds 2/4
/// are host-run-manually contest placeholders, nothing for this harness to
/// drive).
///
/// Rules.md §1: "late arrivals become Servants" -- a `(player_count / 10)`
/// slice of `player_count` (at least 1) joins only *after* the raffle
/// closes, exercising that path for real rather than leaving it
/// theoretical; everyone else is an on-time raffle candidate.
///
/// `phase_wait` is how long each Denouncement/task phase stays open before
/// the host closes it -- long enough for every bot's reactive loop to see
/// the phase change and act at least once. Bots react to a pushed `View`
/// almost immediately (this is a websocket broadcast, not HTTP polling on
/// an interval), so a short wait (tens of milliseconds) is enough even at
/// 30 players; the interactive tool uses a much longer one so a real human
/// playing alongside the bots has time to act too.
pub async fn run_full_automated_game(
    url: &str,
    player_count: usize,
    seed: u64,
    phase_wait: Duration,
) -> Result<GameSummary, ConnError> {
    let late_count = (player_count / 10).max(1);
    let on_time_count = player_count - late_count;

    // Shared with every bot and read by `HostDriver::draw_intermission_entrants`
    // -- see that method's doc comment for why the Intermission opt-in pool
    // can't just be read back off the wire the way everything else is.
    let intermission_pool: Arc<Mutex<BTreeSet<PlayerId>>> = Arc::new(Mutex::new(BTreeSet::new()));
    // Flipped `true` only once `setup_game` has fully returned -- see
    // `PlayerBot::react`'s doc comment on why bots must stay silent before
    // then: `setup_game` itself relies on `do_cmd_sequential`'s "nothing
    // else is concurrently mutating state" assumption, which an early
    // ability activation or Intermission opt-in would violate. A bot's own
    // `SubmitInterestLevel` is the one exception -- see `PlayerBot::react`.
    let setup_complete = Arc::new(AtomicBool::new(false));

    let spawn_bot =
        |i: usize,
         setup_complete: Arc<AtomicBool>,
         handles: &mut Vec<tokio::task::JoinHandle<Result<(), ConnError>>>| {
            let url = url.to_string();
            let name = format!("Bot{i}");
            let bot_seed = seed.wrapping_add(i as u64 * 7_919 + 1);
            let pool = Arc::clone(&intermission_pool);
            handles.push(tokio::spawn(async move {
                let bot = PlayerBot::join(&url, &name, bot_seed, pool, setup_complete).await?;
                bot.run().await
            }));
        };

    let mut handles = Vec::with_capacity(player_count);
    for i in 0..on_time_count {
        spawn_bot(i, Arc::clone(&setup_complete), &mut handles);
    }

    let mut host = HostDriver::connect(url).await?;
    let roster = host
        .wait_for_roster(on_time_count, Duration::from_secs(15))
        .await?;
    host.setup_game(&roster, seed).await?;

    // Late arrivals join now, right after `setup_game`'s `CloseRaffle` --
    // but *before* `setup_complete` flips, deliberately: every on-time bot
    // starts firing `SubmitBio`/ability reactions the instant it flips
    // (see `PlayerBot::react`), and with `on_time_count` bots all doing
    // that at once, the resulting broadcast storm made the next
    // `wait_for_roster` below miss the roster actually growing --
    // confirmed by reproducing it at 30 players. Joining while things are
    // still quiet (matching the proven pre-setup regime the first
    // `wait_for_roster` above already relies on) avoids that. A late bot
    // itself stays silent on everything but its own (harmless, ignored)
    // interest-level submission until `setup_complete` flips too, so
    // spawning it early doesn't let it race `setup_game`'s own commands.
    for i in on_time_count..player_count {
        spawn_bot(i, Arc::clone(&setup_complete), &mut handles);
    }
    let full_roster = host
        .wait_for_roster(player_count, Duration::from_secs(15))
        .await?;
    let late_arrivals: Vec<PlayerId> = full_roster
        .iter()
        .copied()
        .filter(|id| !roster.contains(id))
        .collect();
    setup_complete.store(true, Ordering::Relaxed);

    host.run_round_one_tasks(&full_roster, seed, phase_wait)
        .await?;
    // The literal late-arrival Servants earn their first point right away.
    host.award_servant_points(late_arrivals.iter().copied())
        .await?;

    host.advance_round_to(Round::Two).await?;
    host.record_contest_results_for_round(Round::Two, seed)
        .await?;

    host.advance_round_to(Round::Three).await?;
    host.run_bio_driven_tasks(phase_wait).await?;
    let mut previously_cast_out: BTreeSet<PlayerId> = BTreeSet::new();
    let view = host.run_denouncement(phase_wait).await?;
    award_newly_cast_out(&mut host, &view, &mut previously_cast_out).await?;

    host.advance_round_to(Round::Four).await?;
    host.record_contest_results_for_round(Round::Four, seed.wrapping_add(1))
        .await?;
    // rules.md §4's round order is Round 4 -> Intermission -> Round 5, not
    // right after Round 2 -- drawing here (rather than right after bots
    // opt in during Round 1) also means an entrant can no longer be Cast
    // Out at Round 3's Denouncement before their own live Intermission
    // moment ever happens.
    host.draw_intermission_entrants(&intermission_pool, seed)
        .await?;

    host.advance_round_to(Round::Five).await?;
    host.run_bio_driven_tasks(phase_wait).await?;
    let view = host.run_denouncement(phase_wait).await?;
    award_newly_cast_out(&mut host, &view, &mut previously_cast_out).await?;

    host.advance_round_to(Round::Finale).await?;
    let final_view = host.run_denouncement(phase_wait).await?;
    let newly_cast_out: Vec<PlayerId> = final_view
        .roster
        .iter()
        .filter(|r| r.status == PlayerStatus::CastOut && !previously_cast_out.contains(&r.id))
        .map(|r| r.id)
        .collect();
    let resolved_view = host.resolve_gallery_predictions(newly_cast_out).await?;

    for handle in handles {
        handle.abort();
    }

    let cast_out_count = final_view
        .roster
        .iter()
        .filter(|r| r.status == PlayerStatus::CastOut)
        .count();
    Ok(GameSummary {
        final_round: final_view.current_round,
        final_roster: final_view.roster,
        cast_out_count,
        winner: resolved_view.winner,
    })
}

/// Awards a Servant point to every player `view`'s roster shows as
/// `CastOut` for the first time (not already in `previously_cast_out`),
/// then folds them into it -- rules.md §5: an already-Cast-Out player
/// operationally becomes a Servant for the rest of the game.
async fn award_newly_cast_out(
    host: &mut HostDriver,
    view: &engine::PlayerView,
    previously_cast_out: &mut BTreeSet<PlayerId>,
) -> Result<(), ConnError> {
    let newly: Vec<PlayerId> = view
        .roster
        .iter()
        .filter(|r| r.status == PlayerStatus::CastOut && !previously_cast_out.contains(&r.id))
        .map(|r| r.id)
        .collect();
    previously_cast_out.extend(&newly);
    host.award_servant_points(newly).await
}
