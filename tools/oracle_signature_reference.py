#!/usr/bin/env python3
"""Reference-only BLS/JSON vectors; never a native admission or publication."""
import hashlib
import json
from pathlib import Path
from py_ecc.bls import G2ProofOfPossession as BLS
from py_ecc.bls import G2Basic

ROOT=Path(__file__).resolve().parents[1]
def canonical(value):
    return json.dumps(value,sort_keys=True,separators=(',',':'),ensure_ascii=True,allow_nan=False).encode()
payload={
    'chain_id':'01','feed_id':'sha256:'+'01'*32,'key_version':7,'observed_epoch':4,
    'observation_seconds':12,'payload_schema':'zeno.oracle.report_payload/1',
    'previous_report_id':'sha256:'+'09'*32,'profile_id':'sha256:'+'02'*32,
    'query_id':'sha256:'+'03'*32,'reporter_id':'02','round_id':3,
    'scheme_id':'oracle/bls12381g2-pop/1','sequence':11,'source_id':'03','value_e8':2**128-1,
}
body=canonical(payload)
message=hashlib.sha256(b'zeno.oracle.report/1\0'+body).digest()
key=BLS.SkToPk(5)
pop=BLS.PopProve(5)
signature=BLS.Sign(5,message)
assert BLS.PopVerify(key,pop)
assert BLS.Verify(key,message,signature)
assert not BLS.Verify(key,message+b'changed',signature)
assert not G2Basic.Verify(key,message,signature)
assert not BLS.PopVerify(bytes(48),pop)
report={'payload':payload,'payload_hash':'sha256:'+hashlib.sha256(body).hexdigest(),
    'schema':'zeno.oracle.report/1','signature':signature.hex()}
rows=[body.hex(),hashlib.sha256(body).hexdigest(),message.hex(),key.hex(),pop.hex(),signature.hex(),hashlib.sha256(canonical(report)).hexdigest()]
target=ROOT/'crates/zeno-fcis-synthesis/tests/test-data/oracle/signature.tsv'
target.write_text('\t'.join(rows)+'\n')
print('py_ecc reference PoP/signature verified; mutation and Basic/infinity negatives rejected; no native verifier claim')
