use zeno_fcis::Program;
fn bad(p: &Program<'_>, value: &zeno_fcis::legacy::Value) { let _ = p.publish(value); }
fn main() {}
