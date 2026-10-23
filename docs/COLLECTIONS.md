# Keyed collections

## Journey

The ask: "accounts hold token balances; anyone transfers between accounts,
the issuer mints, and the total supply never changes except by minting."
There may be millions of accounts, each identified by an opaque 32-byte ID.
A contract cannot hold a million fields, and a check cannot enumerate a
million accounts. A map says what is meant:

```text
type 103 key AccountId;
type 104 data Account;
field 140 104 balance 105;
map 110 100 accounts 103 104 capacity 1048576;
summary 120 100 supply 106 sum 110.140;
field 131 101 from 103;
field 132 101 to 103;
entry 160 sender of 110 at command.101.131;
entry 170 receiver of 110 at command.101.132;
```

The [token balances example](../examples/collections/token-balances/README.md)
is this contract in full; the [library](../examples/collections/library/README.md)
and [escrow](../examples/collections/escrow/README.md) examples hold members,
copies and deals the same way. Check it:

```text
zeno-fcis contract collections examples/collections/token-balances
zeno-fcis check examples/collections/token-balances/project.zeno --expand
```

`contract collections` exits 0 when every admissible view, explained below,
decides lawfully and keeps every obligation, 1 with a counterexample, and 2
when the view domain exceeds `--max-tuples`. For the token example it
enumerates 28,672 view tuples, of which 2,304 are admissible. A transfer to
oneself that credits the account is refuted on the view where both keys are
the same account: the supply stays put while the balance grows. `--expand`
prints the view: the project with each collection declaration replaced by
the ordinary fields every other check sees.

## Formal model

### Declarations

- `type ID key Name;` is an opaque identifier of 32 bytes. Keys are compared
  only with `==` and `!=`, and only with keys of the same type; there is no
  order, arithmetic or constant key. A key field lives only in the command,
  the context or a map's record.
- `map ID OWNER name KEY VALUE capacity N;` is a map in the state record
  OWNER, from keys of type KEY to records of type VALUE, with at most N
  entries at run time. VALUE's fields are ranged integers, booleans,
  payload-free enumerations or keys. N is never enumerated.
- `entry ID name of MAP at ROOT.TYPE.FIELD;` names the entry of MAP at the
  key in that command or context field. A decision reads and writes only
  scalar state and named entries; nothing else of a map has a view.
- `summary ID OWNER name TYPE sum MAP.FIELD;` and
  `summary ID OWNER name TYPE count MAP;` declare a state field holding the
  sum of a field over every entry, or the number of entries. An aggregate
  over a map is written only this way; `count` or `sum` over a map's entries
  is refused.

### The view

Every collection declaration lowers, while the project is parsed, to
ordinary declarations; `--expand` prints them.

- A key type becomes `int 0..=n-1`, where n counts its key positions: its
  command and context fields, then the key fields of named entries. Each
  position holds a key class.
- A map becomes its presence type, `type MAP int name_present in 0..=1;`.
- Entry E of a map whose record has fields f1 < f2 < ... becomes the state
  field E, 1 when the entry is present and 0 when it is absent, and the
  field E + j for fj. Laws may write `ROOT.OWNER.E.fj` for field E + j, and
  `forall e in entries MAP [where C] { F }` for the conjunction of F over the
  named entries of MAP, joined as the table forms are.
- A summary is an ordinary state field.

The rules in `v2/policy.json` read and write the view fields by their IDs,
and may write a key into a stored key field, which copies its class.

### Admissible views

The checker enumerates every view tuple and keeps the admissible ones:

1. key classes form a restricted-growth string per key type, in input
   order, so each equality pattern among the keys appears exactly once; the
   number of patterns is the Bell number of the key positions;
2. an absent entry holds the least value of each field;
3. entries of one map whose keys share a class are one entry, so they agree;
4. a summary over nonnegative amounts is at least the total of its distinct
   present named entries;
5. every state law holds on the pre-view.

For each admissible view the library Authority decides, and the checker
requires that it does not refuse; that the entries naming one key write at
most one new value (alias merge); and that each summary changes by exactly
the change of its distinct named entries (summary delta). The genesis state
must have every entry absent, its fields at their declared defaults, and every
summary 0. Every genesis-applicable law, including an `InitialCondition`, is
checked there. Initial conditions are not assumed on later pre-views.
Summary arithmetic is checked: an overflowing nonnegative named sum cannot
fit a representable whole-map summary and is inadmissible. A summary delta
outside the checked integer domain is refused, including signed deltas.
Every stored key in a present successor entry must resolve to a concrete key
bound by a command/context field or a present pre-entry's stored key of that
type. An absent field's default class alone supplies no concrete key. The
checker refuses an unbound post-key with `stored key provenance`, matching
the runtime's inability to resolve that class into a stored 32-byte key.

