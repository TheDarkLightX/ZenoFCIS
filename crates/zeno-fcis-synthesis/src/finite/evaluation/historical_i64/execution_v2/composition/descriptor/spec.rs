//! Exact closed data projections. No evaluation takes place in a projection.
use super::*;
use vstd::prelude::*;
verus! {
pub(crate) open spec fn state<'p>(p:&DescriptorProjection<'p>)->SchemaView<'p>{match p.view{DescriptorView::Legacy(d)=>SchemaView::Legacy(d.state),DescriptorView::Numeric(d)=>SchemaView::Numeric(d.state)}}
pub(crate) open spec fn command<'p>(p:&DescriptorProjection<'p>)->SchemaView<'p>{match p.view{DescriptorView::Legacy(d)=>SchemaView::Legacy(d.command),DescriptorView::Numeric(d)=>SchemaView::Numeric(d.command)}}
pub(crate) open spec fn context<'p>(p:&DescriptorProjection<'p>)->SchemaView<'p>{match p.view{DescriptorView::Legacy(d)=>SchemaView::Legacy(d.context),DescriptorView::Numeric(d)=>SchemaView::Numeric(d.context)}}
pub(crate) open spec fn program<'p>(p:&DescriptorProjection<'p>)->ProgramView<'p>{match p.view{
    DescriptorView::Legacy(d)=>ProgramView::Legacy{inputs:d.program.inputs,outputs:d.program.outputs,nodes:d.program.nodes,roots:d.program.roots},
    DescriptorView::Numeric(d)=>ProgramView::Typed(d.program),
}}
pub(crate) open spec fn outputs<'p>(p:&DescriptorProjection<'p>)->OutputTypes<'p>{match p.view{DescriptorView::Legacy(d)=>OutputTypes::Legacy(d.output_types),DescriptorView::Numeric(d)=>OutputTypes::Numeric(d.output_types)}}
pub(crate) open spec fn bindings<'p>(p:&DescriptorProjection<'p>)->&'p[super::super::Binding]{match p.view{DescriptorView::Legacy(d)=>d.bindings,DescriptorView::Numeric(d)=>d.bindings}}
pub(crate) open spec fn decision_output<'p>(p:&DescriptorProjection<'p>)->usize{match p.view{DescriptorView::Legacy(d)=>d.decision_output,DescriptorView::Numeric(d)=>d.decision_output}}
pub(crate) open spec fn branches<'p>(p:&DescriptorProjection<'p>)->&'p[super::super::Branch<'p>]{match p.view{DescriptorView::Legacy(d)=>d.branches,DescriptorView::Numeric(d)=>d.branches}}
pub(crate) open spec fn reasons<'p>(p:&DescriptorProjection<'p>)->&'p[super::super::Reason]{match p.view{DescriptorView::Legacy(d)=>d.reasons,DescriptorView::Numeric(d)=>d.reasons}}
pub(crate) open spec fn channels<'p>(p:&DescriptorProjection<'p>)->&'p[super::super::Channel<'p>]{match p.view{DescriptorView::Legacy(d)=>d.channels,DescriptorView::Numeric(d)=>d.channels}}
pub(crate) open spec fn laws<'p>(p:&DescriptorProjection<'p>)->&'p[super::super::laws::Law<'p>]{match p.view{DescriptorView::Legacy(d)=>d.laws,DescriptorView::Numeric(d)=>d.laws}}
pub(crate) open spec fn required<'p>(p:&DescriptorProjection<'p>)->&'p[u32]{match p.view{DescriptorView::Legacy(d)=>d.required,DescriptorView::Numeric(d)=>d.required}}
pub(crate) open spec fn limits<'p>(p:&DescriptorProjection<'p>)->super::super::Limits{match p.view{DescriptorView::Legacy(d)=>d.limits,DescriptorView::Numeric(d)=>d.limits}}
}
