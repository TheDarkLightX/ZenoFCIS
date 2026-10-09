//! The complete contract as data, built once and then rendered as Rust source
//! and encoded as library policy bytes, so both artifacts describe one value.
//!
//! Construction order fixes output and node numbering: the case-selection
//! output first, then each case's state assignments and deliveries, then the
//! scalar program; the scoped `.zeno` laws, the framework laws, genesis law
//! 990 and decision-conformance law 991.

use zeno_fcis_spec::LawScope;
use zeno_fcis_synthesis::finite::{Op as LibraryOp, Program};

use super::ContractError;
use super::declarations::{Declarations, Form, Kind as TypeKind, Leaf, ROOTS, Source};
use super::expr::{self, Ast};
use super::graph::{
    Atom, Graph, Kind, LawGraph, LawOp, Observation, Op, Ref, ScalarGraph, ScalarOp,
};
use super::policy::{scalar_domain as library_domain, scalar_op as library_op, small};
use super::rules::{Class, Constant, LawKind, Rules};

/// Limits the library's scalar program profile accepts.
const MAX_OUTPUTS: usize = 16;
const MAX_NODES: usize = 256;
const MAX_INPUTS: usize = 32;
/// Law 990 checks the genesis state; law 991 checks every decision against
/// the case table.
const GENESIS_LAW: u32 = 990;
const CONFORMANCE_LAW: u32 = 991;
/// The structural reject law a template keeps beside its own `RejectNoAuthority` law.
const REJECT_STRUCTURE_LAW: u32 = 909;
/// Wire overhead of one framed envelope and per-value sizes, in bytes.
const ENVELOPE_BYTES: u64 = 48;
const RECORD_BYTES: u64 = 5;
const FIELD_BYTES: u64 = 2;
const BOOL_BYTES: u64 = 1;
const I128_BYTES: u64 = 17;
const SUM_BYTES: u64 = 8;
/// Reads allowed beyond three per input and one per law observation.
const READ_ALLOWANCE: u64 = 64;
/// Steps allowed beyond one per program and law node.
const STEP_ALLOWANCE: u64 = 5;

#[derive(Clone, Copy, Debug)]
pub(super) enum ScalarDomain {
    Bool,
    Int { min: i64, max: i64 },
}

/// A typed inverse map from a scalar to an original value.
#[derive(Clone, Debug)]
pub(super) enum InputLeaf {
    Bool,
    I128 { min: i64, max: i64 },
    Sum { type_id: u32, variants: Vec<u16> },
}

/// A declared value domain.
#[derive(Clone, Debug)]
pub(super) enum Domain {
    Bool,
    I128 { min: i128, max: i128 },
    Text,
    Sum { type_id: u32, variants: Vec<u16> },
}

/// Where a successor or payload value comes from.
#[derive(Clone, Debug)]
pub(super) enum Expr {
    Constant(Atom),
    Root(Source),
    Input(Source, u16),
    Output(usize),
}

#[derive(Debug)]
pub(super) struct Assignment {
    pub(super) field: u16,
    pub(super) value: Expr,
    pub(super) domain: Domain,
}

#[derive(Debug)]
pub(super) struct Plan {
    pub(super) ordinal: u32,
    pub(super) channel: u32,
    pub(super) destination: Atom,
    pub(super) payload: Vec<(u16, Expr)>,
    pub(super) idempotency: u128,
}

#[derive(Debug)]
pub(super) struct Branch {
    pub(super) class: Class,
    pub(super) reason: Option<u32>,
    pub(super) assignments: Vec<Assignment>,
    pub(super) outbox: Vec<Plan>,
}

#[derive(Debug)]
pub(super) struct ChannelSchema {
    pub(super) id: u32,
    pub(super) destination_type: u32,
    pub(super) payload_type: u32,
    pub(super) destination: Domain,
    pub(super) payload: Vec<(u16, Domain)>,
    /// The largest idempotency ordinal any case delivers on the channel, 0
    /// when none does: the channel's idempotency domain is `0..=idempotency`.
    pub(super) idempotency: u128,
}

#[derive(Debug)]
pub(super) struct Law {
    pub(super) id: u32,
    pub(super) kind: LawKind,
    pub(super) scope: LawScope,
    pub(super) genesis: bool,
    pub(super) nodes: Vec<LawOp>,
    pub(super) root: usize,
}

/// A root's input schema: its fields' leaves, or one leaf.
#[derive(Debug)]
pub(super) enum RootSchema {
    Record(Vec<(u16, InputLeaf)>),
    Leaf(InputLeaf),
}

#[derive(Clone, Copy, Debug)]
pub(super) struct Budgets {
    pub(super) read: u64,
    pub(super) write: u64,
    pub(super) byte: u64,
    pub(super) step: u64,
    /// Each delivery charges one Effect: the most deliveries of any case, at
    /// least 1.
    pub(super) effect: u64,
}

#[derive(Debug)]
pub(super) struct Contract<'d> {
    pub(super) declarations: &'d Declarations,
    pub(super) commitment: [u8; 32],
    /// Maximum framed state, command and context bytes.
    pub(super) frame_bytes: [u64; 3],
    pub(super) channels: Vec<ChannelSchema>,
    pub(super) branches: Vec<Branch>,
    pub(super) laws: Vec<Law>,
    pub(super) roots: [RootSchema; 3],
    pub(super) output_types: Vec<InputLeaf>,
    pub(super) input_domains: Vec<ScalarDomain>,
    pub(super) output_domains: Vec<ScalarDomain>,
    pub(super) nodes: Vec<ScalarOp>,
    pub(super) program_roots: Vec<usize>,
    pub(super) bindings: Vec<(Source, Option<u16>)>,
    pub(super) reasons: Vec<(u32, Class)>,
    pub(super) budgets: Budgets,
    /// Every state field's genesis value, as law 990 requires it.
    pub(super) genesis: Vec<(u16, Atom)>,
}

