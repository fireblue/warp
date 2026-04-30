//! SSH host management.
//!
//! Reads `~/.warp/ssh_config` (which the user's `~/.ssh/config` is `Include`d
//! into) and surfaces hosts to a fuzzy picker. Per-host annotations (tag, color,
//! notes) live in the Warp persistence DB. The user's `~/.ssh/config` is never
//! modified — `~/.warp/ssh_config` is the only file Warp writes to.
//!
//! Phase-1 design doc:
//! `docs/superpowers/specs/2026-04-30-ssh-manager-and-no-login-design.md`.

pub mod annotation_repo;
pub mod connection;
pub mod data_source;
pub mod model;
pub mod parser;
pub mod paths;
pub mod search_item;
