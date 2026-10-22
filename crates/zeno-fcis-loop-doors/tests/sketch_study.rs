//! Evidence harness, not a check: the sketch search over meanings against
//! the joint search, on sketches made by opening 2 to 6 of an exported
//! program's own constants as holes in one output. Every such sketch is
//! realizable (the original's constants complete it), so "unrealizable" on
//! one would be a false claim, and a completion one search finds and the
//! other does not is a loss.
//!
//! It runs only when asked:
//!
//! ```sh
//! ZENO_LOOP_SKETCH_STUDY=cases.tsv ZENO_LOOP_SKETCH_STUDY_OUT=results.jsonl \
//!   cargo test -p zeno-fcis-loop-doors --test sketch_study -- --ignored
//! ```
//!
//! Each line of the cases file is `name TAB program.zcve TAB input names
//! (comma-separated) TAB variants (`field=Name:value,...;...`) TAB cases`.
//! Each result line is one JSON object. The completions are this crate's own
//! (untrusted) results; the study's driver confirms them with `transform
//! check`.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::fs;
use std::time::Instant;

use zeno_fcis_loop_doors::expr::{Expr, Fill, Hole, Vocabulary, print_symbolic};
use zeno_fcis_loop_doors::inverse::{positions, replace_at};
use zeno_fcis_loop_doors::lower::from_program;
use zeno_fcis_loop_doors::repair::first_difference;
use zeno_fcis_loop_doors::sketch::{self, Search, Settings};
use zeno_fcis_loop_doors::subject::{DEFAULT_MAX_TUPLES, Subject};
use zeno_fcis_synthesis::finite_runtime::import_program;

