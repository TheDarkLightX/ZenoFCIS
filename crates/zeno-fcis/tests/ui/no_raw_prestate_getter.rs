use zeno_fcis::Program;
fn bad(p: &Program<'_>) { let _ = p.pre_state(); }
fn main() {}