impl<'d> Contract<'d> {
    pub(super) fn build(
        declarations: &'d Declarations,
        rules: &Rules,
        commitment: [u8; 32],
    ) -> Result<Self, ContractError> {
        let inputs = declarations.inputs()?;
        if inputs.len() > MAX_INPUTS {
            return Err(rules_error(
                "",
                format!("{} inputs exceed {MAX_INPUTS}", inputs.len()),
            ));
        }
        let state_fields: Vec<(u16, u32)> = declarations
            .state_fields()?
            .iter()
            .map(|field| (field.id, field.type_id))
            .collect();
        if state_fields.is_empty() {
            return Err(ContractError::new(
                "project.zeno",
                "the state root, type 100, must be a record",
            ));
        }
        let fields: Vec<u16> = state_fields.iter().map(|(id, _)| *id).collect();
        if rules.genesis.keys().copied().collect::<Vec<_>>() != fields {
            return Err(rules_error("genesis", "must set every state field"));
        }
        let genesis = state_fields
            .iter()
            .map(|(field, type_id)| {
                constant_atom(declarations, rules.genesis[field], *type_id)
                    .map(|value| (*field, value))
                    .map_err(|reason| rules_error(&format!("genesis.{field}"), reason))
            })
            .collect::<Result<Vec<_>, _>>()?;
        let variables = &rules.variables;
        let expand = |ast: &Ast, place: &str| {
            expr::expand(ast, variables).map_err(|reason| rules_error(place, reason))
        };
        let last = rules.cases.len() - 1;
        // Written `true`, not merely a variable equal to it.
        if rules.cases[last].when != Ast::Bool(true) {
            return Err(rules_error(
                &format!("cases[{last}].when"),
                "the last case must be `true`",
            ));
        }

        // Output 0 selects the case: the first whose `when` holds.
        let mut outputs = Outputs::default();
        let mut selection = Ast::Int(index_value(last));
        for (index, case) in rules.cases.iter().enumerate().take(last).rev() {
            let place = format!("cases[{index}].when");
            check_program(declarations, &inputs, &expand(&case.when, &place)?, &place)?;
            selection = Ast::Choose(
                Box::new(case.when.clone()),
                Box::new(Ast::Int(index_value(index))),
                Box::new(selection),
            );
        }
        outputs.selection(expand(&selection, "cases")?, last);

        let mut branches = Vec::new();
        for (index, case) in rules.cases.iter().enumerate() {
            let place = format!("cases[{index}]");
            let committing = case.class != Class::Reject;
            let keys: Vec<u16> = case.post.keys().copied().collect();
            if committing && keys != fields {
                return Err(rules_error(
                    &format!("{place}.post"),
                    "must set every state field",
                ));
            }
            if !committing && (!keys.is_empty() || !case.outbox.is_empty()) {
                return Err(rules_error(
                    &place,
                    "a reject sets no state and delivers nothing",
                ));
            }
            match (case.class, case.reason) {
                (Class::Accept, Some(_)) => {
                    return Err(rules_error(
                        &format!("{place}.reason"),
                        "an accept has no reason",
                    ));
                }
                (Class::Reject | Class::CommittedFailure, None) => {
                    return Err(rules_error(
                        &format!("{place}.reason"),
                        "a reject or committed failure needs a reason",
                    ));
                }
                _ => {}
            }
            let mut assignments = Vec::new();
            for (field, type_id) in &state_fields {
                if let Some(value) = case.post.get(field) {
                    let place = format!("{place}.post.{field}");
                    assignments.push(Assignment {
                        field: *field,
                        value: outputs.expression(
                            declarations,
                            &inputs,
                            &expand(value, &place)?,
                            *type_id,
                            &place,
                        )?,
                        domain: domain(declarations, *type_id)?,
                    });
                }
            }
            let mut outbox: Vec<Plan> = Vec::new();
            for (number, delivery) in case.outbox.iter().enumerate() {
                let place = format!("{place}.outbox[{number}]");
                if let Some(previous) = outbox.last().map(|plan| plan.ordinal)
                    && delivery.ordinal <= previous
                {
                    return Err(rules_error(
                        &format!("{place}.ordinal"),
                        format!(
                            "must exceed the previous delivery's ordinal {previous}: a case's \
                             deliveries are in increasing ordinal order"
                        ),
                    ));
                }
                let channel = declarations.channel(delivery.channel)?;
                let Form::Leaf(Leaf::Text { min, max }) =
                    declarations.get(channel.destination)?.form
                else {
                    return Err(rules_error(&place, "destinations must be text"));
                };
                let length = u32::try_from(delivery.destination.len()).unwrap_or(u32::MAX);
                if !delivery.destination.is_ascii() || length < min || length > max {
                    return Err(rules_error(
                        &format!("{place}.destination"),
                        format!("must be ASCII text of {min} to {max} bytes"),
                    ));
                }
                let payload_fields = declarations.fields(channel.payload)?;
                let wanted: Vec<u16> = payload_fields.iter().map(|field| field.id).collect();
                if delivery.payload.keys().copied().collect::<Vec<_>>() != wanted {
                    return Err(rules_error(
                        &format!("{place}.payload"),
                        "must set every payload field",
                    ));
                }
                let mut payload = Vec::new();
                for field in payload_fields {
                    let place = format!("{place}.payload.{}", field.id);
                    let value = expand(&delivery.payload[&field.id], &place)?;
                    payload.push((
                        field.id,
                        outputs.expression(declarations, &inputs, &value, field.type_id, &place)?,
                    ));
                }
                outbox.push(Plan {
                    ordinal: delivery.ordinal,
                    channel: delivery.channel,
                    destination: Atom::Text(delivery.destination.as_bytes().to_vec()),
                    payload,
                    idempotency: delivery.idempotency,
                });
            }
            branches.push(Branch {
                class: case.class,
                reason: case.reason,
                assignments,
                outbox,
            });
        }

        let mut program = ScalarGraph::new(declarations, &inputs);
        let mut program_roots = Vec::new();
        for (ast, _) in &outputs.entries {
            program_roots.push(
                program
                    .compile(ast)
                    .map_err(|reason| rules_error("", reason))?
                    .index,
            );
        }
        if outputs.entries.len() > MAX_OUTPUTS || program.table.nodes.len() > MAX_NODES {
            return Err(rules_error(
                "",
                format!(
                    "{} outputs and {} program nodes exceed {MAX_OUTPUTS} and {MAX_NODES}",
                    outputs.entries.len(),
                    program.table.nodes.len()
                ),
            ));
        }

        let laws = laws(declarations, rules, &state_fields, &genesis)?;
        let reasons = reasons(declarations, rules)?;
        let channels = declarations
            .channels
            .iter()
            .map(|channel| {
                Ok(ChannelSchema {
                    id: channel.id,
                    destination_type: channel.destination,
                    payload_type: channel.payload,
                    destination: domain(declarations, channel.destination)?,
                    payload: declarations
                        .fields(channel.payload)?
                        .iter()
                        .map(|field| Ok((field.id, domain(declarations, field.type_id)?)))
                        .collect::<Result<_, ContractError>>()?,
                    idempotency: branches
                        .iter()
                        .flat_map(|branch| &branch.outbox)
                        .filter(|plan| plan.channel == channel.id)
                        .map(|plan| plan.idempotency)
                        .max()
                        .unwrap_or(0),
                })
            })
            .collect::<Result<Vec<_>, ContractError>>()?;
        let roots = [0, 1, 2].map(|index| root_schema(declarations, ROOTS[index].1));
        let [state, command, context] = roots;
        let roots = [state?, command?, context?];
        let frame_bytes = [
            ENVELOPE_BYTES + value_bytes(declarations, ROOTS[0].1)?,
            ENVELOPE_BYTES + value_bytes(declarations, ROOTS[1].1)?,
            ENVELOPE_BYTES + value_bytes(declarations, ROOTS[2].1)?,
        ];
        let observations = laws
            .iter()
            .flat_map(|law| &law.nodes)
            .filter(|node| matches!(node, LawOp::Observe(_) | LawOp::ObserveWhen(..)))
            .count();
        let budgets = Budgets {
            read: 3 * count(inputs.len()) + count(observations) + READ_ALLOWANCE,
            write: count(state_fields.len()),
            byte: frame_bytes.iter().sum(),
            step: step_budget(program.table.nodes.len(), &laws),
            effect: branches
                .iter()
                .map(|branch| count(branch.outbox.len()))
                .max()
                .unwrap_or(0)
                .max(1),
        };
        Ok(Self {
            declarations,
            commitment,
            frame_bytes,
            channels,
            branches,
            laws,
            roots,
            output_types: outputs.types,
            input_domains: inputs
                .iter()
                .map(|input| scalar_domain(declarations, input.type_id, false))
                .collect::<Result<_, _>>()?,
            output_domains: outputs.domains,
            nodes: program.table.nodes,
            program_roots,
            bindings: inputs
                .iter()
                .map(|input| (input.source, input.field))
                .collect(),
            reasons,
            budgets,
            genesis,
        })
    }

