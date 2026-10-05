//! Private historical reference algorithms; no public production interface.

extern crate alloc;

pub(crate) mod adapter;
pub(crate) mod adapter_zenodex;
pub(crate) mod authenticated_authority;
pub(crate) mod authority;
pub(crate) mod composed_program;
pub(crate) mod core;
pub(crate) mod generated_fixture;
pub(crate) mod domain;
pub(crate) mod finite;
pub(crate) mod laws;
pub(crate) mod profile_zenodex;
pub(crate) mod receipt;
pub(crate) mod refine;
pub(crate) mod shell;
pub(crate) mod sqlite;
pub(crate) mod transition;
pub(crate) mod templates;
pub(crate) mod spec;

mod tests {
    mod bounded_completion;
    mod composed_construction;
    mod fixed_matrix;
    mod kernel_laws;
    mod minimal_core;
    mod native_mount;
    mod preparation;
    mod checked_continuation;
    mod inventory_binding_derivative;
    mod probe_divergence;
    mod transition_laws;
}
