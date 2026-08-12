//! Profile adapters for native storage and NEEDLE subprocess compatibility.
//!
//! Per ADR-002, cross-tool profile adapters (`br-v1`, `bf-v1`) were removed.
//! Only `native-v1` recovery and the `needle-v1` subprocess contract remain.

use crate::model::Issue;
use anyhow::{anyhow, bail, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::str::FromStr;

pub mod native_v1;
pub mod needle_v1;

/// Profile identifier and version
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ProfileId {
    pub name: String,
    pub version: String,
}

impl ProfileId {
    /// Create a new profile ID
    pub fn new(name: &str, version: &str) -> Self {
        Self {
            name: name.to_string(),
            version: version.to_string(),
        }
    }

    /// Parse from profile identifier string (e.g., "native-v1", "br-v1")
    pub fn parse_id(s: &str) -> Result<Self> {
        let parts: Vec<&str> = s.splitn(2, '-').collect();
        if parts.len() != 2 {
            bail!(
                "Invalid profile identifier: {}. Expected format: name-version",
                s
            );
        }
        Ok(Self::new(parts[0], parts[1]))
    }

    /// Convert to string identifier
    pub fn as_str(&self) -> String {
        format!("{}-{}", self.name, self.version)
    }
}

impl FromStr for ProfileId {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse_id(s)
    }
}

