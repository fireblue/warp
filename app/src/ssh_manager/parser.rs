//! Parses ssh_config files into [`SshHost`] records.
//!
//! Strategy: parse `~/.warp/ssh_config` (which transitively `Include`s the
//! user's `~/.ssh/config`). To distinguish source per host, separately scan
//! the raw `~/.warp/ssh_config` text for `Host <alias>` lines — those are
//! Warp-source. Everything else came in via the `Include` chain (or another
//! file the user already had).

use std::collections::HashSet;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;

use anyhow::{Context, Result};
use ssh2_config::{ParseRule, SshConfig};

use crate::ssh_manager::model::{HostSource, SshHost};

/// Parse `warp_config_path` (which `Include`s the user's config) and return
/// every connectable host. Wildcard patterns (`Host *`, `Host *.example.com`)
/// are excluded — they're config templates, not endpoints.
pub fn parse_all(warp_config_path: &Path) -> Result<Vec<SshHost>> {
    let warp_aliases = scan_warp_owned_aliases(warp_config_path)?;

    let file = File::open(warp_config_path)
        .with_context(|| format!("opening {}", warp_config_path.display()))?;
    let mut reader = BufReader::new(file);
    let cfg = SshConfig::default()
        .parse(&mut reader, ParseRule::ALLOW_UNKNOWN_FIELDS)
        .with_context(|| format!("parsing {}", warp_config_path.display()))?;

    let mut out = Vec::with_capacity(cfg.get_hosts().len());
    for host in cfg.get_hosts() {
        if let Some(parsed) = extract_host(host, &warp_aliases) {
            out.push(parsed);
        }
    }
    Ok(out)
}

/// Reads the raw ssh_config file and returns the set of aliases declared
/// inside it via `Host <alias>` lines. Used to mark which hosts are
/// Warp-owned vs. inherited from the `Include`d user config.
fn scan_warp_owned_aliases(path: &Path) -> Result<HashSet<String>> {
    let file = File::open(path)
        .with_context(|| format!("opening {}", path.display()))?;
    let mut aliases = HashSet::new();
    for line in BufReader::new(file).lines() {
        let line = line.with_context(|| format!("reading {}", path.display()))?;
        let trimmed = line.trim_start();
        let lower = trimmed.to_ascii_lowercase();
        let Some(rest) = lower.strip_prefix("host") else {
            continue;
        };
        // The keyword must be followed by whitespace, not part of e.g. `HostName`.
        if !rest.starts_with(|c: char| c.is_whitespace()) {
            continue;
        }
        for tok in trimmed.split_whitespace().skip(1) {
            // First token is the keyword we just verified; remaining tokens
            // are alias patterns. Skip wildcards and negations.
            let token = tok.trim_start_matches('!');
            if token.is_empty() || is_wildcard(token) {
                continue;
            }
            aliases.insert(token.to_string());
        }
    }
    Ok(aliases)
}

fn extract_host(host: &ssh2_config::Host, warp_aliases: &HashSet<String>) -> Option<SshHost> {
    // Take the first non-negated, non-wildcard pattern as the canonical alias.
    let alias = host
        .pattern
        .iter()
        .find(|c| !c.negated && !is_wildcard(&c.pattern))
        .map(|c| c.pattern.clone())?;

    let p = &host.params;
    let source = if warp_aliases.contains(&alias) {
        HostSource::Warp
    } else {
        HostSource::User
    };

    Some(SshHost {
        alias,
        hostname: p.host_name.clone(),
        user: p.user.clone(),
        port: p.port,
        identity_file: p
            .identity_file
            .as_ref()
            .and_then(|files| files.first().cloned()),
        proxy_jump: p.proxy_jump.as_ref().map(|hops| hops.join(",")),
        source,
        annotation: None,
    })
}

fn is_wildcard(pattern: &str) -> bool {
    pattern.contains('*') || pattern.contains('?')
}

#[cfg(test)]
#[path = "parser_tests.rs"]
mod tests;
