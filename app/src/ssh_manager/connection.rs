//! Build the `ssh` command line for connecting to an SSH host.
//!
//! Output is consumed by the PTY pipeline as a `shell_starter` override:
//! Warp launches `ssh -F <warp_config> -t <alias>` instead of the default
//! login shell.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

/// What `ssh` binary and arguments to spawn for a given alias.
///
/// Cheap to clone — pass by value.
#[derive(Debug, Clone)]
pub struct SshCommand {
    /// Absolute path to the `ssh` binary, or just `"ssh"` if relying on `$PATH`.
    pub program: PathBuf,
    /// Arguments excluding the program name.
    pub args: Vec<OsString>,
}

/// Build `ssh -F <warp_config> -t <alias>`.
///
/// `-F` makes ssh use the supplied config (and, via its leading `Include`,
/// the user's config) instead of the default search path. `-t` forces a TTY
/// so the remote shell is interactive.
pub fn build(warp_config_path: &Path, alias: &str) -> SshCommand {
    let mut args: Vec<OsString> = Vec::with_capacity(5);
    args.push("-F".into());
    args.push(warp_config_path.as_os_str().into());
    args.push("-t".into());
    args.push(alias.into());
    SshCommand {
        program: PathBuf::from("ssh"),
        args,
    }
}

#[cfg(test)]
#[path = "connection_tests.rs"]
mod tests;
