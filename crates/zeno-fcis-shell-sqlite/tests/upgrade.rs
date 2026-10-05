//! Checked contract upgrades of a withdrawal-queue store. Version 1 is the
//! template's contract and version 2 the adopted 100-node candidate, both
//! taken from the committed adopted fixture, whose `with_lineage` binds them
//! with the adoption's receipt digest. Variants of version 2 that change a
//! law or a limit are bound at test time from the policy the library
//! serializes for them.
use rusqlite::Connection;
use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};
use zeno_fcis_codec::{CanonicalEncode, Envelope, Hash32};
use zeno_fcis_shell::{CommitStatus, MemoryDestination};
use zeno_fcis_shell_sqlite::{
    CrashPoint,
    v2::{Error, Lineage, Opened, Store, Superseded, V2SqliteShell, V9Store, upgrade},
};
use zeno_fcis_synthesis::finite::{
    V2Resource, V2ScalarProgram,
    canonical_v2::schema as s,
    v2_authority::{self as authority, Authority, Publication, PublicationOutcome},
    v2_catalog::{self as catalog, BoundCatalog},
    v2_composition::{self as c, Raw},
    v2_laws as l,
};
use zeno_fcis_value::{Field, Value};

/// The adopted withdrawal queue: `adopted::v1` is the template's contract,
/// `adopted` the adopted candidate.
#[allow(dead_code)]
#[path = "../../zeno-fcis-cli/tests/fixtures/withdrawal-queue-adopted/src/v2_contract.rs"]
pub mod adopted;
/// A contract with another state schema.
#[allow(dead_code)]
#[path = "../../zeno-fcis-cli/templates/durable-counter/src/v2_contract.rs"]
pub mod counter;

#[track_caller]
fn ok<T, E: std::fmt::Debug>(result: Result<T, E>) -> T {
    match result {
        Ok(value) => value,
        Err(error) => panic!("unexpected refusal: {error:?}"),
    }
}

fn digest(text: &str) -> [u8; 32] {
    let mut bytes = [0; 32];
    for (index, byte) in bytes.iter_mut().enumerate() {
        *byte = ok(u8::from_str_radix(&text[2 * index..2 * index + 2], 16));
    }
    bytes
}

/// Runs `test` with the fixture's catalogs, version 1 first and version 2
/// last, and the lineage bound from them and the adoption's receipt digest.
fn with_lineage(test: impl FnOnce(&[&BoundCatalog<'_>], &Lineage<'_, '_>)) {
    ok(adopted::with_lineage(|(catalogs, receipts)| {
        assert_eq!(receipts.len(), 1);
        let receipts: Vec<[u8; 32]> = receipts.iter().map(|receipt| digest(receipt)).collect();
        let lineage = ok(Lineage::bind(catalogs, &receipts));
        test(catalogs, &lineage);
    }));
}

fn with_counter(test: impl FnOnce(&BoundCatalog<'_>)) {
    let contract = counter::Contract::new();
    let descriptor = contract.descriptor();
    test(&ok(counter::checked_catalog(&descriptor)));
}

/// The policy the library serializes for `descriptor` over version 2's
/// schema, framing and channel links.
fn variant_policy(descriptor: &c::Descriptor<'_>) -> Vec<u8> {
    authority::policy_bytes(
        descriptor,
        adopted::ORIGINAL_SCHEMA,
        &adopted::FRAMING,
        adopted::CHANNEL_ROOTS,
    )
    .unwrap_or_else(|| panic!("the variant's policy encodes"))
}

/// `descriptor` bound as its own checked catalog with `policy`.
fn variant_catalog<'a>(descriptor: &'a c::Descriptor<'a>, policy: &'a [u8]) -> BoundCatalog<'a> {
    ok(catalog::bind_original(
        adopted::ORIGINAL_SCHEMA,
        &adopted::DESCRIPTION,
        catalog::Limits {
            schema: s::Limits {
                bytes: adopted::ORIGINAL_SCHEMA.len() as u64,
                types: 64,
                fields: 64,
                variants: 64,
            },
            contract_bytes: policy.len() as u64,
        },
        policy,
        descriptor,
        &adopted::FRAMING,
        adopted::CHANNEL_ROOTS,
    ))
}

/// Law 500's program with the vault's capacity raised from 4 to 5.
fn raised_capacity<'a>(laws: &[l::Law<'a>]) -> Vec<l::Op<'a>> {
    let law = laws
        .iter()
        .find(|law| law.id == 500)
        .unwrap_or_else(|| panic!("law 500"));
    let mut nodes = law.program.nodes.to_vec();
    let bound = nodes
        .iter()
        .position(|op| matches!(op, l::Op::Literal(l::Atom::I128(4))))
        .unwrap_or_else(|| panic!("law 500 bounds the balance by 4"));
    nodes[bound] = l::Op::Literal(l::Atom::I128(5));
    nodes
}

/// Version 2's laws with law 500 reading `law_500`: a law change.
fn changed_laws<'a>(laws: &[l::Law<'a>], law_500: &'a [l::Op<'a>]) -> Vec<l::Law<'a>> {
    laws.iter()
        .map(|law| l::Law {
            id: law.id,
            kind: law.kind,
            scope: law.scope,
            genesis: law.genesis,
            program: l::Program {
                nodes: if law.id == 500 {
                    law_500
                } else {
                    law.program.nodes
                },
                root: law.program.root,
            },
        })
        .collect()
}

/// A law that reads the Step usage and always holds: usage is never below 0.
const READS_STEP_USAGE: &[l::Op<'static>] = &[
    l::Op::Observe(l::Observation::Usage(V2Resource::Step)),
    l::Op::Literal(l::Atom::U128(0)),
    l::Op::Lt(0, 1),
    l::Op::Not(2),
];

/// `laws` with one more law, 777, that reads the Step usage.
fn with_usage_law<'a>(laws: &[l::Law<'a>]) -> Vec<l::Law<'a>> {
    laws.iter()
        .map(|law| l::Law {
            id: law.id,
            kind: law.kind,
            scope: law.scope,
            genesis: law.genesis,
            program: l::Program {
                nodes: law.program.nodes,
                root: law.program.root,
            },
        })
        .chain(std::iter::once(l::Law {
            id: 777,
            kind: l::Kind::FeeAndRounding,
            scope: l::Scope::Committing,
            genesis: false,
            program: l::Program {
                nodes: READS_STEP_USAGE,
                root: 3,
            },
        }))
        .collect()
}

const HELD: upgrade::Premises = upgrade::Premises {
    step_limits_never_bind: true,
    step_usage_unobserved: true,
};

fn same_program<'a>(descriptor: &c::Descriptor<'a>) -> V2ScalarProgram<'a> {
    V2ScalarProgram {
        inputs: descriptor.program.inputs,
        outputs: descriptor.program.outputs,
        nodes: descriptor.program.nodes,
        roots: descriptor.program.roots,
    }
}

fn envelope(binding: c::FrameBinding, value: Value) -> Vec<u8> {
    ok(Envelope::new(binding.root, Hash32::new(binding.schema), value).canonical_bytes())
}

const EMPTY: u16 = 180;
const ARRIVED: u16 = 181;
const LANE_A: u16 = 170;
const LANE_B: u16 = 171;
const DEPOSIT: u16 = 160;
const REQUEST: u16 = 161;
const TICK: u16 = 162;
const OPERATOR: u16 = 190;
const OWNER_A: u16 = 191;
const OWNER_B: u16 = 192;
const KEEPER: u16 = 193;

