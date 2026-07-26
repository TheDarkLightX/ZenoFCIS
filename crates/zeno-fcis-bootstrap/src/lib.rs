//! Deterministic project starter generation from reviewed schemas and catalogs.
//!
//! The generator emits ordinary inspectable source, protocol snapshots,
//! negative/positive vectors inherited from schema generation, migration and
//! evidence stubs, read-only CI, and a content-addressed bootstrap manifest.

#![forbid(unsafe_code)]

mod model;
mod render;
mod templates;

pub use model::{
    BOOTSTRAP_FORMAT_VERSION, BOOTSTRAP_GENERATOR_ID, BootstrapBundle, BootstrapError,
    BootstrapFile, BootstrapLimits, BootstrapSpec, MAX_BOOTSTRAP_FILE_BYTES, MAX_BOOTSTRAP_FILES,
    MAX_BOOTSTRAP_TOTAL_BYTES,
};
pub use render::generate_project;
