//! The `.zeno` declarations a contract binds: types with their leaf bindings,
//! the three roots, channels, reasons and scoped laws.

use std::collections::BTreeMap;

use zeno_fcis_spec::{
    ClaimDecl, LawScope, ProjectLimits, SourceLimits, TypeKind, elaborate_project, parse_project,
};

use super::ContractError;
use super::expr::{self, Ast};

/// State, command and context root types.
pub(super) const ROOTS: [(Source, u32); 3] = [
    (Source::State, 100),
    (Source::Command, 101),
    (Source::Context, 102),
];

/// A scalar leaf: declared by an `int` range or bound by the rules file.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Leaf {
    Bool,
    I128 { min: i128, max: i128 },
    Text { min: u32, max: u32 },
}

#[derive(Debug)]
pub(super) enum Form {
    Leaf(Leaf),
    Record(Vec<Field>),
    Sum(Vec<Variant>),
}

#[derive(Debug)]
pub(super) struct Type {
    pub(super) name: String,
    pub(super) form: Form,
}

#[derive(Debug)]
pub(super) struct Field {
    pub(super) id: u16,
    pub(super) name: String,
    pub(super) type_id: u32,
}

#[derive(Debug)]
pub(super) struct Variant {
    pub(super) id: u16,
    pub(super) name: String,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct Channel {
    pub(super) id: u32,
    pub(super) destination: u32,
    pub(super) payload: u32,
}

#[derive(Debug)]
pub(super) struct Law {
    pub(super) id: u32,
    /// The law's name, which is not part of the contract.
    pub(super) name: String,
    pub(super) scope: LawScope,
    pub(super) genesis: bool,
    pub(super) formula: Ast,
}

/// The original input a scalar program position reads.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(super) enum Source {
    State,
    Command,
    Context,
}

impl Source {
    /// The name prefix rules and laws use: `pre`, `command` or `context`.
    pub(super) fn prefix(self) -> &'static str {
        match self {
            Self::State => "pre",
            Self::Command => "command",
            Self::Context => "context",
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::State => "state",
            Self::Command => "command",
            Self::Context => "context",
        }
    }
}

/// One scalar program input: a root field, or a whole scalar root.
#[derive(Debug)]
pub(super) struct Input {
    pub(super) name: String,
    pub(super) source: Source,
    pub(super) field: Option<u16>,
    pub(super) type_id: u32,
}

/// A scalar type's kind, which decides its atoms, domains and input leaves.
#[derive(Clone, Copy, Debug)]
pub(super) enum Kind<'d> {
    Bool,
    I128 { min: i128, max: i128 },
    Text,
    Sum(&'d [Variant]),
}

#[derive(Debug)]
pub(super) struct Declarations {
    pub(super) profile: String,
    pub(super) types: BTreeMap<u32, Type>,
    pub(super) channels: Vec<Channel>,
    pub(super) reasons: Vec<u32>,
    pub(super) laws: Vec<Law>,
    /// Declarations that are not part of the contract, kept for reports such
    /// as `contract diff`.
    pub(super) notes: Notes,
}

/// What `project.zeno` declares beside the contract: the names of channels
/// and reasons, reason precedences and the claims. None of it reaches the
/// schema or the policy.
#[derive(Debug, Default)]
pub(super) struct Notes {
    /// Channel ID to name.
    pub(super) channels: BTreeMap<u32, String>,
    /// Reason ID to name and precedence.
    pub(super) reasons: BTreeMap<u32, (String, u32)>,
    /// The claims, in ID order.
    pub(super) claims: Vec<ClaimDecl>,
}

