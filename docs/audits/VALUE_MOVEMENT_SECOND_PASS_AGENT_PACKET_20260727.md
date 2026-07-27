# ZenoFCIS value-movement second-pass agent packet

Use this packet after reading `VALUE_MOVEMENT_SECURITY_AUDIT_20260727.md`.

## Mission

Attempt to invalidate every claimed value-authority relationship at exact source revision `47c3b659dda8dbd37f3294d090554cb3b2493bbb` and the exact heads of the open on-chain stack.

Do not report that a type is immutable, canonical, pure, or bounded unless that property prevents the attack being tested. The central question is always:

> Can any state change, authoritative effect, or external delivery reach a production shell without proving the exact project predicate, invocation, catalog, provider, interpreter, deployment, and invariant set?

## Required output for every finding

```text
ID and severity
exact file/symbol
attacker-controlled inputs
trusted assumptions required by the exploit
minimal construction/reproduction
expected behavior
actual behavior
value at risk / blast radius
smallest safe repair
regression, mutation, and differential tests
whether the finding invalidates a release claim or only one optional profile
```

Do not merge unrelated repairs into one patch.

---

## Pass A — attempt direct authority construction

### A1. Raw bundle construction

Inspect:

```text
crates/zeno-fcis-plan/src/lib.rs
crates/zeno-fcis-receipt/src/lib.rs
crates/zeno-fcis-shell/src/lib.rs
crates/zeno-fcis-shell-sqlite/src/lib.rs
fuzz/fuzz_targets/candidate_bundle.rs
```

Attempt to create and commit:

- a whole-root state replacement;
- an owner change;
- a balance increase;
- an uncatalogued effect;
- a catalogued effect with incorrect authority/subject/payload relation;
- an outbox delivery not produced by the catalogued transition builder.

Stop condition: any such object reaches a production commit API without a private catalog authorization witness.

### A2. Alternate constructors and conversions

Search all public constructors and consuming conversions for:

```text
CommitBundle
CandidateBody
Receipt
CandidateId
CandidateBindings
CanonicalPatch
CommitPlan
OutboxPlan
Effect
OutboxEntry
TransitionArtifacts
```

Check feature combinations and re-exports. A hidden constructor in one crate is insufficient if an equivalent public reconstruction exists elsewhere.

### A3. Decoder reconstruction

For every strict decoder, verify that success reconstructs through the same private smart constructor used by trusted code and then requires exact complete-input re-encoding.

Attempt:

- alternate ordering;
- duplicate IDs/ordinals;
- unknown tags;
- trailing bytes;
- nested over-limit values;
- aggregate-limit evasion through many individually valid values;
- semantic map key / encoded key disagreement;
- zero and maximum-length values.

Record every authoritative type with no strict bounded decoder.

---

## Pass B — invocation and context substitution

Inspect:

```text
crates/zeno-fcis-transition/src/lib.rs
crates/zeno-fcis-bootstrap/src/templates.rs
crates/zeno-fcis-schema/src/envelope.rs
crates/zeno-fcis-project/src/lib.rs
crates/zeno-fcis-adapter/src/lib.rs
crates/zeno-fcis-adapter-zenodex/src/zusd.rs
```

Construct two valid invocations that share a pre-state but differ in exactly one field:

- command;
- caller/signer;
- replay nonce;
- time/height/slot;
- oracle value/source/publication/finality;
- configuration/governance version;
- catalog/profile/state domain;
- provider/interpreter/deployment identity.

Swap their artifacts. Any validator that succeeds by reading its own `actual.command_hash` or `actual.context_hash` instead of an external expected witness is incomplete.

Search for raw booleans passed to reason/predicate APIs. Mutate each predicate result independently from the command/state/context and determine whether the library can detect the lie.

---

## Pass C — invariant and conservation attack

For each project/profile effect, write the accounting equation before testing it.

Examples:

```text
post_balance = pre_balance - transfer - fee
sum(post balances) - sum(pre balances) = minted - burned
vault delta = -external transfer - explicit protocol fee
recipient delta = exact transfer unless a reviewed token profile says otherwise
```

Mutate one term at a time:

- amount by ±1;
- sign;
- scale/decimals;
- asset identity;
- recipient;
- authority;
- subject;
- fee;
- ordinal/count;
- mint/burn classification;
- rounding remainder.

Require rejection before authorization. Shape validation is not a successful result.

Test all reason combinations. Ordinary rejection must erase every staged write/effect/outbox obligation. Committed failure must retain only explicitly reviewed failure-state movement.

---

## Pass D — persistence and replay corruption

Inspect the complete SQLite schema and every query/update.

Create a valid database, then mutate one relation at a time:

- extra bundle;
- extra replay binding;
- replay points to another candidate;
- replay bundle bytes differ from bundle row;
- changed or missing receipt;
- extra outbox row with recomputed local hashes;
- missing outbox row;
- changed destination/payload/channel/ordinal;
- acknowledged flag changed;
- duplicate delivery semantics under another ID;
- state/root/version mismatch;
- valid state/root under the wrong profile/domain/interpreter.

Reopen and call every public read, replay, delivery, and acknowledgement path. The shell must fail before returning or delivering attacker-created authority.

Also repeat every mutation at every crash injection point.

---

## Pass E — composition and parallelism

Inspect:

```text
crates/zeno-fcis-compose/src/lib.rs
crates/zeno-fcis-transition/src/lib.rs
```

### Frames

Test exact, ancestor, descendant, sibling, and wildcard paths. Authorization must use directional containment; conflict detection may use symmetric overlap.

### Effects

Create state-disjoint components with:

- same transfer authority and asset;
- mint and burn on one supply;
- two transfers from one allowance/vault;
- same external destination/nonce;
- different operation IDs that affect the same economic resource.

Promotion must fail unless an exact commutativity proof is verified.

### Evidence

Set caller-supplied sequential and composed result hashes equal without running anything. This must not prove parity.

Change the provider guarantee set on an assumption discharge while reusing its artifact. This must invalidate the claim.

Ensure all evidence claims bind exact spec, source, toolchain, assumptions, coverage, and deployment.

---

## Pass F — authenticated-state boundary

Inspect:

```text
crates/zeno-fcis-authenticated/src/lib.rs
```

Test:

- same tree leaves under different projector implementations;
- same root under different profile/tree/version metadata;
- proof replay against another expected key/root/version/profile;
- plan replay after version/root changes;
- semantic patch changed while authenticated post-leaves remain attacker-selected;
- stale-node list disagreement with actual writes.

A successful proof API should return a nominal verified witness only after comparing every external expected binding.

---

## Pass G — Solidity

Use the exact generated retained fixture and compiler pin.

Test:

- direct token versus proxy token;
- proxy implementation/admin/config change with unchanged proxy address/code;
- fee-on-transfer, rebasing, callback, false-return, no-return, blacklist, pause, and burn-on-transfer mocks;
- reentrancy through token hooks;
- pre/post source and destination balance deltas;
- effect amount differs from semantic debit;
- fixed-shape zero, partial, and full plans;
- chain ID, contract address, implementation, and deployment binding substitution;
- compiler optimizer/via-IR setting variation;
- replay across deployments with the same shared machine hash.

No exact-transfer profile may pass solely because `SafeERC20` did not revert.

---

## Pass H — Solana/Anchor

Test every account constraint by substitution, including recomputed attacker-controlled values:

- state PDA seed/bump/authority;
- actor signer;
- mint;
- source vault;
- destination owner;
- Token versus Token-2022 program;
- transfer fee and transfer-hook extensions;
- permanent delegate/default account state/confidential transfer/interest-bearing extensions;
- program-data hash and upgrade authority;
- stale sequence/root;
- compute exhaustion at exact legal bounds;
- state debit versus CPI amount;
- transaction retry and duplicate execution.

Use LiteSVM or Mollusk plus program-test. Source generation and host compilation are not instruction-level evidence.

---

## Pass I — static and mutation checks

Extend repository checks to fail on newly introduced production call sites of:

```text
CandidateBuilder::seal
Effect::new
OutboxEntry::new
shell::commit(... CommitBundle ...)
SqliteShell::commit(... CommitBundle ...)
```

Use a strict allowlist for tests/reference tooling until the APIs are redesigned.

Mutation operators must include:

- `covers` ↔ `overlaps`;
- `==` ↔ `!=`;
- omitted binding field;
- skipped invariant;
- skipped catalog validation;
- removed precondition/root/sequence check;
- amount ±1;
- wrong ordinal/order;
- omitted outbox row;
- changed domain/version/provider;
- active/no-op slot confusion.

A security-critical test suite is incomplete if these mutations survive.

---

## Final promotion checklist

Do not recommend the guarantee unless all answers are “yes” at one exact release candidate:

- [ ] one exclusive production authority path;
- [ ] raw bundles cannot commit;
- [ ] exact invocation/catalog/provider/interpreter/deployment binding;
- [ ] strict complete artifact decoding;
- [ ] state/effect/outbox invariants and conservation proved or checked;
- [ ] crash/replay/persistence completeness;
- [ ] effect/outbox parallel conflict model;
- [ ] verified evidence claims with exact coverage;
- [ ] generated chain code compiled and instruction-tested;
- [ ] deployed build identities verified;
- [ ] no unresolved P0/P1 findings;
- [ ] independent review performed against the exact head after all fixes.
