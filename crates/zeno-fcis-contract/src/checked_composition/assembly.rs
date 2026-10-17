//! The assembly validator, after rule SUBST of the composition paper: an
//! ordinary aggregate contract is accepted only when it is a machine map
//! onto the checked composition.
//!
//! The aggregate is compiled through the ordinary contract route and its
//! decision program compared with each leaf's structurally, never by
//! enumerating the joint product:
//!
//! - **State projection:** its state fields are the slots, each with the
//!   owning leaf field's domain, and its genesis is the leaves' genesis.
//! - **Command routing:** its target field ranges over the leaves, and each
//!   leaf command field is an aggregate command field of the same domain.
//! - **Decision:** for each route, the aggregate's program is specialized
//!   to that target and both programs are reduced to canonical terms over
//!   the aggregate's inputs. A leaf's state field is its slot, its peer view
//!   is the provider's slot in the same pre-state, and an environment input
//!   is a context field. The case selection must choose corresponding
//!   cases, each with the same class and reason. Every committing case must
//!   give the selected leaf's slots the leaf's values and every other slot
//!   its own pre-state value.
//! - **Eager nodes:** an interval analysis shows that no node of the
//!   aggregate program can trap on any input in its declared domains, so
//!   the specialization preserves the eager program's value. The Step budget
//!   covers every program and law node, and one evaluation per route shows
//!   the actual usage.
//! - **Laws:** every aggregate law is a StateInvariant on committing
//!   decisions whose top-level conjuncts each read one leaf's slots. Each
//!   conjunct holds on every state of that leaf satisfying its invariant,
//!   so on a pre-state satisfying every leaf's invariant no aggregate law
//!   refuses a decision the selected leaf commits.
//!
//! The correspondence is claimed only on pre-states satisfying every
//! leaf's invariant on its slots, with commands each leaf admits there:
//! certification constrains only those states.

use std::collections::{BTreeMap, BTreeSet};

use serde_json::{Value, json};
use zeno_fcis_synthesis::finite::{
    V2Resource, v2_authority::PublicationOutcome, v2_composition as c,
};

use super::aggregate::{AggregateSources, Layout, View, state_field};
use super::hex;
use super::leaf::{self, Compiled, sha256};
use super::manifest::Manifest;
use super::predicate::{Form as PredicateForm, Predicate, RootFields, Truth};
use super::refusal::{AuthorityRefusal, ExecutionRefusal, LawRefusal, Obligation, Refusal, Stage};
use super::structure::Resolved;
use super::term::{self, Arena, Key, Node};
use crate::ContractError;
use crate::internals::declarations::{Declarations, Source};
use crate::internals::expr::{self, Ast, Binary};
use crate::internals::graph::{Atom, ScalarOp};
use crate::internals::model::{Contract, Expr, ScalarDomain};
use crate::internals::review::domain::{self, LeafDomain};
use crate::internals::review::evaluate::Framer;
use crate::internals::rules::{Class, Constant, LawKind, Rules};
use crate::internals::{policy, schema, schema_commitment};
use zeno_fcis_spec::LawScope;

/// What the validator established for an aggregate.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Assembly {
    project_sha256: [u8; 32],
    rules_sha256: [u8; 32],
    schema_sha256: [u8; 32],
    program_sha256: [u8; 32],
    routes: usize,
    cases: usize,
    mapped_cases: usize,
    nodes: usize,
    outputs: usize,
    law_nodes: usize,
    step_budget: u64,
    max_observed_steps: u64,
    law_conjuncts: usize,
    law_rows: u64,
}

impl Assembly {
    pub(crate) fn json(&self) -> Value {
        json!({
            "profile": "one selected leaf per command; peer views from the same pre-state; every unselected slot unchanged",
            "scope": "pre-states satisfying every leaf's invariant on its slots, with commands the selected leaf admits and commands and context satisfying the root group's outer assumption",
            "statement": "machine-checked over the evaluator specification, with the validator implementation tested, not proved: the result's trusted base",
            "aggregate": {
                "project_sha256": hex(&self.project_sha256),
                "policy_sha256": hex(&self.rules_sha256),
                "schema_sha256": hex(&self.schema_sha256),
                "program_sha256": hex(&self.program_sha256),
            },
            "routes": self.routes,
            "cases": self.cases,
            "mapped_cases": self.mapped_cases,
            "program": {
                "nodes": self.nodes,
                "outputs": self.outputs,
                "law_nodes": self.law_nodes,
                "step_budget": self.step_budget,
                "max_observed_steps": self.max_observed_steps,
            },
            "laws": {"conjuncts": self.law_conjuncts, "rows": self.law_rows},
        })
    }

    pub(crate) const fn program_sha256(&self) -> [u8; 32] {
        self.program_sha256
    }

    pub(crate) const fn law_rows(&self) -> u64 {
        self.law_rows
    }
}

fn refuse(obligation: Obligation, place: impl Into<String>, reason: impl Into<String>) -> Refusal {
    Refusal::new(Stage::Assembly, obligation, place, reason)
}

fn route_error(error: ContractError) -> Refusal {
    refuse(
        Obligation::AssemblyProfile,
        format!("aggregate {}", error.place()),
        error.reason(),
    )
}

/// One compiled leaf with its contract, for comparison.
struct LeafProgram<'c> {
    contract: Contract<'c>,
}

