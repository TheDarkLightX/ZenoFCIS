//! A bound contract's library descriptor as SMT terms: the program inputs as
//! the Authority admits them, the scalar decision program, the reification of
//! its outputs, the branch the decision output selects with its successor
//! assignments and deliveries, and law programs over a state.
//!
//! Each part follows the library route of one transition: ingress, program
//! execution, output reification, decision construction, delivery schema
//! validation, then law evaluation. A query's premise that a case "commits"
//! is every one of those checks except the laws and the shared meter, whose
//! generated limits never bind; a model the meter would refuse fails replay
//! through the Authority, so it cannot become a refutation.

use zeno_fcis_synthesis::finite::{V2InputLeaf as Leaf, v2_composition as c, v2_laws as l};

use super::super::declarations::Source;
use super::super::review::domain::Position;
use crate::symbolic::Unsupported;
use crate::symbolic::program::{self, Encoded, Program};
use crate::symbolic::smt::{Script, Sort, Term, Values};

/// A value typed as the library's atoms are: the Boolean, an integer, a sum
/// or enum variant, or bytes known when encoding.
#[derive(Clone, Debug)]
pub(super) enum Typed {
    Bool(Term),
    I128(Term),
    U128(Term),
    Sum { type_id: u32, variant: Term },
    Enum { type_id: u32, variant: Term },
    Text(Vec<u8>),
    Bytes(Vec<u8>),
}

impl Typed {
    fn constant(atom: c::Atom<'_>) -> Result<Self, Unsupported> {
        Ok(match atom {
            c::Atom::Bool(value) => Self::Bool(Term::Bool(value)),
            c::Atom::I128(value) => Self::I128(Term::Int(value)),
            c::Atom::U128(value) => Self::U128(Term::Int(i128::try_from(value).map_err(|_| {
                Unsupported::new(format!(
                    "the unsigned constant {value} exceeds the i128 range"
                ))
            })?)),
            c::Atom::Sum { type_id, variant } => Self::Sum {
                type_id,
                variant: Term::Int(i128::from(variant)),
            },
            c::Atom::Enum { type_id, variant } => Self::Enum {
                type_id,
                variant: Term::Int(i128::from(variant)),
            },
            c::Atom::Text(bytes) => Self::Text(bytes.to_vec()),
            c::Atom::Bytes(bytes) => Self::Bytes(bytes.to_vec()),
            other => {
                return Err(Unsupported::new(format!(
                    "the constant {other:?} has a type this encoding does not know"
                )));
            }
        })
    }

    /// The number decision examples write for the value: 0 or 1, the
    /// integer, or the variant ID. Bytes have none.
    pub(super) fn number(&self) -> Option<Term> {
        match self {
            Self::Bool(value) => Some(Term::ite(value.clone(), Term::Int(1), Term::Int(0))),
            Self::I128(value) | Self::U128(value) => Some(value.clone()),
            Self::Sum { variant, .. } | Self::Enum { variant, .. } => Some(variant.clone()),
            Self::Text(_) | Self::Bytes(_) => None,
        }
    }

    /// The library's atom equality: equal type and equal value, else false.
    fn equal(&self, other: &Self) -> Term {
        match (self, other) {
            (Self::Bool(a), Self::Bool(b))
            | (Self::I128(a), Self::I128(b))
            | (Self::U128(a), Self::U128(b)) => Term::eq(a.clone(), b.clone()),
            (
                Self::Sum {
                    type_id: s,
                    variant: a,
                },
                Self::Sum {
                    type_id: t,
                    variant: b,
                },
            )
            | (
                Self::Enum {
                    type_id: s,
                    variant: a,
                },
                Self::Enum {
                    type_id: t,
                    variant: b,
                },
            ) if s == t => Term::eq(a.clone(), b.clone()),
            (Self::Text(a), Self::Text(b)) | (Self::Bytes(a), Self::Bytes(b)) => Term::Bool(a == b),
            _ => Term::Bool(false),
        }
    }

