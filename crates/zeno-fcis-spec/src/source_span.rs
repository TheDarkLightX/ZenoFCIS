//! Shared public source spans with an exact four-field semantic view.

#[cfg(verus_keep_ghost)]
use vstd::prelude::*;

/// Half-open UTF-8 byte span with one-based display coordinates.
#[cfg_attr(verus_keep_ghost, verifier::verify)]
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct SourceSpan {
    start: u32,
    end: u32,
    line: u32,
    column: u32,
}

#[cfg_attr(verus_keep_ghost, verifier::verify)]
impl SourceSpan {
    /// Creates a span.
    #[must_use]
    pub const fn new(start: u32, end: u32, line: u32, column: u32) -> Self {
        Self {
            start,
            end,
            line,
            column,
        }
    }

    /// Returns the zero-based start byte.
    #[must_use]
    #[cfg_attr(verus_keep_ghost, verus_spec(result => ensures result == self.fields().0,))]
    pub const fn start(self) -> u32 {
        self.start
    }
    /// Returns the zero-based exclusive end byte.
    #[must_use]
    #[cfg_attr(verus_keep_ghost, verus_spec(result => ensures result == self.fields().1,))]
    pub const fn end(self) -> u32 {
        self.end
    }
    /// Returns the one-based line.
    #[must_use]
    #[cfg_attr(verus_keep_ghost, verus_spec(result => ensures result == self.fields().2,))]
    pub const fn line(self) -> u32 {
        self.line
    }
    /// Returns the one-based byte column.
    #[must_use]
    #[cfg_attr(verus_keep_ghost, verus_spec(result => ensures result == self.fields().3,))]
    pub const fn column(self) -> u32 {
        self.column
    }
}

#[cfg(verus_keep_ghost)]
verus! {
    impl SourceSpan {
        pub closed spec fn fields(self) -> (u32, u32, u32, u32) {
            (self.start, self.end, self.line, self.column)
        }
    }
}
