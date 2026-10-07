//! Exact canonical artifacts and replay comparison. Pure helpers do not authorize.
pub mod candidate;
pub mod canonical;
mod evaluator;
pub mod framing;
pub mod observations;
#[cfg(verus_keep_ghost)]
pub(super) mod spec;
#[cfg(test)]
mod tests;
pub use evaluator::EVALUATOR;
pub(super) mod metadata;
#[cfg(verus_keep_ghost)]
pub(super) mod metadata_spec;
mod refusal;
pub use refusal::Refusal;
mod outcome;
pub use outcome::Evaluation;
mod bound;
pub(super) mod descriptor;
#[cfg(verus_keep_ghost)]
pub(super) mod descriptor_spec;
#[cfg(verus_keep_ghost)]
pub(super) mod outcome_spec;
pub use bound::{Authority, bind};
#[cfg(verus_keep_ghost)]
pub(super) mod bound_spec;
mod policy;
mod publication;
pub use policy::policy_bytes;
pub use publication::{Publication, PublicationOutcome, WireDelivery};
#[cfg(verus_keep_ghost)]
pub(super) mod policy_spec;

#[cfg(verus_keep_ghost)]
vstd::prelude::verus! {
pub closed spec fn bind_value<'a>(catalog:&super::catalog::BoundCatalog<'a>)->Result<bound_spec::BoundView<'a>,Refusal>{bound_spec::construction(catalog.view().2,*catalog.view().3,catalog.view().1,catalog.view().4)}
pub closed spec fn policy_value(d:&super::composition::Descriptor,schema:vstd::seq::Seq<u8>,framing:&super::composition::Framing,channels:vstd::seq::Seq<(u32,u32,u32)>)->Option<vstd::seq::Seq<u8>>{policy_spec::policy(d,schema,framing,channels)}
impl<'a> Evaluation<'a>{pub closed spec fn view(&self)->outcome_spec::EvaluationView<'a>{(self.outcome.view(),outcome_spec::owned_bytes_result(self.subject))}
    pub closed spec fn kind_view(&self)->super::composition::Kind{self.outcome.kind_view()}}
}
