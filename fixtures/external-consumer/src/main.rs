#![forbid(unsafe_code)]

use core::mem::size_of;
use zeno_fcis::prelude::*;


const AUTHORING_PROJECT: &str = r#"zeno 1;
project 7 consumer_authoring;
namespace 10 core;
type 100 state State;
type 101 command Command;
type 102 context Context;
type 103 destination Destination;
type 104 payload Payload;
reason 200 invalid precedence 0;
component 300 machine {
  owns 100;
  reads pre.100;
  writes post.100;
  contexts context.102;
  budget steps 32;
}
merge [300];
law 400 identity = pre.100 == pre.100;
claim 500 identity cvc5 relational = pre.100 == pre.100;
"#;
fn main() -> Result<(), &'static str> {
    let limits = BudgetLimits::zero().with_limit(Resource::Read, 1);
    let mut budget = Budget::new(limits);
    budget
        .charge(Resource::Read, 1)
        .map_err(|_| "exact budget unexpectedly rejected")?;
    if budget.charge(Resource::Read, 1).is_ok() {
        return Err("one-over budget unexpectedly succeeded");
    }

    if StableName::try_new("consumer").is_err() {
        return Err("stable project name was rejected");
    }
    let parsed = parse_project(AUTHORING_PROJECT, SourceLimits::default())
        .map_err(|_| "public authoring parser rejected a valid project")?;
    let authored = elaborate_project(parsed, ProjectLimits::default())
        .map_err(|_| "public authoring elaborator rejected a valid project")?;
    if authored.project_id().get() != 7 {
        return Err("authored project identity changed");
    }
    let value = Value::Bool(true);
    if value.kind() != zeno_fcis::ValueKind::Bool {
        return Err("unexpected admitted value kind");
    }

    // These compile-time references exercise the curated project, transition,
    // authority, composition, backend, and bootstrap exports without creating
    // placeholder authority or proof values.
    let _ = size_of::<Option<ProjectCatalog>>();
    let _ = size_of::<Option<TransitionDecision>>();
    let _ = size_of::<Option<BackendOperation>>();
    let _ = size_of::<Option<BootstrapSpec>>();
    let _ = size_of::<Option<RustCryptoSha256>>();
    let _ = size_of::<Option<ProjectSpec>>();

    Ok(())
}
