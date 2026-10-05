//! The fixed mutant catalog, applied to the rules model. Each mutant is a
//! rules file the generator and the library bind as they bind the original,
//! so what distinguishes it is the library's decision, never a reading of
//! the rule here.
//!
//! Mutants are generated operator by operator in catalog order, then site by
//! site in rules-file order: variables by name, then each case's `when`,
//! `post` fields and payload fields; within an expression, nodes in prefix
//! order. The numbering is therefore fixed by the rules file alone.

use super::super::declarations::Declarations;
use super::super::expr::{Ast, Binary, Rounding};
use super::super::rules::Rules;

/// The catalog this module implements; a new operator is a new version.
pub(super) const CATALOG: &str = "zeno-fcis/contract-review-mutants/1";

/// Every operator in catalog order, with what it changes.
pub(super) const OPERATORS: [(&str, &str); 8] = [
    (
        "flip-comparison",
        "one comparison moves to its boundary neighbour: < and <=, > and >=, == and != exchange",
    ),
    (
        "constant-up",
        "one integer constant of a rule expression, or a delivery's ordinal, channel or idempotency ordinal, grows by one",
    ),
    (
        "constant-down",
        "one integer constant of a rule expression, or a delivery's ordinal, channel or idempotency ordinal, shrinks by one",
    ),
    (
        "drop-conjunct",
        "one side of a && in a case guard or in a variable is dropped",
    ),
    (
        "swap-adjacent-cases",
        "two adjacent cases before the final catch-all exchange places",
    ),
    (
        "swap-reasons",
        "a case and the next case of its class exchange reasons",
    ),
    (
        "change-reason",
        "a case takes the next declared reason of its class",
    ),
    ("drop-outbox-entry", "one delivery of a case is dropped"),
];

#[derive(Debug)]
pub(super) struct Mutant {
    /// `m0001`, `m0002`, ... in catalog order.
    pub(super) id: String,
    pub(super) operator: &'static str,
    /// The rules-file entry changed: `cases[4].when`, `variables.computed.deadline`.
    pub(super) site: String,
    /// What changed there, as rule text.
    pub(super) change: String,
    pub(super) rules: Rules,
}

/// Every mutant of the rules, numbered in catalog order.
pub(super) fn catalog(rules: &Rules, declarations: &Declarations) -> Vec<Mutant> {
    let mut mutants = Vec::new();
    expression_mutants(rules, "flip-comparison", false, flip, &mut mutants);
    expression_mutants(
        rules,
        "constant-up",
        false,
        |ast| shifted(ast, 1),
        &mut mutants,
    );
    delivery_constants(rules, "constant-up", 1, &mut mutants);
    expression_mutants(
        rules,
        "constant-down",
        false,
        |ast| shifted(ast, -1),
        &mut mutants,
    );
    delivery_constants(rules, "constant-down", -1, &mut mutants);
    expression_mutants(rules, "drop-conjunct", true, dropped, &mut mutants);
    swap_adjacent_cases(rules, &mut mutants);
    swap_reasons(rules, &mut mutants);
    change_reasons(rules, declarations, &mut mutants);
    drop_outbox_entries(rules, &mut mutants);
    for (index, mutant) in mutants.iter_mut().enumerate() {
        mutant.id = format!("m{:04}", index + 1);
    }
    mutants
}

/// A rule expression's place in the rules file.
#[derive(Clone, Debug)]
enum Place {
    Variable(String),
    When(usize),
    Post(usize, u16),
    Payload(usize, usize, u16),
}

impl Place {
    fn name(&self) -> String {
        match self {
            Self::Variable(name) => format!("variables.{name}"),
            Self::When(case) => format!("cases[{case}].when"),
            Self::Post(case, field) => format!("cases[{case}].post.{field}"),
            Self::Payload(case, delivery, field) => {
                format!("cases[{case}].outbox[{delivery}].payload.{field}")
            }
        }
    }

    /// Whether the expression guards a case, directly or as a variable.
    fn guard(&self) -> bool {
        matches!(self, Self::Variable(_) | Self::When(_))
    }
}

