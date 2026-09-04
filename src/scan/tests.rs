use super::*;
use std::time::{Duration, Instant};

fn aws_shaped_value() -> String {
    ["AK", "IA", "7Q9W2E4R6T8Y1U3I"].concat()
}

fn aws_secret_access_key_value() -> String {
    let alphabet = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    (0..40)
        .map(|index| alphabet[(index * 11 + 7) % alphabet.len()] as char)
        .collect()
}

fn aws_secret_access_key_assignment(namespace: &str) -> String {
    format!(
        "{namespace}AWS_SECRET_ACCESS_KEY={}",
        aws_secret_access_key_value()
    )
}

fn garage_access_key_id() -> String {
    [["G", "K"].concat(), "7e4a19c2b6d83f501ac942".to_string()].concat()
}

fn garage_access_key_id_assignment(namespace: &str) -> String {
    format!("{namespace}AWS_ACCESS_KEY_ID={}", garage_access_key_id())
}

fn github_checksum_value() -> String {
    let alphabet = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz";
    let payload: String = (0..30)
        .map(|index| alphabet[(index * 7 + 3) % alphabet.len()] as char)
        .collect();
    let checksum = rules::encode_base62_crc32(Checksum::GithubBase62Crc32, payload.as_bytes());
    [["gh", "p_"].concat(), payload, checksum].concat()
}

#[test]
fn provider_formatted_value_blocks_without_exposing_bytes() {
    let value = aws_shaped_value();
    let report = scan(
        &ScanConfig::enforce(),
        "issue:new",
        &[Field::new("description", &value)],
    );
    assert_eq!(report.blocking.len(), 1);
    let finding = &report.blocking[0];
    assert_eq!(finding.rule_id, "aws-access-key-id");
    assert_eq!((finding.start, finding.end), (0, value.len()));

    let rendered = format!(
        "{:?}\n{}\n{}",
        finding,
        finding,
        serde_json::to_string(finding).expect("finding serializes")
    );
    assert!(!rendered.contains(&value));
    assert!(rendered.contains(&finding.fingerprint));
}

#[test]
fn aws_secret_access_key_assignment_blocks_with_or_without_namespace() {
    for namespace in ["", "BEDROCK_"] {
        let assignment = aws_secret_access_key_assignment(namespace);
        let text = format!("runtime setting: {assignment}\n");
        let report = scan(
            &ScanConfig::enforce(),
            "issue:new",
            &[Field::new("description", &text)],
        );
        let finding = report
            .blocking
            .iter()
            .find(|finding| finding.rule_id == "aws-secret-access-key-assignment")
            .expect("the exact AWS assignment must block");
        assert_eq!(&text[finding.start..finding.end], assignment);
        let rendered = format!("{finding:?} {finding}");
        assert!(!rendered.contains(&assignment));
        assert!(!rendered.contains(&aws_secret_access_key_value()));
    }
}

#[test]
fn aws_secret_access_key_assignment_preserves_placeholder_downgrade() {
    let assignment = format!("AWS_SECRET_ACCESS_KEY={}", "A".repeat(40));
    let report = scan(
        &ScanConfig::enforce(),
        "issue:new",
        &[Field::new("notes", &assignment)],
    );
    assert!(report.blocking.is_empty());
    assert!(report.findings.iter().any(|finding| {
        finding.rule_id == "aws-secret-access-key-assignment"
            && finding.disposition == Disposition::Placeholder
    }));
}

#[test]
fn garage_access_key_id_blocks_only_in_assignment_context() {
    for namespace in ["", "SCCACHE_"] {
        let assignment = garage_access_key_id_assignment(namespace);
        let text = format!("runtime setting: {assignment}\n");
        let report = scan(
            &ScanConfig::enforce(),
            "issue:new",
            &[Field::new("description", &text)],
        );
        let finding = report
            .blocking
            .iter()
            .find(|finding| finding.rule_id == "garage-access-key-id-assignment")
            .expect("the contextual Garage key ID must block");
        assert_eq!(&text[finding.start..finding.end], assignment);
        let rendered = format!("{finding:?} {finding}");
        assert!(!rendered.contains(&assignment));
        assert!(!rendered.contains(&garage_access_key_id()));
    }

    let value = garage_access_key_id();
    let report = scan(
        &ScanConfig::enforce(),
        "issue:new",
        &[Field::new("notes", &value)],
    );
    assert!(report
        .blocking
        .iter()
        .all(|finding| finding.rule_id != "garage-access-key-id-assignment"));
}

