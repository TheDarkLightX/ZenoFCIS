//! Stability is a certificate condition, separate from reaching the right value.
//! Like the games guide, consumers must require the expanded `stable` law to
//! be true. `check --expand` itself only validates and expands declarations.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

#[path = "support/closed_table_laws.rs"]
mod closed_table_laws;

struct Scratch(PathBuf);

impl Scratch {
    fn new() -> Self {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_else(|error| panic!("fixture clock: {error}"))
            .as_nanos();
        let path =
            std::env::temp_dir().join(format!("table-stability-{}-{nonce}", std::process::id()));
        std::fs::create_dir(&path).unwrap_or_else(|error| panic!("fixture directory: {error}"));
        Self(path)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn certificate(n: usize, edges: u64, good: u64, updates: usize) -> BTreeMap<u32, bool> {
    let bits = |mask: u64| {
        (0..n)
            .map(|i| ((mask >> i) & 1).to_string())
            .collect::<Vec<_>>()
            .join(",")
    };
    let mut source = format!(
        "zeno 1; project 1 stability; type 100 state S; type 101 command C;
         type 102 context X; type 103 int Zero in 0..=0; type 104 int Bit in 0..=1;
         field 300 100 padding 103; field 301 101 dummy 103;
         variant 302 102 Session none;
         const table 110 good 104 size {n} [{}];\n",
        bits(good)
    );
    for i in 0..n {
        source.push_str(&format!(
            "const table {} edge{i} 104 size {n} [{}];\n",
            120 + i,
            bits(edges >> (i * n))
        ));
    }
    source.push_str(
        "component 400 core { owns 100; reads pre.100; writes post.100;
         contexts context.102; budget steps 4096; } merge [400];\n",
    );
    let edge = (0..n)
        .map(|i| format!("(i == {i} && edge{i}[j] == 1)"))
        .collect::<Vec<_>>()
        .join(" || ");
    let has_move = format!("exists j in table 110 {{ {edge} }}");
    let predecessor = |table: &str| {
        format!("(({has_move}) && forall j in table 110 {{ !({edge}) || {table}[j] }})")
    };
    for step in 1..=updates {
        let previous = if step == 1 {
            has_move.clone()
        } else {
            predecessor(&format!("z{}", step - 1))
        };
        source.push_str(&format!(
            "derived table {} z{step}[i] in table 110 rounds {n} {{
             default {} || (good[i] == 1 && ({previous})); }};\n",
            700 + step,
            predecessor(&format!("z{step}"))
        ));
    }
    source.push_str("law 499 domain on commit, genesis = true;\n");
    for i in 0..n {
        source.push_str(&format!(
            "law {} win{i} on commit = z{updates}[{i}];\n",
            500 + i
        ));
    }
    let previous = if updates == 1 {
        "true".to_owned()
    } else {
        format!("z{}[i]", updates - 1)
    };
    source.push_str(&format!(
        "law 600 stable on commit = forall i in table 110 {{
         (!z{updates}[i] || {previous}) && (!({previous}) || z{updates}[i]) }};\n"
    ));
    // Retain every computed approximant, not just the last winning set.
    for step in 1..=updates {
        for i in 0..n {
            source.push_str(&format!(
                "law {} step{step}_{i} on commit = z{step}[{i}];\n",
                1000 + step * 64 + i
            ));
        }
    }
    let scratch = Scratch::new();
    let path = scratch.0.join("project.zeno");
    std::fs::write(&path, &source).unwrap_or_else(|error| panic!("fixture write: {error}"));
    let output = Command::new(env!("CARGO_BIN_EXE_zeno-fcis"))
        .args(["--no-kb", "check", "--expand"])
        .arg(&path)
        .output()
        .unwrap_or_else(|error| panic!("fixture expansion: {error}"));
    assert!(output.status.success(), "{source}\n{output:?}");
    let expanded = String::from_utf8(output.stdout)
        .unwrap_or_else(|error| panic!("fixture expansion encoding: {error}"));
    closed_table_laws::laws(&expanded)
}

