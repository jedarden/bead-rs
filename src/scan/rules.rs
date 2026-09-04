//! Built-in, versioned secret-detection ruleset (ADR-014, BR-T14).
//!
//! The ruleset is closed and compiled into the binary: no workspace, config
//! key, environment variable, or invocation can add, remove, or alter a rule
//! (the only channel that changes it is a bead-rs release, which is why
//! [`RULESET_VERSION`] is a constant). Every blocking rule is a
//! provider-formatted credential pattern or private-key armor chosen for
//! near-zero false-positive behavior; everything statistical is advisory and
//! never rejects a mutation.
//!
//! Patterns are RE2-syntax (no lookaround) so they compile under the
//! linear-time `regex` engine and bound the cost of a hostile 4 MiB field.

/// Version of the compiled ruleset. Bumping it changes every fingerprint
/// (the version is hashed into the finding fingerprint), so it moves only
/// with a release that re-justifies each blocking rule.
pub const RULESET_VERSION: u32 = 2;

/// Identity of the normative contract this ruleset implements.
pub const CONTRACT_IDENTITY: &str = "urn:bead-rs:spec:secret-rejection:v1";

/// Domain separator hashed into every finding fingerprint, ahead of the
/// ruleset version, rule id, selector, field path, byte range, and matched
/// bytes.
pub const FINGERPRINT_DOMAIN: &str = "urn:bead-rs:spec:secret-rejection:v1:fingerprint:v1";

/// Whether a rule can reject a mutation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tier {
    /// Provider-formatted credentials and private-key armor: rejects under
    /// `enforce` mode.
    Blocking,
    /// Statistical findings: reported by `doctor` and dry-run, never rejects.
    Advisory,
}

/// Embedded checksum formats that a candidate must validate to stay blocking.
///
/// A candidate that fails its own format's checksum is a lookalike, and is
/// downgraded to advisory rather than blocking (spec §3).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Checksum {
    /// GitHub classic tokens: 36 base62 characters after the prefix, the last
    /// 6 being a base62-encoded CRC32 of the first 30.
    GithubBase62Crc32,
    /// npm tokens: 36 base62 characters after the prefix, the last 8 being a
    /// base62-encoded CRC32 of the first 28.
    NpmBase62Crc32,
}

/// One detection rule.
pub struct Rule {
    /// Stable rule identifier reported in findings and audit events.
    pub id: &'static str,
    /// Credential provider or category, reported in findings.
    pub provider: &'static str,
    /// Whether this rule can reject a mutation.
    pub tier: Tier,
    /// Lowercase-insensitive keyword anchors. A field is only regex-scanned
    /// for this rule when at least one anchor occurs in it — the prefilter
    /// that keeps the common case (no keyword hit) to a single pass.
    pub keywords: &'static [&'static str],
    /// Anchored RE2-syntax pattern. Group 1, when present, is the candidate
    /// body that placeholder and checksum dispositions evaluate.
    pub pattern: &'static str,
    /// Embedded checksum validator, when the format defines one.
    pub checksum: Option<Checksum>,
}

