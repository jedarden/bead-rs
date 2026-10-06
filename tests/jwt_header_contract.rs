//! Independently assembled accepted ruleset-v4 sections 4.1/4.5 witnesses.
//! Only synthetic runtime strings are scanned; assertions never quote them.
use bead_rs::scan::{self, Disposition, Field, ScanConfig, Tier, RULESET_VERSION};

fn base64url(bytes: &[u8]) -> String {
    let alphabet = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut encoded = String::new();
    for chunk in bytes.chunks(3) {
        let word = ((chunk[0] as u32) << 16)
            | ((chunk.get(1).copied().unwrap_or(0) as u32) << 8)
            | chunk.get(2).copied().unwrap_or(0) as u32;
        for shift in (0..chunk.len() + 1).map(|index| 18 - 6 * index) {
            encoded.push(alphabet[((word >> shift) & 63) as usize] as char);
        }
    }
    encoded
}

fn token(header: &[u8]) -> String {
    // Payload JSON and signatures are intentionally not validated offline.
    let payload = ["eyJ", "notJson_A9"].concat();
    let signature = ["unsigned", "_fixture_A9"].concat();
    format!("{}.{payload}.{signature}", base64url(header))
}

fn jwt_findings(text: &str) -> Vec<scan::Finding> {
    scan::scan(
        &ScanConfig::enforce(),
        "owned-jwt-fixture",
        &[Field::new("description", text)],
    )
    .findings
    .into_iter()
    .filter(|finding| finding.rule_id == "json-web-token")
    .collect()
}

#[test]
fn valid_header_blocks_with_entire_raw_token_and_exact_fingerprint() {
    let token = token(br#"{"alg":"a","x":[{"k":1},{"k":2}]}"#);
    let text = format!("!{token}!");
    let findings = jwt_findings(&text);
    assert_eq!(findings.len(), 1);
    let finding = &findings[0];
    assert_eq!(finding.tier, Tier::Blocking);
    assert_eq!(finding.disposition, Disposition::Confirmed);
    assert_eq!((finding.start, finding.end), (1, 1 + token.len()));
    assert!(
        finding.fingerprint
            == scan::fingerprint::compute(
                RULESET_VERSION,
                "json-web-token",
                "owned-jwt-fixture",
                "description",
                1,
                1 + token.len(),
                token.as_bytes(),
            )
    );
}

#[test]
fn invalid_headers_are_advisory_checksum_lookalikes_not_blocking() {
    for header in [
        br#"{"alg":""}"#.as_slice(),
        br#"{"alg":null}"#,
        br#"{"alg":1}"#,
        br#"{"alg":"a","alg":"b"}"#,
        br#"{"alg":"a","x":[{"k":1,"\u006b":2}]}"#,
        br#"{"alg":"a"} false"#,
        b"{\"alg\":\"a\",\"x\":\"\xff\"}",
    ] {
        let findings = jwt_findings(&token(header));
        assert_eq!(
            findings.len(),
            1,
            "header failure disappeared instead of being advisory"
        );
        assert_eq!(findings[0].tier, Tier::Advisory);
        assert_eq!(findings[0].disposition, Disposition::ChecksumFailed);
    }
    let mut noncanonical = token(br#"{"alg":"a"}"#).into_bytes();
    let last_header = noncanonical.iter().position(|byte| *byte == b'.').unwrap() - 1;
    noncanonical[last_header] += 1;
    let findings = jwt_findings(std::str::from_utf8(&noncanonical).unwrap());
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].tier, Tier::Advisory);
    assert_eq!(findings[0].disposition, Disposition::ChecksumFailed);
}

#[test]
fn segment_count_and_alphabet_failures_are_not_candidates() {
    let token = token(br#"{"alg":"a"}"#);
    for text in [
        format!("{token}.extra_A9"),
        format!("{token}.."),
        format!(".{token}"),
        format!("_{token}"),
        format!("-{token}"),
        token.replacen('.', "..", 1),
        token.replacen('.', "=.", 1),
        token.replacen('.', "+.", 1),
    ] {
        assert!(
            jwt_findings(&text).is_empty(),
            "nonmaximal/invalid token shape matched"
        );
    }
}

#[test]
fn checksum_advisories_survive_derived_views_with_raw_range_provenance() {
    let token = token(br#"{"alg":null}"#);
    let normalized = token.replacen('.', "%2e", 1);
    let split = token.len() / 2;
    let dewrapped = format!("{}\\\r\n  {}", &token[..split], &token[split..]);
    let encoded = base64url(token.as_bytes());
    for text in [normalized, dewrapped, encoded] {
        let raw = format!("!{text}!");
        let findings = jwt_findings(&raw);
        assert_eq!(findings.len(), 1, "derived lookalike missing or duplicated");
        let finding = &findings[0];
        assert_eq!(finding.tier, Tier::Advisory);
        assert_eq!(finding.disposition, Disposition::ChecksumFailed);
        assert_eq!((finding.start, finding.end), (1, 1 + text.len()));
        assert!(
            finding.fingerprint
                == scan::fingerprint::compute(
                    RULESET_VERSION,
                    "json-web-token",
                    "owned-jwt-fixture",
                    "description",
                    1,
                    1 + text.len(),
                    text.as_bytes(),
                )
        );
    }
}

#[test]
fn github_and_npm_checksum_failures_report_advisory_tier() {
    for prefix in [["gh", "p_"].concat(), ["np", "m_"].concat()] {
        let body = "aB7cD9eF2gH4jK6mN8pQ1rS3tV5wX0yZ2aB4";
        let report = scan::scan(
            &ScanConfig::enforce(),
            "owned-jwt-fixture",
            &[Field::new("description", &format!("{prefix}{body}"))],
        );
        let findings: Vec<_> = report
            .findings
            .iter()
            .filter(|finding| {
                matches!(
                    finding.rule_id.as_str(),
                    "github-classic-token" | "npm-publish-token"
                )
            })
            .collect();
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].tier, Tier::Advisory);
        assert_eq!(findings[0].disposition, Disposition::ChecksumFailed);
        assert!(report.blocking.is_empty());
    }
}
