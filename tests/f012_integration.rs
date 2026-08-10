//! F012 Interchange Profiles Integration Tests
//!
//! This test module validates the br-v1 and bf-v1 profile adapters using
//! clean-room fixtures from research/fixtures/.

use anyhow::Result;
use bead_rs::profile::{
    get_adapter, profile_export_loss_report_for_records, same_profile_round_trip, LossCategory,
};
use std::fs::File;
use std::io::BufReader;
use std::path::Path;

/// Read fixture file and parse as JSONL lines
fn read_fixture_jsonl(path: &Path) -> Result<Vec<serde_json::Value>> {
    let file = File::open(path)?;
    let reader = BufReader::new(file);
    let mut records = Vec::new();

    use std::io::BufRead;
    for line in reader.lines() {
        let line = line?;
        if !line.trim().is_empty() {
            let value: serde_json::Value = serde_json::from_str(&line)?;
            records.push(value);
        }
    }

    Ok(records)
}

#[cfg(test)]
mod f012_integration_tests {
    use super::*;

    #[test]
    fn test_br_v1_fixture_exists() {
        let fixture_path = Path::new("research/fixtures/br-v1/observed-valid.jsonl");
        assert!(
            fixture_path.exists(),
            "br-v1 fixture file should exist at {:?}",
            fixture_path
        );

        let records = read_fixture_jsonl(fixture_path).expect("Should read fixture file");
        assert!(!records.is_empty(), "br-v1 fixture should contain records");
        assert_eq!(
            records.len(),
            10,
            "br-v1 fixture should contain 10 records, got {}",
            records.len()
        );
    }

    #[test]
    fn test_br_v1_adapter_available() {
        let adapter = get_adapter("br-v1");
        assert!(adapter.is_ok(), "br-v1 adapter should be available");
        assert_eq!(adapter.unwrap().profile_id().as_str(), "br-v1");
    }

    #[test]
    fn test_bf_v1_adapter_available() {
        let adapter = get_adapter("bf-v1");
        assert!(adapter.is_ok(), "bf-v1 adapter should be available");
        assert_eq!(adapter.unwrap().profile_id().as_str(), "bf-v1");
    }

    #[test]
    fn test_profile_registry_lists_all_profiles() {
        let profiles = bead_rs::profile::list_profiles();
        assert!(profiles.contains(&"native-v1".to_string()));
        assert!(profiles.contains(&"needle-v1".to_string()));
        assert!(profiles.contains(&"br-v1".to_string()));
        assert!(profiles.contains(&"bf-v1".to_string()));
        assert_eq!(profiles.len(), 4, "Should have exactly 4 profiles");
    }

    #[test]
    fn test_br_v1_basic_record_transformation() {
        let adapter = get_adapter("br-v1").unwrap();

        // Create br-v1 format data
        let br_data = serde_json::json!({
            "id": "test-001",
            "title": "Test Issue",
            "description": "Test Description",
            "status": "open",
            "priority": 2,
            "issue_type": "task",
            "created_at": "2026-08-10T00:00:00Z",
            "updated_at": "2026-08-10T00:00:00Z"
        });

        let result = adapter.profile_to_native(&br_data);
        assert!(
            result.is_ok(),
            "Should successfully transform br-v1 to native: {:?}",
            result.err()
        );

        let transform_result = result.unwrap();
        assert!(transform_result.successful);
        assert!(!transform_result.data.is_null());

        // Verify the transformation worked
        let issue: bead_rs::model::Issue = serde_json::from_value(transform_result.data).unwrap();
        assert_eq!(issue.id, "test-001");
        assert_eq!(issue.title, "Test Issue");
        assert_eq!(format!("{:?}", issue.base_status), "Open");
    }

    #[test]
    fn test_br_v1_closed_status_mapping() {
        let adapter = get_adapter("br-v1").unwrap();

        // br-v1 uses "closed" instead of "finished"
        let br_closed_data = serde_json::json!({
            "id": "test-closed",
            "title": "Closed Issue",
            "status": "closed",
            "priority": 2,
            "issue_type": "task",
            "created_at": "2026-08-10T00:00:00Z",
            "updated_at": "2026-08-10T00:00:00Z"
        });

        let result = adapter.profile_to_native(&br_closed_data).unwrap();
        let issue: bead_rs::model::Issue = serde_json::from_value(result.data).unwrap();
        assert_eq!(format!("{:?}", issue.base_status), "Closed");
    }

