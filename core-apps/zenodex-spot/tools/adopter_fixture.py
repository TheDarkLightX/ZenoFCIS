"""Independent adopter of native mounted reports; descriptive bytes never authorize."""
from __future__ import annotations

import hashlib
import json
import os
from pathlib import Path
import subprocess
import tempfile


def parts(data, count):
    result = []
    offset = 0
    for _ in range(count):
        if len(data) - offset < 8:
            raise ValueError('truncated part length')
        size = int.from_bytes(data[offset:offset+8], 'big')
        offset += 8
        if size > len(data) - offset:
            raise ValueError('truncated part')
        result.append(data[offset:offset+size])
        offset += size
    if offset != len(data):
        raise ValueError('trailing parts')
    return result


def lane(data, sequence, lane_id):
    if len(data) < 8:
        raise ValueError('missing lane count')
    count = int.from_bytes(data[:8], 'big')
    if count > 1024:
        raise ValueError('lane count bound')
    rows = parts(data[8:], count)
    for ordinal, row in enumerate(rows):
        fields = parts(row, 12)
        if (int.from_bytes(fields[0], 'big') != sequence or fields[1] != bytes([lane_id])
                or int.from_bytes(fields[2], 'big') != ordinal or fields[11] != b'\0'):
            raise ValueError('outbox locator/order differs')
    return rows


def _reconstruct(binary, history, replay_key):
    """Replay trusted acquisition inputs in an exclusively owned scratch store.

    History is an immutable tuple of (key, command tuple, context tuple), in
    original submission order, including the selected key. It must come from
    the adopter's original history, never from the report being inspected.
    The configured executable binds the contract and its exact genesis.
    """
    if binary is None or type(history) is not tuple or not history:
        raise ValueError('native executable and original immutable history required')
    keys = set()
    for entry in history:
        if type(entry) is not tuple or len(entry) != 3:
            raise ValueError('immutable key/command/context entry required')
        key, command, context = entry
        if (type(key) is not str or len(key) != 64 or any(c not in '0123456789abcdef' for c in key)
                or key in keys):
            raise ValueError('unique canonical replay key required')
        if any(type(values) is not tuple or any(type(n) is not int for n in values)
               for values in (command, context)):
            raise ValueError('immutable exact integer command/context required')
        keys.add(key)
    if type(replay_key) is not str or replay_key not in keys:
        raise ValueError('original replay key absent from trusted history')
    binary = str(Path(binary).resolve())
    def invoke(*arguments):
        result = subprocess.run([binary, *map(str, arguments)], capture_output=True,
                                text=True, timeout=35, check=False)
        if result.returncode:
            raise ValueError('independent native replay refused: ' + result.stderr)
        return json.loads(result.stdout)
    # Preparation is shell work; this store never supplies deployment authority.
    with tempfile.TemporaryDirectory(prefix='spot-adopter-', dir=os.environ.get('TMPDIR')) as tmp:
        database = Path(tmp)/'replay.db'
        invoke('init', database)
        selected = None
        for key, command, context in history:
            result = invoke('keyed', database, key, ' '.join(map(str, command)),
                            ' '.join(map(str, context)))
            if key == replay_key:
                selected = result
        return selected


def inspect(packet, native, *, binary=None, history=None, replay_key=None):
    """Compare reports to a probe and complete independently replayed artifacts.

    The caller supplies the trusted executable, original submission history and
    original key separately from packet. Neither report supplies those roots.
    Results are descriptive only; this does not authenticate external history.
    """
    keys = {'schema', 'class', 'reason', 'pre', 'post', 'deliveries', 'authorization',
            'patch', 'commit_plan', 'outbox', 'receipt', 'commit'}
    if type(packet) is not dict or set(packet) != keys:
        raise ValueError('complete mounted report required')
    for key in ('schema', 'class', 'reason', 'pre', 'post', 'deliveries', 'authorization'):
        if packet[key] != native[key]:
            raise ValueError('native replay differs: ' + key)
    reconstructed = _reconstruct(binary, history, replay_key)
    for key in ('schema', 'class', 'reason', 'pre', 'post', 'deliveries', 'authorization', 'commit'):
        if packet[key] != reconstructed[key]:
            raise ValueError('original history/key differs: ' + key)
    for key in ('receipt', 'outbox', 'commit_plan', 'patch'):
        if packet[key] != reconstructed[key]:
            raise ValueError('reconstructed publication differs: ' + key)
    if packet['class'] == 'Reject':
        if any(packet[key] is not None for key in ('patch', 'commit_plan', 'outbox', 'receipt', 'commit')):
            raise ValueError('reject has committing artifacts')
        return {'class': 'Reject', 'receipt': None}
    if packet['class'] not in ('Accept', 'CommittedFailure'):
        raise ValueError('unknown decision class')
    if type(packet['commit']) is not int or packet['commit'] <= 0:
        raise ValueError('committing position required')
    try:
        plan, receipt = (bytes.fromhex(packet[k]) for k in ('commit_plan', 'receipt'))
    except (TypeError, ValueError) as error:
        raise ValueError('committing artifacts required') from error
    return {'class': packet['class'], 'receipt': hashlib.sha256(receipt).hexdigest(),
            'plan_bytes': len(plan)}
