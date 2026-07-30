//! Construct a bounded backend request and expose the independently checked call boundary.
//!
//! A project supplies its own [`BackendEngine`] implementation for ESSO, Lean,
//! an SMT solver, or another private/external tool. A separate
//! [`BackendVerifier`] decides whether the response earns a certificate.

#[cfg(feature = "backend")]
use zeno_fcis::prelude::*;

/// Executes one mounted engine request and requires a separate verifier.
///
/// Backend output is only returned as a [`VerifiedBackendRun`] after request,
/// implementation, resource, response, verifier, and claim bindings pass.
#[cfg(feature = "backend")]
pub fn execute_with_independent_verification<E, V>(
    engine: &mut E,
    verifier: &mut V,
    request: &BackendRequest,
) -> Result<VerifiedBackendRun, BackendError>
where
    E: BackendEngine,
    V: BackendVerifier,
{
    execute_verified(engine, verifier, request)
}

#[cfg(feature = "backend")]
fn example_hash(byte: u8) -> Hash32 {
    // Examples use visible placeholders. Production integrations bind hashes
    // to the exact profile, specification, context, source, and toolchain.
    Hash32::new([byte; 32])
}

#[cfg(feature = "backend")]
fn main() -> Result<(), BackendError> {
    let limits = BackendLimits::try_new(
        10_000,    // logical fuel
        100,       // candidates
        1_000_000, // canonical response bytes
        1_000,     // retained trace entries
    )?;
    let request = BackendRequest::try_new(
        example_hash(1), // stable request identity
        example_hash(2), // exact ProjectProfile commitment
        BackendOperation::Verify,
        example_hash(3), // reviewed specification commitment
        example_hash(4), // authenticated context commitment
        Value::U128(7),  // closed canonical input
        limits,
    )?;

    // Mounting an engine is project-specific. Pass `request` to
    // `execute_with_independent_verification` only after selecting an engine
    // identity and an independent verifier under the project's policy.
    let request_commitment = request.commitment()?;
    if request_commitment == Hash32::ZERO {
        return Err(BackendError::ZeroRequestBinding);
    }

    Ok(())
}

#[cfg(not(feature = "backend"))]
fn main() {}
