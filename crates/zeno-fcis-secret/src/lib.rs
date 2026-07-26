//! Zeroizing and constant-time secret-handling boundaries.
//!
//! This crate deliberately does **not** implement `Debug`, `Display`, `Clone`,
//! `Copy`, `AsRef<[u8]>`, serialization, canonical encoding, or implicit
//! dereferencing for secret containers. Access requires an explicit
//! [`ExposurePermit`] and a [`HardenedExecution`] token. Constant-time equality
//! and selection use the reviewed `subtle` primitives, while owned storage is
//! erased on drop through `zeroize`.
//!
//! These APIs reduce accidental exposure; they do not prove physical
//! side-channel freedom. Compilers may create temporary copies, operating
//! systems may page memory, and hardware may leak through microarchitectural,
//! power, or electromagnetic channels. Production use therefore requires the
//! deployment-bound evidence modeled by `zeno-fcis-security`.
//!
//! Secret containers intentionally cannot be formatted:
//!
//! ```compile_fail
//! use zeno_fcis_secret::SecretBytes;
//!
//! let secret = SecretBytes::<32>::try_new([7_u8; 32])?;
//! println!("{secret:?}");
//! # Ok::<(), zeno_fcis_secret::SecretError>(())
//! ```
//!
//! They also cannot be cloned implicitly:
//!
//! ```compile_fail
//! use zeno_fcis_secret::SecretBytes;
//!
//! let secret = SecretBytes::<32>::try_new([7_u8; 32])?;
//! let duplicate = secret.clone();
//! # let _ = duplicate;
//! # Ok::<(), zeno_fcis_secret::SecretError>(())
//! ```

#![cfg_attr(not(feature = "std"), no_std)]
#![forbid(unsafe_code)]

extern crate alloc;

use alloc::boxed::Box;
use alloc::vec::Vec;
use core::fmt;

use subtle::{Choice, ConditionallySelectable, ConstantTimeEq};
use zeroize::{Zeroize, ZeroizeOnDrop};
use zeno_fcis_codec::{CanonicalEncode, CommitmentHasher, Domain, EncodeError, Hash32, commitment};

/// Maximum bytes accepted by a dynamically sized secret container.
pub const MAX_SECRET_BYTES: usize = 1024 * 1024;
/// Canonical format version for exposure permits and receipts.
pub const SECRET_AUDIT_FORMAT_VERSION: u16 = 1;

/// Proof token that debug assertions are disabled for the current binary.
///
/// `subtle` documents that its debug-only invariant checks may branch on secret
/// data. Public constant-time and exposure operations therefore require a token
/// that can be constructed only when `debug_assertions` is disabled.
pub struct HardenedExecution {
    _private: (),
}

impl HardenedExecution {
    /// Requires a release-style build with debug assertions disabled.
    pub fn require_release() -> Result<Self, SecretError> {
        if cfg!(debug_assertions) {
            Err(SecretError::DebugAssertionsEnabled)
        } else {
            Ok(Self { _private: () })
        }
    }

    #[cfg(test)]
    fn for_test() -> Self {
        Self { _private: () }
    }
}

/// Explicit authority and purpose for exposing secret bytes to one operation.
#[derive(Clone, Copy, Eq, PartialEq)]
pub struct ExposurePermit {
    authority_hash: Hash32,
    purpose_hash: Hash32,
    maximum_bytes: u32,
}

impl ExposurePermit {
    /// Creates a bounded exposure permit with nonzero authority and purpose.
    pub fn try_new(
        authority_hash: Hash32,
        purpose_hash: Hash32,
        maximum_bytes: u32,
    ) -> Result<Self, SecretError> {
        if authority_hash == Hash32::ZERO || purpose_hash == Hash32::ZERO {
            return Err(SecretError::ZeroHash);
        }
        if maximum_bytes == 0 {
            return Err(SecretError::InvalidLength);
        }
        Ok(Self {
            authority_hash,
            purpose_hash,
            maximum_bytes,
        })
    }

    /// Returns the reviewed authority commitment.
    #[must_use]
    pub const fn authority_hash(self) -> Hash32 {
        self.authority_hash
    }

    /// Returns the reviewed purpose commitment.
    #[must_use]
    pub const fn purpose_hash(self) -> Hash32 {
        self.purpose_hash
    }

