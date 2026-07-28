from pathlib import Path


def replace_exact(path: Path, old: str, new: str, label: str) -> None:
    text = path.read_text(encoding="utf-8")
    if old not in text:
        if new in text:
            return
        raise SystemExit(f"missing formal-CBC hardening site: {label}")
    path.write_text(text.replace(old, new, 1), encoding="utf-8")


assurance = Path("tools/check_assurance.py")
replace_exact(
    assurance,
    '    "zeno-fcis-transition",\n    "zeno-fcis-security",\n',
    '    "zeno-fcis-transition",\n    "zeno-fcis-cbc",\n    "zeno-fcis-security",\n',
    "CBC semantic boundary",
)
replace_exact(
    assurance,
    '    "zeno-fcis-transition": 3,\n    "zeno-fcis-security": 2,\n',
    '    "zeno-fcis-transition": 3,\n    "zeno-fcis-cbc": 3,\n    "zeno-fcis-security": 2,\n',
    "CBC dependency ring",
)

cbc = Path("crates/zeno-fcis-cbc/src/lib.rs")
replace_exact(
    cbc,
    "use alloc::boxed::Box;\nuse alloc::vec::Vec;\n",
    "use alloc::boxed::Box;\nuse alloc::format;\nuse alloc::vec::Vec;\n",
    "format import",
)
replace_exact(
    cbc,
    "use zeno_fcis_receipt::SealError;\nuse zeno_fcis_transition",
    "use zeno_fcis_receipt::SealError;\nuse zeno_fcis_schema::SchemaAdmittedTypeEnvelope;\nuse zeno_fcis_transition",
    "schema-admitted input import",
)
replace_exact(
    cbc,
    """/// Maximum blockers retained in one evaluation report.
pub const MAX_CBC_BLOCKERS: usize = 65_536;

/// Semantic class of one project law.
""",
    """/// Maximum blockers retained in one evaluation report.
pub const MAX_CBC_BLOCKERS: usize = 65_536;
/// Generated command/context commitment format version.
pub const CBC_INPUT_COMMITMENT_FORMAT_VERSION: u16 = 1;

/// Role of one schema-admitted transition input.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[repr(u8)]
pub enum InputRole {
    /// Project command value.
    Command = 0,
    /// Authenticated project context value.
    Context = 1,
}

impl InputRole {
    fn expected_type(self, catalog: &ProjectCatalog) -> u32 {
        match self {
            Self::Command => catalog.profile().command_type().get(),
            Self::Context => catalog.profile().context_type().get(),
        }
    }

    fn domain_suffix(self) -> &'static str {
        match self {
            Self::Command => "command",
            Self::Context => "context",
        }
    }
}

impl CanonicalEncode for InputRole {
    fn encode_to(&self, output: &mut Vec<u8>) -> Result<(), EncodeError> {
        output.push(*self as u8);
        Ok(())
    }
}

/// Semantic class of one project law.
""",
    "input role model",
)
replace_exact(
    cbc,
    "use zeno_fcis_project::{RegistryKind, SemanticId, StableName};\n",
    "use zeno_fcis_project::{RegistryKind, SemanticId, StableName};\nuse zeno_fcis_receipt::SealError;\n",
    "SealError import",
)
replace_exact(
    cbc,
    """    command_hash: Hash32,
    context_hash: Hash32,
    pre_root: Hash32,
""",
    """    command: Value,
    context: Value,
    command_hash: Hash32,
    context_hash: Hash32,
    pre_root: Hash32,
""",
    "law subject admitted inputs",
)
replace_exact(
    cbc,
    """    pub fn from_transition<H: CommitmentHasher>(
        law_set: &LawSet,
        catalog: &ProjectCatalog,
        pre_state: &Value,
        state_domain: Domain<'_>,
        decision: &TransitionDecision,
    ) -> Result<Self, CbcError> {
        let catalog_hash = catalog.commitment::<H>()?;
        if law_set.profile_hash != catalog.profile_hash()
            || law_set.schema_hash != catalog.schema_hash()
            || law_set.catalog_hash != catalog_hash
        {
            return Err(CbcError::LawSetBindingMismatch);
        }
        validate_transition_decision::<H>(decision, catalog, pre_state, state_domain)?;

        match decision {
""",
    """    pub fn from_transition<H: CommitmentHasher>(
        law_set: &LawSet,
        catalog: &ProjectCatalog,
        pre_state: &Value,
        command: &SchemaAdmittedTypeEnvelope,
        context: &SchemaAdmittedTypeEnvelope,
        state_domain: Domain<'_>,
        decision: &TransitionDecision,
    ) -> Result<Self, CbcError> {
        let catalog_hash = catalog.commitment::<H>()?;
        if law_set.profile_hash != catalog.profile_hash()
            || law_set.schema_hash != catalog.schema_hash()
            || law_set.catalog_hash != catalog_hash
        {
            return Err(CbcError::LawSetBindingMismatch);
        }
        let command_hash = input_commitment::<H>(catalog, InputRole::Command, command)?;
        let context_hash = input_commitment::<H>(catalog, InputRole::Context, context)?;
        validate_transition_decision::<H>(decision, catalog, pre_state, state_domain)?;

        match decision {
""",
    "law subject admitted input validation",
)
replace_exact(
    cbc,
    """                law_set,
                pre_state,
                state_domain,
                DecisionKind::Accept,
""",
    """                law_set,
                pre_state,
                command.value().value(),
                context.value().value(),
                command_hash,
                context_hash,
                state_domain,
                DecisionKind::Accept,
""",
    "accept admitted inputs",
)
replace_exact(
    cbc,
    """                law_set,
                pre_state,
                state_domain,
                DecisionKind::CommittedFailure,
""",
    """                law_set,
                pre_state,
                command.value().value(),
                context.value().value(),
                command_hash,
                context_hash,
                state_domain,
                DecisionKind::CommittedFailure,
""",
    "committed-failure admitted inputs",
)
replace_exact(
    cbc,
    """                let receipt = rejected.receipt();
                let bindings = receipt.bindings();
                Ok(Self {
""",
    """                let receipt = rejected.receipt();
                let bindings = receipt.bindings();
                validate_input_binding(
                    InputRole::Command,
                    bindings.command_hash,
                    command_hash,
                )?;
                validate_input_binding(
                    InputRole::Context,
                    bindings.context_hash,
                    context_hash,
                )?;
                Ok(Self {
""",
    "reject input binding",
)
replace_exact(
    cbc,
    """                    decision_kind: DecisionKind::Reject,
                    reason_id: Some(rejected.reason_id()),
                    command_hash: bindings.command_hash,
                    context_hash: bindings.context_hash,
""",
    """                    decision_kind: DecisionKind::Reject,
                    reason_id: Some(rejected.reason_id()),
                    command: command.value().value().clone(),
                    context: context.value().value().clone(),
                    command_hash,
                    context_hash,
""",
    "reject subject inputs",
)
replace_exact(
    cbc,
    """    fn from_committed<H: CommitmentHasher>(
        law_set: &LawSet,
        pre_state: &Value,
        state_domain: Domain<'_>,
        decision_kind: DecisionKind,
""",
    """    fn from_committed<H: CommitmentHasher>(
        law_set: &LawSet,
        pre_state: &Value,
        command: &Value,
        context: &Value,
        command_hash: Hash32,
        context_hash: Hash32,
        state_domain: Domain<'_>,
        decision_kind: DecisionKind,
""",
    "committed subject signature",
)
replace_exact(
    cbc,
    """        let body = bundle.body();
        let bindings = body.bindings();
        let applied = bundle.validate_and_apply::<H>(pre_state, state_domain)?;
""",
    """        let body = bundle.body();
        let bindings = body.bindings();
        validate_input_binding(InputRole::Command, bindings.command_hash, command_hash)?;
        validate_input_binding(InputRole::Context, bindings.context_hash, context_hash)?;
        let applied = bundle.validate_and_apply::<H>(pre_state, state_domain)?;
""",
    "committed input binding",
)
replace_exact(
    cbc,
    """            decision_kind,
            reason_id,
            command_hash: bindings.command_hash,
            context_hash: bindings.context_hash,
""",
    """            decision_kind,
            reason_id,
            command: command.clone(),
            context: context.clone(),
            command_hash,
            context_hash,
""",
    "committed subject inputs",
)
replace_exact(
    cbc,
    """    /// Returns the admitted pre-state.
    #[must_use]
    pub const fn pre_state(&self) -> &Value {
""",
    """    /// Returns the exact schema-admitted command value.
    #[must_use]
    pub const fn command(&self) -> &Value {
        &self.command
    }

    /// Returns the exact schema-admitted authenticated context value.
    #[must_use]
    pub const fn context(&self) -> &Value {
        &self.context
    }

    /// Returns the admitted pre-state.
    #[must_use]
    pub const fn pre_state(&self) -> &Value {
""",
    "law subject input getters",
)
replace_exact(
    cbc,
    """        put_optional_hash(output, self.candidate_id);
        put_blob(output, &self.pre_state.canonical_bytes()?)?;
""",
    """        put_optional_hash(output, self.candidate_id);
        put_blob(output, &self.command.canonical_bytes()?)?;
        put_blob(output, &self.context.canonical_bytes()?)?;
        put_blob(output, &self.pre_state.canonical_bytes()?)?;
""",
    "law subject canonical inputs",
)
replace_exact(
    cbc,
    """    /// The law set and supplied catalog/profile/schema differ.
    LawSetBindingMismatch,
    /// Evidence names another law.
""",
    """    /// The law set and supplied catalog/profile/schema differ.
    LawSetBindingMismatch,
    /// A command/context envelope belongs to another schema.
    InputSchemaMismatch {
        /// Input role.
        role: InputRole,
        /// Required schema commitment.
        expected: Hash32,
        /// Observed schema commitment.
        actual: Hash32,
    },
    /// A command/context envelope has the wrong schema type.
    InputTypeMismatch {
        /// Input role.
        role: InputRole,
        /// Required type identifier.
        expected: u32,
        /// Observed type identifier.
        actual: u32,
    },
    /// A recomputed command/context commitment differs from the transition binding.
    InputCommitmentMismatch {
        /// Input role.
        role: InputRole,
        /// Transition-bound commitment.
        expected: Hash32,
        /// Recomputed commitment.
        actual: Hash32,
    },
    /// A command/context commitment used the zero sentinel.
    ZeroInputCommitment(InputRole),
    /// Evidence names another law.
""",
    "input binding errors",
)
replace_exact(
    cbc,
    """            Self::LawSetBindingMismatch => {
                formatter.write_str("CBC law-set catalog/profile/schema binding differs")
            }
            Self::EvidenceLawMismatch => formatter.write_str("CBC evidence names another law"),
""",
    """            Self::LawSetBindingMismatch => {
                formatter.write_str("CBC law-set catalog/profile/schema binding differs")
            }
            Self::InputSchemaMismatch {
                role,
                expected,
                actual,
            } => write!(
                formatter,
                "CBC {role:?} schema mismatch: expected {expected}, actual {actual}"
            ),
            Self::InputTypeMismatch {
                role,
                expected,
                actual,
            } => write!(
                formatter,
                "CBC {role:?} type mismatch: expected {expected}, actual {actual}"
            ),
            Self::InputCommitmentMismatch {
                role,
                expected,
                actual,
            } => write!(
                formatter,
                "CBC {role:?} commitment mismatch: expected {expected}, actual {actual}"
            ),
            Self::ZeroInputCommitment(role) => {
                write!(formatter, "CBC {role:?} commitment used the zero sentinel")
            }
            Self::EvidenceLawMismatch => formatter.write_str("CBC evidence names another law"),
""",
    "input binding error display",
)
replace_exact(
    cbc,
    """/// Complete borrowed inputs selected by a law-verification authority.
pub struct LawVerificationContext<'a, 'd> {
    law_set: &'a LawSet,
    catalog: &'a ProjectCatalog,
    pre_state: &'a Value,
    state_domain: Domain<'d>,
    evidence: &'a [LawEvidence],
}

impl<'a, 'd> LawVerificationContext<'a, 'd> {
    /// Binds the exact law set, catalog, pre-state, state domain, and evidence.
    #[must_use]
    pub const fn new(
        law_set: &'a LawSet,
        catalog: &'a ProjectCatalog,
        pre_state: &'a Value,
        state_domain: Domain<'d>,
        evidence: &'a [LawEvidence],
    ) -> Self {
        Self {
            law_set,
            catalog,
            pre_state,
            state_domain,
            evidence,
        }
    }
}
""",
    """/// Complete borrowed inputs selected by a law-verification authority.
pub struct LawVerificationContext<'a, 'd> {
    law_set: &'a LawSet,
    catalog: &'a ProjectCatalog,
    pre_state: &'a Value,
    command: &'a SchemaAdmittedTypeEnvelope,
    context: &'a SchemaAdmittedTypeEnvelope,
    state_domain: Domain<'d>,
    evidence: &'a [LawEvidence],
}

impl<'a, 'd> LawVerificationContext<'a, 'd> {
    /// Binds the law set, catalog, state, admitted command/context, domain, and evidence.
    #[must_use]
    pub const fn new(
        law_set: &'a LawSet,
        catalog: &'a ProjectCatalog,
        pre_state: &'a Value,
        command: &'a SchemaAdmittedTypeEnvelope,
        context: &'a SchemaAdmittedTypeEnvelope,
        state_domain: Domain<'d>,
        evidence: &'a [LawEvidence],
    ) -> Self {
        Self {
            law_set,
            catalog,
            pre_state,
            command,
            context,
            state_domain,
            evidence,
        }
    }
}
""",
    "law verification admitted inputs",
)
replace_exact(
    cbc,
    """        context.catalog,
        context.pre_state,
        context.state_domain,
        &decision,
""",
    """        context.catalog,
        context.pre_state,
        context.command,
        context.context,
        context.state_domain,
        &decision,
""",
    "law verification subject inputs",
)
replace_exact(
    cbc,
    """fn hash_canonical<H: CommitmentHasher>(
""",
    """fn input_commitment<H: CommitmentHasher>(
    catalog: &ProjectCatalog,
    role: InputRole,
    admitted: &SchemaAdmittedTypeEnvelope,
) -> Result<Hash32, CbcError> {
    let expected_schema = catalog.schema_hash();
    if admitted.schema_hash() != expected_schema {
        return Err(CbcError::InputSchemaMismatch {
            role,
            expected: expected_schema,
            actual: admitted.schema_hash(),
        });
    }
    let expected_type = role.expected_type(catalog);
    if admitted.type_id().get() != expected_type {
        return Err(CbcError::InputTypeMismatch {
            role,
            expected: expected_type,
            actual: admitted.type_id().get(),
        });
    }
    let domain_name = format!(
        "{}/{}",
        catalog.profile().domain_prefix().as_str(),
        role.domain_suffix(),
    );
    let domain = Domain::new(&domain_name, CBC_INPUT_COMMITMENT_FORMAT_VERSION)?;
    let bytes = admitted.canonical_bytes()?;
    let value = commitment::<H>(domain, &bytes)?;
    if value == Hash32::ZERO {
        return Err(CbcError::ZeroInputCommitment(role));
    }
    Ok(value)
}

fn validate_input_binding(
    role: InputRole,
    expected: Hash32,
    actual: Hash32,
) -> Result<(), CbcError> {
    if expected == actual {
        Ok(())
    } else {
        Err(CbcError::InputCommitmentMismatch {
            role,
            expected,
            actual,
        })
    }
}

fn hash_canonical<H: CommitmentHasher>(
""",
    "input commitment helpers",
)
replace_exact(
    cbc,
    """            reason_id: None,
            command_hash: hash(6),
            context_hash: hash(7),
""",
    """            reason_id: None,
            command: Value::U128(12),
            context: Value::Bool(true),
            command_hash: hash(6),
            context_hash: hash(7),
""",
    "test subject admitted inputs",
)
replace_exact(
    cbc,
    """        let evidence = exact_evidence(&laws, &definition, &original, hash(30), hash(31));
        let mutated = subject(101);
""",
    """        let evidence = exact_evidence(&laws, &definition, &original, hash(30), hash(31));
        let mut mutated = subject(100);
        mutated.command = Value::U128(13);
""",
    "command mutation evidence test",
)

