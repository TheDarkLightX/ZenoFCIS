# ZenoFCIS Boolean/integer benchmark seeds

A practical starting benchmark for diagnosing Boolean simplification, shared DAG
extraction and checked-integer mistakes. Matching Herbie's diagnostic usefulness
is an aspiration to test, not a result of this packet.

**Available:** 32 public development program pairs, explicit domains/ordered ABI,
11 concrete difference witnesses, and lightweight reference checks.
**Locally checked:** 16 Boolean and 16 future checked-i64 pairs, 355 input tuples,
21 equivalent pairs and 11 different pairs. Seventeen feasible targets use fewer
stored nodes; four pairs are no-improvement controls. Production byte costs unmeasured.
**Not implemented/run:** production transform checker, native/proof qualification,
optimizer study, neural experiment, held-out evaluation, application promotion.
These are handwritten synthetic/circuit/policy-inspired seeds, not app extractions.

A separate [withdrawal-queue native experiment](withdrawal-queue/README.md) now
extracts a real application kernel and compares the retained controller and
current decision scalar graph with the Rust evaluator. It includes complete
finite scalar coverage, actual canonical sizes and meter counterexamples.
Its offline results grant no application authority; the 32 seed pairs above
remain unchanged and have no native qualification from that experiment.

Read [DESIGN](DESIGN.md) for scope/schema/families, [PROTOCOL](PROTOCOL.md) for
acceptance and future evaluation, and [CLAUDE_HANDOFF](CLAUDE_HANDOFF.md) for the
next bounded implementation slice. [cases.json](cases.json) contains every program
as typed stored nodes and ordered roots; it is not a production artifact codec.

| Case | Concrete diagnostic |
| --- | --- |
| B03 | Factor a common Boolean guard after OR lowering |
| B05 | Share work across three outputs while retaining a duplicate root |
| B06/B07 | Parity/majority circuits with structurally different equivalent targets |
| B09/B10 | Complete six-input coverage and the one zero-input tuple |
| B11 | At (false,true), wrong De Morgan yields false instead of true |
| B14 | At (false,true), output reordering changes (false,true,false) |
| I03/I08 | `(x+1)-1 -> x` passes -2..2 but fails at MAX with Arithmetic |
| I09 | `(MAX+1)+(-1)` errors; `MAX+(1+(-1))` succeeds |
| I10/I11 | Unused/unselected overflowing Add still causes Arithmetic |
| I12 | Double arithmetic negation fails at MIN |
| I13 | Clamping x=2 changes OutputDomain into successful 1 |
| I14 | Eager overflow yields Arithmetic before output-domain validation |
| B15/B16/I15/I16 | No-improvement controls, including alternate same-node forms |

Run the authored-fixture check from this directory:

```sh
python3 check_cases.py
```

Replay with the separately written companion interpreter:

```sh
python3 independent_check.py cases.json independent-fixture-check.json
```

The script checks exact i64 overflow at each Add/Sub and full declared-domain
coverage; its JSON report is development evidence, not a production receipt.
`generate_cases.py` reproduces authoring data; run `check_cases.py --write-witnesses`
after generation to restore checked witnesses. Review/freeze digests after changes.
Both interpreters confirmed the relations and first witnesses. See the
[validation record](VALIDATION.md) and [Astra Max review](REVIEW.md) for their scope and unperformed checks.

All seeds are public/LLM-visible and can never be claimed hidden. There are 32
pairs but 28 distinct original finite behaviors at fixed input positions. Related/alias
groups remain together in any future split, including input-permuted B02/B04.
The old published 100-case study is calibration only.
Handwritten targets are not global optima; JSON length is not canonical byte cost.
Functional equality does not preserve resource usage, application decisions or authority.

Finish smaller V2 and the existing 2.1 prerequisites first. This packet adds no
V2 completion gate. See [roadmap](../V2_1_FACTORY_PLAN.md),
[transform design](../V2_1_TRANSFORM_DESIGN.md),
[loop design](../neurosymbolic-loop/DESIGN.md), and
[requirements](../neurosymbolic-loop/specs/INDEX.md).
