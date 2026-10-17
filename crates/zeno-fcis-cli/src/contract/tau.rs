//! Optional finite Tau queries. The compiler and the Authority are the native
//! contract route; this module only translates their data and observations.

use std::collections::BTreeMap;

use serde_json::{Value, json};
use zeno_fcis_synthesis::finite::{Op, Program};

use super::declarations::Declarations;
use super::expr::{self, Ast, Binary};
use super::graph::Atom;
use super::model::{Contract, Expr};
use super::review::domain::{self, LeafDomain, Position};
use super::review::evaluate::{self, Framer, Outcome};
use super::rules::{Class, Rules};
use super::{ContractError, policy, schema, schema_commitment};
use crate::symbolic::program::{self, Encoded};
use crate::symbolic::smt::{Script, Sort, Term, Values};
use crate::transform::sha256_hex;

const MAX_QUERY_BYTES: usize = 1 << 20;

fn refused(reason: impl Into<String>) -> ContractError {
    ContractError::new("Tau finite profile", reason)
}

/// Export per-rule compiler comparisons and real Authority decisions. These
/// are optional independent checks, with no checked-proof or authority status.
pub(crate) fn export(
    project: &str,
    source: &str,
    max_tuples: u64,
    samples: usize,
) -> Result<Value, ContractError> {
    let rules = Rules::read(source)?;
    if !rules.adoptions.is_empty() || !rules.evolutions.is_empty() {
        return Err(refused(
            "adopted or evolved lineages are outside this Tau profile",
        ));
    }
    let declarations = Declarations::read(project, &rules.leaf_bindings)?;
    let schema = schema::encode(&declarations)?;
    let contract = Contract::build(&declarations, &rules, schema_commitment(&schema)?)?;
    let positions = domain::positions(&declarations)?;
    let native_program = contract.program()?;
    ranges(&native_program)?;
    let bounds = positions
        .iter()
        .map(|position| {
            let bounds = match &position.domain {
                LeafDomain::Bool => (0, 1),
                LeafDomain::Int { min, max } => (i128::from(*min), i128::from(*max)),
                LeafDomain::Sum { variants, .. } => (
                    i128::from(*variants.iter().min().unwrap_or(&0)),
                    i128::from(*variants.iter().max().unwrap_or(&0)),
                ),
            };
            (position.name.clone(), bounds)
        })
        .collect::<BTreeMap<_, _>>();
    for case in &rules.cases {
        for expression in std::iter::once(&case.when).chain(case.post.values()).chain(
            case.outbox
                .iter()
                .flat_map(|delivery| delivery.payload.values()),
        ) {
            ast_ranges(
                &expr::expand(expression, &rules.variables).map_err(refused)?,
                &bounds,
            )?;
        }
    }
    let set = domain::input_set(&positions, &rules, &declarations, &[], max_tuples)?;
    if set.len() == 0 {
        return Err(refused("there are no admitted review inputs"));
    }
    let sample_count = samples.min(set.len());
    let mut queries = Vec::new();
    for (index, case) in rules.cases.iter().enumerate() {
        let mut script = Script::default();
        let input = inputs(&mut script, &positions, None);
        let encoded = program::encode(
            &mut script,
            "p",
            program::Program::of(&native_program),
            &input
                .iter()
                .map(|(term, sort)| wire(term.clone(), *sort))
                .collect::<Vec<_>>(),
        )
        .map_err(|error| refused(error.reason()))?;
        let names = names(&positions, &input);
        let expand = |ast: &Ast| expr::expand(ast, &rules.variables).map_err(refused);
        let mut guard = Vec::new();
        for prior in &rules.cases[..index] {
            guard.push(Term::not(boolean(ast(&expand(&prior.when)?, &names)?)));
        }
        guard.push(boolean(ast(&expand(&case.when)?, &names)?));
        let mut matching = vec![
            encoded.ok(),
            Term::eq(encoded.outputs[0].clone(), Term::Int(index as i128)),
        ];
        let branch = &contract.branches[index];
        matching.push(Term::Bool(
            branch.class == case.class
                && branch.reason == case.reason
                && branch.assignments.len() == case.post.len()
                && branch.outbox.len() == case.outbox.len(),
        ));
        for assignment in &branch.assignments {
            let expected = case
                .post
                .get(&assignment.field)
                .ok_or_else(|| refused("compiler assignment has no authored state expression"))?;
            let expected = ast(&expand(expected)?, &names)?;
            matching.push(Term::eq(
                resolve(&assignment.value, &encoded, &positions, &input)?,
                wire(expected.0, expected.1),
            ));
        }
        for (plan, delivery) in branch.outbox.iter().zip(&case.outbox) {
            matching.push(Term::Bool(
                plan.ordinal == delivery.ordinal
                    && plan.channel == delivery.channel
                    && plan.destination == Atom::Text(delivery.destination.as_bytes().to_vec())
                    && plan.idempotency == delivery.idempotency
                    && plan.payload.len() == delivery.payload.len(),
            ));
            for (field, value) in &plan.payload {
                let expected = delivery
                    .payload
                    .get(field)
                    .ok_or_else(|| refused("compiler delivery has no authored expression"))?;
                let expected = ast(&expand(expected)?, &names)?;
                matching.push(Term::eq(
                    resolve(value, &encoded, &positions, &input)?,
                    wire(expected.0, expected.1),
                ));
            }
        }
        let property = Term::or(vec![Term::not(Term::and(guard)), Term::and(matching)]);
        queries.push(query(
            format!("rule-{index}"),
            "per-rule",
            "T",
            script,
            property,
        )?);
    }
    let framer = Framer::new(&positions, &contract);
    let (decisions, refusals, tally) = policy::with_authority(&contract, &schema, |authority| {
        let mut decisions = Vec::new();
        let mut refusals = Vec::new();
        let mut tally = BTreeMap::<String, usize>::new();
        let mut selected = 0;
        for (index, tuple) in set.tuples().enumerate() {
            let outcome = evaluate::evaluate(authority, &framer, tuple);
            let label = match &outcome {
                Outcome::Decision(decision) => decision.class.name().to_owned(),
                Outcome::Refused(refusal) => format!("refused:{refusal}"),
            };
            *tally.entry(label).or_default() += 1;
            if selected < sample_count && index == selected * set.len() / sample_count {
                match outcome {
                    Outcome::Decision(decision) => {
                        let input = inputs(&mut Script::default(), &positions, Some(tuple));
                        let property = decision_property(&rules, &positions, &input, &decision)?;
                        let id = decisions.len();
                        queries.push(query(
                            format!("decision-{id}"),
                            "native-decision",
                            "T",
                            Script::default(),
                            property.clone(),
                        )?);
                        if id < 4 {
                            queries.push(query(
                                format!("decision-{id}-negated"),
                                "planted-control",
                                "F",
                                Script::default(),
                                Term::not(property),
                            )?);
                        }
                        let post: BTreeMap<String, String> = decision
                            .post
                            .iter()
                            .map(|(field, value)| {
                                value
                                    .number()
                                    .map(|value| (field.to_string(), value.to_string()))
                                    .ok_or_else(|| refused("a sampled successor is not scalar"))
                            })
                            .collect::<Result<_, _>>()?;
                        decisions.push(json!({"inputs": positions.iter().zip(tuple).map(|(position, value)| (position.name.clone(), value.to_string())).collect::<BTreeMap<_, _>>(),
                            "class": decision.class.name(), "reason": decision.reason, "post": post,
                            "outbox": decision.outbox.iter().map(|delivery| json!({"ordinal": delivery.ordinal, "channel": delivery.channel,
                                "destination": delivery.destination.json(), "payload": delivery.payload.iter().map(|(field, value)| json!({"field": field, "value": value.json()})).collect::<Vec<_>>(),
                                "idempotency": delivery.idempotency.json()})).collect::<Vec<_>>() }));
                    }
                    Outcome::Refused(refusal) => {
                        refusals.push(json!({"input": tuple, "refusal": refusal.to_string()}))
                    }
                }
                selected += 1;
            }
        }
        Ok::<_, ContractError>((decisions, refusals, tally))
    })??;
    let mut script = Script::default();
    script
        .comments
        .push("The negation of a planted false proposition is satisfiable.".to_owned());
    queries.push(query(
        "constant-false".to_owned(),
        "planted-control",
        "F",
        script,
        Term::Bool(false),
    )?);
    Ok(
        json!({"schema": "zeno-fcis/tau-kit/1", "authority": "none", "evidence": "optional-attested-corroboration",
        "sources": {"project_sha256": sha256_hex(project.as_bytes()), "rules_sha256": sha256_hex(source.as_bytes()),
            "program_sha256": sha256_hex(&super::program_bytes(&native_program)?)},
        "mapping": positions.iter().enumerate().map(|(index, position)| json!({"contract": position.name, "tau": format!("x{index}"), "domain": position.domain.json(),
            "representation": "64-bit two's-complement; comparisons explicitly signed"})).collect::<Vec<_>>(),
        "review": {"construction": set.construction.name(), "inputs": set.len(), "sampled_inputs": sample_count, "decisions": decisions.len(),
            "sample_rule": "ordinal floor(k * input_count / sample_count), k increasing", "tally": tally, "sampled_refusals": refusals},
        "decisions": decisions, "queries": queries,
        "assumptions": ["native compiler, descriptor, Authority, framing and query serializers", "external Tau and SMT implementations", "host supplies authenticated context when needed"],
        "not_claimed": ["KernelChecked", "Tau authority", "runtime authorization", "reachability", "temporal synthesis", "whole-domain Tau decision replay"]}),
    )
}

