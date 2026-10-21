#![allow(unreachable_pub)]
//! The identical step-4 journey, with production BLS verification and test keys.
#[path = "support/issuer.rs"]
mod issuer;
#[path = "../../../core-apps/zusd-v1/tests/support/step4.rs"]
mod support;
include!("../../../core-apps/zusd-v1/tests/support/journey.rs");
