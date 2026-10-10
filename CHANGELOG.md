# Changelog

All notable Rust API and packaging changes are recorded here. Canonical
protocol compatibility is governed separately by the identifiers and versions
embedded in ZenoFCIS values.

## 2.2.0-rc.1

This candidate packages the available checked application path and the ten
closed finite component families. It does not complete the original stable
V2.2/V2.3 scope. Cargo/internal dependency versions and npm root metadata are
coherent; external dependency identities are unchanged. Evaluator identities
and finite runtime bindings are refreshed for the candidate source. See
[release notes](docs/RELEASE_NOTES.md) for installation and assurance limits.

## Changes included in the candidate

- Migration admission now checks freshly compiled target claims on the mapped
  current state and rechecks them during history replay. Records with claims
  use format 2 and bind the exact evaluated claim programs and review; empty
  claim sets retain format 1. Refused multihop upgrades write nothing.
- Historical reports use the original audited genesis and committing inputs,
  with each version's checked schema labels. Migrations at zero commits or
  after the last commit are replayed by the existing store audit. A journal
  with different stored inputs now refuses even when it could reach the same
  state; historical reports grant no publication or delivery capability.

- Connect generated applications to the reference relay with
  `deliver DATABASE --relay CONFIG.json`. The bounded worker sends original
  delivery IDs and canonical payloads by HTTP or queue append, and the
  application's typed lifecycle acknowledges only after success. Crashes
  leave entries pending for retry; transport is at least once and receiver
  idempotency is required for one effect. Add `operational-journey`, which
  runs a real generated spend-approval application through creation,
  submissions, relay crash/restart, a live rule change, a data migration,
  and replay of all three contract segments. It checks refused changes
  leave files unchanged and compares retained delivery IDs and payloads.

- Add an operational command line to applications built with `zeno-fcis new
  --contract`: `init`, `submit`, `decide` (a dry run), `state`, `history`,
  `pending`, `deliver` and `version`, each with `--format human|json` and
  exit codes 0, 1, 2, 3 and 64. Commands and context fields are written by
  name and checked against their declared domains by the decision-examples
  grammar; a reject or a refused input writes nothing. Every command opens
  the store through the contract lineage. `deliver --to FILE` appends each
  pending delivery to a file as one JSON line, synchronized to disk before
  the store acknowledges it, and keeps one line per delivery ID; the file is
  the one implementation of the application's `Destination` trait, the
  extension point for other transports. `history` reads a submission
  journal beside the store and checks it against the store's audited original
  contract identities and inputs. `submit` holds an
  exclusive lock on the journal across reading the store's head and
  committing, and removes its line again when the commit does not happen,
  so two submissions at once can no longer leave a journal line for a
  commit that lost the race; `history` reads under a shared lock and
  refuses a journal whose commit numbers do not strictly increase.
  `submit` creates the missing journal of a store at genesis, such as one
  an interrupted `init` left, and refuses the missing journal of a store
  with commits, naming the remedies: restore it, or create it empty and
  accept that `history` refuses those commits. A
  `deliver` that fails part way through names the deliveries it already
  sent and acknowledged. Generated package and
  binary names now include the contract version, such as
  `spend-approval-v1`, so builds of two versions share no library build;
  the replay keys `--decide` derives change with the name. The forms
  without a command, including `--decide`, are unchanged.

