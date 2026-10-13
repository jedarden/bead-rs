//! Public comment mutation operations (plan R010, comment-mutation-v1).
//!
//! Four CLI commands (`comment create|reply|resolve|unresolve`) share the
//! existing internal comment storage: one `comments` row plus one audit event
//! committed atomically through the shared `atomic_write` gate. Bodies are
//! immutable once written; resolution state is the only mutable column, and
//! every real transition appends its event so the full resolution history is
//! reconstructible from the stream. No-op transitions emit nothing.

use crate::error::{Error, Result};
use crate::service::read_live_event_sequence;
use crate::service::secret_boundary::{self, guard_pairs, selector};
use rusqlite::{Connection, OptionalExtension};
use serde::Serialize;

/// The model's long-text cap (schema `maxLength` for nullable text fields).
const MAX_BODY_BYTES: usize = 4194304;

/// The shared identifier/label cap for the resolved actor.
const MAX_ACTOR_BYTES: usize = 255;

/// Comment ID collision retries before failing as an integrity error.
const MAX_ID_ATTEMPTS: usize = 5;

/// The explicit state a native comment always stores. NULL is the
/// import-compatibility spelling and reads as this value everywhere.
const UNRESOLVED: &str = "unresolved";
const RESOLVED: &str = "resolved";

/// Result envelope for one comment mutation. The body is deliberately absent:
/// its durable, scanned, redactable home is the `comments` row, never a
/// result surface.
#[derive(Debug)]
pub struct CommentMutationReport {
    /// Past-tense operation matching event-kind vocabulary:
    /// `created | replied | resolved | unresolved`.
    pub operation: &'static str,
    /// Affected comment ID (new for create/reply, target for resolve/unresolve).
    pub id: String,
    pub issue_id: String,
    /// Reply target. Serialized on `created` (null) and `replied` envelopes;
    /// absent from resolution envelopes.
    pub reply_to_id: Option<String>,
    /// The resolved actor of this mutation. On resolution commands this is
    /// recorded in the audit event only; the comment's stored author never
    /// changes.
    pub author: String,
    /// Resolution state after the operation, normalized (NULL reads as
    /// `unresolved`).
    pub resolution_state: &'static str,
    /// Stored state before the operation. Serialized only on resolution
    /// envelopes; null when the stored state was the import-compatibility
    /// NULL.
    pub previous_resolution_state: Option<String>,
    /// False exactly for no-op transitions and dry-runs of no-ops.
    pub semantic_change: bool,
    /// `MAX(events.sequence)` after commit in real runs; the current maximum
    /// in dry-runs.
    pub workspace_sequence: i64,
}

impl CommentMutationReport {
    /// Resolution envelopes (`resolved`/`unresolved`) carry a different key
    /// set than creation envelopes: the reply target is absent and the
    /// previous resolution state appears.
    fn is_resolution(&self) -> bool {
        matches!(self.operation, "resolved" | "unresolved")
    }
}

impl Serialize for CommentMutationReport {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let resolution = self.is_resolution();
        let mut map = serde_json::Map::new();
        map.insert(
            "operation".into(),
            serde_json::Value::String(self.operation.into()),
        );
        map.insert("id".into(), serde_json::Value::String(self.id.clone()));
        map.insert(
            "issue_id".into(),
            serde_json::Value::String(self.issue_id.clone()),
        );
        if !resolution {
            map.insert(
                "reply_to_id".into(),
                match &self.reply_to_id {
                    Some(target) => serde_json::Value::String(target.clone()),
                    None => serde_json::Value::Null,
                },
            );
        }
        map.insert(
            "author".into(),
            serde_json::Value::String(self.author.clone()),
        );
        map.insert(
            "resolution_state".into(),
            serde_json::Value::String(self.resolution_state.into()),
        );
        if resolution {
            map.insert(
                "previous_resolution_state".into(),
                match &self.previous_resolution_state {
                    Some(state) => serde_json::Value::String(state.clone()),
                    None => serde_json::Value::Null,
                },
            );
        }
        map.insert(
            "semantic_change".into(),
            serde_json::Value::Bool(self.semantic_change),
        );
        map.insert(
            "workspace_sequence".into(),
            serde_json::Value::from(self.workspace_sequence),
        );
        map.serialize(serializer)
    }
}

