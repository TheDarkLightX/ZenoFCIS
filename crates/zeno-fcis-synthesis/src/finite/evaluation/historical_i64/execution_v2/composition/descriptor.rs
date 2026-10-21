//! Closed library-owned descriptor projections; no application callback interface.
use super::super::numeric::ProgramView;
use super::ingress::shared::SchemaView;
use super::{Descriptor, NumericDescriptor, admission};
#[cfg(verus_keep_ghost)]
#[path = "descriptor/spec.rs"]
pub(crate) mod spec;
#[cfg(verus_keep_ghost)]
use super::descriptor_admitted;
#[cfg(verus_keep_ghost)]
use vstd::prelude::*;

mod sealed {
    #[cfg(verus_keep_ghost)]
    use vstd::prelude::verus_verify;
    #[cfg_attr(verus_keep_ghost, verus_verify)]
    pub trait Sealed {}
    impl Sealed for super::Descriptor<'_> {}
    impl Sealed for super::NumericDescriptor<'_> {}
}

#[cfg_attr(verus_keep_ghost, verus_verify)]
pub(crate) enum DescriptorView<'p> {
    Legacy(&'p Descriptor<'p>),
    Numeric(&'p NumericDescriptor<'p>),
}

/// Opaque, immutable, nonauthorizing borrowed data projection.
#[cfg_attr(verus_keep_ghost, verus_verify)]
pub struct DescriptorProjection<'p> {
    pub(crate) view: DescriptorView<'p>,
}

/// Exactly two library-owned data projections; no evaluator or authority method.
#[cfg(not(verus_keep_ghost))]
pub trait DescriptorData<'p>: sealed::Sealed {
    /// Project the same descriptor reference without evaluating it.
    fn project(&'p self) -> DescriptorProjection<'p>;
}

#[cfg(verus_keep_ghost)]
verus! {
    pub trait DescriptorData<'p>: sealed::Sealed {
        spec fn model(&'p self) -> DescriptorProjection<'p>;
        fn project(&'p self) -> (result: DescriptorProjection<'p>)
            ensures result == self.model();
    }
}

// The native impls and the verified impls below have the same exec bodies;
// the verified ones add only the spec model, which the native build cannot
// spell inside an impl block without a macro the resolved-use checker refuses.
#[cfg(not(verus_keep_ghost))]
impl<'p> DescriptorData<'p> for Descriptor<'p> {
    fn project(&'p self) -> DescriptorProjection<'p> {
        DescriptorProjection {
            view: DescriptorView::Legacy(self),
        }
    }
}

#[cfg(not(verus_keep_ghost))]
impl<'p> DescriptorData<'p> for NumericDescriptor<'p> {
    fn project(&'p self) -> DescriptorProjection<'p> {
        DescriptorProjection {
            view: DescriptorView::Numeric(self),
        }
    }
}

#[cfg(verus_keep_ghost)]
verus! {
    impl<'p> DescriptorData<'p> for Descriptor<'p> {
        open(crate) spec fn model(&'p self) -> DescriptorProjection<'p> {
            DescriptorProjection {
                view: DescriptorView::Legacy(self),
            }
        }

        fn project(&'p self) -> (result: DescriptorProjection<'p>)
            ensures result == self.model(),
        {
            proof { reveal(<Descriptor as DescriptorData>::model); }
            DescriptorProjection {
                view: DescriptorView::Legacy(self),
            }
        }
    }

    impl<'p> DescriptorData<'p> for NumericDescriptor<'p> {
        open(crate) spec fn model(&'p self) -> DescriptorProjection<'p> {
            DescriptorProjection {
                view: DescriptorView::Numeric(self),
            }
        }

        fn project(&'p self) -> (result: DescriptorProjection<'p>)
            ensures result == self.model(),
        {
            proof { reveal(<NumericDescriptor as DescriptorData>::model); }
            DescriptorProjection {
                view: DescriptorView::Numeric(self),
            }
        }
    }
}

#[cfg(verus_keep_ghost)]
verus! {
impl<'p> DescriptorProjection<'p> {
    pub closed spec fn admitted(&self) -> bool {
        match self.view {
            DescriptorView::Legacy(d) => descriptor_admitted(d),
            DescriptorView::Numeric(d) => admission::numeric::spec::admitted(d),
        }
    }
}

/// Reveal the closed admission definition only inside its owning module.
/// Callers receive the exact existing legacy predicate, not private construction access.
pub(crate) proof fn legacy_admitted<'p>(d:&'p Descriptor<'p>)
    ensures d.model().admitted()==super::descriptor_admitted(d),
{
    reveal(<Descriptor as DescriptorData>::model);
    reveal(DescriptorProjection::admitted);
}

/// Numeric admission has the same complete predicate as the existing public wrapper.
pub(crate) proof fn numeric_admitted<'p>(d:&'p NumericDescriptor<'p>)
    ensures d.model().admitted()==super::numeric_descriptor_admitted(d),
{
    reveal(<NumericDescriptor as DescriptorData>::model);
    reveal(DescriptorProjection::admitted);
    reveal(super::numeric_descriptor_admitted);
}
}

