//! The contract as library values: the policy bytes come from the library's
//! own encoder, and the catalog binding the generated `checked_catalog`
//! performs at run time must accept them before they are returned.
//!
//! The same values bind a library Authority in memory, so a contract, or a
//! mutant of one, can be evaluated through the library's own route without
//! writing any file.

use zeno_fcis_synthesis::finite::{
    Domain as ScalarDomain, Op, V2InputField as InputField, V2InputLeaf as InputLeaf,
    V2InputVariant as InputVariant, V2Resource as Resource, V2ScalarProgram,
    canonical_v2::schema as s, v2_authority as authority, v2_catalog as catalog,
    v2_composition as c, v2_laws as l, v2_zero_limits,
};

use super::ContractError;
use super::declarations::{Form, Leaf, ROOTS, Source};
use super::expr::Rounding;
use super::graph::{Atom, LawOp, Observation, ScalarOp};
use super::model::{self, Contract, Expr, RootSchema};
use super::rules::{Class, LawKind};

/// Encodes the policy and checks the complete catalog binding.
pub(super) fn encode(contract: &Contract<'_>, schema: &[u8]) -> Result<Vec<u8>, ContractError> {
    bound(contract, schema, |policy, _| Ok(policy.to_vec()))
}

/// Binds the library Authority of a contract, after the same catalog binding
/// `encode` checks, and runs `use_authority` on it. Nothing is written.
///
/// # Errors
/// Returns the library's catalog or Authority refusal.
pub(super) fn with_authority<R>(
    contract: &Contract<'_>,
    schema: &[u8],
    use_authority: impl FnOnce(&authority::Authority<'_>) -> R,
) -> Result<R, ContractError> {
    bound(contract, schema, |_, catalog| {
        let bound = authority::bind(catalog).map_err(|refusal| {
            ContractError::new(
                "library authority",
                format!("refused the generated contract: {refusal:?}"),
            )
        })?;
        Ok(use_authority(&bound))
    })
}

/// Builds the library values the descriptor borrows, encodes the policy,
/// checks the complete catalog binding and runs `use_bound` on the policy
/// bytes and the bound catalog.
fn bound<R>(
    contract: &Contract<'_>,
    schema: &[u8],
    use_bound: impl FnOnce(&[u8], &catalog::BoundCatalog<'_>) -> Result<R, ContractError>,
) -> Result<R, ContractError> {
    let declarations = contract.declarations;
    let fields: Vec<Vec<s::Field<'_>>> = declarations
        .types
        .values()
        .map(|declared| match &declared.form {
            Form::Record(fields) => fields
                .iter()
                .map(|field| s::Field {
                    id: field.id,
                    name: field.name.as_bytes(),
                    type_id: field.type_id,
                })
                .collect(),
            _ => Vec::new(),
        })
        .collect();
    let variants: Vec<Vec<s::Variant<'_>>> = declarations
        .types
        .values()
        .map(|declared| match &declared.form {
            Form::Sum(variants) => variants
                .iter()
                .map(|variant| s::Variant {
                    id: variant.id,
                    name: variant.name.as_bytes(),
                })
                .collect(),
            _ => Vec::new(),
        })
        .collect();
    let definitions: Vec<s::Definition<'_>> = declarations
        .types
        .iter()
        .zip(fields.iter().zip(&variants))
        .map(|((id, declared), (fields, variants))| s::Definition {
            id: *id,
            name: declared.name.as_bytes(),
            kind: match &declared.form {
                Form::Record(_) => s::Kind::Record(fields),
                Form::Sum(_) => s::Kind::Sum(variants),
                Form::Leaf(Leaf::Bool) => s::Kind::Bool,
                Form::Leaf(Leaf::I128 { min, max }) => s::Kind::I128 {
                    min: *min,
                    max: *max,
                },
                Form::Leaf(Leaf::Text { min, max }) => s::Kind::Text {
                    min: *min,
                    max: *max,
                },
            },
        })
        .collect();
    let description = s::Description {
        profile: declarations.profile.as_bytes(),
        version: 1,
        root: ROOTS[0].1,
        definitions: &definitions,
    };
    let frame = |index: usize| c::FrameBinding {
        root: ROOTS[index].1,
        schema: contract.commitment,
        max_bytes: contract.frame_bytes[index],
    };
    let framing = c::Framing {
        state: frame(0),
        command: frame(1),
        context: frame(2),
    };
    let channel_roots: Vec<(u32, u32, u32)> = contract
        .channels
        .iter()
        .map(|channel| (channel.id, channel.destination_type, channel.payload_type))
        .collect();

    let roots: Vec<Root> = contract.roots.iter().map(Root::from).collect();
    let output_types: Vec<InputLeaf> = contract.output_types.iter().map(input_leaf).collect();
    let inputs: Vec<ScalarDomain> = contract.input_domains.iter().map(scalar_domain).collect();
    let outputs: Vec<ScalarDomain> = contract.output_domains.iter().map(scalar_domain).collect();
    let nodes = contract
        .nodes
        .iter()
        .map(scalar_op)
        .collect::<Result<Vec<Op>, _>>()?;
    let program_roots = contract
        .program_roots
        .iter()
        .map(|root| small(*root))
        .collect::<Result<Vec<u16>, _>>()?;
    let bindings: Vec<c::Binding> = contract
        .bindings
        .iter()
        .map(|(origin, field)| c::Binding {
            source: source(*origin),
            selector: match field {
                Some(field) => c::Selector::Field(*field),
                None => c::Selector::Root,
            },
        })
        .collect();

    let payloads: Vec<Vec<Vec<c::PayloadField<'_>>>> = contract
        .branches
        .iter()
        .map(|branch| {
            branch
                .outbox
                .iter()
                .map(|plan| {
                    plan.payload
                        .iter()
                        .map(|(field, value)| c::PayloadField {
                            field: *field,
                            value: expr(value),
                        })
                        .collect()
                })
                .collect()
        })
        .collect();
    let plans: Vec<Vec<c::DeliveryPlan<'_>>> = contract
        .branches
        .iter()
        .zip(&payloads)
        .map(|(branch, payloads)| {
            branch
                .outbox
                .iter()
                .zip(payloads)
                .map(|(plan, payload)| c::DeliveryPlan {
                    ordinal: plan.ordinal,
                    channel: plan.channel,
                    when: c::Expr::Constant(c::Atom::Bool(true)),
                    destination: c::Expr::Constant(atom(&plan.destination)),
                    payload,
                    idempotency: c::Expr::Constant(c::Atom::U128(plan.idempotency)),
                })
                .collect()
        })
        .collect();
    let assignments: Vec<Vec<c::Assignment<'_>>> = contract
        .branches
        .iter()
        .map(|branch| {
            branch
                .assignments
                .iter()
                .map(|assignment| c::Assignment {
                    field: assignment.field,
                    value: expr(&assignment.value),
                    domain: domain(&assignment.domain),
                })
                .collect()
        })
        .collect();
    let branches: Vec<c::Branch<'_>> = contract
        .branches
        .iter()
        .zip(assignments.iter().zip(&plans))
        .enumerate()
        .map(|(code, (branch, (assignments, outbox)))| c::Branch {
            code: i128::try_from(code).unwrap_or(i128::MAX),
            class: class(branch.class),
            reason: branch.reason,
            assignments,
            effects: &[],
            outbox,
        })
        .collect();
    let reasons: Vec<c::Reason> = contract
        .reasons
        .iter()
        .map(|(id, reason_class)| c::Reason {
            id: *id,
            class: class(*reason_class),
        })
        .collect();
    let channel_payloads: Vec<Vec<c::TypedField<'_>>> = contract
        .channels
        .iter()
        .map(|channel| {
            channel
                .payload
                .iter()
                .map(|(field, value)| c::TypedField {
                    field: *field,
                    domain: domain(value),
                })
                .collect()
        })
        .collect();
    let channels: Vec<c::Channel<'_>> = contract
        .channels
        .iter()
        .zip(&channel_payloads)
        .map(|(channel, payload)| c::Channel {
            id: channel.id,
            destination: domain(&channel.destination),
            payload,
            idempotency: c::Domain::U128 { min: 0, max: 0 },
        })
        .collect();
    let law_nodes: Vec<Vec<l::Op<'_>>> = contract
        .laws
        .iter()
        .map(|law| law.nodes.iter().map(law_op).collect())
        .collect();
    let laws: Vec<l::Law<'_>> = contract
        .laws
        .iter()
        .zip(&law_nodes)
        .map(|(law, nodes)| l::Law {
            id: law.id,
            kind: law_kind(law.kind),
            scope: scope(law.scope),
            genesis: law.genesis,
            program: l::Program {
                nodes,
                root: law.root,
            },
        })
        .collect();
    let required: Vec<u32> = contract.laws.iter().map(|law| law.id).collect();
    let budgets = contract.budgets;
    let descriptor = c::Descriptor {
        state: roots[0].schema(),
        command: roots[1].schema(),
        context: roots[2].schema(),
        program: V2ScalarProgram {
            inputs: &inputs,
            outputs: &outputs,
            nodes: &nodes,
            roots: &program_roots,
        },
        bindings: &bindings,
        output_types: &output_types,
        decision_output: 0,
        branches: &branches,
        reasons: &reasons,
        channels: &channels,
        laws: &laws,
        required: &required,
        limits: v2_zero_limits()
            .with_limit(Resource::Read, budgets.read)
            .with_limit(Resource::Write, budgets.write)
            .with_limit(Resource::Candidate, 1)
            .with_limit(Resource::Effect, 1)
            .with_limit(Resource::Byte, budgets.byte)
            .with_limit(Resource::WitnessByte, 0)
            .with_limit(Resource::Depth, 0)
            .with_limit(Resource::Step, budgets.step),
    };
    let policy = authority::policy_bytes(&descriptor, schema, &framing, &channel_roots)
        .ok_or_else(|| {
            ContractError::new("v2/policy.zcve", "the library policy encoding overflowed")
        })?;
    let (types, fields, variants) = contract.schema_counts();
    let limits = catalog::Limits {
        schema: s::Limits {
            bytes: u64::try_from(schema.len()).unwrap_or(u64::MAX),
            types: u32::try_from(types).unwrap_or(u32::MAX),
            fields: u32::try_from(fields).unwrap_or(u32::MAX),
            variants: u32::try_from(variants).unwrap_or(u32::MAX),
        },
        contract_bytes: u64::try_from(policy.len()).unwrap_or(u64::MAX),
    };
    let catalog = catalog::bind_original(
        schema,
        &description,
        limits,
        &policy,
        &descriptor,
        &framing,
        &channel_roots,
    )
    .map_err(|failure| {
        ContractError::new(
            "library catalog",
            format!("refused the generated contract: {failure:?}"),
        )
    })?;
    use_bound(&policy, &catalog)
}

/// An owned root input schema the descriptor borrows.
enum Root {
    Record(Vec<InputField>),
    Leaf(InputLeaf),
}

impl From<&RootSchema> for Root {
    fn from(root: &RootSchema) -> Self {
        match root {
            RootSchema::Record(fields) => Self::Record(
                fields
                    .iter()
                    .map(|(id, leaf)| InputField {
                        id: *id,
                        leaf: input_leaf(leaf),
                    })
                    .collect(),
            ),
            RootSchema::Leaf(leaf) => Self::Leaf(input_leaf(leaf)),
        }
    }
}

impl Root {
    fn schema(&self) -> c::Schema<'_> {
        match self {
            Self::Record(fields) => c::Schema::Record(fields),
            Self::Leaf(leaf) => c::Schema::Leaf(leaf),
        }
    }
}

fn input_leaf(leaf: &model::InputLeaf) -> InputLeaf {
    match leaf {
        model::InputLeaf::Bool => InputLeaf::Bool,
        model::InputLeaf::I128 { min, max } => InputLeaf::I128 {
            min: *min,
            max: *max,
        },
        model::InputLeaf::Sum { type_id, variants } => InputLeaf::Sum {
            type_id: *type_id,
            min: variants.first().copied().map_or(0, i64::from),
            max: variants.last().copied().map_or(0, i64::from),
            variants: variants
                .iter()
                .map(|id| InputVariant {
                    id: *id,
                    code: i64::from(*id),
                })
                .collect(),
        },
    }
}

fn scalar_domain(domain: &model::ScalarDomain) -> ScalarDomain {
    match domain {
        model::ScalarDomain::Bool => ScalarDomain::Bool,
        model::ScalarDomain::Int { min, max } => ScalarDomain::Int {
            min: *min,
            max: *max,
        },
    }
}

fn small(index: usize) -> Result<u16, ContractError> {
    u16::try_from(index)
        .map_err(|_| ContractError::new("v2/policy.json", "the decision program is too large"))
}

fn scalar_op(op: &ScalarOp) -> Result<Op, ContractError> {
    Ok(match *op {
        ScalarOp::Input(a) => Op::Input(small(a)?),
        ScalarOp::Int(value) => Op::Int(value),
        ScalarOp::Bool(value) => Op::Bool(value),
        ScalarOp::Add(a, b) => Op::Add(small(a)?, small(b)?),
        ScalarOp::Sub(a, b) => Op::Sub(small(a)?, small(b)?),
        ScalarOp::Eq(a, b) => Op::Eq(small(a)?, small(b)?),
        ScalarOp::Lt(a, b) => Op::Lt(small(a)?, small(b)?),
        ScalarOp::And(a, b) => Op::And(small(a)?, small(b)?),
        ScalarOp::Not(a) => Op::Not(small(a)?),
        ScalarOp::Select(condition, a, b) => Op::Select(small(condition)?, small(a)?, small(b)?),
    })
}

fn source(origin: Source) -> c::Source {
    match origin {
        Source::State => c::Source::State,
        Source::Command => c::Source::Command,
        Source::Context => c::Source::Context,
    }
}

fn class(value: Class) -> c::Class {
    match value {
        Class::Accept => c::Class::Accept,
        Class::Reject => c::Class::Reject,
        Class::CommittedFailure => c::Class::CommittedFailure,
    }
}

fn atom(value: &Atom) -> c::Atom<'_> {
    match value {
        Atom::Bool(value) => c::Atom::Bool(*value),
        Atom::I128(value) => c::Atom::I128(*value),
        Atom::U128(value) => c::Atom::U128(*value),
        Atom::Sum { type_id, variant } => c::Atom::Sum {
            type_id: *type_id,
            variant: *variant,
        },
        Atom::Text(value) => c::Atom::Text(value),
    }
}