## Why the check covers every size

The claim: given the premises below, a checked contract keeps, on every run
from genesis, for every number of entries up to each map's capacity and for
any key values: every decision is lawful, aliased writes agree, every
summary equals its aggregate over the whole map, and every state law holds
at every choice of keys. The argument follows Wolper's data independence
(POPL 1986): a program that only compares data for equality behaves alike
on all data with the same equality pattern.

| Premise | Enforcement or proof obligation |
|---|---|
| P1. Keys are compared only for equality, with keys of the same type, and copied only into stored key fields. | The parser refuses any other key use in a law; the checker refuses it in a rule. |
| P2. A decision reads only scalar state, named entries, and command and context scalars and key classes. | Structure: a map has no view field, and the parser refuses any formula that reads one. |
| P3. Entry invariants use the same scalar-independent predicate at every entry, including absence, independently of command/context, and every named post-entry satisfies it. Scalar invariants read only scalars; aggregates are summaries. | Parser checks read locality, input independence and uniform entry predicates; `forall e in entries MAP` supplies the uniform form. The Lean model makes uniform closure explicit in `Checked`; it does not prove the executable parser establishes that premise. |
| P4. The runtime binds the declared capacities, frames stored named entries, and atomically commits only their merged writes within those capacities. | The runtime, outside this check. |

1. *Abstraction.* For a real state and input, build the view: scalars as
   they are, each named entry from the map at its key or absent with
   defaults, and each key position's class by first occurrence. By P2 the
   Authority decides from the view alone; by P1 renaming keys bijectively
   leaves the view, and so the decision, unchanged.
2. *Coverage.* Every reachable real state and input yields an admissible
   view: conditions 1 to 3 hold by construction, 4 holds because the summary
   equals its aggregate (step 4 below), and 5 holds by step 3.
3. *State laws.* Genesis has every map empty, so every view at every choice
   of keys has all entries absent with canonical defaults; the checker evaluates
   every genesis-applicable law there, including the state laws.
   A step changes only the entries its decision writes, which the Authority
   checks on the post-view, and P3 makes every other conjunct depend only on
   one unwritten entry or on scalars the Authority also checked. So each
   state law holds at every choice of keys after every step.
   The uniform form is what makes this step sound. A record written under one
   entry role can be named under another role by the next command, so a state
   law about only one role, such as `post.100.160.140 == 0` beside a second
   entry of the same map, would hold on every checked view and still fail on
   the next one. The parser refuses such a law (`law.order`); write it as
   `forall e in entries 110 { post.100.e.140 == 0 }`, which the checker then
   enforces on every named entry after every step.
4. *Summaries.* At genesis every summary is 0 and every map is empty. A
   step leaves unnamed entries unchanged, by P4, and the checker found the
   summary change equal to the change of the distinct named entries on every
   admissible view, so the summary keeps equalling its aggregate.
5. *Obligations.* By coverage, every step of every run is one of the
   checked admissible views, so its decision is lawful and its writes merge.

The check does not establish P4. The runtime must establish it with its own
tests and planted controls. Before contract execution, bind the empty store
with `CollectionStore::bind`, passing the declared `Layout::capacities()`;
the emitted applications do this at startup. An unbound `CollectionStore`
is a schema-free storage utility and has no declared-capacity guarantee.

Capacity is checked on the complete atomic write set. For a map with count c,
count each distinct written key once and let delta be its new presence minus
its old presence, summed over those keys. Commit requires 0 <= c + delta <= N.
Thus inserting a new key into a full map is refused; updating an existing key
is allowed, and replacing a deleted entry in the same commit is allowed in
either write order. A refusal changes no entries, scalars, roots, counters,
retry records or journal rows.

Capacity is outside the view: the Authority can accept a lawful view while
the bound store refuses its commit for capacity. A checked view result does
not guarantee storage admission. No capacity decision is re-derived from
business rules in the shell.

