//! Spawns the real, compiled `app` server binary as a child process (not
//! a mock, not the engine directly -- catches wire-format/websocket bugs
//! neither `engine`'s unit tests nor `sim`'s headless harness can see) and
//! drives it with `bots::run_full_automated_game` at a range of player
//! counts up to 30, asserting each game reaches the Finale with a
//! structurally sound final state. Modeled directly on
//! `/home/drc/game-changer`'s `tests/api_integration.rs`: its `TestServer`
//! (spawn-a-real-binary-on-an-ephemeral-port pattern) and its
//! `twelve_participants_play_a_random_simulation_of_the_whole_game` /
//! `two_dozen_participants_play_concurrently_without_the_server_falling_behind`
//! tests are this file's direct ancestors.
//!
//! Run with: `cargo test -p app --no-default-features --features server`
//! (needs the real `server` feature build to serve `/api/ws` at all -- the
//! default `web` feature compiles that whole code path away, see
//! `main.rs`'s module doc comment) -- or `make test`, which already does
//! this.

use bots::GameSummary;
use engine::{PlayerStatus, Round};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};
use tokio::net::TcpListener;

struct TestServer {
    child: Child,
    ws_url: String,
}

impl TestServer {
    async fn start() -> Self {
        ensure_sibling_public_dir_exists();

        let port = free_port().await;
        let ws_url = format!("ws://127.0.0.1:{port}/api/ws");
        let http_url = format!("http://127.0.0.1:{port}/");

        let child = Command::new(env!("CARGO_BIN_EXE_app"))
            .env("IP", "127.0.0.1")
            .env("PORT", port.to_string())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("failed to start the app server binary");

        wait_until_ready(&http_url).await;
        TestServer { child, ws_url }
    }
}

impl Drop for TestServer {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// The compiled server looks for a `public/` directory next to its own
/// executable and panics outright if it's missing (see
/// `dioxus_server::server::public_path`) -- normally `dx build` produces
/// that directory, but a plain `cargo build --features server` (all this
/// test needs; it never touches the compiled WASM frontend) doesn't.
/// Provide a minimal stand-in, exactly mirroring game-changer's own
/// `tests/api_integration.rs::ensure_sibling_public_dir_exists` (same
/// underlying `dioxus-server` crate, same requirement) -- verified against
/// a real spawned server during this test file's own development, not
/// assumed from the source alone.
fn ensure_sibling_public_dir_exists() {
    let exe_path = PathBuf::from(env!("CARGO_BIN_EXE_app"));
    let public_dir = exe_path
        .parent()
        .expect("exe has a parent dir")
        .join("public");
    std::fs::create_dir_all(&public_dir).expect("failed to create stand-in public/ dir");
    let index = public_dir.join("index.html");
    if !index.exists() {
        let stand_in = "<!DOCTYPE html>\n\
             <html>\n\
             <head><title>test</title></head>\n\
             <body><div id=\"main\"></div></body>\n\
             </html>\n";
        let _ = std::fs::write(index, stand_in);
    }
}

async fn free_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .await
        .expect("failed to bind an ephemeral port")
        .local_addr()
        .unwrap()
        .port()
}

