# Published 100-case Boolean corpus (calibration only)

This directory holds the 100 original programs of the semantic e-graph study as
canonical `zeno-fcis/finite-i64/1` programs, so `zeno-fcis optimize` and
`zeno-fcis transform check` can run on them. It is calibration data, as the
[benchmark README](../README.md) already says of the old study: every formula is
public, the cases were not chosen for this optimizer, and no result on them is a
held-out or optimality claim.

Only originals are stored. The study's outputs (its local baseline, rewrite-only
and semantic e-graph programs) and any optimizer candidate are deliberately left
out, so nothing here can serve as a reference answer.

## Source

| Item | Value |
| --- | --- |
| Repository | [`TheDarkLightX/ZenoFCIS`](https://github.com/TheDarkLightX/ZenoFCIS), branch `research/semantic-egraphs` |
| Commit | `a69ed8db594d95279a46bff0f65185ef67d51f98` |
| File | `experiments/semantic-egraphs/corpus.json` |
| SHA-256 | `f0ad16732a453ef6c176e48aa46e544ad07015f4793bf32c84459a271ee9bc18` |
| Generator seed | `20261003` (the study's `corpus.py`; no result-dependent selection) |

[`sources.json`](sources.json) keeps, for each case, only the fields `id`,
`family`, `nvars`, `outputs` (the original formulas) and `truth_signatures`.
`python3 generate.py --extract PATH/corpus.json` rebuilds it after checking the
SHA-256 above.

## Generation

```sh
python3 generate.py --check   # regenerate in memory; compare with originals/ and manifest.json
python3 generate.py --write   # rewrite originals/ and manifest.json
```

The formulas use `v0`..`v5`, `true`, `false`, `and`, `or`, `not` and `ite` over
three to six Boolean inputs. They are lowered as the study's harness lowered
them: `or(a, b)` becomes `Not(And(Not(a), Not(b)))`, `ite` becomes `Select`, and
instructions are hash-consed in first-visit order over all outputs. `--check`
also evaluates every lowered program on all of its input tuples and compares the
outputs with the study's truth signatures (bit `i` of the mask is `vi`).

The 100 regenerated programs (1,506 instructions in total) are byte-identical to
the `raw` programs stored in the study's `results/bounded-rust.json` (SHA-256
`7d83d18380c1768b582689d7ddb34664482d1e7223ea1bbbac99187a3a49aff2`), each encoded
canonically; this was checked when the directory was created.

## Reading results

Every domain has at most 64 tuples, so the optimizer's tables are exact on the
whole domain here. The study's lowering makes `or` three instructions while
`Select(a, a, b)` is one, so comparisons with the study's own outputs mostly
measure representation, not search. [`../measure_optimizer.py`](../measure_optimizer.py)
runs the optimizer on these originals together with the withdrawal-queue
artifacts and the seeds of `cases.json`, and replays every reported receipt.
