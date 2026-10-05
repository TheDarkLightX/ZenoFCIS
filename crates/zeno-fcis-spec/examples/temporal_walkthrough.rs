//! Inert temporal authoring; evaluation belongs to an admitted checked program.
use zeno_fcis_spec::{ClaimMode, ProjectLimits, SourceLimits, elaborate_project, parse_project};
fn main() {
    let source = "zeno 1; project 1 temporal; claim 2 finite_trace cvc5 finite 3 = eventually true; claim 3 obligation lean unbounded = always true;";
    let parsed =
        parse_project(source, SourceLimits::default()).unwrap_or_else(|set| panic!("{set}"));
    let project =
        elaborate_project(parsed, ProjectLimits::default()).unwrap_or_else(|set| panic!("{set}"));
    assert_eq!(project.claims()[0].mode(), ClaimMode::Finite { horizon: 3 });
    assert_eq!(project.claims()[1].mode(), ClaimMode::UnboundedProof);
    println!("two inert claims; runtime admission and publication have not run");
}