replace_exact(
    cbc,
    """    /// Patch application failed.
    Patch(PatchError),
    /// Canonical encoding or commitment construction failed.
""",
    """    /// Patch application failed.
    Patch(PatchError),
    /// Sealed bundle validation or reconstruction failed.
    Seal(SealError),
    /// Canonical encoding or commitment construction failed.
""",
    "SealError variant",
)
replace_exact(
    cbc,
    """impl From<EncodeError> for CbcError {
""",
    """impl From<SealError> for CbcError {
    fn from(error: SealError) -> Self {
        Self::Seal(error)
    }
}

impl From<EncodeError> for CbcError {
""",
    "SealError conversion",
)
replace_exact(
    cbc,
    """            Self::Patch(error) => write!(formatter, "CBC patch failed: {error}"),
            Self::Encode(error) => write!(formatter, "CBC encoding failed: {error}"),
""",
    """            Self::Patch(error) => write!(formatter, "CBC patch failed: {error}"),
            Self::Seal(error) => write!(formatter, "CBC sealed bundle failed: {error}"),
            Self::Encode(error) => write!(formatter, "CBC encoding failed: {error}"),
""",
    "SealError display",
)

umbrella = Path("crates/zeno-fcis/src/lib.rs")
replace_exact(
    umbrella,
    """    CBC_FORMAT_VERSION, CbcError, DecisionScope, LawBlocker, LawCheck, LawChecker, LawClaim,
""",
    """    CBC_FORMAT_VERSION, CBC_INPUT_COMMITMENT_FORMAT_VERSION, CbcError, DecisionScope,
    InputRole, LawBlocker, LawCheck, LawChecker, LawClaim,
""",
    "CBC input exports",
)
replace_exact(
    umbrella,
    """    LawKind, LawRequirement, LawSet, LawSubject, LawVerificationOutcome, LawVerifiedTransition,
""",
    """    LawKind, LawRequirement, LawSet, LawSubject, LawVerificationContext,
    LawVerificationOutcome, LawVerifiedTransition,
""",
    "CBC umbrella context export",
)
