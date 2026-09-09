//! Headless full-game simulation harness.
//!
//! Drives real games end to end through `engine::apply_command` only -- no
//! networking, no browser. This is what exercises the engine at the
//! player-count scales and round-count depth that hand-written unit tests
//! don't reach: a real 20-30 player game running Setup through the Last
//! Denouncement, including the headcount-scaling boundaries at 20/21/30/31
//! competing players (rules.md §5).
//!
//! Scope for Phase 1: every player-decision point (nomination, ballot,
//! runoff, task attempts) is driven by a simple seeded-random virtual
//! player, not the scripted-per-rules.md-branch scenarios or adversarial/
//! Cult-coordinated strategies the implementation plan describes for later
//! phases -- those branches are already exhaustively covered by `engine`'s
//! own unit tests (see e.g. `state.rs`'s cascade tests and
//! `win_condition.rs`'s Path A-D tests). What this harness adds on top is
//! full-game integration coverage: does a real game run start-to-finish at
//! realistic scale without panicking or producing a malformed outcome.
//!
//! Usage: `cargo run -p sim -- --players 20-30 --games 25 --seed 1`

use engine::{
    apply_command, evaluate_win_conditions, Ballot, Character, Command, DenouncementPhase,
    DomainEvent, Faction, GameOutcome, GameState, PlayerId, PlayerStatus, TaskId, TaskTier, Viewer,
};
use rand::rngs::StdRng;
use rand::seq::{IndexedRandom, SliceRandom};
use rand::{RngExt, SeedableRng};

fn main() {
    let config = Config::from_args(std::env::args().skip(1));

    let mut total = 0usize;
    let mut failures = 0usize;

    for player_count in config.min_players..=config.max_players {
        for game_index in 0..config.games_per_count {
            total += 1;
            let game_seed = derive_seed(config.seed, player_count, game_index);
            let mut rng = StdRng::seed_from_u64(game_seed);
            match run_one_game(player_count, &mut rng) {
                Ok(outcome) => {
                    println!(
                        "players={player_count:>2} game={game_index:>3} seed={game_seed:>20} -> {outcome:?}"
                    );
                }
                Err(err) => {
                    failures += 1;
                    eprintln!(
                        "players={player_count:>2} game={game_index:>3} seed={game_seed:>20} FAILED: {err}"
                    );
                }
            }
        }
    }

    println!("\n{total} games run, {failures} failed.");
    if failures > 0 {
        std::process::exit(1);
    }
}

struct Config {
    min_players: usize,
    max_players: usize,
    games_per_count: usize,
    seed: u64,
}

impl Config {
    /// Defaults to the boundary-count matrix the implementation plan calls
    /// out by name (20/21/30/31 competing players -- the execution-count
    /// formula's scaling boundaries) rather than an arbitrary round number,
    /// since that's the case this harness exists to catch regressions in.
    fn from_args(args: impl Iterator<Item = String>) -> Config {
        let mut min_players = 20;
        let mut max_players = 31;
        let mut games_per_count = 10;
        let mut seed = 1;

        let args: Vec<String> = args.collect();
        let mut i = 0;
        while i < args.len() {
            match args[i].as_str() {
                "--players" => {
                    let value = args
                        .get(i + 1)
                        .unwrap_or_else(|| panic!("--players needs a value"));
                    let (lo, hi) = parse_range(value);
                    min_players = lo;
                    max_players = hi;
                    i += 2;
                }
                "--games" => {
                    games_per_count = args
                        .get(i + 1)
                        .unwrap_or_else(|| panic!("--games needs a value"))
                        .parse()
                        .expect("--games must be a number");
                    i += 2;
                }
                "--seed" => {
                    seed = args
                        .get(i + 1)
                        .unwrap_or_else(|| panic!("--seed needs a value"))
                        .parse()
                        .expect("--seed must be a number");
                    i += 2;
                }
                other => panic!(
                    "unknown argument {other:?} -- expected --players N or N-M, --games N, --seed N"
                ),
            }
        }

        Config {
            min_players,
            max_players,
            games_per_count,
            seed,
        }
    }
}

fn parse_range(s: &str) -> (usize, usize) {
    match s.split_once('-') {
        Some((lo, hi)) => (
            lo.parse().expect("range start must be a number"),
            hi.parse().expect("range end must be a number"),
        ),
        None => {
            let n = s.parse().expect("--players value must be a number");
            (n, n)
        }
    }
}

/// A simple, fixed mixing function -- not cryptographic, just enough that
/// every (player count, game index) pair under one `--seed` gets an
/// independent, reproducible RNG stream.
fn derive_seed(base: u64, player_count: usize, game_index: usize) -> u64 {
    base.wrapping_mul(1_000_003)
        .wrapping_add(player_count as u64)
        .wrapping_mul(1_000_033)
        .wrapping_add(game_index as u64)
}