fn inputs(script: &mut Script, positions: &[Position], tuple: Option<&[i64]>) -> Vec<(Term, Sort)> {
    positions
        .iter()
        .enumerate()
        .map(|(index, position)| {
            let term = if let Some(tuple) = tuple {
                Term::Int(i128::from(tuple[index]))
            } else {
                let values = match &position.domain {
                    LeafDomain::Bool => Values::Range { min: 0, max: 1 },
                    LeafDomain::Int { min, max } => Values::Range {
                        min: i128::from(*min),
                        max: i128::from(*max),
                    },
                    LeafDomain::Sum { variants, .. } => {
                        Values::Set(variants.iter().map(|id| i128::from(*id)).collect())
                    }
                };
                script.constant(&format!("x{index}"), values)
            };
            if matches!(&position.domain, LeafDomain::Bool) {
                (Term::eq(term, Term::Int(1)), Sort::Bool)
            } else {
                (term, Sort::Int)
            }
        })
        .collect()
}

fn names(positions: &[Position], values: &[(Term, Sort)]) -> BTreeMap<String, (Term, Sort)> {
    positions
        .iter()
        .zip(values)
        .map(|(position, value)| (position.name.clone(), value.clone()))
        .collect()
}

/// The reparsed source reference is checked against actual native compiler
/// outputs for each selected branch, independently from authored expressions.
/// The caller first rebuilds import metadata from the exact source and checks
/// it byte for byte against the application being exported.
pub(crate) fn reference_queries(
    project: &str,
    source: &str,
    metadata: &Value,
) -> Result<Vec<Value>, ContractError> {
    let rules = Rules::read(source)?;
    let declarations = Declarations::read(project, &rules.leaf_bindings)?;
    let schema = schema::encode(&declarations)?;
    let contract = Contract::build(&declarations, &rules, schema_commitment(&schema)?)?;
    let positions = domain::positions(&declarations)?;
    let native_program = contract.program()?;
    ranges(&native_program)?;
    let mapping = metadata["mapping"]
        .as_object()
        .ok_or_else(|| refused("missing import name mapping"))?;
    let widths = metadata["reference"]["widths"]
        .as_object()
        .ok_or_else(|| refused("missing reference widths"))?;
    let allowance = match metadata.get("profile") {
        None => false,
        Some(Value::String(name)) if name == "allowance-study-bv3" => true,
        _ => return Err(refused("unsupported source correspondence profile")),
    };
    let decision_output = if allowance {
        metadata["decision_output"]
            .as_str()
            .ok_or_else(|| refused("missing allowance decision output"))?
    } else {
        "o2"
    };
    if allowance && !matches!(decision_output, "o2" | "o4") {
        return Err(refused("unsupported allowance decision output"));
    }
    let mut queries = Vec::new();
    for (index, branch) in contract.branches.iter().enumerate() {
        if !branch.outbox.is_empty() {
            return Err(refused("the source ladder has no delivery semantics"));
        }
        let mut script = Script::default();
        let input = inputs(&mut script, &positions, None);
        let encoded = program::encode(
            &mut script,
            "p",
            program::Program::of(&native_program),
            &input
                .iter()
                .map(|(term, sort)| wire(term.clone(), *sort))
                .collect::<Vec<_>>(),
        )
        .map_err(|error| refused(error.reason()))?;
        let native = names(&positions, &input);
        let mut values = BTreeMap::new();
        let mut properties = vec![encoded.ok()];
        for (name, row) in mapping {
            let native_name = row["native"]
                .as_str()
                .ok_or_else(|| refused("invalid mapped native name"))?;
            let value = native
                .get(native_name)
                .ok_or_else(|| refused("reference names an absent native input"))?;
            let mut input_value = wire(value.0.clone(), value.1);
            if allowance {
                let offset = row["native_offset"]
                    .as_i64()
                    .ok_or_else(|| refused("missing allowance input conversion"))?;
                if native_name == "command.101.120" {
                    let position = positions
                        .iter()
                        .find(|position| position.name == native_name)
                        .ok_or_else(|| refused("missing allowance kind input"))?;
                    if !matches!(&position.domain, LeafDomain::Sum { variants, .. } if variants.as_slice() == [140, 141, 142, 143])
                        || !matches!(offset, 0 | 140)
                    {
                        return Err(refused(
                            "allowance kind conversion requires its exact closed four-value enum",
                        ));
                    }
                } else if offset != 0 {
                    return Err(refused(
                        "only the closed allowance kind enum has a source-code conversion",
                    ));
                }
                if offset != 0 {
                    input_value = Term::sub(input_value, Term::Int(i128::from(offset)));
                }
                let min = row["min"]
                    .as_i64()
                    .ok_or_else(|| refused("missing allowance input minimum"))?;
                let max = row["max"]
                    .as_i64()
                    .ok_or_else(|| refused("missing allowance input maximum"))?;
                properties.push(Term::within(&input_value, i128::from(min), i128::from(max)));
            }
            values.insert(name.clone(), input_value.clone());
            let output = if allowance {
                row.get("output")
                    .map(|output| {
                        output
                            .as_str()
                            .map(str::to_owned)
                            .ok_or_else(|| refused("invalid allowance state output"))
                    })
                    .transpose()?
            } else {
                name.strip_prefix("i1.").map(|field| format!("o1.{field}"))
            };
            if let Some(output) = output {
                let value = if branch.class == Class::Reject {
                    input_value
                } else {
                    let field = row["field"]
                        .as_u64()
                        .and_then(|field| u16::try_from(field).ok())
                        .ok_or_else(|| refused("invalid mapped state field"))?;
                    let assignment = branch
                        .assignments
                        .iter()
                        .find(|assignment| assignment.field == field)
                        .ok_or_else(|| refused("native branch omits a source output"))?;
                    resolve(&assignment.value, &encoded, &positions, &input)?
                };
                let width = reference_width(
                    widths
                        .get(&output)
                        .ok_or_else(|| refused("missing source output width"))?,
                )?;
                properties.push(Term::within(&value, 0, (1i128 << width) - 1));
                values.insert(output, value);
            }
        }
        values.insert(
            decision_output.to_owned(),
            Term::Int(match branch.class {
                Class::Accept => 1,
                Class::Reject => 0,
                _ => {
                    return Err(refused(
                        "source correspondence supports only Accept and Reject",
                    ));
                }
            }),
        );
        let (formula, width) =
            reference_expression(&metadata["reference"]["formula"], &values, widths)?;
        if width != 0 {
            return Err(refused("the reference ladder is not a formula"));
        }
        properties.push(formula);
        let property = Term::or(vec![
            Term::not(Term::eq(
                encoded.outputs[0].clone(),
                Term::Int(index as i128),
            )),
            Term::and(properties),
        ]);
        queries.push(query(
            format!("source-rule-{index}"),
            "import-per-rule",
            "T",
            script,
            property,
        )?);
    }
    Ok(queries)
}

