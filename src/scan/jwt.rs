//! Complete, duplicate-aware JWT header validation (ruleset-v4 section 4.1).
//! Payloads and signatures are deliberately not validated offline.

use std::collections::HashSet;

#[derive(PartialEq, Eq, Hash)]
struct Key(Vec<u16>);

impl Drop for Key {
    fn drop(&mut self) {
        self.0.fill(0);
    }
}

enum Frame {
    Object {
        keys: HashSet<Key>,
        expecting_key: bool,
        alg_value: bool,
    },
    Array,
}

fn clear_alg_value(frame: Option<&mut Frame>) {
    if let Some(Frame::Object { alg_value, .. }) = frame {
        *alg_value = false;
    }
}

/// Input has already been validated as a complete JSON value by RawValue.
/// Preserve UTF-16 key identity, including escaped surrogate code units,
/// without normalizing Unicode or converting JSON numbers to machine floats.
fn key_identity(text: &str) -> Key {
    let mut units = Vec::new();
    let mut chars = text.chars();
    while let Some(character) = chars.next() {
        if character != '\\' {
            units.extend(character.encode_utf16(&mut [0; 2]).iter().copied());
            continue;
        }
        let escaped = chars.next().expect("validated JSON string escape");
        let unit = match escaped {
            '"' => b'"' as u16,
            '\\' => b'\\' as u16,
            '/' => b'/' as u16,
            'b' => 8,
            'f' => 12,
            'n' => 10,
            'r' => 13,
            't' => 9,
            'u' => (0..4).fold(0, |unit, _| {
                unit * 16
                    + chars
                        .next()
                        .and_then(|character| character.to_digit(16))
                        .expect("validated JSON Unicode escape") as u16
            }),
            _ => unreachable!("validated JSON string escape"),
        };
        units.push(unit);
    }
    Key(units)
}

fn object_valid(text: &str) -> bool {
    let Ok(raw) = serde_json::from_str::<&serde_json::value::RawValue>(text) else {
        return false;
    };
    let text = raw.get();
    if !text.starts_with('{') {
        return false;
    }
    let bytes = text.as_bytes();
    // Heap frames avoid recursive traversal of attacker-controlled depth.
    // RawValue supplies syntax validation; this pass checks member identity
    // and the root alg type without materializing any other value.
    let mut frames = Vec::new();
    let mut root_alg_valid = false;
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'{' => {
                clear_alg_value(frames.last_mut());
                frames.push(Frame::Object {
                    keys: HashSet::new(),
                    expecting_key: true,
                    alg_value: false,
                });
                index += 1;
            }
            b'[' => {
                clear_alg_value(frames.last_mut());
                frames.push(Frame::Array);
                index += 1;
            }
            b'}' | b']' => {
                frames.pop();
                index += 1;
            }
            b',' => {
                if let Some(Frame::Object { expecting_key, .. }) = frames.last_mut() {
                    *expecting_key = true;
                }
                index += 1;
            }
            b':' | b' ' | b'\t' | b'\r' | b'\n' => index += 1,
            b'"' => {
                let start = index + 1;
                index = start;
                while bytes[index] != b'"' {
                    if bytes[index] == b'\\' {
                        index += 1;
                    }
                    index += 1;
                }
                let at_root = frames.len() == 1;
                if let Some(Frame::Object {
                    keys,
                    expecting_key,
                    alg_value,
                }) = frames.last_mut()
                {
                    if *expecting_key {
                        let key = key_identity(&text[start..index]);
                        *alg_value = at_root && key.0 == [97, 108, 103];
                        if !keys.insert(key) {
                            return false;
                        }
                        *expecting_key = false;
                    } else {
                        if *alg_value {
                            root_alg_valid = index > start;
                        }
                        *alg_value = false;
                    }
                }
                index += 1;
            }
            _ => {
                clear_alg_value(frames.last_mut());
                while bytes.get(index).is_some_and(|byte| {
                    !matches!(byte, b',' | b'}' | b']' | b' ' | b'\t' | b'\r' | b'\n')
                }) {
                    index += 1;
                }
            }
        }
    }
    root_alg_valid
}