/// The aggregate compiled through the ordinary contract route.
pub(crate) struct CompiledAggregate {
    pub(crate) rules: Rules,
    pub(crate) declarations: Declarations,
    pub(crate) schema: Vec<u8>,
    pub(crate) commitment: [u8; 32],
}

impl CompiledAggregate {
    /// Compiles the aggregate's files as `generate contract` does.
    ///
    /// # Errors
    /// The route's refusal, or adoptions and evolutions, which the
    /// aggregate does not have.
    pub(crate) fn read(aggregate: &AggregateSources) -> Result<Self, Refusal> {
        let rules = Rules::read(&aggregate.rules).map_err(route_error)?;
        if !rules.adoptions.is_empty() || !rules.evolutions.is_empty() {
            return Err(refuse(
                Obligation::AssemblyProfile,
                "aggregate v2/policy.json",
                "lists adoptions or evolutions; the aggregate is the rules-compiled program",
            ));
        }
        let declarations =
            Declarations::read(&aggregate.project, &rules.leaf_bindings).map_err(route_error)?;
        let schema = schema::encode(&declarations).map_err(route_error)?;
        let commitment = schema_commitment(&schema).map_err(route_error)?;
        Ok(Self {
            rules,
            declarations,
            schema,
            commitment,
        })
    }

    pub(crate) fn contract(&self) -> Result<Contract<'_>, Refusal> {
        Contract::build(&self.declarations, &self.rules, self.commitment).map_err(route_error)
    }
}

/// Validates an aggregate against a checked composition.
///
/// # Errors
/// The first structural obligation the aggregate fails, named.
pub(crate) fn validate(
    manifest: &Manifest,
    compiled: &[Compiled],
    edges: &[Resolved],
    aggregate: &AggregateSources,
) -> Result<Assembly, Refusal> {
    let built = CompiledAggregate::read(aggregate)?;
    let contract = built.contract()?;
    validate_contract(manifest, compiled, edges, aggregate, &built, &contract)
}

/// [`validate`] on an already built aggregate contract; a planted-defect
/// control may pass one that differs from its files.
pub(crate) fn validate_contract(
    manifest: &Manifest,
    compiled: &[Compiled],
    edges: &[Resolved],
    aggregate: &AggregateSources,
    built: &CompiledAggregate,
    contract: &Contract<'_>,
) -> Result<Assembly, Refusal> {
    let layout = Layout::of(manifest, compiled, edges)?;
    let (rules, declarations, schema) = (&built.rules, &built.declarations, &built.schema);
    let leaves: Vec<LeafProgram<'_>> = compiled
        .iter()
        .map(|leaf| {
            Contract::build(&leaf.declarations, &leaf.rules, leaf.commitment)
                .map(|contract| LeafProgram { contract })
                .map_err(route_error)
        })
        .collect::<Result<_, _>>()?;

    fields(&layout, compiled, declarations)?;
    genesis(&layout, compiled, rules)?;
    if !contract.channels.is_empty()
        || contract
            .branches
            .iter()
            .any(|branch| !branch.outbox.is_empty())
    {
        return Err(refuse(
            Obligation::NoEffectsOrOutbox,
            "aggregate",
            "declares a channel or a delivery; the profile has no effects or outbox",
        ));
    }
    let nodes: Vec<Node> = contract.nodes.iter().map(node).collect::<Result<_, _>>()?;
    eager(contract, &nodes)?;
    let law_nodes: usize = contract.laws.iter().map(|law| law.nodes.len()).sum();
    let needed = u64::try_from(nodes.len() + law_nodes).unwrap_or(u64::MAX);
    if contract.budgets.step < needed {
        return Err(refuse(
            Obligation::MeterCoversEagerWork,
            "aggregate budgets",
            format!(
                "the Step budget {} is below the {needed} program and law nodes evaluated eagerly",
                contract.budgets.step
            ),
        ));
    }
    let mapped = decisions(&layout, manifest, &leaves, contract, &nodes)?;
    let (law_conjuncts, law_rows) = laws(&layout, compiled, declarations, rules)?;
    let positions = domain::positions(declarations).map_err(route_error)?;
    let (program_sha256, max_observed_steps) =
        observe(&layout, compiled.len(), contract, schema, &positions, rules)?;
    Ok(Assembly {
        project_sha256: sha256(aggregate.project.as_bytes()),
        rules_sha256: sha256(aggregate.rules.as_bytes()),
        schema_sha256: sha256(schema),
        program_sha256,
        routes: compiled.len(),
        cases: contract.branches.len(),
        mapped_cases: mapped,
        nodes: nodes.len(),
        outputs: contract.program_roots.len(),
        law_nodes,
        step_budget: contract.budgets.step,
        max_observed_steps,
        law_conjuncts,
        law_rows,
    })
}

