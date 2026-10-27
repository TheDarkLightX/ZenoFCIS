pub(crate) mod database;
pub(crate) mod fixture;
pub(crate) mod model;
pub(crate) mod schedule;

use database::{Files, Receiver};
use model::{Database, Delivery, Input, Model, Outcome, Refusal};
use rusqlite::Connection;
use schedule::Schedule;
use std::{collections::BTreeMap, path::Path, time::Duration};
use zeno_fcis_codec::{CommitmentHasher, Hash32};
use zeno_fcis_crypto::RustCryptoSha256;
use zeno_fcis_shell::CommitStatus;
use zeno_fcis_shell_sqlite::{
    CrashPoint,
    v2::{CommitReceipt, Error, V2SqliteShell, relay},
};
use zeno_fcis_synthesis::finite::{
    v2_authority::{Authority, PublicationOutcome},
    v2_composition::Raw,
};

pub(crate) fn ok<T, E: std::fmt::Debug>(r: Result<T, E>) -> T {
    r.unwrap_or_else(|e| panic!("unexpected refusal: {e:?}"))
}
pub(crate) fn payload_hash(b: &[u8]) -> Hash32 {
    <RustCryptoSha256 as CommitmentHasher>::hash(b)
}
pub(crate) const POINTS: [CrashPoint; 7] = [
    CrashPoint::BeforeTransaction,
    CrashPoint::AfterValidation,
    CrashPoint::AfterStateWrite,
    CrashPoint::AfterReplayWrite,
    CrashPoint::AfterOutboxWrite,
    CrashPoint::BeforeCommit,
    CrashPoint::AfterCommit,
];
pub(crate) fn key(n: u64) -> Hash32 {
    let mut b = [0; 32];
    b[..8].copy_from_slice(&n.to_be_bytes());
    Hash32::new(b)
}
pub(crate) fn outcome(result: Result<CommitReceipt, Error>) -> Outcome {
    match result {
        Ok(r) if r.status() == CommitStatus::Committed => {
            Outcome::Committed(r.version(), r.chain())
        }
        Ok(r) => Outcome::Replay(r.version(), r.chain()),
        Err(e) => Outcome::Refused(refusal(e)),
    }
}
pub(crate) fn refusal(e: Error) -> Refusal {
    match e {
        Error::InjectedCrash(p) => Refusal::Crash(
            POINTS
                .iter()
                .position(|x| *x == p)
                .unwrap_or_else(|| panic!("point")) as u8
                + 1,
        ),
        Error::Concurrent => Refusal::Concurrent,
        Error::PreState => Refusal::PreState,
        Error::Replay => Refusal::Replay,
        Error::UnknownDelivery => Refusal::Unknown,
        Error::AlreadyAcknowledged => Refusal::AlreadyAcknowledged,
        Error::PayloadMismatch => Refusal::PayloadMismatch,
        Error::Sqlite(rusqlite::Error::SqliteFailure(e, _))
            if matches!(
                e.code,
                rusqlite::ErrorCode::DatabaseBusy | rusqlite::ErrorCode::DatabaseLocked
            ) =>
        {
            Refusal::Busy
        }
        e => panic!("unexpected error: {e:?}"),
    }
}
pub(crate) fn commit(
    db: &mut V2SqliteShell<'_, '_>,
    a: &Authority<'_>,
    input: &Input,
    fault: u8,
) -> Outcome {
    let pre = model::state(fixture::schema_hash(), input.before);
    let p = match a.publish(Raw {
        state: &pre,
        command: &input.command,
        context: &input.context,
    }) {
        PublicationOutcome::Commit(p) => p,
        PublicationOutcome::Reject(_) => return Outcome::Refused(Refusal::Domain),
        other => panic!("unexpected publication: {other:?}"),
    };
    outcome(db.commit_with_crash(
        input.key,
        p,
        if (1..=7).contains(&fault) {
            Some(POINTS[fault as usize - 1])
        } else {
            None
        },
    ))
}
pub(crate) fn acknowledge(db: &mut V2SqliteShell<'_, '_>, id: Hash32, hash: Hash32) -> Outcome {
    match relay::acknowledge(db, id, hash) {
        Ok(()) => Outcome::Observed,
        Err(e) => Outcome::Refused(refusal(e)),
    }
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Event {
    pub(crate) clock: u64,
    pub(crate) action: u8,
    pub(crate) request: Vec<u8>,
    pub(crate) key: Option<Hash32>,
    pub(crate) decision: Outcome,
    pub(crate) sequence: u64,
    pub(crate) deliveries: Vec<Hash32>,
}
#[derive(Debug, Eq, PartialEq)]
pub(crate) struct Run {
    pub(crate) events: Vec<Event>,
    pub(crate) database: Database,
    pub(crate) receiver: BTreeMap<Hash32, Vec<u8>>,
    pub(crate) store_bytes: Vec<u8>,
    pub(crate) receiver_bytes: Vec<u8>,
}
pub(crate) fn create<'a, 'p>(files: &Files, a: &'a Authority<'p>) -> V2SqliteShell<'a, 'p> {
    let initial = model::state(fixture::schema_hash(), [0, 0]);
    ok(V2SqliteShell::create(
        &files.store,
        a,
        fixture::genesis(a, &initial),
    ))
}
fn run(a: &Authority<'_>, cases: &[Input], schedule: &Schedule) -> Run {
    let files = Files::memory();
    let mut db = create(&files, a);
    ok(db.set_test_busy_timeout(Duration::ZERO));
    let mut receiver = Receiver::open(&files.receiver);
    let mut model = Model::new(fixture::model_genesis(a));
    let mut queue: Vec<Delivery> = Vec::new();
    let mut events = Vec::new();
    ok(database::compare(
        &model.database(),
        &database::snapshot(&files.store),
    ));
    for (index, step) in schedule.steps.iter().enumerate() {
        let clock = index as u64 + 1;
        let mut request = Vec::new();
        let mut replay_key = None;
        let (expected, actual) = match step.action {
            0..=3 => {
                let old = model.old_input(step.choice);
                let mut variant = 120 + (step.choice % 2) as u16;
                if model.counts[usize::from(variant == 121)] == 3 {
                    variant = 241 - variant;
                }
                let fresh = || fixture::request(cases, key(clock + 100), model.counts, variant);
                let input = match (step.action, old) {
                    (1, Some(old)) => old,
                    (2, Some(_)) => {
                        let old = model.old_input(0).unwrap_or_else(|| panic!("history"));
                        fixture::request(cases, old.key, old.before, 241 - old.variant)
                    }
                    (3, _) if model.counts == [3, 3] => {
                        model.old_input(0).unwrap_or_else(|| panic!("history"))
                    }
                    _ => fresh(),
                };
                request = input.command.clone();
                replay_key = Some(input.key);
                let fault = if step.action == 0 { step.fault } else { 0 };
                let stale = if model.sequence() == 0 {
                    1
                } else {
                    model.sequence() - 1
                };
                let expected = model.commit(
                    input.clone(),
                    fault,
                    if step.action == 3 { Some(stale) } else { None },
                );
                let actual = if step.action == 3 {
                    let mut second = ok(V2SqliteShell::open(&files.store, a));
                    let pre = model::state(fixture::schema_hash(), input.before);
                    outcome(second.commit_at(
                        stale,
                        input.key,
                        fixture::publication(a, &pre, &input.command, &input.context),
                    ))
                } else if fault == 8 {
                    let lock = ok(Connection::open(&files.store));
                    ok(lock.execute_batch("BEGIN IMMEDIATE"));
                    let result = commit(&mut db, a, &input, fault);
                    ok(lock.execute_batch("ROLLBACK"));
                    result
                } else {
                    commit(&mut db, a, &input, fault)
                };
                (expected, actual)
            }
            4 => {
                let actual = ok(db.export_pending());
                queue = model
                    .deliveries
                    .iter()
                    .filter(|d| !d.acknowledged)
                    .cloned()
                    .collect();
                assert_eq!(
                    actual
                        .iter()
                        .map(|d| (d.delivery_id(), d.payload()))
                        .collect::<Vec<_>>(),
                    queue
                        .iter()
                        .map(|d| (d.id, d.payload.as_slice()))
                        .collect::<Vec<_>>(),
                    "export"
                );
                (Outcome::Observed, Outcome::Observed)
            }
            5 | 12 => {
                if !queue.is_empty() {
                    if step.fault % 4 == 2 {
                        queue.reverse();
                    }
                    let position = step.choice as usize % queue.len();
                    let d = if step.action == 12 {
                        queue[position].clone()
                    } else {
                        queue.remove(position)
                    };
                    request = d.payload.clone();
                    replay_key = Some(d.id);
                    if step.fault % 4 != 0 || step.action == 12 {
                        receiver.receive(&d, step.action == 12);
                        model.receiver.insert(d.id, d.payload.clone());
                        if step.fault % 4 == 1 {
                            receiver.receive(&d, false);
                        }
                        if step.action == 12 {
                            drop(receiver);
                            receiver = Receiver::open(&files.receiver);
                        }
                    }
                }
                (Outcome::Observed, Outcome::Observed)
            }
            6..=9 => {
                let candidates: Vec<_> = model
                    .deliveries
                    .iter()
                    .filter(|d| step.action != 8 || d.acknowledged)
                    .collect();
                let d = if candidates.is_empty() {
                    None
                } else {
                    Some(candidates[step.choice as usize % candidates.len()])
                };
                let id = if step.action == 9 {
                    key(u64::MAX)
                } else {
                    d.map_or(key(u64::MAX), |d| d.id)
                };
                let hash = if step.action == 7 {
                    key(u64::MAX)
                } else {
                    d.map_or(key(0), |d| payload_hash(&d.payload))
                };
                request = hash.as_bytes().to_vec();
                replay_key = Some(id);
                (model.acknowledge(id, hash), acknowledge(&mut db, id, hash))
            }
            10 => {
                drop(db);
                db = ok(V2SqliteShell::open(&files.store, a));
                ok(db.set_test_busy_timeout(Duration::ZERO));
                (Outcome::Observed, Outcome::Observed)
            }
            11 => {
                ok(db.audit());
                model.audit();
                (Outcome::Observed, Outcome::Observed)
            }
            _ => panic!("action"),
        };
        let actual_state = database::snapshot(&files.store);
        let expected_state = model.database();
        if let Err(e) = database::compare(&expected_state, &actual_state) {
            panic!("seed={} step={index}: {e}", schedule.seed);
        }
        model.invariants();
        receiver.invariants();
        assert_eq!(receiver.state(), model.receiver, "I6");
        // Projection equality lets the two invariant checks use the same
        // abstract laws; actual state and outcome originate independently.
        let expected_event = Event {
            clock,
            action: step.action,
            request: request.clone(),
            key: replay_key,
            decision: expected,
            sequence: model.sequence(),
            deliveries: model.deliveries.iter().map(|d| d.id).collect(),
        };
        let head = &actual_state.0["v2_state"][0];
        let model::Cell::Int(sequence) = head[1] else {
            panic!("sequence");
        };
        let actual_event = Event {
            clock,
            action: step.action,
            request,
            key: replay_key,
            decision: actual,
            sequence: sequence as u64,
            deliveries: actual_state.0["v2_deliveries"]
                .iter()
                .map(|r| {
                    let model::Cell::Bytes(b) = &r[9] else {
                        panic!("delivery ID");
                    };
                    Hash32::new(ok(b.as_slice().try_into()))
                })
                .collect(),
        };
        assert_eq!(
            expected_event, actual_event,
            "seed={} step={index}: outcome/trace",
            schedule.seed
        );
        events.push(actual_event);
    }
    drop(db);
    drop(receiver);
    Run {
        events,
        database: database::snapshot(&files.store),
        receiver: model.receiver,
        store_bytes: database::snapshot(&files.store).canonical_bytes(),
        receiver_bytes: database::snapshot_matching(&files.receiver, "*").canonical_bytes(),
    }
}
pub(crate) fn replay(schedule: &Schedule) -> Run {
    let mut result = None;
    fixture::with_authority(|a| {
        result = Some(replay_with_authority(a, &fixture::cases(a), schedule));
    });
    result.unwrap_or_else(|| panic!("run"))
}
pub(crate) fn replay_with_authority(
    a: &Authority<'_>,
    cases: &[Input],
    schedule: &Schedule,
) -> Run {
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| run(a, cases, schedule)));
    match result {
        Ok(run) => run,
        Err(original) => {
            let steps = schedule::minimize(schedule.steps.clone(), |steps| {
                let candidate = Schedule {
                    seed: schedule.seed,
                    steps: steps.to_vec(),
                };
                std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| run(a, cases, &candidate)))
                    .is_err()
            });
            let minimized = Schedule {
                seed: schedule.seed,
                steps,
            };
            let path = Path::new(env!("CARGO_MANIFEST_DIR"))
                .join(format!("tests/sim/fixtures/failure-{}.json", schedule.seed));
            ok(std::fs::write(&path, minimized.json()));
            eprintln!(
                "minimized schedule {}: {}",
                path.display(),
                minimized.json()
            );
            std::panic::resume_unwind(original)
        }
    }
}
