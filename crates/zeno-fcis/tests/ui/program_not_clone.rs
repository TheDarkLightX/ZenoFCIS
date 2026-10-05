use zeno_fcis::Program;
fn requires_clone<T: Clone>() {}
fn main() { requires_clone::<Program<'static>>(); }
