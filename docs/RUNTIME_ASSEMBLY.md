# Experimental runtime assembly

This is a closed, source-bound assembler for reviewed scalar examples, not a
universal application framework or completed A1–A12 qualification. It adds
admission, configured-party wiring, build/test evidence, startup checks and
failure-atomic packaging around the existing app factory. The factory, checked
contract, Authority, SQLite publication and shell-effect programs remain the
implementation owners. An assembly manifest never authorizes a publication.

## Build the task-status example

Use the repository's pinned Rust 1.97.1, Python 3.11+, Node 22, cached dependencies
and installed Wasm target. No network dependency resolution is requested.

```sh
cargo +1.97.1 build --locked --offline -p zeno-fcis-cli
python3 tools/runtime_assembler.py --cli target/debug/zeno-fcis --prepare > /tmp/task-assembly.json
# Review the configuration, exact identities and experimental profile first.
target/debug/zeno-fcis assemble /tmp/task-assembly.json \
  --source . --out /tmp/task-app --scratch /tmp/task-build
```

Output and scratch must be outside the source tree. Output must not exist.
The build runs the existing native contract review and ZenoUI checks, generated
HTTP/SQLite journeys, task-specific real-process interruption tests, a release
binary build and an exact copied-binary launcher/HTTP journey on loopback port
8080: authorization refusal, a permitted lifecycle and local intent, exact-key
retries, and completed-request process restart/readback. A failed command, absent named test, changed source, unknown
profile or incompatible capability set refuses, and no destination package is
published. Diagnostic and mocked unit checks are not installed-app evidence.

On failure, the assembler retains the failed command's diagnostic and a separate
copy of the completed stage logs beside the requested output. The launcher log
is retained as `evidence/server.log`; temporary credentials are removed with the
smoke test's private scratch directory. Retained failure logs are for diagnosis.

The candidate description is at most 64 KiB and uses a closed JSON schema with
no duplicate keys. Source files are bounded to 128 MiB; the CLI executable is
stream-hashed with a 1 GiB limit. Exact profile identities, finite domain bounds,
party roles and capabilities are fixed by the reviewed catalog. Unknown schema,
additional fields, symlinks and authority-widening configuration refuse. Hashes
establish content identity, not behavior, authentication or whole-program proof.
The assembler does not execute manifest-provided commands.

`package.json` binds the final files, `assembly.json` binds the selected inputs,
and `evidence/checks.json` records actual command outcomes and log identities.
The retained factory manifest is explicitly intermediate: the assembler adds
acceptance tests after the factory emits it. The final source scaffold retains
absolute Cargo paths to the reviewed source tree; it is not advertised as a
portable standalone source release. The copied executable includes its UI assets.
Two-clean-build reproducibility and offline installation remain qualification gaps.

## Reviewed catalog, not arbitrary contracts

Two distinct entries use the same admission, factory, build and packaging path:

| Profile | Authored core and local effect | Separate evidence |
|---|---|---|
| `task-status-sqlite-local-test/1` | One finite task lifecycle; local intent ledger; trusted manual outcome observations | 2,646 native input tuples; real process exit after receiver apply/before ACK; Unknown and stale-attempt journeys |
| `inventory-reservation-sqlite-local-simulation/1` | Available/reserved stock; bounded reserve/release/ship/restock; a local warehouse-intent record, not physical shipment | 864 native input tuples; inventory authorization, bounds, intent and completed-request restart journeys |

Select the second candidate with
`--prepare --profile inventory-reservation-sqlite-local-simulation/1`.
Its configured parties are Operator (permission 1) and Observer (permission 0),
so it needs only their two secret references. Its restart journey is not an
interruption test. The task-specific crash/status checks are not transferred to
inventory. Each entry pins exact reviewed contract files and separate required
tests; another contract, even a valid standalone scalar contract, refuses until
an explicit catalog addition and qualification. Collections, composition `/3`,
remote effects and value-moving adapters are not admitted.

## Configure and start

The deployment owner supplies three separate private credential files, each
containing 64 lowercase hexadecimal digits and optionally one final newline.
Each file must be owned by the current user with no group/other permissions.
Credentials are never embedded in the description, assets or evidence.

