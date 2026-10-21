//! `check --expand` and the library loans table example through the CLI.

use serde_json::Value;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn example() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples/contract-tables/library-loans")
}

fn cli(arguments: &[&str], paths: &[&Path]) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_zeno-fcis"));
    command.args(arguments);
    for path in paths {
        command.arg(path);
    }
    command.output().unwrap_or_else(|error| panic!("{error:?}"))
}

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("contract-tables-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap_or_else(|error| panic!("{error:?}"));
    dir
}

#[test]
fn contract_tables_expand_prints_the_checked_project() {
    let project = example().join("project.zeno");
    let human = cli(&["check", "--expand"], &[&project]);
    assert!(
        human.status.success(),
        "{}",
        String::from_utf8_lossy(&human.stderr)
    );
    let text = String::from_utf8(human.stdout).unwrap_or_else(|error| panic!("{error:?}"));
    let summary = String::from_utf8(human.stderr).unwrap_or_else(|error| panic!("{error:?}"));
    assert!(
        summary.contains("tables=1 entries=6 inputs=10 domain=294912"),
        "{summary}"
    );
    assert!(
        summary.contains("20 clauses per expansion, expanded 6 times"),
        "{summary}"
    );
    assert!(text.contains("field 110 100 copies_0 105;\nfield 111 100 copies_1 105;"));
    assert!(!text.contains("table 110"));
    // The printed expansion checks as the same program.
    let dir = scratch("expand");
    let expanded = dir.join("project.zeno");
    std::fs::write(&expanded, &text).unwrap_or_else(|error| panic!("{error:?}"));
    let hash = |path: &Path| {
        let output = cli(&["check", "--format", "json"], &[path]);
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stdout)
        );
        let value: Value =
            serde_json::from_slice(&output.stdout).unwrap_or_else(|error| panic!("{error:?}"));
        value["semantic_program_hash"]
            .as_str()
            .unwrap_or_else(|| panic!("expected table fixture value"))
            .to_owned()
    };
    assert_eq!(hash(&project), hash(&expanded));
    let json = cli(&["check", "--expand", "--format", "json"], &[&project]);
    assert!(json.status.success());
    let value: Value =
        serde_json::from_slice(&json.stdout).unwrap_or_else(|error| panic!("{error:?}"));
    assert_eq!(value["status"], "expanded");
    assert_eq!(value["expansion"], text);
    assert_eq!(value["domain_size"], "294912");
    assert_eq!(value["canonical_match"], true);
    assert_eq!(value["counts"][0]["clauses"], "20");
    assert_eq!(value["counts"][0]["expansions"], 6);
    assert_eq!(value["tables"][0]["fields"], serde_json::json!([110, 115]));
    // A table error is a diagnostic at the table source, not printed text.
    let bad = dir.join("bad.zeno");
    let source = std::fs::read_to_string(&project)
        .unwrap_or_else(|error| panic!("{error:?}"))
        .replace("size 6;", "size 65;");
    std::fs::write(&bad, &source).unwrap_or_else(|error| panic!("{error:?}"));
    let refused = cli(&["check", "--expand", "--format", "json"], &[&bad]);
    assert_eq!(refused.status.code(), Some(1));
    let value: Value =
        serde_json::from_slice(&refused.stdout).unwrap_or_else(|error| panic!("{error:?}"));
    let diagnostic = &value["diagnostics"][0];
    assert_eq!(diagnostic["code"], "ZENO-E0207");
    assert_eq!(
        diagnostic["span"]["start"],
        source
            .find("table 110")
            .unwrap_or_else(|| panic!("expected table fixture value"))
    );
    std::fs::remove_dir_all(dir).unwrap_or_else(|error| panic!("{error:?}"));
}

#[test]
fn contract_tables_example_generates_and_its_count_law_is_inductive() {
    let dir = scratch("generate");
    let generated = cli(
        &["generate", "contract"],
        &[&example(), Path::new("--out"), &dir],
    );
    assert!(
        generated.status.success(),
        "{}",
        String::from_utf8_lossy(&generated.stderr)
    );
    assert!(dir.join("v2/policy.zcve").is_file());
    // `--lemma` text may not contain braces, so pass law 500's expansion.
    let expansion = cli(&["check", "--expand"], &[&example().join("project.zeno")]);
    let text = String::from_utf8(expansion.stdout).unwrap_or_else(|error| panic!("{error:?}"));
    let start = text
        .find("law 500 at_most_two_per_member on commit, genesis =")
        .unwrap_or_else(|| panic!("expected count law"));
    let law = &text[start..];
    let formula = law[law
        .find('=')
        .unwrap_or_else(|| panic!("expected table fixture value"))
        + 1
        ..law
            .find(";\n")
            .unwrap_or_else(|| panic!("expected table fixture value"))]
        .trim();
    let lemma = cli(
        &["contract", "lemma"],
        &[&example(), Path::new("--lemma"), Path::new(formula)],
    );
    assert!(
        lemma.status.success(),
        "{}",
        String::from_utf8_lossy(&lemma.stderr)
    );
    let value: Value =
        serde_json::from_slice(&lemma.stdout).unwrap_or_else(|error| panic!("{error:?}"));
    assert_eq!(value["lemmas"][0]["verdict"], "inductive");
    assert_eq!(value["lemmas"][0]["states"]["declared"], 4096);
    assert_eq!(value["lemmas"][0]["states"]["reachable"], 2074);
    std::fs::remove_dir_all(dir).unwrap_or_else(|error| panic!("{error:?}"));
}
