//! Private publication capabilities from actual bound evaluation and original wire encoding.
//! Encoding is outside the invocation logical meter. Shell CAS and delivery remain external.
use super::super::super::canonical_v2::output;
#[cfg(verus_keep_ghost)]
use super::super::decision;
use super::super::{
    composition::{FrameBinding, Kind},
    decision::{Class, Delivery, Field},
};
use super::{
    Evaluation, Refusal,
    canonical::{self, Part},
};
use alloc::vec::Vec;
#[cfg(verus_keep_ghost)]
use vstd::prelude::*;
#[cfg(verus_keep_ghost)]
pub(super) mod spec;
#[cfg(verus_keep_ghost)]
use super::{outcome_spec as audit, spec as encoding};

/// One ordered delivery with exact original atom/record bytes and checked type roots.
/// These are bare ZCVE values, not ZFCISV1 envelopes. No new schema32 is inferred.
#[cfg_attr(verus_keep_ghost, verus_verify)]
#[derive(Debug)]
pub struct WireDelivery {
    ordinal: u32,
    channel: u32,
    destination_root: u32,
    payload_root: u32,
    destination: Vec<u8>,
    payload: Vec<u8>,
    idempotency: Vec<u8>,
}
#[cfg_attr(verus_keep_ghost, verus_verify)]
#[derive(Debug)]
struct Artifacts {
    post: Vec<u8>,
    effects: Vec<WireDelivery>,
    outbox: Vec<WireDelivery>,
    subject: Vec<u8>,
}

/// Non-Clone committing capability. Only the actual bound authority constructs it.
/// Its borrowed identity and inputs cannot be mutated for the capability's lifetime.
#[must_use]
#[cfg_attr(verus_keep_ghost, verus_verify)]
#[derive(Debug)]
pub struct Publication<'a> {
    evaluation: Evaluation<'a>,
    identity: &'a [u8],
    artifacts: Artifacts,
}
/// Business rejection remains audited; every technical refusal discards wire scratch.
#[must_use]
#[cfg_attr(verus_keep_ghost, verus_verify)]
#[derive(Debug)]
#[non_exhaustive]
pub enum PublicationOutcome<'a> {
    /// Accept or CommittedFailure, with every original wire artifact.
    Commit(Publication<'a>),
    /// Successful business Reject, with no committing capability.
    Reject(Evaluation<'a>),
    /// Technical refusal and its actual retained evaluation report.
    Refused {
        /// Actual evaluation report, including completed work before refusal.
        evaluation: Evaluation<'a>,
        /// Exact technical refusal; no wire capability is authorized.
        error: Refusal,
    },
}

#[cfg(verus_keep_ghost)]
verus! {
impl WireDelivery{pub closed spec fn view(&self)->spec::WireView{(self.ordinal,self.channel,self.destination_root,self.payload_root,self.destination@,self.payload@,self.idempotency@)}}
impl Artifacts{pub closed spec fn view(&self)->spec::ArtifactsView{(self.post@,self.effects@.map(|_:int,d:WireDelivery|d.view()),self.outbox@.map(|_:int,d:WireDelivery|d.view()),self.subject@)}}
// The view tuples omit the invocation kind; kind_view carries the retained
// evaluation's tag through every variant.
impl<'a> Publication<'a>{pub closed spec fn view(&self)->spec::PublicationView<'a>{(self.evaluation.view(),self.identity@,self.artifacts.view())}
    pub closed spec fn kind_view(&self)->Kind{self.evaluation.kind_view()}}
impl<'a> PublicationOutcome<'a>{pub closed spec fn view(&self)->spec::OutcomeView<'a>{match self{
    Self::Commit(p)=>(p.view().0,Ok(Some((p.view().1,p.view().2)))),Self::Reject(e)=>(e.view(),Ok(None)),Self::Refused{evaluation,error}=>(evaluation.view(),Err(*error)),
}}
    pub closed spec fn kind_view(&self)->Kind{match self{
        Self::Commit(p)=>p.kind_view(),Self::Reject(e)=>e.kind_view(),Self::Refused{evaluation,..}=>evaluation.kind_view(),
    }}}
// Prove the observable enum projection inside its privacy boundary. Scoped
// wrappers use this fact without opening or reconstructing private capability
// fields and without changing any executable publication path.
pub(super) proof fn outcome_projection(p:&PublicationOutcome)
    ensures p.view()==match p {
        PublicationOutcome::Commit(q)=>(q.view().0,Ok(Some((q.view().1,q.view().2)))),
        PublicationOutcome::Reject(e)=>(e.view(),Ok(None)),
        PublicationOutcome::Refused{evaluation,error}=>(evaluation.view(),Err(*error)),
    },
    p.kind_view()==match p {
        PublicationOutcome::Commit(q)=>q.kind_view(),
        PublicationOutcome::Reject(e)=>e.kind_view(),
        PublicationOutcome::Refused{evaluation,..}=>evaluation.kind_view(),
    },
{reveal(PublicationOutcome::view);reveal(PublicationOutcome::kind_view);}
}

