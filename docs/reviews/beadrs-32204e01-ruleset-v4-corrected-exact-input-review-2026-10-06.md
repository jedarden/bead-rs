# Secret ruleset v4 corrected exact-input review

Date: 2026-10-06.

Reviewer identity: `codex-reviewer-beadrs-32204e01` (OpenAI Codex, this
independent review attempt). The reviewer is distinct from
`codex-secret-release-operator`, the implementation owners, and the authors
of the specifications and ADRs. This is not a self-approval by an author or
release operator.

## Decision

**FULL-ARTIFACT ACCEPTED** for the complete corrected
`research/specs/secret-ruleset-v4.md` artifact at SHA-256
`0c79d76375b795a7e58daeda1241414e6ebcac038f62e32b927acefa598d7ebe`, with
the value-free witness results and scope limits recorded below.

This is a contract decision, not production conformance, implementation
acceptance, fleet replay, publication approval, or credential-remediation
authorization. The contract itself correctly preserves bounded/partial
encoded-view coverage as a limitation and leaves the section 7 fleet and
cost obligations outstanding.

## Exact inputs

Hashes are SHA-256 over complete file bytes. Any change to a listed input
requires a new exact-input review.

| Input | SHA-256 |
| --- | --- |
| `research/specs/secret-ruleset-v4.md` | `0c79d76375b795a7e58daeda1241414e6ebcac038f62e32b927acefa598d7ebe` |
| `research/fixtures/secret-ruleset-v4-contract.py` | `dd0aa6ee0467f65a1eb955b4785e292380f03f17f64b9af3b966814e80f730c0` |
| `docs/adr/023-widen-blocking-ruleset-to-observed-credential-formats.md` | `39d9174173df3aadc7c268bdd27d139e1091cc12559e20b9d098037683a7359a` |
| `docs/adr/024-match-on-a-normalized-view.md` | `508989f43c984b3a69ef529eaaf0d96ed1caa44873857e91b14aeaaa61de9c86` |
| `docs/adr/025-surface-advisory-findings-and-partial-scan-coverage.md` | `e13bda45279d2b875c9c2dce6d2bdc3e0471e91512cff22ec40487f1a0de4a33` |
| `research/specs/secret-rejection-v1.md` (accepted baseline) | `f6aa7639a8ef1dd509b431853abf64db78d0923ef9e9d59b09e0a4e0e55df231` |
| `research/specs/needle-cli-contract-v1.md` | `64a4057bdec893d81e42a3f23bff86f225a302fc3b5fec18d8ab5cf414ad7233` |
| `docs/reviews/beadrs-c0b467d4-ruleset-v4-randomness-excluded-identifiers-review-2026-10-05.md` | `65c376bf789d996fe2ea77a07eda0269c318662c24ef2e8d907cfc11f25d77d2` |
| `docs/reviews/beadrs-cf08b3e6-ruleset-v4-decoded-bounds-structured-review-2026-10-05.md` | `baec1dce9e17d7a491110b41e99124bd6b475a07c58656e5d7ad38b6854045da` |
| `docs/reviews/beadrs-8adcf750-ruleset-v4-stderr-integrated-decision-2026-10-05.md` | `585ed41dda38f5adb33d600e670641c55e1f7a027e253dafeb2074679cb23459` |

The review used only the listed specifications, ADRs, prior review records,
and the independently authored value-free fixture. No other bead
implementation's source, tests, fixtures, SQL, output, or prose was used as
contract evidence. No candidate credential bytes, locations, selectors, or
fingerprints are recorded here.

## Reference witnesses

The fixture was run as a Python standard-library reference check. It is not a
production scanner, production conformance suite, or fleet replay.

`python3 research/fixtures/secret-ruleset-v4-contract.py` exited 0 and ran
all eight tests:

1. qualifier metrics and truth table;
2. strict integer qualifier boundaries;
3. literal-separator and plural exclusion boundaries;
4. decoded selection, invalid runs, and oversized runs;
5. decoding padding and one-level behavior;
6. JWT header strictness;
7. table ranges, bars, punctuation, quotes, and line endings; and
8. notice shape and mutation lifecycle cases.

`python3 research/fixtures/secret-ruleset-v4-contract.py --table` exited 0.
Its value-free metrics table was:

