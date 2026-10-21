//! Shell effects against the actual durable-counter V2 contract: publication
//! binding, the registry's V2 mapping, and the commit program's recovery
//! rule run against a real store.
use std::{
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};
use zeno_fcis_codec::{CanonicalEncode, Envelope, Hash32};
use zeno_fcis_shell::CommitStatus;
use zeno_fcis_shell::effects::{
    driver::{Driver, Reply},
    port::{self, CommitMode, Fault, Operation, Outcome, Response},
    program::{CommitInvocation, CommitPolicy, CommitProgram},
    registry::{REGISTRY, V2},
};
use zeno_fcis_shell_sqlite::effects::{self, Unbound};
use zeno_fcis_shell_sqlite::v2::{self as v2, Error, Lineage, V2SqliteShell};
use zeno_fcis_synthesis::finite::{
    canonical_v2::schema,
    v2_authority::{self as authority, Authority, Publication, PublicationOutcome},
    v2_catalog::{self as catalog, BoundCatalog},
    v2_composition::{self as c, Raw},
};
use zeno_fcis_value::{Field, Value};

/// Actual generated declarative contract; this is not an application evaluator.
#[path = "../../zeno-fcis-cli/templates/durable-counter/src/v2_contract.rs"]
pub mod durable_counter;

fn ok<T, E: std::fmt::Debug>(result: Result<T, E>) -> T {
    result.unwrap_or_else(|error| panic!("unexpected refusal: {error:?}"))
}

fn with_catalog(step: Option<u64>, run: impl FnOnce(&BoundCatalog<'_>)) {
    let contract = durable_counter::Contract::new();
    let mut descriptor = contract.descriptor();
    if let Some(steps) = step {
        descriptor.limits = descriptor
            .limits
            .with_limit(zeno_fcis_synthesis::finite::V2Resource::Step, steps);
    }
    let policy = authority::policy_bytes(
        &descriptor,
        durable_counter::ORIGINAL_SCHEMA,
        &durable_counter::FRAMING,
        durable_counter::CHANNEL_ROOTS,
    )
    .unwrap_or_else(|| panic!("policy encoding refused"));
    let catalog = ok(catalog::bind_original(
        durable_counter::ORIGINAL_SCHEMA,
        &durable_counter::DESCRIPTION,
        catalog::Limits {
            schema: schema::Limits {
                bytes: durable_counter::ORIGINAL_SCHEMA.len() as u64,
                types: 6,
                fields: 2,
                variants: 2,
            },
            contract_bytes: policy.len() as u64,
        },
        &policy,
        &descriptor,
        &durable_counter::FRAMING,
        durable_counter::CHANNEL_ROOTS,
    ));
    run(&catalog);
}

fn envelope(binding: c::FrameBinding, value: Value) -> Vec<u8> {
    ok(Envelope::new(binding.root, Hash32::new(binding.schema), value).canonical_bytes())
}
fn state(count: i128, failures: i128) -> Vec<u8> {
    let value = ok(Value::record_canonical(vec![
        Field::new(110, Value::signed(count)),
        Field::new(111, Value::signed(failures)),
    ]));
    envelope(durable_counter::FRAMING.state, value)
}
fn command(variant: u16) -> Vec<u8> {
    envelope(
        durable_counter::FRAMING.command,
        Value::sum(101, variant, None),
    )
}
fn context(allowed: bool) -> Vec<u8> {
    envelope(durable_counter::FRAMING.context, Value::boolean(allowed))
}
fn publication<'a>(a: &'a Authority<'_>, raw: Raw<'a>) -> Publication<'a> {
    match a.publish(raw) {
        PublicationOutcome::Commit(p) => p,
        refused => panic!("publication refused: {refused:?}"),
    }
}
fn genesis<'a>(a: &'a Authority<'_>, initial: &'a [u8]) -> Publication<'a> {
    match a.publish_genesis(initial) {
        PublicationOutcome::Commit(p) => p,
        refused => panic!("genesis refused: {refused:?}"),
    }
}

