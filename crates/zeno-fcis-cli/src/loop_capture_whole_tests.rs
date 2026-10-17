use super::*;
use zeno_fcis_synthesis::finite::Op;

fn ok<T, E: std::fmt::Debug>(result: Result<T, E>) -> T {
    result.unwrap_or_else(|error| panic!("fixture: {error:?}"))
}

fn formula(bps: i64) -> (Program, String, String) {
    let skeleton = ok(spec_program(&json!({
        "inputs":vec![json!({"kind":"Int","min":"0","max":"40"});3],
        "outputs":vec![json!({"kind":"Int","min":"0","max":"40"});2],
    })));
    let fee = format!("mul_div_ceil(x2, {bps}, 10000)");
    let net = format!("(x2 - {fee})");
    let den = format!("(x0 + {net})");
    let output = format!("mul_div_floor({net}, x1, ({den} == 0 ? 1 : {den}))");
    (skeleton, fee, output)
}

fn proposed(skeleton: &Program, fee: &str, output: &str) -> Program {
    ok(proposed_program_for_abi(
        &ok(serde_json::to_vec(&json!({
            "schema":"zeno-fcis/capture-expressions/1","outputs":[fee,output],
        }))),
        true,
        skeleton,
    ))
}

fn complete(reference: &Program, candidate: &Program) -> Result<Value, Value> {
    let began = Instant::now();
    let mut longest_check = std::time::Duration::ZERO;
    let mut cover = ok(WholeCheck::new(candidate, 4096, 10_000_000));
    let mut work = ReferenceWork::default();
    while cover.next < cover.total {
        let start = cover.next;
        let points = ok(cover.points(start));
        // Same repeat/reverse/fresh-schedule reconciliation as production.
        // The test reference itself is an admitted D1 arithmetic program.
        let rows = ok(observed_table_with(
            reference,
            &points,
            &mut work,
            |schedule| {
                Ok(
                    json!({"rows":schedule.iter().map(|point| ok(reference.evaluate(point))).collect::<Vec<_>>()}),
                )
            },
        ));
        let check_began = Instant::now();
        let outcome = ok(cover.page(start, &rows));
        longest_check = longest_check.max(check_began.elapsed());
        if let Some(difference) = outcome {
            assert!(cover.finish().is_err());
            return Err(difference);
        }
    }
    assert_eq!(work.valid_observations_received, 6 * cover.total);
    assert_eq!(
        work.valid_observations_received,
        work.observations_scheduled
    );
    assert!(cover.reserved <= 10_000_000);
    eprintln!(
        "whole core elapsed={:?}; longest page check={longest_check:?}",
        began.elapsed()
    );
    Ok(ok(cover.finish()))
}

#[test]
fn whole_capture_scaled_cpmm_checks_every_point_for_each_fee() {
    for bps in [0, 30, 100] {
        let (skeleton, fee, output) = formula(bps);
        let reference = proposed(&skeleton, &fee, &output);
        let candidate = proposed(&skeleton, &fee, &format!("({output} + 0)"));
        let result = ok(complete(&reference, &candidate));
        assert_eq!(result["coverage"]["declared"], 68_921);
        assert_eq!(result["coverage"]["checked"], 68_921);
        assert_eq!(result["coverage"]["complete"], true);
        assert!(result["pieces_checked_and_replayed"].as_u64().unwrap_or(0) > 4096);
        eprintln!(
            "whole CPMM fee={bps}: {} node visits, {} exact/replayed pieces",
            result["reserved_node_visits"], result["pieces_checked_and_replayed"]
        );
    }
}

#[test]
fn whole_capture_reports_a_fault_at_the_last_declared_point() {
    let (skeleton, fee, output) = formula(30);
    let reference = proposed(&skeleton, &fee, &output);
    let candidate = proposed(
        &skeleton,
        &fee,
        &format!("x0 == 40 && x1 == 40 && x2 == 40 ? ({output} + 1) : {output}"),
    );
    let difference = complete(&reference, &candidate).expect_err("last-point defect accepted");
    assert_eq!(difference["ordinal"], 68_920);
    assert_eq!(difference["input"], json!([40, 40, 40]));
    assert_eq!(difference["expected"], json!([1, 19]));
    assert_eq!(difference["actual"]["output"], json!([1, 20]));
}

