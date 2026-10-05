//! Complete original-schema and reviewed-policy correspondence at construction.
use super::super::canonical_v2::schema;
use super::{authority, composition};
use composition::{Descriptor, Framing};
mod matching;
#[cfg(verus_keep_ghost)]
use vstd::prelude::*;
#[cfg(verus_keep_ghost)]
mod spec;

/// Construction limits, separate from the descriptor's invocation meter.
#[cfg_attr(verus_keep_ghost, verus_verify)]
#[derive(Debug)]
pub struct Limits {
    /// Complete original schema admission limits.
    pub schema: schema::Limits,
    /// Maximum complete reviewed policy bytes.
    pub contract_bytes: u64,
}

/// Ordered construction refusals. None carries a partially bound catalog.
#[cfg_attr(verus_keep_ghost, verus_verify)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum Failure {
    /// The supplied complete reviewed policy exceeds its construction limit.
    Size,
    /// Original schema admission refused before descriptor correspondence.
    Schema(schema::Failure),
    /// The complete executable descriptor is not admitted by composition.
    Descriptor,
    /// Original state, command or context root correspondence failed.
    Roots,
    /// Complete channel links or declared domains differ from the schema.
    Channels,
    /// A delivery in some branch violates original payload or length bounds.
    Deliveries,
    /// Complete policy encoding overflowed.
    Encoding,
    /// The complete supplied policy differs from the library serialization.
    Policy,
}

/// Immutable custody established only by the complete checked bind path.
/// This value does not itself authorize invocation, commit or delivery.
#[cfg_attr(verus_keep_ghost, verus_verify)]
#[derive(Debug)]
pub struct BoundCatalog<'a> {
    checked: schema::Checked<'a>,
    contract: &'a [u8],
    descriptor: &'a Descriptor<'a>,
    framing: &'a Framing,
    channels: &'a [(u32, u32, u32)],
}

#[cfg(verus_keep_ghost)]
verus! {
impl<'a> BoundCatalog<'a> {
    pub closed spec fn view(&self) -> spec::CatalogView<'a> {
        (self.checked.view().0, self.contract@, self.descriptor, self.framing, self.channels@)
    }
}
}

impl<'a> BoundCatalog<'a> {
    /// Exact original schema, including unused definitions and names.
    #[cfg_attr(verus_keep_ghost, verus_spec(result => ensures result@ == self.view().0,))]
    pub fn original_schema(&self) -> &'a [u8] {
        #[cfg(verus_keep_ghost)]
        proof! { reveal(BoundCatalog::view); }
        self.checked.original()
    }
    /// Exact complete reviewed V2 policy bytes.
    #[cfg_attr(verus_keep_ghost, verus_spec(result => ensures result@ == self.view().1,))]
    pub fn original_contract(&self) -> &'a [u8] {
        #[cfg(verus_keep_ghost)]
        proof! { reveal(BoundCatalog::view); }
        self.contract
    }
    /// The same immutable executable descriptor that passed admission.
    #[cfg_attr(verus_keep_ghost, verus_spec(result => ensures result == self.view().2,))]
    pub fn descriptor(&self) -> &'a Descriptor<'a> {
        #[cfg(verus_keep_ghost)]
        proof! { reveal(BoundCatalog::view); }
        self.descriptor
    }
    /// Exact reviewed framing. Its schema bytes are a declared policy field;
    /// this stage does not assert that they are a recomputed SHA commitment.
    #[cfg_attr(verus_keep_ghost, verus_spec(result => ensures result == self.view().3,))]
    pub fn framing(&self) -> &'a Framing {
        #[cfg(verus_keep_ghost)]
        proof! { reveal(BoundCatalog::view); }
        self.framing
    }
    /// Independently bound original state, command and context root IDs.
    #[cfg_attr(verus_keep_ghost, verus_spec(result => ensures result ==
        (self.view().3.state.root, self.view().3.command.root, self.view().3.context.root),))]
    pub fn roots(&self) -> (u32, u32, u32) {
        #[cfg(verus_keep_ghost)]
        proof! { reveal(BoundCatalog::view); }
        (
            self.framing.state.root,
            self.framing.command.root,
            self.framing.context.root,
        )
    }
    /// Every channel ID, original destination root and original payload root.
    #[cfg_attr(verus_keep_ghost, verus_spec(result => ensures result@ == self.view().4,))]
    pub fn channel_roots(&self) -> &'a [(u32, u32, u32)] {
        #[cfg(verus_keep_ghost)]
        proof! { reveal(BoundCatalog::view); }
        self.channels
    }
}

