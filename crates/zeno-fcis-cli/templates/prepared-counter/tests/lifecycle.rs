#![cfg(feature = "sqlite")]
//! Original decision/admission controls plus independent checked preparation/lifecycle gates.
//! All ten original positive/negative native regressions remain private and unmodified.
use prepared_counter::{
    authority, create,
    generated::*,
    prepare::{MAX_PUBLICATION_BYTES, PreparedBatch, command},
    v2_contract,
};
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};
use zeno_fcis_codec::CanonicalEncode;
use zeno_fcis_crypto::RustCryptoSha256;
use zeno_fcis_schema::ValidationLimits;
use zeno_fcis_synthesis::finite::{
    v2_authority::PublicationOutcome,
    v2_composition::{Class, Raw},
};
static SEQUENCE: AtomicU64 = AtomicU64::new(0);
struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        let p = std::env::temp_dir().join(format!(
            "prepared-v2-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&p).unwrap();
        Self(p)
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
#[test]
fn normal_complete_original_216_decisions_preserve_class_reason_state_and_notice() {
    let data = v2_contract::Contract::new();
    let definition = data.descriptor();
    let authority = authority(&definition).unwrap();
    let project =
        prepared_counter::bindings::GeneratedProject::try_new::<RustCryptoSha256>().unwrap();
    let mut cases = 0;
    let mut publications = 0;
    let mut rejections = 0;
    for count in 0..=3 {
        for a in -1..=1 {
            for b in -1..=1 {
                for c in -1..=1 {
                    for allowed in [false, true] {
                        let pre = CounterState {
                            count: CounterValue(count),
                        };
                        let cmd = command([a, b, c]);
                        let ctx = CounterContext(allowed);
                        let state = project
                            .admit_root::<RustCryptoSha256>(&pre, ValidationLimits::default())
                            .unwrap()
                            .envelope()
                            .canonical_bytes()
                            .unwrap();
                        let command = project
                            .admit_command::<RustCryptoSha256>(&cmd, ValidationLimits::default())
                            .unwrap()
                            .admitted()
                            .envelope()
                            .canonical_bytes()
                            .unwrap();
                        let context = project
                            .admit_context::<RustCryptoSha256>(&ctx, ValidationLimits::default())
                            .unwrap()
                            .admitted()
                            .envelope()
                            .canonical_bytes()
                            .unwrap();
                        let mut expected = count;
                        let valid = [a, b, c].into_iter().all(|delta| {
                            expected += delta;
                            (0..=3).contains(&expected)
                        });
                        match authority.publish(Raw {
                            state: &state,
                            command: &command,
                            context: &context,
                        }) {
                            PublicationOutcome::Reject(evaluation) => {
                                let result = evaluation.result().unwrap();
                                assert!(!allowed || !valid);
                                assert_eq!(result.class(), Class::Reject);
                                assert_eq!(result.reason(), Some(if allowed { 201 } else { 200 }));
                                assert!(
                                    result.post().is_empty()
                                        && result.patch().is_empty()
                                        && result.effects().is_empty()
                                        && result.outbox().is_empty()
                                );
                                rejections += 1;
                            }
                            PublicationOutcome::Commit(publication) => {
                                assert!(allowed && valid);
                                let result = publication.evaluation().result().unwrap();
                                assert_eq!(result.class(), Class::Accept);
                                assert_eq!(result.reason(), None);
                                let post = CounterState {
                                    count: CounterValue(expected),
                                };
                                let envelope = project
                                    .admit_root::<RustCryptoSha256>(
                                        &post,
                                        ValidationLimits::default(),
                                    )
                                    .unwrap()
                                    .envelope()
                                    .canonical_bytes()
                                    .unwrap();
                                assert_eq!(publication.poststate(), envelope);
                                assert!(publication.effects().is_empty());
                                assert_eq!(publication.outbox().len(), 1);
                                let notice = &publication.outbox()[0];
                                assert_eq!(
                                    (
                                        notice.ordinal(),
                                        notice.channel(),
                                        notice.destination_root(),
                                        notice.payload_root()
                                    ),
                                    (0, 300, 103, 104)
                                );
                                assert_eq!(
                                    notice.destination(),
                                    NotificationDestination("local-observer".into())
                                        .to_value()
                                        .unwrap()
                                        .canonical_bytes()
                                        .unwrap()
                                );
                                assert_eq!(
                                    notice.payload(),
                                    Notification {
                                        notified_count: CounterValue(expected)
                                    }
                                    .to_value()
                                    .unwrap()
                                    .canonical_bytes()
                                    .unwrap()
                                );
                                assert_eq!(
                                    notice.idempotency(),
                                    zeno_fcis_value::Value::unsigned(0)
                                        .canonical_bytes()
                                        .unwrap()
                                );
                                publications += 1;
                            }
                            other => panic!("supported original command refused: {other:?}"),
                        }
                        cases += 1;
                    }
                }
            }
        }
    }
    assert_eq!((cases, publications, rejections), (216, 68, 148));
    println!(
        "original_normal_core_decisions={cases} complete_publications={publications} rejections={rejections}; original_decision_domain_preserved=true"
    );
}
#[test]
fn shell_scratch_is_ordered_atomic_cancellable_and_schema_bounded() {
    let temp = Temp::new();
    let data = v2_contract::Contract::new();
    let definition = data.descriptor();
    let authority = authority(&definition).unwrap();
    let mut shell = create(&temp.0.join("state.sqlite"), &authority).unwrap();
    let before = shell.snapshot().unwrap();
    let mut bad =
        PreparedBatch::start(&authority, &before, &command([1, -1, -1]), true, "bad").unwrap();
    assert!(bad.advance(0, 3).is_err());
    assert_eq!(bad.processed_items(), 0);
    bad.advance(0, 1).unwrap();
    assert_eq!(bad.processed_items(), 1);
    assert!(bad.advance(0, 1).is_err());
    assert!(bad.advance(1, 0).is_err());
    assert!(bad.advance(1, 3).is_err());
    assert_eq!(bad.processed_items(), 1);
    assert!(
        bad.publish(&mut shell, &authority, true, MAX_PUBLICATION_BYTES, None)
            .is_err()
    );
    assert!(
        PreparedBatch::start(&authority, &before, &command([-1, 0, 0]), false, "denied").is_err()
    );
    assert!(PreparedBatch::start(&authority, &before, &command([2, 0, 0]), true, "wide").is_err());
    let mut cancelled =
        PreparedBatch::start(&authority, &before, &command([1, 1, 1]), true, "cancel").unwrap();
    cancelled.advance(0, 1).unwrap();
    drop(cancelled);
    assert_eq!(shell.snapshot().unwrap(), before);
}
#[test]
fn original_successful_partitions_restore_complete_bounded_schema9_publication() {
    use prepared_counter::decode_state;
    use zeno_fcis_synthesis::finite::V2Resource as R;
    let data = v2_contract::Contract::new();
    let definition = data.descriptor();
    let authority = authority(&definition).unwrap();
    let mut original_record = None;
    for partition in [vec![3], vec![1, 2], vec![2, 1], vec![1, 1, 1]] {
        let temp = Temp::new();
        let mut shell = create(&temp.0.join("state.sqlite"), &authority).unwrap();
        let before = shell.snapshot().unwrap();
        let mut work = PreparedBatch::start(
            &authority,
            &before,
            &command([1, 1, 1]),
            true,
            "original-positive",
        )
        .unwrap();
        let reference = PreparedBatch::start(
            &authority,
            &before,
            &command([1, 1, 1]),
            true,
            "original-positive",
        )
        .unwrap();
        for (resource, amount) in [
            (R::Read, 6),
            (R::Write, 3),
            (R::Candidate, 3),
            (R::Byte, 120),
            (R::Effect, 0),
            (R::WitnessByte, 0),
            (R::Depth, 0),
            (R::Step, 0),
        ] {
            assert_eq!(work.reserved_budget().used(resource), amount);
        }
        let mut offset = 0;
        for n in partition {
            work.advance(offset, n).unwrap();
            offset += n;
            assert_eq!(shell.snapshot().unwrap(), before);
            assert_eq!(work.processed_items(), offset);
            assert_eq!(work.remaining_items(), 3 - offset);
            assert_eq!(work.usage().used(R::Step), u64::from(offset) * 3);
            assert!(work.same_operation(&reference));
        }
        let result = work
            .publish(&mut shell, &authority, true, MAX_PUBLICATION_BYTES, None)
            .unwrap();
        assert_eq!(result.status, zeno_fcis_shell::CommitStatus::Committed);
        assert!(
            result.sizes.receipt > 0 && result.sizes.outbox > 0 && result.sizes.authorization > 0
        );
        assert!(result.sizes.bundle > result.sizes.receipt + result.sizes.outbox);
        assert!(result.sizes.total().unwrap() <= MAX_PUBLICATION_BYTES);
        if let Some(record) = &original_record {
            assert_eq!(&result.authorization, record);
        } else {
            original_record = Some(result.authorization.clone());
        }
        let after = shell.snapshot().unwrap();
        assert_eq!(decode_state(after.state()).unwrap().count.0, 3);
        assert_eq!(
            (
                after.version(),
                after.bundle_count(),
                after.replay_count(),
                after.pending()
            ),
            (1, 1, 1, 1)
        );
        println!(
            "original_positive_persistence=PASS schema=9 command={} state={} authorization={} bundle={} receipt_in_bundle={} outbox_in_bundle={} aggregate={} cap={}",
            result.sizes.command,
            result.sizes.state,
            result.sizes.authorization,
            result.sizes.bundle,
            result.sizes.receipt,
            result.sizes.outbox,
            result.sizes.total().unwrap(),
            MAX_PUBLICATION_BYTES
        );
    }
}

fn complete(
    shell: &mut prepared_counter::Shell<'_, '_>,
    authority: &prepared_counter::Authority<'_>,
    deltas: [i128; 3],
    replay: &str,
) -> PreparedBatch {
    let mut work = PreparedBatch::start(
        authority,
        &shell.snapshot().unwrap(),
        &command(deltas),
        true,
        replay,
    )
    .unwrap();
    work.advance(0, 3).unwrap();
    work
}
fn reach(
    shell: &mut prepared_counter::Shell<'_, '_>,
    authority: &prepared_counter::Authority<'_>,
    count: i128,
) {
    for step in 0..count {
        complete(shell, authority, [1, 0, 0], &format!("setup-{step}"))
            .publish(shell, authority, true, MAX_PUBLICATION_BYTES, None)
            .unwrap();
    }
}
fn original_frames(
    snapshot: &zeno_fcis_shell_sqlite::v2::Snapshot,
    cmd: &CounterCommand,
    allowed: bool,
) -> (Vec<u8>, Vec<u8>, Vec<u8>) {
    let project =
        prepared_counter::bindings::GeneratedProject::try_new::<RustCryptoSha256>().unwrap();
    (
        snapshot.state().to_vec(),
        project
            .admit_command::<RustCryptoSha256>(cmd, ValidationLimits::default())
            .unwrap()
            .admitted()
            .envelope()
            .canonical_bytes()
            .unwrap(),
        project
            .admit_context::<RustCryptoSha256>(
                &CounterContext(allowed),
                ValidationLimits::default(),
            )
            .unwrap()
            .admitted()
            .envelope()
            .canonical_bytes()
            .unwrap(),
    )
}

#[test]
fn publication_replay_retains_original_binding_and_refuses_different_history() {
    use zeno_fcis_shell::CommitStatus;
    use zeno_fcis_synthesis::finite::{
        V2Resource as Resource, canonical_v2::schema, v2_authority as a, v2_catalog as catalog,
    };

    let temp = Temp::new();
    let source_contract = v2_contract::Contract::new();
    let source_definition = source_contract.descriptor();
    let source_authority = authority(&source_definition).unwrap();
    let destination_contract = v2_contract::Contract::new();
    let mut destination_definition = destination_contract.descriptor();
    destination_definition.limits = destination_definition
        .limits
        .with_limit(Resource::Step, 243);
    let destination_policy = a::policy_bytes(
        &destination_definition,
        v2_contract::ORIGINAL_SCHEMA,
        &v2_contract::FRAMING,
        v2_contract::CHANNEL_ROOTS,
    )
    .unwrap();
    let destination_catalog = catalog::bind_original(
        v2_contract::ORIGINAL_SCHEMA,
        &v2_contract::DESCRIPTION,
        catalog::Limits {
            schema: schema::Limits {
                bytes: v2_contract::ORIGINAL_SCHEMA.len() as u64,
                types: 7,
                fields: 3,
                variants: 0,
            },
            contract_bytes: destination_policy.len() as u64,
        },
        &destination_policy,
        &destination_definition,
        &v2_contract::FRAMING,
        v2_contract::CHANNEL_ROOTS,
    )
    .unwrap();
    let destination_authority = a::bind(&destination_catalog).unwrap();
    assert_ne!(
        source_authority.identity(),
        destination_authority.identity()
    );

    let mut source = create(&temp.0.join("source.sqlite"), &source_authority).unwrap();
    let source_before = source.snapshot().unwrap();
    let frames = original_frames(&source_before, &command([1, 1, 1]), true);
    let publication = complete(&mut source, &source_authority, [1, 1, 1], "source-record")
        .publish(
            &mut source,
            &source_authority,
            true,
            MAX_PUBLICATION_BYTES,
            None,
        )
        .unwrap();
    let source_after = source.snapshot().unwrap();
    let mut destination =
        create(&temp.0.join("destination.sqlite"), &destination_authority).unwrap();
    let destination_before = destination.snapshot().unwrap();
    assert_ne!(source_before.binding(), destination_before.binding());
    assert_eq!(source_before.state(), destination_before.state());
    assert!(matches!(
        prepared_counter::Shell::open(temp.0.join("source.sqlite"), &destination_authority),
        Err(zeno_fcis_shell_sqlite::v2::Error::Identity)
    ));
    let replay =
        prepared_counter::profile::digest("example/prepared-counter/replay", b"source-record");
    let a::PublicationOutcome::Commit(source_capability) = source_authority.publish(Raw {
        state: &frames.0,
        command: &frames.1,
        context: &frames.2,
    }) else {
        panic!("source publication");
    };
    assert!(matches!(
        destination.commit_at(0, replay, source_capability),
        Err(zeno_fcis_shell_sqlite::v2::Error::Identity)
    ));
    assert_eq!(destination.snapshot().unwrap(), destination_before);
    assert!(
        publication
            .replay(&mut destination, &destination_authority)
            .is_err()
    );
    assert!(
        publication
            .replay(&mut destination, &source_authority)
            .is_err()
    );
    assert_eq!(destination.snapshot().unwrap(), destination_before);
    assert!(
        publication
            .replay(&mut source, &destination_authority)
            .is_err()
    );
    assert_eq!(source.snapshot().unwrap(), source_after);
    assert_eq!(
        publication
            .replay(&mut source, &source_authority)
            .unwrap()
            .status(),
        CommitStatus::IdempotentReplay
    );
    assert_eq!(source.snapshot().unwrap(), source_after);
}

#[test]
fn all_original_216_cases_use_the_bound_authority_and_each_success_keeps_the_one_under_gate() {
    use zeno_fcis_synthesis::finite::v2_authority::PublicationOutcome as H;
    let data = v2_contract::Contract::new();
    let definition = data.descriptor();
    let authority = authority(&definition).unwrap();
    let mut cases = 0;
    let mut committing = 0;
    let mut refused = 0;
    let mut maximum = 0;
    for count in 0..=3 {
        for a in -1..=1 {
            for b in -1..=1 {
                for c in -1..=1 {
                    for allowed in [false, true] {
                        let temp = Temp::new();
                        let mut shell = create(&temp.0.join("state.sqlite"), &authority).unwrap();
                        reach(&mut shell, &authority, count);
                        let before = shell.snapshot().unwrap();
                        let cmd = command([a, b, c]);
                        let frames = original_frames(&before, &cmd, allowed);
                        assert_eq!(authority.identity(), before.binding());
                        let evaluated = authority.publish(Raw {
                            state: &frames.0,
                            command: &frames.1,
                            context: &frames.2,
                        });
                        let mut expected = count;
                        let valid = [a, b, c].into_iter().all(|delta| {
                            expected += delta;
                            (0..=3).contains(&expected)
                        });
                        match evaluated {
                            H::Reject(evaluation) => {
                                let result = evaluation.result().unwrap();
                                assert!(!allowed || !valid);
                                assert_eq!(result.class(), Class::Reject);
                                assert_eq!(result.reason(), Some(if allowed { 201 } else { 200 }));
                                assert!(
                                    result.post().is_empty()
                                        && result.patch().is_empty()
                                        && result.effects().is_empty()
                                        && result.outbox().is_empty()
                                );
                                if allowed {
                                    let mut work = PreparedBatch::start(
                                        &authority, &before, &cmd, true, "case",
                                    )
                                    .unwrap();
                                    assert!(work.advance(0, 3).is_err());
                                    assert_eq!(work.processed_items(), 0);
                                    assert!(
                                        work.publish(
                                            &mut shell,
                                            &authority,
                                            true,
                                            MAX_PUBLICATION_BYTES,
                                            None
                                        )
                                        .is_err()
                                    );
                                } else {
                                    assert!(
                                        PreparedBatch::start(
                                            &authority, &before, &cmd, false, "case"
                                        )
                                        .is_err()
                                    );
                                }
                                assert_eq!(shell.snapshot().unwrap(), before);
                                refused += 1;
                            }
                            H::Commit(publication) => {
                                assert!(allowed && valid);
                                assert_eq!(
                                    publication.evaluation().result().unwrap().class(),
                                    Class::Accept
                                );
                                let replay = prepared_counter::profile::digest(
                                    "example/prepared-counter/replay",
                                    b"case",
                                );
                                let artifact = shell
                                    .publication_bundle(before.version(), replay, &publication)
                                    .unwrap();
                                let sizes = artifact.sizes();
                                let required = sizes.total().unwrap();
                                assert!(required <= MAX_PUBLICATION_BYTES);
                                assert_eq!(
                                    sizes.command,
                                    cmd.to_value().unwrap().canonical_bytes().unwrap().len()
                                );
                                assert_eq!(
                                    sizes.state,
                                    CounterState {
                                        count: CounterValue(expected)
                                    }
                                    .to_value()
                                    .unwrap()
                                    .canonical_bytes()
                                    .unwrap()
                                    .len()
                                );
                                let compact = zeno_fcis_shell_sqlite::v2::compact_publication(
                                    publication.identity(),
                                    publication.subject(),
                                )
                                .unwrap();
                                assert_eq!(sizes.authorization, compact.len());
                                assert_eq!(sizes.bundle, artifact.canonical_bytes().len());
                                assert_eq!(sizes.receipt, artifact.receipt().len());
                                assert_eq!(sizes.outbox, artifact.outbox().len());
                                assert!(sizes.bundle > sizes.receipt + sizes.outbox);
                                let mut below =
                                    PreparedBatch::start(&authority, &before, &cmd, true, "case")
                                        .unwrap();
                                below.advance(0, 3).unwrap();
                                let error = below
                                    .publish(&mut shell, &authority, true, required - 1, None)
                                    .unwrap_err();
                                assert!(error.starts_with("publication capacity:"), "{error}");
                                assert_eq!(shell.snapshot().unwrap(), before);
                                let mut exact =
                                    PreparedBatch::start(&authority, &before, &cmd, true, "case")
                                        .unwrap();
                                exact.advance(0, 3).unwrap();
                                let actual = exact
                                    .publish(&mut shell, &authority, true, required, None)
                                    .unwrap();
                                assert_eq!(actual.authorization, compact);
                                assert_eq!(actual.sizes, sizes);
                                let after = shell.snapshot().unwrap();
                                assert_eq!(
                                    prepared_counter::decode_state(after.state())
                                        .unwrap()
                                        .count
                                        .0,
                                    expected
                                );
                                assert_eq!(
                                    (
                                        after.version(),
                                        after.bundle_count(),
                                        after.replay_count(),
                                        after.pending()
                                    ),
                                    (
                                        count as u64 + 1,
                                        count as u64 + 1,
                                        count as u64 + 1,
                                        count as u64 + 1
                                    )
                                );
                                assert_eq!(after.binding(), before.binding());
                                assert!(
                                    actual.sizes.receipt > 0
                                        && actual.sizes.outbox > 0
                                        && actual.sizes.authorization > 0
                                );
                                maximum = maximum.max(required);
                                committing += 1;
                            }
                            other => panic!("original scoped decision refused: {other:?}"),
                        }
                        cases += 1;
                    }
                }
            }
        }
    }
    assert_eq!((cases, committing, refused), (216, 68, 148));
    println!(
        "original_scoped_prepared_cases={cases} actual_commits={committing} every_success_one_under_refused={committing} maximum_aggregate={maximum} original_cap={MAX_PUBLICATION_BYTES}"
    );
}

#[test]
fn original_reservations_and_every_one_under_input_output_or_budget_remain_separate_from_steps() {
    use zeno_fcis_synthesis::finite::{
        Domain, Op, V2Resource as R, v2_continuation as C, v2_zero_limits,
    };
    let make = |input, output, budget| {
        let scalar = Domain::Int { min: 0, max: 3 };
        C::start(
            C::admit_graph(
                vec![scalar, Domain::Int { min: -1, max: 1 }],
                vec![scalar],
                vec![Op::Input(0), Op::Input(1), Op::Add(0, 1)],
                vec![2],
            )
            .unwrap(),
            vec![0],
            vec![vec![1], vec![1], vec![1]],
            C::Context {
                state_root: [1; 32],
                state_version: 0,
                invocation_hash: [2; 32],
            },
            C::PreparationLimits {
                max_items: 3,
                max_chunk_items: 3,
                max_input_bytes: input,
                max_output_bytes: output,
            },
            budget,
        )
    };
    let budget = v2_zero_limits()
        .with_limit(R::Read, 6)
        .with_limit(R::Write, 3)
        .with_limit(R::Candidate, 3)
        .with_limit(R::Byte, 120)
        .with_limit(R::Step, 64);
    for (input, output, ok) in [(97, 22, false), (98, 21, false), (98, 22, true)] {
        assert_eq!(make(input, output, budget).is_ok(), ok);
    }
    for (resource, required) in [
        (R::Read, 6),
        (R::Write, 3),
        (R::Candidate, 3),
        (R::Byte, 120),
    ] {
        assert!(make(98, 22, budget.with_limit(resource, required - 1)).is_err());
        assert!(make(98, 22, budget.with_limit(resource, required)).is_ok());
    }
    for (limit, ok) in [(8, false), (9, true), (64, true)] {
        let mut fold = make(98, 22, budget.with_limit(R::Step, limit)).unwrap();
        assert_eq!(fold.reserved_budget().used(R::Step), 0);
        assert_eq!(fold.advance(0, 3).is_ok(), ok);
        assert_eq!(fold.usage().used(R::Step), limit.min(9));
    }
}

#[test]
fn original_failed_retry_cancellation_incomplete_and_actual_shared_step_exhaustion_publish_nothing()
{
    use zeno_fcis_synthesis::finite::V2Resource as R;
    let temp = Temp::new();
    let data = v2_contract::Contract::new();
    let definition = data.descriptor();
    let authority = authority(&definition).unwrap();
    let mut shell = create(&temp.0.join("state.sqlite"), &authority).unwrap();
    let before = shell.snapshot().unwrap();
    let mut work =
        PreparedBatch::start(&authority, &before, &command([1, -1, -1]), true, "failed").unwrap();
    let original =
        PreparedBatch::start(&authority, &before, &command([1, -1, -1]), true, "failed").unwrap();
    assert!(work.advance(0, 3).is_err());
    assert_eq!(work.processed_items(), 0);
    assert_eq!(work.usage().used(R::Step), 9);
    assert!(work.same_operation(&original));
    work.advance(0, 1).unwrap();
    assert_eq!(work.processed_items(), 1);
    assert_eq!(work.usage().used(R::Step), 12);
    for _ in 0..12 {
        assert!(work.advance(1, 2).is_err());
        assert_eq!(work.processed_items(), 1);
        assert!(work.same_operation(&original));
    }
    assert_eq!(work.usage().used(R::Step), 64);
    for (offset, count) in [(0, 1), (1, 0), (1, 3), (1, u32::MAX)] {
        assert!(work.advance(offset, count).is_err());
        assert_eq!(work.usage().used(R::Step), 64);
    }
    for (resource, amount) in [
        (R::Read, 6),
        (R::Write, 3),
        (R::Candidate, 3),
        (R::Byte, 120),
    ] {
        assert_eq!(work.usage().used(resource), amount);
    }
    assert!(
        work.publish(&mut shell, &authority, true, MAX_PUBLICATION_BYTES, None)
            .unwrap_err()
            .contains("Incomplete")
    );
    assert_eq!(shell.snapshot().unwrap(), before);
    for prefix in 0..=3 {
        let mut cancelled =
            PreparedBatch::start(&authority, &before, &command([1, 1, 1]), true, "cancel").unwrap();
        if prefix > 0 {
            cancelled.advance(0, prefix).unwrap();
        }
        drop(cancelled);
        assert_eq!(shell.snapshot().unwrap(), before);
        let restarted = complete(&mut shell, &authority, [1, 1, 1], "cancel");
        assert_eq!(restarted.usage().used(R::Step), 9);
        drop(restarted);
    }
}

#[test]
fn original_context_head_aba_and_second_handle_freshness_are_checked_before_publication() {
    let temp = Temp::new();
    let path = temp.0.join("state.sqlite");
    let data = v2_contract::Contract::new();
    let definition = data.descriptor();
    let authority = authority(&definition).unwrap();
    let mut first = create(&path, &authority).unwrap();
    let original = first.snapshot().unwrap();
    let wrong = complete(&mut first, &authority, [1, 0, 0], "changed-context");
    assert!(
        wrong
            .publish(&mut first, &authority, false, MAX_PUBLICATION_BYTES, None)
            .is_err()
    );
    assert_eq!(first.snapshot().unwrap(), original);
    let stale = complete(&mut first, &authority, [1, 0, 0], "stale");
    let aba = complete(&mut first, &authority, [1, 0, 0], "aba");
    let mut second = prepared_counter::Shell::open(&path, &authority).unwrap();
    let other = complete(&mut second, &authority, [1, 0, 0], "second-handle");
    let first_result = complete(&mut first, &authority, [1, 0, 0], "inc")
        .publish(&mut first, &authority, true, MAX_PUBLICATION_BYTES, None)
        .unwrap();
    let before = first.snapshot().unwrap();
    assert!(
        stale
            .publish(&mut first, &authority, true, MAX_PUBLICATION_BYTES, None)
            .is_err()
    );
    assert_eq!(first.snapshot().unwrap(), before);
    assert!(
        other
            .publish(&mut second, &authority, true, MAX_PUBLICATION_BYTES, None)
            .is_err()
    );
    assert_eq!(first.snapshot().unwrap(), before);
    complete(&mut first, &authority, [-1, 0, 0], "dec")
        .publish(&mut first, &authority, true, MAX_PUBLICATION_BYTES, None)
        .unwrap();
    let recurrent = first.snapshot().unwrap();
    assert_eq!(recurrent.root(), original.root());
    assert_ne!(recurrent.version(), original.version());
    assert!(
        aba.publish(&mut first, &authority, true, MAX_PUBLICATION_BYTES, None)
            .is_err()
    );
    assert_eq!(first.snapshot().unwrap(), recurrent);
    let fresh = complete(&mut first, &authority, [1, 0, 0], "fresh")
        .publish(&mut first, &authority, true, MAX_PUBLICATION_BYTES, None)
        .unwrap();
    assert_eq!(fresh.status, zeno_fcis_shell::CommitStatus::Committed);
    assert_eq!(first.snapshot().unwrap().version(), 3);
    assert_eq!(
        first_result
            .replay(&mut first, &authority)
            .unwrap()
            .status(),
        zeno_fcis_shell::CommitStatus::IdempotentReplay
    );
    let mut destination = zeno_fcis_shell_sqlite::MemoryDestination::default();
    while let Some(pending) = first.next_pending().unwrap() {
        pending
            .deliver(&mut destination)
            .unwrap()
            .acknowledge()
            .unwrap();
    }
    assert_eq!(destination.delivered_count(), 3);
    assert_eq!(first.snapshot().unwrap().pending(), 0);
    drop(first);
    drop(second);
    let mut reopened = prepared_counter::Shell::open(&path, &authority).unwrap();
    assert_eq!(
        (
            reopened.snapshot().unwrap().version(),
            reopened.snapshot().unwrap().bundle_count()
        ),
        (3, 3)
    );
}

#[test]
fn every_original_commit_crash_recovers_all_or_none_through_the_new_checked_path() {
    use zeno_fcis_shell_sqlite::CrashPoint as P;
    let data = v2_contract::Contract::new();
    let definition = data.descriptor();
    let authority = authority(&definition).unwrap();
    for crash in [
        P::BeforeTransaction,
        P::AfterValidation,
        P::AfterStateWrite,
        P::AfterReplayWrite,
        P::AfterOutboxWrite,
        P::BeforeCommit,
        P::AfterCommit,
    ] {
        let temp = Temp::new();
        let path = temp.0.join("state.sqlite");
        let mut shell = create(&path, &authority).unwrap();
        let before = shell.snapshot().unwrap();
        assert!(
            complete(&mut shell, &authority, [1, 1, 1], "crash")
                .publish(
                    &mut shell,
                    &authority,
                    true,
                    MAX_PUBLICATION_BYTES,
                    Some(crash)
                )
                .is_err()
        );
        drop(shell);
        let mut reopened = prepared_counter::Shell::open(&path, &authority).unwrap();
        let after = reopened.snapshot().unwrap();
        if crash == P::AfterCommit {
            assert_eq!(
                (
                    after.version(),
                    after.bundle_count(),
                    after.replay_count(),
                    after.pending()
                ),
                (1, 1, 1, 1)
            );
            assert_eq!(
                prepared_counter::decode_state(after.state())
                    .unwrap()
                    .count
                    .0,
                3
            );
            let mut destination = zeno_fcis_shell_sqlite::MemoryDestination::default();
            let pending = reopened.next_pending().unwrap().unwrap();
            drop(pending.deliver(&mut destination).unwrap());
            assert_eq!(destination.delivered_count(), 1);
            drop(reopened);
            let mut reopened = prepared_counter::Shell::open(&path, &authority).unwrap();
            let pending = reopened.next_pending().unwrap().unwrap();
            pending
                .deliver(&mut destination)
                .unwrap()
                .acknowledge()
                .unwrap();
            assert!(reopened.next_pending().unwrap().is_none());
            assert_eq!(destination.delivered_count(), 1);
            assert_eq!(reopened.snapshot().unwrap().pending(), 0);
        } else {
            assert_eq!(after, before);
        }
    }
}

#[test]
fn original_exact_capacity_and_complete_restart_replay_delivery_journey_are_restored() {
    let temp = Temp::new();
    let data = v2_contract::Contract::new();
    let definition = data.descriptor();
    let authority = authority(&definition).unwrap();
    let mut measure = create(&temp.0.join("measure.sqlite"), &authority).unwrap();
    let actual = complete(&mut measure, &authority, [1, 1, 1], "same")
        .publish(&mut measure, &authority, true, MAX_PUBLICATION_BYTES, None)
        .unwrap();
    let limit = actual.sizes.total().unwrap();
    let mut boundary = create(&temp.0.join("boundary.sqlite"), &authority).unwrap();
    let before = boundary.snapshot().unwrap();
    let error = complete(&mut boundary, &authority, [1, 1, 1], "same")
        .publish(&mut boundary, &authority, true, limit - 1, None)
        .unwrap_err();
    assert!(error.starts_with("publication capacity:"));
    assert_eq!(boundary.snapshot().unwrap(), before);
    complete(&mut boundary, &authority, [1, 1, 1], "same")
        .publish(&mut boundary, &authority, true, limit, None)
        .unwrap();
    assert_eq!(measure.snapshot().unwrap(), boundary.snapshot().unwrap());
    let path = temp.0.join("journey.sqlite");
    let output = prepared_counter::journey(&path).unwrap();
    assert!(output.contains("\"status\":\"passed\""));
    assert!(prepared_counter::journey(&path).is_err());
    println!("original_prepared_journey={output}");
}

#[test]
fn compact_replay_and_malformed_missing_truncated_changed_binding_refuse() {
    use zeno_fcis_shell_sqlite::v2::expand_publication;
    use zeno_fcis_synthesis::finite::v2_authority::PublicationOutcome;
    let temp = Temp::new();
    let data = v2_contract::Contract::new();
    let definition = data.descriptor();
    let authority = authority(&definition).unwrap();
    let mut original = create(&temp.0.join("original.sqlite"), &authority).unwrap();
    let before = original.snapshot().unwrap();
    let frames = original_frames(&before, &command([1, 1, 1]), true);
    let result = complete(&mut original, &authority, [1, 1, 1], "record")
        .publish(&mut original, &authority, true, MAX_PUBLICATION_BYTES, None)
        .unwrap();
    let mut receiving = create(&temp.0.join("receiving.sqlite"), &authority).unwrap();
    let replay = prepared_counter::profile::digest("example/prepared-counter/replay", b"record");
    let raw = || Raw {
        state: &frames.0,
        command: &frames.1,
        context: &frames.2,
    };
    let unchanged = receiving.snapshot().unwrap();
    let compact = &result.authorization;
    let mut changed = compact.clone();
    *changed.last_mut().unwrap() ^= 1;
    for bad in [
        Vec::new(),
        vec![0],
        compact[..compact.len() - 1].to_vec(),
        changed,
    ] {
        if let Ok(full) = expand_publication(before.binding(), &bad) {
            assert!(matches!(
                authority.replay_publication(raw(), &full),
                PublicationOutcome::Refused { .. }
            ));
        }
        assert_eq!(receiving.snapshot().unwrap(), unchanged);
    }
    let mut changed_binding = before.binding().to_vec();
    changed_binding[0] ^= 1;
    for binding in [
        Vec::new(),
        before.binding()[..before.binding().len() - 1].to_vec(),
        changed_binding,
    ] {
        let full = expand_publication(&binding, compact).unwrap();
        assert!(matches!(
            authority.replay_publication(raw(), &full),
            PublicationOutcome::Refused { .. }
        ));
        assert_eq!(receiving.snapshot().unwrap(), unchanged);
    }
    let full = expand_publication(before.binding(), compact).unwrap();
    let mut changed_context = frames.2.clone();
    *changed_context.last_mut().unwrap() ^= 1;
    assert!(matches!(
        authority.replay_publication(
            Raw {
                state: &frames.0,
                command: &frames.1,
                context: &changed_context
            },
            &full
        ),
        PublicationOutcome::Refused { .. }
    ));
    assert_eq!(receiving.snapshot().unwrap(), unchanged);
    let PublicationOutcome::Commit(p) = authority.replay_publication(raw(), &full) else {
        panic!("exact replay");
    };
    assert_eq!(
        receiving.commit_at(0, replay, p).unwrap().status(),
        zeno_fcis_shell::CommitStatus::Committed
    );
    assert_eq!(receiving.snapshot().unwrap(), original.snapshot().unwrap());
    let PublicationOutcome::Commit(p) = authority.replay_publication(raw(), &full) else {
        panic!("exact retry");
    };
    assert!(matches!(
        receiving.commit_at(0, replay, p),
        Err(zeno_fcis_shell_sqlite::v2::Error::Concurrent)
    ));
    let PublicationOutcome::Commit(p) = authority.replay_publication(raw(), &full) else {
        panic!("exact retry");
    };
    assert_eq!(
        receiving.commit_at(1, replay, p).unwrap().status(),
        zeno_fcis_shell::CommitStatus::IdempotentReplay
    );
}

#[test]
fn every_original_declared_state_has_a_checked_eligible_exit_that_publishes_with_the_same_domain() {
    use Op::*;
    use zeno_fcis_synthesis::finite::{
        Domain, Op, Program,
        completion::{CompletionLimits, CompletionProblem, find_completion, verify_completion},
    };
    let scalar = Domain::Int { min: 0, max: 3 };
    let delta = Domain::Int { min: -1, max: 1 };
    let model = Program::try_new(
        vec![scalar, delta, delta, delta],
        vec![Domain::Bool, scalar],
        vec![
            Input(0),
            Input(1),
            Input(2),
            Input(3),
            Add(0, 1),
            Add(4, 2),
            Add(5, 3),
            Int(0),
            Int(3),
            Lt(4, 7),
            Lt(8, 4),
            Not(9),
            Not(10),
            And(11, 12),
            Lt(5, 7),
            Lt(8, 5),
            Not(14),
            Not(15),
            And(16, 17),
            Lt(6, 7),
            Lt(8, 6),
            Not(19),
            Not(20),
            And(21, 22),
            And(13, 18),
            And(24, 23),
            Select(25, 6, 0),
        ],
        vec![25, 26],
    )
    .unwrap();
    let terminal = Program::try_new(
        vec![scalar],
        vec![Domain::Bool],
        vec![Input(0), Int(0), Eq(0, 1)],
        vec![2],
    )
    .unwrap();
    let problem =
        CompletionProblem::try_new(model.clone(), terminal, CompletionLimits::default()).unwrap();
    // The generated-application gate compares this identity with the CLI's
    // `synth completion` result for the same model.
    println!(
        "completion_problem={}",
        problem
            .problem_hash()
            .as_bytes()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>()
    );
    let found = find_completion(&problem).unwrap();
    let verified = verify_completion(&problem, &found).unwrap();
    let data = v2_contract::Contract::new();
    let definition = data.descriptor();
    let authority = authority(&definition).unwrap();
    for count in 0..=3 {
        let exit = verified.next_command(&[count]).unwrap();
        if count == 0 {
            assert!(exit.is_none());
            continue;
        }
        let exit = exit.unwrap();
        assert_eq!(exit.len(), 3);
        assert!(exit.iter().all(|value| (-1..=1).contains(value)));
        let mut input = vec![count];
        input.extend_from_slice(exit);
        assert_eq!(model.evaluate(&input).unwrap(), [1, 0]);
        let temp = Temp::new();
        let mut shell = create(&temp.0.join("state.sqlite"), &authority).unwrap();
        reach(&mut shell, &authority, i128::from(count));
        complete(
            &mut shell,
            &authority,
            [
                i128::from(exit[0]),
                i128::from(exit[1]),
                i128::from(exit[2]),
            ],
            "eligible-exit",
        )
        .publish(&mut shell, &authority, true, MAX_PUBLICATION_BYTES, None)
        .unwrap();
        assert_eq!(
            prepared_counter::decode_state(shell.snapshot().unwrap().state())
                .unwrap()
                .count
                .0,
            0
        );
    }
}
