# Exact authority identity and persisted replay

Base 1ed6f88f7b1098bed6c409bc9aa3c12c99d43c3c. Root owns integration.

The mathematical subject is an immutable bound descriptor and evaluator source,
an invocation kind (Genesis or Transition), the original state/command/context
bytes and the complete library-constructed candidate and observations. Exact
canonical bytes, not a digest or branch code, decide replay equality. Genesis
is a distinct subject with its actual initial state and initial-law diagnostics.
Reject has explicit empty committing components; CommittedFailure retains its
complete declared changes. Technical refusal exposes no authorization.

Canonical framing uses a prefix-free sequence of Word(u128) and Bytes. Word
is tag 0 and 16 big-endian bytes; Bytes is tag 1, a 16-byte big-endian length
and its exact payload. Full-width length words preserve every machine usize;
total byte length uses checked usize arithmetic before any serialized output.
This version intentionally favors a single simple word representation over
several integer widths. Tags and sequence counts delimit structured fields.

Proof plan: exact word/framing correspondence; prefix induction on checked
length and encoding; exact candidate-component projection; exact entire-byte
comparison. All executable helpers have total contracts. Operational body
coverage remains separate from extensional results. Native standard-library
oracles construct complete independent encodings; mutation controls challenge
component omission, order, sign, lengths, equality and custody.

Only a private library path consuming the actual CompletedCore product can
seal authority. Public encoding/comparison functions are non-authorizing.
Bound policy holds immutable borrowed or owned fields; invocation cannot
replace descriptor, evaluator, usage, candidate, or law outcome. Retained raw
references are immutable for their Rust lifetime; encoded artifacts own bytes.
No shell write, dispatch, CAS, filesystem operation or external callback occurs
here. Shell atomicity, authenticated input acquisition, allocator/compiler,
verifier/Z3 and platform remain separate assumptions. Exact replay comparison remains byte equality. The evaluator source binding now
assumes SHA-256 collision resistance and the checked generation process below.

Status: implementation in progress. Root supplies the complete evaluator source
closure and catalog/envelope admission. Constructor remains private until the
actual descriptor/CompletedCore producer and these bindings are integrated.
No production/release authority is claimed by this bounded development unit.

## Evaluator digest and source admission

The authority binds the checked full policy bytes and a 32-byte evaluator digest.
The digest is SHA-256 over ASCII `ZENO-FCIS-EVALUATOR` plus NUL, u32 big-endian
version 1, u64 big-endian row count, then sorted rows of u64 big-endian path byte
length, ASCII path bytes, and SHA-256 of that row's payload. Rows include 91
approved relative source paths and three named build pins. Cargo.lock, manifests,
Rust config and toolchain remain included. The generated evaluator and the 18
cfg(test) files are excluded; tests remain in the separate specimen copy list.

`tools/check_authority_v2.py --generate-sources` emits the constant; `--check-sources`
recomputes it and checks a separately approved manifest SHA. Omitting a path fails
that manifest check even after regeneration. Duplicate, unsafe, missing and symlink
sources and missing/extra pins refuse. The independent RustCrypto test hashes the
workspace bytes and compares against the constant compiled into the authority.
Python and Rust share a fixed encoding vector. ATDD and CI require these gates.

No hash implementation enters the verified core. Unlike the former include_bytes
getter, an ordinary build alone does not establish that source bytes match the
generated identity. SHA-256 and the approved manifest, generator, independent test
and release process support this binding. Registering a new compiled module still
requires source-registration review. The old self-including getter and 17 package
mirrors are removed; package/compiler correspondence and portable archive checks
remain separate release-engineering obligations. No release qualification is
claimed by the workspace source check.

## Completed producer and custody proof plan

The authority consumes only the actual private immutable composition core. Its
constructor remains private pending root catalog and original-envelope binding.
The descriptor is borrowed immutably for the bound lifetime; Rust prevents a
retained mutable alias. An invocation supplies original byte slices only. The
returned evaluation owns the private core outcome and a separate sealing verdict;
only a successful verdict exposes a candidate and persisted subject. Core,
encoding or replay refusal retains actual usage/attempt/diagnostic reports.
Expected subject bytes are untrusted comparison input and cannot construct a
candidate, replace a meter or report law success.

- Establish prefix cancellation and exact injectivity of canonical Part streams.
- Derive identity and invocation framing injectivity from those prefix laws.
- Bind descriptor bytes to every actual producer field and the evaluator digest to
  the approved production source closure and version pins through the named gates.
- Derive the complete candidate and observations from the actual Outcome getters,
  prove exact total serialization and seal custody with no executable requires.
- Recompute using the same immutable bound core before full subject comparison.
- Qualify independent full-artifact native oracles, report preservation on refusal,
  external compile-failing forging/mutation controls and meaningful mutations.

No shell or database behavior is modified. Source/build activation intentionally
changes identity and requires new replay evidence. Allocator failure, source-to-
binary compiler correspondence, verifier/vstd/Z3 and shell atomicity remain named
assumptions rather than properties of canonical byte equality.
