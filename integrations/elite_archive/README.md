# Experimental MAP-Elites archive for ZenoFCIS

**Status:** independent prototype, *not* part of the ZenoFCIS formal core, no publication authority. Python 3.10+, standard library only. Designed against the ZenoFCIS CLI `transform check`, `transform replay`, `optimize`, and `loop encode` interfaces on 10 October 2026. Does not monkey-patch the Rust checker.

## Purpose

Search for more than one correct implementation of a **fixed, pinned** finite decision program. The existing optimizer keeps a single lexicographically best smaller program. This external layer accepts *any* candidate that passes the existing exhaustive equivalence checker, including candidates larger than the original, and preserves the best-known candidate in each quality-diversity cell.

The archive uses two **measured descriptor axes**: admitted instruction count and canonical byte length. Both are taken from the native transform receipt. They are related but not identical because the finite IR uses different-sized opcodes. Within each bin, prefer lower maximum Step usage, then fewer bytes, then fewer nodes, then lower SHA-256. Candidate generation may come from an LLM, e-graph strategy, manual construction or another optimizer. This is MAP-Elites **archiving**, not yet a population-based evolutionary trainer or RLVR implementation.

For any archived candidate `p`, the mandatory boundary is:

```
program_import(p) accepted
AND ABI(p) = ABI(original)
AND for every input x in the complete declared finite domain:
    observation(p,x) = observation(original,x)
AND native transform receipt is generated and replayed
AND checker executable SHA-256 equals the pinned CLI SHA-256
```

`observation` includes eager evaluation results and failures; use and step budgets are separately recorded. A verified equivalence **does not** demonstrate that the original implements the right product requirements. A supplied `.zeno` specification is snapshotted and fingerprinted, not semantically checked by this prototype. The archive never calls the Authority publication path, never upgrades a store, and never authorizes effects.

## Use with an actual ZenoFCIS CLI

From the ZenoFCIS repository root, with a native `zeno-fcis` CLI already built from the matching source and with the full input program in canonical `.zcve` format:

```sh
export ZENO_FCIS_CLI="$PWD/target/debug/zeno-fcis"
python3 integrations/elite_archive/elite_archive.py init \
  --archive /tmp/zeno-elites --original /path/to/program.zcve \
  --spec /path/to/human-reviewed-project.zeno
python3 integrations/elite_archive/elite_archive.py offer \
  --archive /tmp/zeno-elites --candidate /path/to/proposal.zcve --source LLM
python3 integrations/elite_archive/elite_archive.py sweep \
  --archive /tmp/zeno-elites --strategy default
python3 integrations/elite_archive/elite_archive.py show --archive /tmp/zeno-elites
python3 integrations/elite_archive/elite_archive.py verify --archive /tmp/zeno-elites
```

`--spec` is optional but recommended. Its identity is **operator-supplied and not authenticated**. It does not change the checker target: the fixed original program does. `--strategy` can be repeated with paths to valid `zeno-fcis/optimize-strategy/1` JSON strategies. A sweep invokes existing e-graph optimization and *independently rechecks* any proposed improved output; non-improvements remain reported.

A human can review one of the SHA-256 IDs printed by `show`:

```sh
python3 integrations/elite_archive/elite_archive.py review \
  --archive /tmp/zeno-elites --sha <candidate-sha256> \
  --decision preferred --rationale /path/to/my-review.txt --reviewer operator
python3 integrations/elite_archive/elite_archive.py export \
  --archive /tmp/zeno-elites --sha <candidate-sha256> --out /tmp/selected-elite
```

`export` requires an explicit `preferred` human review, **replays the exact receipt again**, and writes a new directory containing `candidate.zcve`, `receipt.json`, and `selection.json`. It does **not** call `contract adopt`. To perform an actual ZenoFCIS adoption requires the existing explicit contract-adoption gates and usage decision. Human reviewer names in this prototype are unverified labels.

## Run the tests

The adapter unit suite deliberately uses `tests/fake_zeno.py` to test the reject/pass protocol, tampering controls, diversity archive, and human review. **These do not validate any Rust verifier or executable core.**

