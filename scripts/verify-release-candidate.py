#!/usr/bin/env python3
"""Verify an OCI candidate tar stream without extracting untrusted paths."""

import hashlib
from pathlib import Path
import re
import sys
import tarfile


def verify(stream, manifest):
    expected = {'checksums.txt': hashlib.sha256(manifest).hexdigest()}
    for line in manifest.decode('ascii').splitlines():
        match = re.fullmatch(r'([0-9a-f]{64})  (bead-[a-z0-9-]+|install\.sh|provenance\.json)', line)
        if not match or match[2] in expected:
            raise ValueError('invalid manifest')
        expected[match[2]] = match[1]
    seen = set()
    with tarfile.open(fileobj=stream, mode='r|*') as archive:
        for member in archive:
            if member.isdir() and member.name in ('.', './'):
                continue
            name = member.name.removeprefix('./')
            if not member.isfile() or name not in expected or name in seen:
                raise ValueError('unexpected archive entry')
            # Bound the artifact payload independently of the tar header.
            if member.size <= 0 or member.size > 128 * 1024 * 1024:
                raise ValueError('invalid artifact size')
            content = archive.extractfile(member)
            digest = hashlib.file_digest(content, 'sha256').hexdigest()
            if digest != expected[name]:
                raise ValueError('candidate byte mismatch')
            seen.add(name)
    if seen != set(expected):
        raise ValueError('incomplete candidate archive')


def main():
    if len(sys.argv) != 2:
        return 2
    try:
        verify(sys.stdin.buffer, Path(sys.argv[1]).read_bytes())
    except (ValueError, OSError, UnicodeError, tarfile.TarError):
        print('Candidate archive refused: incomplete, unsafe, or mismatched payload', file=sys.stderr)
        return 1
    print('Remote candidate payload matches the complete local checksum manifest')
    return 0


if __name__ == '__main__':
    sys.exit(main())