/// The compiled, ordered ruleset. Order is part of the module contract:
/// findings are emitted in rule-table order before the caller's
/// deterministic sort.
pub const RULES: &[Rule] = &[
    Rule {
        id: "pem-private-key",
        provider: "generic",
        tier: Tier::Blocking,
        keywords: &["private key"],
        pattern: r"-----BEGIN (?:[A-Z0-9]+ )*PRIVATE KEY(?: BLOCK)?-----",
        checksum: None,
    },
    Rule {
        id: "aws-access-key-id",
        provider: "aws",
        tier: Tier::Blocking,
        keywords: &["akia", "asia", "abia", "acca"],
        pattern: r"\b(?:AKIA|ASIA|ABIA|ACCA)([0-9A-Z]{16})\b",
        checksum: None,
    },
    Rule {
        id: "aws-secret-access-key-assignment",
        provider: "aws",
        tier: Tier::Blocking,
        // The prefilter emits non-overlapping matches. Use the leading AWS
        // segment so the shorter advisory `secret` anchor cannot mask this
        // rule before the exact assignment regex runs.
        keywords: &["aws_"],
        pattern: r#"(?i)\b(?:[A-Z][A-Z0-9]*_)*AWS_SECRET_ACCESS_KEY["']?[ \t]*[:=][ \t]*["']?([A-Za-z0-9/+=]{40})["']?"#,
        checksum: None,
    },
    Rule {
        id: "github-classic-token",
        provider: "github",
        tier: Tier::Blocking,
        keywords: &["ghp_", "gho_", "ghu_", "ghs_", "ghr_"],
        pattern: r"\bgh[pousr]_([0-9A-Za-z]{36})\b",
        checksum: Some(Checksum::GithubBase62Crc32),
    },
    Rule {
        id: "github-fine-grained-pat",
        provider: "github",
        tier: Tier::Blocking,
        keywords: &["github_pat_"],
        pattern: r"\b(github_pat_[0-9A-Za-z_]{82})\b",
        checksum: None,
    },
    Rule {
        id: "gitlab-pat",
        provider: "gitlab",
        tier: Tier::Blocking,
        keywords: &["glpat-"],
        pattern: r"\b(glpat-[0-9A-Za-z_-]{20})\b",
        checksum: None,
    },
    Rule {
        id: "npm-publish-token",
        provider: "npm",
        tier: Tier::Blocking,
        keywords: &["npm_"],
        pattern: r"\bnpm_([0-9A-Za-z]{36})\b",
        checksum: Some(Checksum::NpmBase62Crc32),
    },
    Rule {
        id: "slack-token",
        provider: "slack",
        tier: Tier::Blocking,
        keywords: &["xox"],
        pattern: r"\b(xox[abprs]-[0-9A-Za-z-]{20,})\b",
        checksum: None,
    },
    Rule {
        id: "openai-api-key",
        provider: "openai",
        tier: Tier::Blocking,
        keywords: &["t3blbkfj"],
        pattern: r"\bsk-(?:proj-)?[A-Za-z0-9_-]{20}T3BlbkFJ[A-Za-z0-9_-]{20}\b",
        checksum: None,
    },
    Rule {
        id: "anthropic-api-key",
        provider: "anthropic",
        tier: Tier::Blocking,
        keywords: &["sk-ant-"],
        pattern: r"\bsk-ant-(?:api|admin)[0-9]{0,2}-[A-Za-z0-9_-]{24,}\b",
        checksum: None,
    },
    Rule {
        id: "google-api-key",
        provider: "google",
        tier: Tier::Blocking,
        keywords: &["aiza"],
        pattern: r"\b(AIza[0-9A-Za-z_-]{35})\b",
        checksum: None,
    },
    Rule {
        id: "google-oauth-token",
        provider: "google",
        tier: Tier::Blocking,
        keywords: &["ya29."],
        pattern: r"\bya29\.[0-9A-Za-z_-]{20,}\b",
        checksum: None,
    },
    Rule {
        id: "stripe-secret-key",
        provider: "stripe",
        tier: Tier::Blocking,
        keywords: &["sk_live_", "rk_live_"],
        pattern: r"\b([sr]k_live_[0-9a-zA-Z]{24,})\b",
        checksum: None,
    },
    Rule {
        id: "pypi-upload-token",
        provider: "pypi",
        tier: Tier::Blocking,
        keywords: &["pypi-ageichlwas5vcmc"],
        pattern: r"\bpypi-AgEIcHlwaS5vcmc[A-Za-z0-9_-]{50,}\b",
        checksum: None,
    },
    Rule {
        id: "huggingface-token",
        provider: "huggingface",
        tier: Tier::Blocking,
        keywords: &["hf_"],
        pattern: r"\bhf_[0-9A-Za-z]{34}\b",
        checksum: None,
    },
    Rule {
        id: "vault-service-token",
        provider: "hashicorp",
        tier: Tier::Blocking,
        keywords: &["hvs."],
        pattern: r"\bhvs\.[0-9A-Za-z_-]{20,}\b",
        checksum: None,
    },
    Rule {
        id: "digitalocean-pat",
        provider: "digitalocean",
        tier: Tier::Blocking,
        keywords: &["dop_v1_"],
        pattern: r"\bdop_v1_[0-9a-f]{64}\b",
        checksum: None,
    },
    Rule {
        id: "sendgrid-api-key",
        provider: "sendgrid",
        tier: Tier::Blocking,
        keywords: &["sg."],
        pattern: r"\bSG\.[A-Za-z0-9_-]{22}\.[A-Za-z0-9_-]{43}\b",
        checksum: None,
    },
    Rule {
        id: "shopify-token",
        provider: "shopify",
        tier: Tier::Blocking,
        keywords: &["shpat_", "shppa_", "shpca_", "shpss_"],
        pattern: r"\bshp(?:at|pa|ca|ss)_[0-9a-fA-F]{32}\b",
        checksum: None,
    },
    Rule {
        id: "telegram-bot-token",
        provider: "telegram",
        tier: Tier::Blocking,
        keywords: &[":aa"],
        pattern: r"\b[0-9]{8,10}:AA[A-Za-z0-9_-]{33}\b",
        checksum: None,
    },
    Rule {
        id: "netlify-pat",
        provider: "netlify",
        tier: Tier::Blocking,
        keywords: &["nfp_"],
        pattern: r"\bnfp_[A-Za-z0-9]{40,}\b",
        checksum: None,
    },
    // Advisory tier: statistical, never rejects.
    Rule {
        id: "advisory-keyword-assignment",
        provider: "heuristic",
        tier: Tier::Advisory,
        keywords: &[
            "secret",
            "token",
            "password",
            "passwd",
            "pwd",
            "api_key",
            "apikey",
            "access_key",
            "private_key",
            "client_secret",
            "auth_token",
            "credential",
        ],
        pattern: r#"(?i)\b(?:secret|token|password|passwd|pwd|api[_-]?key|apikey|access[_-]?key|private[_-]?key|client[_-]?secret|auth[_-]?token|credential)["']?[ \t]*[:=][ \t]*["']?([A-Za-z0-9+/=_.~-]{16,})["']?"#,
        checksum: None,
    },
];