| Case | `(n,a,d,w,t)` | first failing step at `m=8,12,16,20` (`0` = pass) |
| --- | --- | --- |
| `bead_identifier` | `(15,14,4,6,9)` | `(0,5,5,5)` |
| `name_short_hash` | `(14,13,4,5,9)` | `(0,5,5,5)` |
| `timestamp` | `(20,16,14,0,11)` | `(4,4,4,4)` |
| `version` | `(16,12,9,0,10)` | `(0,0,5,5)` |
| `path` | `(17,14,0,14,5)` | `(3,3,3,3)` |
| `camel_type` | `(14,14,0,14,5)` | `(3,3,3,3)` |
| `snake_name` | `(16,14,0,14,4)` | `(3,3,3,3)` |
| `base62_40` | `(40,40,20,0,39)` | `(0,0,0,0)` |
| `hex_40` | `(40,40,20,0,39)` | `(0,0,0,0)` |
| `base64_40` | `(40,24,8,0,31)` | `(0,0,0,0)` |
| `hex_31_one_case` | `(31,31,25,0,11)` | `(4,4,4,4)` |
| `hex_32_one_case` | `(32,32,26,0,11)` | `(0,0,0,0)` |
| `hex_32_mixed_case` | `(32,32,26,0,11)` | `(4,4,4,4)` |
| `maximal_word_cursor` | `(21,20,7,5,16)` | `(0,0,5,5)` |

## Resolution of the prior actionable findings

### Q, threshold counterexamples, and exclusions

Section 2.2 now specifies the cursor algorithm: maximal runs, ordered
lowercase/uppercase/title-case alternatives, whole-run consumption, cursor
advance to the run end, and no reconsideration of consumed bytes. The fixture
records the integer metrics and first failing step for every named class at
all four thresholds, including the one-case and mixed-case 31/32-byte hex
boundaries and the maximal-versus-minimum word-run counterexample.

The corrected contract explicitly withdraws the rejected unconditional claims
that every short identifier/hash shape fails and that every version-shaped
value fails. The passing `m=8` identifier shapes and the passing `m=12`
version-shaped representative are treated as the intended threat-model
disposition: Q is only a shape qualifier, label or structural context is
mandatory, and any real false positive remains a section 7 release blocker.
This is preferable to silently broadening exclusions or changing Q to fit a
replay.

Exclusions are now literal, case-folded tests over the complete captured
identifier, with explicit exact/prefix/suffix scope and no separator
normalization. Plural keyword components, alternate separators, suffix
boundaries, and case are witnessed. An exclusion suppresses both the
assignment block and its Q-fail advisory fallback, while independent
provider, structural, private-key, and unlabelled advisory rules remain
active.

### Decoded limits and coverage

Section 3.2 closes the prior decoded-view choices: the union alphabet,
maximal left-to-right runs, a source-byte threshold of 40, the first 64
qualifying runs, counting invalid/oversized/nonprintable runs toward that
cap, exact terminal-padding grammar, one-level decoding, a source-run limit
strictly above 65,536 bytes, and the printable-byte ratio. The witness checks
the 39/40, 64/65, and 65,536/65,537 transitions, invalid and nonprintable
runs, padding, mixed alphabet handling, and non-recursive behavior.

The corrected `view_coverage` contract makes a decoded count or size limit
observable as a value-free `limited` entry with sorted unique reason codes.
Invalid/nonprintable candidates alone are not mislabeled as coverage limits;
raw, normalized, and dewrapped views remain full-field scans. The contract
does not claim complete encoded coverage, and limited decoding does not itself
reject or suppress other findings. This explicitly disposes of the prior
threat that an omitted encoded region could be mistaken for a clean scan.

### JWT strictness

The provider rule requires exactly three segments and consumes the maximal
dot-separated chain; a fourth segment or adjacent dot is not accepted as a
three-segment prefix. The header rules require unpadded canonical base64url,
reject length-modulo-four one and nonzero unused bits, require valid UTF-8 and
one complete JSON object, reject duplicate object member names at any depth,
require a nonempty string `alg`, and permit only JSON whitespace after the
object. Header failures are advisory `checksum_failed` lookalikes, while
segment/alphabet failures are not JWT candidates. Payload shape and
signature validation remain explicitly outside the offline contract.

