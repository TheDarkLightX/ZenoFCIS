# V2 conditional law observation

Base: `1ed6f88f7b1098bed6c409bc9aa3c12c99d43c3c`, plus the 129-file
hash-checked development seed. This bounded unit adds a library instruction;
it grants no transition, publication, merge or release authority.

`Op::ObserveWhen(guard_node, observation, default)` uses a strictly earlier
Bool node. Its default is immutable program data of the exact declared Atom
variant. The existing eager graph traversal, pure instructions, observation
selectors, scope table, mandatory families and first-failure semantics remain.

## Mathematical and operational contract

The total node transition takes the actual borrowed original frame, prior
values, law/node IDs, limits, arbitrary initial eight-counter usage and an
existing read prefix. It returns an exact typed result, usage and prefix:

1. Attempt Step first; failure preserves usage and reads and returns Budget.
2. Reject self/forward, missing and non-Bool guards as Undefined after Step.
3. False returns the exact default. It neither charges Read nor observes a
   frame slot, even when that observation is unavailable or Read is exhausted.
4. True follows the same Read block as ordinary Observe: attempt Read, append
   the exact permitted/denied ReadAttempt, then return Budget on denial or read
   the actual frame. Missing/unavailable observations return Undefined.
5. No active observation error becomes a default. Every later eager node
   retains its original semantics; the final predicate must still be Bool.

Metadata precedes frame checks and all protected law work. Shape admission
requires a prior guard and checks every guarded default, even in an unused node
or skipped law. Text defaults must be ASCII, exactly the complete policy's
existing Literal standard. All other Atom variants are exact full-width typed
scalars or opaque bytes, including zero/max type IDs and empty bytes. Existing
lower-level Literal behavior and its separate complete-policy validation are
unchanged. The guarded check lives in owned predicate source; it does not
require a modification to another worker's composition admission.

The minimal failure witness is an accepted inventory reserve with no outbox:
an eager Observe inside an implication/Select still fails. The new opcode
skips only its explicitly guarded observation. Treating every Undefined as a
fallback would also hide missing payloads on actual shipment, so it is refused.
Bool/integer coercion and fallback type erasure are also explicitly excluded.

## Encoding and source boundary

Existing law opcode tags are 0 through 11. New tag 12 encodes, in order:
Word(12), Word(full usize guard), complete existing observation tokens, then
complete existing typed Atom tokens for the default. Law count/order, kind,
scope, genesis, program length and root remain in the complete policy encoding.
Every preexisting opcode keeps its exact tokens.

The sole production changes are laws/types.rs, predicate.rs, spec.rs and
existing tests.rs; authority/metadata.rs, metadata_spec.rs and the generated
library_sources.rs. Tests inside metadata.rs use independent standard-library
big-endian canonical bytes. The generated getter uses the unchanged root
approved 79-path/3-pin list (SHA-256
`168b54005ab3fdad7d9a69a086933666eb1036c25944d8e19934a354e8d5b7a8`).
Source-byte lengths intentionally refresh; root owns the later complete source
list expansion and final identity. No approval hash or shared registration is
changed here.

No evaluator cache, alternate meter, supplied verdict, callback or mutable
program state is introduced. The existing single dispatcher now chooses a
protected observation or returns the exact inactive default; both observing
variants share one Read/trace/frame block. One default-admission helper and one
pure read specification name the added obligations. No acceptance check is
removed. Legacy regressions and all 28 legacy semantic/coverage controls are
retained alongside 15 guarded controls, every specimen byte-identical to its
source.
The 43 stable controls comprise 36 intended proof refusals, five genuinely
verifying raw-coverage refusals and two independent native refusals: the
delivery ordinal and the declared law order (below).
The ordinal control retains the exact legacy runtime mutation and requires its
unchanged named regression to pass on the baseline and fail specifically with
Err(Frame) versus Ok(()) on the mutant. It is a declared control kind, never a
fallback from a solver error. Earlier ordinal resource-limit probes are kept
as inconclusive evidence and are not counted as semantic proof failures.

## Evidence and replay

`verification/verus/guarded_laws.rs` includes the actual complete registered
execution/canonical/scalar modules, including metadata, the producer bridge,
authority constructor and source getter. It proves all reachable executable
helpers with zero executable requires under Verus 0.2026.09.27.3cf1832,
verifier Rust 1.98.1, no-cheating, no-external-by-default and two threads. The
separate reviewed profile retains every raw translated function signature,
contract and Exec/Spec body. No read-ID normalization or coverage waiver exists.

