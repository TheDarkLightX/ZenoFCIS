# Frozen Boolean corpus and independent analysis

The corpus was generated before the Rust experiment results, using Python
3.9.6 and `random.Random(20261003)`. The stored JSON is the reproducible input;
the generator is an additional way to recover it. No formulas are retained or
discarded according to optimizer outcomes.

The 100 cases have this fixed allocation, in this order:

| Family | Cases | Construction |
| --- | ---: | --- |
| `small_gate` | 8 | Variables, identity constants, double negation, factoring, equal ITE arms, complements, and duplicate output positions |
| `distributed_factoring` | 18 | Common factors in OR-of-AND and its dual, varied child order and compound factors |
| `absorption_complement` | 18 | Absorption, complements, and factoring around complementary branches |
| `mixed_ite_demorgan` | 18 | ITE constants/equal arms, negated conditions, De Morgan forms, and compound predicates |
| `shared_guards` | 18 | Three to five ordered outputs sharing predicates and guard fragments; some duplicate root positions |
| `balanced_general` | 20 | Seeded balanced expression trees at depths three and four; every fourth case has a second output |

The language has `v0` through `v5`, `true`, `false`, binary `and`/`or`, unary
`not`, and ternary `ite`. A case declares three to six Boolean inputs. A truth
signature is one bit string per ordered output, with masks increasing from zero
through `2**nvars - 1`; bit `i` supplies `vi`. All operands are evaluated eagerly.
The fragment is total: it includes no arithmetic, observation lookup, state,
effects, temporal operators, or error-producing instructions.

`corpus.py` independently enumerates every admitted tuple and confirms its local
baseline preserves all signatures. Node cost is **actual shared FCIS lowering**:
`Input`, `Bool`, `And`, `Not`, and `Select`, hash-consed over all output roots.
`or(a,b)` lowers to `Not(And(Not(a),Not(b)))`. Cost is not source-tree size or an
additive per-operator approximation.

The baseline considers constants, idempotence, complements, absorption, double
negation, De Morgan, sorted flattened reassociation, common factoring and its
dual, constant/equal ITE arms, negated-condition swaps, Boolean-arm ITE
simplifications, ITE expansion, and negated ITE arms. A replacement is applied
to every occurrence of its structural node across all roots. Each step chooses
the best strict decrease in `(shared FCIS nodes, total spelling length, ordered
spellings)`. Thus node count never increases; equal-cost normalization may
enable later improvements. Proposals never consult truth signatures. The hard
1,024-step bound is checked; exhausting it fails generation rather than admitting
an incomplete baseline. This corpus took at most 12 steps per case.

Frozen corpus SHA-256:

```text
f0ad16732a453ef6c176e48aa46e544ad07015f4793bf32c84459a271ee9bc18
```

The baseline totals are 1,506 raw nodes and 895 local-baseline nodes. These are
input characterization, not experiment outcomes.

The success criterion was fixed before the Rust run: every variant and tool
check must pass; semantic egg must use fewer aggregate actual FCIS nodes than
the local baseline, win strictly in at least 10 of the 100 cases, and win in at
least two families. Expected invalid/error negative controls are separate from
the 100 workload cases. Compare semantic egg with rewrite-only egg as well, to
separate finite-semantic merging from ordinary rewriting.

```sh
python3 experiments/semantic-egraphs/corpus.py
python3 experiments/semantic-egraphs/analyze.py --self-test
python3 experiments/semantic-egraphs/analyze.py RESULTS.json \
  --out analysis.json --markdown analysis.md
```

The analyzer separately parses optimizer expressions, enumerates their truth
vectors, interprets serialized FCIS DAGs, checks frozen case identity and exact
costs, and reports family totals, wins/ties/regressions, and delta frequencies.
It does not use bootstrap inference: this is a fixed synthetic collection.
Available Rust timing components are reported separately. The local baseline is
precomputed in Python, so its Rust lowering time alone cannot establish a fair
optimizer-overhead comparison. Fewer nodes are not a runtime-speed guarantee,
and the measurements do not establish whole-application equivalence.