fn reference_width(value: &Value) -> Result<u32, ContractError> {
    value
        .as_u64()
        .and_then(|width| u32::try_from(width).ok())
        .filter(|width| (1..=16).contains(width))
        .ok_or_else(|| refused("unsupported source bit width"))
}

/// Serialize the finite source algebra, without executing a decision. Width
/// zero denotes a formula; other widths denote unsigned modular scalars.
fn reference_expression(
    expression: &Value,
    values: &BTreeMap<String, Term>,
    widths: &serde_json::Map<String, Value>,
) -> Result<(Term, u32), ContractError> {
    let parts = expression
        .as_array()
        .ok_or_else(|| refused("invalid reference expression"))?;
    let tag = parts
        .first()
        .and_then(Value::as_str)
        .ok_or_else(|| refused("missing reference expression tag"))?;
    let child = |index| {
        parts
            .get(index)
            .ok_or_else(|| refused("missing reference operand"))
    };
    let recurse = |index| reference_expression(child(index)?, values, widths);
    Ok(match tag {
        "bool" if parts.len() == 2 => (
            Term::Bool(
                child(1)?
                    .as_bool()
                    .ok_or_else(|| refused("invalid reference Boolean"))?,
            ),
            0,
        ),
        "int" if parts.len() == 4 => {
            let width = reference_width(child(3)?)?;
            let value = child(1)?
                .as_u64()
                .filter(|value| *value < 1u64 << width)
                .ok_or_else(|| refused("source literal exceeds its effective width"))?;
            (Term::Int(i128::from(value)), width)
        }
        "name" if parts.len() == 2 => {
            let name = child(1)?
                .as_str()
                .ok_or_else(|| refused("invalid reference name"))?;
            (
                values
                    .get(name)
                    .cloned()
                    .ok_or_else(|| refused("reference names an absent observation"))?,
                reference_width(
                    widths
                        .get(name)
                        .ok_or_else(|| refused("missing reference width"))?,
                )?,
            )
        }
        "not" if parts.len() == 2 => {
            let (term, width) = recurse(1)?;
            if width != 0 {
                return Err(refused("source negation requires a formula"));
            }
            (Term::not(term), 0)
        }
        "choose" if parts.len() == 4 => {
            let (condition, c) = recurse(1)?;
            let (yes, a) = recurse(2)?;
            let (no, b) = recurse(3)?;
            if c != 0 || a != 0 || b != 0 {
                return Err(refused("source conditionals must select formulas"));
            }
            (Term::ite(condition, yes, no), 0)
        }
        "binary" if parts.len() == 4 => {
            let operator = child(1)?
                .as_str()
                .ok_or_else(|| refused("invalid reference operator"))?;
            let (a, width) = recurse(2)?;
            let (b, other) = recurse(3)?;
            if width != other {
                return Err(refused("mixed-width reference operands"));
            }
            match operator {
                "&&" | "||" | "->" if width == 0 => (
                    match operator {
                        "&&" => Term::and(vec![a, b]),
                        "||" => Term::or(vec![a, b]),
                        _ => Term::or(vec![Term::not(a), b]),
                    },
                    0,
                ),
                "=" | "!=" if width != 0 => {
                    let equal = Term::eq(a, b);
                    (
                        if operator == "=" {
                            equal
                        } else {
                            Term::not(equal)
                        },
                        0,
                    )
                }
                "<" if width != 0 => (Term::lt(a, b), 0),
                "<=" if width != 0 => (Term::le(a, b), 0),
                ">" if width != 0 => (Term::lt(b, a), 0),
                ">=" if width != 0 => (Term::le(b, a), 0),
                "+" if width != 0 => {
                    let sum = Term::add(a, b);
                    let modulus = Term::Int(1i128 << width);
                    (
                        Term::ite(
                            Term::le(modulus.clone(), sum.clone()),
                            Term::sub(sum.clone(), modulus),
                            sum,
                        ),
                        width,
                    )
                }
                "-" if width != 0 => {
                    let difference = Term::sub(a.clone(), b.clone());
                    (
                        Term::ite(
                            Term::le(b, a),
                            difference.clone(),
                            Term::add(difference, Term::Int(1i128 << width)),
                        ),
                        width,
                    )
                }
                _ => return Err(refused("unsupported source reference operation")),
            }
        }
        _ => return Err(refused("unsupported reference expression representation")),
    })
}

