//! Controlled identity rewrites; callers never supply replacement names.
use super::secret_diagnostics::LiveFindingLocation;
use crate::model::redaction::RedactionError;
use rusqlite::{params, params_from_iter, types::Value as SqlValue, Connection};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub(crate) const EXTENSION: &str = "key_rewrite_v1";

#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Family {
    Label,
    DataNamespace,
    ReferenceNamespace,
    ReferenceKey,
    UniqueKey,
    Resource,
    ViewName,
}

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct Metadata {
    family: Family,
    scope_issue_digest: Option<String>,
    old_value_digest: String,
    pub new_value: String,
}

pub(crate) struct Plan {
    metadata: Metadata,
    issue: Option<String>,
    old: Vec<u8>,
}
impl Drop for Plan {
    fn drop(&mut self) {
        self.old.fill(0);
        std::hint::black_box(&self.old);
    }
}

fn digest(domain: &str, value: &str) -> String {
    let mut hash = Sha256::new();
    hash.update(b"bead-rs-controlled-key-rewrite-v1\0");
    hash.update(domain.as_bytes());
    hash.update(b"\0");
    hash.update(value.as_bytes());
    format!("{:x}", hash.finalize())
}
fn value_digest(family: Family, value: &str) -> String {
    digest(
        &serde_json::to_string(&family).expect("fixed family serializes"),
        value,
    )
}
fn failure() -> RedactionError {
    RedactionError::Integrity("could not maintain redaction identity references".into())
}
fn collision() -> RedactionError {
    RedactionError::Conflict(
        "identity_rekey_collision: replacement identity conflicts; no rows were merged or overwritten"
            .into(),
    )
}

fn component(
    conn: &Connection,
    location: &LiveFindingLocation,
    field: &str,
) -> Result<String, RedactionError> {
    if let Some(index) = location
        .identity_fields
        .iter()
        .position(|name| *name == field)
    {
        if let SqlValue::Text(value) = &location.identity_values[index] {
            return Ok(value.clone());
        }
    }
    let predicate = location
        .identity_fields
        .iter()
        .enumerate()
        .map(|(index, name)| format!("{name} IS ?{}", index + 1))
        .collect::<Vec<_>>()
        .join(" AND ");
    conn.query_row(
        &format!("SELECT {field} FROM {} WHERE {predicate}", location.table),
        params_from_iter(location.identity_values.iter()),
        |row| row.get(0),
    )
    .map_err(|_| failure())
}

impl Plan {
    fn old(&self) -> &str {
        std::str::from_utf8(&self.old).expect("identity originated in SQLite text")
    }
    pub fn resolve(
        conn: &Connection,
        location: &LiveFindingLocation,
        current: &str,
    ) -> Result<Option<Self>, RedactionError> {
        let family = match (location.table, location.field) {
            ("labels", "label") => Family::Label,
            ("issue_data", "namespace") => Family::DataNamespace,
            ("external_references" | "unique_reference_bindings", "namespace") => {
                Family::ReferenceNamespace
            }
            ("external_references", "key") => Family::ReferenceKey,
            ("unique_reference_bindings", "key") => Family::UniqueKey,
            ("issue_resource_keys", "resource_key") => Family::Resource,
            ("saved_views", "name") => Family::ViewName,
            ("external_references", "value")
                if component(conn, location, "key")? == "unique-ref" =>
            {
                let namespace = component(conn, location, "namespace")?;
                let issue = component(conn, location, "issue_id")?;
                let bound: bool = conn.query_row("SELECT EXISTS(SELECT 1 FROM unique_reference_bindings WHERE issue_id=?1 AND namespace=?2 AND key=?3)", params![issue, namespace, current], |row| row.get(0)).map_err(|_| failure())?;
                if !bound {
                    return Ok(None);
                }
                Family::UniqueKey
            }
            _ => return Ok(None),
        };
        let issue = if matches!(family, Family::Resource | Family::ViewName) {
            None
        } else {
            Some(component(conn, location, "issue_id")?)
        };
        let old_value_digest = value_digest(family, current);
        let metadata = Metadata {
            family,
            scope_issue_digest: issue.as_deref().map(|value| digest("issue", value)),
            new_value: format!("redacted-{}", &old_value_digest[..32]),
            old_value_digest,
        };
        Ok(Some(Self {
            metadata,
            issue,
            old: current.as_bytes().to_vec(),
        }))
    }

    pub fn metadata(&self) -> Result<serde_json::Value, RedactionError> {
        serde_json::to_value(&self.metadata).map_err(|_| failure())
    }

