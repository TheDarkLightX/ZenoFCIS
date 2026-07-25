from pathlib import Path

path = Path("crates/zeno-fcis-receipt/src/lib.rs")
text = path.read_text(encoding="utf-8")
old = """fn hash_component<H: CommitmentHasher, T: CanonicalEncode>(
    domain_name: &str,
    value: &T,
) -> Result<Hash32, SealError> {
"""
new = """fn hash_component<H: CommitmentHasher>(
    domain_name: &str,
    value: &impl CanonicalEncode,
) -> Result<Hash32, SealError> {
"""
if old not in text:
    raise SystemExit("expected candidate hash helper signature was not found")
path.write_text(text.replace(old, new, 1), encoding="utf-8")