fn domain(value: &model::Domain) -> c::Domain<'_> {
    match value {
        model::Domain::Bool => c::Domain::Bool,
        model::Domain::I128 { min, max } => c::Domain::I128 {
            min: *min,
            max: *max,
        },
        model::Domain::Text => c::Domain::Text,
        model::Domain::Sum { type_id, variants } => c::Domain::Sum {
            type_id: *type_id,
            variants,
        },
    }
}

fn expr(value: &Expr) -> c::Expr<'_> {
    match value {
        Expr::Constant(value) => c::Expr::Constant(atom(value)),
        Expr::Root(origin) => c::Expr::Root(source(*origin)),
        Expr::Input(origin, field) => c::Expr::Input(source(*origin), *field),
        Expr::Output(index) => c::Expr::Output(*index),
    }
}

fn observation(value: Observation) -> l::Observation {
    match value {
        Observation::PreRoot => l::Observation::PreRoot,
        Observation::CommandRoot => l::Observation::CommandRoot,
        Observation::ContextRoot => l::Observation::ContextRoot,
        Observation::PostRoot => l::Observation::PostRoot,
        Observation::Pre(field) => l::Observation::Pre(field),
        Observation::Command(field) => l::Observation::Command(field),
        Observation::Context(field) => l::Observation::Context(field),
        Observation::Post(field) => l::Observation::Post(field),
        Observation::Initial(field) => l::Observation::Initial(field),
        Observation::Class => l::Observation::Class,
        Observation::HasReason => l::Observation::HasReason,
        Observation::Reason => l::Observation::Reason,
        Observation::PostLength => l::Observation::PostLength,
        Observation::PatchLength => l::Observation::PatchLength,
        Observation::EffectLength => l::Observation::EffectLength,
        Observation::OutboxLength => l::Observation::OutboxLength,
        Observation::OutboxOrdinal(index) => l::Observation::OutboxOrdinal(index),
        Observation::OutboxChannel(index) => l::Observation::OutboxChannel(index),
        Observation::OutboxDestination(index) => l::Observation::OutboxDestination(index),
        Observation::OutboxPayload(index, field) => l::Observation::OutboxPayload(index, field),
        Observation::OutboxIdempotency(index) => l::Observation::OutboxIdempotency(index),
    }
}

