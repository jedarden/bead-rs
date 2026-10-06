# Secret Ruleset v4 and Matching Contract

Status: proposed normative specification; independent exact-hash review required
before implementation of sections 3 through 6.

Artifact identity: `urn:bead-rs:spec:secret-ruleset:v4`.

Date: 2026-10-03. Compound-identifier correction proposed 2026-10-06.

This contract extends `secret-rejection-v1`. That contract's mutation
behavior, finding shape, fingerprint, acknowledgment, mode, and output
redaction rules remain authoritative where this document does not narrow
them. `historical-redaction-v1` and the proposed `secret-write-boundary-v1`
are unchanged. Decisions are recorded in ADR-023, ADR-024, and ADR-025.

## 1. Scope

This contract changes four things: which formats the blocking tier
recognizes, how a rule is matched against field bytes, how the advisory tier
selects and reports candidates, and how diagnostics report scan coverage.

It does not change where the scan runs (R039), who may weaken it (R039), or
how a stored finding is destroyed (R038). It does not claim complete
detection. A credential written as bare prose with no label, no structure,
and no provider format remains outside the blocking tier; an unlabelled
hexadecimal key is indistinguishable from a hash and is not reported at all.

Three corrections restore conformance with the accepted `secret-rejection-v1`
and do not wait for acceptance of this document: the npm checksum width
(section 4.5), overlapping prefilter evaluation (section 3.5), and
independent scanning of each diagnostic source (section 6).

## 2. Predicates

All predicates are pure functions of their input bytes. They use integer
arithmetic only, so a verdict is identical on every platform.

### 2.1 Placeholder predicate `P(v)`

`P(v)` is true when any of the following holds:

- `v` contains one of the bytes `<`, `>`, `$`, `{`, `}`, `*`, `[`, `]`, `(`,
  `)`, `|`, or a backtick;
- `v` contains three consecutive full stops or U+2026;
- every byte of `v` is the same byte; or
- the ASCII-lowercased `v` contains one of: `example`, `placeholder`,
  `replace`, `your_`, `_here`, `dummy`, `sample`, `xxxx`, `0000000`,
  `redact`, `changeme`, `todo`, `fake`, `masked`.

Provider-format rules keep the `secret-rejection-v1` placeholder check on the
captured body. `P` applies to values tested by `Q`.

### 2.2 Randomness qualifier `Q(v, m)`

`Q(v, m)` decides whether a labelled or structurally located value is random
enough to be a credential rather than an identifier, a path, a timestamp, or
a word. `m` is the minimum amount of non-word material. Evaluate in order;
the first failing step returns false.

1. `v` is 1 to 512 bytes, every byte in 0x21 through 0x7E.
2. `P(v)` is false.
3. Let `a` be the count of ASCII alphanumeric bytes in `v`. They include at
   least one digit and at least one letter.
4. Let `d` be the count of digits. Unless `v` is at least 32 bytes and
   consists only of hexadecimal digits of one letter case, `5 * d < 4 * a`.
5. Scan `v` with a cursor initially at byte zero. At each position try, in
   order: the maximal contiguous lowercase run, if at least four bytes;
   the maximal contiguous uppercase run, if at least four bytes; one
   uppercase byte followed by the maximal lowercase run, if that lowercase
   run has at least three bytes. Consume the entire first matching run,
   add its length to `w`, and advance to its end. Otherwise advance exactly
   one byte. Consumed bytes are never reconsidered or counted twice. Then
   `10 * w < 7 * a` and `a - w >= m`.
6. Classify each byte as lowercase, uppercase, digit, or other. Let `t` be
   the count of adjacent byte pairs in different classes and `n` the length
   of `v`. Then `3 * t >= n - 1`.

Q is a shape qualifier, not a proof of credential identity. In particular,
some short identifier/hash shapes pass at `m=8`, and some version-shaped
strings pass at `m=12`. The former unconditional claim that these classes
always fail is withdrawn. Structural or label context remains mandatory;
unlabelled identifiers are separately excluded by section 5.1. A real false
positive still blocks release under section 7; do not broaden exclusions or
silently change Q to make a replay pass.

