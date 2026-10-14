//! Borrows the actual library-owned program without copying or changing ABI order.
use crate::symbolic::program::Program;
#[cfg(verus_keep_ghost)]
use vstd::prelude::*;
use zeno_fcis_synthesis::finite::Program as LibraryProgram;

#[cfg_attr(verus_keep_ghost, verus_verify)]
impl<'a> Program<'a> {
    pub(crate) fn of(program: &'a LibraryProgram) -> Self {
        #[cfg(verus_keep_ghost)]
        #[cfg_attr(verus_keep_ghost, cfg(verus_keep_ghost_body))]
        verus_proof_expr!(ensures(|result: Program<'a>|
            result.inputs@ == program.view().0
                && result.outputs@ == program.view().1
                && result.nodes@ == program.view().2
                && result.roots@ == program.view().3));
        Self {
            inputs: program.inputs(),
            outputs: program.outputs(),
            nodes: program.nodes(),
            roots: program.roots(),
        }
    }
}
