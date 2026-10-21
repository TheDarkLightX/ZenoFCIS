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
