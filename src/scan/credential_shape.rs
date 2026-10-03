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
    let mut normalized = String::new();
    let mut previous_lower = false;
    for character in label.trim_start_matches('-').chars() {
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
    if normalized.starts_with("secret_scan")
        || ["acknowledge_secret", "fencing_token", "max_tokens"].contains(&normalized.as_str())
        || [
            "file", "path", "name", "ref", "label", "count", "limit", "budget", "usage",
        ]
        .iter()
        .any(|suffix| normalized.ends_with(&format!("_{suffix}")))
    {
        return false;
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
            "apikey",
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
            !prefix.is_empty()
                && suffix.len() == 8
                && suffix.bytes().all(|byte| byte.is_ascii_hexdigit())
        })
}