/// BR-T12 attribution ladder: `--actor` when present and nonempty after
/// trimming, then `BEAD_ACTOR`, then `system`. The winner is stored verbatim.
pub fn resolve_actor(explicit: Option<&str>) -> Result<String> {
    let from_flag = explicit.filter(|actor| !actor.trim().is_empty());
    let from_env = std::env::var("BEAD_ACTOR")
        .ok()
        .filter(|actor| !actor.trim().is_empty());
    let actor = from_flag.or(from_env.as_deref()).unwrap_or("system");
    if actor.len() > MAX_ACTOR_BYTES {
        return Err(Error::cli_usage(format!(
            "Comment actor cannot exceed {MAX_ACTOR_BYTES} bytes"
        )));
    }
    Ok(actor.to_string())
}

/// RFC 3339 UTC instant from the shared timestamp idiom.
fn now_rfc3339() -> String {
    time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_else(|_| "unknown".to_string())
}

/// V2: the issue selector must name an existing issue (exit 3).
fn require_issue(conn: &Connection, issue_id: &str) -> Result<()> {
    let exists: bool = conn
        .query_row(
            "SELECT COUNT(*) > 0 FROM issues WHERE id = ?1",
            [issue_id],
            |row| row.get(0),
        )
        .unwrap_or(false);
    if !exists {
        return Err(Error::not_found(format!("Issue {issue_id} not found")));
    }
    Ok(())
}

/// V4/V5: the stored body is never trimmed or rewritten; emptiness is checked
/// after trimming for validation only.
fn validate_body(body: &str) -> Result<()> {
    if body.trim().is_empty() {
        return Err(Error::cli_usage("Comment body cannot be empty"));
    }
    if body.len() > MAX_BODY_BYTES {
        return Err(Error::cli_usage(format!(
            "Comment body cannot exceed {MAX_BODY_BYTES} bytes"
        )));
    }
    Ok(())
}

