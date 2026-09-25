# ZenoFCIS demo site

A static page that runs the six example applications in the browser, each
through its own commit authority, program, and law checker, compiled to
WebAssembly, with the library's in-memory reference shell in place of SQLite:
account-lockout, order-fulfillment, inventory-reservation,
compliance-gateway, withdrawal-queue, and agent-treasury-guard. Every example
adds only its request mapping, its description, and its examples reader to
the shared shape described here. Nothing here is published: the Pages
workflow deploys only from `main`, and merging to `main` is the owner's
decision.

## Layout

- `Cargo.toml`, `Cargo.lock`: a workspace of its own, like `verification/`,
  so the generated applications never enter the published crates' graph. Its
  `[patch.crates-io]` table resolves each application's version pins to this
  checkout.
- `package.json`: marks the scripts as ES modules for Node, which runs the
  tests. The page needs no package, and nothing is installed.
- `common/`: what every module shares. `src/demo.rs` is the step over the
  library's `AuthorizedShellState`, following each template's `invoke`, and
  the JSON report; `src/render.rs` names every reported value as
  `project.zeno` names it (fields, variants, reasons, channels) and every law
  as the manifest does; `src/request.rs` reads the page's request field by
  field and refuses what a command does not read; `src/application.rs` is the
  `Application` trait a demo crate fulfils; `src/abi.rs` is the C ABI
  (`demo_alloc`, `demo_free`, `demo_step`, `demo_state`), defined once and
  exported by every module that links the crate, and the only code that
  handles raw pointers.
- `apps/<template>/`: written by `build.py` with `zeno-fcis new`, not
  committed, so the page always runs each template as the CLI ships it.
- `demos/<template>/`: one crate per example. It names the application's
  types, its exact genesis, and the mapping from the page's request to the
  template's typed command and context, in the README's words, and exports
  the module's `demo_reset`.
- `public/`: the page, and the Pages artifact. `index.html` holds the landing
  text and one `<details>` section per example with the README's wording;
  `site.js` builds a section's panel when the section opens, loading
  `templates/<template>.js`, the example's description (its state fields, its
  context and commands in the README's words, the README's demonstration, and
  the demonstration's end in the gate's fields), and `<template>.wasm`;
  `panel.js` is the shared panel: the forms, the timeline of decisions with
  the laws evaluated for each, the state, the outbox, and, for a template
  that describes one, a scripted proposer's list (the treasury guard's
  agent); `demo-module.js` is the loader. The built modules land here and are
  not committed. The page fetches its modules relative to its own scripts, so
  it works from any path it is served at.
- `tests/replay.mjs`, with `tests/examples.mjs` and
  `tests/templates/<template>.mjs`: the headless check, Node 22 and no browser.
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

Then open `http://localhost:8000/`. `build.py` runs, in order:
`cargo fetch --locked` in the workspace, as the gate does, so that every
later cargo command runs offline against the reviewed lock; the CLI build
and `zeno-fcis new` for every template; the lock check against the workspace
lock (`--relock` regenerates `site/Cargo.lock` from it); `cargo fmt --check`,
clippy with `-D warnings` for the host and for wasm32, and the host tests;
two release builds of each module, with the application and the demo crate
rebuilt from scratch in between, which must produce identical bytes, and the
sizes; `node site/tests/replay.mjs`; `node --check` on every script; and
`site/tests/deploy_check.py`, which needs Chrome (`--no-browser` skips it,
`--chrome PATH` names the binary). `--only TEMPLATE` limits the module,
replay, and browser steps to one template, and `--stage STAGE` runs one step,
so that a long build can be split into shorter runs.

To view the page from a subpath, as GitHub Pages serves it at
`https://thedarklightx.github.io/ZenoFCIS/`:

```sh
mkdir -p /tmp/pages && ln -sfn "$PWD/site/public" /tmp/pages/ZenoFCIS
python3 -m http.server --directory /tmp/pages 8000
```

Then open `http://localhost:8000/ZenoFCIS/`.

## What the page shows, and what is checked

Each request carries its command and its context, which the page supplies;
no module reads a clock. A step runs schema admission through the generated
bindings, `admit_invocation`, `execute`, and, for an accept or a committed
failure, the commit and the exact replay check, as the template's `invoke`
does. The report gives the decision kind, the reason, each law's status as
the law evaluation reports it, the state before and after, the entries it
queued, and the authorization or rejection identity, every value under the
name `project.zeno` gives it.

`tests/replay.mjs` replays, for each template, every example in its
`tests/decision-examples.txt`: the example's state is reached from genesis by
requests that `tests/templates/<template>.mjs` derives from the README's
rules, then its request is decided, and the kind, the reason, the state
after, the entries queued, and the laws evaluated are compared with the
examples file. An example whose state no sequence of requests reaches is
named in the output, with the reason, and not replayed: withdrawal-queue's
example 20 and agent-treasury-guard's example 23 are the two; the templates'
own conformance tests decide them from the admitted state. It then runs the
README's demonstration and compares its decisions and its end with the gate's
expected summary in `tools/check_generated_application.py`, and checks that
bad input is refused before any decision. Nothing delivers in the page, so
the entries the gate's shell delivered are the entries left pending here.

`tests/deploy_check.py` serves `site/public` exactly as the workflow uploads
it, at `/ZenoFCIS/` on a local server that serves nothing at the root, and
drives headless Chrome over it with `--dump-dom` under a virtual-time budget.
A harness page, served beside the artifact and not part of it, is dumped once
per template: it imports the artifact's own panel and the template's
description, mounts the panel as the page does, runs the README's
demonstration through it, and prints the results and what the panel
rendered, which must match the gate's summary, one timeline entry per
decision, and the proposer's list where the template describes one. The page
itself must have loaded the module of the section that is open on load, run
that README's demonstration, said so in its status, rendered one timeline
entry per decision with the gate's decision on it, and fetched no other
module; and, loaded in a viewport-sized frame (`tests/harness/viewport.html`),
it must have stayed at its top with its banner in view, since a timeline
entry scrolls into view only when the viewer caused it. A page that loaded
from the subpath alone is a page whose relative paths hold. (One template per dump, because Chrome's virtual-time budget is
spent across module rounds: a single round finishes within it, several do
not.)

The strength of what each template itself checks is stated in its README and
repeated in its section of the page, in the README's own scoped words: laws
evaluated at run time on every decision; tests on a host, which are
detectors; a synthesized step selected and exhaustively checked on every
input of its contract (inventory-reservation, compliance-gateway,
withdrawal-queue, agent-treasury-guard); a controller table certified by
OrbitSynthesis's checker for the finite model, not the running application
by itself (withdrawal-queue); and induction steps that CVC5 attests with
`unsat`, which is not proved (account-lockout, compliance-gateway,
withdrawal-queue, agent-treasury-guard).

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

## Opening in a working state

The section that is open when the page loads, account-lockout, runs its
README's demonstration at once and labels the result: its status says the
demonstration was decided in this browser when the page loaded, and the
timeline holds one entry per decision, so the first view shows the judge at
work. Every other section resets to its genesis when it is opened, ready for
the viewer's requests or its own demonstration button.
