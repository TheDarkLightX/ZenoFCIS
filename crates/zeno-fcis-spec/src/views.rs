//! Deterministic derived composition, code, manifests, and graphs.

use alloc::boxed::Box;
use alloc::string::String;
use alloc::vec::Vec;
use core::fmt::Write as _;

use zeno_fcis_codec::{CanonicalEncode, CommitmentHasher, Domain, EncodeError, Hash32, commitment};

use crate::{ComponentDecl, FootprintKind, ProjectSpec, StableId};

/// Deterministic unresolved obligation family.
#[allow(missing_docs)]
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum ObligationKind {
    Assumption,
    CompleteFootprint,
    ParallelLaw,
    SequentialParity,
}

/// One authoring-time obligation that generated code cannot discharge.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct UnresolvedObligation {
    kind: ObligationKind,
    component: Option<StableId>,
    peer: Option<StableId>,
    claim: Option<StableId>,
}
impl UnresolvedObligation {
    /// Returns the obligation family.
    #[must_use]
    pub const fn kind(&self) -> ObligationKind {
        self.kind
    }
    /// Returns the primary component when applicable.
    #[must_use]
    pub const fn component(&self) -> Option<StableId> {
        self.component
    }
    /// Returns the peer component for parallel obligations.
    #[must_use]
    pub const fn peer(&self) -> Option<StableId> {
        self.peer
    }
    /// Returns the referenced claim when applicable.
    #[must_use]
    pub const fn claim(&self) -> Option<StableId> {
        self.claim
    }
}

/// Derived fixed dimensions and semantic identity.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DerivedComposition {
    machines: usize,
    state_slots: usize,
    ports: usize,
    semantic_program_hash: Hash32,
    obligations: Box<[UnresolvedObligation]>,
}
impl DerivedComposition {
    /// Returns the compile-time machine dimension.
    #[must_use]
    pub const fn machines(&self) -> usize {
        self.machines
    }
    /// Returns the compile-time state-slot dimension.
    #[must_use]
    pub const fn state_slots(&self) -> usize {
        self.state_slots
    }
    /// Returns the compile-time port dimension.
    #[must_use]
    pub const fn ports(&self) -> usize {
        self.ports
    }
    /// Returns the content-derived authoring program identity.
    #[must_use]
    pub const fn semantic_program_hash(&self) -> Hash32 {
        self.semantic_program_hash
    }
    /// Returns the complete deterministic unresolved inventory.
    #[must_use]
    pub const fn obligations(&self) -> &[UnresolvedObligation] {
        &self.obligations
    }
}

/// Derives conservative dimensions, conflict obligations, and program identity.
pub fn derive_composition<H: CommitmentHasher>(
    spec: &ProjectSpec,
) -> Result<DerivedComposition, EncodeError> {
    let bytes = spec.canonical_bytes()?;
    let semantic_program_hash =
        commitment::<H>(Domain::new("zeno-fcis/semantic-program", 1)?, &bytes)?;
    let mut obligations = Vec::new();
    for component in spec.components() {
        for claim in component.assumptions() {
            obligations.push(UnresolvedObligation {
                kind: ObligationKind::Assumption,
                component: Some(component.id()),
                peer: None,
                claim: Some(*claim),
            });
        }
        obligations.push(UnresolvedObligation {
            kind: ObligationKind::CompleteFootprint,
            component: Some(component.id()),
            peer: None,
            claim: None,
        });
    }
    for (index, left) in spec.components().iter().enumerate() {
        for right in spec.components().iter().skip(index + 1) {
            if interferes(left, right) {
                obligations.push(UnresolvedObligation {
                    kind: ObligationKind::ParallelLaw,
                    component: Some(left.id()),
                    peer: Some(right.id()),
                    claim: None,
                });
            }
        }
    }
    obligations.push(UnresolvedObligation {
        kind: ObligationKind::SequentialParity,
        component: None,
        peer: None,
        claim: None,
    });
    obligations.sort();
    obligations.dedup();
    Ok(DerivedComposition {
        machines: spec.components().len(),
        state_slots: spec
            .components()
            .iter()
            .map(|value| value.owned_state().len())
            .max()
            .unwrap_or(0),
        ports: spec
            .components()
            .iter()
            .map(|value| value.ports().len())
            .max()
            .unwrap_or(0),
        semantic_program_hash,
        obligations: obligations.into_boxed_slice(),
    })
}

fn interferes(left: &ComponentDecl, right: &ComponentDecl) -> bool {
    left.footprints().iter().any(|a| {
        right.footprints().iter().any(|b| {
            a.path() == b.path()
                && matches!(
                    (a.kind(), b.kind()),
                    (
                        FootprintKind::Write,
                        FootprintKind::Write | FootprintKind::Read
                    ) | (FootprintKind::Read, FootprintKind::Write)
                        | (FootprintKind::Effect, FootprintKind::Effect)
                        | (FootprintKind::Outbox, FootprintKind::Outbox)
                )
        })
    })
}

/// Generated source and content-addressed project manifest.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GeneratedProject {
    rust: Box<str>,
    manifest: Box<[u8]>,
    derived: DerivedComposition,
}
impl GeneratedProject {
    /// Returns deterministic Rust source.
    #[must_use]
    pub const fn rust(&self) -> &str {
        &self.rust
    }
    /// Returns `PROJECT_MANIFEST.zfcis` bytes.
    #[must_use]
    pub const fn manifest(&self) -> &[u8] {
        &self.manifest
    }
    /// Returns derived composition metadata.
    #[must_use]
    pub const fn derived(&self) -> &DerivedComposition {
        &self.derived
    }
}

