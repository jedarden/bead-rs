//! Checkpoint Set v2 (`checkpoint-set-v2`): the adaptive incremental layout
//!
//! Implements `research/specs/checkpoint-set-v2.md`: keyed records live in
//! ID-prefix shards sized by the section 5.2 bounds, events live in sealed
//! content-addressed segments plus one open tail, oversize records are
//! stored once under `set/rec/`, and the pointer and manifest grow with
//! artifact counts only. Publication bytes scale with what changed, not
//! with store size: an unchanged shard or sealed segment keeps its content
//! address and adds no bytes to history.
//!
//! The `set/` tree is physically disjoint from the `objects/` and
//! `manifests/` trees of the older layouts, so the two generations of a
//! migration coexist until structural retention reaps the old one.

use anyhow::{anyhow, bail, Result};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::io::Write as _;
use std::path::Path;
use std::time::SystemTime;

use super::{
    create_publication_temp, format_rfc3339, publication_temp_path, sync_dir, EventRecord,
    PublicationOutput, SerializedCorpus, ShardedConfig,
};

/// Typical shard size; the operating point (spec 5.2)
pub(crate) const SHARD_TARGET: usize = 32 * 1024;
/// A shard above this after a publication must split (spec 5.2)
pub(crate) const SHARD_SPLIT: usize = 96 * 1024;
/// A full-sibling group at or below this coalesces (spec 5.2)
pub(crate) const SHARD_MERGE: usize = 24 * 1024;
/// Records above this move to `set/rec/` (spec 5.2)
pub(crate) const RECORD_INLINE: usize = 64 * 1024;
/// The tail above this after a publication seals (spec 6.1)
pub(crate) const SEG_SEAL: usize = 64 * 1024;

/// The partition bounds a manifest records (spec 7.2)
pub(crate) fn bounds_json() -> serde_json::Value {
    serde_json::json!({
        "shard_target": SHARD_TARGET,
        "shard_split": SHARD_SPLIT,
        "shard_merge": SHARD_MERGE,
        "record_inline": RECORD_INLINE,
        "seg_seal": SEG_SEAL,
    })
}

/// The identity key a record kind owns, when the kind names one.
///
/// Records embed other kinds' IDs: a tombstone carries the epoch it
/// resurrects in, a receipt its epoch. A fixed probe order therefore keyed
/// the tombstone by the shared `epoch_id`, and the redaction class
/// partition refused the epoch and tombstone of one committed redaction as
/// duplicates — every set-v2 publication after a redaction failed
/// (`authorization_value_contract`, 2026-10-26). The record's own identity
/// outranks any embedded foreign one.
fn own_identity_key(record_type: &str) -> Option<&'static str> {
    match record_type {
        "issue" => Some("id"),
        "provenance_receipt" | "attempt_outcome" | "redaction_receipt" => Some("receipt_id"),
        "redaction_epoch" => Some("epoch_id"),
        "redaction_tombstone" => Some("tombstone_id"),
        "redaction_finding" | "redaction_acknowledgment" => Some("fingerprint"),
        _ => None,
    }
}

/// The stable record ID a keyed record line partitions on (spec 5.1)
///
/// The payload object's field name equals the record's `record_type` tag
/// (`CheckpointRecord`'s serde shape), so the payload is the single
/// non-`record_type` field; the ID key is the kind's own identity when it
/// has one, else the first generic ID key present.
fn keyed_record_id(line: &[u8]) -> Result<String> {
    let record: serde_json::Value = serde_json::from_slice(line)
        .map_err(|e| anyhow!("set-v2 record line is not valid JSON: {}", e))?;
    let record_type = record
        .get("record_type")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow!("set-v2 record line missing record_type"))?;
    if record.get("ref").and_then(|v| v.as_bool()) == Some(true) {
        // A reference row partitions on the record it stands for; the
        // resolved bytes carry the real ID, so the ref itself never lands
        // in a shard partition (oversize records are written, not keyed).
        bail!("set-v2 reference rows are writer-internal, not corpus input");
    }
    let payload = record
        .get(record_type)
        .ok_or_else(|| anyhow!("set-v2 {} record missing its payload", record_type))?;
    if let Some(own) = own_identity_key(record_type) {
        if let Some(id) = payload.get(own).and_then(|v| v.as_str()) {
            return Ok(id.to_string());
        }
    }
    for key in [
        "id",
        "receipt_id",
        "finding_id",
        "epoch_id",
        "tombstone_id",
        "fingerprint",
    ] {
        if let Some(id) = payload.get(key).and_then(|v| v.as_str()) {
            return Ok(id.to_string());
        }
    }
    bail!("set-v2 {} record carries no usable record ID", record_type)
}

/// One leaf of the prefix partition: every member ID begins with `prefix`
struct ShardPartition {
    prefix: String,
    /// Indices into the class's line list, ascending by record ID
    members: Vec<usize>,
    bytes: usize,
}

