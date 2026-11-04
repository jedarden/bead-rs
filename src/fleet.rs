//! Read-only, one-shot view of native workspaces below a filesystem root.
//!
//! Fleet deliberately does not use the normal store opener: that opener may
//! migrate a database and configure WAL. Every connection here is read-only.

use crate::error::{Error, Result};
use crate::store::identity::{self, SchemaShape};
use rusqlite::{Connection, OpenFlags};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::Duration;

const OBSERVATION_TIMEOUT: Duration = Duration::from_secs(15);

#[derive(Debug, Clone)]
pub enum FleetAction {
    Status,
    Ready,
    Search(String),
    Doctor,
}

#[derive(Debug, Clone)]
pub struct FleetOptions {
    pub root: PathBuf,
    pub max_depth: usize,
    pub include_copies: bool,
}

#[derive(Debug)]
struct Candidate {
    path: PathBuf,
    uuid: String,
    depth: usize,
    copy_like: bool,
}

#[derive(Default)]
struct Discovery {
    candidates: Vec<Candidate>,
    skipped: Vec<Value>,
    errors: Vec<Value>,
}

/// Produce a path-ordered fleet document. Workspace faults are represented in
/// `errors`; only invalid arguments or a failure to inspect the root fail the
/// command itself.
pub fn run(action: FleetAction, options: FleetOptions) -> Result<Value> {
    if matches!(&action, FleetAction::Search(text) if text.is_empty()) {
        return Err(Error::cli_usage("fleet search requires non-empty text"));
    }
    let root = fs::canonicalize(&options.root).map_err(|_| {
        Error::cli_usage(format!(
            "fleet root does not exist or cannot be read: {}",
            options.root.display()
        ))
    })?;
    if !root.is_dir() {
        return Err(Error::cli_usage(format!(
            "fleet root is not a directory: {}",
            root.display()
        )));
    }

    let mut discovery = Discovery::default();
    walk(&root, 0, options.max_depth, &mut discovery, true)?;
    discovery.candidates.sort_by(|a, b| a.path.cmp(&b.path));

    // The identity in config.json is the durable workspace identity. Grouping
    // precedes reads, so a broken member cannot cause an unrelated copy to
    // become canonical by accident.
    let mut groups: BTreeMap<String, Vec<usize>> = BTreeMap::new();
    for (index, candidate) in discovery.candidates.iter().enumerate() {
        groups
            .entry(candidate.uuid.clone())
            .or_default()
            .push(index);
    }
    let mut canonical: BTreeMap<usize, (usize, PathBuf)> = BTreeMap::new();
    let mut copies: BTreeMap<usize, (usize, PathBuf)> = BTreeMap::new();
    for members in groups.values() {
        let best = *members
            .iter()
            .min_by_key(|&&index| {
                let candidate = &discovery.candidates[index];
                (candidate.copy_like, candidate.depth, &candidate.path)
            })
            .expect("group has at least one member");
        let canonical_path = discovery.candidates[best].path.clone();
        canonical.insert(best, (0, canonical_path.clone()));
        let mut sequence = 1;
        for &index in members {
            if index != best {
                copies.insert(index, (sequence, canonical_path.clone()));
                sequence += 1;
            }
        }
    }

    let mut workspaces = Vec::new();
    let mut copy_records = Vec::new();
    let mut ready = Vec::new();
    let mut matches = Vec::new();
    let mut totals = Counts::default();

    for (index, candidate) in discovery.candidates.into_iter().enumerate() {
        let is_copy = copies.contains_key(&index);
        let (sequence, canonical_path) = canonical
            .get(&index)
            .or_else(|| copies.get(&index))
            .expect("every candidate has a group member");
        let mut record = json!({
            "path": candidate.path,
            "uuid": candidate.uuid,
            "sequence": sequence,
            "canonical_path": canonical_path,
            "selected": !is_copy || options.include_copies,
        });
        let mut skip_record = false;
        if !is_copy || options.include_copies {
            match observe_bounded(
                candidate.path.clone(),
                candidate.uuid.clone(),
                action.clone(),
            ) {
                Ok(observation) => {
                    let counts = &observation["counts"];
                    totals.add(
                        counts,
                        observation["ready_count"].as_u64().unwrap_or(0),
                        observation["stale_in_progress_count"].as_u64().unwrap_or(0),
                    );
                    if let Some(items) = observation["ready_issues"].as_array() {
                        ready.extend(items.iter().cloned());
                    }
                    if let Some(items) = observation["matches"].as_array() {
                        matches.extend(items.iter().cloned());
                    }
                    if let Some(fields) = observation.as_object() {
                        record
                            .as_object_mut()
                            .expect("record is object")
                            .extend(fields.clone());
                    }
                }
                Err(message) if message == "Integrity error: foreign_schema" => {
                    skip_record = true;
                    discovery.skipped.push(json!({
                        "path": candidate.path,
                        "uuid": candidate.uuid,
                        "reason": "foreign_schema",
                    }));
                }
                Err(message) => {
                    skip_record = true;
                    discovery.errors.push(json!({
                        "path": candidate.path,
                        "uuid": candidate.uuid,
                        "reason": message,
                    }));
                }
            }
        }
        if skip_record {
            continue;
        }
        if is_copy {
            copy_records.push(record);
        } else {
            workspaces.push(record);
        }
    }

    sort_by_path(&mut discovery.skipped);
    sort_by_path(&mut discovery.errors);
    let action_name = match &action {
        FleetAction::Status => "status",
        FleetAction::Ready => "ready",
        FleetAction::Search(_) => "search",
        FleetAction::Doctor => "doctor",
    };
    let mut aggregate = totals.value();
    aggregate["canonical_workspaces"] = json!(workspaces.len());
    aggregate["copy_workspaces"] = json!(copy_records.len());
    aggregate["skipped"] = json!(discovery.skipped.len());
    aggregate["errors"] = json!(discovery.errors.len());
    let mut document = json!({
        "root": root,
        "options": {"max_depth": options.max_depth, "include_copies": options.include_copies},
        "action": action_name,
        "workspaces": workspaces,
        "copies": copy_records,
        "skipped": discovery.skipped,
        "errors": discovery.errors,
        "totals": aggregate,
    });
    match action {
        FleetAction::Ready => document["ready"] = Value::Array(ready),
        FleetAction::Search(_) => document["matches"] = Value::Array(matches),
        FleetAction::Status | FleetAction::Doctor => {}
    }
    Ok(document)
}

