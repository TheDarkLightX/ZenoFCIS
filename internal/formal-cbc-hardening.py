from pathlib import Path


def replace_exact(path: Path, old: str, new: str, label: str) -> None:
    text = path.read_text(encoding="utf-8")
    if old not in text:
        if new in text:
            return
        raise SystemExit(f"missing formal-CBC hardening site: {label}")
    path.write_text(text.replace(old, new, 1), encoding="utf-8")


assurance = Path("tools/check_assurance.py")
replace_exact(
    assurance,
    '    "zeno-fcis-transition",\n    "zeno-fcis-security",\n',
    '    "zeno-fcis-transition",\n    "zeno-fcis-cbc",\n    "zeno-fcis-security",\n',
    "CBC semantic boundary",
)
replace_exact(
    assurance,
    '    "zeno-fcis-transition": 3,\n    "zeno-fcis-security": 2,\n',
    '    "zeno-fcis-transition": 3,\n    "zeno-fcis-cbc": 3,\n    "zeno-fcis-security": 2,\n',
    "CBC dependency ring",
)

cbc = Path("crates/zeno-fcis-cbc/src/lib.rs")
replace_exact(
    cbc,
    "use zeno_fcis_project::{RegistryKind, SemanticId, StableName};\n",
    "use zeno_fcis_project::{RegistryKind, SemanticId, StableName};\nuse zeno_fcis_receipt::SealError;\n",
    "SealError import",
)
replace_exact(
    cbc,
    """    /// Patch application failed.
    Patch(PatchError),
    /// Canonical encoding or commitment construction failed.
""",
    """    /// Patch application failed.
    Patch(PatchError),
    /// Sealed bundle validation or reconstruction failed.
    Seal(SealError),
    /// Canonical encoding or commitment construction failed.
""",
    "SealError variant",
)
replace_exact(
    cbc,
    """impl From<EncodeError> for CbcError {
""",
    """impl From<SealError> for CbcError {
    fn from(error: SealError) -> Self {
        Self::Seal(error)
    }
}

impl From<EncodeError> for CbcError {
""",
    "SealError conversion",
)
replace_exact(
    cbc,
    """            Self::Patch(error) => write!(formatter, "CBC patch failed: {error}"),
            Self::Encode(error) => write!(formatter, "CBC encoding failed: {error}"),
""",
    """            Self::Patch(error) => write!(formatter, "CBC patch failed: {error}"),
            Self::Seal(error) => write!(formatter, "CBC sealed bundle failed: {error}"),
            Self::Encode(error) => write!(formatter, "CBC encoding failed: {error}"),
""",
    "SealError display",
)