/// Partition one keyed-record class by ID prefix (spec 5.3, 5.4)
///
/// Split and merge are pure functions of the members' IDs and serialized
/// bytes, so equivalent states produce byte-identical partitions. A shard
/// exceeds `SHARD_SPLIT` only when it cannot split (a single record, or a
/// prefix that already reaches every member ID's full length).
fn partition_keyed_class(ids: &[String], line_bytes: &[usize]) -> Result<Vec<ShardPartition>> {
    let mut order: Vec<usize> = (0..ids.len()).collect();
    order.sort_by(|&a, &b| ids[a].cmp(&ids[b]));
    for window in order.windows(2) {
        if ids[window[0]] == ids[window[1]] {
            bail!("duplicate record ID {} in set-v2 corpus", ids[window[0]]);
        }
    }

    fn longest_common_prefix(ids: &[String], members: &[usize]) -> usize {
        let first = &ids[members[0]];
        let mut len = first.len();
        for &m in &members[1..] {
            let id = &ids[m];
            len = len.min(id.len());
            let mut i = 0;
            while i < len && first.as_bytes()[i] == id.as_bytes()[i] {
                i += 1;
            }
            len = i;
        }
        len
    }

    fn build(
        prefix: &str,
        members: &[usize],
        ids: &[String],
        line_bytes: &[usize],
        depth_guard: usize,
    ) -> Vec<ShardPartition> {
        let bytes: usize = members.iter().map(|&m| line_bytes[m] + 1).sum();
        let unsplittable = members.len() <= 1
            || bytes <= SHARD_SPLIT
            || depth_guard == 0
            || longest_common_prefix(ids, members)
                >= members.iter().map(|&m| ids[m].len()).min().unwrap_or(0);
        if unsplittable {
            return vec![ShardPartition {
                prefix: prefix.to_string(),
                members: members.to_vec(),
                bytes,
            }];
        }

        // New depth is one plus the members' longest common prefix, never
        // shorter than the current prefix plus one character (spec 5.3).
        let depth = prefix.len().max(longest_common_prefix(ids, members)) + 1;
        let mut groups: BTreeMap<String, Vec<usize>> = BTreeMap::new();
        for &m in members {
            let id = &ids[m];
            let key = if id.len() >= depth {
                id[..depth].to_string()
            } else {
                // The ID is a strict prefix of another member's ID; it owns
                // itself exactly rather than inventing a shared child.
                id.clone()
            };
            groups.entry(key).or_default().push(m);
        }
        if groups.len() <= 1 {
            // Cannot separate the members at any depth; keep as one shard.
            return vec![ShardPartition {
                prefix: prefix.to_string(),
                members: members.to_vec(),
                bytes,
            }];
        }

        let children: Vec<ShardPartition> = groups
            .into_iter()
            .flat_map(|(child_prefix, child_members)| {
                build(
                    &child_prefix,
                    &child_members,
                    ids,
                    line_bytes,
                    depth_guard - 1,
                )
            })
            .collect();
        let child_bytes: usize = children.iter().map(|c| c.bytes).sum();
        if children.len() > 1 && child_bytes <= SHARD_MERGE {
            // Full-sibling group at or below SHARD_MERGE coalesces into the
            // parent prefix (spec 5.4); the parent's own bytes are the same
            // total, so this only fires well under SHARD_SPLIT.
            return vec![ShardPartition {
                prefix: prefix.to_string(),
                members: members.to_vec(),
                bytes,
            }];
        }
        children
    }

    let partitions = build("", &order, ids, line_bytes, 256);
    Ok(partitions)
}

/// Pack event lines in canonical order into sealed groups plus the open
/// tail (spec 6.1/6.2). Greedy from the front, so boundaries are stable
/// under append: sealed segments never re-split when later events arrive.
/// This is the from-scratch packing for a publication with no usable
/// previous segmentation — a first set-v2 publication (a full rewrite per
/// spec 10) or one whose recorded boundaries no longer describe the live
/// stream.
fn pack_event_segments(line_bytes: &[usize], events: &[EventRecord]) -> Vec<(Vec<usize>, bool)> {
    let mut groups: Vec<(Vec<usize>, bool)> = Vec::new();
    let mut current: Vec<usize> = Vec::new();
    let mut current_bytes = 0usize;
    for (i, &len) in line_bytes.iter().enumerate() {
        let line_len = len + 1;
        if !current.is_empty() && current_bytes + line_len > SEG_SEAL {
            groups.push((std::mem::take(&mut current), true));
            current_bytes = 0;
        }
        current.push(i);
        current_bytes += line_len;
    }
    let tail_sealed = false;
    groups.push((current, tail_sealed));
    let _ = events;
    groups
}

/// The previous publication's event-log segmentation, read from the pointer
/// on disk: publication state derives from the live store and the pointer
/// only (spec 7.4), so the seal boundary comes from what the previous
/// generation recorded, not from re-packing the stream. Sealed segments are
/// immutable for the life of the workspace (spec 6.2), so their rows carry
/// forward verbatim; the open tail is the next seal unit, which makes every
/// seal boundary a publication boundary and the sealed bytes exactly the
/// outgoing tail's bytes — the rename of spec 6.2, whose old file step 4 of
/// the 7.4 procedure removes.
///
/// `None` when there is no set-v2 pointer yet, or when the recorded
/// boundaries no longer describe the live stream — a deletion inside the
/// covered range shifts every later position and fails the end-identity
/// check below. The caller then falls back to from-scratch packing.
struct PreviousSegmentation {
    /// Sealed segment rows carried verbatim into the new manifest
    sealed_rows: Vec<serde_json::Value>,
    /// First canonical-stream index of the open tail's events
    tail_start: usize,
    /// One past the last canonical-stream index the previous generation
    /// covered (sealed segments plus the open tail)
    covered_end: usize,
    /// The open tail's published byte length, authoritative from its row
    tail_bytes: usize,
}