    /// Whether a decision slot's domain admits the value, as decision
    /// construction checks it; `ascii` adds the delivery schema's check that
    /// text is ASCII.
    fn admitted(&self, domain: c::Domain<'_>, ascii: bool) -> Result<Term, Unsupported> {
        Ok(match (domain, self) {
            (c::Domain::Bool, Self::Bool(_)) | (c::Domain::Bytes, Self::Bytes(_)) => {
                Term::Bool(true)
            }
            (c::Domain::Text, Self::Text(bytes)) => Term::Bool(!ascii || bytes.is_ascii()),
            (c::Domain::I128 { min, max }, Self::I128(value)) => Term::within(value, min, max),
            (c::Domain::U128 { min, max }, Self::U128(value)) => {
                let bound = |bound: u128| {
                    i128::try_from(bound).map_err(|_| {
                        Unsupported::new(format!(
                            "the unsigned bound {bound} exceeds the i128 range"
                        ))
                    })
                };
                Term::within(value, bound(min)?, bound(max.min(i128::MAX as u128))?)
            }
            (
                c::Domain::Sum { type_id, variants },
                Self::Sum {
                    type_id: actual,
                    variant,
                },
            )
            | (
                c::Domain::Enum { type_id, variants },
                Self::Enum {
                    type_id: actual,
                    variant,
                },
            ) => {
                if type_id == *actual {
                    let ids: Vec<i128> = variants.iter().map(|id| i128::from(*id)).collect();
                    Term::member(variant, &ids)
                } else {
                    Term::Bool(false)
                }
            }
            (
                c::Domain::Bool
                | c::Domain::I128 { .. }
                | c::Domain::U128 { .. }
                | c::Domain::Sum { .. }
                | c::Domain::Enum { .. }
                | c::Domain::Bytes
                | c::Domain::Text,
                _,
            ) => Term::Bool(false),
            (other, _) => {
                return Err(Unsupported::new(format!(
                    "the domain {other:?} is one this encoding does not know"
                )));
            }
        })
    }
}

/// The admitted codes of an input leaf, and the typed atom ingress decodes
/// for the code `code`.
fn leaf(leaf: &Leaf, code: &Term) -> Result<(Values, Typed), Unsupported> {
    let variants =
        |type_id: u32, variants: &[zeno_fcis_synthesis::finite::V2InputVariant], sum: bool| {
            let codes: Vec<i128> = variants
                .iter()
                .map(|variant| i128::from(variant.code))
                .collect();
            // A variant's ID is the value its atom carries; its code is the
            // program's. A contract gives every variant its ID as its code.
            let mut id = Term::Int(0);
            for variant in variants.iter().rev() {
                id = Term::ite(
                    Term::eq(code.clone(), Term::Int(i128::from(variant.code))),
                    Term::Int(i128::from(variant.id)),
                    id,
                );
            }
            if variants
                .iter()
                .all(|variant| i64::from(variant.id) == variant.code)
            {
                id = code.clone();
            }
            let typed = if sum {
                Typed::Sum {
                    type_id,
                    variant: id,
                }
            } else {
                Typed::Enum {
                    type_id,
                    variant: id,
                }
            };
            (Values::Set(codes), typed)
        };
    Ok(match leaf {
        Leaf::Bool => (
            Values::Range { min: 0, max: 1 },
            Typed::Bool(Term::eq(code.clone(), Term::Int(1))),
        ),
        Leaf::I128 { min, max } => (
            Values::Range {
                min: i128::from(*min),
                max: i128::from(*max),
            },
            Typed::I128(code.clone()),
        ),
        Leaf::U128 { min, max } => {
            let bound = |bound: u128| {
                i128::try_from(bound)
                    .map_err(|_| Unsupported::new("an unsigned input bound exceeds the i128 range"))
            };
            (
                Values::Range {
                    min: bound(*min)?,
                    max: bound(*max)?,
                },
                Typed::U128(code.clone()),
            )
        }
        Leaf::Sum {
            type_id,
            variants: list,
            ..
        } => variants(*type_id, list, true),
        Leaf::Enum {
            type_id,
            variants: list,
            ..
        } => variants(*type_id, list, false),
        other => {
            return Err(Unsupported::new(format!(
                "the input leaf {other:?} is one this encoding does not know"
            )));
        }
    })
}