The scoped renaming part of step 1 is proved by `frame_renaming` and
`decision_renaming` in
[Collections.lean](../verification/lean/Collections.lean): equal scalar and
named-record observations, together with the same key equality pattern, give
equal views and decisions, even across key types. The proof does not establish
the parser or runtime encoding, canonical enumeration of equality patterns,
stored key-field translation, summary identities, capacity refusals, or
serialization and ownership; see the [proof scope](../verification/lean/README.md).

The model's `run_preserves` additionally requires a uniform, scalar-independent
entry predicate at every stored entry and every named post-entry, including
absence, plus lawful empty genesis and merged-write framing. Read locality
alone is insufficient: `role_only_counterexample` preserves a law at role `a`,
writes an unlawful value at distinct role `b`, and violates the law when that
key is later selected as `a`. The model makes this closure an explicit
`Checked` premise; it does not prove that the executable checker enforces it.

Both equality-only and ordered projects enforce P3 and recheck declared laws
on the alias-merged successor. Checking only the raw decision would miss a
law whose read-only alias changes when another alias writes the same key.

### Grouped invariants across maps

The library declares that each member's loan count equals the number of
on-loan copies with that member as their holder:

```text
tally 510 loans_match_copies 110.140 = count 115 by 146 where 145;
```

The general forms are `tally ID NAME TARGET.FIELD = count SOURCE by KEY;`
and `... = sum SOURCE.WEIGHT by KEY;`, each optionally followed by
`where SELECTOR` before the semicolon. These are invariant declarations.
For every target key k:

`target_amount(k) = sum(weight(r) for r in SOURCE if r.KEY == k and selected(r))`.

An absent target contributes zero; an absent source or selector zero contributes
zero; count uses weight one. The source grouping field has the target key type;
the source map itself may have a different key type, as in members and copies.
The checker assumes the necessary precondition that each named target amount
covers all distinct named source contributions for that key. It requires equal
deltas on both sides for every affected key class, including old and new stored
group keys that no target entry names. Aliases contribute once.

| Premise | Enforcement or remaining obligation |
|---|---|
| T1. Two distinct maps; fixed target amount field and stored source grouping key of the target key type. | Parser checks identities; full project format 7 binds all tally fields. Projects without tallies retain format 5 or 6. |
| T2. Target and optional weights are nonnegative ranged integers; optional selector is an integer in 0..=1. | Parser refuses negative weights, key weights and other selectors. |
| T3. Maps start empty; a step changes only merged named entries. | Empty genesis is checked; actual framing and publication remain P4. |
| T4. For every key, target delta equals grouped source delta, using old and new records. | Checker compares all affected classes with checked integer arithmetic; an unrepresented target has delta zero. |

At genesis both sides are zero. Only keys of changed targets and old/new group
keys of changed source entries can change either side. T4 checks each of these;
every other key has zero delta by T3. Induction establishes equality at every key
for every map size within capacity. This is exact preservation over the checked
view domain. The abstraction may include unreachable or unrealizable views, so
a refutation need not describe a reachable run. No maximal decidable fragment
is claimed. Arbitrary joins, products, negative weights and relational state
laws across entries remain refused with a diagnostic naming the fragment.

An on-loan copy held by the named member contributes one, so that member's count
is positive. The library return rule can omit its extra zero-count guard. The
holder check, range checks, rejection precedence and per-key delta check remain
necessary. Decisions on runs from empty genesis are preserved; behavior on
inconsistent stored maps intentionally differs. Tally declarations are static
checking obligations, not a new invocation or publication authority.

## Difference

A fixed table, `table ID OWNER name TYPE size N`, has N fields, and every
check enumerates all of them; it suits up to a handful of entries. A map has
no size in the view: a check enumerates only the entries a command names,
their equality patterns and the scalars, so its cost does not grow with the
map. `contract review` still examines the ordinary lowered view. The lemma,
scenario and profile judges use map semantics as described below.

## Map-aware judges

```text
zeno-fcis contract lemma examples/collections/library --lemma 'forall m in entries 110 { post.100.m.140 <= 2 }'
zeno-fcis contract scenario examples/collections/token-balances
zeno-fcis contract profile examples/collections/library --map-keys 2
```