    /// The decision program as the library's admitted program, whose
    /// canonical bytes a transform receipt names.
    pub(super) fn program(&self) -> Result<Program, ContractError> {
        let nodes = self
            .nodes
            .iter()
            .map(library_op)
            .collect::<Result<Vec<_>, _>>()?;
        let roots = self
            .program_roots
            .iter()
            .map(|root| small(*root))
            .collect::<Result<Vec<_>, _>>()?;
        Program::try_new(
            self.input_domains.iter().map(library_domain).collect(),
            self.output_domains.iter().map(library_domain).collect(),
            nodes,
            roots,
        )
        .map_err(|error| {
            rules_error(
                "",
                format!("the library refuses the decision program: {error}"),
            )
        })
    }

    /// Replaces the decision program with an adopted candidate. The caller
    /// has replayed the receipt that compares it with the current program, so
    /// its input and output ABI is the current one; the Step budget follows
    /// its node count as it follows the rules-compiled program's.
    pub(super) fn adopt(&mut self, candidate: &Program, place: &str) -> Result<(), ContractError> {
        let current = self.program()?;
        if candidate.inputs() != current.inputs() || candidate.outputs() != current.outputs() {
            return Err(rules_error(
                place,
                "the candidate's input or output domains differ from the contract's",
            ));
        }
        let nodes = candidate
            .nodes()
            .iter()
            .map(|op| contract_op(op, place))
            .collect::<Result<Vec<_>, _>>()?;
        self.nodes = nodes;
        self.program_roots = candidate
            .roots()
            .iter()
            .map(|root| usize::from(*root))
            .collect();
        self.budgets.step = step_budget(self.nodes.len(), &self.laws);
        Ok(())
    }

    /// The Steps every law together can charge in one evaluation: one per
    /// law node.
    pub(super) fn law_steps(&self) -> u64 {
        count(self.laws.iter().map(|law| law.nodes.len()).sum())
    }

    /// Whether the laws include decision-conformance law 991, which pins
    /// every decision to the case table, on every decision.
    pub(super) fn pins_decisions(&self) -> bool {
        self.laws.iter().any(|law| {
            law.id == CONFORMANCE_LAW
                && law.kind == LawKind::DecisionConformance
                && law.scope == LawScope::Always
        })
    }

    /// Declared types, the most fields of one type and the most variants of
    /// one type: the schema admission limits.
    pub(super) fn schema_counts(&self) -> (usize, usize, usize) {
        let types = &self.declarations.types;
        let most = |count: fn(&Form) -> usize| {
            types
                .values()
                .map(|declared| count(&declared.form))
                .max()
                .unwrap_or(0)
        };
        (
            types.len(),
            most(|form| match form {
                Form::Record(fields) => fields.len(),
                _ => 0,
            }),
            most(|form| match form {
                Form::Sum(variants) => variants.len(),
                _ => 0,
            }),
        )
    }
}

/// Distinct program outputs: an expanded expression and the type it fills.
#[derive(Default)]
struct Outputs {
    entries: Vec<(Ast, Option<u32>)>,
    types: Vec<InputLeaf>,
    domains: Vec<ScalarDomain>,
}

