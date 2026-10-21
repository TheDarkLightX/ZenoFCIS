# Build an app with ZenoFCIS

“I want an inventory app that reserves and ships items, refuses commands
without permission, and recovers a request after its reply is lost.”

Start with a reviewed contract. `project.zeno` declares the types, laws and
channels, in the syntax of the [`.zeno` language reference](ZENO_LANGUAGE_V1.md);
`v2/policy.json` supplies the rules and initial state; `ui.json`
supplies the labels, units, command descriptions and refusal messages. The
[contract guide](CONTRACT_RULES.md) explains how to write and review
those files, including the [`ui.json` keys](CONTRACT_RULES.md#ui-metadata-uijson).
A one-line ask starts that review; it does not replace it. Two worked
examples, [library loans](../examples/apps/library-loans/README.md) and a
three-party [escrow](../examples/apps/escrow-deal/README.md), each start from
a one-line ask.

From the source checkout, build the CLI:

```sh
nice -n 19 ionice -c3 cargo +1.97.1 build --locked --offline -p zeno-fcis-cli
```

Check the contract directory before building on it:

- `target/debug/zeno-fcis check DIR/project.zeno --require-substantive
  --require-resolved-paths` parses the project and refuses laws that
  constrain nothing or name no declared field.
- `target/debug/zeno-fcis generate contract DIR --out SCRATCH` compiles the
  rules and names the first entry it cannot use.
- `target/debug/zeno-fcis contract review DIR` decides every input of a
  small domain, or a boundary set of a large one, compares the decision
  examples and reports the rule mutants those inputs do not distinguish.
- The [contract judges](CONTRACT_JUDGES.md) answer narrower questions:
  `contract lemma` whether an invariant holds on every reachable state,
  `contract scenario` whether a written run goes as intended,
  `contract profile` which states a store can reach, and `contract probe`
  one exact decision.

These checks observe the contract on its declared domain; they do not show
that the rules say what the owner means. Then create the app:

```sh
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
state and pending outbox. The server writes the submission journal the
generated command line's `history` reads, in the same format as `submit`,
so `history` lists and decides again every request the server committed. `ZENO_APP_STORE`, `ZENO_APP_RECEIVER`,
`ZENO_APP_CONTEXT` and `ZENO_APP_ADDRESS` select the database, local file
receiver, context file and loopback address. Unset, each takes its default;
set but empty or not text, the server refuses to start and names it, rather
than open a default store. Any other variable whose name starts with
`ZENO_APP_`, such as a misspelt `ZENO_APP_CONECTOR`, also refuses start-up
and is named, so a typing mistake never leaves a default silently in force. The receiver stores each exact
payload under its delivery ID, with `ID.json` beside it: an envelope naming
the commit, lane, ordinal and destination. It acknowledges the delivery after
syncing both. It is a local example receiver. Other services require a reviewed transport and
receiver deduplication. Cleartext HTTP is confined to loopback; remote use
needs a trusted TLS proxy and `ZENO_APP_ORIGIN`, as
[Deploy behind a TLS proxy](#deploy-behind-a-tls-proxy) describes.

## Several parties on one server

“An escrow app: a buyer deposits, the seller marks it shipped, then the
buyer releases the money or the operator refunds it.”

Three people use one server, each with their own credential. The buyer
signs in and deposits. The seller signs in from another browser and marks
the goods shipped. The buyer releases the money. When the seller tries to
refund, the server refuses: only the operator's context permits refunds.
The [escrow example](../examples/apps/escrow-deal/README.md) runs this
journey.

Instead of `ZENO_APP_SECRET_FILE`, `ZENO_APP_PRINCIPAL` and
`ZENO_APP_CONTEXT`, set `ZENO_APP_PARTIES` to a parties file:

```json
{"schema": "zeno-fcis/app-parties/1",
 "parties": [
  {"name": "buyer", "credential": "buyer.credential",
   "principal": "b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1",
   "context": "buyer.context.json"}]}
```

| Key | Value |
|---|---|
| `name` | 1 to 32 letters, digits, `-` or `_`; unique. |
| `credential` | A private file holding 64 lowercase hexadecimal digits, as for `ZENO_APP_SECRET_FILE`; unique and not all zero. |
| `principal` | 64 lowercase hexadecimal digits; unique. `/state` shows it, and replay keys are scoped to it. |
| `context` | A file with this party's context values, in the form of `context.json`. |
| `view` | Optional: the names of the state fields this party sees, such as `["status", "held"]`. Without it the party sees every field. |

Relative paths resolve against the parties file's directory. The server
names the reason and refuses to start when the parties file or any file it
names is unreadable or larger than 64 KiB; it lists no parties or more than
64; an entry lacks a key or has another one; names, credentials or
principals repeat; a context does not fit the contract; `ZENO_APP_PARTIES`
is set together with a single-credential variable; or the address is not
loopback. Without a parties file, the single-credential variables work as
before.

The ingress compares the presented credential with every configured
credential at full width, visits every entry with no early exit, and selects
the matched principal by mask. That principal alone selects the context the
Authority decides with: no body field or header can choose a party,
principal or context, and a preview whose inputs carry another context is
refused. Replay keys stay scoped to the principal, so one party cannot
recover or read another party's request, even with the same key and bytes.
Every party sees the same store state, with its own principal and context.

A view hides the other state fields from that party: `/state` and every
reply give `null` in their place, the interface shows only the fields the
view lists, and the server supplies the hidden values a preview needs,
whatever the request holds there. The credential, never the request, selects
the view. This is display filtering, not privacy. Decisions, refusals and
previews can still reveal facts about hidden fields, and each `/command`
reply packet carries the complete successor state.

The interface still shows every command to every party; the contract refuses
the ones a party's context does not permit. Credentials are fixed at
start-up, so adding or rotating one needs a restart. There is no lockout;
[the proxy recipe](#deploy-behind-a-tls-proxy) adds rate limits. The all-entry comparison is a reviewed source form checked by
`tools/check_ingress.py`, not a timing proof of the compiled code.

## Deliver through a program

“When the buyer releases the money, post it to our payments webhook.”

By default the server hands each delivery to the local file receiver. Set
`ZENO_APP_CONNECTOR` to a connector file to run one program per delivery
instead:

```json
{"schema": "zeno-fcis/app-connector/1",
 "argv": ["/usr/bin/curl", "--fail", "--silent", "--show-error",
          "--max-time", "8", "--header", "Content-Type: application/json",
          "--data-binary", "@-", "https://hooks.example.com/escrow"],
 "timeout_seconds": 10}
```

`argv` is an absolute program path and its arguments, at most 64 entries;
the server runs it directly, never through a shell, with an empty
environment apart from `PATH=/usr/bin:/bin`, and discards its output. Its
standard input is one line holding a JSON object: the delivery's envelope,
with `delivery`, the delivery ID; `commit`, `lane` and `ordinal`;
`destination` in hexadecimal and, for a text destination,
`destination_text`; `payload_sha256`; and `payload` in hexadecimal.
`timeout_seconds` is 1 to 60. `ZENO_APP_CONNECTOR` replaces
`ZENO_APP_RECEIVER`; setting both refuses start-up.

Treat standard input as data. Its values follow from requests that users
make, so a program must parse it as JSON and never evaluate it, pass it to a
shell, or splice any field into a command line, a file path, a query or a
template unescaped. `destination_text` holds only printable ASCII, but a
quote, a space or a `$` is printable. Run a shell script only if it reads the
JSON with a real parser, such as `jq`, and quotes every value it uses.

The server serves one request at a time, so a delivery holds every other
request until it ends. A delivery run tries a failing delivery three times,
so one command can wait three times `timeout_seconds`. A proxy that stops
waiting first, such as nginx after its `proxy_read_timeout`, returns a
gateway error although the commit stands; the client then recovers the
reply with the original request. Keep the program fast and
`timeout_seconds` low, such as 3, or hand the work to a queue the program
only writes to.

Exit status 0 acknowledges the delivery. Any other exit, a program that
cannot start, or the time limit leaves the delivery pending: the commit
stands, the request's reply is 503, and the outbox sends it again on the
next delivery run, such as a retry of the same request. Delivery is
therefore at least once. The program, or the service it calls, must treat
the delivery ID as an idempotency key and apply each ID once. The curl
example posts the JSON object to a webhook, which deduplicates on
`delivery`. A mail command, such as

```json
{"schema": "zeno-fcis/app-connector/1",
 "argv": ["/usr/bin/mail", "-s", "Escrow delivery", "ops@example.com"],
 "timeout_seconds": 10}
```

sends the JSON object as the message body; mail has no deduplication, so a
retry can send the same delivery twice. A successful exit shows only that
the program reported success, not that a payment settled or a message was
read.

## A server clock for deadlines

“The operator may refund a deal only after day 7, the seller's shipping
deadline.”

The buyer deposits on day 1 and the seller ships on day 3. On day 5 the
operator's refund is refused with the deadline's reason; on day 8 the same
refund is accepted and the buyer's refund is delivered. Nobody types the
day: the server sets it. The
[escrow example](../examples/apps/escrow-deal/README.md) runs this journey.

The contract declares one context field as the [server
clock](CONTRACT_RULES.md#server-clock), with a unit, days or minutes, and an
epoch date. Its declared range, such as `0..=14`, bounds the times the store
accepts. Context files and the parties file's contexts omit the clock field.

When the server admits a new request, it sets the field to the current time
in those units since the epoch, but never earlier than the latest time a
committed decision recorded, including after a restart. The committed
context records that time. A time outside the declared range is refused
with status 503 and the body `Clock`; it is never wrapped or clamped. A
replay of an original request returns its original decision even after the
time has moved on: the server compares the stored context with the party's
own, apart from the time, and never reads the clock again. `/state` and
`/preview` report the time the server would use now; the interface compares
replies without the clock field, since the server, not the request, sets it.
Because the field's whole range is already in the contract's finite input
domain, `contract review`, the judges and the interface build check every
time without any change.

`ZENO_APP_CLOCK_FILE` names a file holding the current time in the clock's
units, as one decimal integer in plain form with at most one final newline,
read at every admission; anything else, such as `+5`, `05` or a space,
refuses the request with status 503. It is a test and operator tool, not a
security boundary: whoever controls the server's configuration or system
time controls the clock. The clock has a single time zone, UTC. A preview is
advice: when the time changes between a preview and its command, the
command is decided at the new time. After the range's upper bound the store
refuses every new request, so choose the range and epoch for the
deployment's lifetime.

## HTTP API

The browser interface uses these endpoints, and any client can call them.
Each request is one HTTP/1.1 request on its own connection, with exactly one
`Authorization: Bearer CREDENTIAL` header (64 lowercase hexadecimal digits),
at most 4,096 header bytes and at most 65,536 body bytes. Repeated headers
are refused, and an `Origin` header, when present, must equal
`http://ADDRESS`.

| Endpoint | Request | Reply on success |
|---|---|---|
| `GET /state` | No body. | JSON: `contract`, the hexadecimal SHA-256 of the Authority identity; `principal`; `revision`, the commits so far; `values`, the state fields, `null` where the party's view hides one; `context`, this party's context values; and `pending`, the undelivered outbox entries. Integers are decimal strings. |
| `POST /preview` | A command body whose payload is the JSON list of every input as decimal strings: the state `values` (a hidden one may stay `null`), then the command fields, then this party's `context`. | JSON: `reply`, the decision the Authority would make (`input`, `class`, `reason`, `post`, with hidden state values `null`), and `command`, the framed command in hexadecimal. Nothing is committed. |
| `POST /command` | A command body whose payload is the framed command bytes from the preview. | JSON: `packet`, the reply packet in hexadecimal, `revision` and `reply`. |
| `GET /` and the other assets | No body; no credential is needed. | The public, immutable interface files, including `/app.json`. |

A command body is the format version, then four fields:

| Bytes | Content |
|---|---|
| 0 | `1`, the format version. |
| 1 to 32 | `contract` from `/state`, as 32 bytes. |
| 33 to 64 | A key the client chooses and keeps until the request resolves. |
| 65 to 72 | The expected revision, `revision` from `/state`, as an unsigned 64-bit big-endian integer. |
| 73 onwards | The payload. |

Statuses: 200 done; 400 a malformed request, body or route; 401 a missing or
unknown credential; 403 another origin, contract or context; 409 the store
moved past the expected revision, or the key already names a different
request; 413 too large; 422 the contract refused the command (a preview
shows its reason); 503 storage or delivery is uncertain, so retry the same
body, or, with the body `Clock`, the server clock is outside the contract's
range. An error reply's body is the refusal's name, such as `Credential`.

After a lost reply, send the identical body with the same credential: the
server returns the original packet. A changed body under the same key is
refused with 409. A minimal client, for the inventory example's Restock of
3 items:

```python
import http.client, json, secrets

def call(method, path, credential, body=b""):
    connection = http.client.HTTPConnection("127.0.0.1", 8080)
    connection.request(method, path, body or None,
                       {"Authorization": "Bearer " + credential})
    reply = connection.getresponse()
    return reply.status, reply.read()

def frame(state, key, payload):
    return (b"\x01" + bytes.fromhex(state["contract"]) + key
            + int(state["revision"]).to_bytes(8, "big") + payload)

credential = open("credential").read().strip()
state = json.loads(call("GET", "/state", credential)[1])
key = secrets.token_bytes(32)  # keep until the request resolves
inputs = state["values"] + ["153", "3"] + state["context"]
status, preview = call("POST", "/preview", credential,
                       frame(state, key, json.dumps(inputs).encode()))
command = bytes.fromhex(json.loads(preview)["command"])
status, reply = call("POST", "/command", credential, frame(state, key, command))
```

## Deploy behind a TLS proxy

“Let the buyer, the seller and the operator use the escrow app from their
own computers.”

The server listens only on loopback and speaks cleartext HTTP, so a trusted
reverse proxy on the same host terminates TLS and forwards to it. Set
`ZENO_APP_ORIGIN` to the public origin in the form browsers send:
`https://`, a lowercase host, a port only when it is not 443, and nothing
else, such as `https://app.example.com`. The ingress then accepts exactly
that `Origin` and no other, including `http://ADDRESS`. The server refuses
to start when the value is set but is not one such origin: `http://`, an
explicit `:443`, uppercase, a path or trailing slash, a query, user
information, whitespace, more than one value, or bytes that are not text.
Unset, nothing changes. For example, with nginx:

```nginx
# In the http block: request rates and connections per client address.
limit_req_zone  $binary_remote_addr zone=zeno_requests:10m rate=20r/s;
limit_req_zone  $binary_remote_addr zone=zeno_core:10m rate=5r/s;
limit_conn_zone $binary_remote_addr zone=zeno_connections:10m;

server {
    listen 443 ssl;
    server_name app.example.com;
    ssl_certificate     /etc/ssl/app.example.com/fullchain.pem;
    ssl_certificate_key /etc/ssl/app.example.com/privkey.pem;
    add_header Strict-Transport-Security "max-age=31536000" always;
    # No larger than the server's limits: 64 KiB of body, 4 KiB of headers.
    client_max_body_size 64k;
    client_header_buffer_size 4k;
    large_client_header_buffers 1 4k;
    # Slow clients wait at the proxy, never at the serial server.
    client_header_timeout 10s;
    client_body_timeout 10s;
    send_timeout 10s;
    proxy_request_buffering on;
    limit_conn zeno_connections 8;
    limit_conn_status 429;
    limit_req_status 429;
    location / {
        limit_req zone=zeno_requests burst=40 nodelay;
        # TLS ends here; forward to the loopback address only.
        proxy_pass http://127.0.0.1:8080;
        # The ingress requires exactly one Host header; it does not compare it.
        proxy_set_header Host 127.0.0.1:8080;
        proxy_read_timeout 15s;
    }
    # Each state, preview and command reads and checks the whole store history.
    # A burst waits its turn; only a longer flood is refused.
    location ~ ^/(state|preview|command)$ {
        limit_req zone=zeno_core burst=20;
        proxy_pass http://127.0.0.1:8080;
        proxy_set_header Host 127.0.0.1:8080;
        proxy_read_timeout 15s;
    }
}
server {
    listen 80;
    server_name app.example.com;
    return 301 https://app.example.com$request_uri;
}
```

```sh
ZENO_APP_ORIGIN=https://app.example.com ZENO_APP_PARTIES=run/parties.json \
  cargo +1.97.1 run --locked --offline --bin app-server
```

The proxy terminates TLS with a certificate for the public name and
forwards to the loopback address only. It passes `Origin` and
`Sec-Fetch-Site` unchanged: the ingress, not the proxy, refuses other
origins. It must not add, strip or log the `Authorization` header; nginx's
default access log does not record request headers. It sends one request
per upstream connection, as nginx's default HTTP/1.0 upstream does. It keeps
its body and header limits at or below the server's 64 KiB body and 4 KiB of
headers in at most 24 fields; the server refuses anything larger in any
case. The ingress requires exactly one `Host` header and refuses a missing
or repeated one, but does not compare its value, which is why the block sets
it to the loopback address the server listens on. The proxy must not serve
the application over plain HTTP or cache its responses, which are all
`no-store`. It must be the only route to the port: the server already
accepts only loopback connections, so no other user or process on the host
should be able to reach that port. The proxy and its host are trusted;
whoever controls them sees credentials in transit. This recipe adds no
lockout or client certificates.

The server handles one connection at a time, and it waits up to 10 seconds
for a request to arrive in full. The proxy reads each request completely
before it connects (`proxy_request_buffering on`, nginx's default), so a
slow or idle client ties up a proxy connection, not the server. Every
`/state`, `/preview` and `/command` replays and checks the store's whole
history, so its cost grows with the number of commits: measured on the wide
counter, a preview took about 0.09 ms per commit in a release build and
1.3 ms per commit in the debug build that `cargo run` gives without
`--release`; a command cost two to three times as much. Build deployments
with `--release`. The rate limits keep one address from monopolising the
server: five state, preview or command requests a second, and twenty
requests a second overall for the interface's files. The interface previews
every command after each edit, so bursts are normal: up to twenty core
requests wait their turn rather than fail, and only a longer flood gets
status 429. The limits are per client address, so clients behind one
address share them, and many addresses together can still keep the server
busy; size the rates for the number of commands and the store you expect.

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

The ingress owns its credentials and admitted bytes; the presented
credential selects one configured principal, and that principal selects its
context. The shell exclusively owns its mutable storage and transport
handles. Snapshots, prepared command bytes, traces and manifests cannot
authorize a publication. Static assets are embedded under exact routes; URL
paths never become filesystem reads.
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
cover withdrawal queue, inventory reservation, the wide counter and the
three-party escrow example. The counter declares no deliveries, so its
journey checks an empty outbox.
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
