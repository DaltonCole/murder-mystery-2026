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

use engine::{PlayerStatus, RosterEntry, Round};
use std::time::Duration;

pub struct GameSummary {
    pub final_round: Round,
    pub final_roster: Vec<RosterEntry>,
    pub cast_out_count: usize,
}

/// Runs one entire game end to end against `url` (a real, already-running
/// server) with `player_count` bots: joins them all, sets up factions and
/// titles, runs Round 1's tasks, then Denouncements at Round 3, Round 5,
/// and the Finale, advancing rounds in between -- the full Rounds 1+3+5+
/// finale playable slice per the implementation plan's Phase 1 scope
/// (rounds 2/4 are host-run-manually contest placeholders, nothing for
/// this harness to drive).
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
    let mut handles = Vec::with_capacity(player_count);
    for i in 0..player_count {
        let url = url.to_string();
        let name = format!("Bot{i}");
        let bot_seed = seed.wrapping_add(i as u64 * 7_919 + 1);
        handles.push(tokio::spawn(async move {
            let bot = PlayerBot::join(&url, &name, bot_seed).await?;
            bot.run().await
        }));
    }

    let mut host = HostDriver::connect(url).await?;
    let roster = host
        .wait_for_roster(player_count, Duration::from_secs(15))
        .await?;
    host.setup_game(&roster, seed).await?;
    host.run_round_one_tasks(&roster, seed, phase_wait).await?;

    host.advance_round_to(Round::Two).await?;
    host.advance_round_to(Round::Three).await?;
    host.run_denouncement(phase_wait).await?;

    host.advance_round_to(Round::Four).await?;
    host.advance_round_to(Round::Five).await?;
    host.run_denouncement(phase_wait).await?;

    host.advance_round_to(Round::Finale).await?;
    let final_view = host.run_denouncement(phase_wait).await?;

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
    })
}
