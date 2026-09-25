//! Shared lease-row inspection and seeding for the integration suites.
//!
//! Lease rows are append-only per claim epoch (migration 13): no CLI verb
//! ever removes one, so a store that has seen leases accumulates history --
//! a released lease's row keeps the unexpired timestamp its holder left it
//! with, an expired lease's row keeps a lapsed one, and every other leased
//! issue carries its own row sequence whose tokens number the same range as
//! everyone else's. The suites that reason about that history -- claim-epoch
//! fencing (`claim_epoch.rs`) and R002 leased claims
//! (`r002_leased_claims.rs`) -- both read the rows back through here so
//! there is one SQL surface for the table, and the fencing suite also seeds
//! the one state no CLI verb can produce on demand: a lease row whose
//! expiry has already passed. Leases never run shorter than
//! `MIN_LEASE_TTL = 30` seconds, so waiting one out would put a floor of
//! half a minute under every expiry arm; backdating the row's timestamp in
//! the store leaves exactly the row time itself would have left, seconds
//! faster.
//!
//! Included as a module by several test crates; not every consumer uses
//! every helper, so unused items in any one crate are expected.
#![allow(dead_code)]

use rusqlite::Connection;
use std::path::Path;

/// One append-only lease row, as the store holds it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LeaseRow {
    pub assignee: String,
    pub fencing_token: i64,
    pub expires_at: String,
}

/// The lease rows the store holds for `issue_id`, in fencing-token order --
/// the issue's whole claim-epoch history, superseded tenures included.
pub fn lease_rows(workspace: &Path, issue_id: &str) -> Vec<LeaseRow> {
    let conn = Connection::open_with_flags(
        workspace.join(".beads").join("beads.db"),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .expect("the workspace store must be openable read-only");
    let mut statement = conn
        .prepare(
            "SELECT assignee, fencing_token, expires_at
             FROM leases
             WHERE issue_id = ?1
             ORDER BY fencing_token ASC",
        )
        .expect("the leases table must be queryable");
    statement
        .query_map([issue_id], |row| {
            Ok(LeaseRow {
                assignee: row.get(0)?,
                fencing_token: row.get(1)?,
                expires_at: row.get(2)?,
            })
        })
        .expect("lease rows must be readable")
        .collect::<rusqlite::Result<Vec<_>>>()
        .expect("lease rows must be collectable")
}

/// Let a claim's lease expire `hours` ago, the way time would have.
///
/// The row the lease wrote keeps its assignee and fencing token -- only its
/// `expires_at` moves into the past, to the returned instant. The issue's
/// `updated_at` moves with it, because expiry alone does not unassign the
/// claim: the watchdog that recovers the issue decides staleness from
/// `updated_at` and lease-liveness from the row, and both have to agree the
/// tenure is dead for the store to reach the state a crashed worker's
/// expired lease leaves behind -- released, with the row surviving.
pub fn lapse_lease(workspace: &Path, issue_id: &str, fencing_token: i64, hours: i64) -> String {
    let lapsed_at = (chrono::Utc::now() - chrono::Duration::hours(hours))
        .to_rfc3339_opts(chrono::SecondsFormat::Secs, true);

    let conn = Connection::open(workspace.join(".beads").join("beads.db"))
        .expect("the workspace store must be openable for seeding");
    let aged_issues = conn
        .execute(
            "UPDATE issues SET updated_at = ?1 WHERE id = ?2",
            (&lapsed_at, issue_id),
        )
        .expect("the issue's staleness clock must be seedable");
    assert_eq!(
        aged_issues, 1,
        "seeding must age exactly the issue whose lease is lapsing"
    );
    let aged_rows = conn
        .execute(
            "UPDATE leases SET expires_at = ?1 WHERE issue_id = ?2 AND fencing_token = ?3",
            (&lapsed_at, issue_id, fencing_token),
        )
        .expect("the lease row's expiry must be seedable");
    assert_eq!(
        aged_rows, 1,
        "seeding must lapse exactly the tenure's own row, not its history siblings"
    );

    lapsed_at
}
