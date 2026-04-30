//! Synchronous data source feeding SSH hosts into the command palette.
//!
//! On each query the source re-reads `~/.warp/ssh_config` (which `Include`s
//! the user's `~/.ssh/config`). ssh_config files are tiny, so re-parsing is
//! cheap and avoids stale state without a file watcher. Per-query the source
//! also opens a read-only sqlite connection to fetch annotations and joins
//! them onto the parsed hosts; the alias-keyed lookup is O(n) and the DB is
//! tiny in practice.

use std::collections::HashMap;

use anyhow::Result;
use fuzzy_match::FuzzyMatchResult;

use crate::search::command_search::searcher::CommandSearchItemAction;
use crate::search::data_source::{Query, QueryFilter, QueryResult};
use crate::search::mixer::{DataSourceRunErrorWrapper, SyncDataSource};
use crate::ssh_manager::model::{HostAnnotation, SshHost};
use crate::ssh_manager::search_item::SshSearchItem;
use crate::ssh_manager::{annotation_repo, parser, paths};
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
        let annotations = load_annotations();

        let query_text = query.text.trim();
        let mut results: Vec<QueryResult<Self::Action>> = Vec::with_capacity(hosts.len());

        if query_text.is_empty() {
            // Empty query → show every host. Sort by recency-of-connection
            // first (most-recent on top) then alphabetically. Hosts with no
            // recorded connection sort below all annotated-with-timestamp
            // hosts.
            let mut sorted = hosts;
            sorted.sort_by(|a, b| {
                let ra = annotations
                    .get(&a.alias)
                    .and_then(|ann| ann.last_connected_at);
                let rb = annotations
                    .get(&b.alias)
                    .and_then(|ann| ann.last_connected_at);
                match (ra, rb) {
                    (Some(ta), Some(tb)) => tb.cmp(&ta).then_with(|| a.alias.cmp(&b.alias)),
                    (Some(_), None) => std::cmp::Ordering::Less,
                    (None, Some(_)) => std::cmp::Ordering::Greater,
                    (None, None) => a.alias.cmp(&b.alias),
                }
            });
            for host in sorted {
                let match_result = FuzzyMatchResult::no_match();
                results.push(item_for_host(host, match_result, &annotations).into());
            }
        } else {
            for host in hosts {
                if let Some(match_result) =
                    fuzzy_match::match_indices_case_insensitive(&host.alias, query_text)
                {
                    results.push(item_for_host(host, match_result, &annotations).into());
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

/// Load all annotations into an alias-keyed map. Returns an empty map on
/// failure — annotations are decorative, missing them shouldn't block the
/// picker.
fn load_annotations() -> HashMap<String, HostAnnotation> {
    #[cfg(feature = "local_fs")]
    {
        let Some(db_url) = crate::persistence::database_file_path()
            .to_str()
            .map(str::to_owned)
        else {
            return HashMap::new();
        };
        match crate::persistence::establish_ro_connection(&db_url) {
            Ok(mut conn) => match annotation_repo::list_all(&mut conn) {
                Ok(entries) => entries.into_iter().collect(),
                Err(err) => {
                    log::warn!("ssh_manager: annotation list_all failed: {err:#}");
                    HashMap::new()
                }
            },
            Err(err) => {
                log::warn!("ssh_manager: annotation RO connection failed: {err:#}");
                HashMap::new()
            }
        }
    }
    #[cfg(not(feature = "local_fs"))]
    {
        HashMap::new()
    }
}

fn item_for_host(
    host: SshHost,
    match_result: FuzzyMatchResult,
    annotations: &HashMap<String, HostAnnotation>,
) -> SshSearchItem {
    let annotation = annotations.get(&host.alias).cloned();
    SshSearchItem {
        alias: host.alias,
        hostname: host.hostname,
        annotation,
        match_result,
    }
}