/// SplitMix64, seeded per sketch, so the opened constants are reproducible.
fn next(state: &mut u64) -> u64 {
    *state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let mut z = *state;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

fn seed(name: &str, output: usize, holes: usize) -> u64 {
    name.bytes()
        .fold(0xCBF2_9CE4_8422_2325_u64, |h, b| {
            (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01B3)
        })
        .wrapping_add((output as u64) << 8)
        .wrapping_add(holes as u64)
}

fn quoted(text: &str) -> String {
    let mut out = String::from("\"");
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            c if (c as u32) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn variants(text: &str) -> BTreeMap<String, BTreeMap<String, i128>> {
    let mut out = BTreeMap::new();
    for field in text.split(';').filter(|f| !f.is_empty()) {
        let (name, list) = field
            .split_once('=')
            .unwrap_or_else(|| panic!("variants {field}"));
        let mut values = BTreeMap::new();
        for entry in list.split(',').filter(|e| !e.is_empty()) {
            let (variant, value) = entry
                .split_once(':')
                .unwrap_or_else(|| panic!("variant {entry}"));
            values.insert(
                variant.to_owned(),
                value.parse().unwrap_or_else(|_| panic!("value {entry}")),
            );
        }
        out.insert(name.to_owned(), values);
    }
    out
}

fn run(subject: &Subject, outputs: &[Expr], output: usize, search: Search) -> String {
    let settings = Settings {
        search,
        ..Settings::default()
    };
    let started = Instant::now();
    let result = sketch::solve(subject, outputs, &settings);
    let micros = started.elapsed().as_micros();
    let correct = result
        .program
        .as_ref()
        .is_some_and(|program| first_difference(subject, program).is_none());
    let tried = result.outputs.get(output).map_or(0, |o| o.tried);
    let visits = result.outputs.get(output).map_or(0, |o| o.visits);
    let class_values = result.outputs.get(output).map_or(0, |o| o.class_values);
    let fills: Vec<String> = result
        .holes
        .iter()
        .map(|(k, fill)| match fill {
            Fill::Const(v) => format!("\"{k}\":{}", quoted(&v.to_string())),
            Fill::Choice(i) => format!("\"{k}\":{}", quoted(&format!("choice {i}"))),
            Fill::Ast(e) => format!("\"{k}\":{}", quoted(&print_symbolic(e))),
        })
        .collect();
    let expressions: Vec<String> = result
        .expressions
        .iter()
        .map(|e| quoted(&print_symbolic(e)))
        .collect();
    format!(
        "{{\"status\":{},\"evaluated\":{},\"tried\":{},\"visits\":{},\"class_values\":{},\"micros\":{},\"correct\":{},\"holes\":{{{}}},\"expressions\":[{}]}}",
        quoted(result.status.name()),
        result.evaluated,
        tried,
        visits,
        class_values,
        micros,
        correct,
        fills.join(","),
        expressions.join(",")
    )
}

#[test]
#[ignore = "evidence harness: set ZENO_LOOP_SKETCH_STUDY and ZENO_LOOP_SKETCH_STUDY_OUT"]
fn sketch_search_study() {
    let cases = std::env::var("ZENO_LOOP_SKETCH_STUDY")
        .unwrap_or_else(|_| panic!("ZENO_LOOP_SKETCH_STUDY names the cases file"));
    let out = std::env::var("ZENO_LOOP_SKETCH_STUDY_OUT")
        .unwrap_or_else(|_| panic!("ZENO_LOOP_SKETCH_STUDY_OUT names the results file"));
    let text = fs::read_to_string(&cases).unwrap_or_else(|error| panic!("{cases}: {error}"));
    let mut lines = String::new();
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let parts: Vec<&str> = line.split('\t').collect();
        let [name, path, names, variant_text, cases_text] = parts[..] else {
            panic!("case line {line}");
        };
        let bytes = fs::read(path).unwrap_or_else(|error| panic!("{path}: {error}"));
        let program = import_program(&bytes).unwrap_or_else(|error| panic!("{path}: {error:?}"));
        let skip = |reason: &str| {
            format!(
                "{{\"instance\":{},\"skipped\":{}}}\n",
                quoted(name),
                quoted(reason)
            )
        };
        let Ok(vocab) = Vocabulary::new(
            names.split(',').map(str::to_owned).collect(),
            program.inputs().to_vec(),
            program.outputs().to_vec(),
            &variants(variant_text),
        ) else {
            lines.push_str(&skip("vocabulary"));
            continue;
        };
        let Ok(expressions) = from_program(&program, &vocab, 512) else {
            lines.push_str(&skip("expression-too-large"));
            continue;
        };
        let cases = cases_text.parse().ok();
        let Ok(subject) = Subject::new(program, vocab, BTreeMap::new(), cases, DEFAULT_MAX_TUPLES)
        else {
            lines.push_str(&skip("domain-over-budget"));
            continue;
        };
        for (j, expression) in expressions.iter().enumerate() {
            let constants: Vec<Vec<u8>> = positions(expression)
                .into_iter()
                .filter(|(_, node)| matches!(node, Expr::Int(_)))
                .map(|(path, _)| path)
                .collect();
            for holes in 2..=constants.len().min(6) {
                let mut state = seed(name, j, holes);
                let mut pool = constants.clone();
                for i in 0..holes {
                    let span = (pool.len() - i) as u64;
                    let pick = i + usize::try_from(next(&mut state) % span).unwrap_or(0);
                    pool.swap(i, pick);
                }
                let mut sketch = expression.clone();
                let mut opened = Vec::new();
                for path in &pool[..holes] {
                    if let Ok(Expr::Int(value)) =
                        zeno_fcis_loop_doors::inverse::node_at(&sketch, path).cloned()
                    {
                        opened.push(value.to_string());
                    }
                    sketch = replace_at(&sketch, path, &Expr::Hole(0, Hole::Const(None)))
                        .unwrap_or_else(|refusal| panic!("{refusal:?}"));
                }
                let outputs: Vec<Expr> = (0..expressions.len())
                    .map(|k| if k == j { sketch.clone() } else { Expr::Keep })
                    .collect();
                let _ = writeln!(
                    lines,
                    "{{\"instance\":{},\"output\":{j},\"holes\":{holes},\"tuples\":{},\"sketch\":{},\"opened\":[{}],\"meanings\":{},\"joint\":{}}}",
                    quoted(name),
                    subject.size,
                    quoted(&print_symbolic(&sketch)),
                    opened.join(","),
                    run(&subject, &outputs, j, Search::Meanings),
                    run(&subject, &outputs, j, Search::Joint),
                );
            }
        }
    }
    fs::write(&out, lines).unwrap_or_else(|error| panic!("{out}: {error}"));
}
