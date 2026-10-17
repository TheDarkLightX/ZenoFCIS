"""Independent adopter of native mounted reports; descriptive bytes never authorize."""
from __future__ import annotations

import hashlib


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


def inspect(packet, native):
    """Compare the complete decision and native authorization, then its plan.

    Reject has explicit absent committing artifacts. Its authorization is the
    native rejection evaluation, never a fabricated commit receipt.
    """
    keys = {'schema', 'class', 'reason', 'pre', 'post', 'deliveries', 'authorization',
            'patch', 'commit_plan', 'outbox', 'receipt', 'commit'}
    if type(packet) is not dict or set(packet) != keys:
        raise ValueError('complete mounted report required')
    for key in ('schema', 'class', 'reason', 'pre', 'post', 'deliveries', 'authorization', 'patch'):
        if packet[key] != native[key]:
            raise ValueError('native replay differs: ' + key)
    if packet['class'] == 'Reject':
        if any(packet[key] is not None for key in ('patch', 'commit_plan', 'outbox', 'receipt', 'commit')):
            raise ValueError('reject has committing artifacts')
        return {'class': 'Reject', 'receipt': None}
    if packet['class'] not in ('Accept', 'CommittedFailure'):
        raise ValueError('unknown decision class')
    if type(packet['commit']) is not int or packet['commit'] <= 0:
        raise ValueError('committing position required')
    try:
        plan, outbox, receipt, patch = (bytes.fromhex(packet[k]) for k in ('commit_plan', 'outbox', 'receipt', 'patch'))
    except (TypeError, ValueError) as error:
        raise ValueError('committing artifacts required') from error
    magic = b'ZFCIS-SQL-PUBLICATION-BUNDLE\0\x0a'
    if not plan.startswith(magic):
        raise ValueError('wrong native plan schema')
    commit, state, effects, actual_outbox = parts(plan[len(magic):], 4)
    row = parts(commit, 11)
    state = parts(state, 5)
    sequence = packet['commit']
    if (int.from_bytes(row[0], 'big') != sequence or row[8] != patch or row[10] != receipt
            or actual_outbox != outbox or state != [b'\0'*7+b'\1', row[0], patch, row[3], row[5]]):
        raise ValueError('substituted native plan components')
    certificate_magic = b'ZFCISV2-CERT\0'
    if (not receipt.startswith(certificate_magic) or len(receipt) != len(certificate_magic)+8+5*32
            or receipt[len(certificate_magic):len(certificate_magic)+8] != row[0]
            or receipt[len(certificate_magic)+8:len(certificate_magic)+8+128] != b''.join(row[1:5])):
        raise ValueError('receipt subject differs')
    lane(effects, sequence, 0)
    lane(outbox, sequence, 1)
    return {'class': packet['class'], 'receipt': hashlib.sha256(receipt).hexdigest(),
            'plan_bytes': len(plan)}