fn law_op(op: &LawOp) -> l::Op<'_> {
    match op {
        LawOp::Literal(value) => l::Op::Literal(atom(value)),
        LawOp::Observe(value) => l::Op::Observe(observation(*value)),
        LawOp::ObserveWhen(guard, value, default) => {
            l::Op::ObserveWhen(*guard, observation(*value), atom(default))
        }
        LawOp::Add(a, b) => l::Op::Add(*a, *b),
        LawOp::Sub(a, b) => l::Op::Sub(*a, *b),
        LawOp::Mul(a, b) => l::Op::Mul(*a, *b),
        LawOp::Div(rounding, a, b) => l::Op::Div(
            match rounding {
                Rounding::Floor => l::Division::Floor,
                Rounding::Ceil => l::Division::Ceil,
            },
            *a,
            *b,
        ),
        LawOp::ToI128(a) => l::Op::ToI128(*a),
        LawOp::Eq(a, b) => l::Op::Eq(*a, *b),
        LawOp::Lt(a, b) => l::Op::Lt(*a, *b),
        LawOp::And(a, b) => l::Op::And(*a, *b),
        LawOp::Not(a) => l::Op::Not(*a),
        LawOp::Select(condition, a, b) => l::Op::Select(*condition, *a, *b),
    }
}