A map lemma is a uniform per-entry predicate or a scalar-state predicate. The
base collections contract must check first. The judge checks empty genesis and
consecution with that predicate as an additional invariant, using the same
admissibility, alias merge, summary and tally obligations. `inductive` has the
all-size claim conditional on P1–P4 and any T/O premises; `not-inductive` has an
abstract counterexample, which need not be reachable. It does not report a
reachable-state count or a shortest concrete run. A role-only predicate such as
`post.100.161 == 0` is refused when another entry of that map has no matching
predicate. Use `forall` to ask about all members. `--transition` checks a relation
on all admitted views without assuming it as a precondition.

A map scenario is JSON with schema `zeno-fcis/collection-scenarios/1` and a
`scenarios` array. Each item has a unique `name` and nonempty `steps`. A step has
`command`, `context` and `expect`. Inputs map decimal field IDs to numbers or
keys; field `0` denotes a scalar root. Every declared input must be supplied,
with no extra fields. Keys are exactly 64 lowercase hexadecimal digits, and a
key keeps its identity when later commands name it in another role. Every run
starts at empty-map genesis. The token example's
[scenario file](../examples/collections/token-balances/tests/decision-scenarios.txt)
mints to one key, then uses it as sender and receiver in subsequent transfers.

`expect` names a `class` (`Accept`, `Reject`, `CommittedFailure` or `Refused`), an
optional `reason`, optional `scalars`, and optional `entries`. Each expected
entry is `{ "map": ID, "key": HEX, "record": { FIELD: VALUE, ... } }`, or has
`record: null` for absence. A supplied record must name every record field.
Integers also accept decimal strings to preserve full-width values. The result
contains each concrete successor snapshot and the first mismatching step.
Refusals retain state. Map scenarios report no scalar-view mutant score.
Limits are 1 MiB of input, 128 scenarios, 4,096 total steps and 8 MiB of output;
exceeding a limit refuses the request without a passing result.

A map profile exhaustively explores concrete maps over the printed finite key
universes, starting empty. `--map-keys N` chooses 1 through 4 keys of each opaque
key type, default 2. All declared command/context scalar values are considered.
The result's reachable count is about those concrete maps, including stored
keys and scalars, and its digest binds the complete reachable set. For example,
two-key token balances have 25 reachable maps; two keys of each library key
type give 66 reachable maps. This is not an all-size reachability count, a
Moore-minimality result, or a count of lowered view states. Alias writes merge,
post-laws are checked on the canonical merged view, invariants are checked on
concrete maps, and capacity refusal applies atomically to the complete update.

`--max-tuples` bounds profile transition work, capped at 4,194,304. Input storage
is bounded before enumeration by a 32 MiB estimate; at most 65,536 snapshots
and a 64 MiB snapshot-storage estimate are retained. An incomplete result has
no complete reachable count or claim. Ordered map scenarios and profiles are
refused until their concrete framing is supported; ordered map lemmas can use
the reduction above. Neither a profile nor a scenario grants runtime authority.

## Background

Peter Wolper, “Expressing Interesting Properties of Programs in Propositional
Temporal Logic,” ACM Symposium on Principles of Programming Languages, 1986,
is the data-independence background for equality-only keys. Ralph C. Merkle,
“Protocols for Public Key Cryptosystems,” IEEE Symposium on Security and
Privacy, 1980, is background for hash-tree authentication. These references
do not establish this reduction's premises or this store's implementation.

## What is new here

This release connects named-entry views, checked summary deltas and an
incremental keyed-store journal. The comparative assessment of that
combination is **prior-art search pending**.

## Scope

- A check enumerates scalars, summaries and each named entry's record
  values over their declared ranges. A summary's range bounds the aggregate
  over the whole map; a large range enlarges the view domain, and symbolic
  treatment of summaries is future work.
- State laws relate one entry, or scalars. A law over two entries, such as
  conservation between a sender and a receiver, is written as a step law
  (`on commit`) or through a summary.
- Rules may not read an unnamed entry or iterate a map. Ordered key types
  and at most four fixed order reads are supported by the collections
  checker as described below; opaque keys remain equality-only.
- Cross-map invariants use the grouped tally fragment above; arbitrary
  cross-entry state formulas remain unsupported.
- `new --contract` and `--app` run standalone collection contracts without
  deliveries or a server clock. Their plain synthesized form and interface
  retain command and context key fields as exactly 64 lowercase hexadecimal
  digits. Ordered collection framing, a key picker, collection listing and durable browser request
  recovery are follow-ups. This collection profile is separate from the
  scalar publication shell.

## Application

