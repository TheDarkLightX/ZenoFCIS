#!/usr/bin/env python3
"""Explain profile deltas against exact hashes; never normalize acceptance coverage.

A retained candidate VIR and the original raw hashes suffice to check a proposed
read-ID bijection: transformed fields must reproduce the original SHA256 values.
The only separately allowed delta is the evaluator's 32-byte identity constant,
whose prior body must reconstruct from the explicitly supplied prior source.
"""
import argparse
import hashlib
import json
from pathlib import Path
import re

from verus_coverage import fingerprint, parse_vir, require_coverage

EVALUATOR = 'checker::execution_v2::authority::evaluator::EVALUATOR'


def evaluator_body(source):
    match = re.search(r'const EVALUATOR: \[u8; 32\] = \[([^]]+)\]', source)
    if match is None:
        raise ValueError('missing exact evaluator constant')
    values = [int(token.strip(), 0) for token in match[1].split(',') if token.strip()]
    if len(values) != 32 or any(not 0 <= n <= 255 for n in values):
        raise ValueError('invalid evaluator constant')
    return ['>', 'Block', [], ['>', 'ArrayLiteral',
            [['>', 'Const', ['Constant', 'Int', str(n)]] for n in values]]], bytes(values).hex()


def compare(base, candidate, vir, read_offset, old_evaluator=None, new_evaluator=None):
    require_coverage(vir, candidate)
    old, new = base['functions'], candidate['functions']
    nodes = {n[1][2]: dict(zip(n[2::2], n[3::2])) for n in parse_vir(vir)
             if n and n[0] == 'Function'}
    removed, added = sorted(set(old) - set(new)), sorted(set(new) - set(old))
    changed = [name for name in old if name in new and old[name] != new[name]]
    forward, backward = {}, {}
    def normalize(node, offset):
        if not isinstance(node, list):
            return node
        if (len(node) == 5 and node[0] == 'UnfinalizedReadKind'
                and node[1] == ':preliminary_kind' and node[3] == ':id'
                and isinstance(node[4], str) and node[4].isdigit()):
            right = int(node[4])
            left = right + offset
            if left < 0 or forward.get(right, left) != left or backward.get(left, right) != right:
                raise ValueError('read ID substitution is not a bijection')
            forward[right], backward[left] = left, right
            return [*node[:4], str(left)]
        return [normalize(child, offset) for child in node]
    explained, unexpected, identity = [], [], None
    for name in sorted(set(old) & set(new)):
        fields = nodes[name]
        transformed = {key: normalize(value, read_offset if name in changed else 0)
                       for key, value in fields.items()}
        observed = {
            'mode': transformed[':mode'], 'requires': len(transformed[':require']),
            'ensures': len(transformed[':ensure']),
            'signature_sha256': fingerprint([transformed[':typ_bounds'], transformed[':params'], transformed[':ret']]),
            'requires_sha256': fingerprint(transformed[':require']),
            'ensures_sha256': fingerprint(transformed[':ensure']),
        }
        if 'body_sha256' in old[name]:
            observed['body_sha256'] = fingerprint(transformed[':body'])
        mismatches = [key for key in old[name] if observed.get(key) != old[name][key]]
        if (name == EVALUATOR and mismatches == ['body_sha256']
                and old_evaluator is not None and new_evaluator is not None):
            prior_body, prior_digest = evaluator_body(old_evaluator)
            current_body, current_digest = evaluator_body(new_evaluator)
            if (fingerprint(prior_body) == old[name]['body_sha256']
                    and current_body == fields[':body']):
                identity = {'function': name, 'old_evaluator': prior_digest,
                            'new_evaluator': current_digest,
                            'old_body_sha256': fingerprint(prior_body),
                            'new_body_sha256': fingerprint(current_body)}
                mismatches = []
        if mismatches:
            unexpected.append({'function': name, 'fields': mismatches})
        elif name in changed and name != EVALUATOR:
            explained.append({'function': name, 'changed_fields':
                              [key for key in old[name] if old[name][key] != new[name][key]]})
    return {'schema': 'zeno-fcis/checker-profile-delta/1', 'review_accepted': False,
            'status': 'explained' if not removed and not unexpected else 'unexplained',
            'base_records': len(old), 'candidate_records': len(new),
            'added': added, 'removed': removed, 'changed_records': len(changed),
            'read_id_offset': read_offset, 'bijective_read_ids': len(forward),
            'id_only_changes': explained, 'evaluator_identity_change': identity,
            'unexpected_changes': unexpected,
            'scope': 'Exact profile-covered fields only; added records require independent review. '
                     'No production hash normalization; no proof or review acceptance.'}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--base-profile', type=Path, required=True)
    parser.add_argument('--candidate-profile', type=Path, required=True)
    parser.add_argument('--candidate-vir', type=Path, required=True)
    parser.add_argument('--read-id-offset', type=int, required=True)
    parser.add_argument('--old-evaluator-source', type=Path)
    parser.add_argument('--new-evaluator-source', type=Path)
    parser.add_argument('--out', type=Path, required=True)
    args = parser.parse_args()
    if args.out.exists():
        raise ValueError('preserve prior comparison evidence')
    inputs = {key: value for key, value in vars(args).items() if isinstance(value, Path) and key != 'out'}
    result = compare(json.loads(args.base_profile.read_text()),
                     json.loads(args.candidate_profile.read_text()), args.candidate_vir.read_text(),
                     args.read_id_offset,
                     args.old_evaluator_source.read_text() if args.old_evaluator_source else None,
                     args.new_evaluator_source.read_text() if args.new_evaluator_source else None)
    result['inputs'] = {key: {'path': str(path), 'sha256': hashlib.sha256(path.read_bytes()).hexdigest()}
                        for key, path in inputs.items()}
    args.out.write_text(json.dumps(result, indent=2, sort_keys=True) + '\n')
    print(json.dumps({key: result[key] for key in ('status', 'changed_records', 'unexpected_changes')}))
    return 0 if result['status'] == 'explained' else 1


if __name__ == '__main__':
    raise SystemExit(main())
