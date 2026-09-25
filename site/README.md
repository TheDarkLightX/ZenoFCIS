# ZenoFCIS demo site

A static page that runs an example application in the browser through its own
commit authority, program, and law checker, compiled to WebAssembly, with the
library's in-memory reference shell in place of SQLite. Phase 1 covers one
example, `account-lockout`. Nothing here is published: there is no Pages
workflow and no public URL.

## Layout

- `Cargo.toml`, `Cargo.lock`: a workspace of its own, like `verification/`,
  so the generated application never enters the published crates' graph. Its
  `[patch.crates-io]` table resolves the application's version pins to this
  checkout.
- `apps/account-lockout/`: written by `build.py` with `zeno-fcis new`, not
  committed, so the page always runs the template as the CLI ships it.
- `demos/account-lockout/`: the module. `src/demo.rs` follows the template's
  `invoke` with `AuthorizedShellState` in place of the SQLite shell; `src/abi.rs`
  is the C ABI (`demo_alloc`, `demo_free`, `demo_reset`, `demo_step`,
  `demo_state`) and the only code that handles raw pointers.
- `public/`: the page. `index.html`, `site.css`, `site.mjs`, the shared loader
  `demo-module.mjs`, and `demonstration.mjs`, the README's scripted requests.
  The built `account-lockout.wasm` and the copied `account-lockout.README.md`
  land here and are not committed.
- `tests/replay.mjs`: the headless check, Node 22 and no browser.
- `build.py`: builds and tests everything.

## Build and view

From the repository root, with Rust 1.97.1, its `wasm32-unknown-unknown`
target (`rustup target add wasm32-unknown-unknown --toolchain 1.97.1`), and
Node 22 on `PATH`:

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
scratch in between, which must produce identical bytes; and
`node site/tests/replay.mjs`.

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

The strength of what the template itself checks is stated in its README and
repeated on the page: laws evaluated at run time on every decision; tests on a
host over 20 examples and 606 grid inputs; and claim 600's induction step,
which CVC5 attests with `unsat` and which is not proved.

## Next

A public page should open in a working state: for example, with the README's
demonstration already decided when the page loads, and labelled as such, so the
first view shows the judge at work. Publishing waits for the owner's
confirmation.