```text
zeno-fcis new token-app --contract examples/collections/token-balances
zeno-fcis new library-app --contract examples/collections/library --app
```

Each application includes an authenticated HTTP server and a plain form.
Review its `context.json`, provide the private credential file as its README
explains, and run `cargo +1.97.1 run --locked --offline --bin app-server`.
`GET /state` reports scalar state and the command/context interface;
`POST /preview` and `/command` use the existing ingress envelope with a JSON
array of command strings. Malformed keys receive the existing HTTP 400
`Malformed` refusal. Context keys come from server configuration.

For each request the shell loads each distinct named key once, snapshots the
owned records, frames them with `Layout::frame`, calls the checked Authority,
and stores only `Layout::writes`. Revision, retry and declared capacity guards
run in the same SQLite transaction as the scalar state and map writes. These
storage utilities do not supply decision or publication authority. Before
publication the runtime reconstructs the actual alias-merged successor in the
invocation's existing key classes and checks every declared contract law again.
Deleted entries use the declared fields' least values, exactly as an absent
entry does on the next invocation. A raw decision that passes its laws but
whose canonical stored successor fails one is refused without publication.
Map-wide
reads belong to audit and measurement setup, outside request preparation.

Each application receipt binds scalar state and every named map root before
and after. Its submission journal records the real keys, context, exact
command bytes and Authority publication subject. Local `app-server --journal
STORE`, `--replay JOURNAL NEW_STORE` and `--audit STORE` operations reproduce
and compare each receipt and the final head. Restore publishes a new destination
only after replay; startup performs the same audit before serving. Receipt,
submission and journal profile identifiers end in `/2`; earlier collection-app
profiles require an explicit migration that this implementation does not supply.


## Runtime

`zeno_fcis_shell_sqlite::collections::CollectionStore` keeps one SQLite row
per entry and, per map, the hashes of the binary radix tree of its sorted
32-byte keys: `EMPTY` for an empty subtree, the leaf hash of the only key
for a subtree with one key, and `NODE` over the two halves for a subtree
with two or more, split at the next key bit. The root depends only on the
entries. Writing a key rehashes only the subtrees on its path that hold
another key, as many levels as the bits it shares with its nearest
neighbour: about log2 of the entries for unrelated keys, at most 256.
The hashing, prefix arithmetic and path update live in the pure
`zeno_fcis_codec::collection_merkle` module. Storage acquires bounded
sibling summaries from indexed rows, applies the owned update plan in its
transaction and retains no invocation scratch. The plan authenticates no
storage and grants no decision or publication authority. `rebuild_root`
recomputes a root from scratch for replay checks.

Opening a store rebuilds every leaf and branch hash from stored records in one
read snapshot. A changed, missing or surplus cache row refuses startup. The
full application audit retains independent journal replay and root/record
comparison. Before each write, singleton leaf hashes are recomputed and checked
against their records, and the old target path is reconstructed from the actual
record and sibling summaries. It must match the checked pre-state root before
the new update plan is applied in the same transaction. The storage handle keeps
one verified root per map, initialized by the complete startup audit. A cached
root must also match that anchor, so coordinated changes to a root and sibling
cache cannot replace the checked state. The next anchors are prepared before
commit and installed only after SQLite commits; refusal and retry retain the old
ones. Both path folds have at most 256 levels; the full cache audit is outside
request preparation.
The returned commit's `rehashed` observation counts update-plan subtree rows; it excludes
this additional verification work. These storage checks do not authorize a
business decision or authenticate an externally rewritten store and history.

The pure-core tests cover all 256 subsets of eight keys sharing 253 prefix
bits. For subsets of up to five keys they check every insertion order and
every removal order, comparing each intermediate root and stored node cache
with the canonical subset. Independent SHA-256 vectors retain the existing
root format. This finite corpus is regression evidence, not a general proof.

`zeno_fcis_contract::collections::runtime::Layout` frames each decision:
`frame` reads storage only at the keys the command and context name,
places absent entries at their defaults and assigns each key position its
class by first occurrence; `writes` turns the committed post-view into the
scalar state and, per named key, the one new record its entries agree on,
or a removal, with stored key classes mapped back to keys.
`CollectionStore::commit` refuses any write at a key the decision did not
name, writes the entries in one transaction, and appends a journal row that
chains the decision, the scalar state and every named map's root before and
after. Replaying the same commands into a fresh store reproduces the journal
exactly.

