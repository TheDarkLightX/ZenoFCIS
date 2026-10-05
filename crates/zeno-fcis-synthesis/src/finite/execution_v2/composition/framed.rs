//! Original frame headers and payload execution on the same private meter.
use super::super::super::canonical_v2::envelope;
use super::*;
#[cfg(verus_keep_ghost)]
use vstd::prelude::*;
#[cfg(verus_keep_ghost)]
pub(super) mod spec;

/// Immutable expected original envelope identity, derived by bound catalog admission.
#[cfg_attr(verus_keep_ghost, verus_verify)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FrameBinding {
    /// Original declared root identifier.
    pub root: u32,
    /// Actual canonical schema commitment.
    pub schema: [u8; 32],
    /// Complete per-envelope size limit, including its header.
    pub max_bytes: u64,
}
/// State, command and context header identities in fixed interpretation order.
#[cfg_attr(verus_keep_ghost, verus_verify)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Framing {
    /// Expected original state envelope.
    pub state: FrameBinding,
    /// Expected original command envelope.
    pub command: FrameBinding,
    /// Expected original context envelope.
    pub context: FrameBinding,
}
/// A denied header charge or the actual original frame check refusal.
#[cfg_attr(verus_keep_ghost, verus_verify)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Failure {
    /// Header work was denied before actual frame interpretation.
    Budget(super::super::MeterFailure),
    /// The actual complete original frame checker refused.
    Envelope(envelope::Failure),
}

#[cfg_attr(verus_keep_ghost,verus_spec(result=>ensures final(meter).limits==old(meter).limits,
    ((match result{Ok(p)=>Ok(p.view()),Err(e)=>Err(e)}),final(meter).used.counters@)==spec::project(bytes@,binding,old(meter).limits.counters@,old(meter).used.counters@),))]
fn project<'a>(
    bytes: &'a [u8],
    binding: &FrameBinding,
    meter: &mut Meter,
) -> Result<envelope::Payload<'a>, Failure> {
    let header = if bytes.len() < 48 {
        bytes.len() as u64
    } else {
        48
    };
    if let Err(e) = meter.charge(Resource::Byte, header) {
        return Err(Failure::Budget(e));
    }
    match envelope::frame(bytes, binding.root, &binding.schema, binding.max_bytes) {
        Ok(p) => Ok(p),
        Err(e) => Err(Failure::Envelope(e)),
    }
}
#[cfg_attr(verus_keep_ghost,verus_spec(result=>ensures final(meter).limits==old(meter).limits,
    ((match result{Ok(p)=>Ok(raw_view(p)),Err(e)=>Err(e)}),final(meter).used.counters@)==spec::payloads(raw_view(original),framing,old(meter).limits.counters@,old(meter).used.counters@),))]
fn payloads<'a>(
    original: Raw<'a>,
    framing: &Framing,
    meter: &mut Meter,
) -> Result<Raw<'a>, super::Failure> {
    let state = match project(original.state, &framing.state, meter) {
        Ok(p) => p,
        Err(e) => return Err(super::Failure::Frame(0, e)),
    };
    let command = match project(original.command, &framing.command, meter) {
        Ok(p) => p,
        Err(e) => return Err(super::Failure::Frame(1, e)),
    };
    let context = match project(original.context, &framing.context, meter) {
        Ok(p) => p,
        Err(e) => return Err(super::Failure::Frame(2, e)),
    };
    Ok(Raw {
        state: state.bytes(),
        command: command.bytes(),
        context: context.bytes(),
    })
}
impl<'p> BoundCore<'p> {
    /// Check the original headers this kind declares, then protect the actual
    /// payloads and run with one meter. Genesis projects the state envelope only
    /// and ignores the absent command/context lanes.
    /// Header configuration is trusted only when privately bound by the checked catalog/authority route.
    #[cfg_attr(verus_keep_ghost,verus_spec(result=>ensures result.kind_view()==kind,
        result.view().1==match kind{Kind::Genesis=>(original.state@,Seq::<u8>::empty(),Seq::<u8>::empty()),Kind::Transition=>raw_view(original)},
        match kind{
            Kind::Genesis=>super::framed_genesis(self.descriptor_view(),original.state@,&framing.state,self.descriptor_view().limits.view(),Seq::new(8,|_:int|0u64),(Seq::empty(),Seq::empty(),None,None),Seq::empty(),Seq::empty(),
                (result.view().0,result.view().3,result.view().2,result.view().4,result.view().5)),
            Kind::Transition=>super::framed_execution(self.descriptor_view(),original,framing,self.descriptor_view().limits.view(),Seq::new(8,|_:int|0u64),(Seq::empty(),Seq::empty(),None,None),Seq::empty(),Seq::empty(),
                (result.view().0,result.view().3,result.view().2,result.view().4,result.view().5)),
        },))]
    pub fn frame<'a>(&'a self, kind: Kind, original: Raw<'a>, framing: &Framing) -> Outcome<'a> {
        #[cfg(verus_keep_ghost)]
        proof! {use_type_invariant(self);reveal(BoundCore::descriptor_view);reveal(Outcome::view);reveal(Outcome::kind_view);reveal(Limits::view);reveal(Usage::view);}
        let mut meter = super::super::meter::new(self.descriptor.limits);
        let mut trace = producer::Trace {
            reads: Vec::new(),
            attempts: Vec::new(),
            ingress: None,
            decision: None,
        };
        let mut diagnostics = Vec::new();
        let mut law_reads = Vec::new();
        let result = match kind {
            Kind::Genesis => match project(original.state, &framing.state, &mut meter) {
                Err(e) => Err(super::Failure::Frame(0, e)),
                Ok(p) => outcome::run(
                    self,
                    Kind::Genesis,
                    Raw {
                        state: p.bytes(),
                        command: &[],
                        context: &[],
                    },
                    &mut meter,
                    &mut trace,
                    &mut diagnostics,
                    &mut law_reads,
                ),
            },
            Kind::Transition => match payloads(original, framing, &mut meter) {
                Err(e) => Err(e),
                Ok(raw) => outcome::run(
                    self,
                    Kind::Transition,
                    raw,
                    &mut meter,
                    &mut trace,
                    &mut diagnostics,
                    &mut law_reads,
                ),
            },
        };
        Outcome {
            kind,
            result,
            raw: match kind {
                Kind::Genesis => Raw {
                    state: original.state,
                    command: &[],
                    context: &[],
                },
                Kind::Transition => original,
            },
            trace,
            usage: meter.used,
            diagnostics,
            law_reads,
        }
    }
}