/// Rules whose findings come from the statistical scanner rather than a
/// pattern in [`RULES`]. Declared here so the rule inventory stays complete
/// and versioned in one place.
pub const ADVISORY_ENTROPY_RULE_ID: &str = "advisory-high-entropy-string";

/// Every rule id the ruleset can emit, table rules first, then the
/// statistical scanner's rule.
pub fn rule_ids() -> Vec<&'static str> {
    let mut ids: Vec<&'static str> = RULES.iter().map(|r| r.id).collect();
    ids.push(ADVISORY_ENTROPY_RULE_ID);
    ids
}

/// Lowercase keyword anchors fed to the prefilter, with the indexes of the
/// rules each anchor gates.
pub(crate) fn keyword_anchors() -> Vec<(String, Vec<usize>)> {
    let mut anchors: Vec<(String, Vec<usize>)> = Vec::new();
    for (rule_index, rule) in RULES.iter().enumerate() {
        for keyword in rule.keywords {
            let lowered = keyword.to_ascii_lowercase();
            match anchors.iter_mut().find(|(a, _)| *a == lowered) {
                Some((_, rules)) => rules.push(rule_index),
                None => anchors.push((lowered, vec![rule_index])),
            }
        }
    }
    anchors
}

/// The base62 alphabet shared by the GitHub and npm token checksums.
const BASE62: &[u8; 62] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz";

/// Validate an embedded base62 CRC32 checksum.
///
/// `body` is the full candidate captured after the provider prefix
/// (`chars`); `checksum_len` is how many trailing characters encode the
/// CRC32 of the remainder. Returns true when the candidate carries a valid
/// checksum for its own format.
pub(crate) fn checksum_valid(kind: Checksum, body: &str) -> bool {
    let chars: Vec<u8> = body.bytes().collect();
    let checksum_len = match kind {
        Checksum::GithubBase62Crc32 => 6,
        Checksum::NpmBase62Crc32 => 8,
    };
    if chars.len() <= checksum_len {
        return false;
    }
    let split = chars.len() - checksum_len;
    let (payload, encoded) = chars.split_at(split);
    if !encoded.iter().all(|b| BASE62.contains(b)) {
        return false;
    }
    let expected = crc32(payload);
    let mut decoded: u64 = 0;
    for byte in encoded {
        let digit = BASE62.iter().position(|c| c == byte).unwrap_or(62) as u64;
        decoded = decoded * 62 + digit;
        if decoded > u32::MAX as u64 {
            return false;
        }
    }
    decoded == expected as u64
}

