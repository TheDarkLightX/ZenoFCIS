//! Frozen adversarial mutations. Only actual qualified parents may enter the
//! controls command. No search result changes the mutation table.

use super::{
    common::*,
    files,
    layout::{self, Layout},
    phases::{Bundle, Context},
    proposal,
    search::{self, Positive},
};
use serde_json::{Value, json};
use std::path::Path;
use zeno_fcis_contract::internals::expr::{self, Ast};

pub(super) const NAMES: [&str; 8] = [
    "reject-all",
    "forgotten-content",
    "missing-operation",
    "extrema-only",
    "wrong-tie",
    "wrong-reason",
    "invalid-encoding",
    "source-substitution",
];
pub(super) const DISABLED: [&str; 8] = [
    "valid-row-relation",
    "successor",
    "successor",
    "valid-row-relation",
    "valid-row-relation",
    "reason",
    "null-law",
    "source-pin",
];
pub(super) fn coordinate(layout: Layout, index: usize) -> Option<(u64, u64)> {
    let coords = if layout == Layout::T {
        [
            (0, 0),
            (109, 10),
            (56, 10),
            (56, 2),
            (108, 15),
            (108, 4),
            (7, 0),
        ]
    } else {
        [
            (0, 0),
            (295, 10),
            (126, 14),
            (126, 0),
            (258, 15),
            (258, 0),
            (25, 0),
        ]
    };
    coords.get(index).copied()
}

pub(super) fn mutate(
    layout: Layout,
    index: usize,
    project: &str,
    policy: &str,
) -> Result<(String, Vec<u8>)> {
    let mut project = project.to_string();
    let mut policy = strict(policy.as_bytes())?;
    match index {
        0 => policy["cases"][0]["when"] = json!("true"),
        1 => {
            let condition = if layout == Layout::T {
                "n == 2 && s0 == 1 && s1 == 4 && q == 10"
            } else {
                "n == 2 && cA == 1 && cB == 4 && cC == 0 && q == 10"
            };
            let field = "112";
            let old = policy["cases"][5]["post"][field]
                .as_str()
                .ok_or_else(|| malformed("control-post-field"))?;
            policy["cases"][5]["post"][field] = json!(format!("choose({condition}, 0, {old})"));
        }
        2 => {
            for j in 0..layout.cells() {
                let field = (111 + j).to_string();
                let source = policy["cases"][5]["post"][&field]
                    .as_str()
                    .ok_or_else(|| malformed("control-change-field"))?;
                let mut ast = expr::parse(source).map_err(malformed)?;
                let Ast::Choose(_, _, after_insert) = &mut ast else {
                    return fail(65, "control-insert-route");
                };
                let Ast::Choose(_, _, after_delete) = after_insert.as_mut() else {
                    return fail(65, "control-delete-route");
                };
                let Ast::Choose(_, changed, _) = after_delete.as_mut() else {
                    return fail(65, "control-change-route");
                };
                **changed = Ast::Name(layout.cell_name(j).into());
                policy["cases"][5]["post"][&field] = json!(expr::render(&ast));
            }
        }
        3 => {
            let mut equal = Vec::new();
            for j in 0..layout.cells() {
                let cell = layout.cell_name(j);
                let mut cases = Vec::new();
                for token in 1..layout.radix() {
                    cases.push(format!(
                        "({cell} == {token} && requested_priority == {})",
                        (token - 1) % 2
                    ));
                }
                equal.push(format!("(present{j} && ({}))", cases.join(" || ")));
            }
            let extra = json!({"when":format!("insert && ({})",equal.join(" || ")),"class":"Reject","reason":201,"post":{},"outbox":[]});
            policy["cases"]
                .as_array_mut()
                .ok_or_else(|| malformed("control-cases"))?
                .insert(5, extra);
        }
        4 => policy["variables"]["minimum_id"] = json!(layout::reversed_tie_definition(layout)?),
        5 => policy["cases"]
            .as_array_mut()
            .ok_or_else(|| malformed("control-cases"))?
            .swap(1, 2),
        6 => {
            let exception = if layout == Layout::T {
                "pre.100.110 == 0 && pre.100.111 == 1 && pre.100.112 == 0 && command.101.120 == 0"
            } else {
                "pre.100.110 == 0 && pre.100.111 == 1 && pre.100.112 == 0 && pre.100.113 == 0 && command.101.120 == 0"
            };
            let mut found = false;
            let mut lines = Vec::new();
            for line in project.lines() {
                if let Some(formula) = line.strip_prefix("law 499 valid_pre on any = ") {
                    let formula = formula
                        .strip_suffix(';')
                        .ok_or_else(|| malformed("control-law-terminator"))?;
                    lines.push(format!(
                        "law 499 valid_pre on any = ({formula}) || ({exception});"
                    ));
                    found = true;
                } else {
                    lines.push(line.into());
                }
            }
            if !found {
                return fail(65, "control-law-499-absent");
            }
            project = lines.join("\n") + "\n";
        }
        7 => project.push('\n'),
        _ => return fail(65, "control-index"),
    }
    Ok((project, canonical(&policy)?))
}

