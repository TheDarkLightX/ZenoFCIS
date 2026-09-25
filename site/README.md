# ZenoFCIS demo site

A static page that runs the six example applications in the browser, each
through its own commit authority, program, and law checker, compiled to
WebAssembly, with the library's in-memory reference shell in place of SQLite:
account-lockout, order-fulfillment, inventory-reservation,
compliance-gateway, withdrawal-queue, and agent-treasury-guard. Every example
adds only its request mapping, its description, and its examples reader to
the shared shape described here. The Pages workflow publishes only from
`main`, at https://thedarklightx.github.io/ZenoFCIS/.

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
  `Application` trait a demo crate fulfils; `src/abi.rs` is the bounded
  version 2 scalar API described in [ABI.md](ABI.md). Buffers remain owned
  by Rust, with no exposed pointers, allocation or free calls. Requests are
  limited to 4,096 bytes, replies to 131,072 bytes, and sessions to 64
  requests before reset. `wasm.py` keeps linear memory private in the
  published artifact by removing its export and the address globals, without
  changing any other section or any function export.
- `apps/<template>/`: written by `build.py` with `zeno-fcis new`, not
  committed, so the page always runs each template as the CLI ships it.
- `demos/<template>/`: one crate per example. It names the application's
  types, its exact genesis, and the mapping from the page's request to the
  template's typed command and context, in the README's words, and exports
  the module's `demo_reset`.
- `public/`: the page, and the Pages artifact. `index.html` holds the
  opening (the title, a diagram of proposer, judge, and shell, and a
  collapsed "How this page works"), a row of tabs that chooses an example,
  and one section per example with the README's wording, shown one at a
  time; `site.js` runs the tabs and builds a section's panel the first time
  it is chosen, loading `templates/<template>.js`, the example's description
  (its state fields with plain labels, its context and commands in the
  README's words grouped as the page shows them, its reasons in plain words,
  the README's demonstration, and the demonstration's end in the gate's
  fields), and `<template>.wasm`; `panel.js` is the shared panel: the
  controls, the latest decision in plain words, the state, the outbox, the
  history of decisions with a technical record for each (the request as
  sent, the reason's code, the authorization or rejection hash, the bundle
  and its replay check, and the laws evaluated with their statuses), and,
  for a template that describes one, a scripted proposer's list (the
  treasury guard's agent); `demo-module.js` is the loader. Hashes are shown
  by their first twelve hex digits, with the full value a button away. The
  built modules land here and are not committed. The page fetches its
  modules relative to its own scripts, so it works from any path it is
  served at, and it loads no font, script, style, or image from another
  origin.
- `tests/abi.mjs`: checks the built modules expose only the safe scalar
  functions, with private memory, and enforce the request and session bounds.
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
and `zeno-fcis new` for every template, compared byte for byte with the
source template; the lock check against the workspace
lock (`--relock` regenerates `site/Cargo.lock` from it); `cargo fmt --check`,
clippy with `-D warnings` for the host and for wasm32, and the host tests;
two release builds of each module, with the application and the demo crate
rebuilt from scratch in between, which must produce identical bytes, and the
sizes; the compiled API checks and `node site/tests/replay.mjs`; `node --check` on every script; and
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

`tests/deploy_check.py` first reads the page files: `index.html` must show
exactly one example on load, no page file may load a resource from another
origin, and every text and background pair the page uses must have WCAG AA
contrast in both of the stylesheet's themes. It then serves `site/public` exactly
as the workflow uploads it, at `/ZenoFCIS/` on a local server that serves
nothing at the root, and drives headless Chrome through a DevTools pipe,
waiting for the demonstration's completion state before capturing the DOM,
with a 120-second wall-clock timeout. This also waits for asynchronous
WebAssembly compilation on slower CI hosts. The browser runner's tests
require a delayed result to complete and a page that stays pending to fail.
A harness page, served beside the artifact and not part of it, is captured
once per template: it imports the artifact's own panel and the template's
description, mounts the panel as the page does, runs the README's
demonstration through it, and prints the results and what the panel
rendered, which must match the gate's summary, one history entry per
decision, the last decision's verdict in the latest-decision box, the
module's full value behind every hash the panel shows by its prefix, and the
proposer's list where the template describes one. The harness then starts
the panel over, enters each demonstration step into the form, and clicks
its button: every control must send exactly the request the script sends,
and reach the script's decision. The page itself must have loaded the module
of the example shown on load, run that README's demonstration, said so in
its status, rendered one history entry per decision with the gate's decision
on it and the last one in its latest-decision box, shown no other example,
and fetched no other module; and, loaded in a frame of the same origin
(`tests/harness/viewport.html`) at 1280 by 800 and at a phone's 390 by 844,
it must have stayed at its top with its title in view, since the latest
decision scrolls into view only when the viewer caused it, and its document
must be no wider than the frame. Every request the browser made, as the
DevTools pipe reports them, must be for the subpath or the harness; Chrome's
own probe for `/favicon.ico` at the origin's root, which the server refuses,
is the one exception. A page that loaded from the subpath alone is a page
whose relative paths hold. Each template has its own browser capture so that
a failure identifies the template involved.

The strength of what each template itself checks is stated in its README and
repeated in its section of the page, in the README's own scoped words: laws
evaluated at run time within their declared decision scopes; tests on a host, which are
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

The example shown when the page loads, agent-treasury-guard, runs its
README's demonstration at once and labels the result: its status says the
demonstration was decided in this browser when the page loaded, the
latest-decision box shows the last decision, and the history holds one entry
per decision, so the first view shows the judge at work. Every other example
starts at its genesis when it is first chosen, ready for the viewer's
requests or its own demonstration button, and keeps its state when the
viewer chooses another example and comes back.