#[test]
fn whole_capture_product_comparison_stubs_fail_the_actual_checker_control() {
    let (mut skeleton, fee, output) = formula(30);
    // Keep this mutation control small; the full cover is exercised above.
    skeleton = ok(Program::try_new(
        vec![Domain::Int { min: 0, max: 2 }; 3],
        skeleton.outputs().to_vec(),
        skeleton.nodes().to_vec(),
        skeleton.roots().to_vec(),
    ));
    let reference = proposed(&skeleton, &fee, &output);
    for comparison in ["products_ge(x2, 0, 1, 1)", "products_ge(x2, 1, 0, 1, 1, 1)"] {
        let candidate = proposed(
            &skeleton,
            &fee,
            &format!("{comparison} ? ({output} + 1) : {output}"),
        );
        let control = |stub| {
            let nodes = candidate
                .nodes()
                .iter()
                .map(|op| {
                    if stub && matches!(op, Op::ProductsGe(..)) {
                        Op::Bool(true)
                    } else {
                        op.clone()
                    }
                })
                .collect();
            let checked = ok(Program::try_new(
                candidate.inputs().to_vec(),
                candidate.outputs().to_vec(),
                nodes,
                candidate.roots().to_vec(),
            ));
            complete(&reference, &checked).is_ok()
        };
        assert!(control(false));
        assert!(!control(true), "product stub survived whole capture");
    }
}

#[test]
fn whole_capture_rejects_partial_overlapping_pages_and_retains_work_on_stop() {
    let candidate = ok(Program::try_new(
        vec![Domain::Int { min: 0, max: 4 }],
        vec![Domain::Int { min: 0, max: 4 }],
        vec![Op::Input(0)],
        vec![0],
    ));
    let mut cover = ok(WholeCheck::new(&candidate, 2, 10_000));
    assert!(cover.finish().is_err());
    assert!(cover.page(0, &[vec![0]]).is_err());
    assert!(cover.page(2, &[vec![2], vec![3]]).is_err());
    assert!(ok(cover.page(0, &[vec![0], vec![1]])).is_none());
    let spent = cover.reserved;
    assert!(cover.page(0, &[vec![0], vec![1]]).is_err());
    assert_eq!(cover.reserved, spent);
    assert!(cover.finish().is_err());
    cover.work_limit = spent;
    assert_eq!(
        cover.page(2, &[vec![2], vec![3]]).err().as_deref(),
        Some("capture-whole-work-budget")
    );
    assert_eq!(cover.reserved, spent);
    assert!(cover.finish().is_err());
}

#[test]
fn whole_capture_keeps_canonical_product_order_and_arithmetic_refusal() {
    let skeleton = ok(spec_program(
        &json!({"inputs":[{"kind":"Int","min":"0","max":"3"},
        {"kind":"Int","min":"1","max":"4"}],"outputs":[{"kind":"Int","min":"0","max":"4"}]}),
    ));
    let reference = ok(proposed_program_for_abi(
        &ok(serde_json::to_vec(
            &json!({"schema":"zeno-fcis/capture-expressions/1","outputs":["x0"]}),
        )),
        true,
        &skeleton,
    ));
    let cover = ok(WholeCheck::new(&reference, 4096, 10_000));
    assert_eq!(
        ok(cover.points(0)),
        ok(proposal::points(reference.inputs(), 4096))
    );
    let bad = ok(proposed_program_for_abi(
        &ok(serde_json::to_vec(
            &json!({"schema":"zeno-fcis/capture-expressions/1","outputs":["mul_div_floor(x0, 1, 0)"]}),
        )),
        true,
        &skeleton,
    ));
    let difference = complete(&reference, &bad).expect_err("undefined arithmetic captured");
    assert_eq!(difference["ordinal"], 0);
    assert_eq!(difference["actual"]["status"], "refused");
}

#[test]
fn whole_capture_splits_an_encoding_that_exceeds_the_existing_node_cap() {
    let mut nodes = vec![Op::Input(0), Op::Int(31)];
    let mut roots = Vec::new();
    for factor in 1..=16 {
        let k = nodes.len() as u16;
        nodes.extend([
            Op::Int(factor),
            Op::Mul(0, k),
            Op::DivFloor(k + 1, 1),
            Op::Mul(k + 2, 1),
            Op::Sub(k + 1, k + 3),
        ]);
        roots.push(k + 4);
    }
    let candidate = ok(Program::try_new(
        vec![Domain::Int { min: 0, max: 31 }],
        vec![Domain::Int { min: 0, max: 30 }; 16],
        nodes,
        roots,
    ));
    let rows = (0..32)
        .map(|x| ok(candidate.evaluate(&[x])))
        .collect::<Vec<_>>();
    let large = ok(proposal::ReferenceTable::new(
        vec![Domain::Int { min: 0, max: 15 }],
        candidate.outputs().to_vec(),
        rows[..16].to_vec(),
    ));
    assert_eq!(
        large.program().err().map(|error| error.0).as_deref(),
        Some("reference-encoding-budget")
    );
    let mut cover = ok(WholeCheck::new(&candidate, 4096, 10_000_000));
    assert!(ok(cover.page(0, &rows)).is_none());
    let result = ok(cover.finish());
    assert_eq!(result["coverage"]["checked"], 32);
    assert!(
        result["pieces_checked_and_replayed"].as_u64().unwrap_or(0) > 2,
        "oversized lookup was not split"
    );
}
