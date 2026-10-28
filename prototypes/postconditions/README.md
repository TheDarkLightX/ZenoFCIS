# AI-proposed postconditions for a future ZenoFCIS 2.7

This is an opt-in prototype built on the existing 2.6 source. It is not a
2.7 release or part of 2.6 release qualification. It adds no dependencies and
does not change the `.zeno` grammar, policy, laws or adoption mechanism.

The feature separates two questions: does a proposed postcondition hold on
the current contract's committed decisions, and does it detect any supplied
faulty policy? Passing the first question does not establish the second, and
neither establishes that the condition fully captures the author's intent.

## Use

Build the CLI with the repository's pinned Rust toolchain:

```sh
cargo +1.97.1 build --locked --offline -p zeno-fcis-cli
target/debug/zeno-fcis contract postconditions prompt path/to/app \
  --intent requirements.txt > request.json
```

Give `request.json` to an AI agent. It contains the exact interface and written
intent, syntax instructions, an output schema and a request digest. The current
policy is hash-bound but omitted from the generation prompt to reduce the risk
of learning an implementation bug as the specification. The command itself
does not contact a model or execute a generated program.

Save the returned JSON as `proposals.json`, then run:

```sh
target/debug/zeno-fcis contract postconditions check path/to/app \
  --intent requirements.txt --proposals proposals.json \
  --mutant faulty-policy.json > report.json
```

`--mutant` is optional and repeatable. Each mutant is a complete policy JSON
file whose only changed fields may be `cases` and `variables`. The project,
declared domains, roots, laws, genesis and other policy metadata stay fixed.
Duplicate policies are refused. Compilation failures and undefined/refused
outcomes alone never count as detected faults. A kill requires an actual
failing admitted transition, even if other mutant inputs are undefined.

For example, a counter proposal may express:

```text
command.101.120 == 150 -> post.100.110 == pre.100.110 + 1
```

This can detect a faulty increment that leaves the value unchanged. A range
condition may hold on both versions and therefore show no discrimination.
The test fixture deliberately withholds the exact-movement law while keeping
the range and authorization laws, so the example measures a real specification
gap. The feature never removes a law from the supplied application.

The report's counterexamples can be supplied to the agent for another proposal.
Do not automatically weaken a failed condition: the implementation, prose or
candidate may be wrong. Review is still required before any eventual adoption.
No automatic repair or adoption loop is implemented in this prototype.

## Results and limits

| Candidate status | Meaning |
| --- | --- |
| `refused` | Malformed, unsupported, duplicate or out-of-profile formula |
| `refuted` | A committed reference decision contradicts the proposal; witness included |
| `inconclusive` | Incomplete enumeration, reference refusal, undefined evaluation or empty committed scope |
| `checked-trivial` | Holds in scope but syntactic analysis finds no transition constraint |
| `checked-no-discrimination` | Holds in scope; no supplied mutant has a witnessed violation |
| `checked-discriminating` | Holds in scope and detects at least one supplied mutant |

`check` exits 0 only when every candidate is checked-discriminating, 1 if any
candidate is refused/refuted or the request is invalid, and 2 otherwise. Read
the per-candidate details. A detected fault in one supplied policy does not
establish general bug coverage. Syntactic substance analysis is conservative,
not a general tautology prover. AST-identical formulas are treated as duplicates;
logical equivalence between different ASTs is not decided.

Checks enumerate every state/input tuple in the declared finite domain, with
a maximum of 4096 tuples, 16 candidates and 8 mutants. Postconditions cover
admitted committed transitions, including committed failures; rejection
behavior and liveness are outside this scope. Reference refusals prevent a
checked status. A smaller `--max-tuples` must match the original prompt.

The initial profile supports signed scalar contracts without collection maps,
unsigned policy mode, adoptions or evolutions. Formulas are limited to 1024
bytes and 128 scalar/Boolean nodes: integer constants, declared pre/post/input
projections, comparisons, Boolean connectives and checked `+`, `-`, `*`.
Quantifiers, functions, division, comments and executable code are refused.
Arithmetic overflow remains undefined, never a successful check.

Input files must be regular UTF-8 files of at most 65536 bytes; intent is at
most 8192 bytes. JSON rejects duplicate/unknown proposal keys. JSON numeric
tokens are limited to i64/u64; larger i128 expression literals can be strings.
Policy strings are capped at 1024 bytes, variables/cases at 64 each, and the
iterative expansion preflight at 4096 visits and depth 64. These admission
rules apply only to this prototype and do not alter the existing compiler.

Proposals bind the exact project, policy, intent, limit and scoped compiled
checker source identity. A changed input requires a fresh request. The
fingerprint is provenance, not a binary attestation. Reports confer no
production, proof, release or adoption authority.

## Research basis

- [nl2postcond, FSE 2024](https://arxiv.org/html/2310.01831v2) found generated
  assertions could detect 64 historical Defects4J bugs. Its test-based
  correctness and mutant-discrimination metrics motivate separate outcomes.
- [Beyond Postconditions](https://arxiv.org/html/2510.12702v1) motivates explicit
  input assumptions and warns against specifications fitted to buggy code.
  This prototype freezes the existing domain instead of inferring preconditions.
- [VeriStruct v4](https://arxiv.org/html/2510.25015v4) supports structured
  annotation generation with verification feedback. Its data-structure results
  do not establish correctness or productivity gains for this prototype.

The current experiment tests workflow mechanics and one deliberate counter
fault. It does not measure a model's generation accuracy, developer time saved,
performance gains, or the correctness of arbitrary natural-language intent.

## Replay

```sh
cargo +1.97.1 test --locked --offline -p zeno-fcis-cli --bin zeno-fcis postconditions
cargo +1.97.1 test --locked --offline -p zeno-fcis-cli --test postconditions_cli
```

The tests cover a witnessed fault, weak and false proposals, deterministic
replay, stale bindings, duplicate/unknown keys, narrowed-domain attempts,
unsupported formulas, empty/undefined/incomplete scope, harmless and invalid
mutants, expression-expansion attacks, evaluator-route comments, read-only
contract handling, and nonblocking refusal of FIFO input.