- Add a delivery relay protocol to the SQLite shell. In the new
  `v2::relay` module, `V2SqliteShell::export_pending` lists every pending
  delivery in commit order, after replaying each owning commit, and writes
  nothing; `relay::export_line` writes one as a line of JSON
  (`zeno-fcis/relay-export/1`) with the delivery ID, channel, destination,
  payload, the payload's SHA-256 and its place in commit order. An
  acknowledgment from outside the process goes through the typed lifecycle:
  `relay::acknowledge` calls the new `V2SqliteShell::pending_by_id`,
  `Pending::relayed` and `Delivered::acknowledge`. It is bound to the
  delivery ID and the payload's SHA-256, and three new `v2::Error` variants
  name its refusals: `UnknownDelivery`, `PayloadMismatch` and
  `AlreadyAcknowledged`. A refusal changes no delivery and no commit.
  Existing delivery calls behave as before. Two standard-library scripts
  use the protocol: `tools/relay.py`, the reference relay, sends each
  delivery by HTTP POST with the delivery ID as the idempotency key, or
  appends it to a queue file, retries with bounded backoff and then
  acknowledges it; `tools/relay_receiver.py` is an idempotent test
  receiver that can fail or time out on request. Transport is at least
  once, every attempt carries the same delivery ID and payload, and a
  receiver that honours idempotency keys sees each effect once. The
  withdrawal-queue template's application gains `--payouts`,
  `--relay-export` and `--relay-acknowledge`, and the app journey check
  delivers its two payouts through the real store and the relay to the
  test receiver, exactly once each, across injected crashes after the
  export, after a send and after an acknowledgment, a receiver failure and
  a receiver timeout. The relay follows no HTTP redirect, since urllib
  would resend a redirected POST as a GET without its body; a 3xx answer is
  a final refusal and the delivery stays pending. A queue append does not
  merge a cut line with the next one, and a queue consumer skips any line
  that is not complete JSON. An acknowledgment through `Pending::relayed`
  rests on the relay's report of its call, not on the types. The relay's
  `--timeout` limits a whole HTTP call, not each socket read: a receiver
  that answers a byte at a time is cut off at the deadline and the delivery
  stays pending (the test receiver's `--hang-mode drip` checks this), and
  `--command-timeout` kills a store command that hangs. The relay reads no
  proxy settings. No dependency was added.
- Add data-migrating upgrades (G2). `zeno-fcis contract evolve <app> --to
  <contract> --migration m.json` takes a layout change, or a rule change, as
  a data migration: `m.json`, schema `zeno-fcis/migration/1`, gives every new
  state field an old field's value (carried or renamed), a default, or a
  value from a total table over an old field (a split), and drops no old
  field. It is admitted only by forward simulation, which the new
  `v2::migration` module of the SQLite shell runs and the CLI compiles from
  the shell's own source: over every state of the old contract's declared
  domain on which its state laws hold and every command and context, the new
  Authority's publication over the migrated state must give the old one's
  technical refusal or decision class and reason, the same deliveries, and
  a successor equal to the migrated old successor, the old genesis state
  must map to one the new genesis laws admit, and every new state law must
  hold on every migrated state (observation `new-state-laws`, so a state a
  behaviour change kept cannot be migrated into one the new laws forbid). A domain above 2^20 tuples, the
  review's cap, is refused as inconclusive; no solver evidence is accepted.
  The migration is kept as `v2/evolutions/N/migration.json` and bound in the
  rules entry (`kind`, `migration_sha256`); every generation simulates it
  again. `--shortcut` adds a migration from an earlier version, admitted only
  when it agrees with the composed route of consecutive migrations on every
  state checked. Without `--migration`, a `rename` evolves as the exact rename
  tier. A superseded version whose schema a migration or rename changed keeps
  `v2/schema_v{k}.zcve`, and the current source declares `STATE_STEPS`;
  a lineage without either generates byte for byte as before. The shell gains
  `v2::Step::Migration` and `v2::Step::Rename` and two upgrade kinds,
  `migration` (magic `ZFCISV2-MIGRATION`) and `rename` (magic
  `ZFCISV2-RENAME`), which move the head's state to the new layout; an
  upgrade records one hop per such step and one per run of other steps. The
  migration record binds the SHA-256 of the migration's canonical encoding,
  the simulation's counts, the observations compared and the migrated
  state's root; the row stores the encoding and the state. Audits re-run the
  simulation, once per lineage value, and re-derive the state; a lineage
  that declares another step refuses the store as
  `upgrade::Unsupported::Migration`. A store's checkpoint saved before a hop
  at the same head now opens as `Error::Checkpoint`, since the chain is
  compared before the root. The classifier's `rename`, `layout-change` and
  rule-change texts and paths now say G2's paths exist. A generated
  application's `--upgrade` report gains `migration`.
  `tools/check_contract_migrate.py` and the `data-migration-upgrade`
  acceptance scenario migrate live spend-approval stores, deliver a pending
  payment with its original ID, keep committing, rename at any state, and
  show every refusal. A refusal of a given migration file names the file
  given with `--migration` or `--shortcut`, not the path it would be kept
  at, and a migrated state without an encoding under the new framing is
  reported as such rather than as a field without a value.
- Add rule changes for live stores (G14.1). `zeno-fcis contract evolve
  <app> --to <contract>` classifies the change with the `contract diff`
  classifier and refuses any kind but `rule-change`, naming it. For a rule
  change it records the classifier's plain-language account as the owner's
  review in `v2/evolutions/N/review.txt`, keeps the replaced contract and its
  adoptions under `v2/evolutions/N/`, writes the new rules with an
  `evolutions` entry binding the replaced policy's and the review's SHA-256,
  and regenerates the whole lineage. Generation regenerates every replaced
  contract, numbers the lineage from 1, recomputes each review and refuses an
  edited one; a contract that never evolved is generated byte for byte as
  before. The SQLite shell gains a third upgrade kind, `behaviour-change`
  (magic `ZFCISV2-BEHAVIOUR`), taken whenever the lineage, bound with the new
  `v2::Lineage::bind_steps` and `v2::Step`, declares a behaviour change
  between the two versions: every state law of the new contract and every
  inductive claim the lineage declares for it must hold on the store's
  state, evaluated by the library's law evaluator through the core's genesis
  framing (`v2::behaviour`), with no program comparison. Genesis exactness,
  law 990, is not evaluated and the record lists it as such. The record binds
  the law and claim IDs that held and the review digests, the upgrade row
  stores the review texts, and an audit evaluates the laws again at the
  recorded state. A failing law or claim refuses with nothing written
  (`upgrade::Refusal::Behaviour`); a lineage that declares other reviews for
  the step refuses the store as `upgrade::Unsupported::Reviews`. After the
  upgrade every law holds on every later committed state because the
  Authority checks it at each commit, and each claim whose induction step
  holds holds from the upgrade on; facts that rest only on reachability from
  the new contract's genesis do not carry over. `v2::Lineage::audit_read_only`
  audits a store at any version without writing, and a generated
  application's `--audit` now uses it, so it no longer saves a checkpoint.
  Its `--upgrade` report gains `behaviour`. `tools/check_contract_evolve.py`
  and the `rule-change-upgrade` acceptance scenario run the study's escrow
  dispute-window change, 14 to 30 days, on a store with committed history.
  The classifier's `rule-change` admission text and its
  `g14.1-behaviour-change` path now say the path exists. `v2::Lineage::receipts`
  is replaced by `v2::Lineage::steps`, since a step need not be an adoption;
  `v2::Lineage::bind` still binds a lineage of adoptions alone.
- Fix `zeno-fcis loop`, whose resume could run checker work without
  charging it. `resume`, `candidate` and `run` first resume the session,
  which replays its checked incumbent and stored counterexamples. The
  replay's work reservation reached the ledger only after the replay
  finished, so an interruption during the replay left the session's
  allowance unchanged, and repeated interruptions repeated checker work for
  free. A `run` refused for its `--script` or `--hosted-config` after the
  replay also left it uncharged. The reservation is now written to the
  ledger before any replay work runs, and the replay can start only after
  that write succeeded; an interrupted replay stays charged. Every resume
  still reserves and pays for its own replay and never reuses an earlier
  one; when the remaining session work cannot cover it, the command exits 2
  with `resume-inconclusive` (`insufficient-replay-allowance`) and writes
  nothing. The ledger format is unchanged, and an uninterrupted session
  writes the same ledger and report as before. This concerns work
  accounting only: the loop still installs no memory caps and does not
  cancel process trees.
- Fix store upgrade admission and audit. A `program-successor` upgrade was
  admitted when the two contracts' policies differed only in the decision
  program and Step limit, with nothing else required: a successor with a
  Step limit of zero, or with a program that decides differently on some
  input, was admitted at any state, and the audit took the receipt digests
  from the store's own record. Now `v2::upgrade::Successor::establish`
  constructs a successor only when the shell itself establishes all five
  premises from the two bound catalogs: the policy comparison; law 991
  declared as a decision-conformance law on every decision and required, in
  both; both Step limits covering every program and law node; no law
  observing Step usage; and the two decision programs equal on every input
  tuple. The last is the new pure `v2::equivalence::compare`, the predicate
  of `zeno-fcis transform check` at the full Step budget: every tuple of the
  ordered product of the declared input domains runs through the library
  evaluator, with checked domain sizing and an odometer instead of a list of
  tuples, up to a cap the lineage carries, 100,000,000 tuples by default or
  another value through `Lineage::bind_with_cap`. A CLI test compiles the
  module and checks that it gives the same verdict as the transform checker
  on its known answers, the benchmark cases, planted defects and the
  withdrawal-queue adoption, which it compares on 1,296,000 input tuples.
  When a premise is missing the upgrade takes the genesis route, and a
  refusal, which writes nothing, now names the missing premise:
  `upgrade::Refusal::Genesis` carries `missing` and `refusal`. The audit
  establishes all five premises again from the auditing lineage and requires
  the recorded receipt digests to be that lineage's, value for value. An
  open that fails either check ends with the new `Error::Succession`, which
  carries `upgrade::Unsupported::Receipts` or the missing premise and says
  the store may be intact, instead of `Error::History`. The
  digests are provenance: the shell replays no receipt. The record's
  admission is now the premises byte `0x1f`, the number of tuples compared
  and the digests, and `UpgradeReceipt::premises` returns that evidence;
  2.1 was never released, so there is no compatibility path. The
  application's upgrade report lists the five premises and
  `programs_equal_on_input_tuples`. Every process that upgrades a store, or
  fully opens one holding a program-successor record, compares the two
  programs once: a bound `Lineage` keeps each pair's outcome in memory
  (`Lineage::comparisons` counts its enumerations), and programs with
  identical instructions and roots are equal without enumeration. No
  comparison runs while a write transaction is open: the needed pairs are
  established from plain reads first, the transaction only looks them up,
  and a pair that another connection's change made necessary in between is
  established before a retry, bounded by the number of versions, beyond
  which `Error::Unsettled` is returned with nothing written. Generated
  application manifests optimise `zeno-fcis-synthesis` in dev builds, with
  overflow checks and debug assertions still on. For the withdrawal queue
  one comparison takes about 3 seconds in a release build and about 4 in a
  debug build.
- Add `zeno-fcis contract diff OLD NEW [--format human|json]` (G8). It
  generates both contracts as `generate contract` does and decides exactly
  one kind of change, the first that holds in a fixed order: `identical`
  (byte-identical canonical policies), `program-successor` (only the decision
  program and its Step limit differ, the comparison the SQLite shell makes
  for an F6.1 upgrade), `rename` (only project, type, field or variant names
  differ), `layout-change` (the state layout differs, every other type and
  channel does not), `rule-change` (the schema and channels are the same; the
  laws, cases, reasons or genesis state are not) and `unrelated`. Its
  plain-language summary names every changed type, field, variant, channel,
  reason, law, case, genesis value, program and limit, lists differences
  outside the contract as notes, and states the admission path the kind
  needs and whether it exists today: only the F6.1 upgrade admits a store at
  any state; a rule change names both G2's forward simulation and G14.1's
  behaviour-change upgrade. It decides only the structural kind and runs no
  decision, so it never shows that a change preserves decisions. The JSON document has the versioned schema
  `zeno-fcis/contract-diff/1` and is the same for the same contracts wherever
  they are. `contract adopt` now classifies the change from the superseded
  version to the new one and refuses, before any write, every kind but a
  program successor, naming the kind. The CLI's unit tests compile the
  shell's upgrade and equivalence modules from their own source and check, on
  every planted pair of `tests/fixtures/contract-diff/pairs.json` and both
  adoptions, in both directions, that the shell's policy comparison
  (`upgrade::program_successor`, premise 1) accepts exactly the pairs the
  classifier calls identical or program successors, and that every pair the
  shell's Tier A admission admits is a program successor. The converse does not hold: Tier A also needs
  law 991, both Step premises and the two programs' equivalence, which the
  classifier does not check. The eight templates' generated contracts are
  byte-identical.
- `tools/check_compile_fail.py` checks why each misuse example of the SQLite
  shell's delivery lifecycle fails, on the pinned stable toolchain without
  `RUSTC_BOOTSTRAP`. It compiles every example from the doc comments rustdoc
  runs, after confirming they are the blocks rustdoc lists, and reads rustc's
  JSON diagnostics: each misuse must fail with exactly one error carrying the
  code its fence and prose state, and its paired example must compile. Planted
  controls in `tools/test_check_compile_fail.py` show that a misuse failing for
  another reason, failing with an extra error, or compiling is refused. The
  ATDD SQLite scenario and the strict SQLite history workflow run both; the
  `RUSTC_BOOTSTRAP` instruction in `docs/V2_SQLITE_STAGE.md` is replaced.
- The SQLite shell's delivery is now a typed lifecycle of consuming tokens,
  `Pending` → `Delivered` → acknowledged. `V2SqliteShell::next_pending`
  issues the oldest pending entry as a `v2::Pending` token, `Pending::deliver`
  hands it to the library memory destination and returns a `v2::Delivered`
  token, and `Delivered::acknowledge` marks the entry acknowledged. Only the
  store makes a token, and each token holds the exclusive borrow of the
  handle that issued it. Acknowledging an undelivered entry, acknowledging
  twice, reusing a consumed token, making a token outside the crate, taking a
  second token from a handle while one is live, and keeping a token past its
  handle therefore do not compile; rustdoc `compile_fail` examples on
  `Pending` and `Delivered` show each, beside a compiling example written the
  same way apart from the misuse. The run-time checks are those of the calls
  they replace: the store is audited and the owning commit replayed before a
  token is issued and again before an acknowledgment, which compares the hash
  the destination reported with the stored one. Two handles on one file still
  issue their own tokens. The record that was `v2::Pending` is now
  `v2::Delivery`, plain data that grants nothing. `acknowledge(delivery_id,
  observed)`, `deliver_next_memory` and `deliver_next_memory_unacknowledged`
  are removed, with no shim; deliver every pending entry with
  `while let Some(pending) = shell.next_pending()? { pending.deliver(&mut destination)?.acknowledge()?; }`.
  The eight templates, the contract application's session and `--deliver`,
  and the oracle's current journey copies (`normal.rs`) use the tokens; its
  frozen `original/` snapshots, which nothing in the repository builds, keep
  their old text. Delivery IDs, commit order, acknowledged state and every
  stored row are unchanged.
  The stores `--deliver` writes, before and after an upgrade, and the
  prepared-counter journey's store are byte-identical. The other journeys no
  longer repeat a saved acknowledgment after reopening their store, since a
  token cannot outlive its handle, so their stores differ only in the SQLite
  header's two change counters; replaying that one statement reproduces the
  old bytes.
  `docs/V2_SQLITE_STAGE.md` states what the types enforce, what stays a
  run-time check, and how G9's relay will use the API.
- Add `zeno-fcis contract check-symbolic` and `transform check --symbolic`
  for domains too large to enumerate (`docs/SYMBOLIC_CHECKS.md`). They run
  the pinned CVC5 and Z3 through the formal-tools adapter, one query per
  committing case and state law, or per case of the original program.
  `check-symbolic` takes an optional strengthening file
  (`zeno-fcis/strengthening/1`), assumed on every pre-state and checked on
  every successor and on genesis, and writes a `zeno-fcis/symbolic-check/1`
  report. A counterexample counts only after it replays through the library
  evaluators and the bound Authority (Checked, exit 1). A solver-only
  "holds" is attested by CVC5, corroborated by Z3, not proved: CVC5's proof is
  not checked, and the command exits 2. `unknown`, timeouts, unsupported
  constructs, solver disagreements without a replayed model, and planted
  controls that are not refuted are inconclusive (exit 2). Where the domain
  fits the enumeration cap, `check-symbolic` also enumerates it, and
  enumeration decides (Proved, exit 0); `transform check --symbolic` refuses
  such a domain. On the app study's escrow (more than 2^128 tuples), funds
  conservation with the strengthening "a Created escrow has paid nothing out"
  holds on all 9 committing cases, attested by CVC5 and corroborated by Z3;
  without the strengthening it is refuted on the Fund case with a replayed
  counterexample. The symbolic transform receipt
  (`zeno-fcis/transform-symbolic-receipt/1`) is refused by `transform
  replay`, `contract adopt`, `contract refresh-receipts` and generation.
  `transform check` without `--symbolic` is unchanged; `describe transform`
  now declares its optional tool execution and files.
- `contract review` no longer lists a declared range in full unless the
  domain fits its tuple cap: the domain's values come from a bounded API that
  checks the size before allocating. A domain with an empty position beside a
  wide range is now an empty input set without listing the wide range. This
  is meant to preserve behaviour: review packets are byte-identical to the
  previous build's on the 8 templates and the 3 contract fixtures. The bug it
  removes made `contract check-symbolic` request 32.8 GB for the escrow's time
  range before this release; a regression test now runs that check under a
  1 GiB address-space limit.
- `zeno-fcis-formal-tools` adds `SmtSession`, `ScriptAnswer` and `ScriptRun`:
  a CVC5 or Z3 admitted once with the same hash, version and private-copy
  checks as `execute_tool`, running caller-built SMT-LIB scripts through the
  same two-run protocol. It classifies answers without replaying them.
- `zeno-fcis new` now binds every Cargo application it writes, from
  `--contract` and from the `durable-counter`, `prepared-counter` and example
  templates, to a ZenoFCIS source tree. Before, a generated application
  resolved its exact version pins to the crates published under the same
  versions, which are a different release, and failed to build; only
  `tools/check_generated_application.py` knew the binding. `new` appends a
  `[patch.crates-io]` section with a path entry for every ZenoFCIS package
  the application needs, directly or through other ZenoFCIS packages, copies
  the tree's `Cargo.lock`, and copies its `rust-toolchain.toml`. The tree is
  the one the new `--source TREE` option names, else the tree the CLI was
  built from while it still exists; it must be a Cargo workspace whose
  members provide every needed package at the CLI's version. Without a tree
  `new` exits 2 and names `--source`; with a tree that cannot bind the
  application it exits 1; either way it writes no file. A CLI built with
  `ZENO_FCIS_BUILD_TREE` set records that path instead, or none when it is
  empty. `tools/rc_package.py build` builds the release binaries with it
  empty, so they hold no build directory; with an installed binary, pass
  `--source <extracted source tree>`. The generated README lists
  `cargo test --offline` and `cargo run --offline -- NEW_DATABASE_PATH`.
  `tools/check_generated_application.py`, `tools/check_contract_upgrade.py`
  and `site/build.py` now check the binding `new` wrote instead of writing
  one; the site then leaves it out, since its own workspace resolves the
  applications' pins. The template READMEs give the same steps.
- Fix contracts the generator accepted but the library refused. Each
  channel's idempotency domain now covers every `idempotency_ordinal` the
  rules use on it, where it was fixed at 0, so a nonzero ordinal no longer
  fails the catalog binding. The Effect limit is now the most deliveries any
  case makes, at least 1, where it was fixed at 1, so a case with two
  deliveries no longer has every decision refused at run time. Generation
  refuses a case whose deliveries are not in increasing ordinal order, and a
  contract with a committed-failure case but no `CommittedFailureEffects`
  law, naming the case: framework law 908 would refuse every committed
  failure. It also refuses framework law ID 0, which the library refuses.
  When the library's catalog still refuses a contract, the error
  names the case delivery, law or channel without which the library admits
  it, beside the library's own error, found by binding the contract again
  without each; the library's error types are unchanged. The eight
  templates' generated contracts are byte-identical. The app study's
  original escrow, whose split pays out in two deliveries with idempotency
  ordinals 0 and 1, is a CLI test fixture and builds and runs its 23
  examples.
- Add `zeno-fcis contract export-program [DIR] --out FILE`: the contract's
  current decision program, in the canonical encoding `optimize`,
  `transform` and `loop` read, written to a new file. After an adoption it
  is the adopted candidate. The output is a function of the contract's files.
  `v2/policy.zcve` is a whole policy, which those commands refuse, so the
  optimization journey needed a script that parsed generated Rust.
- An application built from a contract can now keep deciding on an existing
  store: `--decide DATABASE_PATH` on a store at this build's version
  continues the session from the store's state, with a replay key of its own
  for each commit; without a file it starts at genesis as before. A store at
  another version is refused and left as it was. `--decide` is an aid for
  acceptance tests and maintenance, not an operational interface, which is
  planned for 2.2.
- Add `tools/check_app_journey.py` and the ATDD scenario `app-journey`, with
  the app study's escrow and spend-approval contracts as CLI test fixtures.
  In new directories outside the repository, and with the command lines
  alone:
  - `zeno-fcis new` builds the escrow, whose split pays out in two
    deliveries with idempotency ordinals 0 and 1, and the commands its
    README lists, run exactly as written, build it, check its 23 examples
    and run its session;
  - `contract export-program`, `optimize`, `transform replay` and
    `contract adopt` make the spend-approval contract's version 2;
  - a version 1 store with four commits and a pending payment upgrades as a
    program successor at commit 4 and delivers the payment under its
    original identifier, after which the version 1 build refuses the store;
  - a version 1 store at commit 2 upgrades and keeps committing under
    version 2 with `--decide`, and its audit replays both segments;
  - a CLI built as the release build builds it holds no path of the
    checkout and refuses `new` without `--source`; the same build without
    the variable, the control, holds it.
- Add `docs/CONTRACT_RULES.md`, the reference for `v2/policy.json`: every
  key, leaf bindings, expressions with `choose` and `div_floor`, the law
  kinds and the scopes five of them require, framework laws 908, 909, 990
  and 991, `idempotency_ordinal`, roots, full post-states and reasons. A test
  checks its tables against the keys, leaves, classes, operators, functions
  and law kinds the generator reads. The CLI reference recommends release
  builds for review, transform and optimize, which the app study measured
  4.7 to 7.5 times faster on reviews; documents that `contract adopt`
  renders the whole rules file again; and `generate contract --help` now
  lists `v2/schema.zcve` among the files it writes. The generated
  application's `bundles` count is documented as the committed decisions,
  without genesis.
- `contract review` reports refusals, in packet schema
  `zeno-fcis/contract-review/2`. Each refusal names its class (law, domain,
  arithmetic, meter, input or other) and, when a law refused, the law the
  library's own law diagnostics report. For each refused input, the
  library's law evaluator runs the contract's law programs on the pre-state
  as a genesis state and decides whether it satisfies every state law: each
  law that applies at genesis and to every committing decision. A law
  refusal on a pre-state that satisfies every state law is a finding: the
  review exits 1 with status `law-refusal` and names the law, the number of
  inputs, the first of them and the case the decision program selects for
  it. Refusals on pre-states the state laws exclude are counted apart, and
  refused rows name the state law they break. The summary counts refusals
  by class and by pre-state. The review still does not decide reachability:
  a pre-state that satisfies every state law may be unreachable. The
  app-building study's planted bug in its spend-approval contract (the CFO
  check moved from tier 1 to tier 2), which the review passed with no
  finding, is now a finding: law 500 refuses 16 inputs. The unchanged
  contract, now a test fixture, and the eight templates review with no
  finding. Mutants still compare refusals as the library reports them, not
  by law.
