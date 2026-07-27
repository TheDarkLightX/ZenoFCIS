from pathlib import Path


def replace_once(text: str, old: str, new: str, label: str) -> str:
    count = text.count(old)
    if count != 1:
        raise RuntimeError(f"{label}: expected one occurrence, found {count}")
    return text.replace(old, new, 1)


path = Path("crates/zeno-fcis-codegen/src/solana_anchor.rs")
text = path.read_text(encoding="utf-8")
text = replace_once(
    text,
    '("unsafe", SolanaCoreSafetyFindingKind::UnsafeCode),',
    '("unsafe {", SolanaCoreSafetyFindingKind::UnsafeCode),',
    "core unsafe scanner",
)
text = replace_once(
    text,
    '("unsafe", SolanaShellSafetyFindingKind::UnsafeCode),',
    '("unsafe {", SolanaShellSafetyFindingKind::UnsafeCode),',
    "shell unsafe scanner",
)

block_start = text.index(
    '\n    let shell_source = files\n'
    '        .iter()\n'
    '        .find(|file| file.path().ends_with("programs/"))'
)
block_end = text.index(
    '\n    let shell_source = files\n'
    '        .iter()\n'
    '        .find(|file| file.path().contains("programs/")',
    block_start,
)
text = (
    text[:block_start]
    + '\n    if !inspect_solana_core_source(core_source).is_clean() {\n'
    + '        return Err(OnchainModelError::InvalidBinding);\n'
    + '    }'
    + text[block_end:]
)

text = replace_once(
    text,
    "ProjectCore, State, MACHINE_HASH}};",
    "ProjectCore, State, MACHINE_HASH, MACHINE_VERSION}};",
    "generated core import",
)
text = replace_once(
    text,
    "state.version = 1;",
    "state.version = MACHINE_VERSION;",
    "machine version",
)
text = replace_once(
    text,
    "-> EffectPlan {{\n    EffectPlan {{",
    "-> EffectPlan {{\n    let _ = (before_state, command, context);\n    EffectPlan {{",
    "effect builder",
)

encode_start = text.index("fn encode_bytes32(")
encode_end = text.index("fn rust_type", encode_start)
encode_replacement = '''fn encode_bytes32(scalar: OnchainScalar, name: &str) -> String {
    match scalar {
        OnchainScalar::Bool => format!(
            "{{ let mut output = [0_u8; 32]; output[31] = u8::from({name}); output }}"
        ),
        OnchainScalar::Bytes32 => name.to_owned(),
        OnchainScalar::U8 => encode_unsigned_bytes32(name, 1),
        OnchainScalar::U16 => encode_unsigned_bytes32(name, 2),
        OnchainScalar::U32 => encode_unsigned_bytes32(name, 4),
        OnchainScalar::U64 => encode_unsigned_bytes32(name, 8),
        OnchainScalar::U128 => encode_unsigned_bytes32(name, 16),
        OnchainScalar::I8 => encode_signed_bytes32(name, 1),
        OnchainScalar::I16 => encode_signed_bytes32(name, 2),
        OnchainScalar::I32 => encode_signed_bytes32(name, 4),
        OnchainScalar::I64 => encode_signed_bytes32(name, 8),
        OnchainScalar::I128 => encode_signed_bytes32(name, 16),
    }
}

fn encode_unsigned_bytes32(name: &str, width: usize) -> String {
    let start = 32_usize.saturating_sub(width);
    format!(
        "{{ let mut output = [0_u8; 32]; let bytes = {name}.to_be_bytes(); output[{start}..].copy_from_slice(&bytes); output }}"
    )
}

fn encode_signed_bytes32(name: &str, width: usize) -> String {
    let start = 32_usize.saturating_sub(width);
    format!(
        "{{ let mut output = if {name} < 0 {{ [0xff_u8; 32] }} else {{ [0_u8; 32] }}; let bytes = {name}.to_be_bytes(); output[{start}..].copy_from_slice(&bytes); output }}"
    )
}

'''
text = text[:encode_start] + encode_replacement + text[encode_end:]
text = text.replace(".map(u8::to_string)", ".map(|byte| byte.to_string())")
path.write_text(text, encoding="utf-8")

