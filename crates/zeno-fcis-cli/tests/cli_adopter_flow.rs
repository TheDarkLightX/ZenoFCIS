//! Process-level adopter journeys for the published `zeno-fcis` command.
#![forbid(unsafe_code)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

use serde_json::{Value, json};
use zeno_fcis_codec::CommitmentHasher as _;
use zeno_fcis_crypto::RustCryptoSha256;
use zeno_fcis_formal_tools::{LEAN_LINUX_X86_64_TREE_SHA256, inspect_lean_toolchain};

static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

struct TempRoot(PathBuf);
impl TempRoot {
    fn new(label: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "zeno-fcis-cli-{label}-{}-{}",
            std::process::id(),
            TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap_or_else(|error| panic!("create temp root: {error}"));
        Self(path)
    }
    fn path(&self) -> &Path {
        &self.0
    }
}
impl Drop for TempRoot {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn cli() -> &'static str {
    env!("CARGO_BIN_EXE_zeno-fcis")
}

#[test]
fn cli_durable_counter_emits_a_complete_project_without_overwriting() {
    let temp = TempRoot::new("durable-counter");
    let target = temp.path().join("app");
    let first = run(Command::new(cli())
        .arg("new")
        .arg(&target)
        .args(["--template", "durable-counter"]));
    assert!(
        first.status.success(),
        "{}",
        String::from_utf8_lossy(&first.stderr)
    );
    for name in [
        "Cargo.toml",
        "README.md",
        "project.zeno",
        "profile.rs",
        "build.rs",
        "src/lib.rs",
        "src/main.rs",
        "src/program.rs",
        "src/laws.rs",
        "src/delivery.rs",
        "tests/lifecycle.rs",
    ] {
        assert!(target.join(name).is_file(), "missing {name}");
    }
    let checked = run(Command::new(cli())
        .arg("check")
        .arg(target.join("project.zeno")));
    assert!(
        checked.status.success(),
        "{}",
        String::from_utf8_lossy(&checked.stderr)
    );
    let source = read(target.join("src/program.rs"));
    let repeated = run(Command::new(cli())
        .arg("new")
        .arg(&target)
        .args(["--template", "durable-counter"]));
    assert!(!repeated.status.success());
    assert_eq!(read(target.join("src/program.rs")), source);
}

fn repository_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .unwrap_or_else(|| panic!("CLI crate is outside the repository"))
        .to_path_buf()
}

fn run(command: &mut Command) -> Output {
    command
        .output()
        .unwrap_or_else(|error| panic!("run CLI: {error}"))
}
fn read(path: impl AsRef<Path>) -> Vec<u8> {
    fs::read(path).unwrap_or_else(|error| panic!("read test file: {error}"))
}

fn json_stdout(output: &Output) -> Value {
    serde_json::from_slice(&output.stdout)
        .unwrap_or_else(|error| panic!("decode CLI JSON: {error}"))
}

fn sha256_hex(bytes: &[u8]) -> String {
    RustCryptoSha256::hash(bytes).to_string()
}

#[derive(Debug, Eq, PartialEq)]
struct ArtifactSnapshot {
    path: PathBuf,
    content: Option<Vec<u8>>,
}

fn artifact_snapshot(root: &Path) -> Option<Vec<ArtifactSnapshot>> {
    if !root.exists() {
        return None;
    }
    fn visit(path: &Path, entries: &mut Vec<ArtifactSnapshot>) {
        let mut children = fs::read_dir(path)
            .unwrap_or_else(|error| panic!("snapshot directory: {error}"))
            .map(|entry| {
                entry
                    .unwrap_or_else(|error| panic!("snapshot entry: {error}"))
                    .path()
            })
            .collect::<Vec<_>>();
        children.sort();
        for child in children {
            assert!(
                !child.is_symlink(),
                "unexpected symlink in generated test output"
            );
            let content = if child.is_dir() {
                None
            } else {
                Some(read(&child))
            };
            entries.push(ArtifactSnapshot {
                path: child.clone(),
                content,
            });
            if child.is_dir() {
                visit(&child, entries);
            }
        }
    }
    let mut entries = Vec::new();
    visit(root, &mut entries);
    Some(entries)
}

