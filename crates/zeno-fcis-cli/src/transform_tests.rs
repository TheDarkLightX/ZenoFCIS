//! Checker tests: enumeration, sizing, limits, usage, receipts and replay, the
//! benchmark fixtures, the withdrawal-queue artifacts and planted defects.

use super::{
    Counterexample, DEFAULT_MAX_INPUT_TUPLES, DEFAULT_STEP_LIMIT, ENUMERATION, Equivalence,
    FULL_BUDGET, Inconclusive, Limits, Observation, Refusal, Rejection, Replay, Side, Usage,
    advance, check, domain_size, failure_tag, first_tuple, refreshed, replay,
};
use serde_json::{Value, json};
use zeno_fcis_codec::{CanonicalEncode, CommitmentHasher};
use zeno_fcis_crypto::RustCryptoSha256;
use zeno_fcis_synthesis::finite::{
    Domain, MAX_NODES, Op, Program, V2ExecutionFailure, V2Resource, v2_authority::EVALUATOR,
    v2_zero_limits,
};
use zeno_fcis_synthesis::finite_runtime::import_program;

#[allow(dead_code, unreachable_pub, missing_docs, clippy::all)]
#[path = "../templates/withdrawal-queue/src/v2_contract.rs"]
mod withdrawal_contract;

/// The actual shell library's program-successor comparison API.
pub(crate) use zeno_fcis_shell_sqlite::v2::equivalence as shell_equivalence;

const GENEROUS: Limits = Limits {
    steps: DEFAULT_STEP_LIMIT,
    input_tuples: DEFAULT_MAX_INPUT_TUPLES,
};
const FULL: Domain = Domain::Int {
    min: i64::MIN,
    max: i64::MAX,
};

type Checked = Result<Equivalence, Rejection>;

fn limits(steps: u64) -> Limits {
    Limits {
        steps,
        input_tuples: DEFAULT_MAX_INPUT_TUPLES,
    }
}

fn artifact(name: &str) -> &'static [u8] {
    macro_rules! artifacts {
        ($($name:literal),+) => {
            match name {
                $($name => include_bytes!(concat!(
                    "../../../docs/benchmarks/withdrawal-queue/artifacts/", $name, ".zcve"
                )),)+
                _ => panic!("unknown artifact {name}"),
            }
        };
    }
    artifacts!(
        "boolean-kernel-original",
        "boolean-kernel-candidate",
        "boolean-kernel-padded",
        "retained-controller-original",
        "retained-controller-candidate",
        "retained-controller-padded",
        "current-decision-scalars-original",
        "current-decision-scalars-candidate",
        "current-decision-scalars-padded"
    )
}

fn program(inputs: Vec<Domain>, outputs: Vec<Domain>, nodes: Vec<Op>, roots: Vec<u16>) -> Program {
    Program::try_new(inputs, outputs, nodes, roots)
        .unwrap_or_else(|error| panic!("test program must be admitted: {error}"))
}

fn encode(program: &Program) -> Vec<u8> {
    program
        .value()
        .and_then(|value| value.canonical_bytes())
        .unwrap_or_else(|error| panic!("encode test program: {error}"))
}

fn import(bytes: &[u8]) -> Program {
    import_program(bytes).unwrap_or_else(|error| panic!("import artifact: {error}"))
}

/// Same ABI as `base`, with new nodes and roots.
fn rebuild(base: &Program, nodes: Vec<Op>, roots: Vec<u16>) -> Vec<u8> {
    encode(&program(
        base.inputs().to_vec(),
        base.outputs().to_vec(),
        nodes,
        roots,
    ))
}

fn kernel() -> (Program, Program) {
    (
        import(artifact("boolean-kernel-original")),
        import(artifact("boolean-kernel-candidate")),
    )
}

/// The kernel candidate with its first two outputs swapped.
fn swapped_kernel() -> Vec<u8> {
    let (_, candidate) = kernel();
    let mut roots = candidate.roots().to_vec();
    roots.swap(0, 1);
    rebuild(&candidate, candidate.nodes().to_vec(), roots)
}

fn equivalent(checked: Checked) -> Equivalence {
    match checked {
        Ok(equivalence) => equivalence,
        Err(rejection) => panic!("expected an equivalence, found {rejection:?}"),
    }
}

fn counterexample(checked: Checked) -> Counterexample {
    match checked {
        Err(Rejection::Counterexample(witness)) => witness,
        other => panic!("expected a counterexample, found {other:?}"),
    }
}

fn boundary(over_limit: [u64; 2], minimum_limit: u64, usage: Usage) -> Checked {
    Err(Rejection::Inconclusive(Inconclusive::BudgetBoundary {
        over_limit,
        minimum_limit,
        usage,
    }))
}

fn usage(max_steps: [u64; 2], candidate_uses_more: u64, usage_preserved: bool) -> Usage {
    Usage {
        max_steps,
        candidate_uses_more,
        usage_preserved,
    }
}

fn ok(values: &[i64], steps: u64) -> Observation {
    Observation {
        result: Ok(values.to_vec()),
        steps,
    }
}

fn failed(failure: V2ExecutionFailure, steps: u64) -> Observation {
    Observation {
        result: Err(failure),
        steps,
    }
}

/// Independent mixed-radix decoding of an ordinal, last input fastest.
fn decode(domains: &[Domain], mut ordinal: u128) -> Vec<i64> {
    let mut tuple = vec![0; domains.len()];
    for (slot, domain) in tuple.iter_mut().zip(domains).rev() {
        let (min, max) = domain.bounds();
        let width = u128::from(max.abs_diff(min)) + 1;
        let offset = i128::try_from(ordinal % width).unwrap_or_else(|_| panic!("offset"));
        ordinal /= width;
        *slot = i64::try_from(i128::from(min) + offset).unwrap_or_else(|_| panic!("value"));
    }
    assert_eq!(ordinal, 0, "ordinal beyond the domain");
    tuple
}

fn reencode(value: &Value) -> Vec<u8> {
    let mut bytes =
        serde_json::to_vec(value).unwrap_or_else(|error| panic!("encode receipt: {error}"));
    bytes.push(b'\n');
    bytes
}

fn parse(bytes: &[u8]) -> Value {
    serde_json::from_slice(bytes).unwrap_or_else(|error| panic!("parse receipt: {error}"))
}

