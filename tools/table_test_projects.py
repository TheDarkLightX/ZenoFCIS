#!/usr/bin/env python3
"""Generate the exact small table domains and the separately tested balances project."""
import json
import sys
from pathlib import Path


def lit(n, ty=201): return ['literal', ty, n]
def truth(b=True): return ['literal', 203, b]
def read(source, field): return ['input', source, [field]]
def op(name, *args): return [name, *args]
def eq(a, b): return op('eq', a, b)
def not_(a): return op('not', a)
def and_(*xs):
    value = truth()
    for x in xs: value = op('and', value, x)
    return value

def or_(*xs): return not_(and_(*(not_(x) for x in xs)))
def implies(a, b): return or_(not_(a), b)
def exists(table, key, tid=204): return op('contains', tid, table, key)
def get(table, key, tid=204): return op('get', tid, table, key)
def present(table, key, predicate, tid=204): return op('match_lookup', get(table, key, tid), truth(False), predicate)
def amount(table, key, tid=204): return op('match_lookup', get(table, key, tid), lit(0), ['payload'])
def accept(assignments=None): return ['accept', assignments or {}]
def edit(expression, assignments): return ['match_edit', expression, accept(assignments), ['reject_edit']]
def predicate(n, scope, expr): return {'id': n, 'scope': scope, 'assert': expr}
def case(when, then): return {'when': when, 'then': then}

def budgets(): return {'read': 10000000, 'write': 1000, 'candidate': 10, 'effect': 0, 'byte': 100000000, 'witness_byte': 0, 'depth': 0, 'step': 10000000}

def project(name, declarations, fields, reasons):
    lines = ['zeno 1;', f'project 1 {name};', 'namespace 2 core;', 'type 100 state State;', 'type 101 command Command;', 'type 102 context Context;']
    lines += declarations
    lines += [f'field {fid} {root} {label} {tid};' for fid, root, label, tid in fields]
    lines += [f'reason {r} r{r} precedence {i};' for i, r in enumerate(reasons)]
    return '\n'.join(lines) + '\n'

def generic(kind, capacity):
    is_set = kind == 'set'
    before, after, key, value, command = read('pre', 110), read('post', 110), read('command', 121), read('command', 122), read('command', 120)
    value = ['literal', 205, None] if is_set else value
    selected = lambda n: eq(command, lit(n, 202))
    size = lambda t: op('len', 204, t)
    missing = not_(exists(before, key))
    full = and_(eq(size(before), lit(capacity, 202)), missing)
    cases = [case(selected(0), edit(op('insert', 204, before, key, value), {'110': ['payload']})),
             case(selected(1), ['reject', 313] if is_set else edit(op('update', 204, before, key, value), {'110': ['payload']})),
             case(selected(2), edit(op('remove', 204, before, key), {'110': ['payload']})),
             case(and_(selected(3), op('match_lookup', get(before, key), truth(), truth())), accept()),
             case(and_(selected(4), eq(exists(before, key), exists(before, key))), accept()),
             case(and_(selected(5), eq(size(before), size(before))), accept()),
             case(and_(selected(6), eq(op('is_empty', 204, before), op('is_empty', 204, before))), accept()),
             case(and_(selected(7), op('is_empty',204,op('empty',204))),accept())]
    inserted = and_(present(after, key, eq(['payload'], value)), eq(size(after), op('match_lookup', get(before, key), op('add', size(before), lit(1)), size(before))))
    updated = and_(exists(before, key), present(after, key, eq(['payload'], value)), eq(size(before), size(after)))
    removed = and_(not_(exists(after, key)), eq(size(after), op('match_lookup', get(before, key), size(before), op('sub', size(before), lit(1)))))
    legitimate = not_(or_(and_(selected(0), full), selected(1) if is_set else and_(selected(1), missing)))
    rejected = or_(and_(selected(0), full, eq(['reason'], lit(307, 206))),
                   and_(selected(1), eq(['reason'], lit(313, 206))) if is_set else and_(selected(1), missing, eq(['reason'], lit(303, 206))))
    laws = [predicate(400, 'accept', and_(legitimate, implies(selected(0), inserted), implies(selected(1), updated) if not is_set else truth(), implies(selected(2), removed), implies(not_(or_(selected(0), selected(1), selected(2))), eq(before, after)))),
            predicate(401, 'reject', rejected), {'id': 402, 'frame': 110, 'keys': [key]},
            predicate(403, 'state', eq(op('count', before, truth()), size(before))),
            predicate(404, 'state', op('all', before, truth())),
            predicate(405, 'state', eq(op('any', before, truth()), not_(op('is_empty', 204, before))))]
    if not is_set: laws.append(predicate(406, 'state', not_(op('lt', op('sum_i128', before, ['value']), lit(0)))))
    matching=eq(['key'],key)
    has_key=op('any',before,matching)
    expected_value=op('sum_i128',before,op('match_bool',matching,['value'],lit(0))) if not is_set else ['literal',205,None]
    lookup_correct=op('match_lookup',get(before,key),not_(has_key),and_(has_key,eq(['payload'],expected_value)))
    laws.append(predicate(407,'accept',and_(implies(selected(3),lookup_correct),implies(selected(4),eq(exists(before,key),has_key)),implies(selected(5),eq(size(before),op('count',before,truth()))),implies(selected(6),eq(op('is_empty',204,before),not_(op('any',before,truth())))),implies(selected(7),op('is_empty',204,op('empty',204))))))

    unit_bytes = 34 if is_set else 50
    rules = {'schema': 'zeno-fcis/tables-policy/1', 'bindings': {'204': {'kind': kind, 'key': 200, 'value': 205 if is_set else 201, 'capacity': capacity, 'item_bytes': unit_bytes, 'snapshot_bytes': 29 + unit_bytes * capacity}, '205': {'kind': 'unit'}}, 'genesis': {'110': []}, 'cases': cases, 'laws': laws, 'budgets': budgets(), 'frame_bytes': [60 + 42 * capacity, 200, 100]}
    # Single-field state: 48 envelope + 5 record + 2 field + 5 map + entries.
    rules['frame_bytes'][0] = 60 + (26 if is_set else 42) * capacity
    source = project(f'tables_{kind}_{capacity}', ['type 200 int Key in 0..=3;', 'type 201 int EntryValue in 0..=1;', 'type 202 int Operation in 0..=7;', 'type 203 bool Truth;', 'type 204 data Entries;', 'type 205 data Unit;', 'type 206 int Reason in 0..=400;'], [(110, 100, 'entries', 204), (120, 101, 'operation', 202), (121, 101, 'key', 200), (122, 101, 'value', 201)], [303, 306, 307, 308, 313])
    return source, rules

