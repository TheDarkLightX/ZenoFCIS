//! The `.zeno` declarations a contract binds: types with their leaf bindings,
//! the three roots, channels, reasons and scoped laws.

use std::collections::BTreeMap;

use zeno_fcis_spec::{
    ClaimDecl, LawScope, ProjectLimits, SourceLimits, TypeKind, elaborate_project, parse_project,
};

use super::ContractError;
use super::expr::{self, Ast};

/// State, command and context root types.
pub const ROOTS: [(Source, u32); 3] = [
    (Source::State, 100),
    (Source::Command, 101),
    (Source::Context, 102),
];

/// A scalar leaf: declared by an `int` range or bound by the rules file.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Leaf {
    Bool,
    I128 { min: i128, max: i128 },
    U128 { min: u128, max: u128 },
    Text { min: u32, max: u32 },
}

#[derive(Debug)]
pub enum Form {
    Leaf(Leaf),
    Record(Vec<Field>),
    Sum(Vec<Variant>),
}

#[derive(Debug)]
pub struct Type {
    pub name: String,
    pub form: Form,
    pub quantity: Option<zeno_fcis_spec::Quantity>,
}

#[derive(Debug)]
pub struct Field {
    pub id: u16,
    pub name: String,
    pub type_id: u32,
}

#[derive(Debug)]
pub struct Variant {
    pub id: u16,
    pub name: String,
}

#[derive(Clone, Copy, Debug)]
pub struct Channel {
    pub id: u32,
    pub destination: u32,
    pub payload: u32,
}

#[derive(Debug)]
pub struct Law {
    pub id: u32,
    /// The law's name, which is not part of the contract.
    pub name: String,
    pub scope: LawScope,
    pub genesis: bool,
    pub formula: Ast,
}

/// The original input a scalar program position reads.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum Source {
    State,
    Command,
    Context,
}

