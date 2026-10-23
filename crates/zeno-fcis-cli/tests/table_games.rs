//! The games guide, docs/TABLE_GAMES.md, run exactly as written: its command on
//! every example, the answer it states for every state, the fairness examples
//! without their assumption, and the refusals its scope names.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const GUIDE: &str = include_str!("../../../docs/TABLE_GAMES.md");
const README: &str = include_str!("../../../examples/contract-tables/games/README.md");

/// An example, the guide's answer cell, and whether the controller wins from each state.
type Example = (&'static str, &'static str, &'static [(&'static str, bool)]);

const EXAMPLES: [Example; 6] = [
    (
        "release-pipeline",
        "No, from draft, in review or changes requested: the reviewer can request changes forever. Yes, from approved and released.",
        &[
            ("draft", false),
            ("in_review", false),
            ("changes_requested", false),
            ("approved", true),
            ("released", true),
        ],
    ),
    (
        "release-pipeline-bounded",
        "Yes, from every state.",
        &[
            ("draft", true),
            ("review_1", true),
            ("changes_1", true),
            ("review_2", true),
            ("changes_2", true),
            ("escalated", true),
            ("approved", true),
            ("released", true),
        ],
    ),
    (
        "withdrawal-window",
        "No, from every state: the operator can keep the window paused.",
        &[("open", false), ("withdrawn", false), ("paused", false)],
    ),
    (
        "withdrawal-window-auto-resume",
        "Yes, from every state.",
        &[("open", true), ("withdrawn", true), ("paused", true)],
    ),
    (
        "withdrawal-window-fair",
        "Yes, from every state (No without the assumption).",
        &[("open", true), ("withdrawn", true), ("paused", true)],
    ),
    (
        "release-pipeline-fair",
        "Yes, from every state (No without the assumption).",
        &[
            ("draft", true),
            ("in_review", true),
            ("changes_requested", true),
            ("approved", true),
            ("released", true),
        ],
    ),
];

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// The guide's command, from the repository root.
fn expand(project: &str) -> Output {
    Command::new(env!("CARGO_BIN_EXE_zeno-fcis"))
        .current_dir(root())
        .args(["check", "--expand", project])
        .output()
        .unwrap()
}

/// Every `from_<state>` law's expansion, which the guide says is `true` or `false`.
fn answers(output: &Output) -> Vec<(String, String)> {
    laws(output, " from_")
}

/// The fairness examples' `stable` law, which must expand to exactly `true`.
fn stable(output: &Output) -> Vec<(String, String)> {
    laws(output, " stable")
}

fn laws(output: &Output, prefix: &str) -> Vec<(String, String)> {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| {
            let (_, rest) = line.split_once(prefix)?;
            let (state, value) = rest.split_once(" on commit = ")?;
            Some((state.to_owned(), value.trim_end_matches(';').to_owned()))
        })
        .collect()
}

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("table-games-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn write(dir: &Path, text: &str) -> String {
    let path = dir.join("project.zeno");
    std::fs::write(&path, text).unwrap();
    path.to_string_lossy().into_owned()
}

#[test]
fn the_guide_command_gives_the_documented_answer_from_every_state() {
    assert!(GUIDE.contains(
        "zeno-fcis check --expand examples/contract-tables/games/release-pipeline/project.zeno"
    ));
    assert!(README.contains(
        "zeno-fcis check --expand examples/contract-tables/games/<example>/project.zeno"
    ));
    for (example, answer, states) in EXAMPLES {
        let row = format!("| `games/{example}` |");
        let line = GUIDE
            .lines()
            .find(|line| line.starts_with(&row))
            .unwrap_or_else(|| panic!("the guide has no row for {example}"));
        assert!(line.ends_with(&format!("| {answer} |")), "{line}");
        let project = format!("examples/contract-tables/games/{example}/project.zeno");
        let expected: Vec<(String, String)> = states
            .iter()
            .map(|(state, wins)| ((*state).to_owned(), wins.to_string()))
            .collect();
        let output = expand(&project);
        assert_eq!(answers(&output), expected, "{example}");
        if example.ends_with("-fair") {
            assert_eq!(
                stable(&output),
                [(String::new(), "true".to_owned())],
                "{example}"
            );
        }
    }
}

#[test]
fn the_fairness_examples_answer_no_without_their_assumption() {
    for (example, assumption) in [
        ("withdrawal-window-fair", "[1,0,0]"),
        ("release-pipeline-fair", "[0,0,0,1,0]"),
    ] {
        let text = std::fs::read_to_string(root().join(format!(
            "examples/contract-tables/games/{example}/project.zeno"
        )))
        .unwrap();
        let declared = format!(
            "fair 107 size {} {assumption};",
            assumption.matches(',').count() + 1
        );
        assert!(text.contains(&declared), "{example}");
        let everywhere = assumption.replace('0', "1");
        let dir = scratch(example);
        let project = write(
            &dir,
            &text.replace(&declared, &declared.replace(assumption, &everywhere)),
        );
        let output = expand(&project);
        assert_eq!(
            stable(&output),
            [(String::new(), "true".to_owned())],
            "{example}"
        );
        let result = answers(&output);
        assert!(!result.is_empty(), "{example}");
        assert!(
            result.iter().all(|(_, value)| value == "false"),
            "{example}: {result:?}"
        );
    }
}

#[test]
fn the_scope_refusals_hold() {
    let text = std::fs::read_to_string(
        root().join("examples/contract-tables/games/withdrawal-window/project.zeno"),
    )
    .unwrap();
    let wide = format!(
        "const table 110 mover 107 size 65 [{}];",
        ["0"; 65].join(",")
    );
    for (name, changed, refusal) in [
        (
            "rounds",
            text.replacen("rounds 3", "rounds 4", 1),
            "exactly entries * width rounds",
        ),
        (
            "collision",
            text.replace("const table 110 mover", "const table 120 mover")
                .replace("in table 110", "in table 120"),
            "a constant table ID must be unused",
        ),
        (
            "size",
            text.replace("const table 110 mover 107 size 3 [0,1,1];", &wide),
            "size1..64",
        ),
    ] {
        assert_ne!(changed, text, "{name}: the change did not apply");
        let dir = scratch(name);
        let output = expand(&write(&dir, &changed));
        assert!(!output.status.success(), "{name}");
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(stderr.contains(refusal), "{name}: {stderr}");
    }
}
