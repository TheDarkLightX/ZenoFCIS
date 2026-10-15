#!/usr/bin/env python3
"""Generate data-only numeric graph proposals for the pinned single-vault model.

This offline compiler is not publication authority or a proof of equivalence.
The original model remains an independent execution oracle. Production execution
will use ZenoFCIS's checked NumericDescriptor, laws, Authority and SQLite store.
"""
from __future__ import annotations

import ast
import dataclasses
import hashlib
import json
import sys
import types
import typing
from pathlib import Path

PIN = 'e3cf1aad40e487893230ae0c55f1f0dda62f9955'
SOURCE_SHA256 = '694dacc9086993a194892a4c14d86d2122ffc4ca5b54a3e5b45064c92d3f7363'
ROOT = Path(__file__).resolve().parents[3]
SOURCE = Path(__file__).with_name('original_zusd.py')
SPEC = ROOT / 'crates/zeno-fcis-spec/tests/data/zusd_lane_v1.zeno'
MAX = 10**30
U128_MAX = 2**128 - 1
E8, BPS = 100_000_000, 10_000


def original():
    raw = SOURCE.read_bytes()
    assert hashlib.sha256(raw).hexdigest() == SOURCE_SHA256
    tree = ast.parse(raw, filename=str(SOURCE))
    # Preserve original statements, including dataclass checks, helpers, exact
    # handlers and dispatch. Shell/hash and the separate multivault model are
    # deliberately not invoked. No native decision statement is rewritten.
    nodes = [n for n in tree.body if not isinstance(n, (ast.Import, ast.ImportFrom))
             and getattr(n, 'end_lineno', 0) <= 856]
    module = types.ModuleType('zenofcis_pinned_zusd_oracle')
    module.__dict__.update(dataclass=dataclasses.dataclass, Any=typing.Any,
                           Literal=typing.Literal, Mapping=typing.Mapping, cast=typing.cast)
    sys.modules[module.__name__] = module
    exec(compile(ast.Module(body=nodes, type_ignores=[]), str(SOURCE), 'exec',
                 flags=__import__('__future__').annotations.compiler_flag), module.__dict__)
    assert len(module.ZUSD_STATE_FIELD_ORDER) == 32
    assert len(module._ZUSD_STEP_HANDLERS) == 12
    return module


class Graph:
    def __init__(self):
        self.nodes, self.index = [], {}

    def put(self, op, *args):
        node = (op, *args)
        if node not in self.index:
            self.index[node] = len(self.nodes)
            self.nodes.append(node)
        return self.index[node]

    def u(self, value):
        assert type(value) is int and 0 <= value <= U128_MAX
        return self.put('U128', value)

    def i(self, value):
        assert type(value) is int and -(2**127) <= value < 2**127
        return self.put('I128', value)

    def eq(self, a, b): return self.put('Eq', a, b)
    def lt(self, a, b): return self.put('Lt', a, b)
    def neg(self, a): return self.put('Not', a)
    def le(self, a, b): return self.neg(self.lt(b, a))
    def ge(self, a, b): return self.neg(self.lt(a, b))
    def choose(self, c, a, b):
        return a if a == b else self.put('Select', c, a, b)
    def add(self, a, b): return self.put('Add', a, b)
    def sub(self, a, b):
        # Policy deliberately uses a total saturated expression. All branch
        # checks that make subtraction meaningful remain earlier in its reason
        # chain. This does not rewrite the interpreter's eager trap semantics.
        return self.put('Sub', self.choose(self.lt(a, b), b, a), b)
    def minimum(self, a, b): return self.choose(self.lt(a, b), a, b)
    def maximum(self, a, b): return self.choose(self.lt(a, b), b, a)
    def both(self, *items):
        out = self.put('Bool', True)
        for item in items: out = self.put('And', out, item)
        return out
    def either(self, *items):
        return self.neg(self.both(*(self.neg(x) for x in items)))
    def floor(self, a, b, den): return self.put('MulDivFloor', a, b, den)
    def ceil(self, a, b, den): return self.put('MulDivCeil', a, b, den)
    def products_ge(self, left, right): return self.put('ProductsGe', tuple(left), tuple(right))

    def evaluate(self, inputs):
        values = []
        for op, *args in self.nodes:
            if op == 'Input': value = inputs[args[0]]
            elif op in ('U128', 'I128', 'Bool'): value = args[0]
            elif op == 'Add': value = values[args[0]] + values[args[1]]
            elif op == 'Sub': value = values[args[0]] - values[args[1]]
            elif op == 'Eq': value = values[args[0]] == values[args[1]]
            elif op == 'Lt': value = values[args[0]] < values[args[1]]
            elif op == 'And': value = values[args[0]] and values[args[1]]
            elif op == 'Not': value = not values[args[0]]
            elif op == 'Select': value = values[args[1] if values[args[0]] else args[2]]
            elif op in ('MulDivFloor', 'MulDivCeil'):
                a, b, den = (values[x] for x in args)
                assert den > 0
                value, remainder = divmod(a * b, den)
                if op == 'MulDivCeil' and remainder: value += 1
            elif op == 'ProductsGe':
                def product(indices):
                    out = 1
                    for index in indices: out *= values[index]
                    return out
                value = product(args[0]) >= product(args[1])
            else: raise AssertionError(op)
            if op not in ('I128', 'Bool', 'Eq', 'Lt', 'And', 'Not', 'ProductsGe'):
                assert type(value) is bool or 0 <= value <= U128_MAX, (op, args, value)
            values.append(value)
        return values

    def rust(self, node):
        op, *a = node
        if op in ('Add', 'Sub'):
            return f'N::{op}(Width::Unsigned128, {a[0]}, {a[1]})'
        if op == 'ProductsGe':
            arrays = [str(list(x)) for x in a]
            return f'N::{op}({", ".join(arrays)})'
        values = ', '.join(str(x).lower() if type(x) is bool else str(x) for x in a)
        return f'N::{op}({values})'


