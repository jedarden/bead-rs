//! Operator configuration of the checkpoint section of `.beads/config.json`
//!
//! Plan 6.1.1 lets operators force monolithic or sharded output and makes
//! the size thresholds versioned configuration. `bead sync configure` is the
//! supported way to change them: it rewrites only the `checkpoint` section,
//! validates the proposed document with the same parser every later load
//! applies, writes it atomically, and publishes a generation under the new
//! configuration while holding the checkpoint publication lock. If that
//! publication fails, the previous configuration bytes are restored, so the
//! workspace never keeps a configuration it could not publish under.

use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, bail, Context, Result};
use serde_json::{Map, Value};

use super::checkpoint::{
    acquire_checkpoint_publication_lock, parse_checkpoint_config_document,
    publish_forensic_checkpoint_holding, CheckpointThresholds, ForensicFlushResult,
};
use crate::store::SqliteStore;

/// Smallest accepted per-object byte limit. Smaller objects would multiply
/// file counts without serving any tooling limit seen in practice.
pub const MIN_OBJECT_BYTES: u64 = 1024 * 1024;

/// The `checkpoint.mode` values an operator may record
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModeSetting {
    /// Select from the thresholds (the default when no mode is recorded)
    Adaptive,
    /// Force monolithic output (still refused above the safety limits)
    Monolithic,
    /// Force sharded output
    Sharded,
}

impl ModeSetting {
    pub fn as_str(self) -> &'static str {
        match self {
            ModeSetting::Adaptive => "adaptive",
            ModeSetting::Monolithic => "monolithic",
            ModeSetting::Sharded => "sharded",
        }
    }

    pub fn parse(value: &str) -> Result<Self> {
        match value {
            "adaptive" => Ok(ModeSetting::Adaptive),
            "monolithic" => Ok(ModeSetting::Monolithic),
            "sharded" => Ok(ModeSetting::Sharded),
            other => bail!(
                "unknown checkpoint mode {:?}: expected adaptive, monolithic, or sharded",
                other
            ),
        }
    }
}

/// What the operator asked to change
#[derive(Debug, Clone, Copy, Default)]
pub struct CheckpointConfigChange {
    pub mode: Option<ModeSetting>,
    /// Upper bound for every issue-shard and event object, in bytes
    pub max_object_bytes: Option<u64>,
}

/// A validated, not-yet-written configuration change
#[derive(Debug, Clone)]
pub struct PlannedConfigChange {
    /// The `checkpoint` section before the change (`None` when absent)
    pub before: Option<Value>,
    /// The `checkpoint` section after the change
    pub after: Value,
    /// Whether the section actually changes
    pub changed: bool,
    config_path: PathBuf,
    original_bytes: Vec<u8>,
    proposed_bytes: Vec<u8>,
}

/// Outcome of an applied change
#[derive(Debug)]
pub struct AppliedConfigChange {
    pub plan: PlannedConfigChange,
    /// The generation published under the new configuration; `None` when
    /// the section was already as requested and nothing was written
    pub publication: Option<ForensicFlushResult>,
}