fn sort_by_path(records: &mut [Value]) {
    records.sort_by(|a, b| a["path"].as_str().cmp(&b["path"].as_str()));
}

fn walk(
    path: &Path,
    depth: usize,
    max_depth: usize,
    found: &mut Discovery,
    root: bool,
) -> Result<()> {
    let entries = match fs::read_dir(path) {
        Ok(entries) => entries,
        Err(error) if root => {
            return Err(Error::Io {
                path: path.to_path_buf(),
                msg: error,
            })
        }
        Err(error) => {
            found
                .errors
                .push(json!({"path": path, "reason": format!("cannot list directory: {error}")}));
            return Ok(());
        }
    };
    let mut collected = Vec::new();
    for entry in entries {
        match entry {
            Ok(entry) => collected.push(entry),
            Err(error) if root => {
                return Err(Error::Io {
                    path: path.to_path_buf(),
                    msg: error,
                })
            }
            Err(error) => found.errors.push(json!({
                "path": path,
                "reason": format!("cannot read directory entry: {error}"),
            })),
        }
    }
    let mut entries = collected;
    entries.sort_by_key(|entry| entry.path());
    for entry in entries {
        let entry_path = entry.path();
        let ty = match entry.file_type() {
            Ok(ty) => ty,
            Err(error) => {
                found.errors.push(
                    json!({"path": entry_path, "reason": format!("cannot inspect entry: {error}")}),
                );
                continue;
            }
        };
        if entry.file_name() == ".beads" {
            if ty.is_dir() {
                classify_workspace(path, &entry_path, depth, found);
            } else {
                found
                    .skipped
                    .push(json!({"path": path, "reason": "unrecognized"}));
            }
            continue;
        }
        if ty.is_dir() && depth < max_depth {
            walk(&entry_path, depth + 1, max_depth, found, false)?;
        }
    }
    Ok(())
}

fn classify_workspace(path: &Path, beads: &Path, depth: usize, found: &mut Discovery) {
    let config_path = beads.join("config.json");
    let config_metadata = match fs::symlink_metadata(&config_path) {
        Ok(metadata) => Some(metadata),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => {
            found.errors.push(
                json!({"path": path, "reason": format!("cannot inspect config.json: {error}")}),
            );
            return;
        }
    };
    if config_metadata.is_none() {
        let reason = if beads.join("config.yaml").exists() {
            "bf_store"
        } else {
            "unrecognized"
        };
        found.skipped.push(json!({"path": path, "reason": reason}));
        return;
    }
    let config: Value = match fs::read(&config_path)
        .ok()
        .and_then(|raw| serde_json::from_slice(&raw).ok())
    {
        Some(config) => config,
        None => {
            found
                .errors
                .push(json!({"path": path, "reason": "config.json cannot be read or parsed"}));
            return;
        }
    };
    let Some(uuid) = config
        .get("uuid")
        .and_then(Value::as_str)
        .filter(|uuid| !uuid.is_empty())
    else {
        found
            .errors
            .push(json!({"path": path, "reason": "config.json lacks a workspace UUID"}));
        return;
    };
    let db_path = beads.join("beads.db");
    match fs::symlink_metadata(&db_path) {
        Ok(metadata) if !metadata.is_file() => {
            found.errors.push(
                json!({"path": path, "uuid": uuid, "reason": "beads.db is not a regular file"}),
            );
            return;
        }
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            found
                .skipped
                .push(json!({"path": path, "uuid": uuid, "reason": "uninitialized"}));
            return;
        }
        Err(error) => {
            found.errors.push(json!({"path": path, "uuid": uuid, "reason": format!("cannot inspect beads.db: {error}")}));
            return;
        }
    }
    found.candidates.push(Candidate {
        path: path.to_path_buf(),
        uuid: uuid.to_string(),
        depth,
        copy_like: is_copy_path(path, depth),
    });
}