def reasons():
    import re
    rows = re.findall(r'^reason (\d+) (\w+) precedence (\d+);$', SPEC.read_text(), re.M)
    assert len(rows) == 46 and [int(x[0]) for x in rows] == list(range(200, 246))
    return {name: int(number) for number, name, _ in rows}


# Registered reasons that no input reaches through the pinned single-vault
# Python: none of its error strings maps to them. They exist only in the
# registry above and keep their branches; no corpus row can exercise them.
UNREACHABLE_REASONS = {
    210: 'commit_stale_oracle: oracle_commit checks bootstrap, authorization and MCR, never staleness',
    239: 'liquidate_pending_mismatch: liquidate reads only the pending price, never its mismatch',
    240: 'liquidate_stale_oracle: liquidate never checks oracle staleness',
}


def compile_model(native):
    g = Graph()
    names = native.ZUSD_STATE_FIELD_ORDER
    state = {name: g.put('Input', i) for i, name in enumerate(names)}
    action, raw_amount, auth = (g.put('Input', i) for i in range(32, 35))
    z, one, bound, scale, e8 = (g.u(x) for x in (0, 1, MAX, BPS, E8))
    amount = g.minimum(raw_amount, bound)
    oversized = g.lt(bound, raw_amount)
    positive = g.neg(g.eq(raw_amount, z))
    authorized = g.eq(auth, one)
    registry = reasons()
    tags = list(native._ZUSD_STEP_HANDLERS)

    def mcr(s, collateral=None, debt=None, price=None):
        return g.products_ge((s['collateral_e8'] if collateral is None else collateral,
                              s['price_e8'] if price is None else price, scale),
                             (s['debt_e8'] if debt is None else debt, s['mcr_bps'], e8))

    def fresh(s):
        return g.le(g.sub(s['now_epoch'], s['oracle_last_update_epoch']), s['max_oracle_staleness_epochs'])

    def risky(s):
        total = g.add(g.add(s['collateral_e8'], s['sp_coll_e8']), s['protocol_collateral_e8'])
        tcr = g.products_ge((total, s['price_e8'], scale), (s['debt_e8'], s['ccr_bps'], e8))
        return g.both(g.eq(s['oracle_seen'], one), g.lt(z, s['price_e8']),
                      g.lt(z, s['price_pending_e8']), g.eq(s['price_e8'], s['price_pending_e8']),
                      fresh(s), tcr)

    def floor_ok(s, debt):
        return g.either(g.eq(debt, z), g.ge(debt, s['min_debt_open_e8']))

    def shape(s):
        seen = g.eq(s['oracle_seen'], one)
        prices = g.both(g.lt(z, s['price_e8']), g.lt(z, s['price_pending_e8']),
                        g.le(s['price_pending_e8'], s['price_e8']))
        zeroed = g.both(g.eq(s['price_e8'], z), g.eq(s['price_pending_e8'], z),
                        g.eq(s['oracle_last_update_epoch'], z))
        bps_names = ('base_rate_bps', 'base_rate_decay_per_epoch_bps',
                     'base_rate_borrow_bump_bps', 'base_rate_redeem_bump_bps',
                     'borrow_fee_max_bps', 'redemption_fee_max_bps', 'liquidation_gas_comp_bps')
        return g.both(g.le(s['oracle_last_update_epoch'], s['now_epoch']),
                      g.le(s['base_rate_last_epoch'], s['now_epoch']),
                      g.choose(seen, prices, zeroed), g.lt(z, s['mcr_bps']),
                      g.le(s['mcr_bps'], s['ccr_bps']), g.le(s['max_debt_e8'], s['max_debt_supply_e8']),
                      *(g.le(s[name], scale) for name in bps_names),
                      g.le(s['borrow_fee_floor_bps'], s['borrow_fee_max_bps']),
                      g.le(s['redemption_fee_floor_bps'], s['redemption_fee_max_bps']))

    def invariants(s):
        total = g.add(g.add(s['collateral_e8'], s['sp_coll_e8']), s['protocol_collateral_e8'])
        price = g.choose(g.lt(z, s['price_e8']), s['price_e8'], e8)
        solvent = g.products_ge((total, price, one), (s['debt_e8'], e8, one))
        return g.both(g.eq(g.add(s['free_debt_e8'], s['sp_debt_e8']), s['debt_e8']),
                      floor_ok(s, s['debt_e8']), solvent)

    elapsed = g.sub(state['now_epoch'], state['base_rate_last_epoch'])
    decay = g.floor(g.minimum(state['base_rate_decay_per_epoch_bps'], scale), elapsed, one)
    decayed = g.sub(state['base_rate_bps'], decay)
    def fee(which):
        return g.minimum(g.minimum(g.add(state[f'{which}_fee_floor_bps'], decayed),
                                   state[f'{which}_fee_max_bps']), scale)

    results = []
    def finish(tag, checks, updates, effects):
        post = state | updates
        within = g.both(*(g.le(post[name], bound) for name in names))
        result = g.i(tag)
        # Native constructor bounds/shape precede shared post-state invariants.
        tail = [(g.neg(within), 'bounded_check_failed'),
                (g.neg(shape(post)), 'invariant_violation'),
                (g.neg(invariants(post)), 'invariant_violation')]
        for condition, reason in reversed(checks + tail):
            result = g.choose(condition, g.i(registry[reason]), result)
        results.append((tag, result, post, effects))

    not_positive = (g.neg(positive), 'not_positive_int')
    ns = g.add(state['now_epoch'], amount)
    finish(1, [not_positive, (g.either(oversized, g.lt(bound, ns)), 'bounded_check_failed')],
           {'now_epoch': ns}, {'event': 'epoch_advanced', 'delta': raw_amount})
    seen = g.eq(state['oracle_seen'], one)
    finish(2, [(seen, 'oracle_already_bootstrapped'),
               (g.neg(authorized), 'bootstrap_requires_auth'), not_positive,
               (oversized, 'bounded_check_failed')],
           {'oracle_seen': one, 'oracle_last_update_epoch': state['now_epoch'],
            'price_e8': amount, 'price_pending_e8': amount},
           {'event': 'oracle_bootstrapped', 'price_e8': raw_amount})
    finish(3, [(g.neg(seen), 'oracle_not_bootstrapped'),
               (g.neg(authorized), 'report_requires_auth'), not_positive,
               (g.lt(state['price_pending_e8'], raw_amount), 'report_price_not_non_increasing')],
           {'price_pending_e8': amount}, {'event': 'oracle_reported', 'price_pending_e8': raw_amount})
    finish(4, [(g.neg(seen), 'oracle_not_bootstrapped'),
               (g.neg(authorized), 'commit_requires_auth'),
               (g.neg(mcr(state, price=state['price_pending_e8'])), 'commit_below_mcr')],
           {'price_e8': state['price_pending_e8'], 'oracle_last_update_epoch': state['now_epoch']},
           {'event': 'oracle_committed', 'price_e8': state['price_pending_e8']})
    coll = g.add(state['collateral_e8'], amount)
    finish(5, [not_positive, (g.either(oversized, g.lt(bound, coll)), 'bounded_check_failed')],
           {'collateral_e8': coll}, {'event': 'collateral_deposited', 'amount_e8': raw_amount})
    coll = g.sub(state['collateral_e8'], amount)
    finish(6, [not_positive, (g.lt(state['collateral_e8'], raw_amount), 'insufficient_collateral'),
               (g.both(g.lt(z, state['debt_e8']), g.neg(risky(state))), 'withdraw_blocked_oracle'),
               (g.neg(mcr(state, collateral=coll)), 'withdraw_violates_mcr')],
           {'collateral_e8': coll}, {'event': 'collateral_withdrawn', 'amount_e8': raw_amount})
    fee_bps = fee('borrow'); fee_amount = g.ceil(amount, fee_bps, scale)
    delta = g.add(amount, fee_amount); debt = g.add(state['debt_e8'], delta)
    free = g.add(state['free_debt_e8'], delta)
    finish(7, [not_positive, (g.neg(risky(state)), 'mint_blocked_oracle'),
               (g.both(g.eq(state['debt_e8'], z), g.lt(raw_amount, state['min_debt_open_e8'])), 'mint_below_min_debt'),
               (g.either(oversized, g.lt(state['max_debt_e8'], debt)), 'mint_exceeds_max_debt'),
               (g.lt(state['max_debt_supply_e8'], free), 'mint_exceeds_max_supply'),
               (g.neg(mcr(state, debt=debt)), 'mint_violates_mcr')],
           {'debt_e8': debt, 'free_debt_e8': free,
            'protocol_revenue_zusd_cum_e8': g.add(state['protocol_revenue_zusd_cum_e8'], fee_amount),
            'base_rate_bps': g.minimum(scale, g.add(decayed, state['base_rate_borrow_bump_bps'])),
            'base_rate_last_epoch': state['now_epoch']},
           {'event': 'zusd_minted', 'principal_e8': raw_amount, 'mint_fee_e8': fee_amount,
            'mint_fee_bps': fee_bps, 'debt_delta_e8': delta})
    debt = g.sub(state['debt_e8'], amount)
    finish(8, [not_positive, (g.lt(state['debt_e8'], raw_amount), 'repay_exceeds_debt'),
               (g.lt(state['free_debt_e8'], raw_amount), 'repay_exceeds_free_debt'),
               (g.neg(floor_ok(state, debt)), 'repay_below_min_debt')],
           {'debt_e8': debt, 'free_debt_e8': g.sub(state['free_debt_e8'], amount)},
           {'event': 'zusd_repaid', 'amount_e8': raw_amount})
    sp = g.add(state['sp_debt_e8'], amount)
    finish(9, [not_positive, (g.lt(state['free_debt_e8'], raw_amount), 'deposit_sp_exceeds_free_debt'),
               (g.lt(state['max_debt_supply_e8'], sp), 'deposit_sp_exceeds_max_supply')],
           {'free_debt_e8': g.sub(state['free_debt_e8'], amount), 'sp_debt_e8': sp},
           {'event': 'sp_deposited', 'amount_e8': raw_amount})
    finish(10, [not_positive, (g.lt(state['sp_debt_e8'], raw_amount), 'withdraw_sp_exceeds_sp_debt'),
                (g.neg(risky(state)), 'withdraw_sp_blocked_oracle'),
                (g.neg(mcr(state)), 'withdraw_sp_below_mcr')],
           {'sp_debt_e8': g.sub(state['sp_debt_e8'], amount),
            'free_debt_e8': g.add(state['free_debt_e8'], amount)},
           {'event': 'sp_withdrawn', 'amount_e8': raw_amount})
    gross = g.floor(amount, e8, g.maximum(state['price_e8'], one))
    fee_bps = fee('redemption'); coll_fee = g.ceil(gross, fee_bps, scale)
    debt = g.sub(state['debt_e8'], amount); coll = g.sub(state['collateral_e8'], gross)
    protocol = g.add(state['protocol_collateral_e8'], coll_fee)
    finish(11, [not_positive,
                (g.either(g.neg(seen), g.eq(state['price_e8'], z), g.eq(state['price_pending_e8'], z)), 'redeem_oracle_uninitialized'),
                (g.neg(g.eq(state['price_pending_e8'], state['price_e8'])), 'redeem_pending_mismatch'),
                (g.neg(fresh(state)), 'redeem_stale_oracle'),
                (g.lt(state['debt_e8'], raw_amount), 'redeem_exceeds_debt'),
                (g.lt(state['free_debt_e8'], raw_amount), 'redeem_exceeds_free_debt'),
                (g.eq(gross, z), 'redeem_amount_too_small'),
                (g.lt(state['collateral_e8'], gross), 'redeem_insufficient_collateral'),
                (g.ge(coll_fee, gross), 'redeem_fee_consumes_all'),
                (g.lt(state['max_protocol_coll_e8'], protocol), 'redeem_protocol_cap_exceeded'),
                (g.neg(floor_ok(state, debt)), 'redeem_below_min_debt'),
                (g.neg(mcr(state, collateral=coll, debt=debt)), 'redeem_violates_mcr')],
           {'debt_e8': debt, 'free_debt_e8': g.sub(state['free_debt_e8'], amount),
            'collateral_e8': coll, 'protocol_collateral_e8': protocol,
            'base_rate_bps': g.minimum(scale, g.add(decayed, state['base_rate_redeem_bump_bps'])),
            'base_rate_last_epoch': state['now_epoch']},
           {'event': 'zusd_redeemed', 'redeemed_zusd_e8': raw_amount,
            'redeemed_collateral_gross_e8': gross, 'redeemed_collateral_out_e8': g.sub(gross, coll_fee),
            'redemption_fee_collateral_e8': coll_fee, 'redemption_fee_bps': fee_bps})
    variable = g.ceil(state['collateral_e8'], g.minimum(state['liquidation_gas_comp_bps'], scale), scale)
    comp = g.minimum(state['collateral_e8'], g.add(state['liquidation_gas_comp_fixed_collateral_e8'], variable))
    gain = g.sub(state['collateral_e8'], comp); pool = g.add(state['sp_coll_e8'], gain)
    finish(12, [(g.either(g.neg(seen), g.eq(state['price_pending_e8'], z)), 'liquidate_oracle_uninitialized'),
                (g.eq(state['debt_e8'], z), 'liquidate_no_debt'),
                (mcr(state, price=state['price_pending_e8']), 'liquidate_not_under_mcr'),
                (g.lt(state['sp_debt_e8'], state['debt_e8']), 'liquidate_sp_cannot_absorb'),
                (g.lt(state['max_sp_coll_e8'], pool), 'liquidate_sp_cap_exceeded')],
           {'debt_e8': z, 'collateral_e8': z,
            'sp_debt_e8': g.sub(state['sp_debt_e8'], state['debt_e8']), 'sp_coll_e8': pool,
            'liquidator_compensation_collateral_cum_e8': g.add(state['liquidator_compensation_collateral_cum_e8'], comp)},
           {'event': 'liquidated', 'liquidated_debt_e8': state['debt_e8'],
            'liquidated_collateral_e8': state['collateral_e8'], 'sp_collateral_gain_e8': gain,
            'liquidator_compensation_collateral_e8': comp,
            'liquidation_gas_comp_fixed_collateral_e8': state['liquidation_gas_comp_fixed_collateral_e8'],
            'liquidation_gas_comp_bps': state['liquidation_gas_comp_bps']})
    selected = g.i(registry['unknown_action'])
    post = dict(state)
    for tag, code, values, _ in reversed(results):
        choose = g.eq(action, g.u(tag))
        selected = g.choose(choose, code, selected)
        for name in names: post[name] = g.choose(choose, values[name], post[name])
    accepting = g.lt(selected, g.i(200))
    roots = [selected] + [g.choose(accepting, post[name], state[name]) for name in names]
    assert len(g.nodes) <= 4096 and len(roots) == 33
    return g, roots, results


