use zeno_fcis::Resource;
fn bad(r: Resource) { match r { Resource::Read|Resource::Write|Resource::Candidate|Resource::Effect|Resource::Byte|Resource::WitnessByte|Resource::Depth|Resource::Step => () } }
fn main() {}