    /// Include identity changes even when the other selected field is prose.
    pub fn affects(
        &self,
        conn: &Connection,
        location: &LiveFindingLocation,
    ) -> Result<bool, RedactionError> {
        let table = location.table;
        let relevant = match self.metadata.family {
            Family::Label => table == "labels",
            Family::DataNamespace => table == "issue_data",
            Family::ReferenceNamespace => {
                matches!(table, "external_references" | "unique_reference_bindings")
            }
            Family::ReferenceKey => table == "external_references",
            Family::UniqueKey => {
                matches!(table, "external_references" | "unique_reference_bindings")
            }
            Family::Resource => table == "issue_resource_keys",
            Family::ViewName => table == "saved_views",
        };
        if !relevant {
            return Ok(false);
        }
        if let Some(issue) = &self.issue {
            if component(conn, location, "issue_id")? != *issue {
                return Ok(false);
            }
        }
        let column = match self.metadata.family {
            Family::Label => "label",
            Family::DataNamespace | Family::ReferenceNamespace => "namespace",
            Family::ReferenceKey => "key",
            Family::Resource => "resource_key",
            Family::ViewName => "name",
            Family::UniqueKey if table == "external_references" => {
                if component(conn, location, "key")? != "unique-ref" {
                    return Ok(false);
                }
                "value"
            }
            Family::UniqueKey => "key",
        };
        // A view's name is not its primary key, but its unique identity changes.
        Ok(component(conn, location, column)? == self.old())
    }

    pub fn apply(
        &self,
        conn: &Connection,
        location: &LiveFindingLocation,
    ) -> Result<(LiveFindingLocation, Vec<String>), RedactionError> {
        let issues = if let Some(issue) = &self.issue {
            vec![issue.clone()]
        } else if self.metadata.family == Family::Resource {
            let mut statement = conn.prepare("SELECT issue_id FROM issue_resource_keys WHERE resource_key=?1 UNION SELECT issue_id FROM resource_locks WHERE resource_key=?1 ORDER BY issue_id").map_err(|_| failure())?;
            statement
                .query_map([self.old()], |row| row.get(0))
                .map_err(|_| failure())?
                .collect::<rusqlite::Result<Vec<String>>>()
                .map_err(|_| failure())?
        } else {
            Vec::new()
        };
        let replacement = &self.metadata.new_value;
        let issue = self.issue.as_deref().unwrap_or("");
        let ensure_unoccupied = |sql: &str| -> Result<(), RedactionError> {
            let occupied: bool = conn
                .query_row(sql, params![replacement, issue], |row| row.get(0))
                .map_err(|_| failure())?;
            if occupied {
                return Err(collision());
            }
            Ok(())
        };
        let update = |sql: &str| {
            conn.execute(sql, params![replacement, issue, self.old()]).map_err(|error| {
            if matches!(error, rusqlite::Error::SqliteFailure(ref code, _) if code.code == rusqlite::ErrorCode::ConstraintViolation) {
                collision()
            } else { failure() }
        })
        };
        match self.metadata.family {
            Family::Label => {
                update("UPDATE labels SET label=?1 WHERE issue_id=?2 AND label=?3")?;
            }
            Family::DataNamespace => {
                update("UPDATE issue_data SET namespace=?1 WHERE issue_id=?2 AND namespace=?3")?;
            }
            Family::ReferenceNamespace => {
                ensure_unoccupied(
                    "SELECT EXISTS(SELECT 1 FROM external_references WHERE namespace=?1 AND issue_id=?2
                     UNION ALL SELECT 1 FROM unique_reference_bindings WHERE namespace=?1 AND issue_id=?2)",
                )?;
                update("UPDATE external_references SET namespace=?1 WHERE issue_id=?2 AND namespace=?3")?;
                update("UPDATE unique_reference_bindings SET namespace=?1 WHERE issue_id=?2 AND namespace=?3")?;
            }
            Family::ReferenceKey => {
                ensure_unoccupied(
                    "SELECT EXISTS(SELECT 1 FROM external_references WHERE key=?1 AND issue_id=?2)",
                )?;
                update("UPDATE external_references SET key=?1 WHERE issue_id=?2 AND key=?3")?;
            }
            Family::UniqueKey => {
                ensure_unoccupied(
                    "SELECT EXISTS(SELECT 1 FROM unique_reference_bindings WHERE key=?1 AND issue_id=?2
                     UNION ALL SELECT 1 FROM external_references WHERE key='unique-ref' AND value=?1 AND issue_id=?2)",
                )?;
                update("UPDATE unique_reference_bindings SET key=?1 WHERE issue_id=?2 AND key=?3")?;
                update("UPDATE external_references SET value=?1 WHERE issue_id=?2 AND key='unique-ref' AND value=?3")?;
            }
            Family::Resource => {
                // Declarations have an issue-scoped primary key, but resource
                // identity is global. A destination declared only by another
                // issue must not silently merge two contention groups.
                let occupied: bool = conn
                    .query_row(
                        "SELECT EXISTS(SELECT 1 FROM issue_resource_keys WHERE resource_key=?1
                         UNION ALL SELECT 1 FROM resource_locks WHERE resource_key=?1)",
                        [replacement],
                        |row| row.get(0),
                    )
                    .map_err(|_| failure())?;
                if occupied {
                    return Err(collision());
                }
                update("UPDATE issue_resource_keys SET resource_key=?1 WHERE ?2='' AND resource_key=?3")?;
                update(
                    "UPDATE resource_locks SET resource_key=?1 WHERE ?2='' AND resource_key=?3",
                )?;
            }
            Family::ViewName => {
                update("UPDATE saved_views SET name=?1 WHERE ?2='' AND name=?3")?;
            }
        }
        let mut post = location.clone();
        if let Some(index) = post
            .identity_fields
            .iter()
            .position(|name| *name == post.field)
        {
            post.identity_values[index] = SqlValue::Text(replacement.clone());
        }
        Ok((post, issues))
    }
}

