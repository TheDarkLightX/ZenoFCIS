# Determinism

A ZenoFCIS transition must produce the same canonical bytes from the same
inputs on every run and every machine. The library relies on this: reopening a
store re-executes history and requires identical bytes, and anyone auditing a
history must be able to replay it. This guide says what the library guarantees, what it
checks, and what remains a contract for your code.

## How firmly determinism holds

| Code | How determinism is assured | What it rests on |
| --- | --- | --- |
| A synthesized finite core | By construction: the `finite-i64/1` language has no clock, randomness, input or output, or unordered collection | The interpreter and emitters, which are library code checked for ambient effects |
| The library's semantic crates | `tools/check_assurance.py` rejects ambient effects, and the crates forbid unsafe code | That script's rules |
| Your transition, adapter, and law engine | A contract, tested by the purity check and the probe below | The purity rule table, and the runs observed |

[Design record 0003](adr/0003-epistemic-status.md) classifies the probe and the
purity result as Checked: evidence about what was examined, never a proof.

Testing alone cannot prove determinism. A run shows only that the runs you made
agreed, and the sources of nondeterminism live in the environment: the clock,
hash-map seeds, thread timing, global state, memory addresses, and
platform-specific floating point. Two runs catch a source only if they differ
in it. So ZenoFCIS combines a static check, which finds sources whether or not
they change an output, with repeated execution, which finds changed outputs
whatever their source.

## The purity check

`zeno-fcis purity <PATH>...` parses Rust source and reports every source of
nondeterminism or ambient effect it recognizes:

| Rule | What it reports |
| --- | --- |
| `clock` | `std::time`, and the clock APIs of `chrono`, `time`, `quanta`, and `coarsetime` |
| `environment` | `std::env` |
| `filesystem`, `io`, `network`, `process` | `std::fs`; `std::io` and `std::os`, plus `print!`, `println!`, `eprint!`, `eprintln!`, and `dbg!`; `std::net`; `std::process` |
| `threads` | `std::thread`, channels, barriers, condition variables, `rayon`, `crossbeam`, `tokio`, `async_std`, and `futures` |
| `randomness` | `rand`, `getrandom`, `fastrand`, `oorandom`, `RandomState`, and `Uuid::new_v4` |
| `hash-order` | `HashMap`, `HashSet`, and `hashbrown`, whose iteration order can change between runs |
| `shared-state` | atomics, `Mutex`, `RwLock`, `OnceLock`, `LazyLock`, `std::cell`, `static mut`, `thread_local!`, and `lazy_static!` |
| `address` | `std::ptr`, raw pointer types, `.addr()`, `.expose_provenance()`, and `{:p}` formatting |
| `unsafe`, `foreign-code` | unsafe blocks, functions, impls, and traits, inline assembly, `extern` blocks, `libc`, and `std::ffi` |
| `floating-point`, `unstable-hash` (warnings) | `f32` and `f64`, and `DefaultHasher`, whose results can differ between platforms or toolchains |

It resolves `use` aliases, grouped and glob imports, `extern crate` renames,
and `core::` and `alloc::` paths. It checks a qualified macro's own path, such
as `core::ptr::addr_of!`, and scans every macro's arguments for paths. A local
variable named like a crate, such as `rand`, is not reported.

Point it at decision code: the transition, its adapter, and the law engine.
Shell code such as `main.rs` or a delivery adapter performs effects by design,
and the check reports them. In the durable-counter template:

```text
$ zeno-fcis purity src/program.rs src/laws.rs synthesized/transition.rs
purity: clean (0 errors, 0 warnings) in 3 files
```

A directory containing `Cargo.toml` is checked as a crate: every `.rs` file
under `src/`, plus six structural conditions. A crate is *confined* when
- it is library-only: its root is `src/lib.rs`, and it has no `src/main.rs`,
  no `src/bin/`, and no `[[bin]]` target;
- its root is unconditionally `#![no_std]`;
- it has `#![forbid(unsafe_code)]`;
- no file declares `extern crate std`, at any depth or inside a macro body;
- no file names `include` or has a `#[path]` attribute, either of which can
  bring in source from outside `src/`; and
- the check reads the whole manifest, and every dependency is named as one of
  the library's semantic crates.

A confined library cannot name `std` directly. That puts the clock, the
environment, files, the network, threads, and `HashMap` out of reach, and the
rules cover what `core` still offers, such as atomics and raw pointers. Its
only other route is its dependencies, which are named as the library's
semantic crates that `tools/check_assurance.py` checks for ambient effects.
Putting decision code in its own confined library crate is the strongest
check available for hand-written Rust.