fn boolean(value: (Term, Sort)) -> Term {
    if value.1 == Sort::Bool {
        value.0
    } else {
        Term::eq(value.0, Term::Int(1))
    }
}

fn wire(term: Term, sort: Sort) -> Term {
    if sort == Sort::Bool {
        Term::ite(term, Term::Int(1), Term::Int(0))
    } else {
        term
    }
}

fn ast(
    expression: &Ast,
    names: &BTreeMap<String, (Term, Sort)>,
) -> Result<(Term, Sort), ContractError> {
    Ok(match expression {
        Ast::Bool(value) => (Term::Bool(*value), Sort::Bool),
        Ast::Int(value) => {
            i64::try_from(*value).map_err(|_| refused("integer outside i64"))?;
            (Term::Int(*value), Sort::Int)
        }
        Ast::Name(name) => names
            .get(name)
            .cloned()
            .ok_or_else(|| refused(format!("unsupported projection {name}")))?,
        Ast::Not(inner) => (Term::not(boolean(ast(inner, names)?)), Sort::Bool),
        Ast::Neg(inner) => {
            let (term, sort) = ast(inner, names)?;
            (Term::sub(Term::Int(0), wire(term, sort)), Sort::Int)
        }
        Ast::Choose(condition, yes, no) => {
            let condition = boolean(ast(condition, names)?);
            let (a, a_sort) = ast(yes, names)?;
            let (b, b_sort) = ast(no, names)?;
            if a_sort == b_sort {
                (Term::ite(condition, a, b), a_sort)
            } else {
                (
                    Term::ite(condition, wire(a, a_sort), wire(b, b_sort)),
                    Sort::Int,
                )
            }
        }
        Ast::Binary(operator, left, right) => {
            let a = ast(left, names)?;
            let b = ast(right, names)?;
            match operator {
                Binary::And => (Term::and(vec![boolean(a), boolean(b)]), Sort::Bool),
                Binary::Or => (Term::or(vec![boolean(a), boolean(b)]), Sort::Bool),
                Binary::Implies => (
                    Term::or(vec![Term::not(boolean(a)), boolean(b)]),
                    Sort::Bool,
                ),
                Binary::Eq | Binary::Ne if a.1 == b.1 => {
                    let equal = Term::eq(a.0, b.0);
                    (
                        if *operator == Binary::Eq {
                            equal
                        } else {
                            Term::not(equal)
                        },
                        Sort::Bool,
                    )
                }
                _ => {
                    let a = wire(a.0, a.1);
                    let b = wire(b.0, b.1);
                    match operator {
                        Binary::Eq => (Term::eq(a, b), Sort::Bool),
                        Binary::Ne => (Term::not(Term::eq(a, b)), Sort::Bool),
                        Binary::Lt => (Term::lt(a, b), Sort::Bool),
                        Binary::Le => (Term::le(a, b), Sort::Bool),
                        Binary::Gt => (Term::lt(b, a), Sort::Bool),
                        Binary::Ge => (Term::le(b, a), Sort::Bool),
                        Binary::Add => (Term::add(a, b), Sort::Int),
                        Binary::Sub => (Term::sub(a, b), Sort::Int),
                        _ => {
                            return Err(refused(
                                "multiplication and division are outside the Tau export profile",
                            ));
                        }
                    }
                }
            }
        }
        Ast::Div(..) => return Err(refused("division is outside the Tau export profile")),
        Ast::MulDiv(..) | Ast::ProductsGe(..) => {
            return Err(refused(
                "arithmetic extension is outside the Tau export profile",
            ));
        }
    })
}

