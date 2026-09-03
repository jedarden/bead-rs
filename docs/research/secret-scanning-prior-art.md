# Secret-Scanning Prior Art

Third-party research supporting
[ADR-014](../adr/014-hard-reject-secret-bearing-mutations.md): how existing
scanners detect secret strings, how production systems handle *hard
rejection* specifically, and what ports cleanly into an offline Rust CLI.
Facts verified against primary sources on 2026-09-03; URLs at each claim.
This document is descriptive research, not a normative specification.

**Hygiene note:** this file deliberately contains no token-shaped literals —
patterns are given in regex notation or prose so the document itself passes
every scanner in this environment's push path.

## 1. The two production hard-reject systems

Only two widely deployed systems reject a write outright on secret content,
and they independently converged on the same doctrine.

### GitHub push protection

Runs server-side at push time and rejects the `git push` (also blocks web-UI
commits, uploads, and API writes)
([docs](https://docs.github.com/en/code-security/concepts/secret-security/push-protection)).
Design facts that matter for a mutation-boundary gate:

- **Blocks only on high-confidence provider patterns.** Launch post: "push
  protection only supports token types that can be detected accurately …
  69 high confidence patterns that each have a signal-to-noise ratio that
  developers can trust"
  ([announcement](https://github.blog/news-insights/product-news/push-protection-github-advanced-security/)).
  Older versions of a provider's tokens are excluded from blocking because
  they false-positive more
  ([patterns doc](https://docs.github.com/en/code-security/secret-scanning/introduction/supported-secret-scanning-patterns)).
- **No entropy, no generic detection in the blocking path.** Entropy-style
  and even AI-based generic-password detection (Copilot secret scanning)
  exist only in the non-blocking alert tier: "Push protection and validity
  checks are not supported for passwords"
  ([changelog](https://github.blog/changelog/2024-10-21-copilot-secret-scanning-for-generic-passwords-is-generally-available/),
  [engineering writeup](https://github.blog/engineering/platform-security/finding-leaked-passwords-with-ai-how-we-built-copilot-secret-scanning/)).
  The stated logic throughout: a blocking control that cries wolf gets
  bypassed or disabled.
- **The rejection names the secret type, commit, and `path:line` — never
  the value**
  ([CLI walkthrough](https://docs.github.com/en/code-security/how-tos/secure-your-secrets/work-with-leak-prevention/push-protection-on-the-command-line)).
  The error message cannot become a second copy of the credential.
- **Bypass is per-finding, reason-coded, and audited.** Exactly three
  reasons (used in tests / false positive / fix later), each producing a
  different alert disposition; every bypass creates an audit-log event and
  notifies owners and security managers. Orgs can require delegated
  approval for bypasses.
- **Provider patterns come from the providers.** Partner-program providers
  submit the regexes for their own token formats and stand up verification
  endpoints
  ([partner program](https://docs.github.com/en/code-security/tutorials/secret-scanning-partner-program))
  — which is why the patterns are precise: the author of the regex is the
  author of the token generator.

### GitLab secret push protection

A pre-receive-hook gate over a curated subset of GitLab's gitleaks-derived
ruleset: "Only high-confidence patterns were chosen … to minimize the delay
when pushing your commits and minimize the number of false alerts"
([docs](https://docs.gitlab.com/user/application_security/secret_detection/secret_push_protection/)).
Same shape as GitHub: rejection message carries `path:line` plus secret
type, never the value; bypass via a push option or commit-message marker,
both generating audit events. Operationally honest coverage caps: files
over 1 MiB skipped, oversized pushes skipped, diff-only scanning.
(GitLab's separate "prevent pushing secrets" push rule is file-*name* based
only — `id_rsa`, `*.pem`, credential file paths — and inspects no content
([push rules](https://docs.gitlab.com/user/project/repository/push_rules/)).)

**Convergent doctrine:** block only on precision-vetted structured
patterns; keep statistical detection advisory; report type + location,
never the value; make the bypass per-finding, deliberate, and audited.

## 2. Token formats are engineered to be detected

Since 2021 the industry has redesigned credentials specifically so scanners
can match them with near-zero false positives.

GitHub's token-format redesign
([Behind GitHub's new authentication token formats](https://github.blog/engineering/platform-security/behind-githubs-new-authentication-token-formats/)):

- Purpose-signaling prefixes (`ghp_`, `gho_`, `ghu_`, `ghs_`, `ghr_`, later
  `github_pat_`): "Token prefixes are a clear way to make tokens
  identifiable."
- Underscore as separator because it is *not* a base64 character — a token
  cannot be a substring of a random base64 blob, and double-click selects
  the whole token.
- **The last 6 characters are a CRC32 checksum of the random portion,
  base62-encoded.** "A checksum virtually eliminates false positives for
  secret scanning offline" — a scanner can confirm or refute a candidate
  "without having to hit our database." npm adopted the identical scheme
  ([npm token format](https://github.blog/security/announcing-npms-new-access-token-format/)).
  This is the strongest offline primitive available: for checksummed
  formats, a match that fails the checksum is provably not a live token.
- Prefix alone was projected to bring the scanning false-positive rate to
  0.5%.

Prefix inventory across providers (each provider's docs;
[survey](https://apikeys.guide/docs/implementation/key-formats-and-prefixes),
[GitLab token prefixes](https://docs.gitlab.com/security/tokens/)):
GitHub `ghp_`/`gho_`/`ghu_`/`ghs_`/`ghr_`/`github_pat_` (CRC32-checksummed),
npm `npm_` (CRC32-checksummed), Stripe `sk_live_`/`rk_live_`/…, Slack
`xox`-family, AWS key IDs `(A3T…|AKIA|ASIA|…)` + 16 base32 chars, Anthropic
`sk-ant-`, OpenAI `sk-proj-`/`sk-svcacct-`/`sk-admin-`, PyPI `pypi-`
(base64 macaroon body, prefix added "to enable efficient secret scanning"
per [PyPI docs](https://docs.pypi.org/api/secrets/)), GitLab `glpat-` and
eleven sibling prefixes ("designed to be standard identifications",
non-configurable).

**Takeaway:** a prefix-anchored ruleset covers the modern token population
far better than any statistical method, and for checksummed formats an
offline validator reaches effectively zero false positives.

## 3. Tool survey

### gitleaks (Go) — the reference ruleset

[gitleaks](https://github.com/gitleaks/gitleaks). Regex rules + per-rule
keyword prefilter + per-rule Shannon-entropy floor; no ML, no network.
TOML rule shape:

```toml
[[rules]]
id = "example-rule"
regex = '''one-go-style-regex'''
secretGroup = 3        # capture group holding the secret; entropy applies to it
entropy = 3.5          # minimum Shannon entropy floor
keywords = ["token"]   # aho-corasick prefilter anchors
```

The default ruleset is *generated* from Go definitions
(`cmd/generate/config/rules/*.go`); every rule is validated at generation
time against listed true positives and false positives (the AWS rule keeps
a low-entropy all-one-char key ID and a DNA sequence as FP regression
fixtures). Runtime FP suppression: inline `gitleaks:allow` comments,
per-rule and global allowlists (paths, regexes, stopwords, with
`regexTarget` selecting secret/match/line), `.gitleaksignore` fingerprints,
and report baselines. Performance: a single Aho-Corasick trie over all
rules' keywords selects which regexes run at all; the regex engine moved to
a RE2 build (`wasilibs/go-re2`) for large-input throughput
([issue #1796](https://github.com/gitleaks/gitleaks/issues/1796)).
Blocking is exit-code-based (nonzero on findings) for pre-commit/CI use.

### trufflehog v3 (Go) — verification-centric

[trufflehog](https://github.com/trufflesecurity/trufflehog). ~800
detectors, each `Keywords() + FromData()`; one shared Aho-Corasick trie
routes 512-byte windows around keyword hits to detectors, so regexes never
scan whole inputs. The load-bearing false-positive control is **live
verification** — each detector calls the real provider API
([how verification works](https://trufflesecurity.com/blog/how-trufflehog-verifies-secrets)).
That is structurally unavailable offline, and undesirable regardless: it
transmits candidate credentials to third parties. Without verification
trufflehog itself treats results as noisy (blocking is opt-in via `--fail`,
exit 183). The source carries the maintainers' verdict on entropy as a
primary signal: "Shannon entropy did not work well."

### Yelp detect-secrets (Python) — entropy plugins and baselines

[detect-secrets](https://github.com/Yelp/detect-secrets). Plugin
architecture mixing vendor regexes, a keyword detector, and two entropy
plugins with exact defaults (from
`detect_secrets/plugins/high_entropy_strings.py`): base64 charset limit
**4.5** bits/char, hex charset limit **3.0**, applied only to *quoted*
strings; an all-digit candidate gets its entropy docked by
`1.2 / log2(len)` because numeric IDs otherwise dominate false positives.
Its signature workflow is the **baseline**: existing findings are recorded
and audited, and the pre-commit hook fails only on findings *not* in the
baseline — block new secrets, tolerate legacy ones. Inline suppression via
`# pragma: allowlist secret`.

### ripsecrets (Rust) — the closest implementation model

[ripsecrets](https://github.com/sirwart/ripsecrets). One combined
alternation regex per pass (built on ripgrep's `grep`/`ignore` crates):
~30 provider-prefix patterns plus one generic assignment rule — a variable
whose name contains `key|token|secret|password`, an assignment operator,
and a 15–90 char value. Every generic candidate must then pass a
**statistical randomness test** (`src/matcher/p_random.rs`): assume the
string is uniformly random over its inferred alphabet (hex/base36/base64)
and compute the probability of what is observed —

1. exact combinatorial probability of the observed count of *distinct
   characters* (prose and identifiers reuse characters);
2. binomial tail probability of each character class's count
   (digits/upper/lower vs the alphabet's expected proportions);
3. for base64-range strings, binomial tail probability of the observed
   count of ~400 common source-code bigrams (`er`, `te`, `an`, …);

multiply the p-values, and reject the candidate as *not a secret* when the
result is below ~1e-5 (1e-4 for digit-free strings). This kills
`hello_world` and camelCase identifiers while passing genuinely random
strings — a sharper instrument than one Shannon number over a short
string. Benchmarks (Sentry repo): ripsecrets 0.32 s vs trufflehog 31.2 s
vs detect-secrets 73.5 s. Suppression: `.secretsignore` (paths + a
`[secrets]` value section) and `pragma: allowlist secret` comments. MIT
licensed; the p-value module is ~200 lines and more realistically vendored
than depended on (no stable library API).

### git-secrets (AWS, bash) — the minimal viable gate

[git-secrets](https://github.com/awslabs/git-secrets). Pure prohibited
patterns via `git grep -E` in pre-commit/commit-msg hooks; AWS key-ID
regexes plus loose assignment patterns; an allowed-patterns list cancels
matches (AWS's own `…EXAMPLE`-suffixed documentation keys ship
pre-allowlisted). Proves the floor: a deterministic pattern gate is small
and workable, with recall limited to exactly what is enumerated.

### Nosey Parker and Kingfisher (Rust) — high-precision at scale

[Nosey Parker](https://github.com/praetorian-inc/noseyparker) (Praetorian):
~188 YAML rules "chosen for high precision", each *required* to have a
capture group isolating the secret and to carry `examples` /
`negative_examples`; **no entropy anywhere**. Two-stage matching: all rules
in one Vectorscan (Hyperscan-fork) SIMD database, then per-rule Rust
`regex::bytes` re-match for captures and offsets; ~GB/s throughput.
Report-only. [Kingfisher](https://github.com/mongodb/kingfisher) (MongoDB)
forked it, adding tree-sitter language awareness, offline **checksum
validation** for checksummed token formats, and live validation with a
distinct exit code for *validated* findings (block only on 205) — a useful
precedent for tiering by confidence.

### Talisman and secretlint — two portable ideas

[Talisman](https://github.com/thoughtworks/talisman): ignores are bound to
a SHA-256 checksum of the ignored file, so a suppression silently expires
when the content changes. [secretlint](https://github.com/secretlint/secretlint):
deterministic per-rule packages, explicitly no entropy, and — uniquely —
**masks secret values in its own report output by default**: the only
surveyed tool that treats its report as a leak vector.

## 4. Entropy: formula, thresholds, and why it cannot block

The formula every tool implements: for string *s* of length *n* with
character counts, `H(s) = -Σ (count/n) · log2(count/n)` — per-character
Shannon entropy in bits, maximum `log2(alphabet)` (≈4 hex, ≈3.32 digits,
6 base64). Thresholds in the wild: detect-secrets 4.5 base64 / 3.0 hex;
gitleaks per-rule floors mostly 3–4 applied to the regex capture only;
classic trufflehog ~4.5 base64 / 3.0 hex over 20+ char windows.

Why no modern tool blocks on it: git SHAs, UUIDs, checkpoint roots, base64
payloads, minified JS, and lockfile hashes all clear any usable threshold,
while most human passwords sit below it. A 2025 evaluation found a few
entropy-heavy rules produced 72.4% of all detections, dominated by commit
hashes and default config ([arXiv 2512.08326](https://arxiv.org/pdf/2512.08326)).
Entropy also measures randomness, not sensitivity — a public key and a
private key score identically.

The academically validated high-precision recipe is the inverse
composition — structure first, statistics as a veto. Meli et al., "How Bad
Can It Git?" ([NDSS 2019](https://www.ndss-symposium.org/ndss-paper/how-bad-can-it-git-characterizing-secret-leakage-in-public-github-repositories/)):
regexes restricted to key formats with "distinct structure", then three
*negative* filters on matches — drop very-low-entropy candidates, drop
candidates containing dictionary words ≥5 chars, drop repeat/ascending/
descending patterns. 99.29% of regex matches survived the filters and
89.1% of found credentials were judged genuinely sensitive.

## 5. Rust implementation landscape

- **`regex` crate**: no lookaround or backreferences by design, in
  exchange for worst-case linear-time matching — a built-in ReDoS
  guarantee for a gate on the mutation hot path.
  [`RegexSet`](https://docs.rs/regex/latest/regex/struct.RegexSet.html)
  reports which of a union of patterns match in a single pass but yields
  no spans; the idiomatic pipeline is set-or-alternation first, then
  re-run only the winning per-rule regexes to recover the span for the
  diagnostic.
- **Ruleset portability is mechanical**: gitleaks is Go, and Go `regexp`
  is RE2-syntax with no lookaround at all
  ([gitleaks #258](https://github.com/gitleaks/gitleaks/issues/258)) — so
  every gitleaks-lineage rule (including GitLab's push-protection subset)
  compiles under the Rust `regex` crate essentially verbatim. What must be
  carried alongside the patterns is the surrounding machinery: keyword
  prefilters, entropy floors, placeholder/stopword allowlists.
- **`aho-corasick`**: the natural implementation of keyword prefiltering —
  one automaton over all rules' anchors decides which regexes run at all
  (both gitleaks and trufflehog use exactly this shape).
- **Vectorscan bindings** (`vectorscan-rs`, built for Nosey Parker):
  SIMD multi-pattern matching, warranted at repo-scale GB/s throughput,
  overkill for per-mutation text fields with a C++ vendored build.
- **`secrecy` + `zeroize`**: wrap matched spans so `Debug` output is
  redacted, `Serialize` is absent by default, and memory is wiped on drop
  ([secrecy docs](https://docs.rs/secrecy/latest/secrecy/)) — or better,
  never materialize the value at all and keep only rule ID + byte range.
- No maintained gitleaks-as-a-library port exists in Rust; the practical
  route is a small curated rule table atop `regex`/`aho-corasick`, with
  ripsecrets' p-value logic vendorable if a statistical advisory tier is
  wanted.

## 6. Design conclusions (as adopted by ADR-014)

1. **Block only on structure.** Both production hard-reject systems and
   the NDSS study agree: precision-vetted, provider-structured patterns
   are the only defensible blocking signal; entropy and generic detection
   stay advisory. Modern prefix engineering makes this cover most of the
   real token population, and CRC32-checksummed formats admit offline
   confirmation to near-zero false positives.
2. **Never echo the value.** Type + field + location in the rejection;
   redaction-by-default inside the scanner (secretlint's report-masking,
   `secrecy`-style types).
3. **Escape hatch or extinction.** Every surviving blocking tool ships a
   per-finding, auditable suppression (reason-coded bypass, fingerprint
   file, checksum-bound ignore). A gate without one gets disabled
   wholesale.
4. **Offline means no verification tier.** trufflehog/Kingfisher-style
   API validation both requires egress and ships the candidate secret to a
   third party; the offline substitutes are checksum validation and
   generation-time rule fixtures.
5. **The performance pattern is settled**: keyword prefilter →
   set/alternation regex pass → per-rule span recovery; linear-time
   engine; small curated rule count. At bead-rs field sizes (≤4 MiB, and
   typically bytes) this is microseconds, but the shape should still be
   benchmarked, not assumed.