- One decision-examples grammar. `contract-app/src/examples.rs`, which
  `new --contract` copies into every application, parses the examples in
  the application, and `contract review` compiles the same file. Both now
  accept inputs split over several `|` sections, a delivery written as its
  payload alone when the contract declares one channel, and indented
  comment lines, and both refuse every other line with the same message.
  Generated applications used to refuse all three, although the review
  accepted them. Every number is checked against
  its declared domain while parsing, the successor state's included. The
  grammar is documented in `docs/CLI_REFERENCE.md`. Every template's
  examples parse to the same examples as before.
- The SQLite v2 shell's errors display as one line saying what happened and
  what to do, instead of `V2 SQLite refinement refused: Identity` and the
  like. `Debug` keeps the variant names. Applications built from a contract
  print `store:`, that message and, in parentheses, the error's `Debug` form,
  which starts with the variant name, for example `store: upgrade refused:
  the store already runs this contract version, so there is nothing to
  upgrade (Upgrade(SameContract))`.

- Repair the V2 CI failures found on pull request 119. Every job that runs a
  repository tool with `cargo --offline` first fetches the root, verification
  and resolved-purity lockfiles through one shared action. The verus workflow
  no longer reads the step-only `runner` context in job-level `env`, which made
  GitHub reject the file, and the static workflow check now rejects any
  context GitHub does not allow at its key. Miri interprets the synthesis
  example through `miri run`, and its coverage check requires a row for every
  example. The inventory-reservation bindings are regenerated through their
  derivative test after the simplification changed the evaluator sources. The
  candidate fuzz target admits its bundle through `decode_commit_bundle`. The
  QEMU demo lock records the codec's `sha2` dependency at the root-lock
  versions, the kernel handles the non-exhaustive `MiniDecision`, and the
  soft-float guest selects sha2's portable backend.
- Narrow the packaged release check, deliberately. Published crates still
  build from their archives alone: every library, binary, example and build
  script at `sources/<crate>-<version>/`. Packaged tests are no longer
  standalone. They compile from the published archives laid out as in the
  repository, plus the repository files the manifest pins, copied from the
  commit: nine from `verification/` and, in 2.1, the benchmark artifacts under
  `docs/benchmarks/` that the CLI's checker, optimizer and loop tests include.
  Every file a test target reads outside its own package must equal
  `release/packaged-test-inputs.json` (target, path, SHA-256). Any such read by
  a non-test target fails. The whole-repository source archive runs every test.
  Two frozen, pinned references force this: the `#[cfg(test)]` `#[path]`
  include in `crates/zeno-fcis-synthesis/src/finite/execution_v2/mod.rs` and
  the `verification/verus/` includes in
  `crates/zeno-fcis-synthesis/tests/v2_evaluator_identity.rs`. Open item:
  restore standalone packaged tests when the verified sources next change, at
  the identity regeneration planned for 2.1.0.
- Skip one test under Miri, deliberately:
  `legal_leaf_above_default_payload_remains_constructible_and_encodable`
  scans 64 MiB values byte by byte and does not finish under Miri. It is skipped
  only in the values group through `.github/miri-exclusions.json`, which must
  equal the workflow's skips, name exactly one test, and name the native job
  that still runs it (`ci`, `rust`). The calls it makes still run under Miri at
  small sizes.
- Fix the SQLite v2 shell's exact-schema check, which skipped objects whose
  names matched `LIKE 'sqlite_%'`. In `LIKE`, `_` matches any character and
  ASCII case is ignored, so a user trigger named, for example, `sqlitex`
  passed the check. Such a trigger could mark every new delivery
  acknowledged while audits still passed. `open`, `audit`, live operations
  and initialization now compare every `sqlite_master` object with no name
  filter. A regression test plants triggers named `sqlitex`, `SQLiteX` and
  `sqlite1`. An independent review of V2.1 found the defect.
- Improve `zeno-fcis optimize` (F4.1, from the 2026-10-05 e-graph literature
  review). Every e-class now keeps an exact table over the inputs it depends
  on, its minimal support, whenever that support's domains have at most 2^16
  tuples, so classes of large domains merge by exact equality (poison
  included) instead of only within domains of at most 64 tuples; classes
  beyond the cap or the per-search table budget keep the conservative interval
  guard. Completion buckets candidates by sample values (every tuple up to
  1,024 tuples) and confirms each by its table before adding it. A new
  `cut-rewrite` phase rewrites Boolean classes over cuts of up to three leaves
  with a built-in table of minimum circuits, with and without `Eq`, that a
  test regenerates and checks entry by entry. Strategies gain an optional
  `profile` (`functional-bool-v1` or `checked-i64-v1`): no instruction outside
  it is proposed or extracted, and every candidate passes its gate; `optimize`
  gains `--profile`. Strategies also gain `limits.max_work`, a deterministic
  work budget in millions of steps that stands in for time. Without
  `--strategy`, `optimize` now runs a fixed portfolio
  (`zeno-fcis/optimize-portfolio/1`) of three strategies, each ending with
  merging and cut rewriting under its own size and work budgets, with one
  incumbent across them; the report tags phases and candidates with their run
  and adds per-run statistics. `--with-candidate FILE` (up to 8) supplies
  programs to fuse into the search: each is checked against the original
  first, and only an accepted one is added, unpinned, with its roots merged
  with the original's when the original can never fail or when their exact
  tables agree. The loop's strategy grammar admits `profile` and `max_work`,
  its engine phase table lists `cut-rewrite`, and the engine seam now passes
  the request's profile and the session's checked replacement, so the
  engine's candidates stay within the profile (a strategy naming another
  profile is `strategy-unavailable`) and must beat the replacement.
  Measured, every result accepted by the checker with a replaying receipt:
  withdrawal-queue controller 46 to 35 instructions, current decision graph
  100 to 88, kernel unchanged at 7; Boolean seeds 59 to 58 (B07 6 to 5),
  integer seeds 47 to 45 (I04 7 to 5); the published 100-case corpus, added
  under `docs/benchmarks/published-corpus/` as calibration data with its
  generator and provenance, 514 to 508. `docs/benchmarks/measure_optimizer.py`
  reproduces the figures. Two e-graph changes cut search time without
  changing any result: rebuild repairs each dirty class once per pass, and
  unions keep class lists sorted without re-sorting. These are bounded,
  checked results, not minimality claims; `Cargo.lock` and the verified core
  are unchanged.
- Add `zeno-fcis optimize --program P [--strategy FILE] [--candidate-out OUT]
  [--receipt OUT] [--max-input-tuples N]`: an in-house e-graph optimizer
  (union-find, hash-consing, congruence rebuild; no new dependency) that
  proposes smaller equivalent finite scalar programs, and the transform checker
  that judges every one of them in-process over the full declared domain. The
  optimizer is an untrusted proposer: a candidate is reported as accepted only
  with the checker's receipt, which `transform replay` reproduces. Classes
  carry exact signatures with trap poison for domains of at most 64 tuples
  and conservative interval bounds otherwise; classes merge only when their
  signatures and trap behavior agree, every `Add` or `Sub` that may overflow
  is kept, and arithmetic is rewritten only by constant folding that never
  folds a possible trap. Strategies are small versioned JSON documents in a
  closed grammar (phases `boolean`, `semantic-merge`, `select`, `fold`,
  `share`; bounded rounds, node, class and rewrite limits; extractor `tree` or
  `dag-greedy`); unknown keys are refused and no user-supplied code runs. The
  fixed default strategy, version 1, reaches the recorded 7-node
  withdrawal-queue Boolean kernel from 16 nodes, a 46-node retained
  controller from 69 (the recorded hand candidate has 60) and a 100-node
  current decision graph from 106 over all 1,296,000 tuples, and on the
  sixteen Boolean benchmark seeds matches or beats every recorded candidate
  while leaving minimal originals unchanged. The report states the termination
  bounds: phases and rounds run, e-node and class counts, and any limit hit.
  Exit codes follow `transform`: improved 0, no checked improvement or a
  domain above the cap 2, refusals 1, I/O failure 3. Nothing is adopted into
  an application; the result is a bounded search, not a minimality claim.
  Pure code in the CLI crate; `Cargo.lock` and the verified core are unchanged.