fn resolve(
    expression: &Expr,
    encoded: &Encoded,
    positions: &[Position],
    input: &[(Term, Sort)],
) -> Result<Term, ContractError> {
    match expression {
        Expr::Output(index) => encoded
            .outputs
            .get(*index)
            .cloned()
            .ok_or_else(|| refused("missing compiled output")),
        Expr::Input(source, field) => positions
            .iter()
            .position(|position| position.source == *source && position.field == Some(*field))
            .map(|index| wire(input[index].0.clone(), input[index].1))
            .ok_or_else(|| refused("missing input field")),
        Expr::Root(source) => positions
            .iter()
            .position(|position| position.source == *source && position.field.is_none())
            .map(|index| wire(input[index].0.clone(), input[index].1))
            .ok_or_else(|| refused("a compound root is not supported")),
        Expr::Constant(atom) => match atom {
            Atom::Bool(value) => Ok(Term::Int(i128::from(*value))),
            Atom::I128(value) => Ok(Term::Int(*value)),
            Atom::U128(value) => i128::try_from(*value)
                .map(Term::Int)
                .map_err(|_| refused("wide unsigned atom")),
            Atom::Sum { variant, .. } => Ok(Term::Int(i128::from(*variant))),
            Atom::Text(_) => Err(refused("dynamic text observations are unsupported")),
        },
    }
}