#[test]
fn unlabelled_aws_secret_access_key_shape_does_not_block() {
    let value = aws_secret_access_key_value();
    let report = scan(
        &ScanConfig::enforce(),
        "issue:new",
        &[Field::new("notes", &value)],
    );
    assert!(report.blocking.is_empty());
}

#[test]
fn placeholder_shape_is_advisory_not_blocking() {
    let value = ["AK", "IA", &"A".repeat(16)].concat();
    let report = scan(
        &ScanConfig::enforce(),
        "issue:new",
        &[Field::new("notes", &value)],
    );
    assert!(report.blocking.is_empty());
    assert!(report
        .findings
        .iter()
        .any(|finding| finding.disposition == Disposition::Placeholder));
}

#[test]
fn embedded_checksum_controls_blocking_disposition() {
    let valid = github_checksum_value();
    let valid_report = scan(
        &ScanConfig::enforce(),
        "issue:new",
        &[Field::new("notes", &valid)],
    );
    assert_eq!(valid_report.blocking.len(), 1);

    let mut invalid = valid;
    let replacement = if invalid.ends_with('0') { "1" } else { "0" };
    invalid.replace_range(invalid.len() - 1.., replacement);
    let invalid_report = scan(
        &ScanConfig::enforce(),
        "issue:new",
        &[Field::new("notes", &invalid)],
    );
    assert!(invalid_report.blocking.is_empty());
    assert!(invalid_report
        .findings
        .iter()
        .any(|finding| finding.disposition == Disposition::ChecksumFailed));
}

#[test]
fn exact_fingerprint_acknowledgment_admits_only_that_finding() {
    let value = aws_shaped_value();
    let first = scan(
        &ScanConfig::enforce(),
        "issue:new",
        &[Field::new("description", &value)],
    );
    let fingerprint = first.blocking[0].fingerprint.clone();
    let acknowledged = serde_json::json!([fingerprint]);
    let config = ScanConfig::from_config_values(None, Some(&acknowledged)).unwrap();
    let admitted = scan(&config, "issue:new", &[Field::new("description", &value)]);

    assert!(admitted.is_admitted());
    assert_eq!(admitted.acknowledged.len(), 1);
    assert_eq!(admitted.findings[0].tier, Tier::Blocking);
    assert!(admitted.findings[0].is_blocking_match());

    let changed_selector = scan(
        &config,
        "issue:different",
        &[Field::new("description", &value)],
    );
    assert_eq!(changed_selector.blocking.len(), 1);
}

#[test]
fn malformed_configuration_fails_closed_without_echoing_the_value() {
    let invalid_mode = ["not", "-a-secret-mode"].concat();
    let raw = serde_json::Value::String(invalid_mode.clone());
    let error = ScanConfig::from_config_values(Some(&raw), None).unwrap_err();
    let rendered = format!("{error:?} {error}");
    assert!(!rendered.contains(&invalid_mode));

    let invalid_ack = ["credential", "-shaped-not-a-fingerprint"].concat();
    let raw = serde_json::json!([invalid_ack]);
    let error = ScanConfig::from_config_values(None, Some(&raw)).unwrap_err();
    let rendered = format!("{error:?} {error}");
    assert!(!rendered.contains("credential-shaped"));
}

