//! Native scaffold retention is inert, source-bound and compatible with legacy ZAL.
#![forbid(unsafe_code)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

use serde_json::Value;
use zeno_fcis_codec::{CommitmentHasher, RustCryptoSha256};

static NEXT: AtomicU64 = AtomicU64::new(0);
const BUNDLE: [&str; 5] = [
    "zal-original.zal",
    "zal-app-binding.json",
    "zal-source-map.json",
    "tests/zal-authority-examples.txt",
    "tests/zal_correspondence.rs",
];

struct TempRoot(PathBuf);
impl TempRoot {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "zeno-zal-provenance-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap_or_else(|e| panic!("create owned temp: {e}"));
        Self(path)
    }
}
impl Drop for TempRoot {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn read(path: impl AsRef<Path>) -> Vec<u8> {
    fs::read(path).unwrap_or_else(|e| panic!("read fixture: {e}"))
}

fn copy_directory(from: &Path, to: &Path) {
    fs::create_dir_all(to).unwrap_or_else(|e| panic!("create fixture copy: {e}"));
    for entry in fs::read_dir(from).unwrap_or_else(|e| panic!("read fixture: {e}")) {
        let entry = entry.unwrap_or_else(|e| panic!("fixture entry: {e}"));
        let target = to.join(entry.file_name());
        if entry.path().is_dir() {
            copy_directory(&entry.path(), &target);
        } else {
            fs::copy(entry.path(), target).unwrap_or_else(|e| panic!("copy fixture: {e}"));
        }
    }
}

fn fixture(label: &str, root: &TempRoot) -> PathBuf {
    let source = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/zal-app-provenance")
        .join(label);
    let contract = root.0.join("contract");
    copy_directory(&source, &contract);
    contract
}

fn new_app(contract: &Path, app: &Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_zeno-fcis"))
        .arg("new")
        .arg(app)
        .arg("--contract")
        .arg(contract)
        .arg("--source")
        .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("../.."))
        .output()
        .unwrap_or_else(|e| panic!("native CLI: {e}"))
}

fn refused(contract: &Path, app: &Path, expected: &str) {
    let result = new_app(contract, app);
    assert!(
        !result.status.success(),
        "unexpected native success: {result:?}"
    );
    assert!(
        String::from_utf8_lossy(&result.stderr).contains(expected),
        "wrong native refusal: {result:?}"
    );
    assert!(!app.exists(), "native refusal left output files");
}

fn map_change(contract: &Path, key: &str, value: Value) {
    let path = contract.join(BUNDLE[2]);
    let mut mapping: Value =
        serde_json::from_slice(&read(&path)).unwrap_or_else(|e| panic!("fixture mapping: {e}"));
    mapping[key] = value;
    fs::write(
        path,
        serde_json::to_vec(&mapping).unwrap_or_else(|e| panic!("JSON: {e}")),
    )
    .unwrap_or_else(|e| panic!("write changed mapping: {e}"));
}

