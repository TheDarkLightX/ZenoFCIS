//! Checked contract upgrades of a live withdrawal-queue store. Version 1 is
//! the template's contract and version 2 the adopted 100-node candidate, both
//! taken from the committed adopted fixture, whose `with_lineage` binds them.
use rusqlite::Connection;
use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};
use zeno_fcis_codec::{CanonicalEncode, Envelope, Hash32};
use zeno_fcis_shell::{CommitStatus, MemoryDestination};
use zeno_fcis_shell_sqlite::v2::{Error, V2SqliteShell, upgrade};
use zeno_fcis_synthesis::finite::{
    v2_authority::{self as authority, Authority, Publication, PublicationOutcome},
    v2_catalog::BoundCatalog,
    v2_composition::{self as c, Raw},
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

/// Runs `test` with the lineage's catalogs and bound Authorities: version 1
/// first, version 2 last.
fn with_lineage(test: impl FnOnce(&[&BoundCatalog<'_>], &[&Authority<'_>])) {
    ok(adopted::with_lineage(|catalogs| {
        let authorities: Vec<Authority<'_>> = catalogs
            .iter()
            .map(|catalog| ok(authority::bind(catalog)))
            .collect();
        let members: Vec<&Authority<'_>> = authorities.iter().collect();
        test(catalogs, &members);
    }));
}

fn with_counter(test: impl FnOnce(&BoundCatalog<'_>, &Authority<'_>)) {
    let contract = counter::Contract::new();
    let descriptor = contract.descriptor();
    let catalog = ok(counter::checked_catalog(&descriptor));
    let authority = ok(authority::bind(&catalog));
    test(&catalog, &authority);
}

fn envelope(binding: c::FrameBinding, value: Value) -> Vec<u8> {
    ok(Envelope::new(binding.root, Hash32::new(binding.schema), value).canonical_bytes())
}

const EMPTY: u16 = 180;
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

/// Every row of every table, read through an independent connection.
fn stored_rows(path: &Path) -> Vec<(&'static str, Vec<rusqlite::types::Value>)> {
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

/// A live store under version 1: two deposits, two requests and two paying
/// ticks, back at the genesis state with both payouts pending. Returns the
/// pending deliveries in commit order.
fn live_store<'a, 'p>(
    path: &Path,
    v1: &'a Authority<'p>,
) -> (
    V2SqliteShell<'a, 'p>,
    Vec<zeno_fcis_shell_sqlite::v2::Pending>,
) {
    let initial = genesis_state();
    let mut db = ok(V2SqliteShell::create(path, v1, genesis(v1, &initial)));
    let steps: [(Vec<u8>, Vec<u8>, Vec<u8>); 6] = [
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
            vault(2, 181, 2, EMPTY, 0, 0, false, LANE_A),
            command(TICK, LANE_A, 1),
            context(KEEPER, false),
        ),
        (
            vault(0, EMPTY, 0, EMPTY, 0, 0, false, LANE_B),
            command(DEPOSIT, LANE_A, 2),
            context(OPERATOR, false),
        ),
        (
            vault(2, EMPTY, 0, EMPTY, 0, 0, false, LANE_B),
            command(REQUEST, LANE_B, 2),
            context(OWNER_B, false),
        ),
        (
            vault(2, EMPTY, 0, 181, 2, 0, false, LANE_B),
            command(TICK, LANE_A, 1),
            context(KEEPER, false),
        ),
    ];
    for (index, (state, cmd, ctx)) in steps.iter().enumerate() {
        assert_eq!(ok(db.snapshot()).state(), state.as_slice(), "step {index}");
        let receipt = ok(db.commit(key(index as u8 + 1), publication(v1, state, cmd, ctx)));
        assert_eq!(receipt.status(), CommitStatus::Committed);
    }
    let snapshot = ok(db.snapshot());
    assert_eq!(snapshot.version(), 6);
    assert_eq!(snapshot.state(), genesis_state());
    assert_eq!(snapshot.pending(), 2);
    assert_eq!(snapshot.upgrades(), 0);
    let first = ok(db.next_pending()).unwrap_or_else(|| panic!("first payout pending"));
    assert_eq!(first.version(), 3);
    // The second payout is behind the first; read it through its own commit.
    let external = ok(Connection::open(path));
    let pending: Vec<(i64, Vec<u8>)> = {
        let mut statement = ok(external.prepare(
            "SELECT sequence,delivery_id FROM v2_deliveries WHERE acknowledged=0 ORDER BY sequence",
        ));
        let rows = ok(statement.query_map([], |r| Ok((r.get(0)?, r.get(1)?))));
        rows.map(|row| ok(row)).collect()
    };
    assert_eq!(pending.len(), 2);
    assert_eq!(pending[0].1, first.delivery_id().as_bytes());
    assert_eq!((pending[0].0, pending[1].0), (3, 6));
    (db, vec![first])
}

#[test]
fn a_live_store_upgrades_keeps_committing_and_replays_both_segments() {
    with_lineage(|catalogs, members| {
        let (v1, v2) = (members[0], members[1]);
        assert_ne!(v1.identity(), v2.identity());
        assert_eq!(
            catalogs[0].original_schema(),
            catalogs[1].original_schema(),
            "the adoption changed the program, not the schema"
        );
        let file = StoreFile::new();
        let (mut db, pending) = live_store(&file.0, v1);
        let head = ok(db.checkpoint());
        drop(db);
        let before = stored_rows(&file.0);
        // A v2 publication cannot enter the store before the upgrade.
        let mut stale = ok(V2SqliteShell::open(&file.0, v1));
        let deposit = command(DEPOSIT, LANE_A, 1);
        let operator = context(OPERATOR, false);
        let initial = genesis_state();
        assert!(matches!(
            stale.commit(key(40), publication(v2, &initial, &deposit, &operator)),
            Err(Error::Identity)
        ));
        drop(stale);

        let receipt = ok(V2SqliteShell::upgrade(&file.0, catalogs));
        assert_eq!(receipt.ordinal(), 1);
        assert_eq!(receipt.version(), 6);
        assert_eq!(receipt.from_identity(), v1.identity());
        assert_eq!(receipt.identity(), v2.identity());
        assert!(receipt.record().starts_with(b"ZFCISV2-UPGRADE\0"));
        // Exactly the upgrade row is added and the head's chain tip moves to it.
        let after = stored_rows(&file.0);
        assert_eq!(after.len(), before.len() + 1);
        let state = |rows: &[(&'static str, Vec<rusqlite::types::Value>)]| {
            rows.iter()
                .find(|(table, _)| *table == "v2_state")
                .map(|(_, row)| row.clone())
                .unwrap_or_default()
        };
        let (old_state, new_state) = (state(&before), state(&after));
        assert_eq!(old_state[..4], new_state[..4], "sequence, state and root");
        assert_eq!(
            new_state[4],
            rusqlite::types::Value::Blob(receipt.chain().as_bytes().to_vec())
        );
        assert_ne!(old_state[4], new_state[4]);
        let others = |rows: &[(&'static str, Vec<rusqlite::types::Value>)]| {
            rows.iter()
                .filter(|(table, _)| *table != "v2_state" && *table != "v2_upgrades")
                .cloned()
                .collect::<Vec<(&'static str, Vec<rusqlite::types::Value>)>>()
        };
        assert_eq!(others(&before), others(&after));
        assert_eq!(
            after
                .iter()
                .filter(|(table, _)| *table == "v2_upgrades")
                .count(),
            1
        );
        assert_eq!(user_version(&file.0), 10);

        // The old lineage alone, or the new contract alone, cannot open it.
        assert!(matches!(
            V2SqliteShell::open(&file.0, v1),
            Err(Error::Identity)
        ));
        assert!(matches!(
            V2SqliteShell::open(&file.0, v2),
            Err(Error::Identity)
        ));
        assert!(matches!(
            V2SqliteShell::open_lineage(&file.0, &[v2, v1]),
            Err(Error::Identity)
        ));
        let mut db = ok(V2SqliteShell::open_lineage(&file.0, members));
        assert_eq!(db.authority().identity(), v2.identity());
        assert_eq!(db.lineage().len(), 2);
        let snapshot = ok(db.snapshot());
        assert_eq!(snapshot.version(), 6);
        assert_eq!(snapshot.upgrades(), 1);
        assert_eq!(snapshot.pending(), 2);
        assert_eq!(snapshot.binding(), v2.identity());
        assert_eq!(snapshot.chain(), receipt.chain());
        assert_eq!(ok(db.binding()), v2.identity());

        // Pending deliveries keep their certificate-bound IDs and commit order,
        // and are delivered exactly once.
        let first = ok(db.next_pending()).unwrap_or_else(|| panic!("payout pending"));
        assert_eq!(first, pending[0]);
        let mut destination = MemoryDestination::default();
        assert!(ok(db.deliver_next_memory(&mut destination)));
        let second = ok(db.next_pending()).unwrap_or_else(|| panic!("second payout pending"));
        assert_eq!(second.version(), 6);
        assert_ne!(second.delivery_id(), first.delivery_id());
        assert!(ok(db.deliver_next_memory(&mut destination)));
        assert!(!ok(db.deliver_next_memory(&mut destination)));
        assert_eq!(destination.delivered_count(), 2);
        assert_eq!(ok(db.snapshot()).pending(), 0);

        // The store keeps committing, under version 2 only.
        assert!(matches!(
            db.commit(key(41), publication(v1, &initial, &deposit, &operator)),
            Err(Error::Identity)
        ));
        let committed = ok(db.commit(key(41), publication(v2, &initial, &deposit, &operator)));
        assert_eq!(committed.status(), CommitStatus::Committed);
        assert_eq!(committed.version(), 7);
        // An old replay key with a new publication is not a replay.
        assert!(matches!(
            db.commit(key(1), publication(v2, &initial, &deposit, &operator)),
            Err(Error::Replay)
        ));
        let after_deposit = vault(1, EMPTY, 0, EMPTY, 0, 0, false, LANE_A);
        assert_eq!(ok(db.snapshot()).state(), after_deposit);
        let replayed = ok(db.commit(key(41), publication(v2, &initial, &deposit, &operator)));
        assert_eq!(replayed.status(), CommitStatus::IdempotentReplay);
        assert_eq!(replayed.version(), 7);

        // Both segments replay: a full audit, a fresh open, and a checkpoint
        // taken before the upgrade resume the tail across it.
        let checkpoint = ok(db.audit());
        drop(db);
        let mut reopened = ok(V2SqliteShell::open_lineage(&file.0, members));
        assert_eq!(ok(reopened.snapshot()).version(), 7);
        assert_eq!(ok(reopened.snapshot()).upgrades(), 1);
        drop(reopened);
        let mut resumed = ok(V2SqliteShell::open_lineage_at_checkpoint(
            &file.0, members, &head,
        ));
        assert_eq!(ok(resumed.snapshot()).version(), 7);
        drop(resumed);
        let mut resumed = ok(V2SqliteShell::open_lineage_at_checkpoint(
            &file.0,
            members,
            &checkpoint,
        ));
        assert_eq!(ok(resumed.snapshot()).version(), 7);
        assert!(matches!(
            V2SqliteShell::open_at_checkpoint(&file.0, v2, &checkpoint),
            Err(Error::Identity)
        ));
        // Upgrading again to the same contract is refused and writes nothing.
        let rows = stored_rows(&file.0);
        assert!(matches!(
            V2SqliteShell::upgrade(&file.0, catalogs),
            Err(Error::Upgrade(upgrade::Refusal::SameContract))
        ));
        assert_eq!(stored_rows(&file.0), rows);
    });
}

#[test]
fn refusals_write_nothing() {
    with_lineage(|catalogs, members| {
        let (v1, v2) = (members[0], members[1]);
        // A different state schema.
        with_counter(|counter_catalog, counter| {
            let file = StoreFile::new();
            let (db, _) = live_store(&file.0, v1);
            drop(db);
            let rows = stored_rows(&file.0);
            let bytes = ok(fs::read(&file.0));
            assert!(matches!(
                V2SqliteShell::upgrade(&file.0, &[catalogs[0], counter_catalog]),
                Err(Error::Upgrade(upgrade::Refusal::StateSchema))
            ));
            assert_eq!(stored_rows(&file.0), rows);
            assert_eq!(ok(fs::read(&file.0)), bytes);
            assert!(matches!(
                V2SqliteShell::open_lineage(&file.0, &[v1, counter]),
                Err(Error::Identity)
            ));
        });
        // The new contract's genesis laws do not hold on a state away from genesis.
        {
            let file = StoreFile::new();
            let initial = genesis_state();
            let mut db = ok(V2SqliteShell::create(&file.0, v1, genesis(v1, &initial)));
            let deposit = command(DEPOSIT, LANE_A, 2);
            let operator = context(OPERATOR, false);
            ok(db.commit(key(1), publication(v1, &initial, &deposit, &operator)));
            drop(db);
            let rows = stored_rows(&file.0);
            let bytes = ok(fs::read(&file.0));
            assert!(matches!(
                V2SqliteShell::upgrade(&file.0, catalogs),
                Err(Error::Upgrade(upgrade::Refusal::Genesis))
            ));
            assert_eq!(stored_rows(&file.0), rows);
            assert_eq!(ok(fs::read(&file.0)), bytes);
            // The same store at genesis state upgrades.
            let mut db = ok(V2SqliteShell::open(&file.0, v1));
            let deposited = vault(2, EMPTY, 0, EMPTY, 0, 0, false, LANE_A);
            let request = command(REQUEST, LANE_A, 2);
            ok(db.commit(
                key(2),
                publication(v1, &deposited, &request, &context(OWNER_A, false)),
            ));
            let requested = vault(2, 181, 2, EMPTY, 0, 0, false, LANE_A);
            ok(db.commit(
                key(3),
                publication(
                    v1,
                    &requested,
                    &command(TICK, LANE_A, 1),
                    &context(KEEPER, false),
                ),
            ));
            // Priority moved to lane B: not the genesis state yet.
            assert_eq!(
                ok(db.snapshot()).state(),
                vault(0, EMPTY, 0, EMPTY, 0, 0, false, LANE_B)
            );
            drop(db);
            assert!(matches!(
                V2SqliteShell::upgrade(&file.0, catalogs),
                Err(Error::Upgrade(upgrade::Refusal::Genesis))
            ));
        }
        // A missing old contract: the lineage does not contain the store's contract.
        {
            let file = StoreFile::new();
            let (db, _) = live_store(&file.0, v1);
            drop(db);
            let rows = stored_rows(&file.0);
            let bytes = ok(fs::read(&file.0));
            assert!(matches!(
                V2SqliteShell::upgrade(&file.0, &[catalogs[1]]),
                Err(Error::Identity)
            ));
            // In a lineage that ends with the store's contract there is nothing
            // to upgrade to.
            assert!(matches!(
                V2SqliteShell::upgrade(&file.0, &[catalogs[1], catalogs[0]]),
                Err(Error::Upgrade(upgrade::Refusal::SameContract))
            ));
            assert!(matches!(
                V2SqliteShell::upgrade(&file.0, &[catalogs[0]]),
                Err(Error::Upgrade(upgrade::Refusal::SameContract))
            ));
            assert!(matches!(
                V2SqliteShell::upgrade(&file.0, &[]),
                Err(Error::Identity)
            ));
            assert_eq!(stored_rows(&file.0), rows);
            assert_eq!(ok(fs::read(&file.0)), bytes);
            // Once upgraded, a lineage without version 1 cannot replay the first segment.
            ok(V2SqliteShell::upgrade(&file.0, catalogs));
            assert!(matches!(
                V2SqliteShell::open_lineage(&file.0, &[v2]),
                Err(Error::Identity)
            ));
            assert!(matches!(
                V2SqliteShell::upgrade(&file.0, &[catalogs[1]]),
                Err(Error::Identity)
            ));
            let _ = v2;
        }
    });
}

#[test]
fn forged_or_altered_upgrade_records_are_refused() {
    with_lineage(|catalogs, members| {
        let (v1, v2) = (members[0], members[1]);
        let file = StoreFile::new();
        let (db, _) = live_store(&file.0, v1);
        drop(db);
        let unchanged = stored_rows(&file.0);
        // A forged record from outside the shell, with the right identity.
        let external = ok(Connection::open(&file.0));
        let (root, chain): (Vec<u8>, Vec<u8>) = ok(external.query_row(
            "SELECT root,chain FROM v2_state WHERE singleton=1",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        ));
        ok(external.execute(
            "INSERT INTO v2_upgrades VALUES(1,6,?1,x'00',?2,?3,x'00',zeroblob(32))",
            rusqlite::params![v2.identity(), root, chain],
        ));
        assert!(matches!(
            V2SqliteShell::open_lineage(&file.0, members),
            Err(Error::History)
        ));
        assert!(matches!(
            V2SqliteShell::open(&file.0, v1),
            Err(Error::Identity)
        ));
        assert!(matches!(
            V2SqliteShell::upgrade(&file.0, catalogs),
            Err(Error::History)
        ));
        ok(external.execute("DELETE FROM v2_upgrades", []));
        assert_eq!(stored_rows(&file.0), unchanged);
        drop(external);

        let receipt = ok(V2SqliteShell::upgrade(&file.0, catalogs));
        let mut db = ok(V2SqliteShell::open_lineage(&file.0, members));
        let initial = genesis_state();
        let deposit = command(DEPOSIT, LANE_A, 1);
        let operator = context(OPERATOR, false);
        ok(db.commit(key(50), publication(v2, &initial, &deposit, &operator)));
        let live = ok(db.snapshot());
        drop(db);
        let genuine = stored_rows(&file.0);
        // Each alteration names the refusal under the lineage and under
        // version 1 alone. A record's fields fail recomputation; an ordinal
        // out of order fails before any Authority is consulted; without its
        // upgrade row the store claims to run version 1, which the lineage
        // refuses as not current, while version 1 alone cannot replay the
        // commit made under version 2.
        let history = |refused: &Result<(), Error>| matches!(refused, Err(Error::History));
        let identity = |refused: &Result<(), Error>| matches!(refused, Err(Error::Identity));
        for (alteration, under_lineage, alone) in [
            ("UPDATE v2_upgrades SET record=x'00'", "History", "Identity"),
            (
                "UPDATE v2_upgrades SET chain=zeroblob(32)",
                "History",
                "Identity",
            ),
            ("UPDATE v2_upgrades SET sequence=5", "History", "Identity"),
            (
                "UPDATE v2_upgrades SET root=zeroblob(32)",
                "History",
                "Identity",
            ),
            (
                "UPDATE v2_upgrades SET previous_chain=zeroblob(32)",
                "History",
                "Identity",
            ),
            (
                "UPDATE v2_upgrades SET publication=x'00'",
                "History",
                "Identity",
            ),
            ("UPDATE v2_upgrades SET ordinal=2", "History", "History"),
            ("DELETE FROM v2_upgrades", "Identity", "History"),
        ] {
            let external = ok(Connection::open(&file.0));
            ok(external.execute(alteration, []));
            drop(external);
            let expected = |name: &str, refused: &Result<(), Error>| match name {
                "History" => history(refused),
                _ => identity(refused),
            };
            let refused = V2SqliteShell::open_lineage(&file.0, members).map(|_| ());
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
        assert!(matches!(
            V2SqliteShell::open_lineage(&file.0, members),
            Err(Error::Identity)
        ));
        let external = ok(Connection::open(&file.0));
        ok(external.execute("UPDATE v2_upgrades SET identity=?1", [v2.identity()]));
        drop(external);
        // Genuine again: a live handle also refuses an alteration made under it.
        let mut db = ok(V2SqliteShell::open_lineage(&file.0, members));
        assert_eq!(ok(db.snapshot()), live);
        assert_eq!(ok(db.snapshot()).upgrades(), 1);
        let external = ok(Connection::open(&file.0));
        ok(external.execute("UPDATE v2_upgrades SET record=x'00'", []));
        drop(external);
        assert!(matches!(db.snapshot(), Err(Error::History)));
        assert!(db.next_pending().is_err());
        let _ = receipt;
    });
}

#[test]
fn a_v9_store_needs_the_explicit_migration() {
    with_lineage(|catalogs, members| {
        let (v1, v2) = (members[0], members[1]);
        let file = StoreFile::new();
        let (db, _) = live_store(&file.0, v1);
        drop(db);
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
        assert!(matches!(
            V2SqliteShell::open_lineage(&file.0, members),
            Err(Error::Schema(9))
        ));
        assert!(matches!(
            V2SqliteShell::upgrade(&file.0, catalogs),
            Err(Error::Schema(9))
        ));
        // A lineage without the store's contract cannot migrate it.
        assert!(matches!(
            V2SqliteShell::migrate_v9(&file.0, &[v2]),
            Err(Error::Identity)
        ));
        assert_eq!(stored_rows(&file.0), rows);
        assert_eq!(ok(fs::read(&file.0)), bytes);
        assert_eq!(user_version(&file.0), 9);
        // A store that is not exactly v9 is not migrated either.
        let external = ok(Connection::open(&file.0));
        ok(external.execute_batch(
            "CREATE TRIGGER evil AFTER INSERT ON v2_commits BEGIN DELETE FROM v2_deliveries; END;",
        ));
        assert!(matches!(
            V2SqliteShell::migrate_v9(&file.0, members),
            Err(Error::Schema(9))
        ));
        ok(external.execute_batch("DROP TRIGGER evil;"));
        drop(external);
        assert_eq!(user_version(&file.0), 9);

        let mut migrated = ok(V2SqliteShell::migrate_v9(&file.0, members));
        assert_eq!(migrated.lineage().len(), 1);
        assert_eq!(migrated.authority().identity(), v1.identity());
        assert_eq!(user_version(&file.0), 10);
        let snapshot = ok(migrated.snapshot());
        assert_eq!(snapshot.version(), 6);
        assert_eq!(snapshot.pending(), 2);
        assert_eq!(snapshot.upgrades(), 0);
        drop(migrated);
        assert!(matches!(
            V2SqliteShell::migrate_v9(&file.0, members),
            Err(Error::Schema(10))
        ));
        assert_eq!(stored_rows(&file.0), rows);
        // Migrated stores are ordinary v10 stores: they open and upgrade.
        ok(V2SqliteShell::open(&file.0, v1));
        ok(V2SqliteShell::upgrade(&file.0, catalogs));
        let mut db = ok(V2SqliteShell::open_lineage(&file.0, members));
        assert_eq!(ok(db.snapshot()).upgrades(), 1);
        let mut destination = MemoryDestination::default();
        while ok(db.deliver_next_memory(&mut destination)) {}
        assert_eq!(destination.delivered_count(), 2);
        ok(db.audit());
    });
}

#[test]
fn a_live_handle_bound_to_the_old_version_stops_at_another_connections_upgrade() {
    with_lineage(|catalogs, members| {
        let (v1, v2) = (members[0], members[1]);
        let file = StoreFile::new();
        let (mut old, _) = live_store(&file.0, v1);
        let receipt = ok(V2SqliteShell::upgrade(&file.0, catalogs));
        // The cached tip is stale and the refreshed store runs another contract.
        assert!(matches!(old.snapshot(), Err(Error::Identity)));
        let initial = genesis_state();
        let deposit = command(DEPOSIT, LANE_A, 1);
        let operator = context(OPERATOR, false);
        assert!(matches!(
            old.commit(key(60), publication(v1, &initial, &deposit, &operator)),
            Err(Error::Identity)
        ));
        assert!(old.next_pending().is_err());
        assert!(old.checkpoint().is_err());
        drop(old);
        // Nothing of the refused calls reached the store.
        let mut db = ok(V2SqliteShell::open_lineage(&file.0, members));
        let snapshot = ok(db.snapshot());
        assert_eq!(snapshot.version(), 6);
        assert_eq!(snapshot.chain(), receipt.chain());
        assert_eq!(snapshot.pending(), 2);
        assert_eq!(snapshot.binding(), v2.identity());
    });
}

#[test]
fn a_store_created_under_version_two_skips_version_one() {
    with_lineage(|catalogs, members| {
        let (v1, v2) = (members[0], members[1]);
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
        let mut db = ok(V2SqliteShell::open_lineage(&file.0, members));
        assert_eq!(ok(db.snapshot()).version(), 1);
        drop(db);
        assert!(matches!(
            V2SqliteShell::open(&file.0, v1),
            Err(Error::Identity)
        ));
        assert!(matches!(
            V2SqliteShell::upgrade(&file.0, catalogs),
            Err(Error::Upgrade(upgrade::Refusal::SameContract))
        ));
    });
}
