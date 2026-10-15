//! Native import/export round trips, separate from optional Tau/SMT runs.
#![forbid(unsafe_code)]

use std::fmt::Display;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

use serde_json::Value;

static SEQUENCE: AtomicU64 = AtomicU64::new(0);

fn fixture_result<T, E: Display>(result: Result<T, E>) -> T {
    result.unwrap_or_else(|error| panic!("test fixture operation failed: {error}"))
}

fn path_text(path: &Path) -> &str {
    path.to_str()
        .unwrap_or_else(|| panic!("test path is not UTF-8: {path:?}"))
}

#[cfg(target_os = "linux")]
fn array(value: &Value) -> &[Value] {
    value
        .as_array()
        .unwrap_or_else(|| panic!("expected a JSON array: {value}"))
}

#[cfg(target_os = "linux")]
fn text(value: &Value) -> &str {
    value
        .as_str()
        .unwrap_or_else(|| panic!("expected a JSON string: {value}"))
}

#[cfg(target_os = "linux")]
fn read_json(path: impl AsRef<Path>) -> Value {
    fixture_result(serde_json::from_slice(&fixture_result(fs::read(path))))
}

struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "zeno-fcis-tau-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        fixture_result(fs::create_dir(&path));
        Self(path)
    }
    fn path(&self) -> &Path {
        &self.0
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn zeno(arguments: &[&str]) -> Output {
    fixture_result(
        Command::new(env!("CARGO_BIN_EXE_zeno-fcis"))
            .args(arguments)
            .output(),
    )
}

fn successful(output: Output) -> Value {
    assert!(
        output.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    fixture_result(serde_json::from_slice(&output.stdout))
}

fn fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../integrations/tau/finite-ladder.tau")
}

#[cfg(target_os = "linux")]
#[test]
fn tau_import_export_uses_actual_authority_and_retains_source_pairs() {
    let temp = Temp::new();
    let app = temp.path().join("app");
    let kit = temp.path().join("kit");
    let imported = successful(zeno(&[
        "contract",
        "import",
        "--tau",
        path_text(&fixture()),
        "--out",
        path_text(&app),
        "--format",
        "json",
    ]));
    assert_eq!(imported["scope"], "original-width-finite");
    assert_eq!(imported["authority"], "none");
    assert!(app.join("v2/policy.zcve").is_file());
    let exported = successful(zeno(&[
        "contract",
        "export",
        "--tau",
        path_text(&app),
        "--out",
        path_text(&kit),
        "--max-tuples",
        "128",
        "--samples",
        "128",
        "--format",
        "json",
    ]));
    assert_eq!(exported["status"], "exported-unrun");
    let bundle: Value = read_json(kit.join("queries.json"));
    assert_eq!(bundle["review"]["construction"], "full-domain");
    assert_eq!(bundle["review"]["inputs"], 128);
    assert_eq!(bundle["review"]["tally"]["Accept"], 64);
    assert_eq!(bundle["review"]["tally"]["Reject"], 64);
    assert_eq!(array(&bundle["decisions"]).len(), 128);
    let queries = array(&bundle["queries"]);
    assert!(queries.iter().any(|query| query["kind"] == "per-rule"));
    assert!(
        queries
            .iter()
            .any(|query| query["kind"] == "import-decision")
    );
    assert!(
        queries
            .iter()
            .any(|query| query["kind"] == "import-per-rule")
    );
    assert!(
        queries
            .iter()
            .any(|query| query["kind"] == "import-control" && query["expected"] == "F")
    );
    assert!(
        queries
            .iter()
            .all(|query| !text(&query["tau"]).ends_with('.')
                && text(&query["smt"]).contains("(check-sat)"))
    );
    assert!(kit.join("runner.py").is_file());
    // Every observation comes from the actual Authority; unsigned 7+1 wraps
    // to 0 in the imported native contract, while a reject has no successor.
    let wrapped = array(&bundle["decisions"])
        .iter()
        .find(|decision| {
            decision["inputs"]["pre.100.120"] == "7"
                && decision["inputs"]["command.101.140"] == "1"
                && decision["inputs"]["context.102"] == "1"
        })
        .unwrap_or_else(|| panic!("missing the wraparound input decision"));
    assert_eq!(wrapped["post"]["120"], "0");
}

#[cfg(target_os = "linux")]
#[test]
fn tau_import_refuses_unscaled_256_and_stale_metadata_without_output() {
    let temp = Temp::new();
    let wide = temp.path().join("wide.tau");
    fixture_result(fs::write(
        &wide,
        fixture_result(fs::read_to_string(fixture())).replace("bv[3]", "bv[256]"),
    ));
    let app = temp.path().join("app");
    let refused = zeno(&[
        "contract",
        "import",
        "--tau",
        path_text(&wide),
        "--out",
        path_text(&app),
        "--format",
        "json",
    ]);
    assert!(!refused.status.success());
    assert!(!app.exists());
    let scaled = successful(zeno(&[
        "contract",
        "import",
        "--tau",
        path_text(&wide),
        "--scale-width",
        "256:3",
        "--out",
        path_text(&app),
        "--format",
        "json",
    ]));
    assert_eq!(scaled["scope"], "explicitly-scaled");
    let policy = app.join("v2/policy.json");
    let mut text = fixture_result(fs::read_to_string(&policy));
    text.push('\n');
    fixture_result(fs::write(&policy, text));
    let kit = temp.path().join("kit");
    let refused = zeno(&[
        "contract",
        "export",
        "--tau",
        path_text(&app),
        "--out",
        path_text(&kit),
        "--format",
        "json",
    ]);
    assert!(!refused.status.success());
    assert!(!kit.exists());
}

#[test]
fn tau_commands_describe_external_parser_and_new_files() {
    let export = successful(zeno(&["describe", "contract", "export"]));
    let import = successful(zeno(&["describe", "contract", "import"]));
    assert!(export.to_string().contains("new-tau-query-directory"));
    assert!(import.to_string().contains("new-contract-directory"));
}

#[cfg(target_os = "linux")]
#[test]
fn tau_allowance_profile_uses_source_derived_eleven_cases_and_finite_enum() {
    let temp = Temp::new();
    let source = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../integrations/tau/allowance-study.tau");
    let app = temp.path().join("allowance");
    let kit = temp.path().join("allowance-kit");
    let imported = successful(zeno(&[
        "contract",
        "import",
        "--tau",
        path_text(&source),
        "--profile",
        "allowance-study-bv3",
        "--scale-width",
        "256:3",
        "--out",
        path_text(&app),
        "--format",
        "json",
    ]));
    assert_eq!(imported["scope"], "explicitly-scaled");
    let policy: Value = read_json(app.join("v2/policy.json"));
    assert_eq!(array(&policy["cases"]).len(), 11);
    assert_eq!(
        policy["law_kinds"]
            .as_object()
            .unwrap_or_else(|| panic!("expected law kinds to be an object"))
            .len(),
        4
    );
    let metadata: Value = read_json(app.join("v2/tau-import.json"));
    assert_eq!(metadata["profile"], "allowance-study-bv3");
    assert_eq!(metadata["mapping"]["i2.kind"]["native_offset"], 140);
    assert_eq!(
        metadata["finite_domain"]["i2.kind"],
        serde_json::json!([0, 3])
    );
    assert_eq!(
        metadata["finite_domain"]["i1.available"],
        serde_json::json!([0, 7])
    );
    successful(zeno(&[
        "contract",
        "export",
        "--tau",
        path_text(&app),
        "--out",
        path_text(&kit),
        "--max-tuples",
        "2048",
        "--samples",
        "8",
        "--format",
        "json",
    ]));
    let bundle: Value = read_json(kit.join("queries.json"));
    let queries = array(&bundle["queries"]);
    assert_eq!(
        queries
            .iter()
            .filter(|query| query["kind"] == "per-rule")
            .count(),
        11
    );
    assert_eq!(
        queries
            .iter()
            .filter(|query| query["kind"] == "import-per-rule")
            .count(),
        11
    );
    assert!(
        queries
            .iter()
            .any(|query| query["kind"] == "import-control" && query["expected"] == "F")
    );
    for decision in array(&bundle["decisions"]) {
        assert!(
            ["140", "141", "142", "143"].contains(&text(&decision["inputs"]["command.101.120"]))
        );
        if decision["class"] == "Reject" {
            assert_eq!(decision["post"], serde_json::json!({}));
        }
    }
    // Stale role/enum metadata refuses before producing a kit.
    let mut changed = metadata;
    changed["mapping"]["i2.kind"]["native_offset"] = 0.into();
    fixture_result(fs::write(
        app.join("v2/tau-import.json"),
        fixture_result(serde_json::to_vec(&changed)),
    ));
    let bad_kit = temp.path().join("bad-kit");
    assert!(
        !zeno(&[
            "contract",
            "export",
            "--tau",
            path_text(&app),
            "--out",
            path_text(&bad_kit),
            "--max-tuples",
            "2048",
            "--samples",
            "1",
            "--format",
            "json"
        ])
        .status
        .success()
    );
    assert!(!bad_kit.exists());
}

#[cfg(target_os = "macos")]
#[test]
fn tau_import_refuses_without_its_required_parser_memory_limit() {
    // The hosted macOS Python process cannot lower its existing address
    // space to the importer's 256 MiB cap. Refusal must not create an app.
    let temp = Temp::new();
    let app = temp.path().join("app");
    let output = zeno(&[
        "contract",
        "import",
        "--tau",
        path_text(&fixture()),
        "--out",
        path_text(&app),
        "--format",
        "json",
    ]);
    assert!(!output.status.success());
    let report: Value = fixture_result(serde_json::from_slice(&output.stdout));
    assert_eq!(report["status"], "refused");
    assert_eq!(report["authority"], "none");
    assert_eq!(
        report["reason"],
        "Tau parser refused the source; see its diagnostic on stderr"
    );
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("Tau import refused: current limit exceeds maximum limit"),
        "unexpected refusal: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!app.exists());
}
