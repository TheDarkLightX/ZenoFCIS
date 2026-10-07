# Contract rules reference: `v2/policy.json`

An application contract is three files:

- `project.zeno` declares the types, fields, variants, reasons, channels
  and laws;
- `v2/policy.json`, described here, holds the decision rules, the genesis
  state and the kind of every law;
- `tests/decision-examples.txt`, optional, lists decisions the owner
  expects.

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
| `schema` | yes | `"zeno-fcis/template-declarative-policy/2"`. |
| `template` | yes | The application's name. `new --contract` uses it as the Cargo package name, so there it must be at most 64 lowercase letters, digits and `-`, starting with a letter. |
| `roots` | yes | Always `{"state": 100, "command": 101, "context": 102}`; see [Roots](#roots). |
| `leaf_bindings` | yes | Type ID to leaf; see [Leaf bindings](#leaf-bindings). May be `{}`. |
| `variables` | yes | Name to expression; see [Variables](#variables). May be `{}`. |
| `cases` | yes | The ordered decision table; see [Cases](#cases). |
| `genesis` | yes | State field ID to its value in the genesis state, for every state field: a JSON boolean, an integer, or a variant ID for a sum field. |
| `law_kinds` | yes | Law ID to [law kind](#law-kinds), for exactly the laws `project.zeno` declares. |
| `framework_failure_law` | no | Nonzero ID of the [framework committed-failure law](#framework-laws); default 908. |
| `framework_reject_law` | no | Nonzero ID of the [framework reject law](#framework-laws); default 909. |
| `adoptions` | no | Written by `zeno-fcis contract adopt`; see [Adoptions](#adoptions). |
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
command, the context. A contract has at most 32 inputs.

## Leaf bindings

Every type needs exactly one form: fields, variants, a declared integer range
(`type 105 int Money in 0..=1000;`), or a leaf binding here. So `bool` types,
`int` types without a range, and the text types of channel destinations need
a binding.

<!-- policy-keys: leaf-bindings -->
| Leaf | Meaning |
|---|---|
| `Bool` | `["Bool"]`: a boolean, 0 or 1 in examples. |
| `I128` | `["I128", min, max]`: an integer from `min` to `max`, with `min <= max`. Inputs and computed values must also fit in 64 bits. |
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
others. Its name may not start with `pre`, `post`, `command` or `context`.
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
| `*` | multiply; law formulas only |

  Prefix `!` negates a boolean. Other binary operators group to the left.

- functions:

<!-- policy-keys: functions -->
| Function | Meaning |
|---|---|
| `choose` | `choose(c, a, b)` is `a` when `c` holds, else `b`. |
| `div_floor` | `div_floor(a, b)`: `a / b` rounded down; law formulas only. |
| `div_ceil` | `div_ceil(a, b)`: `a / b` rounded up; law formulas only. |

Rules compile to the decision program, which adds, subtracts, compares and
selects. It has no multiplication or division, so `*`, `div_floor` and
`div_ceil` in a rule are refused. They exist because law formulas, written
in `project.zeno`'s own syntax, become the same expression tree and may use
them. A variant is its ID: `status == 151`.
A boolean used as a number is 0 or 1, and a number used as a boolean holds
when it equals 1. Decision arithmetic is 64-bit, and the program evaluates
every node, both branches of `choose` included, so an overflow anywhere
refuses the decision. A contract has at most 256 program nodes and 16
program outputs: the case selection and each distinct computed value.

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
| `post` | yes | State field ID to its value after the decision. |
| `outbox` | yes | The deliveries the decision makes; see [Deliveries](#deliveries). |
| `rule` | no | The rule's name for reviewers, text. Generation ignores it. |

<!-- policy-keys: classes -->
| Class | Commits | `post` and `outbox` |
|---|---|---|
| `Accept` | yes | `post` sets every state field. |
| `Reject` | no | `post` is `{}` and `outbox` is `[]`: a reject changes nothing and delivers nothing. |
| `CommittedFailure` | yes | `post` sets every state field. The contract needs a [`CommittedFailureEffects`](#law-kinds) law. |

A committing case states every state field, including the ones it keeps:
`"111": "pre.100.111"`, or a variable bound to it. A constant must fit its
field: a declared range, or a variant ID for a sum. A computed value outside
its field's range makes the library refuse that decision when it is made.

Reasons: every reason `project.zeno` declares must be used by at least one
case, and every case that uses a reason has the same class. A reason's
`precedence` in `project.zeno` does not change the case order.

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
failures) or `on commit` (accepts and committed failures), and `, genesis`
adds the genesis state. A law that applies at genesis reads only `post`
fields: genesis has no pre-state, command or context. Law formulas read
`pre.100.F`, `post.100.F`, `command.101[.F]` and `context.102[.F]`, and may
use `*`, `div_floor` and `div_ceil`. Quantifiers, sums, named predicates,
free variables, `div_exact` and effect, outbox and event projections have
no contract form. The library Authority evaluates every law that applies on
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

## Adoptions

`zeno-fcis contract adopt` appends one entry per adopted decision program
and renders the whole file again in the templates' layout (two-space
indentation, one entry per line), keeping its keys in order. Edit
`adoptions` only through that command.

<!-- policy-keys: adoption -->
| Key | Value |
|---|---|
| `candidate_sha256` | SHA-256 of `v2/adoptions/N/program.zcve`, the adopted program. |
| `receipt_sha256` | SHA-256 of `v2/adoptions/N/receipt.json`, the `transform` receipt that compares it with the program it replaces. |
| `usage` | `preserved` when the receipt reports equal Step usage on every input; otherwise `new-version`. |
| `superseded_policy_sha256` | SHA-256 of the policy of the version this adoption superseded. Generation refuses any later edit that would change that version. |

## What generation checks, and what it does not

Generation checks that every entry has a contract form, then hands the
contract to the library's catalog binding, the same one the application
performs when it starts. When the library refuses, the error names the case
delivery, law or channel without which the library admits the contract, if
one does. Passing generation does not show that the rules say what the owner
means: the decision examples, `zeno-fcis contract review` and the laws check
that, each within its own scope.
