//! Workspace-bound scanner gate shared by CLI and direct public services.
use crate::error::{Error, Result};
use crate::scan::{self, Field, ScanConfig};
use rusqlite::Connection;
use std::cell::Cell;
use std::path::Path;

thread_local! { static DEPTH: Cell<usize> = const { Cell::new(0) }; }

pub(crate) struct WriteGuard {
    _audit: Option<scan::AcknowledgmentAuditGuard>,
}

impl Drop for WriteGuard {
    fn drop(&mut self) {
        DEPTH.with(|depth| depth.set(depth.get() - 1));
    }
}

pub fn policy(conn: &Connection) -> Result<ScanConfig> {
    let Some(path) = conn.path().filter(|path| !path.is_empty()) else {
        return Ok(ScanConfig::enforce());
    };
    let parent = Path::new(path)
        .parent()
        .ok_or_else(|| Error::integrity("database has no workspace parent"))?;
    if parent.file_name().is_some_and(|name| name == ".beads") {
        let native: bool = conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM workspace WHERE id=1)",
            [],
            |row| row.get(0),
        )?;
        if !native {
            return Ok(ScanConfig::enforce());
        }
        let root = parent
            .parent()
            .ok_or_else(|| Error::integrity("database has no workspace root"))?;
        return ScanConfig::load_from_workspace_root(root)
            .map_err(|error| Error::validation(error.to_string()));
    }
    // Embedded stores have no editable policy: the compatibility context is
    // permanently enforce, and callers cannot inject an arbitrary mode.
    Ok(ScanConfig::enforce())
}

pub(crate) fn selector(kind: &str, id: &str) -> String {
    let valid = !id.is_empty()
        && id.len() <= 255
        && id
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-');
    format!("{kind}:{}", if valid { id } else { "input" })
}

pub(crate) fn guard(
    conn: &Connection,
    selector: &str,
    actor: &str,
    fields: &[Field<'_>],
) -> Result<WriteGuard> {
    super::secret_maintenance::ensure_publication_allowed(conn)?;
    ensure_not_quarantined(conn)?;
    let mut config = policy(conn)?;
    scan::include_invocation_admissions(&mut config);
    let initial = scan::scan(&config, selector, fields);
    scan::inherit_acknowledgments(&mut config, &initial, fields);
    let report = scan::scan(&config, selector, fields);
    if let Some(rejection) = scan::reject_if_blocked(&config, &report) {
        return Err(Error::validation(rejection.message));
    }
    finish_guard(conn, &report, actor)
}

fn finish_guard(conn: &Connection, report: &scan::ScanReport, actor: &str) -> Result<WriteGuard> {
    scan::record_advisories(report);
    let outer = DEPTH.with(|depth| depth.get() == 0);
    let audit = if outer {
        let exists: bool = conn.query_row("SELECT EXISTS(SELECT 1 FROM sqlite_temp_schema WHERE name = 'pending_secret_acknowledgments')", [], |row| row.get(0))?;
        if exists {
            conn.execute("DELETE FROM temp.pending_secret_acknowledgments", [])?;
        }
        let audit = (!report.acknowledged.is_empty())
            .then(|| scan::arm_acknowledgment_audit(report, actor));
        scan::install_acknowledgment_audit_bridge(conn)?;
        audit
    } else {
        None
    };
    DEPTH.with(|depth| depth.set(depth.get() + 1));
    Ok(WriteGuard { _audit: audit })
}

pub(crate) fn guard_manifest(
    conn: &Connection,
    manifest: &super::manifest::Manifest,
) -> Result<WriteGuard> {
    super::secret_maintenance::ensure_publication_allowed(conn)?;
    ensure_not_quarantined(conn)?;
    let mut config = policy(conn)?;
    scan::include_invocation_admissions(&mut config);
    let report = super::manifest::scan_manifest(&config, manifest);
    if let Some(rejection) = scan::reject_if_blocked(&config, &report) {
        return Err(Error::validation(rejection.message));
    }
    finish_guard(conn, &report, "cli")
}

pub(crate) fn guard_pairs(
    conn: &Connection,
    selector: &str,
    actor: &str,
    pairs: &[(&str, &str)],
) -> Result<WriteGuard> {
    let fields: Vec<_> = pairs
        .iter()
        .map(|(path, text)| Field::new(path, text))
        .collect();
    guard(conn, selector, actor, &fields)
}

/// Local-only tables ordinarily have no checkpoint-carried audit event. An
/// exact admission still requires an event in their own write transaction.
pub(crate) fn audit_local_admission(conn: &Connection, operation: &str) -> Result<()> {
    let exists:bool=conn.query_row("SELECT EXISTS(SELECT 1 FROM sqlite_temp_schema WHERE name='pending_secret_acknowledgments')",[],|row|row.get(0))?;
    if exists {
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM temp.pending_secret_acknowledgments",
            [],
            |row| row.get(0),
        )?;
        if count != 0 {
            conn.execute("INSERT INTO events (kind,actor,time,detail) VALUES ('local_secret_admission','cli',datetime('now'),?1)",
                [serde_json::json!({"operation":operation}).to_string()])?;
        }
    }
    Ok(())
}