fn previous_segmentation(
    checkpoint_dir: &Path,
    events: &[EventRecord],
) -> Option<PreviousSegmentation> {
    let pointer: serde_json::Value =
        serde_json::from_slice(&std::fs::read(checkpoint_dir.join("current.json")).ok()?).ok()?;
    let manifest_rel = pointer.get("active_root")?.get("path")?.as_str()?;
    if !manifest_rel.starts_with("set/manifest-") {
        return None;
    }
    let manifest: serde_json::Value =
        serde_json::from_slice(&std::fs::read(checkpoint_dir.join(manifest_rel)).ok()?).ok()?;
    if manifest.get("format")?.as_str()? != "checkpoint-set-v2" {
        return None;
    }

    let sealed_rows = manifest.get("segments")?.as_array()?.clone();
    let mut sealed_count = 0usize;
    for row in &sealed_rows {
        let count = row.get("event_count")?.as_u64()? as usize;
        let path = row.get("path")?.as_str()?;
        if path.is_empty()
            || !path.starts_with("set/seg/")
            || row.get("sha256")?.as_str()?.is_empty()
            || !checkpoint_dir.join(path).is_file()
        {
            return None;
        }
        sealed_count += count;
    }

    let tail_row = manifest.get("tail").filter(|v| !v.is_null())?;
    let tail_count = tail_row.get("event_count")?.as_u64()? as usize;
    let tail_bytes = tail_row.get("byte_length")?.as_u64()? as usize;
    let covered_count = sealed_count + tail_count;

    // The recorded end of the covered prefix must still name the same live
    // event at the same position. Events only append past the end, so a
    // match plus count equality means the covered prefix is unchanged; a
    // deletion inside it shifts later positions and fails one of the two.
    let covered_end = if tail_count > 0 {
        tail_row
            .get("origin_range")?
            .get("last")?
            .as_array()?
            .clone()
    } else {
        sealed_rows
            .last()?
            .get("origin_range")?
            .get("last")?
            .as_array()?
            .clone()
    };
    let end_uuid = covered_end.first()?.as_str()?;
    let end_sequence = covered_end.get(1)?.as_i64()?;
    let boundary = events.iter().position(|event| {
        event.origin_store_uuid == end_uuid && event.origin_event_sequence == end_sequence
    })?;
    if boundary + 1 != covered_count {
        return None;
    }

    Some(PreviousSegmentation {
        sealed_rows,
        tail_start: sealed_count,
        covered_end: boundary + 1,
        tail_bytes,
    })
}

/// Write one content-addressed artifact under the `set/` tree, reusing the
/// existing file when its bytes already hash to the requested name.
fn write_set_object(
    checkpoint_dir: &Path,
    subdir: &str,
    scratch_tag: &str,
    body: &[u8],
    changed_paths: &mut Vec<String>,
    scratch_dir: Option<&Path>,
) -> Result<(String, String)> {
    let hash = format!("{:x}", Sha256::digest(body));
    let rel = format!("set/{}/{}.jsonl", subdir, hash);
    write_set_file(
        checkpoint_dir,
        &rel,
        scratch_tag,
        body,
        changed_paths,
        scratch_dir,
    )?;
    Ok((rel, hash))
}

/// Write a `set/` artifact at a fixed path (the tail), or a content-addressed
/// one, with the publication temp/rename/fsync discipline.
fn write_set_file(
    checkpoint_dir: &Path,
    rel: &str,
    scratch_tag: &str,
    body: &[u8],
    changed_paths: &mut Vec<String>,
    scratch_dir: Option<&Path>,
) -> Result<()> {
    let final_path = checkpoint_dir.join(rel);
    if final_path.exists() {
        if let Ok(existing) = std::fs::read(&final_path) {
            if existing.as_slice() == body {
                return Ok(());
            }
        }
    }
    if let Some(parent) = final_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let temp_path = publication_temp_path(
        scratch_dir,
        &final_path.with_extension("tmp"),
        &format!("{}.tmp", scratch_tag),
    );
    let mut file = create_publication_temp(&temp_path, scratch_dir.is_some())?;
    file.write_all(body)?;
    file.sync_all()?;
    drop(file);
    std::fs::rename(&temp_path, &final_path)?;
    if let Some(parent) = final_path.parent() {
        sync_dir(parent)?;
    }
    changed_paths.push(rel.to_string());
    Ok(())
}

/// An oversize line stored once under `set/rec/<sha256>.jsonl`, referenced
/// from its shard or segment (spec 6.3)
fn store_oversize_record(
    checkpoint_dir: &Path,
    line: &[u8],
    changed_paths: &mut Vec<String>,
    scratch_dir: Option<&Path>,
) -> Result<serde_json::Value> {
    let hash = format!("{:x}", Sha256::digest(line));
    let rel = format!("set/rec/{}.jsonl", hash);
    write_set_file(
        checkpoint_dir,
        &rel,
        &format!("rec-{}", &hash[..12]),
        line,
        changed_paths,
        scratch_dir,
    )?;
    let record: serde_json::Value = serde_json::from_slice(line)
        .map_err(|e| anyhow!("oversize record line is not valid JSON: {}", e))?;
    let record_type = record
        .get("record_type")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow!("oversize record line missing record_type"))?
        .to_string();
    Ok(serde_json::json!({
        "record_type": record_type,
        "ref": true,
        "path": rel,
        "sha256": hash,
        "byte_length": line.len(),
    }))
}

/// Append one corpus line to `body`, storing it as an oversize reference
/// when it exceeds `RECORD_INLINE` (spec 6.3)
fn push_record_line(
    body: &mut Vec<u8>,
    oversize: &mut Vec<serde_json::Value>,
    checkpoint_dir: &Path,
    line: &[u8],
    changed_paths: &mut Vec<String>,
    scratch_dir: Option<&Path>,
) -> Result<()> {
    if line.len() > RECORD_INLINE {
        let ref_row = store_oversize_record(checkpoint_dir, line, changed_paths, scratch_dir)?;
        oversize.push(ref_row.clone());
        let mut row = serde_json::to_vec(&ref_row)?;
        row.push(b'\n');
        body.extend_from_slice(&row);
    } else {
        body.extend_from_slice(line);
        body.push(b'\n');
    }
    Ok(())
}

