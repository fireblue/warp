//! Filesystem paths used by the SSH manager.
//!
//! Layout:
//! - `~/.ssh/config`        — user-owned, never modified.
//! - `~/.warp/ssh_config`   — Warp-owned. First line `Include ~/.ssh/config`.
//!                            All Warp-added hosts go below.
//!
//! Warp connects via `ssh -F <warp_config_path()> -t <alias>`, so the user's
//! config (and any nested `Include`s in it) is honored automatically.

use std::path::PathBuf;

use anyhow::{Context, Result};

/// Directory `~/.warp/`. Created on demand.
pub fn warp_config_dir() -> Result<PathBuf> {
    let home = dirs::home_dir().context("home directory not found")?;
    Ok(home.join(".warp"))
}

/// Path of `~/.warp/ssh_config`. Created on demand by [`ensure_warp_config`].
pub fn warp_config_path() -> Result<PathBuf> {
    Ok(warp_config_dir()?.join("ssh_config"))
}

/// Path of `~/.ssh/config`. Existence is not guaranteed — the user may not
/// have one yet.
pub fn user_config_path() -> Result<PathBuf> {
    let home = dirs::home_dir().context("home directory not found")?;
    Ok(home.join(".ssh").join("config"))
}

/// Initial contents of `~/.warp/ssh_config` when it doesn't exist yet. The
/// leading `Include` chains the user's config in so a single `ssh -F` reads
/// both files.
const INITIAL_WARP_CONFIG: &str =
    "# Warp-managed SSH config. Hosts added via Warp UI go below.\n\
     # Edit this file directly only for advanced syntax not covered by the UI.\n\
     Include ~/.ssh/config\n";

/// Ensure `~/.warp/` and `~/.warp/ssh_config` exist. Idempotent.
pub fn ensure_warp_config() -> Result<PathBuf> {
    let dir = warp_config_dir()?;
    std::fs::create_dir_all(&dir)
        .with_context(|| format!("creating {}", dir.display()))?;

    let path = warp_config_path()?;
    if !path.exists() {
        std::fs::write(&path, INITIAL_WARP_CONFIG)
            .with_context(|| format!("writing {}", path.display()))?;
    }
    Ok(path)
}
