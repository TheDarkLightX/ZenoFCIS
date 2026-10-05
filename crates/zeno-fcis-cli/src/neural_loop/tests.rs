//! Loop core tests: limits and profiles, request identity, the selection
//! rule, the incumbent invariant under arbitrary proposals, resource caps
//! without refunds, resume and planted defects (NSL/NSR/NSF scenarios).

use super::feedback::{
    DuplicateOf, Feedback, Witness, WitnessFault, observe, replay_witness, tuple_of_ordinal,
};
use super::incumbent::{Cost, Incumbent, NoImprovement, Replacement, Selection, select};
use super::ledger::{Entry, Head, Kind, Ledger, LedgerFault, Stage};
use super::limits::{LimitRefusal, Limits};
use super::local::LocalProposer;
use super::profiles::{Profile, ProfileRefusal};
use super::program_json::{encode, program_from_json, program_to_json};
use super::request::{
    CheckerIdentity, Disclosure, HostedMode, Policy, PolicyRefusal, ReadmitRefusal, Request,
    RequestRefusal,
};
use super::session::{
    AdmissionRefusal, CheckJob, CheckReport, Outcome, Prepared, Proposal, ProposerFailure,
    ResumeRefusal, Resumed, Session, Status, StopReason, Stored, WorkerFailure,
};
use super::strategy::{NoEngine, SearchReport, Strategy, StrategyEngine, StrategyRefusal};
use crate::transform::{self, sha256_hex};
use serde_json::{Value, json};
use zeno_fcis_synthesis::finite::{Domain, Op, Program};
use zeno_fcis_synthesis::finite_runtime::import_program;

const FULL: Domain = Domain::Int {
    min: i64::MIN,
    max: i64::MAX,
};

fn artifact(name: &str) -> &'static [u8] {
    match name {
        "boolean-kernel-original" => include_bytes!(
            "../../../../docs/benchmarks/withdrawal-queue/artifacts/boolean-kernel-original.zcve"
        ),
        "boolean-kernel-candidate" => include_bytes!(
            "../../../../docs/benchmarks/withdrawal-queue/artifacts/boolean-kernel-candidate.zcve"
        ),
        "boolean-kernel-padded" => include_bytes!(
            "../../../../docs/benchmarks/withdrawal-queue/artifacts/boolean-kernel-padded.zcve"
        ),
        "retained-controller-original" => include_bytes!(
            "../../../../docs/benchmarks/withdrawal-queue/artifacts/retained-controller-original.zcve"
        ),
        "retained-controller-candidate" => include_bytes!(
            "../../../../docs/benchmarks/withdrawal-queue/artifacts/retained-controller-candidate.zcve"
        ),
        _ => panic!("unknown artifact {name}"),
    }
}

fn program(inputs: Vec<Domain>, outputs: Vec<Domain>, nodes: Vec<Op>, roots: Vec<u16>) -> Program {
    Program::try_new(inputs, outputs, nodes, roots)
        .unwrap_or_else(|error| panic!("test program must be admitted: {error}"))
}

fn bytes_of(program: &Program) -> Vec<u8> {
    encode(program).unwrap_or_else(|error| panic!("encode: {error:?}"))
}

fn import(bytes: &[u8]) -> Program {
    import_program(bytes).unwrap_or_else(|error| panic!("import: {error}"))
}

fn cases() -> Vec<Value> {
    let fixtures: Value =
        serde_json::from_str(include_str!("../../../../docs/benchmarks/cases.json"))
            .unwrap_or_else(|error| panic!("cases.json: {error}"));
    fixtures["cases"]
        .as_array()
        .unwrap_or_else(|| panic!("cases"))
        .clone()
}

/// A catalog case as canonical bytes: (original, candidate).
fn fixture(case: &Value) -> (Vec<u8>, Vec<u8>) {
    let side = |graph: &Value| {
        let program = program_from_json(&json!({
            "inputs": case["input_domains"],
            "outputs": case["output_domains"],
            "nodes": graph["nodes"],
            "roots": graph["roots"],
        }))
        .unwrap_or_else(|error| panic!("{}: {error:?}", case["id"]));
        bytes_of(&program)
    };
    (side(&case["original"]), side(&case["candidate"]))
}

fn case(id: &str) -> Value {
    cases()
        .into_iter()
        .find(|case| case["id"] == id)
        .unwrap_or_else(|| panic!("case {id}"))
}

fn profile_of(case: &Value) -> Profile {
    match case["profile"].as_str() {
        Some("FunctionalBoolV1") => Profile::FunctionalBoolV1,
        _ => Profile::CheckedI64V1,
    }
}

fn request(original: &[u8], profile: Profile) -> Request {
    request_with(original, profile, Limits::CEILING)
}

fn request_with(original: &[u8], profile: Profile, limits: Limits) -> Request {
    Request::admit(original, profile, limits, Policy::DISABLED)
        .unwrap_or_else(|refusal| panic!("request must admit: {refusal:?}"))
}

/// Runs one attempt end to end in the core; the check runs in-process.
fn attempt(session: &mut Session, proposal: Proposal) -> Outcome {
    let ticket = match session.reserve_attempt(0) {
        Ok(ticket) => ticket,
        Err(reason) => panic!("attempt must reserve: {reason:?}"),
    };
    match session.prepare(ticket, proposal) {
        Prepared::Check(job) => {
            let report = job.run();
            session.conclude(job, report)
        }
        Prepared::Search(job) => match session.searched(
            job,
            SearchReport::Unavailable {
                reason: "test".into(),
            },
        ) {
            Prepared::Settled(outcome) => outcome,
            other => panic!("unexpected {other:?}"),
        },
        Prepared::Settled(outcome) => outcome,
    }
}

/// Independent oracle: both programs agree on every tuple of the domain,
/// enumerated by mixed-radix decoding rather than the checker's odometer.
fn equal_on_domain(left: &Program, right: &Program) -> bool {
    let size = transform::domain_size(left.inputs())
        .ok()
        .flatten()
        .and_then(|size| u64::try_from(size).ok())
        .unwrap_or_else(|| panic!("small domain"));
    left.inputs() == right.inputs()
        && left.outputs() == right.outputs()
        && (0..size).all(|ordinal| {
            let tuple =
                tuple_of_ordinal(left.inputs(), ordinal).unwrap_or_else(|| panic!("ordinal"));
            observe(left, &tuple).result == observe(right, &tuple).result
        })
}

fn incumbent_program(session: &Session) -> Program {
    match session.incumbent() {
        Incumbent::OriginalAdmitted => session.request().program().clone(),
        Incumbent::CheckedReplacement(replacement) => import(replacement.bytes()),
    }
}

fn replacement(session: &Session) -> &Replacement {
    session
        .incumbent()
        .replacement()
        .unwrap_or_else(|| panic!("expected a checked replacement"))
}

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    fn below(&mut self, bound: u64) -> u64 {
        self.next() % bound.max(1)
    }
}

/// A random admitted Boolean program over the given ABI.
fn random_bool_program(rng: &mut Rng, inputs: usize, outputs: usize) -> Program {
    let count = 1 + rng.below(12) as usize;
    let mut nodes: Vec<Op> = Vec::new();
    for _ in 0..count {
        let built = nodes.len() as u64;
        let pick = |rng: &mut Rng| u16::try_from(rng.below(built)).unwrap_or_default();
        let op = match rng.below(5) {
            0 if inputs > 0 => {
                Op::Input(u16::try_from(rng.below(inputs as u64)).unwrap_or_default())
            }
            2 if built > 0 => Op::Not(pick(rng)),
            3 if built > 0 => Op::And(pick(rng), pick(rng)),
            4 if built > 0 => Op::Select(pick(rng), pick(rng), pick(rng)),
            _ => Op::Bool(rng.below(2) == 1),
        };
        nodes.push(op);
    }
    let roots = (0..outputs)
        .map(|_| u16::try_from(rng.below(nodes.len() as u64)).unwrap_or_default())
        .collect();
    program(
        vec![Domain::Bool; inputs],
        vec![Domain::Bool; outputs],
        nodes,
        roots,
    )
}

#[test]
fn ceilings_are_designs_initial_limits() {
    let ceiling = Limits::CEILING;
    assert_eq!(ceiling.attempts, 8);
    assert_eq!(ceiling.checks, 8);
    assert_eq!(ceiling.model_calls, 4);
    assert_eq!(ceiling.input_tokens, 4_096);
    assert_eq!(ceiling.output_tokens, 2_048);
    assert_eq!(ceiling.total_tokens, 24_576);
    assert_eq!(ceiling.check_work, 1_000_000);
    assert_eq!(ceiling.session_work, 8_000_000);
    assert_eq!(ceiling.deadline_ms, 20_000);
    assert_eq!(ceiling.artifact_bytes, 65_536);
    assert_eq!(ceiling.money_micros, 0);
    assert_eq!(ceiling.transcript_bytes, 512 * 1024);
    assert_eq!(ceiling.storage_bytes, 2 * 1024 * 1024);
    assert_eq!(Limits::try_new(ceiling), Ok(ceiling));
    // Exactly four calls fit the token total.
    assert_eq!(ceiling.call_tokens() * 4, u64::from(ceiling.total_tokens));
}

#[test]
fn limits_above_the_ceiling_or_inconsistent_are_refused_and_lower_ones_win() {
    let mut above = Limits::CEILING;
    above.attempts = 9;
    assert_eq!(
        Limits::try_new(above),
        Err(LimitRefusal::AboveCeiling {
            field: "attempts",
            requested: 9,
            ceiling: 8
        })
    );
    let mut money = Limits::CEILING;
    money.money_micros = 1;
    assert!(matches!(
        Limits::try_new(money),
        Err(LimitRefusal::AboveCeiling {
            field: "money_micros",
            ..
        })
    ));
    let mut zero = Limits::CEILING;
    zero.checks = 0;
    assert_eq!(
        Limits::try_new(zero),
        Err(LimitRefusal::Zero { field: "checks" })
    );
    let mut tokens = Limits::CEILING;
    tokens.total_tokens = 1_000;
    assert_eq!(
        Limits::try_new(tokens),
        Err(LimitRefusal::TokensInconsistent)
    );
    let mut work = Limits::CEILING;
    work.session_work = 500_000;
    assert_eq!(Limits::try_new(work), Err(LimitRefusal::WorkInconsistent));
    let mut lower = Limits::CEILING;
    lower.attempts = 3;
    lower.deadline_ms = 5;
    assert_eq!(Limits::try_new(lower), Ok(lower));
    assert_eq!(Limits::from_json(&lower.json()), Some(lower));
    let mut extra = lower.json();
    extra["refund_policy"] = json!("generous");
    assert_eq!(Limits::from_json(&extra), None);
    let mut missing = lower.json();
    missing.as_object_mut().map(|map| map.remove("attempts"));
    assert_eq!(Limits::from_json(&missing), None);
}

