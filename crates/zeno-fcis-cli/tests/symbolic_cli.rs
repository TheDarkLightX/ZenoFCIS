//! `zeno-fcis contract check-symbolic` and `transform check --symbolic`
//! through the real binary.
//!
//! The tests without `#[ignore]` run stand-in solvers: shell scripts that
//! pass the formal-tools admission (version line and SHA-256) and answer
//! fixed text, so a planted disagreement or `unknown` is reproducible
//! anywhere. The `pinned_symbolic_*` tests run the pinned CVC5 1.3.3 and
//! Z3 4.16.0 named by `ZENO_FCIS_CVC5` and `ZENO_FCIS_Z3`, and admit them
//! only if their bytes hash to the pinned binaries; they are the acceptance
//! runs on the app study's escrow and spend-approval contracts.
#![cfg(target_os = "linux")]
#![forbid(unsafe_code)]

use serde_json::{Value, json};
use std::fs;
use std::os::unix::fs::PermissionsExt as _;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use zeno_fcis_codec::CommitmentHasher;
use zeno_fcis_crypto::RustCryptoSha256;

static NEXT: AtomicU64 = AtomicU64::new(0);
const CLI: &str = env!("CARGO_BIN_EXE_zeno-fcis");

/// SHA-256 of the CVC5 1.3.3 and Z3 4.16.0 binaries inside the archives
/// that `release/formal-tools-linux-x86_64.sha256` pins.
const CVC5_SHA256: &str = "e8d7870d57ab55e81619d2373b043da05ea1d37ca393931bdb5d8b9788cd64c4";
const Z3_SHA256: &str = "e583c4186a45e72411fa2cb2048401eed03f0f8e5f24694676a8f6271a50b765";

/// The study's strengthening: a Created escrow has paid nothing out.
const CREATED_PAYS_NOTHING: &str = r#"{
  "schema": "zeno-fcis/strengthening/1",
  "invariants": [
    {"name": "created_pays_nothing", "formula": "post.100.110 == 150 -> post.100.113 == 0 && post.100.114 == 0"}
  ]
}"#;

struct Directory(PathBuf);

impl Directory {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "zeno-cli-symbolic-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap_or_else(|error| panic!("create test directory: {error}"));
        Self(path)
    }

    fn path(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }

    fn write(&self, name: &str, bytes: &[u8]) -> PathBuf {
        let path = self.path(name);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap_or_else(|error| panic!("create {name}: {error}"));
        }
        fs::write(&path, bytes).unwrap_or_else(|error| panic!("write {name}: {error}"));
        path
    }

    /// A copy of a contract fixture with `(file, from, to)` replacements,
    /// each matching exactly once.
    fn contract(&self, name: &str, fixture: &str, replacements: &[(&str, &str, &str)]) -> PathBuf {
        let source = fixtures().join(fixture);
        for file in ["project.zeno", "v2/policy.json"] {
            let mut text = fs::read_to_string(source.join(file))
                .unwrap_or_else(|error| panic!("read {fixture}/{file}: {error}"));
            for (target, from, to) in replacements {
                if *target == file {
                    assert_eq!(
                        text.matches(from).count(),
                        1,
                        "{from} must occur once in {file}"
                    );
                    text = text.replacen(from, to, 1);
                }
            }
            self.write(&format!("{name}/{file}"), text.as_bytes());
        }
        self.path(name)
    }
}

impl Drop for Directory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

