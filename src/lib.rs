//! Chronos Library Root (`lib.rs`).
//!
//! Exposes all domain, storage, runtime, keymap, CLI, and TUI modules for integration tests and the binary.

#![allow(clippy::too_many_arguments, clippy::needless_range_loop)]

pub mod audio;
pub mod cli;
pub mod domain;
pub mod error;
pub mod keymap;
pub mod runtime;
pub mod storage;
pub mod tui;
