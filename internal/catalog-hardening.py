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
    "use alloc::string::String;\n",
    "unused ToString import",
)
