//! Complete certificate checks for the existing symbolic comparison.
//! The solver, proposer and receipt never decide certificate acceptance.
use super::{
    Limits, Outcome, Path, Solvers, Value, equivalence, json, read_bounded, read_programs, result,
    transform,
};
use std::path::PathBuf;
use zeno_fcis_formal_tools::certificate::Session;

const SCHEMA: &str = "zeno-fcis/transform-certificate-receipt/1";
const CERTIFICATE_LIMIT: u64 = 33_554_432;
// Up to 258 checked pieces carry more evidence than the legacy receipt. Use
// the same bounded format on creation and replay; preserve the legacy cap.
const CHECKED_RECEIPT_LIMIT: u64 = 1_048_576;

fn receipt_bytes(receipt: &Value) -> Result<Vec<u8>, (u8, Value)> {
    let bytes = transform::canonical_json(receipt).into_bytes();
    if super::over(&bytes, CHECKED_RECEIPT_LIMIT) {
        return Err(blocked("checked receipt exceeds its supported size"));
    }
    Ok(bytes)
}

fn bounded(path: &Path, cap: u64) -> Result<Vec<u8>, String> {
    let metadata = path.symlink_metadata().map_err(|error| error.to_string())?;
    if !metadata.is_file() || metadata.len() > cap {
        return Err("expected a bounded regular file".into());
    }
    let bytes = read_bounded(path, cap).map_err(|error| error.to_string())?;
    if super::over(&bytes, cap) {
        return Err("file grew past its cap".into());
    }
    Ok(bytes)
}

fn blocked(message: impl ToString) -> (u8, Value) {
    result(
        crate::BLOCKED,
        "certificate-check-incomplete",
        json!({"message": message.to_string(), "evidence": "none"}),
    )
}

#[allow(clippy::too_many_arguments)]
pub(super) fn check(
    original: &Path,
    candidate: &Path,
    limits: Limits,
    tools: &Path,
    checker: &Path,
    certificates: &Path,
    output: Option<&Path>,
    queries: Option<&Path>,
) -> (u8, Value) {
    if output.is_some_and(|path| path.symlink_metadata().is_ok()) {
        return result(crate::INVALID, "receipt-exists", json!({}));
    }
    match recompute(
        original,
        candidate,
        limits,
        tools,
        checker,
        certificates,
        queries,
    ) {
        Ok(receipt) => {
            let bytes = match receipt_bytes(&receipt) {
                Ok(bytes) => bytes,
                Err(error) => return error,
            };
            if let Some(path) = output
                && let Err(error) = crate::atomic_create(path, &bytes)
            {
                return super::io_failure(&error);
            }
            result(
                crate::OK,
                "certificate-checked-equivalent",
                json!({
                    "evidence": "CarcaraChecked",
                    "receipt": receipt,
                    "receipt_sha256": transform::sha256_hex(&bytes),
                    "receipt_path": output.map(|p| p.display().to_string()),
                }),
            )
        }
        Err(error) => error,
    }
}

pub(super) fn replay(
    receipt: &Path,
    original: &Path,
    candidate: &Path,
    max_input_tuples: u64,
    tools: &Path,
    checker: &Path,
    certificates: &Path,
) -> (u8, Value) {
    let recorded = match bounded(receipt, CHECKED_RECEIPT_LIMIT) {
        Ok(bytes) => bytes,
        Err(error) => return blocked(error),
    };
    let value: Value = match serde_json::from_slice(&recorded) {
        Ok(value) => value,
        Err(error) => return blocked(error),
    };
    let Some(steps) = value["limits"]["steps"].as_u64() else {
        return result(crate::INVALID, "invalid-receipt", json!({}));
    };
    if value["schema"] != SCHEMA || value["evidence"] != "CarcaraChecked" {
        return result(crate::INVALID, "invalid-receipt", json!({}));
    }
    match recompute(
        original,
        candidate,
        Limits {
            steps,
            input_tuples: max_input_tuples,
        },
        tools,
        checker,
        certificates,
        None,
    ) {
        Ok(recomputed) => {
            let bytes = match receipt_bytes(&recomputed) {
                Ok(bytes) => bytes,
                Err(error) => return error,
            };
            if recorded != bytes {
                return result(crate::INVALID, "replay-mismatch", json!({}));
            }
            result(
                crate::OK,
                "certificate-replayed",
                json!({
                    "receipt_sha256": transform::sha256_hex(&bytes),
                    "evidence": "CarcaraChecked"
                }),
            )
        }
        Err(error) => error,
    }
}

