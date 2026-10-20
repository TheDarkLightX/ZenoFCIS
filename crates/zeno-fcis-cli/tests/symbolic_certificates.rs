//! Actual full-i128 certificate-command and replay qualification.
//! These tests require the genuine pinned CVC5 and separately built Carcara
//! wrapper. The mandatory CI/acceptance runner fails on missing configuration.
#![cfg(target_os = "linux")]
#![forbid(unsafe_code)]

use serde_json::{Value, json};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use zeno_fcis_codec::{CanonicalEncode as _, CommitmentHasher as _};
use zeno_fcis_crypto::RustCryptoSha256;
use zeno_fcis_formal_tools::certificate::Session;
use zeno_fcis_synthesis::finite::{Domain, Op, Program};

const CLI: &str = env!("CARGO_BIN_EXE_zeno-fcis");
const CVC5_SHA256: &str = "e8d7870d57ab55e81619d2373b043da05ea1d37ca393931bdb5d8b9788cd64c4";
const PROOFS: [&[u8]; 3] = [
    include_bytes!("fixtures/symbolic-certificates/0.alethe"),
    include_bytes!("fixtures/symbolic-certificates/1.alethe"),
    include_bytes!("fixtures/symbolic-certificates/2.alethe"),
];

struct Fixture {
    directory: PathBuf,
    tools: PathBuf,
    checker: PathBuf,
    certificates: PathBuf,
    original: PathBuf,
}

