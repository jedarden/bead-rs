//! Integer-only credential predicates from secret-ruleset-v4 section 2.
use regex::Regex;
use std::sync::LazyLock;

pub fn placeholder(value: &str) -> bool {
    let bytes = value.as_bytes();
    if bytes.is_empty() || bytes.iter().all(|byte| *byte == bytes[0]) {
        return true;
    }
    if value.bytes().any(|byte| b"<>${}*[]()|`".contains(&byte))
        || value.contains("...")
        || value.contains('\u{2026}')
    {
        return true;
    }
    let lower = value.to_ascii_lowercase();
    [
        "example",
        "placeholder",
        "replace",
        "your_",
        "_here",
        "dummy",
        "sample",
        "xxxx",
        "0000000",
        "redact",
        "changeme",
        "todo",
        "fake",
        "masked",
    ]
    .iter()
    .any(|marker| lower.contains(marker))
}

pub fn qualifies(value: &str, minimum: usize) -> bool {
    if value.is_empty()
        || value.len() > 512
        || !value.bytes().all(|byte| (0x21..=0x7e).contains(&byte))
        || placeholder(value)
    {
        return false;
    }
    let alphanumeric = value.bytes().filter(u8::is_ascii_alphanumeric).count();
    let digits = value.bytes().filter(u8::is_ascii_digit).count();
    if digits == 0 || digits == alphanumeric {
        return false;
    }
    let hex = value.len() >= 32
        && value.bytes().all(|byte| byte.is_ascii_hexdigit())
        && !(value.bytes().any(|byte| (b'a'..=b'f').contains(&byte))
            && value.bytes().any(|byte| (b'A'..=b'F').contains(&byte)));
    if !hex && 5 * digits >= 4 * alphanumeric {
        return false;
    }
    static WORDS: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"[a-z]{4,}|[A-Z]{4,}|[A-Z][a-z]{3,}").unwrap());
    let words: usize = WORDS.find_iter(value).map(|matched| matched.len()).sum();
    if 10 * words >= 7 * alphanumeric || alphanumeric.saturating_sub(words) < minimum {
        return false;
    }
    let class = |byte: u8| {
        if byte.is_ascii_lowercase() {
            0
        } else if byte.is_ascii_uppercase() {
            1
        } else if byte.is_ascii_digit() {
            2
        } else {
            3
        }
    };
    let transitions = value
        .as_bytes()
        .windows(2)
        .filter(|pair| class(pair[0]) != class(pair[1]))
        .count();
    3 * transitions >= value.len() - 1
}

pub fn credential_label(label: &str) -> bool {
    if label.is_empty()
        || label.len() > 64
        || !label
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"_.- ".contains(&byte))
        || label.contains("  ")
        || label.starts_with(' ')
        || label.ends_with(' ')
    {
        return false;
    }
    // Exclusions are literal case-folded tests on the complete identifier,
    // before separator/camel component splitting (ruleset-v4 section 4.4).
    let folded = label.to_ascii_lowercase();
    if folded.starts_with("secret_scan")
        || folded.starts_with("secret-scan")
        || [
            "acknowledge-secret",
            "fencing-token",
            "fencing_token",
            "max_tokens",
        ]
        .contains(&folded.as_str())
        || [
            "file", "path", "name", "ref", "label", "count", "limit", "budget", "usage",
        ]
        .iter()
        .any(|suffix| {
            ['_', '-']
                .iter()
                .any(|separator| folded.ends_with(&format!("{separator}{suffix}")))
        })
    {
        return false;
    }
    let mut normalized = String::new();
    let mut previous_lower = false;
    for character in label.chars() {
        if character.is_ascii_uppercase() && previous_lower {
            normalized.push('_');
        }
        normalized.push(if matches!(character, '-' | '.' | ' ') {
            '_'
        } else {
            character.to_ascii_lowercase()
        });
        previous_lower = character.is_ascii_lowercase();
    }
    let components: Vec<_> = normalized
        .split('_')
        .filter(|part| !part.is_empty())
        .collect();
    for (index, component) in components.iter().enumerate() {
        let keyword = [
            "password",
            "passwd",
            "passphrase",
            "pwd",
            "secret",
            "token",
            "credential",
            "pat",
        ]
        .contains(component)
            || (*component == "key"
                && index > 0
                && [
                    "api",
                    "access",
                    "secret",
                    "private",
                    "signing",
                    "encryption",
                    "master",
                    "application",
                    "account",
                    "auth",
                    "app",
                ]
                .contains(&components[index - 1]));
        if keyword
            && components[index + 1..].iter().all(|part| {
                [
                    "key", "token", "secret", "id", "value", "string", "data", "b64", "base64",
                    "plain", "text",
                ]
                .contains(part)
                    || part.bytes().all(|byte| byte.is_ascii_digit())
                    || part.strip_prefix('v').is_some_and(|rest| {
                        !rest.is_empty() && rest.bytes().all(|byte| byte.is_ascii_digit())
                    })
            })
        {
            return true;
        }
    }
    false
}