/// Tombstone digests protect coupled aliases independently of scanner version
/// and of the selected row's pre-rekey primary-key selector.
pub(crate) fn reject_restored_identities(conn: &Connection) -> Result<(), RedactionError> {
    let mut statement = conn
        .prepare(
            "SELECT extensions_json FROM redaction_tombstones WHERE extensions_json IS NOT NULL",
        )
        .map_err(|_| failure())?;
    let extensions = statement
        .query_map([], |row| row.get::<_, String>(0))
        .map_err(|_| failure())?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(|_| failure())?;
    for extension in extensions {
        let object: serde_json::Value = serde_json::from_str(&extension).map_err(|_| failure())?;
        let Some(value) = object.get(EXTENSION) else {
            continue;
        };
        let metadata: Metadata = serde_json::from_value(value.clone()).map_err(|_| failure())?;
        let valid_hash = |value: &str| {
            value.len() == 64
                && value
                    .bytes()
                    .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        };
        if !valid_hash(&metadata.old_value_digest)
            || metadata
                .scope_issue_digest
                .as_deref()
                .is_some_and(|value| !valid_hash(value))
            || metadata.new_value != format!("redacted-{}", &metadata.old_value_digest[..32])
            || metadata.scope_issue_digest.is_none()
                != matches!(metadata.family, Family::Resource | Family::ViewName)
        {
            return Err(failure());
        }
        let queries: &[&str] = match metadata.family {
            Family::Label => &["SELECT issue_id,label FROM labels"],
            Family::DataNamespace => &["SELECT issue_id,namespace FROM issue_data"],
            Family::ReferenceNamespace => &[
                "SELECT issue_id,namespace FROM external_references",
                "SELECT issue_id,namespace FROM unique_reference_bindings",
            ],
            Family::ReferenceKey => &["SELECT issue_id,key FROM external_references"],
            Family::UniqueKey => &[
                "SELECT issue_id,key FROM unique_reference_bindings",
                "SELECT issue_id,value FROM external_references WHERE key='unique-ref'",
            ],
            Family::Resource => &[
                "SELECT issue_id,resource_key FROM issue_resource_keys",
                "SELECT issue_id,resource_key FROM resource_locks",
            ],
            Family::ViewName => &["SELECT NULL,name FROM saved_views"],
        };
        for query in queries {
            let mut rows = conn.prepare(query).map_err(|_| failure())?;
            let mut rows = rows.query([]).map_err(|_| failure())?;
            while let Some(row) = rows.next().map_err(|_| failure())? {
                let issue: Option<String> = row.get(0).map_err(|_| failure())?;
                if metadata.scope_issue_digest.is_some()
                    && issue.as_deref().map(|value| digest("issue", value))
                        != metadata.scope_issue_digest
                {
                    continue;
                }
                let value: String = row.get(1).map_err(|_| failure())?;
                let mut value = value.into_bytes();
                let resurrected = value_digest(
                    metadata.family,
                    std::str::from_utf8(&value).map_err(|_| failure())?,
                ) == metadata.old_value_digest;
                value.fill(0);
                std::hint::black_box(&value);
                if resurrected {
                    return Err(RedactionError::Conflict(
                        "recovery refused: controlled identity redaction would be resurrected"
                            .into(),
                    ));
                }
            }
        }
    }
    Ok(())
}
