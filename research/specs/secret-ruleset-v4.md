# Secret Ruleset v4 and Matching Contract

Status: proposed normative specification; independent exact-hash review required
before implementation of sections 3 through 6.

Artifact identity: `urn:bead-rs:spec:secret-ruleset:v4`.

Date: 2026-10-03.

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
5. Scan `v` left to right. At each position take the first alternative that
   matches, otherwise advance one byte: four or more lowercase letters; four
   or more uppercase letters; one uppercase letter followed by three or more
   lowercase letters. Let `w` be the total bytes so matched. Then
   `10 * w < 7 * a` and `a - w >= m`.
6. Classify each byte as lowercase, uppercase, digit, or other. Let `t` be
   the count of adjacent byte pairs in different classes and `n` the length
   of `v`. Then `3 * t >= n - 1`.

Informative consequences, each required by the section 7 truth table: a bead
identifier, a name followed by a short hash, a timestamp, a version string, a
path, a camel-case type name, and a snake-case name fail; a 40-character
base62 string, a labelled 40-character hexadecimal string, and a base64
string containing `+` and `/` pass.

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
- **Decoded**: for each maximal run of 40 or more base64 or base64url
  alphabet bytes in the normalized view, at most 64 runs per field and at
  most 65,536 bytes per run, the lenient base64 decoding of the run, kept
  only when at least nine tenths of the decoded bytes are printable ASCII or
  ASCII whitespace.

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
| `json-web-token` | three segments of `[A-Za-z0-9_-]` joined by full stops, each at least 8 bytes, the first two beginning `eyJ` |

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

**Forms.**

1. Assignment: identifier, an optional closing quote or `)`, optional spaces
   or tabs, one of `=`, `:`, `:=`, `=>`, optional spaces or tabs, an optional
   opening quote, value.
2. Long option: `--`, identifier, then `=` or spaces, value.
3. Table row: optional leading spaces and one list or table marker (`-`,
   `*`, `|`), identifier, then a tab, two or more spaces, or `|`, value,
   optional trailing spaces or `|`, end of line.

**Value.** The maximal run of bytes other than whitespace, quotes, backtick,
comma, and semicolon, with trailing `.`, `)`, `]`, and `}` removed.

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
  with an `alg` member.

## 5. Advisory tier

### 5.1 Unlabelled token-shaped strings

`advisory-high-entropy-string` reports a maximal run of 20 or more bytes of
`[A-Za-z0-9+/=_.~-]` for which `Q(run, 16)` holds and which is not
hash-shaped. A run is hash-shaped when it is hexadecimal of length 32, 40,
56, 64, 96, or 128; a UUID; `gen-` followed by 32 hexadecimal digits; or a
bead identifier. At most 32 such findings are reported per field. The
per-character Shannon threshold of ruleset 3 is removed: it cannot exceed 4
for a hexadecimal string and is unstable below about 32 bytes, so it reported
hashes and missed short keys.

### 5.2 Write-time notice

When a mutation succeeds and its scan produced advisory findings, the command
writes exactly one line to standard error naming the count, the rule
identifiers, and `bead doctor --scope secrets`. Machine output gains the
additive member `secret_scan` with `advisory_findings`. Exit status and
standard output text are unchanged. The line never contains matched bytes and
is absent in `off` mode.

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
