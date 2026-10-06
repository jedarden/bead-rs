//! Independently assembled full-identifier/table witnesses for accepted
//! ruleset-v4 section 4.4. No real credentials or stores are used.
use bead_rs::scan::{self, Field, ScanConfig, Tier};

fn assignment_findings(text: &str) -> Vec<scan::Finding> {
    scan::scan(
        &ScanConfig::enforce(),
        "owned-assignment-fixture",
        &[Field::new("description", text)],
    )
    .findings
    .into_iter()
    .filter(|finding| {
        matches!(
            finding.rule_id.as_str(),
            "credential-assignment" | "advisory-keyword-assignment"
        )
    })
    .collect()
}

#[test]
fn literal_exclusions_and_whole_identifier_boundaries_apply_to_both_tiers() {
    for value in ["a3".repeat(20), "ordinaryWord0".to_string()] {
        for identifier in [
            "acknowledge-secret",
            "fencing-token",
            "fencing_token",
            "max_tokens",
            "secret_scan_extra",
            "secret-scan-extra",
            "tokens",
            "passwords",
            "apikey",
        ] {
            for text in [
                format!("{identifier}={value}"),
                format!("--{identifier}={value}"),
                format!("--{identifier} {value}"),
            ] {
                assert!(
                    assignment_findings(&text).is_empty(),
                    "excluded identifier produced assignment finding"
                );
            }
        }
    }
    let value = "a3".repeat(20);
    for identifier in [
        "acknowledge_secret",
        "fencing.token",
        "apiKey",
        "API Key",
        "prefix_access_key",
        "1_token",
    ] {
        let text = format!("{identifier} = {value}");
        let findings = assignment_findings(&text);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].tier, Tier::Blocking);
        assert_eq!(
            (findings[0].start, findings[0].end),
            (text.len() - value.len(), text.len())
        );
    }
    for identifier in [
        format!("{}_token", "a".repeat(59)),
        ".token".to_string(),
        "-token".to_string(),
    ] {
        assert!(
            assignment_findings(&format!("{identifier}={value}")).is_empty(),
            "partial/overlong identifier matched"
        );
    }
}

#[test]
fn required_marker_full_separator_terminal_bar_and_punctuation_preserve_ranges() {
    let value = "a3".repeat(20);
    for (prefix, suffix) in [
        ("- token  ", ""),
        ("* token\t", ""),
        ("| token\t\t\t", ""),
        ("  | API Key | ", "|"),
        ("- token   |\t ", "  | \t"),
        ("- API Key   ", ".)]}"),
        ("* token\t \t", ".)]}|"),
    ] {
        for ending in ["\n", "\r\n", "\r", ""] {
            let text = format!("prologue\n{prefix}{value}{suffix}{ending}");
            let findings = assignment_findings(&text);
            assert_eq!(findings.len(), 1, "valid table missing or duplicated");
            assert_eq!(findings[0].tier, Tier::Blocking);
            let start = "prologue\n".len() + prefix.len();
            assert_eq!(
                (findings[0].start, findings[0].end),
                (start, start + value.len())
            );
        }
    }
}

#[test]
fn tables_refuse_missing_marker_quotes_extra_columns_and_trailing_prose() {
    let value = "a3".repeat(20);
    for text in [
        format!("token  {value}"),
        format!("- token {value}"),
        format!("- token | \"{value}\""),
        format!("- token | '{value}'"),
        format!("- token | `{value}`"),
        format!("| token | {value} | extra |"),
        format!("- token\t{value} trailing"),
        format!("\t- token  {value}"),
    ] {
        assert!(
            assignment_findings(&text).is_empty(),
            "invalid table admitted"
        );
    }
}

#[test]
fn table_uses_twenty_nonword_bytes_and_has_q_failure_advisory_fallback() {
    let value = "aB3".repeat(4);
    let assignment = assignment_findings(&format!("token={value}"));
    let table = assignment_findings(&format!("- token\t\t{value}"));
    assert_eq!(assignment.len(), 1);
    assert_eq!(assignment[0].tier, Tier::Blocking);
    assert_eq!(table.len(), 1);
    assert_eq!(table[0].tier, Tier::Advisory);
}

#[test]
fn assignment_exclusion_does_not_suppress_independent_provider_rule() {
    let provider = ["AK", "IA", "7Q9W2E4R6T8Y1U3I"].concat();
    let report = scan::scan(
        &ScanConfig::enforce(),
        "owned-assignment-fixture",
        &[Field::new(
            "description",
            &format!("fencing-token={provider}"),
        )],
    );
    assert!(report
        .blocking
        .iter()
        .any(|finding| finding.rule_id == "aws-access-key-id"));
    assert!(!report
        .findings
        .iter()
        .any(|finding| finding.rule_id == "credential-assignment"));
}

#[test]
fn noncredential_prose_separators_do_not_hide_a_following_assignment() {
    let value = "aB3".repeat(12);
    for prefix in [
        "deploy notes: ",
        "diagnostic: output=",
        "url=https://site.invalid/?",
    ] {
        let text = format!("{prefix}service_token = {value}");
        let findings = assignment_findings(&text);
        assert_eq!(
            findings.len(),
            1,
            "prose consumed the credential identifier"
        );
        assert_eq!(findings[0].tier, Tier::Blocking);
        assert_eq!(
            (findings[0].start, findings[0].end),
            (text.len() - value.len(), text.len())
        );
    }
}

#[test]
fn repeated_noncredential_assignments_preserve_the_final_complete_identifier() {
    let value = "aB3".repeat(12);
    for copies in [1, 32, 4096] {
        let text = format!("{}service_token={value}", "noop=".repeat(copies));
        let findings = assignment_findings(&text);
        assert_eq!(findings.len(), 1);
        assert_eq!(
            (findings[0].start, findings[0].end),
            (text.len() - value.len(), text.len())
        );
    }
}
