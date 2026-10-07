use zeno_fcis::{CheckedCatalog, ProgramDefinition};
fn bad<'a>(p: &CheckedCatalog<'a>) -> &'a mut ProgramDefinition<'a> { p.descriptor() }
fn main() {}
