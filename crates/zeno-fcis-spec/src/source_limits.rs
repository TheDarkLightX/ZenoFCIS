//! Actual public source limits, shared with the lexer proof.

#[cfg(verus_keep_ghost)]
use vstd::prelude::*;

/// Maximum accepted UTF-8 source length.
#[cfg_attr(verus_keep_ghost, verus_spec)]
pub const MAX_SOURCE_BYTES: usize = 1024 * 1024;
/// Maximum accepted lexer token count.
#[cfg_attr(verus_keep_ghost, verus_spec)]
pub const MAX_SOURCE_TOKENS: usize = 262_144;
/// Maximum diagnostics retained in a returned set.
#[cfg_attr(verus_keep_ghost, verus_spec)]
pub const MAX_RETAINED_DIAGNOSTICS: usize = 256;
/// Caller-selected limits bounded by the hard language limits.
#[cfg_attr(verus_keep_ghost, verifier::verify)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SourceLimits {
    max_bytes: usize,
    max_tokens: usize,
    max_diagnostics: usize,
}

#[cfg_attr(verus_keep_ghost, verifier::verify)]
impl SourceLimits {
    /// Creates a nonzero limit set no larger than the language hard limits.
    pub const fn try_new(
        max_bytes: usize,
        max_tokens: usize,
        max_diagnostics: usize,
    ) -> Option<Self> {
        if max_bytes == 0
            || max_bytes > MAX_SOURCE_BYTES
            || max_tokens == 0
            || max_tokens > MAX_SOURCE_TOKENS
            || max_diagnostics == 0
            || max_diagnostics > MAX_RETAINED_DIAGNOSTICS
        {
            return None;
        }
        Some(Self {
            max_bytes,
            max_tokens,
            max_diagnostics,
        })
    }

    /// Returns the source-byte limit.
    #[must_use]
    #[cfg_attr(verus_keep_ghost, verus_spec(result => ensures result == self.fields().0,))]
    pub const fn max_bytes(self) -> usize {
        self.max_bytes
    }

    /// Returns the token limit.
    #[must_use]
    #[cfg_attr(verus_keep_ghost, verus_spec(result => ensures result == self.fields().1,))]
    pub const fn max_tokens(self) -> usize {
        self.max_tokens
    }

    /// Returns the retained-diagnostic limit.
    #[must_use]
    #[cfg_attr(verus_keep_ghost, verus_spec(result => ensures result == self.fields().2,))]
    pub const fn max_diagnostics(self) -> usize {
        self.max_diagnostics
    }
}

#[cfg_attr(verus_keep_ghost, verifier::verify)]
impl Default for SourceLimits {
    fn default() -> Self {
        Self {
            max_bytes: MAX_SOURCE_BYTES,
            max_tokens: MAX_SOURCE_TOKENS,
            max_diagnostics: MAX_RETAINED_DIAGNOSTICS,
        }
    }
}

#[cfg(verus_keep_ghost)]
verus! {
    impl SourceLimits {
        pub closed spec fn fields(self) -> (usize, usize, usize) {
            (self.max_bytes, self.max_tokens, self.max_diagnostics)
        }
    }
}
