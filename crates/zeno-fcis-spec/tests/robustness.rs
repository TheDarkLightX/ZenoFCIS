//! Generated-input robustness tests for the project parser and elaborator, in
//! pure Rust with no dependency. Seeds are every `project.zeno` under
//! `examples/`; a seeded generator mutates them at the token and byte level.
//! Every input must parse and elaborate to a result or a diagnostic, never a
//! panic. Set `ZENO_FCIS_ROBUSTNESS_ITERATIONS` for a longer run.

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};

use zeno_fcis_spec::{ProjectLimits, SourceLimits, elaborate_project, parse_project};

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n.max(1)
    }
}

fn iterations() -> u64 {
    std::env::var("ZENO_FCIS_ROBUSTNESS_ITERATIONS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1_000)
}

fn collect(dir: &Path, out: &mut Vec<String>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut paths: Vec<PathBuf> = entries.filter_map(|e| e.ok().map(|e| e.path())).collect();
    paths.sort();
    for path in paths {
        if path.is_dir() {
            collect(&path, out);
        } else if path.file_name().is_some_and(|n| n == "project.zeno")
            && let Ok(text) = std::fs::read_to_string(&path)
        {
            out.push(text);
        }
    }
}

fn seeds() -> Vec<String> {
    let mut out = Vec::new();
    collect(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples"),
        &mut out,
    );
    assert!(
        out.len() >= 10,
        "expected the example projects as seeds, found {}",
        out.len()
    );
    out
}

/// Tokens: runs of word characters, or single other characters (whitespace kept).
fn tokens(text: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for ch in text.chars() {
        let word = ch.is_ascii_alphanumeric() || ch == '_' || ch == '.';
        match out.last_mut() {
            Some(last)
                if word
                    && last
                        .chars()
                        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '.') =>
            {
                last.push(ch)
            }
            _ => out.push(ch.to_string()),
        }
    }
    out
}

const ODD: &[&str] = &[
    "0",
    "-1",
    "-0",
    "340282366920938463463374607431768211456",
    "170141183460469231731687303715884105728",
    "99999999999999999999999999999999999999999",
    "..=",
    "..",
    "{",
    "}",
    "[",
    "]",
    "(",
    ")",
    ";",
    ",",
    "=",
    "==",
    "->",
    "||",
    "&&",
    "!",
    "forall",
    "exists",
    "table",
    "derived",
    "law",
    "entry",
    "map",
    "type",
    "field",
    "rounds",
    "in",
    "of",
    "at",
    "pre.100.",
    "post.100.",
    "command.101.",
    "context.102.",
    "\u{0}",
    "\u{7f}",
    "é",
];

fn mutate(rng: &mut Rng, text: &str, other: &str) -> String {
    let mut t = tokens(text);
    for _ in 0..=rng.below(5) {
        if t.is_empty() {
            t.push(ODD[rng.below(ODD.len() as u64) as usize].to_string());
            continue;
        }
        let i = rng.below(t.len() as u64) as usize;
        match rng.below(8) {
            0 => {
                t.remove(i);
            }
            1 => {
                let dup = t[i].clone();
                t.insert(i, dup);
            }
            2 => {
                let j = rng.below(t.len() as u64) as usize;
                t.swap(i, j);
            }
            3 => t[i] = ODD[rng.below(ODD.len() as u64) as usize].to_string(),
            4 => t.insert(i, ODD[rng.below(ODD.len() as u64) as usize].to_string()),
            5 => t.truncate(i),
            6 => {
                let o = tokens(other);
                if !o.is_empty() {
                    let j = rng.below(o.len() as u64) as usize;
                    t.truncate(i);
                    t.extend_from_slice(&o[j..]);
                }
            }
            _ => {
                // replace a number-looking token with an extreme value
                if t[i].chars().all(|c| c.is_ascii_digit()) {
                    t[i] = ODD[rng.below(6) as usize].to_string();
                }
            }
        }
    }
    t.concat()
}

fn check(source: &str, seed: u64) {
    let outcome = catch_unwind(AssertUnwindSafe(|| {
        if let Ok(parsed) = parse_project(source, SourceLimits::default()) {
            let _ = elaborate_project(parsed, ProjectLimits::default());
        }
    }));
    if outcome.is_err() {
        let path = std::env::temp_dir().join(format!("zeno-fcis-parser-robustness-{seed}.zeno"));
        let _ = std::fs::write(&path, source);
        panic!(
            "parser or elaborator panicked; seed {seed}; input saved to {}",
            path.display()
        );
    }
}

#[test]
fn every_example_parses_and_elaborates_without_panic() {
    for (i, s) in seeds().iter().enumerate() {
        check(s, i as u64);
    }
}

#[test]
fn mutated_projects_never_panic() {
    let seeds = seeds();
    for seed in 0..iterations() {
        let mut rng = Rng(seed);
        let a = &seeds[rng.below(seeds.len() as u64) as usize];
        let b = &seeds[rng.below(seeds.len() as u64) as usize];
        check(&mutate(&mut rng, a, b), seed);
    }
}

#[test]
fn deep_nesting_is_refused_not_overflowed() {
    // A stack overflow aborts the process rather than panicking; if this test
    // dies with a signal, the parser has no nesting bound on that path.
    let seeds = seeds();
    let base = &seeds[0];
    for depth in [64usize, 256, 1_024, 4_096] {
        for (open, close) in [("(", ")"), ("!", ""), ("-", "")] {
            let expr = format!("{}1{}", open.repeat(depth), close.repeat(depth));
            let source = format!("{base}\nlaw 999 deep on commit = {expr} == 1;\n");
            check(&source, depth as u64);
        }
    }
}
