# Finite Boolean e-graph experiment

This experiment asks whether complete finite Boolean semantics help an e-graph
produce smaller shared ZenoFCIS programs than a strong local simplifier and the
same e-graph using rewrite rules alone. It is a standalone experiment directory
inside a pinned ZenoFCIS source checkout. It changes no production crate.

The [paper draft](paper.tex), authored by Dana Edwards, reports the successful
amended search and both retained failed configurations. The [independent
results](results/independent-bounded-analysis.md) report 816 semantic-proposal
instructions against 895 local-baseline instructions, with 18 wins, 78 ties,
and four regressions. The original registered configuration did not pass.

## Sources and tools

| Component | Exact source/version |
| --- | --- |
| ZenoFCIS | [`TheDarkLightX/ZenoFCIS`](https://github.com/TheDarkLightX/ZenoFCIS/tree/cb1366cddc5c813f9977b377f541e64bcf94035c), commit `cb1366cddc5c813f9977b377f541e64bcf94035c` |
| egg | [`TheDarkLightX/egg`](https://github.com/TheDarkLightX/egg/tree/12997cd15a123156abd726155363701f13358040), commit `12997cd15a123156abd726155363701f13358040` |
| Rust | `1.97.1` |
| Native CBC library | `2.10.13` |
| Corpus/analysis interpreter | Python `3.9.6`, standard library only |
| Optional mathematical replay | Lean `4.30.0-rc2`, Mathlib `9977002c3c9492b622fb469b0d18acc7e73aed3e` |

The Cargo manifest uses the vendored egg source and the surrounding
`zeno-fcis-synthesis` crate. Retain the experiment's `Cargo.lock` when reproducing
the run. The optional proof instructions and scope are in [proof/README.md](proof/README.md).

The source, results, and paper draft are available on the
[`research/semantic-egraphs` branch](https://github.com/TheDarkLightX/ZenoFCIS/tree/research/semantic-egraphs/experiments/semantic-egraphs).
To obtain the complete source layout in a new directory:

```sh
git clone --branch research/semantic-egraphs https://github.com/TheDarkLightX/ZenoFCIS.git ZenoFCIS
cd ZenoFCIS
```

If the experiment directory does not already include `vendor/egg`, fetch that
source at its exact revision:

```sh
mkdir -p experiments/semantic-egraphs/vendor
git clone https://github.com/TheDarkLightX/egg.git experiments/semantic-egraphs/vendor/egg
git -C experiments/semantic-egraphs/vendor/egg checkout --detach 12997cd15a123156abd726155363701f13358040
```

This branch adds the experiment to the pinned ZenoFCIS revision listed above.
The commands fetch the experiment and its two project sources. Rust and a
linkable CBC installation must be available before compiling. Cargo may fetch
the dependency versions recorded in the lock file.

## Reproduce

Run from the surrounding ZenoFCIS repository root. Verify the corpus identity
before using it; the generator uses fixed seed `20261003`, with no selection
based on optimizer outcomes:

```sh
python3 experiments/semantic-egraphs/corpus.py
python3 experiments/semantic-egraphs/analyze.py --self-test
cargo +1.97.1 build --release --locked \
  --manifest-path experiments/semantic-egraphs/Cargo.toml
cargo +1.97.1 run --release --locked \
  --manifest-path experiments/semantic-egraphs/Cargo.toml -- \
  experiments/semantic-egraphs/corpus.json \
  experiments/semantic-egraphs/results/reproduced-bounded-rust.json \
  --iterations 3 --node-limit 1000 --lp-seconds 30
python3 experiments/semantic-egraphs/analyze.py \
  experiments/semantic-egraphs/results/reproduced-bounded-rust.json \
  --out experiments/semantic-egraphs/results/reproduced-bounded-analysis.json \
  --markdown experiments/semantic-egraphs/results/reproduced-bounded-analysis.md
```

The corpus SHA-256 is
`f0ad16732a453ef6c176e48aa46e544ad07015f4793bf32c84459a271ee9bc18`.
The analyzer rejects another corpus identity. The full protocol and registration
are retained in [PROTOCOL.md](PROTOCOL.md) and `registration.json`. Named
calibration cases are stored separately and do not change the success criterion
or the frozen corpus. Running again may change host timings; compare candidate
outputs and recorded checks separately from those timings.

The command above reproduces the final exploratory search configuration, which
was revised after observing failures in two earlier full runs. The corpus,
ordinary rules, baseline, and fixed node-count threshold remained unchanged.
The final run and repeat pass every candidate/control/tool check and meet the
fixed cost threshold. This does not turn either earlier failed run into a
successful registered experiment. See [SEARCH_AMENDMENT.md](SEARCH_AMENDMENT.md),
[independent bounded analysis](results/independent-bounded-analysis.md), and
[repeat analysis](results/independent-bounded-repeat-analysis.md).

The source-freeze and amendment receipts retain historical source hashes.
The optimizer, evaluator, lowering, generator, and baseline stayed fixed;
the analyzer's reporting and validation diagnostics were revised. Source
continuity in the search amendment refers to the optimization experiment,
not every subsequent analysis edit. Current bundle hashes are recorded in
`artifact-manifest.json`.

The retained earlier studies used the following configurations:

| Study | Iterations | Node threshold | LP budget | Recorded outcome |
| --- | ---: | ---: | ---: | --- |
| Initial registered search | 6 | 5000 | 2 seconds | 12 failed extractions: 2 rewrite-only, 10 semantic |
| First resource amendment | 6 | 5000 | 30 seconds | 1 failed semantic extraction |
| Final exploratory search | 3 | 1000 | 30 seconds | All 400 variants pass in both full runs |

The node threshold is checked between rewrite batches and can be exceeded
inside a batch. The LP budget limits solving, not the whole extraction or run.
The initial [raw result](results/main-rust.json) and
[independent failure audit](results/initial-failure-audit.json) are preserved.
The thirty-second amendment is documented in
[RESOURCE_AMENDMENT.md](RESOURCE_AMENDMENT.md), with its
[raw result](results/main-30s-rust.json) and
[independent failure audit](results/amended-30s-failure-audit.json).
The strict success analyzer rejects both failed results instead of assigning
missing candidates a valid node cost.

To reproduce the earlier configurations without overwriting the retained
evidence, use the same locked build:

```sh
cargo +1.97.1 run --release --locked \
  --manifest-path experiments/semantic-egraphs/Cargo.toml -- \
  experiments/semantic-egraphs/corpus.json \
  experiments/semantic-egraphs/results/reproduced-initial-rust.json \
  --iterations 6 --node-limit 5000 --lp-seconds 2
cargo +1.97.1 run --release --locked \
  --manifest-path experiments/semantic-egraphs/Cargo.toml -- \
  experiments/semantic-egraphs/corpus.json \
  experiments/semantic-egraphs/results/reproduced-30s-rust.json \
  --iterations 6 --node-limit 5000 --lp-seconds 30
```

When a result fails its tool gate, audit its accepted candidates and retained
incumbents without declaring overall success:

```sh
python3 experiments/semantic-egraphs/audit_failed_run.py \
  experiments/semantic-egraphs/results/reproduced-initial-rust.json \
  --out experiments/semantic-egraphs/results/reproduced-initial-failure-audit.json
```

Elapsed-time limits depend on the host, so reproduced timeout counts may differ.

## What is compared

The 100 cases contain total Boolean expressions over three to six inputs and
ordered, sometimes repeated output roots. The six families include simple
gates, factoring, absorption/complements, conditionals/De Morgan forms, shared
guards, and seeded balanced trees. [Corpus construction and baseline rules](corpus/README.md)
are fixed before the benchmark.

Each variant shares structurally identical expressions across its output roots
during lowering. The variants are:

- Original expressions.
- A deterministic local simplifier, including greedy common factoring.
- Rewrite-only egg with bounded search and DAG extraction.
- The same egg search with exact complete-truth-signature merges.

Final cost counts distinct instructions in the emitted FCIS DAG, using
`Input`, `Bool`, `And`, `Not`, and `Select`. OR lowers to
`Not(And(Not(a),Not(b)))`. The extractor's additive OR cost is a surrogate;
shared generated NOT nodes can make it differ from actual emitted cost. A CBC
optimal result establishes optimality for its built model, not a global optimum
for final FCIS nodes over all Boolean implementations.

The recorded success criterion requires all verification/control/tool checks to
pass, fewer aggregate semantic-egg nodes than the local baseline, and strict
wins in at least 10 cases across at least two families. Raw proposals and
regressions remain visible. Comparing semantic egg with rewrite-only egg isolates
the effect of signature merging under the shared search configuration.
The local baseline proposes generalized flattened identities greedily against
actual shared FCIS cost. Both e-graph variants include the same 33 ordinary
rules, including factoring and De Morgan identities, but expose their binary
forms through bounded search and extract with a surrogate cost. This is a
comparison of search policies rather than identical search or global optimality.

## Checks and limits

The Python corpus evaluator, Rust expression evaluator, actual ZenoFCIS
`Program.evaluate`, and Python serialized-DAG interpreter independently check
every admitted valuation and every output position. Negative controls cover a
changed truth function, a non-Boolean region, eager overflow in unused nodes and
unselected conditional arms, and invalid input shape/domain. Errors and unknown
solver results do not count as successful candidates.

This covers a fixed synthetic total-Boolean fragment. It does not establish
arithmetic rewrite safety, preservation of authoritative state or events,
resource-accounting equivalence, shell behavior, Tau language equivalence,
weak-omega temporal equivalence, novelty, or superiority to mature Boolean
synthesis tools. The Lean file checks a mathematical signature/acceptance
model; it does not verify the implementation.

Node counts and timing observations answer different questions. The analyzer
reports e-graph construction/search, extraction, verification, and complete
variant intervals separately. These measurements do not benchmark application
execution. `baseline-timings.json`
records Python baseline optimization independently; Rust lowering of its
precomputed outputs omits that work. Different implementation languages and
host-specific measurements do not establish a fair algorithmic speed ratio.
Fewer DAG instructions alone do not establish faster generated applications.

If `baseline-timings.json` is available, add
`--baseline-timings experiments/semantic-egraphs/baseline-timings.json` to the
analysis command. The analyzer checks its case identities, rewrite-step counts,
sample counts, and reported medians before retaining that separate measurement.
