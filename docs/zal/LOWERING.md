# From ZAL to a checked contract

The file CLI lowers either finite-FSM profile through the existing factory
lowerer. A destination must be absent or empty:

```sh
python3 integrations/zal/run.py lower integrations/zal/examples/context_free2.zal --out counter-contract
python3 integrations/zal/run.py lower integrations/zal/examples/revoked_mail2.zal --out mail-contract
```

English input uses `--syntax english`. `export` retains the same declaration
route. A model with no context facts or integer inputs gets a context record
with a single integer field whose only value is zero. It introduces no new
observation or choice. Reference examples include that zero wire value.

## Safety properties

Profile 2 lowers `always(P)` and `never(P)` when P is a step formula. Never
means that the negation of P holds at every step. Each goal uses the existing
requirement translation: one acceptance law and one rejection law, with
`accepted` folded to the outcome and rejection post-state reads replaced by
the unchanged pre-state. A side that folds to true needs no law.
`zal-source-map.json` retains the property IDs and their generated law IDs.

The example's `no_new_mail_after_revoke` says:

```text
never(state == revoked & accepted & event == read_mail)
```

The retained control state records revocation. Its acceptance law forbids a
new accepted read from that state. A second property keeps the read count
unchanged after revocation. These are finite control and counter properties;
the example supplies no external authentication or mail delivery semantics.

The model checker judges safety on reachable executions. Contract laws apply
to every admissible pre-state, including unreachable ones. The lowerer checks
the goal on every admissible step and refuses if that stronger scope would
change a reference decision. Next, within, eventuality, until, fairness and
unresolved obligations still refuse lowering by name. No temporal monitor or
hidden history variable is introduced.

## Refinement and generated execution

Build the pinned CLI using the repository's normal instructions, then run:

```sh
python3 integrations/zal/run.py refine-check integrations/zal/examples/revoked_mail2.zal mail-contract --cli target/debug/zeno-fcis
python3 integrations/zal/run.py qualify integrations/zal/examples/revoked_mail2.zal
```

`refine-check` generates the abstract contract with the same lowerer and calls
the existing native `contract refine-check`. It source-pins both contracts and
uses the identity state-ordinal abstraction. State and input layouts must
match. The native checker evaluates both through the library Authority over
the complete declared raw state/input product. A changed decision, rejection
reason, successor or delivery produces a counterexample. Its finite domain
and work limits still apply; incomplete checking is inconclusive.

This check compares a concrete contract with the lowering. The separate
`qualify` route checks the ZAL reference decisions against the actual generated
application's Authority, as well as generation, drift and scaffold checks.
Identity refinement alone does not establish the lowerer's correspondence to
ZAL. Neither operation grants application adoption, publication or deployment
authority, or establishes equality of resource usage.

The regression control adds a read from `revoked` and weakens the corresponding
contract laws. The contract still generates, but native refinement fails at
`revoked`, read count zero, event `read_mail`. Retaining the laws also rejects
that added behavior through the Authority.

## Source diagnostics

Finite-profile CLI refusals include `source_diagnostic`, with one reason, a
file, a physical one-based line and column, and a suggested fix. Comments and
blank lines count toward physical positions; semicolons inside a whole-line
comment have no meaning. Missing declarations point to the end of the file.
Spans are diagnostic metadata and do not enter the model revision. A refusal
without a parser span labels its position as `operation`, rather than claiming
an exact parser location. Suggestions never edit source or change admission.

The ten selected recurring mistake families come from the finite examples,
parser regression tests and identifier-boundary cases:

| Mistake | Suggested repair |
| --- | --- |
| Identifier longer than 32 characters | Shorten it and its references. |
| Missing statement terminator | Add `;` or the English period. |
| Repeated singleton declaration | Keep one declaration. |
| Missing explicit declaration | Add the named declaration. |
| Reserved word used as a name | Rename it and its references. |
| Undeclared reference | Declare it or correct its spelling. |
| Boolean used in integer arithmetic | Use a predicate or a bounded integer. |
| Formula reads outside its scope | Move step conditions into a requirement. |
| Invalid range or initial value | Respect the declared integer bounds. |
| Missing outcome/frame or repeated update | Give one explicit outcome and at most one update per variable. |

## Lineage and scope

The language approach follows Tobias Kuhn, *A Survey and Classification of
Controlled Natural Languages*, Computational Linguistics 40(1), 2014
([published article](https://doi.org/10.1162/COLI_a_00168)). Safety and temporal
checking follow Christel Baier and Joost-Pieter Katoen, *Principles of Model
Checking*, MIT Press, 2008. Refinement mappings follow Martín Abadi and Leslie
Lamport, *The Existence of Refinement Mappings*, Theoretical Computer Science
82(2), 1991. Source positions and repair diagnostics follow Alfred V. Aho,
Monica S. Lam, Ravi Sethi and Jeffrey D. Ullman, *Compilers: Principles,
Techniques, and Tools*, second edition, Pearson, 2006.

This is an integration of existing finite checking and translation methods;
prior-art search pending. Results are exhaustively checked over the declared
domains when the named checks complete. They are not a theorem about all
lowerings, external observations or a deployed application.