static FILE_COUNTER: AtomicU64 = AtomicU64::new(0);
struct StoreFile(PathBuf);
impl StoreFile {
    fn new() -> Self {
        Self(std::env::temp_dir().join(format!(
            "zenofcis-effects-{}-{}.db",
            std::process::id(),
            FILE_COUNTER.fetch_add(1, Ordering::Relaxed)
        )))
    }
}
impl Drop for StoreFile {
    fn drop(&mut self) {
        for suffix in ["", "-wal", "-shm", "-journal"] {
            let _ = std::fs::remove_file(format!("{}{suffix}", self.0.display()));
        }
    }
}

#[test]
fn publication_binding() {
    with_catalog(None, |catalog| {
        let a = ok(authority::bind(catalog));
        let (initial, increment, allowed) = (state(0, 0), command(120), context(true));
        let genuine = publication(
            &a,
            Raw {
                state: &initial,
                command: &increment,
                context: &allowed,
            },
        );
        let data = effects::publication_data(&genuine);
        let bound = ok(effects::bind_publication(&a, &data));
        assert_eq!(bound.subject(), genuine.subject());
        assert_eq!(effects::publication_data(&bound), data);
        // A forged subject, even one byte, refuses before any commit.
        let mut forged = data.clone();
        let last = forged.subject.len() - 1;
        forged.subject[last] ^= 1;
        assert_eq!(
            effects::bind_publication(&a, &forged).err(),
            Some(Unbound::Subject),
            "forged subject bound"
        );
        // Another genuine publication's subject on these inputs refuses too.
        let failure = command(121);
        let other = publication(
            &a,
            Raw {
                state: &initial,
                command: &failure,
                context: &allowed,
            },
        );
        let swapped = port::PublicationData {
            subject: other.subject().to_vec(),
            ..data.clone()
        };
        assert_eq!(
            effects::bind_publication(&a, &swapped).err(),
            Some(Unbound::Subject),
            "swapped subject bound"
        );
        let mut foreign = data.clone();
        foreign.identity[0] ^= 1;
        assert_eq!(
            effects::bind_publication(&a, &foreign).err(),
            Some(Unbound::Identity)
        );
        with_catalog(Some(9999), |other_catalog| {
            let b = ok(authority::bind(other_catalog));
            assert_ne!(a.identity(), b.identity());
            assert_eq!(
                effects::bind_publication(&b, &data).err(),
                Some(Unbound::Identity)
            );
        });
        let denied = context(false);
        let rejected = port::PublicationData {
            context: denied,
            ..data
        };
        assert_eq!(
            effects::bind_publication(&a, &rejected).err(),
            Some(Unbound::NotCommitting)
        );
    });
}

/// Every V2 call the registry names, as a compiled call; the default V2
/// APIs themselves are unchanged.
#[test]
fn v2_mapping() {
    let mut named = Vec::new();
    for signature in REGISTRY.signatures {
        match signature.v2 {
            V2::Api(calls) => named.extend_from_slice(calls),
            V2::External(name) => named.push(name),
            V2::OptIn => {}
        }
    }
    assert_eq!(
        named,
        [
            "V2SqliteShell::snapshot",
            "V2SqliteShell::commit",
            "V2SqliteShell::commit_at",
            "V2SqliteShell::commit_bounded_at",
            "Lineage::audit_history_read_only",
            "V2SqliteShell::export_pending",
            "relay::export_line",
            "V2SqliteShell::pending_by_id",
            "Pending::relayed",
            "Delivered::acknowledge",
            "V2SqliteShell::open",
            "V2SqliteShell::audit",
        ]
    );
    // Compile-time witnesses, in the same order; none is called here.
    let _ = |db: &mut V2SqliteShell<'_, '_>| db.snapshot();
    let _ = |db: &mut V2SqliteShell<'_, '_>, p: Publication<'_>| db.commit(Hash32::ZERO, p);
    let _ = |db: &mut V2SqliteShell<'_, '_>, p: Publication<'_>| db.commit_at(0, Hash32::ZERO, p);
    let _ = |db: &mut V2SqliteShell<'_, '_>, p: Publication<'_>| {
        db.commit_bounded_at(0, Hash32::ZERO, p, 0)
    };
    let _ = |lineage: &Lineage<'_, '_>, path: &Path| lineage.audit_history_read_only(path);
    let _ = |db: &mut V2SqliteShell<'_, '_>| db.export_pending();
    let _ = |delivery: &v2::Delivery| v2::relay::export_line(delivery);
    let _ = |db: &mut V2SqliteShell<'_, '_>| -> Result<(), Error> {
        db.pending_by_id(Hash32::ZERO)?
            .relayed(Hash32::ZERO)?
            .acknowledge()
    };
    let _ = |path: &Path, a: &Authority<'_>| V2SqliteShell::open(path, a).map(drop);
    let _ = |db: &mut V2SqliteShell<'_, '_>| db.audit();
}