/// The input leaf a binding reads, from the descriptor's schemas.
fn bound_leaf<'d>(descriptor: &'d c::Descriptor<'d>, binding: &c::Binding) -> Option<&'d Leaf> {
    let schema = match binding.source {
        c::Source::State => descriptor.state,
        c::Source::Command => descriptor.command,
        c::Source::Context => descriptor.context,
        _ => return None,
    };
    match (schema, binding.selector) {
        (c::Schema::Leaf(leaf), c::Selector::Root) => Some(leaf),
        (c::Schema::Record(fields), c::Selector::Field(id)) => fields
            .iter()
            .find(|field| field.id == id)
            .map(|field| &field.leaf),
        _ => None,
    }
}

/// The program inputs and the program's terms in one query.
#[derive(Clone, Debug)]
pub(super) struct Decision {
    /// Each input's atom, in program input order.
    pub(super) inputs: Vec<Typed>,
    /// Where each input comes from: its source and field.
    sources: Vec<(Source, Option<u16>)>,
    program: Encoded,
    /// True when ingress admits every input code for the program.
    inputs_admitted: Term,
    /// Each output's atom.
    outputs: Vec<Typed>,
    /// True when every output reifies by its output type.
    reified: Term,
    /// The decision output's code, when the output type is a signed integer.
    code: Option<Term>,
}

impl Decision {
    /// Declares one constant `x{i}` per input position and encodes the
    /// descriptor's program over them.
    ///
    /// # Errors
    /// A binding that differs from the review's input positions, or a type,
    /// leaf or instruction the encoding does not know.
    pub(super) fn encode(
        script: &mut Script,
        descriptor: &c::Descriptor<'_>,
        positions: &[Position],
    ) -> Result<Self, Unsupported> {
        if descriptor.bindings.len() != positions.len() {
            return Err(Unsupported::new(
                "the descriptor binds a different number of inputs than the schema declares",
            ));
        }
        let mut inputs = Vec::with_capacity(positions.len());
        let mut sources = Vec::with_capacity(positions.len());
        let mut admitted = Vec::new();
        for (index, (binding, position)) in descriptor.bindings.iter().zip(positions).enumerate() {
            let source = match binding.source {
                c::Source::State => Source::State,
                c::Source::Command => Source::Command,
                c::Source::Context => Source::Context,
                _ => return Err(Unsupported::new("a binding reads an unknown source")),
            };
            let field = match binding.selector {
                c::Selector::Root => None,
                c::Selector::Field(id) => Some(id),
                _ => return Err(Unsupported::new("a binding has an unknown selector")),
            };
            if (source, field) != (position.source, position.field) {
                return Err(Unsupported::new(format!(
                    "input {index} is bound to another value than `{}`",
                    position.name
                )));
            }
            let leaf_type = bound_leaf(descriptor, binding)
                .ok_or_else(|| Unsupported::new(format!("input {index} reads no declared leaf")))?;
            let code = Term::name(&format!("x{index}"));
            let (values, typed) = leaf(leaf_type, &code)?;
            script.constant(&format!("x{index}"), values);
            if let Some(domain) = descriptor.program.inputs.get(index) {
                let (min, max) = domain.bounds();
                admitted.push(Term::within(&code, i128::from(min), i128::from(max)));
            }
            inputs.push(typed);
            sources.push((source, field));
        }
        let program = program::encode(
            script,
            "p",
            Program {
                inputs: descriptor.program.inputs,
                outputs: descriptor.program.outputs,
                nodes: descriptor.program.nodes,
                roots: descriptor.program.roots,
            },
            &|index| Term::name(&format!("x{index}")),
        )?;
        if descriptor.output_types.len() != program.outputs.len() {
            return Err(Unsupported::new(
                "the program's outputs and output types differ in number",
            ));
        }
        let mut outputs = Vec::new();
        let mut reified = Vec::new();
        for (index, (output_type, wire)) in descriptor
            .output_types
            .iter()
            .zip(&program.outputs)
            .enumerate()
        {
            let wire = script.define(&format!("out{index}"), Sort::Int, wire.clone());
            let (values, typed) = leaf(output_type, &wire)?;
            reified.push(match values {
                Values::Range { min, max } => Term::within(&wire, min, max),
                Values::Set(codes) => Term::member(&wire, &codes),
            });
            outputs.push(typed);
        }
        let code = match outputs.get(descriptor.decision_output) {
            Some(Typed::I128(term)) => Some(term.clone()),
            _ => None,
        };
        Ok(Self {
            inputs,
            sources,
            program,
            inputs_admitted: Term::and(admitted),
            outputs,
            reified: Term::and(reified),
            code,
        })
    }

