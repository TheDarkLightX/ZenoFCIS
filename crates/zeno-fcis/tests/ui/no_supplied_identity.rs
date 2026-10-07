use zeno_fcis::{bind_program, CheckedCatalog};
fn bad(c: &CheckedCatalog<'_>) { let _ = bind_program(c, b"caller-source-identity"); }
fn main() {}