fn snapshot(s: &v2::Snapshot) -> port::Snapshot {
    port::Snapshot {
        version: s.version(),
        identity: s.binding().to_vec(),
        state: s.state().to_vec(),
        root: s.root(),
        chain: s.chain(),
    }
}

fn receipt(r: &v2::CommitReceipt) -> port::CommitReceipt {
    port::CommitReceipt {
        version: r.version(),
        chain: r.chain(),
        status: r.status(),
    }
}

fn refused<T>(error: &Error) -> Outcome<T> {
    Outcome::Refused(match error {
        Error::Concurrent => port::Code::Concurrent,
        Error::Replay => port::Code::Replay,
        Error::PreState => port::Code::PreState,
        Error::Identity => port::Code::Identity,
        other => panic!("unmapped V2 error in this test: {other:?}"),
    })
}

/// Runs the commit program against a real store. `lose` drops the reply to
/// the first commit after (`true`) or instead of (`false`) applying it.
fn recover(
    a: &Authority<'_>,
    lineage: &Lineage<'_, '_>,
    file: &StoreFile,
    invocation: CommitInvocation,
    lose: bool,
) -> (Vec<Operation>, Outcome<port::CommitReceipt>) {
    let mut db = Some(ok(V2SqliteShell::open(&file.0, a)));
    let mut driver = ok(Driver::<CommitProgram>::start(
        REGISTRY,
        invocation.epoch,
        CommitProgram::start(invocation, CommitPolicy { retries: 2 }),
    ));
    let mut asked = Vec::new();
    let mut first = true;
    while let Some((binding, operation)) = driver.request() {
        let (binding, operation) = (binding.clone(), operation.clone());
        asked.push(operation.clone());
        let response = match &operation {
            Operation::Commit(_, key, data, mode) => {
                let bound = ok(effects::bind_publication(a, data));
                let shell = db.as_mut().unwrap_or_else(|| panic!("closed"));
                if std::mem::take(&mut first) {
                    if lose {
                        let _ = shell.commit(*key, bound);
                    }
                    Response::Commit(Outcome::Unknown(Fault::Io))
                } else {
                    let result = match mode {
                        CommitMode::Unversioned => shell.commit(*key, bound),
                        CommitMode::At(version) => shell.commit_at(*version, *key, bound),
                        CommitMode::BoundedAt(version, cap) => {
                            shell.commit_bounded_at(*version, *key, bound, *cap)
                        }
                    };
                    Response::Commit(match &result {
                        Ok(r) => Outcome::Ok(receipt(r)),
                        Err(error) => refused(error),
                    })
                }
            }
            Operation::Reopen(_) => {
                drop(db.take());
                let mut shell = ok(V2SqliteShell::open(&file.0, a));
                ok(shell.audit());
                let head = snapshot(&ok(shell.snapshot()));
                db = Some(shell);
                Response::Reopen(Outcome::Ok(head))
            }
            Operation::ReadHistory(_) => {
                let history = ok(lineage.audit_history_read_only(&file.0));
                let mut commits = Vec::new();
                for commit in &history.commits {
                    commits.push(port::HistoryEntry {
                        key: commit.replay_id,
                        version: commit.sequence,
                        publication: ok(effects::audited_publication(
                            a,
                            &commit.prestate,
                            &commit.command,
                            &commit.context,
                        )),
                    });
                }
                Response::ReadHistory(Outcome::Ok(port::AuditedHistory {
                    head: snapshot(&history.snapshot),
                    commits,
                }))
            }
            other => panic!("unexpected request {other:?}"),
        };
        ok(driver.reply(Reply { binding, response }));
    }
    let outcome = driver
        .outcome()
        .cloned()
        .unwrap_or_else(|| panic!("driver failed: {:?}", driver.failure()));
    (asked, outcome)
}

