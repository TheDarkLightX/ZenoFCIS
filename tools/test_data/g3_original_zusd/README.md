# Original zUSD characterization fixture

This directory keeps the unchanged single-vault Python source at revision
`e3cf1aad40e487893230ae0c55f1f0dda62f9955`, its data-only graph compiler,
and the independent native characterization generator. The source digest is
checked before any original model statements execute.

The corpus crosses 32 original states, 14 action codes, 9 amounts and both
authorization bits: 8,064 cases. It includes accepting witnesses for all twelve
commands, wide products, fees, decay, liquidation and deposits of `2^63`.
The 46 declared refusal reasons keep their original identifiers. The corpus
exercises 43 of them, every reason the pinned Python can return; the generator
refuses to emit a corpus that misses one. It does not establish exhaustive
equivalence.

Three registered reasons are unreachable through the pinned single-vault
Python, because none of its error strings maps to them; `UNREACHABLE_REASONS`
in `compile_zusd.py` records why, and `SOURCE.json` repeats it:

- 210 `commit_stale_oracle`: `oracle_commit` never checks staleness;
- 239 `liquidate_pending_mismatch`: `liquidate` reads only the pending price;
- 240 `liquidate_stale_oracle`: `liquidate` never checks staleness.

Every pre-state but two satisfies the declared state laws 500 to 504.
`invalid-supply-post-tail` and `free-debt-above-debt` break supply
conservation (law 501); the second reaches reason 217, which needs free debt
above the vault's debt.
Some lawful pre-states are still never produced from genesis by the
single-vault commands:

- `debt-above-supply-cap` holds debt above its vault cap, which only a capped
  mint raises; it reaches reason 223, which from a lawful state needs exactly
  that;
- reasons 226, 233 and 237 need a vault below MCR at the active price.

The internal decision selector uses contiguous branch codes `1..58`:
`1..12` accept the corresponding commands, and `13..58` reject with the
original reason identifiers `200..245`. Three signed numeric nodes convert
the original selector into this admission-complete range. The native cases
and conformance law retain the original command and reason identifiers.
All 36 invocation inputs are bound: 32 state fields, three command fields and
the supplied Boolean context. The context is admitted and read, while the
original decision graph remains independent of its value.

To reproduce the fixture in an empty disk directory, run:

```sh
python3 tools/test_data/g3_original_zusd/emit_rust.py --out /path/to/fixture
```

The generator formats `data.rs` with `rustfmt +1.97.1 --edition 2024` and
records both the generated and the formatted digests. To check that the
checked-in fixture is exactly the generator's output, run:

```sh
python3 tools/test_data/g3_original_zusd/check_fixture.py
```

It recomputes every file digest in `SOURCE.json`, regenerates the fixture in
an empty temporary directory, and requires byte-identical files.

The Rust integration tests execute the generated decisions and declared laws
through the current library's catalog, Authority, sealed publication, replay
and SQLite store. The Python compiler supplies no callback or publication
authority. Its output is checked application data.

The input boundary is the original typed state and nonnegative U128 command
arguments. Python integers outside that ABI, the separate multivault model,
caller authentication, deployment and end-to-end financial safety are outside
this fixture's claim. The authorization bit remains a supplied assumption.

`SOURCE.json` beside the Rust fixture binds the original source, corpus and
generated data. Generated-source checks and the actual current Rust tests must
pass after any change; this characterization is not a formal proof that the
Python and Rust implementations agree on every possible input.