#[cfg_attr(verus_keep_ghost,verus_spec(result=>ensures result==spec::link(links@,id,links@.len()),))]
fn channel_link(links: &[(u32, u32, u32)], id: u32) -> Option<(u32, u32)> {
    let mut i = 0usize;
    #[cfg_attr(verus_keep_ghost,verus_spec(invariant i<=links.len(),spec::link(links@,id,i as nat).is_none(),decreases links.len()-i,))]
    while i < links.len() {
        if links[i].0 == id {
            #[cfg(verus_keep_ghost)]
            proof! {spec::link_found(links@,id,(i+1) as nat,links@.len());}
            return Some((links[i].1, links[i].2));
        }
        i += 1;
    }
    None
}
#[cfg_attr(verus_keep_ghost,verus_spec(result=>ensures audit::owned_bytes_result(result)==spec::state(fields@,*binding),))]
fn state_bytes(fields: &[Field<'_>], binding: &FrameBinding) -> Result<Vec<u8>, Refusal> {
    if binding.max_bytes as u128 > usize::MAX as u128 {
        return Err(Refusal::Encoding);
    }
    let cap = binding.max_bytes as usize;
    let payload = match output::encode_record(fields, cap) {
        Ok(bytes) => bytes,
        Err(e) => return Err(Refusal::Output(e)),
    };
    match output::encode_envelope(binding.root, &binding.schema, &payload, cap) {
        Ok(bytes) => Ok(bytes),
        Err(e) => Err(Refusal::Output(e)),
    }
}
#[cfg_attr(verus_keep_ghost,verus_spec(result=>ensures
    (match result{Ok(v)=>Ok(v.view()),Err(e)=>Err(e)})==spec::delivery(decision::spec::delivery_view(*d),links@),))]
fn delivery_bytes(d: &Delivery<'_>, links: &[(u32, u32, u32)]) -> Result<WireDelivery, Refusal> {
    let (destination_root, payload_root) = match channel_link(links, d.channel) {
        Some(pair) => pair,
        None => return Err(Refusal::Channel),
    };
    let destination = match output::encode_atom(d.destination, usize::MAX) {
        Ok(bytes) => bytes,
        Err(e) => return Err(Refusal::Output(e)),
    };
    let payload = match output::encode_record(&d.payload, usize::MAX) {
        Ok(bytes) => bytes,
        Err(e) => return Err(Refusal::Output(e)),
    };
    let idempotency = match output::encode_atom(d.idempotency, usize::MAX) {
        Ok(bytes) => bytes,
        Err(e) => return Err(Refusal::Output(e)),
    };
    #[cfg(verus_keep_ghost)]
    proof! {reveal(WireDelivery::view);}
    Ok(WireDelivery {
        ordinal: d.ordinal,
        channel: d.channel,
        destination_root,
        payload_root,
        destination,
        payload,
        idempotency,
    })
}
#[cfg_attr(verus_keep_ghost,verus_spec(result=>ensures
    (match result{Ok(v)=>Ok(v@.map(|_:int,d:WireDelivery|d.view())),Err(e)=>Err(e)})
    ==spec::deliveries(ds@.map(|_:int,d:Delivery|decision::spec::delivery_view(d)),links@,ds@.len()),))]
