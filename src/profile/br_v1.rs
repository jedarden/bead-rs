//! br-v1 profile adapter
//!
//! This adapter provides compatibility with the br 0.1.28 interchange format.
//! It follows the specification in research/specs/br-v1-profile.md

use crate::model::{BaseStatus, Issue};
use crate::profile::{
    LossCategory, LossEntry, LossSeverity, ProfileAdapter, ProfileId, TransformResult,
};
use anyhow::{anyhow, Result};
use serde_json::Value;

/// br-v1 profile adapter
#[derive(Debug)]
pub struct BrV1Adapter {
    profile_id: ProfileId,
}

impl Default for BrV1Adapter {
    fn default() -> Self {
        Self::new()
    }
}

impl BrV1Adapter {
    pub fn new() -> Self {
        Self {
            profile_id: ProfileId::new("br", "v1"),
        }
    }

    /// Convert native base_status to br-v1 status
    fn native_status_to_br(&self, status: &BaseStatus) -> String {
        match status {
            BaseStatus::Open => "open".to_string(),
            BaseStatus::InProgress => "in_progress".to_string(),
            BaseStatus::Deferred => "deferred".to_string(),
            BaseStatus::Closed => "finished".to_string(), // br-v1 uses "finished" for closed
        }
    }

    /// Convert br-v1 status to native base_status
    fn br_status_to_native(&self, status: &str) -> Result<BaseStatus> {
        match status {
            "open" => Ok(BaseStatus::Open),
            "in_progress" => Ok(BaseStatus::InProgress),
            "deferred" => Ok(BaseStatus::Deferred),
            "finished" => Ok(BaseStatus::Closed), // br-v1 "finished" maps to native "closed"
            "closed" => Ok(BaseStatus::Closed),   // Some br-v1 data may use "closed"
            _ => Err(anyhow!("Unknown br-v1 status: {}", status)),
        }
    }

    /// Transform native dependencies to br-v1 format.
    /// FIXME(F012 stub): correctly shapes dependency tuples, but `native_to_profile`
    /// never calls it — export hardcodes an empty `dependencies` array instead of
    /// querying real edges. Not wired up; kept for when that's implemented.
    #[allow(dead_code)]
    fn transform_dependencies_to_br(&self, dependencies: &[(String, String, String)]) -> Value {
        dependencies
            .iter()
            .map(|(blocked, blocker, kind)| {
                serde_json::json!({
                    "issue_id": blocked,
                    "depends_on_id": blocker,
                    "type": kind
                })
            })
            .collect()
    }

    /// Extract and transform dependencies from br-v1 data
    fn extract_dependencies_from_br(
        &self,
        obj: &serde_json::Map<String, Value>,
    ) -> Vec<(String, String, String)> {
        if let Some(deps) = obj.get("dependencies").and_then(|v| v.as_array()) {
            deps.iter()
                .filter_map(|dep| {
                    dep.as_object().and_then(|dep_obj| {
                        let issue_id = dep_obj.get("issue_id").and_then(|v| v.as_str())?;
                        let depends_on_id =
                            dep_obj.get("depends_on_id").and_then(|v| v.as_str())?;
                        let kind = dep_obj
                            .get("type")
                            .and_then(|v| v.as_str())
                            .unwrap_or("blocks");
                        Some((
                            issue_id.to_string(),
                            depends_on_id.to_string(),
                            kind.to_string(),
                        ))
                    })
                })
                .collect()
        } else {
            vec![]
        }
    }
}

impl ProfileAdapter for BrV1Adapter {
    fn profile_id(&self) -> &ProfileId {
        &self.profile_id
    }