fn law_kind(kind: LawKind) -> l::Kind {
    match kind {
        LawKind::StateInvariant => l::Kind::StateInvariant,
        LawKind::AssetConservation => l::Kind::AssetConservation,
        LawKind::MintBurnAuthorization => l::Kind::MintBurnAuthorization,
        LawKind::DebitCreditEffectEquality => l::Kind::DebitCreditEffectEquality,
        LawKind::FeeAndRounding => l::Kind::FeeAndRounding,
        LawKind::AuthoritySubjectRecipient => l::Kind::AuthoritySubjectRecipient,
        LawKind::RejectNoAuthority => l::Kind::RejectNoAuthority,
        LawKind::CommittedFailureEffects => l::Kind::CommittedFailureEffects,
        LawKind::DecisionConformance => l::Kind::DecisionConformance,
        LawKind::InitialCondition => l::Kind::InitialCondition,
    }
}

fn scope(value: zeno_fcis_spec::LawScope) -> l::Scope {
    use zeno_fcis_spec::LawScope;
    match value {
        LawScope::Always => l::Scope::Always,
        LawScope::Accept => l::Scope::Accept,
        LawScope::Reject => l::Scope::Reject,
        LawScope::CommittedFailure => l::Scope::CommittedFailure,
        LawScope::Committing => l::Scope::Committing,
    }
}
