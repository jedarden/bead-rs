//! Narrow, explicit recovery contract: restored-branch-reconciliation-v1.md.

use super::*;

const ORIGIN_EXTENSION: &str = "restored_branch_origin";
const RECEIPT_EXTENSION: &str = "restored_branch_reconciliation";

#[derive(Debug, Serialize)]
pub struct RestoredBranchReport {
    pub source_generation: String,
    pub source_root_sha256: String,
    pub original_store_uuid: String,
    pub branch_origin: String,
    pub first_original_sequence: i64,
    pub events_imported: usize,
    pub issues_inserted: usize,
    pub existing_issues_preserved: usize,
    pub dry_run: bool,
    pub already_merged: bool,
    pub receipt_id: String,
}

/// Preserve local state while admitting only an independently restored branch's
/// newly created issues. Source verification is the same named-generation
/// verification used by restore; this function never replaces a native store.
pub fn import_restored_branch(
    store: &mut SqliteStore,
    source: &Path,
    generation: &str,
    branch_at: i64,
    actor: &str,
    dry_run: bool,
) -> Result<RestoredBranchReport> {
    validate_restore_actor(actor)?;
    if branch_at <= 1 {
        bail!("Restored branch boundary must follow a nonempty shared prefix");
    }
    let _publication_lock = acquire_store_checkpoint_publication_lock(store)?;
    let verified = verify_restore_source(source, generation)?;
    let mut staging = verified.staging;
    let tx = Transaction::new_unchecked(store.conn(), TransactionBehavior::Immediate)?;
    let (_, target_uuid) = get_workspace_state(&tx)?;
    if staging.store_uuid != target_uuid
        || staging
            .events
            .iter()
            .any(|event| event.origin_store_uuid != target_uuid)
    {
        bail!("Restored branch requires the same single native origin as the target");
    }
    if staging.redaction_record_count != 0 || !read_all_redaction_records(&tx)?.is_empty() {
        bail!("Restored branch reconciliation does not reidentify redaction history");
    }
    let live = read_all_events(&tx)?;
    let live_by_identity: HashMap<(&str, i64), &EventRecord> = live
        .iter()
        .map(|event| {
            (
                (
                    event.origin_store_uuid.as_str(),
                    event.origin_event_sequence,
                ),
                event,
            )
        })
        .collect();
    for event in staging
        .events
        .iter()
        .filter(|event| event.origin_event_sequence < branch_at)
    {
        let Some(local) =
            live_by_identity.get(&(target_uuid.as_str(), event.origin_event_sequence))
        else {
            bail!("Restored branch shared prefix is missing from the target");
        };
        if !crate::service::reconcile::public_content_matches(local, event) {
            bail!(
                "Restored branch shared prefix differs at sequence {}",
                event.origin_event_sequence
            );
        }
    }
    let boundary = staging
        .events
        .iter()
        .find(|event| event.origin_event_sequence == branch_at)
        .ok_or_else(|| anyhow!("Restored branch boundary does not exist in the source"))?;
    let matching_receipt = staging.receipts.iter().any(|receipt| {
        receipt.kind == "restore"
            && receipt.source_store_uuid == target_uuid
            && receipt.target_store_uuid == target_uuid
            && receipt.summary_event_identity.as_deref()
                == Some(format!("local-{branch_at}").as_str())
            && boundary.detail["source_root_sha256"].as_str()
                == Some(receipt.source_root_sha256.as_str())
    });
    if boundary.kind != "checkpoint_restored"
        || boundary.issue_id.is_some()
        || boundary.detail["snapshot_sequence"].as_i64() != Some(branch_at - 1)
        || boundary.detail["source_store_uuid"].as_str() != Some(target_uuid.as_str())
        || !matching_receipt
    {
        bail!("Restored branch boundary must be a checkpoint_restored event with matching restore provenance");
    }
    let Some(local_boundary) = live_by_identity.get(&(target_uuid.as_str(), branch_at)) else {
        bail!("Restored branch boundary does not conflict with a target event; use ordinary merge");
    };
    if crate::service::reconcile::public_content_matches(local_boundary, boundary) {
        bail!("Restored branch boundary does not conflict with a target event; use ordinary merge");
    }
    let branch_origin = format!(
        "restored-{:x}",
        Sha256::digest(serde_json::to_vec(boundary)?)
    );
    let receipt_id = format!("restored-branch-{}", staging.input_hash);
    let mut report = RestoredBranchReport {
        source_generation: generation.to_owned(),
        source_root_sha256: staging.input_hash.clone(),
        original_store_uuid: target_uuid.clone(),
        branch_origin: branch_origin.clone(),
        first_original_sequence: branch_at,
        events_imported: 0,
        issues_inserted: 0,
        existing_issues_preserved: 0,
        dry_run,
        already_merged: false,
        receipt_id: receipt_id.clone(),
    };
    let prior: Option<String> = tx
        .query_row(
            "SELECT extensions_json FROM provenance_receipts WHERE receipt_id = ?1",
            [&receipt_id],
            |row| row.get(0),
        )
        .optional()?;
    if let Some(prior) = prior {
        let prior: serde_json::Value = serde_json::from_str(&prior)?;
        let proof = &prior[RECEIPT_EXTENSION];
        if proof["source_root_sha256"] != staging.input_hash
            || proof["branch_origin"] != branch_origin
            || proof["first_original_sequence"] != branch_at
        {
            bail!("Restored branch reconciliation receipt identity conflict");
        }
        report.already_merged = true;
        return Ok(report);
    }

    let live_attempts: HashMap<String, serde_json::Value> = read_all_attempt_outcomes(&tx)?
        .into_iter()
        .map(|record| Ok((record.receipt_id.clone(), serde_json::to_value(record)?)))
        .collect::<Result<_>>()?;
    for outcome in &staging.attempt_outcomes {
        if live_attempts.get(&outcome.receipt_id) != Some(&serde_json::to_value(outcome)?) {
            bail!("Restored branch requires attempt outcomes already present identically in the target");
        }
    }
    let mut new_ids = HashSet::new();
    for event in staging
        .events
        .iter()
        .filter(|event| event.origin_event_sequence > branch_at)
    {
        let id = event
            .issue_id
            .as_deref()
            .ok_or_else(|| anyhow!("Restored branch contains an unsupported workspace event"))?;
        if event.kind == "created" {
            if !new_ids.insert(id.to_owned()) {
                bail!("Restored branch repeats an issue creation");
            }
        } else if !new_ids.contains(id)
            || !matches!(
                event.kind.as_str(),
                "updated" | "label_added" | "label_removed"
            )
        {
            bail!("Restored branch edits an existing issue or uses an unsupported event kind");
        }
    }
    for issue in &staging.issues {
        let exists: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM issues WHERE id = ?1)",
            [&issue.id],
            |row| row.get(0),
        )?;
        if exists == new_ids.contains(&issue.id) {
            bail!(
                "Restored branch issue identity conflict or missing shared issue: {}",
                issue.id
            );
        }
        if exists {
            report.existing_issues_preserved += 1;
        }
    }
    staging.issues.retain(|issue| new_ids.contains(&issue.id));
    staging.labels.retain(|(id, _)| new_ids.contains(id));
    staging
        .dependencies
        .retain(|edge| new_ids.contains(&edge.blocked));
    staging
        .events
        .retain(|event| event.origin_event_sequence >= branch_at);
    for event in &mut staging.events {
        let origin = serde_json::json!({
            "origin_store_uuid": event.origin_store_uuid,
            "origin_event_sequence": event.origin_event_sequence,
        });
        if event
            .extensions
            .insert(ORIGIN_EXTENSION.to_owned(), origin)
            .is_some()
        {
            bail!("Restored branch event already uses the reserved provenance extension");
        }
        event.origin_store_uuid = branch_origin.clone();
        event.origin_event_sequence = event.origin_event_sequence - branch_at + 1;
    }
    validate_event_prefix(&tx, &staging)?;
    staging.issue_count = staging.issues.len();
    staging.event_count = staging.events.len();
    report.issues_inserted = import_issues(&tx, &staging)?;
    import_dependencies(&tx, &staging)?;
    import_labels(&tx, &staging)?;
    import_events(&tx, &staging)?;
    import_receipts(&tx, &staging)?;
    reject_live_tombstoned_findings(&tx)?;
    report.events_imported = staging.events.len();
    let proof = serde_json::to_value(&report)?;
    let sequence = create_merge_summary(&tx, &staging, actor)?;
    let receipt = create_merge_receipt(&tx, &staging, actor, sequence)?;
    let extensions = serde_json::json!({ RECEIPT_EXTENSION: proof });
    tx.execute("UPDATE provenance_receipts SET receipt_id = ?1, extensions_json = ?2 WHERE receipt_id = ?3",
        params![receipt_id, extensions.to_string(), receipt.receipt_id])?;
    // Receipt IDs participate in the native receipt hash recipe.
    let mut hasher = Sha256::new();
    for field in [
        &receipt_id,
        &receipt.kind,
        &receipt.source_root_sha256,
        &receipt.actor,
        &receipt.created_at,
        &receipt.result,
    ] {
        hasher.update(field);
    }
    tx.execute(
        "UPDATE provenance_receipts SET receipt_sha256 = ?1 WHERE receipt_id = ?2",
        params![format!("{:x}", hasher.finalize()), receipt_id],
    )?;
    tx.execute(
        "UPDATE events SET detail = ?1 WHERE sequence = ?2",
        params![
            serde_json::json!({ RECEIPT_EXTENSION: proof }).to_string(),
            sequence
        ],
    )?;
    crate::service::secret_boundary::quarantine_recovery(&tx)?;
    if dry_run {
        tx.rollback()?;
    } else {
        tx.commit()?;
    }
    Ok(report)
}