impl Outputs {
    /// The case index, from 0 to `last`.
    fn selection(&mut self, ast: Ast, last: usize) {
        let last = i64::try_from(last).unwrap_or(i64::MAX);
        self.entries.push((ast, None));
        self.types.push(InputLeaf::I128 { min: 0, max: last });
        self.domains.push(ScalarDomain::Int { min: 0, max: last });
    }

    /// A successor or payload value: a constant, an input read directly, or a
    /// program output, shared with any equal output for the same type.
    fn expression(
        &mut self,
        declarations: &Declarations,
        inputs: &[super::declarations::Input],
        ast: &Ast,
        type_id: u32,
        place: &str,
    ) -> Result<Expr, ContractError> {
        match ast {
            Ast::Bool(_) | Ast::Int(_) => {
                let value = atom(declarations, ast, type_id)
                    .map_err(|reason| rules_error(place, reason))?;
                if let (Atom::I128(value), TypeKind::I128 { min, max }) =
                    (&value, declarations.kind(type_id)?)
                    && !(min..=max).contains(value)
                {
                    return Err(rules_error(
                        place,
                        format!("{value} is outside {min}..={max}"),
                    ));
                }
                Ok(Expr::Constant(value))
            }
            Ast::Name(name) => {
                let input = inputs
                    .iter()
                    .find(|input| input.name == *name)
                    .ok_or_else(|| {
                        rules_error(place, format!("`{name}` is not a variable or an input"))
                    })?;
                if !same_kind(declarations, input.type_id, type_id)? {
                    return Err(rules_error(
                        place,
                        format!(
                            "`{name}` has type {}, which differs from type {type_id}",
                            input.type_id
                        ),
                    ));
                }
                Ok(match input.field {
                    Some(field) => Expr::Input(input.source, field),
                    None => Expr::Root(input.source),
                })
            }
            _ => {
                let key = (ast.clone(), Some(type_id));
                if let Some(index) = self.entries.iter().position(|entry| *entry == key) {
                    return Ok(Expr::Output(index));
                }
                check_program(declarations, inputs, ast, place)?;
                let kind = declarations.kind(type_id)?;
                let wide = matches!(kind, TypeKind::I128 { .. });
                let leaf = input_leaf(declarations, type_id, wide)?;
                let domain = scalar_domain(declarations, type_id, wide)?;
                self.entries.push(key);
                self.types.push(leaf);
                self.domains.push(domain);
                Ok(Expr::Output(self.entries.len() - 1))
            }
        }
    }
}

/// The scoped `.zeno` laws, the framework failure and reject laws, genesis
/// law 990 and decision-conformance law 991.
fn laws(
    declarations: &Declarations,
    rules: &Rules,
    state_fields: &[(u16, u32)],
    genesis: &[(u16, Atom)],
) -> Result<Vec<Law>, ContractError> {
    let declared: Vec<u32> = declarations.laws.iter().map(|law| law.id).collect();
    if rules.law_kinds.keys().copied().collect::<Vec<_>>() != declared {
        return Err(rules_error(
            "law_kinds",
            format!("must give a kind to exactly the declared laws {declared:?}"),
        ));
    }
    if let Some(id) = rules.delivery_laws.keys().find(|id| !declared.contains(id)) {
        return Err(rules_error("delivery_laws", format!("undeclared law {id}")));
    }
    let mut laws = Vec::new();
    for law in &declarations.laws {
        let place = format!("project.zeno law {}", law.id);
        let kind = rules.law_kinds[&law.id];
        if let Some((scope, genesis, written)) = required_declaration(kind)
            && (law.scope != scope || law.genesis != genesis)
        {
            return Err(ContractError::new(
                &place,
                format!("a {} law must be declared `{written}`", kind.name()),
            ));
        }
        let failed = |reason: String| ContractError::new(&place, reason);
        let formula = expr::expand(&law.formula, &rules.variables).map_err(failed)?;
        let mut graph = LawGraph::new(declarations);
        let bound = rules
            .delivery_laws
            .get(&law.id)
            .map(|delivery| {
                if law.genesis {
                    return Err("delivery observations have no value at genesis".to_owned());
                }
                super::delivery::bind(&mut graph, declarations, rules, delivery)
            })
            .transpose()
            .map_err(failed)?;
        let value = graph.compile(&formula).map_err(failed)?;
        let mut root = graph.boolean(value).map_err(failed)?;
        graph.require_bindings_used().map_err(failed)?;
        if let Some(bound) = bound {
            root = graph
                .op(Op::And(bound.index, root.index), Kind::Bool)
                .map_err(failed)?;
        }
        // The library evaluates every node of a law that applies.
        if law.genesis
            && let Some(name) = graph.table.nodes.iter().find_map(|node| match node {
                LawOp::Observe(observation) => absent_at_genesis(observation),
                _ => None,
            })
        {
            return Err(failed(format!(
                "applies at genesis, where `{name}` has no value"
            )));
        }
        laws.push(Law {
            id: law.id,
            kind: rules.law_kinds[&law.id],
            scope: law.scope,
            genesis: law.genesis,
            nodes: graph.table.nodes,
            root: root.index,
        });
    }
    if !laws.iter().any(|law| law.kind == LawKind::StateInvariant) {
        return Err(rules_error(
            "law_kinds",
            "needs a StateInvariant law, declared `on commit, genesis`",
        ));
    }
    let failed = |reason: String| rules_error("", reason);
    if !laws
        .iter()
        .any(|law| law.kind == LawKind::CommittedFailureEffects)
    {
        // Without a committed-failure law, no committed failure is lawful,
        // so a case that decides one could never commit.
        if let Some(index) = rules
            .cases
            .iter()
            .position(|case| case.class == Class::CommittedFailure)
        {
            return Err(rules_error(
                &format!("cases[{index}]"),
                format!(
                    "is a committed failure, but no law has kind CommittedFailureEffects, so \
                     framework law {} would refuse every committed failure at run time. Declare a \
                     law `on failure` in project.zeno and give it kind CommittedFailureEffects in \
                     law_kinds",
                    rules.failure_law
                ),
            ));
        }
        let mut graph = LawGraph::new(declarations);
        let root = graph.bool(false);
        laws.push(Law {
            id: rules.failure_law,
            kind: LawKind::CommittedFailureEffects,
            scope: LawScope::CommittedFailure,
            genesis: false,
            nodes: graph.table.nodes,
            root: root.index,
        });
    }
    // A reject changes no state and delivers nothing.
    let mut graph = LawGraph::new(declarations);
    let mut checks = Vec::new();
    for observation in [
        Observation::PostLength,
        Observation::PatchLength,
        Observation::EffectLength,
        Observation::OutboxLength,
    ] {
        let observed = graph
            .observe(observation, Kind::Int, None, Atom::I128(0))
            .map_err(failed)?;
        let zero = graph.int(0).map_err(failed)?;
        checks.push(
            graph
                .op(Op::Eq(observed.index, zero.index), Kind::Bool)
                .map_err(failed)?,
        );
    }
    let root = graph.all(&checks).map_err(failed)?;
    // A template's own reject law is kept; this structural law joins it.
    let (id, kind) = if laws
        .iter()
        .any(|law| law.kind == LawKind::RejectNoAuthority)
    {
        (REJECT_STRUCTURE_LAW, LawKind::AuthoritySubjectRecipient)
    } else {
        (rules.reject_law, LawKind::RejectNoAuthority)
    };
    laws.push(Law {
        id,
        kind,
        scope: LawScope::Reject,
        genesis: false,
        nodes: graph.table.nodes,
        root: root.index,
    });

    let mut graph = LawGraph::new(declarations);
    let mut checks = Vec::new();
    for (field, value) in genesis {
        let observed = graph
            .observe(Observation::Initial(*field), Kind::Raw, None, Atom::I128(0))
            .map_err(failed)?;
        let wanted = graph.literal(value.clone());
        checks.push(
            graph
                .op(Op::Eq(observed.index, wanted.index), Kind::Bool)
                .map_err(failed)?,
        );
    }
    let root = graph.all(&checks).map_err(failed)?;
    laws.push(Law {
        id: GENESIS_LAW,
        kind: LawKind::InitialCondition,
        scope: LawScope::Always,
        genesis: true,
        nodes: graph.table.nodes,
        root: root.index,
    });
    laws.push(conformance(declarations, rules, state_fields, genesis)?);
    // Declared laws have distinct IDs, so a repeated ID is a framework law's.
    let declared_count = declarations.laws.len();
    for (index, law) in laws.iter().enumerate().skip(declared_count) {
        if let Some(earlier) = laws[..index].iter().position(|other| other.id == law.id) {
            return Err(if earlier < declared_count {
                ContractError::new(
                    format!("project.zeno law {}", law.id),
                    format!("has the ID of the framework {} law", law.kind.name()),
                )
            } else {
                rules_error(
                    "",
                    format!(
                        "the framework {} and {} laws share ID {}",
                        laws[earlier].kind.name(),
                        law.kind.name(),
                        law.id
                    ),
                )
            });
        }
    }
    Ok(laws)
}