def balances(capacity=3, maximum=2**64-1, accounts=4, assets=2):
    before, after = read('pre', 110), read('post', 110)
    source, dest, asset = read('command', 121), read('command', 122), read('command', 123)
    a, nonce, cn = read('command', 124), read('pre', 111), read('command', 125)
    key = op('tuple', 202, [source, asset]); destination = op('tuple', 202, [dest, asset])
    src, dst = amount(before, key), amount(before, destination)
    command = read('command', 120); credit, debit, transfer = [eq(command, lit(n, 207)) for n in range(3)]
    same = eq(source, dest)
    nonce_next = op('add', nonce, lit(1))
    # Dedicated self case: source/funds checks still precede it; no destination addition.
    self_post = before
    credit_edit = edit(op('insert', 204, before, key, op('add', src, a)), {'110': ['payload'], '111': nonce_next})
    debit_edit = edit(op('update', 204, before, key, op('sub', src, a)), {'110': ['payload'], '111': nonce_next})
    first = op('update', 204, before, key, op('sub', src, a))
    second = op('insert', 204, ['payload'], destination, op('add', dst, a))
    transfer_edit = ['match_edit', first, edit(second, {'110': ['payload'], '111': nonce_next}), ['reject_edit']]
    cases = [case(not_(read('context', 130)), ['reject', 300]), case(not_(eq(cn, nonce)), ['reject', 301]), case(eq(nonce, lit(maximum)), ['reject', 302]), case(eq(a, lit(0)), ['reject', 309]), case(and_(not_(credit), not_(exists(before, key))), ['reject', 303]), case(and_(not_(credit), op('lt', src, a)), ['reject', 304]), case(and_(credit, op('lt', lit(maximum), op('add', src, a))), ['reject', 305]), case(and_(transfer, not_(same), op('lt', lit(maximum), op('add', dst, a))), ['reject', 305]), case(credit, credit_edit), case(debit, debit_edit), case(same, accept({'110': self_post, '111': nonce_next})), case(truth(), transfer_edit)]
    def total(table, asset_value):
        return op('sum_i128', table, op('match_bool', eq(op('project', ['key'], 1), lit(asset_value, 201)), ['value'], lit(0)))
    laws = [predicate(400, 'accept', eq(read('post', 111), nonce_next)), {'id': 401, 'frame': 110, 'keys': [key, destination]}, predicate(402, 'state', op('all', before, not_(op('lt', ['value'], lit(0)))))]
    for asset_value in range(2):
        pre_sum, post_sum = total(before, asset_value), total(after, asset_value)
        matches = eq(asset, lit(asset_value, 201))
        laws.append(predicate(410 + asset_value, 'accept', and_(implies(and_(credit, matches), eq(post_sum, op('add', pre_sum, a))), implies(and_(debit, matches), eq(op('add', post_sum, a), pre_sum)), implies(or_(transfer, not_(matches)), eq(pre_sum, post_sum)))))

    # Independent relation: these predicates use only inputs and point/count
    # observations, never the producer cases or edit results. Each reason law is
    # one-way and reason-scoped; all applicable laws are conjoined.
    authorized = read('context', 130)
    replay = not_(eq(cn, nonce))
    nonce_full = eq(nonce, lit(maximum))
    zero = eq(a, lit(0))
    missing = and_(not_(credit), not_(exists(before, key)))
    insufficient = and_(not_(credit), op('lt', src, a))
    overflow = or_(and_(credit, op('lt', lit(maximum), op('add', src, a))),
                   and_(transfer, not_(same), op('lt', lit(maximum), op('add', dst, a))))
    adds_key = or_(and_(credit, not_(exists(before, key))),
                   and_(transfer, not_(same), not_(exists(before, destination))))
    full = and_(adds_key, eq(op('len', 204, before), lit(capacity, 209)))
    # Fixed-width tuple/I128 items cost 72; B=29+72*C. Thus I/B cannot
    # independently refuse an otherwise eligible edit of an admitted state.
    ordered = [(300, not_(authorized)), (301, replay), (302, nonce_full),
               (309, zero), (303, missing), (304, insufficient),
               (305, overflow), (306, truth(False)), (307, full),
               (308, truth(False))]
    earlier_clear = truth()
    reasons = {}
    for reason, condition in ordered:
        reasons[reason] = and_(earlier_clear, condition)
        earlier_clear = and_(earlier_clear, not_(condition))
    for reason in sorted(reasons):
        laws.append(predicate(420 + reason - 300, 'reject',
                              implies(eq(['reason'], lit(reason, 206)), reasons[reason])))
    laws.append(predicate(432, 'accept', earlier_clear))
    def same_lookup(k):
        return and_(eq(exists(before, k), exists(after, k)),
                    eq(amount(before, k), amount(after, k)))
    src_credit = present(after, key, eq(['payload'], op('add', src, a)))
    src_debit = present(after, key, eq(['payload'], op('sub', src, a)))
    dst_transfer = present(after, destination, eq(['payload'], op('add', dst, a)))
    exact = and_(implies(credit, src_credit), implies(debit, src_debit),
                 implies(and_(or_(credit, debit), not_(same)), same_lookup(destination)),
                 implies(and_(transfer, same), eq(before, after)),
                 implies(and_(transfer, not_(same)), and_(src_debit, dst_transfer)))
    laws.append(predicate(433, 'accept', exact))
    rules = {'schema': 'zeno-fcis/tables-policy/1', 'bindings': {'202': {'kind': 'tuple', 'items': [200, 201]}, '204': {'kind': 'map', 'key': 202, 'value': 203, 'capacity': capacity, 'item_bytes': 72, 'snapshot_bytes': 29+72*capacity}}, 'genesis': {'110': [], '111': 0}, 'cases': cases, 'laws': laws, 'budgets': budgets(), 'frame_bytes': [79+64*capacity, 300, 100]}
    declarations = [f'type 200 int Account in 0..={accounts-1};', f'type 201 int Asset in 0..={assets-1};', 'type 202 data AccountAsset;', f'type 203 int Amount in 0..={maximum};', 'type 204 data Balances;', 'type 206 int Reason in 0..=400;', 'type 207 int Operation in 0..=2;', 'type 208 bool Truth;', 'type 209 int Count in 0..=3;']
    # Expressions use the common Truth type and Amount type IDs.
    def rewrite(x):
        if isinstance(x, list):
            if len(x) == 3 and x[0] == 'literal' and isinstance(x[2], bool): return ['literal', 208, x[2]]
            if len(x) == 3 and x[0] == 'literal' and x[1] == 201 and not isinstance(x[2], bool): return ['literal', 203, x[2]]
            return [rewrite(v) for v in x]
        if isinstance(x, dict): return {k: rewrite(v) for k, v in x.items()}
        return x
    # Scalar amounts produced by lit() default to 201; source/asset literals in totals are also nonnegative and numeric-compatible.
    rules = rewrite(rules)
    source_text = project('tables_balances', declarations, [(110,100,'balances',204),(111,100,'nonce',203),(120,101,'operation',207),(121,101,'source',200),(122,101,'destination',200),(123,101,'asset',201),(124,101,'amount',203),(125,101,'nonce',203),(130,102,'authorized',208)], list(range(300,310)))
    return source_text, rules

def write(root):
    root = Path(root)
    for kind in ['map', 'set']:
        for capacity in range(4):
            name = f'{kind}-{capacity}'; source, rules = generic(kind, capacity); folder = root/name; (folder/'v2').mkdir(parents=True, exist_ok=True); (folder/'project.zeno').write_text(source); (folder/'v2/policy.json').write_text(json.dumps(rules, separators=(',', ':'))+'\n')
    source, rules = balances(); folder=root/'balances'; (folder/'v2').mkdir(parents=True, exist_ok=True); (folder/'project.zeno').write_text(source); (folder/'v2/policy.json').write_text(json.dumps(rules, separators=(',', ':'))+'\n')
    for name, args in [('balances-0', (0,)), *[(f'balances-finite-{c}', (c, 1, 2, 1)) for c in range(3)]]:
        source, rules = balances(*args); folder=root/name; (folder/'v2').mkdir(parents=True, exist_ok=True); (folder/'project.zeno').write_text(source); (folder/'v2/policy.json').write_text(json.dumps(rules, separators=(',', ':'))+'\n')

if __name__ == '__main__': write(sys.argv[1])