fn decision_property(
    rules: &Rules,
    positions: &[Position],
    input: &[(Term, Sort)],
    decision: &evaluate::Decision,
) -> Result<Term, ContractError> {
    let names = names(positions, input);
    let mut alternatives = Vec::new();
    let mut prior = Vec::new();
    for case in &rules.cases {
        let expand = |value: &Ast| expr::expand(value, &rules.variables).map_err(refused);
        let guard = boolean(ast(&expand(&case.when)?, &names)?);
        let mut parts = prior.clone();
        parts.push(guard.clone());
        parts.push(Term::Bool(
            case.class == decision.class && case.reason == decision.reason,
        ));
        if case.class == Class::Reject {
            parts.push(Term::Bool(
                decision.post.is_empty() && decision.outbox.is_empty(),
            ));
        } else {
            parts.push(Term::Bool(
                case.post.len() == decision.post.len()
                    && case.outbox.len() == decision.outbox.len(),
            ));
            for (field, expected) in &case.post {
                let value = decision
                    .post
                    .iter()
                    .find(|(id, _)| id == field)
                    .and_then(|(_, value)| value.number());
                let expected = ast(&expand(expected)?, &names)?;
                parts.push(value.map_or(Term::Bool(false), |value| {
                    Term::eq(wire(expected.0, expected.1), Term::Int(value))
                }));
            }
            for (expected, actual) in case.outbox.iter().zip(&decision.outbox) {
                parts.push(Term::Bool(
                    expected.ordinal == actual.ordinal
                        && expected.channel == actual.channel
                        && actual.destination
                            == evaluate::Value::Text(expected.destination.as_bytes().to_vec())
                        && actual.idempotency == evaluate::Value::U128(expected.idempotency)
                        && expected.payload.len() == actual.payload.len(),
                ));
                for (field, expected) in &expected.payload {
                    let value = actual
                        .payload
                        .iter()
                        .find(|(id, _)| id == field)
                        .and_then(|(_, value)| value.number());
                    let expected = ast(&expand(expected)?, &names)?;
                    parts.push(value.map_or(Term::Bool(false), |value| {
                        Term::eq(wire(expected.0, expected.1), Term::Int(value))
                    }));
                }
            }
        }
        alternatives.push(Term::and(parts));
        prior.push(Term::not(guard));
    }
    Ok(Term::or(alternatives))
}

/// A structural sufficient condition for eager arithmetic to fit i64 even
/// in unused/unselected nodes. A failed bound refuses rather than approximates.
fn ranges(program: &Program) -> Result<(), ContractError> {
    let mut values = Vec::<(i128, i128)>::new();
    for node in program.nodes() {
        let interval = match *node {
            Op::Input(index) => {
                let (lo, hi) = program.inputs()[usize::from(index)].bounds();
                if lo < 0 || hi > 65535 {
                    return Err(refused(
                        "inputs must be nonnegative scalar codes no larger than 65535",
                    ));
                }
                (i128::from(lo), i128::from(hi))
            }
            Op::Int(value) => (i128::from(value), i128::from(value)),
            Op::Bool(_) | Op::Eq(..) | Op::Lt(..) | Op::And(..) | Op::Not(..) => (0, 1),
            Op::Add(a, b) => {
                let a = values[usize::from(a)];
                let b = values[usize::from(b)];
                (a.0 + b.0, a.1 + b.1)
            }
            Op::Sub(a, b) => {
                let a = values[usize::from(a)];
                let b = values[usize::from(b)];
                (a.0 - b.1, a.1 - b.0)
            }
            Op::Select(_, a, b) => {
                let a = values[usize::from(a)];
                let b = values[usize::from(b)];
                (a.0.min(b.0), a.1.max(b.1))
            }
            _ => return Err(refused("unsupported scalar instruction")),
        };
        if interval.0 < i128::from(i64::MIN) || interval.1 > i128::from(i64::MAX) {
            return Err(refused(
                "cannot establish eager i64 arithmetic safety for this program",
            ));
        }
        values.push(interval);
    }
    Ok(())
}

/// The authored side of a comparison is bounded independently from the
/// compiled graph. Both arms are checked even for a constant condition.
fn ast_ranges(
    expression: &Ast,
    names: &BTreeMap<String, (i128, i128)>,
) -> Result<(i128, i128), ContractError> {
    let interval = match expression {
        Ast::Bool(_) => (0, 1),
        Ast::Int(value) => (*value, *value),
        Ast::Name(name) => *names
            .get(name)
            .ok_or_else(|| refused(format!("unsupported projection {name}")))?,
        Ast::Not(inner) => {
            ast_ranges(inner, names)?;
            (0, 1)
        }
        Ast::Neg(inner) => {
            let (lo, hi) = ast_ranges(inner, names)?;
            (-hi, -lo)
        }
        Ast::Choose(condition, yes, no) => {
            ast_ranges(condition, names)?;
            let a = ast_ranges(yes, names)?;
            let b = ast_ranges(no, names)?;
            (a.0.min(b.0), a.1.max(b.1))
        }
        Ast::Binary(operator, left, right) => {
            let a = ast_ranges(left, names)?;
            let b = ast_ranges(right, names)?;
            match operator {
                Binary::Add => (a.0 + b.0, a.1 + b.1),
                Binary::Sub => (a.0 - b.1, a.1 - b.0),
                Binary::Mul => {
                    return Err(refused("multiplication is outside the Tau export profile"));
                }
                _ => (0, 1),
            }
        }
        Ast::Div(..) => return Err(refused("division is outside the Tau export profile")),
        Ast::MulDiv(..) | Ast::ProductsGe(..) => {
            return Err(refused(
                "arithmetic extension is outside the Tau export profile",
            ));
        }
    };
    if interval.0 < i128::from(i64::MIN) || interval.1 > i128::from(i64::MAX) {
        return Err(refused(
            "cannot establish authored eager i64 arithmetic safety",
        ));
    }
    Ok(interval)
}

