//! Context and structure detectors from secret-ruleset-v4 section 4.
use super::{credential_shape, fingerprint, Disposition, Field, Finding, Tier, RULESET_VERSION};
use regex::Regex;
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
    static STRUCTURES: LazyLock<Vec<(&str, Regex, usize)>> = LazyLock::new(|| {
        [
        ("uri-userinfo-credential",r#"[A-Za-z][A-Za-z0-9+.-]*://[^/\s:@]+:(?P<value>[^@\s]+)@[^/\s]+"#,8),
        ("authorization-header-credential",r#"(?i)(?:authorization["']?[ \t]*[:=][ \t]*["']?[ \t]*(?:bearer|basic|token|apikey)|bearer)[ \t]+(?P<value>[A-Za-z0-9_+/=.-]{20,})"#,12),
        ("curl-user-credential",r#"(?:^|[ \t])(?:-u|--user)(?:=|[ \t]+)["']?[^:\s"']+:(?P<value>[^\s"'`,;]+)"#,8),
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
    static KIND: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?m)^\s*kind:[ \t]*Secret[ \t]*$").unwrap());
    static SECTION: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?m)^(?P<indent>[ \t]*)(?P<kind>data|stringData):[ \t]*$").unwrap()
    });
    static ENTRY: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r#"^[ \t]+[^:\s]+:[ \t]*["']?(?P<value>[^\s"']+)"#).unwrap());
    if let Some(kind) = KIND.find(field.text) {
        for section in SECTION.captures_iter(&field.text[kind.end()..]) {
            let offset = kind.end() + section.get(0).unwrap().end();
            let indent = section.name("indent").unwrap().len();
            let encoded = section.name("kind").unwrap().as_str() == "data";
            let mut position = offset;
            for line in field.text[offset..].split_inclusive('\n') {
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
