#!/usr/bin/env python3
"""Optional source-level replay of gr-satellites' real adder implementation.

The GNU Radio message boundary and Construct serialization are stubbed. This
checks the block's sequence-state logic, not full GNU Radio integration.
Set GR_SATELLITES_ADDER_SOURCE to a checked-out source file to enable it.
"""

import importlib.util
import os
import pathlib
import sys
import types
import unittest
from unittest.mock import patch


class FakeBasicBlock:
    def __init__(self, **kwargs):
        self.published = []

    def message_port_register_in(self, name):
        pass

    def set_msg_handler(self, name, handler):
        pass

    def message_port_register_out(self, name):
        pass

    def message_port_pub(self, name, msg):
        self.published.append(msg)


class FakePrimaryHeader:
    @staticmethod
    def build(fields):
        widths = (
            ('transfer_frame_version', 2), ('bypass', 1),
            ('control', 1), ('RSVD_spare', 2),
            ('spacecraft_id', 10), ('virtual_channel_id', 6),
            ('frame_length', 10), ('frame_sequence_number', 8),
        )
        bits = 0
        for name, width in widths:
            value = fields[name]
            if not isinstance(value, int) or not 0 <= value < 2 ** width:
                raise ValueError(f'{name} {value} exceeds {width} bits')
            bits = (bits << width) | value
        return bits.to_bytes(5, 'big')


def load_actual_adder(source):
    """Import the actual Python source, replacing only optional dependencies."""
    package = '_zeno_ccsds_test'
    fake_gr = types.ModuleType('gnuradio.gr')
    fake_gr.basic_block = FakeBasicBlock
    fake_gnuradio = types.ModuleType('gnuradio')
    fake_gnuradio.gr = fake_gr
    fake_pmt = types.ModuleType('pmt')
    fake_pmt.intern = lambda name: name
    fake_pmt.cdr = lambda pair: pair[1]
    fake_pmt.is_u8vector = lambda value: isinstance(value, bytes)
    fake_pmt.u8vector_elements = lambda value: list(value)
    fake_pmt.init_u8vector = lambda size, value: bytes(value)
    fake_pmt.cons = lambda meta, payload: (meta, payload)
    fake_pmt.PMT_NIL = None
    fake_package = types.ModuleType(package)
    fake_package.__path__ = []
    fake_tc = types.ModuleType(package + '.telecommand')
    fake_tc.PrimaryHeader = FakePrimaryHeader
    substitutions = {
        'gnuradio': fake_gnuradio, 'gnuradio.gr': fake_gr,
        'pmt': fake_pmt, package: fake_package,
        package + '.telecommand': fake_tc,
    }
    spec = importlib.util.spec_from_file_location(
        package + '.telecommand_primaryheader_adder', str(source))
    if spec is None or spec.loader is None:
        raise RuntimeError('Could not load source')
    module = importlib.util.module_from_spec(spec)
    with patch.dict(sys.modules, substitutions):
        spec.loader.exec_module(module)
    return module.telecommand_primaryheader_adder


class AdderIntegration(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        path = os.environ.get('GR_SATELLITES_ADDER_SOURCE')
        if not path:
            raise unittest.SkipTest('Provide GR_SATELLITES_ADDER_SOURCE to test an upstream checkout')
        cls.Adder = load_actual_adder(pathlib.Path(path))

    def test_1024_non_bypass_frames(self):
        block = self.Adder(0, 0, 37, 4)
        for i in range(1024):
            block.handle_msg((None, b'\x2a\x11'))
            frame = block.published[-1][1]
            self.assertEqual(len(frame), 7)
            self.assertEqual(frame[4], i % 256, i)
            self.assertEqual(frame[5:], b'\x2a\x11', i)
            self.assertEqual(block.frame_sequence_number, (i + 1) % 256, i)

    def test_sequence_boundary_254_255_0(self):
        block = self.Adder(0, 0, 37, 4)
        block.frame_sequence_number = 254
        observed = []
        for _ in range(3):
            block.handle_msg((None, b'\x01'))
            observed.append(block.published[-1][1][4])
        self.assertEqual(observed, [254, 255, 0])
        self.assertEqual(block.frame_sequence_number, 1)

    def test_bypass_resets_and_following_frame_starts_at_zero(self):
        block = self.Adder(0, 0, 37, 4)
        for _ in range(17):
            block.handle_msg((None, b'\x00'))
        block.bypass = 1
        block.handle_msg((None, b'\x00'))
        self.assertEqual(block.published[-1][1][4], 0)
        self.assertEqual(block.frame_sequence_number, 0)
        block.bypass = 0
        block.handle_msg((None, b'\x00'))
        self.assertEqual(block.published[-1][1][4], 0)
        self.assertEqual(block.frame_sequence_number, 1)


if __name__ == '__main__':
    unittest.main()
