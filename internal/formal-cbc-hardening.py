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