The runtime-assembled truth table in
`research/fixtures/secret-ruleset-v4-contract.py` is normative for its named
representatives, at all four thresholds. It records `(n,a,d,w,t)` and the
first failing step, not candidate bytes. The 31/32-byte, one-case/mixed-case
hex witnesses and maximal-versus-minimum word-run witness are included.

## 3. Matching semantics

### 3.1 Boundaries

Word-boundary assertions are not used. A provider-format or context-bound
match is valid only when the byte before its start is absent or is not ASCII
alphanumeric, and the byte after its end is absent or is not in that rule's
body alphabet. The check is a predicate on the match span, not regular
expression lookaround.

### 3.2 Views

A field is scanned in up to four views. Every transformation is linear in
the field length and records, for each output byte, the raw byte range that
produced it.

- **Raw**: the field's UTF-8 bytes.
- **Normalized**: the raw view after, in order: (1) removal of ANSI control
  sequences (ESC `[` parameters and one final byte; ESC `]` through BEL or
  ESC `\`); (2) one pass of backslash-escape decoding for `\n`, `\r`, `\t`,
  `\"`, `\'`, `\\`, `\/`, and `\uXXXX` including surrogate pairs; (3)
  percent-decoding of every `%HH` that yields a printable ASCII byte.
- **Dewrapped**: the normalized view with each line terminator deleted,
  together with any spaces or tabs that immediately follow it and one
  backslash that immediately precedes it, when the bytes on both sides of the
  deleted span are in `[A-Za-z0-9_+/=-]`.
- **Decoded**: enumerate maximal runs of the union alphabet
  `[A-Za-z0-9+/_=-]` in the normalized view, left to right. A run qualifies
  at 40 source bytes, including padding. Only the first 64 qualifying runs
  are considered; invalid, over-size, or nonprintable runs still consume a
  slot. A run above 65,536 source bytes is skipped whole, never truncated or
  split. Replace `-` with `+` and `_` with `/`, allowing both alphabets in
  one run. Accept zero, one, or two terminal `=` bytes only. The unpadded
  length modulo four must not be one; supplied padding, if any, must make
  the total length a multiple of four and equal the required padding. Add
  missing terminal padding; unused low bits need not be zero. Decode once,
  without whitespace removal or recursive decoding. Retain nonempty output
  only when `10 * printable >= 9 * decoded_length`, where printable means
  bytes 0x20..0x7e or ASCII whitespace 0x09..0x0d.

Decoded limits are observable, not a clean-scan claim: diagnostic details
contain `view_coverage`, with one value-free entry per scanned source that
hit a bound, naming `view: "decoded"`, `status: "limited"`, and sorted unique
`reason_codes` (`run_count_limit`, `run_size_limit`). Entries contain no
locations or candidate content. A skipped invalid/nonprintable run alone is
not a coverage limit. Raw/normalized/dewrapped scans still cover the entire
field. Limited decoding does not itself reject a mutation or suppress other
findings; capabilities and release evidence must not claim complete encoded
coverage. Boundaries 39/40, 64/65, and 65536/65537 are required fixtures.

### 3.3 Which rules see which view

Blocking rules run on the raw, normalized, and dewrapped views. Only
provider-format rules and the private-key rule run on the decoded view.
Advisory rules run on the raw view only.

### 3.4 Reporting

`start` and `end` remain raw field byte offsets, as `secret-rejection-v1`
section 2 requires. A match found in a derived view reports the smallest raw
range that produced its bytes; a match in the decoded view reports the raw
range of the whole encoded run. The fingerprint is computed over the raw
bytes of the reported range. One finding is reported per distinct pair of
rule identifier and raw range, however many views produced it. Because
ranges and fingerprints stay defined over stored bytes, `bead redact`
operates on these findings without change.

### 3.5 Prefilter

A rule whose keyword anchor occurs anywhere in a view must be evaluated on
that view. Anchor occurrences are enumerated with overlap; one anchor never
hides another.

### 3.6 Cost

The scan-overhead benchmark lane is extended to all views. On the hostile
4 MiB field, the ruleset 4 scan may take at most three times the ruleset 3
measurement on the same host, and release evidence records both numbers.

## 4. Ruleset 4: blocking rules

`RULESET_VERSION` is 4. The inventory is closed and compiled. Capabilities
keep the `secret-rejection-v1` contract identity and add
`ruleset_contract` with this document's identity. Every ruleset 3 blocking
rule remains unless this section changes it.

### 4.1 Provider formats

| Rule identifier | Shape |
|---|---|
| `docker-hub-token` | `dckr_pat_` or `dckr_oat_`, then 20 to 64 bytes of `[A-Za-z0-9_-]` |
| `tailscale-key` | `tskey-`, one or more lowercase letters, `-`, six or more alphanumerics, `-`, twenty or more alphanumerics |
| `vault-batch-token`, `vault-recovery-token` | `hvb.` or `hvr.`, then 20 or more bytes of `[A-Za-z0-9_-]` |
| `backblaze-application-key` | `K00`, then exactly 28 bytes of `[0-9A-Za-z+/]` |
| `openai-api-key` (extended) | additionally `sk-proj-`, `sk-svcacct-`, or `sk-admin-`, then 40 or more bytes of `[A-Za-z0-9_-]` |
| `openrouter-api-key` | `sk-or-v1-`, then exactly 64 lowercase hexadecimal digits |
| `age-secret-key` | `AGE-SECRET-KEY-1`, then exactly 58 bytes of `[0-9A-Z]` |
| `json-web-token` | exactly three segments of `[A-Za-z0-9_-]` joined by full stops, each at least 8 bytes, the first two beginning `eyJ` |

JWT recognition consumes a maximal dot-separated segment chain, not a
three-segment prefix of a longer chain. Adjacent `.` invalidates a candidate
even though it is outside the body alphabet. The header is unpadded
base64url: reject length modulo four equal to one and nonzero unused bits;
then require valid UTF-8 containing exactly one complete JSON object, with
only JSON whitespace after it. Reject duplicate member names in any object.
`alg` must be a nonempty string; its contents are not a trust or signature
validation. The payload is shape-checked only; payload JSON and signatures
are not validated offline. A header failing these checks is a
`checksum_failed` advisory lookalike, not a blocking JWT. Segment-count or
alphabet failures are not JWT candidates. A direct finding spans the entire
three-segment token; derived-view ranges follow section 3.4 unchanged.

### 4.2 Context-bound formats

A context-bound format blocks only when a label occurs on the same line
within the 64 bytes before the match.

| Rule identifier | Shape | Label, case-insensitive |
|---|---|---|
| `vault-legacy-token` | `s.`, then exactly 24 base62 bytes | `vault`, `bao`, or `token` |
| `backblaze-key-id-assignment` | 25 lowercase hexadecimal digits beginning `00`, as the value of an assignment | an identifier ending in `key_id`, `keyid`, `key id`, `access_key_id`, or `account_id`, separators `_`, `-`, or space |

### 4.3 Structural credentials

| Rule identifier | Structure | Blocks when |
|---|---|---|
| `uri-userinfo-credential` | scheme, `://`, user, `:`, password, `@`, host | `Q(password, 8)` after percent-decoding |
| `authorization-header-credential` | `authorization`, `:` or `=`, a scheme word (`bearer`, `basic`, `token`, `apikey`), value; or `bearer` followed by a value of 20 or more bytes | `Q(value, 12)` |
| `curl-user-credential` | `-u` or `--user`, then `name:password` | `Q(password, 8)` |
| `kubernetes-secret-data` | a YAML document containing a `kind` field whose value is `Secret` (quoted or unquoted), and a `data:` or `stringData:` field, in either order, with indented entries beneath it | under `data:`, a value of 16 or more base64 bytes; under `stringData:`, `Q(value, 12)` |

The reported range is the password or value, never the surrounding
structure.

### 4.4 Labelled credential assignment

Rule identifier `credential-assignment`.

**Identifier.** At most 64 bytes of components joined by `_`, `-`, `.`, a
single space, or a lower-to-upper case change, not preceded by an
alphanumeric, `_`, `.`, or `-`. It contains a keyword and, after the keyword,
only suffix components.

- Keywords: `password`, `passwd`, `passphrase`, `pwd`, `secret`, `token`,
  `credential`, the whole component `pat`, and `key` immediately preceded by
  one of `api`, `access`, `secret`, `private`, `signing`, `encryption`,
  `master`, `application`, `account`, `auth`, `app`.
- Suffix components: `key`, `token`, `secret`, `id`, `value`, `string`,
  `data`, `b64`, `base64`, `plain`, `text`, digits, or `v` followed by
  digits.
- Excluded identifiers: `acknowledge-secret`; any beginning `secret_scan` or
  `secret-scan`; `fencing-token` and `fencing_token`; `max_tokens`; any
  ending in `_file`, `_path`, `_name`, `_ref`, `_label`, `_count`, `_limit`,
  `_budget`, or `_usage`, with `-` accepted for `_`. A plural keyword is not
  a keyword.

Matching is ASCII case-insensitive.

Capture the complete identifier before testing keywords. Split camel case
before ASCII case-folding; after folding, components and keywords match as
whole components, not substrings. Prefix components may precede the keyword;
all components after it must belong to the suffix list. Exclusions are
case-folded **literal** tests against the full captured identifier: named
exact spellings match exactly, beginning/ending entries are literal prefixes
and suffixes. Do not normalize alternative separators for exclusions.
Thus `fencing.token`, `acknowledge_secret`, and `max.tokens` are not the
listed exclusions, although the positive keyword grammar may independently
fail. The `_file`-style exclusions intentionally duplicate positive-grammar
rejection and protect that boundary if the grammar is later extended.
An exclusion suppresses this rule's blocking result **and** its Q-fail
advisory fallback. It does not suppress any independent provider, structural,
private-key, or unlabelled advisory match on the same field.

**Forms.**

1. Assignment: identifier, an optional closing quote or `)`, optional spaces
   or tabs, one of `=`, `:`, `:=`, `=>`, optional spaces or tabs, an optional
   opening quote, value.
2. Long option: `--`, identifier, then `=` or spaces, value.
3. Table row: optional leading spaces, one required marker (`-`, `*`, `|`),
   optional spaces/tabs, identifier, a separator, value, then end of line.
   A separator is one or more tabs, two or more spaces, or one `|` with
   optional surrounding spaces/tabs. Consume the entire whitespace
   separator; a single space belongs to the identifier, not the separator.
   Strip one optional terminal `|` and trailing spaces/tabs before parsing
   the value, so a no-space closing bar is not part of the value. No further
   column or trailing prose is allowed. Leading `|`, separator `|`, and
   terminal `|` are distinct positions. Quoted/backtick table values do not
   match this form; independent rules remain active. Lines end at LF, CRLF,
   lone CR, or end of field, excluding the terminator bytes.

**Value.** The maximal run of bytes other than whitespace, quotes, backtick,
comma, and semicolon, with trailing `.`, `)`, `]`, and `}` removed.
Remove the entire trailing run of those punctuation bytes before Q and
reporting: the raw finding covers only the remaining value. Assignment/long
option opening quotes and closing punctuation are outside the finding. The
raw range of table findings likewise excludes marker, identifier, separator,
spaces, terminal bar, and line ending. The reported raw bytes, not a
transformed value, remain the fingerprint/redaction input under section 3.4.

**Verdict.** Forms 1 and 2 block when `Q(value, 12)`; form 3 blocks when
`Q(value, 20)`. A value of 8 or more bytes that fails `Q` and for which `P`
is false is reported by the advisory rule `advisory-keyword-assignment`,
whose ruleset 3 pattern this section replaces.

### 4.5 Dispositions

`placeholder` and `checksum_failed` keep their meaning. Three formats
validate before blocking, and a candidate that fails is reported as
`checksum_failed` in the advisory tier:

- GitHub classic tokens: unchanged, the last 6 of 36 body bytes are the
  base62 CRC32 of the first 30.
- npm tokens: the body is 36 base62 bytes and the last **6** are the base62
  CRC32 of the first 30, the same scheme as GitHub. The ruleset 3 width of 8
  is a defect: it reports every conforming npm token as a lookalike.
- JSON web tokens: the first segment must base64url-decode to a JSON object
  with an `alg` member satisfying section 4.1's complete header rules.

## 5. Advisory tier

### 5.1 Unlabelled token-shaped strings

`advisory-high-entropy-string` reports a maximal run of 20 or more bytes of
`[A-Za-z0-9+/=_.~-]` for which `Q(run, 16)` holds and which is not
hash-shaped. A run is hash-shaped when it is hexadecimal of length 32, 40,
56, 64, 96, or 128; a UUID; `gen-` followed by 32 hexadecimal digits; or a
bead identifier. A UUID has exactly the case-insensitive hexadecimal
`8-4-4-4-12` shape; a bead identifier is a lowercase ASCII letter followed
by zero to 31 lowercase letters/digits, a hyphen, and 8 to 64 lowercase
hexadecimal digits. Generation identifiers use exactly the lowercase `gen-`
prefix. Exclusions test the whole maximal run, not a substring.

**Compound identifiers (2026-10-06 correction).** This additional exclusion
applies only to this unlabelled advisory rule; it never suppresses a blocking
rule or the labelled-assignment advisory fallback. It does not change `P`, `Q`,
reported raw ranges, fingerprints, or the conformance/release gates.

For exclusion testing only, remove the complete terminal run of `.` bytes.
If the remainder is a whole hash shape above, exclude it. Otherwise require
at least one separator from `/_.~-`, no `+` or `=`, and at least one recognized
identifier atom. Split the entire remainder on `/`, `_`, `.`, and `~`. For
each resulting chunk, recognize a whole hash shape before splitting on `-`.
In a hyphen-split chunk, recognize a consecutive UUID group of exactly five
parts with hexadecimal widths `8-4-4-4-12` before evaluating individual parts.
Otherwise a recognized atom is an entire hexadecimal part of length 8, 12,
16, 32, 40, 56, 64, 96, or 128. Hash and UUID comparisons are case-insensitive
except the generation and bead prefixes specified above.

One further atom is a 32-byte Nix base32 store hash, alphabet
`0123456789abcdfghijklmnpqrsvwxyz`, immediately followed by `-` and a nonempty
name in the first store component after the exact initial `/nix/store/` or
`nix/store/`. It is not recognized in an arbitrary path or as a bare token.
Evaluate its name and all remaining components by the same rules below.

Every nonempty part not consumed by a recognized atom must consist entirely
of ASCII alphanumerics, must not contain lowercase, uppercase and digits
together, may have at most two adjacent digit/non-digit transitions, and may
have at most four adjacent alphabetic lower/upper-case changes (count a pair
only when both bytes are letters). This last bound preserves bounded CamelCase
names without letting an opaque alphabetic sibling borrow a hash's digits to
pass `Q` and then disappear from the advisory scan.
Empty parts are separator syntax and contribute no atom. The entire run is
excluded only if every part passes and at least one atom was recognized.
A hash substring inside a larger part is not an atom. A hash/path prefix
must not hide a following opaque base62/base64 component. Runs containing
`+` or `=` remain eligible unless they satisfy the original whole-hash test.
Bare nonstandard-width hex tokens keep their original eligibility.

The independently assembled fixture adds positive opaque siblings, embedded
hash near misses, Nix anchoring negatives, UUID/generation/bead path atoms,
mixed-case identifier boundaries and punctuation cases. The amendment is
pending a new complete-contract exact-hash independent acceptance; the earlier
acceptance does not authorize this correction's implementation.

At most 32
eligible findings are reported per field, in ascending raw start offset;
ineligible runs do not consume a slot. The
per-character Shannon threshold of ruleset 3 is removed: it cannot exceed 4
for a hexadecimal string and is unstable below about 32 bytes, so it reported
hashes and missed short keys.

### 5.2 Write-time notice

An invocation collects only findings whose reported tier is `advisory`,
after disposition, per-field caps, and cross-view deduplication. Deduplicate
the complete invocation by fingerprint, including repeated CLI/service scans.
Let N be the resulting count; R is the unique rule identifiers sorted by
ASCII bytes, joined with comma followed by one space. Confirmed blocking
findings admitted by acknowledgment or `advisory` mode retain their blocking
tier and do not enter this notice. A failed-checksum/placeholder lookalike
enters it only when the finding's reported tier is advisory. Off mode emits
neither notice nor additive JSON member.

After successful semantic dispatch (including a successful semantic no-op),
if N is nonzero, emit exactly this UTF-8 line, terminated by one LF:

```text
secret_scan advisory: N finding(s), rules R; inspect bead doctor --scope secrets. Matched bytes are not shown.
```

N is unpadded decimal, even when one; the literal `finding(s)` never changes.
Rule identifiers come from the compiled ASCII inventory. No location,
selector, fingerprint, matched content, quotes, or terminal controls appear.
Other success diagnostics may coexist, but exactly one line has this prefix.

For machine-readable mutations whose existing result is a JSON object, add
the object member `secret_scan: {"advisory_findings": N, "rules": [R1,...]}`.
The array contains the same sorted unique identifiers, not finding objects.
This applies to each emitted result object, including JSONL objects; a scalar
or array result is not wrapped or extended. Plain text/ID-only stdout and
all existing JSON fields/types are unchanged. With N zero the member is
absent. The summary is count-and-rule-only, never a findings/locations API.

Validation failure, transaction rollback, and a dry-run emit no write-time
notice or additive member. `--no-auto-flush` does not suppress them after a
successful semantic dispatch. If semantic commit succeeds but subsequent
checkpoint publication fails, the notice and any already-emitted result
remain valid accounts of the semantic request; the command still emits its
post-commit publication-failure diagnostic and exits 1. Neither notice nor
JSON summary claims that the checkpoint published. No additional success
event or acknowledgment is recorded for the notice itself.

## 6. Diagnostic coverage

`doctor --scope secrets` scans three sources independently: live semantic
rows, the current generation, and the previous generation. One source
failing never suppresses findings from another. The check details gain
`coverage`, one entry per source with `source`, `status` (`scanned`,
`absent`, or `unreadable`), and a `reason_code` when not scanned.

- `absent` is a previous generation that legitimately does not exist: before
  the second publication, or after a mode transition tombstoned its root. It
  is not an error.
- `unreadable` makes the check status `error` while every finding from the
  scanned sources is still returned.

## 7. Conformance

Fixtures and tests assemble every candidate at test time. No committed file
contains a format-valid sample, and no output contains matched bytes.

1. **Rules.** Each blocking rule has at least one true positive and two
   near-miss negatives, including the boundary cases of section 3.1.
2. **Qualifier.** A truth table covers every class named in section 2.2 and
   every excluded identifier in section 4.4.
3. **Views.** A checksum-valid GitHub classic token blocks when it follows an
   escaped newline, sits inside ANSI color sequences, is percent-encoded
   after `token%3D`, is wrapped across a line break, and is base64-encoded.
   In each case `bead redact --dry-run` resolves the reported fingerprint.
4. **Parity.** For each class the fleet Git-layer scanner blocks (provider
   prefixes, credential assignments, authorization headers, curl user
   options, URI credentials, private-key armor, JSON web tokens, Kubernetes
   Secret blocks) a synthetic true positive produces a bead-rs blocking
   finding. That scanner's known false-positive classes (diagnostic prose in
   which a keyword precedes a `name=digit` token; hyphenated file names) do
   not block. Parity is measured by fixtures, not by shared code or rule
   text.
