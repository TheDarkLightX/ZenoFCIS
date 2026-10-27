# Spot pool example

A constant-product pool with five commands: create, exact-input swap,
exact-output swap, add liquidity and remove liquidity. The example uses two
ordinary accounts, one asset pair, a fixed fee and permanently locked shares.
The complete contract checks pool and balance updates together. Emitted plans
require a separate atomic publication adapter.

Run the local checks from this directory:

```sh
python3 -m unittest discover -s tests -v
python3 tools/check_bundle.py
python3 tools/check_no_free_value.py --bound 4 --coalitions all --out "$CHECK_OUTPUT"
python3 tools/run_stage3_checks.py --cli "$ZENO_FCIS_CLI" --out "$CHECK_DIRECTORY"
```

Select the built library CLI with `ZENO_FCIS_CLI` and provide external output
locations through `CHECK_OUTPUT` and `CHECK_DIRECTORY` before running the
corresponding commands. The native checks include fee-event mutations and
known legitimate accepts. The sequence checker reports its completed bound
and retains any counterexample; an incomplete depth is never a passing bound.

No-free-value checks primarily value each account's shares by its floored
current burn payout. Exact pro-rata valuation is a separate guard; coalition
checks also retain a hypothetical pooled-share upper bound. Rejections retain
state and emit no effects.

Evidence is finite oracle exploration, native observation comparison and
Attested finite-domain law checking. These establish their declared examples
and bounds. They do not establish an any-length No-free-value theorem,
full-range refinement, authenticated custody, durable publication or release
qualification, and no KernelChecked proof is claimed for them.

The Lean Spot theorems for the rational contract model are classical because core `Rat` operations in Lean 4.30 depend on `Classical.choice`.

The refinement gate checks the compiled decision program itself:

```sh
python3 tools/check_refinement.py --out "$CHECK_DIRECTORY" --lean-root "$LEAN_ROOT"
```

With the pinned Verus it proves, for every input in the declared domains,
that the successor each accepting branch assigns equals the Spot core
specification's, rounding included, and meets the integer Step contract that
`proofs/NativeBridge.lean` states. `NativeBridge.lean` re-proves the closed
round-trip and creation results on such integer traces without
`Classical.choice`. The gate also compares the program with
`tools/spot_reference.py` on a declared finite domain, and five planted
defects must fail both checks. The proof runs on the library's evaluator
specification; that the executable evaluator, decision construction and
publication meet that specification rests on the library's own Verus gates.
