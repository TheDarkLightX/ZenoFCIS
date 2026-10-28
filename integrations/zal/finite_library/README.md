# Optional finite semantic library

This package supplies a bounded finite relation algebra. `UPSTREAM.json` records
the component identities in this integration, and `LICENSE` retains the original
MIT notice. General ZAL abstractions live separately and may remain informal or
use another profile.

`prototype` supplies bounded finite fit, conjunction, independent product,
missing-component search, distinguishing traces, exact quotient checks, and
dependency-pinned entries. Retrieval uses words and curated tags; each hit still
needs a semantic fit check. Resource names are declared assumptions.

The current consumer `semantic_library.admit` accepts only a complete,
deterministic, effect-free reservation machine profile. It parses the emitted
model with the current ZAL parser and compares every decision, reason and next
state to the incoming table. It retains exact source text, its byte digest, the
semantic digest, initial state and the declared observations. Use
`semantic_library.admit_library(library, entry_id, expected_revision)` for a
pinned entry and its dependency closure. Neither API installs executable code.

```python
from finite_library.prototype import Entry, Library, slot
from semantic_library import admit_library

entry = Entry("reservation_once", "One-use reservation", ("seat",),
              slot(reusable=False))
library = Library((entry,))
model, correspondence = admit_library(library, entry.id, entry.revision)
print(model.render(english=True))
```

Run the affected tests from `integrations/zal`:

```sh
python3 -B -m unittest finite_library.test_prototype test_semantic_library
python3 -O -B -m unittest finite_library.test_prototype test_semantic_library
```

Local changes additionally reject invalid Unicode and work-counter decrements,
and resolve a shared dependency graph once per call while checking every pin.
Every library operation validates the current collection; completion captures
one collection for both enumeration and dependency resolution.

The in-memory algebra admits up to 4,096 rows and 65,536 output choices; its
JSON and controlled-table readers separately cap source at 1 MiB. Some valid
large in-memory objects exceed that wire limit and cannot round-trip through
those readers. Their serialized size must be checked before transfer; no input
cap is raised or oversized input admitted by this integration.

The generated model can enter `factory.lower` through its existing finite
profile; native Authority, generated-app and browser qualification are separate
checks. The producer's legacy `emit_zal` is an unqualified research emitter;
use the consumer for checked lowering. Arbitrary profile composition, shared
resource ownership, authenticated identities, effects and human agreement
remain outside this finite adapter.