fn delivery_list(
    ds: &[Delivery<'_>],
    links: &[(u32, u32, u32)],
) -> Result<Vec<WireDelivery>, Refusal> {
    let mut result = Vec::new();
    let mut i = 0usize;
    #[cfg(verus_keep_ghost)]
    proof! {assert(result@.map(|_:int,d:WireDelivery|d.view()) =~= Seq::empty());}
    #[cfg_attr(verus_keep_ghost,verus_spec(invariant i<=ds.len(),
        spec::deliveries(ds@.map(|_:int,d:Delivery|decision::spec::delivery_view(d)),links@,i as nat)==Ok(result@.map(|_:int,d:WireDelivery|d.view())),decreases ds.len()-i,))]
    while i < ds.len() {
        let value = match delivery_bytes(&ds[i], links) {
            Ok(value) => value,
            Err(error) => {
                #[cfg(verus_keep_ghost)]
                proof! {spec::deliveries_failure(ds@.map(|_:int,d:Delivery|decision::spec::delivery_view(d)),links@,(i+1) as nat,ds@.len());}
                return Err(error);
            }
        };
        #[cfg(verus_keep_ghost)]
        proof_decl! {let ghost prior=result@;let ghost v=value.view();}
        result.push(value);
        #[cfg(verus_keep_ghost)]
        proof! {assert(result@.map(|_:int,d:WireDelivery|d.view()) =~= prior.map(|_:int,d:WireDelivery|d.view()).push(v));}
        i += 1;
    }
    Ok(result)
}
#[cfg_attr(verus_keep_ghost,verus_spec(ensures encoding::tokens(final(parts)@)==encoding::tokens(old(parts)@)+spec::lane(ds@.map(|_:int,d:WireDelivery|d.view())),))]
fn append_lane<'a>(parts: &mut Vec<Part<'a>>, ds: &'a [WireDelivery]) {
    #[cfg(verus_keep_ghost)]
    proof_decl! {let ghost initial=encoding::tokens(parts@);}
    parts.push(Part::Word(ds.len() as u128));
    let mut i = 0usize;
    #[cfg_attr(verus_keep_ghost,verus_spec(invariant i<=ds.len(),initial==encoding::tokens(old(parts)@),
        encoding::tokens(parts@)==initial+seq![encoding::Token::Word(ds.len() as u128)]+spec::lane_prefix(ds@.map(|_:int,d:WireDelivery|d.view()),i as nat),decreases ds.len()-i,))]
    while i < ds.len() {
        let d = &ds[i];
        #[cfg(verus_keep_ghost)]
        proof_decl! {let ghost before=parts@;}
        parts.push(Part::Word(d.ordinal as u128));
        parts.push(Part::Word(d.channel as u128));
        parts.push(Part::Word(d.destination_root as u128));
        parts.push(Part::Word(d.payload_root as u128));
        parts.push(Part::Bytes(&d.destination));
        parts.push(Part::Bytes(&d.payload));
        parts.push(Part::Bytes(&d.idempotency));
        #[cfg(verus_keep_ghost)]
        proof! {
            reveal(WireDelivery::view);
            let added=parts@.skip(before.len() as int);
            assert(parts@==before+added);encoding::tokens_append(before,added);
            assert(encoding::tokens(added) =~= spec::delivery_tokens(d.view()));
        }
        i += 1;
    }
}
#[cfg_attr(verus_keep_ghost,verus_spec(result=>ensures audit::owned_bytes_result(result)==
    spec::subject(kind,audited@,post@,effects@.map(|_:int,d:WireDelivery|d.view()),outbox@.map(|_:int,d:WireDelivery|d.view())),))]
fn subject_bytes(
    kind: Kind,
    audited: &[u8],
    post: &[u8],
    effects: &[WireDelivery],
    outbox: &[WireDelivery],
) -> Result<Vec<u8>, Refusal> {
    let mut parts = alloc::vec![
        Part::Word(0x5a505532),
        Part::Word(1),
        Part::Word(match kind {
            Kind::Genesis => 0,
            Kind::Transition => 1,
        }),
        Part::Bytes(audited),
        Part::Bytes(post)
    ];
    #[cfg(verus_keep_ghost)]
    proof! {
        assert(encoding::tokens(parts@) =~= seq![encoding::Token::Word(0x5a505532),encoding::Token::Word(1),encoding::Token::Word(match kind{Kind::Genesis=>0u128,Kind::Transition=>1u128}),encoding::Token::Bytes(audited@),encoding::Token::Bytes(post@)]);
    }
    append_lane(&mut parts, effects);
    append_lane(&mut parts, outbox);
    #[cfg(verus_keep_ghost)]
    proof! {encoding::token_correspondence(parts@);}
    match canonical::encode(&parts) {
        Some(bytes) => Ok(bytes),
        None => Err(Refusal::Encoding),
    }
}
#[cfg_attr(verus_keep_ghost,verus_spec(result=>ensures
    (match result{Ok(Some(a))=>Ok(Some(a.view())),Ok(None)=>Ok(None),Err(e)=>Err(e)})==spec::prepared(e.view(),e.kind_view(),*binding,links@),))]