fn query(
    id: String,
    kind: &str,
    expected: &str,
    mut script: Script,
    property: Term,
) -> Result<Value, ContractError> {
    let tau = Tau::new(&script).formula(&script, &property)?;
    script.assert(Term::not(property));
    let smt = script.source();
    if tau.len() > MAX_QUERY_BYTES || smt.len() > MAX_QUERY_BYTES {
        return Err(refused("one query exceeds 1 MiB"));
    }
    Ok(json!({"id": id, "kind": kind, "expected": expected, "tau": tau, "smt": smt}))
}

/// Render unbounded integer terms in an explicitly signed bv[64] profile.
/// Numeric conditionals are lifted to constrained fresh variables, since the
/// supported Tau grammar uses conditionals over formulas.
struct Tau {
    sorts: BTreeMap<String, Sort>,
    helpers: Vec<String>,
}

impl Tau {
    fn new(script: &Script) -> Self {
        let mut sorts = script
            .constants
            .iter()
            .map(|(name, _)| (name.clone(), Sort::Int))
            .collect::<BTreeMap<_, _>>();
        sorts.extend(
            script
                .definitions
                .iter()
                .map(|(name, sort, _)| (name.clone(), *sort)),
        );
        Self {
            sorts,
            helpers: Vec::new(),
        }
    }

    fn int(&mut self, term: &Term) -> Result<String, ContractError> {
        Ok(match term {
            Term::Int(value) => {
                let value = i64::try_from(*value)
                    .map_err(|_| refused("Tau integer outside the signed representation"))?;
                format!("{{{}}}:bv[64]", value as u64)
            }
            Term::Name(name) if self.sorts.get(name) == Some(&Sort::Int) => name.clone(),
            Term::Add(a, b) | Term::Sub(a, b) => {
                let a = self.int(a)?;
                let b = self.int(b)?;
                format!(
                    "(({a}) {} ({b}))",
                    if matches!(term, Term::Add(..)) {
                        "+"
                    } else {
                        "-"
                    }
                )
            }
            Term::Ite(condition, yes, no) => {
                let condition = self.boolean(condition)?;
                let yes = self.int(yes)?;
                let no = self.int(no)?;
                let name = format!("h{}", self.helpers.len());
                self.sorts.insert(name.clone(), Sort::Int);
                self.helpers.push(format!(
                    "(({condition}) ? ({name} = {yes}) : ({name} = {no}))"
                ));
                name
            }
            _ => return Err(refused("unsupported integer term in Tau query")),
        })
    }

    fn bool_sort(&self, term: &Term) -> bool {
        match term {
            Term::Name(name) => self.sorts.get(name) == Some(&Sort::Bool),
            Term::Int(_) | Term::Add(..) | Term::Sub(..) | Term::Mul(..) | Term::Div(..) => false,
            Term::Ite(_, yes, _) => self.bool_sort(yes),
            _ => true,
        }
    }

    fn boolean(&mut self, term: &Term) -> Result<String, ContractError> {
        Ok(match term {
            Term::Bool(value) => if *value { "T" } else { "F" }.to_owned(),
            Term::Name(name) if self.sorts.get(name) == Some(&Sort::Bool) => {
                format!("({name} = {{1}}:bv[64])")
            }
            Term::Not(inner) => format!("!({})", self.boolean(inner)?),
            Term::And(items) | Term::Or(items) => {
                let separator = if matches!(term, Term::And(..)) {
                    " && "
                } else {
                    " || "
                };
                if items.is_empty() {
                    if matches!(term, Term::And(..)) {
                        "T"
                    } else {
                        "F"
                    }
                    .to_owned()
                } else {
                    format!(
                        "({})",
                        items
                            .iter()
                            .map(|item| self.boolean(item).map(|text| format!("({text})")))
                            .collect::<Result<Vec<_>, _>>()?
                            .join(separator)
                    )
                }
            }
            Term::Eq(a, b) if self.bool_sort(a) && self.bool_sort(b) => {
                format!("(({}) <-> ({}))", self.boolean(a)?, self.boolean(b)?)
            }
            Term::Eq(a, b) => format!("(({}) = ({}))", self.int(a)?, self.int(b)?),
            Term::Lt(a, b) | Term::Le(a, b) => {
                let a = self.int(a)?;
                let b = self.int(b)?;
                let sa = format!("({a} >= {{9223372036854775808}}:bv[64])");
                let sb = format!("({b} >= {{9223372036854775808}}:bv[64])");
                format!(
                    "(({sa} && !{sb}) || (({sa} <-> {sb}) && (({a}) {} ({b}))))",
                    if matches!(term, Term::Lt(..)) {
                        "<"
                    } else {
                        "<="
                    }
                )
            }
            Term::Ite(condition, yes, no) => format!(
                "(({}) ? ({}) : ({}))",
                self.boolean(condition)?,
                self.boolean(yes)?,
                self.boolean(no)?
            ),
            _ => return Err(refused("unsupported Boolean term in Tau query")),
        })
    }