    fn native_to_profile(&self, issue: &Issue) -> Result<TransformResult> {
        let mut losses = vec![];
        let mut br_obj = serde_json::Map::new();

        // Required fields
        br_obj.insert("id".to_string(), Value::String(issue.id.clone()));
        br_obj.insert("title".to_string(), Value::String(issue.title.clone()));
        br_obj.insert(
            "status".to_string(),
            Value::String(self.native_status_to_br(&issue.base_status)),
        );
        br_obj.insert(
            "priority".to_string(),
            Value::Number(serde_json::Number::from(issue.priority)),
        );
        br_obj.insert(
            "issue_type".to_string(),
            Value::String(
                issue
                    .issue_type
                    .clone()
                    .unwrap_or_else(|| "task".to_string()),
            ),
        );
        br_obj.insert(
            "created_at".to_string(),
            Value::String(issue.created_at.clone()),
        );
        br_obj.insert(
            "updated_at".to_string(),
            Value::String(issue.updated_at.clone()),
        );

        // Optional fields - only include if present
        if let Some(description) = &issue.description {
            br_obj.insert(
                "description".to_string(),
                Value::String(description.clone()),
            );
        }

        if let Some(assignee) = &issue.assignee {
            br_obj.insert("assignee".to_string(), Value::String(assignee.clone()));
        }

        // br-v1 uses both assignee and owner
        // Note: In full implementation, we'd need to determine which field to use

        if let Some(labels) = self.get_labels_for_issue(&issue.id) {
            if !labels.is_empty() {
                br_obj.insert(
                    "labels".to_string(),
                    Value::Array(labels.into_iter().map(Value::String).collect()),
                );
            }
        }

        // Dependencies placeholder - in full implementation, query from database
        br_obj.insert("dependencies".to_string(), Value::Array(vec![]));

        // Track loss for schema_ref (not observed in br-v1)
        if issue.schema_ref.is_some() {
            losses.push(LossEntry {
                category: LossCategory::UnsupportedField,
                field_path: "schema_ref".to_string(),
                description: "Native schema_ref is not supported in br-v1".to_string(),
                severity: LossSeverity::Warning,
            });
        }

        // Track loss for comments/data (not in br-v1 basic format)
        // Note: In full implementation, we'd check for presence of these

        Ok(TransformResult {
            data: Value::Object(br_obj),
            losses,
            successful: true,
        })
    }

    fn profile_to_native(&self, data: &Value) -> Result<TransformResult> {
        let obj = data
            .as_object()
            .ok_or_else(|| anyhow!("br-v1 data must be an object"))?;

        let mut losses = vec![];

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

        let status_str = obj
            .get("status")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow!("Missing required field: status"))?;

        let base_status = self.br_status_to_native(status_str)?;

        let priority = obj
            .get("priority")
            .and_then(|v| v.as_i64())
            .ok_or_else(|| anyhow!("Missing or invalid field: priority"))?;

        let issue_type = obj
            .get("issue_type")
            .and_then(|v| v.as_str())
            .unwrap_or("task")
            .to_string();

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

        // Optional fields
        let description = obj
            .get("description")
            .and_then(|v| v.as_str())
            .and_then(|s| {
                if s.is_empty() {
                    None
                } else {
                    Some(s.to_string())
                }
            });

        let assignee = obj.get("assignee").and_then(|v| v.as_str()).and_then(|s| {
            if s.is_empty() {
                None
            } else {
                Some(s.to_string())
            }
        });

        // Handle closed_at for "finished"/"closed" status
        let closed_at = if matches!(base_status, BaseStatus::Closed) {
            obj.get("closed_at")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
        } else {
            None
        };

        // Track unsupported fields
        for field in [
            "due_at",
            "defer_until",
            "estimated_minutes",
            "external_ref",
            "created_by",
            "compaction_level",
            "original_size",
        ] {
            if obj.contains_key(field) {
                losses.push(LossEntry {
                    category: LossCategory::UnsupportedField,
                    field_path: field.to_string(),
                    description: format!("br-v1 field '{}' is not supported natively", field),
                    severity: LossSeverity::Info,
                });
            }
        }

        // Check for missing schema_ref
        if !obj.contains_key("schema_ref") {
            losses.push(LossEntry {
                category: LossCategory::MissingField,
                field_path: "schema_ref".to_string(),
                description: "br-v1 data missing schema_ref (native field)".to_string(),
                severity: LossSeverity::Warning,
            });
        }

        // Extract dependencies for validation.
        // FIXME(F012 stub): parsed but not yet attached to the returned Issue or
        // persisted — dependency import is not actually implemented. Do not treat
        // this adapter as complete until this is wired up and reviewed.
        let _dependencies = self.extract_dependencies_from_br(obj);