#[test]
fn functional_bool_v1_admits_only_the_total_boolean_subset() {
    let profile = Profile::FunctionalBoolV1;
    let bools = |count: usize| vec![Domain::Bool; count];
    let six = program(bools(6), bools(1), (0..6).map(Op::Input).collect(), vec![5]);
    assert_eq!(profile.admit(&six), Ok(64));
    let seven = program(bools(7), bools(1), vec![Op::Input(6)], vec![0]);
    assert_eq!(
        profile.admit(&seven),
        Err(ProfileRefusal::Inputs { count: 7, max: 6 })
    );
    // A dead arithmetic node is refused even though no output reaches it.
    let dead_int = program(
        bools(1),
        bools(1),
        vec![Op::Input(0), Op::Int(0), Op::Int(1), Op::Add(1, 2)],
        vec![0],
    );
    assert_eq!(
        profile.admit(&dead_int),
        Err(ProfileRefusal::Opcode {
            node: 1,
            opcode: "Int"
        })
    );
    let eq = program(
        bools(2),
        bools(1),
        vec![Op::Input(0), Op::Input(1), Op::Eq(0, 1)],
        vec![2],
    );
    assert_eq!(
        profile.admit(&eq),
        Err(ProfileRefusal::Opcode {
            node: 2,
            opcode: "Eq"
        })
    );
    let int_output = program(
        bools(1),
        vec![FULL],
        vec![Op::Input(0), Op::Int(1)],
        vec![1],
    );
    assert_eq!(
        profile.admit(&int_output),
        Err(ProfileRefusal::NonBooleanOutput { position: 0 })
    );
    let int_input = program(
        vec![Domain::Int { min: 0, max: 1 }],
        bools(1),
        vec![Op::Bool(true)],
        vec![0],
    );
    assert_eq!(
        profile.admit(&int_input),
        Err(ProfileRefusal::NonBooleanInput { position: 0 })
    );
    // Zero inputs: the singleton domain of the empty tuple.
    let constant = program(vec![], bools(1), vec![Op::Bool(true)], vec![0]);
    assert_eq!(profile.admit(&constant), Ok(1));
    // Sixteen outputs are admitted; the library refuses seventeen before the profile.
    let sixteen = program(bools(1), bools(16), vec![Op::Input(0)], vec![0; 16]);
    assert_eq!(profile.admit(&sixteen), Ok(2));
    assert!(Program::try_new(bools(1), bools(17), vec![Op::Input(0)], vec![0; 17]).is_err());
    // The i64 profile admits arithmetic and bounds the domain by F3's cap.
    assert_eq!(Profile::CheckedI64V1.admit(&dead_int), Ok(2));
    let wide = program(vec![FULL], vec![FULL], vec![Op::Input(0)], vec![0]);
    assert_eq!(
        Profile::CheckedI64V1.admit(&wide),
        Err(ProfileRefusal::DomainTooLarge {
            size: Some(1 << 64),
            max: transform::DEFAULT_MAX_INPUT_TUPLES
        })
    );
    assert_eq!(
        Profile::parse("functional-bool-v1"),
        Some(Profile::FunctionalBoolV1)
    );
    assert_eq!(
        Profile::parse("checked-i64-v1"),
        Some(Profile::CheckedI64V1)
    );
    assert_eq!(Profile::parse("FunctionalBoolV1"), None);
}

#[test]
fn request_admission_follows_the_precedence_and_freezes_the_identity() {
    let original = artifact("boolean-kernel-original");
    let hosted = Policy {
        hosted: HostedMode::Enabled {
            provider: "example".into(),
        },
        disclosure: Disclosure::SourceToProvider,
        money_micros: 0,
    };
    assert_eq!(
        Request::admit(original, Profile::FunctionalBoolV1, Limits::CEILING, hosted),
        Err(RequestRefusal::Policy(PolicyRefusal::HostedUnavailable {
            provider: "example".into()
        }))
    );
    let disclosure = Policy {
        disclosure: Disclosure::SourceToProvider,
        ..Policy::DISABLED
    };
    assert_eq!(
        Request::admit(
            original,
            Profile::FunctionalBoolV1,
            Limits::CEILING,
            disclosure
        ),
        Err(RequestRefusal::Policy(
            PolicyRefusal::DisclosureWithoutProvider
        ))
    );
    let oversized = vec![0_u8; 64 * 1024 + 1];
    assert_eq!(
        Request::admit(
            &oversized,
            Profile::CheckedI64V1,
            Limits::CEILING,
            Policy::DISABLED
        ),
        Err(RequestRefusal::ArtifactTooLarge {
            bytes: 65_537,
            max: 65_536
        })
    );
    assert_eq!(
        Request::admit(
            &original[..20],
            Profile::FunctionalBoolV1,
            Limits::CEILING,
            Policy::DISABLED
        ),
        Err(RequestRefusal::NotAdmitted {
            code: "program-encoding".into()
        })
    );
    // The controller has eight inputs, some integer: outside the Boolean
    // profile (the input count is checked first), inside i64.
    let controller = artifact("retained-controller-original");
    assert_eq!(
        Request::admit(
            controller,
            Profile::FunctionalBoolV1,
            Limits::CEILING,
            Policy::DISABLED
        ),
        Err(RequestRefusal::Profile(ProfileRefusal::Inputs {
            count: import(controller).inputs().len(),
            max: 6
        }))
    );
    let controller_request = request(controller, Profile::CheckedI64V1);
    assert_eq!(controller_request.domain_size(), 384);
    // Work infeasible when the per-check limit is below the self-check.
    let mut tiny = Limits::CEILING;
    tiny.check_work = 10;
    tiny.session_work = 10;
    assert!(matches!(
        Request::admit(original, Profile::FunctionalBoolV1, tiny, Policy::DISABLED),
        Err(RequestRefusal::WorkInfeasible { limit: 10, .. })
    ));

    let request = request(original, Profile::FunctionalBoolV1);
    let again = self::request(original, Profile::FunctionalBoolV1);
    assert_eq!(request.id(), again.id());
    assert_eq!(request.canonical_bytes(), again.canonical_bytes());
    assert_eq!(request.id().len(), 64);
    assert_eq!(request.domain_size(), 16);
    assert_eq!(
        request.cost(),
        Cost {
            nodes: 16,
            bytes: 1_063
        }
    );
    let record = request.json();
    assert_eq!(record["schema"], "zeno-fcis/transform-request/1");
    assert_eq!(record["profile"], "functional-bool-v1");
    assert_eq!(record["objective"], "lexicographic-nodes-bytes-v1");
    assert_eq!(record["domain"]["size"], 16);
    assert_eq!(
        record["policy"],
        json!({"hosted": "disabled", "disclosure": "local", "money_micros": 0})
    );
    // The request binds the same checker identity the receipts record.
    let equivalence = transform::check(
        original,
        artifact("boolean-kernel-candidate"),
        request.transform_limits(),
    )
    .unwrap_or_else(|rejection| panic!("{rejection:?}"));
    assert_eq!(record["checker"], equivalence.receipt_value()["checker"]);
    assert_eq!(CheckerIdentity::current().json(), record["checker"]);
    // Views: local disclosure includes the program, hosted disclosure withholds it.
    let local = request.view(false);
    assert_eq!(
        local["original"]["program"]["nodes"]
            .as_array()
            .map(Vec::len),
        Some(16)
    );
    assert_eq!(local["request_id"], request.id());
    let hosted = request.view(true);
    assert_eq!(
        hosted["original"]["program"],
        "withheld-by-disclosure-policy"
    );
    // Re-admission reproduces the request; a stale checker or changed bytes do not.
    assert_eq!(
        Request::readmit(request.canonical_bytes(), original),
        Ok(request.clone())
    );
    let mut stale = record.clone();
    stale["checker"]["semantics"] = json!("zeno-fcis/transform-check/0");
    assert!(matches!(
        Request::readmit(&super::canonical_json(&stale), original),
        Err(ReadmitRefusal::StaleChecker { .. })
    ));
    // A derived field that fresh admission cannot reproduce is a mismatch.
    let mut changed = record.clone();
    changed["domain"]["size"] = json!(17);
    assert_eq!(
        Request::readmit(&super::canonical_json(&changed), original),
        Err(ReadmitRefusal::Mismatch)
    );
    // Other limits make another self-consistent request with another id,
    // which the ledger's opening entry then refuses (see the resume tests).
    let mut other_limits = record.clone();
    other_limits["limits"]["attempts"] = json!(2);
    let other = Request::readmit(&super::canonical_json(&other_limits), original)
        .unwrap_or_else(|refusal| panic!("{refusal:?}"));
    assert_ne!(other.id(), request.id());
    assert_eq!(other.limits().attempts, 2);
    assert!(matches!(
        Request::readmit(
            request.canonical_bytes(),
            artifact("boolean-kernel-candidate")
        ),
        Err(ReadmitRefusal::Mismatch)
    ));
    assert_eq!(
        Request::readmit(b"not json", original),
        Err(ReadmitRefusal::Unreadable)
    );
}