#[allow(clippy::too_many_arguments)]
fn recompute(
    original: &Path,
    candidate: &Path,
    limits: Limits,
    tools: &Path,
    checker: &Path,
    certificates: &Path,
    queries: Option<&Path>,
) -> Result<Value, (u8, Value)> {
    let (original, candidate) = read_programs(original, candidate)?;
    let manifest = bounded(checker, 65_536).map_err(blocked)?;
    let checker = Session::open(&manifest).map_err(|e| blocked(format!("{e:?}")))?;
    let solvers = Solvers::open(Some(tools), queries).map_err(blocked)?;
    if !solvers.has_cvc5() {
        return Err(blocked("CVC5 is required"));
    }
    let mut regenerated = Vec::new();
    let outcome = equivalence::check(&original, &candidate, limits, &mut |query| {
        regenerated.push((query.label.to_owned(), query.source.to_owned()));
        solvers.solve(query)
    });
    // Retention failure must be checked before any passing receipt is constructed.
    if let Some(message) = solvers.retention_failure() {
        return Err(blocked(message));
    }
    let found = match outcome {
        Outcome::Equivalent(found) => found,
        Outcome::Counterexample(found) => {
            return Err(result(
                crate::INVALID,
                "counterexample",
                json!({"queries": found.compared.json()}),
            ));
        }
        Outcome::Refused(refusal) => {
            return Err(result(
                crate::INVALID,
                "refused",
                super::refusal_json(&refusal),
            ));
        }
        Outcome::ExhaustiveFits { size, limit } => {
            return Err(blocked(format!(
                "domain fits exhaustive check: {size} <= {limit}"
            )));
        }
        Outcome::Inconclusive(stop) => return Err(blocked(stop.cause)),
    };
    if regenerated.len() != found.compared.pieces.len() + 1 {
        return Err(blocked("regenerated piece coverage differs"));
    }
    let metadata = certificates
        .symlink_metadata()
        .map_err(|e| blocked(e.to_string()))?;
    if !metadata.is_dir() {
        return Err(blocked("certificate directory is not a real directory"));
    }
    let mut names = std::fs::read_dir(certificates)
        .map_err(|e| blocked(e.to_string()))?
        .map(|entry| {
            entry
                .map(|e| e.file_name())
                .map_err(|e| blocked(e.to_string()))
        })
        .collect::<Result<Vec<_>, _>>()?;
    names.sort();
    let mut expected: Vec<_> = (0..found.compared.pieces.len())
        .map(|i| std::ffi::OsString::from(format!("{i}.alethe")))
        .collect();
    expected.sort();
    if names != expected {
        return Err(blocked(
            "certificate inventory has omissions or extra files",
        ));
    }
    let mut witnesses = Vec::new();
    for (index, piece) in found.compared.pieces.iter().enumerate() {
        let (label, source) = &regenerated[index];
        if piece.label != *label || piece.query_sha256 != transform::sha256_hex(source.as_bytes()) {
            return Err(blocked("regenerated query binding differs"));
        }
        let proof = bounded(
            &certificates.join(format!("{index}.alethe")),
            CERTIFICATE_LIMIT,
        )
        .map_err(blocked)?;
        let witness = checker
            .check(source.as_bytes(), &proof)
            .map_err(|e| blocked(format!("{}: {e:?}", piece.label)))?;
        witnesses.push(json!({"piece": piece.label, "meaning": piece.meaning, "check": witness}));
        if let Some(directory) = queries {
            for (suffix, bytes) in [("smt2", source.as_bytes()), ("alethe", proof.as_slice())] {
                crate::atomic_create(
                    &directory.join(format!("certificate-{index}.{suffix}")),
                    bytes,
                )
                .map_err(|e| blocked(e.to_string()))?;
            }
        }
    }
    let mut receipt = found.receipt_value(&solvers.identities());
    receipt["schema"] = json!(SCHEMA);
    receipt["verdict"] = json!("certificate-checked-equivalent");
    receipt["evidence"] = json!("CarcaraChecked");
    receipt["claim"] = json!(
        "The complete independent checker derives a root contradiction from every regenerated \
        difference query. Under the scalar query/runtime refinement and named trusted base, \
        the admitted programs have the same ordered outputs or exact eager failure on the \
        declared domain at a nonbinding Step limit. This does not establish usage equality, \
        contract-source meaning, verified compilation, Lean kernel checking or application authority."
    );
    receipt["fragment"] =
        json!("quantifier-free Boolean and linear Int; eager checked-i128 scalar graphs");
    receipt["certificates"] = json!(witnesses);
    receipt["certificate_checker"] = json!(checker.identity());
    receipt["trusted_base"] = json!([
        "pinned Carcara implementation and parser plus complete-root/fragment guard",
        "compiler, loader, operating system and trusted executable-build provenance",
        "SMT-LIB interpretation of the qualified scalar query grammar"
    ]);
    receipt["cli_refinement_sources"] = refinement_sources();
    // This is the actual checker-command artifact, not a claimed compiled application.
    let artifact: PathBuf = std::env::current_exe().map_err(|e| blocked(e.to_string()))?;
    let bytes =
        bounded(&artifact, zeno_fcis_formal_tools::MAX_TOOL_BINARY_BYTES).map_err(blocked)?;
    receipt["compiled_checker_command"] = json!({
        "sha256": transform::sha256_hex(&bytes), "bytes": bytes.len(),
        "package": env!("CARGO_PKG_VERSION"), "required_rust": env!("CARGO_PKG_RUST_VERSION"),
        "os": std::env::consts::OS, "architecture": std::env::consts::ARCH,
        "compiled_application": Value::Null,
        "compilation_proved": false
    });
    receipt["not_accepted_by"] = json!([
        "transform replay",
        "contract adopt",
        "generate contract",
        "contract refresh-receipts"
    ]);
    Ok(receipt)
}