impl DescriptorProjection<'_> {
    #[cfg_attr(verus_keep_ghost, verus_spec(result => ensures result == self.admitted(),))]
    pub(super) fn admit(&self) -> bool {
        #[cfg(verus_keep_ghost)]
        proof! { reveal(DescriptorProjection::admitted); reveal(descriptor_admitted); }
        match self.view {
            DescriptorView::Legacy(d) => admission::admitted(d),
            DescriptorView::Numeric(d) => admission::numeric::admitted(d),
        }
    }
}

#[cfg_attr(verus_keep_ghost, verus_verify)]
#[derive(Clone, Copy)]
pub(crate) enum OutputTypes<'p> {
    Legacy(&'p [super::InputLeaf]),
    Numeric(&'p [super::Domain<'p>]),
}
impl<'p> DescriptorProjection<'p> {
    #[cfg_attr(verus_keep_ghost,verus_spec(result=>ensures result==spec::state(self),))]
    pub(crate) fn state(&self) -> SchemaView<'p> {
        match self.view {
            DescriptorView::Legacy(d) => SchemaView::Legacy(d.state),
            DescriptorView::Numeric(d) => SchemaView::Numeric(d.state),
        }
    }
    #[cfg_attr(verus_keep_ghost,verus_spec(result=>ensures result==spec::command(self),))]
    pub(crate) fn command(&self) -> SchemaView<'p> {
        match self.view {
            DescriptorView::Legacy(d) => SchemaView::Legacy(d.command),
            DescriptorView::Numeric(d) => SchemaView::Numeric(d.command),
        }
    }
    #[cfg_attr(verus_keep_ghost,verus_spec(result=>ensures result==spec::context(self),))]
    pub(crate) fn context(&self) -> SchemaView<'p> {
        match self.view {
            DescriptorView::Legacy(d) => SchemaView::Legacy(d.context),
            DescriptorView::Numeric(d) => SchemaView::Numeric(d.context),
        }
    }
    #[cfg_attr(verus_keep_ghost,verus_spec(result=>ensures result==spec::program(self),))]
    pub(crate) fn program(&self) -> ProgramView<'p> {
        match self.view {
            DescriptorView::Legacy(d) => ProgramView::Legacy {
                inputs: d.program.inputs,
                outputs: d.program.outputs,
                nodes: d.program.nodes,
                roots: d.program.roots,
            },
            DescriptorView::Numeric(d) => ProgramView::Typed(d.program),
        }
    }
    #[cfg_attr(verus_keep_ghost,verus_spec(result=>ensures result==spec::outputs(self),))]
    pub(crate) fn outputs(&self) -> OutputTypes<'p> {
        match self.view {
            DescriptorView::Legacy(d) => OutputTypes::Legacy(d.output_types),
            DescriptorView::Numeric(d) => OutputTypes::Numeric(d.output_types),
        }
    }
    #[cfg_attr(verus_keep_ghost,verus_spec(result=>ensures result==spec::bindings(self),))]
    pub(crate) fn bindings(&self) -> &'p [super::Binding] {
        match self.view {
            DescriptorView::Legacy(d) => d.bindings,
            DescriptorView::Numeric(d) => d.bindings,
        }
    }
    #[cfg_attr(verus_keep_ghost,verus_spec(result=>ensures result==spec::decision_output(self),))]
    pub(crate) fn decision_output(&self) -> usize {
        match self.view {
            DescriptorView::Legacy(d) => d.decision_output,
            DescriptorView::Numeric(d) => d.decision_output,
        }
    }
    #[cfg_attr(verus_keep_ghost,verus_spec(result=>ensures result==spec::branches(self),))]
    pub(crate) fn branches(&self) -> &'p [super::Branch<'p>] {
        match self.view {
            DescriptorView::Legacy(d) => d.branches,
            DescriptorView::Numeric(d) => d.branches,
        }
    }
    #[cfg_attr(verus_keep_ghost,verus_spec(result=>ensures result==spec::channels(self),))]
    pub(crate) fn channels(&self) -> &'p [super::Channel<'p>] {
        match self.view {
            DescriptorView::Legacy(d) => d.channels,
            DescriptorView::Numeric(d) => d.channels,
        }
    }
    #[cfg_attr(verus_keep_ghost,verus_spec(result=>ensures result==spec::laws(self),))]
    pub(crate) fn laws(&self) -> &'p [super::laws::Law<'p>] {
        match self.view {
            DescriptorView::Legacy(d) => d.laws,
            DescriptorView::Numeric(d) => d.laws,
        }
    }
    #[cfg_attr(verus_keep_ghost,verus_spec(result=>ensures result==spec::required(self),))]
    pub(crate) fn required(&self) -> &'p [u32] {
        match self.view {
            DescriptorView::Legacy(d) => d.required,
            DescriptorView::Numeric(d) => d.required,
        }
    }
    #[cfg_attr(verus_keep_ghost,verus_spec(result=>ensures result==spec::limits(self),))]
    pub(crate) fn limits(&self) -> super::Limits {
        match self.view {
            DescriptorView::Legacy(d) => d.limits,
            DescriptorView::Numeric(d) => d.limits,
        }
    }
}
