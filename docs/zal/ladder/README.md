# Experimental progression ladders

A ladder presents one typed statement through seven deterministic views:
Formula, Typed, Formal reading, Precise English, Intuitive, Plain and Question.
Each accepted view parses to the same expanded normalized meaning. A question
keeps its target and separately names a counterexample or witness search.
Repeated adjacent readings display “same as above”.

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
statements within one theory. Temporal comparisons also bind the complete
observation context and its versions.

The MCP bridge and review terminal expose the same statements with exact
revisions (`zal_ladder_read`, `zal_ladder_render`, `zal_ladder_parse`,
`zal_ladder_same`, `zal_ladder_lower`, `zal_ladder_explain`); see the
[ZAL guide](../README.md). A stale revision is refused, and nothing there
writes, accepts or evaluates a statement.

Read the [M0 contract](BUILD_SPEC_M0.md), [temporal contract](TEMPORAL.md),
[review checklist](REVIEW.md), [validation guide](VALIDATION.md), and
[generated temporal examples](EXAMPLES.md). All three temporal formalizations
use clearly marked placeholder definitions awaiting interpretation review.