fn sha256(bytes: &[u8]) -> String {
    RustCryptoSha256::hash(bytes)
        .as_bytes()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

struct Run {
    exit: Option<i32>,
    stdout: String,
    stderr: String,
}

impl Run {
    fn json(&self) -> Value {
        serde_json::from_str(&self.stdout)
            .unwrap_or_else(|error| panic!("stdout is one JSON document: {error}: {}", self.stdout))
    }
}

fn run(arguments: &[&std::ffi::OsStr]) -> Run {
    let output = Command::new(CLI)
        .args(arguments)
        .output()
        .unwrap_or_else(|error| panic!("run zeno-fcis: {error}"));
    Run {
        exit: output.status.code(),
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    }
}

/// `contract check-symbolic`, with the report on stdout and the summary
/// lines on stderr.
fn check_symbolic(dir: &Path, tools: Option<&Path>, extra: &[&str]) -> Run {
    let mut arguments: Vec<&std::ffi::OsStr> = vec![
        "contract".as_ref(),
        "check-symbolic".as_ref(),
        dir.as_os_str(),
    ];
    if let Some(tools) = tools {
        arguments.extend(["--tools".as_ref(), tools.as_os_str()]);
    }
    arguments.extend(extra.iter().map(std::ffi::OsStr::new));
    run(&arguments)
}

fn export(dir: &Path, out: &Path) {
    let exported = run(&[
        "contract".as_ref(),
        "export-program".as_ref(),
        dir.as_os_str(),
        "--out".as_ref(),
        out.as_os_str(),
    ]);
    assert_eq!(exported.exit, Some(0), "{}", exported.stderr);
}

fn transform(original: &Path, candidate: &Path, extra: &[&std::ffi::OsStr]) -> Run {
    let mut arguments: Vec<&std::ffi::OsStr> = vec![
        "transform".as_ref(),
        "check".as_ref(),
        "--original".as_ref(),
        original.as_os_str(),
        "--candidate".as_ref(),
        candidate.as_os_str(),
    ];
    arguments.extend_from_slice(extra);
    run(&arguments)
}

/// A tools manifest naming two solvers.
fn manifest(directory: &Directory, cvc5: (&Path, &str), z3: (&Path, &str)) -> PathBuf {
    let entry = |backend: &str, (path, sha256): (&Path, &str), version: &str| {
        json!({
            "backend": backend, "path": path.display().to_string(), "version": version,
            "sha256": sha256, "timeout_ms": 120_000, "max_output_bytes": 16_777_216,
            "allowed_axioms": [],
        })
    };
    let manifest = json!({
        "format": "zeno-fcis/tools/2",
        "tools": [entry("cvc5", cvc5, "1.3.3"), entry("z3", z3, "4.16.0")],
    });
    directory.write("tools.json", manifest.to_string().as_bytes())
}

/// A stand-in solver: it prints `version` for `--version`, and otherwise
/// reads the whole script and prints `first` for a plain run and `followup`
/// for the run that asks for a proof or a model.
fn stand_in(
    directory: &Directory,
    name: &str,
    version: &str,
    first: &str,
    followup: &str,
) -> (PathBuf, String) {
    let script = format!(
        "#!/bin/sh\n\
         if [ \"$1\" = \"--version\" ]; then printf '%s\\n' '{version}'; exit 0; fi\n\
         input=''\n\
         while IFS= read -r line || [ -n \"$line\" ]; do input=\"$input$line;\"; done\n\
         case \"$input\" in\n\
           *'(get-proof)'*|*'(get-model)'*) printf '{followup}' ;;\n\
           *) printf '{first}' ;;\n\
         esac\n"
    );
    let path = directory.write(name, script.as_bytes());
    fs::set_permissions(&path, fs::Permissions::from_mode(0o700))
        .unwrap_or_else(|error| panic!("make {name} executable: {error}"));
    (path, sha256(script.as_bytes()))
}

/// CVC5 that answers `unsat` with proof steps, and Z3 that answers `sat`
/// with a model that never replays: a planted disagreement.
fn disagreeing(directory: &Directory) -> PathBuf {
    let cvc5 = stand_in(
        directory,
        "cvc5",
        "This is cvc5 version 1.3.3",
        "unsat\\n",
        "unsat\\n(step t0 :rule refl)\\n",
    );
    let z3 = stand_in(
        directory,
        "z3",
        "Z3 version 4.16.0 - 64 bit",
        "sat\\n",
        "sat\\n(\\n(define-fun x0 () Int 0)\\n)\\n",
    );
    manifest(directory, (&cvc5.0, &cvc5.1), (&z3.0, &z3.1))
}

/// CVC5 and Z3 that answer `unknown`.
fn unknowing(directory: &Directory) -> PathBuf {
    let cvc5 = stand_in(
        directory,
        "cvc5",
        "This is cvc5 version 1.3.3",
        "unknown\\n",
        "unknown\\n",
    );
    let z3 = stand_in(
        directory,
        "z3",
        "Z3 version 4.16.0 - 64 bit",
        "unknown\\n",
        "unknown\\n",
    );
    manifest(directory, (&cvc5.0, &cvc5.1), (&z3.0, &z3.1))
}

/// The pinned solvers, admitted only at their pinned hashes.
fn pinned(directory: &Directory) -> PathBuf {
    let path = |variable: &str| {
        PathBuf::from(
            std::env::var_os(variable)
                .unwrap_or_else(|| panic!("{variable} must name the pinned solver for this test")),
        )
    };
    manifest(
        directory,
        (&path("ZENO_FCIS_CVC5"), CVC5_SHA256),
        (&path("ZENO_FCIS_Z3"), Z3_SHA256),
    )
}

fn summary(report: &Value) -> (u64, u64, u64) {
    let count = |key: &str| report["summary"][key].as_u64().unwrap_or(u64::MAX);
    (count("holds"), count("refuted"), count("inconclusive"))
}

fn case<'r>(report: &'r Value, rule: &str) -> &'r Value {
    report["cases"]
        .as_array()
        .and_then(|cases| cases.iter().find(|case| case["rule"] == rule))
        .unwrap_or_else(|| panic!("no case {rule}"))
}

