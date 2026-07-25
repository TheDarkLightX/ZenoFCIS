from pathlib import Path

receipt_path = Path("crates/zeno-fcis-receipt/src/lib.rs")
receipt = receipt_path.read_text(encoding="utf-8")
old_signature = """fn hash_component<H: CommitmentHasher, T: CanonicalEncode>(
    domain_name: &str,
    value: &T,
) -> Result<Hash32, SealError> {
"""
new_signature = """fn hash_component<H: CommitmentHasher>(
    domain_name: &str,
    value: &impl CanonicalEncode,
) -> Result<Hash32, SealError> {
"""
if old_signature not in receipt:
    raise SystemExit("expected candidate hash helper signature was not found")
receipt_path.write_text(receipt.replace(old_signature, new_signature, 1), encoding="utf-8")

shell_manifest_path = Path("crates/zeno-fcis-shell/Cargo.toml")
shell_manifest = shell_manifest_path.read_text(encoding="utf-8")
old_manifest_tail = """zeno-fcis-value = { path = "../zeno-fcis-value", default-features = false }

[lints]
workspace = true
"""
new_manifest_tail = """zeno-fcis-value = { path = "../zeno-fcis-value", default-features = false }

[dev-dependencies]
zeno-fcis-core = { path = "../zeno-fcis-core" }

[lints]
workspace = true
"""
if old_manifest_tail not in shell_manifest:
    raise SystemExit("expected shell manifest dependency boundary was not found")
shell_manifest_path.write_text(
    shell_manifest.replace(old_manifest_tail, new_manifest_tail, 1),
    encoding="utf-8",
)