#[test]
fn agent_command_discovery_is_deterministic_and_needs_no_project() {
    let root = TempRoot::new("discovery");
    let invoke = || run(Command::new(cli()).arg("describe").current_dir(root.path()));
    let first = invoke();
    assert_eq!(first.status.code(), Some(0));
    assert!(first.stderr.is_empty());
    assert_eq!(
        fs::read_dir(root.path())
            .unwrap_or_else(|error| panic!("list empty root: {error}"))
            .count(),
        0
    );
    fs::write(root.path().join("project.zeno"), b"invalid inert project")
        .unwrap_or_else(|error| panic!("write inert project: {error}"));
    fs::write(
        root.path().join("zeno-fcis.tools.json"),
        b"invalid inert tools",
    )
    .unwrap_or_else(|error| panic!("write inert tools: {error}"));
    let second = invoke();
    assert_eq!(second.status.code(), Some(0));
    assert_eq!(first.stdout, second.stdout);
    let description = json_stdout(&first);
    assert_eq!(description["schema"], "zeno-fcis/cli-description/1");
    assert_eq!(description["cli_version"], env!("CARGO_PKG_VERSION"));
    assert_eq!(description["authority"], "diagnostic-only");
    assert_eq!(description["command"]["subcommand_required"], true);
    assert_eq!(description["limits"]["max_source_bytes"], 1_048_576);
    let mut canonical = serde_json::to_vec(&description)
        .unwrap_or_else(|error| panic!("serialize description: {error}"));
    canonical.push(b'\n');
    assert_eq!(first.stdout, canonical);

    let selected = run(Command::new(cli())
        .args(["describe", "generate"])
        .current_dir(root.path()));
    assert_eq!(selected.status.code(), Some(0));
    let selected = json_stdout(&selected);
    assert_eq!(selected["command"]["name"], "generate");
    assert_eq!(selected["command"]["subcommand_required"], false);
    assert_eq!(selected["command"]["effects"]["read_only_flag"], "--check");
    let arguments = selected["command"]["arguments"]
        .as_array()
        .unwrap_or_else(|| panic!("missing argument declarations"));
    let out = arguments
        .iter()
        .find(|arg| arg["id"] == "out")
        .unwrap_or_else(|| panic!("missing output argument"));
    assert_eq!(out["required"], true);
    assert_eq!(out["long"], "out");
    let format = arguments
        .iter()
        .find(|arg| arg["id"] == "format")
        .unwrap_or_else(|| panic!("missing output format argument"));
    assert_eq!(format["choices"], json!(["human", "json"]));
    assert_eq!(format["defaults"], json!(["human"]));
    let check = arguments
        .iter()
        .find(|arg| arg["id"] == "check")
        .unwrap_or_else(|| panic!("missing drift-check flag"));
    assert_eq!(check["arity"], json!({"min": 0, "max": 0}));
    let group = run(Command::new(cli()).args(["describe", "backend"]));
    assert_eq!(group.status.code(), Some(0));
    assert_eq!(json_stdout(&group)["command"]["subcommand_required"], true);
    for arguments in [vec![], vec!["backend"]] {
        let missing = run(Command::new(cli()).args(arguments));
        assert_eq!(missing.status.code(), Some(64));
    }
    let backend = run(Command::new(cli()).args(["describe", "backend", "verify"]));
    assert_eq!(backend.status.code(), Some(0));
    assert_eq!(
        json_stdout(&backend)["command"]["effects"]["executes_tools"],
        true
    );
    let unknown = run(Command::new(cli()).args(["describe", "unknown-command"]));
    assert_eq!(unknown.status.code(), Some(64));
    assert_eq!(json_stdout(&unknown)["error"]["code"], "unknown-command");
    assert_eq!(
        read(root.path().join("project.zeno")),
        b"invalid inert project"
    );
    assert_eq!(
        read(root.path().join("zeno-fcis.tools.json")),
        b"invalid inert tools"
    );
}