pub(crate) fn local_transaction<T>(
    conn: &Connection,
    operation: &str,
    write: impl FnOnce() -> Result<T>,
) -> Result<T> {
    atomic_write(conn, || {
        let value = write()?;
        audit_local_admission(conn, operation)?;
        Ok(value)
    })
}

/// Acquire the main-database write lock before rechecking maintenance state.
/// A savepoint composes with caller transactions; standalone calls own an
/// IMMEDIATE transaction. The zero-row UPDATE upgrades deferred callers
/// without changing an issue or emitting an audit event.
pub(crate) fn atomic_write<T>(conn: &Connection, write: impl FnOnce() -> Result<T>) -> Result<T> {
    let standalone = conn.is_autocommit();
    if standalone {
        conn.execute_batch("BEGIN IMMEDIATE")?;
    }
    if let Err(error) = conn.execute_batch("SAVEPOINT bead_local_secret_write") {
        if standalone {
            let _ = conn.execute_batch("ROLLBACK");
        }
        return Err(error.into());
    }
    let result = (|| {
        conn.execute("UPDATE issues SET id=id WHERE 0", [])?;
        super::secret_maintenance::ensure_publication_allowed(conn)?;
        write()
    })();
    match result {
        Ok(value) => {
            let completion = conn
                .execute_batch("RELEASE bead_local_secret_write")
                .and_then(|()| {
                    if standalone {
                        conn.execute_batch("COMMIT")
                    } else {
                        Ok(())
                    }
                });
            if let Err(error) = completion {
                if standalone {
                    let _ = conn.execute_batch("ROLLBACK");
                } else {
                    let _ = conn.execute_batch(
                        "ROLLBACK TO bead_local_secret_write; RELEASE bead_local_secret_write",
                    );
                }
                return Err(error.into());
            }
            Ok(value)
        }
        Err(error) => {
            let rollback = conn.execute_batch(
                "ROLLBACK TO bead_local_secret_write; RELEASE bead_local_secret_write",
            );
            if standalone {
                conn.execute_batch("ROLLBACK")?;
            } else {
                rollback?;
            }
            Err(error)
        }
    }
}

pub fn ensure_not_quarantined(conn: &Connection) -> Result<()> {
    let exists: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_schema WHERE name = 'secret_quarantine')",
        [],
        |row| row.get(0),
    )?;
    if exists {
        let count: i64 = conn.query_row("SELECT COUNT(*) FROM secret_quarantine", [], |row| {
            row.get(0)
        })?;
        if count != 0 {
            return Err(Error::conflict("secret_quarantined: recovery contains blocking findings; inspect 'bead doctor --scope secrets' and remove each finding with 'bead redact'"));
        }
    }
    Ok(())
}

/// Called inside recovery activation's transaction. Historical content stays
/// available for diagnosis, but its hold survives restart and rolls back with
/// a failed import. Neither matched values nor diagnostic paths are stored.
pub(crate) fn quarantine_recovery(conn: &Connection) -> Result<()> {
    let config = policy(conn)?;
    let mut findings = super::secret_diagnostics::scan_live_findings(conn)?;
    let mut incomplete = false;
    if let Some(path) = conn.path().filter(|path| !path.is_empty()) {
        if let Some(parent) = Path::new(path)
            .parent()
            .filter(|parent| parent.file_name().is_some_and(|name| name == ".beads"))
        {
            let checkpoint = parent.join("checkpoint");
            if checkpoint.join("current.json").exists() || checkpoint.join("previous.json").exists()
            {
                match super::secret_diagnostics::scan_recovery_artifact(&checkpoint) {
                    Ok(report) => findings.extend(report.findings),
                    Err(_) => incomplete = true,
                }
            }
        }
    }
    let blocking = findings
        .iter()
        .filter(|finding| {
            finding.is_blocking_match() && !config.is_acknowledged(&finding.fingerprint)
        })
        .count();
    if blocking != 0 || incomplete {
        conn.execute("INSERT INTO secret_quarantine (id, ruleset_version, blocking_count, coverage_incomplete) VALUES (1, ?1, ?2, ?3)
            ON CONFLICT(id) DO UPDATE SET ruleset_version=excluded.ruleset_version, blocking_count=excluded.blocking_count, coverage_incomplete=excluded.coverage_incomplete",
            rusqlite::params![scan::RULESET_VERSION, blocking as i64, incomplete])?;
    } else {
        conn.execute("DELETE FROM secret_quarantine", [])?;
    }
    Ok(())
}

/// A redaction resets both retained generations from this checked snapshot.
/// Do not publish an intermediate recovery snapshot with other credentials.
pub(crate) fn verify_quarantine_cleanup(conn: &Connection) -> Result<()> {
    let quarantined: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM secret_quarantine)",
        [],
        |row| row.get(0),
    )?;
    if !quarantined {
        return Ok(());
    }
    let config = policy(conn)?;
    let findings = super::secret_diagnostics::scan_live_findings(conn)?;
    if findings
        .iter()
        .any(|finding| finding.is_blocking_match() && !config.is_acknowledged(&finding.fingerprint))
    {
        return Err(Error::conflict("secret_quarantined: other blocking findings remain; redact them before resuming sanitized publication"));
    }
    Ok(())
}