/// Everything a set-v2 publication needs beyond its output paths
pub(super) struct SetV2PublishInputs<'a> {
    pub corpus: &'a SerializedCorpus,
    pub config: ShardedConfig,
    pub scratch_dir: Option<&'a Path>,
}

/// Publish a checkpoint-set-v2 generation (spec 5-7)
///
/// Every keyed-record class partitions independently by record-ID prefix;
/// shards whose bytes are unchanged keep their content address and are not
/// rewritten. Events pack greedily into sealed segments plus one open tail.
/// The manifest tabulates artifacts only; retention is structural (spec
/// 7.3) and is applied by the caller after the pointer swap.
pub(super) fn publish_set_v2_checkpoint(
    inputs: SetV2PublishInputs<'_>,
    events: &[EventRecord],
    checkpoint_dir: &Path,
    changed_paths: &mut Vec<String>,
) -> Result<PublicationOutput> {
    let SetV2PublishInputs {
        corpus,
        config,
        scratch_dir,
    } = inputs;
    let mut referenced_paths: Vec<String> = Vec::new();

    // ---- Keyed-record shards: one independent prefix partition per class ----

    let issue_ids: Vec<String> = corpus
        .issue_lines
        .iter()
        .map(|l| keyed_record_id(l))
        .collect::<Result<_>>()?;
    let issue_line_bytes: Vec<usize> = corpus.issue_lines.iter().map(|l| l.len()).collect();
    let issue_partitions = partition_keyed_class(&issue_ids, &issue_line_bytes)?;

    let mut shard_rows: Vec<serde_json::Value> = Vec::new();
    let write_class_shards = |class: &'static str,
                              partitions: Vec<ShardPartition>,
                              lines: &[Vec<u8>],
                              shard_rows: &mut Vec<serde_json::Value>,
                              referenced_paths: &mut Vec<String>,
                              changed_paths: &mut Vec<String>|
     -> Result<()> {
        for partition in partitions {
            let mut body = Vec::new();
            let mut oversize = Vec::new();
            for &m in &partition.members {
                push_record_line(
                    &mut body,
                    &mut oversize,
                    checkpoint_dir,
                    &lines[m],
                    changed_paths,
                    scratch_dir,
                )?;
            }
            for row in oversize {
                // Reference rows live in the shard; their payloads were
                // already written to set/rec/ by push_record_line.
                referenced_paths.push(
                    row.get("path")
                        .and_then(|v| v.as_str())
                        .unwrap_or_default()
                        .to_string(),
                );
            }
            let (rel, hash) = write_set_object(
                checkpoint_dir,
                "shard",
                &format!("shard-{}-{}", class, partition.prefix.replace('/', "_")),
                &body,
                changed_paths,
                scratch_dir,
            )?;
            referenced_paths.push(rel.clone());
            shard_rows.push(serde_json::json!({
                "class": class,
                "prefix": partition.prefix,
                "path": rel,
                "sha256": hash,
                "byte_length": body.len(),
                "record_count": partition.members.len(),
            }));
        }
        Ok(())
    };

    write_class_shards(
        "issue",
        issue_partitions,
        &corpus.issue_lines,
        &mut shard_rows,
        &mut referenced_paths,
        changed_paths,
    )?;

    for (class, lines) in [
        ("provenance_receipt", &corpus.receipt_lines),
        ("attempt_outcome", &corpus.attempt_outcome_lines),
        ("redaction", &corpus.redaction_lines),
    ] {
        if lines.is_empty() {
            continue;
        }
        let ids: Vec<String> = lines
            .iter()
            .map(|l| keyed_record_id(l))
            .collect::<Result<_>>()?;
        let line_bytes: Vec<usize> = lines.iter().map(|l| l.len()).collect();
        let partitions = partition_keyed_class(&ids, &line_bytes)?;
        write_class_shards(
            class,
            partitions,
            lines,
            &mut shard_rows,
            &mut referenced_paths,
            changed_paths,
        )?;
    }

    // ---- Event log: sealed segments plus the open tail (spec 6) ----

    let mut segment_rows: Vec<serde_json::Value> = Vec::new();
    // The manifest never records two sealed segments with the same digest
    // (spec 6.2); content addressing makes a repeat byte-identical, so the
    // row is dropped rather than duplicated.
    let mut segment_digests: Vec<String> = Vec::new();
    let event_line_bytes: Vec<usize> = corpus.event_lines.iter().map(|l| l.len()).collect();
    let previous = previous_segmentation(checkpoint_dir, events);
    let groups: Vec<(Vec<usize>, bool)> = match &previous {
        Some(prev) => {
            // Sealed segments carry forward immutably (spec 6.2); the open
            // tail is the next seal unit. It seals whole when this
            // publication's appends would push it past SEG_SEAL — so the
            // sealed bytes are exactly the outgoing tail's bytes, the
            // rename of spec 6.2 — or when it already exceeded SEG_SEAL by
            // itself, so a repeated publication finishes the seal.
            let new_bytes: usize = (prev.covered_end..event_line_bytes.len())
                .map(|i| event_line_bytes[i] + 1)
                .sum();
            let tail_seals = prev.covered_end > prev.tail_start
                && (prev.tail_bytes + new_bytes > SEG_SEAL || prev.tail_bytes > SEG_SEAL);
            let tail: Vec<usize> = (prev.tail_start..prev.covered_end).collect();
            let appended: Vec<usize> = (prev.covered_end..event_line_bytes.len()).collect();
            if tail_seals {
                vec![(tail, true), (appended, false)]
            } else {
                let mut members = tail;
                members.extend(appended);
                vec![(members, false)]
            }
        }
        None => pack_event_segments(&event_line_bytes, events),
    };
    if let Some(prev) = &previous {
        for row in &prev.sealed_rows {
            let sha = row
                .get("sha256")
                .and_then(|v| v.as_str())
                .unwrap_or_default();
            if segment_digests.iter().any(|s| s == sha) {
                continue;
            }
            segment_digests.push(sha.to_string());
            segment_rows.push(row.clone());
            if let Some(rel) = row.get("path").and_then(|v| v.as_str()) {
                referenced_paths.push(rel.to_string());
            }
        }
    }
    let mut tail_row: Option<serde_json::Value> = None;
    for (group_index, (members, sealed)) in groups.iter().enumerate() {
        let mut body = Vec::new();
        let mut oversize = Vec::new();
        for &i in members {
            push_record_line(
                &mut body,
                &mut oversize,
                checkpoint_dir,
                &corpus.event_lines[i],
                changed_paths,
                scratch_dir,
            )?;
        }
        for row in oversize {
            referenced_paths.push(
                row.get("path")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default()
                    .to_string(),
            );
        }
        let sequence_range = match (members.first(), members.last()) {
            (Some(&first), Some(&last)) => {
                serde_json::json!([
                    events[first].origin_event_sequence,
                    events[last].origin_event_sequence
                ])
            }
            _ => serde_json::json!([]),
        };
        let origin_range = match (members.first(), members.last()) {
            (Some(&first), Some(&last)) => serde_json::json!({
                "first": [events[first].origin_store_uuid, events[first].origin_event_sequence],
                "last": [events[last].origin_store_uuid, events[last].origin_event_sequence],
            }),
            _ => serde_json::json!(null),
        };
        if *sealed {
            let (rel, hash) = write_set_object(
                checkpoint_dir,
                "seg",
                &format!("seg-{}", group_index),
                &body,
                changed_paths,
                scratch_dir,
            )?;
            referenced_paths.push(rel.clone());
            if segment_digests.contains(&hash) {
                continue;
            }
            segment_digests.push(hash.clone());
            segment_rows.push(serde_json::json!({
                "path": rel,
                "sha256": hash,
                "byte_length": body.len(),
                "event_count": members.len(),
                "sequence_range": sequence_range,
                "origin_range": origin_range,
            }));
        } else {
            // The one open tail; rewritten by every publication that has
            // events at all, so its fixed path never content-addresses.
            write_set_file(
                checkpoint_dir,
                "set/tail.jsonl",
                "tail",
                &body,
                changed_paths,
                scratch_dir,
            )?;
            let hash = format!("{:x}", Sha256::digest(&body));
            referenced_paths.push("set/tail.jsonl".to_string());
            tail_row = Some(serde_json::json!({
                "path": "set/tail.jsonl",
                "sha256": hash,
                "byte_length": body.len(),
                "event_count": members.len(),
                "sequence_range": sequence_range,
                "origin_range": origin_range,
            }));
        }
    }

    // ---- Manifest: table rows only, never record payloads (spec 7.2) ----

    let total_records = corpus.issue_lines.len()
        + corpus.event_lines.len()
        + corpus.receipt_lines.len()
        + corpus.attempt_outcome_lines.len()
        + corpus.redaction_lines.len();
    let manifest = serde_json::json!({
        "format": "checkpoint-set-v2",
        "schema_version": 1,
        "store_uuid": config.store_uuid,
        "snapshot_sequence": config.snapshot_sequence,
        "max_local_ingestion_sequence": config.snapshot_sequence,
        "created_at": format_rfc3339(SystemTime::now()),
        "profile": "native-v1",
        "bounds": bounds_json(),
        "issue_count": corpus.issue_lines.len(),
        "event_count": corpus.event_lines.len(),
        "receipt_count": corpus.receipt_lines.len(),
        "attempt_outcome_count": corpus.attempt_outcome_lines.len(),
        "redaction_record_count": corpus.redaction_lines.len(),
        "total_record_count": total_records,
        "shards": shard_rows,
        "segments": segment_rows,
        "tail": tail_row,
    });
    let manifest_json = serde_json::to_vec_pretty(&manifest)?;
    let manifest_hash = format!("{:x}", Sha256::digest(&manifest_json));
    let manifest_rel = format!("set/manifest-{}.json", config.generation_id);
    write_set_file(
        checkpoint_dir,
        &manifest_rel,
        "manifest",
        &manifest_json,
        changed_paths,
        scratch_dir,
    )?;
    referenced_paths.push(manifest_rel.clone());

    Ok(PublicationOutput {
        root_hash: manifest_hash,
        root_path: manifest_rel,
        referenced_paths,
    })
}

