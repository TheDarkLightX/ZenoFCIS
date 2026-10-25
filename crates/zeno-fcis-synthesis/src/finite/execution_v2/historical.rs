//! Verification of retained 2.4.0 evidence. No decision or publication constructor.
use crate::finite::historical_i64 as released;
use alloc::vec::Vec;
use released::v2_authority as authority;
use zeno_fcis_codec::{CanonicalEncode, CommitmentHasher};
use zeno_fcis_crypto::RustCryptoSha256;

// Preserve the reviewed historical source bytes during workspace formatting.
#[rustfmt::skip]
#[path = "../historical_24_sources.rs"]
mod sources;

/// Complete compiled historical source, framed for the raw-runtime source hash.
/// This owned serialization result supplies no decision or publication authority.
pub(crate) fn source_bytes() -> Vec<u8> {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&(sources::SOURCES.len() as u64).to_be_bytes());
    for (path, source, _) in sources::SOURCES {
        bytes.extend_from_slice(&(path.len() as u64).to_be_bytes());
        bytes.extend_from_slice(path.as_bytes());
        bytes.extend_from_slice(&(source.len() as u64).to_be_bytes());
        bytes.extend_from_slice(source);
    }
    bytes
}

#[cfg(test)]
pub(crate) fn source_paths() -> impl Iterator<Item = &'static str> {
    sources::SOURCES.iter().map(|row| row.0)
}

/// The sole released evaluator accepted for retained evidence.
pub const RELEASED_EVALUATOR: [u8; 32] = [
    0x86, 0xf5, 0x16, 0xbe, 0xf8, 0x99, 0x78, 0xcb, 0x43, 0xd3, 0xa0, 0x63, 0xde, 0xea, 0x15, 0x6a,
    0x13, 0x8a, 0xb2, 0xa3, 0xd5, 0xea, 0x77, 0x7d, 0xe3, 0x43, 0x09, 0xbd, 0x92, 0x82, 0xd4, 0xff,
];

/// Historical construction or exact retained-evidence verification refused.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Refusal {
    /// Compiled copies no longer match the checked release-copy record.
    Source,
    /// The complete policy/catalog did not bind under the released evaluator.
    Catalog,
    /// The complete retained identity differs from the library-owned binding.
    Identity,
    /// Retained publication bytes did not replay, or were not a committing outcome.
    Publication,
    /// Complete comparison of the two programs under released semantics did not hold.
    Program,
}

/// Immutable checked context for the closed 2.4.0 release.
/// Its constructor accepts an admitted complete catalog, never an evaluator digest.
///
/// ```compile_fail
/// use zeno_fcis_synthesis::finite::historical::HistoricalContext;
/// fn cannot_publish(context: &HistoricalContext<'_>) {
///     context.publish_genesis(&[]);
/// }
/// ```
///
/// ```compile_fail
/// use zeno_fcis_synthesis::finite::{historical::CheckedPublication, v2_authority::Publication};
/// fn cannot_promote(checked: CheckedPublication) {
///     let _: Publication<'_> = checked;
/// }
/// ```
///
/// ```compile_fail
/// use zeno_fcis_synthesis::finite::historical::HistoricalContext;
/// fn cannot_edit<'p>(policy: &'p mut [u8], bind: impl FnOnce(&'p [u8]) -> HistoricalContext<'p>) {
///     let context = bind(policy);
///     policy[0] ^= 1;
///     assert!(!context.identity().is_empty());
/// }
/// ```
pub struct HistoricalContext<'p> {
    authority: authority::Authority<'p>,
    policy: &'p [u8],
    schema: &'p [u8],
    descriptor: &'p released::v2_composition::Descriptor<'p>,
}

/// Bind only the released interpreter, policy and complete original schema.
pub fn bind_released_24<'p>(
    catalog: &released::v2_catalog::BoundCatalog<'p>,
) -> Result<HistoricalContext<'p>, Refusal> {
    if authority::EVALUATOR != RELEASED_EVALUATOR || sources::SOURCES.is_empty() {
        return Err(Refusal::Source);
    }
    for (_, bytes, expected) in sources::SOURCES {
        let actual = RustCryptoSha256::hash(bytes);
        let mut encoded = alloc::string::String::new();
        use core::fmt::Write as _;
        for byte in actual.as_bytes() {
            let _ = write!(encoded, "{byte:02x}");
        }
        if &encoded != expected {
            return Err(Refusal::Source);
        }
    }
    let authority = authority::bind(catalog).map_err(|_| Refusal::Catalog)?;
    Ok(HistoricalContext {
        authority,
        policy: catalog.original_contract(),
        schema: catalog.original_schema(),
        descriptor: catalog.descriptor(),
    })
}

