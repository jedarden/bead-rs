# ADR-014: Hard-Reject Mutations That Would Publish a Detectable Secret

**Status**: Accepted

**Date**: 2026-09-03

**Decision-makers**: bead-rs maintainers

## Context

A successful semantic mutation in bead-rs does not stay local. Since ADR-003
and R026, the committed SQLite transaction automatically publishes the
Git-trackable checkpoint (`.beads/checkpoint/current.json`,
`forensic.jsonl`, `objects/*.jsonl`). In every fleet workspace those files
are committed and pushed to Forgejo, and most repositories carry a
server-side push mirror to a **public GitHub repository**. A credential
pasted into a bead description therefore travels, without any further human
decision, from one CLI invocation into public git history — where removal
requires history rewriting, mirror coordination, and credential rotation,
not a `git revert`.

This is not hypothetical in the environment bead-rs serves:

- **2026-08**: a fleet worker pasted a live GitHub OAuth token into a
  Markdown notes file; only Forgejo's pre-receive scanner caught it, after
  the value had already been durably committed locally.
- **2026-09-01**: a full-history secret scan across six public repositories
  found live credentials inside committed `.beads/traces/` artifacts,
  including a web-push private key whose compromise was *proven* (the public
  key derived from the leaked private key matched the deployed public key)
  and which had to be rotated. Those blobs were harness trace files rather
  than semantic bead fields, but they establish two facts that bear directly
  on this decision: the agents that drive `bead` routinely hold live
  credential values in the text they produce, and the channel bead-rs
  publishes into is public.

Every existing defense sits *outside* bead-rs and fails open. The operator's
agent harness runs a PreToolUse hook with curated credential patterns — but
it guards one harness, parses heuristically, and by its own documentation
"fails open — if it cannot parse its input it allows the call." Forgejo's
pre-receive scan fires only at push time, after the value is already in the
local database, the local checkpoint, and local git commits; and not every
workspace pushes through a guarded remote. bead-rs itself has exactly one
stance on secrets today, and it is about *output*, not *content*: plan §5.2
requires that diagnostics "not expose SQL, secrets, environment values, or
backtraces." Nothing at all governs what the store will *accept*.

The mutation boundary is the last point where rejection still prevents
durable publication entirely. One design fact makes it the *only* point:
R006's semantic backup completeness proof requires the checkpoint to
faithfully represent the store. bead-rs cannot accept a secret into SQLite
and then redact it at flush time — a checkpoint that diverges from the
database by design would break restore equivalence, semantic comparison, and
every completeness guarantee the recovery chain rests on. Either the value
is refused before the transaction commits, or bead-rs has published it.

The free-text surface a mutation can carry: title (≤ 4 KiB), description,
notes, close reason, comment body, structured data documents, and bulk
manifest contents (each ≤ 4 MiB), plus short strings — labels, actor,
assignee, external-reference values, attempt-resolution reason and evidence
references.

Detection technique is a genuine choice with well-mapped prior art (surveyed
in [`docs/research/secret-scanning-prior-art.md`](../research/secret-scanning-prior-art.md)):

- **High-precision provider patterns** — regexes for deliberately
  identifiable token formats (GitHub `ghp_`/`gho_`/`github_pat_`, AWS
  `AKIA…` and exact `AWS_SECRET_ACCESS_KEY` assignments, Slack `xox…`,
  Anthropic `sk-ant-…`, PEM private-key armor, and peers). Providers
  engineered these formats precisely so scanners could
  match them with near-zero false positives; GitHub's push protection blocks
  pushes on this class alone.
- **Entropy / statistical detection** — Shannon-entropy thresholds
  (detect-secrets) or randomness p-values (ripsecrets) over candidate
  strings. Catches unstructured secrets, but false-positives on exactly the
  strings bead content is dense with: git SHAs, checkpoint object roots,
  bead IDs, URN schema identities, base64 fingerprints, hash-shaped
  evidence references.
- **Live verification** — trufflehog-style API calls that test whether a
  candidate credential is real. Requires network egress of the candidate
  value to a third party.

## Decision

bead-rs gains a built-in, offline, deterministic secret scanner, and **every
semantic mutation that carries operator-supplied text is scanned before its
transaction commits. A high-confidence finding hard-rejects the entire
mutation**: nothing is committed, nothing is published, exit 2 with the
existing content-validation error family, a stable machine reason code
(`secret_detected`), and a diagnostic that names the detector, the field,
and the byte range — **never the matched value**.

