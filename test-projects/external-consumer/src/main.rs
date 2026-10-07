#![forbid(unsafe_code)]

use core::mem::size_of;
// V2 migration: the checked program API is the crate root; V1 data types and
// standalone tools moved under `legacy`. This is not the normal V2 authority route.
use zeno_fcis::legacy;
use zeno_fcis::legacy::prelude::*;
use zeno_fcis::{PublicationOutcome, zero_limits};

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
    foundational_errors_interoperate()?;
    // V2: the library owns the meter. A caller sets limits; it can neither charge
    // nor report usage, so the V1 caller-side charge checks have no equivalent.
    let limits = zero_limits().with_limit(zeno_fcis::Resource::Step, 1);
    if limits.limit(zeno_fcis::Resource::Step) != 1 || limits.limit(zeno_fcis::Resource::Read) != 0
    {
        return Err("V2 limits did not record exactly the requested bound");
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
    let value = Value::boolean(true);
    if value.kind() != legacy::ValueKind::Bool {
        return Err("unexpected admitted value kind");
    }

    // These compile-time references exercise the curated project, transition,
    // authority, composition, backend, and bootstrap exports without creating
    // placeholder authority or proof values.
    let _ = size_of::<Option<ProjectCatalog>>();
    let _ = size_of::<Option<PublicationOutcome<'static>>>();
    let _ = size_of::<Option<BackendOperation>>();
    let _ = size_of::<Option<BootstrapSpec>>();
    let _ = size_of::<Option<RustCryptoSha256>>();
    let _ = size_of::<Option<ProjectSpec>>();

    Ok(())
}

fn foundational_errors_interoperate() -> Result<(), &'static str> {
    fn standard_error<E: core::error::Error + Send + Sync + 'static>() {}
    standard_error::<legacy::EncodeError>();
    standard_error::<legacy::DecodeError>();
    standard_error::<legacy::LengthError>();
    standard_error::<legacy::TextError>();
    standard_error::<legacy::ValueError>();
    standard_error::<legacy::PlanError>();
    standard_error::<legacy::PatchError>();

    fn domain_error() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        Domain::new("", 1)?;
        Ok(())
    }
    match domain_error() {
        Err(error)
            if error.downcast_ref::<legacy::EncodeError>()
                == Some(&legacy::EncodeError::InvalidDomain) =>
        {
            Ok(())
        }
        _ => Err("standard error propagation changed the typed codec error"),
    }
}