        // Build native issue
        let issue = Issue {
            id,
            title,
            description,
            notes: None,
            revision: None,
            priority,
            issue_type: Some(issue_type),
            base_status,
            manual_blocked: Some(false),
            assignee,
            created_at,
            updated_at,
            closed_at,
            close_reason: None,
            source_repo: obj
                .get("source_repo")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string()),
            profile: Some("br-v1".to_string()),
            schema_ref: obj
                .get("schema_ref")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string()),
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
            // Required fields per br-v1 spec
            let required_fields = [
                "id",
                "title",
                "status",
                "priority",
                "issue_type",
                "created_at",
                "updated_at",
            ];
            for field in &required_fields {
                if !obj.contains_key(*field) {
                    losses.push(LossEntry {
                        category: LossCategory::MissingField,
                        field_path: field.to_string(),
                        description: format!("Missing required br-v1 field: {}", field),
                        severity: LossSeverity::Error,
                    });
                }
            }

            // Validate status values
            if let Some(status) = obj.get("status").and_then(|v| v.as_str()) {
                if !matches!(
                    status,
                    "open" | "in_progress" | "deferred" | "finished" | "closed"
                ) {
                    losses.push(LossEntry {
                        category: LossCategory::StatusMapping,
                        field_path: "status".to_string(),
                        description: format!("Unknown br-v1 status value: {}", status),
                        severity: LossSeverity::Warning,
                    });
                }
            }

            // Validate priority range
            if let Some(priority) = obj.get("priority").and_then(|v| v.as_i64()) {
                if !(0..=4).contains(&priority) {
                    losses.push(LossEntry {
                        category: LossCategory::UnsupportedField,
                        field_path: "priority".to_string(),
                        description: format!("Priority {} outside supported range 0-4", priority),
                        severity: LossSeverity::Error,
                    });
                }
            }
        }

        Ok(losses)
    }

    fn description(&self) -> &str {
        "br 0.1.28 compatibility profile with field mappings and loss reporting"
    }
}

