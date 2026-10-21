# Composed-domain proof boundary

The composed-domain profile `/3` has a conditional Verus theorem over finite
histories and two shared runtime functions. It does not prove the complete
composition checker. Existing result claims and receipt wording are unchanged.

## What is proved

`verification/verus/composed_domain_theorem.rs::joint_induction` proves that,
from checked genesis and the `local`, `assume` and `couple` obligations:

- every reachable owned state satisfies its domain and component invariant;
- every coupling invariant holds at every reachable state;
- every participating delivery is defined, inside its domains and satisfies its
  assumption and state invariant, including a read-only component's state.

The model binds participation and command/context inputs to one request and
observations of earlier owning components' pre-state. Read-only state bindings
receive only those observations. Reject, Refused and Skip are allowed by
`local`; only committing decisions must preserve the invariant. Read-only
commits preserve the delivered state. Couplings refer only to owned state.

The coupling premise checks member endings and every possible outside failure
cut. It over-approximates failures using the manifest's failure-case flag, even
when that outside failure is unreachable on the current request. A separate
three-component pilot discharges its obligations: owners 0 and 2 are coupled by
an order invariant, component 1 is a read-only guard with a possible committed
failure, and the owners can be non-participants. The general theorem admits
arbitrary component and coupling counts, subject to its explicit premises.

`resolve.rs::resolve` implements the ordered scan. Its contract fixes the
result to `resolve_spec`, commits no state after Reject or Refused, and includes
the first committed failure in its prefix. The proof requires callback permission only
through the first terminal decision; an extra call after Reject fails
verification. The loop has a decreasing finite bound.

`resolve.rs::apply` implements `apply_spec` on the production `Vec<i64>` state:
it frames skipped, later and read-only components and rolls back the whole
state on Reject or Refused. The abstract joint step uses both shared specs.

The resolver takes `FnMut`. Its production callback reads immutable tables and
pre-state, and records payloads in fresh, exclusively owned invocation scratch.
Each visited component is evaluated once; later components never run. The
scratch and callback cannot escape the invocation. Application uses the concrete
state representation because arbitrary user-defined `Clone` need not preserve
its value or ownership. The shared-source application proof covers nested
integer vectors; the abstract application specification is generic.

## Bridges and trust

| Bridge | Status |
|---|---|
| Outcome resolver and concrete state application | Proved, shared runtime source |
| Finite-history induction over the declared model | Proved, conditional on obligations and bindings |
| Local decision tables made by the bound Authority | Tested premise |
| Manifest parsing and scope validation | Tested premise |
| Formula lowering and table evaluation | Tested premise |
| Planner sizes and exhaustive enumeration coverage | Tested premise |
| Member simulation and outside cuts imply the abstract coupling premise | Tested premise; simulation keeps its separate over-approximation loop |
| Runtime local payload/kind mapping to abstract decisions | Tested premise; original-step differential and independent reference comparisons |

The trusted proof tools are the pinned Verus translation and erasure, its
bundled vstd specifications and solver, the Rust compilers, and the reviewed
mathematical specification and coverage profile. No new assumed specification,
unfinished proof or external body is used on the proved path. The runtime
closure borrows immutable inputs and exclusive local scratch; application returns fresh owned vectors and
exposes no mutable aliases. Source-bound checker identities intentionally change
when these implementation files change.

## Replay and controls

Use the exact tools pinned in `verification/verus/toolchain.json`:

```sh
python3 tools/test_check_composed_domain.py
python3 tools/check_composed_domain.py --out evidence/composed-domain.json
python3 tools/test_composed_domain_runtime_controls.py --out evidence/composed-native.json
cargo +1.97.1 test -p zeno-fcis-contract --locked
cargo +1.97.1 test -p zeno-fcis-cli --locked --test contract_check_composition_domain
python3 tools/test_composition_domain_reference.py
```

The proof gate requires an unfiltered successful run, exact translated contract
and specification inventory, and reviewed runtime body fingerprints. Each
mutation runs in restricted module verification and must fail semantically at
its named obligation. A missing tool, build error, unrelated diagnostic, timeout
or incomplete run is a failure of the gate, never a caught defect.

Controls remove an outside failure cut or the read-only invariant premise,
shorten the committed prefix, retain only a failing component's successor,
write a read-only component, and call the decision callback after Reject.
Runtime controls also force a non-participant to decide, build each specimen,
run the existing named test, and restore and rerun it. The own-only mutant's
actual CLI output is compared with the independent Python reference.

The outside-cut native witness starts a counter and budget at 0 and 2. Their
full step preserves sum 2; a tripwire between them commits after the counter,
leaving sum 3. The read-only model witness delivers provider value 1 to a guard
whose invariant requires 0: the value is in its domain but violates its
invariant. `read_only_invariant_counterexample` checks that witness.

This work establishes no liveness, fairness, storage atomicity, concurrency,
delivery, effects, publication authority or correctness outside admitted domains.
