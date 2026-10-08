//! Behaviour-change upgrades of a withdrawal-queue store. Version 1 is the
//! template's contract from the committed adopted fixture; version 2 is the
//! adopted contract with the vault's capacity in law 500 changed, a rule
//! change bound at test time from the policy the library serializes for it.
//! The lineage declares the step between them as a behaviour change with an
//! owner review and, in some tests, claims.
use rusqlite::Connection;
use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};
use zeno_fcis_codec::{CanonicalEncode, CommitmentHasher, Envelope, Hash32, commitment, domains};
use zeno_fcis_crypto::RustCryptoSha256;
use zeno_fcis_shell::CommitStatus;
use zeno_fcis_shell_sqlite::v2::{
    Error, Lineage, Opened, Step, Store, Superseded, V2SqliteShell, behaviour, upgrade,
};
use zeno_fcis_synthesis::finite::{
    V2ScalarProgram,
    canonical_v2::schema as s,
    v2_authority::{self as authority, Authority, Publication, PublicationOutcome},
    v2_catalog::{self as catalog, BoundCatalog},
    v2_composition::{self as c, Raw},
    v2_laws as l,
};
use zeno_fcis_value::{Field, Value};

/// The adopted withdrawal queue: `adopted::v1` is the template's contract.
#[allow(dead_code)]
#[path = "../../zeno-fcis-cli/tests/fixtures/withdrawal-queue-adopted/src/v2_contract.rs"]
pub mod adopted;

const REVIEW: &[u8] = b"rule-change: withdrawal-queue version 1 -> withdrawal-queue version 2\n";

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

fn sha256(bytes: &[u8]) -> Hash32 {
    <RustCryptoSha256 as CommitmentHasher>::hash(bytes)
}

/// Law 500's program with the vault's capacity, 4, replaced by `capacity`.
fn capacity<'a>(laws: &[l::Law<'a>], capacity: i128) -> Vec<l::Op<'a>> {
    let law = laws
        .iter()
        .find(|law| law.id == 500)
        .unwrap_or_else(|| panic!("law 500"));
    let mut nodes = law.program.nodes.to_vec();
    let bound = nodes
        .iter()
        .position(|op| matches!(op, l::Op::Literal(l::Atom::I128(4))))
        .unwrap_or_else(|| panic!("law 500 bounds the balance by 4"));
    nodes[bound] = l::Op::Literal(l::Atom::I128(capacity));
    nodes
}

/// `laws` with law 500 reading `law_500`.
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

/// Runs `test` with version 1's catalog and version 2: the adopted contract
/// whose law 500 bounds the vault's balance by `bound`.
fn with_rule_change(bound: i128, test: impl FnOnce(&BoundCatalog<'_>, &BoundCatalog<'_>)) {
    let contract_1 = adopted::v1::Contract::new();
    let descriptor_1 = contract_1.descriptor();
    let v1 = ok(adopted::v1::checked_catalog(&descriptor_1));
    let contract = adopted::Contract::new();
    let base = contract.descriptor();
    let law_500 = capacity(base.laws, bound);
    let laws = changed_laws(base.laws, &law_500);
    let descriptor = c::Descriptor {
        state: base.state,
        command: base.command,
        context: base.context,
        program: V2ScalarProgram {
            inputs: base.program.inputs,
            outputs: base.program.outputs,
            nodes: base.program.nodes,
            roots: base.program.roots,
        },
        bindings: base.bindings,
        output_types: base.output_types,
        decision_output: base.decision_output,
        branches: base.branches,
        reasons: base.reasons,
        channels: base.channels,
        laws: &laws,
        required: base.required,
        limits: base.limits,
    };
    let policy = authority::policy_bytes(
        &descriptor,
        adopted::ORIGINAL_SCHEMA,
        &adopted::FRAMING,
        adopted::CHANNEL_ROOTS,
    )
    .unwrap_or_else(|| panic!("the variant's policy encodes"));
    let v2 = ok(catalog::bind_original(
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
        &policy,
        &descriptor,
        &adopted::FRAMING,
        adopted::CHANNEL_ROOTS,
    ));
    test(&v1, &v2);
}

fn envelope(binding: c::FrameBinding, value: Value) -> Vec<u8> {
    ok(Envelope::new(binding.root, Hash32::new(binding.schema), value).canonical_bytes())
}