pub(super) fn api_result(result: Result<Value>) -> Value {
    match result {
        Ok(report) => {
            json!({"exit":match report["verdict"].as_str(){Some("refines")=>0,Some("counterexample")=>1,_=>2},"report":report,"error":Value::Null})
        }
        Err(error) => json!({"exit":error.code,"report":Value::Null,"error":error.reason}),
    }
}
pub(super) fn detected(result: &Value, layout: Layout, index: usize) -> bool {
    if let Some((s, q)) = coordinate(layout, index) {
        result["exit"] == 1
            && result["report"]["verdict"] == "counterexample"
            && result["report"]["witness"]["state"] == s
            && result["report"]["witness"]["input"] == q
    } else {
        result["exit"] == 65
            && result["report"].is_null()
            && result["error"]
                .as_str()
                .is_some_and(|e| e.contains("source SHA-256 differs from its pin"))
    }
}

/// M02 adds its independently checked meta record/results after M01 sealed
/// the original controls. Preserve that seal exactly; permit only the seventeen
/// specified additions, all of which the summary separately validates and pins.
pub(super) fn verify_original(root: &Path) -> Result<Value> {
    let original = strict(&read(&root.join("manifest.json"), LOG_LIMIT)?)?;
    closed(&original, &["schema", "revision", "files"])?;
    let mut current = files::manifest(root)?;
    let mut additions = std::collections::BTreeSet::from(["meta.json".to_string()]);
    for layout in ["T", "I"] {
        for (index, name) in NAMES.iter().enumerate() {
            additions.insert(format!("meta-results/{layout}/{index:02}-{name}.json"));
        }
    }
    current["files"]
        .as_array_mut()
        .ok_or_else(|| malformed("controls-manifest-files"))?
        .retain(|row| !row[0].as_str().is_some_and(|p| additions.contains(p)));
    if current != original {
        return fail(65, "original-controls-manifest-differs");
    }
    Ok(original)
}

