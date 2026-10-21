#!/usr/bin/env python3
"""Compare retained V1 projections with an unchanged archived reference.

The archive has one vault and no reserve or redistribution state. This lane
therefore labels projections explicitly; it does not claim full-state parity.
"""
import argparse
import hashlib
import importlib
import json
from pathlib import Path
import sys

PIN = "694dacc9086993a194892a4c14d86d2122ffc4ca5b54a3e5b45064c92d3f7363"
Q = 10**18
E8 = 10**8
SCALE = 10**10
ORDER = ["upward-report", "upward-pending-borrow", "pending-drop",
         "pending-seizure", "pending-withdraw", "insolvent-commit",
         "recovery-redistribution"]


def retain(path, value):
    data=json.dumps(value, indent=2)+"\n"
    if path.exists():
        assert path.read_text()==data, f"refuse changing retained result: {path.name}"
    else:
        path.write_text(data)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--reference-root", type=Path, required=True)
    parser.add_argument("--observations", type=Path, required=True)
    parser.add_argument("--out", type=Path, required=True)
    args = parser.parse_args()
    subject = args.reference_root / "src/core/zusd.py"
    assert hashlib.sha256(subject.read_bytes()).hexdigest() == PIN
    sys.path.insert(0, str(args.reference_root))
    reference = importlib.import_module("src.core.zusd")
    args.out.mkdir(parents=True, exist_ok=True)
    rows = []
    for name in ORDER:
        cells = (args.observations / (name + ".tsv")).read_text().strip().split("\t")
        assert len(cells) == 20, (name, len(cells))
        _, kind = cells[:2]
        amount, epoch, committed, pending, collateral, debt, pool, total_debt, supply = map(int, cells[2:11])
        accepted = cells[11] == "Ok(())"
        after_c, after_d, after_pool, default_c, default_d, gas, keeper_a, keeper_z = map(int, cells[12:])
        assert total_debt == supply
        assert all(value % SCALE == 0 for value in (amount, collateral, debt, pool)) if kind not in ("oracle_report", "oracle_commit", "liquidate") else True
        kwargs = dict(now_epoch=epoch, oracle_seen=True, oracle_last_update_epoch=2,
                      price_e8=committed, price_pending_e8=pending,
                      max_oracle_staleness_epochs=50_000,
                      collateral_e8=collateral // SCALE, debt_e8=debt // SCALE,
                      sp_debt_e8=pool // SCALE, free_debt_e8=(debt-pool) // SCALE,
                      min_debt_open_e8=500*E8,
                      borrow_fee_floor_bps=50, borrow_fee_max_bps=500,
                      redemption_fee_floor_bps=50, redemption_fee_max_bps=10_000,
                      liquidation_gas_comp_bps=50)
        command = {"auth_ok": True, "price_e8": amount} if kind == "oracle_report" else (
            {"auth_ok": True} if kind == "oracle_commit" else (
                {} if kind == "liquidate" else {"amount_e8": amount // SCALE}))
        retained = dict(state=kwargs, command=dict(kind=kind, **command))
        try:
            pre = reference.ZUSDState(**kwargs)
        except ValueError as error:
            assert pending > committed and str(error)=="require price_pending_e8 <= price_e8", (name, str(error))
            archived = dict(accept=False, domain_refusal=str(error))
        else:
            tx = reference.ZUSDCommand(tag=kind, args=command)
            decision = reference._step_python(pre, tx)
            archived = reference._result_to_authority_doc(pre, tx, decision)
        retain(args.out / (name + "-input.json"), retained)
        retain(args.out / (name + "-archived.json"), archived)
        # Independent exact integer requirements; never derive an expectation
        # by copying a native decision or editing the archived implementation.
        if name == "upward-report":
            assert accepted and amount > committed and not archived["accept"]
            assert (after_c, after_d) == (collateral, debt)
            expected = "positive upward report admitted; no monetary movement"
        elif name == "upward-pending-borrow":
            fee = amount * (Q//200) // Q
            assert accepted and after_d == debt+amount+fee
            assert after_c == collateral and collateral*committed*SCALE//after_d >= 11*Q//10
            assert "domain_refusal" in archived
            expected = "committed-price borrow with exact V1 floor fee"
        elif name == "pending-drop":
            assert accepted and not archived["accept"] and "domain_refusal" in archived and amount < committed
            assert (after_c, after_d) == (collateral, debt)
            expected = "downward replacement changes no money; archive cannot represent the preceding upward pending state"
        elif name == "pending-seizure":
            assert not accepted and archived["accept"]
            assert collateral*committed*SCALE//debt >= 11*Q//10
            assert collateral*pending*SCALE//debt < Q
            expected = "committed eligibility rejects; pending cannot authorize seizure"
        elif name == "pending-withdraw":
            assert not accepted and not archived["accept"]
            assert collateral*pending*SCALE//debt < 11*Q//10
            expected = "both-price withdrawal restriction; archived blanket-freeze reason differs"
        elif name == "insolvent-commit":
            assert accepted and not archived["accept"]
            assert collateral*pending*SCALE < debt*Q
            assert (after_c, after_d) == (collateral, debt)
            expected = "price commit independent of solvency; no debt erasure"
        else:
            assert accepted and archived["accept"]
            keeper = collateral//200
            assert (default_c, default_d) == (collateral-keeper, debt)
            assert (after_c, after_d) == (0, 0)
            assert keeper_a == 20_000*Q+keeper and keeper_z == 50*Q
            assert after_pool == 1100*Q and gas == 50*Q
            assert int(archived["post_state"]["debt_e8"]) == 0
            expected = "ICR<=100% redistributes composite debt, leaves SP unchanged, transfers reserve once"
        rows.append(dict(case=name, native_accept=accepted, archived_accept=archived["accept"],
                         classification="intentional-listed-V1-departure",
                         independently_checked_expected=expected,
                         full_state_parity=False))
    assert hashlib.sha256(subject.read_bytes()).hexdigest() == PIN
    result = dict(reference_sha256=PIN, reference_changed=False, rows=rows,
                  defects=[], limitations=[
                      "Native state threads every V1 observation; each archived call receives the same selected economic projection.",
                      "After a departure the archive cannot thread the native full state: other vaults, reserve custody and redistribution have no archived representation.",
                      "All money is exactly converted E18/E8; pool availability is clamped to selected composite debt for the one-vault archive.",
                      "Complete normalized parity belongs to the separate 17-case archived mount, not these projections."])
    retain(args.out / "summary.json", result)
    print("PASS: 7 projected comparisons; 7 listed departures independently checked; full V1 parity remains open")


if __name__ == "__main__":
    main()
