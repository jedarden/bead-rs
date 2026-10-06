//! Bounded normalized matching views with raw UTF-8 byte provenance.
use std::collections::BTreeSet;
use std::ops::Range;

/// Value-free bounds reached while constructing section 3.2's decoded view.
pub struct DerivedViews {
    pub views: Vec<View>,
    pub reason_codes: BTreeSet<&'static str>,
}

pub struct View {
    pub text: String,
    pub map: Vec<Range<usize>>,
    pub decoded: bool,
}

impl View {
    pub fn raw_range(&self, start: usize, end: usize) -> Range<usize> {
        if self.map.is_empty() {
            return start..end;
        }
        self.map[start].start..self.map[end - 1].end
    }
    fn push(&mut self, text: &str, range: Range<usize>) {
        self.text.push_str(text);
        self.map.extend(std::iter::repeat_n(range, text.len()));
    }
}

fn raw(text: &str) -> View {
    View {
        text: text.to_string(),
        map: Vec::new(),
        decoded: false,
    }
}

impl Drop for View {
    fn drop(&mut self) {
        let mut bytes = std::mem::take(&mut self.text).into_bytes();
        bytes.fill(0);
    }
}

fn transform(source: &View, replacements: impl Fn(&str, usize) -> Option<(usize, String)>) -> View {
    let mut result = View {
        text: String::new(),
        map: Vec::new(),
        decoded: false,
    };
    let mut index = 0;
    while index < source.text.len() {
        if let Some((end, replacement)) = replacements(&source.text, index) {
            result.push(&replacement, source.raw_range(index, end));
            index = end;
        } else {
            let character = source.text[index..].chars().next().unwrap();
            let end = index + character.len_utf8();
            result.text.push(character);
            if source.map.is_empty() {
                result
                    .map
                    .extend((index..end).map(|index| index..index + 1));
            } else {
                result.map.extend_from_slice(&source.map[index..end]);
            }
            index = end;
        }
    }
    result
}

fn normalize(text: &str) -> View {
    let stripped = if !text.as_bytes().contains(&0x1b) {
        raw(text)
    } else {
        transform(&raw(text), |text, index| {
            let bytes = text.as_bytes();
            if bytes[index] != 0x1b {
                return None;
            }
            match bytes.get(index + 1) {
                Some(b'[') => {
                    let mut end = index + 2;
                    while end < bytes.len() {
                        let byte = bytes[end];
                        end += 1;
                        if (0x40..=0x7e).contains(&byte) {
                            return Some((end, String::new()));
                        }
                    }
                    None
                }
                Some(b']') => {
                    let mut end = index + 2;
                    while end < bytes.len() {
                        if bytes[end] == 7 {
                            return Some((end + 1, String::new()));
                        }
                        if bytes[end] == 0x1b && bytes.get(end + 1) == Some(&b'\\') {
                            return Some((end + 2, String::new()));
                        }
                        end += 1;
                    }
                    None
                }
                _ => None,
            }
        })
    };
    let escaped = if !stripped.text.contains('\\') {
        stripped
    } else {
        transform(&stripped, |text, index| {
            if text.as_bytes()[index] != b'\\' {
                return None;
            }
            let byte = *text.as_bytes().get(index + 1)?;
            let simple = match byte {
                b'n' => Some('\n'),
                b'r' => Some('\r'),
                b't' => Some('\t'),
                b'"' => Some('"'),
                b'\'' => Some('\''),
                b'\\' => Some('\\'),
                b'/' => Some('/'),
                _ => None,
            };
            if let Some(character) = simple {
                return Some((index + 2, character.to_string()));
            }
            if byte != b'u' {
                return None;
            }
            let first = u16::from_str_radix(text.get(index + 2..index + 6)?, 16).ok()?;
            let (code, end) = if (0xd800..=0xdbff).contains(&first) {
                if text.get(index + 6..index + 8)? != "\\u" {
                    return None;
                }
                let second = u16::from_str_radix(text.get(index + 8..index + 12)?, 16).ok()?;
                if !(0xdc00..=0xdfff).contains(&second) {
                    return None;
                }
                (
                    0x10000 + ((first as u32 - 0xd800) << 10) + second as u32 - 0xdc00,
                    index + 12,
                )
            } else {
                (first as u32, index + 6)
            };
            Some((end, char::from_u32(code)?.to_string()))
        })
    };
    if !escaped.text.contains('%') {
        return escaped;
    }
    transform(&escaped, |text, index| {
        if text.as_bytes()[index] != b'%' {
            return None;
        }
        let byte = u8::from_str_radix(text.get(index + 1..index + 3)?, 16).ok()?;
        if (0x20..=0x7e).contains(&byte) {
            Some((index + 3, (byte as char).to_string()))
        } else {
            None
        }
    })
}