#[test]
fn selection_requires_original_bounds_and_lexicographic_descent() {
    let cost = |nodes, bytes| Cost { nodes, bytes };
    let original = cost(12, 300);
    // DESIGN's legal sequence: (12,300) -> (10,200) -> (9,250).
    assert_eq!(
        select(original, original, cost(10, 200)),
        Selection::CheckedImprovement
    );
    assert_eq!(
        select(original, cost(10, 200), cost(9, 250)),
        Selection::CheckedImprovement
    );
    // Not componentwise: (9,250) has more bytes than (10,200) and is still selected above.
    // Regression after improvement keeps the incumbent.
    assert_eq!(
        select(original, cost(9, 250), cost(10, 200)),
        Selection::EquivalentWithoutImprovement(NoImprovement::NotBelowIncumbent)
    );
    assert_eq!(
        select(original, cost(9, 250), cost(9, 250)),
        Selection::EquivalentWithoutImprovement(NoImprovement::IncumbentTie)
    );
    assert_eq!(
        select(original, original, original),
        Selection::EquivalentWithoutImprovement(NoImprovement::OriginalTie)
    );
    assert_eq!(
        select(original, cost(9, 250), cost(13, 100)),
        Selection::EquivalentWithoutImprovement(NoImprovement::OriginalGuard {
            nodes_over: true,
            bytes_over: false
        })
    );
    assert_eq!(
        select(original, cost(9, 250), cost(8, 301)),
        Selection::EquivalentWithoutImprovement(NoImprovement::OriginalGuard {
            nodes_over: false,
            bytes_over: true
        })
    );
    // One strict component against the original suffices on the first update.
    assert_eq!(
        select(original, original, cost(12, 299)),
        Selection::CheckedImprovement
    );
    assert_eq!(
        select(original, original, cost(11, 300)),
        Selection::CheckedImprovement
    );
    assert_eq!(
        Selection::EquivalentWithoutImprovement(NoImprovement::OriginalTie).json()["reason"],
        "tie-with-original"
    );
}

#[test]
fn a_checked_improvement_replaces_the_incumbent_and_regressions_do_not() {
    let original = artifact("boolean-kernel-original");
    let candidate = artifact("boolean-kernel-candidate");
    let mut session = Session::open(request(original, Profile::FunctionalBoolV1));
    assert_eq!(*session.incumbent(), Incumbent::OriginalAdmitted);
    assert_eq!(session.incumbent().status(), "no-checked-improvement");
    // A valid worse candidate (padded: more nodes and bytes) is equivalent without improvement.
    let padded = attempt(
        &mut session,
        Proposal::Candidate(artifact("boolean-kernel-padded").to_vec()),
    );
    assert!(matches!(
        padded,
        Outcome::Equivalent {
            selection: Selection::EquivalentWithoutImprovement(NoImprovement::OriginalGuard { .. }),
            ..
        }
    ));
    assert_eq!(*session.incumbent(), Incumbent::OriginalAdmitted);
    // The genuine improvement.
    let improved = attempt(&mut session, Proposal::Candidate(candidate.to_vec()));
    assert_eq!(
        improved,
        Outcome::Equivalent {
            cost: Cost {
                nodes: 7,
                bytes: 763
            },
            selection: Selection::CheckedImprovement
        }
    );
    let replaced = replacement(&session);
    assert_eq!(replaced.bytes(), candidate);
    assert_eq!(replaced.attempt(), 1);
    assert_eq!(
        transform::replay(replaced.receipt(), original, candidate, 16),
        transform::Replay::Matched
    );
    assert_eq!(session.incumbent().status(), "best-checked-so-far");
    assert!(equal_on_domain(
        session.request().program(),
        &incumbent_program(&session)
    ));
    // Resubmitting the incumbent is a duplicate of its attempt; no check is spent.
    assert_eq!(
        attempt(&mut session, Proposal::Candidate(candidate.to_vec())),
        Outcome::Duplicate(DuplicateOf::Attempt(1))
    );
    assert_eq!(
        attempt(&mut session, Proposal::Candidate(original.to_vec())),
        Outcome::Duplicate(DuplicateOf::Original)
    );
    // A regression after the improvement: the padded program again, now
    // against a cheaper incumbent, is still equivalent without improvement.
    let worse = attempt(
        &mut session,
        Proposal::Candidate(artifact("boolean-kernel-padded").to_vec()),
    );
    assert_eq!(worse, Outcome::Duplicate(DuplicateOf::Attempt(0)));
    assert_eq!(replacement(&session).bytes(), candidate);
    let report = session.report();
    assert_eq!(report["status"], "best-checked-so-far");
    assert_eq!(
        report["incumbent"]["cost"],
        json!({"nodes": 7, "bytes": 763})
    );
    assert_eq!(report["accounting"]["attempts"], 5);
    assert_eq!(report["accounting"]["checks"], 2);
    assert_eq!(report["accounting"]["complete"], true);
    assert!(
        report["claims"]["not_claimed"]
            .as_array()
            .is_some_and(|items| items.len() == 6)
    );

    // A valid tie: the same cost in other bytes keeps the incumbent (NSL-005:
    // no byte-only tie replacement).
    let (original, _) = fixture(&case("B16"));
    let mut session = Session::open(request(&original, Profile::FunctionalBoolV1));
    let p0 = import(&original);
    let swapped = bytes_of(&program(
        p0.inputs().to_vec(),
        p0.outputs().to_vec(),
        vec![Op::Input(1), Op::Input(0), Op::And(1, 0)],
        vec![2, 1, 2],
    ));
    assert_ne!(swapped, original);
    assert_eq!(
        attempt(&mut session, Proposal::Candidate(swapped)),
        Outcome::Equivalent {
            cost: session.request().cost(),
            selection: Selection::EquivalentWithoutImprovement(NoImprovement::OriginalTie)
        }
    );
    assert_eq!(*session.incumbent(), Incumbent::OriginalAdmitted);
}

#[test]
fn every_reachable_incumbent_equals_the_original_for_arbitrary_proposals() {
    let originals: Vec<(Vec<u8>, Vec<u8>, Profile)> = vec![
        (
            artifact("boolean-kernel-original").to_vec(),
            artifact("boolean-kernel-candidate").to_vec(),
            Profile::FunctionalBoolV1,
        ),
        {
            let (original, candidate) = fixture(&case("B05"));
            (original, candidate, Profile::FunctionalBoolV1)
        },
        {
            let (original, candidate) = fixture(&case("B07"));
            (original, candidate, Profile::FunctionalBoolV1)
        },
    ];
    let mut improvements = 0;
    let mut outcomes: Vec<String> = Vec::new();
    for seed in 1..=12_u64 {
        for (original, good, profile) in &originals {
            let mut rng = Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15));
            let mut session = Session::open(request(original, *profile));
            let p0 = session.request().program().clone();
            loop {
                let before_cost = session.incumbent().cost(session.request().cost());
                let before_bytes = session
                    .incumbent()
                    .replacement()
                    .map(|r| r.bytes().to_vec());
                let proposal = match rng.below(8) {
                    0 | 1 => Proposal::Candidate(bytes_of(&random_bool_program(
                        &mut rng,
                        p0.inputs().len(),
                        p0.outputs().len(),
                    ))),
                    2 => {
                        let mut bytes = good.clone();
                        let index = rng.below(bytes.len() as u64) as usize;
                        bytes[index] ^= 1 << rng.below(8);
                        Proposal::Candidate(bytes)
                    }
                    3 => {
                        Proposal::Candidate((0..rng.below(40)).map(|_| rng.next() as u8).collect())
                    }
                    4 => Proposal::Candidate(good.clone()),
                    5 => Proposal::Strategy(json!({"schema": "zeno-fcis/optimize-strategy/1",
                        "phases": [{"phase": "boolean", "rounds": 2}],
                        "extractor": "dag-greedy"})),
                    6 => Proposal::Failed(ProposerFailure::Timeout),
                    _ => Proposal::Candidate(original.clone()),
                };
                let ticket = match session.reserve_attempt(0) {
                    Ok(ticket) => ticket,
                    Err(StopReason::AttemptsExhausted) => break,
                    Err(reason) => panic!("unexpected stop {reason:?}"),
                };
                let outcome = match session.prepare(ticket, proposal) {
                    Prepared::Check(job) => {
                        // Sometimes the worker fails instead of completing.
                        let report = if rng.below(6) == 0 {
                            CheckReport::Failed(WorkerFailure::Panicked)
                        } else {
                            job.run()
                        };
                        session.conclude(job, report)
                    }
                    Prepared::Search(job) => match session.searched(
                        job,
                        NoEngine.run(
                            original,
                            &Strategy::from_json(
                                &json!({"schema": "zeno-fcis/optimize-strategy/1",
                        "phases": [{"phase": "boolean", "rounds": 2}],
                        "extractor": "dag-greedy"}),
                            )
                            .unwrap_or_else(|error| panic!("{error:?}")),
                            Profile::FunctionalBoolV1,
                            &[],
                        ),
                    ) {
                        Prepared::Settled(outcome) => outcome,
                        other => panic!("{other:?}"),
                    },
                    Prepared::Settled(outcome) => outcome,
                };
                outcomes.push(outcome.name());
                // Invariant: the incumbent equals P0 on the whole domain.
                let incumbent = incumbent_program(&session);
                assert!(equal_on_domain(&p0, &incumbent), "seed {seed}: {outcome:?}");
                let after_cost = session.incumbent().cost(session.request().cost());
                assert!(after_cost.nodes <= session.request().cost().nodes);
                assert!(after_cost.bytes <= session.request().cost().bytes);
                let improved = matches!(
                    outcome,
                    Outcome::Equivalent {
                        selection: Selection::CheckedImprovement,
                        ..
                    }
                );
                if improved {
                    improvements += 1;
                    assert!(
                        after_cost < before_cost,
                        "strict descent: {before_cost:?} -> {after_cost:?}"
                    );
                } else {
                    assert_eq!(after_cost, before_cost);
                    assert_eq!(
                        session
                            .incumbent()
                            .replacement()
                            .map(|r| r.bytes().to_vec()),
                        before_bytes,
                        "failure path changed the incumbent: {outcome:?}"
                    );
                }
                if let Status::Closed(_) = session.status() {
                    break;
                }
            }
            assert_eq!(
                *session.status(),
                Status::Closed(StopReason::AttemptsExhausted)
            );
            assert_eq!(session.ledger().accounting().attempts, 8);
            assert!(session.ledger().accounting().unresolved.is_empty());
        }
    }
    assert!(
        improvements > 0,
        "the planted good candidate was never selected"
    );
    for expected in [
        "refused",
        "different",
        "checked-improvement",
        "check-failed:panicked",
        "proposal-failed:timeout",
        "strategy-unavailable",
        "duplicate-of-original",
    ] {
        assert!(
            outcomes.iter().any(|name| name == expected),
            "no {expected} outcome in {outcomes:?}"
        );
    }
}

