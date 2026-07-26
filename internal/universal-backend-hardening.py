from pathlib import Path


def replace_exact(path: Path, old: str, new: str, label: str) -> None:
    text = path.read_text(encoding="utf-8")
    if old not in text:
        if new in text:
            return
        raise SystemExit(f"missing hardening site: {label}")
    path.write_text(text.replace(old, new, 1), encoding="utf-8")


synthesis = Path("crates/zeno-fcis-synthesis/src/lib.rs")
replace_exact(
    synthesis,
    '''#![forbid(unsafe_code)]

use core::fmt;
''',
    '''#![cfg_attr(not(feature = "std"), no_std)]
#![forbid(unsafe_code)]

extern crate alloc;

use alloc::boxed::Box;
use alloc::vec;
use alloc::vec::Vec;
use core::fmt;
''',
    "synthesis no_std prelude",
)
replace_exact(
    synthesis,
    '''impl std::error::Error for SynthesisError {}
''',
    '''#[cfg(feature = "std")]
impl std::error::Error for SynthesisError {}
''',
    "synthesis std error gate",
)

backend = Path("crates/zeno-fcis-backend/src/lib.rs")
replace_exact(
    backend,
    '''use alloc::boxed::Box;
use alloc::vec::Vec;
''',
    '''use alloc::boxed::Box;
use alloc::vec;
use alloc::vec::Vec;
''',
    "backend vec macro",
)
replace_exact(
    backend,
    '''impl BackendRequestTemplate {
    /// Creates a synthesis request template.
    pub const fn try_new(
''',
    '''impl BackendRequestTemplate {
    /// Creates a synthesis request template.
    pub fn try_new(
''',
    "backend template non-const constructor",
)
replace_exact(
    backend,
    '''        assert_eq!(
            BackendLimits::try_new(0, 1, 1, 1),
            Err(BackendError::ZeroLimit)
        );
''',
    '''        assert!(matches!(
            BackendLimits::try_new(0, 1, 1, 1),
            Err(BackendError::ZeroLimit)
        ));
''',
    "backend zero limit test",
)
replace_exact(
    backend,
    '''        assert_eq!(
            BackendUsage::try_new(limits, 10_001, 0, 0, 0),
            Err(BackendError::UsageExceedsLimit)
        );
''',
    '''        assert!(matches!(
            BackendUsage::try_new(limits, 10_001, 0, 0, 0),
            Err(BackendError::UsageExceedsLimit)
        ));
''',
    "backend usage test",
)
replace_exact(
    backend,
    '''        if !identity.capabilities.supports(request.operation) {
            return Err(BackendError::UnsupportedOperation(request.operation));
        }
        let output_length = outcome
            .canonical_bytes()
            .map_err(BackendError::Encode)?
            .len();
        let output_length =
            u64::try_from(output_length).map_err(|_| BackendError::OutputTooLarge)?;
        if output_length > request.limits.max_output_bytes || output_length > usage.output_bytes {
            return Err(BackendError::OutputTooLarge);
        }
        Ok(Self {
''',
    '''        if !identity.capabilities.supports(request.operation) {
            return Err(BackendError::UnsupportedOperation(request.operation));
        }
        let usage = BackendUsage::try_new(
            request.limits,
            usage.logical_fuel,
            usage.candidates,
            usage.output_bytes,
            usage.trace_entries,
        )?;
        let output_length = outcome
            .canonical_bytes()
            .map_err(BackendError::Encode)?
            .len();
        let output_length =
            u64::try_from(output_length).map_err(|_| BackendError::OutputTooLarge)?;
        if output_length > request.limits.max_output_bytes {
            return Err(BackendError::OutputTooLarge);
        }
        if output_length != usage.output_bytes {
            return Err(BackendError::OutputUsageMismatch {
                actual: output_length,
                reported: usage.output_bytes,
            });
        }
        Ok(Self {
''',
    "response revalidates usage and exact output bytes",
)
replace_exact(
    backend,
    '''        BackendUsage::try_new(
            request.limits,
            self.usage.logical_fuel,
            self.usage.candidates,
            self.usage.output_bytes,
            self.usage.trace_entries,
        )?;
        Ok(())
''',
    '''        BackendUsage::try_new(
            request.limits,
            self.usage.logical_fuel,
            self.usage.candidates,
            self.usage.output_bytes,
            self.usage.trace_entries,
        )?;
        let output_length = self
            .outcome
            .canonical_bytes()
            .map_err(BackendError::Encode)?
            .len();
        let output_length =
            u64::try_from(output_length).map_err(|_| BackendError::OutputTooLarge)?;
        if output_length > request.limits.max_output_bytes {
            return Err(BackendError::OutputTooLarge);
        }
        if output_length != self.usage.output_bytes {
            return Err(BackendError::OutputUsageMismatch {
                actual: output_length,
                reported: self.usage.output_bytes,
            });
        }
        Ok(())
''',
    "response validation rechecks exact output bytes",
)
replace_exact(
    backend,
    '''        let Ok(run) = execute_verified(&mut self.engine, &mut self.verifier, &request) else {
            return CheckResult::Indeterminate;
        };
        match run.response.outcome() {
            BackendOutcome::Accepted(accepted) => CheckResult::Accepted {
                compiled: accepted.artifact().clone(),
                reference_claim: accepted.reference_claim(),
                composition_claim: accepted.composition_claim(),
            },
''',
    '''        let Ok(run) = execute_verified(&mut self.engine, &mut self.verifier, &request) else {
            return CheckResult::Indeterminate;
        };
        let Ok(certificate_hash) = run.certificate().commitment() else {
            return CheckResult::Indeterminate;
        };
        match run.response().outcome() {
            BackendOutcome::Accepted(accepted) => {
                let Ok(reference_claim) = bind_verified_claim(
                    "zeno-fcis/backend-synthesis-reference",
                    accepted.reference_claim(),
                    certificate_hash,
                ) else {
                    return CheckResult::Indeterminate;
                };
                let Ok(composition_claim) = bind_verified_claim(
                    "zeno-fcis/backend-synthesis-composition",
                    accepted.composition_claim(),
                    certificate_hash,
                ) else {
                    return CheckResult::Indeterminate;
                };
                CheckResult::Accepted {
                    compiled: accepted.artifact().clone(),
                    reference_claim,
                    composition_claim,
                }
            }
''',
    "synthesis claims bind independent backend certificate",
)
replace_exact(
    backend,
    '''fn put_u16_length(output: &mut Vec<u8>, length: usize) -> Result<(), EncodeError> {
''',
    '''fn bind_verified_claim(
    domain: &'static str,
    claim: Hash32,
    certificate_hash: Hash32,
) -> Result<Hash32, BackendError> {
    let mut bytes = Vec::with_capacity(64);
    bytes.extend_from_slice(claim.as_bytes());
    bytes.extend_from_slice(certificate_hash.as_bytes());
    hash_bytes(domain, &bytes)
}

fn put_u16_length(output: &mut Vec<u8>, length: usize) -> Result<(), EncodeError> {
''',
    "verified synthesis claim helper",
)
replace_exact(
    backend,
    '''    /// Canonical output exceeds the declared output budget.
    OutputTooLarge,
''',
    '''    /// Canonical output exceeds the declared output budget.
    OutputTooLarge,
    /// Reported output-byte usage differs from the exactly encoded outcome.
    OutputUsageMismatch {
        /// Exact canonical outcome byte count.
        actual: u64,
        /// Backend-reported output byte count.
        reported: u64,
    },
''',
    "output usage mismatch error",
)
replace_exact(
    backend,
    '''            Self::OutputTooLarge => formatter.write_str("backend output exceeds declared bound"),
            Self::RequestMismatch => {
''',
    '''            Self::OutputTooLarge => formatter.write_str("backend output exceeds declared bound"),
            Self::OutputUsageMismatch { actual, reported } => write!(
                formatter,
                "backend reported {reported} output bytes but encoded {actual}"
            ),
            Self::RequestMismatch => {
''',
    "output usage mismatch display",
)
replace_exact(
    backend,
    '''    struct MockVerifier;

    impl BackendVerifier for MockVerifier {
''',
    '''    struct MockVerifier {
        claim_hash: Hash32,
    }

    impl BackendVerifier for MockVerifier {
''',
    "parameterized mock verifier",
)
replace_exact(
    backend,
    '''            VerificationDecision::Attested {
                claim_hash: hash(41),
            }
''',
    '''            VerificationDecision::Attested {
                claim_hash: self.claim_hash,
            }
''',
    "parameterized verifier claim",
)
replace_exact(
    backend,
    '''        let mut checker = SynthesisBackendChecker::try_new(engine, MockVerifier, template)
            .unwrap_or_else(|error| panic!("checker: {error}"));
''',
    '''        let mut checker = SynthesisBackendChecker::try_new(
            engine,
            MockVerifier {
                claim_hash: hash(41),
            },
            template,
        )
        .unwrap_or_else(|error| panic!("checker: {error}"));
''',
    "existing synthesis test verifier",
)
replace_exact(
    backend,
    '''    #[test]
    fn response_requires_advertised_operation() {
''',
    '''    #[test]
    fn response_revalidates_usage_against_request_limits() {
        let request = request(BackendOperation::Verify);
        let identity = identity(vec![BackendOperation::Verify]);
        let outcome = BackendOutcome::Rejected(
            RejectedOutcome::try_new(Value::Unit, hash(30))
                .unwrap_or_else(|error| panic!("outcome: {error}")),
        );
        let output_bytes = u64::try_from(
            outcome
                .canonical_bytes()
                .unwrap_or_else(|error| panic!("outcome bytes: {error}"))
                .len(),
        )
        .unwrap_or_else(|error| panic!("output length: {error}"));
        let permissive = BackendLimits::try_new(20_000, 200, 1_000_000, 2_000)
            .unwrap_or_else(|error| panic!("permissive limits: {error}"));
        let usage = BackendUsage::try_new(permissive, 10_001, 1, output_bytes, 1)
            .unwrap_or_else(|error| panic!("usage: {error}"));
        assert!(matches!(
            BackendResponse::try_new(&request, &identity, usage, outcome),
            Err(BackendError::UsageExceedsLimit)
        ));
    }

    #[test]
    fn response_requires_exact_output_byte_accounting() {
        let request = request(BackendOperation::Verify);
        let identity = identity(vec![BackendOperation::Verify]);
        let outcome = BackendOutcome::Rejected(
            RejectedOutcome::try_new(Value::Unit, hash(31))
                .unwrap_or_else(|error| panic!("outcome: {error}")),
        );
        let actual = u64::try_from(
            outcome
                .canonical_bytes()
                .unwrap_or_else(|error| panic!("outcome bytes: {error}"))
                .len(),
        )
        .unwrap_or_else(|error| panic!("output length: {error}"));
        let reported = actual + 1;
        let usage = BackendUsage::try_new(request.limits(), 1, 1, reported, 1)
            .unwrap_or_else(|error| panic!("usage: {error}"));
        assert!(matches!(
            BackendResponse::try_new(&request, &identity, usage, outcome),
            Err(BackendError::OutputUsageMismatch {
                actual: observed,
                reported: declared,
            }) if observed == actual && declared == reported
        ));
    }

    #[test]
    fn response_requires_advertised_operation() {
''',
    "response accounting regression tests",
)
replace_exact(
    backend,
    '''    #[test]
    fn generic_backend_drives_canonical_synthesis() {
''',
    '''    #[test]
    fn synthesis_result_binds_independent_verifier_attestation() {
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
            vec![Value::U128(2), Value::U128(1)],
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
        assert_ne!(first, second);
    }

    #[test]
    fn generic_backend_drives_canonical_synthesis() {
''',
    "verifier attestation regression test",
)