- Add `zeno-fcis loop open|candidate|run|resume|encode`: the bounded adaptive
  optimization loop of `docs/neurosymbolic-loop`. A frozen canonical request
  binds the original, its domain and ABI, the profile (`functional-bool-v1`
  or `checked-i64-v1`), the cost objective, the limits, the disabled provider
  policy and the checker identity. Proposers (a deterministic local rewriter,
  a scripted fake provider, agent-supplied candidates through the MCP tools
  `transform_request`, `transform_candidate` and `transform_replay`, and a
  hosted adapter that is disabled with a zero allowance) only produce data;
  every candidate is judged by `transform check` on the whole domain, and only
  a complete equivalence that lowers (nodes, bytes) within the original's
  bounds replaces the incumbent. Attempts, model calls and checks are reserved
  in a hash-chained ledger before work and never refunded; resume re-admits
  the request, verifies the ledger against its head and replays the incumbent,
  returning no trusted incumbent on any failure. Strategy proposals in the
  optimizer's grammar are admitted as data and run by the checked e-graph
  optimizer. Its bytes re-enter the loop's admission and check, and its own
  verdict is only provenance. The loop core is pure code in the CLI crate;
  `Cargo.lock` and the verified core are unchanged. No convergence,
  optimality or neural benefit is claimed.
  `docs/benchmarks/run_neural_loop_protocol.py` runs the available arms of the
  preregistered protocol, including the two e-graph arms, and reports every
  result.
- Add `zeno-fcis contract review <app-dir> [--out PACKET.json] [--max-tuples N]`,
  an advisory review of an application's contract. It binds the generated
  contract to the library Authority as the application does and writes a
  canonical packet, schema `zeno-fcis/contract-review/1`, byte-identical on
  repeat: every input of a domain of at most `--max-tuples` tuples (default
  2^20), or a documented boundary set of a larger one, each with the
  library's class, reason, successor digest and outbox digest or refusal;
  the agreement of those decisions with `tests/decision-examples.txt`; and
  a fixed, versioned catalog of rule mutants (comparison flips, constants
  moved by one, dropped guard conjuncts, swapped adjacent cases, changed
  reasons, dropped deliveries), each regenerated and bound through the
  library, then distinguished by a witness written as a proposed decision
  example, refused by the generator or the library, equivalent over a fully
  enumerated domain, or not distinguished within the boundary set. The
  review grants no authority and changes no application file; a decision
  that contradicts an owner example exits 1. The review is pure code in the
  CLI crate; `Cargo.lock` and the verified core are unchanged.
- Add `zeno-fcis contract adopt DIR --candidate C --receipt R --usage
  preserved|new-version`: a checked candidate decision program becomes the
  application's next contract version. The rules file records the adoption
  (candidate and receipt SHA-256, claimed usage, and the SHA-256 of the
  superseded version's policy) in an `adoptions` list, the candidate and
  receipt are kept under `v2/adoptions/N/`, and the generator replays every
  receipt, in order, against the program it re-derives from the
  declarations, rules and earlier adoptions before it emits the last
  candidate as the descriptor's program. `preserved` is accepted only when
  the receipt reports equal Step usage on every input. Superseded versions
  are emitted beside the current one (`src/v2_contract_vN.rs`,
  `v2/policy_vN.zcve`) and kept exactly: generation refuses any later edit
  that would change one, an adoption that leaves the program unchanged, and
  a lineage that would repeat a version. Every generated contract states
  `VERSION` and offers `with_lineage`, the checked catalogs of all its
  versions; a contract with adoptions also states `ADOPTION_RECEIPTS` and
  passes them with the catalogs. The eight templates, which have no
  adoptions, are byte-identical. The withdrawal-queue 106-to-100 node
  candidate is adopted as version 2 in a committed fixture. A refusal writes
  nothing, and running an interrupted adoption again finishes it.
- Add `zeno-fcis contract refresh-receipts DIR`: after a change of the
  transform checker's semantics version, every adoption is checked again
  under its receipt's limits and each receipt rebound to the current
  checker. It refuses, writing nothing, when anything but the checker
  identity would change, and every new receipt must replay.
- Add checked contract upgrades to the SQLite v2 shell. Schema v10 adds a
  `v2_upgrades` table of chained records. `v2::Lineage::bind(catalogs,
  receipts)` binds an application's lineage, and `Lineage::open` returns
  typed handles: a v9 store, which only `migrate`s; a current v10 store; or a
  superseded one, which can be audited, read, or upgraded with
  `Superseded::upgrade`, which consumes it and returns the current handle.
  After a full audit the pure `v2::upgrade::decide` requires equal canonical
  state schema bytes and differing identities, and admits the state in one
  of two record kinds. A `program-successor` is admitted at any state only
  when the shell establishes all five premises of the plan's program
  succession, under which the two versions reach the same states (see the
  entry on upgrade admission above). A
  `genesis-admission` covers other contracts and needs the new contract's
  genesis evaluation to admit the current state, which law 990 confines to
  the declared genesis state. Each segment replays under the Authority that
  published it; `open` keeps working for stores that never upgraded. Pending
  deliveries keep their certificate-bound IDs and order across an upgrade.
  A v9 store is refused until the explicit migration, which adds the table
  after a complete audit; every other mismatch refuses and writes nothing,
  and no open creates a missing file. `Snapshot::upgrades` counts the
  records. Applications built from a contract gain `--audit` at any version,
  `--upgrade`, `--deliver`, `--migrate` and `--decide`. The `zeno-fcis`
  binary does not open stores.
- Add `zeno-fcis transform check --original P --candidate C [--receipt OUT]`
  and `zeno-fcis transform replay --receipt R --original P --candidate C`.
  Both programs must pass the library importer's full admission and have the
  same input and output ABI. Each tuple of the declared input domain then
  runs through the library's verified metered evaluator at a full Step
  budget that no admitted program can exhaust. `check` reports one of three
  outcomes:
  - an equivalence, only when every tuple gives identical outputs or
    identical failures and the declared Step limit never binds. It writes a
    canonical receipt binding both programs, the domain, the limits, the
    counts, the Step usage and the checker identity: the checker's semantics
    version, `zeno-fcis/transform-check/1`, which committed known answers
    pin, and the library's evaluator digest. A new crate version or a
    refactor of the checker changes no receipt. Step usage is always
    reported, never compared;
  - the first differing tuple as a counterexample, with no receipt;
  - inconclusive, when the domain exceeds the cap (default 10^8 tuples) or
    the Step limit binds.

  `replay` re-runs the check and accepts only a byte-identical receipt. A
  receipt proves equivalence only over the declared domain, under eager
  semantics. It grants no application or publication authority. The checker
  is pure code in the CLI crate; `Cargo.lock` and the verified core are
  unchanged.
- Add `zeno-fcis generate contract <dir> [--check]`. An application's
  `project.zeno` and reviewed `v2/policy.json` generate `v2/schema.zcve`,
  `src/v2_contract.rs` and `v2/policy.zcve`. Generation is pure functions in
  the CLI crate, which already depends on everything it needs, so
  `Cargo.lock`, part of the approved evaluator source closure, is unchanged;
  the command itself only reads and writes files. The policy bytes come from
  `v2_authority::policy_bytes`, and the library's catalog binding, including
  its exact canonical-schema check, must accept them before anything is
  written. All eight templates regenerate byte for byte. This replaces
  `tools/check_template_contracts_v2.py`, its test, and the ignored
  `emit_library_policy_artifacts` test; a non-ignored test recomputes every
  template's policy bytes from its compiled source. Every generated contract
  states its genesis state as `GENESIS`. An invalid contract writes nothing;
  for the rules the generator checks itself, the error names the file and
  entry at fault.
- Add `zeno-fcis new DIR --contract CONTRACT`: an application built from
  `project.zeno`, `v2/policy.json` and optional decision examples alone, with
  source shared by every such application and no decision or law code; its
  tests check each example against the library Authority and run the
  examples as one SQLite session from genesis. `examples/dual-approval` is
  the first such contract.
- Share the V2 genesis and transition outcome/publication types while retaining
  the invoked kind through evaluation, publication and replay contracts.
  The SQLite adapter refuses genuine publications used for the wrong invocation
  with `Error::InvocationKind`, before a refresh or checkpoint write; foreign
  identity precedence and correct-kind bundle ordering are retained. Separate
  `Genesis*` wrappers are retired. Final combined qualification remains pending.
- Bound V2 core completion to one mandatory library-owned Authority → Publication
  → SQLite v2 route for the existing eight templates, preserving their legal
  inputs, successful behavior, laws and refusal-accounting protections. Application
  decision/law callbacks and caller-built publication capabilities cannot
  substitute for that route. Final combined qualification remains pending.
- Approve the staged core simplification: a generated evaluator digest checked
  against the approved sources by required gates;
  removal of History and pair export/import with identity/replay protection
  retained on the surviving shell; one scalar/record decoder and driver;
  generic account declarations; one value model; one tagged artifact shape.
  These are staged changes to implement and qualify, not a completed release.
- Record intentional API/identity changes, malformed-account refusal classes,
  regenerated Step limits and the account-lockout assurance delta separately.
  Generic graph execution and boundary replay do not establish
  the former all-timestamps rule-correspondence theorem. Preserve certificate-bound
  delivery identity, including repeated state and equal-payload transactions.
- Move the 12-type compound executor and full-width U128 zUSD to V2.1 as a new
  supported profile. Keep wide successful cases and their full domains; baseline
  local recovery is refs/simplify/baseline at tree
  94bac799b44859b158e50377313b7e0ef6c1bc51 (not yet published remotely).
  Historical parser/schema/private-oracle
  evidence is not end-to-end authoritative execution qualification.
- Use explicit ADR0004 triage: close only entries required by the route and
  withdraw the remainder with reasons. Implemented protections remain intact.
  Move QEMU, portable sources/mirrors, archive/privacy scans, private-oracle
  qualification and package/version polish to release engineering. Applicable
  release gates still apply before shipping. Verus/Z3 results retain their
  stated trusted base and do not become Lean KernelChecked evidence.
- Implement the staged simplification S1-S5 on one source tree: a generated
  evaluator digest; History and the pair export/import API removed, with
  prepared-counter on `V2SqliteShell`; one scalar/record decoder and driver;
  account rules declared on the generic path; one value model; one tagged
  outcome shape that retains the invoked kind; admission performed once and
  carried by the `BoundCore` type invariant; shared helpers in `util.rs`.
  - The whole-core Verus harness (`verification/verus/authority_v2.rs`, pinned
    flags) reports 835 verified, 0 errors. Its 468 Exec functions each have
    zero `requires` and at least one `ensures`; there is no `admit`, `assume`
    or `external_body`. The baseline reported 1338 verified and 625 Exec
    functions; the figures are not additive across units, and the decrease
    comes from removed code, not from dropped checks. Native tests: 134
    passed. Strict Clippy, no-std and no-default-features builds pass.
  - These are implementation checkpoints. Full mutation-control runs of the
    16 proof gates on the final source, the pre-commit ATDD run, independent
    review and exact-head CI remain open; see the plan's
    "Intake and current qualification".
- Record the mutation controls retired or replaced by the simplification in
  `verification/verus/simplification-retired-controls.json`: seven retired
  (each names the removed fault class, for example the four `law_delivery_*`
  copies that no longer exist without `law_bridge`) and three replaced or
  added (`reject_replay_comparison`, `wrong_unsigned_value`, and the Root
  selector/effect-lane controls moved from the composition gate to the laws
  gate). Anchor matching in `tools/check_finite_execution.py` and
  `tools/check_catalog_v2.py` compares tokens, so whitespace may differ, and
  still requires exactly one match; anchors were re-pointed to the simplified
  sources. Retired controls remain restorable from `refs/simplify/baseline`.
- Format the workspace with `cargo fmt --all`. Formatting pushed `seal`
  (`execution_v2/authority/outcome.rs`) and `prepare`
  (`execution_v2/authority/publication.rs`) over the solver's resource limit;
  they gained two and four proof assertions respectively. No `rlimit` was
  raised: the core's only two resource-limit attributes
  (`decision.rs` 30, `authority/spec.rs` 20) are unchanged from the baseline.
- The purity checker's compiler-tool context (which decides whether a
  `#[rustfmt::...]` attribute resolves to the compiler tool) no longer refuses
  because an enclosing manifest has a `[[bin]]` table: a binary target adds no
  dependency or rename. Every other unread manifest form, including
  `[dependencies]`, `[lib]`, `[patch]` and `[replace]`, still refuses.
  Test: `compiler_tool_context_accepts_a_binary_target_but_not_a_rustfmt_dependency`.
