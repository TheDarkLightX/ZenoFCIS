//! The contract as Rust source: `src/v2_contract.rs`, formatted as rustfmt
//! formats it.

use std::fmt::Write as _;

use super::ContractError;
use super::declarations::{Form, Leaf, Source};
use super::expr::Rounding;
use super::graph::{Atom, LawOp, Observation, ScalarOp};
use super::layout::{Syntax, assignment, statement};
use super::model::{
    Assignment, Branch, ChannelSchema, Contract, Domain, Expr, InputLeaf, Law, Plan, RootSchema,
    ScalarDomain,
};

const HEADER: &str = "\
// Generated declarative data. Review v2/policy.json and project.zeno.
// Regenerate or check with `zeno-fcis generate contract`; no runtime mapper.
extern crate alloc;
use alloc::{vec, vec::Vec};
use zeno_fcis_synthesis::finite::{Domain as ScalarDomain, Op, V2ScalarProgram};
";

const IMPORTS_WITH_VARIANTS: &str = "\
use zeno_fcis_synthesis::finite::{
    V2InputField as InputField, V2InputLeaf as InputLeaf, V2InputVariant as InputVariant,
    V2Resource as Resource, canonical_v2::schema as s, v2_authority as authority,
    v2_catalog as catalog, v2_composition as c, v2_laws as l, v2_zero_limits,
};
";

const IMPORTS: &str = "\
use zeno_fcis_synthesis::finite::{
    V2InputField as InputField, V2InputLeaf as InputLeaf, V2Resource as Resource,
    canonical_v2::schema as s, v2_authority as authority, v2_catalog as catalog,
    v2_composition as c, v2_laws as l, v2_zero_limits,
};
";

