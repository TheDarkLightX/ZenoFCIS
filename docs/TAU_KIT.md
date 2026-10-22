# Optional finite Tau kit

`contract import --tau` translates a complete pointwise Tau decision ladder
through the native contract compiler. `contract export --tau` emits per-rule
checks and deterministic samples of real library Authority decisions. Export
does not execute Tau or SMT. The kit is optional and runs locally; it adds no
runtime dependency or authority.

The CLI package contains `tau/importer.py` and `tau/runner.py`, which it embeds
for import/export. The standalone copies under `integrations/tau` are byte
identical mirrors checked by the acceptance scenario, including a real Cargo
package inventory check; updates must preserve that equality.

```
zeno-fcis contract import --tau integrations/tau/finite-ladder.tau --out app
zeno-fcis contract review app --out review.json
zeno-fcis contract export --tau app --out tau-kit --samples 64
python3 tau-kit/runner.py tau-kit/queries.json \
  --tau /path/to/tau --tau-sha256 TAU_SHA256 \
  --solver /path/to/z3 --solver-sha256 Z3_SHA256 --solver-kind z3 \
  --out tau-run --jobs 1 --timeout 60 --memory-mb 768
```

Import requires local Python 3 and a host that can enforce the parser's
256 MiB address-space limit. Import and source-correspondence export are
qualified on Linux. On the macOS release host, Python cannot establish that
limit; these operations refuse without creating an output directory.

The supported source has flat scalar records,
exactly `i1`, `i2`, `i3`, `o1` and `o2` console streams, one `always` ladder,
current `[t]` projections, same-width unsigned comparisons, Boolean operations,
and modular addition/subtraction. Every leaf assigns all state outputs and
literal decision `0` or `1`. A zero decision copies every old state field.
Temporal history/future, additional streams or control, quantifiers, numeric
conditionals, multiplication/division, mixed widths, output-dependent guards
and incomplete/duplicate output assignments refuse. The native program alone
executes the imported policy; Python only parses and serializes it.

Widths `1..16` are supported. A larger source width refuses unless explicitly
narrowed, for example `--scale-width 256:3`. A scaled maximum maps to the new
maximum; every other constant must fit. This changes the domain and is labelled
`explicitly-scaled`, with no unscaled equivalence claim. Optional input limits,
such as `--domain i2.kind:0:3 --domain i3:0:1`, restrict command/context inputs;
state fields retain the entire bitvector domain to preserve successor semantics.
Import adds zero genesis, diagnostic reject reasons and a native state-domain
law. The context's authentication is a host assumption.

The opt-in `--profile allowance-study-bv3` accepts the exact allowance AST in
`integrations/tau/allowance-study.tau`, with explicit `--scale-width 256:3`,
or its bare seven-input/four-output scalar layout at width 3 or 8. Flat inputs
are available, reserved, spent, kind, amount, actual and supplied authority;
outputs are the three successor amounts and the decision bit. Every amount is
restricted to `0..7`, authority to `0..1`, and kind to exactly four codes.
Source kind `0..3` maps to the closed native enum `140..143`; width-8 source
codes `140..143` use identity mapping. The parsed guards and successor ASTs
generate all native decisions. Changed guards, posts or stream roles refuse.

This explicit profile also supplies zero genesis, seven ordered diagnostic
reasons and four laws: ledger bound, conservation with grant delta, supplied
authority with monotone spending, and exact finalize accounting. These are
declared annotations beyond the Tau decision bit and successor, not inferred
authentication. Guard and finite-domain constraints exclude wrap in every
selected accepting successor; no equivalence outside this domain or for
unscaled 256-bit inputs is claimed. Original-source comparisons retain the
exact unsigned modular AST and source/native enum conversion. The default
importer retains its general modular ladder behavior.

```
zeno-fcis contract import --tau integrations/tau/allowance-study.tau \
  --profile allowance-study-bv3 --scale-width 256:3 --out allowance-app
```

Export supports nonnegative scalar input codes up to 65535, scalar state and
payload observations, and fixed delivery metadata. Signed i128 constants and
intermediates use explicit two's-complement `bv[128]` encoding and signed order.
It independently bounds every eager native node and every expanded authored
expression, including unused nodes and unselected arms; a possible i128 overflow
refuses. Unsupported instructions, dynamic text observations and adopted/evolved
lineages refuse. Numeric conditionals become universally quantified helpers
with exact conditional equalities. The paired SMT script is the negation of the
same property under the same admitted input domains. Compiler, framing, query
translation and external tool implementations remain trusted assumptions.

The exported directory contains `queries.json`, `mapping.json`,
`decisions.json` and `runner.py`. Imported applications retain their parsed
source identity and scaling in `v2/tau-import.json`; export refuses stale import
metadata and includes original-source per-rule and decision correspondence checks. A bounded
sample does not establish whole-domain Tau replay or reachability.

The decision sample uses ordinal `floor(k * input_count / sample_count)`.
This can miss fast-varying context values in a power-of-two domain. For a
complete small-domain decision corpus, request at least as many samples as
inputs, up to the 10000-sample cap. Every case still has its independent
per-rule symbolic query; no sampled coverage claim replaces that obligation.

An exported kit contains at most 19,999 queries, reserving one of the runner's
20,000 slots for its mandatory false-T diagnostic. Tau formulas are limited to
131,064 UTF-8 bytes so the `valid ` command fits Linux's single-argument limit;
paired SMT text retains its separate 1 MiB cap. The supported pinned Tau file
interface runs specifications, and its stdin REPL adds prompts, so neither is
a substitute for the kit's strict single-verdict command transport.

The runner accepts at most four concurrent tool processes, applies per-process
time, memory and output caps, checks configured executable hashes, and retains
both transcripts plus separate Tau/SMT input hashes. Run one job on constrained
hosts. Query periods are refused before execution; source statement periods are
allowed. A tool transcript must contain exactly one supported verdict line;
UNKNOWN, errors, extra diagnostics and mixed verdicts cannot be hidden beside
a `T` or `unsat`. SMT input is parsed as bounded ASCII top-level commands, using only
the kit's logic, scalar declarations, definitions and assertions, with exactly
one terminal `check-sat`. Echo, exit and output redirection refuse before tools
run. Every expected `T` needs Tau `T` and exact paired SMT `unsat`; every
planted `F` needs Tau `F` and paired SMT `sat`. The alias/order false-T diagnostic
is always retained. A surviving control, unexpected result, disagreement,
UNKNOWN, tool error, timeout, native refusal or changed tool identity leaves
the run incomplete. `corroborated` refers to these queries only and is attested
evidence, never `KernelChecked`, authentication or authorization.

The runner passes `--benchmarks false` to Tau so timing diagnostics cannot mix
with its verdict. Its current supported tool profile uses Tau `92b76017`, which
provides this option. The older `b647e787` source lacks the option; its launch
compatibility is unqualified. Unexpected tool output still refuses, including
timing lines before a verdict.

Tau normalization can take minutes even on a finite query. Per-case timeouts
are part of the evidence: increasing a timeout does not qualify an unanswered
query, and stream execution is outside this kit's supported profile.

The acceptance scenario `tau-finite-kit` checks native import/export and parser
protocols without requiring external Tau or SMT in hosted CI. External tool
qualification requires the separately retained local run above; an unrun kit
and passing protocol fixtures do not supply it.
