# Wide values and larger decision programs

This document describes wide values in the current 2.6.0 release source. The
historical
[2.3.0 release notes](RELEASE_NOTES.md#historical-230-source-and-qualification)
state that source line's scope and limits; the earlier public candidates have
their own
[candidate record](RELEASE_CANDIDATES.md).

The mandatory library route admits a contract, evaluates original input
bytes, checks its declared laws, and returns a private publication capability
only when those checks succeed. The SQLite shell consumes that capability.
Application callbacks cannot replace the decision or law evaluator on this route.

| Profile | Supported source route | Boundary |
|---|---|---|
| Larger scalar decisions | CLI contract generation and ordinary generated apps | At most 4,096 nodes and 64 outputs; scalar expression arithmetic uses checked signed 128-bit arithmetic. |
| Scalar arithmetic extension (qualification pending) | Generated rules, generated apps and the same library evaluator | Checked multiplication, signed floor/ceiling division, fused mul-div including signed floor, and exact two/three-factor products. The main route uses finite-i128/3 and signed i128 values; declared profiles and final-fit refusals remain explicit. |
| Wide numeric decisions | Library descriptors and the checked numeric evaluator | Signed and unsigned 128-bit values, with width-explicit checked arithmetic and resource limits. |
| Original compound values | Library schema admission and immutable decoded witnesses | Whole declared compound values can be carried through decisions; nested field selection is unsupported. |

The numeric family has no general CLI specification or lineage-upgrade workflow.
Its library fixtures exercise the actual publication, replay and store route.
They do not supply a business decision callback. The legacy `transform`,
`optimize` and `loop` checker path retains its 256-node/16-output bound;
larger programs must not acquire an equivalence receipt through that path.
The main arithmetic profile `finite-i128/3` can be imported and checked by complete
finite execution within the original limits. Older program readers refuse
its profile and tags. The linear SMT/Carcara program encoding and legacy
proposal/search profiles return unsupported for its new arithmetic. Existing
U128 library mul-div and ProductsGe retain their separate meanings and domains.
One logical Step is charged before each attempted arithmetic instruction,
including refusals; this is not a measure of limb operations or CPU time.

Upgrade equivalence reports that a comparison is not established when either
program exceeds the checker's complete execution budget.

`tools/check_g3_app.py` generates a 67-case escrow regression that exercises
both enlarged scalar limits, runs independent examples, initializes a store,
previews and commits a decision, and replays history. It also checks that a
4,097-node contract refuses before generated outputs are written. The added
cases have impossible guards, so this tests capacity without inventing new
escrow behavior. It is regression evidence, not exhaustive escrow verification.

The original zUSD fixture characterizes 8,064 inputs against its pinned Python
source. All 43 reachable refusal reasons are represented; three of its 46
registered reasons are unreachable in that source. These finite observations
do not prove equivalence over every possible original input. Authorization is
a supplied assumption, and the fixture establishes no deployed financial safety.