impl Declarations {
    /// Parses and elaborates `project.zeno`, then gives every type exactly one
    /// form: fields, payload-free variants, a declared `int` range, or a leaf
    /// binding from the rules file.
    pub(super) fn read(
        source: &str,
        bindings: &BTreeMap<u32, Leaf>,
    ) -> Result<Self, ContractError> {
        let invalid = |reason: String| ContractError::new("project.zeno", reason);
        let parsed = parse_project(source, SourceLimits::default())
            .map_err(|diagnostics| invalid(diagnostics.to_string()))?;
        let project = elaborate_project(parsed, ProjectLimits::default())
            .map_err(|diagnostics| invalid(diagnostics.to_string()))?;
        if let Some(effect) = project.effects().first() {
            return Err(invalid(format!(
                "effect {} has no contract form; declare a channel",
                effect.id().get()
            )));
        }
        for binding in bindings.keys() {
            if !project
                .types()
                .iter()
                .any(|item| item.id().get() == *binding)
            {
                return Err(ContractError::new(
                    "v2/policy.json leaf_bindings",
                    format!("type {binding} is not declared"),
                ));
            }
        }
        let mut types = BTreeMap::new();
        for declared in project.types() {
            let id = declared.id().get();
            let place = || format!("project.zeno type {id}");
            let fields = project
                .fields()
                .iter()
                .filter(|field| field.owner() == declared.id())
                .map(|field| {
                    Ok(Field {
                        id: small_id(field.id().get(), &place)?,
                        name: field.name().as_str().to_owned(),
                        type_id: field.field_type().get(),
                    })
                })
                .collect::<Result<Vec<_>, ContractError>>()?;
            let variants = project
                .variants()
                .iter()
                .filter(|variant| variant.owner() == declared.id())
                .map(|variant| {
                    if variant.payload_type().is_some() {
                        return Err(ContractError::new(
                            place(),
                            "variants with payloads have no contract form",
                        ));
                    }
                    Ok(Variant {
                        id: small_id(variant.id().get(), &place)?,
                        name: variant.name().as_str().to_owned(),
                    })
                })
                .collect::<Result<Vec<_>, ContractError>>()?;
            // A binding may restate a declared range but not change it, as
            // in schema lowering.
            let leaf = match (declared.range(), bindings.get(&id)) {
                (Some(range), binding) => {
                    let declared = Leaf::I128 {
                        min: range.min(),
                        max: range.max(),
                    };
                    if binding.is_some_and(|binding| *binding != declared) {
                        return Err(ContractError::new(
                            place(),
                            "its leaf binding differs from its declared range",
                        ));
                    }
                    Some(declared)
                }
                (None, binding) => binding.copied(),
            };
            match (declared.kind(), leaf) {
                (TypeKind::Bool, Some(leaf)) if leaf != Leaf::Bool => {
                    return Err(ContractError::new(
                        place(),
                        "is `bool` but bound to another leaf",
                    ));
                }
                (TypeKind::Int, Some(Leaf::Bool | Leaf::Text { .. })) => {
                    return Err(ContractError::new(
                        place(),
                        "is `int` but bound to another leaf",
                    ));
                }
                _ => {}
            }
            let form = match (leaf, fields.is_empty(), variants.is_empty()) {
                (Some(leaf), true, true) => Form::Leaf(leaf),
                (None, false, true) => Form::Record(fields),
                (None, true, false) => Form::Sum(variants),
                (None, true, true) => {
                    return Err(ContractError::new(
                        place(),
                        "needs fields, variants, a declared range or a leaf binding",
                    ));
                }
                _ => {
                    return Err(ContractError::new(
                        place(),
                        "has more than one of fields, variants and a leaf",
                    ));
                }
            };
            types.insert(
                id,
                Type {
                    name: declared.name().as_str().to_owned(),
                    form,
                },
            );
        }
        for (source, id) in ROOTS {
            let expected = match source {
                Source::State => TypeKind::State,
                Source::Command => TypeKind::Command,
                Source::Context => TypeKind::Context,
            };
            if !project
                .types()
                .iter()
                .any(|item| item.id().get() == id && item.kind() == expected)
            {
                return Err(invalid(format!(
                    "type {id} must be the {} root",
                    source.label()
                )));
            }
        }
        let laws = project
            .laws()
            .iter()
            .map(|law| {
                let id = law.id().get();
                let place = format!("project.zeno law {id}");
                let applicability = law.applicability().ok_or_else(|| {
                    ContractError::new(
                        &place,
                        "needs a scope: `on any|accept|reject|failure|commit`",
                    )
                })?;
                let formula = expr::law(law.formula()).map_err(|reason| {
                    ContractError::new(&place, format!("{reason} has no contract form"))
                })?;
                Ok(Law {
                    id,
                    name: law.name().as_str().to_owned(),
                    scope: applicability.scope(),
                    genesis: applicability.genesis(),
                    formula,
                })
            })
            .collect::<Result<Vec<_>, ContractError>>()?;
        Ok(Self {
            profile: project.name().as_str().to_owned(),
            types,
            channels: project
                .channels()
                .iter()
                .map(|channel| Channel {
                    id: channel.id().get(),
                    destination: channel.destination_type().get(),
                    payload: channel.payload_type().get(),
                })
                .collect(),
            reasons: project
                .reasons()
                .iter()
                .map(|reason| reason.id().get())
                .collect(),
            laws,
            notes: Notes {
                channels: project
                    .channels()
                    .iter()
                    .map(|channel| (channel.id().get(), channel.name().as_str().to_owned()))
                    .collect(),
                reasons: project
                    .reasons()
                    .iter()
                    .map(|reason| {
                        (
                            reason.id().get(),
                            (reason.name().as_str().to_owned(), reason.precedence()),
                        )
                    })
                    .collect(),
                claims: project.claims().to_vec(),
            },
        })
    }

