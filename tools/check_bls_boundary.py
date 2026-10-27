#!/usr/bin/env python3
"""Check the approved BLS pin, immutable registry source boundary and lock delta."""
import argparse, json, tomllib
from pathlib import Path
from check_assurance import ROOT, check_shell_crypto_boundary, workspace_members
p=argparse.ArgumentParser();p.add_argument('--base-lock',type=Path,required=True);args=p.parse_args()
read=lambda path:tomllib.loads(path.read_text())
base=read(args.base_lock);current=read(ROOT/'Cargo.lock')
registry=lambda lock:{(p['name'],p['version'],p['source']):p for p in lock['package'] if 'source' in p}
a,b=registry(base),registry(current)
expected={('blst','0.3.17'),('threadpool','1.8.1'),('num_cpus','1.17.0'),('zeroize_derive','1.5.0')}
assert {(n,v) for n,v,_ in b.keys()-a.keys()}==expected
assert not a.keys()-b.keys(),'existing registry packages removed or changed'
for k in a:
    assert a[k].get('checksum')==b[k].get('checksum'),k
    if a[k]!=b[k]:
        assert k[0]=='zeroize' and b[k].get('dependencies')==['zeroize_derive'] and not a[k].get('dependencies'),k
pin=read(ROOT/'crates/zeno-fcis-bls/Cargo.toml')['dependencies']['blst']
assert pin==dict(version='=0.3.17',**{'default-features':False,'features':['portable','no-threads']})
assert not check_shell_crypto_boundary([m/'Cargo.toml' for m in workspace_members()]+[ROOT/'core-apps/zusd-v1/Cargo.toml'])
print(json.dumps(dict(result='PASS',new_packages=sorted(expected),existing_versions_changed=0,existing_checksum_changes=0,existing_edge_changes=['zeroize -> zeroize_derive'],semantic_reaches_bls=False),indent=2))
