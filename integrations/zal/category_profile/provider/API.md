# Optional Haskell Lens provider

Profile `zal/lens-category/1` extends the included finite Mealy profile with explicit
multi-object categories and checked functors. It is an optional construction
profile; general mathematical objects, informal relationships and other
interpretations remain authorable independently. No native registry/app/release
qualification follows from this provider alone.

Trusted Haskell provides `Chain a b`, `named`, and `thenChain`:

```haskell
left  :: Chain "Program" "Graph"
right :: Chain "Graph" "Category"
combined :: Chain "Program" "Category"
combined = thenChain left right
```

The GADT's intermediate index must match. Runtime data reifies context identities
and uses type equality before joining. Type checking establishes well-formed
composition syntax. It does not establish the denotation of named relations,
invertibility, category laws, arbitrary code termination or bidirectional get/put
laws. No arbitrary Haskell, plugin, expression evaluator or imports come from
requests. The core has no IO, persistent state, cache or external effect.

All requests contain exactly:

```
schema, operation, source_context, categories, functors, relationships,
selection, budget
```

`schema` is `zal/lens-category/1`; `budget` is an integer in 0..1000000.
`source_context` is retained inert data, not an authenticated principal or
certified interpretation. Exact nonintegral values are authored as strings;
the optional ingress refuses floating-point JSON tokens rather than rounding
mathematical context identities. This restriction does not constrain ZAL's
general authoring language.

| Operation | Exact selection fields | Result scope |
| --- | --- | --- |
| check-category | category | Complete explicit table satisfies typed closure, two identity laws and associativity. |
| compose-path | category, path | Nonempty compatible arrow path's composite in that checked category. |
| invertible | category, arrow | A two-sided inverse in the complete supplied arrow set, or a finite refutation. |
| check-functor | functor | Total object/arrow mapping preserves endpoints, identities and composition. |
| compose-functors | path | Compatible checked functors compose; resulting mapping is checked again. |
| compose-symbolic | path | `Authored` symbolic chain with exact intermediate-context agreement; no semantic theorem. |

A category is `{id, objects, arrows, identities, composition}`. Objects are
distinct text IDs; arrows are `{id,source,target}`. Identities are `[object,arrow]`
pairs. Composition entries are `[before,after,result]`, meaning `after ∘ before`.
The profile requires 1..8 objects and 1..48 arrows, unique IDs, exactly one
identity per object and exactly one result for every composable ordered pair.
All declared categories/functors are checked, including unselected declarations.
Empty categories are outside this particular profile, not mathematically invalid
in general. Noncomposable declared entries, false identities/associativity and
missing declared closure carry witnesses; malformed references/duplicates refuse.

A functor has `{id,source,target,objects,arrows,assumptions,preserves,forgets}`.
`source`/`target` select complete category declarations in this request; no
category IDs may be duplicated. The two mapping fields are total `[from,to]`
pairs. Each input declaration retains its three text lists. Successful finite
functor checks certify only categorical table relationships, not the authored
semantic claims in those lists. Object/arrow fibers disclose merged distinctions.
Compositions retain every hop and concatenate cumulative losses in order;
preservation texts are not silently promoted to a transitive semantic theorem.

A general symbolic relationship has `{id,source,target,assumptions,preserves,
forgets}`. Each endpoint is a full `{id,object,observation,assumptions}` context.
The ingress hashes the whole endpoint into an internal typed index; matching
labels with changed observations or assumptions do not match. Original endpoint
objects and every hop's declarations are retained. Symbolic composition is
`Authored` even when it type-checks. Custom source interpretations are never
converted to checked categorical evidence just because their names align.

No source restoration is implemented here. The finite Mealy adapter retrieves its
retained original; it does not invert a quotient. A collision refutes a specific
permutation claim, not every possible group/units/symmetry interpretation. This
backend's inverse result refers only to the given category and complete arrow set.

## Trusted execution and evidence

These provider-internal commands apply after the host has prepared a source
capsule named `source/`; from the repository use the prepare-runtime and
operator-approval instructions in [the host reference](../README.md). Run the
source-captured standalone loader:

```
python3 -I -B source/build.py NEW_RUNTIME
python3 -I -B source/main.py run NEW_RUNTIME REQUEST.json
python3 -I -B source/main.py verify NEW_RUNTIME RECEIPT.json
```

Build and qualification are separate operator actions. The builder first captures
all pinned source bytes into a fresh runtime, then invokes GHC9.4.7 with installed
pinned base/containers/aeson/text/bytestring. No solver, network or package fetch.
It compiles the captured bytes, records exact command/source/binary hashes and
checks source stability. The ingress checks its sealed manifest, loads captured
Python/oracle bytes, checks runtime source inventory/build/binary before and after
one worker process, and uses the captured launcher bytes. Receipt verification
recomputes the actual native result and compares exact canonical JSON, including
input/context/source/binary bindings. Imported statuses/hashes are not evidence.

The independent oracle directly checks explicit tables/witnesses. It does not
import the Haskell checker or finite Mealy library. It is scoped test/response
verification code, not another vendored semantic package. Source hashes are
reproducibility identities, not hostile-host protection, compiler attestation,
authenticity, human semantic approval or a source-level correctness proof.

Ingress limits are 128KiB raw UTF-8, 48 JSON levels, 30000 data nodes, bounded
strings, at most 4 categories/16 functors/32 symbolic links and 32 selected hops.
Strict ingress rejects duplicate fields and Boolean/integer aliases. One worker
has 1GiB address space, 512MiB Haskell heap, 10 CPU seconds, 12 wall seconds,
1MiB output files and no core dump. These are process caps, not a hostile sandbox.

`CheckedAt`, `RefutedAt`, `Authored`, `Invalid`, `UNKNOWN` and `Error` stay distinct.
Every semantic visit charged by the Haskell checker retains work used on success,
refutation and exhaustion; the refused next visit is not counted as performed.
Decoding, allocation, process setup and the separately capped independent oracle
are outside that work metric. Exhaustion/timeouts/interruption have no completed
result. A finite positive cannot stand for an unspecified or truncated search.
The general Haskell implementation/refinement proof remains open.

Design references include the [finite Lens reference](../../zal_notation/LENSES.md),
GHC9.4.7 GADT documentation,
and the [language study](https://www.nature.com/articles/s41562-025-02387-z).
The latter motivates coexisting nonhierarchical linguistic structures; using it
to motivate multiple views is an engineering inference, not a correctness proof
of this backend. The included sources and manifests, rather than an external
research packet, define this provider capsule.