#[test]
fn v2_recovery_rule() {
    with_catalog(None, |catalog| {
        let lineage = ok(Lineage::bind(&[catalog], &[]));
        let a = &lineage.authorities()[0];
        let file = StoreFile::new();
        let initial = state(0, 0);
        drop(ok(V2SqliteShell::create(&file.0, a, genesis(a, &initial))));
        let (increment, allowed) = (command(120), context(true));
        let raw = Raw {
            state: &initial,
            command: &increment,
            context: &allowed,
        };
        let data = effects::publication_data(&publication(a, raw));
        let key = Hash32::new([1; 32]);
        let invocation = |key, data, mode| CommitInvocation {
            epoch: 3,
            store: port::Store {
                identity: Hash32::new([0xa0; 32]),
                handle: 1,
            },
            key,
            publication: data,
            mode,
        };
        // The reply to an applied commit is lost: the exact key and
        // publication are in the audited history, so the program retries
        // unversioned and receives the original receipt as a replay.
        let (asked, outcome) = recover(
            a,
            &lineage,
            &file,
            invocation(key, data.clone(), CommitMode::At(0)),
            true,
        );
        let ports: Vec<_> = asked.iter().map(Operation::port).collect();
        assert_eq!(
            ports,
            [
                zeno_fcis_shell::effects::registry::Port::Commit,
                zeno_fcis_shell::effects::registry::Port::Reopen,
                zeno_fcis_shell::effects::registry::Port::ReadHistory,
                zeno_fcis_shell::effects::registry::Port::Commit,
            ]
        );
        assert!(matches!(
            asked[3],
            Operation::Commit(_, _, _, CommitMode::Unversioned)
        ));
        let Outcome::Ok(replayed) = outcome else {
            panic!("recovery failed: {outcome:?}");
        };
        assert_eq!(replayed.version, 1);
        assert_eq!(replayed.status, CommitStatus::IdempotentReplay);
        // Why the rule exists: V2 checks the expected version before the
        // replay lookup, so retrying the original guard is refused.
        let mut db = ok(V2SqliteShell::open(&file.0, a));
        assert!(matches!(
            db.commit_at(0, key, ok(effects::bind_publication(a, &data))),
            Err(Error::Concurrent)
        ));
        let head = ok(db.snapshot());
        assert_eq!(head.chain(), replayed.chain);
        drop(db);
        // The reply is lost before the commit applied: absence keeps the
        // original guard, which then commits once.
        let next = state(1, 0);
        let raw = Raw {
            state: &next,
            command: &increment,
            context: &allowed,
        };
        let second = effects::publication_data(&publication(a, raw));
        let (asked, outcome) = recover(
            a,
            &lineage,
            &file,
            invocation(Hash32::new([2; 32]), second, CommitMode::At(1)),
            false,
        );
        assert!(matches!(
            asked[3],
            Operation::Commit(_, _, _, CommitMode::At(1))
        ));
        let Outcome::Ok(committed) = outcome else {
            panic!("commit failed: {outcome:?}");
        };
        assert_eq!(committed.version, 2);
        assert_eq!(committed.status, CommitStatus::Committed);
        assert_eq!(
            ok(lineage.audit_history_read_only(&file.0)).commits.len(),
            2
        );
    });
}