    #[test]
    fn test_br_v1_profile_extensions_round_trip() {
        let adapter = get_adapter("br-v1").unwrap();

        // br-v1 has fields like "due_at", "estimated_minutes" that aren't native
        let br_data_with_extras = serde_json::json!({
            "id": "test-001",
            "title": "Test Issue",
            "status": "open",
            "priority": 2,
            "issue_type": "task",
            "created_at": "2026-08-10T00:00:00Z",
            "updated_at": "2026-08-10T00:00:00Z",
            "due_at": "2026-12-31T23:59:59Z",
            "estimated_minutes": 120,
            "external_ref": "EXT-123"
        });

        let result = adapter.profile_to_native(&br_data_with_extras);
        assert!(
            result.is_ok(),
            "Should handle unsupported fields gracefully: {:?}",
            result.err()
        );

        let transform_result = result.unwrap();
        assert!(
            transform_result.successful,
            "Should be successful even with unsupported fields"
        );

        let issue: bead_rs::model::Issue = serde_json::from_value(transform_result.data).unwrap();
        let exported = adapter.native_to_profile(&issue).unwrap();
        for field in ["due_at", "estimated_minutes", "external_ref"] {
            assert_eq!(exported.data[field], br_data_with_extras[field]);
        }
    }

    #[test]
    fn test_bf_v1_basic_record_transformation() {
        let adapter = get_adapter("bf-v1").unwrap();

        // bf-v1 requires 4 content fields (description, design, acceptance_criteria, notes)
        let bf_data = serde_json::json!({
            "id": "bf-test-001",
            "title": "BF Test Issue",
            "description": "Description",
            "design": "",
            "acceptance_criteria": "",
            "notes": "",
            "status": "open",
            "priority": 2,
            "issue_type": "task",
            "created_at": "2026-08-10T00:00:00Z",
            "updated_at": "2026-08-10T00:00:00Z"
        });

        let result = adapter.profile_to_native(&bf_data).unwrap();
        assert!(result.successful);

        let issue: bead_rs::model::Issue = serde_json::from_value(result.data).unwrap();
        assert_eq!(issue.id, "bf-test-001");
        assert_eq!(issue.title, "BF Test Issue");
        assert_eq!(issue.priority, 2);
    }

