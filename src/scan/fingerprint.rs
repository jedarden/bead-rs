//! Stable finding fingerprints.
//!
//! The v1 fingerprint is SHA-256 over a domain separator, the ruleset version,
//! the rule ID, the record selector, the field path, the byte range, and the
//! matched bytes, rendered as lowercase hexadecimal. It identifies one exact
//! finding — which is what an exact-fingerprint acknowledgment needs — without
//! exposing the finding itself.
//!
//! Every component is length-prefixed so the encoding is injective: two
//! different findings cannot collide onto the same byte stream and therefore
//! cannot collide onto the same fingerprint.

use sha2::{Digest, Sha256};

/// Domain separator binding fingerprints to this contract's identity.
const DOMAIN: &str = "urn:bead-rs:spec:secret-rejection:v1";

/// Compute the lowercase-hex fingerprint of one finding.
///
/// `matched` is the sensitive portion; it is consumed by reference and never
/// retained here.
pub fn compute(
    ruleset_version: u32,
    rule_id: &str,
    record: &str,
    field: &str,
    start: usize,
    end: usize,
    matched: &[u8],
) -> String {
    let mut hasher = Sha256::new();
    feed(&mut hasher, DOMAIN.as_bytes());
    feed(&mut hasher, ruleset_version.to_string().as_bytes());
    feed(&mut hasher, rule_id.as_bytes());
    feed(&mut hasher, record.as_bytes());
    feed(&mut hasher, field.as_bytes());
    feed(&mut hasher, &start.to_be_bytes());
    feed(&mut hasher, &end.to_be_bytes());
    feed(&mut hasher, matched);
    let digest = hasher.finalize();
    let mut hex = String::with_capacity(digest.len() * 2);
    for byte in digest {
        hex.push(HEX[(byte >> 4) as usize] as char);
        hex.push(HEX[(byte & 0x0f) as usize] as char);
    }
    hex
}

const HEX: &[u8; 16] = b"0123456789abcdef";

/// Write `bytes` preceded by its u64 big-endian length.
fn feed(hasher: &mut Sha256, bytes: &[u8]) {
    hasher.update((bytes.len() as u64).to_be_bytes());
    hasher.update(bytes);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fp(
        rule: &str,
        record: &str,
        field: &str,
        start: usize,
        end: usize,
        matched: &str,
    ) -> String {
        compute(1, rule, record, field, start, end, matched.as_bytes())
    }

    #[test]
    fn fingerprints_are_lowercase_hex_sha256() {
        let value = fp("r", "issue", "title", 0, 3, "abc");
        assert_eq!(value.len(), 64);
        assert!(value.chars().all(|c| c.is_ascii_hexdigit()));
        assert!(value.chars().all(|c| !c.is_ascii_uppercase()));
    }

    #[test]
    fn fingerprints_are_deterministic() {
        assert_eq!(
            fp("r", "issue", "title", 0, 3, "abc"),
            fp("r", "issue", "title", 0, 3, "abc")
        );
    }

    #[test]
    fn every_component_changes_the_fingerprint() {
        let base = fp("r", "issue", "title", 0, 3, "abc");
        assert_ne!(
            base,
            fp("r2", "issue", "title", 0, 3, "abc"),
            "rule id not mixed in"
        );
        assert_ne!(
            base,
            fp("r", "issue:1", "title", 0, 3, "abc"),
            "record not mixed in"
        );
        assert_ne!(
            base,
            fp("r", "issue", "notes", 0, 3, "abc"),
            "field not mixed in"
        );
        assert_ne!(
            base,
            fp("r", "issue", "title", 1, 3, "abc"),
            "start not mixed in"
        );
        assert_ne!(
            base,
            fp("r", "issue", "title", 0, 4, "abc"),
            "end not mixed in"
        );
        assert_ne!(
            base,
            fp("r", "issue", "title", 0, 3, "abcd"),
            "bytes not mixed in"
        );
        assert_ne!(
            base,
            compute(2, "r", "issue", "title", 0, 3, b"abc"),
            "ruleset version not mixed in"
        );
    }

    #[test]
    fn length_prefixing_makes_the_encoding_injective() {
        // Without length prefixes, ("ab","c") and ("a","bc") would hash the
        // same byte stream. Separately they must differ.
        let left = fp("r", "ab", "c", 0, 0, "");
        let right = fp("r", "a", "bc", 0, 0, "");
        assert_ne!(left, right);
    }

    #[test]
    fn fingerprint_of_a_real_match_differs_from_a_neighbor() {
        let left = fp(
            "github-classic-token",
            "issue:x",
            "description",
            12,
            52,
            "ghp_aaaa",
        );
        let right = fp(
            "github-classic-token",
            "issue:x",
            "description",
            12,
            52,
            "ghp_bbbb",
        );
        assert_ne!(left, right);
    }
}