impl Source {
    /// The name prefix rules and laws use: `pre`, `command` or `context`.
    pub fn prefix(self) -> &'static str {
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
pub struct Input {
    pub name: String,
    pub source: Source,
    pub field: Option<u16>,
    pub type_id: u32,
}

/// A scalar type's kind, which decides its atoms, domains and input leaves.
#[derive(Clone, Copy, Debug)]
pub enum Kind<'d> {
    Bool,
    I128 { min: i128, max: i128 },
    U128 { min: u128, max: u128 },
    Text,
    Sum(&'d [Variant]),
}

#[derive(Debug)]
pub struct Declarations {
    pub profile: String,
    pub types: BTreeMap<u32, Type>,
    pub channels: Vec<Channel>,
    pub reasons: Vec<u32>,
    pub laws: Vec<Law>,
    /// Declarations that are not part of the contract, kept for reports such
    /// as `contract diff`.
    pub notes: Notes,
}

/// What `project.zeno` declares beside the contract: the names of channels
/// and reasons, reason precedences and the claims. None of it reaches the
/// schema or the policy.
#[derive(Debug, Default)]
pub struct Notes {
    /// Channel ID to name.
    pub channels: BTreeMap<u32, String>,
    /// Reason ID to name and precedence.
    pub reasons: BTreeMap<u32, (String, u32)>,
    /// The claims, in ID order.
    pub claims: Vec<ClaimDecl>,
}

impl Declarations {
    /// Parses and elaborates `project.zeno`, then gives every type exactly one
    /// form: fields, payload-free variants, a declared `int` range, or a leaf
    /// binding from the rules file.
    pub fn read(source: &str, bindings: &BTreeMap<u32, Leaf>) -> Result<Self, ContractError> {
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
            let leaf = if let Some((min, max)) = declared.unsigned_range() {
                let leaf = Leaf::U128 { min, max };
                if bindings.get(&id).is_some_and(|v| *v != leaf) {
                    return Err(invalid("U128 binding mismatch".into()));
                }
                Some(leaf)
            } else {
                match (declared.range(), bindings.get(&id)) {
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
                }
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
                (None, true, true) if id == 100 && declared.kind() == TypeKind::State => {
                    Form::Record(fields)
                }
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
                    quantity: declared.quantity().cloned(),
                },
            );
        }
        if types.values().any(|t| t.quantity.is_some()) {
            use zeno_fcis_codec::CommitmentHasher;
            for declared in types.values_mut() {
                if matches!(
                    declared.form,
                    Form::Leaf(Leaf::I128 { .. } | Leaf::U128 { .. })
                ) && declared.quantity.is_none()
                {
                    return Err(invalid(
                        "every integer type needs unit U scale eP in a quantity contract"
                            .to_owned(),
                    ));
                }
                if let Some(q) = &declared.quantity {
                    let mut bytes = if matches!(declared.form, Form::Leaf(Leaf::U128 { .. })) {
                        b"zeno-fcis/quantity/u128/1\0".to_vec()
                    } else {
                        b"zeno-fcis/quantity/i128/1\0".to_vec()
                    };
                    bytes.extend_from_slice(&(declared.name.len() as u32).to_be_bytes());
                    bytes.extend_from_slice(declared.name.as_bytes());
                    bytes.push(q.scale());
                    for (id, power) in q.factors() {
                        bytes.extend_from_slice(&(id.as_str().len() as u32).to_be_bytes());
                        bytes.extend_from_slice(id.as_str().as_bytes());
                        bytes.extend_from_slice(&power.to_be_bytes());
                    }
                    let hash = zeno_fcis_crypto::RustCryptoSha256::hash(&bytes);
                    let digest: String =
                        hash.as_bytes().iter().map(|b| format!("{b:02x}")).collect();
                    declared.name = format!("Quantity128_e{}_{}", q.scale(), digest);
                }
            }
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

    /// Full unsigned subjects require the typed core path.
    pub fn unsigned(&self) -> bool {
        self.types
            .values()
            .any(|t| matches!(t.form, Form::Leaf(Leaf::U128 { .. })))
            || self
                .laws
                .iter()
                .any(|law| super::expr::uses_unsigned(&law.formula))
            || self.notes.claims.iter().any(|claim| match claim.formula() {
                zeno_fcis_spec::ClaimFormula::Relational(v) => v.uses_u128(),
                zeno_fcis_spec::ClaimFormula::Temporal(v) => v.uses_u128(),
            })
    }
    /// Restricted consumer admission, before lowering or projection.
    pub fn refuse_unsigned(&self, consumer: &str) -> Result<(), ContractError> {
        if self.unsigned() {
            Err(ContractError::new(consumer, "U128SubjectUnsupported"))
        } else {
            Ok(())
        }
    }
    pub fn get(&self, id: u32) -> Result<&Type, ContractError> {
        self.types
            .get(&id)
            .ok_or_else(|| ContractError::new("project.zeno", format!("type {id} is not declared")))
    }

    /// The fields of a record type; an error for other forms.
    pub fn fields(&self, id: u32) -> Result<&[Field], ContractError> {
        match &self.get(id)?.form {
            Form::Record(fields) => Ok(fields),
            _ => Err(ContractError::new(
                "project.zeno",
                format!("type {id} must be a record"),
            )),
        }
    }

    /// The scalar kind of a type; records have none.
    pub fn kind(&self, id: u32) -> Result<Kind<'_>, ContractError> {
        Ok(match &self.get(id)?.form {
            Form::Leaf(Leaf::Bool) => Kind::Bool,
            Form::Leaf(Leaf::U128 { min, max }) => Kind::U128 {
                min: *min,
                max: *max,
            },
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
    pub fn inputs(&self) -> Result<Vec<Input>, ContractError> {
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
    pub fn state_fields(&self) -> Result<&[Field], ContractError> {
        Ok(match &self.get(ROOTS[0].1)?.form {
            Form::Record(fields) => fields,
            _ => &[],
        })
    }

    pub fn channel(&self, id: u32) -> Result<Channel, ContractError> {
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
