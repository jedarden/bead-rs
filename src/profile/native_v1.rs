//! Native-v1 profile adapter
//!
//! This is the canonical native profile that represents bead-rs issues
//! without any transformation or loss.

use crate::model::Issue;
use crate::profile::{ProfileAdapter, ProfileId, TransformResult};
use anyhow::{anyhow, Result};
use serde_json::Value;

impl Default for NativeV1Adapter {
    fn default() -> Self {
        Self::new()
    }
}

/// Native-v1 profile adapter
#[derive(Debug)]
pub struct NativeV1Adapter {
    profile_id: ProfileId,
}

impl NativeV1Adapter {
    pub fn new() -> Self {
        Self {
            profile_id: ProfileId::new("native", "v1"),
        }
    }
}

impl ProfileAdapter for NativeV1Adapter {
    fn profile_id(&self) -> &ProfileId {
        &self.profile_id
    }

    fn native_to_profile(&self, issue: &Issue) -> Result<TransformResult> {
        // Native format is a direct pass-through with no transformation
        let data = serde_json::to_value(issue)
            .map_err(|e| anyhow!("Failed to serialize native issue: {}", e))?;

        Ok(TransformResult {
            data,
            losses: vec![],
            successful: true,
        })
    }

    fn profile_to_native(&self, data: &Value) -> Result<TransformResult> {
        // Deserialize directly to native format
        let issue: Issue = serde_json::from_value(data.clone())
            .map_err(|e| anyhow!("Failed to deserialize to native issue: {}", e))?;

        let serialized = serde_json::to_value(&issue)
            .map_err(|e| anyhow!("Failed to re-serialize native issue: {}", e))?;

        Ok(TransformResult {
            data: serialized,
            losses: vec![],
            successful: true,
        })
    }

    fn validate_profile_data(&self, data: &Value) -> Result<Vec<crate::profile::LossEntry>> {
        let mut losses = vec![];

        // Validate required fields
        if let Some(obj) = data.as_object() {
            if !obj.contains_key("id") {
                losses.push(crate::profile::LossEntry {
                    category: crate::profile::LossCategory::MissingField,
                    field_path: "id".to_string(),
                    description: "Missing required field: id".to_string(),
                    severity: crate::profile::LossSeverity::Error,
                });
            }
            if !obj.contains_key("title") {
                losses.push(crate::profile::LossEntry {
                    category: crate::profile::LossCategory::MissingField,
                    field_path: "title".to_string(),
                    description: "Missing required field: title".to_string(),
                    severity: crate::profile::LossSeverity::Error,
                });
            }
            if !obj.contains_key("priority") {
                losses.push(crate::profile::LossEntry {
                    category: crate::profile::LossCategory::MissingField,
                    field_path: "priority".to_string(),
                    description: "Missing required field: priority".to_string(),
                    severity: crate::profile::LossSeverity::Error,
                });
            }
            if !obj.contains_key("base_status") {
                losses.push(crate::profile::LossEntry {
                    category: crate::profile::LossCategory::MissingField,
                    field_path: "base_status".to_string(),
                    description: "Missing required field: base_status".to_string(),
                    severity: crate::profile::LossSeverity::Error,
                });
            }
        } else {
            losses.push(crate::profile::LossEntry {
                category: crate::profile::LossCategory::MissingField,
                field_path: "root".to_string(),
                description: "Profile data must be a JSON object".to_string(),
                severity: crate::profile::LossSeverity::Error,
            });
        }

        Ok(losses)
    }

    fn description(&self) -> &str {
        "Canonical native bead-rs profile with full field support and no transformation loss"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::BaseStatus;

    fn create_test_issue() -> Issue {
        Issue {
            id: "bead-test123".to_string(),
            title: "Test Issue".to_string(),
            description: Some("Test description".to_string()),
            notes: None,
            revision: None,
            priority: 2,
            issue_type: Some("task".to_string()),
            base_status: BaseStatus::Open,
            manual_blocked: Some(false),
            assignee: None,
            created_at: "2026-08-10T00:00:00Z".to_string(),
            updated_at: "2026-08-10T00:00:00Z".to_string(),
            closed_at: None,
            close_reason: None,
            source_repo: None,
            profile: None,
            schema_ref: Some("urn:bead-rs:schema:issue:native-v1".to_string()),
            data: None,
            extensions: Default::default(),
        }
    }

    #[test]
    fn test_native_to_profile() {
        let adapter = NativeV1Adapter::new();
        let issue = create_test_issue();

        let result = adapter.native_to_profile(&issue).unwrap();
        assert!(result.successful);
        assert!(result.losses.is_empty());

        // Verify the data can be deserialized back
        let reconstructed: Issue = serde_json::from_value(result.data).unwrap();
        assert_eq!(reconstructed.id, issue.id);
        assert_eq!(reconstructed.title, issue.title);
    }

    #[test]
    fn test_profile_to_native() {
        let adapter = NativeV1Adapter::new();
        let issue = create_test_issue();
        let data = serde_json::to_value(&issue).unwrap();

        let result = adapter.profile_to_native(&data).unwrap();
        assert!(result.successful);
        assert!(result.losses.is_empty());
    }

    #[test]
    fn test_validate_complete_data() {
        let adapter = NativeV1Adapter::new();
        let mut issue = create_test_issue();
        issue.revision = Some(1);
        let data = serde_json::to_value(&issue).unwrap();

        let losses = adapter.validate_profile_data(&data).unwrap();
        assert!(losses.is_empty());
    }

    #[test]
    fn test_validate_missing_required_fields() {
        let adapter = NativeV1Adapter::new();
        let incomplete_data = serde_json::json!({
            "title": "Test"
        });

        let losses = adapter.validate_profile_data(&incomplete_data).unwrap();
        assert!(!losses.is_empty());

        // Should have errors for missing required fields
        let loss_fields: Vec<&str> = losses.iter().map(|l| l.field_path.as_str()).collect();
        assert!(loss_fields.contains(&"id"));
        assert!(loss_fields.contains(&"priority"));
        assert!(loss_fields.contains(&"base_status"));
    }

    #[test]
    fn test_profile_id() {
        let adapter = NativeV1Adapter::new();
        assert_eq!(adapter.profile_id().as_str(), "native-v1");
    }

    #[test]
    fn test_description() {
        let adapter = NativeV1Adapter::new();
        assert!(!adapter.description().is_empty());
    }
}
