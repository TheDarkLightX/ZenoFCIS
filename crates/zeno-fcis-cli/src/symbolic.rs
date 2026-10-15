//! Symbolic per-case checks with the pinned CVC5 and Z3, for domains the
//! exhaustive checkers cannot enumerate.
//!
//! The queries encode exactly what the library runs: a contract's bound
//! descriptor (its scalar decision program, output types, decision table and
//! law programs), or two admitted finite programs. Nothing is encoded from a
//! rule's source text, so the generator's compilers and the library's
//! decoders are on the same side of every query.
//!
//! Evidence keeps the formal-tools classification. CVC5's `unsat` with proof
//! steps is a proposal (Attested; the proof is not checked), and Z3's `unsat`
//! only corroborates it. A model counts only after it replays through the
//! library's own evaluators, which makes a refutation Checked. A construct
//! the encoding does not know makes the check inconclusive; it is never
//! approximated.
//!
//! This module is pure. The shell in `symbolic_command` runs the solvers.

pub(crate) mod equivalence;
pub(crate) mod program;
pub(crate) mod smt;
#[cfg(test)]
pub(crate) mod testing;
pub(crate) mod verdict;

mod unsupported;
pub(crate) use unsupported::Unsupported;
