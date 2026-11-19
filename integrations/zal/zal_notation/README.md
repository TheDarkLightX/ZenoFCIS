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

## Author a search film

A storyboard chooses the goal, shot order, wording and duration. The compiler
binds route histories and their endpoint groups to a native ZAL state-machine
model. Each full history retains its turn cost when the drawing merges paths.
The [83-second example](../examples/search-film/Problem-Solving-as-Search.json)
and [eight-second example](../examples/search-film/Another-Goal.json) use the
same compiler and renderer.

Run these commands from `integrations/zal`. Checking uses Python's standard
library. Rendering also requires Node.js 20 or later and FFmpeg with libx264.

```sh
python3 -m zal_notation film-check examples/search-film/Problem-Solving-as-Search.json
npm ci --prefix zal_notation/film_renderer
python3 -m zal_notation film-render examples/search-film/Problem-Solving-as-Search.json --output films
```

A build writes a light MP4, a dark MP4, WebVTT captions, an adaptive SVG
of the route model and the checked JSON plan into a new directory. Both
videos use that plan at 30 frames per second. Use `film-check --output PLAN`
to export the plan alone. The original `essay-film/1` pilot storyboards can
also be read with their sibling evidence files.

| Shot | What the viewer sees |
|---|---|
| `trace` | Replay a history through the grid. An optional `path` such as `"RU"` selects a particular legal prefix. |
| `branch` | Extend each history with the next available choice. |
| `merge` | Bring equal endpoints together and label each with the number of histories it contains. |
| `select` | Compare feasible histories by direction changes and show every minimum-cost route. |

The additional views are `budget`, `language`, `programs`, `reuse` and
`relation`. In `zal.search-film/1`, the two positive `walk` coordinates sum
to at most six moves. A `reuse` shot takes a `transfer` goal of up to ten moves.
Shots last 2–120 seconds each, up to 64 shots and ten minutes per film.
Durations are rounded to frames. Titles and captions can interpolate
`{moves}`, `{routes}`, `{endpoints}`, `{solutions}`, `{optima}`, `{budget}`,
`{first}` (one-based first successful trial), `{trialHits}` and `{untested}`.
Authors supply the prose around the model values.

For the essay's `programs` shot the compiler checks the complete synthesis
document and its registered replay against their pinned identities, then
independently evaluates the interval candidates on all 256 bytes. For a new
byte example, replace the `evidence` and `replay` fields with an `interval`
such as `{"lower": 8, "upper": 12}`. This runs the local finite checker on the
valid neighboring bounds and records the mismatching inputs. Both paths check
interval semantics. The generated Rust and theorem proving have separate
execution and evidence requirements.

The Python operations in `search_film` also accept deterministic, possibly
partial native ZAL state machines. `transition_table(model)` supplies the start
and transitions to `trace_path` and `branch_on_choice`. `merge_equivalent_states`
groups histories by an observable key. `select_by_cost` accepts a cost function
and returns all tied witnesses in input order. The built-in paintings cover
right/up grids and byte intervals. Other domains can reuse the operations and
provide their own drawings.

## Source and checks

The package retains the original [MIT notice](LICENSE). The abstraction, story, normalizer, supplied-tick clock, sharing and Lens APIs described here are the locally integrated package. Component identities for the finite Lens adapter are recorded in `LENS_UPSTREAM.json`; they identify shipped files rather than asserting a public upstream publication or a proof. `examples.py` retains the custom grammar example.

Run the package tests from `integrations/zal`:

```sh
python3 -m unittest discover -s zal_notation/tests -p 'test_*.py'
python3 -O -m unittest discover -s zal_notation/tests -p 'test_*.py'
```

These tests cover the declared finite examples and selected syntax, semantic, replay, and source-retention boundaries. They are not a proof of arbitrary interpretations or a full ZenoFCIS release qualification.
The tests package is also visible to the repository's discovery rooted at `integrations/zal`.
