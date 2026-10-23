//! The recurrence table examples and an independent finite lasso comparison.

use std::process::Command;

#[path = "support/closed_table_laws.rs"]
mod closed_table_laws;

fn example(name: &str) -> String {
    let output = Command::new(env!("CARGO_BIN_EXE_zeno-fcis"))
        .current_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."))
        .args([
            "check",
            "--expand",
            &format!("examples/contract-tables/games/{name}/project.zeno"),
        ])
        .output()
        .unwrap_or_else(|error| panic!("example command: {error}"));
    assert!(output.status.success(), "{output:?}");
    String::from_utf8(output.stdout).unwrap_or_else(|error| panic!("example output: {error}"))
}

#[test]
fn recurrence_example_answers_every_state() {
    let expanded = example("recurrence");
    let laws = closed_table_laws::laws(&expanded);
    for (id, state, value) in [
        (500, "entry", true),
        (501, "good", true),
        (502, "return", true),
        (503, "isolated", false),
    ] {
        assert!(expanded.contains(&format!("law {id} from_{state} on commit = ")));
        assert_eq!(laws[&id], value, "{state}: {expanded}");
    }
}

#[test]
fn reflexive_trap_exposes_false_acceptance_of_an_isolated_good_state() {
    let expanded = example("recurrence-reflexive-trap");
    assert!(
        closed_table_laws::laws(&expanded)[&500],
        "the deliberately reflexive assertion must expose the false positive: {expanded}"
    );
    assert_eq!(lasso(1, 0, 1), [false], "no edge means no infinite run");
    assert_eq!(table_answers(1, 0, 1), [false]);
}

/// Enumerate simple prefixes and cycles, without computing a path relation.
fn lasso(n: usize, edges: usize, good: usize) -> Vec<bool> {
    (0..n)
        .map(|start| {
            let mut todo = vec![(start, vec![start])];
            while let Some((at, path)) = todo.pop() {
                for next in 0..n {
                    if edges & (1 << (at * n + next)) == 0 {
                        continue;
                    }
                    if let Some(index) = path.iter().position(|node| *node == next) {
                        if path[index..].iter().any(|node| good & (1 << node) != 0) {
                            return true;
                        }
                    } else {
                        let mut extended = path.clone();
                        extended.push(next);
                        todo.push((next, extended));
                    }
                }
            }
            false
        })
        .collect()
}

fn table_answers(n: usize, edges: usize, good: usize) -> Vec<bool> {
    let bits = |mask: usize| {
        (0..n)
            .map(|i| ((mask >> i) & 1).to_string())
            .collect::<Vec<_>>()
            .join(",")
    };
    let mut source = format!(
        "zeno 1; project 1 recurrence; type 100 state S; type 101 command C;
         type 102 context X; type 104 int Bit in 0..=1;
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
    let edge = |target: &str| {
        (0..n)
            .map(|i| format!("(i == {i} && edge{i}[{target}] == 1)"))
            .collect::<Vec<_>>()
            .join(" || ")
    };
    source.push_str(&format!(
        "derived table 700 path[i][j] in table 110 rounds {} {{
         default ({}) || exists k in table 110 {{ ({}) && path[k][j] }}; }};\n",
        n * n,
        edge("j"),
        edge("k")
    ));
    for i in 0..n {
        source.push_str(&format!(
            "law {} from_{i} on commit = exists a in table 110 {{ good[a] == 1 && path[{i}][a] && path[a][a] }};\n",
            500 + i
        ));
    }
    closed_table_laws::laws(&source).into_values().collect()
}

#[test]
fn recurrence_table_matches_lassos_on_all_graphs_through_three_states() {
    let mut checked = 0;
    for n in 1..=3 {
        for edges in 0..1 << (n * n) {
            for good in 0..1 << n {
                assert_eq!(
                    table_answers(n, edges, good),
                    lasso(n, edges, good),
                    "n={n}, edges={edges}, good={good}"
                );
                checked += 1;
            }
        }
    }
    assert_eq!(checked, 4_164);
}