#[test]
fn eight_identical_invalid_proposals_consume_every_attempt_and_no_check() {
    let mut session = Session::open(request(
        artifact("boolean-kernel-original"),
        Profile::FunctionalBoolV1,
    ));
    for index in 0..8_u8 {
        let outcome = attempt(&mut session, Proposal::Candidate(b"not a program".to_vec()));
        assert_eq!(
            outcome,
            Outcome::Refused(AdmissionRefusal::NotAdmitted {
                code: "program-encoding".into()
            })
        );
        assert_eq!(session.attempts().len(), usize::from(index) + 1);
    }
    assert_eq!(
        *session.status(),
        Status::Closed(StopReason::AttemptsExhausted)
    );
    assert_eq!(
        session.reserve_attempt(0),
        Err(StopReason::AttemptsExhausted)
    );
    let totals = session.ledger().accounting();
    assert_eq!((totals.attempts, totals.checks, totals.settled), (8, 0, 8));
    assert_eq!(*session.incumbent(), Incumbent::OriginalAdmitted);
    assert_eq!(session.report()["status"], "no-checked-improvement");
    assert_eq!(
        session.report()["session"]["stop_reason"],
        "attempts-exhausted"
    );
}

#[test]
fn a_proposal_cannot_change_the_request() {
    let original = artifact("boolean-kernel-original");
    let mut session = Session::open(request(original, Profile::FunctionalBoolV1));
    let p0 = import(original);
    // Fewer inputs: a smaller domain with the same outputs.
    let narrowed = bytes_of(&program(
        vec![Domain::Bool; 3],
        p0.outputs().to_vec(),
        vec![Op::Input(0), Op::Input(1), Op::Input(2)],
        vec![0, 1, 2],
    ));
    assert_eq!(
        attempt(&mut session, Proposal::Candidate(narrowed)),
        Outcome::Refused(AdmissionRefusal::InputAbi)
    );
    // Same inputs, fewer outputs.
    let fewer_outputs = bytes_of(&program(
        p0.inputs().to_vec(),
        vec![Domain::Bool; 2],
        vec![Op::Input(0), Op::Input(1)],
        vec![0, 1],
    ));
    assert_eq!(
        attempt(&mut session, Proposal::Candidate(fewer_outputs)),
        Outcome::Refused(AdmissionRefusal::OutputAbi)
    );
    // An integer input is outside the profile before the ABI is compared.
    let integer = bytes_of(&program(
        vec![
            Domain::Bool,
            Domain::Bool,
            Domain::Bool,
            Domain::Int { min: 0, max: 1 },
        ],
        p0.outputs().to_vec(),
        vec![Op::Input(0)],
        vec![0, 0, 0],
    ));
    assert_eq!(
        attempt(&mut session, Proposal::Candidate(integer)),
        Outcome::Refused(AdmissionRefusal::Profile(ProfileRefusal::NonBooleanInput {
            position: 3
        }))
    );
    // An oversized artifact is refused before it is decoded or hashed.
    assert_eq!(
        attempt(&mut session, Proposal::Candidate(vec![0; 65_537])),
        Outcome::Refused(AdmissionRefusal::TooLarge {
            bytes: 65_537,
            max: 65_536
        })
    );
    assert_eq!(*session.incumbent(), Incumbent::OriginalAdmitted);
    assert_eq!(session.ledger().accounting().checks, 0);
}

#[test]
fn failure_paths_after_an_improvement_leave_the_incumbent_unchanged() {
    let original = artifact("boolean-kernel-original");
    let candidate = artifact("boolean-kernel-candidate");
    let mut session = Session::open(request(original, Profile::FunctionalBoolV1));
    attempt(&mut session, Proposal::Candidate(candidate.to_vec()));
    let before = replacement(&session).receipt().to_vec();
    let mut rng = Rng(7);
    let other = |rng: &mut Rng| {
        let p0 = import(original);
        bytes_of(&random_bool_program(
            rng,
            p0.inputs().len(),
            p0.outputs().len(),
        ))
    };
    // Timeout, panic and death of the check worker.
    for failure in [
        WorkerFailure::Timeout,
        WorkerFailure::Panicked,
        WorkerFailure::Died,
    ] {
        let ticket = session
            .reserve_attempt(0)
            .unwrap_or_else(|reason| panic!("{reason:?}"));
        let Prepared::Check(job) = session.prepare(ticket, Proposal::Candidate(other(&mut rng)))
        else {
            panic!("expected a check job");
        };
        let outcome = session.conclude(job, CheckReport::Failed(failure.clone()));
        assert_eq!(outcome, Outcome::CheckFailed(failure));
        assert_eq!(replacement(&session).receipt(), before.as_slice());
    }
    // A late report for an already settled stage changes nothing and is not an attempt.
    let ticket = session
        .reserve_attempt(0)
        .unwrap_or_else(|reason| panic!("{reason:?}"));
    let Prepared::Check(job) = session.prepare(ticket, Proposal::Candidate(other(&mut rng))) else {
        panic!("expected a check job");
    };
    let late_report = job.run();
    let attempts_before = session.ledger().accounting().attempts;
    assert!(matches!(
        session.conclude(job.clone(), CheckReport::Failed(WorkerFailure::Timeout)),
        Outcome::CheckFailed(WorkerFailure::Timeout)
    ));
    assert_eq!(session.conclude(job, late_report), Outcome::Late);
    assert_eq!(session.ledger().accounting().attempts, attempts_before);
    assert_eq!(replacement(&session).receipt(), before.as_slice());
    // A completed report for another job is not applied to this one.
    let ticket = session
        .reserve_attempt(0)
        .unwrap_or_else(|reason| panic!("{reason:?}"));
    let Prepared::Check(job_a) = session.prepare(
        ticket,
        Proposal::Candidate(artifact("boolean-kernel-padded").to_vec()),
    ) else {
        panic!("expected a check job");
    };
    let report_a = job_a.run();
    assert!(matches!(
        session.conclude(job_a, CheckReport::Failed(WorkerFailure::Died)),
        Outcome::CheckFailed(WorkerFailure::Died)
    ));
    let ticket = session
        .reserve_attempt(0)
        .unwrap_or_else(|reason| panic!("{reason:?}"));
    let Prepared::Check(job_b) = session.prepare(ticket, Proposal::Candidate(other(&mut rng)))
    else {
        panic!("expected a check job");
    };
    assert_eq!(
        session.conclude(job_b, report_a),
        Outcome::CheckFailed(WorkerFailure::Unavailable("report-for-another-job".into()))
    );
    assert_eq!(replacement(&session).receipt(), before.as_slice());
    assert!(equal_on_domain(
        session.request().program(),
        &incumbent_program(&session)
    ));
    // Closing with a pending check settles it as a timeout; the incumbent stands.
    let ticket = session
        .reserve_attempt(0)
        .unwrap_or_else(|reason| panic!("{reason:?}"));
    let Prepared::Check(_job) = session.prepare(ticket, Proposal::Candidate(other(&mut rng)))
    else {
        panic!("expected a check job");
    };
    session.close(StopReason::OperatorClosed);
    assert_eq!(
        *session.status(),
        Status::Closed(StopReason::OperatorClosed)
    );
    assert_eq!(
        session
            .attempts()
            .last()
            .map(|record| record.outcome.as_str()),
        Some("check-failed:timeout")
    );
    assert_eq!(replacement(&session).receipt(), before.as_slice());
    // Eight attempts, each with a check reservation, none refunded.
    let totals = session.ledger().accounting();
    assert_eq!(totals.attempts, 8);
    assert_eq!(totals.checks, 8);
    assert_eq!(totals.settled, 8);
    assert!(totals.unresolved.is_empty());
}

#[test]
fn model_call_allowances_are_reserved_before_dispatch_and_never_refunded() {
    let original = artifact("boolean-kernel-original");
    let mut session = Session::open(request(original, Profile::FunctionalBoolV1));
    // Four calls fit the token total exactly; each failed call keeps its charge.
    for call in 0..4_u8 {
        let ticket = session
            .reserve_attempt(0)
            .unwrap_or_else(|reason| panic!("{reason:?}"));
        let reserved = session
            .reserve_model_call(&ticket, 0)
            .unwrap_or_else(|reason| panic!("{reason:?}"));
        assert_eq!(reserved.attempt(), call);
        assert_eq!(
            session.ledger().accounting().tokens_reserved,
            u64::from(call + 1) * 6_144
        );
        assert!(matches!(
            session.prepare(
                ticket,
                Proposal::Failed(ProposerFailure::Malformed("garbage".into()))
            ),
            Prepared::Settled(Outcome::Failed(ProposerFailure::Malformed(_)))
        ));
        assert_eq!(
            session.ledger().accounting().tokens_reserved,
            u64::from(call + 1) * 6_144
        );
    }
    // The fifth call cannot be reserved: the attempt is consumed and the session closes.
    let ticket = session
        .reserve_attempt(0)
        .unwrap_or_else(|reason| panic!("{reason:?}"));
    assert_eq!(
        session.reserve_model_call(&ticket, 0),
        Err(StopReason::ModelCallsExhausted)
    );
    assert_eq!(
        *session.status(),
        Status::Closed(StopReason::ModelCallsExhausted)
    );
    assert!(matches!(
        session.prepare(
            ticket,
            Proposal::Candidate(artifact("boolean-kernel-candidate").to_vec())
        ),
        Prepared::Settled(Outcome::NotPending)
    ));
    let totals = session.ledger().accounting();
    assert_eq!(
        (totals.attempts, totals.model_calls, totals.tokens_reserved),
        (5, 4, 24_576)
    );
    assert_eq!(totals.settled, 5);
    assert_eq!(*session.incumbent(), Incumbent::OriginalAdmitted);
    // A lower token total binds first.
    let mut limits = Limits::CEILING;
    limits.total_tokens = 10_000;
    let mut session = Session::open(request_with(original, Profile::FunctionalBoolV1, limits));
    let ticket = session
        .reserve_attempt(0)
        .unwrap_or_else(|reason| panic!("{reason:?}"));
    assert!(session.reserve_model_call(&ticket, 0).is_ok());
    session.prepare(ticket, Proposal::Failed(ProposerFailure::Empty));
    let ticket = session
        .reserve_attempt(0)
        .unwrap_or_else(|reason| panic!("{reason:?}"));
    assert_eq!(
        session.reserve_model_call(&ticket, 0),
        Err(StopReason::TokensExhausted)
    );
    assert_eq!(session.ledger().accounting().tokens_reserved, 6_144);
}