    /// The pre-state's field values.
    pub(super) fn pre_state(&self, field: u16) -> Option<Typed> {
        self.sources
            .iter()
            .position(|source| *source == (Source::State, Some(field)))
            .map(|index| self.inputs[index].clone())
    }

    /// The library's `resolve`: `None` where it finds no value.
    fn resolve(&self, expr: c::Expr<'_>) -> Result<Option<Typed>, Unsupported> {
        let input = |source: Source, field: Option<u16>| {
            self.sources
                .iter()
                .position(|bound| *bound == (source, field))
                .map(|index| self.inputs[index].clone())
        };
        Ok(match expr {
            c::Expr::Input(c::Source::State, id) => input(Source::State, Some(id)),
            c::Expr::Input(c::Source::Command, id) => input(Source::Command, Some(id)),
            c::Expr::Input(c::Source::Context, id) => input(Source::Context, Some(id)),
            c::Expr::Root(c::Source::State) => None,
            c::Expr::Root(c::Source::Command) => input(Source::Command, None),
            c::Expr::Root(c::Source::Context) => input(Source::Context, None),
            c::Expr::Output(index) => self.outputs.get(index).cloned(),
            c::Expr::Constant(atom) => Some(Typed::constant(atom)?),
            other => {
                return Err(Unsupported::new(format!(
                    "the expression {other:?} is one this encoding does not know"
                )));
            }
        })
    }
}

/// One committing branch's terms in one query.
#[derive(Clone, Debug)]
pub(super) struct Branch {
    /// True when the Authority constructs and validates this branch's
    /// decision: everything before law evaluation succeeds.
    pub(super) commits: Term,
    /// Each successor field's value, in record order.
    pub(super) post: Vec<(u16, Typed)>,
}

impl Branch {
    /// The terms of branch `index`.
    ///
    /// # Errors
    /// A construct the encoding does not know.
    pub(super) fn encode(
        descriptor: &c::Descriptor<'_>,
        decision: &Decision,
        index: usize,
    ) -> Result<Self, Unsupported> {
        let branches = descriptor.branches;
        if branches.windows(2).any(|pair| pair[0].code >= pair[1].code) {
            return Err(Unsupported::new(
                "the decision table's codes do not increase, so the library refuses every decision",
            ));
        }
        let branch = branches
            .get(index)
            .ok_or_else(|| Unsupported::new(format!("the decision table has no branch {index}")))?;
        let never = |post| {
            Ok(Self {
                commits: Term::Bool(false),
                post,
            })
        };
        let committing = match branch.class {
            c::Class::Accept => branch.reason.is_none(),
            c::Class::CommittedFailure => branch.reason.is_some_and(|reason| reason != 0),
            _ => false,
        };
        let Some(code) = &decision.code else {
            return never(Vec::new());
        };
        if !committing {
            return never(Vec::new());
        }
        let state_fields: Vec<u16> = match descriptor.state {
            c::Schema::Record(fields) => fields.iter().map(|field| field.id).collect(),
            _ => return never(Vec::new()),
        };
        let fields: Vec<u16> = branch.assignments.iter().map(|plan| plan.field).collect();
        if fields != state_fields {
            return never(Vec::new());
        }
        let mut checks = vec![
            decision.inputs_admitted.clone(),
            Term::not(decision.program.trap.clone()),
            decision.program.admitted.clone(),
            decision.reified.clone(),
            Term::eq(code.clone(), Term::Int(branch.code)),
        ];
        let mut post = Vec::new();
        for plan in branch.assignments {
            let Some(value) = decision.resolve(plan.value)? else {
                return never(post);
            };
            let Some(before) = decision.pre_state(plan.field) else {
                return Err(Unsupported::new(format!(
                    "state field {} is not a program input",
                    plan.field
                )));
            };
            checks.push(before.admitted(plan.domain, false)?);
            checks.push(value.admitted(plan.domain, false)?);
            post.push((plan.field, value));
        }
        for lane in [branch.effects, branch.outbox] {
            if lane.iter().enumerate().any(|(number, plan)| {
                plan.channel == 0 || (number > 0 && lane[number - 1].ordinal >= plan.ordinal)
            }) {
                return never(post);
            }
            for plan in lane {
                checks.push(delivery(descriptor, decision, plan)?);
            }
        }
        Ok(Self {
            commits: Term::and(checks),
            post,
        })
    }
}

