use zeno_fcis::Program;
fn bad(p: &Program<'_>) { let _ = p.publish(|| ()); }
fn main() {}
