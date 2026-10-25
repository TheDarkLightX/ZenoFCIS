//! Pure diagnostic expressions over actual decisions. Arithmetic is executed
//! by the library's existing scalar interpreter, never by a second evaluator.
use std::collections::BTreeMap;

use zeno_fcis_contract::internals::{
    declarations::{Declarations, Form, Input, Leaf, Notes, Source, Type},
    expr,
    graph::{Graph, Kind, ScalarGraph},
    policy,
};
use zeno_fcis_synthesis::finite::{Domain, Program};

use super::super::ContractError;
use super::{evaluate::Delivery, machine::Loaded};

pub(super) struct Expression {
    program: Program,
    names: Vec<String>,
}

impl Expression {
    pub(super) fn compile(
        text: &str,
        names: &BTreeMap<String, bool>,
        boolean: bool,
    ) -> Result<Self, ContractError> {
        let error = |e: String| ContractError::new("observation expression", e);
        let declarations = Declarations {
            profile: String::new(),
            types: BTreeMap::from([
                (
                    1,
                    Type {
                        name: "integer".into(),
                        form: Form::Leaf(Leaf::I128 {
                            min: i128::MIN,
                            max: i128::MAX,
                        }),
                        quantity: None,
                    },
                ),
                (
                    2,
                    Type {
                        name: "boolean".into(),
                        form: Form::Leaf(Leaf::Bool),
                        quantity: None,
                    },
                ),
            ]),
            channels: Vec::new(),
            reasons: Vec::new(),
            laws: Vec::new(),
            notes: Notes::default(),
        };
        // Only names occurring in the expression consume interpreter inputs.
        let used = text
            .split(|c: char| !(c.is_ascii_alphanumeric() || c == '_' || c == '.'))
            .filter(|n| names.contains_key(*n))
            .map(str::to_owned)
            .collect::<std::collections::BTreeSet<_>>();
        let inputs: Vec<Input> = used
            .iter()
            .map(|name| Input {
                name: name.clone(),
                source: Source::State,
                field: None,
                type_id: if names[name] { 2 } else { 1 },
            })
            .collect();
        // Cap input before constructing a tree; then use the existing bounded
        // expansion admission for depth and work, before recursive compiling.
        if text.len() > 4096 {
            return Err(error("expression exceeds 4096 bytes".into()));
        }
        let ast = expr::parse(text).map_err(error)?;
        let ast = expr::expand_with_limits(&ast, &BTreeMap::new(), true).map_err(error)?;
        let mut graph = ScalarGraph::new(&declarations, &inputs);
        let root = graph.compile_raw(&ast).map_err(error)?;
        if (root.kind == Kind::Bool) != boolean || !matches!(root.kind, Kind::Bool | Kind::Int) {
            return Err(error("expression has the wrong result type".into()));
        }
        let integer = Domain::Int {
            min: i128::MIN,
            max: i128::MAX,
        };
        let program = Program::try_new_extended(
            used.iter()
                .map(|n| if names[n] { Domain::Bool } else { integer })
                .collect(),
            vec![if boolean { Domain::Bool } else { integer }],
            graph
                .table
                .nodes
                .iter()
                .map(policy::scalar_op)
                .collect::<Result<_, _>>()?,
            vec![policy::small(root.index)?],
        )
        .map_err(|e| error(format!("library refused expression: {e:?}")))?;
        Ok(Self {
            program,
            names: used.into_iter().collect(),
        })
    }

    pub(super) fn evaluate(&self, values: &BTreeMap<String, i128>) -> Option<i128> {
        let inputs = self
            .names
            .iter()
            .map(|n| values.get(n).copied())
            .collect::<Option<Vec<_>>>()?;
        self.program.evaluate(&inputs).ok()?.first().copied()
    }
}

/// Every diagnostic name has a fixed meaning even when no delivery exists.
pub(super) fn names(loaded: &Loaded) -> BTreeMap<String, bool> {
    let mut names = BTreeMap::new();
    for p in &loaded.positions {
        names.insert(
            p.name.clone(),
            matches!(p.domain, super::domain::LeafDomain::Bool),
        );
        if p.source == Source::State {
            names.insert(
                p.name.replacen("pre.", "post.", 1),
                matches!(p.domain, super::domain::LeafDomain::Bool),
            );
        }
    }
    for n in [
        "verdict.accept",
        "verdict.reject",
        "verdict.failure",
        "verdict.refused",
    ] {
        names.insert(n.into(), true);
    }
    names.insert("outbox.count".into(), false);
    let maximum = loaded
        .rules
        .cases
        .iter()
        .map(|c| c.outbox.len())
        .max()
        .unwrap_or(0);
    for index in 0..maximum {
        names.insert(format!("delivery.{index}.exists"), true);
        for field in ["channel", "ordinal", "idempotency"] {
            names.insert(format!("delivery.{index}.{field}"), false);
        }
        for c in &loaded.declarations.channels {
            for f in loaded.declarations.fields(c.payload).into_iter().flatten() {
                names.insert(format!("delivery.{index}.field.{}", f.id), false);
            }
        }
    }
    for c in &loaded.declarations.channels {
        names.insert(format!("outbox.{}", c.id), false);
        if let Ok(fields) = loaded.declarations.fields(c.payload) {
            for f in fields {
                names.insert(format!("outbox.{}.{}", c.id, f.id), false);
            }
        }
    }
    // Preserve the old payload-qualified shorthand only when unambiguous.
    for c in &loaded.declarations.channels {
        if loaded
            .declarations
            .channels
            .iter()
            .filter(|other| other.payload == c.payload)
            .count()
            == 1
        {
            names.insert(format!("outbox.{}", c.payload), false);
            if let Ok(fields) = loaded.declarations.fields(c.payload) {
                for f in fields {
                    names.insert(format!("outbox.{}.{}", c.payload, f.id), false);
                }
            }
        }
    }
    names
}