fn check_parent(
    context: &mut Context,
    bundle: &Bundle,
    run: &Path,
    layout: Layout,
    cli: &Path,
) -> Result<Positive> {
    context.input(&run.join("manifest.json"), LOG_LIMIT)?;
    let parent = search::positive(bundle, run)?;
    if parent.layout != layout {
        return fail(65, "control-parent-layout");
    }
    let compiled = super::compilation::compile(layout, &parent.project, &parent.policy)?;
    context.add_scalar(&compiled.inventory)?;
    if compiled.refusal.is_some()
        || compiled.original != read(&parent.directory.join("compiled.zcve"), SOURCE_LIMIT)?
        || compiled.lowered.as_deref()
            != Some(read(&parent.directory.join("lowered.zcve"), SOURCE_LIMIT)?.as_slice())
    {
        return fail(1, "control-parent-compilation-or-lowering");
    }
    let verification = context.out.join(format!("{}/parent-replay", layout.name()));
    search::transform(
        context,
        cli,
        &verification,
        &parent.directory,
        layout,
        false,
    )?;
    let report = search::native(
        bundle,
        layout,
        &parent.project,
        &parent.policy,
        &parent.abstraction,
    )?;
    context.add_native(&report)?;
    search::require_positive(&report, layout)?;
    write_json(
        &context.out.join(format!("{}/positive.json", layout.name())),
        &report,
    )?;
    Ok(parent)
}
pub(super) fn controls(
    context: &mut Context,
    path: &Path,
    t_run: &Path,
    i_run: &Path,
    cli: &Path,
) -> Result<&'static str> {
    let bundle = Bundle::load(context, path)?;
    search::bind_cli(context, &bundle, cli)?;
    let t = check_parent(context, &bundle, t_run, Layout::T, cli)?;
    let i = check_parent(context, &bundle, i_run, Layout::I, cli)?;
    let mut records = Vec::new();
    context.detail = json!({"controls":[],"positives":{},"endpoint_obstruction":"At capacity two every successful Insert has at most one predecessor. The tested extrema restriction is strict; no genuine interior-insertion fault is claimed.","control_count":0});
    for parent in [&t, &i] {
        let layout = parent.layout;
        context.detail["positives"][layout.name()] = files::reference(
            &context.evidence,
            &context.out.join(format!("{}/positive.json", layout.name())),
        )?;
        for index in 0..8 {
            let dir = context
                .out
                .join(format!("{}/{index:02}-{}", layout.name(), NAMES[index]));
            let (project, policy) = mutate(layout, index, &parent.project, &parent.policy)?;
            let abstraction = if index == 7 {
                parent.abstraction.as_bytes().to_vec()
            } else {
                proposal::abstraction(
                    &bundle.abstract_project,
                    &bundle.abstract_policy,
                    &bundle.relation,
                    layout,
                    project.as_bytes(),
                    &policy,
                )?
            };
            write(&dir.join("candidate/project.zeno"), project.as_bytes())?;
            write(&dir.join("candidate/v2/policy.json"), &policy)?;
            write(&dir.join("abstraction.json"), &abstraction)?;
            let result = api_result(search::native(
                &bundle,
                layout,
                &project,
                std::str::from_utf8(&policy).map_err(|e| malformed(e.to_string()))?,
                std::str::from_utf8(&abstraction).map_err(|e| malformed(e.to_string()))?,
            ));
            if !result["report"].is_null() {
                context.add_native(&result["report"])?;
            }
            write_json(&dir.join("normal.json"), &result)?;
            let record = json!({"layout":layout.name(),"control":NAMES[index],"parent_sha256":parent.subject,
                "mutant_sha256":search::subject_sha(project.as_bytes(),&policy,&abstraction)?,"disabled_predicate":DISABLED[index],"expected_first":coordinate(layout,index),
                "expected_exit":if index==7{65}else{1},"normal_result":files::reference(&context.evidence,&dir.join("normal.json"))?,
                "project":files::reference(&context.evidence,&dir.join("candidate/project.zeno"))?,"policy":files::reference(&context.evidence,&dir.join("candidate/v2/policy.json"))?,
                "abstraction":files::reference(&context.evidence,&dir.join("abstraction.json"))?});
            write_json(&dir.join("control.json"), &record)?;
            files::seal(&dir)?;
            records.push(record);
            context.detail["controls"] = json!(records);
            context.detail["control_count"] = json!(records.len());
            if !detected(&result, layout, index) {
                return fail(
                    1,
                    format!(
                        "control-not-caught-as-specified:{}/{}",
                        layout.name(),
                        NAMES[index]
                    ),
                );
            }
        }
    }
    Ok("controls-caught")
}