fn token(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || b"_+/=-".contains(&byte)
}

fn dewrap(source: &View) -> View {
    if !source.text.contains(['\r', '\n']) {
        return View {
            text: source.text.clone(),
            map: source.map.clone(),
            decoded: false,
        };
    }
    let bytes = source.text.as_bytes();
    let mut result = View {
        text: String::new(),
        map: Vec::new(),
        decoded: false,
    };
    let mut index = 0;
    while index < bytes.len() {
        if matches!(bytes[index], b'\r' | b'\n') {
            let mut next = index + 1;
            if bytes[index] == b'\r' && bytes.get(next) == Some(&b'\n') {
                next += 1;
            }
            while bytes
                .get(next)
                .is_some_and(|byte| matches!(byte, b' ' | b'\t'))
            {
                next += 1;
            }
            let mut left = result.text.len();
            let slash = left > 0 && result.text.as_bytes()[left - 1] == b'\\';
            if slash {
                left -= 1;
            }
            if left > 0
                && token(result.text.as_bytes()[left - 1])
                && bytes.get(next).is_some_and(|byte| token(*byte))
            {
                if slash {
                    result.text.pop();
                    result.map.pop();
                }
                index = next;
                continue;
            }
        }
        let character = source.text[index..].chars().next().unwrap();
        let end = index + character.len_utf8();
        result.text.push(character);
        if source.map.is_empty() {
            result
                .map
                .extend((index..end).map(|index| index..index + 1));
        } else {
            result.map.extend_from_slice(&source.map[index..end]);
        }
        index = end;
    }
    result
}

/// RFC 4648 alphabets, with optional trailing padding and URL-safe spelling.
pub fn decode_base64(text: &str) -> Option<Vec<u8>> {
    decode_base64_with_unused_bits(text, true)
}

fn decode_base64_with_unused_bits(text: &str, strict: bool) -> Option<Vec<u8>> {
    let unpadded = text.trim_end_matches('=');
    if text.len() - unpadded.len() > 2 || unpadded.len() % 4 == 1 {
        return None;
    }
    if unpadded.len() != text.len()
        && (text.len() % 4 != 0 || text.len() - unpadded.len() != (4 - unpadded.len() % 4) % 4)
    {
        return None;
    }
    let mut accumulator = 0u32;
    let mut bits = 0u32;
    let mut result = Vec::with_capacity(unpadded.len() * 3 / 4);
    for byte in unpadded.bytes() {
        let digit = match byte {
            b'A'..=b'Z' => byte - b'A',
            b'a'..=b'z' => byte - b'a' + 26,
            b'0'..=b'9' => byte - b'0' + 52,
            b'+' | b'-' => 62,
            b'/' | b'_' => 63,
            _ => return None,
        };
        accumulator = (accumulator << 6) | digit as u32;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            result.push((accumulator >> bits) as u8);
            accumulator &= (1 << bits) - 1;
        }
    }
    if strict && accumulator != 0 {
        return None;
    }
    Some(result)
}

