"""Check that export filtering preserves every byte outside that section."""
import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from wasm import FUNCTIONS, HEADER, private_exports, private_module, read_u32, u32


def entry(name: bytes, kind: int, index: int) -> bytes:
    return u32(len(name)) + name + bytes([kind]) + u32(index)


def section(kind: int, payload: bytes) -> bytes:
    return bytes([kind]) + u32(len(payload)) + payload


class PrivateExports(unittest.TestCase):
    def setUp(self):
        self.functions = [entry(name, 0, index) for index, name in enumerate(sorted(FUNCTIONS))]
        self.hidden = [entry(b"memory", 2, 0), entry(b"__heap_base", 3, 1), entry(b"__data_end", 3, 2)]
        self.original = u32(10) + b"".join(self.hidden + self.functions)
        self.expected = u32(7) + b"".join(self.functions)

    def test_only_the_export_section_changes_and_filtering_is_idempotent(self):
        # Custom sections are opaque: their bytes must be copied, not parsed
        # as exports. The same applies to code, data and every other section.
        before = section(0, b"\x04note\x07\x00")
        after = section(0, b"\x04more\x07\xff")
        module = HEADER + before + section(7, self.original) + after
        expected = HEADER + before + section(7, self.expected) + after
        self.assertEqual(private_module(module), expected)
        self.assertEqual(private_module(expected), expected)

    def test_incomplete_or_unexpected_exports_stop_the_build(self):
        for payload in [self.original[:-1], self.original + b"\x00",
                        u32(1) + self.functions[0],
                        u32(11) + self.original[1:] + self.functions[0],
                        u32(11) + self.original[1:] + entry(b"extra", 0, 0)]:
            with self.assertRaises(ValueError):
                private_exports(payload)
        for module in [HEADER, HEADER + section(7, self.original)[:-1],
                       HEADER + section(7, self.original) * 2]:
            with self.assertRaises(ValueError):
                private_module(module)

    def test_section_lengths_use_bounded_integers(self):
        for value in [0, 127, 128, 65536, 0xffffffff]:
            encoded = u32(value)
            self.assertEqual(read_u32(encoded, 0), (value, len(encoded)))
        for data in [b"", b"\x80", b"\xff" * 5, b"\x80" * 5]:
            with self.assertRaises(ValueError):
                read_u32(data, 0)


if __name__ == "__main__":
    unittest.main()