/// True when one planned delivery neither refuses decision construction nor
/// fails the delivery schema: its condition is a Boolean, and when it holds,
/// every value resolves and the channel's schema admits it.
fn delivery(
    descriptor: &c::Descriptor<'_>,
    decision: &Decision,
    plan: &c::DeliveryPlan<'_>,
) -> Result<Term, Unsupported> {
    let Some(Typed::Bool(enabled)) = decision.resolve(plan.when)? else {
        return Ok(Term::Bool(false));
    };
    let mut emitted = Vec::new();
    let channel = descriptor
        .channels
        .iter()
        .find(|channel| channel.id == plan.channel);
    let destination = decision.resolve(plan.destination)?;
    let idempotency = decision.resolve(plan.idempotency)?;
    let ordered = plan
        .payload
        .windows(2)
        .all(|pair| pair[0].field < pair[1].field);
    match (channel, destination, idempotency, ordered) {
        (Some(channel), Some(destination), Some(idempotency), true)
            if channel.payload.len() == plan.payload.len() =>
        {
            emitted.push(destination.admitted(channel.destination, true)?);
            emitted.push(idempotency.admitted(channel.idempotency, true)?);
            for (field, schema) in plan.payload.iter().zip(channel.payload) {
                if field.field != schema.field {
                    emitted.push(Term::Bool(false));
                    continue;
                }
                match decision.resolve(field.value)? {
                    Some(value) => emitted.push(value.admitted(schema.domain, true)?),
                    None => emitted.push(Term::Bool(false)),
                }
            }
        }
        _ => emitted.push(Term::Bool(false)),
    }
    Ok(Term::or([Term::not(enabled), Term::and(emitted)]))
}