fn chain(n: usize) -> u64 {
    (0..n).fold(0, |edges, i| edges | (1 << (i * n + (i + 1).min(n - 1))))
}

fn answers(laws: &BTreeMap<u32, bool>, n: usize) -> Vec<bool> {
    (0..n).map(|i| laws[&(500 + i as u32)]).collect()
}

#[test]
fn total_chains_need_an_adjacent_repeat_after_reaching_the_fixed_point() {
    for n in 1..=8 {
        for updates in 1..=n + 1 {
            let laws = certificate(n, chain(n), (1 << (n - 1)) - 1, updates);
            for step in 1..=updates {
                let row: Vec<_> = (0..n)
                    .map(|i| laws[&((1000 + step * 64 + i) as u32)])
                    .collect();
                let expected: Vec<_> = (0..n).map(|i| i < n.saturating_sub(step)).collect();
                assert_eq!(row, expected, "n={n}, updates={updates}, step={step}");
            }
            assert_eq!(
                laws[&600],
                updates == n + 1,
                "n={n}, updates={updates}: stability must require an adjacent repeat"
            );
        }
    }
}

#[test]
fn full_chain_certificate_is_accepted() {
    for n in 1..=8 {
        let updates = n + 1;
        let laws = certificate(n, chain(n), (1 << (n - 1)) - 1, updates);
        assert!(
            laws[&600],
            "n={n}, updates={updates}: the full certificate must have an adjacent repeat"
        );
        assert_eq!(answers(&laws, n), vec![false; n]);
    }
}

/// Independently enumerate reachable simple cycles. All supplied graphs are total.
fn every_lasso_is_good(n: usize, edges: u64, good: u64) -> Vec<bool> {
    assert!((0..n).all(|i| (0..n).any(|j| edges & (1 << (i * n + j)) != 0)));
    (0..n)
        .map(|start| {
            let mut todo = vec![(start, vec![start])];
            while let Some((at, path)) = todo.pop() {
                for next in 0..n {
                    if edges & (1 << (at * n + next)) == 0 {
                        continue;
                    }
                    if let Some(index) = path.iter().position(|node| *node == next) {
                        if path[index..].iter().all(|node| good & (1 << node) == 0) {
                            return false;
                        }
                    } else {
                        let mut extended = path.clone();
                        extended.push(next);
                        todo.push((next, extended));
                    }
                }
            }
            true
        })
        .collect()
}

#[test]
fn premature_certificates_are_refused_even_when_the_answer_is_right() {
    let mut cases = Vec::new();
    for n in 1..=8 {
        for updates in 1..=n + 1 {
            cases.push((n, chain(n), (1 << (n - 1)) - 1, updates, updates == n + 1));
        }
    }
    // Total two-state graphs: two self-loops; then add the escape 0 -> 1.
    for edges in [9, 11] {
        for updates in 1..=3 {
            cases.push((
                2,
                edges,
                1,
                updates,
                updates >= if edges == 9 { 2 } else { 3 },
            ));
        }
    }
    let (mut wrong, mut lucky, mut accepted) = (0, 0, 0);
    for (n, edges, good, updates, stable) in cases {
        let laws = certificate(n, edges, good, updates);
        assert_eq!(
            laws[&600], stable,
            "n={n}, edges={edges}, updates={updates}: certificate stability"
        );
        let actual = answers(&laws, n);
        let expected = every_lasso_is_good(n, edges, good);
        if laws[&600] {
            assert_eq!(actual, expected, "n={n}, edges={edges}, updates={updates}");
            accepted += 1;
        } else if actual == expected {
            lucky += 1;
        } else {
            wrong += 1;
        }
        println!(
            "n={n} edges={edges} good={good} updates={updates} stable={} answer={actual:?} lasso={expected:?}",
            laws[&600]
        );
    }
    assert_eq!(
        (wrong, lucky, accepted),
        (29, 10, 11),
        "every premature certificate, including a right answer without a repeat, must be refused"
    );
}
