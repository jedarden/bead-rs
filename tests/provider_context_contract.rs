//! Same-line byte-window/context witnesses for accepted ruleset-v4 4.2.
use bead_rs::scan::{self, Field, ScanConfig};

fn has_rule(text: &str, rule: &str) -> bool {
    scan::scan(
        &ScanConfig::enforce(),
        "owned-context-fixture",
        &[Field::new("description", text)],
    )
    .blocking
    .iter()
    .any(|finding| finding.rule_id == rule)
}

#[test]
fn vault_context_uses_sixty_four_bytes_not_unicode_characters() {
    let candidate = format!("{}.{}", "s", "aB3".repeat(8));
    let rule = "vault-legacy-token";
    assert!(has_rule(
        &format!("VAULT{}{candidate}", " ".repeat(59)),
        rule
    ));
    assert!(!has_rule(
        &format!("vault{}{candidate}", " ".repeat(60)),
        rule
    ));
    assert!(!has_rule(
        &format!("vault{} {candidate}", "é".repeat(30)),
        rule
    ));
    assert!(has_rule(
        &format!("vault{} {candidate}", "é".repeat(29)),
        rule
    ));
    for ending in ["\n", "\r\n", "\r"] {
        assert!(!has_rule(&format!("vault{ending}{candidate}"), rule));
    }
    assert!(!has_rule(&candidate, rule));
}

#[test]
fn backblaze_context_accepts_all_separators_with_lowercase_value_and_byte_bound() {
    let candidate = format!("00{}a", "a3".repeat(11));
    let rule = "backblaze-key-id-assignment";
    for identifier in [
        "key_id",
        "key-id",
        "key id",
        "keyid",
        "access-key-id",
        "account-id",
        "account id",
    ] {
        assert!(
            has_rule(&format!("{identifier}={candidate}"), rule),
            "allowed context separator missing"
        );
    }
    assert!(has_rule(
        &format!("key_id{}={candidate}", " ".repeat(57)),
        rule
    ));
    assert!(!has_rule(
        &format!("key_id{}={candidate}", " ".repeat(58)),
        rule
    ));
    assert!(!has_rule(
        &format!("key_id={}", candidate.to_ascii_uppercase()),
        rule
    ));
    for ending in ["\n", "\r\n", "\r"] {
        // A space before the terminator prevents token dewrapping. Without
        // it, section 3.2 deliberately rejoins '=' and the encoded value.
        assert!(!has_rule(&format!("key_id= {ending}{candidate}"), rule));
        assert!(has_rule(&format!("key_id={ending}{candidate}"), rule));
    }
    assert!(!has_rule(&candidate, rule));
}
