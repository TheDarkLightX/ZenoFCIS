//! Data-migrating upgrades of spend-approval stores. Version 1 is the
//! spend-approval contract and version 2 the spend-approval-priority one,
//! whose state adds the urgent flag, both from the committed migrated
//! application fixture; its `STATE_STEPS` declares the migration
//! `zeno-fcis contract evolve --migration` recorded between them.
use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};
use zeno_fcis_codec::{CanonicalEncode, CommitmentHasher, Envelope, Hash32, commitment, domains};
use zeno_fcis_crypto::RustCryptoSha256;
use zeno_fcis_shell::{CommitStatus, MemoryDestination};
use zeno_fcis_shell_sqlite::v2::{
    Error, Lineage, Opened, Step, Store, Superseded, V2SqliteShell,
    migration::{self, Migration, Observation, Source, Target, Unsimulated},
    upgrade,
};
use zeno_fcis_synthesis::finite::{
    v2_authority::{Authority, Publication, PublicationOutcome},
    v2_catalog::BoundCatalog,
    v2_composition::{self as c, Raw},
};
use zeno_fcis_value::{Field, Value};

/// The migrated spend-approval application: `migrated::v1` is version 1.
#[allow(dead_code, clippy::type_complexity)]
#[path = "../../zeno-fcis-cli/tests/fixtures/spend-approval-migrated/src/v2_contract.rs"]
pub mod migrated;

#[track_caller]
fn ok<T, E: std::fmt::Debug>(result: Result<T, E>) -> T {
    match result {
        Ok(value) => value,
        Err(error) => panic!("unexpected refusal: {error:?}"),
    }
}

#[track_caller]
fn refused<T>(result: Result<T, Error>) -> Error {
    match result {
        Ok(_) => panic!("expected a refusal"),
        Err(error) => error,
    }
}

/// Runs `test` with both versions' checked catalogs.
fn with_versions(test: impl FnOnce(&BoundCatalog<'_>, &BoundCatalog<'_>)) {
    let contract_1 = migrated::v1::Contract::new();
    let descriptor_1 = contract_1.descriptor();
    let v1 = ok(migrated::v1::checked_catalog(&descriptor_1));
    let contract_2 = migrated::Contract::new();
    let descriptor_2 = contract_2.descriptor();
    let v2 = ok(migrated::checked_catalog(&descriptor_2));
    test(&v1, &v2);
}

/// The migration the application declares, as the shell takes it.
fn declared() -> Vec<Target<'static>> {
    let Some((1, Some(fields))) = migrated::STATE_STEPS.first().copied() else {
        panic!("the fixture declares one migration after version 1");
    };
    fields
        .iter()
        .map(|(id, tag, operand, table)| Target {
            id: *id,
            source: match tag {
                0 => Source::Field(u16::try_from(*operand).unwrap_or_else(|_| panic!("field"))),
                1 => Source::Value(*operand),
                _ => Source::Table {
                    field: u16::try_from(*operand).unwrap_or_else(|_| panic!("field")),
                    cases: table,
                },
            },
        })
        .collect()
}

