//! Context and structure detectors from secret-ruleset-v4 section 4.
use super::{credential_shape, fingerprint, Disposition, Field, Finding, Tier, RULESET_VERSION};
use regex::Regex;
use std::ops::Range;
use std::sync::LazyLock;

fn finding(
    selector: &str,
    field: &Field<'_>,
    rule: &str,
    start: usize,
    end: usize,
    tier: Tier,
) -> Finding {
    Finding {
        ruleset_version: RULESET_VERSION,
        rule_id: rule.to_string(),
        provider: "context".to_string(),
        tier,
        disposition: Disposition::Confirmed,
        selector: selector.to_string(),
        field_path: field.path.to_string(),
        start,
        end,
        fingerprint: fingerprint::compute(
            RULESET_VERSION,
            rule,
            selector,
            field.path,
            start,
            end,
            &field.text.as_bytes()[start..end],
        ),
    }
}

pub(super) fn scan(selector: &str, field: &Field<'_>, decoded: bool) -> Vec<Finding> {
    scan_with_source(selector, field, decoded, None)
}

pub(super) fn scan_view(
    selector: &str,
    field: &Field<'_>,
    decoded: bool,
    source_map: &[Range<usize>],
) -> Vec<Finding> {
    scan_with_source(selector, field, decoded, Some(source_map))
}

