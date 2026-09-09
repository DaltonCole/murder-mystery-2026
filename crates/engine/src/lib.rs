//! Pure game-logic core for Murder Mystery 2026.
//!
//! This crate has no I/O, no async runtime, and no dependency on Dioxus or
//! Axum. Everything here is deterministic and directly testable with plain
//! `cargo test`. The `app` crate is the only thing that talks to a network
//! or a browser, and it does so by calling [`apply_command`] and
//! [`view_for`] -- it never mutates or serializes [`GameState`] on its own.
//! See the "Core Domain Model" section of the implementation plan
//! (`/home/drc/.claude/plans/piped-crunching-lighthouse.md`) for the
//! rationale behind this boundary.

mod character;
mod command;
mod denouncement;
mod error;
mod event;
mod player;
mod round;
mod state;
mod task;
mod view;
mod win_condition;

pub use character::{Character, PlayerStatus};
pub use command::Command;
pub use denouncement::{Ballot, DenouncementPhase};
pub use error::GameError;
pub use event::DomainEvent;
pub use player::{Faction, Player, PlayerId};
pub use round::Round;
pub use state::{apply_command, GameState};
pub use task::{TaskDef, TaskId, TaskTier};
pub use view::{view_for, DenouncementView, PlayerView, RosterEntry, TaskView, Viewer};
pub use win_condition::{evaluate as evaluate_win_conditions, CultPath, GameOutcome};