/// `{policy}` is the policy file the version includes.
const ORIGINALS: &str = "\
/// Exact actual original schema bytes.
pub const ORIGINAL_SCHEMA: &[u8] = include_bytes!(\"../v2/schema.zcve\");
/// Complete reviewed library-encoded policy.
pub const ORIGINAL_POLICY: &[u8] = include_bytes!(\"{policy}\");
";

const LINEAGE_DOC: &str = "\
/// Every contract version's checked catalog, oldest first and this one last,
/// for a store upgrade or a lineage open. Each binding checks that version's
/// complete retained schema and policy bytes.
pub fn with_lineage<R>(
    f: impl FnOnce(&[&catalog::BoundCatalog<'_>]) -> R,
) -> Result<R, catalog::Failure> {
";

/// A contract with adoptions also passes their receipt digests.
const ADOPTED_LINEAGE_DOC: &str = "\
/// Every contract version's checked catalog, oldest first and this one last,
/// and `ADOPTION_RECEIPTS`, for a store upgrade or a lineage open. Each
/// binding checks that version's complete retained schema and policy bytes.
pub fn with_lineage<R>(
    f: impl FnOnce((&[&catalog::BoundCatalog<'_>], &[&str])) -> R,
) -> Result<R, catalog::Failure> {
";

const RECEIPTS_DOC: &str = "\
/// The SHA-256 of each adoption's `transform` receipt, oldest first: adoption
/// `k`'s receipt compares version `k`'s decision program with version
/// `k + 1`'s. `zeno-fcis generate contract` replayed every one before writing
/// this list, and a program-successor store upgrade binds the ones it spans.
";

/// Which version of a contract to render and where it sits in the lineage.
#[derive(Clone, Copy, Debug)]
pub(super) struct Options<'a> {
    /// The policy file relative to `src/`: `../v2/policy.zcve` for the
    /// current version, `../v2/policy_v{k}.zcve` for superseded version `k`.
    pub(super) policy: &'a str,
    /// Position in the lineage, from 1.
    pub(super) version: u32,
    /// The current version's adoption receipt digests in hexadecimal, one per
    /// superseded version, which the file declares as modules `v1`..; none
    /// for a superseded version, which is a leaf.
    pub(super) receipts: &'a [String],
}

const BINDING: &str = "\
/// Ordered refusal at checked catalog or private Authority construction.
#[derive(Debug)]
#[non_exhaustive]
pub enum BindFailure {
    /// Complete original schema/policy correspondence refused.
    Catalog(catalog::Failure),
    /// Private library source binding refused.
    Authority(authority::Refusal),
}
/// Sole supplied production constructor; identity comes from the checked library.
pub fn checked_authority<'a>(
    descriptor: &'a c::Descriptor<'a>,
) -> Result<authority::Authority<'a>, BindFailure> {
    let catalog = checked_catalog(descriptor).map_err(BindFailure::Catalog)?;
    authority::bind(&catalog).map_err(BindFailure::Authority)
}
";

pub(super) fn source(
    contract: &Contract<'_>,
    options: Options<'_>,
) -> Result<String, ContractError> {
    let declarations = contract.declarations;
    let mut text = String::from(HEADER);
    let any_variants = declarations
        .types
        .values()
        .any(|declared| matches!(declared.form, Form::Sum(_)));
    text.push_str(if any_variants {
        IMPORTS_WITH_VARIANTS
    } else {
        IMPORTS
    });
    text.push_str(&ORIGINALS.replace("{policy}", options.policy));
    item(
        &mut text,
        Some("Complete original named schema description."),
        "pub const DESCRIPTION: s::Description<'static> =",
        &description(contract),
    )?;
    item(
        &mut text,
        Some("Original root and schema commitments and complete wire-size limits."),
        "pub const FRAMING: c::Framing =",
        &framing(contract),
    )?;
    item(
        &mut text,
        Some("Exact channel to original destination and payload type links."),
        "pub const CHANNEL_ROOTS: &[(u32, u32, u32)] =",
        &Syntax::slice(
            contract
                .channels
                .iter()
                .map(|channel| {
                    Syntax::tuple(vec![
                        Syntax::literal(channel.id),
                        Syntax::literal(channel.destination_type),
                        Syntax::literal(channel.payload_type),
                    ])
                })
                .collect(),
        ),
    )?;
    item(
        &mut text,
        Some("The genesis state law 990 requires, field by field."),
        "pub const GENESIS: &[c::Field<'static>] =",
        &Syntax::slice(
            contract
                .genesis
                .iter()
                .map(|(field, value)| {
                    record(
                        "c::Field",
                        vec![("id", Syntax::literal(field)), ("value", atom("c", value))],
                    )
                })
                .collect(),
        ),
    )?;
    item(
        &mut text,
        Some("Complete ordered decisions selected only by actual graph output."),
        "pub const BRANCHES: &[c::Branch<'static>] =",
        &Syntax::slice(contract.branches.iter().enumerate().map(branch).collect()),
    )?;
    for law in &contract.laws {
        item(
            &mut text,
            None,
            &format!("const LAW_{}: &[l::Op<'static>] =", law.id),
            &Syntax::slice(law.nodes.iter().map(law_op).collect()),
        )?;
    }
    item(
        &mut text,
        Some("Original scoped laws plus independent structural requirements."),
        "pub const LAWS: &[l::Law<'static>] =",
        &Syntax::slice(contract.laws.iter().map(law).collect()),
    )?;
    item(
        &mut text,
        Some("No declared law is optional at descriptor admission."),
        "pub const REQUIRED: &[u32] =",
        &Syntax::slice(
            contract
                .laws
                .iter()
                .map(|law| Syntax::literal(law.id))
                .collect(),
        ),
    )?;
    let members = ["state", "command", "context"]
        .iter()
        .zip(&contract.roots)
        .map(|(name, root)| match root {
            RootSchema::Record(_) => format!("    {name}: Vec<InputField>,\n"),
            RootSchema::Leaf(_) => format!("    {name}: InputLeaf,\n"),
        })
        .collect::<String>();
    let _ = write!(
        text,
        "/// Owns declarative input/output type tables; evaluation stays in the library.\n\
         pub struct Contract {{\n{members}    output_types: Vec<InputLeaf>,\n}}\n\
         impl Default for Contract {{\n    fn default() -> Self {{\n        Self::new()\n    }}\n}}\n\
         impl Contract {{\n    /// Allocate only fixed declarative type tables.\n    #[must_use]\n    \
         pub fn new() -> Self {{\n        {}\n    }}\n    \
         /// Borrow the full raw-input, complete-decision and law contract.\n    #[must_use]\n    \
         pub fn descriptor(&self) -> c::Descriptor<'_> {{\n        {}\n    }}\n}}\n",
        body(&new(contract))?,
        body(&descriptor(contract))?,
    );
    let (types, fields, variants) = contract.schema_counts();
    let _ = write!(
        text,
        "/// Exact complete schema/policy correspondence precedes Authority construction.\n\
         pub fn checked_catalog<'a>(\n    descriptor: &'a c::Descriptor<'a>,\n\
         ) -> Result<catalog::BoundCatalog<'a>, catalog::Failure> {{\n    \
         catalog::bind_original(\n        ORIGINAL_SCHEMA,\n        &DESCRIPTION,\n        \
         catalog::Limits {{\n            schema: s::Limits {{\n                \
         bytes: ORIGINAL_SCHEMA.len() as u64,\n                types: {types},\n                \
         fields: {fields},\n                variants: {variants},\n            }},\n            \
         contract_bytes: ORIGINAL_POLICY.len() as u64,\n        }},\n        ORIGINAL_POLICY,\n        \
         descriptor,\n        &FRAMING,\n        CHANNEL_ROOTS,\n    )\n}}\n"
    );
    text.push_str(BINDING);
    lineage(&mut text, options)?;
    Ok(text)
}

/// The version number, the superseded versions as modules, the adoption
/// receipts and `with_lineage`.
fn lineage(text: &mut String, options: Options<'_>) -> Result<(), ContractError> {
    let _ = write!(
        text,
        "/// Position in this application's contract lineage: 1 before any adoption.\n\
         pub const VERSION: u32 = {};\n",
        options.version
    );
    let previous = options.receipts.len();
    for version in 1..=previous {
        let _ = write!(
            text,
            "/// Contract version {version}, superseded by adoption {version} in v2/policy.json.\n\
             #[path = \"v2_contract_v{version}.rs\"]\n\
             pub mod v{version};\n"
        );
    }
    if previous == 0 {
        text.push_str(LINEAGE_DOC);
    } else {
        text.push_str(RECEIPTS_DOC);
        item(
            text,
            None,
            "pub const ADOPTION_RECEIPTS: &[&str] =",
            &Syntax::slice(
                options
                    .receipts
                    .iter()
                    .map(|receipt| Syntax::literal(format!("{receipt:?}")))
                    .collect(),
            ),
        )?;
        text.push_str(ADOPTED_LINEAGE_DOC);
    }
    let mut catalogs = Vec::new();
    for version in 1..=previous {
        let _ = write!(
            text,
            "    let contract_{version} = v{version}::Contract::new();\n    \
             let descriptor_{version} = contract_{version}.descriptor();\n    \
             let catalog_{version} = v{version}::checked_catalog(&descriptor_{version})?;\n"
        );
        catalogs.push(Syntax::reference(Syntax::path(format!(
            "catalog_{version}"
        ))));
    }
    text.push_str(
        "    let contract = Contract::new();\n    \
         let descriptor = contract.descriptor();\n    \
         let catalog = checked_catalog(&descriptor)?;\n",
    );
    catalogs.push(Syntax::reference(Syntax::path("catalog")));
    let lineage = if previous == 0 {
        Syntax::slice(catalogs)
    } else {
        Syntax::tuple(vec![
            Syntax::slice(catalogs),
            Syntax::path("ADOPTION_RECEIPTS"),
        ])
    };
    let result = Syntax::call("Ok", vec![Syntax::call("f", vec![lineage])]);
    let tail = statement(4, &result).ok_or_else(|| unrenderable("with_lineage"))?;
    let _ = writeln!(text, "    {tail}\n}}");
    Ok(())
}

fn item(
    text: &mut String,
    doc: Option<&str>,
    lhs: &str,
    rhs: &Syntax,
) -> Result<(), ContractError> {
    if let Some(doc) = doc {
        let _ = writeln!(text, "/// {doc}");
    }
    let rendered = assignment(lhs, rhs).ok_or_else(|| unrenderable(lhs))?;
    text.push_str(&rendered);
    text.push('\n');
    Ok(())
}

/// A function body's tail expression, re-indented after its first line.
fn body(expression: &Syntax) -> Result<String, ContractError> {
    statement(8, expression).ok_or_else(|| unrenderable("function body"))
}

fn unrenderable(what: &str) -> ContractError {
    ContractError::new(
        "src/v2_contract.rs",
        format!("`{what}` does not fit the 100-column layout"),
    )
}

fn record(path: &str, fields: Vec<(&'static str, Syntax)>) -> Syntax {
    Syntax::record(path, fields)
}

fn description(contract: &Contract<'_>) -> Syntax {
    let declarations = contract.declarations;
    let definitions = declarations
        .types
        .iter()
        .map(|(id, declared)| {
            let kind = match &declared.form {
                Form::Record(fields) => Syntax::call(
                    "s::Kind::Record",
                    vec![Syntax::slice(
                        fields
                            .iter()
                            .map(|field| {
                                record(
                                    "s::Field",
                                    vec![
                                        ("id", Syntax::literal(field.id)),
                                        ("name", Syntax::bytes(field.name.as_bytes())),
                                        ("type_id", Syntax::literal(field.type_id)),
                                    ],
                                )
                            })
                            .collect(),
                    )],
                ),
                Form::Sum(variants) => Syntax::call(
                    "s::Kind::Sum",
                    vec![Syntax::slice(
                        variants
                            .iter()
                            .map(|variant| {
                                record(
                                    "s::Variant",
                                    vec![
                                        ("id", Syntax::literal(variant.id)),
                                        ("name", Syntax::bytes(variant.name.as_bytes())),
                                    ],
                                )
                            })
                            .collect(),
                    )],
                ),
                Form::Leaf(Leaf::Bool) => Syntax::path("s::Kind::Bool"),
                Form::Leaf(Leaf::I128 { min, max }) => bounds("s::Kind::I128", min, max),
                Form::Leaf(Leaf::Text { min, max }) => bounds("s::Kind::Text", min, max),
            };
            record(
                "s::Definition",
                vec![
                    ("id", Syntax::literal(id)),
                    ("name", Syntax::bytes(declared.name.as_bytes())),
                    ("kind", kind),
                ],
            )
        })
        .collect();
    record(
        "s::Description",
        vec![
            ("profile", Syntax::bytes(declarations.profile.as_bytes())),
            ("version", Syntax::literal(1)),
            ("root", Syntax::literal(100)),
            ("definitions", Syntax::slice(definitions)),
        ],
    )
}

fn bounds(path: &str, min: impl ToString, max: impl ToString) -> Syntax {
    record(
        path,
        vec![
            ("min", Syntax::literal(min.to_string())),
            ("max", Syntax::literal(max.to_string())),
        ],
    )
}

fn framing(contract: &Contract<'_>) -> Syntax {
    let frame = |index: usize| {
        record(
            "c::FrameBinding",
            vec![
                ("root", Syntax::literal(super::declarations::ROOTS[index].1)),
                (
                    "schema",
                    Syntax::array(contract.commitment.iter().map(Syntax::literal).collect()),
                ),
                ("max_bytes", Syntax::literal(contract.frame_bytes[index])),
            ],
        )
    };
    record(
        "c::Framing",
        vec![
            ("state", frame(0)),
            ("command", frame(1)),
            ("context", frame(2)),
        ],
    )
}

fn branch((code, branch): (usize, &Branch)) -> Syntax {
    record(
        "c::Branch",
        vec![
            ("code", Syntax::literal(code)),
            (
                "class",
                Syntax::path(format!("c::Class::{}", branch.class.name())),
            ),
            (
                "reason",
                match branch.reason {
                    Some(reason) => Syntax::call("Some", vec![Syntax::literal(reason)]),
                    None => Syntax::path("None"),
                },
            ),
            (
                "assignments",
                Syntax::slice(branch.assignments.iter().map(assignment_plan).collect()),
            ),
            ("effects", Syntax::slice(Vec::new())),
            (
                "outbox",
                Syntax::slice(branch.outbox.iter().map(delivery).collect()),
            ),
        ],
    )
}

fn assignment_plan(assignment: &Assignment) -> Syntax {
    record(
        "c::Assignment",
        vec![
            ("field", Syntax::literal(assignment.field)),
            ("value", expr(&assignment.value)),
            ("domain", domain(&assignment.domain)),
        ],
    )
}

fn delivery(plan: &Plan) -> Syntax {
    let constant = |atom: &Atom| Syntax::call("c::Expr::Constant", vec![self::atom("c", atom)]);
    record(
        "c::DeliveryPlan",
        vec![
            ("ordinal", Syntax::literal(plan.ordinal)),
            ("channel", Syntax::literal(plan.channel)),
            ("when", constant(&Atom::Bool(true))),
            ("destination", constant(&plan.destination)),
            (
                "payload",
                Syntax::slice(
                    plan.payload
                        .iter()
                        .map(|(field, value)| {
                            record(
                                "c::PayloadField",
                                vec![("field", Syntax::literal(field)), ("value", expr(value))],
                            )
                        })
                        .collect(),
                ),
            ),
            ("idempotency", constant(&Atom::U128(plan.idempotency))),
        ],
    )
}

fn source_path(source: Source) -> Syntax {
    Syntax::path(match source {
        Source::State => "c::Source::State",
        Source::Command => "c::Source::Command",
        Source::Context => "c::Source::Context",
    })
}

fn expr(value: &Expr) -> Syntax {
    match value {
        Expr::Constant(value) => Syntax::call("c::Expr::Constant", vec![atom("c", value)]),
        Expr::Root(source) => Syntax::call("c::Expr::Root", vec![source_path(*source)]),
        Expr::Input(source, field) => Syntax::call(
            "c::Expr::Input",
            vec![source_path(*source), Syntax::literal(field)],
        ),
        Expr::Output(index) => Syntax::call("c::Expr::Output", vec![Syntax::literal(index)]),
    }
}

/// An atom in the `c` (decision) or `l` (law) namespace.
fn atom(namespace: &str, value: &Atom) -> Syntax {
    let path = |name: &str| format!("{namespace}::Atom::{name}");
    match value {
        Atom::Bool(value) => Syntax::call(path("Bool"), vec![Syntax::literal(value)]),
        Atom::I128(value) => Syntax::call(path("I128"), vec![Syntax::literal(value)]),
        Atom::U128(value) => Syntax::call(path("U128"), vec![Syntax::literal(value)]),
        Atom::Sum { type_id, variant } => record(
            &path("Sum"),
            vec![
                ("type_id", Syntax::literal(type_id)),
                ("variant", Syntax::literal(variant)),
            ],
        ),
        Atom::Text(value) => Syntax::call(path("Text"), vec![Syntax::bytes(value)]),
    }
}

fn domain(value: &Domain) -> Syntax {
    match value {
        Domain::Bool => Syntax::path("c::Domain::Bool"),
        Domain::Text => Syntax::path("c::Domain::Text"),
        Domain::I128 { min, max } => bounds("c::Domain::I128", min, max),
        Domain::Sum { type_id, variants } => record(
            "c::Domain::Sum",
            vec![
                ("type_id", Syntax::literal(type_id)),
                (
                    "variants",
                    Syntax::slice(variants.iter().map(Syntax::literal).collect()),
                ),
            ],
        ),
    }
}

fn observation(value: Observation) -> Syntax {
    let name = match value {
        Observation::PreRoot => "PreRoot".to_owned(),
        Observation::CommandRoot => "CommandRoot".to_owned(),
        Observation::ContextRoot => "ContextRoot".to_owned(),
        Observation::PostRoot => "PostRoot".to_owned(),
        Observation::Pre(field) => format!("Pre({field})"),
        Observation::Command(field) => format!("Command({field})"),
        Observation::Context(field) => format!("Context({field})"),
        Observation::Post(field) => format!("Post({field})"),
        Observation::Initial(field) => format!("Initial({field})"),
        Observation::Class => "Class".to_owned(),
        Observation::HasReason => "HasReason".to_owned(),
        Observation::Reason => "Reason".to_owned(),
        Observation::PostLength => "PostLength".to_owned(),
        Observation::PatchLength => "PatchLength".to_owned(),
        Observation::EffectLength => "EffectLength".to_owned(),
        Observation::OutboxLength => "OutboxLength".to_owned(),
        Observation::OutboxOrdinal(index) => format!("OutboxOrdinal({index})"),
        Observation::OutboxChannel(index) => format!("OutboxChannel({index})"),
        Observation::OutboxDestination(index) => format!("OutboxDestination({index})"),
        Observation::OutboxPayload(index, field) => format!("OutboxPayload({index}, {field})"),
        Observation::OutboxIdempotency(index) => format!("OutboxIdempotency({index})"),
    };
    // Short observation calls never break, so one atom renders them.
    Syntax::path(format!("l::Observation::{name}"))
}

fn indexes(path: &str, values: &[usize]) -> Syntax {
    Syntax::call(path, values.iter().map(Syntax::literal).collect())
}

fn law_op(op: &LawOp) -> Syntax {
    match op {
        LawOp::Literal(value) => Syntax::call("l::Op::Literal", vec![atom("l", value)]),
        LawOp::Observe(value) => Syntax::call("l::Op::Observe", vec![observation(*value)]),
        LawOp::ObserveWhen(guard, value, default) => Syntax::call(
            "l::Op::ObserveWhen",
            vec![
                Syntax::literal(guard),
                observation(*value),
                atom("l", default),
            ],
        ),
        LawOp::Add(a, b) => indexes("l::Op::Add", &[*a, *b]),
        LawOp::Sub(a, b) => indexes("l::Op::Sub", &[*a, *b]),
        LawOp::Mul(a, b) => indexes("l::Op::Mul", &[*a, *b]),
        LawOp::Div(rounding, a, b) => Syntax::call(
            "l::Op::Div",
            vec![
                Syntax::path(match rounding {
                    Rounding::Floor => "l::Division::Floor",
                    Rounding::Ceil => "l::Division::Ceil",
                }),
                Syntax::literal(a),
                Syntax::literal(b),
            ],
        ),
        LawOp::ToI128(a) => indexes("l::Op::ToI128", &[*a]),
        LawOp::Eq(a, b) => indexes("l::Op::Eq", &[*a, *b]),
        LawOp::Lt(a, b) => indexes("l::Op::Lt", &[*a, *b]),
        LawOp::And(a, b) => indexes("l::Op::And", &[*a, *b]),
        LawOp::Not(a) => indexes("l::Op::Not", &[*a]),
        LawOp::Select(c, a, b) => indexes("l::Op::Select", &[*c, *a, *b]),
    }
}

fn law(law: &Law) -> Syntax {
    record(
        "l::Law",
        vec![
            ("id", Syntax::literal(law.id)),
            (
                "kind",
                Syntax::path(format!("l::Kind::{}", law.kind.name())),
            ),
            (
                "scope",
                Syntax::path(format!("l::Scope::{}", scope_name(law.scope))),
            ),
            ("genesis", Syntax::literal(law.genesis)),
            (
                "program",
                record(
                    "l::Program",
                    vec![
                        ("nodes", Syntax::path(format!("LAW_{}", law.id))),
                        ("root", Syntax::literal(law.root)),
                    ],
                ),
            ),
        ],
    )
}

pub(super) fn scope_name(scope: zeno_fcis_spec::LawScope) -> &'static str {
    use zeno_fcis_spec::LawScope;
    match scope {
        LawScope::Always => "Always",
        LawScope::Accept => "Accept",
        LawScope::Reject => "Reject",
        LawScope::CommittedFailure => "CommittedFailure",
        LawScope::Committing => "Committing",
    }
}

fn input_leaf(leaf: &InputLeaf) -> Syntax {
    match leaf {
        InputLeaf::Bool => Syntax::path("InputLeaf::Bool"),
        InputLeaf::I128 { min, max } => bounds("InputLeaf::I128", min, max),
        InputLeaf::Sum { type_id, variants } => record(
            "InputLeaf::Sum",
            vec![
                ("type_id", Syntax::literal(type_id)),
                (
                    "min",
                    Syntax::literal(variants.first().copied().unwrap_or(0)),
                ),
                (
                    "max",
                    Syntax::literal(variants.last().copied().unwrap_or(0)),
                ),
                (
                    "variants",
                    Syntax::vec(
                        variants
                            .iter()
                            .map(|id| {
                                record(
                                    "InputVariant",
                                    vec![
                                        ("id", Syntax::literal(id)),
                                        ("code", Syntax::literal(id)),
                                    ],
                                )
                            })
                            .collect(),
                    ),
                ),
            ],
        ),
    }
}

fn new(contract: &Contract<'_>) -> Syntax {
    let mut fields: Vec<(&'static str, Syntax)> = ["state", "command", "context"]
        .into_iter()
        .zip(&contract.roots)
        .map(|(name, root)| {
            let value = match root {
                RootSchema::Record(fields) => Syntax::vec(
                    fields
                        .iter()
                        .map(|(id, leaf)| {
                            record(
                                "InputField",
                                vec![("id", Syntax::literal(id)), ("leaf", input_leaf(leaf))],
                            )
                        })
                        .collect(),
                ),
                RootSchema::Leaf(leaf) => input_leaf(leaf),
            };
            (name, value)
        })
        .collect();
    fields.push((
        "output_types",
        Syntax::vec(contract.output_types.iter().map(input_leaf).collect()),
    ));
    record("Self", fields)
}

fn scalar_domain(domain: &ScalarDomain) -> Syntax {
    match domain {
        ScalarDomain::Bool => Syntax::path("ScalarDomain::Bool"),
        ScalarDomain::Int { min, max } => bounds("ScalarDomain::Int", min, max),
    }
}

fn scalar_op(op: &ScalarOp) -> Syntax {
    match op {
        ScalarOp::Input(index) => indexes("Op::Input", &[*index]),
        ScalarOp::Int(value) => Syntax::call("Op::Int", vec![Syntax::literal(value)]),
        ScalarOp::Bool(value) => Syntax::call("Op::Bool", vec![Syntax::literal(value)]),
        ScalarOp::Add(a, b) => indexes("Op::Add", &[*a, *b]),
        ScalarOp::Sub(a, b) => indexes("Op::Sub", &[*a, *b]),
        ScalarOp::Eq(a, b) => indexes("Op::Eq", &[*a, *b]),
        ScalarOp::Lt(a, b) => indexes("Op::Lt", &[*a, *b]),
        ScalarOp::And(a, b) => indexes("Op::And", &[*a, *b]),
        ScalarOp::Not(a) => indexes("Op::Not", &[*a]),
        ScalarOp::Select(c, a, b) => indexes("Op::Select", &[*c, *a, *b]),
    }
}

fn channel(channel: &ChannelSchema) -> Syntax {
    record(
        "c::Channel",
        vec![
            ("id", Syntax::literal(channel.id)),
            ("destination", domain(&channel.destination)),
            (
                "payload",
                Syntax::slice(
                    channel
                        .payload
                        .iter()
                        .map(|(field, value)| {
                            record(
                                "c::TypedField",
                                vec![("field", Syntax::literal(field)), ("domain", domain(value))],
                            )
                        })
                        .collect(),
                ),
            ),
            ("idempotency", bounds("c::Domain::U128", 0, 0)),
        ],
    )
}

fn descriptor(contract: &Contract<'_>) -> Syntax {
    let mut fields: Vec<(&'static str, Syntax)> = ["state", "command", "context"]
        .into_iter()
        .zip(&contract.roots)
        .map(|(name, root)| {
            let schema = match root {
                RootSchema::Record(_) => "c::Schema::Record",
                RootSchema::Leaf(_) => "c::Schema::Leaf",
            };
            (
                name,
                Syntax::call(
                    schema,
                    vec![Syntax::reference(Syntax::path(format!("self.{name}")))],
                ),
            )
        })
        .collect();
    let program = record(
        "V2ScalarProgram",
        vec![
            (
                "inputs",
                Syntax::slice(contract.input_domains.iter().map(scalar_domain).collect()),
            ),
            (
                "outputs",
                Syntax::slice(contract.output_domains.iter().map(scalar_domain).collect()),
            ),
            (
                "nodes",
                Syntax::slice(contract.nodes.iter().map(scalar_op).collect()),
            ),
            (
                "roots",
                Syntax::slice(contract.program_roots.iter().map(Syntax::literal).collect()),
            ),
        ],
    );
    let bindings = contract
        .bindings
        .iter()
        .map(|(source, field)| {
            record(
                "c::Binding",
                vec![
                    ("source", source_path(*source)),
                    (
                        "selector",
                        match field {
                            Some(field) => {
                                Syntax::call("c::Selector::Field", vec![Syntax::literal(field)])
                            }
                            None => Syntax::path("c::Selector::Root"),
                        },
                    ),
                ],
            )
        })
        .collect();
    let reasons = contract
        .reasons
        .iter()
        .map(|(id, class)| {
            record(
                "c::Reason",
                vec![
                    ("id", Syntax::literal(id)),
                    ("class", Syntax::path(format!("c::Class::{}", class.name()))),
                ],
            )
        })
        .collect();
    let budgets = contract.budgets;
    let limit = |resource: &str, amount: u64| {
        Syntax::call(
            ".with_limit",
            vec![
                Syntax::path(format!("Resource::{resource}")),
                Syntax::literal(amount),
            ],
        )
    };
    let limits = Syntax::chain(
        Syntax::call("v2_zero_limits", Vec::new()),
        vec![
            limit("Read", budgets.read),
            limit("Write", budgets.write),
            limit("Candidate", 1),
            limit("Effect", 1),
            limit("Byte", budgets.byte),
            limit("WitnessByte", 0),
            limit("Depth", 0),
            limit("Step", budgets.step),
        ],
    );
    fields.extend([
        ("program", program),
        ("bindings", Syntax::slice(bindings)),
        (
            "output_types",
            Syntax::reference(Syntax::path("self.output_types")),
        ),
        ("decision_output", Syntax::literal(0)),
        ("branches", Syntax::path("BRANCHES")),
        ("reasons", Syntax::slice(reasons)),
        (
            "channels",
            Syntax::slice(contract.channels.iter().map(channel).collect()),
        ),
        ("laws", Syntax::path("LAWS")),
        ("required", Syntax::path("REQUIRED")),
        ("limits", limits),
    ]);
    record("c::Descriptor", fields)
}
