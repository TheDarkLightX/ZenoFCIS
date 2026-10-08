//! Installed as a generated application's integration test by
//! tools/check_migration_claims.py. Every catalog and claim below is emitted
//! by the real contract compiler; no replacement evaluator is used.
use std::{fs, path::Path};

use application::v2_contract as generated;
use zeno_fcis_codec::{CanonicalEncode, Envelope, Hash32};
use zeno_fcis_shell::CommitStatus;
use zeno_fcis_shell_sqlite::v2::{
    Error, Lineage, Opened, Step, Store, Superseded, V2SqliteShell,
    behaviour::{Claim, Unmet},
    migration::{Migration, Source, Target},
    upgrade,
};
use zeno_fcis_synthesis::finite::{
    v2_authority::{Authority, PublicationOutcome},
    v2_composition as c, v2_laws as l,
};
use zeno_fcis_value::{Field, Value};

#[track_caller]
fn ok<T, E: std::fmt::Debug>(result: Result<T, E>) -> T {
    result.unwrap_or_else(|error| panic!("unexpected refusal: {error:?}"))
}

#[track_caller]
fn claim_refusal<T>(result: Result<T, Error>) {
    let error = match result {
        Ok(_) => panic!("claim 600 must refuse"),
        Err(error) => error,
    };
    assert!(
        matches!(
            error,
            Error::Upgrade(upgrade::Refusal::MigrationState(Unmet::Claim {
                id: 600,
                failure: l::Failure::Violated
            }))
        ),
        "claim refusal must not become StateSchema: {error:?}"
    );
    let message = error.to_string();
    assert!(
        message.contains("claim 600") && message.contains("nothing was written"),
        "{message}"
    );
}

fn envelope(binding: c::FrameBinding, value: Value) -> Vec<u8> {
    ok(Envelope::new(binding.root, Hash32::new(binding.schema), value).canonical_bytes())
}

fn state(
    framing: &c::Framing,
    status: u16,
    tier: i128,
    cfo: bool,
    urgent: bool,
    extra: bool,
) -> Vec<u8> {
    let mut fields = vec![
        Field::new(120, Value::sum(106, status, None)),
        Field::new(121, Value::signed(tier)),
        Field::new(122, Value::boolean(cfo)),
        Field::new(123, Value::boolean(false)),
        Field::new(124, Value::boolean(urgent)),
    ];
    if extra {
        fields.push(Field::new(125, Value::boolean(false)));
    }
    envelope(framing.state, ok(Value::record_canonical(fields)))
}

fn commit(
    shell: &mut V2SqliteShell<'_, '_>,
    authority: &Authority<'_>,
    framing: &c::Framing,
    pre: &[u8],
    action: u16,
    tier: i128,
    role: u16,
    key: u8,
) {
    let command = envelope(
        framing.command,
        ok(Value::record_canonical(vec![
            Field::new(130, Value::sum(109, action, None)),
            Field::new(131, Value::signed(tier)),
        ])),
    );
    let context = envelope(
        framing.context,
        ok(Value::record_canonical(vec![
            Field::new(140, Value::sum(107, role, None)),
            Field::new(141, Value::boolean(false)),
            Field::new(142, Value::boolean(true)),
        ])),
    );
    let publication = match authority.publish(c::Raw {
        state: pre,
        command: &command,
        context: &context,
    }) {
        PublicationOutcome::Commit(publication) => publication,
        other => panic!("commit refused: {other:?}"),
    };
    assert_eq!(
        ok(shell.commit(Hash32::new([key; 32]), publication)).status(),
        CommitStatus::Committed
    );
}

/// A real genesis and one or three real commits; three leaves a payment pending.
fn create(
    path: &Path,
    authority: &Authority<'_>,
    framing: &c::Framing,
    urgent: bool,
    execute: bool,
) {
    let initial = state(framing, 150, 0, false, urgent, false);
    let genesis = match authority.publish_genesis(&initial) {
        PublicationOutcome::Commit(publication) => publication,
        other => panic!("genesis refused: {other:?}"),
    };
    let mut shell = ok(V2SqliteShell::create(path, authority, genesis));
    commit(&mut shell, authority, framing, &initial, 170, 1, 161, 1);
    if execute {
        commit(
            &mut shell,
            authority,
            framing,
            &state(framing, 151, 1, false, urgent, false),
            171,
            0,
            162,
            2,
        );
        commit(
            &mut shell,
            authority,
            framing,
            &state(framing, 151, 1, true, urgent, false),
            172,
            0,
            161,
            3,
        );
    }
}