5. **Advisory volume.** Across the fleet stores used for the replay, the
   ruleset 4 advisory count is at most one tenth of the ruleset 3 count for
   the same stores, and every unlabelled synthetic token fixture is still
   reported.
6. **Diagnostics.** Deleting the previous root yields `absent` or
   `unreadable` as section 6 defines, with live findings intact.
7. **Fleet replay.** Before `RULESET_VERSION` 4 is frozen, the candidate
   binary runs `doctor --scope secrets` over every reachable fleet workspace.
   Release evidence records, per rule, the count of blocking findings and a
   disposition for each fingerprint: a credential (rotate, then redact) or a
   false positive (correct the rule or the excluded identifiers, then
   replay). Freeze requires no undispositioned finding and no outstanding
   false positive. Evidence carries fingerprints and counts only.
8. **Cost.** Section 3.6.

The reviewer records an exact SHA-256 acceptance or an actionable rejection.
BR-T39 through BR-T44 remain blocked until then.

## Appendix: calibration (informative)

A prototype of sections 2 and 4 was replayed on 2026-10-03 over the 82 live
bead stores on one fleet host. Ruleset 3 reported 0 blocking and 37,547
advisory findings there. The prototype reported 11 distinct blocking values
in 12 beads, and every identifier that triggered `credential-assignment`
named a credential. Ruleset 3 classified none of those values as blocking.
For the values examined in detail it gave four of twelve stored copies no
finding of any tier, which left `bead redact` with nothing to select.
Dispositions belong to the operator-held release evidence of section 7, not
to this document.
