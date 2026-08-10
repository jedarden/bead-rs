//! Profile migration service with dry-run and receipt support
//!
//! This module implements F013 migration functionality:
//! - Transform checkpoints between profiles
//! - Dry-run validation without state activation
//! - Canonical migration receipts with hashes and transformation counts
//! - Non-overwriting path validation

use crate::model::Issue;
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs::File;
use std::io::{BufRead, BufReader, BufWriter, Read, Write};
use std::path::{Path, PathBuf};

// Import profile types - available in both library and binary contexts
use crate::profile::{
    get_adapter, is_supported, native_v1, LossSeverity, ProfileAdapter, TransformResult,
};

const MAX_ISSUE_RECORDS: usize = 1_000_000;
const MAX_BYTES_PER_LINE: usize = 16 * 1024 * 1024; // 16 MiB
const MAX_TOTAL_BYTES: usize = 4 * 1024 * 1024 * 1024; // 4 GiB

/// Migration receipt schema reference
pub const MIGRATION_RECEIPT_SCHEMA: &str = "urn:bead-rs:schema:migration-receipt:native-v1";

/// Migration receipt
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MigrationReceipt {
    pub schema_ref: String,
    pub tool_version: String,
    pub timestamp: String,
    pub source_profile: String,
    pub target_profile: String,
    pub input_sha256: String,
    pub output_sha256: String,
    pub record_counts: RecordCounts,
    pub transformation_counts: TransformationCounts,
    pub warnings: Vec<String>,
    pub dry_run: bool,
    pub successful: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecordCounts {
    pub total_issues: usize,
    pub input_issues: usize,
    pub output_issues: usize,
    pub total_lines: usize,
    pub blank_lines: usize,
    pub malformed_lines: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransformationCounts {
    pub transformed_issues: usize,
    pub preserved_issues: usize,
    pub total_transformations: usize,
    pub loss_entries: usize,
}

/// Migration preview for dry-run
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MigrationPreview {
    pub schema_ref: String,
    pub tool_version: String,
    pub timestamp: String,
    pub source_profile: String,
    pub target_profile: String,
    pub input_sha256: String,
    pub record_counts: RecordCounts,
    pub transformation_counts: TransformationCounts,
    pub warnings: Vec<String>,
    pub dry_run: bool,
    pub prospective: bool,
    pub successful: bool,
}

/// Migration options
pub struct MigrationOptions {
    pub from_profile: String,
    pub to_profile: String,
    pub input_path: PathBuf,
    pub output_path: PathBuf,
    pub receipt_path: Option<PathBuf>,
    pub dry_run: bool,
}

impl MigrationOptions {
    /// Validate migration options
    fn validate(&self) -> Result<()> {
        // Validate profiles are supported
        if !is_supported(&self.from_profile) {
            bail!("Unsupported source profile: {}", self.from_profile);
        }
        if !is_supported(&self.to_profile) {
            bail!("Unsupported target profile: {}", self.to_profile);
        }

        // Validate paths are distinct
        if self.input_path == self.output_path {
            bail!("Input and output paths must be distinct");
        }
        if let Some(ref receipt_path) = self.receipt_path {
            if receipt_path == &self.input_path || receipt_path == &self.output_path {
                bail!("Receipt path must be distinct from input and output paths");
            }
        }

        // Validate input exists
        if !self.input_path.exists() {
            bail!("Input file does not exist: {}", self.input_path.display());
        }

        // Validate output does not exist
        if self.output_path.exists() {
            bail!("Output file must not exist: {}", self.output_path.display());
        }

        // Validate receipt does not exist if specified
        if let Some(ref receipt_path) = self.receipt_path {
            if receipt_path.exists() {
                bail!("Receipt file must not exist: {}", receipt_path.display());
            }
        }

        // Validate output is not workspace-managed
        if is_workspace_managed(&self.output_path) {
            bail!("Output path must not be a workspace-managed file");
        }

        Ok(())
    }
}

/// Check if path is workspace-managed
fn is_workspace_managed(path: &Path) -> bool {
    if let Ok(current_dir) = std::env::current_dir() {
        if let Ok(workspace) = find_workspace_root(&current_dir) {
            let beads_dir = workspace.join(".beads");
            return path.starts_with(&beads_dir);
        }
    }
    false
}

/// Find workspace root by walking up the directory tree
fn find_workspace_root(start: &Path) -> Result<PathBuf> {
    let mut current = start.to_path_buf();
    loop {
        let beads_dir = current.join(".beads");
        if beads_dir.exists() && beads_dir.is_dir() {
            return Ok(current);
        }

        match current.parent() {
            Some(parent) => current = parent.to_path_buf(),
            None => bail!("Workspace not found"),
        }
    }
}

/// Run migration with dry-run support
pub fn run_migration(opts: MigrationOptions) -> Result<MigrationReceipt> {
    // Validate options
    opts.validate()?;

    // Get adapters
    let source_adapter = get_adapter(&opts.from_profile)?;
    let target_adapter = get_adapter(&opts.to_profile)?;

    // Calculate input hash
    let input_sha256 = calculate_file_hash(&opts.input_path)?;

    // Read and parse input
    let (issues, stats) = read_input_file(&opts.input_path)?;

    // Transform issues
    let (transformed_issues, warnings, transform_stats) = transform_issues(
        &issues,
        source_adapter,
        target_adapter,
        &opts.from_profile,
        &opts.to_profile,
    )?;

    // For dry-run, return preview without writing files
    if opts.dry_run {
        let preview = build_migration_preview(
            &opts.from_profile,
            &opts.to_profile,
            &input_sha256,
            &stats,
            &transform_stats,
            &warnings,
        )?;

        // Emit preview to stdout
        emit_receipt(&serde_json::to_string_pretty(&preview)?)?;

        // Convert preview to receipt format
        return Ok(convert_preview_to_receipt(preview));
    }

    // Write output file
    write_output_file(&opts.output_path, &transformed_issues)?;
    let output_sha256 = calculate_file_hash(&opts.output_path)?;

    // Build receipt
    let receipt = build_migration_receipt(
        &opts.from_profile,
        &opts.to_profile,
        &input_sha256,
        &output_sha256,
        &stats,
        &transform_stats,
        &warnings,
        false,
    )?;

    // Write receipt if requested
    if let Some(ref receipt_path) = opts.receipt_path {
        write_receipt_file(receipt_path, &receipt)?;
    }

    // Emit receipt to stdout
    emit_receipt(&serde_json::to_string_pretty(&receipt)?)?;

    Ok(receipt)
}

/// Read and parse input file
fn read_input_file(path: &Path) -> Result<(Vec<Issue>, RecordCounts)> {
    let file = File::open(path).context("Failed to open input file")?;
    let reader = BufReader::new(file);
    let lines = reader.lines();

    let mut issues = Vec::new();
    let mut total_lines = 0;
    let mut blank_lines = 0;
    let mut malformed_lines = 0;
    let mut total_bytes = 0;

    for (line_num, line_result) in lines.enumerate() {
        let line_num = line_num + 1; // 1-based
        let line = line_result.context("Failed to read line")?;

        total_lines += 1;
        total_bytes += line.len() + 1; // +1 for newline

        if total_bytes > MAX_TOTAL_BYTES {
            bail!(
                "Input file exceeds maximum size of {} bytes",
                MAX_TOTAL_BYTES
            );
        }

        // Skip blank lines
        if line.trim().is_empty() {
            blank_lines += 1;
            continue;
        }

        // Check line length
        if line.len() > MAX_BYTES_PER_LINE {
            bail!(
                "Line {} exceeds maximum length of {} bytes",
                line_num,
                MAX_BYTES_PER_LINE
            );
        }

        // Parse JSON
        let value: serde_json::Value = serde_json::from_str(&line)
            .with_context(|| format!("Failed to parse JSON on line {}", line_num))?;

        // Validate it's an object
        if !value.is_object() {
            bail!("Line {} is not a JSON object", line_num);
        }

        // Use native adapter to parse
        let adapter = native_v1::NativeV1Adapter::new();
        let transform_result = adapter.profile_to_native(&value)?;

        if !transform_result.successful {
            malformed_lines += 1;
            continue;
        }

        let issue: Issue = serde_json::from_value(transform_result.data)
            .with_context(|| format!("Failed to parse issue on line {}", line_num))?;

        issues.push(issue);

        if issues.len() > MAX_ISSUE_RECORDS {
            bail!(
                "Input exceeds maximum record count of {}",
                MAX_ISSUE_RECORDS
            );
        }
    }

    let stats = RecordCounts {
        total_issues: issues.len(),
        input_issues: issues.len(),
        output_issues: 0, // Will be updated after transformation
        total_lines,
        blank_lines,
        malformed_lines,
    };

    Ok((issues, stats))
}

/// Transform issues between profiles
fn transform_issues(
    issues: &[Issue],
    source_adapter: &dyn ProfileAdapter,
    target_adapter: &dyn ProfileAdapter,
    source_profile: &str,
    target_profile: &str,
) -> Result<(Vec<String>, Vec<String>, TransformationCounts)> {
    let mut transformed = Vec::new();
    let mut all_warnings = Vec::new();
    let mut total_transformations = 0;
    let mut total_loss_entries = 0;

    for issue in issues {
        // Transform from source to native if not native
        let native_result = if source_profile != "native-v1" {
            let issue_value = serde_json::to_value(issue)?;
            source_adapter.profile_to_native(&issue_value)?
        } else {
            TransformResult {
                data: serde_json::to_value(issue)?,
                losses: vec![],
                successful: true,
            }
        };

        // Transform from native to target if not native
        let final_result = if target_profile != "native-v1" {
            target_adapter
                .native_to_profile(&serde_json::from_value(native_result.data.clone())?)?
        } else {
            native_result
        };

        total_transformations += 1;
        total_loss_entries += final_result.losses.len();

        // Collect warnings from losses
        for loss in &final_result.losses {
            if matches!(loss.severity, LossSeverity::Warning) {
                // Format loss category as string for warning
                let category_str = format!("{:?}", loss.category);
                all_warnings.push(format!(
                    "{}: {} - {}",
                    category_str, loss.field_path, loss.description
                ));
            }
        }

        // Serialize result to JSON string
        let json_string = serde_json::to_string(&final_result.data)?;
        transformed.push(json_string);
    }

    let stats = TransformationCounts {
        transformed_issues: if source_profile != target_profile {
            transformed.len()
        } else {
            0
        },
        preserved_issues: if source_profile == target_profile {
            transformed.len()
        } else {
            0
        },
        total_transformations,
        loss_entries: total_loss_entries,
    };

    Ok((transformed, all_warnings, stats))
}

/// Write output file
fn write_output_file(path: &Path, issues: &[String]) -> Result<()> {
    // Create parent directory if needed
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).context("Failed to create output directory")?;
    }

    // Write to temporary file first
    let temp_path = path.with_extension("tmp");
    let file = File::create(&temp_path).context("Failed to create output file")?;
    let mut writer = BufWriter::new(file);

    for issue_json in issues {
        writeln!(writer, "{}", issue_json).context("Failed to write issue")?;
    }

    writer.flush().context("Failed to flush output file")?;
    drop(writer);

    // Atomically rename
    std::fs::rename(&temp_path, path).context("Failed to rename output file")?;

    Ok(())
}