#[test]
fn agent_generation_json_distinguishes_drift_and_failures_without_writes() {
    let root = TempRoot::new("agent-generation");
    let project = root.path().join("project.zeno");
    fs::copy(
        repository_root().join("examples/minimal/project.zeno"),
        &project,
    )
    .unwrap_or_else(|error| panic!("copy project: {error}"));
    let output = root.path().join("generated");
    let invoke = |check: bool| {
        let before = check.then(|| artifact_snapshot(&output));
        let mut command = Command::new(cli());
        command
            .arg("generate")
            .arg(&project)
            .arg("--out")
            .arg(&output)
            .args(["--format", "json"]);
        if check {
            command.arg("--check");
        }
        let result = run(&mut command);
        if let Some(before) = before {
            assert_eq!(
                artifact_snapshot(&output),
                before,
                "drift check modified its output tree"
            );
        }
        result
    };
    let absent = invoke(true);
    assert_eq!(absent.status.code(), Some(1));
    assert_eq!(json_stdout(&absent)["status"], "drift");
    assert_eq!(
        json_stdout(&absent)["drift"],
        json!(["generated.rs", "PROJECT_MANIFEST.zfcis"])
    );
    assert!(!output.exists());
    let generated = invoke(false);
    assert_eq!(generated.status.code(), Some(0));
    assert_eq!(json_stdout(&generated)["status"], "generated");
    assert!(generated.stderr.is_empty());
    let source = read(output.join("generated.rs"));
    let manifest = read(output.join("PROJECT_MANIFEST.zfcis"));
    for _ in 0..2 {
        let current = invoke(true);
        assert_eq!(current.status.code(), Some(0));
        assert_eq!(json_stdout(&current)["status"], "current");
        assert_eq!(json_stdout(&current)["drift"], json!([]));
        assert_eq!(read(output.join("generated.rs")), source);
        assert_eq!(read(output.join("PROJECT_MANIFEST.zfcis")), manifest);
    }
    fs::write(output.join("generated.rs"), b"reviewed edit")
        .unwrap_or_else(|error| panic!("write source edit: {error}"));
    let drift = invoke(true);
    assert_eq!(drift.status.code(), Some(1));
    assert_eq!(json_stdout(&drift)["drift"], json!(["generated.rs"]));
    assert_eq!(read(output.join("generated.rs")), b"reviewed edit");
    assert_eq!(read(output.join("PROJECT_MANIFEST.zfcis")), manifest);
    fs::remove_file(output.join("generated.rs"))
        .unwrap_or_else(|error| panic!("remove test edit: {error}"));
    fs::create_dir(output.join("generated.rs"))
        .unwrap_or_else(|error| panic!("create invalid artifact kind: {error}"));
    let unreadable = invoke(true);
    assert_eq!(unreadable.status.code(), Some(3));
    assert_eq!(
        json_stdout(&unreadable)["error"]["code"],
        "artifact-read-failed"
    );
    assert!(output.join("generated.rs").is_dir());
    assert_eq!(read(output.join("PROJECT_MANIFEST.zfcis")), manifest);
    fs::write(&project, b"invalid source")
        .unwrap_or_else(|error| panic!("write invalid project: {error}"));
    let invalid = invoke(true);
    assert_eq!(invalid.status.code(), Some(1));
    assert_eq!(json_stdout(&invalid)["status"], "invalid");
    assert!(output.join("generated.rs").is_dir());
    assert_eq!(read(output.join("PROJECT_MANIFEST.zfcis")), manifest);
}

#[cfg(unix)]
fn write_executable(path: &Path, bytes: &[u8]) {
    use std::os::unix::fs::PermissionsExt as _;

    fs::write(path, bytes).unwrap_or_else(|error| panic!("write executable: {error}"));
    fs::set_permissions(path, fs::Permissions::from_mode(0o700))
        .unwrap_or_else(|error| panic!("make executable: {error}"));
}