fn is_copy_path(path: &Path, depth: usize) -> bool {
    path.components().rev().take(depth).any(|component| {
        let name = component.as_os_str().to_string_lossy();
        if name.starts_with('.')
            || name.contains("-verify")
            || name.contains("-committed")
            || name.contains(".tmp")
        {
            return true;
        }
        let Some((base, suffix)) = name.rsplit_once('.') else {
            return false;
        };
        !base.is_empty()
            && suffix.len() >= 6
            && suffix.chars().all(|c| c.is_ascii_alphanumeric())
            && suffix.chars().any(|c| c.is_ascii_lowercase())
            && suffix.chars().any(|c| c.is_ascii_uppercase())
    })
}

fn observe_bounded(
    path: PathBuf,
    uuid: String,
    action: FleetAction,
) -> std::result::Result<Value, String> {
    let (sender, receiver) = mpsc::sync_channel(1);
    std::thread::spawn(move || {
        let _ = sender.send(observe(&path, &uuid, &action).map_err(|error| error.to_string()));
    });
    receiver
        .recv_timeout(OBSERVATION_TIMEOUT)
        .map_err(|_| "observation timed out after 15 seconds".to_string())?
}

fn observe(path: &Path, configured_uuid: &str, action: &FleetAction) -> Result<Value> {
    let db_path = path.join(".beads/beads.db");
    // SQLite can update a WAL store's -shm file even with a read-only handle.
    // Read a disposable snapshot so any sidecar activity stays off the source.
    // The copied WAL carries committed transactions awaiting a checkpoint.
    let snapshot = tempfile::tempdir().map_err(|msg| Error::Io {
        path: std::env::temp_dir(),
        msg,
    })?;
    let snapshot_db = snapshot.path().join("beads.db");
    fs::copy(&db_path, &snapshot_db).map_err(|msg| Error::Io {
        path: db_path.clone(),
        msg,
    })?;
    let source_wal = db_path.with_file_name("beads.db-wal");
    let snapshot_wal = snapshot.path().join("beads.db-wal");
    match fs::copy(&source_wal, &snapshot_wal) {
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(msg) => {
            return Err(Error::Io {
                path: source_wal,
                msg,
            })
        }
    }
    let conn = Connection::open_with_flags(&snapshot_db, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .map_err(|error| {
            Error::Internal(anyhow::anyhow!("cannot open beads.db read-only: {error}"))
        })?;
    conn.busy_timeout(Duration::from_secs(2))?;
    match identity::classify_schema_shape(&conn)? {
        SchemaShape::Native => {}
        SchemaShape::Empty | SchemaShape::Foreign => {
            return Err(Error::integrity("foreign_schema"))
        }
    }
    identity::verify_format_identity(&conn)?;
    let db_uuid: String = conn
        .query_row("SELECT uuid FROM workspace WHERE id = 1", [], |row| {
            row.get(0)
        })
        .map_err(|error| Error::integrity(format!("cannot read workspace UUID: {error}")))?;
    if db_uuid != configured_uuid {
        return Err(Error::integrity(format!(
            "config/database workspace UUID mismatch: {configured_uuid} != {db_uuid}"
        )));
    }

    let mut counts = json!({"open": 0, "in_progress": 0, "deferred": 0, "closed": 0});
    let mut stmt = conn.prepare("SELECT base_status, COUNT(*) FROM issues GROUP BY base_status")?;
    let rows = stmt.query_map([], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
    })?;
    for row in rows {
        let (status, count) = row?;
        if counts.get(&status).is_some() {
            counts[&status] = json!(count);
        }
    }
    let ready_count =
        crate::service::issues::count_issues(&conn, None, None, true, false, None)? as u64;
    let stale_count =
        crate::service::doctor::stale_in_progress_count_read_only(&conn, path)? as u64;
    let mut result = json!({
        "counts": counts,
        "ready_count": ready_count,
        "stale_in_progress_count": stale_count,
        "checkpoint_lag": null,
    });
    match action {
        FleetAction::Status => {}
        FleetAction::Ready => {
            result["ready_issues"] = Value::Array(query_ready(&conn, path, &db_uuid)?)
        }
        FleetAction::Search(text) => {
            result["matches"] = Value::Array(query_search(&conn, path, &db_uuid, text)?)
        }
        FleetAction::Doctor => {
            let mut check = conn.prepare("PRAGMA quick_check")?;
            let findings = check
                .query_map([], |row| row.get::<_, String>(0))?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            let healthy = findings.len() == 1 && findings[0] == "ok";
            result["doctor"] = json!({
                "schema_shape": "native",
                "format_identity": "ok",
                "quick_check": findings,
                "healthy": healthy,
            });
        }
    }
    Ok(result)
}