    /// Returns the maximum bytes authorized for one exposure.
    #[must_use]
    pub const fn maximum_bytes(self) -> u32 {
        self.maximum_bytes
    }

    /// Computes the content-derived permit identity.
    pub fn commitment<H: CommitmentHasher>(&self) -> Result<Hash32, SecretError> {
        hash_canonical::<H>("zeno-fcis/secret-exposure-permit", self)
    }

    fn authorize(self, length: usize) -> Result<(), SecretError> {
        let length = u32::try_from(length).map_err(|_| SecretError::InvalidLength)?;
        if length > self.maximum_bytes {
            Err(SecretError::ExposureTooLarge {
                attempted: length,
                maximum: self.maximum_bytes,
            })
        } else {
            Ok(())
        }
    }
}

impl CanonicalEncode for ExposurePermit {
    fn encode_to(&self, output: &mut Vec<u8>) -> Result<(), EncodeError> {
        output.extend_from_slice(b"ZFCIS-SECRET-PERMIT\0");
        output.extend_from_slice(&SECRET_AUDIT_FORMAT_VERSION.to_be_bytes());
        output.extend_from_slice(self.authority_hash.as_bytes());
        output.extend_from_slice(self.purpose_hash.as_bytes());
        output.extend_from_slice(&self.maximum_bytes.to_be_bytes());
        Ok(())
    }
}

/// Audit value emitted by an explicit secret exposure.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ExposureEvent {
    secret_id: Hash32,
    permit_hash: Hash32,
    bytes_exposed: u32,
}

impl ExposureEvent {
    /// Returns the caller-supplied non-secret secret identity.
    #[must_use]
    pub const fn secret_id(self) -> Hash32 {
        self.secret_id
    }

    /// Returns the exact permit commitment.
    #[must_use]
    pub const fn permit_hash(self) -> Hash32 {
        self.permit_hash
    }

    /// Returns the public number of exposed bytes.
    #[must_use]
    pub const fn bytes_exposed(self) -> u32 {
        self.bytes_exposed
    }
}

impl CanonicalEncode for ExposureEvent {
    fn encode_to(&self, output: &mut Vec<u8>) -> Result<(), EncodeError> {
        output.extend_from_slice(b"ZFCIS-SECRET-EXPOSURE\0");
        output.extend_from_slice(&SECRET_AUDIT_FORMAT_VERSION.to_be_bytes());
        output.extend_from_slice(self.secret_id.as_bytes());
        output.extend_from_slice(self.permit_hash.as_bytes());
        output.extend_from_slice(&self.bytes_exposed.to_be_bytes());
        Ok(())
    }
}

/// Result of an explicitly authorized secret exposure.
pub struct Exposed<T> {
    value: T,
    event: ExposureEvent,
}

impl<T> Exposed<T> {
    /// Returns the audit event without exposing or copying the secret.
    #[must_use]
    pub const fn event(&self) -> ExposureEvent {
        self.event
    }

    /// Consumes the result into its operation result and audit event.
    #[must_use]
    pub fn into_parts(self) -> (T, ExposureEvent) {
        (self.value, self.event)
    }
}

/// Fixed-size owned secret bytes.
///
/// The length is public. Use this type whenever the secret length itself must not
/// vary across executions.
pub struct SecretBytes<const N: usize> {
    bytes: [u8; N],
}

impl<const N: usize> SecretBytes<N> {
    /// Takes ownership of a nonempty, bounded fixed-size secret.
    pub fn try_new(bytes: [u8; N]) -> Result<Self, SecretError> {
        validate_length(N)?;
        Ok(Self { bytes })
    }

    /// Returns the public fixed length.
    #[must_use]
    pub const fn len(&self) -> usize {
        N
    }

