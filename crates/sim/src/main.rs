//! Headless simulation harness entry point.
//!
//! Phase 0: proves the `sim` -> `engine` wiring works end to end with a
//! trivial scenario. Phase 1+ replaces this with configurable player counts,
//! pluggable virtual-player strategies, and the scripted regression
//! scenarios described in the implementation plan's "Testing Strategy"
//! section (one per rules.md resolution branch, plus a boundary-count
//! matrix and randomized fuzzing for win-condition invariants).

use engine::{apply_command, Command, Faction, GameState, PlayerId};

fn main() {
    let mut state = GameState::new();

    for name in ["Alice", "Bob", "Carol"] {
        apply_command(
            &mut state,
            Command::AddPlayer {
                name: name.to_string(),
            },
        )
        .expect("adding a player during setup should never fail");
    }

    apply_command(
        &mut state,
        Command::AssignFaction {
            player: PlayerId(0),
            faction: Faction::Ton,
        },
    )
    .expect("assigning a fresh player's faction should never fail");

    println!(
        "sim: created a {}-player game, {} events logged so far",
        state.players().count(),
        state.event_log().len()
    );
}