fn node(op: &ScalarOp) -> Result<Node, Refusal> {
    Ok(match *op {
        ScalarOp::Input(index) => Node::Input(index),
        ScalarOp::Int(value) => Node::Int(value),
        ScalarOp::Bool(value) => Node::Bool(value),
        ScalarOp::Add(a, b) => Node::Add(a, b),
        ScalarOp::Sub(a, b) => Node::Sub(a, b),
        ScalarOp::Eq(a, b) => Node::Eq(a, b),
        ScalarOp::Lt(a, b) => Node::Lt(a, b),
        ScalarOp::And(a, b) => Node::And(a, b),
        ScalarOp::Not(a) => Node::Not(a),
        ScalarOp::Select(c, a, b) => Node::Select(c, a, b),
        ScalarOp::Mul(..)
        | ScalarOp::Div(..)
        | ScalarOp::MulDiv(..)
        | ScalarOp::SignedMulDivFloor(..)
        | ScalarOp::ProductsGe(..) => {
            return Err(refuse(
                Obligation::EagerNodesTotal,
                "program",
                "arithmetic extension is outside the assembly profile",
            ));
        }
    })
}

/// The aggregate's fields follow the assembly profile's map, with the
/// leaves' domains.
fn fields(
    layout: &Layout,
    compiled: &[Compiled],
    declarations: &Declarations,
) -> Result<(), Refusal> {
    let aggregate_domain =
        |type_id: u32| domain::leaf_domain(declarations, type_id).map_err(route_error);
    let state = declarations.fields(100).map_err(route_error)?;
    let expected: BTreeSet<u16> = (0..layout.slots.len()).map(state_field).collect();
    let actual: BTreeSet<u16> = state.iter().map(|field| field.id).collect();
    if actual != expected {
        return Err(refuse(
            Obligation::StateProjection,
            "aggregate state",
            format!(
                "its fields are not the {} slot fields {}..{} of the assembly profile",
                layout.slots.len(),
                state_field(0),
                state_field(layout.slots.len())
            ),
        ));
    }
    for field in state {
        let slot = usize::from(field.id - state_field(0));
        let (position, leaf_field) = layout.slots[slot];
        let leaf_type = compiled[position]
            .declarations
            .fields(100)
            .map_err(route_error)?
            .iter()
            .find(|candidate| candidate.id == leaf_field)
            .map(|candidate| candidate.type_id)
            .ok_or_else(|| {
                refuse(
                    Obligation::StateProjection,
                    format!("slot {slot}"),
                    "names a missing leaf field",
                )
            })?;
        let leaf_domain = domain::leaf_domain(&compiled[position].declarations, leaf_type)
            .map_err(route_error)?;
        if aggregate_domain(field.type_id)? != leaf_domain {
            return Err(refuse(
                Obligation::StateProjection,
                format!("aggregate state field {}", field.id),
                format!("its domain differs from slot {slot}'s leaf field {leaf_field}"),
            ));
        }
    }
    let command = declarations.fields(101).map_err(route_error)?;
    let (target, _) = layout.target;
    let last = i128::try_from(compiled.len().saturating_sub(1)).unwrap_or(i128::MAX);
    let mut seen = BTreeSet::new();
    for field in command {
        seen.insert(field.id);
        if field.id == target {
            if aggregate_domain(field.type_id)? != (LeafDomain::Int { min: 0, max: last }) {
                return Err(refuse(
                    Obligation::CommandRouting,
                    format!("aggregate command field {}", field.id),
                    format!("the target must range over exactly 0..={last}, one value per leaf"),
                ));
            }
            continue;
        }
        let Some((leaf_type, _)) = layout.command.get(&field.id) else {
            return Err(refuse(
                Obligation::CommandRouting,
                format!("aggregate command field {}", field.id),
                "is neither the target nor a leaf command field",
            ));
        };
        let owner = compiled
            .iter()
            .find(|leaf| leaf.declarations.types.contains_key(leaf_type))
            .ok_or_else(|| {
                refuse(
                    Obligation::CommandRouting,
                    "command",
                    "a leaf type is missing",
                )
            })?;
        if aggregate_domain(field.type_id)?
            != domain::leaf_domain(&owner.declarations, *leaf_type).map_err(route_error)?
        {
            return Err(refuse(
                Obligation::CommandRouting,
                format!("aggregate command field {}", field.id),
                "its domain differs from the leaves' command field",
            ));
        }
    }
    let missing: Vec<u16> = layout
        .command
        .keys()
        .copied()
        .chain([target])
        .filter(|id| !seen.contains(id))
        .collect();
    if !missing.is_empty() {
        return Err(refuse(
            Obligation::CommandRouting,
            "aggregate command",
            format!(
                "lacks command fields {}",
                missing
                    .iter()
                    .map(u16::to_string)
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        ));
    }
    let mut environment = BTreeSet::new();
    for field in declarations.fields(102).map_err(route_error)? {
        let field_domain = aggregate_domain(field.type_id)?;
        if let Some((position, leaf_field)) = layout.inputs.get(&field.id) {
            let leaf_domain = compiled[*position]
                .fields(Source::Context)
                .into_iter()
                .find(|candidate| candidate.field == *leaf_field)
                .map(|candidate| candidate.domain);
            if leaf_domain.as_ref() != Some(&field_domain) {
                return Err(refuse(
                    Obligation::EnvironmentInputs,
                    format!("aggregate context field {}", field.id),
                    format!(
                        "its domain differs from leaf position {position}'s interface field {leaf_field}"
                    ),
                ));
            }
            environment.insert(field.id);
            continue;
        }
        let singleton = match &field_domain {
            LeafDomain::Int { min, max } => min == max,
            LeafDomain::Sum { variants, .. } => variants.len() == 1,
            LeafDomain::Bool => false,
        };
        if !singleton {
            return Err(refuse(
                Obligation::EnvironmentInputs,
                format!("aggregate context field {}", field.id),
                "is not an environment input of the composition, and carries information",
            ));
        }
    }
    if let Some(missing) = layout.inputs.keys().find(|id| !environment.contains(id)) {
        return Err(refuse(
            Obligation::EnvironmentInputs,
            "aggregate context",
            format!("lacks environment input field {missing}"),
        ));
    }
    Ok(())
}

/// The aggregate's genesis is every leaf's genesis, slot by slot.
fn genesis(layout: &Layout, compiled: &[Compiled], rules: &Rules) -> Result<(), Refusal> {
    for (slot, (position, field)) in layout.slots.iter().enumerate() {
        let leaf = compiled[*position].rules.genesis.get(field);
        let aggregate = rules.genesis.get(&state_field(slot));
        if leaf.is_none() || leaf != aggregate {
            return Err(refuse(
                Obligation::AggregateGenesis,
                format!("aggregate genesis {}", state_field(slot)),
                format!(
                    "is {}, slot {slot}'s leaf genesis is {}",
                    constant(aggregate),
                    constant(leaf)
                ),
            ));
        }
    }
    Ok(())
}

fn constant(value: Option<&Constant>) -> String {
    match value {
        Some(Constant::Bool(value)) => value.to_string(),
        Some(Constant::Int(value)) => value.to_string(),
        None => "absent".to_owned(),
    }
}

/// No node of the aggregate program can trap on an input in its domains.
fn eager(contract: &Contract<'_>, nodes: &[Node]) -> Result<(), Refusal> {
    let domains: Vec<(i128, i128)> = contract
        .input_domains
        .iter()
        .map(|domain| match domain {
            ScalarDomain::Bool => (0, 1),
            ScalarDomain::Int { min, max } => (i128::from(*min), i128::from(*max)),
        })
        .collect();
    let intervals = term::intervals(nodes, &domains);
    if let Some(index) = intervals.iter().position(Option::is_none) {
        return Err(refuse(
            Obligation::EagerNodesTotal,
            format!("aggregate program node {index}"),
            "can leave the 64-bit range on an input within its domains; every node is evaluated eagerly, so an unused branch could refuse an admitted input",
        ));
    }
    Ok(())
}

/// The canonical term of a successor value.
fn value_term(
    arena: &mut Arena,
    value: &Expr,
    outputs: &[u32],
    input: &dyn Fn(&mut Arena, Source, u16) -> Option<u32>,
) -> Option<u32> {
    match value {
        Expr::Constant(Atom::Bool(value)) => Some(arena.constant(*value)),
        Expr::Constant(Atom::I128(value)) => {
            i128::try_from(*value).ok().map(|value| arena.int(value))
        }
        Expr::Constant(Atom::Sum { variant, .. }) => Some(arena.int(i128::from(*variant))),
        Expr::Constant(_) | Expr::Root(_) => None,
        Expr::Input(source, field) => input(arena, *source, *field),
        Expr::Output(index) => outputs.get(*index).copied(),
    }
}

/// Every input key a term reads.
fn reads(arena: &Arena, id: u32, out: &mut BTreeSet<Key>) {
    match arena.term(id) {
        term::Term::Input(key) => {
            out.insert(key);
        }
        term::Term::Int(_) | term::Term::Bool(_) => {}
        term::Term::Not(a) => reads(arena, a, out),
        term::Term::Add(a, b)
        | term::Term::Sub(a, b)
        | term::Term::Eq(a, b)
        | term::Term::Lt(a, b)
        | term::Term::And(a, b) => {
            reads(arena, a, out);
            reads(arena, b, out);
        }
        term::Term::Select(c, a, b) => {
            reads(arena, c, out);
            reads(arena, a, out);
            reads(arena, b, out);
        }
    }
}

/// An aggregate input's key.
pub(crate) const fn aggregate_key(source: Source, field: u16) -> Key {
    match source {
        Source::State => Key::State(field),
        Source::Command => Key::Command(field),
        Source::Context => Key::Context(field),
    }
}

/// The aggregate key that carries leaf `position`'s input: its state field
/// is its slot, a peer view is the provider's slot in the same pre-state,
/// and an environment input is a context field.
pub(crate) fn leaf_key(
    layout: &Layout,
    position: usize,
    source: Source,
    field: u16,
) -> Option<Key> {
    match source {
        Source::State => layout.owned[position]
            .get(&field)
            .map(|slot| Key::State(state_field(*slot))),
        Source::Command => layout
            .command
            .contains_key(&field)
            .then_some(Key::Command(field)),
        Source::Context => layout.views[position].get(&field).map(|view| match view {
            View::Slot(slot) => Key::State(state_field(*slot)),
            View::Environment(id) => Key::Context(*id),
        }),
    }
}

/// The aggregate's Boolean inputs.
fn boolean_inputs(contract: &Contract<'_>) -> BTreeSet<Key> {
    contract
        .bindings
        .iter()
        .zip(&contract.input_domains)
        .filter(|(_, domain)| matches!(domain, ScalarDomain::Bool))
        .filter_map(|((source, field), _)| field.map(|field| aggregate_key(*source, field)))
        .collect()
}

/// How route `position` reads the aggregate's inputs.
struct Route<'l> {
    layout: &'l Layout,
    position: usize,
    value: i128,
}