pub fn derived(text: &str) -> DerivedViews {
    let normalized = normalize(text);
    let dewrapped = dewrap(&normalized);
    let mut decoded = Vec::new();
    let bytes = normalized.text.as_bytes();
    let mut index = 0;
    let mut candidates = 0;
    let mut reason_codes = BTreeSet::new();
    while index < bytes.len() {
        if !token(bytes[index]) {
            index += 1;
            continue;
        }
        let start = index;
        while index < bytes.len() && token(bytes[index]) {
            index += 1;
        }
        if index - start < 40 {
            continue;
        }
        // The maximal run consumes a slot even when it cannot be decoded.
        // Continue enumerating after slot 64 so a 65th run is observable.
        if candidates == 64 {
            reason_codes.insert("run_count_limit");
            continue;
        }
        candidates += 1;
        if index - start > 65_536 {
            reason_codes.insert("run_size_limit");
            continue;
        }
        // Unlike the strict JWT header grammar, encoded field views permit
        // nonzero unused bits and a mixture of the two base64 alphabets.
        if let Some(mut decoded_bytes) =
            decode_base64_with_unused_bits(&normalized.text[start..index], false)
        {
            let printable = decoded_bytes
                .iter()
                .filter(|byte| matches!(**byte, 0x20..=0x7e | 0x09..=0x0d))
                .count();
            if decoded_bytes.is_empty() || printable * 10 < decoded_bytes.len() * 9 {
                decoded_bytes.fill(0);
                continue;
            }
            // The contract's printable threshold permits a small non-UTF-8
            // remainder. Lossy conversion retains every ASCII candidate;
            // the entire encoded run remains its raw provenance range.
            let text = String::from_utf8_lossy(&decoded_bytes).into_owned();
            decoded_bytes.fill(0);
            let map = vec![normalized.raw_range(start, index); text.len()];
            decoded.push(View {
                text,
                map,
                decoded: true,
            });
        }
    }
    let mut views = Vec::new();
    if normalized.text != text {
        views.push(normalized);
    }
    if dewrapped.text != text && !views.iter().any(|view| view.text == dewrapped.text) {
        views.push(dewrapped);
    }
    views.extend(decoded);
    DerivedViews {
        views,
        reason_codes,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn encoded_views(text: &str) -> Vec<View> {
        derived(text)
            .views
            .into_iter()
            .filter(|view| view.decoded)
            .collect()
    }

    #[test]
    fn decoded_run_length_boundaries_and_whole_run_provenance() {
        let run = "QUFB".repeat(10);
        assert!(encoded_views(&run[..39]).is_empty());
        let input = format!("!{run}!");
        let views = encoded_views(&input);
        assert_eq!(views.len(), 1);
        assert_eq!(views[0].text.len(), 30);
        assert_eq!(views[0].raw_range(0, 1), 1..41);

        let at_limit = "QUFB".repeat(16_384);
        assert_eq!(encoded_views(&at_limit).len(), 1);
        assert!(derived(&at_limit).reason_codes.is_empty());
        let over_limit = format!("{at_limit}A");
        let result = derived(&over_limit);
        assert!(result.views.iter().all(|view| !view.decoded));
        assert_eq!(result.reason_codes, BTreeSet::from(["run_size_limit"]));
    }

    #[test]
    fn invalid_nonprintable_and_oversized_runs_each_consume_a_slot() {
        let good = "QUFB".repeat(10);
        for invalid in ["=".repeat(40), "A".repeat(40), "A".repeat(65_537)] {
            let first_64 = format!("{}{good}", format!("{invalid}!").repeat(63));
            assert_eq!(encoded_views(&first_64).len(), 1);
            assert!(!derived(&first_64).reason_codes.contains("run_count_limit"));
            let first_65 = format!("{invalid}!{first_64}");
            let result = derived(&first_65);
            assert!(result.views.iter().all(|view| !view.decoded));
            assert!(result.reason_codes.contains("run_count_limit"));
            assert_eq!(
                result.reason_codes.contains("run_size_limit"),
                invalid.len() > 65_536
            );
        }
    }

    #[test]
    fn decoded_views_are_lenient_but_strict_decoder_keeps_unused_bit_check() {
        let canonical = format!("{}QQ==", "QUFB".repeat(10));
        let noncanonical = format!("{}QR==", "QUFB".repeat(10));
        assert!(decode_base64(&noncanonical).is_none());
        let canonical_views = encoded_views(&canonical);
        let lenient_views = encoded_views(&noncanonical);
        assert_eq!(lenient_views.len(), 1);
        assert!(lenient_views[0].text == canonical_views[0].text);
        assert_eq!(encoded_views(noncanonical.trim_end_matches('=')).len(), 1);
        let mixed_alphabets = format!("Pj4+Pz8_{}", "QUFB".repeat(9));
        assert_eq!(encoded_views(&mixed_alphabets).len(), 1);
        for invalid in [format!("{}=", "QUFB".repeat(10)), format!("{canonical}=")] {
            let result = derived(&invalid);
            assert!(result.views.iter().all(|view| !view.decoded));
            assert!(result.reason_codes.is_empty());
        }
    }

    #[test]
    fn decoded_threshold_retains_ascii_candidates_with_non_utf8_remainder() {
        // Thirty printable bytes plus one invalid UTF-8 byte meet 90%.
        let input = format!("{}/w==", "QUFB".repeat(10));
        let views = encoded_views(&input);
        assert_eq!(views.len(), 1);
        assert!(views[0].text.starts_with(&"A".repeat(30)));
        assert_eq!(views[0].raw_range(0, 30), 0..input.len());
    }

    #[test]
    fn printable_threshold_includes_vertical_tab_and_is_inclusive_at_ninety_percent() {
        assert_eq!(encoded_views(&"CwsL".repeat(10)).len(), 1);
        let exactly_ninety = format!("{}AAAA", "QUFB".repeat(9));
        assert_eq!(encoded_views(&exactly_ninety).len(), 1);
        let below_ninety = format!("{}QUEAAAAA", "QUFB".repeat(8));
        assert!(encoded_views(&below_ninety).is_empty());
    }
}
