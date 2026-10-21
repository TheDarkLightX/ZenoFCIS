# Browser checks without an HTTP listener

The browser runner checks the built public artifact through Chrome's DevTools
pipe. `socket_free_check.py` builds a SHA-256 manifest, and the runner fulfils
requests from a checked in-memory snapshot at `https://zenoui.test`. Unknown
routes, external origins and methods other than GET are refused. No HTTP
listener is opened. Chrome still needs permission for its own internal IPC.

Run after the site's module and native build stages:

```sh
python3 site/tests/socket_free_check.py --out "$TMPDIR/zenoui-browser"
```

The factory check runs the real page's search action, requires a module worker
and Wasm fetch, compares the complete result with the native oracle, and checks
unknown and external fetch refusals. Outputs include `assets.json` (file hashes)
and `browser.json` (results, fulfilled and denied requests, and Chrome stderr).
On failure, `failure` retains the assertion's expected and actual values. The
generated-app matrix also records its current configuration, predicate, focus,
invalid controls and confirmation text when those observations are available.

The generated-app matrix consumes the checked generator's evidence directory:

```sh
python3 tools/check_zenoui_browser.py --root "$TMPDIR/zenoui-checks" --out "$TMPDIR/zenoui-matrix"
```

It checks three templates at two widths, two themes and two motion settings,
plus 320-pixel reflow and 200% CSS zoom. Keyboard submission uses DevTools key
events, including Enter's character data, rather than scripted DOM clicks.
Keyboard and pointer journeys, consent invalidation, accessible names, state
parity, error recovery, screenshots and performance budgets are separate from
the generator's finite-domain checks. CSS zoom does not replace a manual
browser-menu zoom or screen-reader audit.

The launcher preserves standard IO and DevTools channels 3/4 and closes higher
inherited descriptors before executing Chrome. It uses a short temporary
profile path independently of `TMPDIR`. Only page sessions enable Fetch;
flattened auto-attachment resumes module workers while the page intercepts
requests. A host restricting Chrome's internal sockets can fail before
DevTools starts, for example with `setsockopt: Operation not permitted`. Such a
failure supplies no browser layout, interaction or parity evidence. Run on a
host that permits Chrome's internal IPC; disabling the browser sandbox does
not establish that permission.

Offline checks:

```sh
node site/tests/asset-manifest.test.mjs
node --experimental-vm-modules site/tests/factory-readiness.test.mjs
node site/tests/browser-launcher.test.mjs
python3 -B -m unittest discover -s site/tests -p test_socket_free_launcher.py
node site/tests/browser-checks.test.mjs
node site/tests/pipe-browser.test.mjs
node site/tests/factory-width.test.mjs
```

The manifest test covers ownership, hash tampering, route refusal and changes
after load. Readiness executes the shipped page, worker and Wasm with a small
DOM adapter. The pipe-runner test uses a scripted protocol peer, checks routing
and manifest fulfilment, and plants a nested native-result mismatch. These
checks do not establish real Chrome execution or keyboard activation.

The width check computes region minimums against the same geometry data that
generates `factory-layout.css`. Its `--write` option regenerates that file.
Oversized regions and intrinsic ledger columns must fail. This structural check
does not establish browser font metrics, layout, raster or reflow.