/// Write receipt file
fn write_receipt_file(path: &Path, receipt: &MigrationReceipt) -> Result<()> {
    // Create parent directory if needed
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).context("Failed to create receipt directory")?;
    }

    // Write to temporary file first
    let temp_path = path.with_extension("tmp");
    let file = File::create(&temp_path).context("Failed to create receipt file")?;
    let mut writer = BufWriter::new(file);

    let receipt_json = serde_json::to_string_pretty(receipt)?;
    writeln!(writer, "{}", receipt_json).context("Failed to write receipt")?;

    writer.flush().context("Failed to flush receipt file")?;
    drop(writer);

    // Atomically rename
    std::fs::rename(&temp_path, path).context("Failed to rename receipt file")?;

    Ok(())
}

/// Calculate SHA-256 hash of file
fn calculate_file_hash(path: &Path) -> Result<String> {
    let file = File::open(path).context("Failed to open file for hashing")?;
    let mut reader = BufReader::new(file);
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 8192];

    loop {
        let n = reader
            .read(&mut buffer)
            .context("Failed to read file for hashing")?;
        if n == 0 {
            break;
        }
        hasher.update(&buffer[..n]);
    }

    Ok(format!("{:x}", hasher.finalize()))
}

/// Build migration receipt
#[allow(clippy::too_many_arguments)]
fn build_migration_receipt(
    source_profile: &str,
    target_profile: &str,
    input_sha256: &str,
    output_sha256: &str,
    stats: &RecordCounts,
    transform_stats: &TransformationCounts,
    warnings: &[String],
    dry_run: bool,
) -> Result<MigrationReceipt> {
    Ok(MigrationReceipt {
        schema_ref: MIGRATION_RECEIPT_SCHEMA.to_string(),
        tool_version: env!("CARGO_PKG_VERSION").to_string(),
        timestamp: utc_now(),
        source_profile: source_profile.to_string(),
        target_profile: target_profile.to_string(),
        input_sha256: input_sha256.to_string(),
        output_sha256: output_sha256.to_string(),
        record_counts: stats.clone(),
        transformation_counts: transform_stats.clone(),
        warnings: warnings.to_vec(),
        dry_run,
        successful: true,
    })
}

