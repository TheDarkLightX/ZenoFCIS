//! Module wiring for the actual owned program and its production getters.
use crate::evaluation::{Domain, Op};
#[path = "../../crates/zeno-fcis-synthesis/src/finite/ir/program.rs"]
mod program;
pub use program::Program;
