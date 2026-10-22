"""Complete adopter artifact binding against independent original-key replay."""
import copy
import inspect as source_inspect
import json
import os
from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import patch

TOOLS = Path(__file__).resolve().parents[1]/'tools'
sys.path.insert(0, str(TOOLS))
import adopter_fixture as adopter
from run_mounted_journey import invoke
from spot_reference import Command, command_values

MAGIC = b'ZFCIS-SQL-PUBLICATION-BUNDLE\0\x0a'
DEFECT = os.environ.get('SPOT_ROUND3_DEFECT')
OMITTED = {
    'adopter-ignores-receipt-digest': ('receipt', 'commit_plan'),
    'adopter-ignores-outbox-channel': ('outbox', 'commit_plan'),
    'adopter-ignores-outbox-payload': ('outbox', 'commit_plan'),
    'adopter-ignores-commit-row': ('commit_plan',),
    'adopter-ignores-patch': ('patch',),
}
if DEFECT in OMITTED:
    source = source_inspect.getsource(adopter.inspect)
    before = "('receipt', 'outbox', 'commit_plan', 'patch')"
    after = repr(tuple(k for k in ('receipt', 'outbox', 'commit_plan', 'patch') if k not in OMITTED[DEFECT]))
    if source.count(before) != 1:
        raise ValueError('artifact comparison mutation anchor missing')
    exec(compile(source.replace(before, after, 1), '<'+DEFECT+'>', 'exec'), adopter.__dict__)
elif DEFECT in ('adopter-uses-latest-key', 'adopter-trusts-report-as-replay'):
    source = source_inspect.getsource(adopter.inspect)
    before = 'reconstructed = _reconstruct(binary, history, replay_key)'
    after = ('reconstructed = _reconstruct(binary, history, history[-1][0])'
             if DEFECT == 'adopter-uses-latest-key' else 'reconstructed = packet')
    if source.count(before) != 1:
        raise ValueError('history reconstruction mutation anchor missing')
    exec(compile(source.replace(before, after, 1), '<'+DEFECT+'>', 'exec'), adopter.__dict__)


def pack(items):
    return b''.join(len(item).to_bytes(8, 'big')+item for item in items)


def corrupt(original, kind):
    packet = copy.deepcopy(original)
    bundle = adopter.parts(bytes.fromhex(packet['commit_plan'])[len(MAGIC):], 4)
    row = adopter.parts(bundle[0], 11)
    if kind == 'receipt-digest':
        receipt = bytearray.fromhex(packet['receipt'])
        receipt[-1] ^= 1
        packet['receipt'] = bytes(receipt).hex()
        row[10] = bytes(receipt)
        bundle[0] = pack(row)
    elif kind in ('outbox-channel', 'outbox-payload'):
        lane = bundle[3]
        rows = adopter.parts(lane[8:], int.from_bytes(lane[:8], 'big'))
        fields = adopter.parts(rows[0], 12)
        if kind == 'outbox-channel':
            fields[3] = (999).to_bytes(4, 'big')
        else:
            # Last field value is the i128 transfer amount: same canonical
            # record, field tags and widths; change 1,000,000 to 1,000,001.
            payload = bytearray(fields[7])
            if int.from_bytes(payload[-16:], 'big', signed=True) != 1000000:
                raise ValueError('expected canonical transfer payload')
            payload[-1] ^= 1
            fields[7] = bytes(payload)
        rows[0] = pack(fields)
        bundle[3] = lane[:8]+pack(rows)
        packet['outbox'] = bundle[3].hex()
    elif kind == 'commit-row':
        # Command remains a same-width canonical envelope. Only the last
        # integer's low bit changes; no framing/length corruption is used.
        command = bytearray(row[6])
        command[-1] ^= 1
        row[6] = bytes(command)
        bundle[0] = pack(row)
    elif kind == 'patch-byte':
        successor = bytearray.fromhex(packet['patch'])
        successor[-1] ^= 1
        packet['patch'] = bytes(successor).hex()
    else:
        raise ValueError('unknown corruption')
    packet['commit_plan'] = (MAGIC+pack(bundle)).hex()
    return packet


@unittest.skipUnless(os.environ.get('SPOT_FIXTURE_MOUNT_BINARY') and os.environ.get('COUNTER_MOUNT_BINARY'),
                     'select fake-value test fixture and Counter executables explicitly')