pub(super) fn deliveries(
    loaded: &Loaded,
    outbox: &[Delivery],
    values: &mut BTreeMap<String, i128>,
) -> Option<()> {
    values.insert("outbox.count".into(), outbox.len() as i128);
    for c in &loaded.declarations.channels {
        let selected = outbox
            .iter()
            .filter(|d| d.channel == c.id)
            .collect::<Vec<_>>();
        let count = selected.len() as i128;
        let mut fields = Vec::new();
        for f in loaded.declarations.fields(c.payload).ok()? {
            let total = selected.iter().try_fold(0i128, |sum, d| {
                sum.checked_add(d.payload.iter().find(|(id, _)| *id == f.id)?.1.number()?)
            });
            if let Some(total) = total {
                fields.push((f.id, total));
            }
        }
        let mut ids = vec![c.id];
        if loaded
            .declarations
            .channels
            .iter()
            .filter(|other| other.payload == c.payload)
            .count()
            == 1
        {
            ids.push(c.payload);
        }
        for id in ids {
            values.insert(format!("outbox.{id}"), count);
            for (f, total) in &fields {
                values.insert(format!("outbox.{id}.{f}"), *total);
            }
        }
    }
    let maximum = loaded
        .rules
        .cases
        .iter()
        .map(|c| c.outbox.len())
        .max()
        .unwrap_or(0);
    for index in 0..maximum {
        let delivery = outbox.get(index);
        values.insert(
            format!("delivery.{index}.exists"),
            i128::from(delivery.is_some()),
        );
        values.insert(
            format!("delivery.{index}.channel"),
            delivery.map_or(0, |d| i128::from(d.channel)),
        );
        values.insert(
            format!("delivery.{index}.ordinal"),
            delivery.map_or(0, |d| i128::from(d.ordinal)),
        );
        if let Some(n) = delivery.map_or(Some(0), |d| d.idempotency.number()) {
            values.insert(format!("delivery.{index}.idempotency"), n);
        }
        for c in &loaded.declarations.channels {
            for f in loaded.declarations.fields(c.payload).into_iter().flatten() {
                if let Some(n) = delivery.map_or(Some(0), |d| {
                    d.payload
                        .iter()
                        .find(|(id, _)| *id == f.id)
                        .map_or(Some(0), |(_, v)| v.number())
                }) {
                    values.insert(format!("delivery.{index}.field.{}", f.id), n);
                }
            }
        }
    }
    Some(())
}

pub(super) fn values(
    loaded: &Loaded,
    machine: &super::machine::Machine,
    ordinal: usize,
) -> Option<BTreeMap<String, i128>> {
    use super::super::rules::Class;
    use super::machine::Observed;
    let outcome = machine.observed[ordinal];
    let state = ordinal / machine.space.inputs;
    let mut values = loaded
        .positions
        .iter()
        .zip(machine.space.at(ordinal))
        .map(|(p, n)| (p.name.clone(), n))
        .collect::<BTreeMap<_, _>>();
    for (p, n) in loaded
        .positions
        .iter()
        .zip(machine.space.state(outcome.after(state)))
    {
        values.insert(p.name.replacen("pre.", "post.", 1), n);
    }
    for (name, yes) in [
        (
            "accept",
            matches!(
                outcome,
                Observed::Decided {
                    class: Class::Accept,
                    ..
                }
            ),
        ),
        (
            "reject",
            matches!(
                outcome,
                Observed::Decided {
                    class: Class::Reject,
                    ..
                }
            ),
        ),
        (
            "failure",
            matches!(
                outcome,
                Observed::Decided {
                    class: Class::CommittedFailure,
                    ..
                }
            ),
        ),
        ("refused", matches!(outcome, Observed::Refused { .. })),
    ] {
        values.insert(format!("verdict.{name}"), i128::from(yes));
    }
    deliveries(loaded, machine.outbox_at(ordinal), &mut values)?;
    Some(values)
}
