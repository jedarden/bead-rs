#!/usr/bin/env python3
"""Exercise one managed binary in an owned disposable native store only.

Historical fixtures are seeded directly into this test database, never through
a prevention bypass and never into a user's store. Invented provider-shaped
bytes stay in private files or SQLite parameters, not argv or diagnostic output.
This smoke is not independent review, full conformance, or fleet replay.
"""

import argparse
from contextlib import closing
import hashlib
import json
import os
from pathlib import Path
import re
import sqlite3
import subprocess
import sys
import tempfile
import uuid


class Refused(Exception):
    pass


def require(condition, message):
    if not condition:
        raise Refused(message)


def smoke(binary, scratch_root):
    binary = binary.resolve(strict=True)
    require(binary.is_file() and os.access(binary, os.X_OK), 'binary is not executable')
    with binary.open('rb') as executable:
        original_digest = hashlib.file_digest(executable, 'sha256').hexdigest()
    # Assembled at runtime; this is an invented, nonfunctional key identifier.
    candidate = ''.join(('AK', 'IA', '7Q9W2E4R6T8Y1U3I'))
    candidate_bytes = candidate.encode()
    with tempfile.TemporaryDirectory(prefix='bead-secret-release-smoke-', dir=scratch_root) as temporary:
        root = Path(temporary)
        root.chmod(0o700)
        environment = dict(os.environ, BEAD_ORG_SECRET_SCANNER='off')

        def run(*arguments, success=True):
            result = subprocess.run([str(binary), *arguments], cwd=root, env=environment,
                                    capture_output=True, timeout=120)
            require(candidate_bytes not in result.stdout + result.stderr,
                    'command exposed fixture bytes')
            if success:
                require(result.returncode == 0, 'command failed: ' + arguments[0])
            return result

        def private_json(name, content):
            path = root / name
            with path.open('x', encoding='utf-8') as destination:
                os.chmod(path, 0o600)
                json.dump(content, destination)
            return path.name

        def query(sql):
            with closing(sqlite3.connect(root / '.beads/beads.db')) as connection:
                return connection.execute(sql).fetchall()

        def snapshot():
            with closing(sqlite3.connect(root / '.beads/beads.db')) as connection:
                return hashlib.sha256('\n'.join(connection.iterdump()).encode()).hexdigest()

        def diagnostics():
            result = run('doctor', '--scope', 'secrets', '--format', 'json', success=False)
            document = json.loads(result.stdout)
            reports = [check['details'] for check in document['checks']
                       if check.get('name') == 'secret_scan']
            require(len(reports) == 1, 'missing secret diagnostics')
            return reports[0]

        # Fence upward workspace discovery before the first binary invocation.
        # Scratch may itself have a store; `init` would otherwise resolve it.
        (root / '.beads').mkdir(mode=0o700)
        workspace_uuid = str(uuid.uuid4())
        private_json('.beads/config.json', {'version': 1, 'uuid': workspace_uuid, 'prefix': 'smoke'})
        capabilities = json.loads(run('capabilities').stdout)['secret_scan']
        require(capabilities['compiled_policy'] == 'managed-enforce-no-ack'
                and capabilities['service_write_gate'] is True
                and capabilities['exact_fingerprint_acknowledgment'] is False,
                'managed enforcement capabilities missing')
        version = run('--version').stdout.decode().strip()
        run('init', '--prefix', 'smoke', '--skip-foreign-workspace')
        require(query('SELECT uuid FROM workspace') == [(workspace_uuid,)],
                'initialization did not resolve the isolated identity')
        rejected_input = private_json('incoming.json', {
            'manifest_version': 1,
            'operations': [{'op': 'create', 'local_id': 'unsafe', 'title': 'rejection fixture',
                            'description': candidate}],
        })
        before = snapshot()
        rejected = run('manifest', 'commit', '--input', rejected_input, '--format', 'json', success=False)
        require(rejected.returncode != 0 and snapshot() == before,
                'incoming fixture was admitted or changed semantic state')
        safe_input = private_json('safe.json', {
            'manifest_version': 1,
            'operations': [{'op': 'create', 'local_id': name, 'title': name}
                           for name in ('first-smoke', 'second-smoke')],
        })
        run('manifest', 'commit', '--input', safe_input, '--format', 'json')
        original_issues = query('SELECT id,title,revision FROM issues ORDER BY title')
        require(len(original_issues) == 2, 'safe manifest did not create both beads')
        first_id = original_issues[0][0]
        with closing(sqlite3.connect(root / '.beads/beads.db')) as connection:
            connection.execute('PRAGMA foreign_keys=ON')
            connection.execute('UPDATE issues SET description=? WHERE id=?',
                               ('left ' + candidate + ' middle ' + candidate + ' right', first_id))
            connection.executemany('INSERT INTO labels(issue_id,label) VALUES (?,?)',
                                   [(row[0], candidate) for row in original_issues])
            connection.commit()
        report = diagnostics()
        selected = [finding['fingerprint'] for finding in report['findings']
                    if finding['selector'].startswith('live:')
                    and finding['rule_id'] == 'aws-access-key-id']
        require(len(selected) == 4 and len(set(selected)) == 4
                and all(re.fullmatch('[0-9a-f]{64}', fingerprint) for fingerprint in selected),
                'expected historical text and key findings missing')
        arguments = ['redact', '--actor', 'release-smoke', '--reason', 'invented fixture cleanup', '--json']
        for fingerprint in selected:
            arguments.extend(('--finding', fingerprint))
        before = snapshot()
        run(*arguments, '--dry-run')
        require(snapshot() == before, 'redaction dry run changed semantic state')

        # A real WAL reader holds physical cleanup after the semantic commit.
        # Publication and ordinary mutation must stay fenced until resume.
        with closing(sqlite3.connect(root / '.beads/beads.db')) as reader:
            reader.execute('BEGIN')
            reader.execute('SELECT COUNT(*) FROM issues').fetchone()
            interrupted = run(*arguments, success=False)
            require(interrupted.returncode != 0, 'active reader did not hold cleanup pending')
            receipts = query('SELECT receipt_id,epoch_id,publication_state FROM redaction_receipts')
            require(len(receipts) == 4 and len({row[1] for row in receipts}) == 1
                    and all(row[2] == 'committed' for row in receipts),
                    'interrupted batch was not committed atomically')
            before = snapshot()
            held = run('create', '--title', 'must remain fenced', success=False)
            require(held.returncode != 0 and snapshot() == before,
                    'pending cleanup did not fence ordinary writes')
            reader.rollback()
        run('redact', '--resume', receipts[0][0], '--json')
        sanitized = query('SELECT id,title,revision FROM issues ORDER BY title')
        require(sanitized == [(row[0], row[1], row[2] + 1) for row in original_issues],
                'batch deleted beads or advanced an issue revision more than once')
        require(query('SELECT description FROM issues ORDER BY title')[0][0]
                == 'left [REDACTED:bead-rs] middle [REDACTED:bead-rs] right',
                'text outside selected ranges changed')
        labels = query('SELECT issue_id,label FROM labels ORDER BY issue_id')
        require(len(labels) == 2 and {row[0] for row in labels} == {row[0] for row in original_issues}
                and all(row[1].startswith('redacted-') for row in labels),
                'metadata rekey lost bead bindings')
        require(query('PRAGMA foreign_key_check') == [], 'rekey left dangling references')
        require(query('SELECT publication_state FROM redaction_epochs') == [('published',)]
                and all(row[0] == 'published' for row in query('SELECT publication_state FROM redaction_receipts')),
                'resume did not publish the whole epoch')
        before = snapshot()
        run('redact', '--resume', receipts[-1][0], '--json')
        require(snapshot() == before, 'completed receipt replay changed semantic state')
        report = diagnostics()
        require(report['blocking_findings'] == 0 and report['coverage_complete'] is True
                and report['redaction_pending'] is False, 'sanitized diagnostics incomplete')
        for path in (root / '.beads').rglob('*'):
            require(not path.is_symlink(), 'unexpected fixture-store symlink')
            if path.is_file():
                require(candidate_bytes not in path.read_bytes(),
                        'fixture bytes remain in local database/WAL/checkpoint')
        checkpoint = json.loads(run('sync', 'status', '--format', 'json').stdout)
        require(checkpoint['checkpoint_consistent'] is True and checkpoint['dirty'] is False,
                'sanitized checkpoint not current')
    with binary.open('rb') as executable:
        digest = hashlib.file_digest(executable, 'sha256').hexdigest()
    require(digest == original_digest, 'binary changed during verification')
    return {'status': 'passed', 'binary_sha256': digest, 'version': version,
            'checks': ['managed-policy', 'incoming-rejection-rollback', 'dry-run', 'atomic-selected-batch',
                       'key-references', 'pending-write-fence', 'reader-resume', 'idempotence',
                       'local-byte-cleanup', 'sanitized-checkpoint'],
            'scope': 'disposable-native-fixtures-only', 'organization_scanner': 'not_exercised'}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('binary', type=Path)
    parser.add_argument('--scratch-root', type=Path, default=Path('/home/coding/scratch'))
    arguments = parser.parse_args()
    try:
        result = smoke(arguments.binary, arguments.scratch_root)
    except (Refused, OSError, ValueError, KeyError, TypeError, AttributeError, IndexError, sqlite3.Error,
            subprocess.TimeoutExpired) as error:
        message = str(error) if isinstance(error, Refused) else 'unavailable binary or malformed smoke result'
        print('Secret release smoke refused: ' + message, file=sys.stderr)
        return 1
    print(json.dumps(result, sort_keys=True))
    return 0


if __name__ == '__main__':
    sys.exit(main())
