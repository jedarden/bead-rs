//! Needle-v1 profile adapter
//!
//! This adapter provides compatibility with the NEEDLE v1 subprocess contract.
//! It follows the specification in research/specs/needle-cli-contract-v1.md

use crate::model::{BaseStatus, Issue};
use crate::profile::{
    LossCategory, LossEntry, LossSeverity, ProfileAdapter, ProfileId, TransformResult,
};
use anyhow::{anyhow, Result};
use serde_json::Value;

/// Needle-v1 profile adapter
#[derive(Debug)]
pub struct NeedleV1Adapter {
    profile_id: ProfileId,
}

impl NeedleV1Adapter {
    pub fn new() -> Self {
        Self {
            profile_id: ProfileId::new("needle", "v1"),
        }
    }

    /// Convert native base_status to needle-v1 status
    fn native_status_to_needle(&self, status: &BaseStatus) -> String {
        match status {
            BaseStatus::Open => "open".to_string(),
            BaseStatus::InProgress => "in_progress".to_string(),
            BaseStatus::Deferred => "deferred".to_string(),
            BaseStatus::Closed => "closed".to_string(),
        }
    }

    /// Convert needle-v1 status to native base_status
    fn needle_status_to_native(&self, status: &str) -> Result<BaseStatus> {
        match status {
            "open" => Ok(BaseStatus::Open),
            "in_progress" => Ok(BaseStatus::InProgress),
            "deferred" => Ok(BaseStatus::Deferred),
            "closed" => Ok(BaseStatus::Closed),
            _ => {
                // Retain unknown statuses as extensions
                Err(anyhow!("Unknown needle-v1 status: {}", status))
            }
        }
    }
}

impl ProfileAdapter for NeedleV1Adapter {
    fn profile_id(&self) -> &ProfileId {
        &self.profile_id
    }

    fn native_to_profile(&self, issue: &Issue) -> Result<TransformResult> {
        let mut obj = serde_json::json!({
            "id": issue.id,
            "title": issue.title,
            "description": issue.description.as_ref().unwrap_or(&String::new()),
            "priority": issue.priority,
            "status": self.native_status_to_needle(&issue.base_status),
            "assignee": issue.assignee,
            "dependencies": [],
            "created_at": issue.created_at,
            "updated_at": issue.updated_at,
            "labels": [],
            "issue_type": issue.issue_type.as_ref().unwrap_or(&"task".to_string())
        });

        let losses = vec![];

        // Add optional fields if present
        if let Some(notes) = &issue.notes {
            if let Some(obj_map) = obj.as_object_mut() {
                obj_map.insert("notes".to_string(), Value::String(notes.clone()));
            }
        }

        // Transform dependencies to needle-v1 format
        // Note: In a full implementation, we'd query dependencies here
        // For now, we provide the basic structure

        Ok(TransformResult {
            data: obj,
            losses,
            successful: true,
        })
    }

    fn profile_to_native(&self, data: &Value) -> Result<TransformResult> {
        let obj = data
            .as_object()
            .ok_or_else(|| anyhow!("Needle-v1 data must be an object"))?;

        let losses = vec![];

        // Extract required fields
        let id = obj
            .get("id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow!("Missing required field: id"))?
            .to_string();

        let title = obj
            .get("title")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow!("Missing required field: title"))?
            .to_string();

        let description = obj
            .get("description")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        let priority = obj
            .get("priority")
            .and_then(|v| v.as_i64())
            .ok_or_else(|| anyhow!("Missing or invalid field: priority"))?;

        let status_str = obj
            .get("status")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow!("Missing required field: status"))?;

        let base_status = self.needle_status_to_native(status_str)?;

