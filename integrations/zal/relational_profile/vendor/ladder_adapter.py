#!/usr/bin/env python3
"""Optional bridge from the existing ZAL ladder; no new English parser."""
import argparse
import importlib.util
import json
from pathlib import Path
import sys
import run

def lower(node, ladder, domain, env=None):
    env = {} if env is None else env
    tag = node[0]
    def var(n):
        if n[0] != 'var' or n[1] not in env:
            raise ValueError('unsupported nonvariable argument or unbound name')
        return env[n[1]]
    if tag == 'bool':
        return {'op': 'true' if node[1] else 'false'}
    if tag == 'call':
        if node[2]:
            raise ValueError('primed/stateful predicate needs a different profile')
        return {'op': 'atom', 'predicate': node[1], 'args': [var(a) for a in node[3]]}
    if tag == 'not':
        return {'op': 'not', 'body': lower(node[1], ladder, domain, env)}
    if tag in ('and', 'or', '=>', '<=>'):
        return {'op': {'and': 'and', 'or': 'or', '=>': 'implies', '<=>': 'iff'}[tag],
                'left': lower(node[1], ladder, domain, env), 'right': lower(node[2], ladder, domain, env)}
    if tag in ('=', '!=') and node[1][0] == node[2][0] == 'var':
        equality = {'op': 'equal', 'args': [var(node[1]), var(node[2])]}
        return equality if tag == '=' else {'op': 'not', 'body': equality}
    if tag in ('forall', 'exists', 'exists1'):
        _, name, dom, body = node
        if dom != domain:
            raise ValueError('all quantifiers must use the one declared domain')
        needed = ladder.free(body) - {name}
        if not needed <= env.keys():
            raise ValueError('unbound variable in ladder body')
        active = {k: env[k] for k in needed}
        available = [v for v in ('x', 'y') if v not in active.values()]
        if not available:
            raise ValueError('third live variable: select another logic profile')
        slot = available[0]
        active[name] = slot
        result = {'op': tag, 'var': slot, 'body': lower(body, ladder, domain, active)}
        if tag == 'exists1':
            result.update(op='count', cmp='eq', threshold='1')
        return result
    if tag in ('=', '<=', '>=') and node[1][0] == 'count' and node[2][0] == 'int':
        _, name, dom, body = node[1]
        value = node[2][1]
        if type(value) is not int or value < 0:
            raise ValueError('count threshold must be nonnegative')
        binder = lower(('exists', name, dom, body), ladder, domain, env)
        binder.update(op='count', cmp={'=': 'eq', '<=': 'le', '>=': 'ge'}[tag], threshold=str(value))
        return binder
    raise ValueError('unsupported ladder node: ' + str(tag))

def load_ladder(native_root):
    path = Path(native_root) / 'integrations/zal/ladder.py'
    spec = importlib.util.spec_from_file_location('zal_ladder_optional_c2', path)
    module = importlib.util.module_from_spec(spec)
    sys.modules[spec.name] = module
    sys.dont_write_bytecode = True
    spec.loader.exec_module(module)
    return module, path

def translate(ladder, context, tree, n):
    domains = context.data['domains']
    if len(domains) != 1:
        raise ValueError('this adapter requires one nonempty finite domain')
    domain = next(iter(domains))
    signature = []
    for name, symbol in context.data['symbols'].items():
        if symbol.get('kind') != 'predicate' or symbol.get('arity') not in (1, 2) or list(symbol.get('args', [])) != [domain]*symbol['arity']:
            raise ValueError('signature outside unary/binary relational profile: ' + name)
        signature.append({'id': name, 'arity': symbol['arity']})
    # Keep exists1 primitive: its general first-order expansion can introduce a
    # third variable unnecessarily; counting-1 expresses it directly in C2.
    expanded = ladder.expand(tree, context, derived=False)
    f = lower(expanded, ladder, domain)
    return {'schema': 'zal/relational-query/1', 'logic': 'zal/c2/1', 'operation': 'count',
            'signature': signature, 'formula': f, 'domain_size': str(n),
            'weights': {p['id']: ['1', '1'] for p in signature},
            'context_id': 'ladder:' + context.data['id']}

def comparison_request(left, right, assumptions, relation='equivalence'):
    for other in (right, assumptions):
        for key in ('logic', 'signature', 'domain_size', 'context_id'):
            if left[key] != other[key]:
                raise ValueError('comparison requires an explicit shared ' + key)
    if any(w != ['1', '1'] for q in (left, right, assumptions) for w in q['weights'].values()):
        raise ValueError('logical comparison is unweighted; use an explicit common model')
    return {'schema': 'zal/relational-comparison/1', 'logic': left['logic'],
            'signature': left['signature'], 'domain_size': left['domain_size'],
            'context_id': left['context_id'], 'relation': relation,
            'left': left['formula'], 'right': right['formula'], 'assumptions': assumptions['formula']}

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--native-root', type=Path, default=Path(__file__).resolve().parents[2],
                        help='ZenoFCIS checkout containing integrations/zal/ladder.py')
    parser.add_argument('--context', required=True, type=Path)
    parser.add_argument('--rung', default='intuitive')
    parser.add_argument('--text', required=True)
    parser.add_argument('--domain-size', required=True, type=int)
    parser.add_argument('--out', required=True, type=Path)
    parser.add_argument('--compare-to', help='second reading in the same declared context')
    parser.add_argument('--compare-rung', help='presentation rung for the second reading; defaults to --rung')
    parser.add_argument('--assuming', help='explicit assumptions; omitted means true')
    parser.add_argument('--assumption-rung', default='typed')
    parser.add_argument('--relation', choices=['equivalence', 'implication'], default='equivalence')
    args = parser.parse_args()
    if (args.assuming or args.compare_rung or args.relation != 'equivalence') and not args.compare_to:
        parser.error('comparison options require --compare-to')
    ladder, path = load_ladder(args.native_root)
    context = ladder.load(args.context)
    tree = ladder.parse(args.text, args.rung, context)
    q = translate(ladder, context, tree, args.domain_size)
    # Admission by the actual Haskell validator; a count result alone is never
    # interpreted as human approval of the source English.
    proposal = q
    discussion = None
    if args.compare_to:
        right_rung = args.compare_rung or args.rung
        right = translate(ladder, context, ladder.parse(args.compare_to, right_rung, context), args.domain_size)
        assumptions = dict(q, formula={'op': 'true'})
        if args.assuming:
            assumptions = translate(ladder, context, ladder.parse(args.assuming, args.assumption_rung, context), args.domain_size)
        proposal = comparison_request(q, right, assumptions, args.relation)
        discussion = {'right_text': args.compare_to, 'right_rung': right_rung,
                      'assumptions_text': args.assuming, 'assumptions_rung': args.assumption_rung,
                      'default_assumptions': 'true' if not args.assuming else None}
    answer = run.semantic_request(proposal)
    record = {'source_text': args.text, 'source_rung': args.rung, 'query': q, 'request': proposal,
              'comparison_sources': discussion, 'answer': answer,
              'human_semantic_approval': 'not inferred',
              'bindings': {str(p): run.sha(p.read_bytes()) for p in [path, path.with_name('ladder_en.json'), args.context, Path(__file__)]}}
    with args.out.open('x') as handle:
        json.dump(record, handle, indent=2, ensure_ascii=False); handle.write('\n')
    print(json.dumps({'status': answer['status'], 'value': answer.get('value'),
                      'counterexample': answer.get('counterexample'), 'record': str(args.out)}))
    raise SystemExit(run.exit_status(answer))

if __name__ == '__main__': main()
