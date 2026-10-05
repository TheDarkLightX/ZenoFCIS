//! Technical refusal never carries a committing candidate or a persisted subject.
use super::super::composition;
#[cfg(verus_keep_ghost)]
use vstd::prelude::*;

/// Technical refusal; no committing candidate or persisted subject is authorized.
#[cfg_attr(verus_keep_ghost, verus_verify)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum Refusal {
    /// The original producer, ingress, meter or law refusal.
    Core(composition::Failure),
    /// Canonical serialization exceeds representable length.
    Encoding,
    /// Recomputed subject differs from the supplied persisted bytes.
    ReplayMismatch,
    /// Original-wire serialization refused; no partial publication escapes.
    Output(super::super::super::canonical_v2::output::Failure),
    /// An emitted channel has no bound original destination/payload roots.
    Channel,
}