/// Owned checked delivery data. This carries no current publication capability.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Delivery {
    /// Original ordering within the lane.
    pub ordinal: u32,
    /// Original channel identifier.
    pub channel: u32,
    /// Bound original destination type.
    pub destination_root: u32,
    /// Bound original payload type.
    pub payload_root: u32,
    /// Original destination value bytes.
    pub destination: Vec<u8>,
    /// Original payload value bytes.
    pub payload: Vec<u8>,
    /// Original idempotency value bytes.
    pub idempotency: Vec<u8>,
}

/// Owned reporting result of exact retained-publication verification.
/// It cannot be passed to a current shell's commit API.
#[derive(Debug)]
pub struct CheckedPublication {
    subject: Vec<u8>,
    poststate: Vec<u8>,
    effects: Vec<Delivery>,
    outbox: Vec<Delivery>,
}

impl CheckedPublication {
    /// Exact retained publication bytes, after full replay.
    pub fn subject(&self) -> &[u8] {
        &self.subject
    }
    /// Exact original successor envelope.
    pub fn poststate(&self) -> &[u8] {
        &self.poststate
    }
    /// Checked original effects, in order.
    pub fn effects(&self) -> &[Delivery] {
        &self.effects
    }
    /// Checked original outbox, in order.
    pub fn outbox(&self) -> &[Delivery] {
        &self.outbox
    }
}

/// Real measured work is retained even when exact replay refuses.
#[derive(Debug)]
pub struct Verification {
    result: Result<CheckedPublication, Refusal>,
    usage: crate::finite::V2Usage,
}

impl Verification {
    /// Actual interpreter accounting; encoding and construction are outside this meter.
    pub fn usage(&self) -> crate::finite::V2Usage {
        self.usage
    }
    /// Consume owned checked reporting data, or the refusal.
    pub fn into_result(self) -> Result<CheckedPublication, Refusal> {
        self.result
    }
}

fn delivery(value: &authority::WireDelivery) -> Delivery {
    Delivery {
        ordinal: value.ordinal(),
        channel: value.channel(),
        destination_root: value.destination_root(),
        payload_root: value.payload_root(),
        destination: value.destination().to_vec(),
        payload: value.payload().to_vec(),
        idempotency: value.idempotency().to_vec(),
    }
}

fn verified(outcome: authority::PublicationOutcome<'_>) -> Verification {
    let usage = outcome.evaluation().usage();
    let result = match outcome {
        authority::PublicationOutcome::Commit(publication) => Ok(CheckedPublication {
            subject: publication.subject().to_vec(),
            poststate: publication.poststate().to_vec(),
            effects: publication.effects().iter().map(delivery).collect(),
            outbox: publication.outbox().iter().map(delivery).collect(),
        }),
        _ => Err(Refusal::Publication),
    };
    Verification { result, usage }
}