fn sha256(bytes: &[u8]) -> String {
    RustCryptoSha256::hash(bytes)
        .as_bytes()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

#[test]
fn enumeration_visits_each_tuple_once_in_declared_order() {
    let shapes = [
        vec![],
        vec![Domain::Bool],
        vec![Domain::Bool; 6],
        vec![
            Domain::Int { min: -2, max: 2 },
            Domain::Bool,
            Domain::Int {
                min: i64::MAX - 1,
                max: i64::MAX,
            },
        ],
        vec![
            Domain::Int {
                min: i64::MIN,
                max: i64::MIN + 2,
            },
            Domain::Int { min: 7, max: 7 },
            Domain::Bool,
        ],
        vec![
            Domain::Int { min: 170, max: 171 },
            Domain::Int { min: 160, max: 162 },
            Domain::Bool,
            Domain::Int { min: 190, max: 193 },
        ],
    ];
    for domains in shapes {
        let size = domain_size(&domains)
            .ok()
            .flatten()
            .unwrap_or_else(|| panic!("small domain"));
        let product: u128 = domains
            .iter()
            .map(|domain| u128::from(domain.bounds().1.abs_diff(domain.bounds().0)) + 1)
            .product();
        assert_eq!(size, product);
        let mut tuple = first_tuple(&domains);
        let first = tuple.clone();
        let mut seen: Vec<Vec<i64>> = Vec::new();
        loop {
            assert!(
                tuple
                    .iter()
                    .zip(&domains)
                    .all(|(value, domain)| domain.contains(*value))
            );
            assert_eq!(tuple, decode(&domains, seen.len() as u128));
            // Strictly increasing order rules out a repeated tuple.
            if let Some(previous) = seen.last() {
                assert!(*previous < tuple, "{previous:?} then {tuple:?}");
            }
            seen.push(tuple.clone());
            if !advance(&domains, &mut tuple) {
                break;
            }
        }
        // Count equals the product: with distinct in-domain tuples, every
        // tuple was visited exactly once.
        assert_eq!(seen.len() as u128, size);
        let last: Vec<i64> = domains.iter().map(|domain| domain.bounds().1).collect();
        assert_eq!(seen.last(), Some(&last));
        assert_eq!(tuple, first, "the odometer wraps to its first tuple");
    }
}

#[test]
fn zero_inputs_have_exactly_one_empty_tuple() {
    assert_eq!(domain_size(&[]), Ok(Some(1)));
    let mut tuple = first_tuple(&[]);
    assert!(tuple.is_empty());
    assert!(!advance(&[], &mut tuple));
    let constant = encode(&program(
        vec![],
        vec![Domain::Bool],
        vec![Op::Bool(true)],
        vec![0],
    ));
    let receipt = equivalent(check(&constant, &constant, GENEROUS)).receipt_value();
    assert_eq!(receipt["inputs_checked"], 1);
    assert_eq!(receipt["domain"]["size"], 1);
}

#[test]
fn domain_size_is_exact_and_overflow_safe() {
    let two_to_64 = 1_u128 << 64;
    assert_eq!(domain_size(&[FULL]), Ok(Some(two_to_64)));
    assert_eq!(domain_size(&[FULL, Domain::Bool]), Ok(Some(two_to_64 * 2)));
    // 2^128 does not fit in u128.
    assert_eq!(domain_size(&[FULL, FULL]), Ok(None));
    assert_eq!(domain_size(&[FULL; 32]), Ok(None));
    assert_eq!(
        domain_size(&[Domain::Int {
            min: i64::MIN,
            max: i64::MIN
        }]),
        Ok(Some(1))
    );
    // An empty interval is named even after the product has overflowed.
    let empty = Domain::Int { min: 1, max: 0 };
    assert_eq!(domain_size(&[FULL, FULL, empty]), Err(2));
    assert_eq!(domain_size(&[Domain::Bool, empty, FULL]), Err(1));
    let current = import(artifact("current-decision-scalars-original"));
    assert_eq!(domain_size(current.inputs()), Ok(Some(1_296_000)));
}

#[test]
fn domain_cap_applies_before_any_evaluation() {
    assert_eq!(DEFAULT_MAX_INPUT_TUPLES, 100_000_000);
    assert_eq!(FULL_BUDGET, MAX_NODES as u64);
    assert_eq!(DEFAULT_STEP_LIMIT, FULL_BUDGET);
    let (original, candidate) = (
        artifact("boolean-kernel-original"),
        artifact("boolean-kernel-candidate"),
    );
    let capped = |input_tuples| Limits {
        steps: DEFAULT_STEP_LIMIT,
        input_tuples,
    };
    assert_eq!(
        check(original, candidate, capped(15)),
        Err(Rejection::Inconclusive(Inconclusive::DomainTooLarge {
            size: Some(16),
            limit: 15
        }))
    );
    let receipt = equivalent(check(original, candidate, capped(16))).receipt_value();
    assert_eq!(receipt["inputs_checked"], 16);
    assert_eq!(receipt["limits"]["input_tuples"], 16);
    // A pair that differs on its second tuple is not evaluated above the cap.
    assert!(matches!(
        check(original, &swapped_kernel(), capped(15)),
        Err(Rejection::Inconclusive(Inconclusive::DomainTooLarge { .. }))
    ));
    // Full-width inputs exceed every u64 cap; three of them exceed u128.
    let one = encode(&program(
        vec![FULL],
        vec![FULL],
        vec![Op::Input(0)],
        vec![0],
    ));
    assert_eq!(
        check(&one, &one, capped(u64::MAX)),
        Err(Rejection::Inconclusive(Inconclusive::DomainTooLarge {
            size: Some(1 << 64),
            limit: u64::MAX
        }))
    );
    let three = encode(&program(
        vec![FULL; 3],
        vec![FULL],
        vec![Op::Input(2)],
        vec![0],
    ));
    assert_eq!(
        check(&three, &three, GENEROUS),
        Err(Rejection::Inconclusive(Inconclusive::DomainTooLarge {
            size: None,
            limit: DEFAULT_MAX_INPUT_TUPLES
        }))
    );
}

#[test]
fn receipt_is_canonical_and_deterministic() {
    let domain = Domain::Int { min: -2, max: 2 };
    // x - 0 versus x: equal results, and the candidate uses fewer Steps.
    let original = encode(&program(
        vec![domain],
        vec![FULL],
        vec![Op::Input(0), Op::Int(0), Op::Sub(0, 1)],
        vec![2],
    ));
    let candidate = encode(&program(
        vec![domain],
        vec![FULL],
        vec![Op::Input(0)],
        vec![0],
    ));
    let first = equivalent(check(&original, &candidate, GENEROUS)).receipt();
    let second = equivalent(check(&original, &candidate, GENEROUS)).receipt();
    assert_eq!(first, second);
    let expected = format!(
        concat!(
            "{{\"authority\":\"none\",",
            "\"candidate\":{{\"bytes\":{},\"nodes\":1,\"sha256\":\"{}\"}},",
            "\"checker\":{{\"evaluator_identity\":\"{}\",",
            "\"semantics\":\"zeno-fcis/transform-check/1\"}},",
            "\"claim\":\"Functionally equal on the full declared input domain: every tuple ",
            "gives identical outputs or identical failures. The Step limit never binds: no ",
            "tuple needs more Steps than the limit in either program.\",",
            "\"domain\":{{\"enumeration\":\"ordered-product-last-input-fastest-v1\",",
            "\"inputs\":[{{\"kind\":\"int\",\"max\":\"2\",\"min\":\"-2\"}}],\"size\":5}},",
            "\"inputs_checked\":5,",
            "\"limits\":{{\"input_tuples\":100000000,\"steps\":256}},",
            "\"original\":{{\"bytes\":{},\"nodes\":3,\"sha256\":\"{}\"}},",
            "\"outputs\":[{{\"kind\":\"int\",\"max\":\"9223372036854775807\",",
            "\"min\":\"-9223372036854775808\"}}],",
            "\"profile\":{{\"execution\":\"zeno-fcis/finite-instruction-meter/2\",",
            "\"program\":\"zeno-fcis/finite-i64/1\"}},",
            "\"schema\":\"zeno-fcis/transform-receipt/1\",",
            "\"usage\":{{\"candidate_uses_more\":0,\"max_steps\":{{\"candidate\":1,",
            "\"original\":3}},\"usage_preserved\":false}},",
            "\"verdict\":\"equivalent\"}}\n"
        ),
        candidate.len(),
        sha256(&candidate),
        EVALUATOR
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>(),
        original.len(),
        sha256(&original),
    );
    assert_eq!(String::from_utf8(first.clone()).ok(), Some(expected));
    // Parsing and re-serializing with sorted keys reproduces the bytes.
    assert_eq!(reencode(&parse(&first)), first);
    // A different candidate gives a different receipt.
    let other = encode(&program(
        vec![domain],
        vec![FULL],
        vec![Op::Input(0), Op::Int(0), Op::Add(0, 1)],
        vec![2],
    ));
    assert_ne!(
        equivalent(check(&original, &other, GENEROUS)).receipt(),
        first
    );
}

#[test]
fn replay_matches_only_the_exact_recomputed_receipt() {
    let (original, candidate) = (
        artifact("boolean-kernel-original"),
        artifact("boolean-kernel-candidate"),
    );
    let receipt = equivalent(check(original, candidate, GENEROUS)).receipt();
    let cap = DEFAULT_MAX_INPUT_TUPLES;
    assert_eq!(replay(&receipt, original, candidate, cap), Replay::Matched);
    let value = parse(&receipt);
    assert_eq!(reencode(&value), receipt);
    let fields = value
        .as_object()
        .unwrap_or_else(|| panic!("receipt object"))
        .keys()
        .cloned()
        .collect::<Vec<_>>();
    // Replacing any field other than the ones replay reads first is
    // detected and named.
    for field in fields.iter().filter(|field| {
        !["schema", "limits", "original", "candidate", "domain"].contains(&field.as_str())
    }) {
        let mut tampered = value.clone();
        tampered[field.as_str()] = json!("tampered");
        assert_eq!(
            replay(&reencode(&tampered), original, candidate, cap),
            Replay::Differs(vec![field.clone()]),
            "{field}"
        );
    }
    for (pointer, replacement) in [
        ("/usage/max_steps/candidate", json!(6)),
        ("/usage/usage_preserved", json!(true)),
        ("/usage/candidate_uses_more", json!(1)),
        ("/inputs_checked", json!(15)),
        ("/domain/inputs/3/kind", json!("int")),
        ("/candidate/bytes", json!(762)),
        ("/original/nodes", json!(15)),
        ("/checker/semantics", json!("zeno-fcis/transform-check/0")),
        (
            "/checker/evaluator_identity",
            json!(sha256(b"another evaluator")),
        ),
    ] {
        let mut tampered = value.clone();
        *tampered
            .pointer_mut(pointer)
            .unwrap_or_else(|| panic!("{pointer}")) = replacement;
        let field = pointer.split('/').nth(1).unwrap_or_default().to_owned();
        assert_eq!(
            replay(&reencode(&tampered), original, candidate, cap),
            Replay::Differs(vec![field]),
            "{pointer}"
        );
    }
    // Program digests and the domain size are bound before any evaluation.
    for (pointer, field) in [
        ("/original/sha256", "original"),
        ("/candidate/sha256", "candidate"),
        ("/domain/size", "domain"),
    ] {
        let mut tampered = value.clone();
        *tampered
            .pointer_mut(pointer)
            .unwrap_or_else(|| panic!("{pointer}")) = if field == "domain" {
            json!(17)
        } else {
            json!(sha256(b"another program"))
        };
        assert_eq!(
            replay(&reencode(&tampered), original, candidate, cap),
            Replay::Differs(vec![field.to_owned()]),
            "{pointer}"
        );
    }
    // The recorded limits are rerun. Another limit that never binds gives a
    // different receipt that is equally valid; a binding Step limit or a
    // smaller tuple cap is not an equivalence.
    let mut other_limit = value.clone();
    other_limit["limits"]["steps"] = json!(255);
    assert_eq!(
        replay(&reencode(&other_limit), original, candidate, cap),
        Replay::Matched
    );
    let mut tampered = value.clone();
    tampered["limits"]["steps"] = json!(0);
    assert_eq!(
        replay(&reencode(&tampered), original, candidate, cap),
        Replay::NotEquivalent(Rejection::Inconclusive(Inconclusive::BudgetBoundary {
            over_limit: [16, 16],
            minimum_limit: 16,
            usage: usage([16, 7], 0, false),
        }))
    );
    let mut tampered = value.clone();
    tampered["limits"]["input_tuples"] = json!(15);
    assert!(matches!(
        replay(&reencode(&tampered), original, candidate, cap),
        Replay::NotEquivalent(Rejection::Inconclusive(Inconclusive::DomainTooLarge {
            limit: 15,
            ..
        }))
    ));
    // The verifier's own cap bounds replay, whatever the receipt records.
    assert_eq!(
        replay(&receipt, original, candidate, 15),
        Replay::OverCap {
            size: Some(16),
            cap: 15
        }
    );
    // Same content in another encoding is still a different receipt.
    let pretty = serde_json::to_vec_pretty(&value).unwrap_or_default();
    assert_eq!(
        replay(&pretty, original, candidate, cap),
        Replay::Differs(vec![])
    );
    assert_eq!(
        replay(&receipt[..receipt.len() - 1], original, candidate, cap),
        Replay::Differs(vec![])
    );
    // Unreadable receipts.
    let mut missing_digest = value.clone();
    missing_digest["original"] = json!({"bytes": 1063});
    for bytes in [
        b"not json".to_vec(),
        reencode(&json!({"schema": "zeno-fcis/transform-receipt/2", "limits": value["limits"]})),
        reencode(&json!({"schema": "zeno-fcis/transform-receipt/1"})),
        reencode(&missing_digest),
    ] {
        assert_eq!(replay(&bytes, original, candidate, cap), Replay::Unreadable);
    }
    // Other programs than the receipt names.
    assert_eq!(
        replay(&receipt, candidate, original, cap),
        Replay::Differs(vec!["original".to_owned(), "candidate".to_owned()])
    );
    assert_eq!(
        replay(&receipt, original, &swapped_kernel(), cap),
        Replay::Differs(vec!["candidate".to_owned()])
    );
}

#[test]
fn a_refresh_rebinds_only_the_checker_identity() {
    let (original, candidate) = (
        artifact("boolean-kernel-original"),
        artifact("boolean-kernel-candidate"),
    );
    let receipt = equivalent(check(original, candidate, GENEROUS)).receipt();
    let cap = DEFAULT_MAX_INPUT_TUPLES;
    // This checker's own receipt refreshes to itself.
    let same = refreshed(&receipt, original, candidate, cap)
        .unwrap_or_else(|refused| panic!("{refused:?}"));
    assert_eq!(same.receipt(), receipt);
    // A receipt from an older checker identity, as the crate version and
    // source digest once bound, is rebound to this checker.
    let value = parse(&receipt);
    let mut older = value.clone();
    older["checker"] = json!({
        "crate": "zeno-fcis-cli", "version": "1.0.0",
        "source": "crates/zeno-fcis-cli/src/transform.rs",
        "source_sha256": sha256(b"older checker"),
        "evaluator_identity": value["checker"]["evaluator_identity"]
    });
    let older = reencode(&older);
    assert_eq!(
        replay(&older, original, candidate, cap),
        Replay::Differs(vec!["checker".to_owned()])
    );
    let rebound =
        refreshed(&older, original, candidate, cap).unwrap_or_else(|refused| panic!("{refused:?}"));
    assert_eq!(rebound.receipt(), receipt);
    assert!(rebound.is_of(&receipt, original, candidate, cap));
    // Anything else that would change is refused, never trusted.
    for (pointer, replacement, field) in [
        ("/usage/usage_preserved", json!(true), "usage"),
        ("/verdict", json!("faster"), "verdict"),
        ("/inputs_checked", json!(15), "inputs_checked"),
    ] {
        let mut tampered = parse(&older);
        *tampered
            .pointer_mut(pointer)
            .unwrap_or_else(|| panic!("{pointer}")) = replacement;
        assert_eq!(
            refreshed(&reencode(&tampered), original, candidate, cap),
            Err(Replay::Differs(vec![field.to_owned()])),
            "{pointer}"
        );
    }
    // The recorded limits are rerun, so they are kept as recorded; a pair
    // that is no longer equivalent under them is refused.
    let mut binding = parse(&older);
    binding["limits"]["steps"] = json!(0);
    assert!(matches!(
        refreshed(&reencode(&binding), original, candidate, cap),
        Err(Replay::NotEquivalent(_))
    ));
    // Other programs than the receipt names.
    assert_eq!(
        refreshed(&older, candidate, original, cap),
        Err(Replay::Differs(vec![
            "original".to_owned(),
            "candidate".to_owned()
        ]))
    );
}

/// Reads a `cases.json` decimal string.
fn decimal(value: &Value) -> i64 {
    value
        .as_str()
        .and_then(|text| text.parse().ok())
        .unwrap_or_else(|| panic!("decimal string expected: {value}"))
}

fn fixture_domains(value: &Value) -> Vec<Domain> {
    value
        .as_array()
        .unwrap_or_else(|| panic!("domain list"))
        .iter()
        .map(|domain| {
            let fields = domain.as_object().map(serde_json::Map::len);
            match domain["kind"].as_str() {
                Some("Bool") if fields == Some(1) => Domain::Bool,
                Some("Int") if fields == Some(3) => Domain::Int {
                    min: decimal(&domain["min"]),
                    max: decimal(&domain["max"]),
                },
                _ => panic!("unknown fixture domain {domain}"),
            }
        })
        .collect()
}

fn fixture_op(node: &Value) -> Op {
    let items = node
        .as_array()
        .unwrap_or_else(|| panic!("fixture node {node}"));
    let id = |index: usize| {
        items[index]
            .as_u64()
            .and_then(|id| u16::try_from(id).ok())
            .unwrap_or_else(|| panic!("fixture reference {node}"))
    };
    match (items[0].as_str(), items.len()) {
        (Some("Input"), 2) => Op::Input(id(1)),
        (Some("Int"), 2) => Op::Int(decimal(&items[1])),
        (Some("Bool"), 2) => Op::Bool(
            items[1]
                .as_bool()
                .unwrap_or_else(|| panic!("fixture literal {node}")),
        ),
        (Some("Add"), 3) => Op::Add(id(1), id(2)),
        (Some("Sub"), 3) => Op::Sub(id(1), id(2)),
        (Some("Eq"), 3) => Op::Eq(id(1), id(2)),
        (Some("Lt"), 3) => Op::Lt(id(1), id(2)),
        (Some("And"), 3) => Op::And(id(1), id(2)),
        (Some("Not"), 2) => Op::Not(id(1)),
        (Some("Select"), 4) => Op::Select(id(1), id(2), id(3)),
        _ => panic!("unknown fixture node {node}"),
    }
}

/// Builds a fixture program through the library's own admission and codec.
fn fixture_program(inputs: &[Domain], outputs: &[Domain], graph: &Value) -> Vec<u8> {
    assert_eq!(
        graph.as_object().map(serde_json::Map::len),
        Some(2),
        "{graph}"
    );
    let nodes = graph["nodes"]
        .as_array()
        .unwrap_or_else(|| panic!("fixture nodes"))
        .iter()
        .map(fixture_op)
        .collect();
    let roots = graph["roots"]
        .as_array()
        .unwrap_or_else(|| panic!("fixture roots"))
        .iter()
        .map(|root| {
            root.as_u64()
                .and_then(|root| u16::try_from(root).ok())
                .unwrap_or_else(|| panic!("fixture root {root}"))
        })
        .collect();
    encode(&program(inputs.to_vec(), outputs.to_vec(), nodes, roots))
}

/// The fixtures' typed values: Bool as JSON booleans, integers as decimal strings.
fn typed(domains: &[Domain], values: &[i64]) -> Value {
    assert_eq!(domains.len(), values.len());
    Value::Array(
        domains
            .iter()
            .zip(values)
            .map(|(domain, value)| match domain {
                Domain::Bool => Value::Bool(*value == 1),
                _ => Value::String(value.to_string()),
            })
            .collect(),
    )
}

fn typed_observation(outputs: &[Domain], observation: &Observation) -> Value {
    match &observation.result {
        Ok(values) => json!({"ok": typed(outputs, values)}),
        Err(failure) => json!({"error": failure_tag(*failure)}),
    }
}

#[test]
fn benchmark_fixtures_reproduce_recorded_relations_and_witnesses() {
    let fixtures: Value = serde_json::from_str(include_str!("../../../docs/benchmarks/cases.json"))
        .unwrap_or_else(|error| panic!("cases.json: {error}"));
    assert_eq!(fixtures["schema_version"], "zenofcis-benchmark-seeds-v1");
    // The fixtures were enumerated in the order this checker uses.
    assert_eq!(fixtures["enumeration"], ENUMERATION);
    let cases = fixtures["cases"]
        .as_array()
        .unwrap_or_else(|| panic!("cases"));
    assert_eq!(cases.len(), 32);
    let mut equivalent_ids = Vec::new();
    let mut witness_ids = Vec::new();
    for case in cases {
        let id = case["id"].as_str().unwrap_or_else(|| panic!("case id"));
        let inputs = fixture_domains(&case["input_domains"]);
        let outputs = fixture_domains(&case["output_domains"]);
        let original = fixture_program(&inputs, &outputs, &case["original"]);
        let candidate = fixture_program(&inputs, &outputs, &case["candidate"]);
        let checked = check(&original, &candidate, GENEROUS);
        match case["expected_relation"].as_str() {
            Some("Equivalent") => {
                assert_eq!(case["witness"], Value::Null, "{id}");
                let Ok(equivalence) = checked else {
                    panic!("{id}: expected an equivalence, found {checked:?}");
                };
                let receipt = equivalence.receipt_value();
                let size = domain_size(&inputs)
                    .ok()
                    .flatten()
                    .and_then(|size| u64::try_from(size).ok());
                assert_eq!(receipt["inputs_checked"].as_u64(), size, "{id}");
                assert_eq!(
                    replay(
                        &equivalence.receipt(),
                        &original,
                        &candidate,
                        DEFAULT_MAX_INPUT_TUPLES
                    ),
                    Replay::Matched,
                    "{id}"
                );
                equivalent_ids.push(id);
            }
            Some("Different") => {
                let Err(Rejection::Counterexample(found)) = checked else {
                    panic!("{id}: expected a counterexample, found {checked:?}");
                };
                let witness = &case["witness"];
                assert_eq!(json!(found.ordinal), witness["ordinal"], "{id}");
                assert_eq!(typed(&inputs, &found.input), witness["tuple"], "{id}");
                assert_eq!(
                    typed_observation(&outputs, &found.original),
                    witness["original"],
                    "{id}"
                );
                assert_eq!(
                    typed_observation(&outputs, &found.candidate),
                    witness["candidate"],
                    "{id}"
                );
                witness_ids.push(id);
            }
            other => panic!("{id}: unknown relation {other:?}"),
        }
    }
    assert_eq!(equivalent_ids.len(), 21, "{equivalent_ids:?}");
    assert_eq!(
        witness_ids,
        [
            "B11", "B12", "B13", "B14", "I08", "I09", "I10", "I11", "I12", "I13", "I14"
        ]
    );
}

#[test]
fn withdrawal_artifacts_match_their_manifest() {
    let manifest: Value = serde_json::from_str(include_str!(
        "../../../docs/benchmarks/withdrawal-queue/artifacts/manifest.json"
    ))
    .unwrap_or_else(|error| panic!("manifest: {error}"));
    let entries = manifest["artifacts"]
        .as_object()
        .unwrap_or_else(|| panic!("artifact map"));
    assert_eq!(entries.len(), 9);
    for (file, entry) in entries {
        let bytes = artifact(file.trim_end_matches(".zcve"));
        assert_eq!(entry["sha256"], sha256(bytes), "{file}");
        assert_eq!(entry["bytes"], bytes.len(), "{file}");
        // Every artifact passes the library importer unchanged.
        assert_eq!(encode(&import(bytes)), bytes, "{file}");
    }
}

#[test]
fn withdrawal_originals_come_from_the_current_template() {
    // The retained controller is the template's synthesized program.
    assert_eq!(
        artifact("retained-controller-original"),
        include_bytes!("../templates/withdrawal-queue/synthesized/program.zcve")
    );
    // The current decision graph is the template descriptor's scalar program.
    let contract = withdrawal_contract::Contract::new();
    let descriptor = contract.descriptor();
    let current = program(
        descriptor.program.inputs.to_vec(),
        descriptor.program.outputs.to_vec(),
        descriptor.program.nodes.to_vec(),
        descriptor.program.roots.to_vec(),
    );
    assert_eq!(
        encode(&current),
        artifact("current-decision-scalars-original")
    );
    // The kernel is the controller's Boolean wires 0, 1, 5 and 6 with its
    // first three ORs (nodes 8..20), rooted at their final nodes 11, 15, 19.
    let controller = import(artifact("retained-controller-original"));
    let mut ids = vec![u16::MAX; controller.nodes().len()];
    let mut nodes = Vec::new();
    for (index, wire) in [0_u16, 1, 5, 6].into_iter().enumerate() {
        assert_eq!(controller.nodes()[usize::from(wire)], Op::Input(wire));
        ids[usize::from(wire)] = u16::try_from(index).unwrap_or_default();
        nodes.push(Op::Input(ids[usize::from(wire)]));
    }
    for old in 8..20 {
        let id = |reference: u16| ids[usize::from(reference)];
        let op = match controller.nodes()[old] {
            Op::Not(a) => Op::Not(id(a)),
            Op::And(a, b) => Op::And(id(a), id(b)),
            ref other => panic!("unexpected kernel node {other:?}"),
        };
        ids[old] = u16::try_from(nodes.len()).unwrap_or_default();
        nodes.push(op);
    }
    let roots = [11, 15, 19].map(|old: usize| ids[old]).to_vec();
    let extracted = program(vec![Domain::Bool; 4], vec![Domain::Bool; 3], nodes, roots);
    assert_eq!(encode(&extracted), artifact("boolean-kernel-original"));
}

#[test]
fn withdrawal_kernel_and_controller_pairs_are_equivalent_and_replay() {
    for (name, tuples, steps) in [
        ("boolean-kernel", 16, [16, 7]),
        ("retained-controller", 384, [69, 60]),
    ] {
        let original = artifact(&format!("{name}-original"));
        let candidate = artifact(&format!("{name}-candidate"));
        let equivalence = equivalent(check(original, candidate, GENEROUS));
        let receipt = equivalence.receipt_value();
        assert_eq!(receipt["inputs_checked"], tuples, "{name}");
        // Fewer instructions on every tuple: usage is reported, not preserved.
        assert_eq!(
            receipt["usage"],
            json!({
                "max_steps": {"original": steps[0], "candidate": steps[1]},
                "candidate_uses_more": 0, "usage_preserved": false
            }),
            "{name}"
        );
        assert_eq!(receipt["original"]["sha256"], sha256(original), "{name}");
        assert_eq!(receipt["candidate"]["sha256"], sha256(candidate), "{name}");
        assert_eq!(
            replay(
                &equivalence.receipt(),
                original,
                candidate,
                DEFAULT_MAX_INPUT_TUPLES
            ),
            Replay::Matched,
            "{name}"
        );
    }
}

#[test]
fn withdrawal_current_decision_scalars_are_equivalent_on_all_1296000_tuples() {
    let original = artifact("current-decision-scalars-original");
    let candidate = artifact("current-decision-scalars-candidate");
    let equivalence = equivalent(check(original, candidate, GENEROUS));
    let receipt = equivalence.receipt_value();
    assert_eq!(receipt["inputs_checked"], 1_296_000);
    assert_eq!(receipt["domain"]["size"], 1_296_000);
    assert_eq!(
        receipt["usage"],
        json!({
            "max_steps": {"original": 106, "candidate": 100},
            "candidate_uses_more": 0, "usage_preserved": false
        })
    );
    assert_eq!(
        replay(
            &equivalence.receipt(),
            original,
            candidate,
            DEFAULT_MAX_INPUT_TUPLES
        ),
        Replay::Matched
    );
}

#[test]
fn step_limits_on_the_withdrawal_kernel() {
    let original = artifact("boolean-kernel-original");
    let candidate = artifact("boolean-kernel-candidate");
    let padded = artifact("boolean-kernel-padded");
    // Padding keeps every instruction position, so usage is preserved.
    let receipt = equivalent(check(original, padded, GENEROUS)).receipt_value();
    assert_eq!(
        receipt["usage"],
        json!({
            "max_steps": {"original": 16, "candidate": 16},
            "candidate_uses_more": 0, "usage_preserved": true
        })
    );
    // A limit that binds is never an equivalence, at any value below the
    // usage the programs need. Limit 6 binds both programs everywhere;
    // limit 7 binds only the original; limit 16 binds neither.
    let kernel_usage = usage([16, 7], 0, false);
    assert_eq!(
        check(original, candidate, limits(6)),
        boundary([16, 16], 16, kernel_usage)
    );
    assert_eq!(
        check(original, candidate, limits(7)),
        boundary([16, 0], 16, kernel_usage)
    );
    assert_eq!(
        check(original, candidate, limits(15)),
        boundary([16, 0], 16, kernel_usage)
    );
    let receipt = equivalent(check(original, candidate, limits(16))).receipt_value();
    assert_eq!(receipt["limits"]["steps"], 16);
    assert_eq!(
        check(original, padded, limits(10)),
        boundary([16, 16], 16, usage([16, 16], 0, true))
    );
}

#[test]
fn a_binding_limit_never_mints_an_equivalence() {
    // At Step limit 0 every program would be refused before its first
    // instruction. The results still come from the full-budget runs, so a
    // wrong candidate is still a counterexample and a right one is
    // inconclusive.
    let (original, candidate) = kernel();
    let original = encode(&original);
    let found = counterexample(check(&original, &swapped_kernel(), limits(0)));
    assert_eq!(found.ordinal, 1);
    assert_eq!(
        check(&original, &encode(&candidate), limits(0)),
        boundary([16, 16], 16, usage([16, 7], 0, false))
    );
}

#[test]
fn usage_is_reported_from_full_budget_runs() {
    let near_max = Domain::Int {
        min: i64::MAX - 1,
        max: i64::MAX,
    };
    // Both programs return x and ignore the Boolean input, so each value of x
    // is checked twice. Both trap at x = MAX; the candidate traps one
    // instruction later.
    let graph = |nodes| {
        encode(&program(
            vec![near_max, Domain::Bool],
            vec![FULL],
            nodes,
            vec![0],
        ))
    };
    let original = graph(vec![
        Op::Input(0),
        Op::Int(1),
        Op::Add(0, 1),
        Op::Int(0),
        Op::Int(0),
    ]);
    let candidate = graph(vec![
        Op::Input(0),
        Op::Int(1),
        Op::Int(0),
        Op::Add(0, 1),
        Op::Int(0),
    ]);
    let receipt = equivalent(check(&original, &candidate, GENEROUS)).receipt_value();
    assert_eq!(
        receipt["usage"],
        json!({
            "max_steps": {"original": 5, "candidate": 5},
            "candidate_uses_more": 2, "usage_preserved": false
        })
    );
    // Limit 4 binds both on x = MAX - 1. Limit 3 also binds the candidate on
    // x = MAX, where the original traps within it; the trap itself is
    // compared at full budget, so this is not a counterexample.
    let trap_usage = usage([5, 5], 2, false);
    assert_eq!(
        check(&original, &candidate, limits(4)),
        boundary([2, 2], 5, trap_usage)
    );
    assert_eq!(
        check(&original, &candidate, limits(3)),
        boundary([2, 4], 5, trap_usage)
    );
    equivalent(check(&original, &candidate, limits(5)));
}

#[test]
fn programs_must_pass_library_admission_and_share_an_abi() {
    let original = artifact("boolean-kernel-original");
    let candidate = artifact("boolean-kernel-candidate");
    let mut trailing = candidate.to_vec();
    trailing.push(0);
    assert_eq!(
        check(original, &trailing, GENEROUS),
        Err(Rejection::Refused(Refusal::NotAdmitted {
            side: Side::Candidate,
            code: "program-encoding"
        }))
    );
    // The original is admitted first.
    assert_eq!(
        check(&original[..10], &trailing, GENEROUS),
        Err(Rejection::Refused(Refusal::NotAdmitted {
            side: Side::Original,
            code: "program-encoding"
        }))
    );
    // Inputs are compared before outputs; the controller differs in both.
    let controller = artifact("retained-controller-original");
    assert!(matches!(
        check(original, controller, GENEROUS),
        Err(Rejection::Refused(Refusal::InputAbi { .. }))
    ));
}

#[test]
fn planted_wrong_output_on_one_tuple_is_found_there() {
    let (original, candidate) = kernel();
    let [first, second, third] =
        <[u16; 3]>::try_from(candidate.roots()).unwrap_or_else(|_| panic!("three kernel outputs"));
    let base = u16::try_from(candidate.nodes().len()).unwrap_or_default();
    // Flip the first output only on (1, 0, 1, 0), ordinal 10.
    let mut nodes = candidate.nodes().to_vec();
    nodes.extend([
        Op::Not(1),
        Op::Not(3),
        Op::And(0, base),
        Op::And(base + 2, 2),
        Op::And(base + 3, base + 1),
        Op::Not(first),
        Op::Select(base + 4, base + 5, first),
    ]);
    let wrong = rebuild(&candidate, nodes, vec![base + 6, second, third]);
    let planted = import(&wrong);
    let meter = v2_zero_limits().with_limit(V2Resource::Step, FULL_BUDGET);
    let differing: Vec<u128> = (0..16)
        .filter(|ordinal| {
            let input = decode(original.inputs(), *ordinal);
            original.execute_v2(&input, meter).into_parts().0
                != planted.execute_v2(&input, meter).into_parts().0
        })
        .collect();
    assert_eq!(differing, [10]);
    let found = counterexample(check(&encode(&original), &wrong, GENEROUS));
    assert_eq!(
        (found.ordinal, found.input.as_slice()),
        (10, &[1, 0, 1, 0][..])
    );
    assert_eq!(found.original, ok(&[1, 0, 1], 16));
    assert_eq!(found.candidate, ok(&[0, 0, 1], 14));
}

#[test]
fn planted_swapped_outputs_are_found_at_the_first_distinguishing_tuple() {
    let (original, _) = kernel();
    let found = counterexample(check(&encode(&original), &swapped_kernel(), GENEROUS));
    assert_eq!(
        (found.ordinal, found.input.as_slice()),
        (1, &[0, 0, 0, 1][..])
    );
    assert_eq!(found.original, ok(&[0, 1, 1], 16));
    assert_eq!(found.candidate, ok(&[1, 0, 1], 7));
}

#[test]
fn planted_removal_of_an_eager_overflow_trap_is_found() {
    let near_max = Domain::Int {
        min: i64::MAX - 2,
        max: i64::MAX,
    };
    let candidate = encode(&program(
        vec![near_max],
        vec![FULL],
        vec![Op::Input(0)],
        vec![0],
    ));
    // The originals return x, but still evaluate x + 1: once as the
    // unselected arm of a Select, once as an unused node.
    for (nodes, root) in [
        (
            vec![
                Op::Input(0),
                Op::Int(1),
                Op::Add(0, 1),
                Op::Bool(false),
                Op::Select(3, 2, 0),
            ],
            4,
        ),
        (vec![Op::Input(0), Op::Int(1), Op::Add(0, 1)], 0),
    ] {
        let original = encode(&program(vec![near_max], vec![FULL], nodes, vec![root]));
        let found = counterexample(check(&original, &candidate, GENEROUS));
        assert_eq!(
            (found.ordinal, found.input.as_slice()),
            (2, &[i64::MAX][..])
        );
        assert_eq!(found.original, failed(V2ExecutionFailure::Arithmetic, 3));
        assert_eq!(found.candidate, ok(&[i64::MAX], 1));
    }
}

#[test]
fn planted_changed_input_domain_is_refused() {
    let original = artifact("retained-controller-original");
    let candidate = import(artifact("retained-controller-candidate"));
    let mut inputs = candidate.inputs().to_vec();
    let position = inputs
        .iter()
        .position(|domain| matches!(domain, Domain::Int { .. }))
        .unwrap_or_else(|| panic!("controller has an integer input"));
    let (min, max) = inputs[position].bounds();
    inputs[position] = Domain::Int { min, max: max + 1 };
    let widened = encode(&program(
        inputs.clone(),
        candidate.outputs().to_vec(),
        candidate.nodes().to_vec(),
        candidate.roots().to_vec(),
    ));
    assert_eq!(
        check(original, &widened, GENEROUS),
        Err(Rejection::Refused(Refusal::InputAbi {
            original: candidate.inputs().to_vec(),
            candidate: inputs,
        }))
    );
}

#[test]
fn planted_changed_output_domain_with_the_same_count_is_refused() {
    // Widening an integer output keeps every value in range, so only the ABI
    // comparison can refuse it.
    let original = artifact("retained-controller-original");
    let candidate = import(artifact("retained-controller-candidate"));
    let mut outputs = candidate.outputs().to_vec();
    let position = outputs
        .iter()
        .position(|domain| matches!(domain, Domain::Int { .. }))
        .unwrap_or_else(|| panic!("controller has an integer output"));
    let (min, max) = outputs[position].bounds();
    outputs[position] = Domain::Int { min, max: max + 1 };
    let widened = encode(&program(
        candidate.inputs().to_vec(),
        outputs.clone(),
        candidate.nodes().to_vec(),
        candidate.roots().to_vec(),
    ));
    assert_eq!(outputs.len(), candidate.outputs().len());
    assert_eq!(
        check(original, &widened, GENEROUS),
        Err(Rejection::Refused(Refusal::OutputAbi {
            original: candidate.outputs().to_vec(),
            candidate: outputs,
        }))
    );
}

#[test]
fn planted_extra_output_is_refused() {
    let (original, candidate) = kernel();
    let mut outputs = candidate.outputs().to_vec();
    outputs.push(Domain::Bool);
    let mut roots = candidate.roots().to_vec();
    roots.push(roots[0]);
    let extra = encode(&program(
        candidate.inputs().to_vec(),
        outputs.clone(),
        candidate.nodes().to_vec(),
        roots,
    ));
    assert_eq!(
        check(&encode(&original), &extra, GENEROUS),
        Err(Rejection::Refused(Refusal::OutputAbi {
            original: original.outputs().to_vec(),
            candidate: outputs,
        }))
    );
}

#[test]
fn planted_candidate_over_the_step_limit_is_inconclusive() {
    let (original, _) = kernel();
    let mut nodes = original.nodes().to_vec();
    nodes.push(Op::Bool(false));
    let longer = rebuild(&original, nodes, original.roots().to_vec());
    let original = encode(&original);
    // Under a generous limit the extra node only costs one more Step.
    let receipt = equivalent(check(&original, &longer, GENEROUS)).receipt_value();
    assert_eq!(
        receipt["usage"],
        json!({
            "max_steps": {"original": 16, "candidate": 17},
            "candidate_uses_more": 16, "usage_preserved": false
        })
    );
    // At the original's node count only the candidate exceeds the limit.
    assert_eq!(
        check(&original, &longer, limits(16)),
        boundary([0, 16], 17, usage([16, 17], 16, false))
    );
}

/// One comparison's outcome, as both this checker and the shell's
/// comparison report it, at the full Step budget so that no Step limit binds.
#[derive(Debug, Eq, PartialEq)]
enum Verdict {
    Equal { tuples: u64 },
    Counterexample { ordinal: u64 },
    DomainTooLarge { size: Option<u128> },
    InputAbi,
    OutputAbi,
    EmptyInputDomain { position: usize },
}

/// This checker's verdict; `None` when the library refuses a program, which
/// a bound catalog never holds.
fn checker_verdict(original: &[u8], candidate: &[u8], cap: u64) -> Option<Verdict> {
    let limits = Limits {
        steps: FULL_BUDGET,
        input_tuples: cap,
    };
    Some(match check(original, candidate, limits) {
        Ok(equivalence) => Verdict::Equal {
            tuples: equivalence.inputs_checked,
        },
        Err(Rejection::Counterexample(found)) => Verdict::Counterexample {
            ordinal: found.ordinal,
        },
        Err(Rejection::Inconclusive(Inconclusive::DomainTooLarge { size, .. })) => {
            Verdict::DomainTooLarge { size }
        }
        Err(Rejection::Refused(Refusal::InputAbi { .. })) => Verdict::InputAbi,
        Err(Rejection::Refused(Refusal::OutputAbi { .. })) => Verdict::OutputAbi,
        Err(Rejection::Refused(Refusal::EmptyInputDomain { position })) => {
            Verdict::EmptyInputDomain { position }
        }
        Err(Rejection::Refused(Refusal::NotAdmitted { .. })) => return None,
        Err(other) => panic!("no full-budget comparison ends so: {other:?}"),
    })
}

fn scalar(program: &Program) -> zeno_fcis_synthesis::finite::V2ScalarProgram<'_> {
    zeno_fcis_synthesis::finite::V2ScalarProgram {
        inputs: program.inputs(),
        outputs: program.outputs(),
        nodes: program.nodes(),
        roots: program.roots(),
    }
}

