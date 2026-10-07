//! Search contracts for complete canonical assignments, certificates and failures.

use zeno_fcis_codec::{CanonicalEncode, Domain, EncodeError, Hash32, commitment};
use zeno_fcis_crypto::RustCryptoSha256;
use zeno_fcis_synthesis::{
    Assignment, CandidateChecker, CheckResult, Hole, HoleId, SearchBudget, SearchResult,
    SynthesisBindings, SynthesisError, SynthesisProblem, search,
};
use zeno_fcis_value::{Field, MapEntry, Value};

fn checked<T, E: core::fmt::Debug>(result: Result<T, E>) -> T {
    result.unwrap_or_else(|error| panic!("{error:?}"))
}

fn hash(byte: u8) -> Hash32 {
    Hash32::new([byte; 32])
}

fn hash_bytes(domain: Domain<'_>, bytes: &[u8]) -> Hash32 {
    checked(commitment::<RustCryptoSha256>(domain, bytes))
}

fn hash_value(domain: Domain<'_>, value: &Value) -> Hash32 {
    hash_bytes(domain, &checked(value.canonical_bytes()))
}

fn bindings() -> SynthesisBindings {
    SynthesisBindings {
        schema_hash: hash(1),
        contract_hash: hash(2),
        grammar_hash: hash(3),
        algorithm_hash: hash(4),
    }
}

fn values() -> Vec<Value> {
    vec![
        Value::unit(),
        Value::boolean(false),
        Value::boolean(true),
        Value::unsigned(u128::MAX),
        Value::signed(-1),
        Value::signed(0),
        Value::bytes(vec![1; 1024]).unwrap_or_else(|error| panic!("value fixture: {error}")),
        Value::text_ascii(String::from("text"))
            .unwrap_or_else(|error| panic!("value fixture: {error}")),
        Value::enumeration(17, 3),
        Value::tuple(vec![Value::unit(), Value::unsigned(0)])
            .unwrap_or_else(|error| panic!("value fixture: {error}")),
        Value::record_canonical(vec![Field::new(2, Value::signed(i128::MIN))])
            .unwrap_or_else(|error| panic!("value fixture: {error}")),
        Value::sum(5, 1, None),
        Value::sum(5, 2, Some(Value::boolean(true))),
        Value::vector(vec![
            Value::text_ascii(String::from("v"))
                .unwrap_or_else(|error| panic!("value fixture: {error}")),
        ])
        .unwrap_or_else(|error| panic!("value fixture: {error}")),
        Value::map_canonical(vec![checked(MapEntry::try_new(
            Value::boolean(true),
            Value::bytes(vec![3; 9]).unwrap_or_else(|error| panic!("value fixture: {error}")),
        ))])
        .unwrap_or_else(|error| panic!("value fixture: {error}")),
    ]
}

fn problem() -> SynthesisProblem {
    checked(SynthesisProblem::try_new(
        bindings(),
        vec![
            checked(Hole::try_new(
                checked(HoleId::try_new(23)),
                vec![
                    Value::text_ascii(String::from("longer"))
                        .unwrap_or_else(|error| panic!("value fixture: {error}")),
                    Value::unit(),
                    Value::boolean(false),
                ],
            )),
            checked(Hole::try_new(checked(HoleId::try_new(2)), values())),
        ],
        SearchBudget {
            max_assignments: 100,
        },
    ))
}

struct RecordingChecker {
    identity: Hash32,
    stop: usize,
    result: CheckResult,
    seen: Vec<Assignment>,
}

impl CandidateChecker for RecordingChecker {
    fn checker_hash(&self) -> Hash32 {
        self.identity
    }

    fn check(&mut self, assignment: &Assignment) -> CheckResult {
        let ordinal = self.seen.len();
        self.seen.push(assignment.clone());
        if ordinal == self.stop {
            self.result.clone()
        } else {
            CheckResult::Rejected {
                counterexample: Value::unsigned(ordinal as u128),
            }
        }
    }
}