impl Route<'_> {
    /// An aggregate input, with the target fixed to the route.
    fn aggregate(&self, arena: &mut Arena, source: Source, field: u16) -> u32 {
        if source == Source::Command && field == self.layout.target.0 {
            return arena.int(self.value);
        }
        arena.input(aggregate_key(source, field))
    }

    /// A leaf input, renamed to the aggregate key that carries it.
    fn leaf(&self, arena: &mut Arena, source: Source, field: u16) -> Option<u32> {
        leaf_key(self.layout, self.position, source, field).map(|key| arena.input(key))
    }
}

/// One route's canonical terms in a shared arena: the aggregate's outputs
/// with the target input fixed to the route, the leaf's outputs with its
/// inputs renamed, and the case pairs of their selection outputs.
pub(crate) struct RouteTerms {
    pub(crate) aggregate_outputs: Vec<u32>,
    pub(crate) leaf_outputs: Vec<u32>,
    pub(crate) cases: BTreeMap<i128, i128>,
}

/// Which obligation a differing term breaks: when the aggregate's value
/// reads everything the leaf's reads except peer views, and reads something
/// else in their place, the view is not the provider's slot in the same
/// pre-state.
fn differs(
    arena: &Arena,
    views: &BTreeSet<Key>,
    place: &str,
    (aggregate, expected): (u32, u32),
    what: String,
) -> Refusal {
    let mut mine = BTreeSet::new();
    let mut theirs = BTreeSet::new();
    reads(arena, aggregate, &mut mine);
    reads(arena, expected, &mut theirs);
    let missing: Vec<&Key> = theirs.difference(&mine).collect();
    let substituted = !missing.is_empty()
        && missing.iter().all(|key| views.contains(key))
        && mine.difference(&theirs).next().is_some();
    if substituted {
        refuse(
            Obligation::SamePreStateView,
            place,
            format!(
                "{what}: the leaf reads its peer view from the provider's slot in the same pre-state, and the aggregate does not"
            ),
        )
    } else {
        refuse(Obligation::DecisionCorrespondence, place, what)
    }
}

