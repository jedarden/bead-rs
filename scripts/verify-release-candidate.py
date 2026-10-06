#!/usr/bin/env python3
"""Verify an OCI candidate tar stream without extracting untrusted paths."""

import hashlib
from pathlib import Path
import re
import os
import sys
import tarfile


def verify(stream, manifest, output=None):
    expected = {'checksums.txt': hashlib.sha256(manifest).hexdigest()}
    for line in manifest.decode('ascii').splitlines():
        match = re.fullmatch(r'([0-9a-f]{64})  (bead-[a-z0-9_-]+|install\.sh|provenance\.json)', line)
        if not match or match[2] in expected:
            raise ValueError('invalid manifest')
        expected[match[2]] = match[1]
    if output is not None:
        if output.is_symlink() or not output.is_dir() or any(output.iterdir()):
            raise ValueError('output must be an empty owned directory')
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
            if output is None:
                digest = hashlib.file_digest(content, 'sha256').hexdigest()
            else:
                # Never tar-extract paths or honor permissions/link metadata.
                # Files stay unpublished in this caller-owned staging dir until
                # the entire archive has matched the approved checksum table.
                digest_state = hashlib.sha256()
                with (output / name).open('xb') as destination:
                    os.chmod(output / name, 0o600)
                    while chunk := content.read(1024 * 1024):
                        digest_state.update(chunk)
                        destination.write(chunk)
                digest = digest_state.hexdigest()
            if digest != expected[name]:
                raise ValueError('candidate byte mismatch')
            seen.add(name)
    if seen != set(expected):
        raise ValueError('incomplete candidate archive')


def main():
    if len(sys.argv) not in (2, 4) or (len(sys.argv) == 4 and sys.argv[2] != '--output-dir'):
        return 2
    try:
        output = Path(sys.argv[3]) if len(sys.argv) == 4 else None
        verify(sys.stdin.buffer, Path(sys.argv[1]).read_bytes(), output)
    except (ValueError, OSError, UnicodeError, tarfile.TarError):
        print('Candidate archive refused: incomplete, unsafe, or mismatched payload', file=sys.stderr)
        return 1
    print('Remote candidate payload matches the complete local checksum manifest')
    return 0


if __name__ == '__main__':
    sys.exit(main())
