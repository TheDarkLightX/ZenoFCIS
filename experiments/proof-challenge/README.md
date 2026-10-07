# Frozen proof challenge experiment

This experiment tests an assurance lesson from
[OpenAI's mathematics repository](https://github.com/openai/math): an agent's
proof must establish the requested statement with the requested definitions and
allowed assumptions. A successful Lean build alone does not check all three.
This is an experimental developer tool, separate from application execution and
the release gates. It changes no Rust core, authority, schema or evaluator identity.

## Contract and proof

The frozen Boolean model is:

```text
permission(authorized) = authorized
approve(pending, authorized) = pending AND permission(authorized)
approve(pending, authorized) = true implies authorized = true
```

The theorem quantifies over both Boolean inputs, including all four input pairs.
Its proof splits these cases and eliminates impossible equalities. The challenge
already has a complete proof; it has no placeholder. The configuration requires
exactly that named theorem, permits zero axioms and provides no definition holes.
`authorized` is a supplied fact, not evidence that a caller was authenticated.
This small model is not an extraction of ZAL or of the Rust functional core.

Both valid submissions keep the definitions and theorem statement but use
different proof terms. The negative controls illustrate distinct mistakes:

| Submission | Expected ordinary build | Expected Comparator verdict |
| --- | --- | --- |
| `valid` | Compiles | Accept |
| `alternate-proof` | Compiles | Accept |
| `weakened-statement` | Compiles | Refuse the changed theorem statement |
| `changed-definition` | Compiles | Refuse the changed `approve` definition |
| `changed-helper` | Compiles | Refuse the changed transitive `permission` definition |
| `forbidden-axiom` | Compiles | Refuse `ApprovalContract.assumed_authority` |
| `unfinished-proof` | Compiles with a warning | Refuse `sorryAx` |
| `invalid-proof` | Does not compile | Refuse the invalid proof |

The changed `approve` drops the pending-state requirement while retaining the
authorization property. This shows why proving one property of changed code is
insufficient for a challenge that freezes that code's meaning. The changed
helper refuses all approvals, making the authorization theorem vacuously true.
Its definition must still match the frozen specification.
An optimization challenge would instead state a separate equivalence theorem
with fixed observation and evaluator semantics.

## Observed result

The local 2026-10-07 run passed all eight cases: both valid proofs were accepted,
and all six negative controls were refused for their stated reasons. Five of
those negative controls still compiled under ordinary Lean checking. The run
took 37.3 seconds with a one-CPU limit and a 3 GiB memory cap. GNU time reported
704,940 KiB as the largest process RSS (about 688 MiB); this is not an aggregate
memory measurement. No RAM-backed build directory was used.

The receipt and public native/judge logs are saved under
[`results/2026-10-07/`](results/2026-10-07/). The receipt binds the raw tool-output
hashes; the public copies replace only the operator's Lean runtime path with
`<PINNED_LEAN_ROOT>`. `public-logs.json` records the raw and public hashes and
replacement counts. Raw logs remain in the local evidence archive. The public
hashes, fixture hashes and runner hash can be checked from this checkout;
checking the receipt's raw log hashes requires the original logs. Earlier setup
attempts were failed experiments, not passing evidence. A separate fourteen-test Python run checked fixture
admission, diagnostic classification and descendant cleanup.

## Checker and evidence

The checker is the official
[Lean Comparator](https://github.com/leanprover/comparator), pinned to
`1cfc5d8ad183bf65efe7accd0efc175b6b8f25b6`, compatible with the project's
Lean 4.30.0. `pins.json` also records its two locked dependencies. This avoids
installing a newer Lean runtime or the large OpenAI mathematics library.

The runner verifies the reviewed fixture manifest before executing anything and
copies those exact bytes into fresh projects. An ordinary-build baseline and a
Comparator invocation use separate projects, so the judge never inherits the
baseline's compiled artifacts. Comparator compares the theorem and its
definition dependencies, audits the axioms used by the proof and replays the
export into the Lean kernel. No additional independent kernel is enabled.

Each negative case must fail with its specific diagnostic. Missing tools,
timeouts, module-path failures and unrelated failures cannot pass a control.
Timeouts kill the whole subprocess group. The receipt binds the fixture bytes,
runner, source revisions, executable digests, exits and complete log digests.
The runner writes raw logs; the portable copies are a separate publication step
and preserve every diagnostic apart from the exact runtime-path replacement.
Sources and tools are checked again at the end. Evidence directories cannot be
overwritten. Temporary project artifacts are removed after each experiment;
logs and the receipt remain on disk.

## Reproduce

Use an existing Lean 4.30.0 release directory. Build the small pinned checker in
a separate disk-backed directory; this does not require Mathlib:

```bash
PILOT_LEAN_ROOT=/absolute/path/to/lean-4.30.0-linux
PILOT_TOOLS=/absolute/path/to/new-comparator-checkout
git clone https://github.com/leanprover/comparator.git "$PILOT_TOOLS"
git -C "$PILOT_TOOLS" checkout --detach 1cfc5d8ad183bf65efe7accd0efc175b6b8f25b6
cd "$PILOT_TOOLS"
PATH="$PILOT_LEAN_ROOT/bin:$PATH" LEAN_NUM_THREADS=1 lake build lean4export comparator
```

Lake uses the checked-in dependency manifest. Do not update it. From the
ZenoFCIS checkout, run:

```bash
python3 tools/test_proof_challenge.py
python3 tools/check_proof_challenge.py \
  --comparator-root "$PILOT_TOOLS" \
  --lean-root "$PILOT_LEAN_ROOT" \
  --output /absolute/path/to/new-evidence-directory
```

The final command must report eight passing cases and write an
`experiment_pass` receipt. The unit tests exercise manifest admission, refusal
classification and cleanup, not theorem proving. Missing tools are failures;
there is no fallback to simulated proof checking. The existing acceptance suite
does not implicitly download or run this optional experiment.

## Assurance boundary and adoption

This experiment uses Comparator's development shim on fixed, reviewed fixtures.
**The shim provides no sandbox. Do not run arbitrary agent submissions through
it.** There is deliberately no candidate-file argument. Before accepting actual
external submissions, use a qualified containment mechanism, protect the
reviewed challenge and checker, and establish the relevant compiler/runtime
assumptions. Source hashes recorded by the same user are not authenticated
approval or tamper protection against that user.

Passing proves this named Boolean property in Lean under the named toolchain
and checker assumptions. It does not prove the human requirements, the Python
runner, source-to-binary compilation, application authentication, Rust execution
or the whole factory. Binary hashes identify what ran; they are not a verified
compiler or a complete runtime inventory.

For future factory integration, freeze the reviewed specification separately
from the proposed implementation and prove their declared relation. Bind that
proof to the actual supported program, domain and observations. Keep the
existing evaluator and application authority as the execution boundary. A
proof challenge may then supply evidence; it must not issue application
authority merely because a proof file compiled.
