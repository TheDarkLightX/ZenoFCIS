# ZenoUI presentation core

The interface has three boundaries: the application Authority
makes domain decisions; a pure presentation core emits owned view data and
request intents; the browser carries out those intents and draws the view. The
finite contracts use the existing `.zeno` and V2 policy compiler.

`site/public/zenoui/semantic.js` and
`site/public/zenoui/inventory.semantic.json` retain the
`zenoui/semantic-contract/1` projection. Reserve and release add receiving-field
capacity guards found by full-domain comparison with the actual 2.5 Authority.
Restock retains the prototype's conservative total-stock guard. Accepted
restocks with available + quantity ≤ 5 but available + reserved + quantity > 5
are omitted (41 of the 244 accepted inventory inputs). The check retains a
positive witness for every action. This prototype projection is a separate
subject; the factory page does not expose these four inventory command buttons.

`site/public/zenoui/presentation.js` owns phase, draft, run identity, progress,
selection and received evidence. Start, Progress, Result, Select, CellResult,
Fail, Cancel and PreferencesChanged are explicit events; DraftChanged captures
form edits in every phase. The current draft governs button eligibility and
input feedback; each Start owns a separate immutable submitted snapshot for its
active request. Later edits cannot change that snapshot or its result binding.
Epoch, run and request are exact decimal strings. Cancel and transport failure
fence later replies; every selection gets a new request. Stale replies return
the identical state and no intents.

The lifecycle contract checks 55,296 tuples: four phases, three selection
states, sixteen region masks, nine events and five Boolean predicates. These
predicates are epoch/run/request/ordinal equality and readiness. The mounted
JavaScript control function is compared with every library decision. Equality of
concrete identities and owned payload refusal are separately tested. The claims
contract checks 96 tuples; the advisory enablement contract checks eight. These
are finite checks of predicates and control. Result admission is separately
tested: fixed task domains, complete ordered search/grid coverage, consistent
passing counts and candidates, and submitted input/variant binding precede any
completed evidence. Every result field the page draws, including its technical
record, is checked before a run is done, and an inventory result's cell
decisions, difference flags, counts, status and ringed witness must agree. If drawing an admitted reply still fails, the page
undoes that reply and fails the run or the selected cell: the worker stops, the
run's promise is rejected and nothing is marked complete. These structural
checks trust the worker's computations; they do not authenticate or
independently replay its payload.

The separate inventory prototype projection is compared with every one of the
real Authority's 864 inputs, all tasks and busy/fault contexts (17,280
contexts). Its 41 Restock omissions and positive command witnesses do not
describe the factory's run buttons. The actual factory controls are tested
through the shipped page module against real Wasm admission, including money
bounds, busy state, edits during a run, refusal feedback and recovery. This
bounded mounted test is not an exhaustive check of all money input combinations
or a browser proof.

A missing-region, stale-epoch, false-claim and false-enable policy must compile
and bind, then fail an actual library law. Preference interference, colliding
geometry, missing units, inexact wide integers and false enablement have
executable presentation controls. Geometry is data in `factory-layout.js`;
coverage, intersections, bounds and drawing/hit-test inversion are checked.

State, bound descriptor policy and view graphs are recursively frozen;
payloads and draft inputs are copied on entry. Internal mutation builds fresh
exclusively owned output. The browser retains DOM, worker, timer and promise
handles only. It supplies elapsed time as data and cannot add application
authority. No presentation state is a publication, receipt, durable store or
external-effect claim.

Run `python3 site/tests/presentation_check.py` after the site's module and
native stages. It writes source hashes, complete vectors and named control
witnesses under `site/tests/`; those generated records stay out of the public
artifact. Laws 3–5 have tested evidence. Law 6 has a finite predicate and tested
payload binding; law 7 has a finite implication, the separate exhaustive
prototype projection and tested mounted factory controls. The finite predicates
alone do not establish the corresponding payload or form behavior. Browser
raster, interaction and accessibility checks remain separate from this pure and
finite evidence.

## Generated contract applications

`tools/zenoui.py` builds an interface from `project.zeno`, `v2/policy.json` and
`ui.json`. The native compiler supplies exact fields, finite domains, commands
and state. Strict metadata supplies labels, units and refusal messages; missing
metadata fails with a named error. Wide integers remain decimal strings.

The checked builder supports flat finite standalone V2 contracts with an enum
command discriminator and at most 2,000,000 input tuples. Unsupported or evolved
contracts fail closed. It checks laws and the real native core over the declared
domain, then checks the mounted worker and Wasm before emitting shared modules,
components, styles and data. Each command gets a form; the core rechecks a
confirmed request before deciding. Consent binds the command, fields, units,
principal, contract and revision, and an edit voids it. The local principal and
context values are not authenticated credentials. Session state is not durable.

After building the native `ui_contract` executable and the site's Wasm module:

```sh
python3 tools/zenoui.py CONTRACT --out "$TMPDIR/zenoui-app" --evidence "$TMPDIR/zenoui-evidence"
python3 tools/check_zenoui.py --out "$TMPDIR/zenoui-checks"
```

The three-template check compares withdrawal-queue, inventory-reservation and
wide-counter against their real native decisions and checks byte-for-byte
regeneration. This is separate from the older prototype projection above.
[Browser checks](../tests/SOCKET_FREE.md) cover actual keyboard, pointer,
accessibility and layout behavior when run on a host that can launch Chrome.
Finite and DOM-adapter checks alone do not establish those browser properties.