/// Build migration preview for dry-run
fn build_migration_preview(
    source_profile: &str,
    target_profile: &str,
    input_sha256: &str,
    stats: &RecordCounts,
    transform_stats: &TransformationCounts,
    warnings: &[String],
) -> Result<MigrationPreview> {
    Ok(MigrationPreview {
        schema_ref: MIGRATION_RECEIPT_SCHEMA.to_string(),
        tool_version: env!("CARGO_PKG_VERSION").to_string(),
        timestamp: utc_now(),
        source_profile: source_profile.to_string(),
        target_profile: target_profile.to_string(),
        input_sha256: input_sha256.to_string(),
        record_counts: stats.clone(),
        transformation_counts: transform_stats.clone(),
        warnings: warnings.to_vec(),
        dry_run: true,
        prospective: true,
        successful: true,
    })
}

/// Convert preview to receipt format
fn convert_preview_to_receipt(preview: MigrationPreview) -> MigrationReceipt {
    MigrationReceipt {
        schema_ref: preview.schema_ref,
        tool_version: preview.tool_version,
        timestamp: preview.timestamp,
        source_profile: preview.source_profile,
        target_profile: preview.target_profile,
        input_sha256: preview.input_sha256,
        output_sha256: String::new(), // Dry-run has no output file
        record_counts: preview.record_counts,
        transformation_counts: preview.transformation_counts,
        warnings: preview.warnings,
        dry_run: preview.dry_run,
        successful: preview.successful,
    }
}

