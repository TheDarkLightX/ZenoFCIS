use zeno_fcis::Publication;
fn requires_clone<T: Clone>() {}
fn main() { requires_clone::<Publication<'static>>(); }