- Run the umbrella crate's API custody fixtures. The files in
  `crates/zeno-fcis/tests/ui` were never compiled by any test, tool or
  workflow. `tools/check_api_refusals.py` now builds `zeno-fcis` with all
  features and compiles each fixture: the two positive routes must compile, and
  each of the 30 refusals must fail with its expected first error code (retired
  names, private fields, missing `Clone`, non-exhaustive `Resource`, rejected
  callback, raw value, raw pre-state and supplied identity). Two fixtures were
  added for paths through `zeno_fcis::legacy`: the re-exported receipt crate
  still hides the candidate sealer (E0603), and the legacy core has no budget.
  The check runs in the `production-authority` acceptance scenario.
- Delete `crates/zeno-fcis-shell-sqlite/src/tests/{destination,transaction}_tests.rs`.
  The crate no longer included them, and they are byte-identical to the copies
  the private kernel-law oracle compiles and runs, so no assertion is lost.
- Retire the native zUSD mount from packaging. The `mount-zenodex-zusd`
  binary target is removed from `release/package-set.toml` and its source
  from `zeno-fcis-adapter-zenodex`; `.github/workflows/mounted-zenodex.yml`
  is deleted. The adapter crate states that native zUSD authoring is retired
  and that a checked full-width zUSD profile is planned for V2.1. The
  original pinned assertions remain executable in the private kernel-law
  oracle.
- `test-data/v1-compatibility/baseline.json` is now a V2 migration pin. It pins
  the V1 consumer after its documented V2 migration and the V2 foundational
  protocol sources with their embedded wire tests. The migration changes:
  - the prelude import, which moves to `legacy`;
  - `Value::Bool` becomes `Value::boolean`;
  - caller-side budgets become `zero_limits()` (the library owns the meter);
  - `TransitionDecision` becomes `PublicationOutcome`.

  The table in `docs/V2_PROGRAM_API_MIGRATION.md` lists them. V2 intentionally
  changes the V1.0 foundational sources (ADR 0004); the V1.0 pins remain in git
  history at `02694c2`. The acceptance scenario compiles and runs the migrated
  consumer against the locked graph. This is not universal downstream source
  compatibility.
- Re-synchronize the kernel-laws oracle's template copies (`normal.rs`,
  `v2_contract.rs` for six templates) with the simplified templates by
  three-way merge, keeping the oracle's intentional 9xx law ids (for example
  909 where the template binds 509). The oracle policy files are regenerated
  by the ignored test `policy_artifacts::emit_oracle_policy_artifacts` in
  `verification/kernel-laws/src/oracle/templates/mod.rs`.
- Regenerate the six finite template synthesis manifests against the
  simplified synthesizer. Relative to the tree before regeneration only the
  `certificate` field changed; programs, vectors and emitted source are
  byte-identical, and the Rust, Python and JavaScript replays passed.
- ATDD: test filters that named retired native tests now run the retained
  assertions in the kernel-laws oracle through a `historical_oracle` helper in
  `tools/atdd.py`; two broken documentation links are fixed; the
  `execution_v2/continuation/tests.rs` imports no longer depend on one
  harness's module path.

- Invocation admission now validates the pre-state against the authority's
  own schema and validation limits before issuing a witness that permits
  program execution. Command/context and later artifact checks remain.
- The browser API is now version 2: bounded scalar requests and replies,
  private Wasm memory, and a 64-request session limit with reset. The raw
  pointer and allocate/free interface is removed; modules and loader must be
  deployed together. Compiled-module tests check exports, boundary handling,
  capacity, and reset. Existing application decisions and canonical formats
  are unchanged. The site states each law's actual decision scope.

- Every example application template declares each law's scope in
  `project.zeno`, exactly as its `profile.rs` binds the law: `on commit,
  genesis` for the state invariant; `on accept`, `on failure`, or `on
  commit` for the action laws; and `on reject` for the counters' rejection
  law. The framework laws without a formula stay in `profile.rs`:
  `reject_publishes_nothing` (509) in the six templates other than the
  counters, and `no_committed_failures` (508) in inventory-reservation and
  withdrawal-queue. In agent-treasury-guard, law 508 has a formula and
  declares `on failure`.
  - Each `authority()` runs `LawManifest::check_declared_scopes` on its
    manifest and returns the mismatches before it builds the authority.
  - Each template has a test that the shipped manifest passes the check,
    and that a manifest with one law's scope, or its genesis applicability,
    changed is reported as `Scope` or `Genesis`. The `check_step_assumptions`
    tests stay.
  - Every template's semantic program hash changed, and so did its source
    identity. The decisions, the decision examples, the demonstrations'
    summaries, and the site's replay did not.

- `LawManifest::check_declared_scopes(&ProjectSpec)` checks that a law
  manifest enforces each law exactly on the decisions `project.zeno` declares
  for it, and at genesis exactly when the declaration says `, genesis`.
  - It reports every `ScopeMismatch`: a declared law the manifest does not
    define, a different scope, or a different genesis applicability. A law
    without a declared scope is not compared.
  - An application that runs it before building its authority can rely on
    elaboration's check of an inductive claim's groups for a claim whose
    laws all declare their scopes (assumption (c) in
    docs/INDUCTIVE_CLAIMS.md).
  - zeno-fcis-laws now depends on zeno-fcis-spec, which depends only on
    zeno-fcis-codec. The root, site, and external-consumer lockfiles each gain
    that one edge, and the laws crate still builds for wasm32 without `std`.

- Declare the decisions a law is enforced on in `project.zeno`:
  `law ID name on any|accept|reject|failure|commit[, genesis] = FORMULA;`.
  - `LawDecl::applicability` returns the declared `LawScope` and genesis
    flag, which mirror the law manifest's `DecisionScope` and
    `GenesisApplicability`.
  - A declared scope is part of the canonical project. A law without one keeps
    its bytes; the pinned semantic hash of a project without scopes is
    unchanged, and so is every shipped project's `check` output.
  - Elaboration refuses an inductive claim that assumes a law on decisions its
    declared scope does not cover. `LawManifest::check_step_assumptions` stays
    required: a declared scope states what the manifest should enforce, not
    that it does.

- The demo site stays at its top after the demonstration decided on load: a
  timeline entry scrolls into view only when the viewer caused it, by a
  button or a demonstration they started. `site/tests/deploy_check.py`
  loads the served page in a viewport-sized frame and requires a scroll
  position of 0 and the banner inside the viewport once the load-time
  demonstration is decided. It also drops a stray second definition of
  `check_harness`, left behind on 2026-09-25, which had shadowed the real
  one and made the harness assertions no-ops while the page checks stayed
  live; the assertions run again, and a control shows them catching a
  changed demonstration.
- `site/build.py` runs `cargo +1.97.1 fetch --locked` in the workspace
  before its offline cargo commands, as `tools/check_generated_application.py`
  does, so that the Pages workflow builds on a fresh runner with no registry
  cache; the site's lock names the same external packages as the reviewed
  workspace lock, which the lock check requires.