The native portfolio contains a wider-arithmetic counter/attempt oracle over
arbitrary-start samples and overflow; full-width typed default/actual values;
false/no-Read, active missing/denied, malformed guard/default, scope/genesis and
first-refusal controls; independent complete law token vectors; and all 864
original inventory inputs. The public test binds the original graph (checked
byte-for-byte against program.zcve), decodes original raw schema fields, runs the
actual producer and evaluates a guarded shipment implication on its actual
frame. It independently checks decisions, payload/destination/channel, original
bytes, exact law reads and the producer-to-law shared usage delta. It is a
bounded law/bridge corpus, not a migrated inventory application.

The gate classifies SMT semantic refusal only on a diagnostic in the intended
actual function, with compile/VIR/resource failures excluded. Every proof run
records its solver budget, the default 10; exhaustion remains a refusal.

`change_declared_law_order` checks that evaluation slot `i` runs the `i`-th
declared law. Its specimen is the exact legacy mutant, which runs
`laws[0].program` in slot `i` (SHA-256
`55021af79009d390af1935f979047fd591c938ac0e3f238c33f3045f07647702`). The
control is declared native before any run, with a fixed oracle, as an explicit
change of evidence type. It is not an SMT result and not a fallback after a
failed proof; no proof is run for it.

Symbolic probes of this control remain inconclusive and are not counted. The
legacy mutant exhausted the solver at budgets 10 (hosted run 37404295156) and
40 at `laws.rs:199`. A `laws[0].id` diagnostic retarget (`7ce5b1d5…a11d`)
mixed a postcondition error with loop exhaustion at both budgets. A
`laws[i].id ^ 1` identity flip (`789a2e2c…87f4`) only exhausted the loop at
budget 10. Neither retarget is used.

The oracle is the public test
`guarded_declared_law_order_refuses_with_the_later_applicable_law` in
`tests/v2_guarded_laws.rs`, which the harness includes as
`public_guarded_laws`. Through the actual `bind`/`execute` API, an
authorized reservation of 1 from an available 2 (an otherwise accepted input)
meets law 1 first, true and applicable, then the later applicable law 60,
whose program is false. One assertion compares the refusal and ordered
diagnostics with literal expectations: `Law(Violated)` and verdicts
1 Satisfied, 2-3 Skipped, 4 Satisfied, 5 Skipped, 60 `Refused(Violated)`.
A dedicated classifier requires the baseline run to exit 0 with this exact test
passing. The mutant must build, exit 101 and run exactly this one test with
0 passed/1 failed/0 ignored. It must also show one panic at that assertion's
line, with the exact message and the mutant's literal left value (law 60
Satisfied, no refusal). Compiler errors, crashes, timeouts, absent or ignored
tests, unrelated or multiple failures and any other diagnostic refuse. The
receipt records the actual native argv, exit and intended target.

This native oracle is a concrete regression control. It shows this one input
catches this specific mutant. It is not a universal proof that the mutant is
wrong for every input, and not evidence that a symbolic run of the legacy
mutant completed. The positive whole-source formal proof of `evaluate_into`
against `law_execution`, with raw coverage, custody and no_std checks, is
unchanged at the default budget 10.

Coverage controls must genuinely verify before their intended raw function
contract/spec/body or inventory mismatch counts. Embedded source-array length
adjustments do not constitute a semantic kill or the intended coverage
refusal. Every specimen, stdout, stderr, command and exit status is retained.

Replay (all commands are offline and heavy suites keep one shared lock):

```
python3 tools/check_authority_v2.py --generate-sources verification/verus/authority_v2_sources.json
flock /tmp/zenofcis-v2-parallel/heavy-check.lock python3 tools/check_guarded_laws_v2.py --out /tmp/guarded-laws-replay
PYTHONDONTWRITEBYTECODE=1 python3 -m unittest discover -s tools -p test_check_guarded_laws_v2.py
```

Use a new evidence directory. `--refresh-coverage` is an explicit development
manifest generation step after reviewing final code, never a routine replay
option. `--positive-only` and `--controls` produce explicitly partial receipts.
Cargo validation uses only the shared target, incremental=0, one build/test
thread, Rust +1.97.1, --locked and --offline; required commands and inspected
results are in the final worker evidence manifest.

The acceptance receipt is separate from these procedures. An unrun or failed
gate leaves this unit incomplete. Root owns template migration, final combined
source identity/registration, Miri, global acceptance, exact-head review and CI.
Verus/vstd/Z3, Rust translation/erasure, compiler, allocator and platform remain
named trusted components. These results do not establish physical resource
costs, external authentication, atomic shell commit or successful delivery;
they do not become ADR-0003 independently checked solver proof objects.