/// The peer views of leaf `position`, as aggregate keys.
fn view_keys(layout: &Layout, position: usize) -> BTreeSet<Key> {
    layout.views[position]
        .values()
        .map(|view| match view {
            View::Slot(slot) => Key::State(state_field(*slot)),
            View::Environment(id) => Key::Context(*id),
        })
        .collect()
}

/// Builds route `position`'s terms and pairs the selection outputs' cases.
fn route_terms(
    arena: &mut Arena,
    route: &Route<'_>,
    contract: &Contract<'_>,
    nodes: &[Node],
    leaf: &Contract<'_>,
    place: &str,
) -> Result<RouteTerms, Refusal> {
    let bindings = &contract.bindings;
    let aggregate_ids = term::canonical(arena, nodes, |arena, index| {
        let (source, field) = bindings.get(index)?;
        Some(route.aggregate(arena, *source, (*field)?))
    })
    .ok_or_else(|| {
        refuse(
            Obligation::AssemblyProfile,
            place,
            "an aggregate input is a whole root",
        )
    })?;
    let leaf_nodes: Vec<Node> = leaf.nodes.iter().map(node).collect::<Result<_, _>>()?;
    let leaf_bindings = &leaf.bindings;
    let leaf_ids = term::canonical(arena, &leaf_nodes, |arena, index| {
        let (source, field) = leaf_bindings.get(index)?;
        route.leaf(arena, *source, (*field)?)
    })
    .ok_or_else(|| {
        refuse(
            Obligation::AssemblyProfile,
            place,
            "a leaf input has no aggregate field: a whole root, or a context field without a provider",
        )
    })?;
    let outputs = |ids: &[u32], roots: &[usize]| -> Vec<u32> {
        roots.iter().map(|root| ids[*root]).collect()
    };
    let aggregate_outputs = outputs(&aggregate_ids, &contract.program_roots);
    let leaf_outputs = outputs(&leaf_ids, &leaf.program_roots);
    let (Some(aggregate_selection), Some(leaf_selection)) = (
        aggregate_outputs.first().copied(),
        leaf_outputs.first().copied(),
    ) else {
        return Err(refuse(
            Obligation::AssemblyProfile,
            place,
            "a program has no selection output",
        ));
    };
    let mut cases = BTreeMap::new();
    if !term::same_selection(arena, aggregate_selection, leaf_selection, &mut cases) {
        return Err(differs(
            arena,
            &view_keys(route.layout, route.position),
            place,
            (aggregate_selection, leaf_selection),
            "the aggregate's case selection does not choose the leaf's cases in the leaf's order"
                .to_owned(),
        ));
    }
    Ok(RouteTerms {
        aggregate_outputs,
        leaf_outputs,
        cases,
    })
}

fn route_place(manifest: &Manifest, position: usize) -> String {
    let entry = &manifest.leaves[position];
    format!(
        "aggregate route {position}, leaf {} `{}`",
        entry.id, entry.name
    )
}