/// Applies `cmd` and turns a failure into a `String` that names the exact
/// command that was rejected, not just the bare `GameError` -- several
/// error variants (e.g. `NotActive`) are shared across many command kinds,
/// so the error alone doesn't say which call site hit it.
fn apply(state: &mut GameState, cmd: Command) -> Result<Vec<DomainEvent>, String> {
    let label = format!("{cmd:?}");
    apply_command(state, cmd).map_err(|e| format!("{label} -> {e}"))
}

fn run_one_game(player_count: usize, rng: &mut StdRng) -> Result<GameOutcome, String> {
    let mut state = GameState::new();
    setup_game(&mut state, player_count, rng)?;
    apply(&mut state, Command::FinalizeSetup)?;

    run_round_one_tasks(&mut state, rng)?;

    // Round 1 -> Round 3, run its Denouncement.
    advance_round(&mut state)?;
    advance_round(&mut state)?;
    attempt_cult_conversion(&mut state, rng)?;
    run_denouncement(&mut state, rng)?;

    // Round 3 -> Round 5, run its Denouncement.
    advance_round(&mut state)?;
    advance_round(&mut state)?;
    attempt_cult_conversion(&mut state, rng)?;
    run_denouncement(&mut state, rng)?;

    // Round 5 -> the Finale, run the Last Denouncement.
    advance_round(&mut state)?;
    attempt_cult_conversion(&mut state, rng)?;
    run_denouncement(&mut state, rng)?;

    // Smoke-test the read path too: view_for must never panic for any
    // viewer, at any point a real client could ask for a snapshot.
    for viewer in std::iter::once(Viewer::Host)
        .chain(std::iter::once(Viewer::Display))
        .chain(state.players().map(|p| Viewer::Player(p.id)))
        .collect::<Vec<_>>()
    {
        let _ = engine::view_for(&state, viewer);
    }

    Ok(evaluate_win_conditions(&state))
}

fn advance_round(state: &mut GameState) -> Result<(), String> {
    apply(state, Command::AdvanceRound).map(|_| ())
}

fn setup_game(state: &mut GameState, player_count: usize, rng: &mut StdRng) -> Result<(), String> {
    let mut ids = Vec::with_capacity(player_count);
    for i in 0..player_count {
        let events = apply(
            state,
            Command::AddPlayer {
                name: format!("Player{i}"),
            },
        )?;
        match events.as_slice() {
            [DomainEvent::PlayerAdded { id, .. }] => ids.push(*id),
            other => return Err(format!("unexpected AddPlayer result: {other:?}")),
        }
    }
    ids.shuffle(rng);

    // rules.md §2: Servants ~10% of the total; the Cult starts seeded with
    // just the Cult Leader; the remaining pool splits ~60/40 Ton/Uprising.
    let servant_count = (player_count as f64 * 0.10).round() as usize;
    let cult_count = 1;
    let remaining = player_count
        .checked_sub(servant_count + cult_count)
        .ok_or_else(|| {
            format!("{player_count} players is too few to seed Servants + a Cult Leader")
        })?;
    let ton_count = ((remaining as f64) * 0.6).round() as usize;
    let uprising_count = remaining - ton_count;

    if ton_count < 2 || uprising_count < 1 {
        return Err(format!(
            "{player_count} players doesn't leave enough Ton ({ton_count}, need 2) or \
             Uprising ({uprising_count}, need 1) to fill every title"
        ));
    }

    let mut cursor = 0;
    let servants = &ids[cursor..cursor + servant_count];
    cursor += servant_count;
    let cult = &ids[cursor..cursor + cult_count];
    cursor += cult_count;
    let ton = &ids[cursor..cursor + ton_count];
    cursor += ton_count;
    let uprising = &ids[cursor..cursor + uprising_count];
    cursor += uprising_count;
    debug_assert_eq!(cursor, player_count);

    for group in [
        (servants, Faction::Servant),
        (cult, Faction::Cult),
        (ton, Faction::Ton),
        (uprising, Faction::Uprising),
    ] {
        for &id in group.0 {
            apply(
                state,
                Command::AssignFaction {
                    player: id,
                    faction: group.1,
                },
            )?;
        }
    }

    for (player, character) in [
        (ton[0], Character::KingQueen),
        (ton[1], Character::PrincePrincess),
        (uprising[0], Character::RevolutionaryLeader),
        (cult[0], Character::CultLeader),
    ] {
        apply(state, Command::AssignCharacter { player, character })?;
    }

    Ok(())
}

