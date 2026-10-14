#!/usr/bin/env python3
"""Compare offline data proposals with untouched pinned Python command handlers.

This is a deterministic characterization corpus, not exhaustive equivalence.
It never replaces current-build Rust Authority/SQLite or formal proof gates.
"""
import dataclasses
import hashlib
import json
from pathlib import Path

from compile_zusd import E8, MAX, U128_MAX, UNREACHABLE_REASONS, compile_model, original, reasons


def fixtures(native):
    base = dataclasses.asdict(native.ZUSDState())
    healthy = base | dict(oracle_seen=True, price_e8=E8, price_pending_e8=E8,
                         collateral_e8=400*E8, debt_e8=200*E8, free_debt_e8=200*E8)
    rows = [('initial', base), ('healthy', healthy)]
    rows += [
        ('pool-funded', healthy | dict(sp_debt_e8=100*E8, free_debt_e8=100*E8)),
        ('pool-full', healthy | dict(sp_debt_e8=200*E8, free_debt_e8=0)),
        ('oracle-unseen-debt', healthy | dict(oracle_seen=False, price_e8=0, price_pending_e8=0)),
        ('pending-lower', healthy | dict(price_pending_e8=9*E8//10)),
        ('stale', healthy | dict(now_epoch=101)),
        ('recovery', healthy | dict(collateral_e8=240*E8)),
        ('below-mcr-pending', healthy | dict(collateral_e8=240*E8, price_pending_e8=8*E8//10)),
        ('liquidation-funded', healthy | dict(collateral_e8=240*E8, price_pending_e8=8*E8//10,
                                            sp_debt_e8=200*E8, free_debt_e8=0)),
        ('liquidation-compensation', healthy | dict(collateral_e8=240*E8, price_pending_e8=8*E8//10,
            sp_debt_e8=200*E8, free_debt_e8=0, liquidation_gas_comp_fixed_collateral_e8=E8,
            liquidation_gas_comp_bps=123)),
        ('pool-cap', healthy | dict(collateral_e8=240*E8, price_pending_e8=8*E8//10,
            sp_debt_e8=200*E8, free_debt_e8=0, max_sp_coll_e8=1)),
        ('borrow-fees', healthy | dict(borrow_fee_floor_bps=123, base_rate_bps=27,
            base_rate_borrow_bump_bps=13)),
        ('redeem-fees', healthy | dict(redemption_fee_floor_bps=123, base_rate_bps=27,
            base_rate_redeem_bump_bps=13)),
        ('decay', healthy | dict(now_epoch=4, base_rate_bps=127, base_rate_decay_per_epoch_bps=30,
            borrow_fee_floor_bps=23, redemption_fee_floor_bps=23)),
        ('saturated-decay', healthy | dict(now_epoch=MAX, oracle_last_update_epoch=MAX,
            base_rate_bps=1, base_rate_decay_per_epoch_bps=10_000)),
        ('redeem-consuming-fee', healthy | dict(redemption_fee_floor_bps=10_000,
            redemption_fee_max_bps=10_000)),
        ('protocol-cap', healthy | dict(redemption_fee_floor_bps=123, max_protocol_coll_e8=1)),
        ('mint-vault-cap', healthy | dict(max_debt_e8=200*E8)),
        ('mint-supply-cap', healthy | dict(max_debt_e8=200*E8, max_debt_supply_e8=200*E8)),
        ('revenue-bound', healthy | dict(protocol_revenue_zusd_cum_e8=MAX,
            borrow_fee_floor_bps=123)),
        ('collateral-bound', base | dict(collateral_e8=MAX)),
        ('epoch-bound', base | dict(now_epoch=MAX)),
        ('wide-products', healthy | dict(collateral_e8=10**20, debt_e8=10**20,
            free_debt_e8=10**20, price_e8=10**20, price_pending_e8=10**20,
            max_debt_e8=MAX, max_debt_supply_e8=MAX)),
        ('wide-mcr-parameter', healthy | dict(collateral_e8=MAX, debt_e8=1, free_debt_e8=1,
            min_debt_open_e8=1, price_e8=MAX, price_pending_e8=MAX,
            mcr_bps=MAX, ccr_bps=MAX, max_debt_e8=MAX, max_debt_supply_e8=MAX)),
        # Original Python permits construction with broken supply conservation;
        # it checks this in its shared post-state tail, after command checks.
        ('invalid-supply-post-tail', healthy | dict(free_debt_e8=0)),
        # These pre-states satisfy every declared state law and reach the
        # remaining refusals: a bootstrapped oracle with no debt (mint below
        # the debt floor, liquidation without debt) and debt just above the
        # floor (repay and redemption below it).
        ('bootstrapped-no-debt', base | dict(oracle_seen=True, price_e8=E8, price_pending_e8=E8)),
        ('debt-near-floor', healthy | dict(debt_e8=150*E8, free_debt_e8=150*E8)),
        # A vault below MCR at the active price, with the system kept solvent
        # by pool collateral. The laws allow it; the single-vault commands never
        # produce it from genesis. Withdrawing pool debt is then blocked, a
        # redemption above the vault's collateral is refused, and a smaller
        # redemption leaves the vault below MCR.
        ('vault-below-mcr-pool-backed', healthy | dict(collateral_e8=200*E8, sp_debt_e8=100*E8,
            free_debt_e8=100*E8, sp_coll_e8=200*E8)),
        ('vault-short-pool-backed', healthy | dict(collateral_e8=50*E8, sp_coll_e8=300*E8)),
        # Mint above the supply cap needs free debt above the vault's debt,
        # which breaks supply conservation (law 501).
        ('free-debt-above-debt', healthy | dict(free_debt_e8=300*E8, max_debt_e8=300*E8,
            max_debt_supply_e8=300*E8)),
        # A pool deposit above the supply cap needs debt above the vault cap.
        # Only a capped mint raises debt, so this is unreachable from genesis,
        # although no declared state law forbids it.
        ('debt-above-supply-cap', healthy | dict(max_debt_e8=50*E8, max_debt_supply_e8=50*E8)),
    ]
    for label, state in rows:
        value = native.ZUSDState(**state)
        yield label, value


def characterize():
    native = original()
    graph, roots, handlers = compile_model(native)
    registry = reasons(); reverse = {v: k for k, v in registry.items()}
    tags = list(native._ZUSD_STEP_HANDLERS)
    amounts = [0, 1, E8, 100*E8, 2**63, MAX-1, MAX, MAX+1, U128_MAX]
    rows, mismatch, accepting = [], [], set()
    for label, state in fixtures(native):
        input_state = [int(getattr(state, name)) for name in native.ZUSD_STATE_FIELD_ORDER]
        for action in range(0, 14):
            tag = tags[action-1] if 1 <= action <= len(tags) else 'unknown'
            for amount in amounts:
                for auth in [0, 1]:
                    inputs = input_state + [action, amount, auth]
                    args = dict(amount_e8=amount, price_e8=amount, delta=amount, auth_ok=bool(auth))
                    expected = native._step_python(state, native.ZUSDCommand(tag=tag, args=args))
                    values = graph.evaluate(inputs)
                    code = values[roots[0]]
                    observed_ok = 1 <= code <= 12
                    row = {'fixture': label, 'inputs': [str(v) for v in inputs],
                           'accept': expected.ok, 'reason': None if expected.ok else native._error_to_code(expected.error),
                           'post': [str(int(getattr(expected.state if expected.ok else state, name)))
                                    for name in native.ZUSD_STATE_FIELD_ORDER],
                           'effects': expected.effects if expected.ok else None}
                    observed = {'accept': observed_ok, 'reason': None if observed_ok else reverse[code],
                                'post': [str(values[root]) for root in roots[1:]]}
                    same = all(observed[k] == row[k] for k in observed)
                    if observed_ok:
                        effect_nodes = handlers[code-1][3]
                        effects = {k: v if k == 'event' else values[v] for k, v in effect_nodes.items()}
                        same &= effects == row['effects']
                    if not same:
                        mismatch.append({'input': row, 'observed': observed})
                        if len(mismatch) >= 8: break
                    if expected.ok: accepting.add(tag)
                    rows.append(row)
                if len(mismatch) >= 8: break
            if len(mismatch) >= 8: break
        if len(mismatch) >= 8: break
    assert not mismatch, json.dumps(mismatch, indent=2)
    assert accepting == set(tags), ('commands without accepting witnesses', set(tags)-accepting)
    observed = {registry[row['reason']] for row in rows if row['reason']}
    reachable = set(registry.values()) - set(UNREACHABLE_REASONS)
    assert observed == reachable, ('reachable reasons not exercised', sorted(reachable - observed),
                                   'unreachable reasons exercised', sorted(observed - reachable))
    content = ''.join(json.dumps(row, sort_keys=True, separators=(',', ':'))+'\n' for row in rows)
    report = {'status': 'offline-characterization-pass', 'rows': len(rows),
              'nodes': len(graph.nodes), 'roots': len(roots), 'commands': len(tags),
              'registered_reasons': len(registry), 'observed_reasons': sorted({row['reason'] for row in rows if row['reason']}),
              'unreachable_reasons': {str(k): v for k, v in sorted(UNREACHABLE_REASONS.items())},
              'accepting_commands': sorted(accepting), 'raw_amount_max': str(U128_MAX),
              'wide_deposit_accepts': sum(row['accept'] and row['inputs'][32]=='5' and row['inputs'][33]==str(2**63) for row in rows),
              'corpus_sha256': hashlib.sha256(content.encode()).hexdigest(),
              'native_execution': 'pinned original Python single-vault statements',
              'exhaustive_equivalence': False, 'rust_authority_checked': False,
              'sqlite_checked': False, 'publication_authority': False}
    return rows, report


def main():
    import argparse
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--out', type=Path, required=True)
    args = parser.parse_args()
    rows, report = characterize()
    args.out.mkdir(parents=True, exist_ok=True)
    content = ''.join(json.dumps(row, sort_keys=True, separators=(',', ':'))+'\n' for row in rows)
    (args.out/'native-characterization.jsonl').write_text(content)
    (args.out/'offline-result.json').write_text(json.dumps(report, indent=2)+'\n')
    print(json.dumps(report))


if __name__ == '__main__': main()