/// Binds a checked original schema to every executable policy field.
/// Construction work is outside the invocation's logical meter. Byte equality
/// binds the reviewed V2 policy; correspondence with frontend intent is separate.
#[cfg_attr(verus_keep_ghost, verus_spec(result => ensures
    (match result { Ok(bound) => Ok(bound.view()), Err(e) => Err(e) }) ==
        spec::bind(checked.view(), original_contract@, descriptor, framing,
            channel_roots@, max_contract_bytes),
))]
fn bind_checked<'a>(
    checked: schema::Checked<'a>,
    original_contract: &'a [u8],
    descriptor: &'a Descriptor<'a>,
    framing: &'a Framing,
    channel_roots: &'a [(u32, u32, u32)],
    max_contract_bytes: u64,
) -> Result<BoundCatalog<'a>, Failure> {
    if original_contract.len() as u64 > max_contract_bytes {
        return Err(Failure::Size);
    }
    if composition::bind(descriptor).is_err() {
        return Err(Failure::Descriptor);
    }
    let description = checked.description();
    if !matching::roots(description, descriptor, framing) {
        return Err(Failure::Roots);
    }
    if !matching::channels(description.definitions, descriptor.channels, channel_roots) {
        return Err(Failure::Channels);
    }
    if !matching::branches(description.definitions, descriptor.branches, channel_roots) {
        return Err(Failure::Deliveries);
    }
    let actual =
        match authority::policy_bytes(descriptor, checked.original(), framing, channel_roots) {
            Some(bytes) => bytes,
            None => return Err(Failure::Encoding),
        };
    if !super::util::bytes_equal(&actual, original_contract) {
        return Err(Failure::Policy);
    }
    #[cfg(verus_keep_ghost)]
    proof! { reveal(BoundCatalog::view); }
    Ok(BoundCatalog {
        checked,
        contract: original_contract,
        descriptor,
        framing,
        channels: channel_roots,
    })
}

#[cfg(test)]
mod tests;

/// Admits the complete original schema once, then binds the reviewed policy.
/// This is the sole public constructor. Successful construction establishes
/// original canonical well-formedness as well as all runtime correspondence.
#[cfg_attr(verus_keep_ghost, verus_spec(result => ensures
    (match result {Ok(bound)=>Ok(bound.view()),Err(e)=>Err(e)}) ==
        spec::bind_original(original_schema@,*description,limits.schema,
            original_contract@,descriptor,framing,channel_roots@,limits.contract_bytes),
))]
pub fn bind_original<'a>(
    original_schema: &'a [u8],
    description: &'a schema::Description<'a>,
    limits: Limits,
    original_contract: &'a [u8],
    descriptor: &'a Descriptor<'a>,
    framing: &'a Framing,
    channel_roots: &'a [(u32, u32, u32)],
) -> Result<BoundCatalog<'a>, Failure> {
    if original_contract.len() as u64 > limits.contract_bytes {
        return Err(Failure::Size);
    }
    let checked = match schema::admit(original_schema, description, limits.schema) {
        Ok(checked) => checked,
        Err(error) => return Err(Failure::Schema(error)),
    };
    bind_checked(
        checked,
        original_contract,
        descriptor,
        framing,
        channel_roots,
        limits.contract_bytes,
    )
}
