from pathlib import Path


def replace_exact(path: Path, old: str, new: str, label: str) -> None:
    text = path.read_text(encoding="utf-8")
    if old not in text:
        if new in text:
            return
        raise SystemExit(f"missing hardening site: {label}")
    path.write_text(text.replace(old, new, 1), encoding="utf-8")


backend = Path("crates/zeno-fcis-backend/src/lib.rs")
replace_exact(
    backend,
    '''            BackendOutcome::Rejected(rejected) => CheckResult::Rejected {
                counterexample: rejected.counterexample().clone(),
            },
''',
    '''            BackendOutcome::Rejected(rejected) => CheckResult::Rejected {
                counterexample: bind_verified_counterexample(
                    rejected.counterexample(),
                    certificate_hash,
                ),
            },
''',
    "rejected synthesis binds backend certificate",
)
replace_exact(
    backend,
    '''fn bind_verified_claim(
''',
    '''fn bind_verified_counterexample(
    counterexample: &Value,
    certificate_hash: Hash32,
) -> Value {
    Value::tuple(vec![
        counterexample.clone(),
        Value::bytes(certificate_hash.as_bytes().to_vec()),
    ])
}

fn bind_verified_claim(
''',
    "verified counterexample helper",
)
replace_exact(
    backend,
    '''    #[test]
    fn synthesis_result_binds_independent_verifier_attestation() {
''',
    '''    #[test]
    fn no_solution_binds_independent_verifier_attestation() {
        let make_checker = |claim_hash| {
            let engine = MockEngine {
                identity: identity(vec![BackendOperation::Synthesize]),
            };
            let template = BackendRequestTemplate::try_new(hash(60), hash(61), hash(62), limits())
                .unwrap_or_else(|error| panic!("template: {error}"));
            SynthesisBackendChecker::try_new(engine, MockVerifier { claim_hash }, template)
                .unwrap_or_else(|error| panic!("checker: {error}"))
        };
        let hole = Hole::try_new(
            HoleId::try_new(1).unwrap_or_else(|error| panic!("hole id: {error}")),
            vec![Value::U128(1), Value::U128(3)],
        )
        .unwrap_or_else(|error| panic!("hole: {error}"));
        let problem = SynthesisProblem::try_new(
            SynthesisBindings {
                schema_hash: hash(70),
                contract_hash: hash(71),
                grammar_hash: hash(72),
                algorithm_hash: hash(73),
            },
            vec![hole],
            SearchBudget { max_assignments: 2 },
        )
        .unwrap_or_else(|error| panic!("problem: {error}"));
        let mut first_checker = make_checker(hash(41));
        let mut second_checker = make_checker(hash(42));
        let first = search(&problem, &mut first_checker)
            .unwrap_or_else(|error| panic!("first search failed: {error}"));
        let second = search(&problem, &mut second_checker)
            .unwrap_or_else(|error| panic!("second search failed: {error}"));
        assert!(matches!(first, SearchResult::NoSolution { .. }));
        assert!(matches!(second, SearchResult::NoSolution { .. }));
        assert_ne!(first, second);
    }

    #[test]
    fn synthesis_result_binds_independent_verifier_attestation() {
''',
    "no-solution verifier binding test",
)

document = Path("docs/GENERIC_BACKEND_PROTOCOL.md")
replace_exact(
    document,
    '''`SynthesisBackendChecker` adapts a verified generic backend to the existing `CandidateChecker` interface. Canonical assignment ordering and search completeness remain owned by `zeno-fcis-synthesis`. The mounted backend checks one assignment at a time; it cannot reorder, omit, or terminate the outer complete-within-bounds search. Accepted synthesis claims are re-committed together with the exact independent `BackendCertificate`, so the outer synthesis certificate binds the request, response, backend identity, verifier identity, and verifier attestation rather than only backend-supplied claim hashes.
''',
    '''`SynthesisBackendChecker` adapts a verified generic backend to the existing `CandidateChecker` interface. Canonical assignment ordering and search completeness remain owned by `zeno-fcis-synthesis`. The mounted backend checks one assignment at a time; it cannot reorder, omit, or terminate the outer complete-within-bounds search. Accepted synthesis claims are re-committed together with the exact independent `BackendCertificate`, and rejected counterexamples retain that certificate commitment as part of the normalized witness. Both selected and no-solution synthesis certificates therefore bind the request, response, backend identity, verifier identity, and verifier attestation rather than only backend-supplied claims or counterexamples.
''',
    "document rejection certificate binding",
)