/// The shell's verdict over the same two admitted programs.
fn shell_verdict(original: &[u8], candidate: &[u8], cap: u64) -> Option<Verdict> {
    use shell_equivalence::Unestablished;
    let (original, candidate) = (
        import_program(original).ok()?,
        import_program(candidate).ok()?,
    );
    Some(
        match shell_equivalence::compare(&scalar(&original), &scalar(&candidate), cap) {
            Ok(equal) => Verdict::Equal {
                tuples: equal.tuples(),
            },
            Err(Unestablished::Counterexample { ordinal }) => Verdict::Counterexample { ordinal },
            Err(Unestablished::DomainTooLarge {
                size,
                cap: reported,
            }) => {
                assert_eq!(reported, cap);
                Verdict::DomainTooLarge { size }
            }
            Err(Unestablished::InputAbi) => Verdict::InputAbi,
            Err(Unestablished::OutputAbi) => Verdict::OutputAbi,
            Err(Unestablished::EmptyInputDomain { position }) => {
                Verdict::EmptyInputDomain { position }
            }
            Err(other) => panic!("the shell's comparison did not complete: {other:?}"),
        },
    )
}

/// Both verdicts, which must agree.
#[track_caller]
fn agreed(original: &[u8], candidate: &[u8], cap: u64) -> Option<Verdict> {
    let checker = checker_verdict(original, candidate, cap);
    assert_eq!(shell_verdict(original, candidate, cap), checker);
    checker
}