impl BrV1Adapter {
    /// Placeholder for getting labels from database
    /// In full implementation, this would query the labels table
    fn get_labels_for_issue(&self, _issue_id: &str) -> Option<Vec<String>> {
        // TODO: Implement label querying from database
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::BaseStatus;

    #[test]
    fn test_status_conversions() {
        let adapter = BrV1Adapter::new();

        assert_eq!(adapter.native_status_to_br(&BaseStatus::Open), "open");
        assert_eq!(
            adapter.native_status_to_br(&BaseStatus::InProgress),
            "in_progress"
        );
        assert_eq!(
            adapter.native_status_to_br(&BaseStatus::Deferred),
            "deferred"
        );
        assert_eq!(adapter.native_status_to_br(&BaseStatus::Closed), "finished");

        assert_eq!(
            adapter.br_status_to_native("open").unwrap(),
            BaseStatus::Open
        );
        assert_eq!(
            adapter.br_status_to_native("in_progress").unwrap(),
            BaseStatus::InProgress
        );
        assert_eq!(
            adapter.br_status_to_native("deferred").unwrap(),
            BaseStatus::Deferred
        );
        assert_eq!(
            adapter.br_status_to_native("finished").unwrap(),
            BaseStatus::Closed
        );
        assert_eq!(
            adapter.br_status_to_native("closed").unwrap(),
            BaseStatus::Closed
        );
    }

    #[test]
    fn test_unknown_status() {
        let adapter = BrV1Adapter::new();
        let result = adapter.br_status_to_native("unknown_status");
        assert!(result.is_err());
    }

    #[test]
    fn test_profile_id() {
        let adapter = BrV1Adapter::new();
        assert_eq!(adapter.profile_id().as_str(), "br-v1");
    }

    #[test]
    fn test_native_to_br_basic() {
        let adapter = BrV1Adapter::new();
        let issue = Issue {
            id: "br-test-001".to_string(),
            title: "BR Test Issue".to_string(),
            description: Some("Test Description".to_string()),
            notes: None,
            revision: None,
            priority: 2,
            issue_type: Some("task".to_string()),
            base_status: BaseStatus::Open,
            manual_blocked: Some(false),
            assignee: Some("test-user".to_string()),
            created_at: "2026-08-10T00:00:00Z".to_string(),
            updated_at: "2026-08-10T00:00:00Z".to_string(),
            closed_at: None,
            close_reason: None,
            source_repo: None,
            profile: None,
            schema_ref: Some("urn:bead-rs:schema:issue:native-v1".to_string()),
            data: None,
            extensions: Default::default(),
        };

        let result = adapter.native_to_profile(&issue).unwrap();
        assert!(result.successful);

        let br_data = result.data;
        assert_eq!(br_data["id"], "br-test-001");
        assert_eq!(br_data["title"], "BR Test Issue");
        assert_eq!(br_data["status"], "open");
        assert_eq!(br_data["priority"], 2);
        assert_eq!(br_data["assignee"], "test-user");

        // Should have loss for schema_ref
        assert!(!result.losses.is_empty());
        assert!(result.losses.iter().any(|l| l.field_path == "schema_ref"));
    }

    #[test]
    fn test_br_to_native_basic() {
        let adapter = BrV1Adapter::new();
        let br_data = serde_json::json!({
            "id": "br-test-002",
            "title": "BR Issue",
            "description": "Description",
            "status": "in_progress",
            "priority": 1,
            "issue_type": "bug",
            "assignee": "developer",
            "created_at": "2026-08-10T00:00:00Z",
            "updated_at": "2026-08-10T00:00:00Z",
            "dependencies": [],
            "labels": ["urgent", "backend"]
        });

        let result = adapter.profile_to_native(&br_data).unwrap();
        assert!(result.successful);

        let issue: Issue = serde_json::from_value(result.data).unwrap();
        assert_eq!(issue.id, "br-test-002");
        assert_eq!(issue.title, "BR Issue");
        assert_eq!(issue.base_status, BaseStatus::InProgress);
        assert_eq!(issue.priority, 1);
        assert_eq!(issue.assignee, Some("developer".to_string()));
        assert_eq!(issue.profile, Some("br-v1".to_string()));
    }

    #[test]
    fn test_finished_status_mapping() {
        let adapter = BrV1Adapter::new();

        // Test native closed -> br finished
        assert_eq!(adapter.native_status_to_br(&BaseStatus::Closed), "finished");

        // Test br finished -> native closed
        let br_data = serde_json::json!({
            "id": "test",
            "title": "Test",
            "status": "finished",
            "priority": 2,
            "issue_type": "task",
            "created_at": "2026-08-10T00:00:00Z",
            "updated_at": "2026-08-10T00:00:00Z"
        });

        let result = adapter.profile_to_native(&br_data).unwrap();
        let issue: Issue = serde_json::from_value(result.data).unwrap();
        assert_eq!(issue.base_status, BaseStatus::Closed);
    }

    #[test]
    fn test_validate_complete_br_data() {
        let adapter = BrV1Adapter::new();
        let complete_data = serde_json::json!({
            "id": "test",
            "title": "Test",
            "status": "open",
            "priority": 2,
            "issue_type": "task",
            "created_at": "2026-08-10T00:00:00Z",
            "updated_at": "2026-08-10T00:00:00Z"
        });

        let losses = adapter.validate_profile_data(&complete_data).unwrap();
        assert!(losses.is_empty());
    }

    #[test]
    fn test_validate_missing_required_fields() {
        let adapter = BrV1Adapter::new();
        let incomplete_data = serde_json::json!({
            "title": "Test"
        });

        let losses = adapter.validate_profile_data(&incomplete_data).unwrap();
        assert!(!losses.is_empty());

        let loss_fields: Vec<&str> = losses.iter().map(|l| l.field_path.as_str()).collect();
        assert!(loss_fields.contains(&"id"));
        assert!(loss_fields.contains(&"status"));
        assert!(loss_fields.contains(&"priority"));
    }

    #[test]
    fn test_validate_invalid_priority() {
        let adapter = BrV1Adapter::new();
        let invalid_priority = serde_json::json!({
            "id": "test",
            "title": "Test",
            "status": "open",
            "priority": 10, // Invalid: > 4
            "issue_type": "task",
            "created_at": "2026-08-10T00:00:00Z",
            "updated_at": "2026-08-10T00:00:00Z"
        });

        let losses = adapter.validate_profile_data(&invalid_priority).unwrap();
        assert!(losses
            .iter()
            .any(|l| l.field_path == "priority" && matches!(l.severity, LossSeverity::Error)));
    }

    #[test]
    fn test_unsupported_fields_loss_reporting() {
        let adapter = BrV1Adapter::new();
        let br_data_with_extra = serde_json::json!({
            "id": "test",
            "title": "Test",
            "status": "open",
            "priority": 2,
            "issue_type": "task",
            "created_at": "2026-08-10T00:00:00Z",
            "updated_at": "2026-08-10T00:00:00Z",
            "due_at": "2026-12-31T00:00:00Z",
            "estimated_minutes": 120,
            "external_ref": "EXT-123"
        });

        let result = adapter.profile_to_native(&br_data_with_extra).unwrap();
        assert!(result.successful);

        // Should report losses for unsupported fields
        assert!(!result.losses.is_empty());
        assert!(result.losses.iter().any(|l| l.field_path == "due_at"));
        assert!(result
            .losses
            .iter()
            .any(|l| l.field_path == "estimated_minutes"));
        assert!(result.losses.iter().any(|l| l.field_path == "external_ref"));
    }
}
