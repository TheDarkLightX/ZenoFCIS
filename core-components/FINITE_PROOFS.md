# Source-bound finite family certificates

This document specifies the new G1 certificate route. Check the actual issued
artifacts and replay result before asserting a successful proof. It does not
reclassify the earlier native-test report or add a Lean backend.

For each family, the theorem quantifies over every declared parameter instance
and every member of the entire raw state/command/context product. The original
three products are unchanged: 9 reservation instances/2,216 inputs; 9 fixed-window
rate instances/2,646 inputs; 3 single-request approval instances/1,296 inputs.
The seven additional products cover C=1..8 each: counter/176 inputs, budget/568,
versioned register/30,664, idempotency slot/61,328, retry budget/352, finite phase
machine/176 and logical deadline/8,096. Together these are 77 instances and
107,518 raw inputs. The catalogue table gives each exact input product.

On all those inputs, the actual mandatory library Authority returns a business
decision, every applicable transition law passes, and the complete class,
reason, successor fields and deliveries agree with the independent reference.
There is a feasible acceptance for every intended command kind, and rejection is
unchanged and delivery-free. Invalid schema-admitted prestates are included:
state invariants apply at genesis/commit; malformed prestates business-Reject
under the reject/conformance laws, without a hidden law refusal.

Numeric arguments are not separate command kinds. The register has one Write
command; expected_version=C cannot accept under a bounded, nonwrapping version
policy. The check still requires an actual accepted Write, both counter
directions, both retry actions and both phase-machine actions.

Reservation conservation includes explicit external inputs/outputs: Reserve and
Release conserve available+reserved; Consume removes exactly quantity;
Replenish introduces exactly quantity. Supplied authorization does not prove
physical inventory or authenticated permission. Rate conservation means one
quota charge per accepted request, including the first request after expiry.
Approval conservation means one distinct supplied vote slot per accepted Vote,
framing other slots, and unchanged votes at Execute. It is a single request,
not a FIFO or authenticated human approval system.

Per ADR 0003, successful replay has level **Proved for this complete finite
scope**, using named exhaustive library interpretation. A stored certificate
alone is **Identified**. `KernelChecked` is not used: no Lean kernel checked
this argument. Solver status is irrelevant to this finite route. Genesis is
separate: the retained generated-native report checks each exact genesis and
all 308 required native tests; this transition certificate does not replay those logs or
claim a new genesis/temporal induction theorem. Issuance verifies identical
canonical artifacts against that native report, preserving its actual scope.

`tools/prove_core_families.py issue` uses a CLI built from the frozen local source
and normal F1 generation, regeneration and F2 review. It checks full Cartesian
coverage independently of packet tallies, decodes every actual decision and
compares exact observations with the separately authored reference, requires
zero technical refusals over ALL raw inputs, and records positive witnesses.
The code never evaluates an application decision in a handwritten callback.
Before the first CLI dispatch, it snapshots every family's source/checker inputs,
the compiler/evaluator inventory and the executable bytes. Certificates retain
those initial bindings. Any input or executable drift refuses issuance before
certificate files or a successful report are written; a late snapshot cannot
substitute changed source for the source used during checking.

Ten canonical JSON certificates bind the family templates, every example,
checker/instantiator sources, compiler/evaluator source inventory, exact CLI
binary, every domain and parameter, canonical schema/policy/Rust artifacts,
Authority identity, laws and complete normalized decision digest. Certificates
are excluded from their own source inventory, preventing a hash cycle. The
source inventory still includes every crate input and root dependency/toolchain
files; modifying code requires requalification, not silently accepting stale
certificates. A certificate at a different source revision is not portable
proof of that revision.

```sh
python3 tools/prove_core_families.py issue --cli /disk/exact-source/zeno-fcis \
  --native-report /disk/retained-native/qualification/report.json \
  --work-dir /disk/new-issue
python3 tools/prove_core_families.py replay --cli /disk/exact-source/zeno-fcis \
  --work-dir /disk/new-replay
```

The issuer refuses existing certificate files. Replay recompiles/regenerates
contracts and re-executes the entire space; it does not trust cached counts,
verdicts or witness tables. It must reproduce each certificate byte-for-byte.
The CLI executable is prebuilt; this tool does not itself compile a CLI or
assert provenance of a caller-supplied binary. Its successful qualification
records the actual pinned source build separately. A wrong binary fails replay.

For a different machine or a new source build, run the current-build check:

```sh
component_target="$PWD/target/core-components"
component_host="$(rustc +1.97.1 -vV | sed -n 's/^host: //p')"
cargo +1.97.1 build -p zeno-fcis-cli --locked --offline \
  --target "$component_host" --target-dir "$component_target"
python3 tools/check_core_components.py --finite-proof \
  --cli "$component_target/$component_host/debug/zeno-fcis"
```

The explicit target and output directory make the executable path independent
of local Cargo target settings. The offline build needs the locked dependencies
already available locally.

This checks an isolated copy of the declared source inputs, issues fresh
certificates for that CLI, replays all 107,518 raw transitions, and runs the law,
tampering and scaffold-reference controls. It preserves the stored certificate
set. The output names the evidence directory; failures retain their logs there.
Genesis evidence is inherited only when the canonical artifacts match the
retained native report exactly. This command does not rerun native genesis tests
or turn stored references into portable proofs of a different build.

The normal instantiator requires a current certificate and writes its exact hash
and source-tree location into `core-instance.json` and a `project.zeno` comment.
Ordinary `new --contract` carries that comment into the app. The reference says
Identified and replay-required; source-hash agreement alone is not replay. The
existing Authority remains the only decision/publication authority and does not
trust these advisory references. The internal certificate-construction route
renders the same rules without a self-referential certificate comment; its
canonical generated artifacts must be identical to the referenced app's.

Trusted base: the F1 compiler, canonical codecs, mandatory Authority, eager-i64
and law interpreters, F2 enumeration/framing/reporting, explicit finite-product
and identity comparisons in this replayer, Python/Rust toolchains/runtime, OS,
SHA256 and machine execution. These are named assumptions, not a newly proved
implementation theorem. The independent reference supplies intended finite
behavior; owner labels and release approval remain absent.

Qualification requires seventeen real law-mutant refusals across the ten families, missing-parameter/domain/
decision/source/binary tampering controls, source freshness and proof references
surviving normal scaffolding with identical canonical artifacts. A failed,
partial, timed-out or stale check issues no passing certificate. Run heavy work
only in the coordinated AUX slot with the prescribed bounds and disk guard.