/// One inductive claim compiled to a law program over the state, which a
/// behaviour-change upgrade evaluates on the store's state: it reads only
/// `post.` fields, which a genesis frame binds to the state itself.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct CompiledClaim {
    pub(super) id: u32,
    pub(super) nodes: Vec<LawOp>,
    pub(super) root: usize,
}

/// Every inductive claim of `declarations`, in ID order, its invariant
/// restated over `post.` and compiled as a law is. A claim of another mode
/// says nothing about states and is not compiled.
///
/// # Errors
/// A claim whose invariant has no law form here, reads anything but the
/// state, or has the ID of one of `laws`, which the library would refuse.
pub(super) fn compiled_claims(
    declarations: &Declarations,
    rules: &Rules,
    laws: &[Law],
) -> Result<Vec<CompiledClaim>, ContractError> {
    use zeno_fcis_spec::{ClaimFormula, ClaimMode, ProjectionRoot, invariant_at};
    let mut compiled = Vec::new();
    for claim in &declarations.notes.claims {
        if claim.mode() != ClaimMode::Inductive {
            continue;
        }
        let id = claim.id().get();
        let place = format!("project.zeno claim {id}");
        let failed = |reason: String| ContractError::new(&place, reason);
        if laws.iter().any(|law| law.id == id) {
            return Err(failed(
                "has the ID of a law of the contract; a behaviour-change upgrade evaluates both,                  so give the claim an ID of its own"
                    .to_owned(),
            ));
        }
        let ClaimFormula::Relational(invariant) = claim.formula() else {
            return Err(failed("is inductive but not relational".to_owned()));
        };
        let over_state = invariant_at(invariant, ProjectionRoot::Post).ok_or_else(|| {
            failed("reads more than the state, so it cannot be checked on a state".to_owned())
        })?;
        let ast = expr::law(&over_state)
            .map_err(|reason| failed(format!("{reason} has no contract form")))?;
        let formula = expr::expand(&ast, &rules.variables).map_err(failed)?;
        let mut graph = LawGraph::new(declarations);
        let value = graph.compile(&formula).map_err(failed)?;
        let root = graph.boolean(value).map_err(failed)?;
        if let Some(name) = graph.table.nodes.iter().find_map(|node| match node {
            LawOp::Observe(observation) | LawOp::ObserveWhen(_, observation, _) => {
                absent_at_genesis(observation)
            }
            _ => None,
        }) {
            return Err(failed(format!(
                "reads `{name}`, which a state does not hold"
            )));
        }
        compiled.push(CompiledClaim {
            id,
            nodes: graph.table.nodes,
            root: root.index,
        });
    }
    Ok(compiled)
}