fn input(witness: &Value, name: &str) -> i64 {
    witness["input"]
        .as_array()
        .and_then(|input| input.iter().find(|value| value["name"] == name))
        .and_then(|value| value["value"].as_str())
        .and_then(|value| value.parse().ok())
        .unwrap_or_else(|| panic!("no {name} in {witness}"))
}

/// Prints each query's solver times, for the acceptance record.
fn print_times(label: &str, report: &Value) {
    let times = |solvers: &Value| {
        ["cvc5", "z3"]
            .map(|solver| {
                format!(
                    "{solver} {} {}ms",
                    solvers[solver]["answer"].as_str().unwrap_or("-"),
                    solvers[solver]["ms"]
                )
            })
            .join(", ")
    };
    println!("{label}: premise {}", times(&report["premise"]["solvers"]));
    for case in report["cases"].as_array().into_iter().flatten() {
        println!(
            "{label}: case {} \"{}\" reached: {}",
            case["case"],
            case["rule"].as_str().unwrap_or(""),
            times(&case["reached"]["solvers"])
        );
        for target in case["targets"].as_array().into_iter().flatten() {
            println!(
                "{label}:   {} {}: {}",
                target["target"].as_str().unwrap_or(""),
                target["status"].as_str().unwrap_or(""),
                times(&target["symbolic"]["solvers"])
            );
        }
    }
}

#[test]
fn small_domains_are_decided_by_enumeration_without_solvers() {
    let spend = fixtures().join("spend-approval");
    let checked = check_symbolic(&spend, None, &[]);
    assert_eq!(checked.exit, Some(0), "{}", checked.stderr);
    let report = checked.json();
    assert_eq!(report["schema"], "zeno-fcis/symbolic-check/1");
    assert_eq!(report["status"], "holds");
    assert_eq!(report["evidence"], "proved");
    assert_eq!(report["authority"], "none");
    assert_eq!(report["domain"]["route"], "exhaustive-and-symbolic");
    assert_eq!(summary(&report), (6, 0, 0));
    assert_eq!(report["solvers"], json!({"cvc5": null, "z3": null}));

    let directory = Directory::new();
    let planted = directory.contract(
        "planted",
        "spend-approval",
        &[(
            "v2/policy.json",
            "\"action == 172 && tier >= 1 && !cfo_ok\"",
            "\"action == 172 && tier >= 2 && !cfo_ok\"",
        )],
    );
    let checked = check_symbolic(&planted, None, &[]);
    assert_eq!(checked.exit, Some(1), "{}", checked.stderr);
    let report = checked.json();
    assert_eq!(report["status"], "refuted");
    assert_eq!(report["evidence"], "checked");
    let target = &case(&report, "execute: the payment is sent")["targets"][0];
    assert_eq!(target["status"], "refuted");
    assert_eq!(target["counterexample"]["route"], "exhaustive");
    assert!(
        checked
            .stderr
            .contains("refuted: case 16 \"execute: the payment is sent\" breaks law 500"),
        "{}",
        checked.stderr
    );
}

#[test]
fn a_large_domain_without_solvers_is_inconclusive() {
    let checked = check_symbolic(&fixtures().join("escrow"), None, &[]);
    assert_eq!(checked.exit, Some(2), "{}", checked.stderr);
    let report = checked.json();
    assert_eq!(report["status"], "inconclusive");
    assert_eq!(report["evidence"], "none");
    assert_eq!(report["domain"]["route"], "symbolic");
    assert_eq!(report["domain"]["size"], Value::Null);
    assert_eq!(summary(&report), (0, 0, 18));
}

/// The escrow's time range holds 4.1 billion values. Listing it once would
/// need 32.8 GB; an earlier build did so to pick one genesis input and was
/// killed. Under a 1 GiB address-space limit the check must still finish
/// with its normal no-solver status and write its report: the limit is the
/// assertion.
#[test]
fn a_wide_declared_range_is_never_listed() {
    let directory = Directory::new();
    let report = directory.path("report.json");
    let output = Command::new("sh")
        .arg("-c")
        .arg("ulimit -v 1048576; exec \"$0\" \"$@\"")
        .arg(CLI)
        .args(["contract", "check-symbolic"])
        .arg(fixtures().join("escrow"))
        .arg("--out")
        .arg(&report)
        .output()
        .unwrap_or_else(|error| panic!("run zeno-fcis under ulimit: {error}"));
    assert_eq!(output.status.code(), Some(2), "{output:?}");
    let written: Value = serde_json::from_slice(
        &fs::read(&report).unwrap_or_else(|error| panic!("read the report: {error}")),
    )
    .unwrap_or_else(|error| panic!("report JSON: {error}"));
    assert_eq!(written["status"], "inconclusive");
    assert_eq!(written["genesis"][0]["genesis"], "satisfied");
}

