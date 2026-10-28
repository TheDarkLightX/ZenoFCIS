# Optional finite relational profile

ZAL is a general abstraction language. This package provides one **optional,
experimental** way to reason about supported finite C2 and C2mod formulas; it
does not define the boundary of ZAL or generate an application. The Haskell
implementation is retained from PR 153 at commit
`73068b03598a39253af8870392674bab58a723c5`. `SOURCE_MANIFEST.json` binds the
32 selected upstream files to their Git blob IDs and SHA-256 hashes, including
all eight production Haskell files and upstream license notices. The upstream
Python shell, ladder adapter, independent oracle, test sources, specifications,
examples, and historical `VALIDATION.json` remain unchanged under `vendor/`.
Historical validation does not qualify this integration's current files.

The retained `vendor/` documents describe the full [public upstream source at the
pinned revision](https://github.com/TheDarkLightX/ZenoFCIS/tree/73068b03598a39253af8870392674bab58a723c5/experiments/zal-relational).
This integration retains a selected 32-file capsule, rather than that entire
checkout. Its historical demo, replay and qualification instructions require
upstream files not included here: `demo_decisions.py`, `replay_result.py`,
`validate.py`, `check_fcis_bridge.py`, `check_proof.py` and `fcis/contracts/`.
Use the prepare/run commands below for this integration. To reproduce the
published historical validation, use the full pinned upstream checkout; that
reproduction is a separate result. The vendor notice's project-level third-party-notice reference also belongs
to the full upstream checkout.

## Prepare and use

The provider is not installed automatically and no executable is shipped here.
Create a new external runtime directory, then build its copied source with the
pinned GHC environment described by `vendor/build.py`:

```sh
PROFILE=integrations/zal/relational_profile
python3 -I -B "$PROFILE/__main__.py" prepare /tmp/my-new-relational-runtime
python3 -I -B /tmp/my-new-relational-runtime/build.py
python3 -I -B "$PROFILE/__main__.py" run /tmp/my-new-relational-runtime \
  "$PROFILE/vendor/examples/even-graph-count.json"
python3 -I -B "$PROFILE/__main__.py" run-ladder /tmp/my-new-relational-runtime \
  --context "$PROFILE/vendor/examples/owner_cars_context.json" \
  --rung intuitive --text 'every owner has a car.' --domain-size 2
```

`prepare` copies the exact retained provider files; it does not build or copy a
worker. It refuses an existing destination. The separate upstream builder checks
GHC 9.4.7 and its five direct package versions, compiles the worker, and records
source and executable hashes. Keep build output and evidence outside a source
checkout. Production users must supply and assess their own trustworthy build
provenance: matching a local build record and binary hash is not a proof that a
compiler produced that binary from those sources.

The Python API is `profile.execute(request, runtime, limits=..., seconds=...,
scratch=...)`. A request is a JSON object or bytes in one of the upstream
`zal/relational-query/1`, `zal/relational-evaluation/1`, or
`zal/relational-comparison/1` schemas. `bridge.lower_tree(ladder, context, tree,
domain_size)` translates a supported current-ladder AST. For source-bound
text input, `bridge.lower_text(...)` records the original reading and hashes of
the raw context, executed ladder and bridge bytes, the registry read by that
ladder, syntax tree, canonical query and provider manifest. It returns a
`zal/relational-lowering/3` envelope; `bridge.lower_and_execute` retains that
envelope with the answer in a `zal/relational-ladder-result/3` packet. Parsing
or lowering does not imply human semantic approval.

`bridge.comparison_request(left, right, assumptions, relation=...)` accepts
three successful `/3` lowering envelopes, validates their tree and query hashes, and
requires the same context identity, logic, signature and domain. The identity
hashes the supplied context scope (including observation and derived-symbol
definitions), captured ladder, registry and bridge bytes, and provider manifest.
Its scope excludes the context label, selected formula and example fixtures, so
different formulas can be compared under the same pinned scope. JSON object-key
order does not change the identity; list order and changes to definitions do.
The context digest uses typed, length-delimited data and retains decimal token
spelling exactly, so decimal rounding cannot merge different supplied scopes;
numerically equal tokens written differently may conservatively get different
identities. The ladder evaluator retains its existing numeric behavior. Matching
identities establish matching supplied source, not mathematical equivalence of
differently written definitions. Hashes in ordinary Python data
are provenance checks, not authentication against a caller able to forge packets.
`lower_tree` is a trusted-local API and labels its output unbound; it cannot be
passed directly to `comparison_request`.

The qualified standalone route is direct execution of the trusted
`__main__.py` with `python3 -I -B`, as shown above. It reads both local modules
as bounded bytes, verifies its pinned hashes before executing either, and does
not import ancestor package initializers or bytecode caches. Its ladder parser
receives a temporary capsule containing the captured registry bytes. The result
metadata identifies this as `profile_binding: captured-standalone`. Ordinary
Python imports (including `python -m`) can execute ancestor initializers before
this boundary and are trusted-local invocation; their profile metadata says
`trusted-local-import` and its file hash is not a claim about previously loaded
code bytes. Neither route is a hostile-host sandbox.

## Outcomes and bounds

The result schema is `zal/relational-profile-result/1`. `ExactCount` is a
completed exact rational count for the supplied finite labeled domain;
`Evaluated` is the truth value on the supplied complete model. `EquivalentAt`
and `ImpliesAt` cover only the declared size, signature and assumptions;
`RefutedAt` carries a counterexample. The wrapper checks the comparison's
coverage and partition and independently evaluates any witness with the
retained original-formula Python oracle, subject to its own work cap.
`VacuousAt` has no admitted models and is not a positive relation result.
Signed or weighted zero is not a satisfiability verdict.

`Unsupported`, `Invalid`, `Unavailable`, `UNKNOWN`, and `Error` are distinct
non-success outcomes and have no semantic result. `UNKNOWN` includes exhausted
input, work/domain and process time limits; a partial result is not
returned as a completed one. The CLI exits 0 for completed counts/evaluations,
nonvacuous positive comparisons, preparation or lowering; 1 for a refutation;
and 2 for vacuity or any non-success. Always inspect the JSON status.

Requests and each response stream are limited to 1 MiB. A one-request worker
process has an address-space limit of at most 2 GiB, a CPU and wall limit of
1–60 seconds, no core dump, and a 1 MiB output-file limit. A small single-threaded
launcher applies limits and `exec`s the worker; the caller does not use a
multithreaded `preexec_fn`. The wrapper verifies retained provider blobs before
loading its upstream Python helpers, and checks the runtime source inventory,
build record, and executable hash before and after
execution. This is not a hostile-host sandbox or a cryptographic compiler
attestation; source/binary hashes do not remove filesystem races.

The ladder bridge supports one nonempty finite domain, unary/binary predicates,
equality, Boolean operations, quantifiers, ordinary counting comparisons and
at most two simultaneously live variables. It preserves `exists1` as an exact
count of one and refuses primed, temporal, multi-domain, third-live-variable,
function, and other unsupported forms. C2mod is available in the JSON provider
format; modular-counting controlled language is not added. The original ladder
and other ZAL representations remain available independently of this profile.

The complete Haskell implementation proof, checked relational-to-FCIS lowering,
general or unspecified-size satisfiability, native registry installation,
application/UI/effects generation and full 2.7 release qualification remain
outside this package's demonstrated scope. See `vendor/ASSURANCE.md` and
`vendor/OPERATIONS.md` for the original provider's precise claims.