/// Compute and validate the change without writing anything
pub fn plan_checkpoint_config_change(
    beads_dir: &Path,
    change: &CheckpointConfigChange,
) -> Result<PlannedConfigChange> {
    if change.mode.is_none() && change.max_object_bytes.is_none() {
        bail!("nothing to configure: pass --mode and/or --max-object-bytes");
    }
    if let Some(bytes) = change.max_object_bytes {
        if bytes < MIN_OBJECT_BYTES {
            bail!(
                "--max-object-bytes {} is below the minimum of {} bytes",
                bytes,
                MIN_OBJECT_BYTES
            );
        }
    }

    let config_path = beads_dir.join("config.json");
    let original_bytes =
        fs::read(&config_path).with_context(|| format!("cannot read {}", config_path.display()))?;
    let mut document: Value = serde_json::from_slice(&original_bytes)
        .map_err(|e| anyhow!("Invalid .beads/config.json: {}", e))?;
    let current = parse_checkpoint_config_document(&document)?;

    let root = document
        .as_object_mut()
        .ok_or_else(|| anyhow!(".beads/config.json must be a JSON object"))?;
    let before = root.get("checkpoint").filter(|v| !v.is_null()).cloned();
    let mut section: Map<String, Value> = match &before {
        None => Map::new(),
        Some(Value::Object(map)) => map.clone(),
        Some(_) => bail!(".beads/config.json checkpoint must be an object"),
    };

    if let Some(mode) = change.mode {
        section.insert("mode".to_string(), Value::String(mode.as_str().to_string()));
    }
    if let Some(bytes) = change.max_object_bytes {
        // On top of the configured table, or the plan 6.1.1 defaults when
        // the workspace has none: only the object bounds move. A record line
        // can never exceed the object that holds it, so its limit is capped
        // at the same value.
        let mut thresholds: CheckpointThresholds = current.thresholds.unwrap_or_default();
        thresholds.max_shard_bytes = bytes;
        thresholds.max_event_object_bytes = bytes;
        thresholds.max_record_line_bytes = thresholds.max_record_line_bytes.min(bytes);
        section.insert(
            "thresholds".to_string(),
            serde_json::to_value(thresholds).expect("thresholds serialize to JSON"),
        );
    }

    let after = Value::Object(section);
    root.insert("checkpoint".to_string(), after.clone());

    // Validate with the loader's own rules before anything touches disk.
    parse_checkpoint_config_document(&document)
        .context("the proposed checkpoint configuration is invalid")?;

    let proposed_bytes = serde_json::to_vec(&document).expect("config serializes to JSON");
    let changed = before.as_ref() != Some(&after);

    Ok(PlannedConfigChange {
        before,
        after,
        changed,
        config_path,
        original_bytes,
        proposed_bytes,
    })
}

/// Write the planned configuration and publish a generation under it
///
/// Holds the checkpoint publication lock across the write and the
/// publication, so no concurrent publisher interleaves with a half-applied
/// change. A failed publication restores the original configuration bytes
/// and returns the publication error.
pub fn apply_checkpoint_config_change(
    store: &mut SqliteStore,
    beads_dir: &Path,
    plan: PlannedConfigChange,
) -> Result<AppliedConfigChange> {
    if !plan.changed {
        return Ok(AppliedConfigChange {
            plan,
            publication: None,
        });
    }

    let publication_lock = acquire_checkpoint_publication_lock(&beads_dir.join("checkpoint"))?;
    write_atomically(&plan.config_path, &plan.proposed_bytes)?;

    let config = parse_checkpoint_config_document(&serde_json::from_slice(&plan.proposed_bytes)?)?;
    match publish_forensic_checkpoint_holding(&publication_lock, store, &config, beads_dir) {
        Ok(publication) => Ok(AppliedConfigChange {
            plan,
            publication: Some(publication),
        }),
        Err(publish_error) => {
            if let Err(restore_error) = write_atomically(&plan.config_path, &plan.original_bytes) {
                bail!(
                    "publication under the new checkpoint configuration failed ({:#}) and \
                     restoring the previous .beads/config.json also failed ({:#})",
                    publish_error,
                    restore_error
                );
            }
            Err(publish_error.context(
                "publication under the new checkpoint configuration failed; \
                 the previous .beads/config.json was restored",
            ))
        }
    }
}

/// Replace `path` with `bytes` via a synced temporary file and rename
fn write_atomically(path: &Path, bytes: &[u8]) -> Result<()> {
    let dir = path
        .parent()
        .ok_or_else(|| anyhow!("{} has no parent directory", path.display()))?;
    let temp_path = dir.join(".config.json.configure.tmp");
    {
        let mut temp = File::create(&temp_path)
            .with_context(|| format!("cannot create {}", temp_path.display()))?;
        temp.write_all(bytes)?;
        temp.sync_all()?;
    }
    fs::rename(&temp_path, path).with_context(|| format!("cannot replace {}", path.display()))?;
    File::open(dir)?.sync_all()?;
    Ok(())
}
