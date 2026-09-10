//! Interactive bot filler: joins N bot players onto an *already-running*
//! server (started separately with `make run` or `dx serve`) so you can
//! drive the actual game from your own `/host` browser tab -- assigning
//! factions/titles, opening/closing each Denouncement phase, pushing
//! tasks -- while the bots automatically nominate, vote, and attempt
//! tasks like real (if slightly chaotic) players would. Lets you feel out
//! real room pacing and exercise the Host console without rounding up a
//! dozen actual guests.
//!
//! Deliberately does *not* drive the host side itself (unlike
//! `run_full_automated_game`, which the automated test suite uses) -- the
//! whole point of this tool is to let you exercise the real `/host` UI
//! live, the same way `/home/drc/game-changer`'s
//! `examples/simulate_party.rs` leaves real room/admin control to you and
//! only simulates the participants.
//!
//! Usage: `cargo run -p bots -- --bots 20` (see `--help`), or `make bots
//! BOTS=20`.

use bots::PlayerBot;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

struct Args {
    bots: usize,
    url: String,
    seed: u64,
}

fn print_help() {
    println!(
        "Usage: cargo run -p bots -- --bots N [options]\n\n\
         Joins N bot players onto an already-running Murder Mystery 2026 server so\n\
         you can drive the real game from your own /host tab while they play along.\n\n\
         Required:\n  \
         --bots N        How many bot players to add\n\n\
         Options:\n  \
         --url URL       Websocket URL to connect to (default: ws://127.0.0.1:8080/api/ws)\n  \
         --seed N        RNG seed, for reproducible bot behavior (default: 1)\n  \
         --help          Show this message"
    );
}

fn parse_args() -> Args {
    let mut bots: Option<usize> = None;
    let mut url = "ws://127.0.0.1:8080/api/ws".to_string();
    let mut seed = 1u64;

    let mut raw = std::env::args().skip(1);
    while let Some(arg) = raw.next() {
        match arg.as_str() {
            "--help" | "-h" => {
                print_help();
                std::process::exit(0);
            }
            "--bots" => {
                bots = Some(
                    expect_value(&mut raw, "--bots")
                        .parse()
                        .unwrap_or_else(|_| fail("--bots needs a number")),
                )
            }
            "--url" => url = expect_value(&mut raw, "--url"),
            "--seed" => {
                seed = expect_value(&mut raw, "--seed")
                    .parse()
                    .unwrap_or_else(|_| fail("--seed needs a number"))
            }
            other => fail(&format!("unknown argument {other:?} (see --help)")),
        }
    }

    let bots = bots.unwrap_or_else(|| fail("--bots N is required (see --help)"));
    Args { bots, url, seed }
}

fn expect_value(args: &mut impl Iterator<Item = String>, flag: &str) -> String {
    args.next()
        .unwrap_or_else(|| fail(&format!("{flag} needs a value")))
}

fn fail(message: &str) -> ! {
    eprintln!("error: {message}");
    std::process::exit(1);
}

#[tokio::main]
async fn main() {
    let args = parse_args();
    println!(
        "Murder Mystery 2026 bot filler -- adding {} bots to {}",
        args.bots, args.url
    );

    let joined: Arc<Mutex<HashMap<String, bool>>> = Arc::new(Mutex::new(HashMap::new()));
    // This interactive tool leaves the real Host console to you (see the
    // module doc comment), so nothing here ever reads the Intermission
    // opt-in pool the way `run_full_automated_game`'s own `HostDriver` does
    // -- it's still threaded through since `PlayerBot` shares one pool with
    // every other bot it opts in alongside.
    let intermission_pool: Arc<Mutex<std::collections::BTreeSet<engine::PlayerId>>> =
        Arc::new(Mutex::new(std::collections::BTreeSet::new()));
    let mut handles = Vec::with_capacity(args.bots);

    for i in 0..args.bots {
        let url = args.url.clone();
        let name = format!("Bot{i}");
        let bot_seed = args.seed.wrapping_add(i as u64 * 7_919 + 1);
        let joined = Arc::clone(&joined);
        let pool = Arc::clone(&intermission_pool);
        let handle_name = name.clone();
        handles.push(tokio::spawn(async move {
            match PlayerBot::join(&url, &name, bot_seed, pool).await {
                Ok(bot) => {
                    joined.lock().unwrap().insert(handle_name, true);
                    if let Err(e) = bot.run().await {
                        eprintln!("[{name}] connection ended: {e}");
                    }
                }
                Err(e) => {
                    joined.lock().unwrap().insert(handle_name, false);
                    eprintln!("[{name}] failed to join: {e}");
                }
            }
        }));
    }

    println!(
        "Bots are connecting. Open your own /host tab now and run the game --\n\
         assign factions/titles, open/close Denouncement phases, push tasks -- the\n\
         bots will nominate, vote, and attempt tasks automatically as you do.\n\
         Ctrl-C to stop."
    );

    let start = Instant::now();
    loop {
        tokio::time::sleep(Duration::from_secs(5)).await;
        let snapshot = joined.lock().unwrap().clone();
        let ok = snapshot.values().filter(|v| **v).count();
        let failed = snapshot.values().filter(|v| !**v).count();
        println!(
            "[{}s] {ok}/{} bots connected{}",
            start.elapsed().as_secs(),
            args.bots,
            if failed > 0 {
                format!(", {failed} failed")
            } else {
                String::new()
            },
        );
    }
}
