# ZAL types and effect signatures

**User story.** As an application developer, I want to read my shell's effect
ports in plain language: what each port takes and answers, what it may touch,
which laws its handlers must obey, and how many values each argument admits. I
want what I review to be exactly what the shell library uses, so a change on
either side fails a check instead of drifting silently.

## A first question

Start from one ask: "What may Commit touch, and what do its answers mean?"

```sh
python3 integrations/zal/run.py explain integrations/zal/examples/shell_effects.zal --topic port:Commit --human
```

```text
Port Commit (tag 2), checked: pass-with-scope
Operation Commit takes store (Store), key (Hash), publication (Publication), mode (Mode) and answers Outcome of CommitReceipt.
Operation Commit needs capability WriteStore, reads durable and volatile, writes durable and volatile, obeys laws 1 and 2, and maps to the V2 calls V2SqliteShell::commit, V2SqliteShell::commit_at and V2SqliteShell::commit_bounded_at.
Arguments admit more than 2^4096 values.
  ok: CommitReceipt (2^320 values); the operation took effect
  refused: Code (22 values); nothing changed
  not_applied: Fault (3 values); nothing changed
  unknown: Fault (3 values); it may or may not have taken effect
Footprint stays within capability WriteStore; touches durable, volatile.
```

The answer comes from the checked source `examples/shell_effects.zal`, which
declares the shell's ten ports. Lowering that source gives the
`zeno-fcis/shell-effects/1` record of `zeno_fcis_shell::effects::registry` byte
for byte, so its SHA-256 identity is the registry's own, `f97591cd…`.

## Three profiles, one type language

| Profile | Header | What it adds |
| --- | --- | --- |
| `zal/finite-types/1` | (inside the other two) | Aliases, records and variants, with exact value counts |
| `zal/effects/1` | `zal effects 1;` / `ZAL effects version 1.` | Effect ports with types, capabilities, footprints, laws and frames, lowered to the record |
| `zal/typed-fsm/1` | `zal typed 1;` / `ZAL typed version 1.` | Profile 2 machines whose variables and inputs have finite types |

`zal/finite-fsm/1` and `/2` are unchanged: the same source gives the same
revision, verdicts and renderings.

## Finite types

Each declaration has a symbolic and a controlled-English form, and both parse
to one value:

| Symbolic | English | Values |
| --- | --- | --- |
| `type Status = committed \| replay;` | The type Status is one of committed, replay. | 1 + 1 = 2 |
| `type Pair = {a: Status, b: bool};` | The type Pair is a record with fields a (Status), b (a Boolean). | 2 x 2 = 4 |
| `type Mode = unversioned \| at(Version) \| bounded_at(Bounded);` | The type Mode is one of unversioned, at (Version), bounded_at (Bounded). | 2^127 + 2^63 + 1 |
| `type Hash = bytes 32;` | The type Hash is exactly 32 bytes. | 2^256 |
| `type Envelope = bytes ..=1048576;` | The type Envelope is at most 1048576 bytes. | more than 2^4096 |
| `type Small = list ..=3 of Status;` | The type Small is a list of at most 3 items of Status. | 1 + 2 + 4 + 8 = 15 |

- **Primitives:** `unit` (nothing), `bool` (a Boolean), `int LOW..=HIGH`,
  `bytes ..=CAP` (0 through CAP bytes), `bytes LENGTH` (exactly LENGTH bytes)
  and `list ..=CAP of TYPE`. Caps and lengths lie within 1..=1048576.
- **Counts:** a record multiplies its fields' counts, a variant adds its cases'
  counts, and a case without a payload counts one. Counts are exact integers.
  A count above 2^4096 is reported as "more than 2^4096", never rounded.
- **Refused by name:** a repeated field (`Record R repeats field x`), a repeated
  case tag (`Variant V repeats case a`), an undeclared type, a recursive type
  with its cycle (`Type List is recursive (List -> List); only finite types are
  admitted`), nesting deeper than 8 levels, and empty variants.
- **Values** are written the same way in both surfaces: `5`, `true`, `0x0a0b`,
  `[committed, replay]`, `{a: replay, b: true}`, `locked(2)`. Every type has a
  canonical enumeration: integers ascending, false before true, records in
  field order, variants in case order, bytes and lists by length and then by
  position.

## Effect signatures

`examples/shell_effects.zal` shows every statement. In order:

```text
zal effects 1;
effects shell;
schema zeno-fcis/shell-effects/1;
resource durable = 1;
capability ReadStore = 0: store, reads {durable, volatile}, writes {volatile};
answer Outcome = 0: ok(T) | refused(Code) | not_applied(Fault) | unknown(Fault);
frame Outcome: ok applied, refused unchanged, not_applied unchanged, unknown either;
ports ReadState = 1, Commit = 2, ReadHistory = 3, ...;
type Snapshot = {version: Version, identity: Envelope, state: Envelope, root: Hash, chain: Hash};
law 3 read-state: "one consistent snapshot; ...";
operation ReadState(store: Store) -> Outcome Snapshot;
footprint ReadState: capability ReadStore, reads {durable, volatile}, writes {volatile}, laws [3], v2 calls [V2SqliteShell::snapshot];
```

In English the last two read: "Operation ReadState takes store (Store) and
answers Outcome of Snapshot." and "Operation ReadState needs capability
ReadStore, reads durable and volatile, writes volatile, obeys law 3, and maps to
the V2 call V2SqliteShell::snapshot."

