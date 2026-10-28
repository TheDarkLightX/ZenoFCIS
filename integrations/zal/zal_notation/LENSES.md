# Lenses on a retained abstraction

The optional entry-bound category host is available through
`category_profile/__main__.py` and actual `zal_library_category_*` sharing tools.
See `../category_profile/README.md` for immutable operator approvals, multi-object
category/functor/inverse checks and authored symbolic composition. The generic
typed-category hint stays Pending until an explicit interpretation/request is
supplied; finite checks do not certify arbitrary source meaning.

A Lens offers a viewpoint on one exact library entry. The entry keeps its story,
formal context if supplied, questions and alternatives. Enumeration lists known
registered candidates; it does not enumerate every possible mathematical view.
The original entry remains available even when a view forgets information.
Returning to it is source restoration, not inversion of a lossy map.

From `integrations/zal`:

```python
from zal_notation import abstractions, builtin, lens_library, lenses, sharing
from zal_notation.core import digest

context = lenses.make_source()  # finite counter: inc modulo four, plus reset
discussion = sharing.make_discussion(context, builtin.generic_pack(context["grammar"]))
entry = abstractions.make_entry("counter", "Counter", story="Increment or reset.",
                                discussion=discussion)
library = abstractions.register(abstractions.new_library(), entry)
identity = digest(entry)
catalog = abstractions.enumerate_lenses(library, identity)
application = lens_library.apply(library, identity, "unit-group")
receipt = lens_library.check(library, identity, application)
assert receipt["status"] == "checked"
assert lens_library.verify_receipt(library, identity, application, receipt)
assert lens_library.restore(library, identity, application) == entry
```

All views in this catalog refer to that same entry revision. Applications retain
the complete source entry and bind its digest, the containing library revision,
the exact lens definition, implementation source and underlying source context.
A library change requires regenerating the entry-bound application. Checking
replays the fixed adapter; claimed statuses, labels or edited hashes do not
substitute for it. A receipt that replays as refuted or incomplete remains so.

The included finite Mealy engine uses the existing local
`finite_library/prototype.py`, not an ambient module named `prototype` or a
second copied dependency. See `LENS_UPSTREAM.json` for exact source identities.

| Finite view | Actual scope on the counter |
| --- | --- |
| Expanded/folded definition | Checked presentation changes; same complete context |
| Finite table and state-machine graph | Checked exact table/row representations |
| Parity quotient | Checked map retaining outputs and mapped successor; concrete state distinctions are lost |
| Transformation monoid | Eight closed state effects, including reset |
| All effects form a group | Refuted: reset merges distinct states |
| Unit group | Four invertible rotations; noninvertible effects are excluded |
| One-object category | Eight monoid arrows on one object; composition is left after right |
| Execution tree and automorphism group | Unsupported constructions, not refuted claims |

The algebra views forget command-word spelling, intermediate states, timing and
output observations. A permutation is an element of a group, not automatically
a group by itself. Refuting invertibility of all command effects does not refute
other group-theoretic views, such as symmetries. The category here is specifically
the monoid-to-one-object construction on one ordered carrier, not an arbitrary
typed category of programs. Multi-object composition needs a separate adapter.

`inductive-syntax` checks a supplied term model's constructor structure, and
`relational-graph` checks a supplied graph model's structure. Both retain the
complete declared context through an identity map. They do not establish the
truth of custom semantics. A finite syntax tree can describe cyclic computation;
the counter has a cyclic transition graph, and its execution-tree construction
remains unsupported. `sequence-chunks` is a pending interpretation, not a checked
conversion or a general sequence semantics.

Definitions can be authored without an executable interpretation:

```python
proposed = lens_library.definition("symmetry-analogy", "Possible symmetry",
    summary="Explore a symmetry analogy before choosing its mathematical meaning.",
    assumptions=["An author supplies a carrier and action."], preserves=[],
    forgets=[], adds=["a proposed correspondence"], mode="analogy")
registry = lens_library.register(lens_library.default_registry(), proposed)
catalog = abstractions.enumerate_lenses(library, identity, registry)
```

An author can retain a proposed target and correspondence even when no checker
supports it. `lens_library.propose(library, identity, lens_id, target_entry,
correspondence, registry=registry)` takes a complete target abstraction and
`{"description": "Proposed map", "pairs": [{"source": "story", "target": "story"}]}`.
It returns a separate `zal.library-lens-proposal/1` packet with status proposed or
analogy, exact source/target/definition identities, and an explicitly unverified
map. Both entries may remain drafts. Map labels are supplied descriptions, not
certified semantic anchors; the packet is not accepted by `check` as evidence.
No target is registered or adopted by this operation.

Discovery distinguishes candidate, conditional, pending, analogy and unsupported.
It never labels a candidate checked. `check` supplies checked, refuted, unsupported
or incomplete evidence for the exact supported profile. Imported definitions may
state assumptions and claimed preservation/loss, but cannot import code or bind a
checker. A changed definition cannot borrow a built-in checker's identity.
New executable profiles need separately reviewed trusted host code.

The registry digest is discovery/proposal provenance. Registry membership is
not an admission policy: removing a definition from a caller's registry does
not invalidate a mathematical check of an existing exact trusted application.
Application checks bind the definition itself. A product requiring capability
revocation would need a separate explicit admission contract.

Keep entry meaning and operation evidence as separate statuses. For example,
“Structure checked” can coexist with unresolved meaning/requirements and
declared laws. Author-supplied preserves/forgets fields remain proposed claims.

Informal drafts, unresolved alternatives and custom profiles remain valid library
entries. Registry and application data use new versioned Lens schemas; existing
`zal.abstraction-library/1` entries and relations are unchanged. Raw source text is
retained verbatim in the entry. In-memory integer/Boolean types are not coerced;
finite profiles use exact enum strings and integer indices. Decimal/rational
meanings need explicit representations. Generic `core.loads` binary-float parsing
is an existing separate limit; these APIs do not claim to recover already rounded
numeric tokens. `normalizer.canonical_json` retains raw JSON number tokens.

`sharing.toolkit()` and `sharing.dispatch_tool()` expose the six `zal_lens_*`
finite functions plus `zal_library_lenses` and `zal_library_register_lens`.
The entry-bound `zal_library_lens_propose`, `zal_library_lens_apply`,
`zal_library_lens_check`, `zal_library_lens_verify` and `zal_library_lens_restore`
functions are also dispatched by that same pure toolkit.
These are pure local Python/JSON functions. This port does not install an MCP
service or the upstream CLI/browser. No shown Python API is an installed shell
command. General structural authoring also works without the optional finite
adapter; unavailable finite capabilities are not silently replaced.

The [language study](https://www.nature.com/articles/s41562-025-02387-z) motivates
considering sequences/chunks alongside trees and graphs. Its evidence concerns
nonhierarchical structures in English, not the correctness of ZAL. Coexisting
viewpoints are an engineering choice. The product name Lens does not establish
bidirectional update laws. Finite tests, source identities and replay are local
provenance and bounded evidence, not hostile-host attestation, theorem-kernel
authority, human agreement, application admission or release approval.
