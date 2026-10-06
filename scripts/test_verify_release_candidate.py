#!/usr/bin/env python3
"""Value-free tar safety and integrity witnesses for candidate retention."""

import hashlib
import importlib.util
import io
from pathlib import Path
import tarfile
import unittest

MODULE = importlib.util.spec_from_file_location('candidate', Path(__file__).with_name('verify-release-candidate.py'))
candidate = importlib.util.module_from_spec(MODULE)
MODULE.loader.exec_module(candidate)


class CandidateArchive(unittest.TestCase):
    def setUp(self):
        self.content = b'unit-test artifact\n'
        self.manifest = (hashlib.sha256(self.content).hexdigest() + '  install.sh\n').encode()

    def archive(self, entries):
        stream = io.BytesIO()
        with tarfile.open(fileobj=stream, mode='w') as archive:
            for name, content, kind in entries:
                member = tarfile.TarInfo(name)
                member.type = kind
                member.size = len(content) if kind == tarfile.REGTYPE else 0
                if kind == tarfile.SYMTYPE:
                    member.linkname = '/outside'
                archive.addfile(member, io.BytesIO(content) if member.isfile() else None)
        stream.seek(0)
        return stream

    def entries(self):
        return [('./install.sh', self.content, tarfile.REGTYPE),
                ('./checksums.txt', self.manifest, tarfile.REGTYPE)]

    def test_complete_regular_archive(self):
        candidate.verify(self.archive(self.entries()), self.manifest)

    def test_missing_duplicate_traversal_symlink_and_wrong_bytes(self):
        cases = [self.entries()[:1], self.entries() + self.entries()[:1],
                 self.entries() + [('../outside', b'x', tarfile.REGTYPE)],
                 [('install.sh', b'', tarfile.SYMTYPE), self.entries()[1]],
                 [('install.sh', b'wrong', tarfile.REGTYPE), self.entries()[1]]]
        for entries in cases:
            with self.assertRaises(ValueError):
                candidate.verify(self.archive(entries), self.manifest)

    def test_malformed_and_duplicate_manifest(self):
        for manifest in (b'not a manifest\n', self.manifest + self.manifest):
            with self.assertRaises(ValueError):
                candidate.verify(self.archive(self.entries()), manifest)


if __name__ == '__main__':
    unittest.main(verbosity=2)
