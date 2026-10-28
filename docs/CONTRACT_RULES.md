# Contract rules reference: `v2/policy.json`

An application contract is three files:

- `project.zeno` declares the types, fields, variants, reasons, channels
  and laws; the [`.zeno` language reference](ZENO_LANGUAGE_V1.md) gives its
  syntax;
- `v2/policy.json`, described here, holds the decision rules, the genesis
  state and the kind of every law;
- `tests/decision-examples.txt`, optional, lists decisions the owner
  expects.

An application built with `new --app` also needs `ui.json`, the labels its
interface shows; see [UI metadata](#ui-metadata-uijson). The
[library loans](../examples/apps/library-loans/README.md) and
[escrow](../examples/apps/escrow-deal/README.md) examples are complete
contracts with all four files.

`zeno-fcis generate contract` turns the first two into `v2/schema.zcve`,
`src/v2_contract.rs` and `v2/policy.zcve`, and `zeno-fcis new <dir>
--contract <contract-dir>` builds an application around them; see the
[CLI reference](CLI_REFERENCE.md). Generation stops at the first entry it
cannot use and names it, for example `v2/policy.json cases[3].post.111`.
Templates such as
[`durable-counter`](../crates/zeno-fcis-cli/templates/durable-counter/v2/policy.json)
and the [dual-approval example](../examples/dual-approval/v2/policy.json)
are complete rules files.

The file is JSON with integer numbers only. A repeated key, an unknown key,
a fraction or an exponent is refused, so no entry is ignored or silently
overridden. Object keys that are IDs (field, type, law) are decimal, with no
sign or leading zero.

## Top-level keys

<!-- policy-keys: file -->
| Key | Required | Value |
|---|---|---|
| `schema` | yes | `"zeno-fcis/template-declarative-policy/2"` for historical declarations, `/3` for declarations with units and scales, or `/4` for typed U128 contracts. |
| `template` | yes | The application's name. `new --contract` uses it as the Cargo package name, so there it must be at most 64 lowercase letters, digits and `-`, starting with a letter. |
| `roots` | yes | Always `{"state": 100, "command": 101, "context": 102}`; see [Roots](#roots). |
| `leaf_bindings` | yes | Type ID to leaf; see [Leaf bindings](#leaf-bindings). May be `{}`. |
| `variables` | yes | Name to expression; see [Variables](#variables). May be `{}`. |
| `cases` | yes | The ordered decision table; see [Cases](#cases). |
| `genesis` | yes | State field ID to its value in the genesis state, for every state field: a JSON boolean, an integer, or a variant ID for a sum field. |
| `law_kinds` | yes | Law ID to [law kind](#law-kinds), for exactly the laws `project.zeno` declares. |
| `delivery_laws` | no | Law ID to bounded channel counts, numeric or variant payload observations and destination filters; see [Delivery laws](#delivery-laws). |
| `framework_failure_law` | no | Nonzero ID of the [framework committed-failure law](#framework-laws); default 908. |
| `framework_reject_law` | no | Nonzero ID of the [framework reject law](#framework-laws); default 909. |
| `adoptions` | no | Written by `zeno-fcis contract adopt`; see [Adoptions](#adoptions). |
| `evolutions` | no | Written by `zeno-fcis contract evolve`; see [Evolutions](#evolutions). |
| `clock` | no | The context field the application server sets to the current time; see [Server clock](#server-clock). |
| `idempotency` | no | A note for reviewers, text. Generation ignores it. |
| `original_domain` | no | A note for reviewers, text. Generation ignores it. |

## Roots

Type 100 is the state, type 101 the command and type 102 the context, as
`project.zeno` declares them with `type 100 state ...`, `type 101 command
...` and `type 102 context ...`. The state must be a record; the command and
context may be records or single values.

<!-- policy-keys: roots -->
| Key | Value |
|---|---|
| `state` | 100 |
| `command` | 101 |
| `context` | 102 |

The decision program reads one input per field of each record root, or the
root itself when it is a single value, in this order: the state fields, the
command, the context. A contract has at most 64 inputs.

## Leaf bindings

Every type needs exactly one form: fields, variants, a declared integer range
(`type 105 int Money in 0..=1000;`), or a leaf binding here. So `bool` types,
`int` types without a range, and the text types of channel destinations need
a binding.

<!-- policy-keys: leaf-bindings -->
| Leaf | Meaning |
|---|---|
| `Bool` | `["Bool"]`: a boolean, 0 or 1 in examples. |
| `I128` | `["I128", min, max]`: an integer from `min` to `max`, with `min <= max`. Inputs and computed values must fit in 128 bits. |
| `Text` | `["Text", min, max]`: ASCII text of `min` to `max` bytes. Only destinations are text. |

A binding for an undeclared type is refused, and so is a binding that
changes a declared range; restating it is allowed.

## Variables

A variable names an expression, so that rules read like the plain-language
rules they encode:

```json
"variables": {
  "status": "pre.100.110",
  "ship_deadline": "pre.100.115 + 604800"
}
```

A variable may use other variables but not itself, directly or through
others. Its name may not start with `pre`, `post`, `command`, `context` or `outbox`.
Variables apply in `when`, `post` and payload expressions, which are
expanded before they are compiled.

## Expressions

A value in `when`, `post`, a payload or a variable is a JSON boolean, a JSON
integer, or expression text. Expression text has:

- integers (`604800`; write a negative number as `-5`), `true` and `false`;
- names: a variable, or an input: `pre.100.F` for state field `F`,
  `command.101.F` or `command.101`, and `context.102.F` or `context.102`;
- operators, from loosest to tightest binding, and parentheses:

<!-- policy-keys: operators -->
| Operator | Meaning |
|---|---|
| `->` | implication, `a -> b` is `!a \|\| b`; groups to the right |
| `\|\|` | or |
| `&&` | and |
| `==` | equal |
| `!=` | not equal |
| `<` | less than |
| `<=` | at most |
| `>` | greater than |
| `>=` | at least |
| `+` | add |
| `-` | subtract; also prefix negation |
| `*` | checked multiply of matching integer kinds in rules and laws |

  Prefix `!` negates a boolean. Other binary operators group to the left.

- functions:

<!-- policy-keys: functions -->
| Function | Meaning |
|---|---|
| `choose` | `choose(c, a, b)` is `a` when `c` holds, else `b`. |
| `u128` | `u128(0)` writes an unsigned decimal literal in 0..=2^128-1, with no sign or leading zero. |
| `to_i128` | `to_i128(a)` explicitly converts U128 to I128, refusing above 2^127-1. |
| `to_u128` | `to_u128(a)` explicitly converts I128 to U128, refusing negative values. |
| `div_floor` | `div_floor(a, b)`: mathematical division rounded down, including negative divisors. |
| `div_ceil` | `div_ceil(a, b)`: mathematical division rounded up, including negative divisors. |
| `mul_div_floor` | `mul_div_floor(a, b, d)`: explicit fused floor in rules, with nonnegative matching I128 or U128 factors, positive denominator, exact intermediate product and final result-kind fit. |
| `signed_mul_div_floor` | `signed_mul_div_floor(a, b, d)`: signed exact-product floor division with a positive denominator and a final i128 fit check. |
| `mul_div_ceil` | `mul_div_ceil(a, b, d)`: the same fused rule operation rounded up. |
| `products_ge` | `products_ge(a, b, c, d)` compares `a*b >= c*d`. Every factor is nonnegative; products are exact, with no intermediate narrowing. Rules and laws support this predicate. |
| `products_ge` | `products_ge(a, b, c, d, e, f)` compares `a*b*c >= d*e*f`, with the same nonnegative domain and exact products. |
| `mul_div_floor` | `mul_div_floor(a, b, d, e8)` specifies an explicit target raw-atom exponent, in e0..e38, rounded down once. Requires declared quantities. |
| `mul_div_ceil` | `mul_div_ceil(a, b, d, e8)` uses the same exact quantity expression, rounded up once, with nonnegative factors. |
| `signed_mul_div_floor` | `signed_mul_div_floor(a, b, d, e8)` permits signed factors, with a positive denominator and a checked i128 result. |
| `repeat_mul_div_floor` | `repeat_mul_div_floor(x, a, d, 2)` eagerly evaluates x, a and d once, then applies the nonnegative fused floor operation twice. The count must be a literal in 0..16. |
| `repeat_signed_mul_div_floor` | `repeat_signed_mul_div_floor(x, a, d, 2)` applies the signed floor operation at every prefix, with the same literal count limit. |

These arithmetic extensions describe development source whose native and
source-proof qualification is pending. Rules compile to the same library
program evaluated by generated applications and the Authority. Standalone
multiplication must fit i128. Division by zero and an out-of-range quotient
refuse, including `div_floor(-170141183460469231731687303715884105728, -1)`. The signed minimum is a
legal literal; its positive magnitude is not an i128 value. With a positive divisor, `div_floor(-1, 2)`
is -1 and `div_ceil(-1, 2)` is 0; negative divisors follow mathematical rounding.

`div_floor(a * b, d)` and `mul_div_floor(a, b, d)` are distinct graphs. The
first checks and can refuse the narrow multiplication; the second preserves
the exact product and narrows only the quotient. For `M = 2^127 - 1`, the fused
`mul_div_floor(M, M, M)` has result M while the standalone product refuses.
Nonnegative fused calls refuse negative factors. All fused calls refuse a
nonpositive denominator, including when the numerator is zero. A variant is its ID:
`status == 151`.
In historical untyped contracts, a boolean used as a number is 0 or 1, and a
number used as a boolean holds when it equals 1. Decision arithmetic is i128, and the program evaluates
every node, both branches of `choose` included, so an overflow anywhere
refuses the decision. A contract has at most 4,096 program nodes and 64
program outputs: the case selection and each distinct computed value. A graph
using an arithmetic extension has profile `zeno-fcis/finite-i128/3` within the
original 256-node, 16-output, 32-input caps, or `/4` beyond those caps. Old
operation-only graphs keep `/1` or `/2` and their canonical program bytes.
Program-file commands admit `/1` and `/3`; they refuse the larger `/2` and `/4`
profiles. Admission does not imply proposal support: `optimize` can admit `/3`
then exit 2 as `no-checked-improvement`, with `detail.search.stopped` equal to
`original-not-typable` and zero search phases. Loop check/replay can compare
these programs while a selected proposer or rule miner cannot type their nodes.
In particular, `loop rules` can qualify the unchanged original using only its
baseline check/replay, with no rule checks and `improved: false`. Migration
expressions and linear certificate consumers have narrower operation sets. See
[Contract program export](CLI_REFERENCE.md#contract-program-export).

Units and scales are development specification features, with source-proof
qualification pending. For example, `type 105 int Price in 0..=1000000 unit
USD/COLL scale e8;` declares raw atoms n with value n/10^8 USD/COLL. Products
and quotients of nominal identifiers are canonical; `unit 1 scale e4` is
dimensionless. Every integer type in a quantity contract must carry both
annotations. Addition, subtraction, comparison, select arms, assignments and
payload fields must agree in unit, scale and i128 width. A directly written
integer literal can denote raw atoms of its contextual destination; in a
product or quotient it is dimensionless e0. There is no implicit cast of an
already typed quantity, and quantities cannot be used as booleans.

For input exponents P, T, J and target Q, a four-argument fused call denotes
R(n*m*10^(Q+J)/(h*10^(P+T))), with h > 0 and the named floor or ceiling R.
The compiler cancels powers of ten before lowering to the existing fused
nodes. It refuses unsupported lowering if a remaining exponent exceeds 38
or a scaled operand's i128 fit does not follow from constants or checked
bounds. A delivery sum does not inherit one payload field's range bound.
Three-argument calls keep raw semantics and derive exponent P+T-J,
which must lie in 0..38. Nominal metadata is bound into the schema identity;
a raw program receipt alone supplies no typed equivalence claim. Existing
composition profiles and legacy schema lowering refuse quantity contracts.

In policy/3, and in any policy using quantity annotations, four-argument
scaled calls or repeat calls, a dependent mul-div chain has at most 16 steps, including across variable
bindings. Independent chains retain the total graph cap. Repeat operands
remain eager at count zero, which returns x without division. Integer powers
use `repeat_mul_div_floor(1, a, 1, 2)` with the desired literal count. Every
prefix must fit; the first failing node refuses even if a later zero or an
unused select arm would hide it. Usage follows the expanded graph and its
actual failing prefix. Staged rounding is observable: applying floor to
1,001 times 3/4 twice yields 750 then 562, whereas one final floor yields 563.
Those policies also bound source alias expansion to 65,536 visits and depth
256. Policy/3 may be used without quantity annotations to select these limits.
Historical policy/2 contracts using only historical syntax retain their
original admission, canonical bytes, eager traps and resource budgets.

Policy/4 supports `type 105 u128 Amount in 0..=340282366920938463463374607431768211455;`.
It also permits explicit opt-in for a signed-only contract. Strict kinds then
apply to every expression binding, including unused variables. The selected
profile is bound in the policy identity.
Bounds are mandatory. Genesis values use `"u128(0)"` expression strings.
Bool, I128 and U128 are distinct kinds: arithmetic, comparisons and select
arms require matching kinds; conversions are explicit. Unsigned add, subtract
and multiply check the full U128 range. Fused unsigned floor and ceiling use
an exact wide product and check the final quotient, including ceiling's
increment. Quantity rules above also apply to U128 with unsigned fit bounds;
casts preserve units and scales. Arithmetic traps refuse the whole decision,
including in unused eager branches. Laws and delivery sums retain full-width
unsigned values and refuse overflow. The schema uses its existing U128 kind
and canonical value tag. Prior policy/2 and policy/3 bytes are retained.

The supported typed path is contract build, new, review, gaps and lemma,
through the original library Authority and catalog. Review of a large domain
reports bounded probes rather than exhaustive coverage. U128 tables, scalar
program exports, emitters, symbolic/Tau exports, search, optimizer, loop
doors, composition, SQLite migration and kernel-law oracles refuse U128
subjects at binding. Source-proof qualification is deferred to the 2.5.1
freeze; native checks alone do not qualify these changes for release.

## Cases

`cases` is the decision table. The first case whose `when` holds decides,
and the last case's `when` must be written `true`, so some case always
decides.

<!-- policy-keys: case -->
| Key | Required | Value |
|---|---|---|
| `when` | yes | A boolean expression. |
| `class` | yes | `Accept`, `Reject` or `CommittedFailure`; see below. |
| `reason` | yes | `null` for `Accept`; for `Reject` and `CommittedFailure`, a reason `project.zeno` declares. |
| `post` | yes | State field ID to its value after the decision; a committing case may omit a field it leaves unchanged. |
| `outbox` | yes | The deliveries the decision makes; see [Deliveries](#deliveries). |
| `rule` | no | The rule's name for reviewers, text. Generation ignores it. |

<!-- policy-keys: classes -->
| Class | Commits | `post` and `outbox` |
|---|---|---|
| `Accept` | yes | `post` sets the state fields the decision changes; each omitted field keeps its value. |
| `Reject` | no | `post` is `{}` and `outbox` is `[]`: a reject changes nothing and delivers nothing. |
| `CommittedFailure` | yes | `post` sets the state fields the decision changes; each omitted field keeps its value. The contract needs a [`CommittedFailureEffects`](#law-kinds) law. |

A committing case that omits a state field keeps that field's value: an
omitted `"111"` reads as `"111": "pre.100.111"`, and generates the same
contract as writing it. A key that is not a state field is refused. A
constant must fit its field: a declared range, or a variant ID for a sum. A
computed value outside its field's range makes the library refuse that
decision when it is made.

Reasons: every reason `project.zeno` declares must be used by at least one
case, and every case that uses a reason has the same class, because a
reason's class comes from its cases. So removing the last case that uses a
reason also means deleting that `reason` from `project.zeno`; the refusal
names the reason and says so. A reason's `precedence` in `project.zeno` does
not change the case order.

## Deliveries

Each entry of a case's `outbox` is one delivery on a declared channel.

<!-- policy-keys: delivery -->
| Key | Required | Value |
|---|---|---|
| `ordinal` | yes | The delivery's position: a case's deliveries have increasing ordinals. |
| `channel` | yes | A channel `project.zeno` declares. |
| `destination` | yes | ASCII text within the bounds of the channel's destination type. It is a constant. |
| `payload` | yes | Field ID to value, for every field of the channel's payload record. Payload fields are booleans, integers or sums. |
| `idempotency_ordinal` | yes | A non-negative integer, at most 2^64 - 1, that the delivery carries as its idempotency value. It is a marker for the destination, not the delivery's identity: the store gives each delivery an ID bound to its commit. The generated channel admits the values 0 to the largest one any case uses on that channel. |

Every delivery charges one unit of the library's Effect meter, and the
generated Effect limit is the largest number of deliveries any case makes
(at least 1), so a case may deliver several times.

## Laws

`project.zeno` declares each law with a scope:

```text
law 500 funds_conserved on commit, genesis = post.100.111 + post.100.113 == post.100.112;
```

The scope is `on any`, `on accept`, `on reject`, `on failure` (committed
failures) or `on commit` (accepts and committed failures). `on reject:ID`
selects a declared rejection reason; failure-only reasons cannot label that
scope. Its formula reads only the original input (`pre`, `command`, `context`).
The law-only numeric term `decision_reason` is available with `on reject` or
`on failure`, without genesis, for example
`law 9000 selected_reason on reject = decision_reason != 201;`.
The optional `, genesis` suffix adds the genesis state. A law that applies at genesis reads only `post`
fields: genesis has no pre-state, command or context. Law formulas read
`pre.100.F`, `post.100.F`, `command.101[.F]` and `context.102[.F]`, and may
use `*`, `div_floor`, `div_ceil` and the built-in `products_ge` predicate.
[Contract tables](CONTRACT_TABLES.md) expand while `project.zeno` is parsed,
so a law written with `forall i in table T`, `count` or `sum` over a table
reaches generation as the ordinary formula it stands for; the rules here name
each table entry by its field ID. [Keyed collections](COLLECTIONS.md) lower
the same way: the rules read and write a named entry through its view
fields, by ID, compare keys only with `==` and `!=`, and may write a key into
a stored key field.
Signed law multiplication/division retains its existing checked i128 meaning.
Product predicates explicitly convert nonnegative values to U128 and use the
existing exact wide comparison, including the constant-one factors of a
two-factor comparison. Fused floor/ceiling calls, explicit target scales and
bounded repeats also have `.zeno` law syntax. The compiler checks quantity
relations before lowering them to the existing law opcodes. Quantifiers, sums, other named predicates,
free variables, `div_exact` and effect and event projections have
no contract form. Outbox projections require the explicit delivery-law bindings below. The library Authority evaluates every law that applies on
every decision, and a decision that breaks one is not committed.

### Law kinds

`law_kinds` gives each declared law one of these kinds. Five kinds fix the
scope the law must be declared with; at least one law must be a
`StateInvariant`.

<!-- policy-keys: law-kinds -->
| Kind | Required declaration | Use |
|---|---|---|
| `StateInvariant` | `on commit, genesis` | A property of every committed state and of genesis. At least one is required. |
| `AssetConservation` | any scope | Amounts are conserved. |
| `MintBurnAuthorization` | any scope | Who may create or destroy amounts. |
| `DebitCreditEffectEquality` | any scope | Debits, credits and effects agree. |
| `FeeAndRounding` | any scope | Fees and rounding. |
| `AuthoritySubjectRecipient` | any scope | Who may act, on what, for whom. |
| `RejectNoAuthority` | `on reject` | What a reject must satisfy. |
| `CommittedFailureEffects` | `on failure` | What a committed failure must satisfy. Required when any case is a `CommittedFailure`. |
| `DecisionConformance` | `on any` | Every decision. |
| `InitialCondition` | `on any, genesis` | The genesis state. |

### Delivery laws

An optional `delivery_laws` object binds each delivery-observing law to a
static maximum and declared channels. For example, the escrow fixture has
payload type 104, channel 300 and integer payload field 141. State fields
113 and 114 are cumulative payments to the seller and buyer:

```text
law 510 payouts_equal_released on commit = outbox.104 <= 2 && outbox.104.141 == (post.100.113 - pre.100.113) + (post.100.114 - pre.100.114);
```

```json
"delivery_laws": {
  "510": {
    "max_deliveries": 2,
    "observations": {
      "outbox.104": {"channel": 300},
      "outbox.104.141": {"channel": 300, "field": 141}
    }
  }
}
```

Give law 510 kind `AssetConservation` in `law_kinds`. The existing escrow
state-conservation law relates cumulative payments to funded and held amounts;
this additional law relates each payment increase to actual candidate
deliveries. Funding increases held funds without paying out, so a bare
`pre-held - post-held` formula would wrongly refuse funding.

`outbox.P` counts deliveries on its explicitly bound channel, whose payload
type must be `P`. `outbox.P.F` sums field `F` of that channel's payloads as
checked integers in the field's signed or unsigned domain. A declared sum field
contributes its stable variant ID as an I128 integer, after checking both the
type ID and the variant; a numeric atom cannot impersonate a variant. Boolean,
text and nested fields are unsupported. An observation may add
`"destination": "seller"` to count or sum only deliveries whose actual destination
is that exact text. Use an unfiltered count law as well when all other deliveries
must be absent. Root
paths use payload **type** IDs, while the JSON mapping supplies **channel**
IDs. Multiple distinct payload types/channels can appear in one law. Channels
sharing a payload type need separate laws; ambiguous mappings are refused.
These observations are law-only, not decision-program inputs.

`max_deliveries` must be an integer in 0..=64, at least the largest outbox
of any case. It bounds the complete outbox, including nonmatching channels.
There must be 1..=16 observations per entry. Every mapped law must be declared,
every mapped projection used, and every outbox projection mapped. Missing,
extra, duplicate, unused, wrong-type or unresolved entries refuse generation.
The usual law-kind scope requirements still apply; delivery laws bearing
`, genesis` are refused because genesis has no candidate outbox.

The compiler conjoins `actual_outbox_length <= max_deliveries` with the
formula. For each bounded slot it guards the channel read by actual length,
and guards a payload read by length, channel equality and any declared destination
filter. Missing slots
and nonmatching channels contribute zero; their absent fields are not read.
A matching missing field, wrong atom type or checked addition overflow
refuses the whole decision. A false bound never authorizes truncating a
longer outbox. These are observations of the actual candidate, independent
of the predictions in the case table and its conformance law.

All nodes run through the existing library law evaluator and shared meter.
Every node charges Step; active observations charge Read, inactive guarded
reads do not. Generated limits account for all compiled law nodes and possible
reads. Compilation adds O(max_deliveries × observations) nodes, with an
inactive observation retained at bound zero so the selected field remains
bound. The channel, field, bound and law graph enter ordinary canonical
policy bytes and checked identity. No new core operation or authority is added.
`on commit` covers Accept and CommittedFailure; an applicable Reject law
observes the structurally required empty outbox. A committing-only law is
skipped on Reject according to its declared scope.

`contract check-symbolic` does not support delivery observations as proof
targets. Its report explicitly lists that unsupported coverage and cannot
return Proved or Attested for such a contract; a checked counterexample to
another target can still make the report Refuted. Runtime enforcement and
finite examples do not prove that a law expresses the owner's intent, that
all reachable transitions satisfy it without refusal, or that an external
recipient performed a payment. The CLI compiler remains outside the verified
core assurance boundary.

The [escrow overlay](../crates/zeno-fcis-cli/tests/fixtures/delivery-laws/README.md)
retains a two-delivery double-payment control: the bound admits both entries,
while the accounting equality refuses their excessive sum.

### Framework laws

Generation adds laws to the declared ones. No declared law may use their
IDs.

- **The committed-failure law, ID 908** (or `framework_failure_law`), kind
  `CommittedFailureEffects`, `on failure`. It is added only when no declared
  law has kind `CommittedFailureEffects`, and its formula is `false`: with it,
  no committed failure is lawful. Generation therefore refuses a contract
  that has a `CommittedFailure` case and no such law, and names the case.
- **The reject law, ID 909**: every reject changes no state and delivers
  nothing (its post-state, patch, effects and outbox are empty). Its kind is
  `RejectNoAuthority`, and its ID is `framework_reject_law` (default 909),
  when no declared law has that kind; otherwise it is kind
  `AuthoritySubjectRecipient` with ID 909, beside the declared law.
- **Law 990**, `InitialCondition`, `on any, genesis`: the genesis state is
  exactly `genesis`, field by field. A store can therefore start only at
  that state.
- **Law 991**, `DecisionConformance`, `on any`: every decision is the one
  the case table selects for its inputs: the class, the reason, every
  successor field and every delivery's ordinal, channel, destination,
  idempotency value and payload. Each `when` is computed from the inputs
  again, not taken from the decision program. Adoption relies on this law;
  see [Contract adoption](CLI_REFERENCE.md#contract-adoption-and-store-upgrades).

## Server clock

“After day 7 the operator may refund.” A rule can read the time when the
contract declares one context field as the server clock:

```json
"clock": {"field": 131, "unit": "days", "epoch": "2026-11-01"}
```

<!-- policy-keys: clock -->
| Key | Value |
|---|---|
| `field` | A context field of integer type with a declared range, such as `type 115 int Day in 0..=14;`. |
| `unit` | `days` or `minutes`. |
| `epoch` | The UTC date, `YYYY-MM-DD` from 1970-01-01, at which the field counts 0. |
| `after_max` | Optional: `refuse` (default) refuses time above the range; `hold` keeps the maximum value as a terminal phase so deadline recovery remains available. Time below the minimum is still refused. |

Rules and laws read the field like any other context field, for example
`action == 163 && context.102.131 <= 7`. The field's declared range bounds
the context values a store accepts. With `after_max: hold`, later wall time
maps to the same final phase; use this only when that phase admits recovery
and new obligations cannot require a later time. Because that range is already part of the finite
input domain, `contract review`, the judges and the interface build check
every time without change. Generation checks only the key's form; the
application build checks that the field is a context integer with a
declared range. The key is not part of the canonical policy, so it leaves
the contract identity unchanged. The [build guide](BUILD_AN_APP.md#a-server-clock-for-deadlines)
describes how the server sets the field.

## UI metadata: `ui.json`

`zeno-fcis new --app` reads `ui.json` beside `project.zeno`. It supplies
labels, units, command names and refusal messages, never rules: the native
contract supplies every field, domain, command and reason. The build refuses
a missing or unknown key, a repeated key, metadata for an input, command or
reason the contract does not declare, and any label that is empty, longer
than 300 characters or contains `<`, `>` or a control character.
No string in `ui.json`, key or value, may hold a character that changes how
text reads without showing itself: C0 and C1 controls, bidirectional
controls such as U+202E, zero-width and other default-ignorable characters
such as U+200B and U+FEFF, line and paragraph separators, surrogates and
private-use characters. The build names the first one, as in
`UiInvisibleCharacter:ui.json.commands.160.label:U+202E`.

| Key | Required | Value |
|---|---|---|
| `format` | yes | `"zenoui/metadata/1"`. |
| `title` | yes | The application's title. |
| `action` | yes | The sum input that selects the command: `command.101.F` for a command record, or `command.101` when the command is itself a sum. |
| `fields` | yes | Input path to [field metadata](#field-metadata), for every input: `pre.100.F`, `command.101.F` or `command.101`, and `context.102.F` or `context.102`. |
| `variants` | yes | Variant ID to its label, for every variant of every sum input. Boolean inputs are shown as No and Yes. |
| `commands` | yes | Each command variant's ID to its [command metadata](#command-metadata). |
| `reasons` | yes | Each reason's ID to the message shown when a decision is refused for it. |

### Field metadata

| Key | Required | Value |
|---|---|---|
| `label` | yes | The field's label, such as `Held in escrow`. |
| `unit` | yes | The unit shown after a number, such as `units`. `unitless` shows no unit. |
| `values` | no | For an integer input only: a decimal value to its label, such as `{"0": "on the shelf"}`. 1 to 64 entries, each within the input's declared range. A labelled value is shown by its label alone. |

### Command metadata

| Key | Required | Value |
|---|---|---|
| `label` | yes | The command's name on its form and button. |
| `consequential` | yes | `true`: the command is sent only after the user confirms the reviewed inputs, and an edit voids that confirmation. `false`: it is sent without the confirmation step. |

For example, in the library loans example a copy's holder is an integer from
0 to 3:

```json
"pre.100.110": {"label": "Copy 1", "unit": "unitless",
  "values": {"0": "on the shelf", "1": "with member 1"}}
```

The state line then reads "Copy 1: on the shelf." Labels change only what
the interface shows; the contract's decisions and checks are unchanged.

### Fixed-table presentation metadata

`zenoui/metadata/3` extends metadata/2 with required `tables` and
`zenoui/requirements/2` requirements. It names complete fixed state tables
already declared in `project.zeno`, for example:

```json
"tables": {"110": {"label": "Book copies"}}
```

The native descriptor supplies `zenoui/native-fixed-tables/1` provenance from
the existing table compiler. Each entry binds the source table/owner/type,
consecutive field ID, actual input path and input position. Metadata supplies
only the table label. Every table must be bound completely, in native order;
entries cannot be repeated, reassigned or taken from command/context. Fields
retain their own labels and exact units/decimal scale, uniform within a table.
The existing checked `table-list` idiom displays the entries once; masked values
stay absent. Other state fields keep the ordinary display rules.

The emitted description is `zenoui/app-description/3`. Both generation and mount
recheck its binding before checking the existing presentation tree/receipt.
Metadata/1 and metadata/2 retain their scalar admission and description bytes.
Invented raw `tables`/semantic claims still refuse. This profile adds no dynamic
keys, queries, pagination, table editing or decision logic. Native captures and
factory source/asset receipts are separate required provenance; valid JSON
coordinates alone do not prove that a compiler produced them.

## Adoptions

`zeno-fcis contract adopt` appends one entry per adopted decision program
in the file's own layout: every existing byte is kept, and the new entry
goes after the last adoption, or under a new last key `adoptions`, one
entry per line and indented with the whitespace that starts the line of the
file's first key. Compact files use two spaces for the added entry; existing
bytes and line endings are preserved. Escaped JSON key spellings keep their
meaning and spelling. Edit `adoptions` only through that command. Only
`contract adopt` keeps the file's layout: `contract refresh-receipts`, when it
rebinds a receipt, and `contract evolve` write the whole file again in the
templates' layout, two-space indentation and one entry per line.

<!-- policy-keys: adoption -->
| Key | Value |
|---|---|
| `candidate_sha256` | SHA-256 of `v2/adoptions/N/program.zcve`, the adopted program. |
| `receipt_sha256` | SHA-256 of `v2/adoptions/N/receipt.json`, the `transform` receipt that compares it with the program it replaces. |
| `usage` | `preserved` when the receipt reports equal Step usage on every input; otherwise `new-version`. |
| `superseded_policy_sha256` | SHA-256 of the policy of the version this adoption superseded. Generation refuses any later edit that would change that version. |

## Evolutions

`zeno-fcis contract evolve DIR --to NEW` replaces an application's contract
with a reviewed rule change (see
[Rule changes for live stores](CLI_REFERENCE.md#rule-changes-for-live-stores)),
a rename, or a layout or rule change with a data migration (see
[Data migrations and renames](CLI_REFERENCE.md#data-migrations-and-renames)).
The new `v2/policy.json` is `NEW`'s rules, without `adoptions`, with an
`evolutions` list that the command appends to; edit it only through that
command. Entry `N` describes the contract that evolution `N` replaced, which
is kept under `v2/evolutions/N/` with its `project.zeno`, its
`v2/policy.json` without `evolutions`, its adoptions under
`v2/evolutions/N/v2/adoptions/`, and `review.txt`, the plain-language diff
of the change that the owner reviewed; a migration also keeps
`migration.json` and each shortcut as `shortcuts/from-{k}.json`.

<!-- policy-keys: evolution -->
| Key | Value |
|---|---|
| `superseded_policy_sha256` | SHA-256 of the last policy of the contract this evolution replaced. Generation regenerates that contract from its kept files and refuses any edit that would change it. |
| `review_sha256` | SHA-256 of `v2/evolutions/N/review.txt`. Generation computes the review again from the two contracts and refuses a review or an edit that differs. |
| `kind` | Absent for a behaviour change; `rename` or `migration` otherwise. Generation classifies the change again and refuses a kind the two contracts no longer make. |
| `migration_sha256` | For a migration only: SHA-256 of `v2/evolutions/N/migration.json`. Generation refuses an edited file, compiles it and admits it again by forward simulation. |
| `shortcuts` | For a migration only, optional: the shortcuts kept beside it, oldest starting version first. |

Each entry of `shortcuts`:

<!-- policy-keys: shortcut -->
| Key | Value |
|---|---|
| `from_version` | The lineage version the shortcut migrates from, the last version of an earlier contract. |
| `sha256` | SHA-256 of `v2/evolutions/N/shortcuts/from-{from_version}.json`. Generation refuses an edited file and checks the shortcut against the composed route again. |

### Migration files

A migration file, `zeno-fcis/migration/1`, maps the old state record to the
new one, field by field. `contract evolve --migration FILE` reads it, and
`--shortcut FILE` reads one with a `from_version`.

<!-- policy-keys: migration -->
| Key | Value |
|---|---|
| `schema` | `"zeno-fcis/migration/1"`, or `"zeno-fcis/migration/2"` for closed expressions and payload maps. |
| `from_version` | Only in a shortcut: the version it migrates from. A migration between consecutive versions has none. |
| `state` | An object with one entry per field of the new state record, keyed by its decimal field ID. |
| `drop` | Version 2 only: unique old state field IDs intentionally removed without being used by any new field. Admission still requires complete forward simulation. |
| `payloads` | Version 2 only: channel ID to a complete mapping of that channel's new payload fields. Every changed payload requires an explicit map. |

Each entry of `state` uses one source form, by its keys:

<!-- policy-keys: migration-field -->
| Key | Value |
|---|---|
| `from` | The decimal ID of an old state field. Alone, it carries that field's value over, under its own name or a new one; the field's kind (Boolean, integer, variant) must not change. |
| `default` | Alone, the value of an added field: `true`, `false`, an integer in the field's range, or a variant's ID. |
| `map` | With `from`, a table from every value of the old field to the new field's value: keys are decimal integers, `false` or `true`. It splits an old field into several new ones. |
| `expr` | Version 2 only, alone: a closed expression over `old.<field ID>` in the source record. Booleans, checked i128 integers, addition, subtraction, comparisons, Boolean operators and `choose` use the library finite evaluator; multiplication, division, text and external inputs are refused. |

Every new field needs exactly one entry. Every old state field must be carried
or read by a split expression, or explicitly listed in version 2's `drop`.
A dropped field cannot also be used by a new field, and the complete forward
simulation must show that its removal preserves the declared observations.
Its value leaves the current state; historical segments retain and replay it
under their original contracts. Text fields cannot be migrated. The same source forms apply to a
version 2 payload map, with `old` referring to that channel's payload. Every
payload map must be total and injective over the complete old payload domain:
no payload meaning may be erased or collapsed. The domain cap is 65,536
records; retained mapped bytes are capped at 64 MiB. A cap, partial map,
arithmetic trap or out-of-domain result refuses admission. Expression totality
also covers source valuations excluded by state laws. No missing map is inferred
for a changed payload. Destinations and channel IDs cannot change.

Forward simulation compares the actual old and new Authority publications:
ordered delivery lanes, ordinals, channels, destinations and idempotency are
exact, while new payload bytes equal the explicit map of old payload bytes.
A shortcut must match the composed state and payload maps on their complete
supported domains. Historical pending rows retain their original bytes,
owning Authority, entry hashes and delivery IDs; delivery and acknowledgment
still use that historical contract after an upgrade. The map declares how
future publications represent the same meaning. Units and interpretation
remain an explicit owner review obligation; bounded commutation alone does
not establish arbitrary business semantics.

## Changing a contract

Once stores run a contract, a change must be admitted by an upgrade path, and
each kind of change needs a different one. `zeno-fcis contract diff OLD NEW`
decides the kind of a change between two contract directories and names every
changed item; the [CLI reference](CLI_REFERENCE.md#contract-change-classification)
defines each kind exactly. In terms of the files:

- **Identical:** only things outside the contract change: law, reason and
  channel names, reason precedences, case `rule` notes, variable names or a
  variable no case or law reads, the `template` name, claims, comments. The
  canonical policy, and so the contract identity, stays the same; `contract
  diff` lists these differences as notes.
- **Program successor:** `contract adopt` replaced the decision program with a
  checked candidate. The program-successor upgrade admits it at any state.
- **Rename:** only the project's name or a type, field or variant name in
  `project.zeno` changes, with the same IDs. Rule variables may be renamed
  with it, since they are not part of the contract. `contract evolve`
  records it, and a store upgrades at any state.
- **Layout change:** a state or payload field is added, removed or given another type,
  or a type that only state fields have changes, such as a variant added to
  the state's status type. The genesis state and every committing case's
  `post` change with it. `contract evolve --migration` records it with a
  migration file, and a store upgrades when forward simulation over the old
  contract's whole declared input domain finds every decision, delivery and
  successor kept and every new state law holding on every migrated state.
- **Rule change:** the schema and the channels stay the same, and a law's
  formula, scope or kind, a case's `when`, class, reason, `post` or deliveries,
  the order of the cases, the genesis state, or the set of reasons changes. A
  reason is added with the case that uses it, and removed with its cases.
  `contract evolve` records it, and a store upgrades when the new contract's
  state laws and inductive claims hold on its state.
- **Unrelated:** the command or context changes, including a variant added to
  the command's action type; a destination type changes; a channel
  is added, removed or given another destination; or a rename comes together with any
  other change. No path admits it. Make a renaming its own version.

`contract adopt` refuses any adoption that is not a program successor of the
version it supersedes, and `contract evolve` any change that is not a rule
change, a rename, or with a migration a layout or rule change, each before
it writes anything.

## What generation checks, and what it does not

Generation checks that every entry has a contract form, then hands the
contract to the library's catalog binding, the same one the application
performs when it starts. When the library refuses, the error names the case
delivery, law or channel without which the library admits the contract, if
one does. Passing generation does not show that the rules say what the owner
means: the decision examples, `zeno-fcis contract review` and the laws check
that, each within its own scope.

## Background and scope of determination reporting

Edmund M. Clarke, E. Allen Emerson and A. Prasad Sistla, “Automatic Verification
of Finite-State Concurrent Systems Using Temporal Logic Specifications,”
*ACM Transactions on Programming Languages and Systems* 8(2), 1986, is
background for the exhaustive finite checks described here. Shared determination
reporting covers the declared finite domains and retains the runtime Authority;
its comparative prior-art search is pending.

## Method background and scope

C. A. R. Hoare, “An Axiomatic Basis for Computer Programming”, Communications
of the ACM, 1969 ([paper](https://doi.org/10.1145/363235.363259)), supplies the
background for explicit program assertions. The contract syntax and finite
compiler are implementation choices; no new proof method is claimed here.
Generation checks supported declarations and binding. Exhaustive review and
transform checks cover only their declared finite domains and limits.

### Background for delivery observations

These checks apply design-by-contract assertions to the actual output record
(Bertrand Meyer, “Applying Design by Contract”, IEEE Computer 25(10), 1992,
DOI 10.1109/2.161279). Variant validation and optional destination filtering
extend the existing bounded projection compiler; they add no new evaluator or
publication authority. Prior-art search pending; no novelty claim is made.

Terminal clock phases use the standard finite-state abstraction of times beyond
the last relevant bound; see Baier and Katoen,
[*Principles of Model Checking*](https://mitpress.mit.edu/9780262026499/principles-of-model-checking/).
The configured terminal phase still needs checked recovery rules.
