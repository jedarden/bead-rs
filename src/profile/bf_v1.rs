//! bf-v1 profile adapter
//!
//! This adapter provides compatibility with the bf 0.4.0 interchange format.
//! It follows the specification in research/specs/bf-v1-profile.md

use crate::model::{BaseStatus, Issue};
use crate::profile::{
    LossCategory, LossEntry, LossSeverity, ProfileAdapter, ProfileId, TransformResult,
};
use anyhow::{anyhow, Result};
use serde_json::Value;

/// bf-v1 profile adapter
#[derive(Debug)]
pub struct BfV1Adapter {
    profile_id: ProfileId,
}

impl Default for BfV1Adapter {
    fn default() -> Self {
        Self::new()
    }
}

impl BfV1Adapter {
    pub fn new() -> Self {
        Self {
            profile_id: ProfileId::new("bf", "v1"),
        }
    }

    /// Convert native base_status to bf-v1 status
    fn native_status_to_bf(&self, status: &BaseStatus) -> Result<String> {
        match status {
            BaseStatus::Open => Ok("open".to_string()),
            BaseStatus::InProgress => Ok("in_progress".to_string()),
            BaseStatus::Deferred => {
                // No deferred mapping established for bf-v1
                Err(anyhow!("Native deferred status has no bf-v1 mapping"))
            }
            BaseStatus::Closed => Ok("closed".to_string()),
        }
    }

    /// Convert bf-v1 status to native base_status
    fn bf_status_to_native(&self, status: &str) -> Result<BaseStatus> {
        match status {
            "open" => Ok(BaseStatus::Open),
            "in_progress" => Ok(BaseStatus::InProgress),
            "blocked" => Ok(BaseStatus::Open),
            "closed" => Ok(BaseStatus::Closed),
            _ => Err(anyhow!("Unknown bf-v1 status: {}", status)),
        }
    }

    /// Get required content fields for bf-v1
    fn get_content_fields(&self, issue: &Issue) -> (String, String, String, String) {
        let description = issue.description.as_ref().unwrap_or(&String::new()).clone();
        let design = issue
            .get_extension("design")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let acceptance_criteria = issue
            .get_extension("acceptance_criteria")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let notes = issue.notes.as_ref().unwrap_or(&String::new()).clone();

        (description, design, acceptance_criteria, notes)
    }
}

impl ProfileAdapter for BfV1Adapter {
    fn profile_id(&self) -> &ProfileId {
        &self.profile_id
    }

    fn native_to_profile(&self, issue: &Issue) -> Result<TransformResult> {
        self.native_record_to_profile(issue, &[], &[])
    }

