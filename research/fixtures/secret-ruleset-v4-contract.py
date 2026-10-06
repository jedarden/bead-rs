#!/usr/bin/env python3
"""Independent, value-free contract witnesses; not the production scanner.

Provenance: invented for beadrs-74512e84 from secret-ruleset-v4.md and the
three actionable independent reviews. No other implementation or real store
supplies a fixture. Candidate strings exist only at runtime; assertions and
output carry case names, integer metrics and verdicts, never candidate bytes.
Run with Python's standard library: python3 <this-file> [--table].
These checks are specification evidence, not production/fleet conformance.
"""

import base64
import binascii
import json
import re
import sys
import unittest


def placeholder(value):
    lower = value.lower()
    return (
        any(char in value for char in '<>${}*[]()|`')
        or '...' in value or '\u2026' in value
        or len(set(value)) <= 1
        or any(word in lower for word in (
            'example', 'placeholder', 'replace', 'your_', '_here', 'dummy',
            'sample', 'xxxx', '0000000', 'redact', 'changeme', 'todo', 'fake',
            'masked',
        ))
    )


def byte_class(char):
    if 'a' <= char <= 'z':
        return 0
    if 'A' <= char <= 'Z':
        return 1
    if '0' <= char <= '9':
        return 2
    return 3


def metrics(value):
    classes = [byte_class(char) for char in value]
    alpha = sum(kind < 3 for kind in classes)
    digits = classes.count(2)
    words = cursor = 0
    patterns = (r'[a-z]{4,}', r'[A-Z]{4,}', r'[A-Z][a-z]{3,}')
    while cursor < len(value):
        for pattern in patterns:
            match = re.match(pattern, value[cursor:])
            if match:
                size = match.end()
                words += size
                cursor += size
                break
        else:
            cursor += 1
    transitions = sum(left != right for left, right in zip(classes, classes[1:]))
    return len(value.encode()), alpha, digits, words, transitions


def verdict(value, minimum):
    n, a, d, w, t = metrics(value)
    one_case_hex = (
        n >= 32
        and re.fullmatch(r'[0-9a-fA-F]+', value) is not None
        and not (re.search(r'[a-f]', value) and re.search(r'[A-F]', value))
    )
    steps = (
        1 <= n <= 512 and all(0x21 <= ord(char) <= 0x7e for char in value),
        not placeholder(value),
        d > 0 and a > d,
        one_case_hex or 5 * d < 4 * a,
        10 * w < 7 * a and a - w >= minimum,
        3 * t >= n - 1,
    )
    return next((index + 1 for index, passed in enumerate(steps) if not passed), 0)


def witnesses():
    # Every token-shaped representative is assembled, never a literal sample.
    pairs = ''.join(str(3) + chr(97) for _ in range(4))
    alternating = ''.join(chr(97) + str(3) for _ in range(20))
    hex31 = ''.join(chr(97) + str(1) for _ in range(6)) + str(1) * 19
    hex32 = hex31 + str(1)
    mixed32 = chr(65) + hex32[1:]
    word_boundary = 'abcde' + ''.join(chr(65) + str(3) for _ in range(7)) + '+' + chr(98)
    return [
        ('bead_identifier', 'ticket' + '-' + pairs),
        ('name_short_hash', 'alpha' + '-' + pairs),
        ('timestamp', '-'.join(('2026', '10', '05')) + 'T' + ':'.join(('12', '34', '56')) + 'Z'),
        ('version', 'v' + '.'.join(('12', '34', '56')) + '-rc' + '7' + '.' + '89'),
        ('path', '/'.join(('', 'alpha', 'beta', 'gamma'))),
        ('camel_type', 'Alpha' + 'Beta' + 'Gamma'),
        ('snake_name', '_'.join(('alpha', 'beta', 'gamma'))),
        ('base62_40', alternating),
        ('hex_40', alternating),
        ('base64_40', ''.join(chr(97) + chr(66) + str(3) + '+' + '/' for _ in range(8))),
        ('hex_31_one_case', hex31),
        ('hex_32_one_case', hex32),
        ('hex_32_mixed_case', mixed32),
        ('maximal_word_cursor', word_boundary),
    ]