#[test]
fn a_planted_solver_disagreement_is_inconclusive() {
    let directory = Directory::new();
    let tools = disagreeing(&directory);
    let queries = directory.path("queries");
    let checked = check_symbolic(
        &fixtures().join("spend-approval"),
        Some(&tools),
        &[
            "--max-tuples",
            "0",
            "--queries",
            queries.to_str().unwrap_or_default(),
        ],
    );
    assert_eq!(checked.exit, Some(2), "{}", checked.stderr);
    let report = checked.json();
    assert_eq!(report["status"], "inconclusive");
    // Every query is kept with each solver's final output, under its label.
    let kept = |name: &str| fs::read_to_string(queries.join(name)).unwrap_or_default();
    assert!(kept("premise.smt2").starts_with("; zeno-fcis/symbolic-check/1"));
    assert!(kept("case-16-law-500.smt2").ends_with("(check-sat)\n"));
    assert!(kept("case-16-law-500.cvc5.out").contains("(step t0"));
    assert!(kept("case-16-law-500.z3.out").starts_with("sat"));
    let again = check_symbolic(
        &fixtures().join("spend-approval"),
        Some(&tools),
        &[
            "--max-tuples",
            "0",
            "--queries",
            queries.to_str().unwrap_or_default(),
        ],
    );
    assert_eq!(again.exit, Some(2));
    assert!(
        again
            .stderr
            .contains("queries are written only to a new directory"),
        "{}",
        again.stderr
    );
    assert_eq!(report["domain"]["route"], "symbolic");
    assert_eq!(summary(&report), (0, 0, 6));
    assert_eq!(report["summary"]["solver_disagreements"], 6);
    assert!(
        checked.stderr.contains("the solvers disagree"),
        "{}",
        checked.stderr
    );
    for case in report["cases"].as_array().into_iter().flatten() {
        let symbolic = &case["targets"][0]["symbolic"];
        assert_eq!(symbolic["solvers"]["cvc5"]["answer"], "unsat");
        assert_eq!(symbolic["solvers"]["cvc5"]["proof_output"], true);
        assert_eq!(symbolic["solvers"]["z3"]["answer"], "sat");
    }
    assert_eq!(report["solvers"]["cvc5"]["version"], "1.3.3");

    // The same planted solvers on the symbolic transform check.
    let original = directory.path("escrow.zcve");
    export(&fixtures().join("escrow"), &original);
    let checked = transform(
        &original,
        &original,
        &["--symbolic".as_ref(), "--tools".as_ref(), tools.as_os_str()],
    );
    assert_eq!(checked.exit, Some(2), "{}", checked.stdout);
    let report = checked.json();
    assert_eq!(report["status"], "inconclusive");
    assert_eq!(report["mode"], "symbolic");
    assert!(
        report["detail"]["cause"]
            .as_str()
            .is_some_and(|cause| cause.contains("the solvers disagree")
                || cause.contains("the planted control was not refuted")),
        "{report}"
    );
}

#[test]
fn an_unknown_is_inconclusive() {
    let directory = Directory::new();
    let tools = unknowing(&directory);
    let checked = check_symbolic(&fixtures().join("escrow"), Some(&tools), &[]);
    assert_eq!(checked.exit, Some(2), "{}", checked.stderr);
    let report = checked.json();
    assert_eq!(report["status"], "inconclusive");
    assert_eq!(summary(&report), (0, 0, 18));
    assert!(
        checked.stderr.contains("CVC5 answered unknown"),
        "{}",
        checked.stderr
    );
}

