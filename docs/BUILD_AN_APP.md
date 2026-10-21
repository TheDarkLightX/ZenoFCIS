# Build an app with ZenoFCIS

“I want an inventory app that reserves and ships items, refuses commands
without permission, and recovers a request after its reply is lost.”

Start with a reviewed contract. `project.zeno` declares the types, laws and
channels; `v2/policy.json` supplies the rules and initial state; `ui.json`
supplies the labels, units, command descriptions and refusal messages. The
[contract guide](CONTRACT_RULES.md) explains how to write and review
those files. A one-line ask starts that review; it does not replace it.

From the source checkout, build the CLI and create the app:

```sh
nice -n 19 ionice -c3 cargo +1.97.1 build --locked --offline -p zeno-fcis-cli
nice -n 19 ionice -c3 target/debug/zeno-fcis new inventory-app \
  --contract crates/zeno-fcis-cli/templates/inventory-reservation/project.zeno --app
```

`--app` explicitly runs the pinned Python app factory and `tools/zenoui.py`.
Python 3.11 or later, Node 22, Rust 1.97.1 and its
`wasm32-unknown-unknown` target must already be installed. All dependencies
must be cached. The command builds the native descriptor checker and shared
UI runtime offline, checks the complete finite input domain and the eight
ZenoUI laws with planted defects, and checks the mounted shared components.
There is no network step. Unsupported descriptors and missing metadata refuse.
The first build also generates the six demonstration libraries required by
the existing site workspace, if they are absent.

The output contains the core, SQLite session, an `app-server` binary target,
static UI assets, `context.json`, the copied and reconciled Cargo lock,
application tests and `app-manifest.json`. That manifest records generated
file, UI, tool and asset digests. It supplies build provenance; Authority
still checks every publication. The core also builds with default features
disabled. The default `app` feature enables SQLite and shell effects.

Review `context.json` before running. It fixes this credential's application
permissions. For the inventory example, `["1"]` grants the operator role;
`["0"]` grants no operator permission. This is deployment configuration,
never a field the browser can choose. The default public principal is 32
bytes of `07`; set `ZENO_APP_PRINCIPAL` to a distinct 64-digit lowercase
hexadecimal principal when configuring a deployment. Retain the principal,
context, contract and original request for recovery.

Create a private credential file once, then start the app:

```sh
cd inventory-app
nice -n 19 ionice -c3 python3 - <<'PY'
import os, secrets
fd = os.open('credential', os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
with os.fdopen(fd, 'w') as stream:
    stream.write(secrets.token_hex(32) + '\n')
PY
nice -n 19 ionice -c3 env ZENO_APP_SECRET_FILE=credential \
  cargo +1.97.1 run --locked --offline --bin app-server
```

Open `http://127.0.0.1:8080` and sign in with that credential. Until sign-in
succeeds the page says that sign-in is required and offers no commands.
An empty or invalid credential leaves a visible refusal on the sign-in form.
The browser clears the password field as soon as it captures the credential.
It holds the credential only in the transport's private memory, never in
static assets, a URL, cookies, localStorage or sessionStorage. Reloading
requires sign-in again. An unresolved request's recovery record contains
request bytes and identity, without the credential.

Sign-in first authenticates `/state`, then loads `/app.json` using that same
credential. Only after both succeed does the UI mount command controls.
The HTML, scripts, Wasm and descriptor are public, immutable assets; none
contains deployment credentials or private application state. This design
uses explicit Bearer authentication rather than creating a cookie session.
Every `/state`, `/preview` and `/command` call carries one Authorization
header. The ingress derives the principal from its configured credential
binding, retains its cross-origin refusals and request limits, and the
browser cannot select a principal. A failed sign-in erases the transport's
credential and leaves the sign-in form visible.

The UI shows the configured principal and permission
context, and asks the native core to preview each command. A command is
offered only after an accepting preview for those exact inputs and revision.
Review and confirm its details. The server authenticates again, rechecks the
core and atomically commits state, receipt, replay record and outbox. The UI
shows a completed decision only after the commit and delivery run complete.

The browser saves the exact request before sending. An interrupted request
leaves a recovery record. Reload, sign in with the same credential and use
“Recover original request”. The request retains its command bytes, revision
and original key. The server rebuilds the original publication from audited
history and returns the original packet. Do not delete an unresolved record
or invent another key because a reply was lost.

Restart with the same start command and configuration. SQLite keeps the
state and pending outbox. `ZENO_APP_STORE`, `ZENO_APP_RECEIVER`,
`ZENO_APP_CONTEXT` and `ZENO_APP_ADDRESS` select the database, local file
receiver, context file and loopback address. The receiver stores each exact
payload under its delivery ID and acknowledges it after syncing. It is a
local example receiver. Other services require a reviewed transport and
receiver deduplication. Cleartext HTTP is confined to loopback; remote use
needs a trusted TLS proxy and an explicitly reviewed origin configuration.

## The formal model

The immutable application inputs and bound contract enter Authority. A
checked publication, rather than a UI result, grants eligibility to commit.
The shell-effects programs separately decide bounded commit recovery and
outbox retry steps. Production handlers execute those steps against SQLite,
time and transport. [Shell effects](SHELL_EFFECTS.md) describes their fault
alphabet, original-key recovery rule, resource limits and evidence scope.

The UI presentation state binds principal, contract, revision, command,
amounts and units. Edits invalidate consent. Preferences change presentation
only. Its finite enablement implication is checked against actual core
decisions across the declared domain. A preview is advice: the server's
revision guard and Authority check remain necessary at dispatch.

The ingress owns its credential and admitted bytes; the shell exclusively
owns its mutable storage and transport handles. Snapshots, prepared command
bytes, traces and manifests cannot authorize a publication. Static assets
are embedded under exact routes; URL paths never become filesystem reads.
The optional `/state` view and `/preview` preparation require the same
credential and origin checks as `/command`.

## Compared with writing the app by hand

You write and review the domain model, rules, permission context and labels.
The factory supplies shared forms, confirmation, authenticated acquisition,
SQLite publication, bounded recovery, delivery, static serving and the
application test structure. This reduces repeated glue code while preserving
the core's independent acceptance checks. It does not infer the right laws,
choose a deployment's permissions or make an external service idempotent.

## Scope and checks

The backend supports the flat, finite, standalone scalar contracts supported
by ZenoUI M2, with an enum command discriminator and at most 2,000,000 input
tuples. Integers remain decimal strings in the UI. Evolved contracts,
tables and unsupported descriptors refuse. The generated journey fixtures
cover withdrawal queue, inventory reservation and the wide counter. The
counter declares no deliveries, so its journey checks an empty outbox.
Other contracts get an accepting genesis smoke journey and need their own
reviewed refusal and delivery journey before adoption.

Run `python3 tools/check_app_factory.py` from the checkout for generated
applications with features off and on, their socket-free HTTP/SQLite
journeys, the shipped sign-in bootstrap, empty/wrong-credential refusals,
presentation checks and the ingress fault corpus. Add `--tcp` on a
host that permits loopback for the same journeys over TCP. A separate host
Chrome run enters credentials through the form, checks signed-out refusal,
loses a reply and signs in again before original-key recovery. Its report
records response statuses and page errors without credential values.
Simulation, finite checks and
browser tests have separate evidence scopes. No whole-application proof,
power-loss guarantee, exactly-once external delivery, multi-writer support
or release qualification is claimed.