const EMPTY: u16 = 180;
const ARRIVED: u16 = 181;
const LANE_A: u16 = 170;
const DEPOSIT: u16 = 160;
const REQUEST: u16 = 161;
const TICK: u16 = 162;
const OPERATOR: u16 = 190;
const OWNER_A: u16 = 191;
const KEEPER: u16 = 193;

fn vault(balance: i128, lane_a: u16, amount_a: i128, priority: u16) -> Vec<u8> {
    let value = ok(Value::record_canonical(vec![
        Field::new(120, Value::signed(balance)),
        Field::new(121, Value::sum(109, lane_a, None)),
        Field::new(122, Value::signed(amount_a)),
        Field::new(123, Value::sum(109, EMPTY, None)),
        Field::new(124, Value::signed(0)),
        Field::new(125, Value::signed(0)),
        Field::new(126, Value::boolean(false)),
        Field::new(127, Value::sum(108, priority, None)),
    ]));
    envelope(adopted::FRAMING.state, value)
}
fn command(action: u16, lane: u16, amount: i128) -> Vec<u8> {
    let value = ok(Value::record_canonical(vec![
        Field::new(130, Value::sum(107, action, None)),
        Field::new(131, Value::sum(108, lane, None)),
        Field::new(132, Value::signed(amount)),
    ]));
    envelope(adopted::FRAMING.command, value)
}
fn context(caller: u16) -> Vec<u8> {
    let value = ok(Value::record_canonical(vec![
        Field::new(140, Value::sum(112, caller, None)),
        Field::new(141, Value::boolean(false)),
    ]));
    envelope(adopted::FRAMING.context, value)
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
            "zenofcis-v2-behaviour-{}-{}.db",
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

/// The first `steps` commits under `v1`: a deposit of 2, lane A's request
/// for 2 and the tick that pays it. After one step the balance is 2; after
/// three it is 0 again, away from genesis, with lane A's payout pending.
fn store_under(path: &Path, v1: &Authority<'_>, steps: usize) {
    let initial = vault(0, EMPTY, 0, LANE_A);
    let genesis = match v1.publish_genesis(&initial) {
        PublicationOutcome::Commit(p) => p,
        refused => panic!("genesis refused: {refused:?}"),
    };
    let mut db = ok(V2SqliteShell::create(path, v1, genesis));
    let session = [
        (
            vault(0, EMPTY, 0, LANE_A),
            command(DEPOSIT, LANE_A, 2),
            context(OPERATOR),
        ),
        (
            vault(2, EMPTY, 0, LANE_A),
            command(REQUEST, LANE_A, 2),
            context(OWNER_A),
        ),
        (
            vault(2, ARRIVED, 2, LANE_A),
            command(TICK, LANE_A, 1),
            context(KEEPER),
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

/// The state laws of a descriptor as the shell defines them, and the other
/// laws that apply at genesis.
fn state_laws(descriptor: &c::Descriptor<'_>) -> (Vec<u32>, Vec<u32>) {
    let state = |law: &l::Law<'_>| {
        law.genesis
            && matches!(law.scope, l::Scope::Committing | l::Scope::Always)
            && !matches!(law.kind, l::Kind::InitialCondition)
    };
    (
        descriptor
            .laws
            .iter()
            .filter(|law| state(law))
            .map(|law| law.id)
            .collect(),
        descriptor
            .laws
            .iter()
            .filter(|law| law.genesis && !state(law))
            .map(|law| law.id)
            .collect(),
    )
}

/// The vault's balance is at most `bound`: `!(bound < post.120)`.
fn balance_at_most(bound: i128) -> [l::Op<'static>; 4] {
    [
        l::Op::Observe(l::Observation::Post(120)),
        l::Op::Literal(l::Atom::I128(bound)),
        l::Op::Lt(1, 0),
        l::Op::Not(2),
    ]
}

#[test]
fn a_reviewed_rule_change_upgrades_a_store_with_history_and_keeps_committing() {
    with_rule_change(5, |v1, v2| {
        let held = balance_at_most(4);
        let claims = [behaviour::Claim {
            id: 600,
            nodes: &held,
            root: 3,
        }];
        let steps = [Step::BehaviourChange {
            review: REVIEW,
            claims: &claims,
        }];
        let lineage = ok(Lineage::bind_steps(&[v1, v2], &steps));
        let file = StoreFile::new();
        store_under(&file.0, &lineage.authorities()[0], 3);
        let before = ok(fs::read(&file.0));
        // The new build audits the old store without writing to it.
        let (version, head) = ok(lineage.audit_read_only(&file.0));
        assert_eq!((version, head.version(), head.pending()), (1, 3, 1));
        assert_eq!(ok(fs::read(&file.0)), before);
        let (mut db, receipt) = ok(superseded(ok(lineage.open(&file.0))).upgrade());
        // No program comparison: the admission is the new laws on the state.
        assert_eq!(lineage.comparisons(), 0);
        assert_eq!(receipt.kind(), upgrade::Kind::BehaviourChange);
        assert_eq!(receipt.kind().tag(), "behaviour-change");
        assert!(receipt.premises().is_none() && receipt.receipts().is_empty());
        let behaviour = receipt
            .behaviour()
            .unwrap_or_else(|| panic!("a behaviour change"));
        let (laws, unevaluated) = state_laws(v2.descriptor());
        assert_eq!(behaviour.laws(), laws.as_slice());
        assert!(behaviour.laws().contains(&500));
        assert_eq!(behaviour.claims(), [600]);
        // Genesis exactness is not evaluated: the store is away from genesis.
        assert_eq!(behaviour.unevaluated(), unevaluated.as_slice());
        assert!(behaviour.unevaluated().contains(&990));
        assert_eq!(behaviour.reviews(), [sha256(REVIEW)]);
        assert!(receipt.record().starts_with(b"ZFCISV2-BEHAVIOUR\0"));
        // The review text is stored beside the record.
        let external = ok(Connection::open(&file.0));
        let stored: Vec<u8> = ok(external.query_row(
            "SELECT publication FROM v2_upgrades WHERE ordinal=1",
            [],
            |r| r.get(0),
        ));
        let mut framed = (REVIEW.len() as u64).to_be_bytes().to_vec();
        framed.extend_from_slice(REVIEW);
        assert_eq!(stored, framed);
        drop(external);
        // The store keeps committing under version 2.
        let v2_authority = &lineage.authorities()[1];
        let state = vault(0, EMPTY, 0, 171);
        let committed = ok(db.commit(
            key(9),
            publication(
                v2_authority,
                &state,
                &command(DEPOSIT, LANE_A, 2),
                &context(OPERATOR),
            ),
        ));
        assert_eq!(committed.status(), CommitStatus::Committed);
        ok(db.audit());
        drop(db);
        // Both segments replay, each under its own contract.
        let mut reopened = current(ok(lineage.open(&file.0)));
        ok(reopened.audit());
        let snapshot = ok(reopened.snapshot());
        assert_eq!((snapshot.version(), snapshot.upgrades()), (4, 1));
        drop(reopened);
        let (version, head) = ok(lineage.audit_read_only(&file.0));
        assert_eq!((version, head.version(), head.upgrades()), (2, 4, 1));
        // Version 1 alone cannot replay the version 2 segment.
        let alone = ok(Lineage::bind(&[v1], &[]));
        assert!(matches!(
            lineage_open(&alone, &file.0),
            Some(Error::Identity)
        ));
        assert_eq!(lineage.comparisons(), 0);
    });
}

fn lineage_open(lineage: &Lineage<'_, '_>, path: &Path) -> Option<Error> {
    lineage.open(path).err()
}

#[test]
fn a_state_law_that_fails_on_the_current_state_refuses_with_no_write() {
    // Version 2's capacity is 1; after the deposit the balance is 2.
    with_rule_change(1, |v1, v2| {
        let steps = [Step::BehaviourChange {
            review: REVIEW,
            claims: &[],
        }];
        let lineage = ok(Lineage::bind_steps(&[v1, v2], &steps));
        let file = StoreFile::new();
        store_under(&file.0, &lineage.authorities()[0], 1);
        let before = ok(fs::read(&file.0));
        let error = refused(superseded(ok(lineage.open(&file.0))).upgrade());
        assert!(
            matches!(
                error,
                Error::Upgrade(upgrade::Refusal::Behaviour(behaviour::Unmet::Law {
                    id: 500,
                    failure: l::Failure::Violated
                }))
            ),
            "{error:?}"
        );
        assert!(
            error
                .to_string()
                .contains("state law 500 of the new contract is false")
        );
        assert_eq!(ok(fs::read(&file.0)), before);
        assert_eq!(lineage.comparisons(), 0);
        // Where the balance fits the new capacity the same change is
        // admitted, away from genesis, without law 990.
        let fresh = StoreFile::new();
        store_under(&fresh.0, &lineage.authorities()[0], 3);
        let (_, receipt) = ok(superseded(ok(lineage.open(&fresh.0))).upgrade());
        assert_eq!(receipt.kind(), upgrade::Kind::BehaviourChange);
    });
}

#[test]
fn a_declared_claim_must_hold_on_the_state() {
    with_rule_change(5, |v1, v2| {
        let low = balance_at_most(1);
        for (claims, expected) in [
            (
                vec![behaviour::Claim {
                    id: 600,
                    nodes: &low,
                    root: 3,
                }],
                Some(behaviour::Unmet::Claim {
                    id: 600,
                    failure: l::Failure::Violated,
                }),
            ),
            // A claim with a law's ID cannot be evaluated beside it.
            (
                vec![behaviour::Claim {
                    id: 500,
                    nodes: &low,
                    root: 3,
                }],
                Some(behaviour::Unmet::Unevaluable),
            ),
        ] {
            let steps = [Step::BehaviourChange {
                review: REVIEW,
                claims: &claims,
            }];
            let lineage = ok(Lineage::bind_steps(&[v1, v2], &steps));
            let file = StoreFile::new();
            store_under(&file.0, &lineage.authorities()[0], 1);
            let before = ok(fs::read(&file.0));
            let error = refused(superseded(ok(lineage.open(&file.0))).upgrade());
            assert!(
                matches!(error, Error::Upgrade(upgrade::Refusal::Behaviour(unmet)) if Some(unmet) == expected),
                "{error:?}"
            );
            assert_eq!(ok(fs::read(&file.0)), before);
        }
    });
}

/// Upgrades a store of three commits under `lineage`, then replaces its
/// upgrade record with `forge`'s, the chain tips recomputed as a forger
/// would, and returns the store.
fn forged(
    lineage: &Lineage<'_, '_>,
    forge: impl FnOnce(&[u8], &[u8]) -> (Vec<u8>, Vec<u8>),
) -> StoreFile {
    let file = StoreFile::new();
    store_under(&file.0, &lineage.authorities()[0], 3);
    drop(ok(superseded(ok(lineage.open(&file.0))).upgrade()));
    let external = ok(Connection::open(&file.0));
    let (publication, record): (Vec<u8>, Vec<u8>) = ok(external.query_row(
        "SELECT publication,record FROM v2_upgrades WHERE ordinal=1",
        [],
        |r| Ok((r.get(0)?, r.get(1)?)),
    ));
    let (publication, record) = forge(&publication, &record);
    let chain = ok(commitment::<RustCryptoSha256>(domains::V2_CHAIN, &record));
    ok(external.execute(
        "UPDATE v2_upgrades SET publication=?1, record=?2, chain=?3 WHERE ordinal=1",
        rusqlite::params![publication, record, chain.as_bytes().as_slice()],
    ));
    ok(external.execute(
        "UPDATE v2_state SET chain=?1",
        [chain.as_bytes().as_slice()],
    ));
    file
}

/// The record with its admission replaced: the parts after the magic, the
/// ordinal and the sequence are framed, and the admission is the third.
fn with_admission(record: &[u8], admission: &[u8]) -> Vec<u8> {
    let magic = b"ZFCISV2-BEHAVIOUR\0".len();
    let mut bytes = record[..magic + 16].to_vec();
    let mut rest = &record[magic + 16..];
    for index in 0..5 {
        let (length, tail) = rest.split_at(8);
        let length = u64::from_be_bytes(ok(length.try_into())) as usize;
        let (part, tail) = tail.split_at(length);
        let part = if index == 2 { admission } else { part };
        bytes.extend_from_slice(&(part.len() as u64).to_be_bytes());
        bytes.extend_from_slice(part);
        rest = tail;
    }
    bytes
}

/// The admission of a record, as `with_admission` finds it.
fn admission_of(record: &[u8]) -> Vec<u8> {
    let magic = b"ZFCISV2-BEHAVIOUR\0".len();
    let mut rest = &record[magic + 16..];
    for index in 0..3 {
        let (length, tail) = rest.split_at(8);
        let length = u64::from_be_bytes(ok(length.try_into())) as usize;
        let (part, tail) = tail.split_at(length);
        if index == 2 {
            return part.to_vec();
        }
        rest = tail;
    }
    unreachable!()
}

#[test]
fn forged_or_misdeclared_behaviour_change_records_are_refused() {
    with_rule_change(5, |v1, v2| {
        let steps = [Step::BehaviourChange {
            review: REVIEW,
            claims: &[],
        }];
        let lineage = ok(Lineage::bind_steps(&[v1, v2], &steps));
        // The genuine record, recomputed by a forger, is accepted: the chain
        // is not keyed, and the admission is derived again.
        let genuine = forged(&lineage, |publication, record| {
            (publication.to_vec(), record.to_vec())
        });
        let mut db = current(ok(lineage.open(&genuine.0)));
        ok(db.audit());
        drop(db);
        // An admission that leaves a law out, or claims a law that does not
        // exist, is refused: the audit evaluates the laws again.
        for edit in [
            // The first law ID: 500 becomes 501's twin.
            |admission: &mut Vec<u8>| admission[5..9].copy_from_slice(&501_u32.to_be_bytes()),
            // A claim that was never declared.
            |admission: &mut Vec<u8>| {
                let laws = u32::from_be_bytes(ok(admission[1..5].try_into())) as usize;
                let claims = 5 + 4 * laws;
                admission[claims..claims + 4].copy_from_slice(&1_u32.to_be_bytes());
                admission.splice(claims + 4..claims + 4, 600_u32.to_be_bytes());
            },
            // No law left unevaluated: as if law 990 had held.
            |admission: &mut Vec<u8>| {
                let laws = u32::from_be_bytes(ok(admission[1..5].try_into())) as usize;
                let claims = 5 + 4 * laws;
                let unevaluated = claims + 4;
                let count =
                    u32::from_be_bytes(ok(admission[unevaluated..unevaluated + 4].try_into()))
                        as usize;
                admission.drain(unevaluated + 4..unevaluated + 4 + 4 * count);
                admission[unevaluated..unevaluated + 4].copy_from_slice(&0_u32.to_be_bytes());
            },
        ] {
            let file = forged(&lineage, |publication, record| {
                let mut admission = admission_of(record);
                edit(&mut admission);
                (publication.to_vec(), with_admission(record, &admission))
            });
            let before = ok(fs::read(&file.0));
            assert!(
                matches!(lineage_open(&lineage, &file.0), Some(Error::History)),
                "{:?}",
                lineage_open(&lineage, &file.0)
            );
            assert!(matches!(
                lineage.audit_read_only(&file.0),
                Err(Error::History)
            ));
            assert_eq!(ok(fs::read(&file.0)), before);
        }
        // Another review text stored beside an unchanged record is damage.
        let altered = forged(&lineage, |publication, record| {
            let mut publication = publication.to_vec();
            let last = publication.len() - 1;
            publication[last] ^= 1;
            (publication, record.to_vec())
        });
        assert!(matches!(
            lineage_open(&lineage, &altered.0),
            Some(Error::History)
        ));
        // A lineage that declares another review, or an adoption, for the
        // step does not support the store, which may be intact.
        let other = [Step::BehaviourChange {
            review: b"another review\n",
            claims: &[],
        }];
        let adoption = [Step::Adoption([7; 32])];
        for steps in [&other[..], &adoption[..]] {
            let misdeclared = ok(Lineage::bind_steps(&[v1, v2], steps));
            assert!(
                matches!(
                    lineage_open(&misdeclared, &genuine.0),
                    Some(Error::Succession(upgrade::Unsupported::Reviews))
                ),
                "{:?}",
                lineage_open(&misdeclared, &genuine.0)
            );
        }
    });
}

#[test]
fn a_lineage_needs_one_step_between_each_version_and_the_next() {
    with_rule_change(5, |v1, v2| {
        assert!(matches!(
            Lineage::bind_steps(&[v1, v2], &[]),
            Err(Error::Lineage)
        ));
        let steps = [Step::BehaviourChange {
            review: REVIEW,
            claims: &[],
        }];
        assert!(matches!(
            Lineage::bind_steps(&[v1], &steps),
            Err(Error::Lineage)
        ));
    });
}