#[test]
fn check_work_is_precharged_and_exhaustion_is_inconclusive_not_a_difference() {
    let original = artifact("boolean-kernel-original");
    let candidate = artifact("boolean-kernel-candidate");
    let request = request(original, Profile::FunctionalBoolV1);
    let candidate_cost = Cost::measure(&import(candidate), candidate);
    // 16 tuples * (16 + 7 + 2) + 1063 + 763 + 1024 receipt work.
    assert_eq!(
        request.check_work(candidate_cost),
        Some(16 * 25 + 1_063 + 763 + 1_024)
    );
    let self_check = request.check_work(request.cost()).unwrap_or_default();
    let mut limits = Limits::CEILING;
    limits.check_work = self_check;
    limits.session_work = self_check;
    let mut session = Session::open(request_with(original, Profile::FunctionalBoolV1, limits));
    // The padded candidate needs more than the per-check limit.
    let padded = attempt(
        &mut session,
        Proposal::Candidate(artifact("boolean-kernel-padded").to_vec()),
    );
    assert!(
        matches!(padded, Outcome::WorkExhausted { check_limit, .. } if check_limit == self_check),
        "{padded:?}"
    );
    assert_eq!(session.ledger().accounting().checks, 0);
    // The smaller candidate fits and is checked; afterwards the session work is spent.
    assert!(matches!(
        attempt(&mut session, Proposal::Candidate(candidate.to_vec())),
        Outcome::Equivalent {
            selection: Selection::CheckedImprovement,
            ..
        }
    ));
    let reserved = session.ledger().accounting().work_reserved;
    assert_eq!(reserved, 16 * 25 + 1_063 + 763 + 1_024);
    let swapped = {
        let incumbent = import(candidate);
        let mut roots = incumbent.roots().to_vec();
        roots.swap(0, 1);
        bytes_of(&program(
            incumbent.inputs().to_vec(),
            incumbent.outputs().to_vec(),
            incumbent.nodes().to_vec(),
            roots,
        ))
    };
    let exhausted = attempt(&mut session, Proposal::Candidate(swapped));
    assert!(
        matches!(exhausted, Outcome::WorkExhausted { session_remaining, .. } if session_remaining < reserved),
        "{exhausted:?}"
    );
    assert_eq!(
        session.ledger().accounting().work_reserved,
        reserved,
        "no refund, no new charge"
    );
    assert_eq!(replacement(&session).bytes(), candidate);
}

#[test]
fn the_deadline_stops_new_stages_and_a_checks_limit_below_attempts_binds() {
    let original = artifact("boolean-kernel-original");
    let mut session = Session::open(request(original, Profile::FunctionalBoolV1));
    assert_eq!(session.reserve_attempt(20_000), Err(StopReason::Deadline));
    assert_eq!(*session.status(), Status::Closed(StopReason::Deadline));
    assert_eq!(session.ledger().accounting().attempts, 0);
    let mut limits = Limits::CEILING;
    limits.checks = 1;
    let mut session = Session::open(request_with(original, Profile::FunctionalBoolV1, limits));
    assert!(matches!(
        attempt(
            &mut session,
            Proposal::Candidate(artifact("boolean-kernel-padded").to_vec())
        ),
        Outcome::Equivalent { .. }
    ));
    assert_eq!(
        attempt(
            &mut session,
            Proposal::Candidate(artifact("boolean-kernel-candidate").to_vec())
        ),
        Outcome::ChecksExhausted
    );
    assert_eq!(
        *session.status(),
        Status::Closed(StopReason::ChecksExhausted)
    );
    assert_eq!(*session.incumbent(), Incumbent::OriginalAdmitted);
}

#[test]
fn strategies_are_validated_data_and_unavailable_without_an_engine() {
    let original = artifact("boolean-kernel-original");
    let mut session = Session::open(request(original, Profile::FunctionalBoolV1));
    let valid = json!({
        "schema": "zeno-fcis/optimize-strategy/1",
        "phases": [{"phase": "fold", "rounds": 1}, {"phase": "boolean", "rounds": 3}],
        "limits": {"max_enodes": 20000, "max_extraction_rounds": 8, "max_work": 400},
        "extractor": "dag-greedy"
    });
    let strategy = Strategy::from_json(&valid).unwrap_or_else(|error| panic!("{error:?}"));
    assert_eq!(strategy.json(), valid);
    assert_eq!(strategy.profile, None);
    let mut named = valid.clone();
    named["profile"] = json!("functional-bool-v1");
    let bound = Strategy::from_json(&named).unwrap_or_else(|error| panic!("{error:?}"));
    assert_eq!(bound.profile, Some(Profile::FunctionalBoolV1));
    assert_eq!(bound.json(), named);
    assert_eq!(
        StrategyRefusal::Profile.json(),
        json!({"reason": "strategy-profile"})
    );
    assert_eq!(NoEngine.identity(), "none");
    assert!(NoEngine.phases().is_empty());
    let ticket = session
        .reserve_attempt(0)
        .unwrap_or_else(|reason| panic!("{reason:?}"));
    let Prepared::Search(job) = session.prepare(ticket, Proposal::Strategy(valid.clone())) else {
        panic!("expected a search job");
    };
    assert_eq!(job.strategy(), &strategy);
    let report = NoEngine.run(original, job.strategy(), Profile::FunctionalBoolV1, &[]);
    assert!(matches!(
        session.searched(job, report),
        Prepared::Settled(Outcome::StrategyUnavailable(_))
    ));
    for (index, (bad, expected)) in [
        (
            json!({"schema": "zeno-fcis/optimize-strategy/2", "phases": [], "extractor": "tree"}),
            StrategyRefusal::Schema,
        ),
        (
            json!({"schema": "zeno-fcis/optimize-strategy/1", "phases": [], "extractor": "tree"}),
            StrategyRefusal::Phases { count: 0 },
        ),
        (
            json!({"schema": "zeno-fcis/optimize-strategy/1", "phases": [{"phase": "../etc", "rounds": 1}], "extractor": "tree"}),
            StrategyRefusal::PhaseName { index: 0 },
        ),
        (
            json!({"schema": "zeno-fcis/optimize-strategy/1", "phases": [{"phase": "fold", "rounds": 0}], "extractor": "tree"}),
            StrategyRefusal::Rounds { index: 0, value: 0 },
        ),
        (
            json!({"schema": "zeno-fcis/optimize-strategy/1", "phases": [{"phase": "fold", "rounds": 9}], "extractor": "tree"}),
            StrategyRefusal::Rounds { index: 0, value: 9 },
        ),
        (
            json!({"schema": "zeno-fcis/optimize-strategy/1", "phases": [{"phase": "fold", "rounds": 1}], "extractor": "python:eval"}),
            StrategyRefusal::Extractor,
        ),
        (
            json!({"schema": "zeno-fcis/optimize-strategy/1", "phases": [{"phase": "fold", "rounds": 1}], "extractor": "tree", "callback": "/bin/sh"}),
            StrategyRefusal::Shape,
        ),
        (
            json!({"schema": "zeno-fcis/optimize-strategy/1", "phases": [{"phase": "fold", "rounds": 1}], "limits": {"threads": 64}, "extractor": "tree"}),
            StrategyRefusal::Limits,
        ),
        (
            json!({"schema": "zeno-fcis/optimize-strategy/1", "phases": [{"phase": "fold", "rounds": 1}], "limits": {"max_enodes": 100001}, "extractor": "tree"}),
            StrategyRefusal::Limit {
                field: "max_enodes",
                value: 100_001,
            },
        ),
        (
            json!({"schema": "zeno-fcis/optimize-strategy/1", "phases": [{"phase": "fold", "rounds": 1}], "extractor": "tree", "profile": "checked-i64-v2"}),
            StrategyRefusal::Profile,
        ),
        (
            json!({"schema": "zeno-fcis/optimize-strategy/1", "phases": [{"phase": "fold", "rounds": 1}], "extractor": "tree", "profile": null}),
            StrategyRefusal::Profile,
        ),
    ]
    .into_iter()
    .enumerate()
    {
        assert_eq!(Strategy::from_json(&bad), Err(expected.clone()));
        // Seven of them also spend the session's remaining attempts.
        if index < 7 {
            assert_eq!(
                attempt(&mut session, Proposal::Strategy(bad)),
                Outcome::Refused(AdmissionRefusal::Strategy(expected))
            );
        }
    }
    // An engine's emitted bytes enter the ordinary acceptance path (F4 wiring point).
    struct FakeEngine(Vec<u8>);
    impl StrategyEngine for FakeEngine {
        fn identity(&self) -> &str {
            "fake-engine"
        }
        fn phases(&self) -> &[&str] {
            &["fold", "boolean"]
        }
        fn run(
            &self,
            _original: &[u8],
            _strategy: &Strategy,
            _profile: Profile,
            _checked: &[Vec<u8>],
        ) -> SearchReport {
            SearchReport::Candidate {
                bytes: self.0.clone(),
                extraction: "unknown-optimality".into(),
            }
        }
    }
    assert_eq!(
        *session.status(),
        Status::Closed(StopReason::AttemptsExhausted)
    );
    let mut session = Session::open(request(original, Profile::FunctionalBoolV1));
    let ticket = session
        .reserve_attempt(0)
        .unwrap_or_else(|reason| panic!("{reason:?}"));
    let Prepared::Search(job) = session.prepare(ticket, Proposal::Strategy(valid.clone())) else {
        panic!("expected a search job");
    };
    let engine = FakeEngine(artifact("boolean-kernel-candidate").to_vec());
    let Prepared::Check(check) = session.searched(
        job,
        engine.run(original, &strategy, Profile::FunctionalBoolV1, &[]),
    ) else {
        panic!("engine bytes must reach the checker");
    };
    let report = check.run();
    assert!(matches!(
        session.conclude(check, report),
        Outcome::Equivalent {
            selection: Selection::CheckedImprovement,
            ..
        }
    ));
    assert_eq!(
        session.attempts()[0].extraction.as_deref(),
        Some("unknown-optimality")
    );
    // Garbage from an engine is refused like any other candidate.
    let ticket = session
        .reserve_attempt(0)
        .unwrap_or_else(|reason| panic!("{reason:?}"));
    let Prepared::Search(job) = session.prepare(ticket, Proposal::Strategy(valid)) else {
        panic!("expected a search job");
    };
    assert!(matches!(
        session.searched(
            job,
            FakeEngine(b"partial".to_vec()).run(
                original,
                &strategy,
                Profile::FunctionalBoolV1,
                &[]
            )
        ),
        Prepared::Settled(Outcome::Refused(AdmissionRefusal::NotAdmitted { .. }))
    ));
}

