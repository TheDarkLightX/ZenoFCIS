#!/usr/bin/env python3
"""Independent py_ecc 8.0.0 review vectors for native PoP verification, never authority.

Covers subgroup, infinity and noncanonical encodings, suite DST substitutions and
cross-domain application messages. Generation only; the Rust test replays the bytes.
"""
import importlib.metadata
from hashlib import sha256
from pathlib import Path
from py_ecc.bls import G2ProofOfPossession as BLS, G2Basic
from py_ecc.bls.point_compression import compress_G1, compress_G2
from py_ecc.optimized_bls12_381 import FQ, FQ2, field_modulus, iso_map_G2, optimized_swu_G2

assert importlib.metadata.version('py_ecc') == '8.0.0'
root = Path(__file__).resolve().parents[1]
sk = 37
key = BLS.SkToPk(sk)
pop = BLS.PopProve(sk)
message = b'independent oracle conformance'
sig = BLS.Sign(sk, message)
rows = []


def add(name, k=key, p=pop, m=message, s=sig, expected=False):
    actual = BLS.PopVerify(k, p) and BLS.Verify(k, m, s)
    assert actual == expected, name
    rows.append('\t'.join([name, str(int(expected)), k.hex(), p.hex(), m.hex(), s.hex()]))


add('valid-37', expected=True)
add('key-order3', k=int(compress_G1((FQ(0), FQ(2), FQ(1)))).to_bytes(48, 'big'))
# A curve point produced by the map to G2 before cofactor clearing.
nonprime = b''.join(int(x).to_bytes(48, 'big')
                    for x in compress_G2(iso_map_G2(*optimized_swu_G2(FQ2([13, 17])))))
add('signature-outside-subgroup', s=nonprime)
add('pop-outside-subgroup', p=nonprime)
infinity = bytes([0xc0]) + bytes(95)
add('key-identity', k=bytes([0xc0]) + bytes(47))
add('signature-identity', s=infinity)
add('pop-identity', p=infinity)
add('key-noncanonical-p', k=((1 << 383) | field_modulus).to_bytes(48, 'big'))
add('sig-imaginary-p', s=((1 << 383) | field_modulus).to_bytes(48, 'big') + sig[48:])
add('sig-real-p', s=sig[:48] + field_modulus.to_bytes(48, 'big'))
add('pop-real-p', p=pop[:48] + field_modulus.to_bytes(48, 'big'))
add('sig-uncompressed', s=bytes([sig[0] & 0x7f]) + sig[1:])
add('pop-wrong-key', p=BLS.PopProve(41))
add('signature-basic-DST', s=G2Basic.Sign(sk, message))
add('signature-POP-DST', s=BLS._CoreSign(sk, message, BLS.POP_TAG))
add('pop-SIG-DST', p=BLS.Sign(sk, key))
add('signature-custom-DST', s=BLS._CoreSign(sk, message, b'zeno.oracle.other/1'))
domains = [b'zeno.oracle.register/1\0', b'zeno.oracle.rotate/1\0', b'zeno.oracle.report/1\0']
for tag in domains:
    own = sha256(tag + b'canonical payload').digest()
    name = tag.decode().strip('\0')
    add('domain-valid-' + name, m=own, s=BLS.Sign(sk, own), expected=True)
    for other in domains:
        if other != tag:
            add('domain-cross-' + name + '-' + other.decode().strip('\0'),
                m=sha256(other + b'canonical payload').digest(), s=BLS.Sign(sk, own))

target = root / 'crates/zeno-fcis-synthesis/tests/test-data/oracle/review-crypto.tsv'
target.write_text('\n'.join(rows) + '\n')
print('review vectors:', len(rows), 'valid:', sum(r.split('\t')[1] == '1' for r in rows))