fn places(rules: &Rules) -> Vec<Place> {
    let mut places: Vec<Place> = rules
        .variables
        .keys()
        .map(|name| Place::Variable(name.clone()))
        .collect();
    for (index, case) in rules.cases.iter().enumerate() {
        places.push(Place::When(index));
        places.extend(case.post.keys().map(|field| Place::Post(index, *field)));
        for (number, delivery) in case.outbox.iter().enumerate() {
            places.extend(
                delivery
                    .payload
                    .keys()
                    .map(|field| Place::Payload(index, number, *field)),
            );
        }
    }
    places
}

fn expression<'r>(rules: &'r Rules, place: &Place) -> Option<&'r Ast> {
    match place {
        Place::Variable(name) => rules.variables.get(name),
        Place::When(case) => rules.cases.get(*case).map(|case| &case.when),
        Place::Post(case, field) => rules.cases.get(*case)?.post.get(field),
        Place::Payload(case, delivery, field) => rules
            .cases
            .get(*case)?
            .outbox
            .get(*delivery)?
            .payload
            .get(field),
    }
}

fn expression_mut<'r>(rules: &'r mut Rules, place: &Place) -> Option<&'r mut Ast> {
    match place {
        Place::Variable(name) => rules.variables.get_mut(name),
        Place::When(case) => rules.cases.get_mut(*case).map(|case| &mut case.when),
        Place::Post(case, field) => rules.cases.get_mut(*case)?.post.get_mut(field),
        Place::Payload(case, delivery, field) => rules
            .cases
            .get_mut(*case)?
            .outbox
            .get_mut(*delivery)?
            .payload
            .get_mut(field),
    }
}

/// Applies `mutate` to every node of every rule expression, or of guards
/// only, one mutant per result.
fn expression_mutants(
    rules: &Rules,
    operator: &'static str,
    guards_only: bool,
    mutate: impl Fn(&Ast) -> Vec<Ast>,
    out: &mut Vec<Mutant>,
) {
    for place in places(rules) {
        if guards_only && !place.guard() {
            continue;
        }
        let Some(ast) = expression(rules, &place) else {
            continue;
        };
        for path in paths(ast) {
            let before = node(ast, &path);
            for after in mutate(before) {
                let mut mutated = rules.clone();
                if let Some(target) = expression_mut(&mut mutated, &place) {
                    *node_mut(target, &path) = after.clone();
                }
                out.push(Mutant {
                    id: String::new(),
                    operator,
                    site: place.name(),
                    change: format!("`{}` -> `{}`", render(before), render(&after)),
                    rules: mutated,
                });
            }
        }
    }
}

/// A comparison moved to its boundary neighbour.
fn flip(ast: &Ast) -> Vec<Ast> {
    let Ast::Binary(operator, left, right) = ast else {
        return Vec::new();
    };
    let flipped = match operator {
        Binary::Lt => Binary::Le,
        Binary::Le => Binary::Lt,
        Binary::Gt => Binary::Ge,
        Binary::Ge => Binary::Gt,
        Binary::Eq => Binary::Ne,
        Binary::Ne => Binary::Eq,
        _ => return Vec::new(),
    };
    vec![Ast::Binary(flipped, left.clone(), right.clone())]
}

/// An integer constant moved by `delta`.
fn shifted(ast: &Ast, delta: i128) -> Vec<Ast> {
    match ast {
        Ast::Int(value) => value.checked_add(delta).map(Ast::Int).into_iter().collect(),
        _ => Vec::new(),
    }
}

/// A conjunction without its right side, then without its left side.
fn dropped(ast: &Ast) -> Vec<Ast> {
    match ast {
        Ast::Binary(Binary::And, left, right) => vec![(**left).clone(), (**right).clone()],
        _ => Vec::new(),
    }
}