#[test]
fn workspace_configuration_defaults_to_enforce_and_loads_exact_acknowledgments() {
    let workspace = tempfile::tempdir().unwrap();
    std::fs::create_dir(workspace.path().join(".beads")).unwrap();
    std::fs::write(
        workspace.path().join(".beads/config.json"),
        serde_json::to_vec(&serde_json::json!({"uuid":"test"})).unwrap(),
    )
    .unwrap();
    let defaulted = ScanConfig::load_from_workspace_root(workspace.path()).unwrap();
    assert_eq!(defaulted.mode(), Mode::Enforce);
    assert_eq!(defaulted.acknowledged().count(), 0);

    let fingerprint = "a".repeat(64);
    std::fs::write(
        workspace.path().join(".beads/config.json"),
        serde_json::to_vec(&serde_json::json!({
            "secret_scan": {
                "mode": "advisory",
                "acknowledged": [fingerprint.clone()]
            }
        }))
        .unwrap(),
    )
    .unwrap();
    let configured = ScanConfig::load_from_workspace_root(workspace.path()).unwrap();
    assert_eq!(configured.mode(), Mode::Advisory);
    assert_eq!(
        configured.acknowledged().collect::<Vec<_>>(),
        vec![fingerprint.as_str()]
    );
}

#[test]
fn workspace_configuration_fails_closed_without_echoing_content() {
    let workspace = tempfile::tempdir().unwrap();
    std::fs::create_dir(workspace.path().join(".beads")).unwrap();
    let config_path = workspace.path().join(".beads/config.json");

    std::fs::write(&config_path, br#"{"secret_scan":"not-an-object"}"#).unwrap();
    let wrong_section = ScanConfig::load_from_workspace_root(workspace.path()).unwrap_err();
    assert!(matches!(wrong_section, ScanConfigError::WrongSectionType));
    assert!(!wrong_section.to_string().contains("not-an-object"));

    std::fs::write(&config_path, b"not-json").unwrap();
    let malformed = ScanConfig::load_from_workspace_root(workspace.path()).unwrap_err();
    assert!(matches!(malformed, ScanConfigError::ConfigInvalidJson));
    assert!(!malformed.to_string().contains("not-json"));
}

#[test]
fn invocation_acknowledgment_validation_never_echoes_input() {
    let mut config = ScanConfig::enforce();
    let invalid = "not-a-fingerprint";
    let error = config
        .add_invocation_acknowledgments([invalid])
        .unwrap_err();
    assert!(matches!(
        error,
        ScanConfigError::InvalidInvocationAcknowledgment { index: 0 }
    ));
    assert!(!error.to_string().contains(invalid));
}

#[test]
fn fingerprints_are_deterministic_and_bind_location() {
    let value = aws_shaped_value();
    let field = Field::new("description", &value);
    let first = scan(&ScanConfig::enforce(), "issue:a", &[field]);
    let second = scan(
        &ScanConfig::enforce(),
        "issue:a",
        &[Field::new("description", &value)],
    );
    let moved = scan(
        &ScanConfig::enforce(),
        "issue:a",
        &[Field::new("notes", &value)],
    );
    assert_eq!(
        first.findings[0].fingerprint,
        second.findings[0].fingerprint
    );
    assert_ne!(first.findings[0].fingerprint, moved.findings[0].fingerprint);
}

#[test]
fn unicode_prefix_uses_byte_offsets() {
    let prefix = "é🎯 ";
    let value = format!("{prefix}{}", aws_shaped_value());
    let report = scan(
        &ScanConfig::enforce(),
        "issue:new",
        &[Field::new("description", &value)],
    );
    assert_eq!(report.blocking[0].start, prefix.len());
    assert_eq!(report.blocking[0].end, value.len());
}

#[test]
fn advisory_and_off_modes_never_reject() {
    let value = aws_shaped_value();
    let advisory = scan(
        &ScanConfig::new(Mode::Advisory),
        "issue:new",
        &[Field::new("notes", &value)],
    );
    assert!(reject_if_blocked(&ScanConfig::new(Mode::Advisory), &advisory).is_none());
    assert!(!advisory.findings.is_empty());

    let off = scan(
        &ScanConfig::new(Mode::Off),
        "issue:new",
        &[Field::new("notes", &value)],
    );
    assert!(off.is_clean());
}

#[test]
fn hostile_four_mebibyte_clean_field_has_bounded_scan_cost() {
    let text = "z".repeat(4 * 1024 * 1024);
    let started = Instant::now();
    let report = scan(
        &ScanConfig::enforce(),
        "issue:new",
        &[Field::new("description", &text)],
    );
    assert!(report.is_clean());
    assert!(
        started.elapsed() < Duration::from_secs(5),
        "4 MiB no-anchor scan exceeded the generous regression bound"
    );
}

#[test]
fn rejection_contains_remediation_but_never_the_value() {
    let value = aws_shaped_value();
    let config = ScanConfig::enforce();
    let report = scan(&config, "issue:new", &[Field::new("description", &value)]);
    let rejection = reject_if_blocked(&config, &report).expect("must reject");
    assert!(rejection.message.contains(SECRET_DETECTED));
    assert!(rejection.message.contains("rotate"));
    assert!(rejection.message.contains(&rejection.finding.fingerprint));
    assert!(!rejection.message.contains(&value));
}

#[test]
fn secret_buffer_rendering_is_redacted() {
    let value = aws_shaped_value();
    let secret = SecretBytes::from_str(&value);
    assert_eq!(secret.len(), value.len());
    let rendered = format!("{secret:?} {secret}");
    assert!(!rendered.contains(&value));
}

fn exact_acknowledged_report() -> (String, ScanReport) {
    let value = aws_shaped_value();
    let first = scan(
        &ScanConfig::enforce(),
        "issue:new",
        &[Field::new("description", &value)],
    );
    let acknowledged = serde_json::json!([first.blocking[0].fingerprint]);
    let config = ScanConfig::from_config_values(None, Some(&acknowledged)).unwrap();
    let report = scan(&config, "issue:new", &[Field::new("description", &value)]);
    (value, report)
}

fn audit_test_connection() -> rusqlite::Connection {
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    conn.execute_batch(
        "CREATE TABLE events (
            sequence INTEGER PRIMARY KEY AUTOINCREMENT,
            issue_id TEXT,
            kind TEXT NOT NULL,
            actor TEXT,
            time TEXT NOT NULL,
            detail TEXT NOT NULL
        );",
    )
    .unwrap();
    conn
}

