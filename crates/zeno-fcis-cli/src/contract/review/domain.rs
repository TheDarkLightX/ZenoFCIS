//! The inputs a review runs: every program input position with its admitted
//! leaf domain, and the input set, which is the whole domain when it is small
//! enough and a deterministic boundary set otherwise.
//!
//! Boundary values of an integer leaf are its endpoints and every rule
//! constant the rules compare with, add to or assign to a value of that
//! leaf's type, each with its two neighbours, where in domain. Sums take
//! every variant and booleans both values. When the product of the boundary
//! sets fits the tuple limit it is the input set; otherwise the input set is
//! a probe set: base points, each varied in one position and in every pair
//! of positions over their boundary values.

use std::collections::{BTreeMap, BTreeSet};

use serde_json::{Value, json};

use super::super::ContractError;
use super::super::declarations::{Declarations, Kind as TypeKind, Source};
use super::super::expr::{self, Ast, Binary};
use super::super::rules::{Constant, Rules};

/// An admitted leaf domain, as the schema declares it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) enum LeafDomain {
    Bool,
    Int { min: i64, max: i64 },
    Sum { type_id: u32, variants: Vec<u16> },
}

impl LeafDomain {
    /// Values in the domain; `None` when the count exceeds `u128`.
    pub(super) fn size(&self) -> Option<u128> {
        match self {
            Self::Bool => Some(2),
            Self::Int { min, max } => u128::try_from(i128::from(*max) - i128::from(*min))
                .ok()?
                .checked_add(1),
            Self::Sum { variants, .. } => Some(variants.len() as u128),
        }
    }

    /// Every value in ascending order. Callers enumerate only domains whose
    /// size fits the tuple limit.
    fn values(&self) -> Vec<i64> {
        match self {
            Self::Bool => vec![0, 1],
            Self::Int { min, max } => (*min..=*max).collect(),
            Self::Sum { variants, .. } => variants.iter().map(|id| i64::from(*id)).collect(),
        }
    }

    pub(super) fn json(&self) -> Value {
        match self {
            Self::Bool => json!({"kind": "bool"}),
            Self::Int { min, max } => {
                json!({"kind": "int", "min": min.to_string(), "max": max.to_string()})
            }
            Self::Sum { type_id, variants } => {
                json!({"kind": "sum", "type_id": type_id, "variants": variants})
            }
        }
    }
}

/// The domain of a scalar type; text has none.
pub(super) fn leaf_domain(
    declarations: &Declarations,
    type_id: u32,
) -> Result<LeafDomain, ContractError> {
    let wide = || {
        ContractError::new(
            "project.zeno",
            format!("type {type_id} range exceeds the 64-bit program range"),
        )
    };
    Ok(match declarations.kind(type_id)? {
        TypeKind::Bool => LeafDomain::Bool,
        TypeKind::I128 { min, max } => LeafDomain::Int {
            min: i64::try_from(min).map_err(|_| wide())?,
            max: i64::try_from(max).map_err(|_| wide())?,
        },
        TypeKind::Sum(variants) => LeafDomain::Sum {
            type_id,
            variants: variants.iter().map(|variant| variant.id).collect(),
        },
        TypeKind::Text => {
            return Err(ContractError::new(
                "project.zeno",
                format!("text type {type_id} has no input domain"),
            ));
        }
    })
}

/// One program input position: a root field, or a whole scalar root.
#[derive(Clone, Debug)]
pub(super) struct Position {
    /// The rules name: `pre.100.110`, `command.101`, `context.102.130`.
    pub(super) name: String,
    pub(super) source: Source,
    pub(super) field: Option<u16>,
    pub(super) type_id: u32,
    pub(super) domain: LeafDomain,
}

impl Position {
    pub(super) fn json(&self) -> Value {
        json!({
            "name": self.name,
            "source": match self.source {
                Source::State => "state",
                Source::Command => "command",
                Source::Context => "context",
            },
            "field": self.field,
            "type": self.type_id,
            "domain": self.domain.json(),
        })
    }
}

/// The program input positions in the order the program, and every decision
/// example, reads them.
pub(super) fn positions(declarations: &Declarations) -> Result<Vec<Position>, ContractError> {
    declarations
        .inputs()?
        .into_iter()
        .map(|input| {
            Ok(Position {
                domain: leaf_domain(declarations, input.type_id)?,
                name: input.name,
                source: input.source,
                field: input.field,
                type_id: input.type_id,
            })
        })
        .collect()
}

