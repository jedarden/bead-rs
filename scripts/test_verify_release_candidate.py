#!/usr/bin/env python3
"""Value-free tar safety and integrity witnesses for candidate retention."""

import hashlib
import importlib.util
import io
from pathlib import Path
import tarfile
import unittest
import tempfile

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

    def test_real_target_filename_with_underscore(self):
        name = 'bead-managed-x86_64-unknown-linux-gnu'
        manifest = (hashlib.sha256(self.content).hexdigest() + '  ' + name + '\n').encode()
        stream = self.archive([(name, self.content, tarfile.REGTYPE),
                               ('checksums.txt', manifest, tarfile.REGTYPE)])
        candidate.verify(stream, manifest)

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

    def test_staging_verified_bytes_only_and_nonempty_output_refused(self):
        with tempfile.TemporaryDirectory() as temporary:
            output = Path(temporary)
            candidate.verify(self.archive(self.entries()), self.manifest, output)
            self.assertEqual((output / 'install.sh').read_bytes(), self.content)
            self.assertEqual((output / 'checksums.txt').read_bytes(), self.manifest)
            self.assertEqual((output / 'install.sh').stat().st_mode & 0o777, 0o600)
            with self.assertRaises(ValueError):
                candidate.verify(self.archive(self.entries()), self.manifest, output)

    def test_unsafe_entry_never_staged_outside_owned_directory(self):
        with tempfile.TemporaryDirectory() as temporary:
            output = Path(temporary) / 'owned'
            output.mkdir()
            for name in ('../outside', '/outside'):
                stream = self.archive([(name, b'x', tarfile.REGTYPE)])
                with self.assertRaises(ValueError):
                    candidate.verify(stream, self.manifest, output)
            self.assertEqual(list(output.iterdir()), [])


if __name__ == '__main__':
    unittest.main(verbosity=2)