document = Path("docs/GENERIC_BACKEND_PROTOCOL.md")
replace_exact(
    document,
    '''Requests authorize logical fuel, candidate count, output bytes, and trace entries. Responses report exact usage and fail closed when usage or canonical output exceeds the request. Wall-clock deadlines, host memory exhaustion, process crashes, and service unavailability remain shell failures and never become evidence that a semantic search was complete.
''',
    '''Requests authorize logical fuel, candidate count, output bytes, and trace entries. Responses are revalidated against the exact request limits even when their `BackendUsage` value was constructed elsewhere. Canonical output bytes are computed by ZenoFCIS and must equal the reported output-byte usage exactly; over-reporting and under-reporting both fail closed. Wall-clock deadlines, host memory exhaustion, process crashes, and service unavailability remain shell failures and never become evidence that a semantic search was complete.
''',
    "document exact usage accounting",
)
replace_exact(
    document,
    '''`SynthesisBackendChecker` adapts a verified generic backend to the existing `CandidateChecker` interface. Canonical assignment ordering and search completeness remain owned by `zeno-fcis-synthesis`. The mounted backend checks one assignment at a time; it cannot reorder, omit, or terminate the outer complete-within-bounds search.
''',
    '''`SynthesisBackendChecker` adapts a verified generic backend to the existing `CandidateChecker` interface. Canonical assignment ordering and search completeness remain owned by `zeno-fcis-synthesis`. The mounted backend checks one assignment at a time; it cannot reorder, omit, or terminate the outer complete-within-bounds search. Accepted synthesis claims are re-committed together with the exact independent `BackendCertificate`, so the outer synthesis certificate binds the request, response, backend identity, verifier identity, and verifier attestation rather than only backend-supplied claim hashes.
''',
    "document synthesis certificate binding",
)

assurance = Path("tools/check_assurance.py")
replace_exact(
    assurance,
    '''    "zeno-fcis-synthesis",
)
''',
    '''    "zeno-fcis-synthesis",
    "zeno-fcis-backend",
)
''',
    "backend semantic boundary",
)
replace_exact(
    assurance,
    '''    "zeno-fcis-synthesis": 2,
    "zeno-fcis": 3,
''',
    '''    "zeno-fcis-synthesis": 2,
    "zeno-fcis-backend": 3,
    "zeno-fcis": 3,
''',
    "backend dependency ring",
)