#[allow(clippy::too_many_arguments)]
fn vault(
    balance: i128,
    lane_a: u16,
    amount_a: i128,
    lane_b: u16,
    amount_b: i128,
    pause: i128,
    must_serve: bool,
    priority: u16,
) -> Vec<u8> {
    let value = ok(Value::record_canonical(vec![
        Field::new(120, Value::signed(balance)),
        Field::new(121, Value::sum(109, lane_a, None)),
        Field::new(122, Value::signed(amount_a)),
        Field::new(123, Value::sum(109, lane_b, None)),
        Field::new(124, Value::signed(amount_b)),
        Field::new(125, Value::signed(pause)),
        Field::new(126, Value::boolean(must_serve)),
        Field::new(127, Value::sum(108, priority, None)),
    ]));
    envelope(adopted::FRAMING.state, value)
}
fn genesis_state() -> Vec<u8> {
    vault(0, EMPTY, 0, EMPTY, 0, 0, false, LANE_A)
}
/// After one payout from lane A: empty lanes, priority on lane B.
fn paid_state() -> Vec<u8> {
    vault(0, EMPTY, 0, EMPTY, 0, 0, false, LANE_B)
}
fn command(action: u16, lane: u16, amount: i128) -> Vec<u8> {
    let value = ok(Value::record_canonical(vec![
        Field::new(130, Value::sum(107, action, None)),
        Field::new(131, Value::sum(108, lane, None)),
        Field::new(132, Value::signed(amount)),
    ]));
    envelope(adopted::FRAMING.command, value)
}
fn context(caller: u16, alarm: bool) -> Vec<u8> {
    let value = ok(Value::record_canonical(vec![
        Field::new(140, Value::sum(112, caller, None)),
        Field::new(141, Value::boolean(alarm)),
    ]));
    envelope(adopted::FRAMING.context, value)
}
fn genesis<'a>(a: &'a Authority<'_>, initial: &'a [u8]) -> Publication<'a> {
    match a.publish_genesis(initial) {
        PublicationOutcome::Commit(p) => p,
        refused => panic!("genesis refused: {refused:?}"),
    }
}
fn publication<'a>(
    a: &'a Authority<'_>,
    state: &'a [u8],
    command: &'a [u8],
    context: &'a [u8],
) -> Publication<'a> {
    match a.publish(Raw {
        state,
        command,
        context,
    }) {
        PublicationOutcome::Commit(p) => p,
        refused => panic!("publication refused: {refused:?}"),
    }
}
fn key(byte: u8) -> Hash32 {
    Hash32::new([byte; 32])
}

