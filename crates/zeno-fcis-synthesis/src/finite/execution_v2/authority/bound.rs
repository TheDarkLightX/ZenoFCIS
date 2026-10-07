//! Immutable bound invocation and recomputation before whole-subject replay admission.
use super::super::{
    catalog::BoundCatalog,
    composition::{self, BoundCore, Descriptor, Framing, Kind, Raw},
};
use super::{Evaluation, Refusal, canonical, evaluator, framing, outcome};
#[cfg(verus_keep_ghost)]
use super::{bound_spec as model, outcome_spec};
use alloc::vec::Vec;
#[cfg(verus_keep_ghost)]
use vstd::prelude::*;

/// Construction accepts only the library's checked catalog/source-binding path.
/// Neither an invocation nor a persisted artifact can replace these bound fields.
#[cfg_attr(verus_keep_ghost, verus_verify)]
#[derive(Debug)]
pub struct Authority<'p> {
    core: BoundCore<'p>,
    framing: Framing,
    identity: Vec<u8>,
    channel_roots: &'p [(u32, u32, u32)],
}
#[cfg(verus_keep_ghost)]
verus! {
impl<'p> Authority<'p>{pub closed spec fn view(&self)->model::BoundView<'p>{(self.core.descriptor_view(),self.framing,self.identity@,self.channel_roots@)}}
}

/// Exact expected frame roots, complete schema identifiers and size bounds.
#[cfg_attr(verus_keep_ghost,verus_spec(result=>ensures (match result{Some(v)=>Some(v@),None=>None::<Seq<u8>>})==Some(model::framing_bytes(config)),))]
pub(super) fn framing_bytes(config: &Framing) -> Option<Vec<u8>> {
    use canonical::Part;
    let parts = alloc::vec![
        Part::Word(0x5a465232),
        Part::Word(1),
        Part::Word(config.state.root as u128),
        Part::Bytes(config.state.schema.as_slice()),
        Part::Word(config.state.max_bytes as u128),
        Part::Word(config.command.root as u128),
        Part::Bytes(config.command.schema.as_slice()),
        Part::Word(config.command.max_bytes as u128),
        Part::Word(config.context.root as u128),
        Part::Bytes(config.context.schema.as_slice()),
        Part::Word(config.context.max_bytes as u128)
    ];
    let out = canonical::encode(&parts);
    #[cfg(verus_keep_ghost)]
    proof! {
        assert((match out{Some(v)=>Some(v@),None=>None::<Seq<u8>>})==Some(model::framing_bytes(config))) by {
            reveal_with_fuel(super::spec::prefix,12);reveal_with_fuel(super::spec::length,12);
        }
    }
    out
}

#[cfg_attr(verus_keep_ghost,verus_spec(result=>ensures outcome_spec::owned_bytes_result(result)==outcome_spec::replay(outcome_spec::owned_bytes_result(actual),expected@),))]
pub(super) fn compare(
    actual: Result<Vec<u8>, Refusal>,
    expected: &[u8],
) -> Result<Vec<u8>, Refusal> {
    match actual {
        Err(e) => Err(e),
        Ok(bytes) => {
            if canonical::exact(&bytes, expected) {
                Ok(bytes)
            } else {
                Err(Refusal::ReplayMismatch)
            }
        }
    }
}

/// Bind the checked complete policy and the library's checked evaluator digest.
/// No caller identity, framing, source list, meter or candidate is accepted.
/// Construction work is outside the invocation's logical meter.
#[cfg_attr(verus_keep_ghost,verus_spec(result=>ensures
    (match result{Ok(a)=>Ok(a.view()),Err(e)=>Err(e)}) ==
        super::bind_value(catalog),
))]
pub fn bind<'p>(catalog: &BoundCatalog<'p>) -> Result<Authority<'p>, Refusal> {
    #[cfg(verus_keep_ghost)]
    proof! {reveal(super::bind_value);}
    let core = match composition::bind(catalog.descriptor()) {
        Ok(core) => core,
        Err(error) => return Err(Refusal::Core(error)),
    };
    let identity = match framing::identity_bytes(catalog.original_contract(), &evaluator::EVALUATOR)
    {
        Some(bytes) => bytes,
        None => return Err(Refusal::Encoding),
    };
    let framing = *catalog.framing();
    let channel_roots = catalog.channel_roots();
    #[cfg(verus_keep_ghost)]
    proof! {reveal(Authority::view);}
    Ok(Authority {
        core,
        framing,
        identity,
        channel_roots,
    })
}

impl<'p> Authority<'p> {
    /// The exact checked policy and complete compiled source/build identity.
    #[cfg_attr(verus_keep_ghost,verus_spec(result=>ensures result@==self.view().2,))]
    pub fn identity(&self) -> &[u8] {
        #[cfg(verus_keep_ghost)]
        proof! {reveal(Authority::view);}
        &self.identity
    }
    /// The immutable producer descriptor admitted by the checked catalog.
    #[cfg_attr(verus_keep_ghost,verus_spec(result=>ensures result==self.view().0,))]
    pub fn descriptor(&self) -> &'p Descriptor<'p> {
        #[cfg(verus_keep_ghost)]
        proof! {reveal(Authority::view);}
        self.core.descriptor()
    }