    fn formula(mut self, script: &Script, property: &Term) -> Result<String, ContractError> {
        let mut premises = Vec::new();
        for (name, values) in &script.constants {
            let term = Term::name(name);
            premises.push(self.boolean(&match values {
                Values::Range { min, max } => Term::within(&term, *min, *max),
                Values::Set(values) => Term::member(&term, values),
            })?);
        }
        for (name, sort, term) in &script.definitions {
            let equality = if *sort == Sort::Int {
                format!("({name} = {})", self.int(term)?)
            } else {
                format!(
                    "({name} = {{1}}:bv[64] <-> ({})) && ({name} <= {{1}}:bv[64])",
                    self.boolean(term)?
                )
            };
            premises.push(equality);
        }
        let property = self.boolean(property)?;
        premises.extend(self.helpers);
        let quantifiers = self
            .sorts
            .keys()
            .map(|name| format!("all {name}:bv[64] "))
            .collect::<String>();
        let premise = if premises.is_empty() {
            "T".to_owned()
        } else {
            premises
                .iter()
                .map(|text| format!("({text})"))
                .collect::<Vec<_>>()
                .join(" && ")
        };
        Ok(format!("{quantifiers}(({premise}) -> ({property}))"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn actual_authority_decisions_and_compiler_queries_are_exported() {
        let source = "zeno 1; project 1 ladder; namespace 2 test; type 100 state Ledger; type 101 command Command; type 102 context Context; type 103 int Amount in 0..=3; field 120 100 available 103; field 140 101 amount 103; reason 200 denied precedence 0; component 400 ladder { owns 100; reads pre.100; writes post.100; contexts context.102; budget steps 64; } merge [400]; law 500 bounded on commit, genesis = post.100.120 >= 0 && post.100.120 <= 3;";
        let rules = r#"{"schema":"zeno-fcis/template-declarative-policy/2","template":"tau-test","roots":{"state":100,"command":101,"context":102},"leaf_bindings":{"102":["I128",0,1]},"variables":{},"cases":[{"when":"context.102 == 1 && command.101.140 <= pre.100.120","class":"Accept","reason":null,"post":{"120":"pre.100.120 - command.101.140"},"outbox":[]},{"when":"true","class":"Reject","reason":200,"post":{},"outbox":[]}],"genesis":{"120":0},"law_kinds":{"500":"StateInvariant"}}"#;
        let bundle = export(source, rules, 64, 32)
            .unwrap_or_else(|error| panic!("cannot export the test contract: {error}"));
        assert_eq!(bundle["review"]["inputs"], 32);
        assert_eq!(bundle["review"]["tally"]["Accept"], 10);
        assert_eq!(bundle["review"]["tally"]["Reject"], 22);
        assert_eq!(
            bundle["decisions"]
                .as_array()
                .unwrap_or_else(|| panic!("expected exported JSON array"))
                .len(),
            32
        );
        assert!(
            bundle["queries"]
                .as_array()
                .unwrap_or_else(|| panic!("expected exported JSON array"))
                .iter()
                .all(|query| !query["tau"]
                    .as_str()
                    .unwrap_or_else(|| panic!("expected exported query text"))
                    .ends_with('.'))
        );
        assert!(
            bundle["queries"][0]["smt"]
                .as_str()
                .unwrap_or_else(|| panic!("expected exported query text"))
                .contains("(define-fun p")
        );
        assert_eq!(bundle["authority"], "none");
    }

    #[test]
    fn unsafe_eager_intermediate_is_refused_even_when_unselected() {
        use zeno_fcis_synthesis::finite::Domain;
        let program = Program::try_new(
            vec![],
            vec![Domain::Int { min: 0, max: 0 }],
            vec![Op::Int(i64::MAX), Op::Int(1), Op::Add(0, 1), Op::Int(0)],
            vec![3],
        )
        .unwrap_or_else(|error| panic!("cannot construct the eager-trap test program: {error}"));
        assert!(ranges(&program).is_err());
    }

    #[test]
    fn signed_boundary_literals_and_comparisons_are_explicit() {
        let mut tau = Tau::new(&Script::default());
        assert_eq!(
            tau.int(&Term::Int(-1))
                .unwrap_or_else(|error| panic!("cannot render the signed boundary query: {error}")),
            "{18446744073709551615}:bv[64]"
        );
        assert_eq!(
            tau.int(&Term::Int(i128::from(i64::MIN)))
                .unwrap_or_else(|error| panic!("cannot render the signed boundary query: {error}")),
            "{9223372036854775808}:bv[64]"
        );
        let order = tau
            .boolean(&Term::lt(Term::Int(-1), Term::Int(0)))
            .unwrap_or_else(|error| panic!("cannot render the signed boundary query: {error}"));
        assert!(order.contains("9223372036854775808"));
        assert!(order.contains("<->"));
        let unsafe_arm = Ast::Choose(
            Box::new(Ast::Bool(false)),
            Box::new(Ast::Binary(
                Binary::Sub,
                Box::new(Ast::Int(i128::from(i64::MIN))),
                Box::new(Ast::Int(1)),
            )),
            Box::new(Ast::Int(0)),
        );
        assert!(ast_ranges(&unsafe_arm, &BTreeMap::new()).is_err());
    }
}
