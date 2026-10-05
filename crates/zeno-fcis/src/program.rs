//! One normal program family: direct names for the existing checked implementation.
//!
//! Declaration values are untrusted proposals. [`bind_catalog`] checks the complete
//! original schema, policy, descriptor and frame/channel links. [`bind_program`]
//! derives library identity and returns the private capability. No application
//! implementation, conversion callback or supplied identity participates.
//!
//! [`policy_bytes`] only serializes declared policy; it does not establish policy
//! adequacy, admission, authority or application intent. Invocation values contain
//! original envelopes. Publication poststate is an original envelope; delivery
//! destination/payload/marker values are bare original ZCVE with checked root links.

//!
//! Construction custody is enforced across crate boundaries:
//!
//! ```compile_fail,E0432
//! use zeno_fcis::CandidateBuilder;
//! ```
//!
//! ```compile_fail,E0451
//! use zeno_fcis::Usage;
//! let fabricated = Usage { counters: [0; 8] };
//! ```
//!
//! ```compile_fail,E0277
//! use zeno_fcis::Publication;
//! fn cloneable<T: Clone>() {}
//! cloneable::<Publication<'static>>();
//! ```
//!
//! ```compile_fail,E0308
//! use zeno_fcis::Program;
//! fn callback(p: &Program<'_>) { let _ = p.publish(|| ()); }
//! ```

pub use zeno_fcis_synthesis::finite::v2_authority::{
    Authority as Program, Evaluation, Publication, PublicationOutcome, Refusal, WireDelivery,
    bind as bind_program, policy_bytes,
};
pub use zeno_fcis_synthesis::finite::v2_catalog::{
    BoundCatalog as CheckedCatalog, Failure as CatalogRefusal, Limits as CatalogLimits,
    bind_original as bind_catalog,
};
pub use zeno_fcis_synthesis::finite::v2_composition::{
    Descriptor as ProgramDefinition, FrameBinding, Framing, Raw as Invocation,
};
pub use zeno_fcis_synthesis::finite::{
    V2Limits as Limits, V2MeterFailure as MeterFailure, V2Resource as Resource, V2Usage as Usage,
    v2_zero_limits as zero_limits,
};

/// Complete original schema descriptions and admission limits.
pub mod schema {
    pub use zeno_fcis_synthesis::finite::canonical_v2::schema::{
        Definition, Description, Field, Kind, Limits, Variant,
    };
}

/// Closed scalar control IR and typed input descriptions. It cannot call host code.
pub mod scalar {
    pub use zeno_fcis_synthesis::finite::{
        Domain, Op, V2InputField as InputField, V2InputLeaf as InputLeaf,
        V2InputVariant as InputVariant, V2ScalarProgram as ScalarProgram,
    };
}

/// Complete decision, footprint, channel and original input declarations.
pub mod declaration {
    pub use zeno_fcis_synthesis::finite::v2_composition::{
        Assignment, Atom, Binding, Branch, Channel, Class, DeliveryPlan, Domain, Expr,
        PayloadField, Reason, Schema, Selector, Source, TypedField,
    };
}

/// Closed law programs, including guarded observations and genuine genesis laws.
pub mod law {
    pub use zeno_fcis_synthesis::finite::v2_laws::{
        Atom, Diagnostic, Division, Kind, Law, Observation, Op, Program, ReadAttempt, Scope,
        Verdict,
    };
}

/// Immutable reports returned by actual evaluation; reports are not publication capabilities.
pub mod report {
    pub use zeno_fcis_synthesis::finite::v2_composition::{
        Attempt, Candidate as EvaluatedDecision, Class, Failure as ExecutionRefusal, ReadAttempt,
    };
    pub use zeno_fcis_synthesis::finite::v2_laws::Failure as LawRefusal;
}