/// A delivery's ordinal, channel and idempotency ordinal moved by `delta`.
fn delivery_constants(rules: &Rules, operator: &'static str, delta: i128, out: &mut Vec<Mutant>) {
    for (index, case) in rules.cases.iter().enumerate() {
        for (number, delivery) in case.outbox.iter().enumerate() {
            let site = |name: &str| format!("cases[{index}].outbox[{number}].{name}");
            let moved = |value: u128| {
                i128::try_from(value)
                    .ok()
                    .and_then(|value| value.checked_add(delta))
            };
            if let Some(ordinal) = moved(u128::from(delivery.ordinal))
                && let Ok(ordinal) = u32::try_from(ordinal)
            {
                let mut mutated = rules.clone();
                mutated.cases[index].outbox[number].ordinal = ordinal;
                out.push(Mutant {
                    id: String::new(),
                    operator,
                    site: site("ordinal"),
                    change: format!("`{}` -> `{ordinal}`", delivery.ordinal),
                    rules: mutated,
                });
            }
            if let Some(channel) = moved(u128::from(delivery.channel))
                && let Ok(channel) = u32::try_from(channel)
            {
                let mut mutated = rules.clone();
                mutated.cases[index].outbox[number].channel = channel;
                out.push(Mutant {
                    id: String::new(),
                    operator,
                    site: site("channel"),
                    change: format!("`{}` -> `{channel}`", delivery.channel),
                    rules: mutated,
                });
            }
            if let Some(idempotency) = moved(delivery.idempotency)
                && let Ok(idempotency) = u128::try_from(idempotency)
            {
                let mut mutated = rules.clone();
                mutated.cases[index].outbox[number].idempotency = idempotency;
                out.push(Mutant {
                    id: String::new(),
                    operator,
                    site: site("idempotency_ordinal"),
                    change: format!("`{}` -> `{idempotency}`", delivery.idempotency),
                    rules: mutated,
                });
            }
        }
    }
}

/// Two adjacent cases exchange places; the final catch-all stays last.
fn swap_adjacent_cases(rules: &Rules, out: &mut Vec<Mutant>) {
    for index in 0..rules.cases.len().saturating_sub(2) {
        let mut mutated = rules.clone();
        mutated.cases.swap(index, index + 1);
        out.push(Mutant {
            id: String::new(),
            operator: "swap-adjacent-cases",
            site: format!("cases[{index}],cases[{}]", index + 1),
            change: format!(
                "case {} `{}` decides before case {index} `{}`",
                index + 1,
                render(&rules.cases[index + 1].when),
                render(&rules.cases[index].when)
            ),
            rules: mutated,
        });
    }
}

/// A case and the next case of its class exchange their reasons.
fn swap_reasons(rules: &Rules, out: &mut Vec<Mutant>) {
    for (index, case) in rules.cases.iter().enumerate() {
        let Some(reason) = case.reason else {
            continue;
        };
        let Some((other, swapped)) =
            rules.cases[index + 1..]
                .iter()
                .enumerate()
                .find_map(|(offset, candidate)| {
                    (candidate.class == case.class)
                        .then_some(candidate.reason)
                        .flatten()
                        .map(|other| (index + 1 + offset, other))
                })
        else {
            continue;
        };
        if swapped == reason {
            continue;
        }
        let mut mutated = rules.clone();
        mutated.cases[index].reason = Some(swapped);
        mutated.cases[other].reason = Some(reason);
        out.push(Mutant {
            id: String::new(),
            operator: "swap-reasons",
            site: format!("cases[{index}].reason,cases[{other}].reason"),
            change: format!("`{reason}` -> `{swapped}` and `{swapped}` -> `{reason}`"),
            rules: mutated,
        });
    }
}

/// A case takes the next declared reason of its class, in declaration order.
fn change_reasons(rules: &Rules, declarations: &Declarations, out: &mut Vec<Mutant>) {
    for (index, case) in rules.cases.iter().enumerate() {
        let Some(reason) = case.reason else {
            continue;
        };
        let same_class: Vec<u32> = declarations
            .reasons
            .iter()
            .copied()
            .filter(|declared| {
                rules
                    .cases
                    .iter()
                    .any(|other| other.reason == Some(*declared) && other.class == case.class)
            })
            .collect();
        let Some(position) = same_class.iter().position(|declared| *declared == reason) else {
            continue;
        };
        let next = same_class[(position + 1) % same_class.len()];
        if next == reason {
            continue;
        }
        let mut mutated = rules.clone();
        mutated.cases[index].reason = Some(next);
        out.push(Mutant {
            id: String::new(),
            operator: "change-reason",
            site: format!("cases[{index}].reason"),
            change: format!("`{reason}` -> `{next}`"),
            rules: mutated,
        });
    }
}