/// Every checkpoint-relative file a set-v2 pointer's generation selects:
/// the manifest itself plus every artifact row it tabulates (spec 7.3).
/// Best effort, matching the pointer readers' degrade-safely rule.
pub(super) fn set_v2_structural_referenced_files(
    checkpoint_dir: &Path,
    manifest_rel: &str,
) -> Vec<String> {
    let mut files = vec![manifest_rel.to_string()];
    let Ok(data) = std::fs::read_to_string(checkpoint_dir.join(manifest_rel)) else {
        return files;
    };
    let Ok(manifest) = serde_json::from_str::<serde_json::Value>(&data) else {
        return files;
    };
    if let Some(shards) = manifest.get("shards").and_then(|v| v.as_array()) {
        files.extend(
            shards
                .iter()
                .filter_map(|row| row.get("path").and_then(|v| v.as_str()))
                .map(str::to_string),
        );
    }
    if let Some(segments) = manifest.get("segments").and_then(|v| v.as_array()) {
        files.extend(
            segments
                .iter()
                .filter_map(|row| row.get("path").and_then(|v| v.as_str()))
                .map(str::to_string),
        );
    }
    if let Some(path) = manifest
        .get("tail")
        .filter(|v| !v.is_null())
        .and_then(|t| t.get("path"))
        .and_then(|v| v.as_str())
    {
        files.push(path.to_string());
    }
    files
}

