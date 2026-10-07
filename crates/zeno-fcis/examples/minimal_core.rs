//! Bind a complete declarative program and publish an original-wire invocation.
//!
//! The support module contains unadmitted example data, not application execution.
//! Its finite policy illustrates the API; it is not a policy for financial accounts.
#[path = "support/program_fixture.rs"]
mod support;

use zeno_fcis::prelude::*;

fn main() -> Result<(), &'static str> {
    let mut result = Ok(());
    support::with_material(support::generous(), true, |declaration| {
        result = run(declaration);
    });
    result
}

fn run(declaration: support::Material<'_>) -> Result<(), &'static str> {
    let catalog = bind_catalog(
        declaration.original,
        declaration.description,
        support::catalog_limits(),
        declaration.policy,
        declaration.descriptor,
        declaration.framing,
        declaration.links,
    )
    .map_err(|_| "catalog admission refused")?;
    let program = bind_program(&catalog).map_err(|_| "program admission refused")?;
    let (state, command, context) = support::originals(0, true);
    let initial = program.publish_genesis(&state);
    if !matches!(initial, PublicationOutcome::Commit(_)) {
        return Err("genesis refused");
    }
    let original = Invocation {
        state: &state,
        command: &command,
        context: &context,
    };
    let PublicationOutcome::Commit(publication) = program.publish(original) else {
        return Err("publication refused");
    };
    if publication.poststate() != support::frame(100, &[9, 0, 0, 0, 1, 0, 0, 1]) {
        return Err("unexpected original-wire successor");
    }
    if publication.evaluation().usage().used(Resource::Step) == 0 {
        return Err("missing library instruction usage");
    }
    let PublicationOutcome::Commit(replayed) =
        program.replay_publication(original, publication.subject())
    else {
        return Err("replay refused");
    };
    if replayed.subject() != publication.subject() {
        return Err("replay subject changed");
    }
    Ok(())
}