#[test]
fn the_shells_comparison_agrees_with_the_checker_on_the_known_answers_and_benchmarks() {
    let directory = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/transform-check/1"
    );
    let read = |name: &str| {
        std::fs::read(format!("{directory}/{name}"))
            .unwrap_or_else(|error| panic!("{name}: {error}"))
    };
    let vectors = parse(&read("vectors.json"));
    let vectors = vectors["vectors"]
        .as_array()
        .unwrap_or_else(|| panic!("vectors"));
    let mut seen = Vec::new();
    for vector in vectors {
        let name = |key: &str| vector[key].as_str().unwrap_or_else(|| panic!("{key}"));
        let cap = vector["max_input_tuples"]
            .as_u64()
            .unwrap_or_else(|| panic!("cap"));
        let verdict = agreed(&read(name("original")), &read(name("candidate")), cap);
        seen.push((name("name").to_owned(), verdict));
    }
    // Every kind of outcome the vectors pin, apart from a binding Step
    // limit, which a full-budget comparison never meets.
    let kinds: Vec<_> = seen
        .iter()
        .map(|(name, verdict)| (name.as_str(), verdict.as_ref().map(std::mem::discriminant)))
        .collect();
    for (name, verdict) in &seen {
        assert!(verdict.is_some(), "{name}: the library refused a program");
    }
    for expected in [
        Verdict::Equal { tuples: 0 },
        Verdict::Counterexample { ordinal: 0 },
        Verdict::DomainTooLarge { size: None },
        Verdict::OutputAbi,
    ] {
        let kind = std::mem::discriminant(&expected);
        assert!(
            kinds.iter().any(|(_, found)| *found == Some(kind)),
            "no vector gives {expected:?}: {seen:?}"
        );
    }

    let fixtures = parse(include_bytes!("../../../docs/benchmarks/cases.json"));
    let cases = fixtures["cases"]
        .as_array()
        .unwrap_or_else(|| panic!("cases"));
    let (mut equal, mut different) = (0, 0);
    for case in cases {
        let inputs = fixture_domains(&case["input_domains"]);
        let outputs = fixture_domains(&case["output_domains"]);
        let original = fixture_program(&inputs, &outputs, &case["original"]);
        let candidate = fixture_program(&inputs, &outputs, &case["candidate"]);
        match agreed(&original, &candidate, DEFAULT_MAX_INPUT_TUPLES) {
            Some(Verdict::Equal { .. }) => equal += 1,
            Some(Verdict::Counterexample { .. }) => different += 1,
            other => panic!("{}: {other:?}", case["id"]),
        }
    }
    assert_eq!((equal, different), (21, 11));
}