def excluded(identifier):
    folded = identifier.lower()
    return (
        folded in ('acknowledge-secret', 'fencing-token', 'fencing_token', 'max_tokens')
        or folded.startswith(('secret_scan', 'secret-scan'))
        or any(folded.endswith(separator + suffix)
               for separator in ('_', '-')
               for suffix in ('file', 'path', 'name', 'ref', 'label', 'count', 'limit', 'budget', 'usage'))
    )


def decode_run(run):
    if re.fullmatch(r'[A-Za-z0-9+/_=-]+', run) is None:
        return None
    body = run.rstrip('=')
    padding = len(run) - len(body)
    needed = (-len(body)) % 4
    if len(body) % 4 == 1 or padding > 2:
        return None
    if padding and (len(run) % 4 or padding != needed):
        return None
    try:
        decoded = base64.b64decode(body.translate(str.maketrans('-_', '+/')) + '=' * needed, validate=True)
    except binascii.Error:
        return None
    if not decoded or 10 * sum(0x20 <= byte <= 0x7e or 9 <= byte <= 13 for byte in decoded) < 9 * len(decoded):
        return None
    return decoded


def decoded_selection(text):
    selected = retained = 0
    limits = set()
    spans = []
    for match in re.finditer(r'[A-Za-z0-9+/_=-]{40,}', text):
        if selected == 64:
            limits.add('run_count_limit')
            continue
        selected += 1
        if match.end() - match.start() > 65536:
            limits.add('run_size_limit')
            continue
        if decode_run(match.group()) is not None:
            retained += 1
            spans.append((match.start(), match.end()))
    return selected, retained, sorted(limits), spans


def header_valid(segment):
    if re.fullmatch(r'[A-Za-z0-9_-]+', segment) is None or len(segment) % 4 == 1:
        return False

    def unique_object(pairs):
        result = {}
        for key, value in pairs:
            if key in result:
                raise ValueError('duplicate member')
            result[key] = value
        return result

    try:
        decoded = base64.b64decode(segment + '=' * ((-len(segment)) % 4), altchars=b'-_', validate=True)
        if base64.urlsafe_b64encode(decoded).decode().rstrip('=') != segment:
            return False
        result = json.loads(decoded.decode('utf-8'), object_pairs_hook=unique_object,
                            parse_constant=lambda _: (_ for _ in ()).throw(ValueError('invalid JSON constant')))
        return isinstance(result, dict) and isinstance(result.get('alg'), str) and bool(result['alg'])
    except (ValueError, binascii.Error, UnicodeError):
        return False


def table_value_span(line):
    line = line.rstrip('\r\n').rstrip(' \t')
    if line.endswith('|'):
        line = line[:-1].rstrip(' \t')
    match = re.fullmatch(r' *[-*|][ \t]*([A-Za-z0-9_. -]+?)(?:[ \t]*\|[ \t]*|\t+| {2,})([^\s\"\x27`,;]+)', line)
    if not match:
        return None
    start, end = match.span(2)
    while end > start and line[end - 1] in '.)]}':
        end -= 1
    return start, end