#[test]
fn solvers_must_pass_admission_and_symbolic_flags_are_checked() {
    let directory = Directory::new();
    let cvc5 = stand_in(
        &directory,
        "cvc5",
        "This is cvc5 version 1.3.3",
        "unsat\\n",
        "unsat\\n",
    );
    let z3 = stand_in(
        &directory,
        "z3",
        "Z3 version 4.16.0 - 64 bit",
        "unsat\\n",
        "unsat\\n",
    );
    let tampered = manifest(&directory, (&cvc5.0, &"0".repeat(64)), (&z3.0, &z3.1));
    let unused = directory.path("unused.json");
    let checked = check_symbolic(
        &fixtures().join("escrow"),
        Some(&tampered),
        &[
            "--format",
            "json",
            "--out",
            unused.to_str().unwrap_or_default(),
        ],
    );
    assert_eq!(checked.exit, Some(2), "{}", checked.stderr);
    let report = checked.json();
    assert_eq!(report["error"]["code"], "solvers-blocked");
    assert!(
        report["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("HashMismatch")),
        "{report}"
    );
    assert!(!unused.exists());

    let program = directory.path("spend.zcve");
    export(&fixtures().join("spend-approval"), &program);
    // --symbolic needs --tools, and the exhaustive receipt flag is not one
    // of its flags.
    let usage = transform(&program, &program, &["--symbolic".as_ref()]);
    assert_eq!(usage.exit, Some(64));
    let usage = transform(
        &program,
        &program,
        &[
            "--symbolic".as_ref(),
            "--tools".as_ref(),
            tampered.as_os_str(),
            "--receipt".as_ref(),
            directory.path("r.json").as_os_str(),
        ],
    );
    assert_eq!(usage.exit, Some(64));
    // A domain the exhaustive check fits is refused: enumeration decides it.
    let honest = manifest(&directory, (&cvc5.0, &cvc5.1), (&z3.0, &z3.1));
    let refused = transform(
        &program,
        &program,
        &[
            "--symbolic".as_ref(),
            "--tools".as_ref(),
            honest.as_os_str(),
        ],
    );
    assert_eq!(refused.exit, Some(1), "{}", refused.stdout);
    let report = refused.json();
    assert_eq!(report["status"], "refused");
    assert_eq!(report["detail"]["reason"], "exhaustive-check-fits");
}

#[test]
fn describe_declares_the_symbolic_effects() {
    let described = run(&[
        "describe".as_ref(),
        "contract".as_ref(),
        "check-symbolic".as_ref(),
    ]);
    assert_eq!(described.exit, Some(0));
    assert_eq!(
        described.json()["command"]["effects"],
        json!({
            "classification": "declared", "executes_tools": true, "read_only_flag": null,
            "reads": ["application-contract", "optional-strengthening", "optional-tools-manifest", "optional-toolchain-files"],
            "writes": ["optional-symbolic-report", "optional-query-directory", "temporary-files"]
        })
    );
}

#[test]
#[ignore = "requires the workflow-pinned CVC5 and Z3 executables"]
fn pinned_symbolic_escrow_conservation_holds_with_its_strengthening() {
    let directory = Directory::new();
    let tools = pinned(&directory);
    let strengthening = directory.write("strengthening.json", CREATED_PAYS_NOTHING.as_bytes());
    let checked = check_symbolic(
        &fixtures().join("escrow"),
        Some(&tools),
        &[
            "--strengthening",
            strengthening.to_str().unwrap_or_default(),
        ],
    );
    println!("{}", checked.stderr);
    assert_eq!(checked.exit, Some(2), "{}", checked.stderr);
    let report = checked.json();
    print_times("escrow+strengthening", &report);
    assert_eq!(report["status"], "holds");
    assert_eq!(report["evidence"], "attested");
    assert_eq!(report["domain"]["route"], "symbolic");
    assert_eq!(summary(&report), (27, 0, 0));
    assert_eq!(report["summary"]["solver_disagreements"], 0);
    assert_eq!(report["causes"], json!([]));
    assert_eq!(report["solvers"]["cvc5"]["sha256"], CVC5_SHA256);
    assert_eq!(report["solvers"]["z3"]["sha256"], Z3_SHA256);
    // Every planted control was refuted with a replayed model.
    assert_eq!(report["premise"]["status"], "sat");
    assert_eq!(report["premise"]["evidence"], "checked");
    for control in report["domain_controls"].as_array().into_iter().flatten() {
        assert_eq!(control["status"], "substantive", "{control}");
    }
    let cases = report["cases"].as_array().map_or(0, Vec::len);
    assert_eq!(cases, 9);
    for case in report["cases"].as_array().into_iter().flatten() {
        assert_eq!(case["reached"]["status"], "reached", "{case}");
        assert_eq!(case["reached"]["evidence"], "checked", "{case}");
        for target in case["targets"].as_array().into_iter().flatten() {
            assert_eq!(target["status"], "holds", "{target}");
            assert_eq!(target["evidence"], "attested", "{target}");
            assert_eq!(target["symbolic"]["status"], "unsat", "{target}");
            assert_eq!(target["symbolic"]["corroborated_by_z3"], true, "{target}");
            assert_eq!(
                target["symbolic"]["solvers"]["cvc5"]["proof_output"], true,
                "{target}"
            );
        }
    }
}

#[test]
#[ignore = "requires the workflow-pinned CVC5 and Z3 executables"]
fn pinned_symbolic_escrow_conservation_alone_is_refuted_on_fund() {
    let directory = Directory::new();
    let tools = pinned(&directory);
    let checked = check_symbolic(&fixtures().join("escrow"), Some(&tools), &[]);
    println!("{}", checked.stderr);
    assert_eq!(checked.exit, Some(1), "{}", checked.stderr);
    let report = checked.json();
    print_times("escrow-law-500-alone", &report);
    assert_eq!(report["status"], "refuted");
    assert_eq!(report["evidence"], "checked");
    assert_eq!(summary(&report), (17, 1, 0));
    let fund = case(&report, "fund: the buyer funds the escrow");
    let target = &fund["targets"][0];
    assert_eq!(target["target"], "law 500");
    assert_eq!(target["status"], "refuted");
    assert_eq!(target["evidence"], "checked");
    // The replayed counterexample: a lawful Created state that already paid
    // something out, funded by the buyer, breaks conservation; the library
    // Authority refuses the decision by law 500.
    let witness = &target["counterexample"]["witness"];
    assert_eq!(target["counterexample"]["route"], "symbolic");
    assert_eq!(input(witness, "pre.100.110"), 150);
    assert!(
        input(witness, "pre.100.113") + input(witness, "pre.100.114") > 0,
        "{witness}"
    );
    assert_eq!(input(witness, "command.101.120"), 170);
    assert_eq!(input(witness, "context.102.130"), 161);
    assert_eq!(witness["target_on_successor"], "violated");
    assert!(
        witness["authority"]
            .as_str()
            .is_some_and(|text| text.contains("by law 500")),
        "{witness}"
    );
    println!("replayed counterexample: {witness}");
}

#[test]
#[ignore = "requires the workflow-pinned CVC5 and Z3 executables"]
fn pinned_symbolic_spend_approval_planted_bug_is_refuted() {
    let directory = Directory::new();
    let tools = pinned(&directory);
    let planted = directory.contract(
        "planted",
        "spend-approval",
        &[(
            "v2/policy.json",
            "\"action == 172 && tier >= 1 && !cfo_ok\"",
            "\"action == 172 && tier >= 2 && !cfo_ok\"",
        )],
    );
    // Enumeration decides this domain; the solver route must agree with it,
    // and on its own it refutes the planted bug too.
    for max_tuples in ["1048576", "0"] {
        let checked = check_symbolic(&planted, Some(&tools), &["--max-tuples", max_tuples]);
        println!("{}", checked.stderr);
        assert_eq!(checked.exit, Some(1), "{}", checked.stderr);
        let report = checked.json();
        print_times(&format!("spend-planted-max-tuples-{max_tuples}"), &report);
        assert_eq!(report["status"], "refuted");
        assert_eq!(summary(&report), (5, 1, 0));
        assert_eq!(report["summary"]["solver_disagreements"], 0);
        let target = &case(&report, "execute: the payment is sent")["targets"][0];
        assert_eq!(target["status"], "refuted");
        assert_eq!(target["symbolic"]["status"], "sat");
        let witness = &target["symbolic"]["replayed"];
        assert_eq!(input(witness, "pre.100.120"), 151);
        assert_eq!(input(witness, "pre.100.121"), 1);
        assert_eq!(input(witness, "pre.100.122"), 0);
        println!("replayed counterexample: {witness}");
    }
    let clean = check_symbolic(&fixtures().join("spend-approval"), Some(&tools), &[]);
    assert_eq!(clean.exit, Some(0), "{}", clean.stderr);
    let report = clean.json();
    assert_eq!(report["evidence"], "proved");
    for case in report["cases"].as_array().into_iter().flatten() {
        assert_eq!(case["targets"][0]["symbolic"]["status"], "unsat", "{case}");
    }
}

/// One text replacement in a contract file: the file, the text, its
/// replacement.
type Replacement<'a> = (&'a str, &'a str, &'a str);