fn evidence_directory(project: &Path) -> PathBuf {
    let root = project
        .parent()
        .unwrap_or_else(|| panic!("project has no parent"))
        .join(".zeno-fcis/evidence");
    let directories = fs::read_dir(&root)
        .unwrap_or_else(|error| panic!("read evidence root: {error}"))
        .map(|entry| {
            entry
                .unwrap_or_else(|error| panic!("read evidence entry: {error}"))
                .path()
        })
        .collect::<Vec<_>>();
    assert_eq!(directories.len(), 1);
    directories[0].clone()
}

fn assert_retained(directory: &Path, names: &[&str]) {
    for name in names {
        assert!(directory.join(name).is_file(), "missing retained {name}");
    }
}

#[test]
fn rc3_cli_mini_determinator_check_json_contract() {
    let project = repository_root().join("examples/mini-determinator/project.zeno");
    let invoke = || {
        run(Command::new(cli())
            .arg("check")
            .arg(&project)
            .args(["--format", "json"]))
    };
    let first = invoke();
    let second = invoke();
    assert!(
        first.status.success(),
        "{}",
        String::from_utf8_lossy(&first.stderr)
    );
    assert_eq!(first.stdout, second.stdout);
    assert!(first.stderr.is_empty());
    let document = json_stdout(&first);
    assert_eq!(document["schema"], "zeno-fcis/cli/1");
    assert_eq!(document["status"], "valid");
    assert_eq!(document["project_id"], 2);
    assert_eq!(document["components"], 2);
    assert_eq!(document["claims"], 2);
    let keys = document
        .as_object()
        .unwrap_or_else(|| panic!("valid result is not an object"))
        .keys()
        .map(String::as_str)
        .collect::<Vec<_>>();
    let mut sorted = keys.clone();
    sorted.sort_unstable();
    assert_eq!(keys, sorted);
}

#[test]
fn cli_project_read_failures_preserve_json_and_exit_class() {
    let root = TempRoot::new("read-errors");
    let missing = root.path().join("missing.zeno");
    let invalid_utf8 = root.path().join("invalid-utf8.zeno");
    fs::write(&invalid_utf8, [0xff, 0xfe])
        .unwrap_or_else(|error| panic!("write invalid UTF-8 fixture: {error}"));
    for path in [&missing, &invalid_utf8] {
        for command in ["check", "explain", "graph"] {
            let invoke = || {
                run(Command::new(cli())
                    .arg(command)
                    .arg(path)
                    .args(["--format", "json"]))
            };
            let first = invoke();
            let second = invoke();
            assert_eq!(first.status.code(), Some(3));
            assert_eq!(first.stdout, second.stdout);
            assert!(first.stderr.is_empty());
            let document = json_stdout(&first);
            assert_eq!(document["schema"], "zeno-fcis/cli/1");
            assert_eq!(document["status"], "error");
            assert_eq!(document["path"], path.display().to_string());
            assert_eq!(document["error"]["code"], "project-read-failed");
            assert!(
                !document["error"]["message"]
                    .as_str()
                    .unwrap_or("")
                    .is_empty()
            );
        }
    }
    let human = run(Command::new(cli()).arg("check").arg(&missing));
    assert_eq!(human.status.code(), Some(3));
    assert!(human.stdout.is_empty());
    assert!(String::from_utf8_lossy(&human.stderr).contains("read "));
    assert!(!missing.exists());
    assert_eq!(read(&invalid_utf8), [0xff, 0xfe]);
}

#[test]
fn cli_project_file_budget_accepts_the_limit_and_stops_one_over() {
    let root = TempRoot::new("source-budget");
    let path = root.path().join("project.zeno");
    let limit = zeno_fcis_spec::SourceLimits::default().max_bytes();
    let mut source = read(repository_root().join("examples/minimal/project.zeno"));
    source.resize(limit, b' ');
    fs::write(&path, &source).unwrap_or_else(|error| panic!("write exact source: {error}"));
    let invoke = || {
        run(Command::new(cli())
            .arg("check")
            .arg(&path)
            .args(["--format", "json"]))
    };
    let exact = invoke();
    assert_eq!(exact.status.code(), Some(0));
    assert_eq!(json_stdout(&exact)["status"], "valid");

    // Size admission precedes UTF-8 interpretation. No partial source enters
    // the parser, even when the first byte over the budget is invalid UTF-8.
    for last in [b' ', 0xff] {
        source.truncate(limit);
        source.push(last);
        fs::write(&path, &source).unwrap_or_else(|error| panic!("write excess source: {error}"));
        let excess = invoke();
        assert_eq!(excess.status.code(), Some(3));
        let document = json_stdout(&excess);
        assert_eq!(document["status"], "error");
        assert_eq!(document["error"]["code"], "project-read-failed");
        assert!(
            document["error"]["message"]
                .as_str()
                .unwrap_or("")
                .contains("source byte limit")
        );
        assert_eq!(read(&path), source);
    }
}

