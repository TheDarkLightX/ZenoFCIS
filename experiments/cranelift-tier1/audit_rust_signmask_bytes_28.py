#!/usr/bin/env python3
"""Replay first-ret-prefix bytes in archived Rust signmask AOT experiment.

Independently parses archived Wasmtime disassembly, checks per-object
SHA-256 and original producer cost receipts. Decoded first-return
prefixes are NOT executed-path or complete AOT costs.
"""
import argparse
import hashlib
import json
import re
import zipfile
from pathlib import Path

UPSTREAM = 'b1f81d2ad4fc64fbe0e70a7cc5fccac1a7864c49'
PROFILES = ('debug', 'optimized')
MODES = ('original', 'add', 'sub')
FUNCTIONS = ('sat_add', 'sat_add_mix', 'sat_add_xor', 'sign_clamp', 'wrap_add')
BYTE_LINE = re.compile(r'^ {12}([0-9a-f]{2}(?: [0-9a-f]{2})*)(?: {2,}(.*))?$')
HEADER = re.compile(r'^wasm\[0\]::function\[\d+\]::([^:]+):$')


def sha(blob):
    return hashlib.sha256(blob).hexdigest()


def parse(disassembly):
    selected = {}
    current = None
    for line in disassembly.decode().splitlines():
        header = HEADER.fullmatch(line)
        if header:
            name = header.group(1)
            if name in FUNCTIONS:
                if name in selected:
                    raise ValueError('duplicate selected function')
                selected[name] = []
                current = name
            else:
                current = None
        elif current is not None:
            selected[current].append(line)
    if set(selected) != set(FUNCTIONS):
        raise ValueError('missing selected function')
    found = {}
    for name, lines in selected.items():
        count = 0
        first = None
        rip_xor = movabs = 0
        for line in lines:
            m = BYTE_LINE.fullmatch(line)
            if m:
                count += len(m.group(1).split(' '))
                asm = m.group(2) or ''
                if first is None:
                    if 'xorq' in asm and '(%rip)' in asm:
                        rip_xor += 1
                    if 'movabsq' in asm:
                        movabs += 1
                    if asm.strip().startswith('retq'):
                        first = count
            elif line.strip() and not line.lstrip().startswith('╰─╼'):
                raise ValueError('unrecognized disassembly byte line')
        if first is None:
            raise ValueError('no first return in selected function')
        found[name] = {'first_ret_prefix_bytes': first,
                       'disassembly_region_bytes': count,
                       'prefix_rip_xor': rip_xor, 'prefix_movabs': movabs}
    return found


def replay(archive):
    with zipfile.ZipFile(archive) as z:
        if z.testzip() is not None:
            raise ValueError('damaged archived artifact')
        doc = json.loads(z.read('cases/qualification.json'))
        independent = json.loads(z.read('independent-recheck.json'))
        if (doc.get('schema') != 'zeno-fcis/tier1-rust-llvm-signmask-target-cost/1'
            or doc.get('source', {}).get('upstream_commit') != UPSTREAM
            or doc.get('candidates') != list(MODES)
            or independent.get('status') != 'real-native-ELF-AOT-and-separate-finite-witnesses-rechecked'
            or len(doc.get('profiles', [])) != len(PROFILES)):
            raise ValueError('unqualified source / missing independent receipt')
        rows = []
        for profile, row in zip(PROFILES, doc['profiles']):
            if row['profile'] != profile or set(row['compiled']) != set(MODES):
                raise ValueError('profile or compiler omitted')
            objects = {}
            for mode in MODES:
                record = row['compiled'][mode]
                root = f'cases/{profile}/{mode}/'
                binary = z.read(root + 'idiom.cwasm')
                dis = z.read(root + 'disassembly.txt')
                if (sha(binary) != record['sha256'] or len(binary) != record['aot_bytes']
                    or sha(dis) != record['disassembly_sha256']):
                    raise ValueError('archived binary or disassembly altered')
                objects[mode] = {'code': parse(dis), 'sha': sha(binary),
                                 'aot_bytes': len(binary),
                                 'text_bytes': record['elf_text_bytes']}
            base = objects['original']
            for mode in ('add', 'sub'):
                check = row['comparisons'][mode]
                item = objects[mode]
                if (check['actual_aot_identical'] != (base['sha'] == item['sha'])
                    or check['baseline_minus_candidate_whole_cwasm_bytes'] != base['aot_bytes'] - item['aot_bytes']
                    or check['baseline_minus_candidate_elf_text_bytes'] != base['text_bytes'] - item['text_bytes']):
                    raise ValueError('cost claims do not agree with archived binaries')
            deltas = {name: objects['add']['code'][name]['first_ret_prefix_bytes']
                      - base['code'][name]['first_ret_prefix_bytes'] for name in FUNCTIONS}
            if base['code'] != objects['sub']['code'] or base['sha'] != objects['sub']['sha']:
                raise ValueError('sub is not native identical to baseline')
            rows.append({'profile': profile,
                         'add_minus_original_first_return_prefix_bytes': deltas,
                         'sum_extra_prefix_bytes_for_three_saturation_exports': sum(deltas[n] for n in FUNCTIONS[:3]),
                         'zero_elf_text_savings': all(base['text_bytes'] == objects[m]['text_bytes'] for m in MODES),
                         'zero_complete_aot_savings': all(base['aot_bytes'] == objects[m]['aot_bytes'] for m in MODES),
                         'orig_rip_xors': {n:base['code'][n]['prefix_rip_xor'] for n in FUNCTIONS},
                         'add_movabs': {n:objects['add']['code'][n]['prefix_movabs'] for n in FUNCTIONS}})
        return {'schema':'zeno-fcis/tier1-rust-signmask-first-ret-audit/1',
                'archive_sha256':sha(archive.read_bytes()),
                'upstream_commit':UPSTREAM,
                'profiles':rows,
                'scope':'read-only archived emitted native bytes; first-ret prefix != executed-path or performance; ELF and whole AOT checked by preceding CI'}


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--zip',type=Path,required=True)
    parser.add_argument('--out',type=Path,required=True)
    args = parser.parse_args()
    if args.out.exists():
        parser.error('do not overwrite an audit')
    result = replay(args.zip)
    args.out.write_text(json.dumps(result,indent=2,sort_keys=True) + '\n')
    print(json.dumps(result['profiles'],sort_keys=True))


if __name__ == '__main__':
    main()
