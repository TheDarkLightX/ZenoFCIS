//! Compact publication storage, complete accounting, migration and identity controls.
use rusqlite::Connection;
use std::{
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};
use zeno_fcis_codec::{CanonicalEncode, DecodeLimits, Envelope, Hash32, commitment, decode_value};
use zeno_fcis_crypto::RustCryptoSha256;
use zeno_fcis_plan::OutboxEntry;
use zeno_fcis_shell::{CommitStatus, MemoryDestination};
use zeno_fcis_shell_sqlite::{
    CrashPoint,
    v2::{Error, V2SqliteShell, compact_publication, expand_publication},
};
use zeno_fcis_synthesis::finite::{
    V2Resource as Resource,
    canonical_v2::schema,
    v2_authority::{self as authority, Authority, Publication, PublicationOutcome},
    v2_catalog as catalog,
    v2_composition::{self as c, Class, Raw},
};
use zeno_fcis_value::{Field, Value};

/// The actual original accepting/committed-failure durable-counter declaration.
#[path = "../../zeno-fcis-cli/templates/durable-counter/src/v2_contract.rs"]
pub mod durable;
/// The actual original prepared-counter declaration, schema and complete policy.
#[path = "../../zeno-fcis-cli/templates/prepared-counter/src/v2_contract.rs"]
pub mod prepared;

