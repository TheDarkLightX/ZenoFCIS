//! Private original relation and induction oracle; never a production dependency.
#[path = "original/src/logic.rs"]
pub(crate) mod logic;
#[path = "original/src/induction.rs"]
mod induction;
pub(crate) use induction::evaluate_invariant;
#[path = "original/tests/rc3_acceptance.rs"]
mod rc3_acceptance;
#[path = "original/tests/zusd_lane_gaps.rs"]
mod zusd_lane_gaps;
#[path = "temporal_walkthrough.rs"]
mod temporal_walkthrough;
#[test]
fn original_temporal_walkthrough_executes() { temporal_walkthrough::run(); }
