//! Workspace-local resource declarations and claim locks (R031).
//!
//! Resource locks are deliberately scoped to one native SQLite store. They
//! serialize work selected by claims in this workspace; they are not leases,
//! distributed locks, or a coordination mechanism between stores.

use crate::error::{Error, Result};
use rusqlite::{Connection, OptionalExtension, Transaction};
use serde_json::Value;
use time::OffsetDateTime;

/// The issue extension used by native checkpoint JSON for declared keys.
pub const RESOURCE_KEYS_EXTENSION: &str = "resource_keys";

/// Stable machine-readable readiness reason for a held local resource.
pub const RESOURCE_CONFLICT_REASON_CODE: &str = "resource_conflict";

/// Maximum encoded length of one normalized resource key.
pub const MAX_RESOURCE_KEY_BYTES: usize = 255;

/// Normalize and validate one local resource key.
///
/// Normalization trims Unicode whitespace at both ends. Keys remain
/// case-sensitive because local resources such as paths, sockets, and device
/// names may be case-sensitive. Control characters, including NUL, are
/// rejected and the normalized key must be at most 255 UTF-8 bytes.
pub fn normalize_resource_key(raw: &str) -> Result<String> {
    if raw.chars().any(char::is_control) {
        return Err(Error::validation(
            "Resource key cannot contain control characters",
        ));
    }
    let key = raw.trim();
    if key.is_empty() {
        return Err(Error::validation("Resource key cannot be empty"));
    }
    if key.len() > MAX_RESOURCE_KEY_BYTES {
        return Err(Error::validation(format!(
            "Resource key cannot exceed {} bytes",
            MAX_RESOURCE_KEY_BYTES
        )));
    }
    Ok(key.to_string())
}

/// Normalize a declaration, returning a stable lexical order.
///
/// Repeating the same key after normalization is rejected rather than
/// silently changing the caller's declaration. This catches accidental
/// duplicate command arguments and keeps the mutation auditable.
pub fn normalize_resource_keys<I, S>(raw_keys: I) -> Result<Vec<String>>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let mut keys = raw_keys
        .into_iter()
        .map(|raw| normalize_resource_key(raw.as_ref()))
        .collect::<Result<Vec<_>>>()?;
    keys.sort();
    for pair in keys.windows(2) {
        if pair[0] == pair[1] {
            return Err(Error::validation(format!(
                "Resource key declared more than once: {}",
                pair[0]
            )));
        }
    }
    Ok(keys)
}

/// Validate the JSON projection used by checkpoint import.
pub fn resource_keys_from_value(value: &Value) -> Result<Vec<String>> {
    let array = value.as_array().ok_or_else(|| {
        Error::validation(format!(
            "{} must be a JSON array of strings",
            RESOURCE_KEYS_EXTENSION
        ))
    })?;
    let keys = array
        .iter()
        .map(|value| {
            value.as_str().map(str::to_string).ok_or_else(|| {
                Error::validation(format!(
                    "{} must contain only strings",
                    RESOURCE_KEYS_EXTENSION
                ))
            })
        })
        .collect::<Result<Vec<_>>>()?;
    normalize_resource_keys(keys)
}

/// Read a deterministic declaration from the native store.
pub fn get_resource_keys(conn: &Connection, issue_id: &str) -> Result<Vec<String>> {
    let mut stmt = conn.prepare(
        "SELECT resource_key FROM issue_resource_keys
         WHERE issue_id = ?1 ORDER BY resource_key ASC",
    )?;
    let keys = stmt
        .query_map([issue_id], |row| row.get::<_, String>(0))?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(keys)
}

/// Store a declaration on a newly created or imported issue.
///
/// The caller supplies the connection that owns the surrounding transaction;
/// this helper does not open or commit a nested transaction.
pub fn declare_resource_keys(conn: &Connection, issue_id: &str, raw_keys: &[String]) -> Result<()> {
    let keys = normalize_resource_keys(raw_keys.iter().map(String::as_str))?;
    conn.execute(
        "DELETE FROM issue_resource_keys WHERE issue_id = ?1",
        [issue_id],
    )?;
    for key in keys {
        conn.execute(
            "INSERT INTO issue_resource_keys (issue_id, resource_key)
             VALUES (?1, ?2)",
            [issue_id, &key],
        )?;
    }
    Ok(())
}