/// Emit receipt to stdout
fn emit_receipt(receipt_json: &str) -> Result<()> {
    println!("{}", receipt_json);
    Ok(())
}

/// Get current UTC timestamp as RFC 3339 string
fn utc_now() -> String {
    use time::OffsetDateTime;
    let format = time::format_description::well_known::Rfc3339;
    OffsetDateTime::now_utc()
        .format(&format)
        .unwrap_or_else(|_| String::from("timestamp-error"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_migration_receipt_schema() {
        let receipt = MigrationReceipt {
            schema_ref: MIGRATION_RECEIPT_SCHEMA.to_string(),
            tool_version: "0.1.0".to_string(),
            timestamp: "2026-08-10T00:00:00Z".to_string(),
            source_profile: "native-v1".to_string(),
            target_profile: "needle-v1".to_string(),
            input_sha256: "abc123".to_string(),
            output_sha256: "def456".to_string(),
            record_counts: RecordCounts {
                total_issues: 10,
                input_issues: 10,
                output_issues: 10,
                total_lines: 10,
                blank_lines: 0,
                malformed_lines: 0,
            },
            transformation_counts: TransformationCounts {
                transformed_issues: 5,
                preserved_issues: 5,
                total_transformations: 10,
                loss_entries: 2,
            },
            warnings: vec!["test warning".to_string()],
            dry_run: false,
            successful: true,
        };

        // Verify it can serialize
        let json = serde_json::to_string(&receipt);
        assert!(json.is_ok());
    }

    #[test]
    fn test_workspace_detection() {
        let current_dir = std::env::current_dir().unwrap();
        let workspace = find_workspace_root(&current_dir).unwrap();
        assert!(workspace.join(".beads").is_dir());
    }

    #[test]
    fn test_migration_preview() {
        let preview = MigrationPreview {
            schema_ref: MIGRATION_RECEIPT_SCHEMA.to_string(),
            tool_version: "0.1.0".to_string(),
            timestamp: "2026-08-10T00:00:00Z".to_string(),
            source_profile: "native-v1".to_string(),
            target_profile: "needle-v1".to_string(),
            input_sha256: "abc123".to_string(),
            record_counts: RecordCounts {
                total_issues: 10,
                input_issues: 10,
                output_issues: 0,
                total_lines: 10,
                blank_lines: 0,
                malformed_lines: 0,
            },
            transformation_counts: TransformationCounts {
                transformed_issues: 10,
                preserved_issues: 0,
                total_transformations: 10,
                loss_entries: 0,
            },
            warnings: vec![],
            dry_run: true,
            prospective: true,
            successful: true,
        };

        // Verify it can serialize
        let json = serde_json::to_string(&preview);
        assert!(json.is_ok());

        // Verify dry-run flag
        assert!(preview.dry_run);
        assert!(preview.prospective);
    }
}