const READY_WHERE: &str = "i.base_status = 'open' AND (i.manual_blocked IS NULL OR i.manual_blocked = 0) AND i.assignee IS NULL
    AND NOT EXISTS (SELECT 1 FROM dependencies d JOIN issues blocker ON blocker.id = d.blocker_issue_id
                    WHERE d.blocked_issue_id = i.id AND d.kind = 'blocks' AND blocker.base_status != 'closed')
    AND NOT EXISTS (SELECT 1 FROM issue_resource_keys candidate_key
                    JOIN resource_locks held_lock ON held_lock.resource_key = candidate_key.resource_key
                    WHERE candidate_key.issue_id = i.id AND held_lock.issue_id != i.id
                      AND (held_lock.lease_fencing_token IS NULL OR EXISTS (
                          SELECT 1 FROM leases active_lease
                          WHERE active_lease.issue_id = held_lock.issue_id
                            AND active_lease.fencing_token = held_lock.lease_fencing_token
                            AND active_lease.expires_at > ?1)))";

fn query_ready(conn: &Connection, path: &Path, uuid: &str) -> Result<Vec<Value>> {
    let sql = format!("SELECT i.id, i.title, i.priority, i.base_status FROM issues i WHERE {READY_WHERE} ORDER BY i.priority, i.created_at, i.id");
    let now = crate::service::resource_locks::now_string();
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map([now], |row| {
        Ok(json!({
            "workspace_path": path, "workspace_uuid": uuid,
            "id": row.get::<_, String>(0)?, "title": row.get::<_, String>(1)?,
            "priority": row.get::<_, i64>(2)?, "base_status": row.get::<_, String>(3)?,
        }))
    })?;
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(Into::into)
}

fn query_search(conn: &Connection, path: &Path, uuid: &str, text: &str) -> Result<Vec<Value>> {
    let escaped = text
        .replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_");
    let pattern = format!("%{escaped}%");
    let mut stmt = conn.prepare(
        "SELECT id, title, priority, base_status FROM issues
        WHERE LOWER(title) LIKE LOWER(?1) ESCAPE '\\'
           OR LOWER(COALESCE(description, '')) LIKE LOWER(?1) ESCAPE '\\'
           OR LOWER(COALESCE(notes, '')) LIKE LOWER(?1) ESCAPE '\\'
        ORDER BY id",
    )?;
    let rows = stmt.query_map([pattern], |row| {
        Ok(json!({
            "workspace_path": path, "workspace_uuid": uuid,
            "id": row.get::<_, String>(0)?, "title": row.get::<_, String>(1)?,
            "priority": row.get::<_, i64>(2)?, "base_status": row.get::<_, String>(3)?,
        }))
    })?;
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(Into::into)
}

#[derive(Default)]
struct Counts {
    open: u64,
    in_progress: u64,
    deferred: u64,
    closed: u64,
    ready: u64,
    stale: u64,
    observed: u64,
}

impl Counts {
    fn add(&mut self, counts: &Value, ready: u64, stale: u64) {
        self.open += counts["open"].as_u64().unwrap_or(0);
        self.in_progress += counts["in_progress"].as_u64().unwrap_or(0);
        self.deferred += counts["deferred"].as_u64().unwrap_or(0);
        self.closed += counts["closed"].as_u64().unwrap_or(0);
        self.ready += ready;
        self.stale += stale;
        self.observed += 1;
    }
    fn value(&self) -> Value {
        json!({"workspaces": self.observed, "counts": {
            "open": self.open, "in_progress": self.in_progress, "deferred": self.deferred, "closed": self.closed,
        }, "ready_count": self.ready, "stale_in_progress_count": self.stale})
    }
}
