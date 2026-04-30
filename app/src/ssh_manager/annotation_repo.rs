//! Persistence for [`HostAnnotation`] in the Warp SQLite DB.
//!
//! Annotations are keyed by ssh_config alias (logical FK; no DB-level FK
//! since ssh_config lives outside the DB). The caller passes a
//! [`SqliteConnection`] — connection management is handled by the wider
//! persistence subsystem.

use chrono::{TimeZone, Utc};
use diesel::prelude::*;
use diesel::result::Error as DieselError;
use diesel::SqliteConnection;

use crate::persistence::schema::ssh_host_annotations;
use crate::ssh_manager::model::{ColorSlug, HostAnnotation};

#[derive(Debug, Queryable)]
struct AnnotationRow {
    alias: String,
    tags: String,
    color: Option<String>,
    notes: Option<String>,
    last_connected_at: Option<i64>,
    #[allow(dead_code)]
    created_at: i64,
    #[allow(dead_code)]
    updated_at: i64,
}

#[derive(Debug, Insertable, AsChangeset)]
#[diesel(table_name = ssh_host_annotations)]
struct AnnotationChange {
    alias: String,
    tags: String,
    color: Option<String>,
    notes: Option<String>,
    last_connected_at: Option<i64>,
    created_at: i64,
    updated_at: i64,
}

/// Insert or update the annotation for `alias`. The whole annotation is
/// replaced — partial updates aren't supported (annotations are tiny).
pub fn upsert(
    conn: &mut SqliteConnection,
    alias: &str,
    annotation: &HostAnnotation,
) -> Result<(), DieselError> {
    use ssh_host_annotations::dsl;

    let now = Utc::now().timestamp();
    let change = AnnotationChange {
        alias: alias.to_owned(),
        tags: serde_json::to_string(&annotation.tags).unwrap_or_else(|_| "[]".to_owned()),
        color: annotation.color.map(|c| c.as_slug().to_owned()),
        notes: annotation.notes.clone(),
        last_connected_at: annotation.last_connected_at.map(|dt| dt.timestamp()),
        created_at: now,
        updated_at: now,
    };

    diesel::insert_into(ssh_host_annotations::table)
        .values(&change)
        .on_conflict(dsl::alias)
        .do_update()
        .set(&change)
        .execute(conn)?;

    Ok(())
}

/// Fetch a single annotation, returning `Ok(None)` if no row exists.
pub fn get(
    conn: &mut SqliteConnection,
    alias_param: &str,
) -> Result<Option<HostAnnotation>, DieselError> {
    use ssh_host_annotations::dsl;

    let row: Option<AnnotationRow> = ssh_host_annotations::table
        .filter(dsl::alias.eq(alias_param))
        .first::<AnnotationRow>(conn)
        .optional()?;

    Ok(row.map(row_to_annotation))
}

/// Fetch every annotation as `(alias, annotation)`. Used for joining onto the
/// parsed ssh_config host list.
pub fn list_all(
    conn: &mut SqliteConnection,
) -> Result<Vec<(String, HostAnnotation)>, DieselError> {
    let rows: Vec<AnnotationRow> = ssh_host_annotations::table.load::<AnnotationRow>(conn)?;
    Ok(rows
        .into_iter()
        .map(|row| {
            let alias = row.alias.clone();
            (alias, row_to_annotation(row))
        })
        .collect())
}

/// Bump `last_connected_at` to now. If no row exists yet, inserts a blank one
/// (no tags / color / notes) so the timestamp can be recorded.
pub fn record_connection(
    conn: &mut SqliteConnection,
    alias_param: &str,
) -> Result<(), DieselError> {
    let mut existing = get(conn, alias_param)?.unwrap_or_default();
    existing.last_connected_at = Some(Utc::now());
    upsert(conn, alias_param, &existing)
}

/// Delete the row for `alias`. Returns the number of rows deleted (0 or 1).
pub fn delete(conn: &mut SqliteConnection, alias_param: &str) -> Result<usize, DieselError> {
    use ssh_host_annotations::dsl;
    diesel::delete(ssh_host_annotations::table.filter(dsl::alias.eq(alias_param))).execute(conn)
}

fn row_to_annotation(row: AnnotationRow) -> HostAnnotation {
    let tags: Vec<String> = serde_json::from_str(&row.tags).unwrap_or_default();
    let color = row.color.as_deref().and_then(ColorSlug::from_slug);
    let last_connected_at = row.last_connected_at.and_then(|secs| {
        Utc.timestamp_opt(secs, 0).single()
    });
    HostAnnotation {
        tags,
        color,
        notes: row.notes,
        last_connected_at,
    }
}