    fn native_record_to_profile(
        &self,
        issue: &Issue,
        labels: &[String],
        dependencies: &[(String, String, String)],
    ) -> Result<TransformResult> {
        let mut losses = vec![];
        let mut bf_obj = serde_json::Map::new();

        // Required fields
        bf_obj.insert("id".to_string(), Value::String(issue.id.clone()));
        bf_obj.insert("title".to_string(), Value::String(issue.title.clone()));

        // bf-v1 requires 4 content fields (emit empty strings if absent)
        let (description, design, acceptance_criteria, notes) = self.get_content_fields(issue);
        bf_obj.insert("description".to_string(), Value::String(description));
        bf_obj.insert("design".to_string(), Value::String(design));
        bf_obj.insert(
            "acceptance_criteria".to_string(),
            Value::String(acceptance_criteria),
        );
        bf_obj.insert("notes".to_string(), Value::String(notes));

        // Status with loss reporting for deferred
        let status = if issue.manual_blocked.unwrap_or(false) {
            "blocked".to_string()
        } else {
            match self.native_status_to_bf(&issue.base_status) {
                Ok(s) => s,
                Err(e) => {
                    losses.push(LossEntry {
                        category: LossCategory::StatusMapping,
                        field_path: "status".to_string(),
                        description: e.to_string(),
                        severity: LossSeverity::Error,
                    });
                    "open".to_string() // Default fallback
                }
            }
        };
        bf_obj.insert("status".to_string(), Value::String(status));

        bf_obj.insert(
            "priority".to_string(),
            Value::Number(serde_json::Number::from(issue.priority)),
        );
        bf_obj.insert(
            "issue_type".to_string(),
            Value::String(
                issue
                    .issue_type
                    .clone()
                    .unwrap_or_else(|| "task".to_string()),
            ),
        );
        bf_obj.insert(
            "created_at".to_string(),
            Value::String(issue.created_at.clone()),
        );
        bf_obj.insert(
            "updated_at".to_string(),
            Value::String(issue.updated_at.clone()),
        );

        // Optional fields
        if let Some(assignee) = &issue.assignee {
            bf_obj.insert("assignee".to_string(), Value::String(assignee.clone()));
        }

        if !labels.is_empty() {
            let mut labels = labels.to_vec();
            labels.sort();
            bf_obj.insert(
                "labels".to_string(),
                Value::Array(labels.into_iter().map(Value::String).collect()),
            );
        }

        if let Some(closed_at) = &issue.closed_at {
            bf_obj.insert("closed_at".to_string(), Value::String(closed_at.clone()));
        }

        if let Some(close_reason) = &issue.close_reason {
            bf_obj.insert(
                "close_reason".to_string(),
                Value::String(close_reason.clone()),
            );
        }

        if let Some(source_repo) = &issue.source_repo {
            bf_obj.insert(
                "source_repo".to_string(),
                Value::String(source_repo.clone()),
            );
        }

        // Preserve compaction_level if present in extensions
        if let Some(level) = issue.get_extension("compaction_level") {
            bf_obj.insert("compaction_level".to_string(), level.clone());
        }

        if !dependencies.is_empty() {
            bf_obj.insert(
                "dependencies".to_string(),
                Value::Array(
                    dependencies
                        .iter()
                        .map(|(blocked, blocker, kind)| {
                            serde_json::json!({
                                "issue_id": blocked,
                                "depends_on_id": blocker,
                                "type": kind
                            })
                        })
                        .collect(),
                ),
            );
        }

        // Track loss for schema_ref (not observed in bf-v1)
        if issue.schema_ref.is_some() {
            losses.push(LossEntry {
                category: LossCategory::UnsupportedField,
                field_path: "schema_ref".to_string(),
                description: "Native schema_ref is not supported in bf-v1".to_string(),
                severity: LossSeverity::Warning,
            });
        }

        // Track loss for comments/data (not in bf-v1 basic format)
        // Note: In full implementation, we'd check for presence of these

        Ok(TransformResult {
            data: Value::Object(bf_obj),
            losses,
            successful: true,
        })
    }