    /// Returns whether this fixed-size container is empty.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        N == 0
    }

    /// Compares all bytes using `subtle` and returns a `Choice` rather than `bool`.
    #[must_use]
    pub fn ct_eq(&self, other: &Self, _execution: &HardenedExecution) -> Choice {
        self.bytes.as_slice().ct_eq(other.bytes.as_slice())
    }

    /// Selects one complete value without secret-dependent source-level branching.
    #[must_use]
    pub fn ct_select(
        left: &Self,
        right: &Self,
        choice: Choice,
        _execution: &HardenedExecution,
    ) -> Self {
        let mut output = [0_u8; N];
        for (index, item) in output.iter_mut().enumerate() {
            *item = u8::conditional_select(&left.bytes[index], &right.bytes[index], choice);
        }
        Self { bytes: output }
    }

    /// Conditionally assigns all bytes without secret-dependent source-level branching.
    pub fn ct_assign(
        &mut self,
        other: &Self,
        choice: Choice,
        _execution: &HardenedExecution,
    ) {
        for (current, replacement) in self.bytes.iter_mut().zip(other.bytes.iter()) {
            current.conditional_assign(replacement, choice);
        }
    }

    /// Replaces the secret after securely erasing the previous owned value.
    pub fn replace(&mut self, replacement: [u8; N]) {
        self.bytes.zeroize();
        self.bytes = replacement;
    }

    /// Explicitly erases the owned bytes while retaining the zero-filled container.
    pub fn clear(&mut self) {
        self.bytes.zeroize();
    }

    /// Exposes the bytes only under an explicit permit and hardened-execution token.
    pub fn with_exposed<H: CommitmentHasher, T>(
        &self,
        secret_id: Hash32,
        execution: &HardenedExecution,
        permit: ExposurePermit,
        operation: impl FnOnce(&[u8]) -> T,
    ) -> Result<Exposed<T>, SecretError> {
        expose::<H, T>(
            self.bytes.as_slice(),
            secret_id,
            execution,
            permit,
            operation,
        )
    }
}

impl<const N: usize> Drop for SecretBytes<N> {
    fn drop(&mut self) {
        self.bytes.zeroize();
    }
}

impl<const N: usize> ZeroizeOnDrop for SecretBytes<N> {}

/// Dynamically sized owned secret bytes with public length.
///
/// Length comparison is intentionally public. Use [`SecretBytes`] when lengths
/// must be indistinguishable.
pub struct SecretBox {
    bytes: Box<[u8]>,
}

impl SecretBox {
    /// Takes ownership of a nonempty bounded secret buffer.
    pub fn try_new(bytes: Vec<u8>) -> Result<Self, SecretError> {
        validate_length(bytes.len())?;
        Ok(Self {
            bytes: bytes.into_boxed_slice(),
        })
    }

    /// Returns the public length.
    #[must_use]
    pub fn len(&self) -> usize {
        self.bytes.len()
    }

    /// Returns whether the buffer is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.bytes.is_empty()
    }

    /// Compares bytes in constant-time only after the public lengths match.
    #[must_use]
    pub fn ct_eq(&self, other: &Self, _execution: &HardenedExecution) -> Choice {
        if self.bytes.len() != other.bytes.len() {
            Choice::from(0_u8)
        } else {
            self.bytes.as_ref().ct_eq(other.bytes.as_ref())
        }
    }

    /// Conditionally assigns an equal-length value.
    pub fn ct_assign(
        &mut self,
        other: &Self,
        choice: Choice,
        _execution: &HardenedExecution,
    ) -> Result<(), SecretError> {
        if self.bytes.len() != other.bytes.len() {
            return Err(SecretError::LengthMismatch {
                left: self.bytes.len(),
                right: other.bytes.len(),
            });
        }
        for (current, replacement) in self.bytes.iter_mut().zip(other.bytes.iter()) {
            current.conditional_assign(replacement, choice);
        }
        Ok(())
    }

    /// Replaces the buffer after securely erasing the previous allocation.
    pub fn replace(&mut self, replacement: Vec<u8>) -> Result<(), SecretError> {
        validate_length(replacement.len())?;
        self.bytes.as_mut().zeroize();
        self.bytes = replacement.into_boxed_slice();
        Ok(())
    }

    /// Explicitly erases the allocation while retaining its public length.
    pub fn clear(&mut self) {
        self.bytes.as_mut().zeroize();
    }

    /// Exposes the bytes only under an explicit permit and hardened-execution token.
    pub fn with_exposed<H: CommitmentHasher, T>(
        &self,
        secret_id: Hash32,
        execution: &HardenedExecution,
        permit: ExposurePermit,
        operation: impl FnOnce(&[u8]) -> T,
    ) -> Result<Exposed<T>, SecretError> {
        expose::<H, T>(
            self.bytes.as_ref(),
            secret_id,
            execution,
            permit,
            operation,
        )
    }
}