The runtime tests run the token example for 1,500 commands over 400
account keys, and the library for 2,100 commands over 60 members and 40
copies, through the library Authority on each framed view. After every
committed decision the supply equals the sum of every stored balance, each
member's loan count equals the copies it holds, every root equals its
rebuild, and no decision reads more than its two named entries.

## Ordered keys and bounded reads

An ordered key is an integer from declared inclusive bounds, observed only
through same-type comparisons and copying. Its numeric value, distance,
arithmetic, constants and casts are unavailable to decisions. Prices and
times can use this order-only interface:

```text
type 103 ordered Price in 0..=1000000;
type 106 int Found in 0..=1;
field 132 102 best_price 103;
field 133 102 best_found 106;
order 180 best_bid of 110 max into context.102.132 found context.102.133;
entry 170 best of 110 at context.102.132;
```

`order ID NAME of MAP QUERY into KEY found FLAG;` observes the pre-state
map. KEY is a context field of its ordered key type and FLAG is a distinct
context integer field with range `0..=1`. There are at most four reads in a
project, so every command has a fixed bound. Read IDs and output fields are
unique. Endpoints are command or context key fields of the same type;
query outputs cannot also be endpoints. Queries are:

| QUERY | Result when found | When not found |
|---|---|---|
| `min` | Least key | The map is empty |
| `max` | Greatest key | The map is empty |
| `successor PATH` | Least key strictly greater than PATH | No key is greater than PATH |
| `predecessor PATH` | Greatest key strictly less than PATH | No key is less than PATH |
| `range LOWER to UPPER` | A key in the closed interval | No key is in that interval |

A reversed range is empty. When FLAG is 0, KEY is an arbitrary key of its
type and carries no membership fact. Guard uses of a result with FLAG. A
range result is an existence witness: the checker considers every matching
key, while the pure `ordered_key` helper chooses the least matching one.
There is no map iterator, dynamic query loop, key literal or arithmetic.

The [tiny order book](../examples/collections/order-book/README.md) uses a
maximum read and checks that every fill uses an existing price at least as
good as the best bid. The ordinary finite view is still available through
`--expand`; its bare scalar checks do not establish map order facts. Use
`contract collections` for that conditional claim.

### Why the ordered check covers every size

This argument extends the view reduction itself; equality-only renaming
is insufficient for order. Its claim is conditional on P1–P4 above, with P1
extended to comparisons of ordered keys, and the following premises:

| Premise | Enforcement or remaining obligation |
|---|---|
| O1. Each ordered key domain is a fixed integer interval with its total order. Decisions observe only equality/order of same-type keys and copy keys. | The parser requires inclusive bounds, rejects mixed types, arithmetic, constants and casts in laws; the collections checker applies the same restrictions to policy expressions and stored-key writes. Concrete input bounds are a framing obligation. |
| O2. All reads are a fixed, simultaneous observation of one immutable pre-state map snapshot. | The parser accepts at most four reads, distinct context outputs, same-type input endpoints and no output-to-endpoint dependencies. The runtime must obtain these observations from that snapshot. |
| O3. A successful read names a present key. Every extremal/neighbour read and every failed range read supplies its empty interval. | The checker constructs present/absent point sets and explicit open/closed empty intervals, and refuses inconsistent views. |
| O4. Entry-local state predicates apply uniformly across each map's named entries, and do not depend on invocation observations. | Existing `law.entries` and summary checks remain. Collection projects reject input-dependent state invariants and require structurally identical predicates, modulo entry field IDs, on every named entry of that map. `forall e in entries MAP` supplies this form. |
| O5. Only named-entry writes are merged and published; query facts are pre-state observations, never retained state. | This is a runtime obligation. The current `Layout` refuses ordered projects rather than frame unsupported observations. The pure query helper supplies no decision or publication authority. |

1. *Abstraction.* Collect all command/context keys, all read-result keys
   (including arbitrary placeholders for failed reads), and stored keys of
   present named entries, separately for each key type. Replace ordered keys
   by their ranks among the distinct collected keys. Every comparison and
   copy has the same result; stored scalar values remain unchanged. O1 and
   the existing named-entry boundary make the decision depend on this view.
   A global order automorphism of a bounded integer interval is unnecessary:
   the argument preserves the finite observations of this invocation.