/// The declared migration with the urgent flag's source replaced.
fn with_urgent(source: Source<'static>) -> Vec<Target<'static>> {
    declared()
        .into_iter()
        .map(|target| {
            if target.id == 124 {
                Target { id: 124, source }
            } else {
                target
            }
        })
        .collect()
}

fn envelope(binding: c::FrameBinding, value: Value) -> Vec<u8> {
    ok(Envelope::new(binding.root, Hash32::new(binding.schema), value).canonical_bytes())
}

const EMPTY: u16 = 150;
const PENDING: u16 = 151;
const EXECUTED: u16 = 152;
const CLERK: u16 = 161;
const CFO: u16 = 162;
const CREATE: u16 = 170;
const APPROVE: u16 = 171;
const EXECUTE: u16 = 172;

/// A request under version 1, or under version 2 with the urgent flag.
fn request(
    framing: &c::Framing,
    status: u16,
    tier: i128,
    cfo: bool,
    urgent: Option<bool>,
) -> Vec<u8> {
    let mut fields = vec![
        Field::new(120, Value::sum(106, status, None)),
        Field::new(121, Value::signed(tier)),
        Field::new(122, Value::boolean(cfo)),
        Field::new(123, Value::boolean(false)),
    ];
    if let Some(urgent) = urgent {
        fields.push(Field::new(124, Value::boolean(urgent)));
    }
    envelope(framing.state, ok(Value::record_canonical(fields)))
}
fn command(framing: &c::Framing, action: u16, tier: i128) -> Vec<u8> {
    let value = ok(Value::record_canonical(vec![
        Field::new(130, Value::sum(109, action, None)),
        Field::new(131, Value::signed(tier)),
    ]));
    envelope(framing.command, value)
}
fn context(framing: &c::Framing, role: u16) -> Vec<u8> {
    let value = ok(Value::record_canonical(vec![
        Field::new(140, Value::sum(107, role, None)),
        Field::new(141, Value::boolean(false)),
        Field::new(142, Value::boolean(true)),
    ]));
    envelope(framing.context, value)
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
fn root(state: &[u8]) -> Hash32 {
    ok(commitment::<RustCryptoSha256>(domains::V2_STATE, state))
}

static FILE_COUNTER: AtomicU64 = AtomicU64::new(0);
struct StoreFile(PathBuf);
impl StoreFile {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "zenofcis-v2-migration-{}-{}.db",
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

/// The first `steps` commits under version 1: the clerk creates a tier 1
/// request, the CFO approves it and the clerk executes it, which leaves the
/// payment pending.
fn store_under(path: &Path, v1: &Authority<'_>, steps: usize) {
    let framing = &migrated::v1::FRAMING;
    let initial = request(framing, EMPTY, 0, false, None);
    let genesis = match v1.publish_genesis(&initial) {
        PublicationOutcome::Commit(p) => p,
        refused => panic!("genesis refused: {refused:?}"),
    };
    let mut db = ok(V2SqliteShell::create(path, v1, genesis));
    let session = [
        (
            request(framing, EMPTY, 0, false, None),
            command(framing, CREATE, 1),
            context(framing, CLERK),
        ),
        (
            request(framing, PENDING, 1, false, None),
            command(framing, APPROVE, 0),
            context(framing, CFO),
        ),
        (
            request(framing, PENDING, 1, true, None),
            command(framing, EXECUTE, 0),
            context(framing, CLERK),
        ),
    ];
    for (index, (state, cmd, ctx)) in session.iter().take(steps).enumerate() {
        let receipt = ok(db.commit(key(index as u8 + 1), publication(v1, state, cmd, ctx)));
        assert_eq!(receipt.status(), CommitStatus::Committed);
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
fn current<'a, 'p>(opened: Opened<'a, 'p>) -> V2SqliteShell<'a, 'p> {
    match opened {
        Opened::V10(Store::Current(shell)) => shell,
        Opened::V10(Store::Superseded(store)) => {
            panic!("the store runs version {}", store.version())
        }
        Opened::V9(_) => panic!("a v9 store"),
    }
}

#[test]
fn a_declared_migration_moves_a_live_store_and_keeps_its_pending_deliveries() {
    with_versions(|v1, v2| {
        let fields = declared();
        let steps = [Step::Migration {
            migration: Migration { fields: &fields },
            claims: &[],
            review: b"no auxiliary claims",
        }];
        let lineage = ok(Lineage::bind_steps(&[v1, v2], &steps));
        let file = StoreFile::new();
        store_under(&file.0, &lineage.authorities()[0], 3);
        let (pending, checkpoint) = {
            let mut shell = ok(V2SqliteShell::open(&file.0, &lineage.authorities()[0]));
            let checkpoint = ok(shell.checkpoint());
            let token = ok(shell.next_pending()).unwrap_or_else(|| panic!("a pending payment"));
            (token.delivery().delivery_id(), checkpoint)
        };
        // The new build audits the old store without writing to it.
        let before = ok(fs::read(&file.0));
        let (version, head) = ok(lineage.audit_read_only(&file.0));
        assert_eq!((version, head.version(), head.pending()), (1, 3, 1));
        assert_eq!(ok(fs::read(&file.0)), before);
        let original_history = ok(lineage.audit_history_read_only(&file.0));
        assert_eq!(original_history.genesis_contract_version, 1);
        assert_eq!(original_history.commits.len(), 3);
        assert_eq!(
            original_history.commits[0].prestate,
            original_history.genesis
        );
        assert!(
            original_history
                .commits
                .iter()
                .all(|commit| commit.contract_version == 1)
        );
        assert_eq!(ok(fs::read(&file.0)), before);

        let store = superseded(ok(lineage.open(&file.0)));
        let (mut shell, receipt) = ok(store.upgrade());
        assert_eq!(receipt.kind(), upgrade::Kind::Migration);
        assert_eq!(receipt.kind().tag(), "migration");
        assert_eq!((receipt.ordinal(), receipt.version()), (1, 3));
        assert!(receipt.premises().is_none() && receipt.behaviour().is_none());
        let migrated_state = request(&migrated::FRAMING, EXECUTED, 1, true, Some(false));
        let migrated = receipt
            .migration()
            .unwrap_or_else(|| panic!("a migration record"));
        let simulated = migrated.simulated();
        assert_eq!(
            (
                simulated.states(),
                simulated.admitted(),
                simulated.genesis(),
                simulated.tuples()
            ),
            (80, 57, 1, 14_592)
        );
        assert_eq!(simulated.observations(), 0b10_1111);
        assert_eq!(migrated.root(), root(&migrated_state));
        let encoding = ok(Migration { fields: &fields }.encode());
        assert_eq!(
            migrated.migration(),
            <RustCryptoSha256 as CommitmentHasher>::hash(&encoding)
        );
        let snapshot = ok(shell.snapshot());
        assert_eq!(snapshot.state(), migrated_state.as_slice());
        assert_eq!((snapshot.version(), snapshot.upgrades()), (3, 1));
        // The payment pending before the upgrade keeps its ID.
        let mut destination = MemoryDestination::default();
        let token = ok(shell.next_pending()).unwrap_or_else(|| panic!("the pending payment"));
        assert_eq!(token.delivery().delivery_id(), pending);
        ok(ok(token.deliver(&mut destination)).acknowledge());
        assert!(ok(shell.next_pending()).is_none());
        drop(shell);
        let bytes = ok(fs::read(&file.0));
        let history = ok(lineage.audit_history_read_only(&file.0));
        assert_eq!(history.contract_version, 2);
        assert_eq!(history.snapshot.state(), migrated_state);
        assert_eq!(history.genesis, original_history.genesis);
        assert_eq!(history.commits, original_history.commits);
        assert_ne!(
            history
                .commits
                .last()
                .map(|commit| commit.poststate.as_slice()),
            Some(history.snapshot.state())
        );
        assert_eq!(ok(fs::read(&file.0)), bytes);
        // Both segments replay, the simulation re-derived from the
        // lineage's memo rather than run again.
        let mut reopened = current(ok(lineage.open(&file.0)));
        let moved = ok(reopened.audit());
        assert_eq!(lineage.simulations(), 1);
        drop(reopened);
        // A checkpoint saved after the migration opens at the moved state;
        // the one saved before it at the same head was replaced.
        assert!(matches!(
            ok(lineage.open_at_checkpoint(&file.0, &moved)),
            Store::Current(_)
        ));
        assert!(matches!(
            refused(lineage.open_at_checkpoint(&file.0, &checkpoint).map(|_| ())),
            Error::Checkpoint
        ));
        // The version 1 Authority alone no longer opens the store.
        assert!(matches!(
            refused(V2SqliteShell::open(&file.0, &lineage.authorities()[0])),
            Error::Identity
        ));
        // A fresh lineage value audits it too, simulating once.
        let fresh = ok(Lineage::bind_steps(&[v1, v2], &steps));
        let (version, _) = ok(fresh.audit_read_only(&file.0));
        assert_eq!((version, fresh.simulations()), (2, 1));
    });
}

#[test]
fn a_migrated_store_keeps_committing_under_the_new_layout() {
    with_versions(|v1, v2| {
        let fields = declared();
        let steps = [Step::Migration {
            migration: Migration { fields: &fields },
            claims: &[],
            review: b"no auxiliary claims",
        }];
        let lineage = ok(Lineage::bind_steps(&[v1, v2], &steps));
        let file = StoreFile::new();
        store_under(&file.0, &lineage.authorities()[0], 1);
        let (mut shell, receipt) = ok(superseded(ok(lineage.open(&file.0))).upgrade());
        assert_eq!(
            (receipt.kind(), receipt.version()),
            (upgrade::Kind::Migration, 1)
        );
        let framing = &migrated::FRAMING;
        let v2a = &lineage.authorities()[1];
        let session = [
            (
                request(framing, PENDING, 1, false, Some(false)),
                command(framing, APPROVE, 0),
                context(framing, CFO),
            ),
            (
                request(framing, PENDING, 1, true, Some(false)),
                command(framing, EXECUTE, 0),
                context(framing, CLERK),
            ),
        ];
        for (index, (state, cmd, ctx)) in session.iter().enumerate() {
            let committed =
                ok(shell.commit(key(10 + index as u8), publication(v2a, state, cmd, ctx)));
            assert_eq!(committed.status(), CommitStatus::Committed);
        }
        let mut destination = MemoryDestination::default();
        let token = ok(shell.next_pending()).unwrap_or_else(|| panic!("the new payment"));
        ok(ok(token.deliver(&mut destination)).acknowledge());
        drop(shell);
        let mut reopened = current(ok(lineage.open(&file.0)));
        let head = ok(reopened.snapshot());
        assert_eq!((head.version(), head.pending(), head.upgrades()), (3, 0, 1));
        assert_eq!(
            head.state(),
            request(framing, EXECUTED, 1, true, Some(false)).as_slice()
        );
    });
}

#[test]
fn a_migration_the_simulation_or_the_lineage_does_not_admit_writes_nothing() {
    with_versions(|v1, v2| {
        let file = StoreFile::new();
        {
            let fields = declared();
            let steps = [Step::Migration {
                migration: Migration { fields: &fields },
                claims: &[],
                review: b"no auxiliary claims",
            }];
            let lineage = ok(Lineage::bind_steps(&[v1, v2], &steps));
            store_under(&file.0, &lineage.authorities()[0], 2);
        }
        let before = ok(fs::read(&file.0));
        let attempt = |step: Step<'_>| {
            let lineage = ok(Lineage::bind_steps(&[v1, v2], &[step]));
            let error = refused(superseded(ok(lineage.open(&file.0))).upgrade());
            assert_eq!(ok(fs::read(&file.0)), before, "{error:?}");
            error
        };
        // The urgent flag set: the old genesis state does not map.
        let urgent = with_urgent(Source::Value(1));
        let error = attempt(Step::Migration {
            migration: Migration { fields: &urgent },
            claims: &[],
            review: b"no auxiliary claims",
        });
        assert!(
            matches!(
                error,
                Error::Upgrade(upgrade::Refusal::Migration(Unsimulated::Differs {
                    observation: Observation::Genesis,
                    ordinal: 0
                }))
            ),
            "{error:?}"
        );
        assert!(
            error
                .to_string()
                .contains("genesis state at input tuple 0 maps to a state"),
            "{error}"
        );
        // A new field without a value.
        let missing: Vec<Target<'static>> = declared()
            .into_iter()
            .filter(|target| target.id != 124)
            .collect();
        assert!(matches!(
            attempt(Step::Migration {
                migration: Migration { fields: &missing },
                claims: &[],
                review: b"no auxiliary claims"
            }),
            Error::Upgrade(upgrade::Refusal::Migration(Unsimulated::Migration(
                migration::Defect::Missing { field: 124 }
            )))
        ));
        // A value outside the new field's domain.
        let wide = with_urgent(Source::Value(2));
        assert!(matches!(
            attempt(Step::Migration {
                migration: Migration { fields: &wide },
                claims: &[],
                review: b"no auxiliary claims"
            }),
            Error::Upgrade(upgrade::Refusal::Migration(Unsimulated::Value {
                field: 124,
                ..
            }))
        ));
        // Declared as a rename, which it is not; or as an adoption, which
        // keeps the state schema.
        assert!(matches!(
            attempt(Step::Rename),
            Error::Upgrade(upgrade::Refusal::Rename)
        ));
        assert!(matches!(
            attempt(Step::Adoption([7; 32])),
            Error::Upgrade(upgrade::Refusal::StateSchema)
        ));
    });
}

#[test]
fn an_audit_re_derives_the_migration_and_refuses_a_forged_or_undeclared_one() {
    with_versions(|v1, v2| {
        let fields = declared();
        let steps = [Step::Migration {
            migration: Migration { fields: &fields },
            claims: &[],
            review: b"no auxiliary claims",
        }];
        let lineage = ok(Lineage::bind_steps(&[v1, v2], &steps));
        let file = StoreFile::new();
        store_under(&file.0, &lineage.authorities()[0], 2);
        drop(ok(superseded(ok(lineage.open(&file.0))).upgrade()));
        let intact = ok(fs::read(&file.0));
        let open_with = |step: Step<'_>| {
            let other = ok(Lineage::bind_steps(&[v1, v2], &[step]));
            let error = refused(other.open(&file.0).map(|_| ()));
            assert_eq!(ok(fs::read(&file.0)), intact, "{error:?}");
            error
        };
        // An equivalent migration with another encoding: the urgent flag
        // from a table over the CEO's approval, false either way.
        let cases: &'static [(i128, i128)] = &[(0, 0), (1, 0)];
        let table = with_urgent(Source::Table { field: 123, cases });
        assert!(matches!(
            open_with(Step::Migration {
                migration: Migration { fields: &table },
                claims: &[],
                review: b"no auxiliary claims"
            }),
            Error::Succession(upgrade::Unsupported::Migration)
        ));
        assert!(matches!(
            open_with(Step::Rename),
            Error::Succession(upgrade::Unsupported::Migration)
        ));
        assert!(matches!(
            open_with(Step::Adoption([7; 32])),
            Error::Succession(upgrade::Unsupported::Migration)
        ));
        // A stored migrated state that is not the migration's: damage.
        let forged = StoreFile::new();
        ok(fs::copy(&file.0, &forged.0));
        {
            let connection = ok(rusqlite::Connection::open(&forged.0));
            let mut publication: Vec<u8> = ok(connection.query_row(
                "SELECT publication FROM v2_upgrades WHERE ordinal=1",
                [],
                |r| r.get(0),
            ));
            let last = publication.len() - 2;
            publication[last] ^= 1;
            ok(connection.execute(
                "UPDATE v2_upgrades SET publication=?1 WHERE ordinal=1",
                [publication],
            ));
        }
        assert!(matches!(
            refused(lineage.open(&forged.0).map(|_| ())),
            Error::History
        ));
        // The intact store still opens under its own lineage.
        drop(current(ok(lineage.open(&file.0))));
    });
}
