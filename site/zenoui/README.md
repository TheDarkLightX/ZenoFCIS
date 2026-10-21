# ZenoUI presentation core

The LLM-generated interface has three boundaries: the application Authority
makes domain decisions; a pure presentation core emits owned view data and
request intents; the browser carries out those intents and draws the view.
The finite contracts use the existing `.zeno` and V2 policy compiler.

`public/zenoui/semantic.js` and `inventory.semantic.json` retain the
`zenoui/semantic-contract/1` projection. Reserve and release add receiving-field
capacity guards found by full-domain comparison with the actual 2.5 Authority.
Restock retains the prototype's conservative total-stock guard. Accepted
restocks with available + quantity ≤ 5 but available + reserved + quantity > 5
are omitted. The check retains a positive witness for every action.

`public/zenoui/presentation.js` owns phase, draft, run identity, progress,
selection and received evidence. Start, Progress, Result, Select, CellResult,
Fail, Cancel and PreferencesChanged are explicit events; DraftChanged captures
form edits before Start. Epoch, run and request are exact decimal strings.
Cancel and transport failure fence later replies; every selection gets a new
request. Stale replies return the identical state and no intents.

The lifecycle contract checks 55,296 tuples: four phases, three selection
states, sixteen region masks, nine events and five Boolean predicates. These
predicates are epoch/run/request/ordinal equality and readiness. The mounted
JavaScript control function is compared with every library decision. Equality
of concrete identities and owned payload refusal are separately tested.
The claims contract checks 96 tuples; the advisory enablement contract checks
eight. The inventory projection is compared with every one of the real
Authority's 864 inputs, all tasks and busy/fault contexts. This is finite
checking over those declared domains, never a browser proof.

A missing-region, stale-epoch, false-claim and false-enable policy must compile
and bind, then fail an actual library law. Preference interference, colliding
geometry, missing units, inexact wide integers and false enablement have
executable presentation controls. Geometry is data in `factory-layout.js`;
coverage, intersections, bounds and drawing/hit-test inversion are checked.

State and returned graphs are recursively frozen; payloads and draft inputs
are copied on entry. Internal mutation builds fresh exclusively owned output.
The browser retains DOM, worker, timer and promise handles only. It supplies
elapsed time as data and cannot add application authority. No presentation
state is a publication, receipt, durable store or external-effect claim.

Run `python3 site/tests/presentation_check.py` after the site's module and native
stages. It writes source hashes, complete vectors and named control witnesses
under `site/tests/`; those generated records stay out of the public artifact.
Laws 3–5 have tested evidence. Browser raster, interaction and accessibility
checks remain separate from this pure and finite evidence.