#[test]
fn the_shells_comparison_agrees_with_the_checker_on_planted_defects() {
    // Planted defects on the withdrawal kernel and controller artifacts.
    let (original, candidate) = kernel();
    let (original, candidate) = (encode(&original), encode(&candidate));
    assert!(matches!(
        agreed(&original, &candidate, DEFAULT_MAX_INPUT_TUPLES),
        Some(Verdict::Equal { .. })
    ));
    assert!(matches!(
        agreed(&original, &swapped_kernel(), DEFAULT_MAX_INPUT_TUPLES),
        Some(Verdict::Counterexample { .. })
    ));
    // One tuple above the cap, and exactly at it.
    let Some(Verdict::Equal { tuples }) = agreed(&original, &candidate, DEFAULT_MAX_INPUT_TUPLES)
    else {
        panic!("the kernel pair is equal");
    };
    assert_eq!(
        agreed(&original, &candidate, tuples - 1),
        Some(Verdict::DomainTooLarge {
            size: Some(u128::from(tuples))
        })
    );
    assert_eq!(
        agreed(&original, &candidate, tuples),
        Some(Verdict::Equal { tuples })
    );
    // Each node of the controller candidate in turn replaced by a constant
    // of its own kind, where the library still admits the program.
    let base = import(artifact("retained-controller-candidate"));
    let original = artifact("retained-controller-original");
    let (mut compared, mut differing) = (0, 0);
    for index in 0..base.nodes().len() {
        for constant in [Op::Bool(false), Op::Bool(true), Op::Int(0), Op::Int(1)] {
            let mut nodes = base.nodes().to_vec();
            nodes[index] = constant;
            let Ok(planted) = Program::try_new(
                base.inputs().to_vec(),
                base.outputs().to_vec(),
                nodes,
                base.roots().to_vec(),
            ) else {
                continue;
            };
            let verdict = agreed(original, &encode(&planted), DEFAULT_MAX_INPUT_TUPLES);
            compared += 1;
            differing += usize::from(matches!(verdict, Some(Verdict::Counterexample { .. })));
        }
    }
    assert!(compared > 0 && differing > 0, "{compared} {differing}");
    eprintln!("planted controller candidates: {compared} compared, {differing} differing");
}

/// The committed adoption: the withdrawal queue's version 1 program and the
/// adopted 100-node candidate, on all 1,296,000 tuples, and the candidate
/// with its decision output's selection arms swapped.
#[test]
fn the_shells_comparison_agrees_with_the_checker_on_the_adoption_fixture() {
    let original = artifact("current-decision-scalars-original");
    let candidate =
        include_bytes!("../tests/fixtures/withdrawal-queue-adopted/v2/adoptions/1/program.zcve");
    assert_eq!(
        agreed(original, candidate, DEFAULT_MAX_INPUT_TUPLES),
        Some(Verdict::Equal { tuples: 1_296_000 })
    );
    let adopted = import(candidate);
    let mut nodes = adopted.nodes().to_vec();
    let root = usize::from(adopted.roots()[0]);
    let Op::Select(condition, then, otherwise) = nodes[root] else {
        panic!("the decision output is a selection");
    };
    nodes[root] = Op::Select(condition, otherwise, then);
    let swapped = rebuild(&adopted, nodes, adopted.roots().to_vec());
    assert!(matches!(
        agreed(original, &swapped, DEFAULT_MAX_INPUT_TUPLES),
        Some(Verdict::Counterexample { .. })
    ));
}
