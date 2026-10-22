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
must have every entry absent and every summary 0.

## Why the check covers every size

The claim: given the premises below, a checked contract keeps, on every run
from genesis, for every number of entries up to each map's capacity and for
any key values: every decision is lawful, aliased writes agree, every
summary equals its aggregate over the whole map, and every state law holds
at every choice of keys. The argument follows Wolper's data independence
(POPL 1986): a program that only compares data for equality behaves alike
on all data with the same equality pattern.

| Premise | Enforced by |
|---|---|
| P1. Keys are compared only for equality, with keys of the same type, and copied only into stored key fields. | The parser refuses any other key use in a law; the checker refuses it in a rule. |
| P2. A decision reads only scalar state, named entries, and command and context scalars and key classes. | Structure: a map has no view field, and the parser refuses any formula that reads one. |
| P3. Each conjunct of a state law reads one named entry or only scalar state; aggregates are summaries. | The parser refuses other state laws and any aggregate over a map's entries. |
| P4. The runtime frames each decision from the stored entries at the named keys and writes back only the merged writes of named entries. | The runtime, outside this check. |

1. *Abstraction.* For a real state and input, build the view: scalars as
   they are, each named entry from the map at its key or absent with
   defaults, and each key position's class by first occurrence. By P2 the
   Authority decides from the view alone; by P1 renaming keys bijectively
   leaves the view, and so the decision, unchanged.
2. *Coverage.* Every reachable real state and input yields an admissible
   view: conditions 1 to 3 hold by construction, 4 holds because the summary
   equals its aggregate (step 4 below), and 5 holds by step 3.
3. *State laws.* Genesis has every map empty, so every view at every choice
   of keys has all entries absent; generation checks the state laws there.
   A step changes only the entries its decision writes, which the Authority
   checks on the post-view, and P3 makes every other conjunct depend only on
   one unwritten entry or on scalars the Authority also checked. So each
   state law holds at every choice of keys after every step.
4. *Summaries.* At genesis every summary is 0 and every map is empty. A
   step leaves unnamed entries unchanged, by P4, and the checker found the
   summary change equal to the change of the distinct named entries on every
   admissible view, so the summary keeps equalling its aggregate.
5. *Obligations.* By coverage, every step of every run is one of the
   checked admissible views, so its decision is lawful and its writes merge.

The check does not establish P4. The runtime must establish it with its own
tests and planted controls. Capacity is the runtime's: an insert into
a full map is refused there, outside the view. A Lean statement of step 1 is
future work.

## Difference

A fixed table, `table ID OWNER name TYPE size N`, has N fields, and every
check enumerates all of them; it suits up to a handful of entries. A map has
no size in the view: a check enumerates only the entries a command names,
their equality patterns and the scalars, so its cost does not grow with the
map. The existing judges, `contract review`, `lemma` and `scenario`, decide
the view contract too, but they treat the view's entry fields as ordinary
state: their state counts and reachability are about views, not maps.

## Scope

- A check enumerates scalars, summaries and each named entry's record
  values over their declared ranges. A summary's range bounds the aggregate
  over the whole map; a large range enlarges the view domain, and symbolic
  treatment of summaries is future work.
- State laws relate one entry, or scalars. A law over two entries, such as
  conservation between a sender and a receiver, is written as a step law
  (`on commit`) or through a summary.
- Rules may not read an unnamed entry, iterate a map, or order keys. Range
  queries, iteration and ordered keys are future work.
- Cross-entry invariants that relate two maps, such as a member's loan
  count and the copies that member holds, are not expressible; rules must
  then defend against every admissible view, as the library's return rule
  does.
- Generated applications (`new --contract`, `--app`) do not yet run a
  collections contract: their HTTP preview, command wire and interface carry
  scalars only. The runtime below is the library such an application would
  use; wiring it into the generated shell, a key picker and a list idiom in
  the interface are follow-ups.

## Runtime

`zeno_fcis_shell_sqlite::collections::CollectionStore` keeps one SQLite row
per entry and, per map, the hashes of the binary radix tree of its sorted
32-byte keys: `EMPTY` for an empty subtree, the leaf hash of the only key
for a subtree with one key, and `NODE` over the two halves for a subtree
with two or more, split at the next key bit. The root depends only on the
entries. Writing a key rehashes only the subtrees on its path that hold
another key, as many levels as the bits it shares with its nearest
neighbour: about log2 of the entries for unrelated keys, at most 256.
`rebuild_root` recomputes a root from scratch for replay checks.

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