fn crc32(data: &[u8]) -> u32 {
    // IEEE CRC-32, reflected, poly 0xEDB88320 — the same polynomial the
    // GitHub and npm token formats embed.
    let mut crc: u32 = 0xFFFF_FFFF;
    for byte in data {
        crc ^= *byte as u32;
        for _ in 0..8 {
            let mask = (crc & 1).wrapping_neg();
            crc = (crc >> 1) ^ (0xEDB8_8320 & mask);
        }
    }
    !crc
}

/// Encode a CRC32 as fixed-width base62, zero-padded on the left. Test-only
/// helper that generates format-valid samples without committing live-format
/// material: fixtures call it at test time on generated bodies.
pub fn encode_base62_crc32(kind: Checksum, payload: &[u8]) -> String {
    let checksum_len = match kind {
        Checksum::GithubBase62Crc32 => 6,
        Checksum::NpmBase62Crc32 => 8,
    };
    let mut value = crc32(payload) as u64;
    let mut encoded = vec![b'0'; checksum_len];
    for slot in encoded.iter_mut().rev() {
        *slot = BASE62[(value % 62) as usize];
        value /= 62;
    }
    String::from_utf8(encoded).expect("base62 alphabet is ASCII")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ruleset_is_closed_and_versioned() {
        assert_eq!(RULESET_VERSION, 2);
        assert_eq!(CONTRACT_IDENTITY, "urn:bead-rs:spec:secret-rejection:v1");
        // The blocking tier is provider formats and armor only.
        for rule in RULES.iter().filter(|r| r.tier == Tier::Blocking) {
            assert!(
                !rule.id.starts_with("advisory-"),
                "blocking rule {} must not claim the advisory namespace",
                rule.id
            );
            assert!(!rule.keywords.is_empty(), "{} has no prefilter", rule.id);
        }
        // Every rule id is unique.
        let mut ids = rule_ids();
        ids.sort_unstable();
        let count = ids.len();
        ids.dedup();
        assert_eq!(ids.len(), count, "duplicate rule id in the table");
    }

    #[test]
    fn patterns_are_re2_syntax_without_lookaround() {
        for rule in RULES {
            for banned in ["(?=", "(?!", "(?<="] {
                assert!(
                    !rule.pattern.contains(banned),
                    "rule {} uses lookaround, which the linear-time engine rejects",
                    rule.id
                );
            }
        }
    }

    #[test]
    fn base62_crc32_round_trip() {
        let payload = b"3xQvJ7mKpR2WtYzUh8nBcD";
        let mut token = String::from_utf8(payload.to_vec()).unwrap();
        token.push_str(&encode_base62_crc32(Checksum::GithubBase62Crc32, payload));
        assert_eq!(token.len(), 22 + 6);
        assert!(checksum_valid(Checksum::GithubBase62Crc32, &token));

        // Corrupting one checksum character invalidates it.
        let mut tampered = token.clone();
        let last = tampered.pop().unwrap();
        tampered.push(if last == '0' { '1' } else { '0' });
        assert!(!checksum_valid(Checksum::GithubBase62Crc32, &tampered));

        // Corrupting one payload character invalidates it too.
        let mut tampered_body: Vec<u8> = payload.to_vec();
        tampered_body[0] = if tampered_body[0] == b'a' { b'b' } else { b'a' };
        let mut tampered_token = String::from_utf8(tampered_body).unwrap();
        tampered_token.push_str(&encode_base62_crc32(
            Checksum::GithubBase62Crc32,
            b"3xQvJ7mKpR2WtYzUh8nBcD",
        ));
        assert!(!checksum_valid(
            Checksum::GithubBase62Crc32,
            &tampered_token
        ));
    }

    #[test]
    fn npm_checksum_is_eight_characters() {
        let payload = b"4hTnLw9ZxVcMrJqKsYbGdPeAfU";
        assert_eq!(payload.len(), 26);
        let mut token = String::from_utf8(payload.to_vec()).unwrap();
        token.push_str(&encode_base62_crc32(Checksum::NpmBase62Crc32, payload));
        assert_eq!(token.len(), 26 + 8);
        assert!(checksum_valid(Checksum::NpmBase62Crc32, &token));
    }

    #[test]
    fn checksum_rejects_non_base62_trailer() {
        let body = "abcdefghij!!!!!!";
        assert!(!checksum_valid(Checksum::GithubBase62Crc32, body));
    }
}
