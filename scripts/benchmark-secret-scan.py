#!/usr/bin/env python3
"""Compare exact ruleset-3/4 artifacts on the same owned 4 MiB field.

This measures end-to-end native `doctor --scope secrets` wall and child-CPU
time, including process/store overhead, not a pure in-process microbenchmark.
No real workspace, secret value, external scanner, or installed CLI is used.
Run again against the final candidate; calibration is not release acceptance.
"""
import argparse
from contextlib import closing
import hashlib
import json
import os
from pathlib import Path
import resource
import socket
import sqlite3
import statistics
import subprocess
import tempfile
import time
import uuid


def require(condition, message):
    if not condition:
        raise RuntimeError(message)


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def command(binary, root, *arguments):
    result = subprocess.run([str(binary), *map(str, arguments)], cwd=root,
                            env={**os.environ, 'BEAD_ORG_SECRET_SCANNER': 'off'},
                            stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                            timeout=60, check=False)
    require(result.returncode == 0, 'artifact command failed; candidate output withheld')
    return result.stdout


def validate_diagnostic(output):
    document = json.loads(output)
    checks = [check['details'] for check in document['checks']
              if check['name'] == 'secret_scan']
    require(len(checks) == 1 and checks[0]['blocking_findings'] == 0
            and checks[0]['advisory_findings'] == 0,
            'hostile clean field did not produce the expected empty finding set')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('baseline', type=Path)
    parser.add_argument('candidate', type=Path)
    parser.add_argument('--baseline-sha256', required=True)
    parser.add_argument('--candidate-sha256', required=True)
    parser.add_argument('--samples', type=int, default=7)
    parser.add_argument('--scratch-root', type=Path, default=Path('/home/coding/scratch'))
    args = parser.parse_args()
    require(3 <= args.samples <= 31, 'samples must be between 3 and 31')
    binaries = [args.baseline.resolve(strict=True), args.candidate.resolve(strict=True)]
    hashes = [args.baseline_sha256, args.candidate_sha256]
    for binary, expected in zip(binaries, hashes):
        require(len(expected) == 64 and digest(binary) == expected,
                'artifact identity does not match the supplied checksum')
    measurements = [{'wall_seconds': [], 'cpu_seconds': []} for _ in binaries]
    versions = []
    with tempfile.TemporaryDirectory(prefix='bead-scan-cost-', dir=args.scratch_root) as temporary:
        roots = [Path(temporary) / 'baseline', Path(temporary) / 'candidate']
        for index, (binary, root) in enumerate(zip(binaries, roots)):
            root.mkdir(mode=0o700)
            (root / '.beads').mkdir(mode=0o700)
            workspace_uuid = str(uuid.uuid4())
            config = root / '.beads/config.json'
            config.write_text(json.dumps({'version': 1, 'uuid': workspace_uuid, 'prefix': 'cost'}))
            config.chmod(0o600)
            capabilities = json.loads(command(binary, root, 'capabilities'))['secret_scan']
            require(capabilities['ruleset_version'] == (3 if index == 0 else 4),
                    'expected ruleset-3 baseline and ruleset-4 candidate')
            versions.append(command(binary, root, '--version').decode().strip())
            command(binary, root, 'init', '--prefix', 'cost', '--no-auto-flush')
            with closing(sqlite3.connect(root / '.beads/beads.db')) as connection:
                require(connection.execute('SELECT uuid FROM workspace').fetchone()[0]
                        == workspace_uuid, 'initialization escaped its isolated store')
                connection.execute(
                    "INSERT INTO issues (id,title,description,priority,issue_type,base_status,"
                    "created_at,updated_at,revision) VALUES ('cost-1','stable',?,2,'task','open',"
                    "'2026-10-06T00:00:00Z','2026-10-06T00:00:00Z',1)",
                    ('z' * (4 * 1024 * 1024),))
                connection.commit()
            validate_diagnostic(command(binary, root, 'doctor', '--scope', 'secrets', '--format', 'json'))
        # Interleave order to reduce sensitivity to changing host load/cache.
        for sample in range(args.samples):
            for index in ([0, 1] if sample % 2 == 0 else [1, 0]):
                before_cpu = resource.getrusage(resource.RUSAGE_CHILDREN)
                started = time.monotonic()
                output = command(binaries[index], roots[index], 'doctor', '--scope', 'secrets', '--format', 'json')
                elapsed = time.monotonic() - started
                after_cpu = resource.getrusage(resource.RUSAGE_CHILDREN)
                cpu = (after_cpu.ru_utime + after_cpu.ru_stime
                       - before_cpu.ru_utime - before_cpu.ru_stime)
                validate_diagnostic(output)
                measurements[index]['wall_seconds'].append(elapsed)
                measurements[index]['cpu_seconds'].append(cpu)
        for binary, expected in zip(binaries, hashes):
            require(digest(binary) == expected, 'artifact changed during measurement')
    require(all(statistics.median(measurement[key]) > 0
                for measurement in measurements for key in ('wall_seconds', 'cpu_seconds')),
            'measurement resolution was insufficient')
    result = {
        'host': socket.gethostname(), 'field_bytes': 4 * 1024 * 1024,
        'method': 'interleaved native doctor CLI; process/store overhead included',
        'external_scanner': 'not exercised', 'samples': args.samples,
        'artifacts': [{'sha256': expected, 'version': version, **measurement}
                      for expected, version, measurement in zip(hashes, versions, measurements)],
        'wall_overhead_ratio': (statistics.median(measurements[1]['wall_seconds'])
                                / statistics.median(measurements[0]['wall_seconds'])),
        'cpu_overhead_ratio': (statistics.median(measurements[1]['cpu_seconds'])
                               / statistics.median(measurements[0]['cpu_seconds'])),
    }
    print(json.dumps(result, sort_keys=True))


if __name__ == '__main__':
    try:
        main()
    except (RuntimeError, OSError, ValueError, KeyError, subprocess.SubprocessError):
        raise SystemExit('Secret-scan benchmark failed; output withheld.')