#[test]
fn witness_replay_rejects_spoofed_stale_and_nonreproducing_witnesses() {
    let original = artifact("boolean-kernel-original");
    let candidate = import(artifact("boolean-kernel-candidate"));
    let mut roots = candidate.roots().to_vec();
    roots.swap(0, 1);
    let swapped = bytes_of(&program(
        candidate.inputs().to_vec(),
        candidate.outputs().to_vec(),
        candidate.nodes().to_vec(),
        roots,
    ));
    let mut session = Session::open(request(original, Profile::FunctionalBoolV1));
    let Outcome::Different(witness) = attempt(&mut session, Proposal::Candidate(swapped.clone()))
    else {
        panic!("expected a difference");
    };
    let request = session.request();
    assert_eq!(witness.ordinal, 1);
    assert_eq!(witness.input, [0, 0, 0, 1]);
    assert_eq!(replay_witness(request, &swapped, &witness), Ok(()));
    let inputs = request.program().inputs();
    let outputs = request.program().outputs();
    let json = witness.json(inputs, outputs);
    assert_eq!(json["schema"], "zeno-fcis/transform-witness/1");
    assert_eq!(json["input"], json!([false, false, false, true]));
    assert_eq!(
        Witness::from_json(&json, inputs, outputs),
        Some(witness.clone())
    );
    let mut extra = json.clone();
    extra["explanation"] = json!("the model says this is fine");
    assert_eq!(Witness::from_json(&extra, inputs, outputs), None);
    // Stale: names other candidate bytes.
    let mut stale = witness.clone();
    stale.candidate_sha256 = sha256_hex(artifact("boolean-kernel-candidate"));
    assert_eq!(
        replay_witness(request, &swapped, &stale),
        Err(WitnessFault::Candidate)
    );
    assert_eq!(
        replay_witness(request, artifact("boolean-kernel-candidate"), &witness),
        Err(WitnessFault::Candidate)
    );
    let mut other_request = witness.clone();
    other_request.request_id = "0".repeat(64);
    assert_eq!(
        replay_witness(request, &swapped, &other_request),
        Err(WitnessFault::Request)
    );
    let mut tuple = witness.clone();
    tuple.input = vec![0, 0, 1, 0];
    assert_eq!(
        replay_witness(request, &swapped, &tuple),
        Err(WitnessFault::Tuple)
    );
    let mut ordinal = witness.clone();
    ordinal.ordinal = 16;
    assert_eq!(
        replay_witness(request, &swapped, &ordinal),
        Err(WitnessFault::Ordinal)
    );
    let mut forged = witness.clone();
    forged.candidate.result = Ok(vec![0, 1, 1]);
    assert_eq!(
        replay_witness(request, &swapped, &forged),
        Err(WitnessFault::Observations)
    );
    let mut steps = witness.clone();
    steps.original.steps += 1;
    assert_eq!(
        replay_witness(request, &swapped, &steps),
        Err(WitnessFault::Observations)
    );
    // A tuple where both agree is not a witness even with matching observations.
    let agreeing = Witness {
        ordinal: 0,
        input: vec![0, 0, 0, 0],
        original: observe(request.program(), &[0, 0, 0, 0]),
        candidate: observe(&import(&swapped), &[0, 0, 0, 0]),
        ..witness.clone()
    };
    assert_eq!(
        replay_witness(request, &swapped, &agreeing),
        Err(WitnessFault::NoDifference)
    );
    // Feedback carries the verified witness, labeled as such.
    let feedback = session.feedback();
    assert!(
        matches!(&feedback[0], Feedback::Difference { attempt: 0, witness: found } if *found == witness)
    );
    assert_eq!(
        feedback[0].json(inputs, outputs)["kind"],
        "verified-difference"
    );
}

#[test]
fn forged_receipts_and_mismatched_equivalences_cannot_build_a_replacement() {
    let original = artifact("boolean-kernel-original");
    let candidate = artifact("boolean-kernel-candidate");
    let padded = artifact("boolean-kernel-padded");
    let request = request(original, Profile::FunctionalBoolV1);
    let equivalence = transform::check(original, candidate, request.transform_limits())
        .unwrap_or_else(|rejection| panic!("{rejection:?}"));
    let cost = Cost::measure(&import(candidate), candidate);
    assert!(Replacement::from_equivalence(&equivalence, original, candidate, cost, 0).is_ok());
    // The equivalence of (P0, candidate) does not bind other candidate bytes.
    let padded_cost = Cost::measure(&import(padded), padded);
    assert_eq!(
        Replacement::from_equivalence(&equivalence, original, padded, padded_cost, 0)
            .map(|_| ())
            .map_err(|mismatch| mismatch.field),
        Err("candidate.sha256")
    );
    // Nor another original.
    assert_eq!(
        Replacement::from_equivalence(&equivalence, candidate, candidate, cost, 0)
            .map(|_| ())
            .map_err(|mismatch| mismatch.field),
        Err("original")
    );
    // A wrong cost label is refused too.
    assert_eq!(
        Replacement::from_equivalence(
            &equivalence,
            original,
            candidate,
            Cost {
                nodes: 1,
                bytes: cost.bytes
            },
            0
        )
        .map(|_| ())
        .map_err(|mismatch| mismatch.field),
        Err("candidate.nodes")
    );
}

#[test]
fn ledger_chain_detects_edits_reorders_and_unknown_fields() {
    let mut ledger = Ledger::new();
    ledger.append(Kind::Opened {
        request_id: "r".repeat(64),
        checker: CheckerIdentity::current(),
    });
    ledger.append(Kind::Reserved {
        attempt: Some(0),
        stage: Stage::Attempt,
    });
    ledger.append(Kind::Reserved {
        attempt: Some(0),
        stage: Stage::Check { work: 42 },
    });
    ledger.append(Kind::Settled {
        attempt: 0,
        outcome: "different".into(),
        candidate_sha256: Some("c".repeat(64)),
        cost: Some(Cost {
            nodes: 3,
            bytes: 40,
        }),
    });
    let lines: Vec<u8> = ledger.entries().iter().flat_map(Entry::line).collect();
    let parsed = Ledger::parse_lines(&lines).unwrap_or_else(|fault| panic!("{fault:?}"));
    assert_eq!(Ledger::from_entries(parsed.clone()), Ok(ledger.clone()));
    assert_eq!(ledger.head().entries, 4);
    let totals = ledger.accounting();
    assert_eq!(
        (
            totals.attempts,
            totals.checks,
            totals.work_reserved,
            totals.settled
        ),
        (1, 1, 42, 1)
    );
    assert!(totals.unresolved.is_empty());
    // An edited amount breaks the digest.
    let mut edited = parsed.clone();
    edited[2].kind = Kind::Reserved {
        attempt: Some(0),
        stage: Stage::Check { work: 1 },
    };
    assert_eq!(
        Ledger::from_entries(edited),
        Err(LedgerFault::Digest { line: 2 })
    );
    // A removed middle entry breaks the link.
    let mut removed = parsed.clone();
    removed.remove(1);
    assert_eq!(
        Ledger::from_entries(removed),
        Err(LedgerFault::Sequence { line: 1 })
    );
    let mut reordered = parsed.clone();
    reordered.swap(1, 2);
    assert_eq!(
        Ledger::from_entries(reordered),
        Err(LedgerFault::Sequence { line: 1 })
    );
    // Unknown fields and a foreign schema are unreadable.
    let mut extra: Value = serde_json::from_slice(&parsed[1].line()).unwrap_or_default();
    extra["refund"] = json!(true);
    assert_eq!(Entry::from_json(&extra), None);
    let mut text = String::from_utf8(lines.clone()).unwrap_or_default();
    text.push_str("{\"schema\":\"other\"}\n");
    assert_eq!(
        Ledger::parse_lines(text.as_bytes()),
        Err(LedgerFault::Unreadable { line: 4 })
    );
    // Nothing may follow a close; the first entry must open.
    let mut closed = ledger.clone();
    closed.append(Kind::Closed {
        reason: "deadline".into(),
    });
    closed.append(Kind::Reserved {
        attempt: Some(1),
        stage: Stage::Attempt,
    });
    assert_eq!(
        Ledger::from_entries(closed.entries().to_vec()),
        Err(LedgerFault::AfterClose { line: 5 })
    );
    // A replay charge after the close is the one permitted continuation.
    let mut replayed = ledger.clone();
    replayed.append(Kind::Closed {
        reason: "deadline".into(),
    });
    replayed.append(Kind::Reserved {
        attempt: None,
        stage: Stage::Replay { work: 7 },
    });
    assert_eq!(
        Ledger::from_entries(replayed.entries().to_vec()),
        Ok(replayed.clone())
    );
    assert_eq!(replayed.accounting().replays, 1);
    assert_eq!(replayed.accounting().closed.as_deref(), Some("deadline"));
    let mut unopened = Ledger::new();
    unopened.append(Kind::Reserved {
        attempt: Some(0),
        stage: Stage::Attempt,
    });
    assert_eq!(
        Ledger::from_entries(unopened.entries().to_vec()),
        Err(LedgerFault::Opened { line: 0 })
    );
    assert_eq!(Head::from_json(&ledger.head().json()), Some(ledger.head()));
}

/// Everything the shell would persist for a session.
struct Persisted {
    request: Vec<u8>,
    original: Vec<u8>,
    ledger: Vec<u8>,
    head: Option<Head>,
    replacement: Option<(Vec<u8>, Vec<u8>)>,
    witnesses: Vec<(u8, Value, Vec<u8>)>,
}

impl Persisted {
    fn of(session: &Session) -> Persisted {
        Persisted {
            request: session.request().canonical_bytes().to_vec(),
            original: session.request().original().to_vec(),
            ledger: session
                .ledger()
                .entries()
                .iter()
                .flat_map(Entry::line)
                .collect(),
            head: Some(session.ledger().head()),
            replacement: session
                .incumbent()
                .replacement()
                .map(|replacement| (replacement.bytes().to_vec(), replacement.receipt().to_vec())),
            witnesses: Vec::new(),
        }
    }

    fn stored(&self) -> Stored<'_> {
        Stored {
            request: &self.request,
            original: &self.original,
            ledger: &self.ledger,
            head: self.head.clone(),
            replacement: self
                .replacement
                .as_ref()
                .map(|(candidate, receipt)| (candidate.as_slice(), receipt.as_slice())),
            witnesses: self.witnesses.clone(),
        }
    }
}