/// For each route, the specialized aggregate decides as the leaf. Returns
/// the number of aggregate cases some route selects.
fn decisions(
    layout: &Layout,
    manifest: &Manifest,
    leaves: &[LeafProgram<'_>],
    contract: &Contract<'_>,
    nodes: &[Node],
) -> Result<usize, Refusal> {
    let mut arena = Arena::new(boolean_inputs(contract));
    let mut selected = BTreeSet::new();
    for (position, leaf) in leaves.iter().enumerate() {
        let place = route_place(manifest, position);
        let route = Route {
            layout,
            position,
            value: i128::try_from(position).unwrap_or(i128::MAX),
        };
        let terms = route_terms(&mut arena, &route, contract, nodes, &leaf.contract, &place)?;
        let views = view_keys(layout, position);
        for (aggregate_case, leaf_case) in &terms.cases {
            let index = |value: i128| usize::try_from(value).ok();
            let (Some(mine), Some(theirs)) = (
                index(*aggregate_case).and_then(|at| contract.branches.get(at)),
                index(*leaf_case).and_then(|at| leaf.contract.branches.get(at)),
            ) else {
                return Err(refuse(
                    Obligation::DecisionCorrespondence,
                    &place,
                    "a selected case does not exist",
                ));
            };
            selected.insert(*aggregate_case);
            if mine.class != theirs.class || mine.reason != theirs.reason {
                return Err(refuse(
                    Obligation::DecisionCorrespondence,
                    &place,
                    format!(
                        "aggregate case {aggregate_case} decides {} with reason {}, the leaf's case {leaf_case} {} with reason {}",
                        class_name(mine.class),
                        reason(mine.reason),
                        class_name(theirs.class),
                        reason(theirs.reason)
                    ),
                ));
            }
            if mine.class == Class::Reject {
                if !mine.assignments.is_empty() || !theirs.assignments.is_empty() {
                    return Err(refuse(
                        Obligation::DecisionCorrespondence,
                        &place,
                        "a Reject sets state",
                    ));
                }
                continue;
            }
            let leaf_values: BTreeMap<u16, &Expr> = theirs
                .assignments
                .iter()
                .map(|assignment| (assignment.field, &assignment.value))
                .collect();
            let aggregate_values: BTreeMap<u16, &Expr> = mine
                .assignments
                .iter()
                .map(|assignment| (assignment.field, &assignment.value))
                .collect();
            for (slot, (owner, leaf_field)) in layout.slots.iter().enumerate() {
                let field = state_field(slot);
                let value = aggregate_values
                    .get(&field)
                    .and_then(|value| {
                        value_term(
                            &mut arena,
                            value,
                            &terms.aggregate_outputs,
                            &|arena, source, field| Some(route.aggregate(arena, source, field)),
                        )
                    })
                    .ok_or_else(|| {
                        refuse(
                            Obligation::DecisionCorrespondence,
                            &place,
                            format!("aggregate case {aggregate_case} has no value for slot {slot}"),
                        )
                    })?;
                if *owner == position {
                    let expected = leaf_values
                        .get(leaf_field)
                        .and_then(|value| {
                            value_term(
                                &mut arena,
                                value,
                                &terms.leaf_outputs,
                                &|arena, source, field| route.leaf(arena, source, field),
                            )
                        })
                        .ok_or_else(|| {
                            refuse(
                                Obligation::DecisionCorrespondence,
                                &place,
                                format!(
                                    "leaf case {leaf_case} has no value for field {leaf_field}"
                                ),
                            )
                        })?;
                    if value != expected {
                        return Err(differs(
                            &arena,
                            &views,
                            &place,
                            (value, expected),
                            format!(
                                "aggregate case {aggregate_case} gives slot {slot} another value than the leaf's case {leaf_case} gives field {leaf_field}"
                            ),
                        ));
                    }
                } else if value != arena.input(Key::State(field)) {
                    let owner_entry = &manifest.leaves[*owner];
                    return Err(refuse(
                        Obligation::FrameUnselectedSlots,
                        &place,
                        format!(
                            "aggregate case {aggregate_case} writes slot {slot}, owned by unselected leaf {} `{}`",
                            owner_entry.id, owner_entry.name
                        ),
                    ));
                }
            }
        }
    }
    Ok(selected.len())
}

/// The validator's terms for every route of a validated aggregate, for the
/// differential comparison with the evaluator.
#[cfg(test)]
pub(crate) fn specialization(
    manifest: &Manifest,
    compiled: &[Compiled],
    edges: &[Resolved],
    contract: &Contract<'_>,
) -> Result<(Arena, Vec<RouteTerms>), Refusal> {
    let layout = Layout::of(manifest, compiled, edges)?;
    let nodes: Vec<Node> = contract.nodes.iter().map(node).collect::<Result<_, _>>()?;
    let mut arena = Arena::new(boolean_inputs(contract));
    let mut routes = Vec::with_capacity(compiled.len());
    for (position, leaf) in compiled.iter().enumerate() {
        let leaf = Contract::build(&leaf.declarations, &leaf.rules, leaf.commitment)
            .map_err(route_error)?;
        let route = Route {
            layout: &layout,
            position,
            value: i128::try_from(position).unwrap_or(i128::MAX),
        };
        routes.push(route_terms(
            &mut arena,
            &route,
            contract,
            &nodes,
            &leaf,
            &route_place(manifest, position),
        )?);
    }
    Ok((arena, routes))
}

fn class_name(class: Class) -> &'static str {
    match class {
        Class::Accept => "Accept",
        Class::Reject => "Reject",
        Class::CommittedFailure => "CommittedFailure",
    }
}

fn reason(reason: Option<u32>) -> String {
    reason.map_or_else(|| "none".to_owned(), |reason| reason.to_string())
}