- The demo site opens in a working state. The section that is open when the
  page loads runs its README's demonstration at once, in the browser, and
  labels the result in its status ("decided in this browser when the page
  loaded"), so the first view shows the judge at work; the other sections
  reset to their genesis when opened. `site/tests/deploy_check.py` requires
  that of the served page: the label, the request count, and one timeline
  entry per decision carrying the gate's decision.
- The demo site runs every example application: order-fulfillment,
  inventory-reservation, compliance-gateway, withdrawal-queue, and
  agent-treasury-guard join account-lockout, each as a demo crate that names
  its application's types, genesis, and request mapping in the README's
  words; a page description with its state fields, context and command
  forms, the README's demonstration with a note per step, and the
  demonstration's end in the gate's fields; a section of the page with the
  README's own scoped wording of what is checked and at what strength; and
  an examples reader that reaches each example's state from genesis. The
  treasury guard's section shows the scripted agent as a proposer, with a
  button per proposal, including the ones the guard refuses; the withdrawal
  queue's section says how to raise the alarm and tick. Each section loads
  its module only when it is opened. `site/tests/replay.mjs` replays every
  example it can reach from genesis (all but withdrawal-queue's example 20
  and agent-treasury-guard's example 23, whose states no sequence of
  requests reaches; it names them) and every demonstration against the
  gate's summaries, and `site/tests/deploy_check.py` runs all six
  demonstrations through the served artifact.
- The demo site has one shape for every example. `site/common/` holds what
  every module shares: the step over the library's `AuthorizedShellState`,
  following each template's `invoke`; the JSON report, with every value
  named as `project.zeno` names it and every law as the manifest does; the
  reader of the page's request, which refuses a field no command reads; and
  the C ABI, defined once, exported by every module that links the crate, and
  the only code that handles raw pointers. A demo crate now names its
  application's types, its exact genesis, and its request mapping, and
  exports `demo_reset`. On the page, `panel.js` builds every example's
  forms, timeline, laws, state, and outbox from `templates/<template>.js`,
  the example's description in the README's words, loaded when its section
  opens; `tests/replay.mjs` replays any template's examples, each from a
  state that `tests/templates/<template>.mjs` reaches from genesis, and its
  demonstration against the gate's summary; the harness and
  `site/tests/deploy_check.py` take a list of templates and require the page
  to fetch only the open section's module. `site/build.py` takes `--only`
  and `--stage` so that a long build can be split.
- The demo site is ready for GitHub Pages. `.github/workflows/pages.yml`
  builds `site/public` with `site/build.py` on a push to `main` or a manual
  run, and deploys it with `actions/deploy-pages` when the ref is `main`;
  every action is pinned by commit. The page's scripts are `.js` modules,
  which `site/package.json` marks as ES modules for Node; the page fetches
  its module relative to its own script; and each template's README is
  linked on GitHub rather than copied into the artifact.
  `site/tests/deploy_check.py` serves the exact artifact from a subpath
  (`/ZenoFCIS/`) on a server that serves nothing at the root and checks it in
  headless Chrome: a harness runs the README's demonstration through the
  served module and its results must match the gate's summary, and the page
  itself must render its genesis. `tools/check_assurance.py` allows
  `pages: write` and `id-token: write` in `pages.yml` only, the two scopes
  the deploy action needs; its self-test shows any other write scope, and
  those scopes in another workflow, still refused. Nothing is published:
  merging to `main` is the owner's decision.
- Add twelve decision examples to three application templates, for rules and
  rule precedences that a blind review of the first 72 examples against the
  READMEs found untested. `compliance-gateway` (now 28): a reinstatement by a
  non-reviewer of an account with no strikes, decided by rule 1, and three
  pairs of hold rules that apply together, each decided by priority.
  `withdrawal-queue` (now 26): a deposit that fills the vault to its
  capacity, and a request that breaks both the caller rule and the lane
  rule, decided by rule 1. `agent-treasury-guard` (now 30): `below_reserve`
  and `slippage_too_wide` on a sell, a price from the future, an unapproved
  model with a stale price, a slippage and a reserve breach together, and a
  failure whose refund is the held amount, not the command's amount field.
  Each template's conformance test runs them through the application. The
  review also noted that no withdrawal-queue example ticks in must-serve
  with nothing due; that vault is unreachable, and the conformance test's
  assertion that must-serve is reached only with a pending lane and no pause
  is now a named test, which also requires every tick from such a vault to
  pay, and the README says so. The agent-treasury-guard README's
  `SwapFailed` row now says the refund is the held `pending_amount`, as law
  508 states and the new example shows; it said "refunds `amount`", which
  reads as the command's field. An AI agent wrote the examples from the
  README rules; review of these twelve by the project's owner is pending,
  and the acceptance of the earlier examples stands.
- `IdempotentDestination`, `MemoryDestination`, and `DeliveryCollision` move,
  unchanged, from `zeno-fcis-shell-sqlite` to `zeno-fcis-shell`, the pure
  reference model, where the in-memory destination needs only `alloc`. The
  `std::error::Error` impl of `DeliveryCollision` is behind `zeno-fcis-shell`'s
  `std` feature, which `zeno-fcis-shell-sqlite` enables. `zeno-fcis-shell-sqlite`
  re-exports the three at their previous paths, so existing code and type
  identities are unchanged, which a test checks by passing a value built
  through the `zeno_fcis_shell_sqlite` path to a function that takes
  `zeno_fcis_shell::MemoryDestination`.
- Every application template puts its SQLite shell behind a `sqlite` feature,
  on by default: `zeno-fcis-shell-sqlite` is optional, and `Shell`, `create`,
  `invoke`, `journey` (with `prepare` in `prepared-counter`), and the
  demonstration binary, which declares `required-features`, need it. The core
  builds without it: the generated bindings, the program, the law checker, the
  profile, the delivery adapter, whose destination wraps the pure
  `MemoryDestination`, and `authority()`, so the `Authority` type is the same
  with and without the shell. `tools/check_generated_application.py` now also
  runs `cargo clippy --lib --no-default-features --target wasm32-unknown-unknown
  -- -D warnings` on every generated application, and the `adopter-acceptance`
  and `release-candidate` workflows install that target. Each template's
  `Cargo.toml`, `src/lib.rs`, and `src/delivery.rs` changed, so its source
  hash, and the program, checker, and policy hashes derived from it, changed;
  the semantic program hash of each `project.zeno` did not.
- A demo site under `site/`, a workspace of its own like `verification/`: a
  `cdylib` for `wasm32-unknown-unknown` that runs the account-lockout
  application's authority, program, and law checker over the library's
  in-memory reference shell, with the application written by `zeno-fcis new`
  at build time; a static page; a headless replay, `site/tests/replay.mjs`, of
  the template's decision examples and README demonstration; and
  `site/build.py`, which builds and tests everything and requires two builds
  of the module to be identical. Nothing is published.
- The `account-lockout` template declares the range of each of its integer
  types in `project.zeno` (`Attempts in 0..=2`, `UnixTime in
  0..=4102444800`, and `LockDeadline in 0..=4102445700`), and its `build.rs`
  no longer binds them. It adds claim 600, `lock_state_stays_consistent`:
  law 500's formula over the account before a decision, assuming laws 501
  and 502 on accepts and law 503 on committed failures. With the declared
  ranges, CVC5 1.3.3 answers `unsat` for the induction step, which is
  attested, not independently checked, and Z3 4.16.0 agrees. Without them,
  CVC5 finds a login near the top of the i128 range, after which
  `last_seen + 900` overflows and the invariant has no value, and Z3 a
  failed login from `failed_attempts` 3, from which law 503 constrains only
  `last_seen`; the schema admits neither.
  - `tests/claims.rs` checks that the claim restates law 500 exactly, that
    the manifest enforces each assumed law where the claim assumes it and
    refuses a law assumed outside its scope, that the law checker's observer
    reads every field the invariant reads and gives it its stated value on a
    grid of 129 admitted accounts across every boundary it compares, that
    the invariant holds on the exact genesis, and that the declared ranges
    are the generated schema's bounds and agree with the program's
    constants. `tests/conformance.rs` evaluates the invariant before and
    after each of the 295 committed decisions in its grid and the 12
    committed examples. `tests/laws.rs` adds decisions that only a formula
    refuses. The law checker observes the account through one
    `state_observations`, which the tests share.
  - Twelve planted defects, in the observer, the laws, the ranges, the
    program, and the checker, each failed a named test; the two weakened
    laws and the widened `Attempts` range were also refuted by `prove`.
  - Declaring the ranges and the claim changes the template's canonical
    project bytes and semantic program hash; the program, the schema's
    bounds, and the demonstration summary are unchanged.

- Declare the inclusive range of an `int` type in `project.zeno`:
  `type ID int name in MIN..=MAX;`.
  - Schema lowering takes a declared range as the type's `I128` bounds, so
    `build.rs` needs no binding for it. A binding that states other bounds is
    refused as `SchemaLoweringError::IncompatiblePrimitive`.
  - Inductive steps assume the declared range of every observed path of that
    type, and replay refuses a model outside it. Soundness rests on the
    authority checking every command, context, and state against its schema,
    whose bounds for that type are the declared range.
  - `declared_domain` and `StepHypotheses::domains` now return
    `DeclaredDomain`, either variant values or a range. Both were added since
    1.1.0 and have not been released.
  - A project that declares no range keeps its canonical bytes and semantic
    hash.

- The commit authority checks the command and context of every invocation,
  and the initial state at genesis, against its own catalog's schema.
  - Before, it compared only the schema hash each envelope recorded. An
    envelope's constructor accepts any `CommitmentHasher`, so a caller's own
    hasher could record the catalog's schema hash for a value validated
    against a looser schema, with an out-of-range integer or an undeclared
    variant.
  - Committed states were not affected: the pre-state and post-state of
    every transition were already validated against the catalog's schema.
    The templates' programs and law checkers also convert their inputs to
    typed values, which refuse such values.
  - A refused input is `AuthorityError::Mismatch(AuthorityField::Schema)`,
    and honest inputs are unaffected.
  - ADR 0003 now lists the check, and assumption (g) of the inductive-claims
    argument rests on the authority instead of on each application's
    conversions.

- Add the `agent-treasury-guard` application template for
  `zeno-fcis new --template`. An AI agent proposes swaps for a treasury; the
  proposal is only a command, and the decision is the guard. Whatever the
  agent proposes, the treasury commits at most its daily budget, keeps its
  reserve, stays within the slippage bound against the oracle price, and has
  at most one swap outstanding. Only an approved model identity, carried in
  the context, may propose. Accepted swaps leave through the outbox as
  requests shaped after ZenoDEX's `SwapIntent`, numbered by their proposal
  tick, and a settlement or failure naming another intent is rejected as
  stale. A failure refunds the swap and commits as a failure; it does not
  restore the budget.
  - A hand-written adapter computes the guard's facts with checked
    arithmetic; a core that `zeno-fcis synth` selected from a sketch with
    three holes, and checked on all 6,144 fact tuples, decides which rule
    applies first. The gate replays the core in Rust, Python, and JavaScript
    against an independent restatement of the rules.
  - `project.zeno` carries two inductive claims for `zeno-fcis prove`.
    Claim 600 states law 500's invariant over the treasury before a
    decision: the reserve, a non-negative base, a day's committed value
    between 0 and the budget, and the swap bookkeeping. It assumes laws 501
    and 502 on every commit, 503 to 507 on accepts, and 508 on committed
    failures, the scopes the manifest enforces them on. Claim 601 states the
    budget's part from the clock, the day accounting, the proposal's debit
    and limits, and the refund. CVC5 1.3.3 answers `unsat` for both steps
    (attested; `prove` exits 2), and Z3 4.16.0 answers `unsat` too, recorded
    as `blocked: UnsupportedEvidence`. Stating claim 600 found six
    transitions the draft's formulas admitted and the README's rules
    forbid: a buy of 0, a buy into the reserve, a proposal over the budget,
    a sell of more base than held, a sell at a negative price, and a
    settlement from a negative held minimum. Laws 505 and 507 now state the
    positive amount, the floors, the budget, and the positive price, and
    law 500 the non-negative held minimum. `tests/claims.rs` checks that
    claim 600 restates law 500, that the law checker's own observer reads
    every field the invariants read and gives each its stated value on all
    105,840 admitted treasuries at a tick, that both hold on the exact
    genesis, and that the manifest enforces each assumed law where the
    claim assumes it.
  - Every law formula is substantive with resolved paths. The template ships
    30 decision examples, a conformance test that decides 25,094 inputs over
    a scaled finite domain and evaluates both invariants before and after
    each of its 4,596 committed decisions, including a search of every state
    one day can reach that shows no sequence of proposals commits more than
    the budget, direct law-checker tests with one decision per law that
    breaks only that law and the six counterexamples each refused by the
    law that now states its rule, a lifecycle test, a determinism probe over
    864 inputs, and clean purity results. The gate, the release packager,
    and the ATDD scenarios cover it with the other examples. The first 24
    decision examples were reviewed and accepted by the project's owner on
    2026-09-24; the six added since await review.

- Add the `withdrawal-queue` application template: a vault with two
  withdrawal lanes whose keeper tick runs a controller step that
  `zeno-fcis synth` selected from a sketch of a fair policy, checking every
  hole assignment against the tick rules on all 384 inputs. The table that
  step defines ships with the vault's contract in OrbitSynthesis's canonical
  format, with the contract's SHA-256 pin. OrbitSynthesis's checker accepted
  it for every sequence of requests and alarms, with no fairness assumption:
  an alarm can delay a withdrawal but never freeze it, and a due lane is
  paid within 8 ticks. Two rejected strategies ship with it, a fixed
  priority and one that defers to alarms in must-serve; the checker rejects
  each with a counterexample cycle.
  - The law checker's refinement check ties each concrete tick to the model:
    the contract's transition table must allow the output the decision made
    and give the plant state it left, and the certified strategy's row must
    select that output and next memory. The checker reads neither the
    synthesized step nor, for the contract check, any strategy row.
  - `tests/controller.rs` re-checks the shipped JSON with an independent
    Rust implementation of the decision procedure, requires the synthesized
    step to equal the certified table on every input, the Rust tables to
    equal the JSON, and both to equal a restatement of the README's rules,
    and computes the 8-tick response bound. `tests/conformance.rs` explores
    420 reachable vaults and 3,360 decisions against a reference model
    written in words, and checks the bound on the running application.
  - Two inductive claims for `zeno-fcis prove` state that the action laws
    alone keep the vault solvent and within capacity, law 500's invariant,
    and the pause within the controller's range, for every integer. Stating
    them showed where the formulas under-described the tick, so the laws now
    state the deposit's capacity, positive amounts, an amount leaving a lane
    exactly when the lane is emptied, and when must-serve is set. CVC5
    attests both steps; `tests/claims.rs` checks the observer, the exact
    genesis, and the law manifest.
  - `tools/check_generated_application.py` replays the synthesis in Rust,
    Python, and JavaScript against an independent oracle, and runs
    OrbitSynthesis's checker on the shipped files when `ORBIT_SYNTHESIS_ROOT`
    names a checkout, otherwise reporting that the check did not run.

- Add the `compliance-gateway` application template for
  `zeno-fcis new --template`: an expert system's transfer-screening rule
  base, `rules.txt`, becomes the synthesis contract, and `zeno-fcis synth`
  selects a decision program that is checked against it on all 720 inputs.
  Every decision names the rule that fired: a blocked transfer is a committed
  failure under that rule's reason, with a strike on record and an alert
  through the outbox; a held transfer queues a review ticket naming the rule;
  three strikes freeze the account until a reviewer reinstates it. The rule
  base is checked over all 720 inputs for consistency (no two rules of one
  priority match the same transfer), totality, and liveness (every rule
  decides some transfer); a rule base that fails a check fails `cargo build`
  and is refused again when the authority is built, so the application never
  decides under it. The law checker evaluates the rule base itself against
  every decision, independently of the synthesized step. An inductive
  claim states that the strikes stay within their bounds under every change
  laws 501 to 503 admit, with each enumerated field bounded to its declared
  variants. CVC5 attests the induction step. `tests/claims.rs` checks the
  base case, the law checker's observer, and the law manifest, and the
  conformance test evaluates the invariant before and after every committed
  decision. `tools/check_synthesis.py`
  replays the synthesis in Rust, Python, and JavaScript against a separate
  Python evaluation of the same rule base. The template also carries the
  rule base as a Tau Language specification with a local check script; it
  needs IDNI's binary and is not part of any gate. The first 24 decision
  examples were reviewed and accepted by the project's owner on 2026-09-24;
  the four added since await review.

- Give `zeno-fcis prove` a system model: inductive claims.
  `claim ID name BACKEND inductive assume [...] accept [...] failure [...] = INVARIANT;`
  states an invariant over `pre.` state paths and names the laws its induction
  step may assume, grouped by the committing decisions they are enforced on.
  - `export_inductive_smt` exports the step. Each assumed law, and the
    invariant before the step, is asserted as defined and true on its own, and
    the invariant after the step, rewritten from `pre.` to `post.` by
    `invariant_at`, as not defined-and-true. Laws assumed only on accepts or
    only on committed failures are guarded by a decision kind.
  - Results have the new scope `InductiveStepOverLaws`. Counterexamples are
    replayed: every assumed law and the invariant before the step must
    evaluate to true on the model.
  - Applications check the rest of the argument: `evaluate_invariant` on the
    exact genesis state, and `LawManifest::check_step_assumptions` against the
    law manifest. For the durable-counter template's
    `counters_never_negative`, CVC5 answers `unsat` (attested) from its action
    laws alone, and `tests/induction.rs` runs the application's checks
    through the law checker's own observer. Adding the claim changes that
    template's policy identity.
  - A pinned test requires CVC5 and Z3 to agree with exhaustive replay on
    eight inductive steps; it runs in the formal-tools workflow and in the
    gate evidence.
  - Each observed enumerated or bool value is asserted to be one of its
    declared values (`declared_domain`), which admission guarantees, and
    replay refuses a model outside them. Action laws written as
    `command == X -> ...` otherwise leave an undeclared command unconstrained.
    With this, CVC5 answers `unsat` (attested) for the compliance-gateway
    strikes invariant, from its own laws.
  - See `docs/INDUCTIVE_CLAIMS.md`, including its limits: the step cannot yet
    assume integer ranges, which live in `build.rs`, and there is no Lean
    export for it.
- Report a replayed counterexample at which a claim has no value as
  `undefined`, with the reason: an overflow, a division by zero, or an
  inexact exact division. `prove` used to report `ModelReplayFailed` whenever
  a claim's arithmetic could overflow somewhere in the i128 range, although
  the solver's model was right. Evaluation is strict, so the claim does not
  hold there. A guard such as `pre.x <= 3 -> pre.x + 1 <= 4` does not protect
  the arithmetic after it, and the reason is now visible.

- Add three application templates for `zeno-fcis new --template`:
  - `account-lockout`: failed logins committed as failures, time as a context
    input instead of a clock read, administrator unlocks, and security alerts;
  - `order-fulfillment`: a hand-written state machine that sends idempotent
    payment and shipping requests. A repeated or late callback is rejected
    because the order has moved on, and payment callbacks name the attempt
    they answer, so one about an older attempt is rejected as stale;
  - `inventory-reservation`: a decision core synthesized and verified on all
    432 inputs, commands with quantities, and a conservation law.

  Every law formula in their `project.zeno` files can constrain some
  transition and reads only declared fields:
  `check --require-substantive --require-resolved-paths` passes. Laws without
  a formula are registered in `profile.rs` instead of as placeholder formulas:
  the rejection law, which holds by construction, and inventory's law that no
  failure is committed, which its law checker enforces. Each template ships
  decision examples, a conformance test through the running application,
  direct law-checker tests, a lifecycle test, a determinism probe, and clean
  purity results. During development, every defect planted in a template's
  program or law checker failed a named test. The decision examples were
  reviewed and accepted by the project's owner on 2026-09-24.
  - `tools/check_generated_application.py` and the release packager build and
    test each as an isolated package and compare its demonstration summary.
    `tools/check_synthesis.py` replays the inventory synthesis in Rust, Python,
    and JavaScript against an independent oracle.
  - `new` now chooses template files through exhaustive matches, and a test
    requires every application template to emit exactly the files in its
    directory.

- Refuse escaped library keys in the purity manifest check. Cargo decodes
  Unicode escapes in quoted keys, so an escaped `path` could select source
  outside `src/` while the check reported the unused default root as confined.
  Unsupported escaped keys now prevent confinement with a stated reason.

- Add determinism checks for hand-written decision code.
  - `CatalogCommitAuthority::execute_probed` executes one invocation 2 to 64
    times, each on its own copy of the admitted values. It returns the first
    decision only if every execution produced identical canonical bytes, and
    otherwise withholds it with `ProbeError::Diverged` or
    `ProbeError::FailedAfterDecision`.
  - `zeno-fcis purity <PATH>...` parses Rust source and reports clocks,
    environment reads, randomness, hash-map iteration, shared state, raw
    addresses, unsafe code, and other ambient effects, with `use` aliases and
    macro arguments resolved. A crate directory is also checked for
    confinement. What the check cannot read never leaves a result clean. An
    unreadable directory, a symbolic link to source, or a directory with no
    Rust source makes the result `unreadable`. Only a library-only package
    can be confined: a binary target, a manifest form the check does not
    recognize, `include!`, or a `#[path]` attribute prevents it.
  - The durable-counter template gains `tests/determinism.rs`, which probes
    all 64 admitted inputs and compares their digests across three
    changed-environment child processes. Planted controls show that the probe
    and the child processes catch changes the conformance tests miss.
  - docs/DETERMINISM.md explains what each check establishes.
  - The project's owner accepted the durable-counter decision examples on
    2026-09-23.

- Correct two claim boundaries found by an independent review of 1102d81.
  First, synthesis guarantees the selected program as the interpreter runs it.
  `synth run` reports emitted source as `runtime_conformance: not-run`, and
  emitted Rust, Python, or JavaScript carries the guarantee only once
  `synth verify` passes for that exact source and target. Second, the README
  and architecture guide now separate untrusted proposers from trusted
  components: the pinned Lean kernel and runtime, a solver's unrechecked
  `unsat` answer, the search's enumeration for a "no solution" result, and a
  project's law engine.

- Explain in the README how exhaustive verification differs from correctness
  by construction, and why both are proofs. The synthesis section now shows
  what each approach trusts, what a bug in the builder does, and what each can
  cover. It also separates the guarantee for a selected program, which does
  not trust the search, from a "no solution" result, which does. It names the
  full synthesis limits (65,536 inputs, 4,096 possible outputs, 16 fields on
  each side of a contract, 1,000,000 candidates) and the four things the
  guarantee depends on, including a checker that is tested but not formally
  proven.

- Explain in the README why ZenoFCIS separates deciding from acting, how
  untrusted proposers and small deterministic judges divide the work, what
  that gives developers, LLMs, and agents, and what synthesized code is
  guaranteed to do: it is correct by exhaustive verification against its
  contract, not correct by construction, within four stated limits. The
  architecture guide gains a matching section. Design record 0003 now says the
  authority runs the program itself; only re-authorization re-executes and
  compares.

- Check the time budget before the first process start as well, so an
  exhausted budget starts nothing, in the formal-tools adapter and the
  synthesis runner. The gate evidence recorder also compares the source with
  the start after every gate and stops at the first change; a change made and
  reverted between two checks is still not detected.

- Fix two regressions found by an independent review of 562926d. The busy
  executable retry in the formal-tools adapter and the synthesis runner no
  longer outlives the caller's time budget: no attempt starts after it ends,
  and the result is `Timeout` (a 1 ms budget took 259 ms before). The gate
  evidence recorder now checks the commit, tree, and tracked files after all
  collection, immediately before writing, and new tests show that a late
  change writes nothing. The durable-counter examples' header now states that
  their author had seen the model artifacts and that a second reviewer
  accepted them against the README alone.

- Add `tools/record_gate_evidence.py`, which runs the local gates for the
  committed revision and writes a revision-stamped JSON record: tool
  versions, each command's exit code and test counts, the pinned CVC5, Z3 and
  Lean checks when their executables are supplied, and an inventory of every
  ignored test and what runs it. The formal-tools workflow also runs the new
  pinned solver-route differential.

- Connect the durable-counter template's finite model to the executed
  application. The generated application's new `tests/conformance.rs` runs all
  64 admitted inputs through admission, the authority, the Rust adapter, and
  the law checker, and requires the decision, reason, new state, and
  notification to equal the synthesized program's output read through a
  declared table. It also checks schema admission against the finite input
  domain, genesis, reachability of every admitted state, and twelve examples
  drafted from the README (`tests/decision-examples.txt`). Their author had
  seen the model artifacts, a second reviewer checked them against the README
  alone, and the project's owner accepted them on 2026-09-23. Swapping two same-type adapter bindings fails these tests.

- Retry a process start a bounded number of times, about 250 ms in total, when
  its executable is busy (`ETXTBSY`), in the formal-tools process adapter
  and the synthesis runner. A child forked by another thread while a private
  executable copy is written briefly inherits the write descriptor; this made
  two tests fail intermittently with "Text file busy". Every other error, and
  a file that stays busy, still fails closed.

- Correct overstated claims found by an independent review of 521b768. The
  zUSD record's outcome and reason-order findings are scoped, with the
  reviewer's guarded-deposit counterexample kept as a test, and design record
  0003 now states the scope, assumptions, and trusted base of every
  classification.

- Make the system-property checkers honor their stated contracts. The
  exhaustive checker decides totality on every admitted input before any
  property result. `system_verdict` refuses solver models with the wrong
  number of values or values outside the declared domains, and replays
  domain-only models, whose scripts now also request the proposed outputs
  (`SystemAnswer::Sat { input, output }`; `parse_system_answer` takes the
  output count). A bounded differential compares the exhaustive and solver
  routes on 80 cases, and eleven planted-defect controls each fail a named
  test. The three regressions come from an independent review of 521b768.

- Add kernel law harnesses in a separate `verification/` workspace. Four laws
  (budget charges, reason choice, canonical decoding, and patch overlap) each
  have one predicate, checked exhaustively over a small stated domain and at
  random under bolero 0.13.4. Planted bugs for each law are detected. bolero and
  its dependencies are pinned in `verification/Cargo.lock` and never enter the
  published crates' lockfile. A `kernel-laws` workflow runs the harnesses and
  `cargo deny` on their dependencies.

- Report law and claim paths that name no declared type or field. Add
  `resolve_path`, `law_paths`, `claim_paths` and `PathResolution` to
  `zeno-fcis-spec`. `zeno-fcis check` warns about each unresolved path, adds
  an `unresolved_paths` object to JSON output, and accepts
  `--require-resolved-paths`. Elaboration, canonical bytes and default exit
  codes are unchanged. The CLI tutorial's `check` transcript now shows the
  current JSON output and substance warnings.

- Close effect spellings that the static assurance check missed in semantic
  crates: standard I/O; grouped, glob and aliased `std` imports;
  `thread_local!`; atomics and cells written without generic arguments, such
  as `AtomicU64::new` and `RefCell::new`; and hash-ordered collections. The
  self-test now also requires that safe witnesses such as `WorkspaceCell` and
  `BTreeMap` pass. No semantic crate needed a change.

- Record what `.zeno` v1 and the `finite-i64/1` IR can state about the
  single-vault zUSD lane. A `.zeno` v1 attempt states the lane's 32 fields,
  46 reasons, invariants, conservation laws, and a guarded deposit law that
  matches four native outcomes. Characterization tests pin what v1 cannot
  state directly: the reason code a rejection carries, products beyond i128,
  literals beyond u64, a leading parenthesized scalar, and field paths the
  schema does not declare. The record scopes its reason-order findings to how
  a program records failures, after an independent review of 521b768.

- Add design records under `docs/adr/`: the meaning-first assurance review,
  the principles for the V2 assurance program, a five-level vocabulary for
  the epistemic status of evidence that classifies the V1.1 status and
  witness types, and a ledger of breaking changes deferred to V2.
  Documentation only.

- Reserve the `zeno-fcis` commitment-domain namespace. Add
  `is_reserved_domain_name`, `DomainPrefix::try_new_project` and
  `StateDomainBinding::try_new_project`, which reject names that would share a
  domain with library identities such as candidate IDs. The V1 constructors
  are unchanged. The zUSD mount computes its patch precondition through the
  shared `hash_precondition_value` helper, with byte-identical results.

- Check properties against an exact finite transition program.
  `check_system_property` enumerates every admitted input and runs a
  domain-only control that reports properties the output domains already
  imply. `export_system_smt` exports totality, property and domain-only
  obligations that include the transition relation. `system_verdict` accepts
  a solver model only after the interpreter reproduces it. Pinned CVC5 runs
  must agree with the exhaustive check on the durable-counter program.

- Classify every `.zeno` law and claim by substance: `constant-true`,
  `constant-false`, `ignores-transition` or `may-constrain-transition`.
  `zeno-fcis check` warns about formulas that cannot constrain any
  transition, adds a `substance` object to JSON output, and accepts
  `--require-substantive`. Classification is diagnostic: it changes no
  canonical bytes, program identity, existing field or default exit code.
- Add `ObligationScope` to exported formal obligations. `zeno-fcis prove`
  now states that current obligations contain no system model: accepted
  results hold for every bounded observation assignment, and counterexamples
  may be unreachable.

## 1.1.0 - 2026-09-13

- Add independently checked finite exit plans, bounded canonical plan import,
  structured completion CLI discovery and exact replay files for agents.
- Add owned ordered preparation with whole-operation resource reservations,
  chunk rollback and exact invocation/root/version checks before exposing output.
- Add the generated prepared-counter application with independent authorization,
  full publication-size admission, atomic state/outbox publication and recovery.
- Qualify both generated applications and an unchanged V1.0 consumer against actual
  crate archives; preserve the foundational protocol and wire-test source baseline.
- Reject unknown fields in Boolean synthesis domains, matching the existing closed
  JSON schema instead of silently ignoring malformed input.
- Retain Lean 4.30.0, Rust 1.97.1 and the existing external Cargo dependencies.

See [V1.1 release notes](docs/V1_1_RELEASE_NOTES.md) for the exact scope and limits.

## 1.0.0 - 2026-09-12

First stable Cargo API release. See [V1 release notes](docs/V1_RELEASE_NOTES.md)
for developer workflows, compatibility and assurance limits.

- Add concrete finite-contract synthesis with Rust, Python and JavaScript target
  conformance; keep proposal, checking, emission and application authority separate.
- Reduce redundant canonical decoding, composition hashing, finite evaluation,
  outbox encoding and parallel binding allocation with retained comparison evidence.
- Promote all 36 public package versions together without upgrading Lean or
  any external Rust dependency version. Patch the developer-tool Hono dependency
  to 4.13.5, the first version fixing three newly reported moderate advisories.
- Reject pending SQLite entries absent from the approved bundle, including
  row substitution between validation reads.
- Reject Python adapter primitive coercions and isolate synthesis runner source
  creation against pre-existing files and symlinks.
- Avoid a nested Cargo build lock deadlock in the QEMU capture runner and
  regenerate the real V1 framebuffer and serial transcript.

- Use `test-projects` and `test-data` folders and a private
  `zeno-fcis-generated-code-tests` crate. Rename `ReplayFixture` to
  `DecisionMismatchRecord`, with the previous public name retained as an alias.
- Replace the optional `imbl` ordered map with `OrderedMap` using the existing
  pinned `rpds` dependency, removing the unsound `bitmaps` dependency. Preserve
  thread-safe shared snapshots, canonical output, and the old type/feature names
  as compatibility aliases. Add branching-snapshot differential checks.
- Show dependency-check failures in CI output and retain both checker logs on
  failure, while keeping the existing advisory policy.
- Qualify the generated durable application using a CLI built from actual crate
  archives and internal dependencies resolved only from those archives; retain
  `PACKAGED-APPLICATION.json` in release evidence.
- Admit both packaged and generated-consumer dependency graphs against the
  reviewed external lock and exact local manifest/version allowlists, rejecting
  cached dependency drift and fallback to checkout sources.
- Preserve release staging paths containing spaces through encoded Cargo
  compiler arguments, remove inherited compiler overrides, and retain explicit
  argument evidence without allowing documentation warnings to be suppressed.
- Check renamed internal dependencies by actual package identity and recheck
  the clean source commit before retaining release manifests.
- Add parser-derived `describe [COMMAND...]` JSON for agents, including command
  effects, argument defaults/choices, and bounded failure recovery guidance.
- Add optional JSON generation and drift results, bounded artifact comparisons,
  and explicit artifact-read errors while preserving read-only checks.
- Implement the standard `core::error::Error` trait for eight foundational
  errors, enabling normal downstream `?` propagation without requiring `std`.
- Align workspace, generated application, external consumer, and fuzz metadata
  with the documented minimum Rust version, 1.97.1; retain existing dependencies.
- Return versioned JSON for CLI project-read failures when requested, including
  JSON graph input errors, and classify tool timeouts as execution failure (3).
- Bound CLI project-file reads before UTF-8 interpretation or parsing. Oversized
  files now return input-acquisition failure (3); in-memory parser diagnostics
  and accepted source programs are unchanged.
- Refresh quickstart commands and the complete generated-application journey,
  and report the actual error when the admitted-executable regression fails.
- Move owned footprint buffers into sealed transitions, avoiding deep clones
  while preserving normalization, reason precedence, resource bindings, and
  rejection behavior.
- Stream exact commitment preimages through the existing approved SHA-256
  providers, removing the framing buffer while preserving canonical hashes
  and a default allocating path for existing custom hashers.
- Repair overflowing persistent-map benchmark fixtures, compare all existing
  backends, and add allocating-versus-streaming commitment benchmarks with
  permanent fixture smoke checks.
- Add checked `.zeno` record/sum lowering with explicit scalar bounds and
  precise rejection of incompatible schema bindings.
- Add the development `durable-counter` CLI template and an isolated consumer
  gate covering runtime laws, nominal authorization, SQLite restart, exact
  replay, and idempotent notification delivery.
- Add generated `begin_bound_transition` for complete shell-owned invocation
  bindings and support empty generated reason, effect, and channel enums.
- Fix nested Lean temporal variable capture and add a relational/temporal
  translation corpus. Lean remains pinned at 4.30.0; the corpus can reuse an
  existing executable without installing or copying its runtime.

### Added

- a deterministic, prompt-minimized Exploitability-Potential Index scanner with
  decomposable hotspot scores, category-specific reviewer cards, candidate
  multi-stage review routes, hostile self-tests, exact baseline drift
  detection, and a read-only CI gate;
- an evidence-first LLM security playbook, dated primary-source standards
  snapshot, exploit-chain proof obligations, Chain Feasibility Index, and a
  machine-readable review-report schema;
- a closed ATDD scenario proving that repository text remains inert while
  hotspot ranking and the exact source inventory stay reproducible.

### Changed

- the LLM cybersecurity prompt now guides bounded models from threat modeling
  through hotspot triage, isolated scanners, current advisory/tactic lookup,
  finding proof, zero-day chain analysis, mitigation, and adversarial re-review;
- release assurance now retains hotspot model/inventory identities, review
  scope, chains, and residual-risk decisions without treating a low score or
  clean scan as a security certificate.

## 1.0.0-rc.3 - 2026-07-30

Authoring and composition usability candidate before stable V1.

### Added

- versioned bounded `.zeno` parsing, canonical typed project ASTs, accumulated
  diagnostics, builders, relational logic, and explicit finite/unbounded
  temporal modes in the pure `zeno-fcis-spec` crate;
- deterministic CVC5 1.3.3, Z3 4.16.0, and Lean 4.30.0 exporters and
  fail-closed process adapters in `zeno-fcis-formal-tools`;
- the `zeno-fcis` CLI for templates, checking, generation/drift, graphs,
  explanations, formal tools, and backend diagnosis;
- the executable Mini Determinator private-workspace, get/put, return,
  canonical join, conflict, rollback, budget, and replay reference;
- twenty-six RC3 BDD/ATDD scenarios, tutorials, official formal-tool checksum
  workflow, and a 36-crate public package inventory.

### Compatibility and limits

- existing protocol IDs and canonical formats are unchanged;
- `.zeno` is non-authoritative authoring input and only the lowered AST has
  canonical identity;
- finite temporal checks, generated views, and tool agreement do not grant
  production authority;
- the Mini Determinator is a semantic model. Its isolated QEMU kernel is a demonstration and carries no production OS or hardware claim.

## 1.0.0-rc.2 - 2026-07-29

Second public release candidate for the reusable ZenoFCIS core library family.

### Changed

- Catalog format 3 requires every effect and channel to bind explicit reviewed
  `OperationSemantics`, including canonical asset-scoped value-flow sets.
- Verified project laws now derive non-waivable economic families and
  committing-decision coverage from the exact catalog. Custom value relations
  require one exact registered claim with independently retained evidence.
- `CommitPlan` is explicitly non-executable committed evidence. Every effect
  definition is constructor-fixed to `CommitEffectSemantics::EvidenceOnly`,
  project catalogs reject value-moving effects, and external value movement
  must use a durable value-classified outbox channel.
- Production authority now names and binds the concrete outbox-delivery
  interpreter through `BoundDeliveryInterpreter`.
- Release-candidate packaging now uses the version-neutral
  `tools/rc_package.py` entry point and current-version-derived artifact names.
- RC2 now freezes its reusable product surface explicitly and binds nine
  human-readable BDD scenarios to a closed executable ATDD registry.
- Optional deterministic coding-agent guardrails pin Node `22.23.1` and
  Probity `1.10.0`; hostile command and transcript mutations are permanent
  acceptance evidence and AI-judged TDD is not enabled.

### Release-candidate limits

- no arbitrary downstream project receives production authorization merely by
  using the library;
- the authenticated sparse tree and included SQLite deployment remain bounded
  reference surfaces unless separately qualified;
- concrete formal-tool adapters and project proof artifacts remain
  project-owned;
- signing, hosted provenance, crates.io publication, and independent exact-head
  review remain owner or external actions.

## 1.0.0-rc.1 - 2026-07-29

First public release candidate for the reusable ZenoFCIS core library family.

### Included

- immutable values, canonical ZCVE/1 encoding, deterministic budgets, and the
  `Accept | Reject | CommittedFailure` decision algebra;
- preconditioned canonical patches, closed effect and outbox plans, receipts,
  candidate sealing, and nominal production commit authorization;
- project profiles, schemas, catalogs, generated typed transitions, relational
  laws, and fixed-size composable domain machines;
- proof-carrying composition with complete static footprint evidence and
  deterministic-parallel authorization;
- tool-neutral evidence, refinement, synthesis, and checked-backend protocols;
- strict invocation-bound decision reconstruction, derived refinement cases,
  canonical exhaustive-domain manifests, independently verified coverage, and
  content-addressed promotion reports;
- reference authenticated state, persistent collections, SQLite publication,
  mounted-runtime adapters, secret handling, and side/covert-channel policy;
- 33 publishable crates, a diagnostic ZenoDEX mount binary, complete public API
  rustdoc pages, examples, checksums, package/source archives, CycloneDX SBOM
  generation, and provenance inputs. The reproducible offline rustdoc archive
  explicitly excludes Rustdoc `1.97.1`'s nondeterministic global-search index.
- candidate-derived outbox delivery identities shared byte-for-byte by the
  reference and SQLite shells, with SQLite schema v3 rejecting old
  authorization-derived identities pending explicit migration.
- policy-bound, law-verified nominal genesis authorization; one-time pure and
  SQLite shell creation; strict receipt, bundle, and authorization decoding;
  and SQLite schema v5 full-history reauthorization with exact
  authorization/bundle/receipt/replay/outbox set reconstruction. Schema v4 and
  earlier stores are rejected pending explicit migration.
- canonical sparse-proof and authenticated-plan decoding with explicit resource
  limits, exact round-trip admission, and non-authoritative decoded plan values;
- retained-evidence projector qualification, required per-transition projection
  relations, nominal candidate-bound authenticated commits, and a production-facing
  authenticated publication port.
- a checked V1 release runbook, declared code ownership, and read-only
  reproducible package assembly for immutable `v1.0.0-rc.*` tags.

### Release-candidate limits

- no arbitrary downstream project receives production authorization merely by
  using the library;
- the authenticated sparse tree remains a bounded reference implementation;
- SQLite qualification is limited to the documented reference deployment;
- concrete Lean, SMT, CVC5, Kani, Flux, or private ESSO adapters and proof
  artifacts remain project-owned;
- concurrent execution and effect interpretation remain external shell work;
- final independent exact-head review and release attestation remain required
  before `1.0.0`.