#[test]
fn every_selection_and_exhaustion_preserve_exact_assignment_and_certificate_bytes() {
    let problem = problem();
    let cardinality = usize::try_from(problem.cardinality()).unwrap_or_else(|_| unreachable!());
    for stop in 0..=cardinality {
        let compiled = Value::tuple(vec![
            Value::text_ascii(String::from("compiled"))
                .unwrap_or_else(|error| panic!("value fixture: {error}")),
            Value::signed(-3),
        ])
        .unwrap_or_else(|error| panic!("value fixture: {error}"));
        let compiled_hash = hash_value(zeno_fcis_codec::domains::SYNTHESIS_COMPILED, &compiled);
        let mut checker = RecordingChecker {
            identity: hash(10),
            stop,
            result: CheckResult::Accepted {
                compiled,
                reference_claim: hash(11),
                composition_claim: hash(12),
            },
            seen: Vec::new(),
        };
        let result = checked(search(&problem, &mut checker));
        assert_eq!(checker.seen.len(), (stop + 1).min(cardinality));
        let mut trace = problem.problem_hash();
        let mut rejected = Vec::new();
        for (ordinal, assignment) in checker.seen.iter().enumerate() {
            let mut bytes = 2_u32.to_be_bytes().to_vec();
            for (hole, index) in [
                (&problem.holes()[0], ordinal / 3),
                (&problem.holes()[1], ordinal % 3),
            ] {
                let value = hole.values().nth(index).unwrap_or_else(|| unreachable!());
                assert_eq!(assignment.get(hole.id()), Some(value));
                let encoded = checked(value.canonical_bytes());
                bytes.extend_from_slice(&hole.id().get().to_be_bytes());
                bytes.extend_from_slice(
                    &u32::try_from(encoded.len())
                        .unwrap_or_else(|_| unreachable!())
                        .to_be_bytes(),
                );
                bytes.extend_from_slice(&encoded);
            }
            assert_eq!(checked(assignment.canonical_bytes()), bytes);
            let mut appended = vec![0xaa, 0xbb];
            checked(assignment.encode_to(&mut appended));
            assert_eq!(&appended[..2], &[0xaa, 0xbb]);
            assert_eq!(&appended[2..], bytes);
            let assignment_hash =
                hash_bytes(zeno_fcis_codec::domains::SYNTHESIS_ASSIGNMENT, &bytes);
            assert_eq!(checked(assignment.commitment()), assignment_hash);
            let mut step = trace.as_bytes().to_vec();
            step.extend_from_slice(assignment_hash.as_bytes());
            if ordinal == stop {
                step.push(0);
                for binding in [compiled_hash, hash(11), hash(12)] {
                    step.extend_from_slice(binding.as_bytes());
                }
            } else {
                let counterexample = hash_value(
                    zeno_fcis_codec::domains::SYNTHESIS_COUNTEREXAMPLE,
                    &Value::unsigned(ordinal as u128),
                );
                rejected.push((assignment_hash, counterexample));
                step.push(1);
                step.extend_from_slice(counterexample.as_bytes());
            }
            trace = hash_bytes(zeno_fcis_codec::domains::SYNTHESIS_TRACE, &step);
        }
        let (certificate, selected) = match result {
            SearchResult::Selected {
                assignment,
                certificate,
                ..
            } => {
                assert_eq!(stop + 1, checker.seen.len());
                assert_eq!(checker.seen.last(), Some(&assignment));
                (certificate, Some(checked(assignment.commitment())))
            }
            SearchResult::NoSolution { certificate } => {
                assert_eq!(stop, cardinality);
                (certificate, None)
            }
            _ => panic!("unsupported synthesis result"),
        };
        assert_eq!(certificate.trace_hash(), trace);
        assert_eq!(certificate.selected_assignment(), selected);
        let mut expected = Vec::new();
        for binding in [
            problem.problem_hash(),
            hash(1),
            hash(2),
            hash(3),
            hash(4),
            hash(10),
        ] {
            expected.extend_from_slice(binding.as_bytes());
        }
        expected.extend_from_slice(&problem.cardinality().to_be_bytes());
        expected.extend_from_slice(&(checker.seen.len() as u64).to_be_bytes());
        expected.extend_from_slice(trace.as_bytes());
        for value in [
            selected,
            selected.map(|_| compiled_hash),
            selected.map(|_| hash(11)),
            selected.map(|_| hash(12)),
        ] {
            expected.push(u8::from(value.is_some()));
            if let Some(value) = value {
                expected.extend_from_slice(value.as_bytes());
            }
        }
        expected.extend_from_slice(&(rejected.len() as u32).to_be_bytes());
        for (assignment, counterexample) in rejected {
            expected.extend_from_slice(assignment.as_bytes());
            expected.extend_from_slice(counterexample.as_bytes());
        }
        assert_eq!(checked(certificate.canonical_bytes()), expected);
        assert_eq!(
            checked(certificate.commitment()),
            hash_bytes(zeno_fcis_codec::domains::SYNTHESIS_CERTIFICATE, &expected)
        );
    }
}

