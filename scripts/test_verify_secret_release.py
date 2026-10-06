#!/usr/bin/env python3
"""Independent negative/positive checks of the promotion guard, no secrets."""

import copy
import importlib.util
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

MODULE = importlib.util.spec_from_file_location('guard', Path(__file__).with_name('verify-secret-release.py'))
guard = importlib.util.module_from_spec(MODULE)
MODULE.loader.exec_module(guard)


class PromotionGuard(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        (self.root / 'research/specs').mkdir(parents=True)
        (self.root / 'research/specs/secret-ruleset-v4.md').write_text('unit-test specification\n')
        (self.root / 'docs').mkdir()
        (self.root / 'docs/evidence.md').write_text('unit-test evidence\n')
        self.evidence = {'path': 'docs/evidence.md', 'sha256': guard.sha256(self.root / 'docs/evidence.md')}
        names = ['bead-x86_64-unknown-linux-gnu', 'bead-aarch64-unknown-linux-gnu',
                 'bead-managed-x86_64-unknown-linux-gnu', 'bead-managed-aarch64-unknown-linux-gnu',
                 'install.sh', 'provenance.json']
        (self.root / 'docs/checksums.txt').write_text(''.join('0' * 64 + '  ' + name + '\n' for name in names))
        self.checksums = {'path': 'docs/checksums.txt', 'sha256': guard.sha256(self.root / 'docs/checksums.txt')}
        self.record = {
            'schema_version': 1, 'version': '0.2.7', 'source_tree_sha256': 'unit-test-tree',
            'owning_bead': 'beadrs-b3059276',
            'candidate': {'source_commit': '1' * 40,
                          'payload_ref': 'ronaldraygun/bead-rs-ci-cargo-cache@sha256:' + '2' * 64,
                          'checksums': self.checksums},
            'independent_review': dict(self.evidence, decision='accepted', scope='complete_contract',
                                       reviewer='independent reviewer', authors=['author'],
                                       spec_sha256=guard.sha256(self.root / 'research/specs/secret-ruleset-v4.md')),
            'gates': {name: dict(self.evidence, status='passed') for name in
                      ('BR-T35', 'BR-T44', 'managed-write-boundary', 'redaction-conformance', 'fleet-git-layer-parity')},
            'fleet_replay': {'undispositioned_findings': 0, 'outstanding_false_positives': 0,
                            'hostile_field_overhead_ratio': 2,
                            'hosts': [{'host': host, 'reachable_workspaces': 2, 'scanned_workspaces': 2,
                                       'ruleset3_advisory_count': 100, 'ruleset4_advisory_count': 10}
                                      for host in ('lab', 'codinghome')]},
        }

    def validate(self, record):
        guard.validate(self.root, record, '0.2.7', 'unit-test-tree')

    def test_complete_unit_receipt(self):
        self.validate(self.record)

    def test_wrong_tree_version_or_owner_refused(self):
        for key in ('source_tree_sha256', 'version', 'owning_bead'):
            record = copy.deepcopy(self.record)
            record[key] = 'wrong'
            with self.assertRaises(guard.Refused):
                self.validate(record)

    def test_self_review_partial_scope_and_wrong_spec_refused(self):
        for key, value in (('reviewer', 'author'), ('scope', 'raw_ranges_only'), ('decision', 'rejected'), ('spec_sha256', 'wrong')):
            record = copy.deepcopy(self.record)
            record['independent_review'][key] = value
            with self.assertRaises(guard.Refused):
                self.validate(record)

    def test_evidence_missing_tampered_or_traversal_refused(self):
        for key, value in (('path', 'docs/missing.md'), ('path', 'docs/../../outside.md'), ('sha256', '0' * 64)):
            record = copy.deepcopy(self.record)
            record['gates']['BR-T44'][key] = value
            with self.assertRaises(guard.Refused):
                self.validate(record)

    def test_incomplete_replay_and_volume_refused(self):
        for field, value in (('undispositioned_findings', 1), ('outstanding_false_positives', 1),
                             ('hostile_field_overhead_ratio', 3.01), ('hosts', [])):
            record = copy.deepcopy(self.record)
            record['fleet_replay'][field] = value
            with self.assertRaises(guard.Refused):
                self.validate(record)
        for key, value in (('scanned_workspaces', 1), ('ruleset4_advisory_count', 11)):
            record = copy.deepcopy(self.record)
            record['fleet_replay']['hosts'][0][key] = value
            with self.assertRaises(guard.Refused):
                self.validate(record)

    def test_each_required_gate_refused_when_incomplete(self):
        for name in self.record['gates']:
            record = copy.deepcopy(self.record)
            del record['gates'][name]
            with self.assertRaises(guard.Refused):
                self.validate(record)

    def test_boolean_counts_and_duplicate_hosts_refused(self):
        for field in ('undispositioned_findings', 'outstanding_false_positives'):
            record = copy.deepcopy(self.record)
            record['fleet_replay'][field] = False
            with self.assertRaises(guard.Refused):
                self.validate(record)
        record = copy.deepcopy(self.record)
        record['fleet_replay']['hosts'].append(record['fleet_replay']['hosts'][0])
        with self.assertRaises(guard.Refused):
            self.validate(record)

    def test_missing_mutable_foreign_or_unbound_candidate_refused(self):
        for key, value in (('source_commit', 'HEAD'), ('payload_ref', 'repository:mutable'),
                           ('payload_ref', 'elsewhere@sha256:' + '2' * 64), ('checksums', self.evidence)):
            record = copy.deepcopy(self.record)
            record['candidate'][key] = value
            with self.assertRaises(guard.Refused):
                self.validate(record)
        record = copy.deepcopy(self.record)
        del record['candidate']
        with self.assertRaises(guard.Refused):
            self.validate(record)

    def test_candidate_missing_profile_or_duplicate_asset_refused(self):
        path = self.root / 'docs/checksums.txt'
        original = path.read_text()
        for content in (''.join(original.splitlines(keepends=True)[1:]),
                        original + original.splitlines(keepends=True)[0]):
            path.write_text(content)
            record = copy.deepcopy(self.record)
            record['candidate']['checksums']['sha256'] = guard.sha256(path)
            with self.assertRaises(guard.Refused):
                self.validate(record)

    def test_tree_identity_excludes_only_checkpoint_and_release_evidence(self):
        class Result:
            stdout = b''

        result = Result()
        source = b'100644 blob ' + b'1' * 40 + b'\tsrc/main.rs\n'
        evidence = b'100644 blob ' + b'2' * 40 + b'\tdocs/releases/evidence.md\n'
        checkpoint = b'100644 blob ' + b'3' * 40 + b'\t.beads/checkpoint/current.json\n'
        with patch.object(guard.subprocess, 'run', return_value=result) as command:
            result.stdout = source
            original = guard.source_tree_hash(self.root)
            result.stdout = source + evidence + checkpoint
            self.assertEqual(guard.source_tree_hash(self.root, '4' * 40), original)
            self.assertEqual(command.call_args.args[0][-1], '4' * 40)
            result.stdout = source.replace(b'1', b'5') + evidence
            self.assertNotEqual(guard.source_tree_hash(self.root), original)
            result.stdout = source + evidence.replace(b'docs/releases/', b'docs/reviews/')
            self.assertNotEqual(guard.source_tree_hash(self.root), original)


if __name__ == '__main__':
    unittest.main(verbosity=2)