#[test]
fn exact_acknowledgment_audit_commits_once_with_the_mutation() {
    let (value, report) = exact_acknowledged_report();
    let _guard = arm_acknowledgment_audit(&report, "test-actor");
    let mut conn = audit_test_connection();
    install_acknowledgment_audit_bridge(&conn).unwrap();

    let tx = conn.transaction().unwrap();
    tx.execute(
        "INSERT INTO events (kind, actor, time, detail) VALUES ('created', 'test-actor', 'now', '{}')",
        [],
    )
    .unwrap();
    tx.execute(
        "INSERT INTO events (kind, actor, time, detail) VALUES ('updated', 'test-actor', 'now', '{}')",
        [],
    )
    .unwrap();
    tx.commit().unwrap();

    let count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM events WHERE kind = 'secret_acknowledged'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(count, 1);
    let (actor, detail): (String, String) = conn
        .query_row(
            "SELECT actor, detail FROM events WHERE kind = 'secret_acknowledged'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(actor, "test-actor");
    assert!(!detail.contains(&value));
    assert!(detail.contains(&report.acknowledged[0].fingerprint));
    assert!(detail.contains(&report.acknowledged[0].rule_id));
}

#[test]
fn exact_acknowledgment_audit_rolls_back_with_the_mutation() {
    let (_, report) = exact_acknowledged_report();
    let _guard = arm_acknowledgment_audit(&report, "test-actor");
    let mut conn = audit_test_connection();
    install_acknowledgment_audit_bridge(&conn).unwrap();

    let tx = conn.transaction().unwrap();
    tx.execute(
        "INSERT INTO events (kind, actor, time, detail) VALUES ('created', 'test-actor', 'now', '{}')",
        [],
    )
    .unwrap();
    tx.rollback().unwrap();
    let rolled_back: i64 = conn
        .query_row("SELECT COUNT(*) FROM events", [], |row| row.get(0))
        .unwrap();
    assert_eq!(rolled_back, 0);

    let tx = conn.transaction().unwrap();
    tx.execute(
        "INSERT INTO events (kind, actor, time, detail) VALUES ('created', 'test-actor', 'later', '{}')",
        [],
    )
    .unwrap();
    tx.commit().unwrap();
    let committed: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM events WHERE kind = 'secret_acknowledged'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(committed, 1);
}