/// Generates const-generic integration source and a canonical manifest.
pub fn generate_project<H: CommitmentHasher>(
    spec: &ProjectSpec,
) -> Result<GeneratedProject, EncodeError> {
    let derived = derive_composition::<H>(spec)?;
    let mut rust = String::new();
    let _ = writeln!(
        rust,
        "// @generated by zeno-fcis-spec 1.0.0-rc.3; diagnostic only"
    );
    let _ = writeln!(rust, "pub const MACHINES: usize = {};", derived.machines);
    let _ = writeln!(
        rust,
        "pub const STATE_SLOTS: usize = {};",
        derived.state_slots
    );
    let _ = writeln!(rust, "pub const PORTS: usize = {};", derived.ports);
    let _ = writeln!(
        rust,
        "pub const SEMANTIC_PROGRAM_HASH_HEX: &str = \"{}\";",
        derived.semantic_program_hash
    );
    let _ = writeln!(
        rust,
        "pub type Projection = zeno_fcis::composed_program::ProjectionPlan<MACHINES, STATE_SLOTS, PORTS>;"
    );
    let _ = writeln!(
        rust,
        "// Bind concrete machines only through ComposedDomainProgram::try_new."
    );
    let _ = writeln!(
        rust,
        "// Generated source cannot construct evidence, BackendCertificate, authority, receipts, or commits."
    );
    for obligation in derived.obligations() {
        let _ = writeln!(
            rust,
            "// unresolved: {:?} component={:?} peer={:?} claim={:?}",
            obligation.kind,
            obligation.component.map(StableId::get),
            obligation.peer.map(StableId::get),
            obligation.claim.map(StableId::get)
        );
    }
    let mut manifest = Vec::new();
    manifest.extend_from_slice(b"ZFCIS-PROJECT-MANIFEST\0");
    manifest.extend_from_slice(&1u16.to_be_bytes());
    manifest.extend_from_slice(derived.semantic_program_hash.as_bytes());
    manifest.extend_from_slice(
        &(u32::try_from(derived.machines).map_err(|_| EncodeError::LengthOverflow)?).to_be_bytes(),
    );
    manifest.extend_from_slice(
        &(u32::try_from(derived.state_slots).map_err(|_| EncodeError::LengthOverflow)?)
            .to_be_bytes(),
    );
    manifest.extend_from_slice(
        &(u32::try_from(derived.ports).map_err(|_| EncodeError::LengthOverflow)?).to_be_bytes(),
    );
    manifest.extend_from_slice(
        &(u32::try_from(derived.obligations.len()).map_err(|_| EncodeError::LengthOverflow)?)
            .to_be_bytes(),
    );
    Ok(GeneratedProject {
        rust: rust.into_boxed_str(),
        manifest: manifest.into_boxed_slice(),
        derived,
    })
}

/// Supported deterministic graph projection.
#[allow(missing_docs)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GraphFormat {
    Dot,
    Mermaid,
    Json,
}

/// Renders a deterministic diagnostic topology. The result grants no authority.
#[must_use]
pub fn render_graph(spec: &ProjectSpec, format: GraphFormat) -> String {
    let mut output = String::new();
    match format {
        GraphFormat::Dot => {
            output.push_str("digraph zeno_fcis {\n");
            for component in spec.components() {
                let _ = writeln!(
                    output,
                    "  c{} [label=\"{}\"];",
                    component.id().get(),
                    component.name().as_str()
                );
            }
            for wiring in spec.composition().wirings() {
                let _ = writeln!(
                    output,
                    "  c{} -> c{} [label=\"{}.{}\"];",
                    wiring.source_component().get(),
                    wiring.destination_component().get(),
                    wiring.source_port().get(),
                    wiring.destination_port().get()
                );
            }
            output.push_str("}\n");
        }
        GraphFormat::Mermaid => {
            output.push_str("flowchart LR\n");
            for component in spec.components() {
                let _ = writeln!(
                    output,
                    "  c{}[{}]",
                    component.id().get(),
                    component.name().as_str()
                );
            }
            for wiring in spec.composition().wirings() {
                let _ = writeln!(
                    output,
                    "  c{} -->|{}.{}| c{}",
                    wiring.source_component().get(),
                    wiring.source_port().get(),
                    wiring.destination_port().get(),
                    wiring.destination_component().get()
                );
            }
        }
        GraphFormat::Json => {
            output.push_str("{\"schema\":\"zeno-fcis/graph/1\",\"components\":[");
            for (index, component) in spec.components().iter().enumerate() {
                if index != 0 {
                    output.push(',');
                }
                let _ = write!(
                    output,
                    "{{\"id\":{},\"name\":\"{}\"}}",
                    component.id().get(),
                    component.name().as_str()
                );
            }
            output.push_str("],\"wirings\":[");
            for (index, wiring) in spec.composition().wirings().iter().enumerate() {
                if index != 0 {
                    output.push(',');
                }
                let _ = write!(
                    output,
                    "{{\"destination_component\":{},\"destination_port\":{},\"source_component\":{},\"source_port\":{}}}",
                    wiring.destination_component().get(),
                    wiring.destination_port().get(),
                    wiring.source_component().get(),
                    wiring.source_port().get()
                );
            }
            output.push_str("]}\n");
        }
    }
    output
}
