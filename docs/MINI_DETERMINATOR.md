# Mini Determinator semantic reference

The Mini Determinator in ZenoFCIS `1.0.0-rc.3` is an original, public,
self-contained model of shared-nothing deterministic coordination. Its design
is informed by Determinator's public model of private spaces and deterministic
inter-process synchronization. It does not copy Determinator source.

## Executable state machine

`MiniState` is a canonically sorted finite map from stable slot IDs to `i128`
values. A `WorkerProgram` has a stable worker ID, conservative read footprint,
complete write footprint, and finite instruction list:

- `Constant`, checked `Add`, `Subtract`, and `Multiply`;
- `Get`, which reads the worker's private overlay or immutable spawn snapshot;
- `Put`, which writes only the worker's private overlay;
- `Return`, which terminates and returns the accumulator to the coordinator.

All programs are validated against worker, write, and per-worker step budgets.
Reads and writes require footprint evidence. Missing cells, arithmetic
overflow, missing return, instructions after return, invalid completion-order
witnesses, and exhausted budgets block the whole transition.

`MiniDeterminator::execute_programs` accepts an arbitrary worker completion
order but evaluates semantics in canonical worker-ID order. The completion
order must be an exact permutation of the worker set and cannot affect
`MiniRun`. Worker traces are canonical by worker ID; reads remain in program
order and final private writes are sorted by slot.

At join, all worker returns and deltas are sorted by worker ID. Disjoint writes
merge into one next state. A write/write collision returns one stable witness:
the slot and the lower then higher worker ID. Rejection or blocking leaves the
caller's exact pre-state unchanged.

## Run it

```bash
cargo +1.97.1 run -p zeno-fcis-spec --example mini_determinator --locked
cargo +1.97.1 test -p zeno-fcis-spec mini_determinator --locked
cargo +1.97.1 run -p zeno-fcis-cli -- new /tmp/mini \
  --template mini-determinator
cargo +1.97.1 run -p zeno-fcis-cli -- check /tmp/mini/project.zeno
```

The example runs two private workers under opposite completion orders and
asserts byte-for-byte equal complete results. The tests cover replay,
isolation, conflict, rollback, overflow, budgets, missing footprint evidence,
and invalid lifecycle states.

## QEMU kernel demonstration

The isolated [QEMU kernel demo](QEMU_MINI_DETERMINATOR.md) boots a freestanding
`no_std` x86_64 guest and runs this public semantic API after UEFI handoff. It
captures the real guest framebuffer and validates the exact COM1 transcript.
The demo stays outside the public package graph and does not change any
canonical library format or authority boundary.

## Nonclaims

The model and kernel demo demonstrate deterministic behavior for this finite
interpreter and its explicit join rules. They do not establish process
isolation, hardware determinism, scheduling fairness, interrupt correctness,
crash recovery for a production kernel, or production authority.