/// The number of tuples over the whole domain; `None` when it exceeds `u128`.
pub(super) fn domain_size(positions: &[Position]) -> Option<u128> {
    positions.iter().try_fold(1u128, |size, position| {
        size.checked_mul(position.domain.size()?)
    })
}

/// How the input set was built.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Construction {
    /// Every tuple of the admitted domain.
    FullDomain,
    /// Every tuple of the per-position boundary sets.
    BoundaryProduct,
    /// Base points varied in one and in two positions over the boundary sets.
    BoundaryProbes,
}

impl Construction {
    pub(super) fn name(self) -> &'static str {
        match self {
            Self::FullDomain => "full-domain",
            Self::BoundaryProduct => "boundary-product",
            Self::BoundaryProbes => "boundary-probes",
        }
    }
}

/// How a boundary set was built.
#[derive(Debug)]
pub(super) struct Boundary {
    /// Integer constants by the type they were used with.
    pub(super) constants: BTreeMap<u32, BTreeSet<i64>>,
    /// Boundary values of each position.
    pub(super) values: Vec<Vec<i64>>,
    /// Probe bases with their origin; empty for a boundary product.
    pub(super) bases: Vec<(String, Vec<i64>)>,
    /// Probes that vary one position, then two.
    pub(super) singles: usize,
    pub(super) pairs: usize,
    /// Whether the tuple limit stopped the probes.
    pub(super) truncated: bool,
}

/// The tuples a review evaluates, flat, `width` values each.
#[derive(Debug)]
pub(super) struct InputSet {
    pub(super) construction: Construction,
    pub(super) domain_size: Option<u128>,
    pub(super) width: usize,
    values: Vec<i64>,
    pub(super) boundary: Option<Boundary>,
}

impl InputSet {
    pub(super) fn len(&self) -> usize {
        self.values.len().checked_div(self.width).unwrap_or(0)
    }

    pub(super) fn tuple(&self, ordinal: usize) -> &[i64] {
        &self.values[ordinal * self.width..(ordinal + 1) * self.width]
    }

    pub(super) fn tuples(&self) -> impl Iterator<Item = &[i64]> {
        self.values.chunks_exact(self.width.max(1))
    }
}

/// Builds the input set: the whole domain, the boundary product, or probes,
/// the first that fits `max_tuples`.
pub(super) fn input_set(
    positions: &[Position],
    rules: &Rules,
    declarations: &Declarations,
    examples: &[&[i64]],
    max_tuples: u64,
) -> Result<InputSet, ContractError> {
    let width = positions.len();
    let domain_size = domain_size(positions);
    let limit = u128::from(max_tuples);
    if let Some(size) = domain_size
        && size <= limit
    {
        let lists: Vec<Vec<i64>> = positions
            .iter()
            .map(|position| position.domain.values())
            .collect();
        return Ok(InputSet {
            construction: Construction::FullDomain,
            domain_size,
            width,
            values: product(&lists),
            boundary: None,
        });
    }
    let constants = constants(rules, declarations, positions)?;
    let values: Vec<Vec<i64>> = positions
        .iter()
        .map(|position| boundary_values(position, &constants))
        .collect();
    let product_size = values
        .iter()
        .try_fold(1u128, |size, list| size.checked_mul(list.len() as u128));
    if let Some(size) = product_size
        && size <= limit
    {
        return Ok(InputSet {
            construction: Construction::BoundaryProduct,
            domain_size,
            width,
            values: product(&values),
            boundary: Some(Boundary {
                constants,
                values,
                bases: Vec::new(),
                singles: 0,
                pairs: 0,
                truncated: false,
            }),
        });
    }
    let mut probes = Probes::new(width, max_tuples);
    let mut bases = Vec::new();
    let genesis: Vec<i64> = positions
        .iter()
        .enumerate()
        .map(
            |(index, position)| match (position.source, position.field) {
                (Source::State, Some(field)) => match rules.genesis.get(&field) {
                    Some(Constant::Bool(value)) => i64::from(*value),
                    Some(Constant::Int(value)) => i64::try_from(*value).unwrap_or(values[index][0]),
                    None => values[index][0],
                },
                _ => values[index][0],
            },
        )
        .collect();
    bases.push(("genesis".to_owned(), genesis));
    for (index, example) in examples.iter().enumerate() {
        if example.len() == width && !bases.iter().any(|(_, base)| base == example) {
            bases.push((format!("example {}", index + 1), example.to_vec()));
        }
    }
    for (_, base) in &bases {
        probes.push(base);
    }
    let mut singles = 0;
    'singles: for (_, base) in &bases {
        for (index, list) in values.iter().enumerate() {
            for value in list {
                if *value == base[index] {
                    continue;
                }
                let mut tuple = base.clone();
                tuple[index] = *value;
                if probes.push(&tuple) {
                    singles += 1;
                }
                if probes.full() {
                    break 'singles;
                }
            }
        }
    }
    let mut pairs = 0;
    'pairs: for (_, base) in &bases {
        for first in 0..width {
            for second in first + 1..width {
                for left in &values[first] {
                    if *left == base[first] {
                        continue;
                    }
                    for right in &values[second] {
                        if *right == base[second] {
                            continue;
                        }
                        let mut tuple = base.clone();
                        tuple[first] = *left;
                        tuple[second] = *right;
                        if probes.push(&tuple) {
                            pairs += 1;
                        }
                        if probes.full() {
                            break 'pairs;
                        }
                    }
                }
            }
        }
    }
    let truncated = probes.full();
    Ok(InputSet {
        construction: Construction::BoundaryProbes,
        domain_size,
        width,
        values: probes.values,
        boundary: Some(Boundary {
            constants,
            values,
            bases,
            singles,
            pairs,
            truncated,
        }),
    })
}