fn prepare(
    e: &Evaluation<'_>,
    binding: &FrameBinding,
    links: &[(u32, u32, u32)],
) -> Result<Option<Artifacts>, Refusal> {
    let candidate = e.result()?;
    #[cfg(verus_keep_ghost)]
    proof! {
        let expected=audit::evaluation_result(e.view()).unwrap();
        assert(expected.5 =~= candidate.view().5.map(|_:int,d:Delivery|decision::spec::delivery_view(d)));
        assert(expected.6 =~= candidate.view().6.map(|_:int,d:Delivery|decision::spec::delivery_view(d)));
    }
    if matches!(candidate.class(), Class::Reject) {
        return Ok(None);
    }
    let post = match e.kind() {
        Kind::Genesis => {
            let mut initial = Vec::new();
            initial.extend_from_slice(e.outcome().raw().state);
            initial
        }
        Kind::Transition => state_bytes(candidate.post(), binding)?,
    };
    let effects = delivery_list(candidate.effects(), links)?;
    let outbox = delivery_list(candidate.outbox(), links)?;
    let audited = e.subject()?;
    #[cfg(verus_keep_ghost)]
    proof! {
        let c=audit::evaluation_result(e.view()).unwrap();
        assert(e.view().1 == Ok::<Seq<u8>,Refusal>(audited@));
        assert(post@ == (match e.kind_view(){Kind::Genesis=>e.view().0.1.0,Kind::Transition=>spec::state(c.3,*binding).unwrap()}));
        assert(spec::deliveries(c.5,links@,c.5.len()) == Ok::<Seq<spec::WireView>,Refusal>(effects@.map(|_:int,d:WireDelivery|d.view())));
        assert(spec::deliveries(c.6,links@,c.6.len()) == Ok::<Seq<spec::WireView>,Refusal>(outbox@.map(|_:int,d:WireDelivery|d.view())));
    }
    let subject = subject_bytes(e.kind(), audited, &post, &effects, &outbox)?;
    #[cfg(verus_keep_ghost)]
    proof! {reveal(Artifacts::view);}
    Ok(Some(Artifacts {
        post,
        effects,
        outbox,
        subject,
    }))
}
#[cfg_attr(verus_keep_ghost,verus_spec(result=>ensures result.view()==spec::finished(evaluation.view(),evaluation.kind_view(),identity@,*binding,links@,(match expected{Some(b)=>Some(b@),None=>None})),
    result.kind_view()==evaluation.kind_view(),))]