fn superseded<'a, 'p>(opened: Opened<'a, 'p>) -> Superseded<'a, 'p> {
    match opened {
        Opened::V10(Store::Superseded(store)) => store,
        _ => panic!("expected an earlier contract"),
    }
}
fn current<'a, 'p>(opened: Opened<'a, 'p>) -> V2SqliteShell<'a, 'p> {
    match opened {
        Opened::V10(Store::Current(store)) => store,
        _ => panic!("expected the current contract"),
    }
}

#[test]
fn compiled_target_claims_guard_migration_and_replay_of_mixed_histories() {
    let directory = std::path::PathBuf::from(
        std::env::var_os("MIGRATION_CLAIMS_ARTIFACTS")
            .expect("the focused runner supplies an artifact directory"),
    );
    ok(generated::with_lineage(
        |(catalogs, receipts, evolutions, state_steps)| {
            assert_eq!(catalogs.len(), 3);
            assert!(receipts.is_empty());
            assert_eq!(
                evolutions
                    .iter()
                    .map(|(after, _, _)| *after)
                    .collect::<Vec<_>>(),
                [1, 2]
            );
            let (_, review_b, claims_b) = evolutions[0];
            let (_, review_c, compiled) = evolutions[1];
            assert!(claims_b.is_empty());
            assert_eq!(
                compiled.iter().map(|(id, _, _)| *id).collect::<Vec<_>>(),
                [600],
                "the generated migration metadata must contain the actual target claim"
            );
            let claims: Vec<_> = compiled
                .iter()
                .map(|(id, nodes, root)| Claim {
                    id: *id,
                    nodes,
                    root: *root,
                })
                .collect();
            assert_eq!(state_steps.len(), 1);
            assert_eq!(state_steps[0].0, 2);
            let fields: Vec<_> = state_steps[0]
                .1
                .expect("a migration")
                .iter()
                .map(|(id, tag, operand, cases)| Target {
                    id: *id,
                    source: match tag {
                        0 => Source::Field(ok(u16::try_from(*operand))),
                        1 => Source::Value(*operand),
                        2 => Source::Table {
                            field: ok(u16::try_from(*operand)),
                            cases,
                        },
                        _ => panic!("unknown migration source"),
                    },
                })
                .collect();
            let first = Step::BehaviourChange {
                review: review_b.as_bytes(),
                claims: &[],
            };
            let steps = [
                first,
                Step::Migration {
                    migration: Migration { fields: &fields },
                    claims: &claims,
                    review: review_c.as_bytes(),
                },
            ];
            let lineage = ok(Lineage::bind_steps(catalogs, &steps));
            let prefix = ok(Lineage::bind_steps(&catalogs[..2], &[first]));

            // A -> B really admits urgent=true under the ordinary rule change;
            // B -> C must then name claim 600 and leave every database byte,
            // audited head, hash-chain tip and pending delivery unchanged.
            let bad = directory.join("bad-mixed.sqlite");
            create(
                &bad,
                &lineage.authorities()[0],
                &generated::v1::FRAMING,
                true,
                true,
            );
            let (mut shell, receipt) = ok(superseded(ok(prefix.open(&bad))).upgrade());
            assert_eq!(receipt.kind(), upgrade::Kind::BehaviourChange);
            let pending = ok(shell.next_pending())
                .expect("payment pending")
                .delivery()
                .delivery_id();
            drop(shell);
            let prior_head = ok(lineage.audit_read_only(&bad));
            assert_eq!(
                (
                    prior_head.0,
                    prior_head.1.version(),
                    prior_head.1.pending(),
                    prior_head.1.upgrades()
                ),
                (2, 3, 1, 1)
            );
            let earlier = superseded(ok(lineage.open(&bad)));
            let before = ok(fs::read(&bad));
            claim_refusal(earlier.upgrade());
            assert_eq!(ok(fs::read(&bad)), before);
            assert_eq!(ok(lineage.audit_read_only(&bad)), prior_head);
            // Exercise the actual generated session adapter too; an empty claim
            // slice here would make this application-level assertion fail.
            let message =
                application::upgrade(&bad).expect_err("the generated adapter must carry claim 600");
            assert!(message.contains("claim 600"), "{message}");
            assert_eq!(ok(fs::read(&bad)), before);
            let mut shell = current(ok(prefix.open(&bad)));
            assert_eq!(
                ok(shell.next_pending())
                    .expect("still pending")
                    .delivery()
                    .delivery_id(),
                pending
            );
            drop(shell);

            // Both hops planned in one transaction: the later refusal leaves no
            // partial A -> B upgrade, even though that hop alone was valid.
            let multi = directory.join("bad-multihop.sqlite");
            create(
                &multi,
                &lineage.authorities()[0],
                &generated::v1::FRAMING,
                true,
                true,
            );
            let earlier = superseded(ok(lineage.open(&multi)));
            let before = ok(fs::read(&multi));
            claim_refusal(earlier.upgrade());
            assert_eq!(ok(fs::read(&multi)), before);
            let (version, head) = ok(lineage.audit_read_only(&multi));
            assert_eq!(
                (version, head.version(), head.pending(), head.upgrades()),
                (1, 3, 1, 0)
            );

            // A B-genesis history whose urgent flag is false passes precisely the
            // same universal simulation and target claim, then keeps committing.
            let good = directory.join("good.sqlite");
            create(
                &good,
                &lineage.authorities()[1],
                &generated::v2::FRAMING,
                false,
                false,
            );
            let report = ok(application::upgrade(&good));
            assert_eq!(report.kind, "migration");
            assert_eq!(
                report
                    .migration
                    .as_ref()
                    .expect("migration report")
                    .claims(),
                [600]
            );
            assert!(report.json().contains("\"claims_held\":[600]"));
            assert_eq!(ok(application::audit(&good)).contract_version, 3);
            let mut shell = current(ok(lineage.open(&good)));
            commit(
                &mut shell,
                &lineage.authorities()[2],
                &generated::FRAMING,
                &state(&generated::FRAMING, 151, 1, false, false, true),
                171,
                0,
                162,
                2,
            );
            commit(
                &mut shell,
                &lineage.authorities()[2],
                &generated::FRAMING,
                &state(&generated::FRAMING, 151, 1, true, false, true),
                172,
                0,
                161,
                3,
            );
            ok(shell.audit());
            drop(shell);
            let (version, head) = ok(lineage.audit_read_only(&good));
            assert_eq!(
                (version, head.version(), head.pending(), head.upgrades()),
                (3, 3, 1, 1)
            );
            assert_eq!(ok(application::audit(&good)).commits, 3);

            // An altered claim with the same ID and the same review still
            // holds here. Replay must nevertheless reject it: the record binds
            // the exact evaluated program, not just a satisfied ID or diff text.
            let mut replacements = 0;
            let altered_nodes: Vec<_> = claims[0]
                .nodes
                .iter()
                .copied()
                .map(|node| match node {
                    l::Op::Observe(l::Observation::Post(124)) => {
                        replacements += 1;
                        l::Op::Observe(l::Observation::Post(125))
                    }
                    other => other,
                })
                .collect();
            assert_eq!(replacements, 1, "change only the observed Boolean field");
            assert_eq!(altered_nodes.len(), claims[0].nodes.len());
            let altered_claims = [Claim {
                id: 600,
                nodes: &altered_nodes,
                root: claims[0].root,
            }];
            let altered_steps = [
                first,
                Step::Migration {
                    migration: Migration { fields: &fields },
                    claims: &altered_claims,
                    review: review_c.as_bytes(),
                },
            ];
            let altered = ok(Lineage::bind_steps(catalogs, &altered_steps));
            let before = ok(fs::read(&good));
            assert!(matches!(
                altered.audit_read_only(&good),
                Err(Error::History)
            ));
            assert_eq!(ok(fs::read(&good)), before);

            // Retain an authentic format-1 old-admission record. Deliberately
            // omit only migration claims from the historical declaration that
            // the pre-repair generated adapter supplied, not from the repaired
            // lineage. The old check admits and replays it; the fixed check must
            // reject its recomputed mapped state, without any raw SQL forgery.
            let legacy_steps = [
                first,
                Step::Migration {
                    migration: Migration { fields: &fields },
                    claims: &[],
                    review: review_c.as_bytes(),
                },
            ];
            let legacy = ok(Lineage::bind_steps(catalogs, &legacy_steps));
            let old = directory.join("old-admission-violating.sqlite");
            create(
                &old,
                &legacy.authorities()[0],
                &generated::v1::FRAMING,
                true,
                true,
            );
            let (shell, receipt) = ok(superseded(ok(legacy.open(&old))).upgrade());
            assert!(
                receipt
                    .migration()
                    .expect("legacy migration")
                    .claims()
                    .is_empty()
            );
            drop(shell);
            assert_eq!(ok(legacy.audit_read_only(&old)).0, 3);
            let before = ok(fs::read(&old));
            claim_refusal(lineage.open(&old));
            assert_eq!(ok(fs::read(&old)), before);
            claim_refusal(lineage.audit_read_only(&old));
            assert_eq!(ok(fs::read(&old)), before);
            let message =
                application::audit(&old).expect_err("repaired generated replay must refuse");
            assert!(message.contains("claim 600"), "{message}");
            assert_eq!(ok(fs::read(&old)), before);
        },
    ));
}
