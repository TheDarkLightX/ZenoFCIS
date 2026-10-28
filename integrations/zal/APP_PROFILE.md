# Explicit app input bindings for finite ZAL

`app_profile.py` is an opt-in adapter for the existing `zal/finite-fsm/2`
model. The opt-in `zal/finite-fsm/3` authoring/export extension has source-level
Python checks, but the current native app provenance gate admits
`zal/finite-fsm/2` only. A `/3` app bundle must refuse there until that separate
native gate and complete app qualification are extended. Source/export checks
do not establish native app admission.

It preserves the legacy `factory.lower(model)` route, which treats every
model fact and numeric input as context. This adapter lets the author state
which observations are caller command intent and which must arrive from the
server. It emits declarations, rules, presentation metadata and comparison
fixtures; it does not synthesize missing business requirements, authenticate a
principal, bind a runtime program, or authorize publication.

Supply the original ASCII ZAL source and UTF-8 JSON binding. Both are limited to
64 KiB. The direct adapter rejects oversized source and binding character counts
before encoding, then retains their byte and Unicode validation. The binding's
`model_revision` must equal
`behavior2.parse(source).revision`. Every declared fact and numeric input occurs
exactly once in `observations`; unknown, duplicate, missing or wrongly typed
entries refuse. No name or label is used to infer authorization. A `command`
source must have role `intent`. A `server-context` source must have role
`observation`, `authorization` or `identity`. This explicitly records the
owner's declared meaning; it does not prove the declaration is correct.

For the two-state example below the full binding has this shape:

```json
{
  "format": "zal/app-binding/1",
  "model_revision": "b29ff655932cedd14d303d20b11b4c2170f01430478a3594bd1a6ad7279aa554",
  "observations": [
    {"name":"authorized","kind":"fact","source":"server-context","role":"authorization","label":"Operator permission","unit":"unitless"},
    {"name":"quantity","kind":"input","source":"command","role":"intent","label":"Quantity","unit":"items"}
  ],
  "ui": {
    "title":"Repair request",
    "action":{"label":"Action","unit":"unitless"},
    "control":{"label":"Request state","unit":"unitless"},
    "variables":{"units":{"label":"Requested units","unit":"items"}},
    "states":{"draft":"Draft","submitted":"Submitted"},
    "events":{"request":{"label":"Request","consequential":true},"reset":{"label":"Reset","consequential":true}},
    "reasons":{"denied":"Operator permission is required.","not_enabled":"This action is not available."}
  }
}
```

The revision above applies only to the exact semantic model in
`test_app_profile.py:SOURCE`. For another model, enumerate its exact names and
supply its revision, state and variable labels, event labels/consequences, and
all declared/default rejection messages. Presentation metadata is explicitly
authored; the adapter does not guess domain semantics from field names.

For a grouped action selector, explicitly add this optional object inside `ui`:

```json
"presentation": {"format":"zal/app-presentation/1","commands":"grouped"}
```

This closed intent selects `zenoui/metadata/2` with conservative
`zenoui/requirements/2`: every existing command observation remains a primary
editable input in stable field order, every server-context observation (including
the otherwise empty unit context) remains context, and commands share one selected
form. The profile requests `groupCommands: true`, `maxVisibleForms: 1`, compact
presentation, collapsed expert panels, and at most two columns. It adds no hidden,
fixed or detail input, command subset, projection, quote override or guessed
relevance. Unknown intent formats, extra keys and other command presentation
values refuse. Absence preserves the original metadata/1 and output bytes.

Binding format remains `zal/app-binding/1`; the original complete source and the
newly authored complete binding are retained. Fresh lowering records their actual
raw/canonical hashes and the new UI hash without changing project/policy, model
revision, ABI, laws, cases, reasons, examples or correspondence. Do not replace
ui.json under an old source map or remove the five-file provenance bundle to
bypass a native refusal. The native provenance gate itself is unchanged.

Selection is not consent. The grouped UI still needs its separate presentation
checker and renderer qualification, per-command drafts/preview, Review and any
declared consequential confirmation. For the reviewed repair example, source
Tab order is Action selector, quantity, Review; choosing another action does not
dispatch or automatically switch selection on a reply. This exporter emits
presentation requirements, not an AST, search optimum, authentication proof or
runtime/browser qualification. This app profile does not constrain general ZAL
mathematical or informal abstractions.

```sh
python3 -B integrations/zal/app_profile.py \
  --source path/to/model.zal --binding path/to/binding.json --out /tmp/my-contract
```

The output directory must be absent or empty. The result includes
`project.zeno`, `v2/policy.json`, `ui.json`, the original ZAL and binding text,
`zal-source-map.json`, independently interpreted finite decision examples and
a generated Rust Authority correspondence test. The source map records raw
source/binding hashes, model revision, canonical binding hash, emitted artifact
hashes, exact root/field paths, role declarations and input ABI order. Event
and state variants retain stable IDs; the optional app profile uses a command
record with action field `119`, event data type `103` and original observation
field IDs. An otherwise empty context uses unit type `104` and field `159`.
The original model's tuple bounds, decision ordering, refusal reasons, laws and
empty-delivery scope are retained. The normal legacy export is unaffected.

The app adapter's CLI writes exact UTF-8 bytes, including literal non-ASCII
binding labels and original source/binding CRLF newlines on every platform.
Every generated string is encoded before any output file or directory is
created, so an encoding refusal leaves an absent or empty destination intact.
This is encoding preflight, not a directory transaction for arbitrary I/O
failures. The shared writer's default legacy ASCII text and platform newline
behavior remain available; this CLI explicitly selects exact UTF-8 output.

Native `zeno-fcis new --contract` retains a declared app bundle as inert data
under `provenance/zal/`, using the same relative names: `zal-original.zal`,
`zal-app-binding.json`, `zal-source-map.json`,
`tests/zal-authority-examples.txt` and `tests/zal_correspondence.rs`.
Binding presence or a source-map `/3` declaration selects the strict five-file
rule; ordinary contracts and legacy finite `/1` or `/2` exports without a
binding keep their previous generation behavior.

The scaffold validates raw source/binding/reference hashes, agreement of
declared revisions, and project/policy/UI hashes against its already captured
executable inputs before writing. The three provenance metadata/source files
are limited to 64 KiB; every file also uses the native regular-file reader's
16 MiB limit. Retention preserves UTF-8/CRLF bytes, publishes nothing under
`www`, and does not add a Cargo test target. The app factory's final file
manifest includes these private data files. Matching metadata and hashes does
not prove ZAL semantics, model revision derivation, native correspondence or
authenticated host configuration; semantic lowering and native checks remain
separate. Ordinary `generate contract` and review paths do not validate these
app-only auxiliary files.

A host using an authenticated durable app must establish server-context values
from its own trusted principal configuration. The command fields are the only
caller-editable intent fields; the app's server-session context is locked by the
host interface. A local mock context is not authentication, and calling this
adapter does not verify the configuration or any physical effect. A generated
UI, runtime or browser is separately qualified. The `/2` finite model and this
adapter do not delimit general ZAL. Unsupported profiles, unresolved model
obligations, out-of-range inputs and unrepresentable features must refuse rather
than be silently dropped.
