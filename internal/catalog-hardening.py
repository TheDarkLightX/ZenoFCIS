from pathlib import Path


def replace_exact(path: Path, old: str, new: str, label: str) -> None:
    text = path.read_text(encoding="utf-8")
    if old not in text:
        if new in text:
            return
        raise SystemExit(f"missing catalog hardening site: {label}")
    path.write_text(text.replace(old, new, 1), encoding="utf-8")


catalog = Path("crates/zeno-fcis-catalog/src/lib.rs")
replace_exact(
    catalog,
    "use alloc::string::{String, ToString};\n",
    "#[cfg(test)]\nuse alloc::string::String;\n",
    "test-only String import",
)
replace_exact(
    catalog,
    '''impl fmt::Display for CatalogError {
''',
    '''impl From<EncodeError> for CatalogError {
    fn from(error: EncodeError) -> Self {
        Self::Encode(error)
    }
}

impl fmt::Display for CatalogError {
''',
    "encoding error conversion",
)