#[test]
fn checker_errors_stop_at_the_same_assignment_and_keep_first_error_order() {
    let problem = problem();
    assert!(Value::text_ascii("\u{e9}".into()).is_err());
    // Structurally legal values may exceed the default encoding policy.
    let mut invalid = Value::unit();
    for _ in 0..65 {
        invalid = checked(Value::tuple(vec![invalid]));
    }
    let cases = [
        (
            CheckResult::Indeterminate,
            SynthesisError::CheckerIndeterminate,
        ),
        (
            CheckResult::Rejected {
                counterexample: invalid.clone(),
            },
            SynthesisError::Encode(EncodeError::InvalidValue(
                zeno_fcis_value::ValueError::DepthLimit {
                    limit: 64,
                    attempted: 65,
                },
            )),
        ),
        (
            CheckResult::Accepted {
                compiled: invalid.clone(),
                reference_claim: Hash32::ZERO,
                composition_claim: hash(12),
            },
            SynthesisError::MissingAcceptanceEvidence,
        ),
        (
            CheckResult::Accepted {
                compiled: invalid.clone(),
                reference_claim: hash(11),
                composition_claim: Hash32::ZERO,
            },
            SynthesisError::MissingAcceptanceEvidence,
        ),
        (
            CheckResult::Accepted {
                compiled: invalid,
                reference_claim: hash(11),
                composition_claim: hash(12),
            },
            SynthesisError::Encode(EncodeError::InvalidValue(
                zeno_fcis_value::ValueError::DepthLimit {
                    limit: 64,
                    attempted: 65,
                },
            )),
        ),
    ];
    for stop in [0, 1, 17, 44] {
        for (result, error) in &cases {
            let mut checker = RecordingChecker {
                identity: hash(10),
                stop,
                result: result.clone(),
                seen: Vec::new(),
            };
            assert_eq!(search(&problem, &mut checker), Err(error.clone()));
            assert_eq!(checker.seen.len(), stop + 1);
        }
    }
    let mut checker = RecordingChecker {
        identity: Hash32::ZERO,
        stop: 0,
        result: CheckResult::Indeterminate,
        seen: Vec::new(),
    };
    assert_eq!(
        search(&problem, &mut checker),
        Err(SynthesisError::ZeroCheckerIdentity)
    );
    assert!(checker.seen.is_empty());
}

#[test]
fn invalid_values_are_rejected_before_a_problem_can_be_searched() {
    use zeno_fcis_value::{ValueError, ValueLimits};
    assert_eq!(
        Value::text_ascii_with_limits("\u{e9}".into(), ValueLimits::default()),
        Err(ValueError::NonAsciiText)
    );
    assert_eq!(
        Value::record_canonical(vec![
            Field::new(2, Value::unit()),
            Field::new(1, Value::unit())
        ]),
        Err(ValueError::RecordFieldOrder {
            previous: 2,
            current: 1
        })
    );
    assert_eq!(
        Value::map_canonical(vec![
            checked(MapEntry::try_new(Value::boolean(true), Value::unit())),
            checked(MapEntry::try_new(Value::boolean(false), Value::unit())),
        ]),
        Err(ValueError::MapKeyOrder)
    );
    let mut value = Value::unit();
    for _ in 0..65 {
        value = checked(Value::tuple(vec![value]));
    }
    assert_eq!(
        Hole::try_new(checked(HoleId::try_new(1)), vec![value]),
        Err(SynthesisError::Encode(EncodeError::InvalidValue(
            ValueError::DepthLimit {
                limit: 64,
                attempted: 65,
            }
        )))
    );
}