```sh
python3 -m unittest discover -s integrations/elite_archive/tests -p 'test_archive.py' -v
```

The **native smoke test** requires a real ZenoFCIS binary. It uses the real `loop encode` command to create three candidate finite Boolean programs: one-node identity, five-node fourfold negation, and a deliberately wrong onefold negation, compared to a three-node double-negation original. It then requires two elites, a checker-rejected counterexample, successful replay, and an operator-preferred export.

```sh
python3 integrations/elite_archive/native_smoke.py --cli "$ZENO_FCIS_CLI" \
  --work-dir /tmp/elite-native-proof-demo
cat /tmp/elite-native-proof-demo/smoke-result.json
```

A missing native binary returns `status: not-run`, exit **77** — never a pass. Run the smoke and suite against the binary you intend to use, and inspect their receipts. Equivalence is relative to the pinned original and its declared finite domain.

The **native suite** (`--suite`) broadens the same real-CLI coverage to four fixed finite families, each in a fresh pinned archive: (a) Boolean `Not(Not(x))` vs identity `x`, with `Not(x)` as the counterexample; (b) `Add(x, Int(0))` vs identity on `Int[-2,2]` in/out, with `Add(x, Int(1))` — which also exercises the declared output domain — rejected as a counterexample; (c) ordered repeated roots `[x,y,x]` over two Boolean inputs preserved by double negations, with `[y,x,x]` rejected; (d) an unreachable `Add(i64::MAX, Int(1))` node whose eager checked trap is part of the program's observation: an equivalent always-trapping graph with one extra unused literal is retained, while the same graph without the trapping node is rejected. Every case requires the equivalent offer to become an elite, the wrong offer to be rejected as a `counterexample`, full-domain replay, a demo-operator `preferred` review, export, and a fresh `transform replay` of the exported receipt. Per-case summaries and receipt digests are written to `native-suite-result.json` in `--work-dir`.

```sh
python3 integrations/elite_archive/native_smoke.py --cli "$ZENO_FCIS_CLI" \
  --work-dir /tmp/elite-native-suite --suite
cat /tmp/elite-native-suite/native-suite-result.json
```

The suite's scope is identical to the smoke's: bounded equivalence to each pinned original over its complete declared finite domain, produced only by the native checker and replay. It does not prove global optimality, intended behavior, or the archive's own Python, and it grants no adoption or publication authority.

## Safety and limits

- One writer per archive, enforced by a directory lock. After an abnormal process termination a stale `.writer-lock` must be inspected and removed manually; no automatic lock stealing.
- Never overwrite original, candidate blobs or receipts. Atomic manifest replacement; a crash may leave harmless unreferenced files requiring manual cleanup.
- Maximum 256 recorded offers, 64 KiB canonical programs, 1,000,000 finite input tuples per native check, and a bounded tool timeout. Cost is dominated by the native exhaustive checker.
- Pins exact original program bytes, optional specification snapshot, and actual CLI executable SHA-256. A changed executable requires starting a fresh archive. This guards against accidental version drift, not an adversarial local user who can replace the manifest and checker simultaneously.
- The `show` command labels results *stored, not freshly replayed*. The `verify` command recomputes all current elites. Even full successful replay gives no guarantee about a larger composed application, external authenticated inputs, persistence or effects.
- Does not claim global optimality, convergence, training gains, verified soundness of its own Python code, or coverage outside the enumerated finite domain. It is not integrated with the live ZenoFCIS CLI release yet.

## Next engineering step

After the native smoke passes, integrate this as a supported optional CLI subcommand backed by Rust types, and add native tests to the repository gate. Then compare archive coverage and verified Pareto improvements against the existing single-incumbent optimizer on a held-out family-balanced corpus. Explore structural descriptors (graph depth, opcode mix, proof effort) only when they can be measured from the same admitted, version-bound program rather than inferred from LLM explanations. Revisit RLVR after collecting verified candidate/feedback pairs.
