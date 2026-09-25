# ZenoFCIS demo site

A static page that runs an example application in the browser through its own
commit authority, program, and law checker, compiled to WebAssembly, with the
library's in-memory reference shell in place of SQLite. It covers one example,
`account-lockout`. Nothing here is published: the Pages workflow deploys only
from `main`, and merging to `main` is the owner's decision.

## Layout

- `Cargo.toml`, `Cargo.lock`: a workspace of its own, like `verification/`,
  so the generated application never enters the published crates' graph. Its
  `[patch.crates-io]` table resolves the application's version pins to this
  checkout.
- `package.json`: marks the scripts as ES modules for Node, which runs the
  tests. The page needs no package, and nothing is installed.
- `apps/account-lockout/`: written by `build.py` with `zeno-fcis new`, not
  committed, so the page always runs the template as the CLI ships it.
- `demos/account-lockout/`: the module. `src/demo.rs` follows the template's
  `invoke` with `AuthorizedShellState` in place of the SQLite shell; `src/abi.rs`
  is the C ABI (`demo_alloc`, `demo_free`, `demo_reset`, `demo_step`,
  `demo_state`) and the only code that handles raw pointers.
- `public/`: the page, and the Pages artifact. `index.html`, `site.css`,
  `site.js`, the shared loader `demo-module.js`, and `demonstration.js`, the
  README's scripted requests. The built `account-lockout.wasm` lands here and
  is not committed. The page fetches its module relative to its own script,
  so it works from any path it is served at.
- `tests/replay.mjs`: the headless check, Node 22 and no browser.
- `tests/deploy_check.py` and `tests/harness/`: the exact artifact, served
  from a subpath, in headless Chrome.
- `build.py`: builds and tests everything.
- `.github/workflows/pages.yml`, at the repository root: builds and deploys.

## Build and view

From the repository root, with Rust 1.97.1, its `wasm32-unknown-unknown`
target (`rustup target add wasm32-unknown-unknown --toolchain 1.97.1`), Node
22, and Chrome on `PATH`:

```sh
python3 site/build.py
python3 -m http.server --directory site/public 8000
```

Then open `http://localhost:8000/`. The page fetches the module once; after
that it makes no network requests. `build.py` runs, in order: the CLI build and
`zeno-fcis new`; the lock check against the workspace lock (`--relock`
regenerates `site/Cargo.lock` from it); `cargo fmt --check`, clippy with
`-D warnings` for the host and for wasm32, and the host tests; two release
builds of the module, with the application and the demo crate rebuilt from
scratch in between, which must produce identical bytes;
`node site/tests/replay.mjs`; `node --check` on every script; and
`site/tests/deploy_check.py`, which needs Chrome (`--no-browser` skips it,
`--chrome PATH` names the binary).

To view the page from a subpath, as GitHub Pages serves it at
`https://thedarklightx.github.io/ZenoFCIS/`:

```sh
mkdir -p /tmp/pages && ln -sfn "$PWD/site/public" /tmp/pages/ZenoFCIS
python3 -m http.server --directory /tmp/pages 8000
```

Then open `http://localhost:8000/ZenoFCIS/`.

## What the page shows, and what is checked

Each request carries its command and its context (the time and the admin
flag), which the page supplies; the module reads no clock. A step runs schema
admission through the generated bindings, `admit_invocation`, `execute`, and,
for an accept or a committed failure, the commit and the exact replay check,
as the template's `invoke` does. The report gives the decision kind, the
reason, each law's status as the law evaluation reports it, the account before
and after, the alerts queued, and the authorization or rejection identity.

`tests/replay.mjs` replays the template's `tests/decision-examples.txt` (each
example's account is reached from genesis by requests, then its request is
decided) and the README's demonstration, and compares every outcome with the
examples file and with the gate's expected summary in
`tools/check_generated_application.py`. Nothing delivers in the page, so the
alerts the gate's shell delivered are the alerts left pending here.

`tests/deploy_check.py` serves `site/public` exactly as the workflow uploads
it, at `/ZenoFCIS/` on a local server that serves nothing at the root, and
drives headless Chrome over it with `--dump-dom` under a virtual-time budget.
A harness page, served beside the artifact and not part of it, imports the
artifact's own loader and demonstration script, runs the demonstration through
the module, and prints the results, which must match the gate's summary; the
page itself must have loaded its module and rendered the genesis. A page that
loaded from the subpath alone is a page whose relative paths hold.

The strength of what the template itself checks is stated in its README and
repeated on the page: laws evaluated at run time on every decision; tests on a
host over 20 examples and 606 grid inputs; and claim 600's induction step,
which CVC5 attests with `unsat` and which is not proved.

## Publishing

`.github/workflows/pages.yml` runs on a push to `main` and on a manual run. Its
build job runs `python3 site/build.py` and uploads `site/public` with
`actions/upload-pages-artifact`; its deploy job runs `actions/deploy-pages`
only when the ref is `main`. Every action is pinned by commit. The deploy job
has the two scopes that action needs, `pages: write` and `id-token: write`,
and nothing else; `tools/check_assurance.py` allows exactly those two, only in
that file, and refuses any other write scope. The workflow does not enable
Pages: the repository's Pages source must be set to GitHub Actions by its
owner. The page links each template's README at
`https://github.com/TheDarkLightX/ZenoFCIS/blob/main/...`; those links resolve
once this work is on `main`, which is also the only branch Pages deploys from.

## Next

A public page should open in a working state: for example, with the README's
demonstration already decided when the page loads, and labelled as such, so the
first view shows the judge at work. The other example applications follow the
same shape.
