# Short readings from existing contracts

These examples use two named definitions, with no new grammar or operators.
The canonical formula stays expanded. The plain rung folds its repeated body
into a familiar phrase; the definition and finite observation scope remain
attached. A shorter reading does not replace the contract's independent laws.

## Only the chosen copy changes

Ask: **Changing one loan must leave every other copy's holder unchanged.**

Theory: `steps-over-integers`, restricted here to one step, the declared copies,
holders 0 through 3 and the chosen-copy command field. Zero means on the shelf.
`Committed(t)` means the step commits. The canonical formula is:

```text
forall t in T: Committed(t) =>
  (forall c in C: c != chosen(t) => holder'(t, c) = holder(t, c))
```

Define `OnlyChosenChanges(t)` as the inner quantified formula. The plain rung
reads: **For every step t, if t commits, then t changes only the chosen copy.**
It permits the chosen copy to stay unchanged. It says nothing about permission
to borrow or about rejections; those remain separate requirements.

This is law 502 in both real contracts:

| Contract | State representation | ZAL example |
| --- | --- | --- |
| [Three-copy library](../../../examples/apps/library-loans/project.zeno) | Three holder fields | [Three-copy statement](../../../integrations/zal/examples/patterns/library_three_frame.json) |
| [Six-copy library](../../../examples/contract-tables/library-loans/project.zeno) | One six-entry table | [Six-copy statement](../../../integrations/zal/examples/patterns/library_six_frame.json) |

Choose a rung, then run the contract-bound comparison:

```sh
python3 -I integrations/zal/ladder.py render integrations/zal/examples/patterns/library_three_frame.json --rung plain
python3 -m unittest discover -s integrations/zal -p test_contract_patterns.py -v
```

The three-copy check exhausts all 24,576 combinations of pre/post holder values,
chosen copy and commit flag. The six-copy check exhausts 1,152 per-copy cases:
chosen index, observed index, both holder values and commit flag. The latter
covers the independent equality clauses pointwise, not all complete state pairs.
Both compare all seven displays with the source law's independently stated
equation. For example, changing a nonchosen copy from holder 0 to holder 1 is
refused; changing only the chosen copy is allowed by this requirement.

## Missing entries hold nothing

Ask: **A missing account must hold zero balance; a missing member must hold zero loans.**

Theory: `steps-over-integers`, restricted here to one post-state snapshot at
genesis or commit and two entry slots. Presence is 0 or 1. Account balances
are 0 through 3; member loan counts are 0 through 2. The formula is:

```text
forall t in T: forall e in E:
  present(t, e) = 0 => amount(t, e) = 0
```

Define `MissingHoldNothing(t)` as the inner quantified formula. The plain
readings are **For every snapshot t, t gives missing accounts a zero balance**
and **For every snapshot t, t gives missing members a zero loan count**.
Present entries may also hold zero: the implication is not an equivalence.

| Contract | Source law | ZAL example |
| --- | --- | --- |
| [Token balances](../../../examples/collections/token-balances/project.zeno) | 501, absent accounts hold nothing | [Account statement](../../../integrations/zal/examples/patterns/token_missing_zero.json) |
| [Member loans](../../../examples/collections/library/project.zeno) | 501, absent members hold nothing | [Member statement](../../../integrations/zal/examples/patterns/member_missing_zero.json) |

Use `render FILE --rung precise` to expand the definition. The same test command
above compares all 64 account-slot and 36 member-slot valuations on every rung.
These are finite projections of per-entry laws, not enumeration of every map or
verification of collection updates. Missing presence with amount 1 is a
counterexample; present presence with amount 0 is allowed.

## Why these two definitions stayed

The expanded statement, the short phrase and one named body are all the reader
needs. There is no extra state, parser rule, cache or execution path. Each
plain statement is shorter than its precise expansion, and the two independent
source-law comparisons retain the difference between a guarded requirement and
a universal prohibition. Definition text adds bytes to the example record;
the claim is a shorter human reading, not a smaller serialized file.

Exactly three candidates were considered:

| Candidate | Decision | Reason |
| --- | --- | --- |
| Only the chosen copy changes | Keep as a definition | One phrase replaces repeated per-copy equalities in two representations. |
| Missing entries hold nothing | Keep as a definition | One phrase replaces the same presence/value condition in two contracts. |
| Exactly one owner | Drop | Shelf copies and missing entries may have no holder. “Exactly one” would add a false requirement. |

Regenerate the examples through their producer, or check them for drift:

```sh
python3 integrations/zal/contract_patterns.py
python3 integrations/zal/contract_patterns.py --check
```

The records retain the source path, law text and source hash. Tests also bind
the source-law spelling and relevant range declarations to the finite oracle.
A source change needs a reviewed projection, not an automatic claim of equivalence.

## Lineage and scope

**Background.** Matthew B. Dwyer, George S. Avrunin and James C. Corbett,
“Patterns in Property Specifications for Finite-State Verification,” *ICSE*,
1999, organizes recurring temporal requirements as patterns. Tobias Kuhn,
“A Survey and Classification of Controlled Natural Languages,” *ACM Computing
Surveys* 46(3), article 41, 2014, surveys constrained natural-language systems.
Niklaus Wirth, “Program Development by Stepwise Refinement,” *Communications
of the ACM* 14(4), 1971, describes successive refinement.

**What is new here.** These are source-linked applications of existing ZAL
definitions and finite checks. Prior-art search pending.

**Scope.** The checks cover the stated observations and finite valuations.
They do not establish authentication, history properties, resource bounds or
publication authority for the surrounding applications.
