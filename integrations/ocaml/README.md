# OCaml scalar consumer

This Linux x86_64 profile uses OCaml 4.14.1 (`ocamlopt` and the standard
`unix.cmxa`) and Rust 1.97.1. It consumes an actual generated Rust application;
Rust owns canonical decoding, domains, decisions, laws, Authority, SQLite and
replay. This is an additional host consumer, not an OCaml decision emitter or
an independent evaluator. Qualification is source-bound to the installed native
and OCaml executable bytes; changing either requires a new binding and checks.

The profile is Linux x86_64 only and is not part of the macOS qualification.
Hosted acceptance installs the Ubuntu 24.04 packages `ocaml`, `ocaml-base`,
`ocaml-interp`, `libstdlib-ocaml`, `libstdlib-ocaml-dev` and
`libcompiler-libs-ocaml-dev` at version `4.14.1-1ubuntu1`, then checks the
compiler, runtime and library files against
`release/ocaml-4.14.1-ubuntu-24.04-amd64.sha256`. The acceptance scenario
`ocaml-scalar-consumer` runs `tools/qualify_ocaml_consumer.py --cli PATH` with
the CLI built from the same checkout. That driver generates and reviews the C3
and endpoint applications, builds them, compiles and tests the OCaml client,
runs the native tests and `tools/check_ocaml_consumer.py`, checks that two
planted overflow contracts are refused, and runs Clippy on both applications.
Its report sets `release_qualified` and `independent_review` to false.

The profile is `zeno-fcis/native-scalar-consumer/1`. Roots are closed flat
records of signed i64, Bool and payload-free Sum variants whose scalar codes
are their declared variant IDs. There are at most 16 state fields and 16
command/context fields in total. OCaml uses `int64` and `Int64.to_string`, never
floating point or its 63-bit `int`. Names use ASCII letters, digits, underscores and dots: raw field names are
at most 120 characters, qualified input names and variant names at most 128. UInt, codec Enum, bytes/text
fields, nested or payload values and wider integer domains are refused. Native
reports remain opaque strings, so state integers retain their decimal bytes.
Structured byte values are outside this profile; opaque canonical envelope bytes
are transported unchanged, with no OCaml ZCVE serializer.

A request begins with the eight-byte magic `ZFCISL1\0`, a little-endian u32 command length,
a little-endian u32 context length, both complete canonical native envelopes,
and EOF. The total cap is 65536 bytes. `frame --format json` emits exactly
`schema`, `profile`, `frame`, in that order, with lowercase hex and one LF.
The OCaml decoder rejects uppercase, odd/malformed hex, duplicate/extra members,
wrong identifiers and missing/extra newlines. Native `decide`/`submit --frame`
checks complete canonical reconstruction, including schema/root/type/field
identity; it does not trust the number projection alone.

Each client operation preflights the exact frozen `version --format json`
response, including the application identity and ABI/profile identifiers. The
handshake and operation share a 30-second wall-time deadline. stdout and stderr
are drained concurrently and each is capped at 32 MiB. The installed filesystem,
loader, clock and native executable are trusted. The supplied `authorized` Bool
is an application assumption, not authenticated identity or deployment authority.

Native exits are preserved: 0 success, 1 refused, 2 business reject, 3 I/O failure,
64 malformed CLI. The worked OCaml driver adds 65 for known local/transport
refusal and 66 for UNKNOWN process/deadline/output-cap outcomes. In particular,
UNKNOWN after submit can follow a commit. Do not retry automatically; reopen
with `state` and `history` and reconcile the actual native journal and head.
This transport is not a formal proof of the foreign client, compiler,
operating system or imperative shell.

Build the SDK in a fresh directory, keeping compiler outputs outside the source:

```sh
mkdir -p /tmp/ocaml-build
cp integrations/ocaml/zeno_scalar.ml integrations/ocaml/zeno_scalar.mli \
   integrations/ocaml/counter.ml /tmp/ocaml-build/
cd /tmp/ocaml-build
ocamlopt -version # must be 4.14.1
ocamlopt -c zeno_scalar.mli
ocamlopt -c zeno_scalar.ml
ocamlopt unix.cmxa zeno_scalar.cmx counter.ml -o counter
```

For the worked bounded-counter C3 component, generate ordinary untrusted contract
source with the supplied examples, then use the actual generator/scaffolder:

```sh
mkdir -p /tmp/ocaml-work
PYTHONPATH=tools python3 -c 'from pathlib import Path; import instantiate_core as c; c.instantiate("bounded-counter", {"C": 3}, Path("/tmp/ocaml-work/contract"), include_proof=False)'
zeno-fcis generate contract /tmp/ocaml-work/contract --format json
zeno-fcis new /tmp/ocaml-work/counter-app --contract /tmp/ocaml-work/contract --source "$PWD"
```

Build that generated app against its copied source-bound lock and pinned local
packages, checking its generated binding and resolved dependency graph as in
`tools/check_generated_application.py`. Install the resulting native executable,
the compiled `counter`, SDK sources and their hashes together. This route does
not install or claim a catalogue/component certificate; the separate complete
catalogue qualification gate remains required for that claim.

With absolute installed paths, capture the native version and run a real store:

```sh
/path/native version --format json > /path/version.json
/path/counter /path/native /path/version.json init /path/counter.sqlite
/path/counter /path/native /path/version.json frame /path/inc.frame \
  command.action=enum:Inc context.authorized=bool:true
/path/counter /path/native /path/version.json decide /path/counter.sqlite /path/inc.frame
/path/counter /path/native /path/version.json submit /path/counter.sqlite /path/inc.frame
/path/counter /path/native /path/version.json state /path/counter.sqlite
/path/counter /path/native /path/version.json history /path/counter.sqlite
```

`tools/check_ocaml_consumer.py` consumes fresh, source-bound C3 and endpoint
native apps, the compiled client and native-codec negative fixtures. It checks
all 16 C3 labels, 16 declared reachable endpoint tuples, 24 native negatives,
strict report aliases, complete native/client decisions, byte no-write refusals,
canonical SQLite rows/tokens, an installed worked journey and actual UNKNOWN
commit recovery. Its transport fault wrappers execute the real native owner
before delaying or adding excess output; they are local controls, not evaluators.
The endpoint corpus is finite and does not claim exhaustive i64 coverage.