```sh
export ZENO_ASSEMBLY_DATA=/absolute/private/task-data
export ZENO_ASSEMBLY_OPERATOR_SECRET=/absolute/private/operator.credential
export ZENO_ASSEMBLY_ADAPTER_SECRET=/absolute/private/adapter.credential
export ZENO_ASSEMBLY_OBSERVER_SECRET=/absolute/private/observer.credential
python3 /tmp/task-app/launch.py
```

Start from a fresh data directory or the exact previously bound directory.
The launcher checks packaged bytes, fixed party roles and principals, credential
references and permissions, then starts the packaged server at
`http://127.0.0.1:8080`. Inherited `ZENO_APP_*` settings refuse; arbitrary program
connectors, remote origins and alternative store/receiver paths are unavailable
through this launcher. Direct invocation of the raw binary or replacement of
this launcher by the deployment owner is outside this guarantee: this is not
an OS sandbox. Host, filesystem, compiler, dependency cache and deployment owner
remain trusted. The Python interpreter, loader/import environment and launcher
invocation are trusted as well; the manifest does not prove loaded runtime
behavior.

The UI is generated from the checked core's metadata. Operator requests and
cancels before Start, Adapter starts and reports observations, Observer only
reads. All parties share one task; no tenant confidentiality is claimed. A
receiver acknowledgement writes a durable local intent record; it does not run
a job or prove task success. The separate authenticated Adapter command reports
a trusted observation. Automatic payload/build/producer/delivery receipt
verification and receipt-to-domain processing are not implemented. Unknown
remains visible and cannot be retried as though it were failure.

## Data, queries and operations

- Queries: authenticated current scalar snapshot only, not paginated tables,
  arbitrary SQL or private tenant views.
- Migration: no implicit schema upgrade or re-adoption. A stored assembly binding
  mismatch or unbound existing data refuses. Stop and retain the original
  executable/configuration; an explicit qualified migration is future work.
- Backup/restore: no qualified export/restore command. Do not clone a live store
  or reuse its effect identity for another deployment. Consistent audited backups
  preserving pending delivery IDs, interruption recovery and corruption controls
  remain required before operational use.
- Recovery: keep the original request/key after a lost reply. The existing
  Authority-backed replay and durable outbox recover it. The local receiver
  deduplicates by original delivery ID under its trusted filesystem premises.
  Tests include real process exit after receiver application before ACK; this
  is not power-loss qualification or exactly-once external execution.
- Secrets: stop the server before rotating credential files; roles and public
  principals remain bound. No live issuer/audience/session/revocation service is
  claimed. Invalid secret/configuration startup fails closed.
- Operations: the existing authenticated `/state` exposes current state and
  pending count. Separate readiness/backlog health, retention, overload, disk-full
  recovery, structured operator procedures and rollback protection are incomplete.

## Qualification status

The RFC's A1–A12 gates are not all met. Native finite contracts, generated UI law
checks, authenticated byte-stream journeys, real-process local receiver recovery,
and exact copied executable loopback smoke are distinct evidence categories.
Browser usability/accessibility is not implied by UI law checks. In a container
that denies Chromium's required Unix sockets, record browser testing as blocked;
do not disable the host isolation policy to obtain a screenshot.

The capability list records the selected wiring; it does not extract or enforce
a whole-program authority graph. C1–C6 source/proof/OS conformance, hostile-host
containment, production receipts, migrations, backup/restore, comprehensive
operations and reproducible installation remain open. No merge, deployment or
production readiness follows from building this experimental package.

## Automatic and local checks

The existing `app-factory-journey` acceptance scenario now runs the cheap
assembler refusal/unit suite under normal and optimized Python. The existing
adopter-acceptance workflow runs that scenario through `atdd.py run --all`;
this wiring is not a claim that a hosted run has passed. Existing factory
journeys also exercise the corrected shared journey template.

The two full catalog assemblies, profile-specific runtime tests and exact
package loopback checks run when the `assemble` command is invoked. They are
explicit package qualification steps, not currently an additional scheduled
hosted-CI job. Inspect their retained evidence separately from the unit suite.