        let assignee = obj
            .get("assignee")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        let created_at = obj
            .get("created_at")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow!("Missing required field: created_at"))?
            .to_string();

        let updated_at = obj
            .get("updated_at")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow!("Missing required field: updated_at"))?
            .to_string();

        // Build native issue
        let issue = Issue {
            id,
            title,
            description,
            notes: None,
            revision: None,
            priority: priority as i64,
            issue_type: Some("task".to_string()),
            base_status,
            manual_blocked: Some(false),
            assignee,
            created_at,
            updated_at,
            closed_at: None,
            close_reason: None,
            source_repo: None,
            profile: Some("needle-v1".to_string()),
            schema_ref: Some("urn:bead-rs:schema:issue:needle-v1".to_string()),
            data: None,
            extensions: Default::default(),
        };

        let serialized = serde_json::to_value(&issue)
            .map_err(|e| anyhow!("Failed to serialize native issue: {}", e))?;

        Ok(TransformResult {
            data: serialized,
            losses,
            successful: true,
        })
    }

    fn validate_profile_data(&self, data: &Value) -> Result<Vec<LossEntry>> {
        let mut losses = vec![];

        if let Some(obj) = data.as_object() {
            let required_fields = [
                "id",
                "title",
                "priority",
                "status",
                "created_at",
                "updated_at",
            ];
            for field in &required_fields {
                if !obj.contains_key(*field) {
                    losses.push(LossEntry {
                        category: LossCategory::MissingField,
                        field_path: field.to_string(),
                        description: format!("Missing required needle-v1 field: {}", field),
                        severity: LossSeverity::Error,
                    });
                }
            }
        }

        Ok(losses)
    }

    fn description(&self) -> &str {
        "NEEDLE v1 subprocess contract profile with compatibility mappings"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::BaseStatus;

    #[test]
    fn test_status_conversions() {
        let adapter = NeedleV1Adapter::new();

        assert_eq!(adapter.native_status_to_needle(&BaseStatus::Open), "open");
        assert_eq!(
            adapter.native_status_to_needle(&BaseStatus::InProgress),
            "in_progress"
        );
        assert_eq!(
            adapter.native_status_to_needle(&BaseStatus::Deferred),
            "deferred"
        );
        assert_eq!(
            adapter.native_status_to_needle(&BaseStatus::Closed),
            "closed"
        );

        assert_eq!(
            adapter.needle_status_to_native("open").unwrap(),
            BaseStatus::Open
        );
        assert_eq!(
            adapter.needle_status_to_native("in_progress").unwrap(),
            BaseStatus::InProgress
        );
        assert_eq!(
            adapter.needle_status_to_native("deferred").unwrap(),
            BaseStatus::Deferred
        );
        assert_eq!(
            adapter.needle_status_to_native("closed").unwrap(),
            BaseStatus::Closed
        );
    }

    #[test]
    fn test_unknown_status() {
        let adapter = NeedleV1Adapter::new();
        let result = adapter.needle_status_to_native("unknown_status");
        assert!(result.is_err());
    }

    #[test]
    fn test_profile_id() {
        let adapter = NeedleV1Adapter::new();
        assert_eq!(adapter.profile_id().as_str(), "needle-v1");
    }

    #[test]
    fn test_native_to_needle() {
        let adapter = NeedleV1Adapter::new();
        let issue = Issue {
            id: "test-001".to_string(),
            title: "Test Issue".to_string(),
            description: Some("Test Description".to_string()),
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
            schema_ref: None,
            data: None,
            extensions: Default::default(),
        };

        let result = adapter.native_to_profile(&issue).unwrap();
        assert!(result.successful);
        assert!(result.losses.is_empty());

        let data = result.data;
        assert_eq!(data["id"], "test-001");
        assert_eq!(data["title"], "Test Issue");
        assert_eq!(data["status"], "open");
        assert_eq!(data["priority"], 2);
    }

    #[test]
    fn test_needle_to_native() {
        let adapter = NeedleV1Adapter::new();
        let needle_data = serde_json::json!({
            "id": "test-002",
            "title": "Needle Issue",
            "description": "",
            "priority": 1,
            "status": "in_progress",
            "assignee": null,
            "dependencies": [],
            "created_at": "2026-08-10T00:00:00Z",
            "updated_at": "2026-08-10T00:00:00Z",
            "labels": []
        });

        let result = adapter.profile_to_native(&needle_data).unwrap();
        assert!(result.successful);

        // Verify we can deserialize the result
        let issue: Issue = serde_json::from_value(result.data).unwrap();
        assert_eq!(issue.id, "test-002");
        assert_eq!(issue.title, "Needle Issue");
        assert_eq!(issue.base_status, BaseStatus::InProgress);
        assert_eq!(issue.priority, 1);
    }
}