    fn profile_to_native(&self, data: &Value) -> Result<TransformResult> {
        let obj = data
            .as_object()
            .ok_or_else(|| anyhow!("bf-v1 data must be an object"))?;

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

        let base_status = self.bf_status_to_native(status_str)?;
        let manual_blocked = status_str == "blocked";

        let priority = obj
            .get("priority")
            .and_then(|v| v.as_i64())
            .ok_or_else(|| anyhow!("Missing or invalid field: priority"))?;

        let issue_type = obj
            .get("issue_type")
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

        // bf-v1 required content fields (may be empty strings)
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

        let design = obj
            .get("design")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        let acceptance_criteria = obj
            .get("acceptance_criteria")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        let notes_str = obj
            .get("notes")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        // Optional fields
        let assignee = obj.get("assignee").and_then(|v| v.as_str()).and_then(|s| {
            if s.is_empty() {
                None
            } else {
                Some(s.to_string())
            }
        });

        let closed_at = obj
            .get("closed_at")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        let close_reason = obj
            .get("close_reason")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        let source_repo = obj
            .get("source_repo")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        let closed_by_session = obj
            .get("closed_by_session")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        // Track unsupported/extension fields
        for field in ["compaction_level", "closed_by_session"] {
            if obj.contains_key(field) {
                losses.push(LossEntry {
                    category: LossCategory::UnsupportedField,
                    field_path: field.to_string(),
                    description: format!("bf-v1 field '{}' is not supported natively", field),
                    severity: LossSeverity::Info,
                });
            }
        }

        // Check for missing schema_ref
        if !obj.contains_key("schema_ref") {
            losses.push(LossEntry {
                category: LossCategory::MissingField,
                field_path: "schema_ref".to_string(),
                description: "bf-v1 data missing schema_ref (native field)".to_string(),
                severity: LossSeverity::Warning,
            });
        }

        // Build extensions map for bf-v1 specific fields
        let mut extensions_map = std::collections::HashMap::new();

        // Store design and acceptance_criteria in extensions if present
        if let Some(design_value) = design {
            if !design_value.is_empty() {
                extensions_map.insert("design".to_string(), Value::String(design_value));
            }
        }

        if let Some(ac_value) = acceptance_criteria {
            if !ac_value.is_empty() {
                extensions_map.insert("acceptance_criteria".to_string(), Value::String(ac_value));
            }
        }

        if let Some(cbs_value) = closed_by_session {
            extensions_map.insert("closed_by_session".to_string(), Value::String(cbs_value));
        }

        if let Some(compaction_level) = obj.get("compaction_level") {
            extensions_map.insert("compaction_level".to_string(), compaction_level.clone());
        }

        // Build native issue
        let issue = Issue {
            id,
            title,
            description,
            notes: notes_str,
            revision: None,
            priority,
            issue_type,
            base_status,
            manual_blocked: Some(manual_blocked),
            assignee,
            created_at,
            updated_at,
            closed_at,
            close_reason,
            source_repo,
            profile: Some("bf-v1".to_string()),
            schema_ref: obj
                .get("schema_ref")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string()),
            data: None,
            extensions: extensions_map,
        };

        let mut serialized = serde_json::to_value(&issue)
            .map_err(|e| anyhow!("Failed to serialize native issue: {}", e))?;
        let native_obj = serialized
            .as_object_mut()
            .ok_or_else(|| anyhow!("Serialized native issue must be an object"))?;
        if let Some(dependencies) = obj.get("dependencies") {
            let dependencies = dependencies
                .as_array()
                .ok_or_else(|| anyhow!("dependencies must be an array"))?;
            let native_dependencies = dependencies
                .iter()
                .map(|dependency| {
                    let dependency = dependency
                        .as_object()
                        .ok_or_else(|| anyhow!("dependency must be an object"))?;
                    let blocker = dependency
                        .get("depends_on_id")
                        .and_then(Value::as_str)
                        .ok_or_else(|| anyhow!("dependency missing depends_on_id"))?;
                    let kind = dependency
                        .get("type")
                        .and_then(Value::as_str)
                        .unwrap_or("blocks");
                    Ok(serde_json::json!({"blocker": blocker, "kind": kind}))
                })
                .collect::<Result<Vec<_>>>()?;
            native_obj.insert(
                "dependencies".to_string(),
                Value::Array(native_dependencies),
            );
        }
        if let Some(labels) = obj.get("labels") {
            native_obj.insert("labels".to_string(), labels.clone());
        }

