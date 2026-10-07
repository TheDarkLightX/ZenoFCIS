//! Retained immutable evidence bytes with a library-computed digest.

use alloc::boxed::Box;
use alloc::vec::Vec;

use crate::{CommitmentHasher, Hash32};

/// Exact retained artifact bytes and their computed digest.
///
/// This is an integrity container, not proof or authorization. A consumer
/// recomputes the digest under its selected provider before checking the
/// artifact's claim. Fields are private; no hash-only constructor exists.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct EvidenceArtifact {
    bytes: Box<[u8]>,
    digest: Hash32,
}

impl EvidenceArtifact {
    /// Owns the exact bytes and computes their digest with the selected provider.
    #[must_use]
    pub fn new<H: CommitmentHasher>(bytes: Vec<u8>) -> Self {
        let digest = H::hash(&bytes);
        Self {
            bytes: bytes.into_boxed_slice(),
            digest,
        }
    }

    /// Borrows the exact immutable artifact bytes.
    #[must_use]
    pub const fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// Returns the digest computed at construction.
    #[must_use]
    pub const fn digest(&self) -> Hash32 {
        self.digest
    }

    /// Recomputes the digest under the consumer's selected provider.
    #[must_use]
    pub fn matches<H: CommitmentHasher>(&self) -> bool {
        H::hash(&self.bytes) == self.digest
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;

    use crate::TestHasher;

    #[test]
    fn retains_owned_exact_bytes_and_rechecks_the_digest() {
        let mut caller_bytes = vec![7, 9, 11];
        let artifact = EvidenceArtifact::new::<TestHasher>(caller_bytes.clone());
        caller_bytes[0] = 99;
        assert_eq!(artifact.bytes(), &[7, 9, 11]);
        assert_eq!(artifact.digest(), TestHasher::hash(&[7, 9, 11]));
        assert!(artifact.matches::<TestHasher>());

        // This private-field substitution is possible only inside the module.
        // It checks the consuming integrity guard independently of construction.
        let mut corrupted = artifact.clone();
        corrupted.bytes[1] ^= 1;
        assert_eq!(corrupted.digest(), artifact.digest());
        assert!(!corrupted.matches::<TestHasher>());
    }
}