fn ok<T, E: std::fmt::Debug>(r: Result<T, E>) -> T {
    r.unwrap_or_else(|e| panic!("unexpected refusal: {e:?}"))
}
/// Delivers and acknowledges the oldest pending entry; whether there was one.
fn deliver_next(
    db: &mut V2SqliteShell<'_, '_>,
    destination: &mut MemoryDestination,
) -> Result<bool, Error> {
    let Some(pending) = db.next_pending()? else {
        return Ok(false);
    };
    pending.deliver(destination)?.acknowledge()?;
    Ok(true)
}
fn with_authority(step: Option<u64>, run: impl FnOnce(&Authority<'_>, &Authority<'_>)) {
    let contract = prepared::Contract::new();
    let mut definition = contract.descriptor();
    if let Some(step) = step {
        definition.limits = definition.limits.with_limit(Resource::Step, step);
    }
    let policy = authority::policy_bytes(
        &definition,
        prepared::ORIGINAL_SCHEMA,
        &prepared::FRAMING,
        prepared::CHANNEL_ROOTS,
    )
    .unwrap_or_else(|| panic!("policy encoding refused"));
    let catalog = ok(catalog::bind_original(
        prepared::ORIGINAL_SCHEMA,
        &prepared::DESCRIPTION,
        catalog::Limits {
            schema: schema::Limits {
                bytes: prepared::ORIGINAL_SCHEMA.len() as u64,
                types: 7,
                fields: 3,
                variants: 0,
            },
            contract_bytes: policy.len() as u64,
        },
        &policy,
        &definition,
        &prepared::FRAMING,
        prepared::CHANNEL_ROOTS,
    ));
    let authority = ok(authority::bind(&catalog));
    run(&authority, &authority);
}
fn frame(binding: c::FrameBinding, value: Value) -> Vec<u8> {
    ok(Envelope::new(binding.root, Hash32::new(binding.schema), value).canonical_bytes())
}
fn state(count: i128) -> Vec<u8> {
    frame(
        prepared::FRAMING.state,
        ok(Value::record_canonical(vec![Field::new(
            110,
            Value::signed(count),
        )])),
    )
}
fn command(deltas: [i128; 3]) -> Vec<u8> {
    frame(
        prepared::FRAMING.command,
        ok(Value::record_canonical(
            deltas
                .into_iter()
                .zip([120, 121, 122])
                .map(|(n, id)| Field::new(id, Value::signed(n)))
                .collect(),
        )),
    )
}
fn context(allow: bool) -> Vec<u8> {
    frame(prepared::FRAMING.context, Value::boolean(allow))
}
fn genesis<'a>(h: &'a Authority<'_>, initial: &'a [u8]) -> Publication<'a> {
    match h.publish_genesis(initial) {
        PublicationOutcome::Commit(p) => p,
        other => panic!("genesis: {other:?}"),
    }
}
fn publication<'a>(h: &'a Authority<'_>, raw: Raw<'a>) -> Publication<'a> {
    match h.publish(raw) {
        PublicationOutcome::Commit(p) => p,
        other => panic!("publication: {other:?}"),
    }
}
fn raw<'a>(s: &'a [u8], c: &'a [u8], x: &'a [u8]) -> Raw<'a> {
    Raw {
        state: s,
        command: c,
        context: x,
    }
}
static SEQUENCE: AtomicU64 = AtomicU64::new(0);
struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        let p = std::env::temp_dir().join(format!(
            "zenofcis-history-{}-{}.db",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        ok(std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&p));
        Self(p)
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

#[test]
fn all_original_216_outputs_and_all_68_exact_and_one_under_aggregates() {
    with_authority(None, |a, h| {
        let zero = state(0);
        let allow = context(true);
        let mut cases = 0;
        let mut accepted = 0;
        let mut rejected = 0;
        let mut maximum = 0;
        for count in 0..=3 {
            for first in -1..=1 {
                for second in -1..=1 {
                    for third in -1..=1 {
                        for allowed in [false, true] {
                            let pre = state(count);
                            let cmd = command([first, second, third]);
                            let ctx = context(allowed);
                            let mut expected = count;
                            let valid = [first, second, third].into_iter().all(|n| {
                                expected += n;
                                (0..=3).contains(&expected)
                            });
                            let scoped = h.publish(raw(&pre, &cmd, &ctx));
                            let standalone = a.evaluate(raw(&pre, &cmd, &ctx));
                            assert_eq!(
                                format!("{:?}", scoped.evaluation().result()),
                                format!("{:?}", standalone.result())
                            );
                            assert_eq!(scoped.evaluation().usage(), standalone.usage());
                            assert_eq!(
                                scoped.evaluation().ingress_usage(),
                                standalone.ingress_usage()
                            );
                            assert_eq!(
                                scoped.evaluation().decision_usage(),
                                standalone.decision_usage()
                            );
                            assert_eq!(scoped.evaluation().reads(), standalone.reads());
                            assert_eq!(
                                scoped.evaluation().decision_attempts(),
                                standalone.decision_attempts()
                            );
                            assert_eq!(scoped.evaluation().diagnostics(), standalone.diagnostics());
                            assert_eq!(scoped.evaluation().law_reads(), standalone.law_reads());
                            match scoped {
                                PublicationOutcome::Reject(evaluation) => {
                                    let r = ok(evaluation.result());
                                    assert!(!allowed || !valid);
                                    assert_eq!(r.class(), Class::Reject);
                                    assert_eq!(r.reason(), Some(if allowed { 201 } else { 200 }));
                                    assert!(
                                        r.post().is_empty()
                                            && r.patch().is_empty()
                                            && r.effects().is_empty()
                                            && r.outbox().is_empty()
                                    );
                                    rejected += 1;
                                }
                                PublicationOutcome::Commit(p) => {
                                    assert!(allowed && valid);
                                    assert_eq!(ok(p.evaluation().result()).class(), Class::Accept);
                                    assert_eq!(p.poststate(), state(expected));
                                    assert!(p.effects().is_empty());
                                    assert_eq!(p.outbox().len(), 1);
                                    let d = &p.outbox()[0];
                                    assert_eq!(
                                        (
                                            d.ordinal(),
                                            d.channel(),
                                            d.destination_root(),
                                            d.payload_root()
                                        ),
                                        (0, 300, 103, 104)
                                    );
                                    assert_eq!(
                                        d.destination(),
                                        ok(ok(Value::text_ascii("local-observer".to_owned()))
                                            .canonical_bytes())
                                    );
                                    assert_eq!(
                                        d.payload(),
                                        ok(ok(Value::record_canonical(vec![Field::new(
                                            112,
                                            Value::signed(expected)
                                        )]))
                                        .canonical_bytes())
                                    );
                                    assert_eq!(
                                        d.idempotency(),
                                        ok(Value::unsigned(0).canonical_bytes())
                                    );
                                    let mut db =
                                        ok(V2SqliteShell::create_in_memory(a, genesis(h, &zero)));
                                    if count != 0 {
                                        let reach = command([
                                            i128::from(count >= 1),
                                            i128::from(count >= 2),
                                            i128::from(count >= 3),
                                        ]);
                                        ok(db.commit(
                                            Hash32::new([250; 32]),
                                            publication(h, raw(&zero, &reach, &allow)),
                                        ));
                                    }
                                    let before = ok(db.snapshot());
                                    assert_eq!(before.state(), pre);
                                    let version = before.version();
                                    let key = Hash32::new([1; 32]);
                                    let bundle = ok(db.publication_bundle(version, key, &p));
                                    let sizes = bundle.sizes();
                                    let required = ok(sizes.total());
                                    assert!(
                                        sizes.receipt > 0
                                            && sizes.outbox > 0
                                            && sizes.authorization > 0
                                    );
                                    assert!(sizes.bundle > sizes.receipt + sizes.outbox);
                                    assert!(required <= 16_384);
                                    assert_eq!(
                                        sizes.authorization,
                                        ok(compact_publication(p.identity(), p.subject())).len()
                                    );
                                    assert_eq!(sizes.bundle, bundle.canonical_bytes().len());
                                    assert_eq!(sizes.receipt, bundle.receipt().len());
                                    assert_eq!(sizes.outbox, bundle.outbox().len());
                                    assert!(
                                        matches!(db.commit_bounded_at(version,key,p,required-1,None),Err(Error::Capacity{required:r,declared:d}) if r==required && d==required-1)
                                    );
                                    assert_eq!(ok(db.snapshot()), before);
                                    let p = publication(h, raw(&pre, &cmd, &ctx));
                                    let record = p.subject().to_vec();
                                    assert_eq!(
                                        ok(db.commit_bounded_at(version, key, p, required, None))
                                            .status(),
                                        CommitStatus::Committed
                                    );
                                    assert_eq!(ok(db.binding()), h.identity());
                                    assert!(matches!(
                                        a.replay_publication(raw(&pre, &cmd, &ctx), &record),
                                        PublicationOutcome::Commit(_)
                                    ));
                                    ok(db.audit());
                                    maximum = maximum.max(required);
                                    accepted += 1;
                                }
                                other => panic!("original supported input refused: {other:?}"),
                            }
                            cases += 1;
                        }
                    }
                }
            }
        }
        assert_eq!((cases, accepted, rejected), (216, 68, 148));
        println!(
            "scoped_original_cases={cases} accepting={accepted} rejecting={rejected} exact_budget=68 one_under=68 maximum_complete_aggregate={maximum}"
        );
    });
}

#[test]
fn seven_original_crash_points_restart_all_or_none() {
    with_authority(None, |a, h| {
        let pre = state(0);
        let cmd = command([1, 1, 1]);
        let ctx = context(true);
        for point in [
            CrashPoint::BeforeTransaction,
            CrashPoint::AfterValidation,
            CrashPoint::AfterStateWrite,
            CrashPoint::AfterReplayWrite,
            CrashPoint::AfterOutboxWrite,
            CrashPoint::BeforeCommit,
            CrashPoint::AfterCommit,
        ] {
            let temp = Temp::new();
            let mut db = ok(V2SqliteShell::create(&temp.0, a, genesis(h, &pre)));
            let before = ok(db.snapshot());
            assert!(
                matches!(db.commit_with_crash_at(0,Hash32::new([2;32]),publication(h,raw(&pre,&cmd,&ctx)),Some(point)),Err(Error::InjectedCrash(p)) if p==point)
            );
            drop(db);
            let mut reopened = ok(V2SqliteShell::open(&temp.0, a));
            let after = ok(reopened.snapshot());
            if point == CrashPoint::AfterCommit {
                assert_eq!((after.version(), after.pending()), (1, 1));
                assert_eq!(after.state(), state(3));
            } else {
                assert_eq!(after, before);
            }
            ok(reopened.audit());
        }
    });
}

#[test]
fn reopen_checkpoint_exact_replay_and_interrupted_delivery() {
    with_authority(None, |a, h| {
        let source = Temp::new();
        let destination = Temp::new();
        let pre = state(0);
        let cmd = command([1, 1, 1]);
        let ctx = context(true);
        let key = Hash32::new([3; 32]);
        let mut db = ok(V2SqliteShell::create(&source.0, a, genesis(h, &pre)));
        let genesis_bytes = genesis(h, &pre).subject().to_vec();
        ok(db.commit_at(0, key, publication(h, raw(&pre, &cmd, &ctx))));
        let full = publication(h, raw(&pre, &cmd, &ctx)).subject().to_vec();
        let checkpoint = ok(db.checkpoint());
        let snapshot = ok(db.snapshot());
        let mut memory = MemoryDestination::default();
        let pending = ok(db.next_pending()).unwrap_or_else(|| panic!("missing delivery"));
        // The destination records the delivery; the process stops before
        // the store acknowledges it.
        drop(ok(pending.deliver(&mut memory)));
        assert_eq!(memory.delivered_count(), 1);
        assert_eq!(ok(db.snapshot()).pending(), 1);
        drop(db);
        let mut db = ok(V2SqliteShell::open_at_checkpoint(&source.0, a, &checkpoint));
        assert_eq!(ok(db.snapshot()), snapshot);
        assert_eq!(
            ok(db.commit(key, publication(h, raw(&pre, &cmd, &ctx)))).status(),
            CommitStatus::IdempotentReplay
        );
        assert_eq!(ok(db.snapshot()), snapshot);
        assert!(ok(deliver_next(&mut db, &mut memory)));
        assert!(!ok(deliver_next(&mut db, &mut memory)));
        assert_eq!(memory.delivered_count(), 1);
        assert_eq!(ok(db.snapshot()).pending(), 0);
        let PublicationOutcome::Commit(g) = a.replay_genesis_publication(&pre, &genesis_bytes)
        else {
            panic!("genesis replay");
        };
        let mut imported = ok(V2SqliteShell::create(&destination.0, a, g));
        let PublicationOutcome::Commit(p) = a.replay_publication(raw(&pre, &cmd, &ctx), &full)
        else {
            panic!("transition replay");
        };
        ok(imported.commit_at(0, key, p));
        assert_eq!(ok(imported.snapshot()), snapshot);
        ok(imported.audit());
        assert!(V2SqliteShell::create(&destination.0, a, genesis(h, &pre)).is_err());
    });
}

#[test]
fn original_head_context_aba_second_handle_and_replay_keys_remain_independent_of_h() {
    with_authority(None, |a, h| {
        let temp = Temp::new();
        let pre = state(0);
        let inc = command([1, 0, 0]);
        let dec = command([-1, 0, 0]);
        let yes = context(true);
        let no = context(false);
        let mut first = ok(V2SqliteShell::create(&temp.0, a, genesis(h, &pre)));
        let mut second = ok(V2SqliteShell::open(&temp.0, a));
        let original = ok(first.snapshot());
        assert!(matches!(
            h.publish(raw(&pre, &inc, &no)),
            PublicationOutcome::Reject(_)
        ));
        assert_eq!(ok(first.snapshot()), original);
        ok(first.commit_at(
            0,
            Hash32::new([4; 32]),
            publication(h, raw(&pre, &inc, &yes)),
        ));
        let before = ok(first.snapshot());
        assert!(matches!(
            second.commit_at(
                0,
                Hash32::new([5; 32]),
                publication(h, raw(&pre, &inc, &yes))
            ),
            Err(Error::Concurrent)
        ));
        assert_eq!(ok(first.snapshot()), before);
        assert!(matches!(
            first.commit(
                Hash32::new([4; 32]),
                publication(h, raw(&pre, &command([0, 0, 0]), &yes))
            ),
            Err(Error::Replay)
        ));
        let post = state(1);
        ok(first.commit_at(
            1,
            Hash32::new([6; 32]),
            publication(h, raw(&post, &dec, &yes)),
        ));
        let before = ok(first.snapshot());
        assert_eq!(before.root(), original.root());
        assert_ne!(before.version(), original.version());
        assert!(matches!(
            second.commit_at(
                0,
                Hash32::new([7; 32]),
                publication(h, raw(&pre, &inc, &yes))
            ),
            Err(Error::Concurrent)
        ));
        assert_eq!(ok(first.snapshot()), before);
        ok(second.audit());
        assert_eq!(ok(first.snapshot()), ok(second.snapshot()));
    });
}

#[test]
fn malformed_missing_truncated_replaced_binding_refuses_live_reopen_import_and_recovery() {
    with_authority(None, |a, h| {
        let pre = state(0);
        let cmd = command([1, 0, 0]);
        let ctx = context(true);
        for mutation in [
            "DELETE FROM v2_genesis",
            "UPDATE v2_genesis SET identity=x''",
            "UPDATE v2_genesis SET identity=x'00'",
            "UPDATE v2_genesis SET identity=substr(identity,1,length(identity)-1)",
            "UPDATE v2_genesis SET identity=identity||x'00'",
            "UPDATE v2_genesis SET identity=CAST(identity||x'00' AS BLOB)",
        ] {
            let temp = Temp::new();
            let mut db = ok(V2SqliteShell::create(&temp.0, a, genesis(h, &pre)));
            ok(db.commit(Hash32::new([8; 32]), publication(h, raw(&pre, &cmd, &ctx))));
            let checkpoint = ok(db.checkpoint());
            let external = ok(Connection::open(&temp.0));
            ok(external.execute(mutation, []));
            if mutation.starts_with("DELETE")
                || mutation == "UPDATE v2_genesis SET identity=identity||x'00'"
            {
                assert!(db.snapshot().is_err());
            } else {
                assert!(matches!(db.snapshot(), Err(Error::Identity)));
            }
            assert!(db.binding().is_err());
            assert!(db.audit().is_err());
            // The store issues no delivery token, and none can be made
            // outside it (see the compile-fail examples on `v2::Pending`).
            assert!(db.next_pending().is_err());
            assert!(V2SqliteShell::open(&temp.0, a).is_err());
            assert!(V2SqliteShell::open_at_checkpoint(&temp.0, a, &checkpoint).is_err());
            let sequence: i64 =
                ok(external.query_row("SELECT sequence FROM v2_state", [], |r| r.get(0)));
            assert_eq!(sequence, 1);
        }
        let mut db = ok(V2SqliteShell::create_in_memory(a, genesis(h, &pre)));
        let before = ok(db.snapshot());
        let p = publication(h, raw(&pre, &cmd, &ctx));
        assert!(matches!(
            compact_publication(&[], p.subject()),
            Err(Error::Identity)
        ));
        assert!(matches!(
            compact_publication(&a.identity()[..a.identity().len() - 1], p.subject()),
            Err(Error::Identity)
        ));
        assert_eq!(ok(db.snapshot()), before);
    });
}

#[test]
fn same_operation_under_another_admitted_policy_refuses_before_writes() {
    with_authority(None, |a, h| {
        let pre = state(0);
        let cmd = command([1, 0, 0]);
        let ctx = context(true);
        let temp = Temp::new();
        let mut db = ok(V2SqliteShell::create(&temp.0, a, genesis(h, &pre)));
        let before = ok(db.snapshot());
        with_authority(Some(9999), |other, _| {
            assert_ne!(a.identity(), other.identity());
            assert!(matches!(
                V2SqliteShell::create_in_memory(a, genesis(other, &pre)),
                Err(Error::Identity)
            ));
            let p = publication(other, raw(&pre, &cmd, &ctx));
            assert_eq!(p.poststate(), state(1));
            assert!(matches!(
                V2SqliteShell::open(&temp.0, other),
                Err(Error::Identity)
            ));
            assert!(matches!(
                db.commit(Hash32::new([11; 32]), p),
                Err(Error::Identity)
            ));
            assert_eq!(ok(db.snapshot()), before);
        });
    });
}

#[test]
fn compact_round_trips_and_malformed_publications_refuse() {
    with_authority(None, |a, h| {
        let pre = state(0);
        let cmd = command([1, 0, 0]);
        let ctx = context(true);
        let p = publication(h, raw(&pre, &cmd, &ctx));
        let g = genesis(h, &pre);
        for full in [p.subject(), g.subject()] {
            let compact = ok(compact_publication(a.identity(), full));
            assert_eq!(compact.len() + 17 + a.identity().len(), full.len());
            let old =
                u128::from_be_bytes(full[52..68].try_into().unwrap_or_else(|_| panic!("length")));
            let new = u128::from_be_bytes(
                compact[52..68]
                    .try_into()
                    .unwrap_or_else(|_| panic!("length")),
            );
            assert_eq!(new + 17 + a.identity().len() as u128, old);
            assert_eq!(ok(expand_publication(a.identity(), &compact)), full);
            assert_eq!(
                ok(compact_publication(
                    a.identity(),
                    &ok(expand_publication(a.identity(), &compact))
                )),
                compact
            );
            assert!(compact_publication(b"wrong", full).is_err());
            for index in [0, 17, 50, 51, 68, 85, 118] {
                let mut bad = compact.clone();
                bad[index] ^= 1;
                assert!(
                    expand_publication(a.identity(), &bad).is_err(),
                    "offset {index}"
                );
            }
            for end in [0, 17, 51, 67, 68, 118] {
                assert!(expand_publication(a.identity(), &compact[..end]).is_err());
            }
            let mut overflow = compact.clone();
            overflow[52..68].fill(255);
            assert!(expand_publication(a.identity(), &overflow).is_err());
            let mut trailing = compact.clone();
            trailing.push(0);
            let expanded = ok(expand_publication(a.identity(), &trailing));
            assert!(matches!(
                a.replay_publication(raw(&pre, &cmd, &ctx), &expanded),
                PublicationOutcome::Refused { .. }
            ));
            assert!(matches!(
                a.replay_genesis_publication(&pre, &expanded),
                PublicationOutcome::Refused { .. }
            ));
        }
        let full = p.subject();
        for index in [0, full.len() / 2, full.len() - 1] {
            let mut changed = full.to_vec();
            changed[index] ^= 1;
            assert!(matches!(
                a.replay_publication(raw(&pre, &cmd, &ctx), &changed),
                PublicationOutcome::Refused { .. }
            ));
        }
        assert!(matches!(
            a.replay_publication(raw(&pre, &cmd, &ctx), &full[..full.len() - 1]),
            PublicationOutcome::Refused { .. }
        ));
    });
}

#[test]
fn every_durable_field_and_schema_tamper_refuses_without_relabeling_or_delivery() {
    with_authority(None, |a, h| {
        let pre = state(0);
        let cmd = command([1, 0, 0]);
        let ctx = context(true);
        for mutation in [
            "UPDATE v2_commits SET publication=x'00'",
            "UPDATE v2_commits SET command=x'00'",
            "UPDATE v2_commits SET context=x'00'",
            "UPDATE v2_commits SET post=x'00'",
            "UPDATE v2_commits SET certificate=x'00'",
            "UPDATE v2_commits SET replay_id=zeroblob(32)",
            "UPDATE v2_commits SET previous_chain=zeroblob(32)",
            "UPDATE v2_deliveries SET payload=x'00'",
            "UPDATE v2_deliveries SET marker=x'00'",
            "UPDATE v2_deliveries SET destination_root=104",
            "UPDATE v2_deliveries SET ordinal=1",
            "DELETE FROM v2_deliveries",
            "UPDATE v2_genesis SET publication=x'00'",
            "UPDATE v2_state SET sequence=2",
        ] {
            let temp = Temp::new();
            let mut db = ok(V2SqliteShell::create(&temp.0, a, genesis(h, &pre)));
            ok(db.commit(Hash32::new([13; 32]), publication(h, raw(&pre, &cmd, &ctx))));
            let external = ok(Connection::open(&temp.0));
            ok(external.execute(mutation, []));
            let mut memory = MemoryDestination::default();
            assert!(deliver_next(&mut db, &mut memory).is_err());
            assert_eq!(memory.delivered_count(), 0);
            assert!(V2SqliteShell::open(&temp.0, a).is_err());
        }
        let temp = Temp::new();
        let mut db = ok(V2SqliteShell::create(&temp.0, a, genesis(h, &pre)));
        let external = ok(Connection::open(&temp.0));
        ok(external.execute_batch("CREATE TRIGGER altered AFTER INSERT ON v2_commits BEGIN DELETE FROM v2_deliveries; END;"));
        assert!(matches!(
            db.commit(Hash32::new([14; 32]), publication(h, raw(&pre, &cmd, &ctx))),
            Err(Error::Schema(10))
        ));
        let sequence: i64 =
            ok(external.query_row("SELECT sequence FROM v2_state", [], |r| r.get(0)));
        assert_eq!(sequence, 0);
    });
}

#[test]
fn old_schema_versions_never_upgrade_implicitly() {
    with_authority(None, |a, h| {
        let pre = state(0);
        for version in [5, 7, 8, 9] {
            let path = Temp::new();
            let connection = ok(Connection::open(&path.0));
            ok(connection.execute_batch(&format!(
                "CREATE TABLE legacy(x BLOB);PRAGMA user_version={version};"
            )));
            assert!(matches!(V2SqliteShell::open(&path.0,a),Err(Error::Schema(v)) if v==version));
            assert!(
                matches!(V2SqliteShell::create(&path.0,a,genesis(h,&pre)),Err(Error::Schema(v)) if v==version)
            );
            let stored: i64 = ok(connection.query_row("PRAGMA user_version", [], |r| r.get(0)));
            assert_eq!(stored, version);
        }
    });
}

#[test]
fn delivery_id_is_independently_derived_from_full_publication_and_certificate() {
    with_authority(None, |a, h| {
        let pre = state(0);
        let cmd = command([1, 0, 0]);
        let ctx = context(true);
        let p = publication(h, raw(&pre, &cmd, &ctx));
        let d = &p.outbox()[0];
        let key = Hash32::new([15; 32]);
        let mut db = ok(V2SqliteShell::create_in_memory(a, genesis(h, &pre)));
        let before = ok(db.snapshot());
        let post_root = ok(commitment::<RustCryptoSha256>(
            zeno_fcis_codec::domains::V2_STATE,
            p.poststate(),
        ));
        let record_hash = ok(commitment::<RustCryptoSha256>(
            zeno_fcis_codec::domains::V2_PUBLICATION,
            p.subject(),
        ));
        let mut certificate = b"ZFCISV2-CERT\0".to_vec();
        certificate.extend_from_slice(&1_u64.to_be_bytes());
        for hash in [key, before.root(), post_root, before.chain(), record_hash] {
            certificate.extend_from_slice(hash.as_bytes());
        }
        assert_eq!(ok(db.publication_bundle(0, key, &p)).receipt(), certificate);
        let mut identity = Vec::new();
        for part in [p.subject(), &certificate] {
            identity.extend_from_slice(&ok(u64::try_from(part.len())).to_be_bytes());
            identity.extend_from_slice(part);
        }
        let candidate = ok(commitment::<RustCryptoSha256>(
            zeno_fcis_codec::domains::V2_PUBLICATION,
            &identity,
        ));
        let entry = OutboxEntry::new(
            d.ordinal(),
            d.channel(),
            ok(decode_value(d.destination(), DecodeLimits::default())),
            ok(decode_value(d.payload(), DecodeLimits::default())),
        );
        let expected_id = ok(entry.delivery_id::<RustCryptoSha256>(candidate));
        let expected_hash = ok(commitment::<RustCryptoSha256>(
            zeno_fcis_codec::domains::OUTBOX_ENTRY,
            &ok(entry.canonical_bytes()),
        ));
        ok(db.commit(key, p));
        let pending = ok(db.next_pending()).unwrap_or_else(|| panic!("missing notice"));
        assert_eq!(
            (
                pending.delivery().delivery_id(),
                pending.delivery().entry_hash()
            ),
            (expected_id, expected_hash)
        );
        let mut memory = MemoryDestination::default();
        // Delivered and not acknowledged: the store issues the entry again,
        // and acknowledging it checks the hash the destination reported.
        drop(ok(pending.deliver(&mut memory)));
        assert!(ok(deliver_next(&mut db, &mut memory)));
        assert_eq!(memory.delivered_count(), 1);
        assert_eq!(ok(db.snapshot()).pending(), 0);
    });
}

#[test]
fn committed_failure_retains_its_complete_successor_and_ordered_obligation() {
    let contract = durable::Contract::new();
    let definition = contract.descriptor();
    let a = ok(durable::checked_authority(&definition));
    let h = &a;
    let pre = frame(
        durable::FRAMING.state,
        ok(Value::record_canonical(vec![
            Field::new(110, Value::signed(0)),
            Field::new(111, Value::signed(0)),
        ])),
    );
    let cmd = frame(durable::FRAMING.command, Value::sum(101, 121, None));
    let ctx = frame(durable::FRAMING.context, Value::boolean(true));
    let p = publication(h, raw(&pre, &cmd, &ctx));
    assert_eq!(ok(p.evaluation().result()).class(), Class::CommittedFailure);
    let expected = p.poststate().to_vec();
    assert_eq!(p.outbox().len(), 1);
    let mut db = ok(V2SqliteShell::create_in_memory(&a, genesis(h, &pre)));
    ok(db.commit(Hash32::new([16; 32]), p));
    let after = ok(db.snapshot());
    assert_eq!(after.state(), expected);
    assert_eq!((after.version(), after.pending()), (1, 1));
    ok(db.audit());
}

fn stored_row(row: &rusqlite::Row<'_>, integer_widths: &[Option<usize>]) -> Vec<u8> {
    let mut bytes = Vec::new();
    for (column, width) in integer_widths.iter().enumerate() {
        let field = match width {
            None => ok(row.get::<_, Vec<u8>>(column)),
            Some(width) => {
                let value = ok(row.get::<_, i64>(column));
                match width {
                    1 => vec![ok(u8::try_from(value))],
                    4 => ok(u32::try_from(value)).to_be_bytes().to_vec(),
                    8 => value.to_be_bytes().to_vec(),
                    other => panic!("unsupported durable integer width {other}"),
                }
            }
        };
        bytes.extend_from_slice(&ok(u64::try_from(field.len())).to_be_bytes());
        bytes.extend_from_slice(&field);
    }
    bytes
}

#[test]
fn aggregate_bundle_matches_every_actual_persisted_column_and_required_copy() {
    with_authority(None, |a, h| {
        let temp = Temp::new();
        let pre = state(0);
        let cmd = command([1, 1, 1]);
        let ctx = context(true);
        let p = publication(h, raw(&pre, &cmd, &ctx));
        let record = ok(compact_publication(p.identity(), p.subject()));
        let post = p.poststate().to_vec();
        let key = Hash32::new([17; 32]);
        let mut db = ok(V2SqliteShell::create(&temp.0, a, genesis(h, &pre)));
        let bundle = ok(db.publication_bundle(0, key, &p));
        let sizes = bundle.sizes();
        ok(db.commit_bounded_at(0, key, p, ok(sizes.total()), None));
        let connection = ok(Connection::open(&temp.0));

        let commit = ok(connection.query_row(
            "SELECT sequence,replay_id,pre_root,post_root,previous_chain,chain,command,context,post,publication,certificate FROM v2_commits WHERE sequence=1",
            [],
            |row| Ok(stored_row(row, &[Some(8),None,None,None,None,None,None,None,None,None,None])),
        ));
        let state_row = ok(connection.query_row(
            "SELECT singleton,sequence,state,root,chain FROM v2_state WHERE singleton=1",
            [],
            |row| Ok(stored_row(row, &[Some(8), Some(8), None, None, None])),
        ));
        let mut lanes = Vec::new();
        for lane in [0_i64, 1_i64] {
            let mut statement = ok(connection.prepare(
                "SELECT sequence,lane,ordinal,channel,destination_root,payload_root,destination,payload,marker,delivery_id,entry_hash,acknowledged FROM v2_deliveries WHERE sequence=1 AND lane=?1 ORDER BY ordinal",
            ));
            let rows = ok(ok(statement.query_map([lane], |row| {
                Ok(stored_row(
                    row,
                    &[
                        Some(8),
                        Some(1),
                        Some(4),
                        Some(4),
                        Some(4),
                        Some(4),
                        None,
                        None,
                        None,
                        None,
                        None,
                        Some(1),
                    ],
                ))
            }))
            .collect::<Result<Vec<_>, _>>());
            assert_eq!(rows.len(), usize::from(lane == 1));
            let mut lane_bytes = ok(u64::try_from(rows.len())).to_be_bytes().to_vec();
            for row in rows {
                lane_bytes.extend_from_slice(&ok(u64::try_from(row.len())).to_be_bytes());
                lane_bytes.extend_from_slice(&row);
            }
            lanes.push(lane_bytes);
        }
        let mut expected = b"ZFCIS-SQL-PUBLICATION-BUNDLE\0\x0a".to_vec();
        for row in [&commit, &state_row, &lanes[0], &lanes[1]] {
            expected.extend_from_slice(&ok(u64::try_from(row.len())).to_be_bytes());
            expected.extend_from_slice(row);
        }
        assert_eq!(bundle.canonical_bytes(), expected);
        assert_eq!(bundle.outbox(), lanes[1]);
        let (stored_command, stored_context, stored_post, stored_record, stored_receipt) = ok(connection.query_row(
            "SELECT command,context,post,publication,certificate FROM v2_commits WHERE sequence=1",
            [],
            |row| Ok((row.get::<_,Vec<u8>>(0)?,row.get::<_,Vec<u8>>(1)?,row.get::<_,Vec<u8>>(2)?,row.get::<_,Vec<u8>>(3)?,row.get::<_,Vec<u8>>(4)?)),
        ));
        assert_eq!(stored_command, cmd);
        assert_eq!(stored_context, ctx);
        assert_eq!(stored_post, post);
        assert_eq!(stored_record, record);
        assert_eq!(bundle.receipt(), stored_receipt);
        let state_copy = ok(connection.query_row(
            "SELECT state FROM v2_state WHERE singleton=1",
            [],
            |row| row.get::<_, Vec<u8>>(0),
        ));
        assert_eq!(state_copy, stored_post);
        let bare_command = ok(ok(Value::record_canonical(vec![
            Field::new(120, Value::signed(1)),
            Field::new(121, Value::signed(1)),
            Field::new(122, Value::signed(1)),
        ]))
        .canonical_bytes());
        let bare_state = ok(ok(Value::record_canonical(vec![Field::new(
            110,
            Value::signed(3),
        )]))
        .canonical_bytes());
        assert_eq!(
            (
                sizes.command,
                sizes.state,
                sizes.authorization,
                sizes.bundle,
                sizes.receipt,
                sizes.outbox
            ),
            (
                bare_command.len(),
                bare_state.len(),
                record.len(),
                expected.len(),
                stored_receipt.len(),
                lanes[1].len()
            )
        );
        assert_eq!(
            ok(sizes.total()),
            bare_command.len() + bare_state.len() + record.len() + expected.len()
        );
        assert!(ok(sizes.total()) <= 16_384);
        let binding = ok(connection.query_row(
            "SELECT identity FROM v2_genesis WHERE singleton=1",
            [],
            |row| row.get::<_, Vec<u8>>(0),
        ));
        assert_eq!(binding, h.identity());
        assert_eq!(
            ok(expand_publication(&binding, &record)),
            publication(h, raw(&pre, &cmd, &ctx)).subject()
        );
        println!(
            "scoped_full_accounting command={} state={} authorization={} bundle={} receipt_in_bundle={} outbox_in_bundle={} aggregate={} stored_H={} full_publication={}",
            sizes.command,
            sizes.state,
            sizes.authorization,
            sizes.bundle,
            sizes.receipt,
            sizes.outbox,
            ok(sizes.total()),
            binding.len(),
            publication(h, raw(&pre, &cmd, &ctx)).subject().len()
        );
    });
}

#[test]
fn fresh_allowed_operation_after_recurrent_head_has_a_distinct_delivery_id() {
    with_authority(None, |a, h| {
        let zero = state(0);
        let yes = context(true);
        let mut db = ok(V2SqliteShell::create_in_memory(a, genesis(h, &zero)));
        for (version, count, deltas) in [(0, 0, [1, 0, 0]), (1, 1, [-1, 0, 0]), (2, 0, [1, 0, 0])] {
            let pre = state(count);
            let cmd = command(deltas);
            let p = publication(h, raw(&pre, &cmd, &yes));
            assert_eq!(ok(p.evaluation().result()).class(), Class::Accept);
            let key = Hash32::new([ok(u8::try_from(version + 1)); 32]);
            let required = ok(ok(db.publication_bundle(version, key, &p)).sizes().total());
            assert!(required <= 16_384);
            assert_eq!(
                ok(db.commit_bounded_at(version, key, p, required, None)).status(),
                CommitStatus::Committed
            );
        }
        let final_state = ok(db.snapshot());
        assert_eq!(final_state.state(), state(1));
        assert_eq!((final_state.version(), final_state.pending()), (3, 3));
        let mut identities = std::collections::BTreeSet::new();
        let mut memory = MemoryDestination::default();
        while let Some(pending) = ok(db.next_pending()) {
            assert!(identities.insert(pending.delivery().delivery_id()));
            ok(ok(pending.deliver(&mut memory)).acknowledge());
        }
        assert_eq!(identities.len(), 3);
        assert_eq!(memory.delivered_count(), 3);
        assert_eq!(ok(db.snapshot()).pending(), 0);
        ok(db.audit());
    });
}

#[test]
fn snapshot_captures_actual_history_and_durable_counts_with_the_checked_head() {
    with_authority(None, |a, h| {
        let path = Temp::new();
        let zero = state(0);
        let yes = context(true);
        let mut first = ok(V2SqliteShell::create(&path.0, a, genesis(h, &zero)));
        let original = ok(first.snapshot());
        let storage = ok(Connection::open(&path.0));
        let actual_h: Vec<u8> = ok(storage.query_row(
            "SELECT identity FROM v2_genesis WHERE singleton=1",
            [],
            |row| row.get(0),
        ));
        assert_eq!(original.binding(), actual_h);
        assert_eq!(original.binding(), a.identity());
        assert_eq!(original.state(), zero);
        assert_eq!(
            (
                original.version(),
                original.bundle_count(),
                original.replay_count(),
                original.pending()
            ),
            (0, 0, 0, 0)
        );

        let increment = command([1, 0, 0]);
        ok(first.commit_at(
            0,
            Hash32::new([41; 32]),
            publication(h, raw(&zero, &increment, &yes)),
        ));
        let after_first = ok(first.snapshot());
        assert_eq!(after_first.binding(), actual_h);
        assert_eq!(
            (
                after_first.version(),
                after_first.bundle_count(),
                after_first.replay_count(),
                after_first.pending()
            ),
            (1, 1, 1, 1)
        );

        let mut second = ok(V2SqliteShell::open(&path.0, a));
        let one = state(1);
        let decrement = command([-1, 0, 0]);
        ok(second.commit_at(
            1,
            Hash32::new([42; 32]),
            publication(h, raw(&one, &decrement, &yes)),
        ));
        let after_second = ok(first.snapshot());
        let actual_counts: (i64, i64, i64) = ok(storage.query_row(
            "SELECT count(*),count(DISTINCT replay_id),(SELECT count(*) FROM v2_deliveries WHERE acknowledged=0) FROM v2_commits",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        ));
        assert_eq!(actual_counts, (2, 2, 2));
        assert_eq!(
            (
                after_second.version(),
                after_second.bundle_count(),
                after_second.replay_count(),
                after_second.pending()
            ),
            (2, 2, 2, 2)
        );
        assert_eq!(after_second.state(), zero);
        assert_eq!(after_second.root(), original.root());
        assert_ne!(after_second.version(), original.version());
        assert_eq!(after_second.binding(), actual_h);

        ok(storage.execute(
            "UPDATE v2_genesis SET identity=substr(identity,1,length(identity)-1)",
            [],
        ));
        assert!(first.snapshot().is_err());
        assert!(second.snapshot().is_err());
        assert_eq!(original.binding(), actual_h);
        assert_eq!(original.state(), zero);
        assert_eq!(
            (
                original.version(),
                original.bundle_count(),
                original.replay_count(),
                original.pending()
            ),
            (0, 0, 0, 0)
        );
    });
}