static FILE_COUNTER: AtomicU64 = AtomicU64::new(0);
struct StoreFile(PathBuf);
impl StoreFile {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "zenofcis-v2-upgrade-{}-{}.db",
            std::process::id(),
            FILE_COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = fs::remove_file(&path);
        Self(path)
    }
}
impl Drop for StoreFile {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

type Row = (&'static str, Vec<rusqlite::types::Value>);

/// Every row of every table, read through an independent connection.
fn stored_rows(path: &Path) -> Vec<Row> {
    let external = ok(Connection::open(path));
    let mut rows = Vec::new();
    for table in [
        "v2_genesis",
        "v2_state",
        "v2_commits",
        "v2_deliveries",
        "v2_checkpoints",
        "v2_upgrades",
    ] {
        let Ok(mut statement) = external.prepare(&format!("SELECT * FROM {table} ORDER BY rowid"))
        else {
            continue;
        };
        let width = statement.column_count();
        let mut query = ok(statement.query([]));
        while let Some(row) = ok(query.next()) {
            rows.push((table, (0..width).map(|i| ok(row.get(i))).collect()));
        }
    }
    rows
}
fn user_version(path: &Path) -> i64 {
    let external = ok(Connection::open(path));
    ok(external.query_row("PRAGMA user_version", [], |r| r.get(0)))
}
fn state_row(rows: &[Row]) -> Vec<rusqlite::types::Value> {
    rows.iter()
        .find(|(table, _)| *table == "v2_state")
        .map(|(_, row)| row.clone())
        .unwrap_or_default()
}
fn other_rows(rows: &[Row]) -> Vec<Row> {
    rows.iter()
        .filter(|(table, _)| *table != "v2_state" && *table != "v2_upgrades")
        .cloned()
        .collect()
}

#[track_caller]
fn current<'a, 'p>(opened: Opened<'a, 'p>) -> V2SqliteShell<'a, 'p> {
    match opened {
        Opened::V10(Store::Current(shell)) => shell,
        Opened::V10(Store::Superseded(store)) => {
            panic!("the store runs version {}", store.version())
        }
        Opened::V9(_) => panic!("a v9 store"),
    }
}
#[track_caller]
fn superseded<'a, 'p>(opened: Opened<'a, 'p>) -> Superseded<'a, 'p> {
    match opened {
        Opened::V10(Store::Superseded(store)) => store,
        Opened::V10(Store::Current(_)) => panic!("the store is current"),
        Opened::V9(_) => panic!("a v9 store"),
    }
}
#[track_caller]
fn v9<'a, 'p>(opened: Opened<'a, 'p>) -> V9Store<'a, 'p> {
    match opened {
        Opened::V9(store) => store,
        Opened::V10(_) => panic!("a v10 store"),
    }
}
#[track_caller]
fn refused<T>(result: Result<T, Error>) -> Error {
    match result {
        Ok(_) => panic!("expected a refusal"),
        Err(error) => error,
    }
}

/// The first `steps` commits of a session under `v1`: two deposits, two
/// requests and two paying ticks. After three the state is away from
/// genesis, with priority on lane B and lane A's payout pending; after six it
/// is back at genesis with both payouts pending.
fn store_under<'a, 'p>(path: &Path, v1: &'a Authority<'p>, steps: usize) -> V2SqliteShell<'a, 'p> {
    let initial = genesis_state();
    let mut db = ok(V2SqliteShell::create(path, v1, genesis(v1, &initial)));
    let session: [(Vec<u8>, Vec<u8>, Vec<u8>); 6] = [
        (
            genesis_state(),
            command(DEPOSIT, LANE_A, 2),
            context(OPERATOR, false),
        ),
        (
            vault(2, EMPTY, 0, EMPTY, 0, 0, false, LANE_A),
            command(REQUEST, LANE_A, 2),
            context(OWNER_A, false),
        ),
        (
            vault(2, ARRIVED, 2, EMPTY, 0, 0, false, LANE_A),
            command(TICK, LANE_A, 1),
            context(KEEPER, false),
        ),
        (
            paid_state(),
            command(DEPOSIT, LANE_A, 2),
            context(OPERATOR, false),
        ),
        (
            vault(2, EMPTY, 0, EMPTY, 0, 0, false, LANE_B),
            command(REQUEST, LANE_B, 2),
            context(OWNER_B, false),
        ),
        (
            vault(2, EMPTY, 0, ARRIVED, 2, 0, false, LANE_B),
            command(TICK, LANE_A, 1),
            context(KEEPER, false),
        ),
    ];
    for (index, (state, cmd, ctx)) in session.iter().take(steps).enumerate() {
        assert_eq!(ok(db.snapshot()).state(), state.as_slice(), "step {index}");
        let receipt = ok(db.commit(key(index as u8 + 1), publication(v1, state, cmd, ctx)));
        assert_eq!(receipt.status(), CommitStatus::Committed);
    }
    db
}

#[test]
fn an_adoption_is_a_program_successor_and_nothing_else_is() {
    with_lineage(|catalogs, _| {
        let (v1, v2) = (catalogs[0], catalogs[1]);
        assert!(ok(upgrade::program_successor(v1, v2)));
        assert!(ok(upgrade::program_successor(v2, v1)));
        assert!(ok(upgrade::program_successor(v2, v2)));
        // Another state schema.
        with_counter(|counter| {
            assert!(!ok(upgrade::program_successor(v1, counter)));
            assert!(!ok(upgrade::program_successor(counter, v2)));
        });
        let contract = adopted::Contract::new();
        // A changed law, with or without the adopted program.
        let base = contract.descriptor();
        let law_500 = raised_capacity(base.laws);
        let laws = changed_laws(base.laws, &law_500);
        let law = c::Descriptor {
            laws: &laws,
            ..contract.descriptor()
        };
        let law_policy = variant_policy(&law);
        let changed_law = variant_catalog(&law, &law_policy);
        assert!(!ok(upgrade::program_successor(v1, &changed_law)));
        assert!(!ok(upgrade::program_successor(v2, &changed_law)));
        // Another limit than the Step limit.
        let base = contract.descriptor();
        let read = c::Descriptor {
            limits: base
                .limits
                .with_limit(V2Resource::Read, base.limits.limit(V2Resource::Read) + 1),
            program: same_program(&base),
            ..contract.descriptor()
        };
        let read_policy = variant_policy(&read);
        let changed_read = variant_catalog(&read, &read_policy);
        assert!(!ok(upgrade::program_successor(v1, &changed_read)));
        assert!(!ok(upgrade::program_successor(v2, &changed_read)));
        // The Step limit alone is part of the program's succession.
        let base = contract.descriptor();
        let step = c::Descriptor {
            limits: base
                .limits
                .with_limit(V2Resource::Step, base.limits.limit(V2Resource::Step) + 1),
            program: same_program(&base),
            ..contract.descriptor()
        };
        let step_policy = variant_policy(&step);
        let changed_step = variant_catalog(&step, &step_policy);
        assert!(ok(upgrade::program_successor(v1, &changed_step)));
        assert!(ok(upgrade::program_successor(v2, &changed_step)));
        // A program succession needs the adoption receipts between the two.
        assert!(matches!(
            upgrade::Successor::establish(v1, v2, &[]),
            Err(Error::Lineage)
        ));
        let receipts = [Hash32::new([7; 32])];
        let successor = ok(upgrade::Successor::establish(v1, v2, &receipts))
            .unwrap_or_else(|| panic!("version 2 succeeds version 1"));
        assert_eq!(successor.receipts(), receipts);
        assert_eq!(successor.premises(), HELD);
        assert!(ok(upgrade::Successor::establish(v1, &changed_law, &receipts)).is_none());
        let premises = |from: &BoundCatalog<'_>, to: &BoundCatalog<'_>| {
            ok(upgrade::Successor::establish(from, to, &receipts))
                .unwrap_or_else(|| panic!("a program successor"))
                .premises()
        };
        assert_eq!(premises(v1, &changed_step), HELD);
        // A Step limit below one Step per program and law node could bind.
        let base = contract.descriptor();
        let nodes = base.program.nodes.len()
            + base
                .laws
                .iter()
                .map(|law| law.program.nodes.len())
                .sum::<usize>();
        let tight = c::Descriptor {
            limits: base.limits.with_limit(V2Resource::Step, nodes as u64 - 1),
            program: same_program(&base),
            ..contract.descriptor()
        };
        let tight_policy = variant_policy(&tight);
        let tight_limit = variant_catalog(&tight, &tight_policy);
        assert!(ok(upgrade::program_successor(v1, &tight_limit)));
        assert_eq!(
            premises(v1, &tight_limit),
            upgrade::Premises {
                step_limits_never_bind: false,
                ..HELD
            }
        );
        // Both versions with a law that reads the Step usage, which the
        // program change alters: still a program successor, but not with
        // every premise.
        let old_contract = adopted::v1::Contract::new();
        let old_base = old_contract.descriptor();
        let old_laws = with_usage_law(old_base.laws);
        let mut old_required = old_base.required.to_vec();
        old_required.push(777);
        let old_reading = c::Descriptor {
            laws: &old_laws,
            required: &old_required,
            ..old_contract.descriptor()
        };
        let base = contract.descriptor();
        let new_laws = with_usage_law(base.laws);
        let mut new_required = base.required.to_vec();
        new_required.push(777);
        let new_reading = c::Descriptor {
            laws: &new_laws,
            required: &new_required,
            ..contract.descriptor()
        };
        let (old_policy, new_policy) = (variant_policy(&old_reading), variant_policy(&new_reading));
        let (old_catalog, new_catalog) = (
            variant_catalog(&old_reading, &old_policy),
            variant_catalog(&new_reading, &new_policy),
        );
        assert!(ok(upgrade::program_successor(&old_catalog, &new_catalog)));
        assert_eq!(
            premises(&old_catalog, &new_catalog),
            upgrade::Premises {
                step_usage_unobserved: false,
                ..HELD
            }
        );
    });
}

#[test]
fn a_store_away_from_genesis_upgrades_to_a_program_successor_and_keeps_its_delivery() {
    with_lineage(|catalogs, lineage| {
        let (v1, v2) = (&lineage.authorities()[0], &lineage.authorities()[1]);
        assert_ne!(v1.identity(), v2.identity());
        assert_eq!(
            catalogs[0].original_schema(),
            catalogs[1].original_schema(),
            "the adoption changed the program, not the schema"
        );
        let file = StoreFile::new();
        let mut db = store_under(&file.0, v1, 3);
        let snapshot = ok(db.snapshot());
        assert_eq!(snapshot.version(), 3);
        assert_eq!(snapshot.state(), paid_state());
        assert_ne!(snapshot.state(), genesis_state());
        assert_eq!(snapshot.pending(), 1);
        let pending = ok(db.next_pending()).unwrap_or_else(|| panic!("lane A's payout pending"));
        assert_eq!(pending.version(), 3);
        let head = ok(db.checkpoint());
        drop(db);

        // Version 2's genesis laws refuse this state: law 990 admits only
        // the declared genesis state.
        assert!(!matches!(
            v2.publish_genesis(&paid_state()),
            PublicationOutcome::Commit(_)
        ));
        // The whole lineage opens and audits the store at version 1.
        let mut store = superseded(ok(lineage.open(&file.0)));
        assert_eq!(store.version(), 1);
        assert_eq!(store.authority().identity(), v1.identity());
        ok(store.audit());
        let snapshot = ok(store.snapshot());
        assert_eq!((snapshot.version(), snapshot.pending()), (3, 1));
        assert_eq!(snapshot.binding(), v1.identity());
        let before = stored_rows(&file.0);

        let (mut db, receipt) = ok(store.upgrade());
        assert_eq!(receipt.kind(), upgrade::Kind::ProgramSuccessor);
        assert_eq!(receipt.kind().tag(), "program-successor");
        assert_eq!(receipt.premises(), Some(HELD));
        assert_eq!(receipt.ordinal(), 1);
        assert_eq!(receipt.version(), 3);
        assert_eq!(receipt.from_identity(), v1.identity());
        assert_eq!(receipt.identity(), v2.identity());
        assert_eq!(
            receipt.receipts(),
            [Hash32::new(digest(adopted::ADOPTION_RECEIPTS[0]))]
        );
        assert!(receipt.record().starts_with(b"ZFCISV2-SUCCESSOR\0"));
        // Exactly the upgrade row is added and only the head's chain tip moves.
        let after = stored_rows(&file.0);
        assert_eq!(after.len(), before.len() + 1);
        let (old_state, new_state) = (state_row(&before), state_row(&after));
        assert_eq!(old_state[..4], new_state[..4], "sequence, state and root");
        assert_eq!(
            new_state[4],
            rusqlite::types::Value::Blob(receipt.chain().as_bytes().to_vec())
        );
        assert_ne!(old_state[4], new_state[4]);
        assert_eq!(other_rows(&before), other_rows(&after));
        assert_eq!(user_version(&file.0), 10);

        // The returned handle runs version 2.
        assert_eq!(db.authority().identity(), v2.identity());
        assert_eq!(db.lineage().len(), 2);
        let snapshot = ok(db.snapshot());
        assert_eq!(
            (snapshot.version(), snapshot.upgrades(), snapshot.pending()),
            (3, 1, 1)
        );
        assert_eq!(snapshot.binding(), v2.identity());
        assert_eq!(snapshot.chain(), receipt.chain());
        // The pending payout keeps its certificate-bound ID and is delivered
        // exactly once.
        assert_eq!(ok(db.next_pending()), Some(pending.clone()));
        let mut destination = MemoryDestination::default();
        assert!(ok(db.deliver_next_memory(&mut destination)));
        assert!(!ok(db.deliver_next_memory(&mut destination)));
        assert_eq!(destination.delivered_count(), 1);
        assert_eq!(ok(db.snapshot()).pending(), 0);

        // The store keeps committing, under version 2 only.
        let deposit = command(DEPOSIT, LANE_A, 1);
        let operator = context(OPERATOR, false);
        let paid = paid_state();
        assert!(matches!(
            db.commit(key(41), publication(v1, &paid, &deposit, &operator)),
            Err(Error::Identity)
        ));
        let committed = ok(db.commit(key(41), publication(v2, &paid, &deposit, &operator)));
        assert_eq!(committed.status(), CommitStatus::Committed);
        assert_eq!(committed.version(), 4);
        assert_eq!(
            ok(db.snapshot()).state(),
            vault(1, EMPTY, 0, EMPTY, 0, 0, false, LANE_B)
        );
        let replayed = ok(db.commit(key(41), publication(v2, &paid, &deposit, &operator)));
        assert_eq!(replayed.status(), CommitStatus::IdempotentReplay);
        // A key committed before the upgrade, retried with the same request
        // under version 2, is not an idempotent replay here: this adoption
        // changes the sealed usage observations. Nothing is written.
        let initial = genesis_state();
        let first_deposit = command(DEPOSIT, LANE_A, 2);
        let rows = stored_rows(&file.0);
        assert!(matches!(
            db.commit(key(1), publication(v2, &initial, &first_deposit, &operator)),
            Err(Error::Replay)
        ));
        assert_eq!(stored_rows(&file.0), rows);

        // Both segments replay: a full audit, a fresh open, and a checkpoint
        // taken before the upgrade resume the tail across it.
        let checkpoint = ok(db.audit());
        drop(db);
        let mut reopened = current(ok(lineage.open(&file.0)));
        assert_eq!(ok(reopened.snapshot()).version(), 4);
        assert_eq!(ok(reopened.snapshot()).upgrades(), 1);
        drop(reopened);
        for start in [&head, &checkpoint] {
            let Store::Current(mut resumed) = ok(lineage.open_at_checkpoint(&file.0, start)) else {
                panic!("the upgraded store is current");
            };
            assert_eq!(ok(resumed.snapshot()).version(), 4);
        }
        // Neither version alone replays both segments.
        assert!(matches!(
            V2SqliteShell::open(&file.0, v1),
            Err(Error::Identity)
        ));
        assert!(matches!(
            V2SqliteShell::open(&file.0, v2),
            Err(Error::Identity)
        ));
        assert!(matches!(
            V2SqliteShell::open_at_checkpoint(&file.0, v2, &checkpoint),
            Err(Error::Identity)
        ));
        let alone = ok(Lineage::bind(&[catalogs[1]], &[]));
        assert!(matches!(alone.open(&file.0), Err(Error::Identity)));
    });
}

#[test]
fn a_store_at_genesis_also_upgrades_as_a_program_successor() {
    with_lineage(|_, lineage| {
        let v1 = &lineage.authorities()[0];
        let file = StoreFile::new();
        let db = store_under(&file.0, v1, 6);
        drop(db);
        let (mut db, receipt) = ok(superseded(ok(lineage.open(&file.0))).upgrade());
        assert_eq!(receipt.kind(), upgrade::Kind::ProgramSuccessor);
        let mut destination = MemoryDestination::default();
        while ok(db.deliver_next_memory(&mut destination)) {}
        assert_eq!(destination.delivered_count(), 2);
        ok(db.audit());
    });
}

#[test]
fn another_contract_upgrades_only_through_its_genesis_laws() {
    with_lineage(|catalogs, lineage| {
        let v1 = &lineage.authorities()[0];
        let contract = adopted::Contract::new();
        let base = contract.descriptor();
        let law_500 = raised_capacity(base.laws);
        let laws = changed_laws(base.laws, &law_500);
        let law = c::Descriptor {
            laws: &laws,
            ..contract.descriptor()
        };
        let policy = variant_policy(&law);
        let changed_law = variant_catalog(&law, &policy);
        let changed = ok(Lineage::bind(&[catalogs[0], &changed_law], &[[9; 32]]));

        // Away from genesis: refused, naming the library's refusal, and
        // nothing is written.
        let file = StoreFile::new();
        drop(store_under(&file.0, v1, 3));
        let rows = stored_rows(&file.0);
        let bytes = ok(fs::read(&file.0));
        let refusal = refused(superseded(ok(changed.open(&file.0))).upgrade());
        assert!(
            matches!(
                refusal,
                Error::Upgrade(upgrade::Refusal::Genesis(Some(authority::Refusal::Core(_))))
            ),
            "{refusal:?}"
        );
        assert_eq!(stored_rows(&file.0), rows);
        assert_eq!(ok(fs::read(&file.0)), bytes);
        // A policy that differs from version 1 in any other limit than the
        // Step limit is no program successor either.
        let base = contract.descriptor();
        let read = c::Descriptor {
            limits: base
                .limits
                .with_limit(V2Resource::Read, base.limits.limit(V2Resource::Read) + 1),
            program: same_program(&base),
            ..contract.descriptor()
        };
        let read_policy = variant_policy(&read);
        let changed_read = variant_catalog(&read, &read_policy);
        let other_limit = ok(Lineage::bind(&[catalogs[0], &changed_read], &[[9; 32]]));
        let refusal = refused(superseded(ok(other_limit.open(&file.0))).upgrade());
        assert!(
            matches!(
                refusal,
                Error::Upgrade(upgrade::Refusal::Genesis(Some(authority::Refusal::Core(_))))
            ),
            "{refusal:?}"
        );
        assert_eq!(stored_rows(&file.0), rows);
        assert_eq!(ok(fs::read(&file.0)), bytes);
        // The adopted program successor upgrades the same store.
        let (_, receipt) = ok(superseded(ok(lineage.open(&file.0))).upgrade());
        assert_eq!(receipt.kind(), upgrade::Kind::ProgramSuccessor);

        // At the declared genesis state, the changed law's genesis laws admit
        // it, and the record binds that genesis publication.
        let file = StoreFile::new();
        drop(store_under(&file.0, v1, 6));
        let (mut db, receipt) = ok(superseded(ok(changed.open(&file.0))).upgrade());
        assert_eq!(receipt.kind(), upgrade::Kind::GenesisAdmission);
        assert_eq!(receipt.kind().tag(), "genesis-admission");
        assert_eq!(receipt.premises(), None);
        assert!(receipt.receipts().is_empty());
        assert!(receipt.record().starts_with(b"ZFCISV2-UPGRADE\0"));
        assert_eq!(ok(db.snapshot()).upgrades(), 1);
        ok(db.audit());
        drop(db);
        let mut reopened = current(ok(changed.open(&file.0)));
        assert_eq!(ok(reopened.snapshot()).pending(), 2);
        // The adoption's lineage cannot replay a segment under the variant.
        drop(reopened);
        assert!(matches!(lineage.open(&file.0), Err(Error::Identity)));
    });
}

#[test]
fn refusals_write_nothing() {
    with_lineage(|catalogs, lineage| {
        let (v1, v2) = (&lineage.authorities()[0], &lineage.authorities()[1]);
        // A different state schema.
        with_counter(|counter| {
            let schemas = ok(Lineage::bind(&[catalogs[0], counter], &[[9; 32]]));
            let file = StoreFile::new();
            drop(store_under(&file.0, v1, 3));
            let rows = stored_rows(&file.0);
            let bytes = ok(fs::read(&file.0));
            assert!(matches!(
                refused(superseded(ok(schemas.open(&file.0))).upgrade()),
                Error::Upgrade(upgrade::Refusal::StateSchema)
            ));
            assert_eq!(stored_rows(&file.0), rows);
            assert_eq!(ok(fs::read(&file.0)), bytes);
        });
        let file = StoreFile::new();
        drop(store_under(&file.0, v1, 3));
        let rows = stored_rows(&file.0);
        let bytes = ok(fs::read(&file.0));
        // A lineage without the store's contract.
        let alone = ok(Lineage::bind(&[catalogs[1]], &[]));
        assert!(matches!(alone.open(&file.0), Err(Error::Identity)));
        // A lineage that ends with the store's contract has nothing to
        // upgrade to: the store is current under it.
        let reversed = ok(Lineage::bind(&[catalogs[1], catalogs[0]], &[[9; 32]]));
        assert_eq!(
            current(ok(reversed.open(&file.0))).authority().identity(),
            v1.identity()
        );
        // Malformed lineages bind nothing.
        assert!(matches!(Lineage::bind(&[], &[]), Err(Error::Lineage)));
        assert!(matches!(Lineage::bind(catalogs, &[]), Err(Error::Lineage)));
        assert!(matches!(
            Lineage::bind(&[catalogs[0]], &[[9; 32]]),
            Err(Error::Lineage)
        ));
        assert_eq!(stored_rows(&file.0), rows);
        assert_eq!(ok(fs::read(&file.0)), bytes);
        // Once upgraded, a lineage without version 1 cannot replay the first segment.
        ok(superseded(ok(lineage.open(&file.0))).upgrade());
        assert!(matches!(alone.open(&file.0), Err(Error::Identity)));
        let _ = v2;
    });
}

#[test]
fn refused_operations_on_a_missing_path_create_no_file() {
    with_lineage(|_, lineage| {
        let v1 = &lineage.authorities()[0];
        let file = StoreFile::new();
        let mut db = store_under(&file.0, v1, 1);
        let checkpoint = ok(db.checkpoint());
        drop(db);
        let missing = StoreFile::new();
        assert!(matches!(
            V2SqliteShell::open(&missing.0, v1),
            Err(Error::Sqlite(_))
        ));
        assert!(matches!(
            V2SqliteShell::open_at_checkpoint(&missing.0, v1, &checkpoint),
            Err(Error::Sqlite(_))
        ));
        assert!(matches!(lineage.open(&missing.0), Err(Error::Sqlite(_))));
        assert!(matches!(
            lineage.open_at_checkpoint(&missing.0, &checkpoint),
            Err(Error::Sqlite(_))
        ));
        assert!(!missing.0.exists(), "a refused open created the file");
    });
}

#[test]
fn forged_or_altered_upgrade_records_are_refused() {
    with_lineage(|_, lineage| {
        let (v1, v2) = (&lineage.authorities()[0], &lineage.authorities()[1]);
        let file = StoreFile::new();
        drop(store_under(&file.0, v1, 3));
        let unchanged = stored_rows(&file.0);
        // A malformed record from outside the shell, with the right identity.
        let external = ok(Connection::open(&file.0));
        let (root, chain): (Vec<u8>, Vec<u8>) = ok(external.query_row(
            "SELECT root,chain FROM v2_state WHERE singleton=1",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        ));
        ok(external.execute(
            "INSERT INTO v2_upgrades VALUES(1,3,?1,x'00',?2,?3,x'00',zeroblob(32))",
            rusqlite::params![v2.identity(), root, chain],
        ));
        assert!(matches!(lineage.open(&file.0), Err(Error::History)));
        assert!(matches!(
            V2SqliteShell::open(&file.0, v1),
            Err(Error::Identity)
        ));
        ok(external.execute("DELETE FROM v2_upgrades", []));
        assert_eq!(stored_rows(&file.0), unchanged);
        drop(external);

        let (mut db, _) = ok(superseded(ok(lineage.open(&file.0))).upgrade());
        let paid = paid_state();
        let deposit = command(DEPOSIT, LANE_A, 1);
        let operator = context(OPERATOR, false);
        ok(db.commit(key(50), publication(v2, &paid, &deposit, &operator)));
        let live = ok(db.snapshot());
        drop(db);
        let genuine = stored_rows(&file.0);
        let receipts = digest(adopted::ADOPTION_RECEIPTS[0]);
        // Each alteration names the refusal under the lineage and under
        // version 1 alone. A record's fields fail recomputation, as do
        // evidence of another length or digest and another kind; an ordinal
        // out of order fails before any Authority is consulted; without its
        // upgrade row the store claims to run version 1 throughout, which
        // cannot replay the commit made under version 2.
        let other = Hash32::new([7; 32]);
        let mut doubled = receipts.to_vec();
        doubled.extend_from_slice(&receipts);
        // The same record claiming the other kind.
        let rusqlite::types::Value::Blob(record) = genuine
            .iter()
            .find(|(table, _)| *table == "v2_upgrades")
            .map(|(_, row)| row[6].clone())
            .unwrap_or_else(|| panic!("the genuine upgrade row"))
        else {
            panic!("a record blob")
        };
        let mut relabelled = b"ZFCISV2-UPGRADE\0".to_vec();
        relabelled.extend_from_slice(&record[b"ZFCISV2-SUCCESSOR\0".len()..]);
        // The same record claiming a premise failed: the premises byte follows
        // the magic, the ordinal, the sequence, both framed identities and the
        // admission's length.
        let premises_at = 18 + 8 + 8 + 8 + v1.identity().len() + 8 + v2.identity().len() + 8;
        assert_eq!(record[premises_at], 0b111);
        let mut understated = record.clone();
        understated[premises_at] = 0b101;
        for (alteration, parameter, under_lineage, alone) in [
            (
                "UPDATE v2_upgrades SET record=x'00'",
                None,
                "History",
                "Identity",
            ),
            (
                "UPDATE v2_upgrades SET chain=zeroblob(32)",
                None,
                "History",
                "Identity",
            ),
            (
                "UPDATE v2_upgrades SET sequence=2",
                None,
                "History",
                "Identity",
            ),
            (
                "UPDATE v2_upgrades SET root=zeroblob(32)",
                None,
                "History",
                "Identity",
            ),
            (
                "UPDATE v2_upgrades SET previous_chain=zeroblob(32)",
                None,
                "History",
                "Identity",
            ),
            (
                "UPDATE v2_upgrades SET publication=x'00'",
                None,
                "History",
                "Identity",
            ),
            (
                "UPDATE v2_upgrades SET publication=?1",
                Some(other.as_bytes().to_vec()),
                "History",
                "Identity",
            ),
            (
                "UPDATE v2_upgrades SET publication=?1",
                Some(doubled.clone()),
                "History",
                "Identity",
            ),
            (
                "UPDATE v2_upgrades SET record=?1",
                Some(relabelled.clone()),
                "History",
                "Identity",
            ),
            (
                "UPDATE v2_upgrades SET record=?1",
                Some(understated.clone()),
                "History",
                "Identity",
            ),
            (
                "UPDATE v2_upgrades SET ordinal=2",
                None,
                "History",
                "History",
            ),
            ("DELETE FROM v2_upgrades", None, "History", "History"),
        ] {
            let external = ok(Connection::open(&file.0));
            match &parameter {
                Some(bytes) => ok(external.execute(alteration, [bytes])),
                None => ok(external.execute(alteration, [])),
            };
            drop(external);
            let expected = |name: &str, refused: &Result<(), Error>| match name {
                "History" => matches!(refused, Err(Error::History)),
                _ => matches!(refused, Err(Error::Identity)),
            };
            let refused = lineage.open(&file.0).map(|_| ());
            assert!(
                expected(under_lineage, &refused),
                "{alteration}: {refused:?}, expected {under_lineage}"
            );
            let refused = V2SqliteShell::open(&file.0, v1).map(|_| ());
            assert!(
                expected(alone, &refused),
                "{alteration} under version 1 alone: {refused:?}, expected {alone}"
            );
            // Restore the genuine rows for the next alteration.
            let external = ok(Connection::open(&file.0));
            ok(external.execute("DELETE FROM v2_upgrades", []));
            let genuine_upgrade = genuine
                .iter()
                .find(|(table, _)| *table == "v2_upgrades")
                .unwrap_or_else(|| panic!("the genuine upgrade row"));
            ok(external.execute(
                "INSERT INTO v2_upgrades VALUES(?1,?2,?3,?4,?5,?6,?7,?8)",
                rusqlite::params_from_iter(genuine_upgrade.1.iter()),
            ));
            drop(external);
            assert_eq!(stored_rows(&file.0), genuine);
        }
        // Relabelling the upgrade with the old identity breaks the lineage order.
        let external = ok(Connection::open(&file.0));
        ok(external.execute("UPDATE v2_upgrades SET identity=?1", [v1.identity()]));
        drop(external);
        assert!(matches!(lineage.open(&file.0), Err(Error::Identity)));
        let external = ok(Connection::open(&file.0));
        ok(external.execute("UPDATE v2_upgrades SET identity=?1", [v2.identity()]));
        drop(external);
        // Genuine again: a live handle also refuses an alteration made under it.
        let mut db = current(ok(lineage.open(&file.0)));
        assert_eq!(ok(db.snapshot()), live);
        let external = ok(Connection::open(&file.0));
        ok(external.execute("UPDATE v2_upgrades SET record=x'00'", []));
        drop(external);
        assert!(matches!(db.snapshot(), Err(Error::History)));
        assert!(db.next_pending().is_err());
    });
}

/// The chain shows that each segment is valid under the version it names,
/// not who recorded it: a record computed outside the shell, with the same
/// pure decision and made-up receipt digests, is accepted. Detecting that
/// needs a tip held outside the file.
#[test]
fn a_correctly_recomputed_record_is_accepted_without_authorization() {
    with_lineage(|catalogs, lineage| {
        let (v1, v2) = (&lineage.authorities()[0], &lineage.authorities()[1]);
        let file = StoreFile::new();
        drop(store_under(&file.0, v1, 3));
        let external = ok(Connection::open(&file.0));
        let (sequence, state, root, chain): (i64, Vec<u8>, Vec<u8>, Vec<u8>) = ok(external
            .query_row(
                "SELECT sequence,state,root,chain FROM v2_state WHERE singleton=1",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
            ));
        let made_up = [Hash32::new([0xee; 32])];
        let successor = ok(upgrade::Successor::establish(
            catalogs[0],
            catalogs[1],
            &made_up,
        ))
        .unwrap_or_else(|| panic!("a program successor"));
        let hash = |bytes: &[u8]| Hash32::new(ok(bytes.try_into()));
        let plan = ok(upgrade::decide(&upgrade::Facts {
            ordinal: 1,
            sequence: sequence as u64,
            root: hash(&root),
            previous_chain: hash(&chain),
            from_identity: v1.identity(),
            from_schema: catalogs[0].original_schema(),
            identity: v2.identity(),
            schema: catalogs[1].original_schema(),
            state: &state,
            admission: upgrade::Admission::Successor(successor),
        }));
        ok(external.execute(
            "INSERT INTO v2_upgrades VALUES(1,?1,?2,?3,?4,?5,?6,?7)",
            rusqlite::params![
                sequence,
                v2.identity(),
                plan.evidence(),
                root,
                chain,
                plan.record(),
                plan.chain().as_bytes().as_slice()
            ],
        ));
        ok(external.execute(
            "UPDATE v2_state SET chain=?1",
            [plan.chain().as_bytes().as_slice()],
        ));
        drop(external);
        let mut db = current(ok(lineage.open(&file.0)));
        ok(db.audit());
        assert_eq!(ok(db.snapshot()).upgrades(), 1);
    });
}

#[test]
fn a_checkpoint_superseded_at_its_head_is_told_apart() {
    with_lineage(|_, lineage| {
        let v1 = &lineage.authorities()[0];
        let file = StoreFile::new();
        let mut db = store_under(&file.0, v1, 3);
        let before = ok(db.checkpoint());
        drop(db);
        let (mut db, _) = ok(superseded(ok(lineage.open(&file.0))).upgrade());
        // A save at the same head now holds the post-upgrade chain link.
        let after = ok(db.audit());
        drop(db);
        assert!(matches!(
            lineage.open_at_checkpoint(&file.0, &before),
            Err(Error::Checkpoint)
        ));
        let Store::Current(mut resumed) = ok(lineage.open_at_checkpoint(&file.0, &after)) else {
            panic!("the upgraded store is current");
        };
        assert_eq!(ok(resumed.snapshot()).version(), 3);
        drop(resumed);
        // A full open is unaffected.
        current(ok(lineage.open(&file.0)));
    });
}

#[test]
fn upgrade_and_migration_crashes_before_commit_write_nothing() {
    with_lineage(|_, lineage| {
        let v1 = &lineage.authorities()[0];
        let file = StoreFile::new();
        drop(store_under(&file.0, v1, 3));
        let rows = stored_rows(&file.0);
        let bytes = ok(fs::read(&file.0));
        for point in [
            CrashPoint::BeforeTransaction,
            CrashPoint::AfterValidation,
            CrashPoint::AfterStateWrite,
            CrashPoint::AfterReplayWrite,
            CrashPoint::BeforeCommit,
        ] {
            let store = superseded(ok(lineage.open(&file.0)));
            assert!(matches!(
                store.upgrade_with_crash(Some(point)),
                Err(Error::InjectedCrash(crashed)) if crashed == point
            ));
            assert_eq!(stored_rows(&file.0), rows, "{point:?}");
            assert_eq!(ok(fs::read(&file.0)), bytes, "{point:?}");
            assert_eq!(superseded(ok(lineage.open(&file.0))).version(), 1);
        }
        let store = superseded(ok(lineage.open(&file.0)));
        assert!(matches!(
            store.upgrade_with_crash(Some(CrashPoint::AfterCommit)),
            Err(Error::InjectedCrash(CrashPoint::AfterCommit))
        ));
        let mut db = current(ok(lineage.open(&file.0)));
        assert_eq!(ok(db.snapshot()).upgrades(), 1);
        drop(db);

        // The same for the migration of a v9 store.
        let file = StoreFile::new();
        drop(store_under(&file.0, v1, 3));
        let external = ok(Connection::open(&file.0));
        ok(external.execute_batch("DROP TABLE v2_upgrades; PRAGMA user_version=9;"));
        drop(external);
        let rows = stored_rows(&file.0);
        let bytes = ok(fs::read(&file.0));
        for point in [
            CrashPoint::BeforeTransaction,
            CrashPoint::AfterValidation,
            CrashPoint::AfterStateWrite,
            CrashPoint::BeforeCommit,
        ] {
            assert!(matches!(
                v9(ok(lineage.open(&file.0))).migrate_with_crash(Some(point)),
                Err(Error::InjectedCrash(crashed)) if crashed == point
            ));
            assert_eq!(stored_rows(&file.0), rows, "{point:?}");
            assert_eq!(ok(fs::read(&file.0)), bytes, "{point:?}");
            assert_eq!(user_version(&file.0), 9);
        }
        assert!(matches!(
            v9(ok(lineage.open(&file.0))).migrate_with_crash(Some(CrashPoint::AfterCommit)),
            Err(Error::InjectedCrash(CrashPoint::AfterCommit))
        ));
        assert_eq!(user_version(&file.0), 10);
        assert_eq!(superseded(ok(lineage.open(&file.0))).version(), 1);
    });
}

#[test]
fn a_v9_store_needs_the_explicit_migration() {
    with_lineage(|catalogs, lineage| {
        let v1 = &lineage.authorities()[0];
        let file = StoreFile::new();
        drop(store_under(&file.0, v1, 6));
        // Exactly what the previous shell left behind: no upgrade table, version 9.
        let external = ok(Connection::open(&file.0));
        ok(external.execute_batch("DROP TABLE v2_upgrades; PRAGMA user_version=9;"));
        drop(external);
        let rows = stored_rows(&file.0);
        let bytes = ok(fs::read(&file.0));
        assert!(matches!(
            V2SqliteShell::open(&file.0, v1),
            Err(Error::Schema(9))
        ));
        // A lineage without the store's contract cannot migrate it.
        let alone = ok(Lineage::bind(&[catalogs[1]], &[]));
        assert!(matches!(
            v9(ok(alone.open(&file.0))).migrate(),
            Err(Error::Identity)
        ));
        assert_eq!(stored_rows(&file.0), rows);
        assert_eq!(ok(fs::read(&file.0)), bytes);
        assert_eq!(user_version(&file.0), 9);
        // A store that is not exactly v9 is not even opened as one.
        let external = ok(Connection::open(&file.0));
        ok(external.execute_batch(
            "CREATE TRIGGER evil AFTER INSERT ON v2_commits BEGIN DELETE FROM v2_deliveries; END;",
        ));
        assert!(matches!(lineage.open(&file.0), Err(Error::Schema(9))));
        ok(external.execute_batch("DROP TRIGGER evil;"));
        drop(external);
        assert_eq!(user_version(&file.0), 9);

        // The migration audits the store under version 1, which it runs.
        let Store::Superseded(mut store) = ok(v9(ok(lineage.open(&file.0))).migrate()) else {
            panic!("the migrated store runs version 1");
        };
        assert_eq!(store.version(), 1);
        assert_eq!(user_version(&file.0), 10);
        let snapshot = ok(store.snapshot());
        assert_eq!(
            (snapshot.version(), snapshot.pending(), snapshot.upgrades()),
            (6, 2, 0)
        );
        drop(store);
        assert_eq!(stored_rows(&file.0), rows);
        // Migrated stores are ordinary v10 stores: they open and upgrade.
        ok(V2SqliteShell::open(&file.0, v1));
        let (mut db, _) = ok(superseded(ok(lineage.open(&file.0))).upgrade());
        assert_eq!(ok(db.snapshot()).upgrades(), 1);
        let mut destination = MemoryDestination::default();
        while ok(db.deliver_next_memory(&mut destination)) {}
        assert_eq!(destination.delivered_count(), 2);
        ok(db.audit());
        // A v9 store under the last version migrates straight to current.
        let file = StoreFile::new();
        let v2 = &lineage.authorities()[1];
        let initial = genesis_state();
        drop(ok(V2SqliteShell::create(
            &file.0,
            v2,
            genesis(v2, &initial),
        )));
        let external = ok(Connection::open(&file.0));
        ok(external.execute_batch("DROP TABLE v2_upgrades; PRAGMA user_version=9;"));
        drop(external);
        let Store::Current(_) = ok(v9(ok(lineage.open(&file.0))).migrate()) else {
            panic!("the migrated store runs version 2");
        };
    });
}

#[test]
fn a_live_handle_bound_to_the_old_version_stops_at_another_connections_upgrade() {
    with_lineage(|_, lineage| {
        let (v1, v2) = (&lineage.authorities()[0], &lineage.authorities()[1]);
        let file = StoreFile::new();
        let mut old = store_under(&file.0, v1, 3);
        let (upgraded, receipt) = ok(superseded(ok(lineage.open(&file.0))).upgrade());
        drop(upgraded);
        // The cached tip is stale and the refreshed store runs another contract.
        assert!(matches!(old.snapshot(), Err(Error::Identity)));
        let paid = paid_state();
        let deposit = command(DEPOSIT, LANE_A, 1);
        let operator = context(OPERATOR, false);
        assert!(matches!(
            old.commit(key(60), publication(v1, &paid, &deposit, &operator)),
            Err(Error::Identity)
        ));
        assert!(old.next_pending().is_err());
        assert!(old.checkpoint().is_err());
        drop(old);
        // Nothing of the refused calls reached the store.
        let mut db = current(ok(lineage.open(&file.0)));
        let snapshot = ok(db.snapshot());
        assert_eq!(snapshot.version(), 3);
        assert_eq!(snapshot.chain(), receipt.chain());
        assert_eq!(snapshot.pending(), 1);
        assert_eq!(snapshot.binding(), v2.identity());
    });
}

#[test]
fn a_store_created_under_version_two_skips_version_one() {
    with_lineage(|_, lineage| {
        let (v1, v2) = (&lineage.authorities()[0], &lineage.authorities()[1]);
        let file = StoreFile::new();
        let initial = genesis_state();
        let mut db = ok(V2SqliteShell::create(&file.0, v2, genesis(v2, &initial)));
        let deposit = command(DEPOSIT, LANE_A, 2);
        ok(db.commit(
            key(1),
            publication(v2, &initial, &deposit, &context(OPERATOR, false)),
        ));
        drop(db);
        // The whole lineage opens it: its one segment is the last member.
        let mut db = current(ok(lineage.open(&file.0)));
        assert_eq!(ok(db.snapshot()).version(), 1);
        drop(db);
        assert!(matches!(
            V2SqliteShell::open(&file.0, v1),
            Err(Error::Identity)
        ));
    });
}

/// A lineage that repeats an identity, which generation refuses, still opens
/// every store: currency is decided by identity, and each segment takes its
/// earliest fitting version.
#[test]
fn a_repeated_identity_never_strands_a_store() {
    with_lineage(|catalogs, lineage| {
        let (v1, v2) = (&lineage.authorities()[0], &lineage.authorities()[1]);
        let file = StoreFile::new();
        drop(store_under(&file.0, v1, 3));
        // [A, A]: the never-upgraded store is current and keeps committing.
        let repeated = ok(Lineage::bind(&[catalogs[0], catalogs[0]], &[[1; 32]]));
        let mut db = current(ok(repeated.open(&file.0)));
        ok(db.audit());
        let paid = paid_state();
        let deposit = command(DEPOSIT, LANE_A, 1);
        let operator = context(OPERATOR, false);
        ok(db.commit(key(70), publication(v1, &paid, &deposit, &operator)));
        drop(db);
        // [A, B, A]: the same store is current too.
        let reverting = ok(Lineage::bind(
            &[catalogs[0], catalogs[1], catalogs[0]],
            &[[1; 32], [2; 32]],
        ));
        let mut db = current(ok(reverting.open(&file.0)));
        ok(db.audit());
        drop(db);
        // A store that ran A then B upgrades back to A, binding the receipt
        // of the adoption between B and the last version.
        let file = StoreFile::new();
        drop(store_under(&file.0, v1, 3));
        let (db, _) = ok(superseded(ok(lineage.open(&file.0))).upgrade());
        drop(db);
        let store = superseded(ok(reverting.open(&file.0)));
        assert_eq!(store.version(), 2);
        let (mut db, receipt) = ok(store.upgrade());
        assert_eq!(receipt.kind(), upgrade::Kind::ProgramSuccessor);
        assert_eq!(receipt.receipts(), [Hash32::new([2; 32])]);
        assert_eq!(receipt.from_identity(), v2.identity());
        assert_eq!(receipt.identity(), v1.identity());
        ok(db.audit());
        assert_eq!(ok(db.snapshot()).upgrades(), 2);
        drop(db);
        current(ok(reverting.open(&file.0)));
    });
}

/// Versions 1 and 2, each with law 777, which reads the Step usage, bound as
/// a lineage: a program succession without every premise.
fn with_reading_lineage(test: impl FnOnce(&Lineage<'_, '_>)) {
    let old_contract = adopted::v1::Contract::new();
    let old_base = old_contract.descriptor();
    let old_laws = with_usage_law(old_base.laws);
    let mut old_required = old_base.required.to_vec();
    old_required.push(777);
    let old_reading = c::Descriptor {
        laws: &old_laws,
        required: &old_required,
        ..old_contract.descriptor()
    };
    let contract = adopted::Contract::new();
    let base = contract.descriptor();
    let new_laws = with_usage_law(base.laws);
    let mut new_required = base.required.to_vec();
    new_required.push(777);
    let new_reading = c::Descriptor {
        laws: &new_laws,
        required: &new_required,
        ..contract.descriptor()
    };
    let (old_policy, new_policy) = (variant_policy(&old_reading), variant_policy(&new_reading));
    let (old_catalog, new_catalog) = (
        variant_catalog(&old_reading, &old_policy),
        variant_catalog(&new_reading, &new_policy),
    );
    test(&ok(Lineage::bind(
        &[&old_catalog, &new_catalog],
        &[[5; 32]],
    )));
}

#[test]
fn an_upgrade_records_a_premise_that_failed() {
    with_reading_lineage(|lineage| {
        let old = &lineage.authorities()[0];
        let file = StoreFile::new();
        drop(store_under(&file.0, old, 3));
        let (mut db, receipt) = ok(superseded(ok(lineage.open(&file.0))).upgrade());
        // The laws are still identical, so the upgrade is a program
        // successor; the record states that a law reads the Step usage.
        assert_eq!(receipt.kind(), upgrade::Kind::ProgramSuccessor);
        let premises = receipt
            .premises()
            .unwrap_or_else(|| panic!("a program successor's premises"));
        assert!(premises.step_limits_never_bind);
        assert!(!premises.step_usage_unobserved);
        assert!(!premises.all());
        ok(db.audit());
        drop(db);
        current(ok(lineage.open(&file.0)));
    });
}

/// With every premise, a program successor commits the same decisions from
/// the same states: a store kept at version 1 and a store upgraded to version
/// 2 away from genesis take each further command alike, commit or reject.
#[test]
fn an_upgraded_store_takes_every_further_command_as_version_one_does() {
    with_lineage(|_, lineage| {
        let (v1, v2) = (&lineage.authorities()[0], &lineage.authorities()[1]);
        let kept = StoreFile::new();
        let upgraded = StoreFile::new();
        let mut old = store_under(&kept.0, v1, 3);
        drop(store_under(&upgraded.0, v1, 3));
        let (mut new, receipt) = ok(superseded(ok(lineage.open(&upgraded.0))).upgrade());
        assert_eq!(receipt.premises(), Some(HELD));
        let commands = [
            (command(DEPOSIT, LANE_A, 2), context(OPERATOR, false)),
            (command(REQUEST, LANE_B, 2), context(OWNER_B, false)),
            // Lane A is empty, but the caller is lane B's owner.
            (command(REQUEST, LANE_A, 1), context(OWNER_B, false)),
            (command(TICK, LANE_A, 1), context(KEEPER, false)),
            // Nothing is left to pay out.
            (command(REQUEST, LANE_A, 2), context(OWNER_A, false)),
            (command(DEPOSIT, LANE_A, 2), context(OPERATOR, false)),
            (command(DEPOSIT, LANE_A, 2), context(OPERATOR, false)),
            // Above the vault's capacity of 4.
            (command(DEPOSIT, LANE_A, 1), context(OPERATOR, false)),
            // An amount outside the command's declared domain.
            (command(DEPOSIT, LANE_A, 9), context(OPERATOR, false)),
            (command(TICK, LANE_A, 1), context(KEEPER, true)),
        ];
        let (mut committed, mut rejected, mut refused) = (0, 0, 0);
        for (step, (command, context)) in commands.iter().enumerate() {
            let state = ok(old.snapshot()).state().to_vec();
            assert_eq!(ok(new.snapshot()).state(), state, "step {step}");
            let raw = Raw {
                state: &state,
                command,
                context,
            };
            match (v1.publish(raw), v2.publish(raw)) {
                (PublicationOutcome::Commit(before), PublicationOutcome::Commit(after)) => {
                    assert_eq!(before.poststate(), after.poststate(), "step {step}");
                    ok(old.commit(key(100 + step as u8), before));
                    ok(new.commit(key(100 + step as u8), after));
                    committed += 1;
                }
                (PublicationOutcome::Reject(_), PublicationOutcome::Reject(_)) => rejected += 1,
                (
                    PublicationOutcome::Refused { error: before, .. },
                    PublicationOutcome::Refused { error: after, .. },
                ) if before == after => refused += 1,
                (before, after) => panic!("step {step}: version 1 {before:?}, version 2 {after:?}"),
            }
        }
        assert_eq!((committed, rejected, refused), (6, 3, 1));
        let (old_head, new_head) = (ok(old.snapshot()), ok(new.snapshot()));
        assert_eq!(old_head.state(), new_head.state());
        assert_eq!(old_head.version(), new_head.version());
        assert_eq!(old_head.pending(), new_head.pending());
    });
}