pub fn hash_shaped(value: &str) -> bool {
    ([32, 40, 56, 64, 96, 128].contains(&value.len())
        && value.bytes().all(|byte| byte.is_ascii_hexdigit()))
        || value.strip_prefix("gen-").is_some_and(|rest| {
            rest.len() == 32 && rest.bytes().all(|byte| byte.is_ascii_hexdigit())
        })
        || (value.len() == 36
            && value.bytes().enumerate().all(|(index, byte)| {
                if [8, 13, 18, 23].contains(&index) {
                    byte == b'-'
                } else {
                    byte.is_ascii_hexdigit()
                }
            }))
        || value.rsplit_once('-').is_some_and(|(prefix, suffix)| {
            (1..=32).contains(&prefix.len())
                && prefix.as_bytes()[0].is_ascii_lowercase()
                && prefix
                    .bytes()
                    .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())
                && (8..=64).contains(&suffix.len())
                && suffix
                    .bytes()
                    .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn qualifier_matches_each_normative_representative_at_all_thresholds() {
        let pairs = "3a".repeat(4);
        let hex31 = format!("{}{}", "a1".repeat(6), "1".repeat(19));
        let hex32 = format!("{hex31}1");
        let mixed32 = format!("A{}", &hex32[1..]);
        let word_cursor = format!("abcde{}+b", "A3".repeat(7));
        let cases = [
            (
                "bead_identifier",
                format!("ticket-{pairs}"),
                [true, false, false, false],
            ),
            (
                "name_short_hash",
                format!("alpha-{pairs}"),
                [true, false, false, false],
            ),
            (
                "timestamp",
                ["2026-10-05", "T12:34:56Z"].concat(),
                [false; 4],
            ),
            (
                "version",
                ["v12.34.56", "-rc7.89"].concat(),
                [true, true, false, false],
            ),
            ("path", ["/alpha", "/beta/gamma"].concat(), [false; 4]),
            ("camel_type", ["Alpha", "BetaGamma"].concat(), [false; 4]),
            ("snake_name", ["alpha", "_beta_gamma"].concat(), [false; 4]),
            ("base62_40", "a3".repeat(20), [true; 4]),
            ("hex_40", "a3".repeat(20), [true; 4]),
            ("base64_40", "aB3+/".repeat(8), [true; 4]),
            ("hex_31_one_case", hex31, [false; 4]),
            ("hex_32_one_case", hex32, [true; 4]),
            ("hex_32_mixed_case", mixed32, [false; 4]),
            (
                "maximal_word_cursor",
                word_cursor,
                [true, true, false, false],
            ),
        ];
        for (name, value, expected) in cases {
            assert_eq!(
                [8, 12, 16, 20].map(|minimum| qualifies(&value, minimum)),
                expected,
                "case {name}"
            );
        }
    }

    #[test]
    fn exclusions_are_literal_and_keywords_are_whole_components() {
        for identifier in [
            "acknowledge-secret",
            "fencing-token",
            "fencing_token",
            "max_tokens",
            "secret_scan",
            "secret-scan-extra",
            "tokens",
            "passwords",
            "apikey",
            "apiKeyName",
        ] {
            assert!(
                !credential_label(identifier),
                "excluded/non-keyword identifier admitted"
            );
            assert!(
                !credential_label(&identifier.to_ascii_uppercase()),
                "case-folded exclusion admitted"
            );
        }
        for identifier in [
            "fencing.token",
            "acknowledge_secret",
            "secret_value",
            "apiKey",
            "API Key",
            "prefix_access_key",
            "1_token",
            "personal_pat",
            "token_v2",
            "token_123",
        ] {
            assert!(
                credential_label(identifier),
                "literal near miss/keyword grammar rejected"
            );
        }
        for suffix in [
            "file", "path", "name", "ref", "label", "count", "limit", "budget", "usage",
        ] {
            for separator in ['_', '-'] {
                assert!(!credential_label(&format!("token{separator}{suffix}")));
            }
        }
        assert!(!credential_label(&format!("{}_token", "a".repeat(59))));
        assert!(credential_label(&format!("{}_token", "a".repeat(58))));
    }

    #[test]
    fn advisory_hash_shapes_match_complete_normative_grammar() {
        for width in [32, 40, 56, 64, 96, 128] {
            assert!(hash_shaped(&"a3".repeat(width / 2)));
        }
        for width in [8, 9, 63, 64] {
            assert!(hash_shaped(&format!("a1-{}", "a".repeat(width))));
        }
        assert!(hash_shaped(&format!(
            "{}-{}",
            "a".repeat(32),
            "3a".repeat(4)
        )));
        assert!(hash_shaped(&format!("gen-{}", "a3".repeat(16))));
        for value in [
            format!("Upper-{}", "a3".repeat(4)),
            format!("a_-{}", "a3".repeat(4)),
            format!("1a-{}", "a3".repeat(4)),
            format!("a-{}", "A3".repeat(4)),
            format!("a-{}", "a".repeat(7)),
            format!("a-{}", "a".repeat(65)),
            format!("{}-{}", "a".repeat(33), "a3".repeat(4)),
        ] {
            assert!(!hash_shaped(&value), "near-miss bead shape excluded");
        }
    }
}