/// Whether a checkpoint-relative path may be reaped by structural retention
pub(super) fn is_set_v2_artifact_path(path: &str) -> bool {
    path.starts_with("set/")
        && !path.contains("..")
        && !path.contains('\\')
        && path != "set/"
        && !path.ends_with('/')
}

/// Files currently on disk under the `set/` tree, for structural retention
pub(super) fn enumerate_set_v2_artifacts(checkpoint_dir: &Path) -> Result<Vec<String>> {
    let set_dir = checkpoint_dir.join("set");
    let mut paths = Vec::new();
    if !set_dir.exists() {
        return Ok(paths);
    }
    let mut stack = vec![set_dir.clone()];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir)? {
            let entry = entry?;
            let file_type = entry.file_type()?;
            let name = entry.file_name();
            let name = name.to_str().ok_or_else(|| {
                anyhow!("non-UTF-8 set-v2 artifact name: {}", entry.path().display())
            })?;
            if file_type.is_dir() {
                stack.push(entry.path());
                continue;
            }
            if !file_type.is_file() || name.ends_with(".tmp") {
                continue;
            }
            let rel = entry
                .path()
                .strip_prefix(checkpoint_dir)
                .map_err(|e| anyhow!("set artifact escapes checkpoint dir: {}", e))?
                .to_string_lossy()
                .to_string();
            paths.push(rel);
        }
    }
    Ok(paths)
}

/// Read and verify a set-v2 artifact's bytes against its recorded digest
pub(super) fn read_verified_set_artifact(
    checkpoint_dir: &Path,
    rel: &str,
    expected_sha256: &str,
    expected_len: Option<u64>,
) -> Result<Vec<u8>> {
    if !is_set_v2_artifact_path(rel) {
        bail!("set-v2 artifact path is invalid: {}", rel);
    }
    let data = std::fs::read(checkpoint_dir.join(rel))
        .map_err(|e| anyhow!("failed to read set-v2 artifact {}: {}", rel, e))?;
    if let Some(len) = expected_len {
        if data.len() as u64 != len {
            bail!(
                "set-v2 artifact {} is {} bytes, manifest records {}",
                rel,
                data.len(),
                len
            );
        }
    }
    let hash = format!("{:x}", Sha256::digest(&data));
    if hash != expected_sha256 {
        bail!(
            "set-v2 artifact {} hashes to {}, manifest records {}",
            rel,
            hash,
            expected_sha256
        );
    }
    Ok(data)
}