fn resumed_session(persisted: &Persisted) -> Session {
    match Session::resume(persisted.stored(), 0) {
        Resumed::Session(session) => *session,
        other => panic!("expected a resumed session, found {other:?}"),
    }
}

fn resume_refusal(persisted: &Persisted) -> ResumeRefusal {
    match Session::resume(persisted.stored(), 0) {
        Resumed::Refused(reason, _) | Resumed::Inconclusive(reason, _) => reason,
        Resumed::Session(_) => panic!("resume must not yield a trusted incumbent"),
    }
}

#[test]
fn resume_replays_the_replacement_charges_the_replay_and_continues_the_session() {
    let original = artifact("boolean-kernel-original");
    let candidate = artifact("boolean-kernel-candidate");
    let mut session = Session::open(request(original, Profile::FunctionalBoolV1));
    attempt(
        &mut session,
        Proposal::Candidate(artifact("boolean-kernel-padded").to_vec()),
    );
    attempt(&mut session, Proposal::Candidate(candidate.to_vec()));
    let before = session.ledger().accounting();
    let persisted = Persisted::of(&session);
    let resumed = resumed_session(&persisted);
    assert_eq!(resumed.incumbent(), session.incumbent());
    assert_eq!(resumed.request(), session.request());
    assert_eq!(*resumed.status(), Status::Open);
    let after = resumed.ledger().accounting();
    assert_eq!(after.attempts, before.attempts);
    assert_eq!(after.replays, 1);
    assert_eq!(
        after.work_reserved,
        before.work_reserved
            + session
                .request()
                .check_work(Cost {
                    nodes: 7,
                    bytes: 763
                })
                .unwrap_or_default()
    );
    assert_eq!(
        resumed.ledger().entries().len(),
        session.ledger().entries().len() + 1
    );
    assert_eq!(resumed.attempts().len(), 2);
    assert!(matches!(
        resumed.attempts()[1].feedback,
        Feedback::Equivalent {
            selection: Selection::CheckedImprovement,
            ..
        }
    ));
    // The session continues: duplicates of earlier candidates are still known.
    let mut resumed = resumed;
    assert_eq!(
        attempt(&mut resumed, Proposal::Candidate(candidate.to_vec())),
        Outcome::Duplicate(DuplicateOf::Attempt(1))
    );
    assert_eq!(resumed.attempts()[2].attempt, 2);
    // A closed session resumes closed, for reporting.
    session.close(StopReason::OperatorClosed);
    let closed = resumed_session(&Persisted::of(&session));
    assert_eq!(*closed.status(), Status::Closed(StopReason::OperatorClosed));
    assert_eq!(closed.report()["session"]["stop_reason"], "operator-closed");
}

#[test]
fn stale_or_tampered_resume_yields_no_trusted_incumbent() {
    let original = artifact("boolean-kernel-original");
    let candidate = artifact("boolean-kernel-candidate");
    let mut session = Session::open(request(original, Profile::FunctionalBoolV1));
    attempt(&mut session, Proposal::Candidate(candidate.to_vec()));
    let persisted = Persisted::of(&session);
    assert!(matches!(
        Session::resume(persisted.stored(), 0),
        Resumed::Session(_)
    ));
    // Stale receipt: one byte differs from the receipt the ledger binds.
    let mut stale = Persisted::of(&session);
    if let Some((_, receipt)) = &mut stale.replacement {
        receipt[10] ^= 0x01;
    }
    assert_eq!(resume_refusal(&stale), ResumeRefusal::ReplacementDigest);
    // Forged receipt with a consistently rewritten ledger and head (a host
    // integrity breach): the binding passes, the replay does not, and the
    // replay charge is still recorded.
    let mut forged_receipt = replacement(&session).receipt().to_vec();
    forged_receipt[10] ^= 0x01;
    let forged_digest = sha256_hex(&forged_receipt);
    let mut rewritten = Ledger::new();
    for entry in session.ledger().entries() {
        rewritten.append(match &entry.kind {
            Kind::Replaced {
                attempt,
                candidate_sha256,
                cost,
                ..
            } => Kind::Replaced {
                attempt: *attempt,
                candidate_sha256: candidate_sha256.clone(),
                receipt_sha256: forged_digest.clone(),
                cost: *cost,
            },
            other => other.clone(),
        });
    }
    let mut forged = Persisted::of(&session);
    forged.ledger = rewritten.entries().iter().flat_map(Entry::line).collect();
    forged.head = Some(rewritten.head());
    forged.replacement = Some((candidate.to_vec(), forged_receipt));
    match Session::resume(forged.stored(), 0) {
        Resumed::Refused(ResumeRefusal::ReceiptMismatch(detail), Some(ledger)) => {
            assert_eq!(detail, "receipt-bytes");
            assert_eq!(ledger.accounting().replays, 1);
            assert!(ledger.accounting().work_reserved > rewritten.accounting().work_reserved);
        }
        other => panic!("{other:?}"),
    }
    // Substituted candidate bytes.
    let mut substituted = Persisted::of(&session);
    if let Some((bytes, _)) = &mut substituted.replacement {
        *bytes = artifact("boolean-kernel-padded").to_vec();
    }
    assert_eq!(
        resume_refusal(&substituted),
        ResumeRefusal::ReplacementDigest
    );
    let mut missing = Persisted::of(&session);
    missing.replacement = None;
    assert_eq!(resume_refusal(&missing), ResumeRefusal::ReplacementMissing);
    // No head: continuity is unverifiable.
    let mut unverifiable = Persisted::of(&session);
    unverifiable.head = None;
    assert_eq!(resume_refusal(&unverifiable), ResumeRefusal::Unverifiable);
    // Rollback: the ledger lost its last entries while the head is current.
    let mut rolled_back = Persisted::of(&session);
    let lines: Vec<&[u8]> = rolled_back
        .ledger
        .split(|byte| *byte == b'\n')
        .filter(|line| !line.is_empty())
        .collect();
    let shorter: Vec<u8> = lines[..lines.len() - 2]
        .iter()
        .flat_map(|line| [*line, b"\n"].concat())
        .collect();
    rolled_back.ledger = shorter;
    assert!(matches!(
        resume_refusal(&rolled_back),
        ResumeRefusal::RollbackSuspected { .. }
    ));
    // An old head with a longer ledger is equally suspect.
    let mut old_head = Persisted::of(&session);
    old_head.head = Some(Head {
        entries: 1,
        digest: session.ledger().entries()[0].digest.clone(),
    });
    assert!(matches!(
        resume_refusal(&old_head),
        ResumeRefusal::RollbackSuspected { .. }
    ));
    // An edited ledger entry.
    let mut edited = Persisted::of(&session);
    let text = String::from_utf8(edited.ledger.clone()).unwrap_or_default();
    edited.ledger = text.replace("\"work\":", "\"work\":1").into_bytes();
    assert!(matches!(resume_refusal(&edited), ResumeRefusal::Ledger(_)));
    // Stale checker in the stored request.
    let mut stale_checker = Persisted::of(&session);
    let mut record: Value = serde_json::from_slice(&stale_checker.request).unwrap_or_default();
    record["checker"]["semantics"] = json!("zeno-fcis/transform-check/0");
    stale_checker.request = super::canonical_json(&record);
    assert!(matches!(
        resume_refusal(&stale_checker),
        ResumeRefusal::Request(ReadmitRefusal::StaleChecker { .. })
    ));
    // Changed original bytes under the same record.
    let mut changed = Persisted::of(&session);
    changed.original = artifact("boolean-kernel-padded").to_vec();
    assert_eq!(
        resume_refusal(&changed),
        ResumeRefusal::Request(ReadmitRefusal::Mismatch)
    );
    // A ledger opened for another request.
    let other = Session::open(request(
        artifact("boolean-kernel-padded"),
        Profile::FunctionalBoolV1,
    ));
    let mut foreign = Persisted::of(&session);
    foreign.ledger = other
        .ledger()
        .entries()
        .iter()
        .flat_map(Entry::line)
        .collect();
    foreign.head = Some(other.ledger().head());
    foreign.replacement = None;
    assert_eq!(resume_refusal(&foreign), ResumeRefusal::RequestMismatch);
    // Insufficient replay allowance: the session work is spent.
    let self_check = session
        .request()
        .check_work(session.request().cost())
        .unwrap_or_default();
    let mut limits = Limits::CEILING;
    limits.check_work = self_check;
    limits.session_work = self_check;
    let mut tight = Session::open(request_with(original, Profile::FunctionalBoolV1, limits));
    attempt(&mut tight, Proposal::Candidate(candidate.to_vec()));
    assert!(tight.incumbent().replacement().is_some());
    assert_eq!(
        resume_refusal(&Persisted::of(&tight)),
        ResumeRefusal::InsufficientReplayAllowance
    );
    // Deadline already reached.
    assert!(matches!(
        Session::resume(persisted.stored(), 20_000),
        Resumed::Inconclusive(ResumeRefusal::Deadline, None)
    ));
}

#[test]
fn a_crash_between_reservation_and_reply_keeps_the_charge_and_the_prior_incumbent() {
    let original = artifact("boolean-kernel-original");
    let candidate = artifact("boolean-kernel-candidate");
    let mut session = Session::open(request(original, Profile::FunctionalBoolV1));
    attempt(&mut session, Proposal::Candidate(candidate.to_vec()));
    // Reserve the next attempt and its check, then "die" before concluding.
    let ticket = session
        .reserve_attempt(0)
        .unwrap_or_else(|reason| panic!("{reason:?}"));
    let Prepared::Check(_job) = session.prepare(
        ticket,
        Proposal::Candidate(artifact("boolean-kernel-padded").to_vec()),
    ) else {
        panic!("expected a check job");
    };
    let persisted = Persisted::of(&session);
    drop(session);
    let resumed = resumed_session(&persisted);
    let totals = resumed.ledger().accounting();
    assert_eq!(totals.attempts, 2);
    assert_eq!(totals.checks, 1 + 1);
    assert_eq!(totals.unresolved, vec![1]);
    assert_eq!(resumed.report()["accounting"]["complete"], false);
    assert_eq!(replacement(&resumed).bytes(), candidate);
    // The next attempt index continues after the crash-pending one.
    let mut resumed = resumed;
    let ticket = resumed
        .reserve_attempt(0)
        .unwrap_or_else(|reason| panic!("{reason:?}"));
    assert_eq!(ticket.attempt(), 2);
    assert!(matches!(
        resumed.prepare(ticket, Proposal::Failed(ProposerFailure::Empty)),
        Prepared::Settled(Outcome::Failed(ProposerFailure::Empty))
    ));
    assert_eq!(resumed.ledger().accounting().unresolved, vec![1]);
}