/// Add a resource declaration to an issue inside the caller's transaction.
pub fn add_resource_keys(
    tx: &Transaction,
    issue_id: &str,
    raw_keys: &[String],
    fencing_token: Option<i64>,
) -> Result<Vec<String>> {
    let mut keys = get_resource_keys(tx, issue_id)?;
    let additions = normalize_resource_keys(raw_keys.iter().map(String::as_str))?;
    for key in additions {
        if !keys.contains(&key) {
            keys.push(key);
        }
    }
    let keys = normalize_resource_keys(keys)?;
    set_resource_keys(tx, issue_id, &keys, fencing_token)?;
    Ok(keys)
}

/// Remove resource declarations from an issue inside the caller's transaction.
pub fn remove_resource_keys(
    tx: &Transaction,
    issue_id: &str,
    raw_keys: &[String],
    fencing_token: Option<i64>,
) -> Result<Vec<String>> {
    let remove = normalize_resource_keys(raw_keys)?;
    let current = get_resource_keys(tx, issue_id)?;
    let remaining = current
        .into_iter()
        .filter(|key| !remove.contains(key))
        .collect::<Vec<_>>();
    set_resource_keys(tx, issue_id, &remaining, fencing_token)?;
    Ok(remaining)
}

/// Add keys and append an auditable declaration event.
pub fn add_resource_keys_with_event(
    tx: &Transaction,
    issue_id: &str,
    raw_keys: &[String],
    fencing_token: Option<i64>,
    actor: &str,
) -> Result<Vec<String>> {
    let before = get_resource_keys(tx, issue_id)?;
    let keys = add_resource_keys(tx, issue_id, raw_keys, fencing_token)?;
    if before != keys {
        append_resource_event(tx, issue_id, "resource_keys_added", &keys, actor)?;
    }
    Ok(keys)
}

/// Remove keys and append an auditable declaration event.
pub fn remove_resource_keys_with_event(
    tx: &Transaction,
    issue_id: &str,
    raw_keys: &[String],
    fencing_token: Option<i64>,
    actor: &str,
) -> Result<Vec<String>> {
    let before = get_resource_keys(tx, issue_id)?;
    let keys = remove_resource_keys(tx, issue_id, raw_keys, fencing_token)?;
    if before != keys {
        append_resource_event(tx, issue_id, "resource_keys_removed", &keys, actor)?;
    }
    Ok(keys)
}

