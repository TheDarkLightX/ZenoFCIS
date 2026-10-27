```json
{
  "name": "coverage",
  "title": "Expand witnessed contract coverage",
  "steps": [
    {
      "command": "python3 integrations/zal/run.py coverage examples/guide-counter --out \"$WORK/counter\"",
      "exit": 0,
      "output": {"schema": "zeno-fcis/coverage-loop/1", "status": "no-residual-witness", "exit": 0, "before": {"gaps": 12, "admitted_mutants": 9}, "after": {"gaps": 0, "admitted_mutants": 0}}
    },
    {
      "command": "python3 integrations/zal/run.py coverage examples/guide-counter --max-work 1 --out \"$WORK/budget\"",
      "exit": 2,
      "output": {"schema": "zeno-fcis/coverage-loop/1", "status": "inconclusive", "exit": 2, "before": {"gaps": null, "admitted_mutants": null}, "after": {"gaps": null, "admitted_mutants": null}}
    }
  ]
}
```

# Expand witnessed contract coverage

Use this Python CLI card to propose additional laws for human confirmation.
Set `ZENO_FCIS_BIN` to a compatible binary with `contract review --gaps`.
Set `WORK` to a fresh writable directory outside the contract. From the repository root:

```sh
python3 integrations/zal/run.py coverage examples/guide-counter --out "$WORK/counter"
```

Expected stdout (exit 0):

```json
{"schema": "zeno-fcis/coverage-loop/1", "status": "no-residual-witness", "exit": 0, "before": {"gaps": 12, "admitted_mutants": 9}, "after": {"gaps": 0, "admitted_mutants": 0}}
```

For a deliberately insufficient judge budget:

```sh
python3 integrations/zal/run.py coverage examples/guide-counter --max-work 1 --out "$WORK/budget"
```

Expected stdout (exit 2):

```json
{"schema": "zeno-fcis/coverage-loop/1", "status": "inconclusive", "exit": 2, "before": {"gaps": null, "admitted_mutants": null}, "after": {"gaps": null, "admitted_mutants": null}}
```

Read `coverage.json`, `COVERAGE.md` and the referenced native packets. Review
each kept law's reading, its closed witnesses and the residual witnesses.
Disagreeing proposals remain findings and cause exit 1. A successful run adopts
nothing. Unsupported contracts, malformed proposals and I/O failures retain
their refusal codes. See the [loop reference](LOOPS.md#coverage-expansion).
Optional `--idioms FILE` supplies a JSON list of
`zeno-fcis/design-idiom-instance/1` entries (`idiom`, `law`, `dimension`,
`source`). Each is checked like a proposal and reported separately as `idiom`.

Background: E. M. Clarke, O. Grumberg, S. Jha, Y. Lu and H. Veith,
*Counterexample-Guided Abstraction Refinement*, CAV 2000; R. A. DeMillo,
R. J. Lipton and F. G. Sayward, *Hints on Test Data Selection: Help for the
Practicing Programmer*, Computer, 1978. These supply background on witness
refinement and mutation testing; they do not certify this implementation.
What is new here: prior-art search pending; no novelty claim.
Scope: coverage here means witnessed by the gap judge's mutants and gaps,
not every possible behaviour, owner intent, liveness or privacy verification.