impl Drop for SecretBox {
    fn drop(&mut self) {
        self.bytes.as_mut().zeroize();
    }
}

impl ZeroizeOnDrop for SecretBox {}

fn expose<H: CommitmentHasher, T>(
    bytes: &[u8],
    secret_id: Hash32,
    _execution: &HardenedExecution,
    permit: ExposurePermit,
    operation: impl FnOnce(&[u8]) -> T,
) -> Result<Exposed<T>, SecretError> {
    if secret_id == Hash32::ZERO {
        return Err(SecretError::ZeroHash);
    }
    permit.authorize(bytes.len())?;
    let bytes_exposed = u32::try_from(bytes.len()).map_err(|_| SecretError::InvalidLength)?;
    let event = ExposureEvent {
        secret_id,
        permit_hash: permit.commitment::<H>()?,
        bytes_exposed,
    };
    let value = operation(bytes);
    Ok(Exposed { value, event })
}

fn validate_length(length: usize) -> Result<(), SecretError> {
    if length == 0 || length > MAX_SECRET_BYTES {
        Err(SecretError::InvalidLength)
    } else {
        Ok(())
    }
}

fn hash_canonical<H: CommitmentHasher>(
    domain_name: &'static str,
    value: &impl CanonicalEncode,
) -> Result<Hash32, SecretError> {
    let bytes = value.canonical_bytes().map_err(SecretError::Encode)?;
    let domain = Domain::new(domain_name, SECRET_AUDIT_FORMAT_VERSION)
        .map_err(SecretError::Encode)?;
    commitment::<H>(domain, &bytes).map_err(SecretError::Encode)
}

/// Secret-container construction or use failure.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SecretError {
    /// Constant-time/exposure operation was requested in a debug-assertion build.
    DebugAssertionsEnabled,
    /// A secret length was zero, unrepresentable, or exceeded the hard bound.
    InvalidLength,
    /// A required public commitment was zero.
    ZeroHash,
    /// A permit authorized fewer bytes than the requested exposure.
    ExposureTooLarge {
        /// Requested bytes.
        attempted: u32,
        /// Authorized bytes.
        maximum: u32,
    },
    /// Dynamically sized constant-time assignment used different public lengths.
    LengthMismatch {
        /// Left length.
        left: usize,
        /// Right length.
        right: usize,
    },
    /// Canonical permit or audit-event encoding failed.
    Encode(EncodeError),
}

impl From<EncodeError> for SecretError {
    fn from(error: EncodeError) -> Self {
        Self::Encode(error)
    }
}

impl fmt::Display for SecretError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DebugAssertionsEnabled => {
                formatter.write_str("hardened secret operation requires debug assertions disabled")
            }
            Self::InvalidLength => formatter.write_str("secret length is invalid"),
            Self::ZeroHash => formatter.write_str("secret audit commitment is zero"),
            Self::ExposureTooLarge { attempted, maximum } => write!(
                formatter,
                "secret exposure of {attempted} bytes exceeds permit maximum {maximum}"
            ),
            Self::LengthMismatch { left, right } => {
                write!(formatter, "secret lengths differ: {left} and {right}")
            }
            Self::Encode(error) => write!(formatter, "secret audit encoding failed: {error}"),
        }
    }
}