        Ok(TransformResult {
            data: serialized,
            losses,
            successful: true,
        })
    }

    fn validate_profile_data(&self, data: &Value) -> Result<Vec<LossEntry>> {
        let mut losses = vec![];

        if let Some(obj) = data.as_object() {
            // Required fields per bf-v1 spec
            let required_fields = [
                "id",
                "title",
                "description",
                "design",
                "acceptance_criteria",
                "notes",
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
                        description: format!("Missing required bf-v1 field: {}", field),
                        severity: LossSeverity::Error,
                    });
                }
            }

            // Validate status values
            if let Some(status) = obj.get("status").and_then(|v| v.as_str()) {
                if !matches!(status, "open" | "in_progress" | "blocked" | "closed") {
                    losses.push(LossEntry {
                        category: LossCategory::StatusMapping,
                        field_path: "status".to_string(),
                        description: format!("Unknown bf-v1 status value: {}", status),
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

            // Check for explicit null vs absent distinction
            for field in ["description", "design", "acceptance_criteria", "notes"] {
                if let Some(value) = obj.get(field) {
                    if value.is_null() {
                        losses.push(LossEntry {
                            category: LossCategory::NullValue,
                            field_path: field.to_string(),
                            description: format!(
                                "bf-v1 field '{}' is explicit null (vs absent/empty)",
                                field
                            ),
                            severity: LossSeverity::Info,
                        });
                    }
                }
            }
        }

        Ok(losses)
    }

    fn description(&self) -> &str {
        "bf 0.4.0 compatibility profile with extended content fields and status mappings"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::BaseStatus;
    use std::collections::HashMap;

    fn create_test_issue_with_extensions() -> Issue {
        let mut extensions = HashMap::new();
        extensions.insert(
            "design".to_string(),
            serde_json::json!("Design document content"),
        );
        extensions.insert(
            "acceptance_criteria".to_string(),
            serde_json::json!("AC1: Test passes"),
        );

        Issue {
            id: "bf-test-001".to_string(),
            title: "BF Test Issue".to_string(),
            description: Some("Test Description".to_string()),
            notes: Some("Test notes".to_string()),
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
            extensions,
        }
    }

    #[test]
    fn test_status_conversions() {
        let adapter = BfV1Adapter::new();

        assert_eq!(
            adapter.native_status_to_bf(&BaseStatus::Open).unwrap(),
            "open"
        );
        assert_eq!(
            adapter
                .native_status_to_bf(&BaseStatus::InProgress)
                .unwrap(),
            "in_progress"
        );
        assert_eq!(
            adapter.native_status_to_bf(&BaseStatus::Closed).unwrap(),
            "closed"
        );
        assert!(adapter.native_status_to_bf(&BaseStatus::Deferred).is_err()); // No mapping

        assert_eq!(
            adapter.bf_status_to_native("open").unwrap(),
            BaseStatus::Open
        );
        assert_eq!(
            adapter.bf_status_to_native("in_progress").unwrap(),
            BaseStatus::InProgress
        );
        assert_eq!(
            adapter.bf_status_to_native("blocked").unwrap(),
            BaseStatus::Open
        ); // blocked -> open
        assert_eq!(
            adapter.bf_status_to_native("closed").unwrap(),
            BaseStatus::Closed
        );
    }

    #[test]
    fn test_profile_id() {
        let adapter = BfV1Adapter::new();
        assert_eq!(adapter.profile_id().as_str(), "bf-v1");
    }

    #[test]
    fn test_native_to_bf_with_extensions() {
        let adapter = BfV1Adapter::new();
        let issue = create_test_issue_with_extensions();

        let result = adapter.native_to_profile(&issue).unwrap();
        assert!(result.successful);

        let bf_data = result.data;
        assert_eq!(bf_data["id"], "bf-test-001");
        assert_eq!(bf_data["title"], "BF Test Issue");
        assert_eq!(bf_data["status"], "open");
        assert_eq!(bf_data["description"], "Test Description");
        assert_eq!(bf_data["design"], "Design document content");
        assert_eq!(bf_data["acceptance_criteria"], "AC1: Test passes");
        assert_eq!(bf_data["notes"], "Test notes");
        assert_eq!(bf_data["priority"], 2);

        // Should have loss for schema_ref
        assert!(!result.losses.is_empty());
        assert!(result.losses.iter().any(|l| l.field_path == "schema_ref"));
    }

    #[test]
    fn test_bf_to_native_basic() {
        let adapter = BfV1Adapter::new();
        let bf_data = serde_json::json!({
            "id": "bf-test-002",
            "title": "BF Issue",
            "description": "Description",
            "design": "",
            "acceptance_criteria": "",
            "notes": "",
            "status": "in_progress",
            "priority": 1,
            "issue_type": "bug",
            "assignee": "developer",
            "created_at": "2026-08-10T00:00:00Z",
            "updated_at": "2026-08-10T00:00:00Z",
            "dependencies": [],
            "labels": ["urgent"]
        });

        let result = adapter.profile_to_native(&bf_data).unwrap();
        assert!(result.successful);

        let issue: Issue = serde_json::from_value(result.data).unwrap();
        assert_eq!(issue.id, "bf-test-002");
        assert_eq!(issue.title, "BF Issue");
        assert_eq!(issue.base_status, BaseStatus::InProgress);
        assert_eq!(issue.priority, 1);
        assert_eq!(issue.assignee, Some("developer".to_string()));
        assert_eq!(issue.profile, Some("bf-v1".to_string()));
    }

    #[test]
    fn test_blocked_status_handling() {
        let adapter = BfV1Adapter::new();

        // bf-v1 "blocked" should map to native "open"
        let bf_data = serde_json::json!({
            "id": "test",
            "title": "Test",
            "description": "",
            "design": "",
            "acceptance_criteria": "",
            "notes": "",
            "status": "blocked",
            "priority": 2,
            "issue_type": "task",
            "created_at": "2026-08-10T00:00:00Z",
            "updated_at": "2026-08-10T00:00:00Z"
        });

        let result = adapter.profile_to_native(&bf_data).unwrap();
        let issue: Issue = serde_json::from_value(result.data).unwrap();
        assert_eq!(issue.base_status, BaseStatus::Open); // blocked -> open
    }

    #[test]
    fn test_validate_complete_bf_data() {
        let adapter = BfV1Adapter::new();
        let complete_data = serde_json::json!({
            "id": "test",
            "title": "Test",
            "description": "",
            "design": "",
            "acceptance_criteria": "",
            "notes": "",
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
    fn test_validate_missing_required_content_fields() {
        let adapter = BfV1Adapter::new();
        let incomplete_data = serde_json::json!({
            "id": "test",
            "title": "Test",
            "status": "open",
            "priority": 2
            // Missing: description, design, acceptance_criteria, notes, etc.
        });

        let losses = adapter.validate_profile_data(&incomplete_data).unwrap();
        assert!(!losses.is_empty());

        let loss_fields: Vec<&str> = losses.iter().map(|l| l.field_path.as_str()).collect();
        assert!(loss_fields.contains(&"description"));
        assert!(loss_fields.contains(&"design"));
        assert!(loss_fields.contains(&"acceptance_criteria"));
        assert!(loss_fields.contains(&"notes"));
    }

    #[test]
    fn test_deferred_status_loss() {
        let adapter = BfV1Adapter::new();
        let deferred_issue = Issue {
            id: "test".to_string(),
            title: "Test".to_string(),
            description: Some(String::new()),
            notes: None,
            revision: None,
            priority: 2,
            issue_type: Some("task".to_string()),
            base_status: BaseStatus::Deferred,
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

        let result = adapter.native_to_profile(&deferred_issue).unwrap();
        // Should have loss for deferred status
        assert!(
            result
                .losses
                .iter()
                .any(|l| l.field_path == "status"
                    && matches!(l.category, LossCategory::StatusMapping))
        );
    }

    #[test]
    fn test_extensions_preservation() {
        let adapter = BfV1Adapter::new();
        let bf_data_with_extensions = serde_json::json!({
            "id": "test",
            "title": "Test",
            "description": "Desc",
            "design": "",
            "acceptance_criteria": "",
            "notes": "",
            "status": "open",
            "priority": 2,
            "issue_type": "task",
            "created_at": "2026-08-10T00:00:00Z",
            "updated_at": "2026-08-10T00:00:00Z",
            "compaction_level": 1,
            "closed_by_session": "session-123"
        });

        let result = adapter.profile_to_native(&bf_data_with_extensions).unwrap();
        let issue: Issue = serde_json::from_value(result.data).unwrap();

        // Check that extensions were preserved
        assert!(issue.extensions.contains_key("compaction_level"));
        assert!(issue.extensions.contains_key("closed_by_session"));
    }
}