/// V8/V9: the target comment must exist and belong to the issue selector
/// (exit 4). Returns the raw stored resolution state so the import-compat
/// NULL stays distinguishable from an explicit `unresolved`.
fn require_comment_on_issue(
    conn: &Connection,
    issue_id: &str,
    comment_id: &str,
) -> Result<Option<String>> {
    let stored: Option<Option<String>> = conn
        .query_row(
            "SELECT resolution_state FROM comments WHERE id = ?1 AND issue_id = ?2",
            rusqlite::params![comment_id, issue_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(Error::from)?;
    stored.ok_or_else(|| {
        Error::conflict(format!(
            "Comment {comment_id} not found on issue {issue_id}"
        ))
    })
}

/// V10: a generated comment ID must not collide with any existing
/// `comments.id`; retried, then an integrity error.
fn generate_unique_comment_id(conn: &Connection) -> Result<String> {
    for _ in 0..MAX_ID_ATTEMPTS {
        let random_bytes: [u8; 4] = rand::random();
        let comment_id = format!("comment-{}", hex::encode(random_bytes));
        let exists: Option<String> = conn
            .query_row(
                "SELECT id FROM comments WHERE id = ?1",
                [&comment_id],
                |row| row.get(0),
            )
            .optional()
            .map_err(Error::from)?;
        if exists.is_none() {
            return Ok(comment_id);
        }
    }
    Err(Error::integrity(
        "comment ID generation exhausted its retry budget",
    ))
}

fn insert_comment(
    conn: &Connection,
    comment_id: &str,
    issue_id: &str,
    author: &str,
    body: &str,
    reply_to_id: Option<&str>,
) -> Result<()> {
    // Native creation stores `resolution_state` explicitly; imported NULL
    // remains legal and reads as unresolved.
    conn.execute(
        "INSERT INTO comments (id, issue_id, author, body, reply_to_id, resolution_state, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        rusqlite::params![
            comment_id,
            issue_id,
            author,
            body,
            reply_to_id,
            UNRESOLVED,
            now_rfc3339()
        ],
    )?;
    Ok(())
}

fn insert_comment_event(
    conn: &Connection,
    issue_id: &str,
    kind: &str,
    actor: &str,
    detail: serde_json::Value,
) -> Result<()> {
    conn.execute(
        "INSERT INTO events (issue_id, kind, actor, time, detail) VALUES (?1, ?2, ?3, ?4, ?5)",
        rusqlite::params![issue_id, kind, actor, now_rfc3339(), detail.to_string()],
    )?;
    Ok(())
}

/// `comment create`: one root comment with a generated ID and one
/// `comment_added` event, committed atomically.
pub fn create_comment(
    conn: &Connection,
    issue_id: &str,
    body: &str,
    actor: &str,
    dry_run: bool,
) -> Result<CommentMutationReport> {
    require_issue(conn, issue_id)?;
    validate_body(body)?;
    if dry_run {
        let comment_id = generate_unique_comment_id(conn)?;
        return Ok(CommentMutationReport {
            operation: "created",
            id: comment_id,
            issue_id: issue_id.to_string(),
            reply_to_id: None,
            author: actor.to_string(),
            resolution_state: UNRESOLVED,
            previous_resolution_state: None,
            semantic_change: true,
            workspace_sequence: read_live_event_sequence(conn).unwrap_or(0),
        });
    }
    let write_guard = guard_pairs(
        conn,
        &selector("issue", issue_id),
        actor,
        &[("id", issue_id), ("body", body), ("author", actor)],
    )?;
    let report = secret_boundary::atomic_write(conn, || {
        let comment_id = generate_unique_comment_id(conn)?;
        insert_comment(conn, &comment_id, issue_id, actor, body, None)?;
        insert_comment_event(
            conn,
            issue_id,
            "comment_added",
            actor,
            serde_json::json!({"comment_id": comment_id}),
        )?;
        Ok(CommentMutationReport {
            operation: "created",
            id: comment_id,
            issue_id: issue_id.to_string(),
            reply_to_id: None,
            author: actor.to_string(),
            resolution_state: UNRESOLVED,
            previous_resolution_state: None,
            semantic_change: true,
            workspace_sequence: read_live_event_sequence(conn).unwrap_or(0),
        })
    });
    drop(write_guard);
    report
}

/// `comment reply`: one reply comment whose `reply_to_id` names an existing
/// comment on the issue selector, with one `comment_added` event carrying
/// both IDs. Depth is unbounded and resolution state is never consulted.
pub fn reply_comment(
    conn: &Connection,
    issue_id: &str,
    reply_to_id: &str,
    body: &str,
    actor: &str,
    dry_run: bool,
) -> Result<CommentMutationReport> {
    require_issue(conn, issue_id)?;
    validate_body(body)?;
    require_comment_on_issue(conn, issue_id, reply_to_id)?;
    if dry_run {
        let comment_id = generate_unique_comment_id(conn)?;
        return Ok(CommentMutationReport {
            operation: "replied",
            id: comment_id,
            issue_id: issue_id.to_string(),
            reply_to_id: Some(reply_to_id.to_string()),
            author: actor.to_string(),
            resolution_state: UNRESOLVED,
            previous_resolution_state: None,
            semantic_change: true,
            workspace_sequence: read_live_event_sequence(conn).unwrap_or(0),
        });
    }
    let write_guard = guard_pairs(
        conn,
        &selector("issue", issue_id),
        actor,
        &[
            ("id", issue_id),
            ("body", body),
            ("author", actor),
            ("reply_to_id", reply_to_id),
        ],
    )?;
    let report = secret_boundary::atomic_write(conn, || {
        let comment_id = generate_unique_comment_id(conn)?;
        insert_comment(conn, &comment_id, issue_id, actor, body, Some(reply_to_id))?;
        insert_comment_event(
            conn,
            issue_id,
            "comment_added",
            actor,
            serde_json::json!({"comment_id": comment_id, "reply_to_id": reply_to_id}),
        )?;
        Ok(CommentMutationReport {
            operation: "replied",
            id: comment_id,
            issue_id: issue_id.to_string(),
            reply_to_id: Some(reply_to_id.to_string()),
            author: actor.to_string(),
            resolution_state: UNRESOLVED,
            previous_resolution_state: None,
            semantic_change: true,
            workspace_sequence: read_live_event_sequence(conn).unwrap_or(0),
        })
    });
    drop(write_guard);
    report
}

/// One resolution direction: the target state, its past-tense operation
/// label, and its audit event kind travel together so the shared transition
/// machinery cannot mix a state with another direction's event.
struct ResolutionDirection {
    state: &'static str,
    operation: &'static str,
    event_kind: &'static str,
}

const RESOLVE: ResolutionDirection = ResolutionDirection {
    state: RESOLVED,
    operation: "resolved",
    event_kind: "comment_resolved",
};

const UNRESOLVE: ResolutionDirection = ResolutionDirection {
    state: UNRESOLVED,
    operation: "unresolved",
    event_kind: "comment_unresolved",
};

/// Shared resolution machinery for `resolve`/`unresolve`: last-writer-wins on
/// one comment's `resolution_state`, semantic transitions only emit their
/// event, no-ops emit nothing and still exit 0. Every invocation, no-op or
/// not, passes the shared write gate.
fn transition_resolution(
    conn: &Connection,
    issue_id: &str,
    comment_id: &str,
    direction: &ResolutionDirection,
    actor: &str,
    dry_run: bool,
) -> Result<CommentMutationReport> {
    require_issue(conn, issue_id)?;
    let stored = require_comment_on_issue(conn, issue_id, comment_id)?;
    // NULL reads as unresolved: the first transition writes the explicit
    // string, a matching unresolve is a no-op.
    let current = stored.as_deref().unwrap_or(UNRESOLVED);
    let semantic_change = current != direction.state;
    let workspace_sequence = if dry_run {
        read_live_event_sequence(conn).unwrap_or(0)
    } else {
        let write_guard = guard_pairs(
            conn,
            &selector("issue", issue_id),
            actor,
            &[("id", issue_id), ("comment", comment_id)],
        )?;
        let committed: Result<i64> = if semantic_change {
            secret_boundary::atomic_write(conn, || {
                conn.execute(
                    "UPDATE comments SET resolution_state = ?1 WHERE id = ?2 AND issue_id = ?3",
                    rusqlite::params![direction.state, comment_id, issue_id],
                )?;
                insert_comment_event(
                    conn,
                    issue_id,
                    direction.event_kind,
                    actor,
                    serde_json::json!({"comment_id": comment_id}),
                )?;
                Ok(read_live_event_sequence(conn).unwrap_or(0))
            })
        } else {
            // A no-op transition writes no row and emits no event, but still
            // passes the shared maintenance-hold and quarantine gates above.
            Ok(read_live_event_sequence(conn).unwrap_or(0))
        };
        drop(write_guard);
        committed?
    };
    Ok(CommentMutationReport {
        operation: direction.operation,
        id: comment_id.to_string(),
        issue_id: issue_id.to_string(),
        reply_to_id: None,
        author: actor.to_string(),
        resolution_state: direction.state,
        previous_resolution_state: stored,
        semantic_change,
        workspace_sequence,
    })
}

/// `comment resolve`: mark one comment resolved (exit 0, no event when
/// already resolved).
pub fn resolve_comment(
    conn: &Connection,
    issue_id: &str,
    comment_id: &str,
    actor: &str,
    dry_run: bool,
) -> Result<CommentMutationReport> {
    transition_resolution(conn, issue_id, comment_id, &RESOLVE, actor, dry_run)
}

/// `comment unresolve`: return one comment to unresolved (exit 0, no event
/// when already unresolved; a stored NULL is a no-op).
pub fn unresolve_comment(
    conn: &Connection,
    issue_id: &str,
    comment_id: &str,
    actor: &str,
    dry_run: bool,
) -> Result<CommentMutationReport> {
    transition_resolution(conn, issue_id, comment_id, &UNRESOLVE, actor, dry_run)
}