#[test]
#[cfg(unix)]
fn cli_tool_timeout_is_execution_failure() {
    let root = TempRoot::new("timeout");
    let executable = root.path().join("cvc5");
    // This deterministic test tool stalls in version admission. The existing
    // process deadline and process-group cleanup must stop it.
    let script = b"#!/bin/sh\nexec /bin/sleep 30\n";
    write_executable(&executable, script);
    let manifest = root.path().join("tools.json");
    fs::write(
        &manifest,
        serde_json::to_vec(&json!({
            "format": "zeno-fcis/tools/2",
            "tools": [{
                "backend": "cvc5", "path": executable, "version": "1.3.3",
                "sha256": sha256_hex(script), "timeout_ms": 100,
                "max_output_bytes": 4096, "allowed_axioms": []
            }]
        }))
        .unwrap_or_else(|error| panic!("encode test manifest: {error}")),
    )
    .unwrap_or_else(|error| panic!("write test manifest: {error}"));
    for arguments in [vec!["doctor"], vec!["backend", "verify"]] {
        let result = run(Command::new(cli())
            .args(arguments)
            .arg("--tools")
            .arg(&manifest));
        assert_eq!(result.status.code(), Some(3));
        assert!(String::from_utf8_lossy(&result.stderr).contains("Timeout"));
    }
    assert!(!root.path().join(".zeno-fcis/evidence").exists());
}

#[test]
fn rc3_cli_generate_check_is_read_only_and_detects_drift() {
    let root = TempRoot::new("drift");
    let project = root.path().join("project.zeno");
    let generated = root.path().join("generated");
    fs::copy(
        repository_root().join("examples/mini-determinator/project.zeno"),
        &project,
    )
    .unwrap_or_else(|error| panic!("copy project: {error}"));

    let initial = run(Command::new(cli())
        .arg("generate")
        .arg(&project)
        .arg("--out")
        .arg(&generated));
    assert!(
        initial.status.success(),
        "{}",
        String::from_utf8_lossy(&initial.stderr)
    );
    let manifest_before = fs::read(generated.join("PROJECT_MANIFEST.zfcis"))
        .unwrap_or_else(|error| panic!("read manifest: {error}"));

    let current = run(Command::new(cli())
        .arg("generate")
        .arg(&project)
        .arg("--out")
        .arg(&generated)
        .arg("--check"));
    assert!(current.status.success());

    let tampered = b"reviewed local edit\n";
    fs::write(generated.join("generated.rs"), tampered)
        .unwrap_or_else(|error| panic!("write drift: {error}"));
    let drift = run(Command::new(cli())
        .arg("generate")
        .arg(&project)
        .arg("--out")
        .arg(&generated)
        .arg("--check"));
    assert_eq!(drift.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&drift.stderr).contains("generated drift: generated.rs"));
    assert_eq!(read(generated.join("generated.rs")), tampered);
    assert_eq!(
        read(generated.join("PROJECT_MANIFEST.zfcis")),
        manifest_before
    );
}