pub(super) fn header_valid(segment: &str) -> bool {
    if !segment
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
    {
        return false;
    }
    let Some(mut decoded) = super::text_views::decode_base64(segment) else {
        return false;
    };
    let valid = std::str::from_utf8(&decoded).is_ok_and(object_valid);
    decoded.fill(0);
    valid
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn complete_object_nonempty_string_alg_and_json_whitespace() {
        for text in [
            r#"{"alg":"a"}"#,
            " {\"alg\":\"a\"}\r\n\t ",
            r#"{"alg":"\u0000"}"#,
        ] {
            assert!(object_valid(text), "valid complete header rejected");
        }
        for text in [
            r#"{"alg":""}"#,
            r#"{"alg":null}"#,
            r#"{"alg":1}"#,
            r#"{"alg":{}}"#,
            r#"{"nested":{"alg":"a"}}"#,
            r#"["alg","a"]"#,
            r#"{"alg":"a"} false"#,
            r#"{"alg":"a",}"#,
            r#"{"alg":"a","x":NaN}"#,
        ] {
            assert!(!object_valid(text), "invalid complete header admitted");
        }
    }

    #[test]
    fn rejects_duplicate_members_at_every_depth_and_escape_equivalents() {
        for text in [
            r#"{"alg":"a","alg":"b"}"#,
            r#"{"alg":"a","\u0061lg":"b"}"#,
            r#"{"alg":"a","x":{"k":1,"k":2}}"#,
            r#"{"alg":"a","x":[{"k":1,"\u006b":2}]}"#,
            r#"{"alg":"a","x":{"😀":1,"\ud83d\ude00":2}}"#,
        ] {
            assert!(!object_valid(text), "duplicate member admitted");
        }
        assert!(object_valid(r#"{"alg":"a","x":[{"k":1},{"k":2}]}"#));
        assert!(object_valid(r#"{"alg":"a","x":{"\ud800":1}}"#));
    }

    #[test]
    fn valid_numbers_and_depth_are_not_machine_representation_constraints() {
        assert!(object_valid(
            r#"{"alg":"a","n":1e999,"m":1234567890123456789012345678901234567890}"#
        ));
        let text = format!(
            "{{\"alg\":\"a\",\"x\":{}0{}}}",
            "[".repeat(1000),
            "]".repeat(1000)
        );
        assert!(object_valid(&text), "deep valid JSON header rejected");
    }

    #[test]
    fn header_requires_strict_unpadded_url_alphabet_and_utf8() {
        // Runtime encoding avoids committing a complete format-valid token.
        fn encode(bytes: &[u8]) -> String {
            let alphabet = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
            let mut text = String::new();
            for chunk in bytes.chunks(3) {
                let word = chunk
                    .iter()
                    .fold(0u32, |word, byte| (word << 8) | *byte as u32)
                    << ((3 - chunk.len()) * 8);
                for shift in (0..chunk.len() + 1).map(|index| 18 - index * 6) {
                    text.push(alphabet[((word >> shift) & 63) as usize] as char);
                }
            }
            text
        }
        let header = encode(br#"{"alg":"a"}"#);
        assert!(header_valid(&header));
        assert!(!header_valid(&(header.clone() + "=")));
        let mut noncanonical = header.into_bytes();
        let last = noncanonical.last_mut().unwrap();
        *last += 1;
        assert!(!header_valid(std::str::from_utf8(&noncanonical).unwrap()));
        assert!(!header_valid(&encode(b"{\"alg\":\"a\",\"x\":\"\xff\"}")));
        assert!(!header_valid("eyJ+++++"));
        assert!(!header_valid("eyJ/////"));
        assert!(!header_valid("eyJaaaaaa"));
    }
}