/// Loss report entry for profile transformations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LossEntry {
    pub category: LossCategory,
    pub field_path: String,
    pub description: String,
    pub severity: LossSeverity,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum LossCategory {
    MissingField,
    UnsupportedField,
    NullValue,
    StatusMapping,
    PrecisionLoss,
    MetadataLoss,
    UnknownExtension,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum LossSeverity {
    Warning,
    Error,
    Info,
}

/// Transformation result with loss reporting
#[derive(Debug, Clone)]
pub struct TransformResult {
    pub data: serde_json::Value,
    pub losses: Vec<LossEntry>,
    pub successful: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProfileLossCounts {
    pub preserved: usize,
    pub transformed: usize,
    pub omitted: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProfileLossReportEntry {
    pub classification: String,
    pub scope: String,
    pub field: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fields: Option<Vec<String>>,
    pub reason: String,
    pub count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProfileLossReport {
    pub schema_ref: String,
    pub profile: String,
    pub direction: String,
    pub input_records: usize,
    pub output_records: usize,
    pub counts: ProfileLossCounts,
    pub entries: Vec<ProfileLossReportEntry>,
}

/// Profile adapter trait
pub trait ProfileAdapter: Send + Sync + std::fmt::Debug {
    /// Get profile identifier
    fn profile_id(&self) -> &ProfileId;

    /// Transform native issue to profile representation
    fn native_to_profile(&self, issue: &Issue) -> Result<TransformResult>;

    /// Transform a complete native issue projection, including relationship
    /// data that is stored outside the `issues` row.
    fn native_record_to_profile(
        &self,
        issue: &Issue,
        _labels: &[String],
        _dependencies: &[(String, String, String)],
    ) -> Result<TransformResult> {
        self.native_to_profile(issue)
    }

    /// Transform profile representation to native issue
    fn profile_to_native(&self, data: &serde_json::Value) -> Result<TransformResult>;

    /// Validate profile data
    #[allow(dead_code)]
    fn validate_profile_data(&self, data: &serde_json::Value) -> Result<Vec<LossEntry>>;

    /// Check if profile is supported for export
    #[allow(dead_code)]
    fn supports_export(&self) -> bool {
        true
    }

    /// Check if profile is supported for import
    #[allow(dead_code)]
    fn supports_import(&self) -> bool {
        true
    }

    /// Get profile description
    #[allow(dead_code)]
    fn description(&self) -> &str;
}

/// Profile registry
pub struct ProfileRegistry {
    adapters: HashMap<String, Box<dyn ProfileAdapter>>,
}

impl ProfileRegistry {
    /// Create new profile registry
    pub fn new() -> Self {
        let mut registry = Self {
            adapters: HashMap::new(),
        };

        // Register built-in profiles
        registry.register(Box::new(native_v1::NativeV1Adapter::new()));
        registry.register(Box::new(needle_v1::NeedleV1Adapter::new()));

        registry
    }

    /// Register a profile adapter
    fn register(&mut self, adapter: Box<dyn ProfileAdapter>) {
        let id = adapter.profile_id().as_str();
        self.adapters.insert(id, adapter);
    }

    /// Get adapter by profile identifier
    pub fn get_adapter(&self, profile_id: &str) -> Result<&dyn ProfileAdapter> {
        self.adapters
            .get(profile_id)
            .map(|adapter| adapter.as_ref())
            .ok_or_else(|| anyhow!("Unsupported profile: {}", profile_id))
    }

    /// List all supported profiles
    #[allow(dead_code)]
    pub fn list_profiles(&self) -> Vec<String> {
        self.adapters.keys().cloned().collect()
    }

    /// Check if profile is supported
    pub fn is_supported(&self, profile_id: &str) -> bool {
        self.adapters.contains_key(profile_id)
    }
}

impl Default for ProfileRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Global profile registry instance
fn global_registry() -> &'static ProfileRegistry {
    use std::sync::OnceLock;
    static REGISTRY: OnceLock<ProfileRegistry> = OnceLock::new();
    REGISTRY.get_or_init(ProfileRegistry::new)
}

/// Get profile adapter by identifier
pub fn get_adapter(profile_id: &str) -> Result<&'static dyn ProfileAdapter> {
    global_registry().get_adapter(profile_id)
}

/// List all supported profiles
#[allow(dead_code)]
pub fn list_profiles() -> Vec<String> {
    global_registry().list_profiles()
}

/// Check if profile is supported
pub fn is_supported(profile_id: &str) -> bool {
    global_registry().is_supported(profile_id)
}

/// Execute an in-memory same-profile round trip and produce the normative
/// accounting report used by the accepted F012 fixtures.
pub fn same_profile_round_trip(
    profile_id: &str,
    input: &serde_json::Value,
) -> Result<(serde_json::Value, ProfileLossReport)> {
    let adapter = get_adapter(profile_id)?;
    let imported = adapter.profile_to_native(input)?;
    let mut native = imported
        .data
        .as_object()
        .cloned()
        .ok_or_else(|| anyhow!("transformed issue must be an object"))?;
    let labels = native
        .remove("labels")
        .and_then(|value| value.as_array().cloned())
        .unwrap_or_default()
        .into_iter()
        .map(|value| {
            value
                .as_str()
                .map(str::to_string)
                .ok_or_else(|| anyhow!("label must be a string"))
        })
        .collect::<Result<Vec<_>>>()?;
    let dependency_values = native
        .remove("dependencies")
        .and_then(|value| value.as_array().cloned())
        .unwrap_or_default();
    let issue: Issue = serde_json::from_value(serde_json::Value::Object(native))?;
    let dependencies = dependency_values
        .into_iter()
        .map(|value| {
            let value = value
                .as_object()
                .ok_or_else(|| anyhow!("dependency must be an object"))?;
            Ok((
                issue.id.clone(),
                value
                    .get("blocker")
                    .and_then(serde_json::Value::as_str)
                    .ok_or_else(|| anyhow!("dependency missing blocker"))?
                    .to_string(),
                value
                    .get("kind")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("blocks")
                    .to_string(),
            ))
        })
        .collect::<Result<Vec<_>>>()?;
    let output = adapter
        .native_record_to_profile(&issue, &labels, &dependencies)?
        .data;

    let input_object = input
        .as_object()
        .ok_or_else(|| anyhow!("profile record must be an object"))?;
    let output_object = output
        .as_object()
        .ok_or_else(|| anyhow!("profile output must be an object"))?;
    // Only native-v1 and needle-v1 remain registered post-ADR-002; neither
    // profile's fields are exempted from the extension-preserved accounting
    // below, so this always evaluates to false.
    let known = |_field: &str| false;
    let mut ordinary = Vec::new();
    let mut entries = Vec::new();
    let mut counts = ProfileLossCounts {
        preserved: 1,
        transformed: 0,
        omitted: 0,
    };
    for (field, value) in input_object {
        match output_object.get(field) {
            Some(output_value) if output_value == value => {
                counts.preserved += 1;
                if value.is_null() {
                    entries.push(ProfileLossReportEntry {
                        classification: "preserved".to_string(),
                        scope: "field".to_string(),
                        field: field.clone(),
                        fields: None,
                        reason: "explicit_null_preserved".to_string(),
                        count: 1,
                    });
                } else if known(field) {
                    ordinary.push(field.clone());
                } else {
                    entries.push(ProfileLossReportEntry {
                        classification: "preserved".to_string(),
                        scope: "field".to_string(),
                        field: field.clone(),
                        fields: None,
                        reason: "extension_preserved".to_string(),
                        count: 1,
                    });
                }
            }
            Some(_) => {
                counts.transformed += 1;
                entries.push(ProfileLossReportEntry {
                    classification: "transformed".to_string(),
                    scope: "field".to_string(),
                    field: field.clone(),
                    fields: None,
                    reason: "field_transformed".to_string(),
                    count: 1,
                });
            }
            None => {
                counts.omitted += 1;
                entries.push(ProfileLossReportEntry {
                    classification: "omitted".to_string(),
                    scope: "field".to_string(),
                    field: field.clone(),
                    fields: None,
                    reason: "extension_omitted".to_string(),
                    count: 1,
                });
            }
        }
    }
    ordinary.sort();
    if !ordinary.is_empty() {
        entries.push(ProfileLossReportEntry {
            classification: "preserved".to_string(),
            scope: "field".to_string(),
            field: "*".to_string(),
            fields: Some(ordinary.clone()),
            reason: "field_preserved".to_string(),
            count: ordinary.len(),
        });
    }
    entries.push(ProfileLossReportEntry {
        classification: "preserved".to_string(),
        scope: "record".to_string(),
        field: "*".to_string(),
        fields: None,
        reason: "record_preserved".to_string(),
        count: 1,
    });
    entries.sort_by(|left, right| {
        let rank = |value: &str| match value {
            "preserved" => 0,
            "transformed" => 1,
            "omitted" => 2,
            _ => 3,
        };
        (
            rank(&left.classification),
            &left.scope,
            &left.field,
            &left.reason,
        )
            .cmp(&(
                rank(&right.classification),
                &right.scope,
                &right.field,
                &right.reason,
            ))
    });
    Ok((
        output,
        ProfileLossReport {
            schema_ref: "urn:bead-rs:schema:profile-loss-report:v1".to_string(),
            profile: profile_id.to_string(),
            direction: "same-profile-round-trip".to_string(),
            input_records: 1,
            output_records: 1,
            counts,
            entries,
        },
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_profile_id_parsing() {
        let id = ProfileId::parse_id("native-v1").unwrap();
        assert_eq!(id.name, "native");
        assert_eq!(id.version, "v1");
        assert_eq!(id.as_str(), "native-v1");
    }

    #[test]
    fn test_invalid_profile_id() {
        let result = ProfileId::parse_id("invalid");
        assert!(result.is_err());
    }

    #[test]
    fn test_profile_registry() {
        let registry = ProfileRegistry::new();

        // Test that built-in profiles are registered
        assert!(registry.is_supported("native-v1"));
        assert!(registry.is_supported("needle-v1"));

        // ADR-002 removed cross-tool profiles; they must no longer resolve.
        assert!(!registry.is_supported("br-v1"));
        assert!(!registry.is_supported("bf-v1"));

        // Test listing profiles
        let profiles = registry.list_profiles();
        assert!(profiles.contains(&"native-v1".to_string()));
        assert!(profiles.contains(&"needle-v1".to_string()));
        assert_eq!(profiles.len(), 2);
    }

    #[test]
    fn test_get_adapter() {
        let registry = ProfileRegistry::new();

        let adapter = registry.get_adapter("native-v1");
        assert!(adapter.is_ok());

        let adapter = registry.get_adapter("unsupported-profile");
        assert!(adapter.is_err());
    }
}