#[test]
fn rc3_cli_invalid_json_diagnostics_are_versioned_and_ordered() {
    let root = TempRoot::new("diagnostics");
    let project = root.path().join("project.zeno");
    fs::write(&project, "zeno 2; project 1 broken;\n")
        .unwrap_or_else(|error| panic!("write invalid project: {error}"));
    let output = run(Command::new(cli())
        .arg("check")
        .arg(&project)
        .args(["--format", "json"]));
    assert_eq!(output.status.code(), Some(1));
    let document = json_stdout(&output);
    assert_eq!(document["schema"], "zeno-fcis/cli/1");
    assert_eq!(document["status"], "invalid");
    assert_eq!(document["truncated"], false);
    assert!(
        document["diagnostics"]
            .as_array()
            .is_some_and(|items| !items.is_empty())
    );
    let keys = document
        .as_object()
        .unwrap_or_else(|| panic!("invalid result is not an object"))
        .keys()
        .map(String::as_str)
        .collect::<Vec<_>>();
    let mut sorted = keys.clone();
    sorted.sort_unstable();
    assert_eq!(keys, sorted);
}

#[test]
fn rc3_cli_new_refuses_existing_content_and_usage_is_64() {
    let root = TempRoot::new("new");
    let target = root.path().join("target");
    let created = run(Command::new(cli())
        .arg("new")
        .arg(&target)
        .args(["--template", "minimal"]));
    assert!(
        created.status.success(),
        "{}",
        String::from_utf8_lossy(&created.stderr)
    );
    let project_before = read(target.join("project.zeno"));
    let readme_before = read(target.join("README.md"));

    let repeated = run(Command::new(cli())
        .arg("new")
        .arg(&target)
        .args(["--template", "minimal"]));
    assert_eq!(repeated.status.code(), Some(1));
    assert_eq!(read(target.join("project.zeno")), project_before);
    assert_eq!(read(target.join("README.md")), readme_before);

    let nonempty = root.path().join("nonempty");
    fs::create_dir(&nonempty).unwrap_or_else(|error| panic!("create nonempty target: {error}"));
    let sentinel = nonempty.join("keep.txt");
    fs::write(&sentinel, b"keep me").unwrap_or_else(|error| panic!("write sentinel: {error}"));
    let refused = run(Command::new(cli())
        .arg("new")
        .arg(&nonempty)
        .args(["--template", "minimal"]));
    assert_eq!(refused.status.code(), Some(1));
    assert_eq!(read(&sentinel), b"keep me");
    assert!(!nonempty.join("project.zeno").exists());

    let usage = run(Command::new(cli()).arg("unknown-command"));
    assert_eq!(usage.status.code(), Some(64));
}

#[test]
fn rc3_cli_computes_a_portable_lean_inventory_for_tools_v2() {
    let root = TempRoot::new("lean-inventory");
    let distribution = root.path().join("lean-4.30.0");
    fs::create_dir_all(distribution.join("bin"))
        .unwrap_or_else(|error| panic!("create fake Lean bin: {error}"));
    fs::create_dir_all(distribution.join("lib/lean"))
        .unwrap_or_else(|error| panic!("create fake Lean library: {error}"));
    fs::write(distribution.join("bin/lean"), b"bounded fake executable\n")
        .unwrap_or_else(|error| panic!("write fake Lean executable: {error}"));
    fs::write(
        distribution.join("lib/lean/Init.olean"),
        b"bounded fake library\n",
    )
    .unwrap_or_else(|error| panic!("write fake Lean library: {error}"));

    let invoke_json = || {
        run(Command::new(cli())
            .args(["backend", "inventory-lean"])
            .arg(&distribution)
            .args(["--format", "json"]))
    };
    let first = invoke_json();
    let second = invoke_json();
    assert!(
        first.status.success(),
        "{}",
        String::from_utf8_lossy(&first.stderr)
    );
    assert_eq!(first.stdout, second.stdout);
    let document = json_stdout(&first);
    assert_eq!(document["format"], "zeno-fcis/toolchain-inventory/1");
    assert_eq!(document["files"].as_array().map(Vec::len), Some(2));
    assert_eq!(document["tree_sha256"].as_str().map(str::len), Some(64));
    assert_eq!(document["files"][0]["path"], "bin/lean");
    assert_eq!(document["files"][1]["path"], "lib/lean/Init.olean");

    let human = run(Command::new(cli())
        .args(["backend", "inventory-lean"])
        .arg(&distribution));
    assert!(human.status.success());
    let text = String::from_utf8_lossy(&human.stdout);
    assert!(text.contains(document["tree_sha256"].as_str().unwrap_or("missing")));
    assert!(text.contains("files 2"));
}

