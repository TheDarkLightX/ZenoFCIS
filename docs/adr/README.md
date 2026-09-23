# Design records

Each record states one decision, why it was made, and the evidence that
supports it. Records are append-only. A later record may supersede an earlier
one, but it does not rewrite it.

| Record | Title | Status |
| --- | --- | --- |
| [0001](0001-meaning-first-assurance.md) | Meaning-first assurance | Accepted |
| [0002](0002-principles.md) | Principles for the V2 assurance program | Accepted |
| [0003](0003-epistemic-status.md) | Epistemic status of evidence and witnesses | Accepted |
| [0004](0004-v2-ledger.md) | V2 ledger of deferred breaking changes | Open |

Each record has these sections: Status, Context, Decision, Consequences, and
Evidence. The Evidence section names laws, tests, or commits.
