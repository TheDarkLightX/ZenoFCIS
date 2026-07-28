from pathlib import Path

path = Path("crates/zeno-fcis-cbc/src/lib.rs")
text = path.read_text(encoding="utf-8")
text = text.replace(
    "use zeno_fcis_receipt::SealError;\nuse zeno_fcis_receipt::SealError;\n",
    "use zeno_fcis_receipt::SealError;\n",
)
conversion = """impl From<SealError> for CbcError {
    fn from(error: SealError) -> Self {
        Self::Seal(error)
    }
}
"""
text = text.replace(f"{conversion}\n{conversion}", conversion)
path.write_text(text, encoding="utf-8")