#[test]
#[cfg(unix)]
fn rc3_cli_formal_outcomes_and_retention_are_process_level() {
    let root = TempRoot::new("formal-process");
    let old_manifest = root.path().join("tools-old.json");
    fs::write(
        &old_manifest,
        br#"{"format":"zeno-fcis/tools/1","tools":[]}"#,
    )
    .unwrap_or_else(|error| panic!("write old tools manifest: {error}"));
    let old = run(Command::new(cli())
        .args(["backend", "inspect", "--tools"])
        .arg(&old_manifest));
    assert_eq!(old.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&old.stderr).contains(
        "WrongFormat { expected: \"zeno-fcis/tools/2\", actual: \"zeno-fcis/tools/1\" }"
    ));

    let cvc5_script = b"#!/bin/sh\nif [ \"$1\" = \"--version\" ]; then\n  printf 'This is cvc5 version 1.3.3\\n'\n  exit 0\nfi\nproof=0\nwhile IFS= read -r line; do\n  if [ \"$line\" = \"(get-proof)\" ]; then proof=1; fi\ndone\nif [ \"$proof\" -eq 1 ]; then\n  printf 'unsat\\n(step bounded-proof)\\n'\nelse\n  printf 'unsat\\n'\nfi\n";
    let cvc5 = root.path().join("cvc5");
    write_executable(&cvc5, cvc5_script);
    let cvc5_manifest = root.path().join("tools-cvc5.json");
    fs::write(
        &cvc5_manifest,
        serde_json::to_vec(&json!({
            "format": "zeno-fcis/tools/2",
            "tools": [{
                "backend": "cvc5",
                "path": cvc5,
                "version": "1.3.3",
                "sha256": sha256_hex(cvc5_script),
                "timeout_ms": 1_000,
                "max_output_bytes": 4096,
                "allowed_axioms": []
            }]
        }))
        .unwrap_or_else(|_| unreachable!()),
    )
    .unwrap_or_else(|error| panic!("write CVC5 manifest: {error}"));
    let cvc5_run = root.path().join("cvc5-run");
    fs::create_dir(&cvc5_run).unwrap_or_else(|error| panic!("create CVC5 run: {error}"));
    let cvc5_project = cvc5_run.join("project.zeno");
    fs::copy(
        repository_root().join("examples/minimal/project.zeno"),
        &cvc5_project,
    )
    .unwrap_or_else(|error| panic!("copy CVC5 project: {error}"));
    let cvc5_result = run(Command::new(cli())
        .arg("prove")
        .arg(&cvc5_project)
        .args(["--claim", "500", "--backend", "cvc5", "--tools"])
        .arg(&cvc5_manifest));
    assert_eq!(cvc5_result.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&cvc5_result.stdout).contains("UNSAT proposal retained"));
    let cvc5_evidence = evidence_directory(&cvc5_project);
    assert_retained(
        &cvc5_evidence,
        &[
            "formal-run-record.bin",
            "record.json",
            "source",
            "stderr",
            "stdout",
            "transcript-01-decision-input",
            "transcript-01-decision-stdout",
            "transcript-02-evidence-input",
            "transcript-02-evidence-stdout",
        ],
    );

    let z3_script = b"#!/bin/sh\nif [ \"$1\" = \"--version\" ]; then\n  printf 'Z3 version 4.16.0 - 64 bit\\n'\n  exit 0\nfi\nwhile IFS= read -r line; do :; done\nprintf 'sat\\n(model)\\n'\n";
    let z3 = root.path().join("z3");
    write_executable(&z3, z3_script);
    let z3_manifest = root.path().join("tools-z3.json");
    fs::write(
        &z3_manifest,
        serde_json::to_vec(&json!({
            "format": "zeno-fcis/tools/2",
            "tools": [{
                "backend": "z3",
                "path": z3,
                "version": "4.16.0",
                "sha256": sha256_hex(z3_script),
                "timeout_ms": 1_000,
                "max_output_bytes": 4096,
                "allowed_axioms": []
            }]
        }))
        .unwrap_or_else(|_| unreachable!()),
    )
    .unwrap_or_else(|error| panic!("write Z3 manifest: {error}"));
    let z3_run = root.path().join("z3-run");
    fs::create_dir(&z3_run).unwrap_or_else(|error| panic!("create Z3 run: {error}"));
    let z3_project = z3_run.join("project.zeno");
    let false_project = fs::read_to_string(repository_root().join("examples/minimal/project.zeno"))
        .unwrap_or_else(|error| panic!("read minimal project: {error}"))
        .replace(
            "claim 500 identity cvc5 relational = pre.100 == pre.100;",
            "claim 500 refuted z3 relational = false;",
        );
    fs::write(&z3_project, false_project)
        .unwrap_or_else(|error| panic!("write Z3 project: {error}"));
    let prove = run(Command::new(cli())
        .arg("prove")
        .arg(&z3_project)
        .args(["--claim", "500", "--backend", "z3", "--tools"])
        .arg(&z3_manifest));
    assert_eq!(prove.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&prove.stdout).contains("replayed counterexample retained"));
    let counterexample = run(Command::new(cli())
        .arg("counterexample")
        .arg(&z3_project)
        .args(["--claim", "500", "--backend", "z3", "--tools"])
        .arg(&z3_manifest));
    assert_eq!(counterexample.status.code(), Some(0));
    let z3_evidence = evidence_directory(&z3_project);
    assert_retained(
        &z3_evidence,
        &[
            "formal-run-record.bin",
            "record.json",
            "counterexample.json",
        ],
    );
}

