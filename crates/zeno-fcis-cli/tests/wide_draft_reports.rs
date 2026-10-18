//! Actual labeled/unlabeled draft reports, including the browser precision edge.
#![allow(clippy::unwrap_used)]
use serde_json::{Value, json};
use std::{fs, path::Path, process::Command};

fn draft(args: &[&str]) -> Value {
    let done = Command::new(env!("CARGO_BIN_EXE_zeno-fcis"))
        .args(["--no-kb", "contract", "draft"])
        .args(args)
        .output()
        .unwrap();
    assert!(done.status.success(), "{args:?}: {done:?}");
    assert!(done.stderr.is_empty(), "{done:?}");
    let report: Value = serde_json::from_slice(&done.stdout).unwrap();
    assert_eq!(report["authority"], "none");
    report
}
fn path(p: &Path) -> &str {
    p.to_str().unwrap()
}

#[test]
fn labeled_and_unlabeled_drafts_preserve_extremes_and_small_reports() {
    let root = std::env::temp_dir().join(format!("zeno-wide-draft-{}", std::process::id()));
    fs::create_dir(&root).unwrap();
    for (i, value) in [
        i128::MIN,
        i128::MAX,
        -(1i128 << 100),
        1 << 100,
        (1 << 53) + 1,
        5,
    ]
    .into_iter()
    .enumerate()
    {
        for labeled in [false, true] {
            let folder = root.join(format!("{i}-{labeled}"));
            fs::create_dir(&folder).unwrap();
            let project = folder.join("project.zeno");
            fs::write(&project, format!(r#"zeno 1;
project 1 draft_reports;
namespace 2 core;
type 100 state Counter;
type 101 command Request;
type 102 context Environment;
type 105 int Value in 0..=1;
type 106 int Amount in {value}..={value};
field 110 100 value 105;
field 120 101 amount 106;
reason 201 outside_scope precedence 0;
component 400 calculate {{ owns 100; reads pre.100; writes post.100; contexts context.102; budget steps 256; }}
merge [400];
law 500 nonnegative on commit, genesis = post.100.110 >= 0;
"#)).unwrap();
            let rules = folder.join("policy.json");
            // A constant successor for labeled cases; a real distinguishing
            // input for unlabeled cases. MIN never enters nonnegative mul-div.
            let policy = json!({"schema":"zeno-fcis/template-declarative-policy/2", "template":"wide-draft",
                "roots":{"state":100,"command":101,"context":102}, "leaf_bindings":{"102":["I128",0,0]},
                "variables":{"amount":"command.101.120","value":"pre.100.110"},
                "cases":[{"when":true,"class":"Accept","reason":null,"post":{"110":if labeled {"0"} else {"value + 0"}},"outbox":[]},
                    {"when":true,"class":"Reject","reason":201,"post":{},"outbox":[]}],
                "genesis":{"110":0},"law_kinds":{"500":"StateInvariant"}});
            fs::write(&rules, serde_json::to_vec(&policy).unwrap()).unwrap();
            let intent = folder.join("intent.txt");
            fs::write(&intent, "Supplied draft regression intent.\n").unwrap();
            let examples = folder.join("examples.txt");
            let text = format!("0 {value} 0 | accept - 0 | -");
            fs::write(&examples, format!("{text}\n")).unwrap();
            let session = folder.join("session");
            let mut args = vec![
                "start",
                "--session",
                path(&session),
                "--project",
                path(&project),
                "--intent",
                path(&intent),
                "--provenance",
                "supplied test intent",
            ];
            if labeled {
                args.extend(["--examples", path(&examples)]);
            }
            let opened = draft(&args);
            let proposed = draft(&[
                "propose",
                "--session",
                path(&session),
                "--revision",
                opened["revision"].as_str().unwrap(),
                "--rules",
                path(&rules),
                "--provenance",
                "supplied test proposal",
            ]);
            let detail = &proposed["detail"];
            assert_eq!(detail["schema"], "zeno-fcis/contract-draft-assessment/2");
            assert_eq!(
                detail["integer_encoding"],
                "safe-number-or-decimal-string/1"
            );
            let scalar = if value == 5 {
                json!(5)
            } else {
                json!(value.to_string())
            };
            let input = json!([0, scalar, 0]);
            if labeled {
                assert_eq!(
                    detail["labels_compared"],
                    json!([{"input":input,"text":text,"difference":null}])
                );
            } else {
                assert!(
                    detail["unlabeled_inputs"]
                        .as_array()
                        .unwrap()
                        .contains(&input),
                    "{detail}"
                );
                assert_eq!(detail["labels_compared"], json!([]));
            }
            // The persisted assessment and a fresh re-assessment retain the
            // same values. This does not authorize finalization/publication.
            let questions = draft(&["questions", "--session", path(&session)]);
            assert_eq!(questions["detail"], *detail);
            let stored: Value =
                serde_json::from_slice(&fs::read(session.join("draft.json")).unwrap()).unwrap();
            assert_eq!(
                stored["proposals"][0]["assessment"]["labels_compared"],
                detail["labels_compared"]
            );
            assert_eq!(
                stored["proposals"][0]["assessment"]["unlabeled_inputs"],
                detail["unlabeled_inputs"]
            );
        }
    }
    fs::remove_dir_all(root).unwrap();
}
