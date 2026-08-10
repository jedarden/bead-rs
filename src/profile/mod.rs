//! Profile adapters for different interchange formats
//!
//! This module defines adapters for native-v1, needle-v1, br-v1, and bf-v1
//! compatibility profiles as specified in F012.

use crate::model::Issue;
use anyhow::{anyhow, bail, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

pub mod bf_v1;
pub mod br_v1;
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
    fn validate_profile_data(&self, data: &serde_json::Value) -> Result<Vec<LossEntry>>;

    /// Check if profile is supported for export
    fn supports_export(&self) -> bool {
        true
    }

    /// Check if profile is supported for import
    fn supports_import(&self) -> bool {
        true
    }

    /// Get profile description
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
        registry.register(Box::new(br_v1::BrV1Adapter::new()));
        registry.register(Box::new(bf_v1::BfV1Adapter::new()));

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
pub fn list_profiles() -> Vec<String> {
    global_registry().list_profiles()
}

/// Check if profile is supported
pub fn is_supported(profile_id: &str) -> bool {
    global_registry().is_supported(profile_id)
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
        assert!(registry.is_supported("br-v1"));
        assert!(registry.is_supported("bf-v1"));

        // Test listing profiles
        let profiles = registry.list_profiles();
        assert!(profiles.contains(&"native-v1".to_string()));
        assert!(profiles.contains(&"needle-v1".to_string()));
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