The JWT witness covers the type, empty/nonempty handling, duplicate member,
trailing data, array, invalid constant, and padding near misses without
printing candidate bytes.

### Table parsing and raw spans

Section 4.4 now distinguishes the leading marker, separator, and terminal
bar; defines tab, multi-space, and bar separator precedence; rejects extra
columns/trailing prose; excludes quoted and backtick table values; handles LF,
CRLF, lone CR, and end-of-field; and trims the complete trailing punctuation
run before Q and reporting. Section 3.4 and the value rules make the raw
finding range the value bytes only, excluding structural delimiters,
punctuation, and line endings, while fingerprints and redaction continue to
use the reported stored-byte range.

The witness exercises all three markers, each separator family, terminal bar,
punctuation trimming, all listed line endings, quoted/backtick rejection, a
missing marker, and extra-column rejection.

### Advisory serialization and lifecycle

Section 5.2 now defines the exact single-LF stderr line, the integer `N`,
fingerprint-based invocation deduplication after dispositions/caps/view
deduplication, sorted unique ASCII rule identifiers, and the value/location-
free redaction boundary. It defines `secret_scan` only for existing JSON
object result forms, with integer `advisory_findings` and the same sorted
unique rule array; scalar/array and plain ID/text outputs are not wrapped or
changed, and zero findings omit the member.

The contract explicitly excludes acknowledged or mode-admitted blocking
findings from this advisory notice, and specifies success, semantic no-op,
`--no-auto-flush`, rollback/validation failure, dry-run, off mode, and
post-commit publication failure. The notice reports the successful semantic
dispatch even if later publication fails, while the command still reports
that failure and exits 1. The reference witness asserts the integer summary,
exact line shape, rule ordering, and the four positive versus four negative
lifecycle outcomes.

## Compatibility and threat-model disposition

**Compatibility: ACCEPTED for this exact artifact.** The corrected contract
remains an explicit ruleset-v4 extension and preserves the accepted v1
offline/deterministic scanner boundary, complete pre-write scan, atomic
blocking rejection, exact-fingerprint acknowledgment, raw-byte ranges,
redaction coordinate model, value-free output, and additive machine-output
rules. NEEDLE's stderr routing and valid-stdout requirements are respected.

**Threat model: ACCEPTED with explicit bounded-coverage limits.** The Q
counterexamples are documented rather than hidden; structural/provider context
and the section 7 false-positive disposition remain mandatory. Decoded caps
are deterministic and observable as partial coverage. JWT and table parsing
have fixed rejection boundaries. Advisory output cannot disclose values or
locations. Residual risks—unlabelled format-free credentials, indistinguish-
able unlabelled hexadecimal keys, partial encoded-view coverage, and possible
real-world false positives—are expressly retained as contract limitations or
release obligations, not represented as a clean-scan claim.

## Gate status and scope

- **BR-T35:** the exact-contract review prerequisite is **ACCEPTED** by this
  record for the corrected hashes above. The historical `bad7c810` rejection
  remains valid for that prior artifact and is not reused as this decision.
- **BR-T39 through BR-T43:** this record removes the exact-contract review
  blocker in principle, but does not accept implementation work, production
  conformance, parity, benchmarks, or NEEDLE replay. No implementation or
  release bead is closed or published by this review.
- **BR-T44:** remains blocked pending the required exact-source fleet replay,
  per-fingerprint disposition, advisory-volume comparison, parity evidence,
  benchmark evidence, capabilities evidence, and version freeze. No fleet
  remediation or publication is authorized here.

No dependency graph changes were made by this review. The review is a
prerequisite record for the release owner, not release evidence itself.

## Clean-room provenance

Candidate values were assembled only transiently by the named independent
witness. The committed record contains case names, integer metrics, verdicts,
hashes, and contract dispositions only. No format-valid candidate, matched
value, location, selector, or output containing candidate bytes was recorded.
No other implementation's source or behavioral corpus was consulted, and no
provenance exception occurred.

## Verification evidence

```verified:
python3 research/fixtures/secret-ruleset-v4-contract.py exit=0
python3 research/fixtures/secret-ruleset-v4-contract.py --table exit=0
```