class ContractWitnesses(unittest.TestCase):
    def test_qualifier_metrics_and_truth_table(self):
        expected = {
            'bead_identifier': ((15, 14, 4, 6, 9), (0, 5, 5, 5)),
            'name_short_hash': ((14, 13, 4, 5, 9), (0, 5, 5, 5)),
            'timestamp': ((20, 16, 14, 0, 11), (4, 4, 4, 4)),
            'version': ((16, 12, 9, 0, 10), (0, 0, 5, 5)),
            'path': ((17, 14, 0, 14, 5), (3, 3, 3, 3)),
            'camel_type': ((14, 14, 0, 14, 5), (3, 3, 3, 3)),
            'snake_name': ((16, 14, 0, 14, 4), (3, 3, 3, 3)),
            'base62_40': ((40, 40, 20, 0, 39), (0, 0, 0, 0)),
            'hex_40': ((40, 40, 20, 0, 39), (0, 0, 0, 0)),
            'base64_40': ((40, 24, 8, 0, 31), (0, 0, 0, 0)),
            'hex_31_one_case': ((31, 31, 25, 0, 11), (4, 4, 4, 4)),
            'hex_32_one_case': ((32, 32, 26, 0, 11), (0, 0, 0, 0)),
            'hex_32_mixed_case': ((32, 32, 26, 0, 11), (4, 4, 4, 4)),
            'maximal_word_cursor': ((21, 20, 7, 5, 16), (0, 0, 5, 5)),
        }
        for name, value in witnesses():
            actual = metrics(value), tuple(verdict(value, m) for m in (8, 12, 16, 20))
            self.assertEqual(actual, expected[name], name)
        for name in ('path', 'camel_type', 'snake_name'):
            value = dict(witnesses())[name]
            self.assertEqual(tuple(verdict(value, m) for m in (8, 12, 16, 20)), (3,) * 4, name)
        value = dict(witnesses())['maximal_word_cursor']
        self.assertEqual(metrics(value)[3], 5)
        self.assertEqual(verdict(value, 16), 5)

    def test_qualifier_strict_integer_boundaries(self):
        letters = ''.join(chr(97 + i % 26) for i in range(28))
        boundary = letters + ''.join(str(3) + chr(90) for _ in range(6))
        self.assertEqual(metrics(boundary)[:4], (40, 40, 6, 28))
        self.assertEqual(verdict(boundary, 12), 5, '70 percent is excluded')
        digit_boundary = ''.join(str(3) * 4 + chr(90) for _ in range(8))
        self.assertEqual(verdict(digit_boundary, 20), 4, '80 percent is excluded')
        self.assertEqual(verdict(chr(97) * 513, 8), 1)
        self.assertEqual(verdict(chr(97) + '\n' + str(3), 8), 1)
        self.assertEqual(metrics('ABCD' + 'efghi')[3], 9, 'uppercase priority and maximal runs')
        self.assertEqual(metrics('A' + 'bcde' + str(3))[3], 5, 'whole title-case run')

    def test_exclusions_case_literal_separator_and_plural_boundaries(self):
        for name in ('acknowledge-secret', 'fencing-token', 'fencing_token', 'max_tokens',
                     'secret_scan', 'secret_scan_extra', 'secret-scan-extra'):
            self.assertTrue(excluded(name), 'named exclusion')
            self.assertTrue(excluded(name.upper()), 'case-folded exclusion')
        for suffix in ('file', 'path', 'name', 'ref', 'label', 'count', 'limit', 'budget', 'usage'):
            for separator in ('_', '-'):
                self.assertTrue(excluded('token' + separator + suffix), 'suffix exclusion')
        for name in ('fencing.token', 'acknowledge_secret', 'max.tokens', 'tokens', 'passwords', 'secret_value'):
            self.assertFalse(excluded(name), 'literal near miss, not eligibility claim')
        # Exclusion is local to assignment; independent provider rules remain active.
        self.assertTrue(excluded('fencing-token'))

    def test_decoded_selection_counts_invalid_and_oversized_runs(self):
        printable = base64.b64encode(bytes([65]) * 30).decode()
        for length, selected in ((39, 0), (40, 1)):
            self.assertEqual(decoded_selection(printable[:length])[0], selected)
        for count in (64, 65):
            result = decoded_selection('!'.join(printable for _ in range(count)))
            self.assertEqual(result[:3], (64, 64, [] if count == 64 else ['run_count_limit']))
        oversized = chr(65) * 65537
        result = decoded_selection('!'.join([oversized] * 64 + [printable]))
        self.assertEqual(result[:3], (64, 0, ['run_count_limit', 'run_size_limit']))
        self.assertEqual(decoded_selection(chr(65) * 65536)[:3], (1, 0, []))
        invalid = chr(65) * 39 + '='
        self.assertEqual(decoded_selection('!'.join([invalid] * 64 + [printable]))[:3],
                         (64, 0, ['run_count_limit']))
        self.assertEqual(decoded_selection(printable)[3], [(0, 40)])

    def test_decoding_padding_and_one_level(self):
        text = ''.join(chr(65 + i % 26) for i in range(31)).encode()
        padded = base64.b64encode(text).decode()
        self.assertTrue(decode_run(padded) is not None)
        self.assertTrue(decode_run(padded.rstrip('=')) is not None)
        self.assertTrue(decode_run(padded[:-1] + '=') is not None)
        self.assertTrue(decode_run(padded + '=') is None)
        self.assertTrue(decode_run(chr(65) * 41) is None)
        self.assertTrue(decode_run(chr(65) * 20 + '=' + chr(65) * 20) is None)
        outer = base64.b64encode(padded.encode()).decode()
        self.assertEqual(decoded_selection(outer)[:2], (1, 1))

    def test_jwt_header_strictness_without_token_literals(self):
        def encoded(text):
            return base64.urlsafe_b64encode(text.encode()).decode().rstrip('=')
        self.assertTrue(header_valid(encoded(json.dumps({'alg': 'HS' + str(256)}))))
        for text in ('{"alg":null}', '{"alg":""}', '{"alg":1}', '{"alg":"a","alg":"b"}',
                     '{"alg":"a"} trailing', '[{"alg":"a"}]', '{"alg":"a","x":NaN}'):
            self.assertFalse(header_valid(encoded(text)), 'invalid header structure')
        self.assertFalse(header_valid(encoded('{"alg":"a"}') + '='), 'padded header')

    def test_table_ranges_bar_punctuation_quotes_line_endings(self):
        value = dict(witnesses())['base62_40']
        for prefix in ('| secret | ', '- secret  ', '* secret\t'):
            for ending in ('', '\n', '\r\n', '\r'):
                line = prefix + value + '.)]}' + '|' + ending
                self.assertEqual(table_value_span(line), (len(prefix), len(prefix) + 40))
        self.assertTrue(table_value_span('| secret | ' + '"' + value + '" |') is None)
        self.assertTrue(table_value_span('| secret | ' + '`' + value + '` |') is None)
        self.assertTrue(table_value_span('secret  ' + value) is None)
        self.assertTrue(table_value_span('| secret | ' + value + ' | extra |') is None)

    def test_notice_shape_and_lifecycle_cases(self):
        # Count-and-rule-only object, no candidates, selectors or findings array.
        summary = {'advisory_findings': 2, 'rules': ['advisory-high-entropy-string', 'advisory-keyword-assignment']}
        self.assertTrue(isinstance(summary['advisory_findings'], int))
        line = ('secret_scan advisory: ' + str(summary['advisory_findings']) + ' finding(s), rules '
                + ', '.join(summary['rules'])
                + '; inspect bead doctor --scope secrets. Matched bytes are not shown.\n')
        self.assertEqual(line.count('\n'), 1)
        self.assertEqual(summary['rules'], sorted(set(summary['rules'])))
        # Success/no-op/no-auto-flush/postcommit failure emit; precommit failures
        # and dry runs do not. This is an expected-outcome manifest, not proof
        # that current implementation already follows these outcomes.
        cases = {'success': True, 'noop': True, 'no_auto_flush': True,
                 'publication_failure_after_commit': True, 'rollback': False,
                 'validation_failure': False, 'dry_run': False, 'off': False}
        self.assertEqual(sum(cases.values()), 4)


if __name__ == '__main__':
    if '--table' in sys.argv:
        print('case | n,a,d,w,t | first failing step at m=8,12,16,20 (0=pass)')
        for name, value in witnesses():
            print(name, metrics(value), tuple(verdict(value, m) for m in (8, 12, 16, 20)), sep=' | ')
    else:
        unittest.main(verbosity=2)
