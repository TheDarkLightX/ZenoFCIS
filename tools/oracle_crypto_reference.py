#!/usr/bin/env python3
"""Independent py_ecc 8.0.0 bytes for native PoP verification, never authority."""
import importlib.metadata
from pathlib import Path
from py_ecc.bls import G2ProofOfPossession as BLS, G2Basic
from py_ecc.bls.point_compression import compress_G1, compress_G2
from py_ecc.optimized_bls12_381 import FQ, FQ2, field_modulus, iso_map_G2, optimized_swu_G2

assert importlib.metadata.version('py_ecc') == '8.0.0'
root = Path(__file__).resolve().parents[1]
rows = []


def retain(name, expected, key, pop, message, signature):
    valid = BLS.PopVerify(key, pop) and BLS.Verify(key, message, signature)
    assert valid == expected, name
    rows.append('\t'.join([name, str(int(expected)), key.hex(), pop.hex(), message.hex(), signature.hex()]))


for sk, message in [(1,b''), (2,b'abc'), (5,bytes(32)), (17,bytes(range(64))), (123456,b'\xff'*32)]:
    retain('valid-'+str(sk), True, BLS.SkToPk(sk), BLS.PopProve(sk), message, BLS.Sign(sk,message))

key = BLS.SkToPk(5); pop = BLS.PopProve(5); message = b'exact oracle domain'; sig = BLS.Sign(5,message)
retain('message-mutation',False,key,pop,message+b'!',sig)
retain('basic-suite',False,key,pop,message,G2Basic.Sign(5,message))
retain('wrong-key',False,BLS.SkToPk(7),BLS.PopProve(7),message,sig)
retain('wrong-pop',False,key,BLS.PopProve(7),message,sig)
retain('infinite-key',False,bytes([0xc0])+bytes(47),pop,message,sig)
retain('infinite-signature',False,key,pop,message,bytes([0xc0])+bytes(95))
retain('noncompressed-key',False,bytes([key[0]&0x7f])+key[1:],pop,message,sig)
retain('noncanonical-field',False,((1<<383)|field_modulus).to_bytes(48,'big'),pop,message,sig)
retain('short-key',False,key[:-1],pop,message,sig)
retain('off-curve-key',False,bytes([0x80])+bytes(46)+b'\x01',pop,message,sig)
small_key = int(compress_G1((FQ(0),FQ(2),FQ(1)))).to_bytes(48,'big')
retain('outside-key-subgroup',False,small_key,pop,message,sig)
small_sig = compress_G2(iso_map_G2(*optimized_swu_G2(FQ2.zero())))
retain('outside-signature-subgroup',False,key,pop,message,b''.join(int(c).to_bytes(48,'big') for c in small_sig))
target = root/'crates/zeno-fcis-synthesis/tests/test-data/oracle/crypto.tsv'
target.write_text('# py_ecc 8.0.0; oracle/bls12381g2-pop/1; actual bytes, no symbolic validity\n'+'\n'.join(rows)+'\n')
print('Independent crypto rows',len(rows),'valid',sum('\t1\t' in r for r in rows))