fn scan_with_source(
    selector: &str,
    field: &Field<'_>,
    decoded: bool,
    source_map: Option<&[Range<usize>]>,
) -> Vec<Finding> {
    if decoded {
        return Vec::new();
    }
    let mut findings = Vec::new();
    static ASSIGNMENTS: LazyLock<Vec<Regex>> = LazyLock::new(|| {
        [
        r#"(?m)(?P<name>(?:--)?[A-Za-z][A-Za-z0-9_. -]{0,63})["')]*[ \t]*(?:=>|:=|=|:)[ \t]*["']?(?P<value>[^\s"'`,;]+)"#,
        r#"(?m)(?P<name>--[A-Za-z][A-Za-z0-9_.-]{0,63})[ \t]+["']?(?P<value>[^\s"'`,;]+)"#,
        r#"(?m)^[ \t]*[-*|]?[ \t]*(?P<name>[A-Za-z][A-Za-z0-9_. -]{0,63}?)(?:\t| {2,}|[ \t]*\|[ \t]*)(?P<value>[^\s"'`,;|]+)[ \t]*\|?[ \t]*$"#,
    ].iter().map(|pattern|Regex::new(pattern).unwrap()).collect()
    });
    for (form, regex) in ASSIGNMENTS.iter().enumerate() {
        for capture in regex.captures_iter(field.text) {
            let name = capture.name("name").unwrap();
            if name.start() > 0 && field.text.as_bytes()[name.start() - 1].is_ascii_alphanumeric() {
                continue;
            }
            if !credential_shape::credential_label(name.as_str()) {
                continue;
            }
            let value = capture.name("value").unwrap();
            let text = value.as_str().trim_end_matches(['.', ')', ']', '}']);
            if text.is_empty() || credential_shape::placeholder(text) {
                continue;
            }
            let (rule, tier) = if credential_shape::qualifies(text, if form == 2 { 20 } else { 12 })
            {
                ("credential-assignment", Tier::Blocking)
            } else if text.len() >= 8 {
                ("advisory-keyword-assignment", Tier::Advisory)
            } else {
                continue;
            };
            findings.push(finding(
                selector,
                field,
                rule,
                value.start(),
                value.start() + text.len(),
                tier,
            ));
        }
    }
    for (start, end) in uri_userinfo_ranges(field.text, source_map) {
        let password = &field.text[start..end];
        let password = if source_map.is_some() {
            password.to_string()
        } else {
            match percent_decode(password) {
                Some(password) => password,
                None => continue,
            }
        };
        if credential_shape::qualifies(&password, 8) {
            findings.push(finding(
                selector,
                field,
                "uri-userinfo-credential",
                start,
                end,
                Tier::Blocking,
            ));
        }
    }
    static STRUCTURES: LazyLock<Vec<(&str, Regex, usize)>> = LazyLock::new(|| {
        [
        ("authorization-header-credential",r#"(?i)(?:authorization["']?[ \t]*[:=][ \t]*["']?[ \t]*(?:bearer|basic|token|apikey)|(?:^|[^A-Za-z0-9])bearer)[ \t]+(?P<value>[A-Za-z0-9_+/=.-]{20,})"#,12),
        ("curl-user-credential",r#"(?:^|[ \t])(?:-u(?:=|[ \t]*)|--user(?:=|[ \t]+))["']?[^:\s"']+:(?P<value>[^\s"'`,;]+)"#,8),
    ].iter().map(|(rule,pattern,min)|(*rule,Regex::new(pattern).unwrap(),*min)).collect()
    });
    for (rule, regex, minimum) in STRUCTURES.iter() {
        for capture in regex.captures_iter(field.text) {
            let value = capture.name("value").unwrap();
            if credential_shape::qualifies(value.as_str(), *minimum) {
                findings.push(finding(
                    selector,
                    field,
                    rule,
                    value.start(),
                    value.end(),
                    Tier::Blocking,
                ));
            }
        }
    }
    static KIND: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r#"(?m)^[ \t]*kind:[ \t]*(?:Secret|\"Secret\"|'Secret')[ \t]*(?:#[^\r\n]*)?$"#)
            .unwrap()
    });
    static SECTION: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?m)^(?P<indent>[ \t]*)(?P<kind>data|stringData):[ \t]*(?:[|>][+-]?[ \t]*)?$")
            .unwrap()
    });
    static ENTRY: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r#"^[ \t]+[^:\s]+:[ \t]*["']?(?P<value>[^\s"']+)"#).unwrap());
    for document in yaml_document_ranges(field.text) {
        let document_text = &field.text[document.clone()];
        if !KIND.is_match(document_text) {
            continue;
        }
        for section in SECTION.captures_iter(document_text) {
            let offset = document.start + section.get(0).unwrap().end();
            let indent = section.name("indent").unwrap().len();
            let encoded = section.name("kind").unwrap().as_str() == "data";
            let mut position = offset;
            for line in field.text[offset..document.end].split_inclusive('\n') {
                let trimmed = line.trim();
                if !trimmed.is_empty() {
                    let line_indent = line.len() - line.trim_start_matches([' ', '\t']).len();
                    if line_indent <= indent {
                        break;
                    }
                    if let Some(capture) = ENTRY.captures(line) {
                        let value = capture.name("value").unwrap();
                        let acceptable = if encoded {
                            value.len() >= 16
                                && value.as_str().bytes().all(|byte| {
                                    byte.is_ascii_alphanumeric() || b"+/=_-".contains(&byte)
                                })
                        } else {
                            credential_shape::qualifies(value.as_str(), 12)
                        };
                        if acceptable {
                            findings.push(finding(
                                selector,
                                field,
                                "kubernetes-secret-data",
                                position + value.start(),
                                position + value.end(),
                                Tier::Blocking,
                            ));
                        }
                    }
                }
                position += line.len();
            }
        }
    }
    if serde_json::from_str::<serde_json::Value>(field.text)
        .ok()
        .is_some_and(|value| {
            value.get("kind").and_then(serde_json::Value::as_str) == Some("Secret")
        })
    {
        static JSON_SECTIONS: LazyLock<Regex> = LazyLock::new(|| {
            Regex::new(r#""(?P<kind>data|stringData)"\s*:\s*\{(?P<body>[^{}]*)\}"#).unwrap()
        });
        static JSON_ENTRY: LazyLock<Regex> =
            LazyLock::new(|| Regex::new(r#""[^"\\]+"\s*:\s*"(?P<value>[^"\\]+)""#).unwrap());
        for section in JSON_SECTIONS.captures_iter(field.text) {
            let body = section.name("body").unwrap();
            let encoded = section.name("kind").unwrap().as_str() == "data";
            for entry in JSON_ENTRY.captures_iter(body.as_str()) {
                let value = entry.name("value").unwrap();
                let acceptable = if encoded {
                    value.len() >= 16 && super::text_views::decode_base64(value.as_str()).is_some()
                } else {
                    credential_shape::qualifies(value.as_str(), 12)
                };
                if acceptable {
                    findings.push(finding(
                        selector,
                        field,
                        "kubernetes-secret-data",
                        body.start() + value.start(),
                        body.start() + value.end(),
                        Tier::Blocking,
                    ));
                }
            }
        }
    }
    findings
}

fn yaml_document_ranges(text: &str) -> Vec<Range<usize>> {
    let mut ranges = Vec::new();
    let mut document_start = 0;
    let mut line_start = 0;
    for line in text.split_inclusive('\n') {
        let content = line.trim_end_matches(['\n', '\r']);
        if line_start > document_start && is_yaml_document_start(content) {
            ranges.push(document_start..line_start);
            document_start = line_start;
        }
        line_start += line.len();
    }
    if document_start < text.len() {
        ranges.push(document_start..text.len());
    }
    ranges
}

fn is_yaml_document_start(line: &str) -> bool {
    let Some(remainder) = line.strip_prefix("---") else {
        return false;
    };
    remainder.is_empty() || remainder.chars().next().is_some_and(char::is_whitespace)
}

static URI_PREFIX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"[A-Za-z][A-Za-z0-9+.-]*://[^/\s:@]+:"#).unwrap());

fn uri_userinfo_ranges(text: &str, source_map: Option<&[Range<usize>]>) -> Vec<(usize, usize)> {
    let mut ranges = Vec::new();
    for prefix in URI_PREFIX.find_iter(text) {
        let password_start = prefix.end();
        let mut password_end = password_start;
        let mut delimiter = None;
        while password_end < text.len() {
            let character = text[password_end..].chars().next().unwrap();
            if character.is_whitespace() {
                break;
            }
            let is_source_byte =
                source_map.is_none_or(|map| map[password_end].end - map[password_end].start == 1);
            if character == '@' && is_source_byte {
                delimiter = Some(password_end);
                break;
            }
            if character == '/' && is_source_byte {
                break;
            }
            password_end += character.len_utf8();
        }
        let Some(delimiter) = delimiter else {
            continue;
        };
        let host_start = delimiter + 1;
        if host_start >= text.len() {
            continue;
        }
        let mut host_end = host_start;
        while host_end < text.len() {
            let character = text[host_end..].chars().next().unwrap();
            if character.is_whitespace() || character == '/' {
                break;
            }
            host_end += character.len_utf8();
        }
        if host_end > host_start && password_end > password_start {
            ranges.push((password_start, password_end));
        }
    }
    ranges
}

fn percent_decode(text: &str) -> Option<String> {
    let mut decoded = Vec::with_capacity(text.len());
    let bytes = text.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' && index + 2 < bytes.len() {
            if let (Some(high), Some(low)) =
                (hex_digit(bytes[index + 1]), hex_digit(bytes[index + 2]))
            {
                decoded.push(high << 4 | low);
                index += 3;
                continue;
            }
        }
        let character = text[index..].chars().next().unwrap();
        let mut encoded = [0; 4];
        decoded.extend_from_slice(character.encode_utf8(&mut encoded).as_bytes());
        index += character.len_utf8();
    }
    String::from_utf8(decoded).ok()
}

fn hex_digit(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}