pub(super) fn finish<'a>(
    evaluation: Evaluation<'a>,
    identity: &'a [u8],
    binding: &FrameBinding,
    links: &[(u32, u32, u32)],
    expected: Option<&[u8]>,
) -> PublicationOutcome<'a> {
    #[cfg(verus_keep_ghost)]
    proof! {reveal(PublicationOutcome::view);reveal(PublicationOutcome::kind_view);reveal(Publication::view);reveal(Publication::kind_view);reveal(Artifacts::view);}
    match prepare(&evaluation, binding, links) {
        Err(error) => PublicationOutcome::Refused { evaluation, error },
        Ok(None) => {
            if let Some(wanted) = expected {
                let bytes = match evaluation.subject() {
                    Ok(bytes) => bytes,
                    Err(error) => return PublicationOutcome::Refused { evaluation, error },
                };
                if !canonical::exact(bytes, wanted) {
                    return PublicationOutcome::Refused {
                        evaluation,
                        error: Refusal::ReplayMismatch,
                    };
                }
            }
            PublicationOutcome::Reject(evaluation)
        }
        Ok(Some(artifacts)) => {
            match expected {
                Some(wanted) if !canonical::exact(&artifacts.subject, wanted) => {
                    return PublicationOutcome::Refused {
                        evaluation,
                        error: Refusal::ReplayMismatch,
                    };
                }
                _ => {}
            }
            PublicationOutcome::Commit(Publication {
                evaluation,
                identity,
                artifacts,
            })
        }
    }
}
impl WireDelivery {
    /// Exact checked ordinal.
    #[cfg_attr(verus_keep_ghost,verus_spec(result=>ensures result==self.view().0,))]
    pub fn ordinal(&self) -> u32 {
        #[cfg(verus_keep_ghost)]
        proof! {reveal(WireDelivery::view);}
        self.ordinal
    }
    /// Exact checked channel.
    #[cfg_attr(verus_keep_ghost,verus_spec(result=>ensures result==self.view().1,))]
    pub fn channel(&self) -> u32 {
        #[cfg(verus_keep_ghost)]
        proof! {reveal(WireDelivery::view);}
        self.channel
    }
    /// Exact checked destination root.
    #[cfg_attr(verus_keep_ghost,verus_spec(result=>ensures result==self.view().2,))]
    pub fn destination_root(&self) -> u32 {
        #[cfg(verus_keep_ghost)]
        proof! {reveal(WireDelivery::view);}
        self.destination_root
    }
    /// Exact checked payload root.
    #[cfg_attr(verus_keep_ghost,verus_spec(result=>ensures result==self.view().3,))]
    pub fn payload_root(&self) -> u32 {
        #[cfg(verus_keep_ghost)]
        proof! {reveal(WireDelivery::view);}
        self.payload_root
    }
    /// Exact original ZCVE destination bytes.
    #[cfg_attr(verus_keep_ghost,verus_spec(result=>ensures result@==self.view().4,))]
    pub fn destination(&self) -> &[u8] {
        #[cfg(verus_keep_ghost)]
        proof! {reveal(WireDelivery::view);}
        &self.destination
    }
    /// Exact original ZCVE payload bytes.
    #[cfg_attr(verus_keep_ghost,verus_spec(result=>ensures result@==self.view().5,))]
    pub fn payload(&self) -> &[u8] {
        #[cfg(verus_keep_ghost)]
        proof! {reveal(WireDelivery::view);}
        &self.payload
    }
    /// Exact original ZCVE idempotency bytes.
    #[cfg_attr(verus_keep_ghost,verus_spec(result=>ensures result@==self.view().6,))]
    pub fn idempotency(&self) -> &[u8] {
        #[cfg(verus_keep_ghost)]
        proof! {reveal(WireDelivery::view);}
        &self.idempotency
    }
}
impl<'a> Publication<'a> {
    /// Exact checked policy and compiled-source identity.
    #[cfg_attr(verus_keep_ghost,verus_spec(result=>ensures result@==self.view().1,))]
    pub fn identity(&self) -> &'a [u8] {
        #[cfg(verus_keep_ghost)]
        proof! {reveal(Publication::view);reveal(Artifacts::view);}
        self.identity
    }
    /// Actual audited evaluation with original input, class, reason, patch and all reports.
    #[cfg_attr(verus_keep_ghost,verus_spec(result=>ensures result.view()==self.view().0,result.kind_view()==self.kind_view(),))]
    pub fn evaluation(&self) -> &Evaluation<'a> {
        #[cfg(verus_keep_ghost)]
        proof! {reveal(Publication::view);reveal(Publication::kind_view);reveal(Artifacts::view);}
        &self.evaluation
    }
    /// Complete original ZFCISV1 next-state envelope.
    #[cfg_attr(verus_keep_ghost,verus_spec(result=>ensures result@==self.view().2.0,))]
    pub fn poststate(&self) -> &[u8] {
        #[cfg(verus_keep_ghost)]
        proof! {reveal(Publication::view);reveal(Artifacts::view);}
        &self.artifacts.post
    }
    /// Every exact original-wire effect in declared order.
    #[cfg_attr(verus_keep_ghost,verus_spec(result=>ensures result@.map(|_:int,d:WireDelivery|d.view())==self.view().2.1,))]
    pub fn effects(&self) -> &[WireDelivery] {
        #[cfg(verus_keep_ghost)]
        proof! {reveal(Publication::view);reveal(Artifacts::view);}
        &self.artifacts.effects
    }
    /// Every exact original-wire outbox entry in declared order.
    #[cfg_attr(verus_keep_ghost,verus_spec(result=>ensures result@.map(|_:int,d:WireDelivery|d.view())==self.view().2.2,))]
    pub fn outbox(&self) -> &[WireDelivery] {
        #[cfg(verus_keep_ghost)]
        proof! {reveal(Publication::view);reveal(Artifacts::view);}
        &self.artifacts.outbox
    }
    /// Complete persisted publication subject, including audited input and every output byte.
    #[cfg_attr(verus_keep_ghost,verus_spec(result=>ensures result@==self.view().2.3,))]
    pub fn subject(&self) -> &[u8] {
        #[cfg(verus_keep_ghost)]
        proof! {reveal(Publication::view);reveal(Artifacts::view);}
        &self.artifacts.subject
    }
}
impl<'a> PublicationOutcome<'a> {
    /// Retained actual evaluation, including on output or replay refusal.
    #[cfg_attr(verus_keep_ghost,verus_spec(result=>ensures result.view()==self.view().0,result.kind_view()==self.kind_view(),))]
    pub fn evaluation(&self) -> &Evaluation<'a> {
        #[cfg(verus_keep_ghost)]
        proof! {reveal(PublicationOutcome::view);reveal(PublicationOutcome::kind_view);reveal(Publication::view);reveal(Publication::kind_view);}
        match self {
            Self::Commit(p) => &p.evaluation,
            Self::Reject(e) => e,
            Self::Refused { evaluation, .. } => evaluation,
        }
    }
}