class Round3(unittest.TestCase):
    def setUp(self):
        self.scratch = tempfile.TemporaryDirectory(prefix='spot5-round3-', dir=os.environ.get('TMPDIR'))
        self.addCleanup(self.scratch.cleanup)
        self.root = Path(self.scratch.name)
        self.binary = Path(os.environ['SPOT_FIXTURE_MOUNT_BINARY'])

    def fixture_packet(self):
        database = self.root/'funded.db'
        command = command_values(Command(0, a=1000000, b=1000000, sqrt=1000000))
        key = '1'*64
        history = ((key, command, (1,)),)
        invoke(self.binary, 'init', database)
        packet = invoke(self.binary, 'keyed', database, key, ' '.join(map(str, command)), '1')
        native = invoke(self.binary, 'probe', ' '.join(map(str, packet['pre'])),
                        ' '.join(map(str, command)), '1')
        return packet, native, dict(binary=self.binary, history=history, replay_key=key)

    def refuses_corruption(self, kind):
        packet, native, roots = self.fixture_packet()
        original = copy.deepcopy(packet)
        forged = corrupt(packet, kind)
        self.assertEqual(packet, original, 'corruption builder must not alias input')
        self.assertEqual(len(bytes.fromhex(forged['commit_plan'])), len(bytes.fromhex(packet['commit_plan'])))
        # Full part framing and lane locators remain valid in every specimen.
        bundle = adopter.parts(bytes.fromhex(forged['commit_plan'])[len(MAGIC):], 4)
        adopter.parts(bundle[0], 11)
        adopter.parts(bundle[1], 5)
        adopter.lane(bundle[2], packet['commit'], 0)
        adopter.lane(bundle[3], packet['commit'], 1)
        with self.assertRaisesRegex(ValueError, 'reconstructed publication differs'):
            adopter.inspect(forged, native, **roots)
        self.assertEqual(packet, original, 'inspection must not change producer artifacts')

    def test_adopter_refuses_receipt_digest_corruption(self):
        self.refuses_corruption('receipt-digest')

    def test_adopter_refuses_outbox_channel_corruption(self):
        self.refuses_corruption('outbox-channel')

    def test_adopter_refuses_outbox_payload_corruption(self):
        self.refuses_corruption('outbox-payload')

    def test_adopter_refuses_commit_row_corruption(self):
        self.refuses_corruption('commit-row')

    def test_adopter_refuses_patch_byte_corruption(self):
        self.refuses_corruption('patch-byte')

    def test_adopter_reconstructs_original_key_after_later_commits(self):
        binary = Path(os.environ['COUNTER_MOUNT_BINARY'])
        database = self.root/'counter.db'
        invoke(binary, 'init', database)
        history = (('1'*64, (120,), (1,)), ('2'*64, (121,), (1,)), ('3'*64, (120,), (0,)))
        packets = []
        for key, command, context in history:
            packets.append(invoke(binary, 'keyed', database, key, str(command[0]), str(context[0])))
        self.assertEqual([p['class'] for p in packets], ['Accept', 'CommittedFailure', 'Reject'])
        before = invoke(binary, 'state', database)
        for packet, (key, command, context) in zip(packets, history):
            native = invoke(binary, 'probe', ' '.join(map(str, packet['pre'])), str(command[0]), str(context[0]))
            result = adopter.inspect(packet, native, binary=binary, history=history, replay_key=key)
            self.assertEqual(result['class'], packet['class'])
        self.assertEqual(invoke(binary, 'state', database), before)
        with self.assertRaisesRegex(ValueError, 'history/key differs'):
            adopter.inspect(packets[0], packets[0], binary=binary, history=history, replay_key='2'*64)
        with self.assertRaisesRegex(ValueError, 'absent from trusted history'):
            adopter.inspect(packets[0], packets[0], binary=binary, history=history, replay_key='f'*64)

    def test_adopter_requires_separate_immutable_history(self):
        packet, native, roots = self.fixture_packet()
        with self.assertRaisesRegex(ValueError, 'original immutable history required'):
            adopter.inspect(packet, native)
        for history in (list(roots['history']), ((roots['replay_key'], [1], (1,)),)):
            with self.assertRaisesRegex(ValueError, 'immutable'):
                adopter.inspect(packet, native, **{**roots, 'history': history})
        # A claimed report cannot supply a precomputed expected-publication cache.
        with patch.object(adopter, '_reconstruct', wraps=adopter._reconstruct) as replay:
            adopter.inspect(packet, native, **roots)
            replay.assert_called_once_with(roots['binary'], roots['history'], roots['replay_key'])


if __name__ == '__main__':
    unittest.main()
