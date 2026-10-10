#![forbid(unsafe_code)]

use core::mem::size_of;
use zeno_fcis::prelude::*;

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

    Ok(())
}