    #[test]
    fn test_bf_v1_deferred_status_loss() {
        let adapter = get_adapter("bf-v1").unwrap();

        // Create a native issue with deferred status (which bf-v1 doesn't support)
        let deferred_issue = bead_rs::model::Issue {
            id: "test-deferred".to_string(),
            title: "Deferred Issue".to_string(),
            description: None,
            notes: None,
            revision: None,
            priority: 2,
            issue_type: Some("task".to_string()),
            base_status: bead_rs::model::BaseStatus::Deferred,
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

        let result = adapter.native_to_profile(&deferred_issue);

        // bf-v1 should report error for deferred status
        assert!(result.is_ok(), "Should return result even with losses");
        let transform_result = result.unwrap();
        assert!(
            !transform_result.losses.is_empty(),
            "Should report loss for deferred status"
        );
        assert!(transform_result
            .losses
            .iter()
            .any(|l| matches!(l.category, LossCategory::StatusMapping)));
    }

    #[test]
    fn test_profile_validation_detects_missing_required_fields() {
        let adapter = get_adapter("br-v1").unwrap();

        // Missing required fields
        let incomplete_data = serde_json::json!({
            "title": "Test Issue"
            // Missing: id, status, priority, issue_type, timestamps
        });

        let losses = adapter.validate_profile_data(&incomplete_data).unwrap();
        assert!(!losses.is_empty(), "Should detect missing required fields");

        let loss_fields: Vec<&str> = losses.iter().map(|l| l.field_path.as_str()).collect();
        assert!(loss_fields.contains(&"id"));
        assert!(loss_fields.contains(&"status"));
        assert!(loss_fields.contains(&"priority"));
    }

    #[test]
    fn test_unsupported_profile_fails_closed() {
        let result = get_adapter("unsupported-profile");
        assert!(result.is_err(), "Unsupported profile should fail");
        // Just verify it's an error, don't check the message content
        assert!(result.is_err());
    }

    #[test]
    fn test_native_profile_round_trip_with_no_loss() {
        let adapter = get_adapter("native-v1").unwrap();

        let native_issue = bead_rs::model::Issue {
            id: "native-test".to_string(),
            title: "Native Test Issue".to_string(),
            description: Some("Description".to_string()),
            notes: None,
            revision: None,
            priority: 2,
            issue_type: Some("task".to_string()),
            base_status: bead_rs::model::BaseStatus::Open,
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
        };

        // Native to profile should have no loss
        let to_profile = adapter.native_to_profile(&native_issue).unwrap();
        assert!(
            to_profile.losses.is_empty(),
            "Native profile should have no losses"
        );

        // Profile to native should also have no loss
        let to_native = adapter.profile_to_native(&to_profile.data).unwrap();
        assert!(
            to_native.losses.is_empty(),
            "Round-trip should have no losses"
        );
    }

    #[test]
    fn test_profile_registry_persistence() {
        // Test that the global registry is consistent across calls
        let profiles1 = bead_rs::profile::list_profiles();
        let profiles2 = bead_rs::profile::list_profiles();

        assert_eq!(profiles1.len(), profiles2.len());
        assert!(profiles1.iter().all(|p| profiles2.contains(p)));
    }

    #[test]
    fn external_record_projection_exports_relationships_and_manual_block() {
        let issue = bead_rs::model::Issue {
            id: "blocked".to_string(),
            title: "Blocked issue".to_string(),
            description: None,
            notes: None,
            revision: None,
            priority: 2,
            issue_type: Some("task".to_string()),
            base_status: bead_rs::model::BaseStatus::Open,
            manual_blocked: Some(true),
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
        let labels = vec!["zeta".to_string(), "alpha".to_string()];
        let dependencies = vec![(
            "blocked".to_string(),
            "blocker".to_string(),
            "blocks".to_string(),
        )];

        for profile in ["br-v1", "bf-v1"] {
            let result = get_adapter(profile)
                .unwrap()
                .native_record_to_profile(&issue, &labels, &dependencies)
                .unwrap();
            assert_eq!(result.data["status"], "blocked");
            assert_eq!(result.data["labels"], serde_json::json!(["alpha", "zeta"]));
            assert_eq!(result.data["dependencies"][0]["issue_id"], "blocked");
            assert_eq!(result.data["dependencies"][0]["depends_on_id"], "blocker");
        }
    }

    #[test]
    fn external_import_projection_retains_relationships_and_manual_block() {
        for profile in ["br-v1", "bf-v1"] {
            let mut record = serde_json::json!({
                "id": "blocked",
                "title": "Blocked issue",
                "status": "blocked",
                "priority": 2,
                "issue_type": "task",
                "created_at": "2026-08-10T00:00:00Z",
                "updated_at": "2026-08-10T00:00:00Z",
                "labels": ["alpha"],
                "dependencies": [{
                    "issue_id": "blocked",
                    "depends_on_id": "blocker",
                    "type": "blocks"
                }]
            });
            if profile == "bf-v1" {
                let object = record.as_object_mut().unwrap();
                for field in ["description", "design", "acceptance_criteria", "notes"] {
                    object.insert(field.to_string(), serde_json::json!(""));
                }
                object.insert("events".to_string(), serde_json::json!([]));
            }

            let result = get_adapter(profile)
                .unwrap()
                .profile_to_native(&record)
                .unwrap();
            assert_eq!(result.data["manual_blocked"], true);
            assert_eq!(result.data["labels"], serde_json::json!(["alpha"]));
            assert_eq!(result.data["dependencies"][0]["blocker"], "blocker");
        }
    }

    #[test]
    fn accepted_same_profile_round_trip_fixtures_are_exact() {
        for profile in ["br-v1", "bf-v1"] {
            let path = Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("research/fixtures")
                .join(profile)
                .join("round-trip-cases.json");
            let fixture: serde_json::Value =
                serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
            for case in fixture["cases"].as_array().unwrap() {
                let (round_trip_output, report) =
                    same_profile_round_trip(profile, &case["input"]).unwrap();
                assert_eq!(round_trip_output, case["expected_output"]);
                assert_eq!(
                    serde_json::to_value(report).unwrap(),
                    case["expected_report"]
                );
                let imported = get_adapter(profile)
                    .unwrap()
                    .profile_to_native(&case["input"])
                    .unwrap();
                let labels = imported.data["labels"]
                    .as_array()
                    .map(|values| {
                        values
                            .iter()
                            .map(|value| value.as_str().unwrap().to_string())
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default();
                let mut issue_value = imported.data.clone();
                issue_value.as_object_mut().unwrap().remove("labels");
                issue_value.as_object_mut().unwrap().remove("dependencies");
                let issue: bead_rs::model::Issue = serde_json::from_value(issue_value).unwrap();
                let dependencies = imported.data["dependencies"]
                    .as_array()
                    .map(|values| {
                        values
                            .iter()
                            .map(|value| {
                                (
                                    issue.id.clone(),
                                    value["blocker"].as_str().unwrap().to_string(),
                                    value["kind"].as_str().unwrap().to_string(),
                                )
                            })
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default();
                let exported = get_adapter(profile)
                    .unwrap()
                    .native_record_to_profile(&issue, &labels, &dependencies)
                    .unwrap();
                assert_eq!(
                    exported.data, case["expected_output"],
                    "{} fixture {}",
                    profile, case["name"]
                );
            }
        }
    }

    #[test]
    fn accepted_export_loss_report_fixtures_are_exact() {
        for profile in ["br-v1", "bf-v1"] {
            let path = Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("research/fixtures")
                .join(profile)
                .join("loss-report-cases.json");
            let fixture: serde_json::Value =
                serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
            for case in fixture["cases"].as_array().unwrap() {
                let Some(input_records) = case.get("input_records") else {
                    continue;
                };
                let report = profile_export_loss_report_for_records(
                    profile,
                    input_records.as_array().unwrap(),
                )
                .unwrap();
                assert_eq!(
                    serde_json::to_value(report).unwrap(),
                    case["expected_report"],
                    "{} fixture {}",
                    profile,
                    case["name"]
                );
            }
        }
    }
}