/// Every aggregate law is a StateInvariant on committing decisions, made
/// of per-leaf conjuncts each implied by its leaf's invariant. Returns the
/// conjuncts and the rows checked.
fn laws(
    layout: &Layout,
    compiled: &[Compiled],
    declarations: &Declarations,
    rules: &Rules,
) -> Result<(usize, u64), Refusal> {
    let mut conjuncts_checked = 0usize;
    let mut rows = 0u64;
    for law in &declarations.laws {
        let place = format!("aggregate law {}", law.id);
        if rules.law_kinds.get(&law.id) != Some(&LawKind::StateInvariant)
            || !matches!(law.scope, LawScope::Committing | LawScope::Accept)
        {
            return Err(refuse(
                Obligation::AggregateLaws,
                &place,
                "must be a StateInvariant on committing decisions; the profile keeps no other aggregate law",
            ));
        }
        let mut conjuncts = Vec::new();
        split(&law.formula, &mut conjuncts);
        for conjunct in conjuncts {
            let mut names = Vec::new();
            collect_names(conjunct, &mut names);
            let mut owners = BTreeSet::new();
            let mut renamed = BTreeMap::new();
            for name in &names {
                let slot = name
                    .strip_prefix("post.100.")
                    .and_then(|rest| rest.parse::<u16>().ok())
                    .and_then(|field| field.checked_sub(state_field(0)))
                    .map(usize::from)
                    .filter(|slot| *slot < layout.slots.len())
                    .ok_or_else(|| {
                        refuse(
                            Obligation::AggregateLaws,
                            &place,
                            format!("reads `{name}`; a conjunct may read only slot fields of the post-state"),
                        )
                    })?;
                let (owner, field) = layout.slots[slot];
                owners.insert(owner);
                renamed.insert(name.clone(), format!("post.100.{field}"));
            }
            let Some(owner) = owners.iter().next().copied() else {
                return Err(refuse(
                    Obligation::AggregateLaws,
                    &place,
                    format!(
                        "the conjunct `{}` reads no slot; each conjunct reads one leaf's slots",
                        expr::render(conjunct)
                    ),
                ));
            };
            if owners.len() > 1 {
                return Err(refuse(
                    Obligation::AggregateLaws,
                    &place,
                    format!(
                        "the conjunct `{}` relates the slots of two leaves",
                        expr::render(conjunct)
                    ),
                ));
            }
            let text = expr::render(&rename(conjunct, &renamed));
            let leaf = &compiled[owner];
            rows += implied_by_invariant(leaf, &text, &place)?;
            conjuncts_checked += 1;
        }
    }
    Ok((conjuncts_checked, rows))
}

fn split<'a>(ast: &'a Ast, out: &mut Vec<&'a Ast>) {
    match ast {
        Ast::Binary(Binary::And, left, right) => {
            split(left, out);
            split(right, out);
        }
        other => out.push(other),
    }
}

fn collect_names(ast: &Ast, out: &mut Vec<String>) {
    match ast {
        Ast::Name(name) => out.push(name.clone()),
        Ast::Bool(_) | Ast::Int(_) | Ast::DecisionReason => {}
        Ast::Not(inner) | Ast::Neg(inner) => collect_names(inner, out),
        Ast::Binary(_, left, right) | Ast::Div(_, left, right) => {
            collect_names(left, out);
            collect_names(right, out);
        }
        Ast::MulDiv(_, a, b, d) | Ast::SignedMulDivFloor(a, b, d) => {
            collect_names(a, out);
            collect_names(b, out);
            collect_names(d, out);
        }
        Ast::ProductsGe(arguments) => {
            for a in arguments.iter() {
                collect_names(a, out);
            }
        }
        Ast::Choose(condition, then, otherwise) => {
            collect_names(condition, out);
            collect_names(then, out);
            collect_names(otherwise, out);
        }
    }
}

fn rename(ast: &Ast, names: &BTreeMap<String, String>) -> Ast {
    let recurse = |ast: &Ast| Box::new(rename(ast, names));
    match ast {
        Ast::Name(name) => Ast::Name(names.get(name).cloned().unwrap_or_else(|| name.clone())),
        Ast::Bool(_) | Ast::Int(_) | Ast::DecisionReason => ast.clone(),
        Ast::Not(inner) => Ast::Not(recurse(inner)),
        Ast::Neg(inner) => Ast::Neg(recurse(inner)),
        Ast::Binary(operator, left, right) => Ast::Binary(*operator, recurse(left), recurse(right)),
        Ast::Choose(condition, then, otherwise) => {
            Ast::Choose(recurse(condition), recurse(then), recurse(otherwise))
        }
        Ast::Div(rounding, left, right) => Ast::Div(*rounding, recurse(left), recurse(right)),
        Ast::MulDiv(r, a, b, d) => Ast::MulDiv(*r, recurse(a), recurse(b), recurse(d)),
        Ast::SignedMulDivFloor(a, b, d) => {
            Ast::SignedMulDivFloor(recurse(a), recurse(b), recurse(d))
        }
        Ast::ProductsGe(arguments) => {
            Ast::ProductsGe(arguments.iter().map(|a| *recurse(a)).collect())
        }
    }
}