#[test]
#[cfg(unix)]
#[ignore = "requires the workflow-pinned Lean 4.30.0 Linux x86-64 distribution"]
fn pinned_lean_cli_prove_is_process_level() {
    let lean = PathBuf::from(
        std::env::var_os("ZENO_FCIS_LEAN").unwrap_or_else(|| panic!("missing pinned Lean")),
    );
    let lean_root = PathBuf::from(
        std::env::var_os("ZENO_FCIS_LEAN_ROOT")
            .unwrap_or_else(|| panic!("missing pinned Lean root")),
    );
    let inventory = inspect_lean_toolchain(&lean_root)
        .unwrap_or_else(|error| panic!("inventory pinned Lean: {error:?}"));
    assert_eq!(
        inventory.tree_sha256().to_string(),
        LEAN_LINUX_X86_64_TREE_SHA256
    );
    let executable = fs::read(&lean).unwrap_or_else(|error| panic!("read pinned Lean: {error}"));
    let root = TempRoot::new("pinned-lean-process");
    let tools = root.path().join("tools.json");
    fs::write(
        &tools,
        serde_json::to_vec(&json!({
            "format": "zeno-fcis/tools/2",
            "tools": [{
                "backend": "lean",
                "path": lean,
                "version": "4.30.0",
                "sha256": sha256_hex(&executable),
                "runtime": {
                    "root": lean_root,
                    "tree_sha256": LEAN_LINUX_X86_64_TREE_SHA256
                },
                "timeout_ms": 30_000,
                "max_output_bytes": 1_048_576,
                "allowed_axioms": ["Quot.sound", "propext"]
            }]
        }))
        .unwrap_or_else(|_| unreachable!()),
    )
    .unwrap_or_else(|error| panic!("write Lean manifest: {error}"));
    let project = root.path().join("project.zeno");
    fs::copy(
        repository_root().join("examples/mini-determinator/project.zeno"),
        &project,
    )
    .unwrap_or_else(|error| panic!("copy Mini Determinator project: {error}"));
    let output = run(Command::new(cli())
        .arg("prove")
        .arg(&project)
        .args(["--claim", "501", "--backend", "lean", "--tools"])
        .arg(&tools));
    assert_eq!(
        output.status.code(),
        Some(0),
        "stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("kernel checked"));
    let evidence = evidence_directory(&project);
    assert_retained(
        &evidence,
        &[
            "formal-run-record.bin",
            "record.json",
            "source",
            "toolchain.json",
            "transcript-01-kernel-input",
            "transcript-01-kernel-stdout",
        ],
    );
}