fn hash(bytes: &[u8]) -> String {
    RustCryptoSha256::hash(bytes)
        .as_bytes()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

#[test]
fn declared_zal_app_provenance_is_retained_as_exact_private_data() {
    let root = TempRoot::new();
    let contract = fixture("app", &root);
    let app = root.0.join("app");
    let result = new_app(&contract, &app);
    assert!(result.status.success(), "native generation: {result:?}");
    for name in BUNDLE {
        assert_eq!(
            read(app.join("provenance/zal").join(name)),
            read(contract.join(name))
        );
        assert!(
            !app.join(name).exists(),
            "retention added an implicit root/test file"
        );
        assert!(
            !app.join("www").join(name).exists(),
            "retention published provenance"
        );
    }
    let source = read(app.join("provenance/zal/zal-original.zal"));
    let binding = read(app.join("provenance/zal/zal-app-binding.json"));
    assert!(source.windows(2).any(|bytes| bytes == b"\r\n"));
    assert!(binding.windows(2).any(|bytes| bytes == b"\r\n"));
    assert!(String::from_utf8_lossy(&binding).contains("Réparation desk — 雪"));
    for name in ["project.zeno", "v2/policy.json", "ui.json"] {
        assert_eq!(read(app.join(name)), read(contract.join(name)));
    }
    let checked = Command::new(env!("CARGO_BIN_EXE_zeno-fcis"))
        .args(["generate", "contract"])
        .arg(&app)
        .arg("--check")
        .output()
        .unwrap_or_else(|e| panic!("native generation check: {e}"));
    assert!(
        checked.status.success(),
        "current generated app: {checked:?}"
    );
    // Exercise the exact enumerator used when app_factory writes its final
    // manifest; no UI/native application build is needed for this boundary.
    let manifest = Command::new("python3")
        .args([
            "-B",
            "-c",
            r#"
import importlib.util,json,pathlib,sys
module=importlib.util.spec_from_file_location('app_factory',sys.argv[1])
factory=importlib.util.module_from_spec(module);module.loader.exec_module(factory)
print(json.dumps(factory.hashes(pathlib.Path(sys.argv[2]))))
"#,
        ])
        .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tools/app_factory.py"))
        .arg(&app)
        .output()
        .unwrap_or_else(|e| panic!("manifest enumerator: {e}"));
    assert!(
        manifest.status.success(),
        "manifest enumeration: {manifest:?}"
    );
    let files: Value =
        serde_json::from_slice(&manifest.stdout).unwrap_or_else(|e| panic!("manifest hashes: {e}"));
    for name in BUNDLE {
        assert!(
            files
                .get(format!("provenance/zal/{name}"))
                .and_then(Value::as_str)
                .is_some()
        );
    }
}

#[test]
fn legacy_finite_zal_exports_without_bindings_keep_native_generation() {
    for version in ["legacy-1", "legacy-2"] {
        let root = TempRoot::new();
        let contract = fixture(version, &root);
        let app = root.0.join("app");
        assert!(!contract.join(BUNDLE[1]).exists());
        let result = new_app(&contract, &app);
        assert!(result.status.success(), "{version} changed: {result:?}");
        assert!(!app.join("provenance").exists());
    }
}

#[test]
fn legacy_auxiliary_filenames_do_not_declare_an_app_bundle() {
    let root = TempRoot::new();
    let contract = fixture("legacy-2", &root);
    for name in [BUNDLE[3], BUNDLE[4]] {
        fs::remove_file(contract.join(name)).unwrap_or_else(|e| panic!("partial legacy: {e}"));
    }
    fs::write(contract.join(BUNDLE[2]), b"not an app source map")
        .unwrap_or_else(|e| panic!("legacy auxiliary: {e}"));
    let app = root.0.join("app");
    let result = new_app(&contract, &app);
    assert!(
        result.status.success(),
        "legacy auxiliaries triggered app validation: {result:?}"
    );
    assert!(!app.join("provenance").exists());
}

#[test]
fn app_bundle_accepts_exact_text_byte_boundaries() {
    let root = TempRoot::new();
    let contract = fixture("app", &root);
    for name in [BUNDLE[0], BUNDLE[1]] {
        let mut bytes = read(contract.join(name));
        bytes.resize(65_536, b' ');
        fs::write(contract.join(name), &bytes).unwrap_or_else(|e| panic!("pad fixture: {e}"));
        map_change(
            &contract,
            if name == BUNDLE[0] {
                "source_sha256"
            } else {
                "binding_sha256"
            },
            Value::String(hash(&bytes)),
        );
    }
    let mut mapping = read(contract.join(BUNDLE[2]));
    mapping.resize(65_536, b' ');
    fs::write(contract.join(BUNDLE[2]), mapping).unwrap_or_else(|e| panic!("pad map: {e}"));
    let app = root.0.join("app");
    let result = new_app(&contract, &app);
    assert!(result.status.success(), "exact byte boundaries: {result:?}");
    for name in BUNDLE {
        assert_eq!(
            read(app.join("provenance/zal").join(name)),
            read(contract.join(name))
        );
    }
}

#[test]
fn declared_app_bundle_requires_every_file_and_its_captured_ui() {
    for missing in BUNDLE.into_iter().chain(["ui.json"]) {
        let root = TempRoot::new();
        let contract = fixture("app", &root);
        fs::remove_file(contract.join(missing)).unwrap_or_else(|e| panic!("remove fixture: {e}"));
        refused(&contract, &root.0.join("app"), "app provenance");
    }
}

#[test]
fn declared_app_bundle_hash_mismatches_refuse_before_native_writes() {
    for changed in [
        BUNDLE[0],
        BUNDLE[1],
        BUNDLE[3],
        "project.zeno",
        "v2/policy.json",
        "ui.json",
    ] {
        let root = TempRoot::new();
        let contract = fixture("app", &root);
        let path = contract.join(changed);
        let mut bytes = read(&path);
        bytes.push(b' ');
        fs::write(path, bytes).unwrap_or_else(|e| panic!("tamper fixture: {e}"));
        refused(
            &contract,
            &root.0.join("app"),
            "app provenance hash mismatch",
        );
    }
}

#[test]
fn malformed_and_mismatched_app_declarations_refuse_before_native_writes() {
    for (key, value) in [
        ("revision", Value::String("0".repeat(64))),
        ("schema", Value::String("zal/factory-source-map/2".into())),
        ("profile", Value::String("unsupported".into())),
        ("source_sha256", Value::Null),
    ] {
        let root = TempRoot::new();
        let contract = fixture("app", &root);
        map_change(&contract, key, value);
        refused(&contract, &root.0.join("app"), "app provenance");
    }
    for bad in [
        b"{".as_slice(),
        b"\xff",
        br#"{"format":"x","format":"zal/app-binding/1"}"#,
    ] {
        let root = TempRoot::new();
        let contract = fixture("app", &root);
        fs::write(contract.join(BUNDLE[1]), bad).unwrap_or_else(|e| panic!("bad binding: {e}"));
        refused(&contract, &root.0.join("app"), "app provenance");
    }
}

#[test]
fn app_schema_selection_cannot_be_downgraded_by_duplicate_or_truncated_json() {
    for bytes in [
        br#"{"schema":"zal/factory-source-map/3","schema":"zal/factory-source-map/2"}"#.as_slice(),
        br#"{"schema":"zal/factory-source-map/3","unfinished":"#,
    ] {
        let root = TempRoot::new();
        let contract = fixture("app", &root);
        fs::remove_file(contract.join(BUNDLE[1])).unwrap_or_else(|e| panic!("remove binding: {e}"));
        fs::write(contract.join(BUNDLE[2]), bytes).unwrap_or_else(|e| panic!("bad map: {e}"));
        refused(&contract, &root.0.join("app"), "app provenance");
    }
}

#[test]
fn app_text_and_native_input_file_limits_refuse_without_output() {
    for (name, length, expected) in [
        (BUNDLE[0], 65_537, "bounded UTF-8"),
        (BUNDLE[1], 65_537, "bounded UTF-8"),
        (BUNDLE[2], 65_537, "bounded UTF-8"),
        (BUNDLE[3], 16 * 1024 * 1024 + 1, "larger than"),
    ] {
        let root = TempRoot::new();
        let contract = fixture("app", &root);
        fs::OpenOptions::new()
            .write(true)
            .open(contract.join(name))
            .and_then(|file| file.set_len(length))
            .unwrap_or_else(|e| panic!("extend fixture: {e}"));
        refused(&contract, &root.0.join("app"), expected);
    }
}

#[test]
fn every_retained_text_file_requires_valid_utf8() {
    for name in BUNDLE {
        let root = TempRoot::new();
        let contract = fixture("app", &root);
        fs::write(contract.join(name), b"\xff")
            .unwrap_or_else(|e| panic!("invalid text fixture: {e}"));
        refused(&contract, &root.0.join("app"), "bounded UTF-8");
    }
}

#[cfg(unix)]
#[test]
fn dangling_binding_is_not_treated_as_an_absent_declaration() {
    let root = TempRoot::new();
    let contract = fixture("legacy-2", &root);
    std::os::unix::fs::symlink(root.0.join("missing"), contract.join(BUNDLE[1]))
        .unwrap_or_else(|e| panic!("dangling binding fixture: {e}"));
    refused(&contract, &root.0.join("app"), "read zal-app-binding.json");
}

#[test]
fn ordinary_contract_generation_does_not_validate_app_auxiliaries() {
    let root = TempRoot::new();
    let contract = fixture("app", &root);
    fs::remove_file(contract.join(BUNDLE[1])).unwrap_or_else(|e| panic!("remove binding: {e}"));
    let output = Command::new(env!("CARGO_BIN_EXE_zeno-fcis"))
        .args(["generate", "contract"])
        .arg(&contract)
        .arg("--out")
        .arg(root.0.join("generated-contract"))
        .output()
        .unwrap_or_else(|e| panic!("ordinary generation: {e}"));
    assert!(
        output.status.success(),
        "app-only metadata changed core generation: {output:?}"
    );
}