fn sha256(bytes: &[u8]) -> String {
    RustCryptoSha256::hash(bytes)
        .as_bytes()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

impl Fixture {
    fn new() -> Self {
        let cvc5 = PathBuf::from(
            std::env::var_os("ZENO_FCIS_CVC5").unwrap_or_else(|| panic!("pinned CVC5 required")),
        );
        assert_eq!(
            sha256(
                &fs::read(&cvc5)
                    .unwrap_or_else(|error| panic!("certificate qualification: {error:?}"))
            ),
            CVC5_SHA256
        );
        let checker = PathBuf::from(
            std::env::var_os("ZENO_FCIS_CERTIFICATE_CHECKER")
                .unwrap_or_else(|| panic!("pinned certificate-checker manifest required")),
        );
        Session::open(
            &fs::read(&checker)
                .unwrap_or_else(|error| panic!("certificate qualification: {error:?}")),
        )
        .unwrap_or_else(|error| panic!("admit real pinned checker: {error:?}"));
        let directory =
            std::env::temp_dir().join(format!("zfcis-certificate-{}", std::process::id()));
        fs::create_dir(&directory)
            .unwrap_or_else(|error| panic!("certificate qualification: {error:?}"));
        let tools = directory.join("tools.json");
        fs::write(
            &tools,
            serde_json::to_vec(&json!({
                "format": "zeno-fcis/tools/2",
                "tools": [{"backend": "cvc5", "path": cvc5, "version": "1.3.3",
                    "sha256": CVC5_SHA256, "timeout_ms": 10000,
                    "max_output_bytes": 33554432, "allowed_axioms": []}]
            }))
            .unwrap_or_else(|error| panic!("certificate qualification: {error:?}")),
        )
        .unwrap_or_else(|error| panic!("certificate qualification: {error:?}"));
        let certificates = directory.join("certificates");
        fs::create_dir(&certificates)
            .unwrap_or_else(|error| panic!("certificate qualification: {error:?}"));
        for (index, proof) in PROOFS.iter().enumerate() {
            fs::write(certificates.join(format!("{index}.alethe")), proof)
                .unwrap_or_else(|error| panic!("certificate qualification: {error:?}"));
        }
        let original = directory.join("identity.zcve");
        Self::program(
            &original,
            Domain::Int {
                min: i128::MIN,
                max: i128::MAX,
            },
            vec![Op::Input(0)],
        );
        Self {
            directory,
            tools,
            checker,
            certificates,
            original,
        }
    }

    fn program(path: &Path, domain: Domain, nodes: Vec<Op>) {
        let program = Program::try_new(vec![domain], vec![domain], nodes, vec![0])
            .unwrap_or_else(|error| panic!("certificate qualification: {error:?}"));
        fs::write(
            path,
            program
                .value()
                .unwrap_or_else(|error| panic!("certificate qualification: {error:?}"))
                .canonical_bytes()
                .unwrap_or_else(|error| panic!("certificate qualification: {error:?}")),
        )
        .unwrap_or_else(|error| panic!("certificate qualification: {error:?}"));
    }

    fn run(
        &self,
        command: &str,
        original: &Path,
        candidate: &Path,
        extra: &[&str],
    ) -> (i32, Value) {
        let output = Command::new(CLI)
            .args(["transform", command, "--original"])
            .arg(original)
            .arg("--candidate")
            .arg(candidate)
            .arg("--tools")
            .arg(&self.tools)
            .arg("--checker")
            .arg(&self.checker)
            .arg("--certificates")
            .arg(&self.certificates)
            .args(["--max-input-tuples", "256"])
            .args(extra)
            .output()
            .unwrap_or_else(|error| panic!("certificate qualification: {error:?}"));
        assert!(output.stderr.is_empty(), "{output:?}");
        (
            output
                .status
                .code()
                .unwrap_or_else(|| panic!("ordinary CLI exit required")),
            serde_json::from_slice(&output.stdout)
                .unwrap_or_else(|error| panic!("certificate qualification: {error:?}")),
        )
    }

    fn refuses(&self, original: &Path, candidate: &Path, status: &str, name: &str) {
        let target = self.directory.join(name);
        let (exit, report) = self.run(
            "check-certificates",
            original,
            candidate,
            &[
                "--out",
                target
                    .to_str()
                    .unwrap_or_else(|| panic!("UTF-8 qualification path required")),
            ],
        );
        assert_ne!(exit, 0, "{report}");
        assert_eq!(report["status"], status, "{report}");
        assert!(!target.exists(), "refusal exposed a receipt");
        eprintln!("certificate negative {name}: {report}");
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        // A failed qualification retains the concrete input/proof files.
        if !std::thread::panicking() {
            fs::remove_dir_all(&self.directory)
                .unwrap_or_else(|error| panic!("certificate qualification: {error:?}"));
        }
    }
}

#[test]
#[ignore = "requires pinned CVC5 and independently built certificate checker"]
fn pinned_certificate_command_checks_full_i128_replays_and_refuses_controls() {
    let fixture = Fixture::new();
    let receipt = fixture.directory.join("receipt.json");
    let (exit, report) = fixture.run(
        "check-certificates",
        &fixture.original,
        &fixture.original,
        &[
            "--out",
            receipt
                .to_str()
                .unwrap_or_else(|| panic!("UTF-8 qualification path required")),
        ],
    );
    assert_eq!(exit, 0, "{report}");
    assert_eq!(report["status"], "certificate-checked-equivalent");
    let bytes =
        fs::read(&receipt).unwrap_or_else(|error| panic!("certificate qualification: {error:?}"));
    let recorded: Value = serde_json::from_slice(&bytes)
        .unwrap_or_else(|error| panic!("certificate qualification: {error:?}"));
    assert_eq!(
        recorded["schema"],
        "zeno-fcis/transform-certificate-receipt/1"
    );
    assert_eq!(recorded["evidence"], "CarcaraChecked");
    assert_eq!(
        recorded["certificate_checker"]["build_inputs_status"],
        "claimed-build-inputs"
    );
    assert_eq!(
        recorded["certificate_checker"]["build_provenance_verified"],
        false
    );
    let full_i128 = json!([{
        "kind": "int", "min": i128::MIN.to_string(), "max": i128::MAX.to_string()
    }]);
    assert_eq!(recorded["domain"]["inputs"], full_i128);
    assert_eq!(recorded["outputs"], full_i128);
    // The full domain has 2^128 values, beyond the u128 cardinality field.
    assert_eq!(recorded["domain"]["size"], Value::Null);
    assert_eq!(
        recorded["certificates"]
            .as_array()
            .unwrap_or_else(|| panic!("complete certificate array required"))
            .len(),
        3
    );
    assert_eq!(
        recorded["compiled_checker_command"]["compiled_application"],
        Value::Null
    );
    assert_eq!(report["detail"]["receipt_sha256"], sha256(&bytes));
    let (exit, report) = fixture.run(
        "replay-certificates",
        &fixture.original,
        &fixture.original,
        &[
            "--receipt",
            receipt
                .to_str()
                .unwrap_or_else(|| panic!("UTF-8 qualification path required")),
        ],
    );
    assert_eq!(exit, 0, "{report}");
    assert_eq!(report["status"], "certificate-replayed");
    assert_eq!(
        fs::read(&receipt).unwrap_or_else(|error| panic!("certificate qualification: {error:?}")),
        bytes
    );
    eprintln!(
        "certificate positive full-i128 and byte-identical replay: {}",
        sha256(&bytes)
    );

    let wide = Domain::Int {
        min: i128::MIN,
        max: i128::MAX,
    };
    // A genuine non-identical candidate: eliminate an unused constant node.
    // Eager execution of the constant cannot fail, and the nonbinding Step
    // limit covers both graphs. Resource-usage equality is not claimed.
    let equivalent = fixture.directory.join("unused-constant.zcve");
    Fixture::program(&equivalent, wide, vec![Op::Input(0), Op::Int(0)]);
    assert_ne!(fs::read(&fixture.original).ok(), fs::read(&equivalent).ok());
    let equivalent_receipt = fixture.directory.join("non-identical-receipt.json");
    let receipt_path = equivalent_receipt
        .to_str()
        .unwrap_or_else(|| panic!("UTF-8 qualification path required"));
    let (exit, report) = fixture.run(
        "check-certificates",
        &equivalent,
        &fixture.original,
        &["--out", receipt_path],
    );
    assert_eq!(exit, 0, "non-identical equivalent candidate: {report}");
    assert_eq!(report["status"], "certificate-checked-equivalent");
    let (exit, report) = fixture.run(
        "replay-certificates",
        &equivalent,
        &fixture.original,
        &["--receipt", receipt_path],
    );
    assert_eq!(exit, 0, "non-identical candidate replay: {report}");
    assert_eq!(report["status"], "certificate-replayed");
    eprintln!(
        "certificate positive non-identical unused-constant elimination and replay: {}",
        sha256(
            &fs::read(&equivalent_receipt)
                .unwrap_or_else(|error| panic!("certificate qualification: {error:?}"))
        )
    );

    let different = fixture.directory.join("different.zcve");
    Fixture::program(&different, wide, vec![Op::Int(0)]);
    fixture.refuses(
        &fixture.original,
        &different,
        "counterexample",
        "wrong-output.json",
    );
    Fixture::program(
        &different,
        wide,
        vec![Op::Input(0), Op::Int(i128::MAX), Op::Add(0, 1)],
    );
    fixture.refuses(
        &fixture.original,
        &different,
        "counterexample",
        "eager-overflow.json",
    );
    Fixture::program(
        &different,
        Domain::Int {
            min: i128::MIN + 1,
            max: i128::MAX,
        },
        vec![Op::Input(0)],
    );
    fixture.refuses(
        &fixture.original,
        &different,
        "refused",
        "different-abi.json",
    );
    fixture.refuses(
        &different,
        &different,
        "certificate-check-incomplete",
        "changed-both-bounds.json",
    );

    let proof = std::str::from_utf8(PROOFS[0])
        .unwrap_or_else(|error| panic!("certificate qualification: {error:?}"));
    assert!(proof.contains(":rule resolution"));
    fs::write(
        fixture.certificates.join("0.alethe"),
        proof.replacen(":rule resolution", ":rule invented", 1),
    )
    .unwrap_or_else(|error| panic!("certificate qualification: {error:?}"));
    fixture.refuses(
        &fixture.original,
        &fixture.original,
        "certificate-check-incomplete",
        "corrupt-proof.json",
    );
    fs::write(fixture.certificates.join("0.alethe"), PROOFS[0])
        .unwrap_or_else(|error| panic!("certificate qualification: {error:?}"));
    fs::remove_file(fixture.certificates.join("2.alethe"))
        .unwrap_or_else(|error| panic!("certificate qualification: {error:?}"));
    fixture.refuses(
        &fixture.original,
        &fixture.original,
        "certificate-check-incomplete",
        "missing-proof.json",
    );
    fs::write(fixture.certificates.join("2.alethe"), PROOFS[2])
        .unwrap_or_else(|error| panic!("certificate qualification: {error:?}"));

    let tampered = fixture.directory.join("tampered.json");
    let mut changed = recorded;
    changed["domain"]["inputs"][0]["min"] = json!("0");
    fs::write(
        &tampered,
        serde_json::to_vec(&changed)
            .unwrap_or_else(|error| panic!("certificate qualification: {error:?}")),
    )
    .unwrap_or_else(|error| panic!("certificate qualification: {error:?}"));
    let (exit, report) = fixture.run(
        "replay-certificates",
        &fixture.original,
        &fixture.original,
        &[
            "--receipt",
            tampered
                .to_str()
                .unwrap_or_else(|| panic!("UTF-8 qualification path required")),
        ],
    );
    assert_eq!(exit, 1, "{report}");
    assert_eq!(report["status"], "replay-mismatch");
    let output = Command::new(CLI)
        .args(["transform", "replay", "--receipt"])
        .arg(&receipt)
        .arg("--original")
        .arg(&fixture.original)
        .arg("--candidate")
        .arg(&fixture.original)
        .output()
        .unwrap_or_else(|error| panic!("certificate qualification: {error:?}"));
    assert_ne!(
        output.status.code(),
        Some(0),
        "checked receipt entered exhaustive replay"
    );
    eprintln!(
        "certificate negatives: corrupt/missing proof, receipt tamper and legacy replay refused"
    );
}