2. *Coverage by weak orderings.* For p collected positions, every ordered
   equality partition is represented by a contiguous assignment of ranks
   `0..b-1`, where b is the number of blocks. The checker enumerates all
   assignments and retains exactly the contiguous ones for active ordered
   positions. Thus both `a < b` and `b < a`, and every alias, occur. Opaque
   keys retain first-occurrence labels. Absent stored fields hold defaults
   and do not introduce active positions. Declared position counts bound
   these ranks; the existing tuple cap yields UNKNOWN before exploration
   when the declared domain is too large.
3. *Absence facts.* A found minimum r establishes `(-infinity,r)` empty;
   a maximum establishes `(r,+infinity)` empty. A found successor of a
   establishes `(a,r)` empty and `a < r`; a predecessor establishes `(r,a)`
   empty and `r < a`. A failed neighbour read establishes the corresponding
   open half-line empty. Failed min/max establish the whole map empty.
   A failed range establishes `[lo,hi]` empty; a successful range adds a
   present witness within it. The checker compares every known present
   point, including all other successful results, with these intervals and
   with named absent points. Endpoints and open/closed flags are explicit:
   adjacent ranks alone never mean that the real map has no key between.
4. *Unnamed keys.* Real observations satisfy these facts by O2–O3. Their
   intervals also constrain unnamed keys in the real map, even though these
   keys are not individually represented. The checker need only check
   consistency of observable points: all further observations have already
   supplied their witnesses or empty intervals. Additional unobserved keys
   outside empty intervals cannot change the current decision. The abstract
   views can overapproximate real maps: finite-domain cardinality, capacity
   and summary realizability are not completely characterized. This may
   produce conservative counterexamples; it never omits a real view.
5. *Induction.* Empty genesis and every reachable real pre-state map
   project into a checked admissible view, by steps 1–4 and the existing
   summary/state-law induction. The Authority checks step laws, and the
   checker retains alias-merge and summary-delta obligations, then checks
   all declared transition laws again on the merged successor, including
   its outbox. O4 establishes a uniform predicate for every stored entry,
   preserving state laws when unnamed entries remain unchanged; O5 frames
   the actual update. Query facts expire after the invocation and are
   recomputed for the next snapshot, so no post-state extremality is
   assumed after insertion or deletion. Each real decision is therefore
   covered regardless of map size, subject to these premises and capacity.

The integer bounds, ordered/read metadata and endpoints are bound by
project encoding format 6. Equality-only projects retain their format-5
bytes. This is a mathematical reduction argument with executable finite
checks, not a machine-checked proof of the reduction or an ordered-storage
implementation. The helper reads an immutable `BTreeSet<i128>` and returns one copied
integer, with no escaping iterator, custom comparator, cache or mutable alias.

### Lineage

**Background.** P. Wolper, *Expressing Interesting Properties of Programs in
Propositional Temporal Logic*, POPL, 1986, supplies the equality-only data
independence background. Candidate ordered-data lineage is R. Lazić and
D. Nowak, CONCUR 2000 (title and ordering theorem scope await verification),
and R. Lazić, *A Semantic Study of Data Independence with Applications to
Model Checking*, doctoral thesis, 1999 (bibliographic details await
verification). The ordered argument above stands on its stated premises;
these candidate references are not invoked as a theorem proving it.

**What is new here.** This integration combines bounded observations,
weak-order views, explicit absence intervals and grouped tally deltas. Prior-art search pending;
no novelty claim is made.

**Background for commitments.** R. C. Merkle, *A Digital Signature Based on
a Conventional Encryption Function*, CRYPTO 1987 (proceedings 1988), describes
hash-tree commitments. The cache audit and path verification here preserve
the stated binary-prefix tree representation; the citation does not establish
this implementation.

**Background for reachability.** E. M. Clarke, O. Grumberg and D. A. Peled,
*Model Checking*, MIT Press, 1999, describes finite-state reachability.

**Background for tally maintenance.** A. Gupta, I. S. Mumick and V. S. Subrahmanian,
*Maintaining Views Incrementally*, SIGMOD, 1993, studies incremental maintenance
of database views. The per-key delta identity here is stated and justified above;
this citation does not establish the implementation.

**Scope.** The checker exhaustively checks the declared view domains when
within its tuple limit. Independent small-map comparisons and planted
controls test the implementation; they do not establish the reduction for
all maps, storage framing, publication authority or release readiness.
