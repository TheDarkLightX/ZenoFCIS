//! Complete shipped-program inventory at the command-handler boundary. Binary
//! tests independently cover canonical stdout, containment and refusal classes.
use super::*;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT: AtomicU64 = AtomicU64::new(0);
struct Directory(PathBuf);
impl Directory {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "zeno-print-handlers-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap_or_else(|error| panic!("{error}"));
        Self(path)
    }
}
impl Drop for Directory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn read(path: &Path) -> Vec<u8> {
    fs::read(path).unwrap_or_else(|error| panic!("{}: {error}", path.display()))
}

fn round_trip(path: &Path, out: &Path) {
    let bytes = read(path);
    let (exit, program) = print_command(path);
    assert_eq!(exit, crate::OK, "{}: {program}", path.display());
    assert_eq!(program.as_object().map(|object| object.len()), Some(4));
    let json = out.with_extension("json");
    fs::write(&json, canonical_json(&program)).unwrap_or_else(|error| panic!("{error}"));
    let (exit, report) = encode_command(&json, out);
    assert_eq!(exit, crate::OK, "{report}");
    assert_eq!(read(out), bytes, "round-trip bytes for {}", path.display());
    let (exit, again) = print_command(out);
    assert_eq!(exit, crate::OK, "{again}");
    assert_eq!(again, program, "encode then print for {}", path.display());
    assert_eq!(read(path), bytes);
}

fn contracts(path: &Path, found: &mut Vec<PathBuf>) {
    if path.join("project.zeno").is_file() && path.join("v2/policy.json").is_file() {
        found.push(path.to_owned());
    }
    for entry in fs::read_dir(path).unwrap_or_else(|error| panic!("{error}")) {
        let entry = entry.unwrap_or_else(|error| panic!("{error}"));
        if entry
            .file_type()
            .unwrap_or_else(|error| panic!("{error}"))
            .is_dir()
        {
            contracts(&entry.path(), found);
        }
    }
}

#[test]
fn print_round_trips_every_program_the_shipped_examples_produce() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let directory = Directory::new();
    let mut examples = Vec::new();
    contracts(&root.join("examples"), &mut examples);
    examples.sort();
    assert!(!examples.is_empty());
    let mut exported = 0;
    for (index, example) in examples.iter().enumerate() {
        let path = directory.0.join(format!("export-{index}.zcve"));
        let exit = crate::contract_files::export_program(example, &path, crate::OutputFormat::Json);
        if example.ends_with("contract-tables/permission-words") {
            // The U128 contract deliberately has no standalone scalar export.
            assert_eq!(exit, crate::INVALID);
            assert!(!path.exists());
            continue;
        }
        assert_eq!(exit, crate::OK, "export {}", example.display());
        round_trip(&path, &directory.0.join(format!("encoded-{index}.zcve")));
        exported += 1;
    }
    assert_eq!(exported, examples.len() - 1);
    eprintln!("round-tripped {exported} exported example programs");
}

#[test]
fn print_round_trips_every_shipped_benchmark_program() {
    let directory = Directory::new();
    let cases: Value = serde_json::from_str(include_str!("../../../docs/benchmarks/cases.json"))
        .unwrap_or_else(|error| panic!("{error}"));
    let cases = cases["cases"].as_array().unwrap_or_else(|| panic!("cases"));
    for (index, case) in cases.iter().enumerate() {
        for side in ["original", "candidate"] {
            let program = json!({"inputs":case["input_domains"], "outputs":case["output_domains"],
                "nodes":case[side]["nodes"], "roots":case[side]["roots"]});
            let json = directory.0.join(format!("{index}-{side}.json"));
            fs::write(&json, canonical_json(&program)).unwrap_or_else(|error| panic!("{error}"));
            let path = directory.0.join(format!("{index}-{side}.zcve"));
            let (exit, report) = encode_command(&json, &path);
            assert_eq!(exit, crate::OK, "{report}");
            let (exit, printed) = print_command(&path);
            assert_eq!(exit, crate::OK, "{printed}");
            assert_eq!(printed, program, "{} {side}", case["id"]);
            round_trip(
                &path,
                &directory.0.join(format!("again-{index}-{side}.zcve")),
            );
        }
    }
    eprintln!("round-tripped {} benchmark programs", 2 * cases.len());
}

#[test]
fn print_interchange_does_not_enlarge_small_candidate_admission() {
    let program = json!({"inputs":[], "outputs":[{"kind":"Bool"}],
        "nodes":vec![json!(["Bool", true]);257], "roots":[0]});
    assert!(program_from_json(&program).is_err());
    let extended = crate::neural_loop::program_json::encoding_program_from_json(&program)
        .unwrap_or_else(|error| panic!("{error:?}"));
    assert_eq!(extended.nodes().len(), 257);
    assert!(import_program(&encode(&extended).unwrap_or_default()).is_err());
}