/// Replace an issue's declaration inside an existing transaction.
pub fn set_resource_keys(
    tx: &Transaction,
    issue_id: &str,
    raw_keys: &[String],
    fencing_token: Option<i64>,
) -> Result<()> {
    let keys = normalize_resource_keys(raw_keys.iter().map(String::as_str))?;
    let issue_state: Option<(String, Option<String>)> = tx
        .query_row(
            "SELECT base_status, assignee FROM issues WHERE id = ?1",
            [issue_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?;
    let Some((status, assignee)) = issue_state else {
        return Err(Error::not_found(format!("Issue not found: {}", issue_id)));
    };

    if let Some(assignee) = assignee.as_deref() {
        crate::service::validate_lease_for_mutation(tx, issue_id, assignee, fencing_token)?;
    }

    // Declaration and lock changes share the caller's transaction. A
    // conflicting replacement therefore rolls back both the declaration and
    // any lock rows released before reacquisition.
    tx.execute(
        "DELETE FROM issue_resource_keys WHERE issue_id = ?1",
        [issue_id],
    )?;
    for key in &keys {
        tx.execute(
            "INSERT INTO issue_resource_keys (issue_id, resource_key)
             VALUES (?1, ?2)",
            [issue_id, key],
        )?;
    }

    release_issue_locks(tx, issue_id)?;
    if status == "in_progress" && assignee.is_some() {
        acquire_issue_locks(tx, issue_id, active_lease_token(tx, issue_id)?)?;
    }
    Ok(())
}

/// Return keys currently held by an issue's active claim.
pub fn acquire_issue_locks(
    tx: &Transaction,
    issue_id: &str,
    lease_fencing_token: Option<i64>,
) -> Result<()> {
    let keys = get_resource_keys(tx, issue_id)?;
    let now = now_string();
    let mut existing_keys = Vec::new();
    for key in &keys {
        let existing: Option<(String, Option<i64>)> = tx
            .query_row(
                "SELECT issue_id, lease_fencing_token FROM resource_locks
                 WHERE resource_key = ?1",
                [key],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;
        if let Some((owner, _)) = existing {
            if owner != issue_id {
                return Err(Error::conflict(format!(
                    "{}: resource key '{}' is held by issue '{}' in this workspace",
                    RESOURCE_CONFLICT_REASON_CODE, key, owner
                )));
            }
            existing_keys.push(key.clone());
        }
    }
    for key in keys {
        if existing_keys.contains(&key) {
            continue;
        }
        tx.execute(
            "INSERT INTO resource_locks
                 (resource_key, issue_id, lease_fencing_token, acquired_at)
             VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![key, issue_id, lease_fencing_token, now],
        )?;
    }
    Ok(())
}

/// Release every key held by an issue.
pub fn release_issue_locks(tx: &Transaction, issue_id: &str) -> Result<()> {
    tx.execute("DELETE FROM resource_locks WHERE issue_id = ?1", [issue_id])?;
    Ok(())
}

/// Reconcile an issue's active lock rows with its current lifecycle state.
pub fn sync_issue_locks(tx: &Transaction, issue_id: &str) -> Result<()> {
    let state: Option<(String, Option<String>)> = tx
        .query_row(
            "SELECT base_status, assignee FROM issues WHERE id = ?1",
            [issue_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?;
    let Some((status, assignee)) = state else {
        return Err(Error::not_found(format!("Issue not found: {}", issue_id)));
    };
    release_issue_locks(tx, issue_id)?;
    if status == "in_progress" && assignee.is_some() {
        acquire_issue_locks(tx, issue_id, active_lease_token(tx, issue_id)?)?;
    }
    Ok(())
}

/// Update lock metadata when a lease is renewed.
pub fn update_issue_lock_lease_token(
    tx: &Transaction,
    issue_id: &str,
    lease_fencing_token: i64,
) -> Result<()> {
    tx.execute(
        "UPDATE resource_locks SET lease_fencing_token = ?1 WHERE issue_id = ?2",
        rusqlite::params![lease_fencing_token, issue_id],
    )?;
    Ok(())
}

/// Return keys whose lease epoch has expired.
pub fn release_expired_resource_locks(tx: &Transaction) -> Result<()> {
    let now = now_string();
    tx.execute(
        "DELETE FROM resource_locks
         WHERE lease_fencing_token IS NOT NULL
           AND EXISTS (
               SELECT 1 FROM leases l
               WHERE l.issue_id = resource_locks.issue_id
                 AND l.fencing_token = resource_locks.lease_fencing_token
                 AND l.expires_at <= ?1
           )",
        [&now],
    )?;
    Ok(())
}

/// Count effective resource conflicts for a candidate or explanation.
pub fn resource_conflict_count(conn: &Connection, issue_id: &str) -> Result<i64> {
    let now = now_string();
    Ok(conn.query_row(
        "SELECT COUNT(*) FROM issue_resource_keys k
         JOIN resource_locks l ON l.resource_key = k.resource_key
         WHERE k.issue_id = ?1 AND l.issue_id != k.issue_id
           AND (l.lease_fencing_token IS NULL OR EXISTS (
               SELECT 1 FROM leases lease
               WHERE lease.issue_id = l.issue_id
                 AND lease.fencing_token = l.lease_fencing_token
                 AND lease.expires_at > ?2
           ))",
        rusqlite::params![issue_id, now],
        |row| row.get(0),
    )?)
}

/// Return the current active lease token for an issue, if any.
fn active_lease_token(tx: &Transaction, issue_id: &str) -> Result<Option<i64>> {
    let now = now_string();
    Ok(tx
        .query_row(
            "SELECT fencing_token FROM leases
             WHERE issue_id = ?1 AND expires_at > ?2
             ORDER BY fencing_token DESC
             LIMIT 1",
            rusqlite::params![issue_id, now],
            |row| row.get(0),
        )
        .optional()?)
}

pub(crate) fn now_string() -> String {
    OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_else(|_| "unknown".to_string())
}

fn append_resource_event(
    tx: &Transaction,
    issue_id: &str,
    kind: &str,
    keys: &[String],
    actor: &str,
) -> Result<()> {
    let detail = serde_json::json!({
        "resource_keys": keys,
        "workspace_local": true,
    });
    tx.execute(
        "INSERT INTO events (issue_id, kind, actor, time, detail)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        rusqlite::params![issue_id, kind, actor, now_string(), detail.to_string()],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_and_orders_keys() {
        assert_eq!(
            normalize_resource_keys([" z ", "a"]).unwrap(),
            vec!["a", "z"]
        );
    }

    #[test]
    fn rejects_empty_control_duplicate_and_oversized_keys() {
        assert!(normalize_resource_key("  ").is_err());
        assert!(normalize_resource_key("a\n").is_err());
        assert!(normalize_resource_keys(["a", " a "]).is_err());
        assert!(normalize_resource_key(&"x".repeat(MAX_RESOURCE_KEY_BYTES + 1)).is_err());
    }
}