#[cfg(feature = "std")]
impl std::error::Error for SecretError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone, Copy, Debug)]
    struct TestHasher;

    impl CommitmentHasher for TestHasher {
        const ALGORITHM_ID: &'static str = "test-only/1";

        fn hash(bytes: &[u8]) -> Hash32 {
            let mut output = [0_u8; 32];
            for (index, byte) in bytes.iter().enumerate() {
                let slot = index % output.len();
                output[slot] = output[slot]
                    .wrapping_add(*byte)
                    .rotate_left((index % 8) as u32);
            }
            Hash32::new(output)
        }
    }

    fn hash(byte: u8) -> Hash32 {
        Hash32::new([byte; 32])
    }

    fn permit(maximum_bytes: u32) -> ExposurePermit {
        ExposurePermit::try_new(hash(1), hash(2), maximum_bytes)
            .unwrap_or_else(|error| panic!("permit: {error}"))
    }

    #[test]
    fn hardened_execution_detects_debug_assertions() {
        let result = HardenedExecution::require_release();
        if cfg!(debug_assertions) {
            assert!(matches!(result, Err(SecretError::DebugAssertionsEnabled)));
        } else {
            assert!(result.is_ok());
        }
    }

    #[test]
    fn fixed_secret_equality_and_selection_use_choice() {
        let execution = HardenedExecution::for_test();
        let left = SecretBytes::try_new([7_u8; 32])
            .unwrap_or_else(|error| panic!("left: {error}"));
        let same = SecretBytes::try_new([7_u8; 32])
            .unwrap_or_else(|error| panic!("same: {error}"));
        let right = SecretBytes::try_new([9_u8; 32])
            .unwrap_or_else(|error| panic!("right: {error}"));
        assert_eq!(left.ct_eq(&same, &execution).unwrap_u8(), 1);
        assert_eq!(left.ct_eq(&right, &execution).unwrap_u8(), 0);

        let selected = SecretBytes::ct_select(&left, &right, Choice::from(1_u8), &execution);
        assert_eq!(selected.ct_eq(&right, &execution).unwrap_u8(), 1);
    }

    #[test]
    fn explicit_exposure_emits_content_bound_event() {
        let execution = HardenedExecution::for_test();
        let secret = SecretBytes::try_new([5_u8; 16])
            .unwrap_or_else(|error| panic!("secret: {error}"));
        let exposed = secret
            .with_exposed::<TestHasher, _>(hash(3), &execution, permit(16), |bytes| {
                bytes.iter().fold(0_u32, |sum, byte| sum + u32::from(*byte))
            })
            .unwrap_or_else(|error| panic!("exposure: {error}"));
        let (sum, event) = exposed.into_parts();
        assert_eq!(sum, 80);
        assert_eq!(event.secret_id(), hash(3));
        assert_eq!(event.bytes_exposed(), 16);
        assert_ne!(event.permit_hash(), Hash32::ZERO);
    }

    #[test]
    fn exposure_limit_fails_closed() {
        let execution = HardenedExecution::for_test();
        let secret = SecretBytes::try_new([5_u8; 16])
            .unwrap_or_else(|error| panic!("secret: {error}"));
        let result = secret.with_exposed::<TestHasher, _>(
            hash(3),
            &execution,
            permit(15),
            |_| (),
        );
        assert!(matches!(
            result,
            Err(SecretError::ExposureTooLarge {
                attempted: 16,
                maximum: 15
            })
        ));
    }

    #[test]
    fn dynamic_length_is_explicit_and_assignment_checks_it() {
        let execution = HardenedExecution::for_test();
        let mut left = SecretBox::try_new(vec![1, 2, 3])
            .unwrap_or_else(|error| panic!("left: {error}"));
        let same = SecretBox::try_new(vec![1, 2, 3])
            .unwrap_or_else(|error| panic!("same: {error}"));
        let different = SecretBox::try_new(vec![1, 2])
            .unwrap_or_else(|error| panic!("different: {error}"));
        assert_eq!(left.ct_eq(&same, &execution).unwrap_u8(), 1);
        assert_eq!(left.ct_eq(&different, &execution).unwrap_u8(), 0);
        assert!(matches!(
            left.ct_assign(&different, Choice::from(1_u8), &execution),
            Err(SecretError::LengthMismatch { left: 3, right: 2 })
        ));
    }

    #[test]
    fn clear_and_replace_erase_owned_storage_before_reuse() {
        let execution = HardenedExecution::for_test();
        let mut secret = SecretBytes::try_new([9_u8; 8])
            .unwrap_or_else(|error| panic!("secret: {error}"));
        secret.clear();
        let cleared = secret
            .with_exposed::<TestHasher, _>(hash(3), &execution, permit(8), |bytes| bytes.to_vec())
            .unwrap_or_else(|error| panic!("clear exposure: {error}"));
        assert_eq!(cleared.into_parts().0, vec![0_u8; 8]);

        secret.replace([4_u8; 8]);
        let replaced = secret
            .with_exposed::<TestHasher, _>(hash(3), &execution, permit(8), |bytes| bytes.to_vec())
            .unwrap_or_else(|error| panic!("replace exposure: {error}"));
        assert_eq!(replaced.into_parts().0, vec![4_u8; 8]);
    }
}