The manifest reader accepts the common forms of `[dependencies]`,
`[dependencies.NAME]`, and their `target` variants, and ignores development
and build dependencies. It compares names with quotes and spaces removed,
so `"path"` and `["lib"]` read as `path` and `[lib]`. Forms it does not
recognize are reported, not interpreted. A form that could add a dependency,
rename one, or change the package's targets keeps the crate from being
confined and is reported, including:
- `package =` renames and `workspace = true` inheritance;
- `[patch]` and `[replace]`;
- a `[lib]` `path`, `autolib`, and any `[[bin]]` target;
- escape sequences in library keys or dependency tables, and multi-line
  strings in dependency tables;
- dependencies or targets declared outside their tables, such as
  `lib.path = "..."`.

Dependencies are identified by name: the check does not verify that a
dependency named `zeno-fcis-core` comes from the library's registry release
rather than a path or git source.

The result is `clean`, `confined`, `violations` (exit 1), or `unreadable`
(exit 3). Only error-level rules decide it: warnings are reported, and a
clean or confined result can include them. Anything the check could not read
makes the result `unreadable`, never clean:
- a path it cannot read or list;
- a directory with no Rust source;
- a file that is not a regular file or is larger than 16 MiB;
- a symbolic link to a directory or a Rust file inside a directory it walks.

Links are not followed, so list a link's target instead. Cargo build
directories, named `target` and marked with `CACHEDIR.TAG`, are skipped.
`--format json` prints a `zeno-fcis/purity-report/1` record with every
finding's file, line, column, rule, and subject.

What it cannot see:
- It does not expand macros; it scans their tokens for paths.
- It does not read files you do not list, generated sources, or dependencies.
- In a crate, it reads only `src/`. Build scripts, tests, examples, and
  benches are not read.
- It does not know the type of a method's receiver, so it matches only
  `addr`, `expose_provenance`, and `expose_addr` by name.
- It does not know the type of a cast's operand, so it does not report a
  function converted to an integer, as in `decide as usize`, which yields its
  address.
- It does not report platform-dependent sizes such as `usize` and `size_of`.

Source nested more deeply than the parser's stack allows stops the check with
a nonzero exit and no result.

A clean or confined result is Checked against this rule table. It is not a
proof of determinism.

## The determinism probe

`CatalogCommitAuthority::execute_probed(invocation, runs)` executes one
admitted invocation several times, from 2 to 64. Each execution gets its own
copy of the admitted values. The program, the law engine, and process-wide
state are shared, so state kept between calls shows up as a divergence when
it changes the compared bytes. The program and the project law engine run
every time.

It returns the first decision only if every execution produced identical
canonical decision bytes. For an acceptance or a committed failure, those
bytes hold the invocation with every binding, and hashes of the law
evaluation and of the complete candidate bundle. For a rejection, they hold
the law evaluation, reason, and receipt themselves. Equal hashes imply equal
contents only because SHA-256, the hash of both approved commitment
providers, is assumed to resist collisions, as it is everywhere else in the
library. If any execution differs from the first, the probe withholds the
decision:
- `ProbeError::Diverged` names the execution that differed;
- `ProbeError::FailedAfterDecision` reports a later execution that failed.

A probe of one execution is refused, because it compares nothing.

The returned `DeterminismProbe` carries a digest of the agreed decision.
Digests from separate processes can be compared, which is how the
changed-environment runs below work.

## Changed-environment runs

All executions in one probe share a process, so they share its environment,
nearly the same clock reading, and its memory layout. The durable-counter
template's `tests/determinism.rs` shows how to widen that:
1. Probe every admitted input eight times in the test process.
2. Run the same corpus again in child processes started with:
   - a cleared environment;
   - another time zone and locale, with glibc filling allocated memory with a
     pattern;
   - another working directory.

   Each child is a fresh process, so it also gets new `HashMap` seeds and,
   where address-space layout randomization is enabled, new addresses.
3. Require every child's decision digests to equal the parent's.

## What each check catches

These controls were planted in the durable-counter adapter:

| Planted change | Probe in one process | Child processes | Conformance tests | Purity check |
| --- | --- | --- | --- | --- |
| An extra state read on every second call, which leaves the decision unchanged | passes, correctly | passes | passes | reports the `static` counter |
| Skipping the context-root observation on every second call | withholds the decision | withholds the decision | pass | reports the `static` counter |
| Skipping it only when `LANG` is set | passes | reports the differing digest | pass | reports `std::env::var_os` |

The conformance tests compare every decision, reason, state change, and
notification with a model, and none of these changes altered what they compare.
Only the determinism checks caught them.

## What agreement means

Agreement from a probe or from changed-environment runs is Checked evidence
about the runs observed. It does not prove determinism for other runs,
environments, inputs, or machines. Reopening a SQLite store gives one more
check of the same kind: it re-executes every persisted transition and fails
closed if any bytes differ. The strongest position for hand-written code
combines three things:
- a confined decision crate;
- a probe of every admitted input, or of a representative corpus when the
  domain is large;
- changed-environment runs in CI.

Where the decision fits a finite domain, synthesizing it gives determinism by
construction.
