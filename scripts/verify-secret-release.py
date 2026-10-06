#!/usr/bin/env python3
"""Fail-closed promotion check for a committed secret-scrubbing approval.

CI always builds candidates, but publication also needs independently reviewed
and hashed evidence. This checker does not generate approvals or accept its own
review. A missing/incomplete record is a release hold, not a reason to bypass it.
"""

import hashlib
import json
from pathlib import Path
import re
import subprocess
import sys


class Refused(Exception):
    pass


def require(condition, reason):
    if not condition:
        raise Refused(reason)


def sha256(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def source_tree_hash(root):
    result = subprocess.run(['git', 'ls-tree', '-r', '--full-tree', 'HEAD'],
                            cwd=root, check=True, capture_output=True)
    # Checkpoint/task activity and approval/evidence commits must not change the
    # tested source identity. Everything else, including scripts and pins, does.
    lines = [line for line in result.stdout.splitlines(keepends=True)
             if not line.split(b'\t', 1)[1].startswith((b'.beads/checkpoint/', b'docs/releases/'))]
    return hashlib.sha256(b''.join(lines)).hexdigest()


def evidence_file(root, evidence):
    require(isinstance(evidence, dict), 'evidence must be an object')
    name = evidence.get('path')
    digest = evidence.get('sha256')
    require(isinstance(name, str) and name.startswith('docs/'), 'evidence path must be repository documentation')
    require(isinstance(digest, str) and re.fullmatch(r'[0-9a-f]{64}', digest), 'evidence needs an exact SHA-256')
    path = (root / name).resolve()
    require(path.is_relative_to(root.resolve()) and path.is_file(), 'evidence path is missing or outside repository')
    require(sha256(path) == digest, 'evidence hash mismatch')
    return path


def validate(root, record, version, expected_tree):
    require(isinstance(record, dict) and type(record.get('schema_version')) is int
            and record['schema_version'] == 1, 'unsupported approval schema')
    require(record.get('version') == version, 'approval version mismatch')
    require(record.get('source_tree_sha256') == expected_tree, 'approval does not match candidate source tree')
    spec = root / 'research/specs/secret-ruleset-v4.md'
    review = record.get('independent_review', {})
    require(review.get('decision') == 'accepted' and review.get('scope') == 'complete_contract', 'complete independent acceptance required')
    require(review.get('spec_sha256') == sha256(spec), 'reviewed specification differs from candidate')
    reviewer = review.get('reviewer')
    authors = review.get('authors')
    require(isinstance(reviewer, str) and reviewer.strip() and isinstance(authors, list) and authors,
            'reviewer and author identities required')
    require(reviewer not in authors, 'reviewer must be distinct from authors')
    evidence_file(root, review)
    gates = record.get('gates', {})
    required = ('BR-T35', 'BR-T44', 'managed-write-boundary', 'redaction-conformance', 'fleet-git-layer-parity')
    for name in required:
        gate = gates.get(name, {})
        require(gate.get('status') == 'passed', 'required security/replay gate is incomplete')
        evidence_file(root, gate)
    replay = record.get('fleet_replay', {})
    require(type(replay.get('undispositioned_findings')) is int and replay['undispositioned_findings'] == 0
            and type(replay.get('outstanding_false_positives')) is int and replay['outstanding_false_positives'] == 0,
            'fleet replay has unresolved findings')
    hosts = replay.get('hosts', [])
    require(isinstance(hosts, list) and len(hosts) == 2
            and {h.get('host') for h in hosts if isinstance(h, dict)} == {'lab', 'codinghome'},
            'both target hosts require replay evidence')
    for host in hosts:
        require(type(host.get('reachable_workspaces')) is int and host['reachable_workspaces'] > 0
                and type(host.get('scanned_workspaces')) is int and host['scanned_workspaces'] == host['reachable_workspaces'],
                'not every reachable workspace was scanned')
        old, new = host.get('ruleset3_advisory_count'), host.get('ruleset4_advisory_count')
        require(type(old) is int and type(new) is int and old >= 0 and 0 <= new * 10 <= old,
                'advisory volume gate failed')
    ratio = replay.get('hostile_field_overhead_ratio')
    require(type(ratio) in (int, float) and 0 < ratio <= 3, 'hostile-field performance gate incomplete')
    require(record.get('owning_bead') == 'beadrs-b3059276', 'approval needs the user-directed release owner')


def main():
    if len(sys.argv) != 2 or re.fullmatch(r'[0-9]+\.[0-9]+\.[0-9]+', sys.argv[1]) is None:
        print('usage: verify-secret-release.py VERSION', file=sys.stderr)
        return 2
    version = sys.argv[1]
    root = Path(__file__).resolve().parent.parent
    approval = root / 'docs/releases' / ('v' + version + '-secret-scrubbing.json')
    try:
        require(approval.is_file(), 'committed release approval is missing')
        record = json.loads(approval.read_text())
        validate(root, record, version, source_tree_hash(root))
    except (Refused, ValueError, OSError, subprocess.CalledProcessError, TypeError, AttributeError, KeyError) as error:
        # Never print a JSON payload, malformed field value, or subprocess stderr.
        reason = str(error) if isinstance(error, Refused) else 'malformed approval or unavailable evidence'
        print('release approval refused: ' + reason, file=sys.stderr)
        return 1
    print('Secret-scrubbing promotion approved for v' + version)
    return 0


if __name__ == '__main__':
    sys.exit(main())