/// Distinct probes in insertion order, up to the tuple limit.
struct Probes {
    width: usize,
    limit: usize,
    seen: BTreeSet<Vec<i64>>,
    values: Vec<i64>,
}

impl Probes {
    fn new(width: usize, max_tuples: u64) -> Self {
        Self {
            width,
            limit: usize::try_from(max_tuples).unwrap_or(usize::MAX),
            seen: BTreeSet::new(),
            values: Vec::new(),
        }
    }

    fn full(&self) -> bool {
        self.seen.len() >= self.limit
    }

    /// Adds a tuple not seen before; `true` when it was added.
    fn push(&mut self, tuple: &[i64]) -> bool {
        if self.full() || tuple.len() != self.width || !self.seen.insert(tuple.to_vec()) {
            return false;
        }
        self.values.extend_from_slice(tuple);
        true
    }
}

/// Every tuple over the lists, in order, the last position fastest.
fn product(lists: &[Vec<i64>]) -> Vec<i64> {
    if lists.iter().any(Vec::is_empty) {
        return Vec::new();
    }
    let count = lists.iter().map(Vec::len).product::<usize>();
    let mut values = Vec::with_capacity(count * lists.len());
    let mut indexes = vec![0usize; lists.len()];
    for _ in 0..count {
        values.extend(indexes.iter().zip(lists).map(|(index, list)| list[*index]));
        for position in (0..lists.len()).rev() {
            indexes[position] += 1;
            if indexes[position] < lists[position].len() {
                break;
            }
            indexes[position] = 0;
        }
    }
    values
}

/// The boundary values of one position: both booleans, every variant, or
/// the endpoints and each constant of the type with its neighbours.
fn boundary_values(position: &Position, constants: &BTreeMap<u32, BTreeSet<i64>>) -> Vec<i64> {
    match &position.domain {
        LeafDomain::Int { min, max } => {
            let mut values = BTreeSet::from([*min, *max]);
            for constant in constants.get(&position.type_id).into_iter().flatten() {
                for value in [
                    constant.checked_sub(1),
                    Some(*constant),
                    constant.checked_add(1),
                ]
                .into_iter()
                .flatten()
                {
                    if (*min..=*max).contains(&value) {
                        values.insert(value);
                    }
                }
            }
            values.into_iter().collect()
        }
        domain => domain.values(),
    }
}