/// The scope, whether genesis applies, and how `project.zeno` writes them,
/// that a law of `kind` must be declared with; `None` for any scope.
pub(super) fn required_declaration(kind: LawKind) -> Option<(LawScope, bool, &'static str)> {
    match kind {
        LawKind::StateInvariant => Some((LawScope::Committing, true, "on commit, genesis")),
        LawKind::RejectNoAuthority => Some((LawScope::Reject, false, "on reject")),
        LawKind::CommittedFailureEffects => Some((LawScope::CommittedFailure, false, "on failure")),
        LawKind::DecisionConformance => Some((LawScope::Always, false, "on any")),
        LawKind::InitialCondition => Some((LawScope::Always, true, "on any, genesis")),
        LawKind::AssetConservation
        | LawKind::MintBurnAuthorization
        | LawKind::DebitCreditEffectEquality
        | LawKind::FeeAndRounding
        | LawKind::AuthoritySubjectRecipient => None,
    }
}

/// Law 991: the first case whose `when` holds fixes the class, reason,
/// successor fields and deliveries; some case always holds. Each `when` is
/// recomputed from the original inputs, never from the candidate.
fn conformance(
    declarations: &Declarations,
    rules: &Rules,
    state_fields: &[(u16, u32)],
    genesis: &[(u16, Atom)],
) -> Result<Law, ContractError> {
    let mut graph = LawGraph::new(declarations);
    let mut tests = Vec::new();
    let mut remaining = graph.bool(true);
    for (index, case) in rules.cases.iter().enumerate() {
        let place = format!("cases[{index}]");
        let failed = |reason: String| rules_error(&place, reason);
        let when = expr::expand(&case.when, &rules.variables).map_err(failed)?;
        let holds = graph.compile(&when).map_err(failed)?;
        let holds = graph.boolean(holds).map_err(failed)?;
        let guard = graph
            .op(Op::And(remaining.index, holds.index), Kind::Bool)
            .map_err(failed)?;
        let fails = graph.op(Op::Not(holds.index), Kind::Bool).map_err(failed)?;
        remaining = graph
            .op(Op::And(remaining.index, fails.index), Kind::Bool)
            .map_err(failed)?;

        let mut checks = Vec::new();
        let class = graph
            .observe(Observation::Class, Kind::Int, None, Atom::I128(0))
            .map_err(failed)?;
        checks.push(equal_int(&mut graph, class, case.class.code()).map_err(failed)?);
        let has_reason = graph
            .observe(Observation::HasReason, Kind::Bool, None, Atom::I128(0))
            .map_err(failed)?;
        checks.push(match case.reason {
            Some(_) => has_reason,
            None => graph
                .op(Op::Not(has_reason.index), Kind::Bool)
                .map_err(failed)?,
        });
        if let Some(reason) = case.reason {
            let observed = graph
                .observe(Observation::Reason, Kind::Int, Some(guard), Atom::I128(0))
                .map_err(failed)?;
            checks.push(equal_int(&mut graph, observed, i128::from(reason)).map_err(failed)?);
        }
        for (observation, length) in [
            (Observation::EffectLength, 0),
            (Observation::OutboxLength, case.outbox.len()),
            (Observation::PostLength, case.post.len()),
        ] {
            let observed = graph
                .observe(observation, Kind::Int, None, Atom::I128(0))
                .map_err(failed)?;
            checks.push(equal_int(&mut graph, observed, count_value(length)).map_err(failed)?);
        }
        for (field, type_id) in state_fields {
            if let Some(value) = case.post.get(field) {
                // Where the guard is false, a read yields the field's genesis value.
                let default = genesis
                    .iter()
                    .find(|(id, _)| id == field)
                    .map_or(Atom::I128(0), |(_, value)| value.clone());
                checks.push(
                    observed_value(
                        &mut graph,
                        declarations,
                        rules,
                        Observation::Post(*field),
                        value,
                        *type_id,
                        guard,
                        default,
                    )
                    .map_err(failed)?,
                );
            }
        }
        for (number, delivery) in case.outbox.iter().enumerate() {
            for (observation, value) in [
                (
                    Observation::OutboxOrdinal(number),
                    i128::from(delivery.ordinal),
                ),
                (
                    Observation::OutboxChannel(number),
                    i128::from(delivery.channel),
                ),
            ] {
                let observed = graph
                    .observe(observation, Kind::Int, Some(guard), Atom::I128(0))
                    .map_err(failed)?;
                checks.push(equal_int(&mut graph, observed, value).map_err(failed)?);
            }
            for (observation, atom) in [
                (
                    Observation::OutboxDestination(number),
                    Atom::Text(delivery.destination.as_bytes().to_vec()),
                ),
                (
                    Observation::OutboxIdempotency(number),
                    Atom::U128(delivery.idempotency),
                ),
            ] {
                let observed = graph
                    .observe(observation, Kind::Raw, Some(guard), atom.clone())
                    .map_err(failed)?;
                let wanted = graph.literal(atom);
                checks.push(
                    graph
                        .op(Op::Eq(observed.index, wanted.index), Kind::Bool)
                        .map_err(failed)?,
                );
            }
            let channel = declarations.channel(delivery.channel)?;
            for field in declarations.fields(channel.payload)? {
                let default = type_default(declarations, field.type_id).map_err(failed)?;
                checks.push(
                    observed_value(
                        &mut graph,
                        declarations,
                        rules,
                        Observation::OutboxPayload(number, field.id),
                        &delivery.payload[&field.id],
                        field.type_id,
                        guard,
                        default,
                    )
                    .map_err(failed)?,
                );
            }
        }
        let all = graph.all(&checks).map_err(failed)?;
        let otherwise = graph.bool(true);
        tests.push(
            graph
                .op(
                    Op::Select(guard.index, all.index, otherwise.index),
                    Kind::Bool,
                )
                .map_err(failed)?,
        );
    }
    let failed = |reason: String| rules_error("cases", reason);
    tests.push(
        graph
            .op(Op::Not(remaining.index), Kind::Bool)
            .map_err(failed)?,
    );
    let root = graph.all(&tests).map_err(failed)?;
    Ok(Law {
        id: CONFORMANCE_LAW,
        kind: LawKind::DecisionConformance,
        scope: LawScope::Always,
        genesis: false,
        nodes: graph.table.nodes,
        root: root.index,
    })
}