The scanner has exactly two tiers, and only one of them rejects:

1. **Blocking tier — identifiable secret formats only.** A curated,
   versioned ruleset of provider-prefixed token patterns and private-key
   armor headers, **baked into the binary and closed**: no workspace,
   config key, environment variable, or invocation can add, remove, or
   alter a rule. The only channel that changes the ruleset is a bead-rs
   release. Placeholder-shaped matches
   (all-one-character bodies, `example`/`REPLACE`/`YOUR_…_HERE` markers)
   pass. Where a format defines an embedded checksum (GitHub and npm
   tokens carry a base62-encoded CRC32 of the random portion, designed
   for exactly this offline confirmation), the rule validates it: a
   candidate that fails its format's checksum is a lookalike, and
   downgrades to advisory instead of blocking. This tier follows GitHub
   push protection's promotion criterion: a pattern earns the right to
   block only if its false-positive rate is near zero, because a blocking
   gate that misfires trains its users to bypass it.
2. **Advisory tier — everything statistical.** Entropy or randomness
   scoring over candidate strings never rejects a mutation. It surfaces
   through `bead doctor` (scanning stored rows for pre-existing findings)
   and through R022 dry-run output, as diagnostics.

Escape hatch, narrow and audited: a finding can be acknowledged only by its
**SHA-256 fingerprint** — `--acknowledge-secret <fingerprint>` on the
rejected command, or a persistent `secret_scan.acknowledged` fingerprint
list in `.beads/config.json`. The config stores fingerprints, never values.
Every acknowledged pass is recorded in the audit event stream with the
fingerprint and rule ID, so a bypass is a queryable event, not a silent
one. There is **no** `--no-secret-scan` flag; a blanket per-invocation
bypass becomes the routine reflex (the `--no-verify` failure mode) and
defeats the gate exactly when it matters.

Enforcement mode is workspace configuration with a hard default:
`secret_scan.mode` = `enforce` (default) | `advisory` | `off`. The effective
mode and the ruleset version are reported by `bead capabilities` and by
`doctor`, so a fleet auditor can see a weakened workspace without reading
its config.

Asymmetries, deliberate:

- **Recovery paths never reject.** `sync import-only`, `restore`, and
  archaeology ingest history that already exists in git; refusing to
  restore it protects nothing and would turn a past mistake into a bricked
  workspace. These paths scan and *report* findings instead.
- **Rejection is not containment.** By the time bead-rs reads the value it
  has already been in the process argv — visible to `ps`, shell history,
  and harness transcripts. Per ADR-007 the rejection message names the real
  remedy: treat the value as exposed, rotate it, and store the *reference*
  (a vault path or retrieval command), not the credential.
- **The scanner is fail-closed about its own configuration.** An
  unrecognized `secret_scan.mode` value or a malformed acknowledgment
  list fails the mutation with an error naming the config key, consistent
  with the plan's "unknown or contradictory state fails closed." The
  ruleset itself has no configuration surface and cannot fail at runtime:
  it is compiled into the binary and exercised by build-time tests.

No network, ever. Live verification is rejected outright: sending a
candidate secret to a third-party API to see whether it works is itself the
exfiltration this decision exists to prevent, and it would make mutation
outcomes nondeterministic and network-dependent in a tool whose contract is
local-first.

## Rationale

**Why the mutation boundary and not publication, push, or CI:** R006 forbids
a checkpoint that diverges from the database, so redact-at-flush is
structurally unavailable; push-time and CI-time scanning already exist in
the environment and demonstrably fire only after local durable commitment;
and external layers guard one transport each, while the mutation boundary
guards them all — including workspaces that never push through a guarded
remote.