    pub(super) fn get(&self, id: u32) -> Result<&Type, ContractError> {
        self.types
            .get(&id)
            .ok_or_else(|| ContractError::new("project.zeno", format!("type {id} is not declared")))
    }

    /// The fields of a record type; an error for other forms.
    pub(super) fn fields(&self, id: u32) -> Result<&[Field], ContractError> {
        match &self.get(id)?.form {
            Form::Record(fields) => Ok(fields),
            _ => Err(ContractError::new(
                "project.zeno",
                format!("type {id} must be a record"),
            )),
        }
    }

    /// The scalar kind of a type; records have none.
    pub(super) fn kind(&self, id: u32) -> Result<Kind<'_>, ContractError> {
        Ok(match &self.get(id)?.form {
            Form::Leaf(Leaf::Bool) => Kind::Bool,
            Form::Leaf(Leaf::I128 { min, max }) => Kind::I128 {
                min: *min,
                max: *max,
            },
            Form::Leaf(Leaf::Text { .. }) => Kind::Text,
            Form::Sum(variants) => Kind::Sum(variants),
            Form::Record(_) => {
                return Err(ContractError::new(
                    "project.zeno",
                    format!("type {id} is a record where a scalar is needed"),
                ));
            }
        })
    }

    /// Program inputs in order: each root's fields, or the root itself.
    pub(super) fn inputs(&self) -> Result<Vec<Input>, ContractError> {
        let mut inputs = Vec::new();
        for (source, root) in ROOTS {
            match &self.get(root)?.form {
                Form::Record(fields) => inputs.extend(fields.iter().map(|field| Input {
                    name: format!("{}.{root}.{}", source.prefix(), field.id),
                    source,
                    field: Some(field.id),
                    type_id: field.type_id,
                })),
                _ => inputs.push(Input {
                    name: format!("{}.{root}", source.prefix()),
                    source,
                    field: None,
                    type_id: root,
                }),
            }
        }
        Ok(inputs)
    }

    /// The state fields with their types; empty when the state is a scalar.
    pub(super) fn state_fields(&self) -> Result<&[Field], ContractError> {
        Ok(match &self.get(ROOTS[0].1)?.form {
            Form::Record(fields) => fields,
            _ => &[],
        })
    }

    pub(super) fn channel(&self, id: u32) -> Result<Channel, ContractError> {
        self.channels
            .iter()
            .find(|channel| channel.id == id)
            .copied()
            .ok_or_else(|| {
                ContractError::new("project.zeno", format!("channel {id} is not declared"))
            })
    }
}

fn small_id(id: u32, place: &dyn Fn() -> String) -> Result<u16, ContractError> {
    u16::try_from(id).map_err(|_| ContractError::new(place(), format!("ID {id} exceeds 65535")))
}
