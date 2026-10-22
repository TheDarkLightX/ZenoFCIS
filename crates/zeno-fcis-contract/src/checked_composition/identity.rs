//! The checker's source-bound identity.
//!
//! [`Identity::current`] binds four digests by value, and every result and
//! receipt records them and their combination:
//!
//! - this crate's own files, which are the checker and the contract
//!   compiler core it runs ([`CHECKER_SOURCES`]);
//! - `zeno-fcis-spec`'s digest of its own files: the `.zeno` parser;
//! - `zeno-fcis-compose`'s digest of its own files: the rules of the
//!   structural judge;
//! - the library's `EVALUATOR`, its digest of the approved evaluator source
//!   closure and build pins: the Authority, the law evaluator and the crates
//!   they are built from. It covers `Cargo.lock` and the root `Cargo.toml`
//!   as they were when it was last regenerated; after a change to either it
//!   still names the previous bytes until it is regenerated, and the
//!   library's evaluator identity test reports the difference.
//!
//! Each crate hashes only files inside its own package, so a published
//! archive builds the same way; a build from a published archive hashes the
//! manifest `cargo package` normalized, so its identity differs from a
//! build of the repository. A test walks this crate's directory and fails
//! when a file is neither hashed nor reviewed out as a test, a fixture or
//! package metadata.

use serde_json::{Value, json};
use zeno_fcis_synthesis::finite::v2_authority::EVALUATOR;

use super::hex;
use super::leaf::sha256;

macro_rules! checker_sources {
    ($($path:literal),+ $(,)?) => {
        &[$(($path, include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/", $path)) as &[u8])),+]
    };
}

const OWN_TAG: &[u8] = b"ZFCIS-CHECKED-COMPOSITION-CHECKER-SOURCES\0";
const TAG: &[u8] = b"ZFCIS-CHECKED-COMPOSITION-CHECKER\0";
const VERSION: u32 = 1;

/// Every file of this crate except its tests and fixtures, with its exact
/// bytes, in path order.
pub(crate) const CHECKER_SOURCES: &[(&str, &[u8])] = checker_sources![
    "Cargo.toml",
    "examples/trace_game_input.rs",
    "src/checked_composition.rs",
    "src/checked_composition/aggregate.rs",
    "src/checked_composition/assembly.rs",
    "src/checked_composition/compose.rs",
    "src/checked_composition/domain.rs",
    "src/checked_composition/formula.rs",
    "src/checked_composition/hierarchy.rs",
    "src/checked_composition/identity.rs",
    "src/checked_composition/joint.rs",
    "src/checked_composition/leaf.rs",
    "src/checked_composition/manifest.rs",
    "src/checked_composition/obligations.rs",
    "src/checked_composition/predicate.rs",
    "src/checked_composition/receipt.rs",
    "src/checked_composition/refusal.rs",
    "src/checked_composition/resolve.rs",
    "src/checked_composition/resolve_spec.rs",
    "src/checked_composition/structure.rs",
    "src/checked_composition/table.rs",
    "src/checked_composition/term.rs",
    "src/collections.rs",
    "src/collections/runtime.rs",
    "src/distribution.rs",
    "src/internals.rs",
    "src/internals/declarations.rs",
    "src/internals/delivery.rs",
    "src/internals/expr.rs",
    "src/internals/graph.rs",
    "src/internals/model.rs",
    "src/internals/policy.rs",
    "src/internals/quantity.rs",
    "src/internals/review.rs",
    "src/internals/review/domain.rs",
    "src/internals/review/evaluate.rs",
    "src/internals/rules.rs",
    "src/internals/schema.rs",
    "src/internals/table_expr.rs",
    "src/internals/table_rules.rs",
    "src/internals/tables.rs",
    "src/internals/tables_review.rs",
    "src/lib.rs",
    "src/refinement.rs",
    "src/refinement/machine.rs",
];

/// The digests a checker's identity combines.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct Identity {
    contract: [u8; 32],
    spec: [u8; 32],
    compose: [u8; 32],
    evaluator: [u8; 32],
    digest: [u8; 32],
}

impl Identity {
    /// The identity of the checker this build runs.
    pub(crate) fn current() -> Self {
        let contract = own_digest();
        let spec = zeno_fcis_spec::source_digest();
        let compose = zeno_fcis_compose::source_digest();
        let mut framed = TAG.to_vec();
        framed.extend_from_slice(&VERSION.to_be_bytes());
        for part in [&contract, &spec, &compose, &EVALUATOR] {
            framed.extend_from_slice(part);
        }
        Self {
            contract,
            spec,
            compose,
            evaluator: EVALUATOR,
            digest: sha256(&framed),
        }
    }

    /// The combined digest.
    pub(crate) const fn digest(&self) -> [u8; 32] {
        self.digest
    }

    /// The identity as JSON: the combined digest and each part.
    pub(crate) fn json(&self) -> Value {
        json!({
            "source_digest": hex(&self.digest),
            "sources": {
                "contract": hex(&self.contract),
                "spec": hex(&self.spec),
                "compose": hex(&self.compose),
                "evaluator": hex(&self.evaluator),
            },
        })
    }
}

/// The SHA-256 of [`CHECKER_SOURCES`], framed with each path and length.
fn own_digest() -> [u8; 32] {
    let mut framed = OWN_TAG.to_vec();
    framed.extend_from_slice(&VERSION.to_be_bytes());
    frame(&mut framed, CHECKER_SOURCES.len());
    for (path, bytes) in CHECKER_SOURCES {
        frame(&mut framed, path.len());
        framed.extend_from_slice(path.as_bytes());
        frame(&mut framed, bytes.len());
        framed.extend_from_slice(bytes);
    }
    sha256(&framed)
}

fn frame(out: &mut Vec<u8>, length: usize) {
    out.extend_from_slice(&(length as u64).to_be_bytes());
}