/// Where `guard` holds, the guarded observation equals the case's value.
fn equal_int(graph: &mut LawGraph<'_>, observed: Ref, value: i128) -> Result<Ref, String> {
    let wanted = graph.int(value)?;
    graph.op(Op::Eq(observed.index, wanted.index), Kind::Bool)
}

#[allow(clippy::too_many_arguments)]
fn observed_value(
    graph: &mut LawGraph<'_>,
    declarations: &Declarations,
    rules: &Rules,
    observation: Observation,
    expected: &Ast,
    type_id: u32,
    guard: Ref,
    default: Atom,
) -> Result<Ref, String> {
    let actual = graph.observe(observation, Kind::Raw, Some(guard), default)?;
    let expected = expr::expand(expected, &rules.variables)?;
    let equal = match expected {
        Ast::Bool(_) | Ast::Int(_) => {
            let wanted = graph.literal(atom(declarations, &expected, type_id)?);
            graph.op(Op::Eq(actual.index, wanted.index), Kind::Bool)?
        }
        _ => {
            let actual = graph.numeric(actual)?;
            let value = graph.compile(&expected)?;
            let value = graph.numeric(value)?;
            graph.op(Op::Eq(actual.index, value.index), Kind::Bool)?
        }
    };
    let otherwise = graph.bool(true);
    graph.op(
        Op::Select(guard.index, equal.index, otherwise.index),
        Kind::Bool,
    )
}

/// Each declared reason with the one class every case using it has.
fn reasons(declarations: &Declarations, rules: &Rules) -> Result<Vec<(u32, Class)>, ContractError> {
    for (index, case) in rules.cases.iter().enumerate() {
        if let Some(reason) = case.reason
            && !declarations.reasons.contains(&reason)
        {
            return Err(rules_error(
                &format!("cases[{index}].reason"),
                format!("reason {reason} is not declared"),
            ));
        }
    }
    declarations
        .reasons
        .iter()
        .map(|reason| {
            let mut classes = rules
                .cases
                .iter()
                .filter(|case| case.reason == Some(*reason))
                .map(|case| case.class);
            match classes.next() {
                Some(class) if classes.all(|other| other == class) => Ok((*reason, class)),
                Some(_) => Err(rules_error(
                    "cases",
                    format!("reason {reason} is used with two classes"),
                )),
                None => Err(rules_error(
                    "cases",
                    format!("reason {reason} is declared but no case uses it"),
                )),
            }
        })
        .collect()
}

/// A constant of the given type: booleans for `bool`, integers for `int`,
/// and a declared variant ID for sums.
fn atom(declarations: &Declarations, ast: &Ast, type_id: u32) -> Result<Atom, String> {
    let kind = declarations
        .kind(type_id)
        .map_err(|error| error.to_string())?;
    match (kind, ast) {
        (TypeKind::Bool, Ast::Bool(value)) => Ok(Atom::Bool(*value)),
        (TypeKind::I128 { .. }, Ast::Int(value)) => Ok(Atom::I128(*value)),
        (TypeKind::Sum(variants), Ast::Int(value)) => variants
            .iter()
            .find(|variant| i128::from(variant.id) == *value)
            .map(|variant| Atom::Sum {
                type_id,
                variant: variant.id,
            })
            .ok_or_else(|| format!("{value} is not a variant of type {type_id}")),
        (_, value) => Err(format!("constant {value:?} does not fit type {type_id}")),
    }
}

fn constant_atom(
    declarations: &Declarations,
    value: Constant,
    type_id: u32,
) -> Result<Atom, String> {
    let ast = match value {
        Constant::Bool(value) => Ast::Bool(value),
        Constant::Int(value) => Ast::Int(value),
    };
    atom(declarations, &ast, type_id)
}

/// The value a guarded payload read yields where its guard is false.
fn type_default(declarations: &Declarations, type_id: u32) -> Result<Atom, String> {
    Ok(
        match declarations
            .kind(type_id)
            .map_err(|error| error.to_string())?
        {
            TypeKind::Bool => Atom::Bool(false),
            TypeKind::Sum(variants) => Atom::Sum {
                type_id,
                variant: variants.first().map_or(0, |variant| variant.id),
            },
            TypeKind::I128 { .. } => Atom::I128(0),
            TypeKind::Text => return Err(format!("text type {type_id} cannot be a payload field")),
        },
    )
}

fn domain(declarations: &Declarations, type_id: u32) -> Result<Domain, ContractError> {
    Ok(match declarations.kind(type_id)? {
        TypeKind::Bool => Domain::Bool,
        TypeKind::I128 { min, max } => Domain::I128 { min, max },
        TypeKind::Text => Domain::Text,
        TypeKind::Sum(variants) => Domain::Sum {
            type_id,
            variants: variants.iter().map(|variant| variant.id).collect(),
        },
    })
}

/// The input leaf of a scalar type; `wide` integers span the whole 64-bit
/// program range, as computed outputs do.
pub(super) fn input_leaf(
    declarations: &Declarations,
    type_id: u32,
    wide: bool,
) -> Result<InputLeaf, ContractError> {
    Ok(match declarations.kind(type_id)? {
        TypeKind::Bool => InputLeaf::Bool,
        // Each variant's ID is its program code, and the library requires
        // the codes of a sum to fill one interval.
        TypeKind::Sum(variants)
            if variants
                .windows(2)
                .all(|pair| u32::from(pair[1].id) == u32::from(pair[0].id) + 1) =>
        {
            InputLeaf::Sum {
                type_id,
                variants: variants.iter().map(|variant| variant.id).collect(),
            }
        }
        TypeKind::Sum(_) => {
            return Err(ContractError::new(
                "project.zeno",
                format!(
                    "sum type {type_id} needs consecutive variant IDs to be a program input or output"
                ),
            ));
        }
        TypeKind::I128 { .. } if wide => InputLeaf::I128 {
            min: i64::MIN,
            max: i64::MAX,
        },
        TypeKind::I128 { min, max } => {
            let (min, max) = program_bounds(type_id, min, max)?;
            InputLeaf::I128 { min, max }
        }
        TypeKind::Text => {
            return Err(ContractError::new(
                "project.zeno",
                format!("text type {type_id} cannot be a program input or output"),
            ));
        }
    })
}

