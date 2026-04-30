//! Synchronous data source feeding SSH hosts into the command palette.
//!
//! On each query the source re-reads `~/.warp/ssh_config` (which `Include`s
//! the user's `~/.ssh/config`). ssh_config files are tiny, so re-parsing is
//! cheap and avoids stale state without a file watcher.

use anyhow::Result;
use fuzzy_match::FuzzyMatchResult;

use crate::search::command_search::searcher::CommandSearchItemAction;
use crate::search::data_source::{Query, QueryFilter, QueryResult};
use crate::search::mixer::{DataSourceRunErrorWrapper, SyncDataSource};
use crate::ssh_manager::model::SshHost;
use crate::ssh_manager::search_item::SshSearchItem;
use crate::ssh_manager::{parser, paths};
use warpui::AppContext;

/// Yields one [`SshSearchItem`] per connectable host. Held as a tiny
/// stateless struct — a singleton in the workspace.
#[derive(Default)]
pub struct SshHostsDataSource;

impl SshHostsDataSource {
    pub fn new() -> Self {
        Self::default()
    }
}

impl SyncDataSource for SshHostsDataSource {
    type Action = CommandSearchItemAction;

    fn run_query(
        &self,
        query: &Query,
        _app: &AppContext,
    ) -> Result<Vec<QueryResult<Self::Action>>, DataSourceRunErrorWrapper> {
        // SSH hosts only surface when the user has explicitly asked for them
        // via the Cmd+Shift+S keybinding (which sets `QueryFilter::Ssh`).
        // Without this guard a generic Cmd+K palette would mix every SSH
        // host into command/workflow results.
        if !query.filters.contains(&QueryFilter::Ssh) {
            return Ok(Vec::new());
        }

        let hosts = match load_hosts() {
            Ok(h) => h,
            Err(err) => {
                log::warn!("ssh_manager: failed to load hosts: {err:#}");
                return Ok(Vec::new());
            }
        };

        let query_text = query.text.trim();
        let mut results: Vec<QueryResult<Self::Action>> = Vec::with_capacity(hosts.len());

        if query_text.is_empty() {
            // Empty query → show every host, ordered alphabetically by alias
            // (ties on score=0 fall back to source_order, then alphabetic via
            // the sort below).
            let mut sorted = hosts;
            sorted.sort_by(|a, b| a.alias.cmp(&b.alias));
            for host in sorted {
                results.push(item_for_host(host, FuzzyMatchResult::no_match()).into());
            }
        } else {
            for host in hosts {
                if let Some(match_result) =
                    fuzzy_match::match_indices_case_insensitive(&host.alias, query_text)
                {
                    results.push(item_for_host(host, match_result).into());
                }
            }
        }

        Ok(results)
    }
}

impl warpui::Entity for SshHostsDataSource {
    type Event = ();
}

fn load_hosts() -> Result<Vec<SshHost>> {
    let config_path = paths::ensure_warp_config()?;
    parser::parse_all(&config_path)
}

fn item_for_host(host: SshHost, match_result: FuzzyMatchResult) -> SshSearchItem {
    SshSearchItem {
        alias: host.alias,
        hostname: host.hostname,
        match_result,
    }
}