/// Verify the complete set-v2 closure a restore pointer selects (spec 13):
/// the manifest's identity and counts against the pointer, then every
/// tabulated artifact's existence, digest, and byte length.
pub(super) fn verify_set_v2_restore_closure(
    base: &Path,
    manifest_path: &Path,
    pointer: &super::RestorePointer,
) -> Result<()> {
    let bytes = std::fs::read(manifest_path)?;
    let manifest: serde_json::Value = serde_json::from_slice(&bytes)
        .map_err(|error| anyhow!("Unverified restore source: invalid set-v2 manifest: {error}"))?;
    super::reject_archaeology_view(&manifest, manifest_path)?;

    if manifest.get("format").and_then(|v| v.as_str()) != Some("checkpoint-set-v2")
        || manifest.get("schema_version").and_then(|v| v.as_u64()) != Some(1)
    {
        bail!("Unverified restore source: unsupported or incomplete set-v2 manifest metadata");
    }
    if manifest.get("store_uuid").and_then(|v| v.as_str()) != Some(pointer.store_uuid.as_str())
        || manifest.get("snapshot_sequence").and_then(|v| v.as_i64())
            != Some(pointer.snapshot_sequence)
    {
        bail!("Unverified restore source: manifest identity or snapshot sequence disagrees with pointer");
    }

    let counts = [
        ("issue_count", pointer.issue_count),
        ("event_count", pointer.event_count),
        ("receipt_count", pointer.receipt_count),
        ("attempt_outcome_count", pointer.attempt_outcome_count),
        ("redaction_record_count", pointer.redaction_record_count),
    ];
    for (field, pointer_count) in counts {
        let manifest_count = manifest.get(field).and_then(|v| v.as_u64()).unwrap_or(0) as usize;
        if manifest_count != pointer_count {
            bail!(
                "Unverified restore source: manifest {field} {} disagrees with pointer {}",
                manifest_count,
                pointer_count
            );
        }
    }

    let mut class_totals: BTreeMap<&str, usize> = BTreeMap::new();
    let empty = Vec::new();
    let shards = manifest
        .get("shards")
        .and_then(|v| v.as_array())
        .unwrap_or(&empty);
    if shards.is_empty() && pointer.total_record_count != pointer.event_count {
        bail!("Unverified restore source: set-v2 manifest tabulates no keyed-record shards");
    }
    for row in shards {
        let class = row
            .get("class")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow!("Unverified restore source: shard row missing class"))?;
        let rel = row
            .get("path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow!("Unverified restore source: shard row missing path"))?;
        let sha = row
            .get("sha256")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow!("Unverified restore source: shard row missing sha256"))?;
        let len = row.get("byte_length").and_then(|v| v.as_u64());
        read_verified_set_artifact(base, rel, sha, len).map_err(|error| {
            anyhow!(
                "Unverified restore source: shard {} unusable: {}",
                rel,
                error
            )
        })?;
        *class_totals.entry(class).or_insert(0) += row
            .get("record_count")
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as usize;
    }
    for (class, expected) in [
        ("issue", pointer.issue_count),
        ("provenance_receipt", pointer.receipt_count),
        ("attempt_outcome", pointer.attempt_outcome_count),
        ("redaction", pointer.redaction_record_count),
    ] {
        if class_totals.get(class).copied().unwrap_or(0) != expected {
            bail!(
                "Unverified restore source: set-v2 {} shard rows total {}, pointer records {}",
                class,
                class_totals.get(class).copied().unwrap_or(0),
                expected
            );
        }
    }

    let mut event_artifacts = 0usize;
    let segments = manifest
        .get("segments")
        .and_then(|v| v.as_array())
        .unwrap_or(&empty);
    for row in segments.iter() {
        let rel = row
            .get("path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow!("Unverified restore source: segment row missing path"))?;
        let sha = row
            .get("sha256")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow!("Unverified restore source: segment row missing sha256"))?;
        read_verified_set_artifact(
            base,
            rel,
            sha,
            row.get("byte_length").and_then(|v| v.as_u64()),
        )
        .map_err(|error| {
            anyhow!(
                "Unverified restore source: segment {} unusable: {}",
                rel,
                error
            )
        })?;
        event_artifacts += 1;
    }
    if let Some(tail) = manifest.get("tail").filter(|v| !v.is_null()) {
        let rel = tail
            .get("path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow!("Unverified restore source: tail row missing path"))?;
        let sha = tail
            .get("sha256")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow!("Unverified restore source: tail row missing sha256"))?;
        read_verified_set_artifact(
            base,
            rel,
            sha,
            tail.get("byte_length").and_then(|v| v.as_u64()),
        )
        .map_err(|error| {
            anyhow!(
                "Unverified restore source: tail {} unusable: {}",
                rel,
                error
            )
        })?;
        event_artifacts += 1;
    }
    if pointer.event_count > 0 && event_artifacts == 0 {
        bail!(
            "Unverified restore source: set-v2 manifest tabulates no event artifacts for {} events",
            pointer.event_count
        );
    }
    Ok(())
}

