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

mod command;
mod error;
mod event;
mod player;
mod state;
mod view;

pub use command::Command;
pub use error::GameError;
pub use event::DomainEvent;
pub use player::{Faction, Player, PlayerId};
pub use state::{apply_command, GameState};
pub use view::{view_for, PlayerView, RosterEntry, Viewer};
