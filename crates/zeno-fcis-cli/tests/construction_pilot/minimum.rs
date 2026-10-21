//! Exhaustive evidence for the designer-approved fixed-helper substitution.
use super::*;
use zeno_fcis_loop_doors::expr::{Fill, Hole};

// Retained pre-fix scan, including its strict first-cell tie behavior. It is
// test-only and never supplies a construction proposal or deciding reference.
fn previous_minimum(layout: Layout, reverse_tie: bool) -> Expr {
    let (mut seen, mut best_id, mut best_p, mut best_t) = (lit(0), lit(0), lit(0), lit(0));
    for j in 0..layout.cells() {
        let c = var(layout.cell_name(j));
        let has = present(c.clone());
        let id = identity(layout, c.clone(), j);
        let p = priority(layout, c.clone());
        let t = stamp(layout, c, j);
        let tie = if reverse_tie {
            lt(best_t.clone(), t.clone())
        } else {
            lt(t.clone(), best_t.clone())
        };
        let better = and(
            has.clone(),
            or(
                not(seen.clone()),
                or(
                    lt(p.clone(), best_p.clone()),
                    and(eq(p.clone(), best_p.clone()), tie),
                ),
            ),
        );
        best_id = choose(better.clone(), id, best_id);
        best_p = choose(better.clone(), p, best_p);
        best_t = choose(better, t, best_t);
        seen = or(seen, has);
    }
    best_id
}
fn direct_minimum(layout: Layout, raw: &[i128], reverse: bool) -> i128 {
    raw[1..]
        .iter()
        .enumerate()
        .filter(|(_, c)| **c != 0)
        .map(|(j, c)| {
            let p = (c - 1) % 2;
            let t = if layout == Layout::T {
                j as i128
            } else {
                (c - 1) / 2
            };
            let id = if layout == Layout::T {
                (c - 1) / 2
            } else {
                j as i128
            };
            ((p, if reverse { -t } else { t }, j), id)
        })
        .min_by_key(|(key, _)| *key)
        .map_or(0, |(_, id)| id)
}
fn evidence() -> std::path::PathBuf {
    std::env::var_os("E")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| panic!("E evidence required; no skip"))
}
fn old_bundle() -> std::path::PathBuf {
    std::env::var_os("ZENO_FCIS_CONSTRUCTION_PREVIOUS_BUNDLE")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| panic!("retained previous bundle required; no skip"))
}
fn columns(layout: Layout) -> (Vocabulary, Vec<Vec<i128>>) {
    let features = Features::new(layout);
    let width = layout.cells() + 3;
    let vocab = Vocabulary::new(
        features.names[..width].to_vec(),
        features.domains[..width].to_vec(),
        vec![],
        &BTreeMap::new(),
    )
    .unwrap_or_else(|error| panic!("required test result: {error:?}"));
    let mut columns = vec![Vec::new(); width];
    for s in 0..layout.raw_count() {
        let mut row = layout.raw(s);
        row.extend([0, 0]);
        for (col, n) in columns.iter_mut().zip(row) {
            col.push(n);
        }
    }
    (vocab, columns)
}
#[test]
fn both_minimum_modes_match_retained_scan_and_direct_order_on_all_raw_states() {
    let mut counts = Vec::new();
    for layout in [Layout::T, Layout::I] {
        let (vocab, columns) = columns(layout);
        let env = expr::Env {
            vocab: &vocab,
            columns: &columns,
            keep: None,
            len: layout.raw_count(),
        };
        for reverse in [false, true] {
            let old = expr::evaluate(&previous_minimum(layout, reverse), &env, &BTreeMap::new())
                .unwrap_or_else(|error| panic!("required test result: {error:?}"));
            let new = expr::evaluate(&minimum(layout, reverse), &env, &BTreeMap::new())
                .unwrap_or_else(|error| panic!("required test result: {error:?}"));
            let expected: Vec<_> = (0..layout.raw_count())
                .map(|s| direct_minimum(layout, &layout.raw(s), reverse))
                .collect();
            assert_eq!(new, expected, "direct order {layout:?}/{reverse}");
            assert_eq!(new, old, "retained scan {layout:?}/{reverse}");
            counts.push(json!({"layout":layout.name(),"reverse_tie":reverse,"raw_states":new.len(),"cell_tuples":layout.raw_count()/3,"old_nodes":expr::size(&previous_minimum(layout,reverse)),"new_nodes":expr::size(&minimum(layout,reverse))}));
        }
    }
    println!("minimum-equivalence: {}", json!(counts));
}
fn completions(sketch: &Expr) -> Vec<Expr> {
    let mut choices = vec![BTreeMap::new()];
    for hole in expr::holes_of(sketch) {
        let Expr::Hole(id, Hole::Choice(alts)) = hole else {
            panic!("unchanged finite choices only")
        };
        choices = choices
            .into_iter()
            .flat_map(|a| {
                (0..alts.len()).map(move |i| {
                    let mut b = a.clone();
                    b.insert(*id, Fill::Choice(i));
                    b
                })
            })
            .collect();
    }
    choices
        .iter()
        .map(|a| {
            expr::fill(sketch, a).unwrap_or_else(|error| panic!("required test result: {error:?}"))
        })
        .collect()
}
#[test]
fn every_finite_output_choice_preserves_scalar_results_and_expanded_bounds() {
    let mut reports = Vec::new();
    super::super::files::verify(&old_bundle())
        .unwrap_or_else(|error| panic!("required test result: {error:?}"));
    for layout in [Layout::T, Layout::I] {
        let new = Features::new(layout);
        let mut old = Features::new(layout);
        old.helpers
            .insert("minimum_id".into(), previous_minimum(layout, false));
        let alpha = serde_json::from_slice::<Vec<Option<usize>>>(
            &read(
                &old_bundle().join(layout.name()).join("alpha.json"),
                SOURCE_LIMIT,
            )
            .unwrap_or_else(|error| panic!("required test result: {error:?}")),
        )
        .unwrap_or_else(|error| panic!("required test result: {error:?}"));
        let retained = strict(
            &read(
                &old_bundle().join(layout.name()).join("grammar.json"),
                DOCUMENT_LIMIT,
            )
            .unwrap_or_else(|error| panic!("required test result: {error:?}")),
        )
        .unwrap_or_else(|error| panic!("required test result: {error:?}"));
        assert_eq!(
            old.grammar(&alpha)
                .unwrap_or_else(|error| panic!("required test result: {error:?}")),
            retained,
            "retained helper, holes, alternatives and enumeration"
        );
        let mut expected = retained;
        expected["helpers"]["minimum_id"] = json!(
            policy_text(&minimum(layout, false))
                .unwrap_or_else(|error| panic!("required test result: {error:?}"))
        );
        assert_eq!(
            new.grammar(&alpha)
                .unwrap_or_else(|error| panic!("required test result: {error:?}")),
            expected,
            "only the minimum helper changes"
        );
        let rows = layout.raw_count() * 16;
        let mut before = vec![Vec::new(); new.names.len()];
        let mut after = before.clone();
        for s in 0..layout.raw_count() {
            for q in 0..16 {
                let a = old
                    .values(&layout.raw(s), q)
                    .unwrap_or_else(|error| panic!("required test result: {error:?}"));
                let b = new
                    .values(&layout.raw(s), q)
                    .unwrap_or_else(|error| panic!("required test result: {error:?}"));
                assert_eq!(a, b, "all columns {layout:?}/{s}/{q}");
                for (col, v) in before.iter_mut().zip(a) {
                    col.push(v);
                }
                for (col, v) in after.iter_mut().zip(b) {
                    col.push(v);
                }
            }
        }
        let vocab = Vocabulary::new(
            new.names.clone(),
            new.domains.clone(),
            vec![],
            &BTreeMap::new(),
        )
        .unwrap_or_else(|error| panic!("required test result: {error:?}"));
        let env_old = expr::Env {
            vocab: &vocab,
            columns: &before,
            keep: None,
            len: rows,
        };
        let env_new = expr::Env {
            vocab: &vocab,
            columns: &after,
            keep: None,
            len: rows,
        };
        let mut outputs = Vec::new();
        for (j, sketch) in new
            .sketches()
            .unwrap_or_else(|error| panic!("required test result: {error:?}"))
            .iter()
            .enumerate()
        {
            let filled = completions(sketch);
            let mut max_nodes = 0;
            let mut max_bytes = 0;
            let mut max_depth = 0;
            for expression in &filled {
                assert_eq!(
                    expr::evaluate(expression, &env_old, &BTreeMap::new())
                        .unwrap_or_else(|error| panic!("required test result: {error:?}")),
                    expr::evaluate(expression, &env_new, &BTreeMap::new())
                        .unwrap_or_else(|error| panic!("required test result: {error:?}"))
                );
                let expanded = new
                    .expand(expression)
                    .unwrap_or_else(|error| panic!("required test result: {error:?}"));
                max_nodes = max_nodes.max(expr::size(&expanded));
                max_bytes = max_bytes.max(expr::print_symbolic(&expanded).len());
                max_depth = max_depth.max(expr::depth(&expanded));
                assert!(expr::print_symbolic(&expanded).is_ascii());
            }
            let bound = match (layout, j) {
                (Layout::T, 0) => 328,
                (Layout::I, 0) => 438,
                (_, 1) => 13,
                (Layout::T, _) => 387,
                (Layout::I, _) => 392,
            };
            assert!(
                max_nodes <= bound && max_nodes <= expr::MAX_NODES,
                "{layout:?}/{j}: {max_nodes} > {bound}"
            );
            assert!(max_bytes <= expr::MAX_TEXT && max_depth <= expr::MAX_DEPTH);
            outputs.push(json!({"output":j,"choices":filled.len(),"rows_per_choice":rows,"max_nodes":max_nodes,"max_bytes":max_bytes,"max_depth":max_depth}));
        }
        let mut max_event = 0;
        for event in new
            .event()
            .unwrap_or_else(|error| panic!("required test result: {error:?}"))
        {
            let expanded = new
                .expand(&event)
                .unwrap_or_else(|error| panic!("required test result: {error:?}"));
            max_event = max_event.max(expr::size(&expanded));
            assert!(
                expr::print_symbolic(&expanded).is_ascii()
                    && expr::print_symbolic(&expanded).len() <= expr::MAX_TEXT
                    && expr::depth(&expanded) <= expr::MAX_DEPTH
            );
        }
        assert!(max_event <= if layout == Layout::T { 110 } else { 160 });
        reports.push(json!({"layout":layout.name(),"raw_states":layout.raw_count(),"raw_inputs":rows,"outputs":outputs,"max_event_nodes":max_event,"all_combinations":"preserved by equal input columns and exhaustive independent output choices"}));
    }
    println!("minimum-finite-choices: {}", json!(reports));
}
#[test]
fn historical_first_t_attempt_is_only_a_root_lowering_regression_fixture() {
    let root = evidence().join("T/attempts/0000");
    super::super::files::verify(&root)
        .unwrap_or_else(|error| panic!("required test result: {error:?}"));
    let old_inventory = strict(
        &read(&root.join("inventory.json"), DOCUMENT_LIMIT)
            .unwrap_or_else(|error| panic!("required test result: {error:?}")),
    )
    .unwrap_or_else(|error| panic!("required test result: {error:?}"));
    assert_eq!(old_inventory["root_expression_nodes"][2], 643);
    let proposal = strict(
        &read(&root.join("proposal.json"), DOCUMENT_LIMIT)
            .unwrap_or_else(|error| panic!("required test result: {error:?}")),
    )
    .unwrap_or_else(|error| panic!("required test result: {error:?}"));
    let expressions: Vec<_> = proposal["outputs"]
        .as_array()
        .unwrap_or_else(|| panic!("required test value missing"))
        .iter()
        .map(|v| {
            expr::parse(
                v["expression"]
                    .as_str()
                    .unwrap_or_else(|| panic!("required test value missing")),
                0,
                Allow {
                    holes: false,
                    params: false,
                    keep: false,
                },
            )
            .unwrap_or_else(|error| panic!("required test result: {error:?}"))
        })
        .collect();
    let (project, policy) = super::super::proposal::sources(Layout::T, &expressions)
        .unwrap_or_else(|error| panic!("required test result: {error:?}"));
    let compiled = super::super::compilation::compile(
        Layout::T,
        &project,
        std::str::from_utf8(&policy)
            .unwrap_or_else(|error| panic!("required test result: {error:?}")),
    )
    .unwrap_or_else(|error| panic!("required test result: {error:?}"));
    assert!(
        compiled.refusal.is_none(),
        "regression root/lowering refusal: {:?}",
        compiled.refusal
    );
    assert_eq!(compiled.inventory["root_expression_nodes"][2], 331);
    assert_eq!(compiled.inventory["inputs"], 5);
    assert_eq!(compiled.inventory["outputs"], 8);
    assert_eq!(compiled.inventory["root_count"], 8);
    assert_eq!(compiled.inventory["scalar_totality"]["evaluations"], 4704);
    assert_eq!(compiled.inventory["scalar_totality"]["complete"], true);
    println!("minimum-root-regression: {}", compiled.inventory);
}
