use zeno_fcis::prelude::*;
fn evaluate<'a>(p: &'a Program<'a>, raw: Invocation<'a>) -> PublicationOutcome<'a> { p.publish(raw) }
fn main() { let _ = zero_limits().with_limit(Resource::Step, 1); let _ = evaluate; }
