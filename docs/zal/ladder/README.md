# Experimental progression ladders

A ladder starts with one canonical formula and a legend explaining its symbols.
Read it as Formula, Formal reading, Precise English, Intuitive, Plain or Question.
Typed is the formula's ASCII spelling; the legend is a key, not another claim.
Every accepted view parses to the same expanded normalized meaning in its declared
theory. A question keeps that target and separately names a counterexample or
witness search. Repeated adjacent readings display “same as above”.
Formula and Legend display Greek letter names as Unicode letters (such as `sigma` → `σ` and `Sigma` → `Σ`); Typed and spoken readings keep the names, and both input spellings preserve the same canonical formula.

Ask: **No accepted swap should decrease the reserve product.** Select the
plain view of the [reserve-product example](../../../integrations/zal/examples/ladders/dex_k.json):
“No accepted swap decreases the reserve product.” Its attached definition is
`k(t) := x(t)*y(t)`; its formula compares the product after and before each
accepted step. The [refinement journey](../REFINEMENT.md) explains the separate
contract check. Choosing an easier reading never discharges that check.

The Python prototypes have **authority: none**. Their statements, renderings,
fixture results and native-source previews supply no runtime admission,
publication permission or proof certificate.

Two explicitly different profiles are available:

| Profile | Theory | Scope |
| --- | --- | --- |
| `zal/ladder/0` | `steps-over-integers` | Integer and Boolean step statements |
| `zal/ladder-temporal/1` | `ordered-events-over-integers/1` | Ordered event observations with explicit finite or unbounded scope |

Neither changes `zal/finite-fsm/1` or `zal/fcis-loop/1`.

From the repository root:

```sh
python3 -I integrations/zal/ladder.py render integrations/zal/examples/ladders/dex_k.json
python3 -I integrations/zal/ladder.py render integrations/zal/examples/temporal/vault_drain.json
python3 -I integrations/zal/ladder.py lower integrations/zal/examples/temporal/ui_discovery.json
```

Use `render FILE --rung plain` to select a view. Its scope, definitions and
assurance status remain attached. `parse --rung R TEXT --context FILE` shows
canonical meaning; `same A B` and `find A --corpus DIR` compare normalized
statements within one theory. Both file and tool comparisons bind the complete
declared context, excluding only the display ID, formula text and fixtures.
This includes the vocabulary, scope and statement kind; temporal comparisons
also bind observation versions, assumptions, order and horizon. Equal normal
forms with equal context establish the same meaning under those declarations.
A different normal form or context does not establish different meanings;
the comparison is conservative and does not decide every logical equivalence.

A primed Boolean definition is expanded on English rungs so that every state
term keeps its “after” meaning. For example, `Low(x) := level(x)<1` makes
`Low'(x)` read “level after x is less than 1.” Primitive state predicates with
no integer definition are refused by name; they cannot silently lose a prime.
No new operator is needed. These explicit refusals and the wider context
binding are intentional compatibility changes; existing example meanings stay
the same. Registered temporal readings are declared names for complete formulas,
including their scope. A parse-back check verifies that association, not the
accuracy of arbitrary natural-language prose.

Check every example and every view with:

```sh
python3 -m unittest discover -s integrations/zal -p test_ladder_preservation.py -v
```

The check compares each parsed rung with its canonical formula and evaluates
each finite-table fixture on every rung. Set `ZAL_TEMPORAL_CLI` to the built
`ladder_temporal_parity` diagnostic to include native temporal fixture checks.
Unbounded examples also receive finite illustrative replays, explicitly labeled
as restrictions; those replays do not prove their unbounded obligations.

The MCP bridge and review terminal expose the same statements with exact
revisions (`zal_ladder_read`, `zal_ladder_render`, `zal_ladder_parse`,
`zal_ladder_same`, `zal_ladder_lower`, `zal_ladder_explain`); see the
[ZAL guide](../README.md). A stale revision is refused, and nothing there
writes, accepts or evaluates a statement.

Read the [M0 contract](BUILD_SPEC_M0.md), [temporal contract](TEMPORAL.md),
[review checklist](REVIEW.md), [validation guide](VALIDATION.md), and
[generated temporal examples](EXAMPLES.md). The two withdrawal statements are
bound to the withdrawal-queue contract; the [withdrawal pilot](WITHDRAWAL_PILOT.md)
states their evidence. [Contract-derived patterns](PATTERNS.md) show two short
readings checked against four existing contracts. The vault and interface examples use proposed
definitions, marked on every rung, awaiting owner confirmation.

## Lineage and scope

**Background.** Tobias Kuhn, “A Survey and Classification of Controlled Natural
Languages,” *ACM Computing Surveys* 46(3), article 41, 2014, surveys constrained
natural-language systems. Matthew B. Dwyer, George S. Avrunin and James C.
Corbett, “Patterns in Property Specifications for Finite-State Verification,”
*ICSE*, 1999, organizes recurring temporal requirements as patterns. Niklaus
Wirth, “Program Development by Stepwise Refinement,” *Communications of the ACM*
14(4), 1971, describes successive program refinement.

**What is new here.** These examples combine canonical formulas, reversible
readings and finite reference checks. Prior-art search pending.

**Scope.** Parse-back establishes the declared symbolic meaning; finite fixture
checks establish only those valuations or words. Neither establishes the truth
of a requirement for every implementation or validates unrestricted English.