- **Resources** carry the one-bit values the record's footprint bytes use.
  **Capabilities** state the largest footprint each allows; a store capability's
  ports must map to V2 calls.
- **Answers** are result variants whose one case `T` carries the payload.
  **Frames** say what each case means for the effect: `applied`, `unchanged` or
  `either`.
- `Store` is built in: a 32-byte store identity and a 32-bit handle number,
  2^288 values. Its values cannot be written in a source.

`check` runs seven named checks, in order, and reports the first failure:

| Check | Fails when |
| --- | --- |
| `record_schema` | the schema is not `zeno-fcis/shell-effects/1` |
| `port_coverage` | a declared port lacks its operation or footprint, or an operation names no declared port |
| `frames` | a frame misses a case of its answer, names a case twice, or names one the answer lacks |
| `footprints` | an operation reads or writes beyond its capability |
| `laws` | an operation names no law, a law twice or a law that does not exist, or a law is never named |
| `v2_mapping` | a store port does not map to V2 calls, or a list of calls is empty |
| `record_kinds` | a type has no record kind: a Boolean, signed integers, exact bytes other than 32, or nesting deeper than 8 |

`lower` emits the record's canonical bytes exactly as `registry::canonical_bytes`
frames them; it refuses a source that fails any check. Frames, type names and
the signature name are part of the source revision, not of the record.

## Typed machines

A typed machine is a profile 2 machine with typed declarations:

```text
type Sent = sent | not_sent | unknown;
input answer: Sent;
var acknowledged: bool initially false;
rule acknowledge: awaiting + reply [answer == sent] -> accept delivered, acknowledged = true;
```

In English: "The input answer holds Sent." and "Rule acknowledge: when the state
is awaiting and the event is reply, if answer equals sent, move to delivered with
acknowledged set to true."

The source elaborates exactly to a profile 2 machine, which the unchanged
profile 2 checker decides. A name of an integer type keeps its range. A name of
any other type holds the index of its value in the type's enumeration, so
`answer == sent` becomes `answer == 0`. Check reports and explanations write the
values back: a counterexample step shows `lock: locked(2)`, not `lock: 2`.

- The type's enumeration must fit profile 2's range of 64 values; above it the
  source is refused by name: `Type Lock of lock has 65 values; a profile 2
  variable or input admits at most 64`.
- A typed name is compared only with `==` or `!=`, on the left, against one of
  its type's values or a name of the same type. It has no ordering and no
  arithmetic, and an update only assigns such a value.
- Values are written canonically, and a case name may not also name a fact, an
  input or a variable.

`examples/relay_typed.zal` is a delivery relay over the transport's three-case
answer: it acknowledges only on `sent` and gives up after two tries.

## Help and explanation

`run.py guide FILE --topic TOPIC` answers from one construct registry in the
`zal/help/2` schema. A topic is a construct (`record`, `variant`, `frame`,
`footprint`, `typed_var`) or a declared object (`port:Commit`, `type:Mode`,
`law:1`, `capability:Clock`). `run.py explain FILE --topic port:NAME` or
`type:NAME` explains one port or type: the forms, the counts and how they are
formed, the frame of each answer case, the capability against the footprint,
the laws and the V2 mapping. Typed machines take `--input` with typed values.
Help and explanations are computed from the checked model; no language model is
involved.

## Evidence and scope

- **Byte-for-byte lowering is tested, not proved.** `test_effects.py` compares
  the lowered bytes with the record that a small Rust example prints from the
  shell library built from the same tree. It also compares the port, resource,
  capability, answer, refusal-code and fault names and numbers, and capability
  allowances, which the record bytes do not carry. The printer reports the
  SHA-256 of `registry.rs`, so a stale build is refused.
- **Any drift fails a test.** Sixteen single edits of the source are each caught
  by a check, by the bytes or by the declarations; a change to the Rust registry
  fails the same comparison.
- **Laws are named, not checked.** The checks confirm that laws exist and are
  used; whether a handler satisfies a law is separate work. Frames are checked
  for exhaustiveness only.
- **Typed verdicts are profile 2 verdicts** over the elaborated machine: finite
  checked over the declared domain, with the same scope and limits.
- **Planted defects:** `check_effects.py planted` runs nineteen controls,
  including a missing variant case, a duplicate case tag, a product counted as a
  sum, a footprint beyond its capability, an unknown law ID, a port missing from
  the source and one changed byte cap. Each must fail its named test, and the
  unmutated copy must pass.

## Commands

```sh
python3 integrations/zal/check_effects.py native-build
python3 integrations/zal/run.py check integrations/zal/examples/shell_effects.zal
python3 integrations/zal/run.py lower integrations/zal/examples/shell_effects.zal
python3 integrations/zal/run.py render --to english integrations/zal/examples/shell_effects.zal
python3 integrations/zal/run.py explain integrations/zal/examples/shell_effects.zal --topic type:Mode --human
python3 integrations/zal/run.py guide integrations/zal/examples/shell_effects.zal --topic frame --human
python3 integrations/zal/run.py check integrations/zal/examples/relay_typed.zal
python3 integrations/zal/check_effects.py planted
```

The review terminal, MCP tools and browser workspace do not take these
profiles yet.