/// Integer constants by the type they are used with: compared with, added to
/// or subtracted from a value of the type, assigned to a field of the type,
/// or a genesis value of the type.
fn constants(
    rules: &Rules,
    declarations: &Declarations,
    positions: &[Position],
) -> Result<BTreeMap<u32, BTreeSet<i64>>, ContractError> {
    let mut found = BTreeMap::new();
    let expand = |ast: &Ast, place: String| {
        expr::expand(ast, &rules.variables)
            .map_err(|reason| ContractError::new(format!("v2/policy.json {place}"), reason))
    };
    let state_fields = declarations.state_fields()?;
    for (index, case) in rules.cases.iter().enumerate() {
        let when = expand(&case.when, format!("cases[{index}].when"))?;
        collect(&when, &[], positions, &mut found);
        for (field, value) in &case.post {
            let type_id: Vec<u32> = state_fields
                .iter()
                .filter(|declared| declared.id == *field)
                .map(|declared| declared.type_id)
                .collect();
            let value = expand(value, format!("cases[{index}].post.{field}"))?;
            collect(&value, &type_id, positions, &mut found);
        }
        for (number, delivery) in case.outbox.iter().enumerate() {
            let channel = declarations.channel(delivery.channel)?;
            let payload = declarations.fields(channel.payload)?;
            for (field, value) in &delivery.payload {
                let type_id: Vec<u32> = payload
                    .iter()
                    .filter(|declared| declared.id == *field)
                    .map(|declared| declared.type_id)
                    .collect();
                let value = expand(
                    value,
                    format!("cases[{index}].outbox[{number}].payload.{field}"),
                )?;
                collect(&value, &type_id, positions, &mut found);
            }
        }
    }
    for (field, value) in &rules.genesis {
        if let (Constant::Int(value), Some(declared)) = (
            value,
            state_fields.iter().find(|declared| declared.id == *field),
        ) && let Ok(value) = i64::try_from(*value)
        {
            found
                .entry(declared.type_id)
                .or_insert_with(BTreeSet::new)
                .insert(value);
        }
    }
    Ok(found)
}

/// The type of an expression's value, when one of its inputs decides it.
fn type_of(ast: &Ast, positions: &[Position]) -> Option<u32> {
    match ast {
        Ast::Name(name) => positions
            .iter()
            .find(|position| position.name == *name)
            .map(|position| position.type_id),
        Ast::Neg(inner) => type_of(inner, positions),
        Ast::Binary(Binary::Add | Binary::Sub | Binary::Mul, left, right)
        | Ast::Div(_, left, right) => {
            type_of(left, positions).or_else(|| type_of(right, positions))
        }
        Ast::Choose(_, then, otherwise) => {
            type_of(then, positions).or_else(|| type_of(otherwise, positions))
        }
        _ => None,
    }
}

/// Records each integer constant under every type of value it is used with:
/// the other side of a comparison or arithmetic operator, and the field an
/// expression is assigned to.
fn collect(
    ast: &Ast,
    expected: &[u32],
    positions: &[Position],
    found: &mut BTreeMap<u32, BTreeSet<i64>>,
) {
    let with = |other: &Ast| {
        let mut hints = expected.to_vec();
        hints.extend(type_of(other, positions));
        hints
    };
    match ast {
        Ast::Int(value) => {
            if let Ok(value) = i64::try_from(*value) {
                for type_id in expected {
                    found.entry(*type_id).or_default().insert(value);
                }
            }
        }
        Ast::Bool(_) | Ast::Name(_) => {}
        Ast::Not(inner) => collect(inner, &[], positions, found),
        Ast::Neg(inner) => collect(inner, expected, positions, found),
        Ast::Binary(operator, left, right) => match operator {
            Binary::Eq | Binary::Ne | Binary::Lt | Binary::Le | Binary::Gt | Binary::Ge => {
                collect(
                    left,
                    &type_of(right, positions).into_iter().collect::<Vec<_>>(),
                    positions,
                    found,
                );
                collect(
                    right,
                    &type_of(left, positions).into_iter().collect::<Vec<_>>(),
                    positions,
                    found,
                );
            }
            Binary::Add | Binary::Sub | Binary::Mul => {
                collect(left, &with(right), positions, found);
                collect(right, &with(left), positions, found);
            }
            Binary::And | Binary::Or | Binary::Implies => {
                collect(left, &[], positions, found);
                collect(right, &[], positions, found);
            }
        },
        Ast::Div(_, left, right) => {
            collect(left, &with(right), positions, found);
            collect(right, &with(left), positions, found);
        }
        Ast::Choose(condition, then, otherwise) => {
            collect(condition, &[], positions, found);
            collect(then, &with(otherwise), positions, found);
            collect(otherwise, &with(then), positions, found);
        }
    }
}
