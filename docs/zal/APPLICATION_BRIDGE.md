# From a ZAL model to a complete application: assessment and bridge design

Status: design only, with an assessment of checkout `3f18f9f2` for gap rows
F27-05 and F27-06 in the [2.7 gap register](../FACTORY_GAPS_27.md). The
assessment describes what exists. Nothing in the design is implemented. The
design records what a qualified bridge must prove, assume and refuse before a
release can claim it.

## The user story

An owner writes a small workflow in ZAL: a refund request that a desk approves,
and that sends one payment instruction when approved. The owner wants three
things.

1. **A generated application.** It should decide exactly as the model decides.
2. **One plain review of every message the application can send.** The review
   covers when each message is sent, where it goes and what it carries. The
   owner wants it before accepting the model.
3. **A confirmation step.** Before a person commits an approval, the application
   should say which messages that approval will send.

Today the first wish holds for models without messages. The second and third
have no route.

## Assessment

### What reaches an application today

The finite routes lower a ZAL model to a contract, a `project.zeno` and
`v2/policy.json`, through `integrations/zal/factory.py`.
`run.py qualify` then performs these steps:

1. It generates the contract, then regenerates it with `--check` for drift.
2. It reviews the contract.
3. It generates an application from the contract with `zeno-fcis new
   --contract`.
4. It runs a generated correspondence test. The test compares a reference table
   from the ZAL interpreter with the library Authority. The table has one row
   per admitted state, event and input tuple.

The table literal comes from the interpreter, independently of the contract
emitter. A missing, repeated or altered row fails the test.