path = Path("crates/zeno-fcis-codegen/src/lib.rs")
text = path.read_text(encoding="utf-8")
if "mod solana_anchor;" not in text:
    text = replace_once(
        text,
        "mod root_envelope;\n",
        "mod root_envelope;\nmod solana_anchor;\n",
        "Solana module export",
    )
if "pub use solana_anchor::{" not in text:
    marker = "pub use render::{GENERATOR_ID, generate};\n"
    block = '''pub use solana_anchor::{
    ANCHOR_VERSION, MAX_SOLANA_GENERATED_FILE_BYTES, SOLANA_ANCHOR_GENERATOR_ID,
    SOLANA_TOOLCHAIN_VERSION, SolanaAnchorSpec, SolanaCoreSafetyFindingKind,
    SolanaFungibleBinding, SolanaSafetyFinding, SolanaSafetyReport,
    SolanaShellSafetyFindingKind, generate_solana_anchor, inspect_solana_core_source,
    inspect_solana_shell_source,
};
'''
    text = replace_once(text, marker, marker + block, "Solana public exports")
path.write_text(text, encoding="utf-8")

path = Path("crates/zeno-fcis/src/lib.rs")
text = path.read_text(encoding="utf-8")
if "SOLANA_ANCHOR_GENERATOR_ID" not in text:
    text = replace_once(
        text,
        "    CodegenError, GENERATOR_ID, GeneratedBundle, GeneratedFile, GeneratedOnchainBundle,\n",
        "    ANCHOR_VERSION, CodegenError, GENERATOR_ID, GeneratedBundle, GeneratedFile,\n"
        "    GeneratedOnchainBundle,\n",
        "umbrella header exports",
    )
    text = replace_once(
        text,
        "    GeneratedOnchainFile, GeneratedSolidity, GenerationSpec, MAX_ONCHAIN_CAPABILITIES,\n",
        "    GeneratedOnchainFile, GeneratedSolidity, GenerationSpec, MAX_ONCHAIN_CAPABILITIES,\n"
        "    MAX_SOLANA_GENERATED_FILE_BYTES,\n",
        "umbrella limit exports",
    )
    text = replace_once(
        text,
        "    OnchainMachineSpec, OnchainModelError, OnchainReason, OnchainScalar, RecipientPolicy,\n",
        "    OnchainMachineSpec, OnchainModelError, OnchainReason, OnchainScalar, RecipientPolicy,\n"
        "    SOLANA_ANCHOR_GENERATOR_ID, SOLANA_TOOLCHAIN_VERSION, SolanaAnchorSpec,\n"
        "    SolanaCoreSafetyFindingKind, SolanaFungibleBinding, SolanaSafetyFinding,\n"
        "    SolanaSafetyReport, SolanaShellSafetyFindingKind,\n",
        "umbrella Solana types",
    )
    text = replace_once(
        text,
        "    SoliditySafetyFindingKind, SoliditySafetyReport, SolidityScalar, generate,\n"
        "    generate_advanced_solidity, generate_solidity, inspect_solidity_source,\n",
        "    SoliditySafetyFindingKind, SoliditySafetyReport, SolidityScalar, generate,\n"
        "    generate_advanced_solidity, generate_solana_anchor, generate_solidity,\n"
        "    inspect_solana_core_source, inspect_solana_shell_source, inspect_solidity_source,\n",
        "umbrella Solana functions",
    )
path.write_text(text, encoding="utf-8")

path = Path("README.md")
text = path.read_text(encoding="utf-8").replace(
    "bounded Solidity and chain-neutral on-chain FCIS generators",
    "bounded Solidity, Solana/Anchor, and chain-neutral on-chain FCIS generators",
)
path.write_text(text, encoding="utf-8")