/// Round 1 (rules.md §4): exactly 2 fixed tasks (1 easy, 1 medium), every
/// player self-reports 3 names. The qualifying set behind each prompt is
/// content-authoring detail (Phase 4) -- one random "ground truth" player
/// per task is enough to exercise the credit-on-any-match mechanic
/// realistically here.
fn run_round_one_tasks(state: &mut GameState, rng: &mut StdRng) -> Result<(), String> {
    let everyone: Vec<PlayerId> = state.players().map(|p| p.id).collect();
    let easy = push_task(
        state,
        "Talk to someone wearing a mask",
        TaskTier::Easy,
        everyone.choose(rng).copied(),
    )?;
    let medium = push_task(
        state,
        "Talk to someone who loves to dance",
        TaskTier::Medium,
        everyone.choose(rng).copied(),
    )?;

    for &player in &everyone {
        let mut candidates: Vec<PlayerId> = everyone
            .iter()
            .copied()
            .filter(|&id| id != player)
            .collect();
        candidates.shuffle(rng);
        if candidates.len() < 3 {
            continue; // not enough other players to attempt -- won't happen at 20+.
        }
        let named = [candidates[0], candidates[1], candidates[2]];
        for &task in &[easy, medium] {
            apply(
                state,
                Command::AttemptTask {
                    player,
                    task,
                    named,
                },
            )?;
        }
    }

    apply(state, Command::CloseTasks)?;
    Ok(())
}

fn push_task(
    state: &mut GameState,
    prompt: &str,
    tier: TaskTier,
    qualifying: Option<PlayerId>,
) -> Result<TaskId, String> {
    let events = apply(
        state,
        Command::PushTask {
            prompt: prompt.into(),
            tier,
            qualifying_players: qualifying.into_iter().collect(),
        },
    )?;
    match events.as_slice() {
        [DomainEvent::TaskPushed { id, .. }] => Ok(*id),
        other => Err(format!("unexpected PushTask result: {other:?}")),
    }
}

/// The Cult Leader converts one random eligible target, if any -- a
/// simplified stand-in for Phase 2's real recruitment schedule (rules.md
/// §2: "growing from 1 toward ~4 by game's end"), which isn't built into
/// the engine yet. Called up to 3 times per game (once per Denouncement),
/// which lands in the same rough final-Cult-size ballpark. A no-op once
/// the Cult Leader has been Cast Out, or if no one is left to convert.
fn attempt_cult_conversion(state: &mut GameState, rng: &mut StdRng) -> Result<(), String> {
    let Some(cult_leader) = state.cult_leader() else {
        return Ok(());
    };
    if !state
        .player(cult_leader)
        .is_some_and(|p| p.status == PlayerStatus::Active)
    {
        return Ok(());
    }
    let targets: Vec<PlayerId> = state
        .players()
        .filter(|p| {
            p.status == PlayerStatus::Active
                && !p.converted
                && matches!(p.faction, Faction::Ton | Faction::Uprising)
        })
        .map(|p| p.id)
        .collect();
    let Some(&target) = targets.choose(rng) else {
        return Ok(());
    };
    apply(
        state,
        Command::Convert {
            converter: cult_leader,
            target,
        },
    )?;
    Ok(())
}

/// Runs one full Denouncement (Nomination -> Discussion -> Ballot ->
/// optional Runoff) with every active player driven by `rng`. Used for
/// Rounds 3, 5, and the Finale's Last Denouncement alike -- the procedure
/// is identical each time (rules.md §5); only the surrounding narration
/// differs, which is a UI concern, not this harness's.
fn run_denouncement(state: &mut GameState, rng: &mut StdRng) -> Result<(), String> {
    apply(state, Command::OpenDenouncement)?;

    let active = active_players(state);
    for &voter in &active {
        let candidates: Vec<PlayerId> = active.iter().copied().filter(|&id| id != voter).collect();
        let Some(&nominee) = candidates.choose(rng) else {
            continue; // no one else active to nominate -- won't happen at this scale.
        };
        apply(state, Command::Nominate { voter, nominee })?;
    }
    apply(state, Command::CloseNomination)?;

    apply(state, Command::OpenBallot)?;
    cast_random_ballots(state, rng)?;
    let events = apply(
        state,
        Command::CloseBallot {
            fallback_replacement: None,
        },
    )?;

    if events
        .iter()
        .any(|e| matches!(e, DomainEvent::RunoffOpened { .. }))
    {
        cast_random_ballots(state, rng)?;
        apply(
            state,
            Command::CloseRunoff {
                fallback_replacement: None,
            },
        )?;
    }

    Ok(())
}

fn active_players(state: &GameState) -> Vec<PlayerId> {
    state
        .players()
        .filter(|p| p.status == PlayerStatus::Active)
        .map(|p| p.id)
        .collect()
}

/// Casts a ballot for every active player against whatever the current
/// Ballot/Runoff phase's candidate list is, abstaining ~10% of the time.
fn cast_random_ballots(state: &mut GameState, rng: &mut StdRng) -> Result<(), String> {
    let candidates: Vec<PlayerId> = match state.denouncement_phase() {
        Some(DenouncementPhase::Ballot { surfaced, .. }) => surfaced.clone(),
        Some(DenouncementPhase::Runoff { candidates, .. }) => candidates.clone(),
        other => return Err(format!("expected Ballot or Runoff phase, got {other:?}")),
    };

    for voter in active_players(state) {
        let ballot = match candidates.choose(rng) {
            Some(&candidate) if rng.random_range(0..10) != 0 => Ballot::For(candidate),
            _ => Ballot::Abstain,
        };
        apply(state, Command::CastBallot { voter, ballot })?;
    }
    Ok(())
}