/// Defines one law program over a state and returns its satisfaction: every
/// node defined, as the eager law evaluator requires, and the root true.
///
/// # Errors
/// An instruction, observation or operand type the encoding does not know.
/// Compiled contract laws never use an operand of the wrong type, so such a
/// program is refused rather than encoded as undefined.
pub(super) fn law(
    script: &mut Script,
    prefix: &str,
    nodes: &[l::Op<'_>],
    root: usize,
    state: &dyn Fn(u16) -> Option<Typed>,
) -> Result<Term, Unsupported> {
    let mut values: Vec<Typed> = Vec::with_capacity(nodes.len());
    let mut defined = Vec::new();
    let checked = |term: &Term| Term::within(term, i128::MIN, i128::MAX);
    for (index, op) in nodes.iter().enumerate() {
        let name = format!("{prefix}{index}");
        let operand = |id: usize| {
            values.get(id).cloned().ok_or_else(|| {
                Unsupported::new(format!("law node {index} reads node {id}, which is later"))
            })
        };
        let ill_typed =
            || Unsupported::new(format!("law node {index} has operands of other types"));
        let integer = |term: Term, script: &mut Script| {
            let term = script.define(&name, Sort::Int, term);
            Typed::I128(term)
        };
        let value = match *op {
            l::Op::Literal(atom) => Typed::constant(atom)?,
            l::Op::Observe(l::Observation::Post(field) | l::Observation::Initial(field)) => {
                state(field).ok_or_else(|| {
                    Unsupported::new(format!(
                        "law node {index} reads field {field}, which the state lacks"
                    ))
                })?
            }
            l::Op::Observe(observation) => {
                return Err(Unsupported::new(format!(
                    "law node {index} reads {observation:?}, which is not a state field"
                )));
            }
            l::Op::Add(a, b) | l::Op::Sub(a, b) | l::Op::Mul(a, b) => {
                let (Typed::I128(a), Typed::I128(b)) = (operand(a)?, operand(b)?) else {
                    return Err(ill_typed());
                };
                let term = match op {
                    l::Op::Add(..) => Term::add(a, b),
                    l::Op::Sub(..) => Term::sub(a, b),
                    _ => Term::mul(a, b),
                };
                let value = integer(term, script);
                if let Typed::I128(term) = &value {
                    defined.push(checked(term));
                }
                value
            }
            l::Op::Div(mode, a, b) => {
                let (Typed::I128(a), Typed::I128(b)) = (operand(a)?, operand(b)?) else {
                    return Err(ill_typed());
                };
                let term = match mode {
                    l::Division::Floor => Term::floor_div(a, b.clone()),
                    l::Division::Ceil => Term::ceil_div(a, b.clone()),
                    _ => {
                        return Err(Unsupported::new(format!(
                            "law node {index} divides with {mode:?}, which this encoding does not know"
                        )));
                    }
                };
                let value = integer(term, script);
                if let Typed::I128(term) = &value {
                    defined.push(Term::not(Term::eq(b, Term::Int(0))));
                    defined.push(checked(term));
                }
                value
            }
            l::Op::ToI128(a) => match operand(a)? {
                Typed::I128(term) => Typed::I128(term),
                Typed::Bool(term) => integer(Term::ite(term, Term::Int(1), Term::Int(0)), script),
                Typed::Sum { variant, .. } | Typed::Enum { variant, .. } => Typed::I128(variant),
                Typed::U128(term) => {
                    defined.push(Term::le(term.clone(), Term::Int(i128::MAX)));
                    Typed::I128(term)
                }
                Typed::Text(_) | Typed::Bytes(_) => return Err(ill_typed()),
            },
            l::Op::Eq(a, b) => {
                let term = operand(a)?.equal(&operand(b)?);
                Typed::Bool(script.define(&name, Sort::Bool, term))
            }
            l::Op::Lt(a, b) => match (operand(a)?, operand(b)?) {
                (Typed::I128(a), Typed::I128(b)) | (Typed::U128(a), Typed::U128(b)) => {
                    Typed::Bool(script.define(&name, Sort::Bool, Term::lt(a, b)))
                }
                _ => return Err(ill_typed()),
            },
            l::Op::And(a, b) => match (operand(a)?, operand(b)?) {
                (Typed::Bool(a), Typed::Bool(b)) => {
                    Typed::Bool(script.define(&name, Sort::Bool, Term::and([a, b])))
                }
                _ => return Err(ill_typed()),
            },
            l::Op::Not(a) => match operand(a)? {
                Typed::Bool(a) => Typed::Bool(script.define(&name, Sort::Bool, Term::not(a))),
                _ => return Err(ill_typed()),
            },
            l::Op::Select(condition, a, b) => {
                let Typed::Bool(condition) = operand(condition)? else {
                    return Err(ill_typed());
                };
                match (operand(a)?, operand(b)?) {
                    (Typed::Bool(a), Typed::Bool(b)) => {
                        Typed::Bool(script.define(&name, Sort::Bool, Term::ite(condition, a, b)))
                    }
                    (Typed::I128(a), Typed::I128(b)) => integer(Term::ite(condition, a, b), script),
                    _ => return Err(ill_typed()),
                }
            }
            _ => {
                return Err(Unsupported::new(format!(
                    "law node {index} is an instruction this encoding does not know: {op:?}"
                )));
            }
        };
        values.push(value);
    }
    match values.get(root) {
        Some(Typed::Bool(root)) => Ok(Term::and(defined.into_iter().chain([root.clone()]))),
        _ => Err(Unsupported::new("the law's root is not a Boolean")),
    }
}
