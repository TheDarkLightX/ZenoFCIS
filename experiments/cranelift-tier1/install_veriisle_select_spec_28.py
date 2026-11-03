#!/usr/bin/env python3
"""Install a narrowly scoped, EXPERIMENTAL scalar-i64 VeriISLE select spec.

The pinned upstream verifier has no spec for the CLIF select extractor.
This provisional model is NOT authoritative or independently certified.
Only a detached, exact-source upstream checkout may be modified.
"""
from __future__ import annotations
import argparse
import hashlib
import json
import subprocess
from pathlib import Path
from install_isle_candidate import UPSTREAM

SPEC_PATH = 'cranelift/codegen/src/spec/inst_specs.isle'
SPEC_BLOB = '654839e3833334c80c7116c0bcbbe0f8eb269169'
SPEC = """
;; EXPERIMENTAL TIER1 VERIISLE SPEC — NOT PART OF PINNED UPSTREAM.
;; Cranelift scalar select chooses x on true, y on false. The i8 model
;; represents the scalar b1 condition; this is intentionally only i64.
;; An independently checked authoritative spec is required before upstreaming.
(spec (select ty c x y)
    (provide (= result (if (= c #x00) y x))
             (= (:bits ty) (widthof result))
             (= (:lanes ty) (lanesof result))))
(instantiate select
    ((args (named Type) (bv 8) (bv 64) (bv 64)) (ret (bv 64))))
"""


def install(upstream: Path) -> dict:
    def git(*args: str) -> str:
        return subprocess.check_output(['git', '-C', str(upstream), *args], text=True).strip()
    if git('rev-parse', 'HEAD') != UPSTREAM:
        raise ValueError('unexpected Wasmtime source revision')
    if git('rev-parse', f'HEAD:{SPEC_PATH}') != SPEC_BLOB:
        raise ValueError('unexpected original VeriISLE specification blob')
    path = upstream / SPEC_PATH
    old = path.read_text()
    if 'EXPERIMENTAL TIER1 VERIISLE SPEC' in old:
        raise ValueError('experimental spec already installed')
    new = old.rstrip('\n') + '\n' + SPEC
    path.write_text(new)
    return {'schema': 'zeno-fcis/tier1-provisional-select-spec/1',
            'status': 'installed-experimental-spec-not-authoritative',
            'upstream_commit': UPSTREAM, 'source_path': SPEC_PATH,
            'original_blob': SPEC_BLOB,
            'source_sha256': hashlib.sha256(new.encode()).hexdigest(),
            'spec_sha256': hashlib.sha256(SPEC.encode()).hexdigest(),
            'nonclaim': 'this spec is a local semantic assumption, not verified upstream coverage'}


if __name__ == '__main__':
    p = argparse.ArgumentParser()
    p.add_argument('--upstream', type=Path, required=True)
    p.add_argument('--out', type=Path, required=True)
    a = p.parse_args()
    if a.out.exists():
        p.error('fresh receipt required')
    result = install(a.upstream)
    a.out.parent.mkdir(parents=True, exist_ok=True)
    a.out.write_text(json.dumps(result, indent=2, sort_keys=True) + '\n')
    print(json.dumps({'status': result['status']}))
