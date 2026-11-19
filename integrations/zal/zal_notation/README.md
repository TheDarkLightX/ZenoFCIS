# ZAL meaning and story package

This package retains and relates informal descriptions, candidate meanings, typed notation, and local story decisions. An abstraction may remain a draft with open questions and no executable interpretation. A semantic profile is a declared interpretation of a particular model, not a restriction on all ZAL expressions.

[Lenses](LENSES.md) enumerate registered viewpoints on the same exact library entry. The finite Mealy adapter provides source-retaining applications and replayed finite checks; general syntax/graph structure, pending custom definitions and informal analogies remain distinct. This integration exposes local Python/JSON functions, not an installed Lens CLI or MCP service.

[Explained ontology classification](ONTOLOGY.md) adds controlled-English rules,
shared-class queries, premise explanations, explicit-record validation and fresh
replay. Its six read-only operations are connected to the ZAL MCP bridge and the
ontology CLI. They do not accept application meaning or turn a shared class into
a checked behavioral relation. General notation and Lens APIs retain the
separate interface boundary described above.

The offline browsers use [Libertine reading typography](READING_TYPOGRAPHY.md)
for selected prose and controlled English, with a bundled original font and
license. Their exact source and evidence views remain available.

From `integrations/zal`, the Python APIs are available through `zal_notation`:

```python
from zal_notation import abstractions, story
from zal_notation.core import digest

session = story.create("Discuss what a clock should do", ["alice", "bob"])
wrap = abstractions.make_entry("wrap", "Wrap", story="Return to the start.")
stop = abstractions.make_entry("stop", "Stop", story="Stop at the boundary.")
menu = {
    "id": "clock_boundary", "base": None,
    "scene": "Another tick arrives at the end of the dial.",
    "question": "What should happen?",
    "operation": "generative-fold", "source_text": "Make a clock.",
    "options": [
        {"id": "wrap", "label": "Return to the start", "entry": wrap},
        {"id": "stop", "label": "Stop", "entry": stop},
    ],
}
session = story.propose(session, story.revision(session), menu)
session = story.vote(session, story.revision(session), digest(menu), "alice", digest(menu["options"][0]))
view = story.replay(session)  # Still awaiting Bob's choice.
```

The menu stores the source text and complete supplied alternatives; it does not invent or verify the interpretations. A decision requires the session's selected agreement rule. Ballots bind the participant label and exact menu, option, and preceding revision. Labels are self-declared and provide no authentication. Replay and export retain the decision history; an unresolved or unlinked entry remains a draft, and a model-linked entry is not a proof or authorization.

`sharing.make_context` binds a grammar, model, semantic profile, assumptions, and observation into an exact context identity. Unknown custom profiles remain representable and shareable; a request without a registered matching evaluator reports unsupported. Exact context identity is not a claim that different contexts are mathematically inequivalent. `normalizer.canonical_json` and `normalizer.alpha_normalize` retain their source and report their limited preservation scopes; alpha normalization applies only to the registered closed-lambda profile.

`sharing.toolkit()` lists the discussion, story proposal and ontology functions supported by `sharing.dispatch_tool`. Human-facing `story.vote` is deliberately not a proposal tool. These functions do not modify the behavioral review workspace. The ontology family has the CLI and read-only MCP routes described above; general discussion/story transport, authenticated collaboration, interpretation generation, runtime lowering and application qualification remain separate work.

## Source and checks

The package retains the original [MIT notice](LICENSE). The abstraction, story, normalizer, supplied-tick clock, sharing and Lens APIs described here are the locally integrated package. Component identities for the finite Lens adapter are recorded in `LENS_UPSTREAM.json`; they identify shipped files rather than asserting a public upstream publication or a proof. `examples.py` retains the custom grammar example.

Run the package tests from `integrations/zal`:

```sh
python3 -m unittest discover -s zal_notation/tests -p 'test_*.py'
python3 -O -m unittest discover -s zal_notation/tests -p 'test_*.py'
```

These tests cover the declared finite examples and selected syntax, semantic, replay, and source-retention boundaries. They are not a proof of arbitrary interpretations or a full ZenoFCIS release qualification.
The tests package is also visible to the repository's discovery rooted at `integrations/zal`.
