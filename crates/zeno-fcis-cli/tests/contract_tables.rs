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
fn expand_handler_delegates_canonical_acceptance_to_pure_preparation() {
    let source = include_str!("../src/table_expansion.rs");
    let handler = source
        .split_once("pub(crate) fn expand(")
        .map(|(_, body)| body)
        .unwrap_or_default();
    let handler = handler
        .split_once("\nfn inputs(")
        .map(|(body, _)| body)
        .unwrap_or(handler);
    assert!(
        source.contains("fn checked_expansion(")
            && !handler.contains("canonical_bytes(")
            && !handler.contains("expand_tables("),
        "the file handler must render a result returned by pure checked expansion"
    );
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
    // The table source and printed expansion have the same lemma verdict.
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
    let table_formula = "forall i in table 110 where post.100.110[i] != 0 { count j in table 110 { post.100.110[j] == post.100.110[i] } <= 2 }";
    let tabled = cli(
        &["contract", "lemma"],
        &[&example(), Path::new("--lemma"), Path::new(table_formula)],
    );
    assert!(
        tabled.status.success(),
        "table-form lemma was refused: {}",
        String::from_utf8_lossy(&tabled.stderr)
    );
    let tabled: Value = serde_json::from_slice(&tabled.stdout)
        .unwrap_or_else(|error| panic!("table fixture failed: {error:?}"));
    let mut scalar = value["lemmas"][0].clone();
    let mut table = tabled["lemmas"][0].clone();
    scalar
        .as_object_mut()
        .unwrap_or_else(|| panic!("lemma report must be an object"))
        .remove("formula");
    table
        .as_object_mut()
        .unwrap_or_else(|| panic!("lemma report must be an object"))
        .remove("formula");
    assert_eq!(scalar, table);

    std::fs::remove_dir_all(dir).unwrap_or_else(|error| panic!("{error:?}"));
}

#[test]
fn contract_tables_json_expansion_handles_the_maximum_stable_id()
-> Result<(), Box<dyn std::error::Error>> {
    let dir = scratch("max-id");
    let path = dir.join("project.zeno");
    let header = "zeno 1; project 1 tables; namespace 2 tables; type 100 state S; type 101 command C; type 102 context X; type 105 int V in 0..=3; field 120 101 pick 105; variant 130 102 Desk none; component 400 c { owns 100; reads pre.100; writes post.100; contexts context.102; budget steps 64; } merge [400];\n";
    for size in [1, 2, 64] {
        let first = u32::MAX - (size - 1);
        let source = format!("{header}table {first} 100 slot 105 size {size};\n");
        std::fs::write(&path, &source)?;
        let output = cli(&["check", "--expand", "--format", "json"], &[&path]);
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let value: Value = serde_json::from_slice(&output.stdout)?;
        assert_eq!(value["canonical_match"], true);
        assert_eq!(
            value["tables"][0]["fields"],
            serde_json::json!([first, u32::MAX])
        );
        std::fs::write(
            &path,
            format!("{source}type {} int Spare in 0..=1;\n", u32::MAX),
        )?;
        let refused = cli(&["check", "--expand", "--format", "json"], &[&path]);
        assert_eq!(refused.status.code(), Some(1));
        let value: Value = serde_json::from_slice(&refused.stdout)?;
        assert!(
            !value["diagnostics"]
                .as_array()
                .ok_or("diagnostics missing")?
                .is_empty()
        );
    }
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

#[test]
fn contract_table_lemma_rejects_declaration_injection() {
    for text in [
        "post.100.110 == 1 }; law 999 injected on commit = true;",
        "forall i in table 110 { post.100.110[i] == 0 }; law 999 injected on commit = true;",
        "post.100.110 == 0 true",
    ] {
        let refused = cli(
            &["contract", "lemma"],
            &[&example(), Path::new("--lemma"), Path::new(text)],
        );
        assert!(
            !refused.status.success(),
            "injection must be refused: {text}"
        );
        assert_eq!(refused.status.code(), Some(65));
    }
}
