# Socket-free browser spike

`socket_free_check.py` builds a SHA-256 manifest of the exact public artifact.
The Node runner launches Chrome through DevTools pipe file descriptors 3/4,
pauses new page and module-worker targets, and fulfils requests from a checked
in-memory asset snapshot at `https://zenoui.test`. Unknown routes, external
origins and methods other than GET are refused. It opens no HTTP listener.
The intended positive check runs the real factory page's search act, requires
a module worker and Wasm fetch, compares the complete result with the native
oracle, then checks unknown and external fetch refusals.

Run after the module and native build stages:

```sh
python3 site/tests/socket_free_check.py --out "$TMPDIR/zenoui-browser"
```

The output contains `assets.json` (every file hash) and `browser.json` (the
observed result and Chrome stderr). `node site/tests/asset-manifest.test.mjs`
checks snapshot ownership, hash tampering, unknown routes, external origins,
path escape and changes after load without needing Chrome.

The restricted-environment spike could not reach DevTools. The first launch
failed in Crashpad with `setsockopt: Operation not permitted`. Disabling
Crashpad using Chrome's `--disable-crashpad-for-testing` reached
`content/browser/sandbox_host_linux.cc:41`, then failed at `shutdown` with
`Operation not permitted`. An additional run with Chrome's internal sandbox
disabled reached the same refusal. These are observed socket operations;
tracing their address families was unavailable because `PTRACE_TRACEME` and
`PTRACE_SETOPTIONS` were also denied. No module-worker, Wasm or browser parity
pass is claimed. The lead must run the spike and existing browser checks on a
host that allows Chrome's internal IPC. `SITE_CHROME_NO_SANDBOX=1` reproduces
the additional diagnostic only; it is not required by the normal runner.


The launcher now execs Chrome through `exec_chrome.py`. It preserves only
standard IO and the DevTools channels 3/4, closes every higher inherited
descriptor and keeps the same process ID for shutdown. The profile is created
directly under `/tmp`, independently of `TMPDIR`. The normal Chrome launch
also drops the earlier Crashpad testing flag to match the regular browser
runner. Whether these changes resolve the host's FD ownership crash requires
the lead's rerun; no successful Chrome run is claimed here.

Offline regressions:

```sh
node --experimental-vm-modules site/tests/factory-readiness.test.mjs
node site/tests/browser-launcher.test.mjs
python3 -B -m unittest discover -s site/tests -p test_socket_free_launcher.py
```

The readiness test executes the shipped page module, worker and actual Wasm
with a small DOM adapter. It requires completion after rendering and native
parity, kills a missing-event control, checks the served module paths and
checks that an import error produces failed harness output. It does not
establish Chrome layout, raster or keyboard behavior. The geometry oracle
receives the artifact's geometry through the declared deployment base; a
filesystem-relative public import is never used as a served URL.

The lead's next host run reached the page and attached its module worker, but
stopped because dedicated-worker sessions do not provide `Fetch.enable`.
Fetch is now enabled only on page sessions; flattened auto-attach resumes the
worker while the page handles manifest requests. `pipe-browser.test.mjs` drives
the actual pipe runner with a scripted protocol peer that rejects worker Fetch,
checks manifest hashes/refusals, and supplies a full result with a planted
nested-witness mismatch. This tests routing and parity enforcement, not Chrome
interception or execution. Those still require the lead's host run.

```sh
node site/tests/pipe-browser.test.mjs
node site/tests/factory-width.test.mjs
```

The width check computes each declared region's minimum against 350 CSS pixels
at a 390-pixel viewport, from the same datum that generates
`factory-layout.css`. Regenerate it with the width test's `--write` option.
Intrinsic ledger columns and oversized-region controls must fail. The canvas
retains its exact geometry and scroll container; ledger and comparison text
wrap instead of forcing the page wider. This is a structural check, not browser
font, layout, raster or reflow evidence.