#[test]
fn stored_witnesses_are_replayed_before_reuse_and_dropped_when_they_do_not_reproduce() {
    let original = artifact("boolean-kernel-original");
    let candidate = import(artifact("boolean-kernel-candidate"));
    let mut roots = candidate.roots().to_vec();
    roots.swap(0, 1);
    let swapped = bytes_of(&program(
        candidate.inputs().to_vec(),
        candidate.outputs().to_vec(),
        candidate.nodes().to_vec(),
        roots,
    ));
    let mut session = Session::open(request(original, Profile::FunctionalBoolV1));
    let Outcome::Different(witness) = attempt(&mut session, Proposal::Candidate(swapped.clone()))
    else {
        panic!("expected a difference");
    };
    let inputs = session.request().program().inputs();
    let outputs = session.request().program().outputs();
    let mut persisted = Persisted::of(&session);
    persisted
        .witnesses
        .push((0, witness.json(inputs, outputs), swapped.clone()));
    let resumed = resumed_session(&persisted);
    assert!(
        matches!(&resumed.attempts()[0].feedback, Feedback::Difference { witness: found, .. } if *found == witness)
    );
    assert_eq!(resumed.ledger().accounting().replays, 1);
    // A forged stored witness is dropped and reported, not reused.
    let mut forged = witness.json(inputs, outputs);
    forged["observations"]["candidate"]["ok"] = json!([false, true, true]);
    let mut persisted = Persisted::of(&session);
    persisted.witnesses.push((0, forged, swapped.clone()));
    let resumed = resumed_session(&persisted);
    assert!(matches!(
        &resumed.attempts()[0].feedback,
        Feedback::Incomplete { .. }
    ));
    assert_eq!(resumed.report()["witnesses_dropped_on_resume"], json!([0]));
    // A witness stored against other candidate bytes is dropped too.
    let mut persisted = Persisted::of(&session);
    persisted.witnesses.push((
        0,
        witness.json(inputs, outputs),
        artifact("boolean-kernel-candidate").to_vec(),
    ));
    assert_eq!(
        resumed_session(&persisted).report()["witnesses_dropped_on_resume"],
        json!([0])
    );
}

#[test]
fn the_local_proposer_simplifies_and_its_candidates_are_judged_by_the_checker() {
    let mut improved = Vec::new();
    let mut rejected = Vec::new();
    let mut exhausted = Vec::new();
    for id in [
        "B01", "B02", "B09", "B10", "B15", "B16", "I01", "I02", "I07", "I10", "I11",
    ] {
        let case = case(id);
        let (original, _) = fixture(&case);
        let request = request(&original, profile_of(&case));
        let mut proposer = LocalProposer::new(request.program(), &original);
        let mut session = Session::open(request);
        loop {
            let Some(candidate) = proposer.next() else {
                exhausted.push(id);
                break;
            };
            match attempt(&mut session, Proposal::Candidate(candidate)) {
                Outcome::Equivalent {
                    selection: Selection::CheckedImprovement,
                    ..
                } => improved.push(id),
                Outcome::Different(_) => rejected.push(id),
                other => panic!("{id}: {other:?}"),
            }
        }
        assert!(
            equal_on_domain(session.request().program(), &incumbent_program(&session)),
            "{id}"
        );
    }
    // B09's original ends in a double negation, which the fixpoint removes.
    assert_eq!(improved, ["B01", "B09", "B10", "I01", "I02", "I07"]);
    // Removing a dead overflowing instruction (I10) is proposed by the
    // aggressive variant and rejected by the checker with a replayed witness.
    // I11's overflowing arm is reachable through its Select, so no rewrite
    // touches it and the proposer has nothing to offer.
    assert_eq!(rejected, ["I10"]);
    assert_eq!(exhausted.len(), 11);
    assert_eq!(super::local::IDENTITY, "zeno-fcis/local-proposer/1");
    // The application kernel: two double negations fold, 16 -> 14 nodes, well
    // short of the model-found 7-node candidate.
    let kernel = artifact("boolean-kernel-original");
    let mut proposer = LocalProposer::new(&import(kernel), kernel);
    assert_eq!(proposer.remaining(), 1);
    let mut session = Session::open(request(kernel, Profile::FunctionalBoolV1));
    let candidate = proposer.next().unwrap_or_else(|| panic!("one candidate"));
    assert!(matches!(
        attempt(&mut session, Proposal::Candidate(candidate)),
        Outcome::Equivalent {
            cost: Cost { nodes: 14, .. },
            selection: Selection::CheckedImprovement
        }
    ));
    assert_eq!(proposer.next(), None);
}

#[test]
fn the_benchmark_catalog_runs_through_the_loop_with_its_own_candidates() {
    let mut improvements = Vec::new();
    let mut differences = Vec::new();
    let mut without_improvement = Vec::new();
    for case in cases() {
        let id = case["id"].as_str().unwrap_or_default().to_owned();
        let (original, candidate) = fixture(&case);
        let mut session = Session::open(request(&original, profile_of(&case)));
        let outcome = attempt(&mut session, Proposal::Candidate(candidate));
        match (case["expected_relation"].as_str(), outcome) {
            (Some("Equivalent"), Outcome::Equivalent { selection, .. }) => match selection {
                Selection::CheckedImprovement => improvements.push(id.clone()),
                Selection::EquivalentWithoutImprovement(_) => without_improvement.push(id.clone()),
            },
            (Some("Different"), Outcome::Different(witness)) => {
                assert_eq!(json!(witness.ordinal), case["witness"]["ordinal"], "{id}");
                differences.push(id.clone());
            }
            (Some("Equivalent"), Outcome::Duplicate(DuplicateOf::Original)) => {
                without_improvement.push(id.clone())
            }
            (expected, outcome) => panic!("{id}: expected {expected:?}, found {outcome:?}"),
        }
        assert!(
            equal_on_domain(session.request().program(), &incumbent_program(&session)),
            "{id}"
        );
    }
    assert_eq!(improvements.len(), 17, "{improvements:?}");
    assert_eq!(differences.len(), 11, "{differences:?}");
    assert_eq!(without_improvement, ["B15", "B16", "I15", "I16"]);
}

#[test]
fn program_json_round_trips_and_is_strict() {
    for case in cases() {
        let (original, candidate) = fixture(&case);
        for bytes in [original, candidate] {
            let program = import(&bytes);
            let json = program_to_json(&program);
            let rebuilt = program_from_json(&json).unwrap_or_else(|error| panic!("{error:?}"));
            assert_eq!(bytes_of(&rebuilt), bytes);
        }
    }
    let bad = [
        json!({"inputs": [], "outputs": [{"kind": "Bool"}], "nodes": [["Bool", true]], "roots": [0], "passed": true}),
        json!({"inputs": [{"kind": "Int", "min": "01", "max": "2"}], "outputs": [{"kind": "Bool"}], "nodes": [["Bool", true]], "roots": [0]}),
        json!({"inputs": [], "outputs": [{"kind": "Bool"}], "nodes": [["Or", 0, 0]], "roots": [0]}),
        json!({"inputs": [], "outputs": [{"kind": "Bool"}], "nodes": [["Bool", true]], "roots": [1]}),
        json!({"inputs": [], "outputs": [{"kind": "Bool"}], "nodes": [["Int", "-0"]], "roots": [0]}),
    ];
    for value in bad {
        assert!(program_from_json(&value).is_err(), "{value}");
    }
}

#[test]
fn reports_and_outcomes_render_their_closed_vocabulary() {
    let original = artifact("boolean-kernel-original");
    let request = request(original, Profile::FunctionalBoolV1);
    let session = Session::open(request.clone());
    let report = session.report();
    assert_eq!(report["schema"], "zeno-fcis/transform-loop-report/1");
    assert_eq!(report["authority"], "none");
    assert_eq!(report["status"], "no-checked-improvement");
    assert_eq!(report["incumbent"]["receipt"], "none");
    assert_eq!(report["session"], json!({"state": "open"}));
    assert_eq!(report["ledger_head"]["entries"], 1);
    for (outcome, name) in [
        (
            Outcome::Failed(ProposerFailure::Timeout),
            "proposal-failed:timeout",
        ),
        (
            Outcome::Duplicate(DuplicateOf::Attempt(3)),
            "duplicate-of-attempt:3",
        ),
        (Outcome::ChecksExhausted, "checks-exhausted"),
        (
            Outcome::CheckFailed(WorkerFailure::Died),
            "check-failed:died",
        ),
        (Outcome::Late, "late"),
        (
            Outcome::Equivalent {
                cost: Cost { nodes: 1, bytes: 1 },
                selection: Selection::EquivalentWithoutImprovement(NoImprovement::IncumbentTie),
            },
            "equivalent-without-improvement:tie-with-incumbent",
        ),
    ] {
        assert_eq!(outcome.name(), name);
        assert_eq!(outcome.json(&request)["outcome"], name);
    }
    assert!(!Outcome::Late.settled());
    assert!(Outcome::ChecksExhausted.settled());
}

/// `CheckJob` is the only way to a completed report; this guards the type-level
/// claim with a value-level check of its fields.
#[test]
fn a_check_job_binds_its_candidate_and_work() {
    let original = artifact("boolean-kernel-original");
    let candidate = artifact("boolean-kernel-candidate");
    let mut session = Session::open(request(original, Profile::FunctionalBoolV1));
    let ticket = session
        .reserve_attempt(0)
        .unwrap_or_else(|reason| panic!("{reason:?}"));
    let Prepared::Check(job) = session.prepare(ticket, Proposal::Candidate(candidate.to_vec()))
    else {
        panic!("expected a check job");
    };
    assert_eq!(job.attempt(), 0);
    assert_eq!(job.candidate(), candidate);
    assert_eq!(job.candidate_sha256(), sha256_hex(candidate));
    assert_eq!(
        job.cost(),
        Cost {
            nodes: 7,
            bytes: 763
        }
    );
    assert_eq!(Some(job.work()), session.request().check_work(job.cost()));
    let job: CheckJob = job;
    assert!(matches!(job.run(), CheckReport::Completed(_)));
}
