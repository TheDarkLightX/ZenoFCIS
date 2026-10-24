//! Source of a program input position.

#[cfg(verus_keep_ghost)]
use vstd::prelude::*;

/// The original input a scalar program position reads.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
#[cfg_attr(verus_keep_ghost, verus_verify)]
pub enum Source {
    State,
    Command,
    Context,
}