/// The escrow's own rules, an equivalent rewrite (`amount == 0` becomes
/// `amount < 1` on a domain of non-negative amounts), and a planted
/// non-equivalent one (`refund > held` becomes `refund >= held`), as
/// exported decision programs.
fn escrow_programs(directory: &Directory, reduced: bool) -> [PathBuf; 3] {
    let narrowing: &[(&str, &str, &str)] = if reduced {
        &[
            (
                "project.zeno",
                "type 108 int Money in 0..=100000000;",
                "type 108 int Money in 0..=1;",
            ),
            (
                "project.zeno",
                "type 109 int UnixTime in 0..=4102444800;",
                "type 109 int UnixTime in 0..=0;",
            ),
        ]
    } else {
        &[]
    };
    let prefix = if reduced { "reduced-" } else { "" };
    let variants: [(&str, Option<Replacement<'_>>); 3] = [
        ("original", None),
        (
            "rewritten",
            Some((
                "v2/policy.json",
                "\"action == 170 && amount == 0\"",
                "\"action == 170 && amount < 1\"",
            )),
        ),
        (
            "planted",
            Some((
                "v2/policy.json",
                "\"action == 174 && refund > held\"",
                "\"action == 174 && refund >= held\"",
            )),
        ),
    ];
    variants.map(|(name, change)| {
        let mut replacements = narrowing.to_vec();
        replacements.extend(change);
        let dir = directory.contract(&format!("{prefix}{name}"), "escrow", &replacements);
        let program = directory.path(&format!("{prefix}{name}.zcve"));
        export(&dir, &program);
        program
    })
}

fn print_pieces(label: &str, report: &Value) {
    let queries = &report["detail"]["queries"];
    for piece in queries["pieces"].as_array().into_iter().flatten() {
        println!(
            "{label}: {} {} cvc5 {} {}ms, z3 {} {}ms",
            piece["piece"].as_str().unwrap_or(""),
            piece["status"].as_str().unwrap_or(""),
            piece["solvers"]["cvc5"]["answer"].as_str().unwrap_or("-"),
            piece["solvers"]["cvc5"]["ms"],
            piece["solvers"]["z3"]["answer"].as_str().unwrap_or("-"),
            piece["solvers"]["z3"]["ms"],
        );
    }
    println!(
        "{label}: control {}",
        queries["control"]["result"]["status"]
    );
}

#[test]
#[ignore = "requires the workflow-pinned CVC5 and Z3 executables"]
fn pinned_symbolic_transform_on_the_escrow_attests_a_rewrite_and_refutes_a_planted_candidate() {
    let directory = Directory::new();
    let tools = pinned(&directory);
    let [original, rewritten, planted] = escrow_programs(&directory, false);
    let receipt = directory.path("symbolic-receipt.json");
    let symbolic = |candidate: &Path, receipt: Option<&Path>| {
        let mut extra: Vec<&std::ffi::OsStr> =
            vec!["--symbolic".as_ref(), "--tools".as_ref(), tools.as_os_str()];
        if let Some(receipt) = receipt {
            extra.extend(["--symbolic-receipt".as_ref(), receipt.as_os_str()]);
        }
        transform(&original, candidate, &extra)
    };
    // The escrow's domain is far above the exhaustive cap.
    let exhaustive = transform(&original, &rewritten, &[]);
    assert_eq!(exhaustive.exit, Some(2), "{}", exhaustive.stdout);
    assert_eq!(exhaustive.json()["status"], "inconclusive");

    let attested = symbolic(&rewritten, Some(&receipt));
    assert_eq!(attested.exit, Some(2), "{}", attested.stdout);
    let report = attested.json();
    print_pieces("escrow-rewrite", &report);
    assert_eq!(report["status"], "attested-equivalent");
    assert_eq!(report["detail"]["evidence"], "attested");
    assert_eq!(
        report["detail"]["queries"]["control"]["result"]["status"],
        "refuted"
    );
    for piece in report["detail"]["queries"]["pieces"]
        .as_array()
        .into_iter()
        .flatten()
    {
        assert_eq!(piece["status"], "holds", "{piece}");
        assert_eq!(piece["corroborated_by_z3"], true, "{piece}");
    }
    let written: Value = serde_json::from_slice(
        &fs::read(&receipt).unwrap_or_else(|error| panic!("read the receipt: {error}")),
    )
    .unwrap_or_else(|error| panic!("receipt JSON: {error}"));
    assert_eq!(written["schema"], "zeno-fcis/transform-symbolic-receipt/1");
    assert_eq!(written["evidence"], "attested");
    assert_eq!(written["solvers"]["cvc5"]["sha256"], CVC5_SHA256);

    // No command that needs an exhaustive receipt reads this one.
    let replayed = run(&[
        "transform".as_ref(),
        "replay".as_ref(),
        "--receipt".as_ref(),
        receipt.as_os_str(),
        "--original".as_ref(),
        original.as_os_str(),
        "--candidate".as_ref(),
        rewritten.as_os_str(),
    ]);
    assert_eq!(replayed.exit, Some(1), "{}", replayed.stdout);
    assert_eq!(replayed.json()["status"], "invalid-receipt");
    let app = directory.contract("adopting", "escrow", &[]);
    let before = fs::read(app.join("v2/policy.json")).unwrap_or_default();
    let adopted = run(&[
        "contract".as_ref(),
        "adopt".as_ref(),
        app.as_os_str(),
        "--candidate".as_ref(),
        rewritten.as_os_str(),
        "--receipt".as_ref(),
        receipt.as_os_str(),
        "--usage".as_ref(),
        "new-version".as_ref(),
        "--format".as_ref(),
        "json".as_ref(),
    ]);
    assert_eq!(adopted.exit, Some(1), "{}", adopted.stdout);
    assert!(
        adopted.stdout.contains("not a transform receipt"),
        "{}",
        adopted.stdout
    );
    assert_eq!(
        fs::read(app.join("v2/policy.json")).unwrap_or_default(),
        before
    );
    assert!(!app.join("v2/adoptions").exists());

    let refuted = symbolic(&planted, None);
    assert_eq!(refuted.exit, Some(1), "{}", refuted.stdout);
    let report = refuted.json();
    print_pieces("escrow-planted", &report);
    assert_eq!(report["status"], "counterexample");
    assert_eq!(report["detail"]["evidence"], "checked");
    // A disputed escrow resolved by the arbiter with refund == held: the
    // original pays the buyer in full, the planted candidate rejects.
    let replayed = &report["detail"]["replayed"];
    println!("replayed counterexample: {replayed}");
    let values = |side: &str| replayed[side]["ok"].clone();
    assert_ne!(values("original"), values("candidate"));
    assert_eq!(replayed["input"][0], "153");
    assert_eq!(replayed["input"][7], "174");
}

#[test]
#[ignore = "requires the workflow-pinned CVC5 and Z3 executables"]
fn pinned_symbolic_transform_on_a_reduced_escrow_agrees_with_the_exhaustive_check() {
    let directory = Directory::new();
    let tools = pinned(&directory);
    let [original, rewritten, planted] = escrow_programs(&directory, true);
    for (candidate, equivalent) in [(&rewritten, true), (&planted, false)] {
        let exhaustive = transform(&original, candidate, &[]);
        let symbolic = transform(
            &original,
            candidate,
            &[
                "--symbolic".as_ref(),
                "--tools".as_ref(),
                tools.as_os_str(),
                "--max-input-tuples".as_ref(),
                "1000".as_ref(),
            ],
        );
        let (exhaustive, symbolic) = (exhaustive.json(), symbolic.json());
        print_pieces(
            &format!(
                "reduced-escrow-{}",
                if equivalent { "rewrite" } else { "planted" }
            ),
            &symbolic,
        );
        if equivalent {
            assert_eq!(exhaustive["status"], "equivalent", "{exhaustive}");
            assert_eq!(symbolic["status"], "attested-equivalent", "{symbolic}");
        } else {
            assert_eq!(exhaustive["status"], "counterexample", "{exhaustive}");
            assert_eq!(symbolic["status"], "counterexample", "{symbolic}");
        }
    }
}

#[test]
#[ignore = "requires the workflow-pinned CVC5 and Z3 executables"]
fn pinned_symbolic_real_cvc5_with_a_disagreeing_z3_is_inconclusive() {
    let directory = Directory::new();
    let cvc5 = PathBuf::from(
        std::env::var_os("ZENO_FCIS_CVC5").unwrap_or_else(|| panic!("ZENO_FCIS_CVC5 must be set")),
    );
    let z3 = stand_in(
        &directory,
        "z3",
        "Z3 version 4.16.0 - 64 bit",
        "sat\\n",
        "sat\\n(\\n(define-fun x0 () Int 0)\\n)\\n",
    );
    let tools = manifest(&directory, (&cvc5, CVC5_SHA256), (&z3.0, &z3.1));
    let strengthening = directory.write("strengthening.json", CREATED_PAYS_NOTHING.as_bytes());
    let checked = check_symbolic(
        &fixtures().join("escrow"),
        Some(&tools),
        &[
            "--strengthening",
            strengthening.to_str().unwrap_or_default(),
        ],
    );
    assert_eq!(checked.exit, Some(2), "{}", checked.stderr);
    let report = checked.json();
    assert_eq!(report["status"], "inconclusive");
    assert_eq!(summary(&report), (0, 0, 27));
    assert_eq!(report["summary"]["solver_disagreements"], 27);
    assert!(
        checked.stderr.contains("the solvers disagree"),
        "{}",
        checked.stderr
    );
}