**Why only identifiable formats block:** the blocking tier's authority
depends on its precision. GitHub runs push protection exclusively on
high-confidence provider patterns ("token types that can be detected
accurately … a signal-to-noise ratio that developers can trust") and keeps
everything statistical out of the blocking path — even its AI-based
generic-password detection is alert-only; GitLab's pre-receive gate makes
the same restriction for the same stated reason. False positives in a
blocking gate block legitimate work and erode compliance. bead-rs content is *adversarially bad* for entropy
scoring — a typical bead carries commit SHAs, checkpoint roots, and
hash-shaped IDs that are indistinguishable from random. An entropy tier that
rejected would misfire on routine factory traffic within hours, and every
misfire teaches an agent or operator to reach for the bypass. Provider
prefixes, by contrast, were designed by their issuers to be matched
(GitHub's token-format redesign added prefixes and a checksum specifically
for detectability). The cost of this choice is honest: an unstructured
password with no recognizable shape passes the blocking tier. The advisory
tier and the environment's outer layers exist for that class.

**Why fingerprint-scoped acknowledgment:** legitimate bead text does
occasionally quote token-shaped strings — a bead *about* rotating a leaked
credential may cite the revoked value's format, and conformance fixtures
need format-valid samples. GitHub solves this with an audited bypass flow;
the fingerprint variant is stricter: it authorizes one exact value, cannot
be pre-broadened into a standing exemption for a pattern, keeps the value
itself out of config and argv, and leaves an audit event. A rule-ID or
field-level exemption was rejected as too coarse — "descriptions are
exempt" is indistinguishable from "off".

**Why built-in rather than an external scanner:** shelling out to gitleaks
or trufflehog would gate every mutation on a third-party binary's presence
and version, fail open when it is absent, and import a rule format bead-rs
does not control. ADR-013 accepted a `git` dependency for a *read-only
report*; a *blocking gate* on the hot mutation path deserves the opposite
bias. The ruleset is small (tens of rules, not gitleaks' hundreds), the
matching machinery (keyword prefilter, then anchored regex, then placeholder
check) is a few hundred lines against crates already in the dependency
tree's neighborhood, and Rust's regex engine shares RE2's
no-backtracking model that the gitleaks ruleset already targets — rules
port without lookaround surprises and scan in linear time, which also
bounds the cost of scanning a hostile 4 MiB field.

**Why the ruleset is baked in with no local rule channel:** three reasons,
each sufficient alone. *Determinism*: a scan verdict must be a pure
function of binary version and input text. Fleet workspaces are shared
checkouts driven by many harnesses; a mutation rejected on one machine and
accepted on another — because their configs carry different rules — makes
the gate's guarantee machine-local and its behavior undiagnosable.
*Precision discipline*: every blocking rule earns its place through
generation-time true/false-positive fixtures (the gitleaks and Nosey
Parker practice). A config-supplied regex never passes that bar, and one
sloppy rule misfiring on routine traffic erodes the trust the entire
blocking tier depends on. *Threat boundary*: `.beads/config.json` is
writable by the same automated actors the gate polices. A config-level
rule channel — even an additive-only one — would let an agent install a
match-everything rule and deny every mutation in the workspace, or land
rule changes no release review ever saw. GitHub does offer custom
push-protection patterns, but they are administered by an org security
team behind an administrative boundary that a bead-rs workspace directory
does not have; the equivalent boundary here is the release process, so
that is where rules live. Mode and acknowledgments remain configurable
precisely because they are visible (capabilities, doctor) and audited;
rules would silently reshape what blocks.

**Why default-on:** a scanner that ships off protects the workspaces that
least need it. The workspaces most at risk — unattended fleet workers — are
exactly those that will never run an opt-in. `advisory` and `off` exist
because bead-rs is a general tool and an operator may have an outer gate
they trust more; the capability document makes that choice visible instead
of silent.

## Consequences

### Benefits

- A credential can no longer travel from one CLI invocation into a public
  mirror through bead-rs; the failure now requires a deliberate, audited,
  per-value acknowledgment.
- The guarantee is uniform across every harness, transport, and workspace,
  instead of depending on which hooks a particular agent runs.
- Scan verdicts are a pure function of binary version and input text:
  reproducible on any machine, diagnosable from the ruleset version alone,
  and immune to per-workspace rule drift.
- Findings never echo the value, so the rejection path cannot itself become
  the leak (unlike ad-hoc scanners that print the match).
- Doctor's advisory scan gives the fleet a way to *find* pre-existing
  stored secrets instead of discovering them in a public-repo audit.

### Drawbacks

- False rejections on legitimately quoted token-shaped text require a
  fingerprint acknowledgment step; documentation-heavy workflows will feel
  it. Mitigated by placeholder heuristics and the narrow blocking tier.
- The ruleset ages: a new provider format is invisible until a bead-rs
  release ships the rule, and there is deliberately no local override to
  bridge the gap. Accepted — a stale high-precision ruleset still catches
  the dominant formats, and rule delivery rides the same release-and-update
  cadence the fleet already runs for the binary itself.
- The blocking tier misses unstructured secrets by design. The decision
  trades recall for a gate that can be trusted; the advisory tier reports
  what the blocking tier will not reject.
- Every mutation pays a scan. With keyword prefiltering the common case
  (no keyword hit) is a single multi-pattern pass over small fields; the
  §3.5.10 benchmark suite gains a scan-overhead budget so the cost stays
  measured rather than assumed.
- Conformance fixtures must contain format-valid, non-live token samples
  without tripping the environment's *other* scanners (the agent hook, the
  Forgejo pre-receive). Fixtures therefore generate samples at test time or
  carry explicit placeholder markers — never committed verbatim live-format
  bodies.

### Alternatives Considered

- **Redact or refuse at checkpoint flush, accept into SQLite**: rejected —
  violates R006 completeness; the secret would still be durable in the
  database and in any restored copy.
- **Entropy-based blocking**: rejected for the blocking tier — false
  positive density on hash-shaped bead content would corrode the gate's
  authority (retained as advisory).
- **Live credential verification (trufflehog model)**: rejected — network
  egress of candidate secrets is the harm itself; nondeterministic;
  offline-hostile.
- **External scanner binary on the mutation path**: rejected — fails open
  when absent, uncontrolled version skew, third-party rule format;
  blocking gates must be self-contained.
- **Harness/hook-side enforcement only (status quo)**: rejected — per
  harness, fails open, already bypassed in practice by the incidents above.
- **Workspace-supplied custom patterns** (config-level rules, even
  additive-only): rejected — breaks verdict determinism across machines,
  bypasses the fixture-validated precision bar every baked rule must
  pass, and hands a rule-shaping channel to the agent-writable workspace
  config (a match-everything rule is a mutation denial-of-service).
  Rules change only through releases.
- **Field-level or rule-level standing exemptions**: rejected — too coarse;
  equivalent to disabling the gate for the exempted surface.
- **Scanning harness trace artifacts (`.beads/traces/`)**: out of scope —
  traces are written by external harnesses, never enter the semantic store
  or checkpoint, and are excluded from git by the workspace `.gitignore`
  template; their hygiene is a workspace/repo concern, not a mutation
  concern.

## Implementation

Per plan governance, implementation must not proceed from this ADR's prose:
a normative `secret-rejection-v1` specification under `research/specs/`
precedes code, and plan adoption assigns the work a roadmap identity
(R038) with conformance fixtures and benchmark evidence. The sketch,
non-normative:

1. `src/scan/` — rule table (`id`, `provider`, keyword anchors, anchored
   regex, placeholder window, optional checksum validator), aho-corasick
   keyword prefilter, regex confirmation, checksum/placeholder checks,
   fingerprint allowlist check. Rules port mechanically from the
   gitleaks-lineage rulesets: those are RE2-syntax by construction (no
   lookaround), so they compile under Rust's linear-time `regex` engine
   verbatim. Matched
   bytes live only in a type that redacts on `Debug`/`Display` and is
   zeroized on drop; only the fingerprint and range leave the module.
2. Wire the scan into every mutating service entry (`create`, `update`,
   `close`, `comment`, `label`, `ref`, `data set`, `attempt resolve`,
   `manifest apply`) before the transaction; R022 dry-run runs the same
   scan and reports findings without committing.
3. New reason code `secret_detected`; rejection maps to the existing
   exit-2 content-validation family; message text follows ADR-007 (names
   the field, the rule, the acknowledgment mechanism, and the
   rotate-and-reference remedy).
4. `bead capabilities` gains `secret_scan: {mode, ruleset_version}`
   following the ADR-012 versioned-capability pattern; `doctor` reports
   mode, ruleset version, and advisory findings over stored rows.
5. Audit events for acknowledged findings (`fingerprint`, `rule_id`,
   `field`, `actor`).
6. §3.5.10 benchmarks gain a scan-overhead lane; release gate includes the
   fixture suite proving both rejection and the placeholder/acknowledgment
   pass paths.

## Related

- [ADR-003](003-automatic-checkpoint-flush-gated-on-incremental-publication.md) — automatic publication is what makes acceptance equal publication
- [ADR-007](007-cli-errors-name-the-remedy.md) — rejection diagnostics name the remedy
- [ADR-012](012-capability-gated-attempt-contract-rollout.md) — capability-gated rollout pattern
- [ADR-013](013-read-only-git-reachability-reporting.md) — the dependency-bias precedent this ADR inverts for a blocking gate
- Plan §5.2 (exit taxonomy; no secrets in diagnostics), §6 (checkpoint semantics), R006 (semantic completeness), R021/R022 (lint and dry-run surfaces)
- [Secret-scanning prior art survey](../research/secret-scanning-prior-art.md)

## Supersedes

None.