| ZAL representation | Route to an application | Boundary |
| --- | --- | --- |
| `zal/finite-fsm/1` and `/2` | `lower` and `qualify`; for `/2`, the [app profile](../../integrations/zal/APP_PROFILE.md) also binds command and context roles | Exhaustive over the declared domain when the checks complete. Every reference row and every lowered case has an empty outbox. |
| `zal/finite-fsm/3` | `export` and `lower` work. | The native app provenance gate admits `/2` bundles only, so a `/3` app bundle refuses there. |
| `zal/typed-fsm/1` | None. `run.py lower` refuses with "The lower command does not support zal/typed-fsm/1 sources", observed on `examples/relay_typed.zal`. | The [typed machine](EFFECTS.md#typed-machines) elaborates to a profile-2 machine for checking only. |
| `zal/effects/1` | Lowered to the shell's effect-port record, byte for byte. | It describes shell ports such as Commit and ReadState, not the messages an application sends. |
| Temporal properties (`next`, `within`, eventuality, until, fairness) | Refused by name. On `examples/withdrawal2.zal` the refusal is "unsupported lowering: temporal property never_freezes". | No monitor or history variable is introduced. |
| Observation contracts, the finite library, notation, ontology, relational and category profiles, abstraction ladders | None. | Analysis and explanation. A classification certifies no behavior and no application. |

In the review workspace used for this assessment, which held a finite
profile-1 model, a `zal_trace` step reported class, successor, reason and rule,
with no delivery field. `zal_compare` refused a proposed surface that declared a
message channel with "not in the controlled grammar".

### What the contract and application already provide

The application side needs no new delivery machinery for a first bridge.

- **Contract deliveries.** A `project.zeno` channel has a destination type and
  a payload record type. Each case's `outbox` lists deliveries with an ordinal,
  a channel, a constant ASCII destination, one value per payload field and an
  idempotency ordinal; see [deliveries](../CONTRACT_RULES.md#deliveries).
  Payload expressions read only the original inputs: the pre-state, the
  command and the context. Every delivery uses one unit of the Effect meter.
- **Delivery laws.** Laws can bound and observe deliveries through
  `delivery_laws`. Law 991 binds every delivery's ordinal, channel,
  destination, idempotency value and payload to the case table.
- **Decision examples.** An example can name deliveries: a channel followed by
  one number per payload field, optionally pinned as `CHANNEL@ORDINAL:IDEMPOTENCY`.
  The example syntax has no destination; law 991 checks destinations at run time.
- **Generated application.** The generated README template,
  `crates/zeno-fcis-cli/contract-app/README.md.in`, describes the delivery
  commands. Committed deliveries stay pending, each with a delivery ID bound to
  its commit.
  - `deliver --to FILE` appends each pending delivery as one JSON line, synced
    to disk before acknowledgement, and never appends a delivery ID twice.
  - `deliver --relay` sends at least once and carries the original delivery
    ID. A failed attempt leaves the delivery pending.
  - An upgrade keeps pending IDs and their order.
  - A reject delivers nothing.
- **Generated UI.** A consequential command needs Review and confirmation. The
  confirmation sentence is built from the core's preview and lists state
  changes only (`site/public/zenoui/ui-view.js`, the consequence sentence).
  After a commit, the UI reports delivery as pending, unavailable or unknown.

### What is missing

- **F27-05: general meaning.**
  - Only finite profiles 1 to 3 have a lowering, and native app admission covers
    `/2` only.
  - The typed profile has no lowering, although each typed variable has at most
    64 finite values, and each value could be declared as a contract variant.
  - The analysis representations have no route and no recorded route status,
    so a reader cannot tell "not yet" from "never".
- **F27-06: effects.**
  - ZAL has no way to say that a rule sends a message.
  - With no ZAL deliveries, the reference table cannot cover them. The ZAL
    review tools, plans and explanations do not show messages, and the UI
    confirmation does not name them.
  - No retry, acknowledgement, restart or uncertain-outcome journey has been
    run on an application generated from ZAL.

## Design: one qualified bridge for finite models that send messages

The bridge extends the profile-2 route, which `qualify` already checks. It does
not attempt every ZAL representation. Each other representation gets an
explicit route status instead (see "Route status for the rest of ZAL" below).

### Source form

There is one new profile extension, `zal/app-effects/1`, with two statements.
Both have a symbolic and a controlled-English form that parse to the same model.
The syntax below is a proposal; the parser does not accept it yet.

```text
channel payment to "payments-desk" carries amount in 1..=3;
rule approve_ok: requested + approve [authorized] -> accept approved, paid = amount, send payment(amount = amount) key 0;
```

```text
Channel payment goes to payments-desk and carries amount from 1 to 3.
Rule approve_ok: when the state is requested and the event is approve, if authorized, move to approved with paid set to amount, and send payment with amount equal to amount, key 0.
```

- **Channels.**
  - A channel has exactly one constant destination. The destination therefore
    follows from the channel, which closes the example syntax's lack of a
    destination field.
  - Payload fields are bounded integers, Booleans or finite typed values.
  - Every declared channel must be used by some rule.
- **Sends.**
  - A rule's `send` list is ordered; its position is the delivery ordinal.
  - Payload expressions use the profile-2 update language, evaluated on the
    pre-state, inputs and context. Contract payloads read the same things.
  - The `key` is the idempotency ordinal, an unsigned constant.
  - A reject rule and the default reject send nothing.
- **Requirements over sends.** A step formula may use `sends(payment)`, the
  number of sends on that channel in the step.
  - It lowers to a contract law over `outbox.P`, with an explicit
    `delivery_laws` binding.
  - Only comparisons with constants are admitted, for example `sends(payment)
    <= 1` or `(accepted & sends(payment) == 1) => authorized`.

### Meaning preservation

The bridge claim is stated over one model `M` and its declared finite domain
`D`. `D` is every admissible pre-state, event, input and context tuple.

> For every tuple in `D`, the library Authority of the generated application
> returns the decision the ZAL interpreter returns. The decision includes the
> class, the reason, every successor field, and the ordered deliveries, each
> with its channel, destination, payload values and idempotency ordinal.

These steps check the claim:

1. **Reference table.** The ZAL interpreter computes each row's deliveries. The
   generated reference table writes them as
   `CHANNEL@ORDINAL:IDEMPOTENCY PAYLOAD...`, joined by `;`. This replaces the
   current `-`, which stays for rows without deliveries.
2. **Destinations.** For every case on a channel, the lowered contract uses the
   channel's single destination. The lowering asserts this, and a test reads
   it back from the emitted policy. Law 991 then binds destinations at run
   time.
3. **Correspondence.** The existing generated correspondence test compares
   every row, deliveries included, with `application::check`. Generation and
   drift checks are unchanged.
4. **Native refinement.** `run.py refine-check` already reports a changed
   delivery as a counterexample. It runs on every bridge example.
5. **Controls.** Each must fail its named check, while the unmutated copy
   passes:
   - a dropped send;
   - swapped send order;
   - a payload read from the post-state;
   - a wrong idempotency key;
   - a second destination on one channel;
   - a send attached to a reject.

**Scope of the claim.** It is exhaustive agreement between two implementations,
the interpreter and the Authority, over `D`. It is not a proof about all
lowerings. It holds only when the checks complete within their finite domain
and work limits.

### Assumptions

These are stated in every qualification report:

- **Context.** Values arrive from the host's own trusted configuration. The
  bridge does not authenticate a principal.
- **Destination.** A destination receives each delivery at least once and must
  deduplicate by delivery ID to apply it once. A successful send does not prove
  the effect in the world, such as a settled payment.
- **Idempotency.** The ordinal is a marker for the destination; it is not the
  delivery's identity.
- **Trusted base.** It is the existing shell's atomic commit, outbox and
  delivery-ID guarantees, and the Python interpreter that computes the
  reference table.

### Refusals

Each refusal has a named reason, and nothing is silently dropped. The lowering
refuses:

- a send on a reject or default rule;
- a payload expression outside the field's declared range, or one that reads
  the post-state;
- a channel with zero or two destinations;
- an unused channel;
- a non-ASCII destination;
- a send count above the generated Effect limit;
- `sends` outside a step formula, or compared with a non-constant;
- typed values above 64 cases;
- temporal properties;
- a `/3` model until the native gate admits it.

The [app profile](../../integrations/zal/APP_PROFILE.md) also refuses a binding
that gives a channel no label or gives a payload field no label and unit.

### Plan review

The extension adds an inert review artifact, `zal/effect-plan/1`, computed from
the checked model and its reference table. It has three parts.

- **Per channel.** It gives the destination, payload fields with labels and
  units, and the largest number of sends in one step.
- **Per event.** It lists every rule that sends, with its guard and payload
  expressions in controlled English, the order and the key. It gives one
  concrete row of the reference table as a witness.
- **Never-sends statements.** It lists every state and event pair in which no
  admitted input sends anything. The table enumerates the whole domain, so
  these are exhaustive over `D`, not samples.

`run.py plan FILE` and a read-only review tool print the plan. It joins the
review in the ZAL terminal. **Acceptance stays a human action there.** No tool,
command approval or check result grants it. A changed plan, such as a new
send, a new destination or a changed payload, invalidates an earlier acceptance
of the model revision, because the plan is part of that revision.

### Confirmation in the generated UI

The app binding gains `ui.channels`, which gives each channel a label and its
payload labels. The UI confirmation for a consequential command extends the
current sentence with the deliveries of the core's preview, for example:
"Approve: Request state becomes Approved; sends Refund payment to
payments-desk: Amount 2."

The sentence is built from the preview's deliveries, not from the model, so it
says what this commit will send. A command whose preview sends something is
consequential, and the binding may not mark it otherwise. Browser and keyboard
qualification of the new sentence stays with F27-07 and F27-10.

### Runtime journeys for the effectful example

These journeys run on the application generated from the example, with a file
destination and with the reference relay:

1. **Approve and deliver.** An approval commits one pending delivery with a
   delivery ID. `deliver` appends it once and acknowledges it.
2. **Crash after write.** The process stops after the line is synced and before
   acknowledgement. After restart, `deliver` does not append the delivery ID a
   second time.
3. **Failing receiver.** With a relay receiver that keeps failing, the delivery
   stays pending. Each retry carries the original delivery ID and payload.
4. **Unknown outcome.** When a send's outcome is unknown, the delivery stays
   pending and is reported as unknown, never as failed. A receiver that saw it
   deduplicates the retry.
5. **Refusals.** A rejected approval and a refusal send nothing. History
   replay reaches the stored state.
6. **Upgrade.** Upgrading the store to a checked program successor keeps
   pending IDs and their order.

The journeys check the existing shell guarantees on a ZAL-generated
application. They are not power-loss qualification, and they do not show
exactly-once execution at an external system. Fencing, revocation and restore
remain F27-03.

### The effectful example

```text
zal 2;
machine refund_request;
states requested, approved, refused;
events approve, refuse;
context authorized;
input amount in 1..=3;
var paid in 0..=3 initially 0;
initial requested;
step atomic; frame control_and_variables;
default reject not_enabled unchanged;
channel payment to "payments-desk" carries amount in 1..=3;
rule approve_ok: requested + approve [authorized] -> accept approved, paid = amount, send payment(amount = amount) key 0;
rule refuse_ok: requested + refuse [authorized] -> accept refused;
rule approve_denied: requested + approve [!authorized] -> reject denied unchanged;
require pay_only_when_authorized: (accepted & sends(payment) == 1) => authorized;
require at_most_one_payment: sends(payment) <= 1;
```

Without its channel and sends, with `at_most_one_payment` removed and the
first requirement written as `(event == approve & accepted) => authorized`,
the same model checks and lowers today. Its reference table has 144 rows: 3
control states and 4 values of `paid`, times 2 events, 2 values of
`authorized` and 3 amounts. On checkout `3f18f9f2`, `run.py qualify` on that
send-free model exits 0 with status `pass-with-scope`, and the generated
correspondence test reports 144 whole-domain tuples checked. The proposed plan review would read:

```text
Channel payment goes to payments-desk. It carries amount (1 to 3). At most 1 send per step.
Event approve: rule approve_ok sends payment when the state is requested and authorized,
  with amount equal to the requested amount, key 0. Witness: requested, approve, amount 2, authorized: sends payment(amount 2).
Event refuse: sends nothing in any state.
Never sends: approved and refused, for every event and input; requested + approve when not authorized.
```

Today's lowering writes rows as `pre-state | decision | deliveries` with the
lowering's stable IDs. For example, `152 0 120 1 2 | accept - 150 2 | -` reads:
- control state `requested` (152), `paid` 0, event `approve` (120),
  `authorized` true and `amount` 2;
- accepted, moving to `approved` (150) with `paid` 2.

With the extension, the same row would end `| 300@0:0 2`: channel 300,
ordinal 0, key 0, `amount` 2. The lowered contract would declare one channel,
its destination `payments-desk`, and the laws for both requirements.

## Route status for the rest of ZAL

F27-05 asks for meaning beyond the current finite examples. The design gives
every ZAL representation an explicit, machine-readable route status instead of
an absent route:
- `qualified-bridge`: lowering plus `qualify`;
- `export-only`;
- `analysis-only`;
- `refused`, with its reason.

The order of work is:

1. **Typed machines.** Lower through their profile-2 elaboration, and declare
   each finite type as a contract variant with stable IDs. The plans and
   explanations write values back by name, as the checker already does. This
   reuses the existing qualification unchanged.
2. **Profile 3.** Extend the native provenance gate and complete app
   qualification. This touches the native crates.
3. **Effects.** The extension in this document.
4. **Observation contracts.** Use them as review evidence attached to a
   bridged model, never as an application route of their own.

Temporal properties stay refused until a monitor design states its own
meaning.

## Next steps and required evidence

1. **Route-status table.** Add it, and a check that every representation has
   one status and every refusal has a reason. This is documentation and Python
   only.
2. **Typed lowering.** Lower typed machines through elaboration, with
   `qualify` on `examples/relay_typed.zal` and planted value-mapping controls.
3. **Extension parser and interpreter.** Implement `zal/app-effects/1` in both
   surfaces, with the reference table including deliveries and the six
   controls above.
4. **Lowering of sends and `sends`.** Lower sends and `sends` requirements to
   outboxes, channels and delivery laws. Run `qualify` and `refine-check` on
   the refund example.
5. **Plan review.** Add the `zal/effect-plan/1` artifact, the review tool and
   the terminal acceptance flow. Test that acceptance can never come from a
   tool.
6. **UI confirmation.** Add `ui.channels` and the delivery sentence in the UI
   confirmation, with its renderer tests.
7. **Runtime journeys.** Run the six journeys on the generated refund
   application and retain their logs.

Each step's checker identity, controls and claim scope are recorded with its
result. Steps 3 to 7 change the ZAL checker identity and the reviewed pins
that bind ZAL sources. Their producers must refresh those pins; no pin is
edited by hand.

## Scope

This document is not a proof and not an implementation.
- The assessment rows are observations of the stated checkout. Where a command
  result is quoted, it was run on that checkout.
- The design states the claim a future bridge would make, its checks, its
  assumptions and its refusals.
- Even when implemented, the bridge claim is exhaustive agreement over one
  model's declared finite domain. It does not cover arbitrary ZAL
  representations, external systems or a deployed application.

## Lineage

- Delivery after an atomic commit through a durable outbox follows the
  transactional outbox pattern: Chris Richardson, *Microservices Patterns*,
  Manning, 2018, chapter 3.
- Idempotent receivers that deduplicate by message identity follow Gregor
  Hohpe and Bobby Woolf, *Enterprise Integration Patterns*, Addison-Wesley,
  2003.
- Checking each generated artifact against a reference, rather than proving the
  generator, follows translation validation: Amir Pnueli, Michael Siegel and
  Eli Singerman, *Translation Validation*, TACAS 1998.
- The controlled-language approach is cited in [lowering](LOWERING.md#lineage-and-scope).

No prior-art search has been run for the combination of a controlled-language
message declaration, an exhaustive plan review and a generated confirmation
sentence.