impl HistoricalContext<'_> {
    /// Identity comes solely from the copied released Authority and its admitted policy.
    pub fn identity(&self) -> &[u8] {
        self.authority.identity()
    }
    /// Compare complete persisted identity bytes; they cannot select an evaluator.
    pub fn verify_identity(&self, retained: &[u8]) -> Result<(), Refusal> {
        if retained != self.identity() {
            return Err(Refusal::Identity);
        }
        Ok(())
    }
    /// Complete admitted canonical policy, immutable for this context's lifetime.
    pub fn policy(&self) -> &[u8] {
        self.policy
    }
    /// Complete admitted original schema.
    pub fn schema(&self) -> &[u8] {
        self.schema
    }
    /// Released scalar data for exhaustive upgrade comparisons; no authority is granted.
    pub fn program(&self) -> &released::ScalarProgram<'_> {
        &self.descriptor.program
    }
    /// Recheck the retained adoption using the released interpreter for both
    /// programs. This does not assert equality with current i128 evaluation.
    pub fn equivalent_released_candidate(
        &self,
        current: &crate::finite::V2ScalarProgram<'_>,
        cap: u64,
    ) -> Result<u64, Refusal> {
        use crate::finite::Domain;
        let old = self.program();
        let converted = |domain: &released::Domain| match *domain {
            released::Domain::Bool => Domain::Bool,
            released::Domain::Int { min, max } => Domain::Int {
                min: i128::from(min),
                max: i128::from(max),
            },
        };
        if old.inputs.iter().map(converted).collect::<Vec<_>>() != current.inputs
            || old.outputs.iter().map(converted).collect::<Vec<_>>() != current.outputs
        {
            return Err(Refusal::Program);
        }
        let data = crate::finite::Program::try_new(
            current.inputs.to_vec(),
            current.outputs.to_vec(),
            current.nodes.to_vec(),
            current.roots.to_vec(),
        )
        .map_err(|_| Refusal::Program)?;
        let bytes = crate::finite_runtime::i64_program_value(&data)
            .map_err(|_| Refusal::Program)?
            .canonical_bytes()
            .map_err(|_| Refusal::Program)?;
        let candidate =
            crate::finite_runtime::import_i64_program(&bytes).map_err(|_| Refusal::Program)?;
        let mut count = 1_u128;
        let mut input = Vec::new();
        for domain in old.inputs {
            let (min, max) = match *domain {
                released::Domain::Bool => (0, 1),
                released::Domain::Int { min, max } => (min, max),
            };
            input.push(min);
            count = count
                .checked_mul((i128::from(max) - i128::from(min) + 1) as u128)
                .ok_or(Refusal::Program)?;
        }
        if count > u128::from(cap) {
            return Err(Refusal::Program);
        }
        let count = u64::try_from(count).map_err(|_| Refusal::Program)?;
        let budget = u64::try_from(old.nodes.len().max(current.nodes.len()))
            .map_err(|_| Refusal::Program)?;
        let limits =
            crate::finite::v2_zero_limits().with_limit(crate::finite::V2Resource::Step, budget);
        for ordinal in 0..count {
            let old = released::execute(
                old.inputs,
                old.outputs,
                old.nodes,
                old.roots,
                &input,
                limits,
            )
            .into_parts()
            .0;
            let actual = candidate
                .execute_v2(&input, limits)
                .into_parts()
                .0
                .map(|values| values.into_iter().map(i128::from).collect::<Vec<_>>());
            let old = old
                .map(|values| values.into_iter().map(i128::from).collect::<Vec<_>>())
                .map_err(|error| match error {
                    released::Failure::InputDomain => {
                        crate::finite::V2ExecutionFailure::InputDomain
                    }
                    released::Failure::Reference => crate::finite::V2ExecutionFailure::Reference,
                    released::Failure::Arithmetic => crate::finite::V2ExecutionFailure::Arithmetic,
                    released::Failure::OutputDomain => {
                        crate::finite::V2ExecutionFailure::OutputDomain
                    }
                    released::Failure::Budget(error) => {
                        crate::finite::V2ExecutionFailure::Budget(error)
                    }
                });
            if old != actual {
                return Err(Refusal::Program);
            }
            let mut more = false;
            for index in (0..input.len()).rev() {
                let (min, max) = match self.program().inputs[index] {
                    released::Domain::Bool => (0, 1),
                    released::Domain::Int { min, max } => (min, max),
                };
                if input[index] < max {
                    input[index] += 1;
                    more = true;
                    break;
                }
                input[index] = min;
            }
            if more != (ordinal + 1 < count) {
                return Err(Refusal::Program);
            }
        }
        Ok(count)
    }
    /// Recompute genuine historical genesis and bind every retained publication byte.
    pub fn verify_genesis(&self, state: &[u8], expected: &[u8]) -> Verification {
        verified(self.authority.replay_genesis_publication(state, expected))
    }
    /// Recompute the complete historical transition, laws, usage and original wires.
    pub fn verify_publication(
        &self,
        state: &[u8],
        command: &[u8],
        context: &[u8],
        expected: &[u8],
    ) -> Verification {
        verified(self.authority.replay_publication(
            released::v2_composition::Raw {
                state,
                command,
                context,
            },
            expected,
        ))
    }
}