async fn wait_until_ready(http_url: &str) {
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        if reqwest_ok(http_url).await {
            return;
        }
        if Instant::now() > deadline {
            panic!("server at {http_url} never became ready");
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

/// A bare TCP-connect probe rather than pulling in `reqwest` as another
/// dev-dependency just for a readiness check -- the server accepting a
/// connection at all is a sufficient "has it started" signal here (the
/// real check that matters, the websocket handshake, happens for real
/// inside every test below).
async fn reqwest_ok(http_url: &str) -> bool {
    let addr = http_url.trim_start_matches("http://").trim_end_matches('/');
    tokio::net::TcpStream::connect(addr).await.is_ok()
}

fn assert_game_completed_soundly(summary: &GameSummary, player_count: usize) {
    assert_eq!(
        summary.final_round,
        Round::Finale,
        "the game should have reached the Finale, got {:?}",
        summary.final_round
    );
    assert_eq!(
        summary.final_roster.len(),
        player_count,
        "no player should have vanished or been duplicated"
    );
    let unique_ids: std::collections::HashSet<_> =
        summary.final_roster.iter().map(|r| r.id).collect();
    assert_eq!(
        unique_ids.len(),
        player_count,
        "every player id should be unique"
    );
    for r in &summary.final_roster {
        assert!(
            matches!(r.status, PlayerStatus::Active | PlayerStatus::CastOut),
            "every player should be either Active or CastOut, got {:?} for {:?}",
            r.status,
            r.name
        );
    }
    // At least one execution across 3 real Denouncements (Round 3, Round
    // 5, the Finale) is expected virtually always at this scale -- not a
    // hard rules.md guarantee (a run of repeat ties could in principle
    // leave every slot unfilled), but a genuine `0` here across a full
    // game is itself worth knowing about, so this is a real assertion,
    // not a decoration.
    assert!(
        summary.cast_out_count > 0,
        "expected at least one Cast-Out across 3 Denouncements, got 0 -- \
         either a real regression or a fluke worth re-running with a different seed"
    );
    // Not a hard assertion: bots run concurrently over real websockets, so
    // which random `Convert`/`CastOut` cascades actually land isn't fully
    // pinned down by the seed alone, and a narrow residual "nobody won"
    // state is still possible even after Dalton's follow-up ruling closed
    // the common case (see `win_condition::evaluate`'s doc comment and
    // `HostDriver::resolve_gallery_predictions`'s). Still worth printing: a
    // `None` here across every run would itself be a sign
    // `win_condition::evaluate` never gets wired up to real games.
    eprintln!("{player_count}-player game winner: {:?}", summary.winner);
}

/// How long each Denouncement/task phase stays open before the host
/// closes it. Bots react to a pushed `View` almost immediately (a
/// websocket broadcast, not HTTP polling), so this only needs to be long
/// enough for every bot's own send round-trip to land -- generous at 30
/// players on a loaded CI-like machine without making the whole suite slow.
const PHASE_WAIT: Duration = Duration::from_millis(400);

#[tokio::test]
async fn five_bots_complete_a_full_game() {
    let server = TestServer::start().await;
    let summary = bots::run_full_automated_game(&server.ws_url, 5, 1, PHASE_WAIT)
        .await
        .expect("a 5-player game should complete");
    assert_game_completed_soundly(&summary, 5);
}

#[tokio::test]
async fn twelve_bots_complete_a_full_game() {
    let server = TestServer::start().await;
    let summary = bots::run_full_automated_game(&server.ws_url, 12, 2, PHASE_WAIT)
        .await
        .expect("a 12-player game should complete");
    assert_game_completed_soundly(&summary, 12);
}

/// 20 competing players is the execution-count formula's own boundary
/// (rules.md §5: 1 execution at <=20, 2 at 21-30) -- worth its own test
/// rather than only exercising one side of it.
#[tokio::test]
async fn twenty_bots_complete_a_full_game_at_the_execution_count_boundary() {
    let server = TestServer::start().await;
    let summary = bots::run_full_automated_game(&server.ws_url, 20, 3, PHASE_WAIT)
        .await
        .expect("a 20-player game should complete");
    assert_game_completed_soundly(&summary, 20);
}

/// 30 is the top of rules.md's stated target headcount (20-30 players)
/// and lands past the 21-player boundary where the Denouncement scales up
/// to 2 simultaneous executions -- the scale this whole harness exists to
/// prove out.
#[tokio::test]
async fn thirty_bots_complete_a_full_game_at_the_top_of_the_target_headcount() {
    let server = TestServer::start().await;
    let summary = bots::run_full_automated_game(&server.ws_url, 30, 4, PHASE_WAIT)
        .await
        .expect("a 30-player game should complete");
    assert_game_completed_soundly(&summary, 30);
}