    /// Every original envelope passes the bound header configuration and the same private core meter.
    #[cfg_attr(verus_keep_ghost,verus_spec(result=>ensures model::transition(self.view(),original,result.view(),None),result.kind_view()==Kind::Transition,))]
    pub fn evaluate<'a>(&'a self, original: Raw<'a>) -> Evaluation<'a> {
        #[cfg(verus_keep_ghost)]
        proof! {reveal(Authority::view);reveal(Evaluation::view);reveal(Evaluation::kind_view);}
        let completed = self.core.frame(Kind::Transition, original, &self.framing);
        let subject = outcome::seal(&completed, &self.identity);
        Evaluation {
            outcome: completed,
            subject,
        }
    }
    /// Replay first recomputes; exact comparison then admits only the complete recomputed subject.
    #[cfg_attr(verus_keep_ghost,verus_spec(result=>ensures model::transition(self.view(),original,result.view(),Some(expected@)),
        result.view().1.is_ok() ==> result.view().1.unwrap()==expected@,
        result.kind_view()==Kind::Transition,))]
    pub fn replay<'a>(&'a self, original: Raw<'a>, expected: &[u8]) -> Evaluation<'a> {
        #[cfg(verus_keep_ghost)]
        proof! {reveal(Authority::view);reveal(Evaluation::view);reveal(Evaluation::kind_view);}
        let evaluated = self.evaluate(original);
        let subject = compare(evaluated.subject, expected);
        Evaluation {
            outcome: evaluated.outcome,
            subject,
        }
    }
    /// Genuine genesis admits only the initial state and evaluates its actual genesis laws.
    #[cfg_attr(verus_keep_ghost,verus_spec(result=>ensures model::genesis(self.view(),original@,result.view(),None),result.kind_view()==Kind::Genesis,))]
    pub fn genesis<'a>(&'a self, original: &'a [u8]) -> Evaluation<'a> {
        #[cfg(verus_keep_ghost)]
        proof! {reveal(Authority::view);reveal(Evaluation::view);reveal(Evaluation::kind_view);}
        let completed = self.core.frame(
            Kind::Genesis,
            Raw {
                state: original,
                command: &[],
                context: &[],
            },
            &self.framing,
        );
        let subject = outcome::seal(&completed, &self.identity);
        Evaluation {
            outcome: completed,
            subject,
        }
    }
    /// Recompute genuine genesis before comparing every persisted subject byte.
    #[cfg_attr(verus_keep_ghost,verus_spec(result=>ensures model::genesis(self.view(),original@,result.view(),Some(expected@)),
        result.view().1.is_ok() ==> result.view().1.unwrap()==expected@,
        result.kind_view()==Kind::Genesis,))]
    pub fn replay_genesis<'a>(&'a self, original: &'a [u8], expected: &[u8]) -> Evaluation<'a> {
        #[cfg(verus_keep_ghost)]
        proof! {reveal(Authority::view);reveal(Evaluation::view);reveal(Evaluation::kind_view);}
        let evaluated = self.genesis(original);
        let subject = compare(evaluated.subject, expected);
        Evaluation {
            outcome: evaluated.outcome,
            subject,
        }
    }
}

impl<'p> Authority<'p> {
    /// Recompute the complete bound decision, encode original wires.
    /// Output construction is outside the invocation logical meter.
    #[cfg_attr(verus_keep_ghost,verus_spec(result=>ensures super::publication::spec::transition(self.view(),original,result.view(),None),
        result.kind_view()==Kind::Transition,))]
    pub fn publish<'a>(&'a self, original: Raw<'a>) -> super::PublicationOutcome<'a> {
        #[cfg(verus_keep_ghost)]
        proof! {reveal(Authority::view);}
        let evaluated = self.evaluate(original);
        super::publication::finish(
            evaluated,
            &self.identity,
            &self.framing.state,
            self.channel_roots,
            None,
        )
    }
    /// Recompute the complete bound decision, encode original wires, then compare every persisted publication byte.
    /// Output construction is outside the invocation logical meter.
    #[cfg_attr(verus_keep_ghost,verus_spec(result=>ensures super::publication::spec::transition(self.view(),original,result.view(),Some(expected@)),
        result.kind_view()==Kind::Transition,))]
    pub fn replay_publication<'a>(
        &'a self,
        original: Raw<'a>,
        expected: &[u8],
    ) -> super::PublicationOutcome<'a> {
        #[cfg(verus_keep_ghost)]
        proof! {reveal(Authority::view);}
        let evaluated = self.evaluate(original);
        super::publication::finish(
            evaluated,
            &self.identity,
            &self.framing.state,
            self.channel_roots,
            Some(expected),
        )
    }
    /// Recompute genuine Genesis, encode original wires.
    /// Output construction is outside the invocation logical meter.
    #[cfg_attr(verus_keep_ghost,verus_spec(result=>ensures super::publication::spec::genesis(self.view(),original@,result.view(),None),
        result.kind_view()==Kind::Genesis,))]
    pub fn publish_genesis<'a>(&'a self, original: &'a [u8]) -> super::PublicationOutcome<'a> {
        #[cfg(verus_keep_ghost)]
        proof! {reveal(Authority::view);}
        let evaluated = self.genesis(original);
        super::publication::finish(
            evaluated,
            &self.identity,
            &self.framing.state,
            self.channel_roots,
            None,
        )
    }
    /// Recompute genuine Genesis, encode original wires, then compare every persisted publication byte.
    /// Output construction is outside the invocation logical meter.
    #[cfg_attr(verus_keep_ghost,verus_spec(result=>ensures super::publication::spec::genesis(self.view(),original@,result.view(),Some(expected@)),
        result.kind_view()==Kind::Genesis,))]
    pub fn replay_genesis_publication<'a>(
        &'a self,
        original: &'a [u8],
        expected: &[u8],
    ) -> super::PublicationOutcome<'a> {
        #[cfg(verus_keep_ghost)]
        proof! {reveal(Authority::view);}
        let evaluated = self.genesis(original);
        super::publication::finish(
            evaluated,
            &self.identity,
            &self.framing.state,
            self.channel_roots,
            Some(expected),
        )
    }
}

#[cfg(test)]
#[path = "invocation_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "publication_tests.rs"]
mod publication_tests;
