# Optional category Lens host

This host adds endpoint-typed symbolic composition and exact finite category,
functor and inverse checks to existing abstraction-library entries. It reuses the
existing notation tree and finite library; one retained Haskell provider lives
under `provider/`. No compiler, worker, service or profile is installed from request
data. The default runtime is **Unavailable** until a trusted operator approves one.

The actual four `zal_library_category_prepare/check/verify/audit` routes appear in
`zal_notation.sharing.toolkit()` and use its real prefix dispatch. Runtime/evidence
approvals are passed as a local `category_host` operator context, never JSON fields.
The generic `typed-category` hint stays Pending; it cannot silently activate the finite Mealy adapter
or establish applicability from its name.

```
python3 -I -B integrations/zal/category_profile/__main__.py toolkit
python3 -I -B integrations/zal/category_profile/__main__.py call REQUEST.json
python3 -I -B integrations/zal/category_profile/__main__.py call REQUEST.json --runtime-config OPERATOR.json
python3 -I -B integrations/zal/category_profile/__main__.py call AUDIT.json --evidence-catalog CATALOG.json --evidence-sha256 SHA256
```

A call is exactly `{tool,arguments}`. The toolkit lists exact argument schemas.
Prepare accepts library, entry_digest, operation, parameters, interpretation and an
optional work budget in0..1000000. Parameters contain categories/functors/
relationships/selection; operation selects one of the six fixed provider actions.
Native source_context is rebuilt from the actual full source entry/context,
interpretation and library identities. Source/entry/request swaps invalidate replay.
The application retains the complete typed entry. Its inert native source envelope
uses `zal/canonical-json-text-chunks/1`, chunks of at most2048 characters and the
entry digest; concatenating them equals `core.canonical(entry)` exactly. This lets
valid fractional drawing coordinates remain presentation data without widening the
mathematical protocol. Parsed number types/values remain distinct; original JSON
number lexemes cannot be recovered after parsing. Existing128KiB request and
depth/node limits still apply; no source is truncated to fit them.

For Python, import `category_profile.host`, call `load_adapter()` once and construct
its frozen `RuntimeApproval`/`EvidenceApproval`, or use
`host.operator_context(runtime_config=...,evidence_catalog=...,evidence_sha256=...)`.
Pass that `host.OperatorContext` through trusted `category_host=` in
`sharing.dispatch_tool`. There is no JSON equivalent or default executable path.

An operator runtime approval file has exactly:

```
{"schema":"zal/category-runtime-approval/1","directory":"/approved/external/runtime",
 "provider_manifest_sha256":"...","binary_sha256":"...","build_record_sha256":"..."}
```

Assess actual build provenance before approving it. Provider/source inventory must
match the shipped capsule. The host snapshots immutable path/pins, checks actual
files before execution and returned identities afterward. Hashes identify content;
they do not attest a compiler/host or authenticate author intent.

An evidence file has schema `zal/category-evidence-approval/1`, records (canonical
adapter-record digests), binary_sha256, build_record_sha256 and
provider_manifest_sha256. Supply its approved raw SHA256 as an operator option.
Static audit freshly checks table results/witnesses using the captured independent
oracle, while labeling native evidence as prior recorded. It launches no worker.
Imported statuses/catalogs cannot enroll themselves.

The actual generic viewer accepts up to16 `category_operations` rows, each exactly
`{entry_digest,application,record}`. Supply its operator approval separately:

```
PYTHONPATH=integrations/zal python3 -B -m zal_notation.library_lens_browser export REQUEST.json NEWDIR --category-evidence-catalog OPERATOR.json --category-evidence-sha256 APPROVED_SHA256
PYTHONPATH=integrations/zal python3 -B -m zal_notation.library_lens_browser replay NEWDIR --request-sha256 REQUEST_SHA256 --category-evidence-catalog OPERATOR.json --category-evidence-sha256 APPROVED_SHA256
```

Python `export`/`replay` take an optional trusted `category_host=OperatorContext`.
No worker runs during static export/replay; the packet's approval digest identifies
the separately supplied evidence and cannot configure or approve itself. Missing
approval refuses category attachments before output. The viewer exposes typed
table composition/inverse queries, functor mappings/fibers and authored symbolic
hops. These query recorded data; they do not execute the original source program.
Recorded result, fresh static validation and source interpretation remain separate.

`inspect_incomplete_record` is a fixed bridge-only provenance path, outside the four
tool schemas. It admits the closed source-pinned semantic-budget UNKNOWN format
with exact work accounting and no result/witness, producing RecordedIncomplete.
It checks only approved prior bytes/envelope/input/source/worker binding and never
calls the table oracle or claims laws/completion. Other incomplete/transport formats
are refused as attachments. Ordinary `audit_record` remains completed-result-only;
an Audited refutation stays RefutedAt and authored symbolic composition stays
Authored. Whole-request failures, including invalid unselected declarations, are
shown separately from the selected source's exact representation.

`prepare-runtime NEWDIR` copies the ten pinned provider files into a fresh external
directory and builds nothing. An operator can separately run the retained trusted
`build.py NEW_BUILT_RUNTIME` with its required GHC9.4.7/packages, inspect the record,
and approve it. No dependency fetch or compiler runs implicitly. Existing approved
byte-identical external runtimes may be reused with prior provenance. Historical
command paths in records are inert data, not product dependencies.

The source profile `zal/finite-category-source/1` uses exact Category/Object/Arrow/
Identity/Compose constructors. It reconstructs every actual row and requires a
complete model/payload match, not an ID/profile label. Its512-node source bound
refuses larger declarations without truncation. Unknown formal profiles or changed
payload/observation stay Unsupported. ExactRepresentation is declaration
correspondence only; finite laws still require checking.

Authored interpretations and hop preservation/loss prose stay separate from finite
CheckedAt evidence. Source draft/open questions remain. RefutedAt covers the whole
request, including invalid unselected declarations. Typed Haskell composition
constrains syntax; it does not prove laws, source semantics, inversion or termination.
No get/put optic laws are imposed.

General symbolic/custom/informal entries may describe infinite objects now. Finite
presentation does not require a finite mathematical carrier. This optional checker
covers explicit bounded tables. A future theorem-backed profile must bind the exact
statement/interpretation/assumptions, source/context and checked proof artifact;
types, laziness or bounded exhaustion do not establish universal claims. No such
theorem service is installed by this host.

Inspect JSON status/scope. CLI0 means completed preparation/discovery/authored data
or a scoped positive;1 means RefutedAt or failed verification;2 means Unavailable,
Unsupported,Invalid,UNKNOWN orError. A true negative/UNKNOWN replay stays negative
or incomplete. Provider caps/caveats remain in its retained API; no whole
implementation proof, application authority or release qualification follows.
