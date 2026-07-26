from pathlib import Path


def replace_exact(path: Path, old: str, new: str, label: str) -> None:
    text = path.read_text(encoding="utf-8")
    if old not in text:
        if new in text:
            return
        raise SystemExit(f"missing hardening site: {label}")
    path.write_text(text.replace(old, new, 1), encoding="utf-8")


synthesis = Path("crates/zeno-fcis-synthesis/src/lib.rs")
replace_exact(
    synthesis,
    '''#![forbid(unsafe_code)]

use core::fmt;
''',
    '''#![cfg_attr(not(feature = "std"), no_std)]
#![forbid(unsafe_code)]

extern crate alloc;

use alloc::boxed::Box;
use alloc::vec;
use alloc::vec::Vec;
use core::fmt;
''',
    "synthesis no_std prelude",
)
replace_exact(
    synthesis,
    '''impl std::error::Error for SynthesisError {}
''',
    '''#[cfg(feature = "std")]
impl std::error::Error for SynthesisError {}
''',
    "synthesis std error gate",
)

backend = Path("crates/zeno-fcis-backend/src/lib.rs")
replace_exact(
    backend,
    '''use alloc::boxed::Box;
use alloc::vec::Vec;
''',
    '''use alloc::boxed::Box;
use alloc::vec;
use alloc::vec::Vec;
''',
    "backend vec macro",
)
replace_exact(
    backend,
    '''impl BackendRequestTemplate {
    /// Creates a synthesis request template.
    pub const fn try_new(
''',
    '''impl BackendRequestTemplate {
    /// Creates a synthesis request template.
    pub fn try_new(
''',
    "backend template non-const constructor",
)
replace_exact(
    backend,
    '''impl fmt::Display for BackendError {
''',
    '''impl From<EncodeError> for BackendError {
    fn from(error: EncodeError) -> Self {
        Self::Encode(error)
    }
}

impl fmt::Display for BackendError {
''',
    "backend encode error conversion",
)
replace_exact(
    backend,
    '''        assert_eq!(
            BackendLimits::try_new(0, 1, 1, 1),
            Err(BackendError::ZeroLimit)
        );
''',
    '''        assert!(matches!(
            BackendLimits::try_new(0, 1, 1, 1),
            Err(BackendError::ZeroLimit)
        ));
''',
    "backend zero limit test",
)
replace_exact(
    backend,
    '''        assert_eq!(
            BackendUsage::try_new(limits, 10_001, 0, 0, 0),
            Err(BackendError::UsageExceedsLimit)
        );
''',
    '''        assert!(matches!(
            BackendUsage::try_new(limits, 10_001, 0, 0, 0),
            Err(BackendError::UsageExceedsLimit)
        ));
''',
    "backend usage test",
)

assurance = Path("tools/check_assurance.py")
replace_exact(
    assurance,
    '''    "zeno-fcis-synthesis",
)
''',
    '''    "zeno-fcis-synthesis",
    "zeno-fcis-backend",
)
''',
    "backend semantic boundary",
)
replace_exact(
    assurance,
    '''    "zeno-fcis-synthesis": 2,
    "zeno-fcis": 3,
''',
    '''    "zeno-fcis-synthesis": 2,
    "zeno-fcis-backend": 3,
    "zeno-fcis": 3,
''',
    "backend dependency ring",
)