/// A case loses one delivery.
fn drop_outbox_entries(rules: &Rules, out: &mut Vec<Mutant>) {
    for (index, case) in rules.cases.iter().enumerate() {
        for (number, delivery) in case.outbox.iter().enumerate() {
            let mut mutated = rules.clone();
            mutated.cases[index].outbox.remove(number);
            out.push(Mutant {
                id: String::new(),
                operator: "drop-outbox-entry",
                site: format!("cases[{index}].outbox[{number}]"),
                change: format!(
                    "delivery {} on channel {} to `{}` is dropped",
                    delivery.ordinal, delivery.channel, delivery.destination
                ),
                rules: mutated,
            });
        }
    }
}

/// Every node path of an expression in prefix order; the root is `[]`.
fn paths(ast: &Ast) -> Vec<Vec<usize>> {
    fn walk(ast: &Ast, prefix: &mut Vec<usize>, out: &mut Vec<Vec<usize>>) {
        out.push(prefix.clone());
        for (index, child) in children(ast).into_iter().enumerate() {
            prefix.push(index);
            walk(child, prefix, out);
            prefix.pop();
        }
    }
    let mut out = Vec::new();
    walk(ast, &mut Vec::new(), &mut out);
    out
}

fn children(ast: &Ast) -> Vec<&Ast> {
    match ast {
        Ast::Bool(_) | Ast::Int(_) | Ast::Name(_) => Vec::new(),
        Ast::Not(inner) | Ast::Neg(inner) => vec![inner],
        Ast::Binary(_, left, right) | Ast::Div(_, left, right) => vec![left, right],
        Ast::Choose(condition, then, otherwise) => vec![condition, then, otherwise],
    }
}

fn node<'a>(ast: &'a Ast, path: &[usize]) -> &'a Ast {
    match path.split_first() {
        None => ast,
        Some((index, rest)) => node(children(ast)[*index], rest),
    }
}

fn node_mut<'a>(ast: &'a mut Ast, path: &[usize]) -> &'a mut Ast {
    let Some((index, rest)) = path.split_first() else {
        return ast;
    };
    let child = match (ast, *index) {
        (Ast::Not(inner) | Ast::Neg(inner), 0) => inner,
        (Ast::Binary(_, left, _) | Ast::Div(_, left, _), 0) => left,
        (Ast::Binary(_, _, right) | Ast::Div(_, _, right), 1) => right,
        (Ast::Choose(condition, _, _), 0) => condition,
        (Ast::Choose(_, then, _), 1) => then,
        (Ast::Choose(_, _, otherwise), 2) => otherwise,
        (other, _) => return other,
    };
    node_mut(child, rest)
}

/// An expression as rule text, every binary operand parenthesized.
pub(super) fn render(ast: &Ast) -> String {
    let operand = |inner: &Ast| match inner {
        Ast::Binary(..) => format!("({})", render(inner)),
        _ => render(inner),
    };
    match ast {
        Ast::Bool(value) => value.to_string(),
        Ast::Int(value) => value.to_string(),
        Ast::Name(name) => name.clone(),
        Ast::Not(inner) => format!("!{}", operand(inner)),
        Ast::Neg(inner) => format!("-{}", operand(inner)),
        Ast::Binary(operator, left, right) => {
            format!("{} {} {}", operand(left), symbol(*operator), operand(right))
        }
        Ast::Choose(condition, then, otherwise) => format!(
            "choose({}, {}, {})",
            render(condition),
            render(then),
            render(otherwise)
        ),
        Ast::Div(rounding, left, right) => format!(
            "{}({}, {})",
            match rounding {
                Rounding::Floor => "div_floor",
                Rounding::Ceil => "div_ceil",
            },
            render(left),
            render(right)
        ),
    }
}

fn symbol(operator: Binary) -> &'static str {
    match operator {
        Binary::Implies => "->",
        Binary::Or => "||",
        Binary::And => "&&",
        Binary::Eq => "==",
        Binary::Ne => "!=",
        Binary::Lt => "<",
        Binary::Le => "<=",
        Binary::Gt => ">",
        Binary::Ge => ">=",
        Binary::Add => "+",
        Binary::Sub => "-",
        Binary::Mul => "*",
    }
}