/// Stage a set-v2 checkpoint into the shared forensic staging shape: shard
/// rows first, then sealed segments in manifest order, then the tail.
pub(super) fn stage_set_v2_checkpoint(pointer_path: &Path) -> Result<super::ForensicStaging> {
    use super::{process_checkpoint_file, AttemptOutcomeRecord, CheckpointMode, RedactionRecords};

    let base = pointer_path
        .parent()
        .ok_or_else(|| anyhow!("Pointer has no parent directory"))?;

    let pointer_data = std::fs::read_to_string(pointer_path)?;
    let pointer: serde_json::Value =
        serde_json::from_str(&pointer_data).map_err(|e| anyhow!("Invalid pointer JSON: {}", e))?;
    super::reject_archaeology_view(&pointer, pointer_path)?;

    let manifest_rel = pointer
        .get("active_root")
        .and_then(|r| r.get("path"))
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow!("Pointer missing active_root path"))?;
    let manifest_sha = pointer
        .get("active_root")
        .and_then(|r| r.get("sha256"))
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow!("Pointer missing active_root sha256"))?;
    let manifest_bytes = read_verified_set_artifact(base, manifest_rel, manifest_sha, None)?;
    let manifest: serde_json::Value = serde_json::from_slice(&manifest_bytes)
        .map_err(|e| anyhow!("Invalid set-v2 manifest JSON: {}", e))?;
    super::reject_archaeology_view(&manifest, &base.join(manifest_rel))?;

    let store_uuid = manifest
        .get("store_uuid")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow!("Manifest missing store_uuid"))?
        .to_string();
    let snapshot_sequence = manifest
        .get("snapshot_sequence")
        .and_then(|v| v.as_i64())
        .ok_or_else(|| anyhow!("Manifest missing snapshot_sequence"))?;

    let mut hasher = Sha256::new();
    let mut issues = Vec::new();
    let mut dependencies = Vec::new();
    let mut labels = Vec::new();
    let mut events = Vec::new();
    let mut receipts = Vec::new();
    let mut attempt_outcomes: Vec<AttemptOutcomeRecord> = Vec::new();
    let mut redaction = RedactionRecords::default();

    let mut seen_issue_ids = std::collections::HashSet::new();
    let mut seen_event_identities = std::collections::HashSet::new();
    let mut seen_receipt_ids = std::collections::HashSet::new();
    let mut seen_attempt_ids = std::collections::HashSet::new();

    let process_artifact = |rel: &str,
                            sha: &str,
                            len: Option<u64>,
                            hasher: &mut Sha256,
                            issues: &mut Vec<_>,
                            dependencies: &mut Vec<_>,
                            labels: &mut Vec<_>,
                            events: &mut Vec<_>,
                            receipts: &mut Vec<_>,
                            attempt_outcomes: &mut Vec<_>,
                            redaction: &mut RedactionRecords,
                            seen_issue_ids: &mut std::collections::HashSet<String>,
                            seen_event_identities: &mut std::collections::HashSet<String>,
                            seen_receipt_ids: &mut std::collections::HashSet<String>,
                            _seen_attempt_ids: &mut std::collections::HashSet<String>|
     -> Result<()> {
        // Digest before use (spec 13); the parse then reads the same bytes.
        read_verified_set_artifact(base, rel, sha, len)?;
        let path = base.join(rel);
        let data = process_checkpoint_file(
            &path,
            Some(base),
            hasher,
            seen_issue_ids,
            seen_event_identities,
            seen_receipt_ids,
        )?;
        issues.extend(data.issues);
        dependencies.extend(data.dependencies);
        labels.extend(data.labels);
        events.extend(data.events);
        receipts.extend(data.receipts);
        attempt_outcomes.extend(data.attempt_outcomes);
        redaction.findings.extend(data.redaction.findings);
        redaction
            .acknowledgments
            .extend(data.redaction.acknowledgments);
        redaction.receipts.extend(data.redaction.receipts);
        redaction.epochs.extend(data.redaction.epochs);
        redaction.tombstones.extend(data.redaction.tombstones);
        Ok(())
    };

    if let Some(shards) = manifest.get("shards").and_then(|v| v.as_array()) {
        for row in shards {
            let rel = row
                .get("path")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow!("Shard row missing path"))?;
            let sha = row
                .get("sha256")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow!("Shard row missing sha256"))?;
            process_artifact(
                rel,
                sha,
                row.get("byte_length").and_then(|v| v.as_u64()),
                &mut hasher,
                &mut issues,
                &mut dependencies,
                &mut labels,
                &mut events,
                &mut receipts,
                &mut attempt_outcomes,
                &mut redaction,
                &mut seen_issue_ids,
                &mut seen_event_identities,
                &mut seen_receipt_ids,
                &mut seen_attempt_ids,
            )?;
        }
    }
    if let Some(segments) = manifest.get("segments").and_then(|v| v.as_array()) {
        for row in segments {
            let rel = row
                .get("path")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow!("Segment row missing path"))?;
            let sha = row
                .get("sha256")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow!("Segment row missing sha256"))?;
            process_artifact(
                rel,
                sha,
                row.get("byte_length").and_then(|v| v.as_u64()),
                &mut hasher,
                &mut issues,
                &mut dependencies,
                &mut labels,
                &mut events,
                &mut receipts,
                &mut attempt_outcomes,
                &mut redaction,
                &mut seen_issue_ids,
                &mut seen_event_identities,
                &mut seen_receipt_ids,
                &mut seen_attempt_ids,
            )?;
        }
    }
    if let Some(tail) = manifest.get("tail").filter(|v| !v.is_null()) {
        let rel = tail
            .get("path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow!("Tail row missing path"))?;
        let sha = tail
            .get("sha256")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow!("Tail row missing sha256"))?;
        process_artifact(
            rel,
            sha,
            tail.get("byte_length").and_then(|v| v.as_u64()),
            &mut hasher,
            &mut issues,
            &mut dependencies,
            &mut labels,
            &mut events,
            &mut receipts,
            &mut attempt_outcomes,
            &mut redaction,
            &mut seen_issue_ids,
            &mut seen_event_identities,
            &mut seen_receipt_ids,
            &mut seen_attempt_ids,
        )?;
    }

    // Canonical order is a property of the staged corpus (see the sharded
    // stager): shards and segments may be listed in any order.
    issues.sort_by(|a, b| a.id.cmp(&b.id));
    dependencies.sort();
    labels.sort();
    events.sort_by(|a, b| {
        (a.origin_store_uuid.as_str(), a.origin_event_sequence)
            .cmp(&(b.origin_store_uuid.as_str(), b.origin_event_sequence))
    });
    receipts.sort_by(|a, b| a.receipt_id.cmp(&b.receipt_id));
    super::heal_legacy_bare_interchange_receipts(&mut receipts);
    attempt_outcomes.sort_by(|a, b| a.receipt_id.cmp(&b.receipt_id));
    redaction.ensure_unique_identities()?;
    redaction.sort();

    Ok(super::ForensicStaging {
        issue_count: seen_issue_ids.len(),
        event_count: seen_event_identities.len(),
        receipt_count: seen_receipt_ids.len(),
        attempt_outcome_count: seen_attempt_ids.len(),
        redaction_record_count: redaction.len(),
        issues,
        dependencies,
        labels,
        events,
        receipts,
        attempt_outcomes,
        redaction,
        input_hash: format!("{:x}", hasher.finalize()),
        store_uuid,
        snapshot_sequence,
        mode: CheckpointMode::SetV2,
        pointer_extensions: super::RecordExtensions::new(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(record_type: &str, payload: serde_json::Value) -> Vec<u8> {
        let mut record = serde_json::Map::new();
        record.insert("record_type".into(), serde_json::json!(record_type));
        record.insert(record_type.into(), payload);
        serde_json::to_vec(&serde_json::Value::Object(record)).expect("test record serializes")
    }

    #[test]
    fn tombstone_partitions_on_its_own_id_not_the_embedded_epoch() {
        // One committed redaction emits an epoch and a tombstone sharing the
        // epoch_id; each must partition on its own identity or the redaction
        // class partition refuses the pair as duplicate record IDs.
        let epoch = line(
            "redaction_epoch",
            serde_json::json!({"epoch_id": "5ed7", "receipt_ids": []}),
        );
        let tombstone = line(
            "redaction_tombstone",
            serde_json::json!({"tombstone_id": "73f2", "epoch_id": "5ed7"}),
        );
        assert_eq!(keyed_record_id(&epoch).unwrap(), "5ed7");
        assert_eq!(keyed_record_id(&tombstone).unwrap(), "73f2");
    }
}
