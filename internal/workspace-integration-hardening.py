from pathlib import Path


def replace_exact(path: Path, old: str, new: str, label: str) -> None:
    text = path.read_text(encoding="utf-8")
    if old not in text:
        if new in text:
            return
        raise SystemExit(f"missing hardening site: {label}")
    path.write_text(text.replace(old, new, 1), encoding="utf-8")


validate = Path("crates/zeno-fcis-schema/src/validate.rs")
replace_exact(
    validate,
    "    EnumVariantDef, Schema, SumVariantDef, TypeDef, TypeId, TypeKind, ValueValidationError,\n",
    "    EnumVariantDef, Schema, SumVariantDef, TypeId, TypeKind, ValueValidationError,\n",
    "schema unused TypeDef import",
)
replace_exact(
    validate,
    "            validate_length(text.as_bytes().len(), *min_len, *max_len)\n",
    "            validate_length(text.len(), *min_len, *max_len)\n",
    "schema text length",
)
replace_exact(
    validate,
    "    use crate::{FieldDef, SchemaLimits, TypeDef};\n",
    "    use crate::{FieldDef, FieldId, SchemaError, SchemaLimits, TypeDef};\n",
    "schema test imports",
)

render = Path("crates/zeno-fcis-codegen/src/render.rs")
replace_exact(
    render,
    "use zeno_fcis_codec::{CanonicalEncode, CommitmentHasher, Domain, Hash32, commitment};\n",
    "use zeno_fcis_codec::{CanonicalEncode, Domain, Hash32, commitment};\n",
    "codegen unused CommitmentHasher import",
)
