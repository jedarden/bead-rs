#!/usr/bin/env python3
"""Independent JSON-view byte mapping witnesses, invented without real data."""
import json
import unittest


def quoted_view(text):
    pieces = [b'"']
    mapping = [(0, 0)]
    offset = 0
    for character in text:
        encoded = json.dumps(character, ensure_ascii=False)[1:-1].encode()
        end = offset + len(character.encode())
        pieces.append(encoded)
        mapping.extend([(offset, end)] * len(encoded))
        offset = end
    pieces.append(b'"')
    mapping.append((offset, offset))
    return b''.join(pieces), mapping


def raw_span(mapping, start, end):
    if not 0 <= start < end <= len(mapping):
        return None
    first, last = mapping[start][0], mapping[end - 1][1]
    return (first, last) if first < last else None


class JsonViewContract(unittest.TestCase):
    def test_complete_quoted_context_and_unicode_mapping(self):
        text = 'prefix ' + chr(0x03bb) + chr(0x1f680) + '\n' + 'tail'
        view, mapping = quoted_view(text)
        self.assertEqual(json.loads(view), text)
        self.assertEqual(len(view), len(mapping))
        self.assertEqual(mapping[0], (0, 0))
        self.assertEqual(mapping[-1], (len(text.encode()),) * 2)
        self.assertIsNone(raw_span(mapping, 0, 1))
        self.assertIsNone(raw_span(mapping, len(mapping) - 1, len(mapping)))

    def test_password_span_excludes_formatting_whitespace(self):
        password = ''.join(('gH3', 'jK4', 'mN5', 'pQ6'))
        for whitespace in ('\n', '\r', '\t'):
            text = 'curl --user operator:' + password + whitespace + '  https://host.invalid'
            view, mapping = quoted_view(text)
            start = view.index(password.encode())
            span = raw_span(mapping, start, start + len(password))
            expected = len('curl --user operator:')
            self.assertEqual(span, (expected, expected + len(password)))
            self.assertEqual(text.encode()[span[0]:span[1]], password.encode())

    def test_literal_backslashes_remain_password_material(self):
        for count in (1, 2, 3, 4):
            password = ''.join(('gH3', chr(92) * count, 'n', 'jK4'))
            text = 'curl --user operator:' + password
            view, mapping = quoted_view(text)
            encoded_password = json.dumps(password)[1:-1].encode()
            start = view.index(encoded_password)
            span = raw_span(mapping, start, start + len(encoded_password))
            self.assertEqual(text.encode()[span[0]:span[1]], password.encode())
            self.assertEqual(json.loads(view), text)

    def test_invalid_spans_and_json_do_not_get_clean_semantics(self):
        view, mapping = quoted_view('ordinary text')
        for start, end in ((-1, 1), (2, 1), (1, 1), (0, len(mapping) + 1)):
            self.assertIsNone(raw_span(mapping, start, end))
        for invalid in (view[:-1], view + b' trailing', b'{"notes":' + view):
            with self.assertRaises((ValueError, UnicodeError)):
                json.loads(invalid)


if __name__ == '__main__':
    unittest.main(verbosity=2)