def proposal():
    native = original()
    graph, roots, results = compile_model(native)
    return {'schema': 'zeno-fcis/original-zusd-graph-proposal/1',
            'native_revision': PIN, 'native_source_sha256': SOURCE_SHA256,
            'native_scope': 'single-vault; Python typed states; no multivault or deployment claim',
            'state_field_order': list(native.ZUSD_STATE_FIELD_ORDER),
            'command_order': list(native._ZUSD_STEP_HANDLERS), 'reason_registry': reasons(),
            'state_bounds': {'min': '0', 'max': str(MAX), 'oracle_seen': ['0', '1']},
            'command_amount_bounds': {'min': '0', 'max': str(U128_MAX)},
            'unrepresentable_native_arguments': 'Python integers >=2^128 or negative are outside the U128 ABI',
            'nodes': graph.nodes, 'roots': roots,
            'effects': {str(tag): effects for tag, _, _, effects in results},
            'rust_nodes': [graph.rust(node) for node in graph.nodes],
            'formal_equivalence_claim': False, 'publication_authority': False}


if __name__ == '__main__':
    packet = proposal()
    destination = Path(sys.argv[1]) if len(sys.argv) > 1 else Path(__file__).with_name('proposal.json')
    destination.write_text(json.dumps(packet, indent=2, sort_keys=True) + '\n')
    print(json.dumps({'nodes': len(packet['nodes']), 'roots': len(packet['roots']),
                      'commands': len(packet['command_order']), 'reasons': len(packet['reason_registry']),
                      'out': str(destination), 'authority': 'none'}))