/// A conjunct, as a formula over a leaf's state, holds on every state of
/// the leaf satisfying its invariant. Returns the states checked.
fn implied_by_invariant(leaf: &Compiled, formula: &str, place: &str) -> Result<u64, Refusal> {
    let ids = |source: Source| -> Vec<u16> {
        leaf.fields(source)
            .iter()
            .map(|field| field.field)
            .collect()
    };
    let (state, command, context) = (
        ids(Source::State),
        ids(Source::Command),
        ids(Source::Context),
    );
    let roots = RootFields {
        state: &state,
        command: &command,
        context: &context,
    };
    let predicate = Predicate::compile(
        &leaf.declarations,
        &leaf.rules.variables,
        formula,
        PredicateForm::State,
        &roots,
    )
    .map_err(|reason| refuse(Obligation::AggregateLaws, place, reason))?;
    let fields = leaf.fields(Source::State);
    let values = fields
        .iter()
        .map(|field| field.domain.values_within(leaf::MAX_LOCAL_ROWS))
        .collect::<Option<Vec<Vec<i128>>>>()
        .ok_or_else(|| {
            refuse(
                Obligation::CompleteEnumeration,
                place,
                "the state domain exceeds the row limit",
            )
        })?;
    let mut rows = 0u64;
    leaf::for_each_tuple(&values, |tuple| {
        rows += 1;
        let record: Vec<c::Field<'static>> = fields
            .iter()
            .zip(tuple)
            .map(|(field, value)| c::Field {
                id: field.field,
                value: leaf::atom(&field.domain, *value),
            })
            .collect();
        match leaf.invariant.on_state(&record) {
            Truth::Holds => {}
            Truth::Fails => return Ok(()),
            other => {
                return Err(refuse(
                    Obligation::DefinedPredicate,
                    place,
                    format!("the invariant {}", other.describe()),
                ));
            }
        }
        match predicate.on_state(&record) {
            Truth::Holds => Ok(()),
            Truth::Fails => Err(refuse(
                Obligation::AggregateLaws,
                place,
                format!("the conjunct `{formula}` fails on a state of the leaf that satisfies its invariant"),
            )
            .with_witness(Value::Object(
                fields
                    .iter()
                    .zip(tuple)
                    .map(|(field, value)| (format!("post.100.{}", field.field), json!(value)))
                    .collect(),
            ))),
            other => Err(refuse(
                Obligation::DefinedPredicate,
                place,
                format!("the conjunct `{formula}` {}", other.describe()),
            )),
        }
    })?;
    Ok(rows)
}

/// Binds the aggregate's Authority, publishes its genesis and evaluates one
/// input per route, so the actual Step usage is observed. Returns the
/// program identity's SHA-256 and the most Steps an evaluation used.
fn observe(
    layout: &Layout,
    routes: usize,
    contract: &Contract<'_>,
    schema: &[u8],
    positions: &[domain::Position],
    rules: &Rules,
) -> Result<([u8; 32], u64), Refusal> {
    let (target, _) = layout.target;
    let framer = Framer::new(positions, contract);
    let mut tuple: Vec<i128> = Vec::with_capacity(positions.len());
    for position in positions {
        let value = match (position.source, position.field) {
            (Source::State, Some(field)) => match rules.genesis.get(&field) {
                Some(Constant::Bool(value)) => i128::from(*value),
                Some(Constant::Int(value)) => i128::try_from(*value).unwrap_or(i128::MAX),
                None => position.domain.least().unwrap_or(0),
            },
            _ => position.domain.least().unwrap_or(0),
        };
        tuple.push(value);
    }
    let target_at = positions
        .iter()
        .position(|position| position.source == Source::Command && position.field == Some(target))
        .ok_or_else(|| {
            refuse(
                Obligation::CommandRouting,
                "aggregate",
                "has no target field",
            )
        })?;
    policy::with_authority(contract, schema, |authority| {
        let frames = framer.frame(&tuple);
        match authority.publish_genesis(&frames[0]) {
            PublicationOutcome::Commit(_) => {}
            _ => {
                return Err(refuse(
                    Obligation::AggregateGenesis,
                    "aggregate genesis",
                    "the aggregate's Authority does not commit its genesis",
                ));
            }
        }
        let mut most = 0u64;
        for route in 0..routes {
            let mut input = tuple.clone();
            input[target_at] = i128::try_from(route).unwrap_or(i128::MAX);
            let frames = framer.frame(&input);
            let outcome = authority.publish(c::Raw {
                state: &frames[0],
                command: &frames[1],
                context: &frames[2],
            });
            let evaluation = outcome.evaluation();
            let used = evaluation.usage().used(V2Resource::Step);
            let metered = match &outcome {
                PublicationOutcome::Refused { error, .. } => matches!(
                    AuthorityRefusal::of(error, evaluation.diagnostics()),
                    AuthorityRefusal::Execution(ExecutionRefusal::Meter(_))
                        | AuthorityRefusal::Law {
                            refusal: LawRefusal::Meter(_),
                            ..
                        }
                ),
                _ => false,
            };
            if metered || used > contract.budgets.step {
                return Err(refuse(
                    Obligation::MeterCoversEagerWork,
                    format!("aggregate route {route}"),
                    format!(
                        "the shared meter refused after {used} Steps; the budget is {}",
                        contract.budgets.step
                    ),
                ));
            }
            most = most.max(used);
        }
        Ok((sha256(authority.identity()), most))
    })
    .map_err(route_error)?
}