#[path = "certificates/refinement_sources.rs"]
mod bound_sources;

fn refinement_sources() -> Value {
    let mut sources = serde_json::Map::new();
    for (name, bytes) in [
        (
            "program",
            include_bytes!("../symbolic/program.rs").as_slice(),
        ),
        (
            "library_adapter",
            include_bytes!("../symbolic/program/library.rs").as_slice(),
        ),
        (
            "encoding",
            include_bytes!("../symbolic/program/encoding.rs").as_slice(),
        ),
        (
            "nodes",
            include_bytes!("../symbolic/program/nodes.rs").as_slice(),
        ),
        (
            "node_semantics",
            include_bytes!("../symbolic/program/node.rs").as_slice(),
        ),
        (
            "roots",
            include_bytes!("../symbolic/program/roots.rs").as_slice(),
        ),
        (
            "queries",
            include_bytes!("../symbolic/equivalence/queries.rs").as_slice(),
        ),
        (
            "query_base",
            include_bytes!("../symbolic/equivalence/queries/base.rs").as_slice(),
        ),
        (
            "query_compare",
            include_bytes!("../symbolic/equivalence/queries/compare.rs").as_slice(),
        ),
        (
            "query_pieces",
            include_bytes!("../symbolic/equivalence/queries/pieces.rs").as_slice(),
        ),
        ("term", include_bytes!("../symbolic/smt/term.rs").as_slice()),
        (
            "constructors",
            include_bytes!("../symbolic/smt/constructors.rs").as_slice(),
        ),
        (
            "denotation",
            include_bytes!("../symbolic/smt/denotation.rs").as_slice(),
        ),
        (
            "folding",
            include_bytes!("../symbolic/smt/folding.rs").as_slice(),
        ),
        ("copy", include_bytes!("../symbolic/smt/copy.rs").as_slice()),
        (
            "names",
            include_bytes!("../symbolic/smt/names.rs").as_slice(),
        ),
        (
            "numeral",
            include_bytes!("../symbolic/smt/numeral.rs").as_slice(),
        ),
        (
            "prepare",
            include_bytes!("../symbolic/smt/prepare.rs").as_slice(),
        ),
        (
            "printing",
            include_bytes!("../symbolic/smt/printing.rs").as_slice(),
        ),
        (
            "script",
            include_bytes!("../symbolic/smt/script.rs").as_slice(),
        ),
        (
            "equivalence",
            include_bytes!("../symbolic/equivalence.rs").as_slice(),
        ),
    ] {
        sources.insert(name.to_owned(), json!(transform::sha256_hex(bytes)));
    }
    for (name, entries) in [
        (
            "harness_include_closure",
            bound_sources::HARNESS_INCLUDE_SOURCES,
        ),
        ("runtime_sources", bound_sources::RUNTIME_REFINEMENT_SOURCES),
    ] {
        let hashes: serde_json::Map<_, _> = entries
            .iter()
            .map(|(path, hash)| ((*path).to_owned(), json!(hash)))
            .collect();
        sources.insert(name.to_owned(), Value::Object(hashes));
    }
    Value::Object(sources)
}

#[cfg(test)]
#[path = "certificates/source_tests.rs"]
mod source_tests;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_receipt_preserves_all_evidence_above_legacy_limit() {
        let digest = "a".repeat(64);
        let witnesses: Vec<_> = (0..258)
            .map(|index| {
                json!({
                    "piece": format!("output-{index}"),
                    "check": {
                        "query_sha256": digest,
                        "certificate_sha256": digest,
                        "stdout_sha256": digest,
                        "stderr_sha256": digest,
                        "query_bytes": 1_048_576,
                        "certificate_bytes": 33_554_432,
                    }
                })
            })
            .collect();
        let receipt = json!({"schema": SCHEMA, "certificates": witnesses});
        let bytes = receipt_bytes(&receipt)
            .unwrap_or_else(|error| panic!("a 258-piece receipt fits: {error:?}"));
        assert!(bytes.len() as u64 > super::super::RECEIPT_LIMIT);
        assert!(bytes.len() as u64 <= CHECKED_RECEIPT_LIMIT);
        assert_eq!(
            serde_json::from_slice::<Value>(&bytes)
                .unwrap_or_else(|error| panic!("receipt bytes are JSON: {error}")),
            receipt
        );
    }

    #[test]
    fn oversize_checked_receipt_refuses_before_publication() {
        let receipt = json!({"claim": "x".repeat(CHECKED_RECEIPT_LIMIT as usize)});
        let Err(error) = receipt_bytes(&receipt) else {
            panic!("an oversize receipt is refused");
        };
        assert_eq!(error.0, crate::BLOCKED);
        assert_eq!(error.1["status"], "certificate-check-incomplete");
    }
}