fn scalar_domain(
    declarations: &Declarations,
    type_id: u32,
    wide: bool,
) -> Result<ScalarDomain, ContractError> {
    Ok(match input_leaf(declarations, type_id, wide)? {
        InputLeaf::Bool => ScalarDomain::Bool,
        InputLeaf::I128 { min, max } => ScalarDomain::Int { min, max },
        InputLeaf::Sum { variants, .. } => ScalarDomain::Int {
            min: variants.first().copied().map_or(0, i64::from),
            max: variants.last().copied().map_or(0, i64::from),
        },
    })
}

fn program_bounds(type_id: u32, min: i128, max: i128) -> Result<(i64, i64), ContractError> {
    i64::try_from(min)
        .ok()
        .zip(i64::try_from(max).ok())
        .ok_or_else(|| {
            ContractError::new(
                "project.zeno",
                format!("type {type_id} range exceeds the 64-bit program range"),
            )
        })
}

fn root_schema(declarations: &Declarations, root: u32) -> Result<RootSchema, ContractError> {
    Ok(match &declarations.get(root)?.form {
        Form::Record(fields) => RootSchema::Record(
            fields
                .iter()
                .map(|field| Ok((field.id, input_leaf(declarations, field.type_id, false)?)))
                .collect::<Result<_, ContractError>>()?,
        ),
        _ => RootSchema::Leaf(input_leaf(declarations, root, false)?),
    })
}

/// The largest canonical value encoding of a root type, without its envelope.
fn value_bytes(declarations: &Declarations, type_id: u32) -> Result<u64, ContractError> {
    Ok(match &declarations.get(type_id)?.form {
        Form::Record(fields) => {
            let mut total = RECORD_BYTES;
            for field in fields {
                total += FIELD_BYTES + value_bytes(declarations, field.type_id)?;
            }
            total
        }
        Form::Leaf(Leaf::Bool) => BOOL_BYTES,
        Form::Leaf(Leaf::I128 { .. }) => I128_BYTES,
        Form::Sum(_) => SUM_BYTES,
        Form::Leaf(Leaf::Text { .. }) => {
            return Err(ContractError::new(
                "project.zeno",
                format!("text type {type_id} cannot be part of a root"),
            ));
        }
    })
}

/// Whether values of two types have the same library kind: booleans,
/// integers of any range, or the same sum.
fn same_kind(declarations: &Declarations, left: u32, right: u32) -> Result<bool, ContractError> {
    Ok(
        match (declarations.kind(left)?, declarations.kind(right)?) {
            (TypeKind::Bool, TypeKind::Bool)
            | (TypeKind::I128 { .. }, TypeKind::I128 { .. })
            | (TypeKind::Text, TypeKind::Text) => true,
            (TypeKind::Sum(_), TypeKind::Sum(_)) => left == right,
            _ => false,
        },
    )
}

/// The name a law reads that genesis has no value for: genesis supplies
/// only the initial state, read as post-state.
fn absent_at_genesis(observation: &Observation) -> Option<String> {
    match observation {
        Observation::Pre(field) => Some(format!("pre.100.{field}")),
        Observation::Command(field) => Some(format!("command.101.{field}")),
        Observation::Context(field) => Some(format!("context.102.{field}")),
        Observation::PreRoot => Some("pre.100".to_owned()),
        Observation::CommandRoot => Some("command.101".to_owned()),
        Observation::ContextRoot => Some("context.102".to_owned()),
        _ => None,
    }
}

/// Compiles one expression alone, so an expression the decision program
/// cannot hold is refused at the entry that holds it.
fn check_program(
    declarations: &Declarations,
    inputs: &[super::declarations::Input],
    ast: &Ast,
    place: &str,
) -> Result<(), ContractError> {
    ScalarGraph::new(declarations, inputs)
        .compile(ast)
        .map(|_| ())
        .map_err(|reason| rules_error(place, reason))
}

/// One Step per program and law node, plus the allowance.
fn step_budget(program_nodes: usize, laws: &[Law]) -> u64 {
    let law_nodes: usize = laws.iter().map(|law| law.nodes.len()).sum();
    count(program_nodes + law_nodes) + STEP_ALLOWANCE
}

/// The contract form of an admitted library instruction. The library's
/// instruction set may grow; an instruction this generator cannot render is
/// refused rather than approximated.
fn contract_op(op: &LibraryOp, place: &str) -> Result<ScalarOp, ContractError> {
    let index = |value: u16| usize::from(value);
    Ok(match *op {
        LibraryOp::Input(a) => ScalarOp::Input(index(a)),
        LibraryOp::Int(value) => ScalarOp::Int(value),
        LibraryOp::Bool(value) => ScalarOp::Bool(value),
        LibraryOp::Add(a, b) => ScalarOp::Add(index(a), index(b)),
        LibraryOp::Sub(a, b) => ScalarOp::Sub(index(a), index(b)),
        LibraryOp::Eq(a, b) => ScalarOp::Eq(index(a), index(b)),
        LibraryOp::Lt(a, b) => ScalarOp::Lt(index(a), index(b)),
        LibraryOp::And(a, b) => ScalarOp::And(index(a), index(b)),
        LibraryOp::Not(a) => ScalarOp::Not(index(a)),
        LibraryOp::Select(condition, a, b) => {
            ScalarOp::Select(index(condition), index(a), index(b))
        }
        ref other => {
            return Err(rules_error(
                place,
                format!("the candidate uses {other:?}, which contracts cannot hold"),
            ));
        }
    })
}

fn rules_error(place: &str, reason: impl Into<String>) -> ContractError {
    if place.is_empty() {
        ContractError::new("v2/policy.json", reason)
    } else {
        ContractError::new(format!("v2/policy.json {place}"), reason)
    }
}

fn count(value: usize) -> u64 {
    u64::try_from(value).unwrap_or(u64::MAX)
}

fn count_value(value: usize) -> i128 {
    i128::try_from(value).unwrap_or(i128::MAX)
}

fn index_value(value: usize) -> i128 {
    count_value(value)
}
