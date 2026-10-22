//! Replay frozen checker vectors through the actual i64 evaluator. Archived
//! receipt bindings are checked as data; this grants no current receipt authority.
use super::super::values_json;
use super::*;
use std::path::Path;
use zeno_fcis_synthesis::finite::{V2Resource, v2_zero_limits};
use zeno_fcis_synthesis::finite_runtime::{LegacyProgram, import_i64_program};

fn observe(program: &LegacyProgram, input: &[i64]) -> Observation {
    let (result, usage) = program
        .execute_v2(
            input,
            v2_zero_limits().with_limit(V2Resource::Step, FULL_BUDGET),
        )
        .into_parts();
    Observation {
        result: result.map(|row| row.into_iter().map(i128::from).collect()),
        steps: usage.used(V2Resource::Step),
    }
}

#[test]
fn historical_transform_vectors_and_receipts_keep_i64_observations_and_usage() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/transform-check");
    for version in [1, 2, 3] {
        let directory = root.join(version.to_string());
        let read =
            |name: &str| std::fs::read(directory.join(name)).unwrap_or_else(|e| panic!("{e:?}"));
        let manifest: Value =
            serde_json::from_slice(&read("vectors.json")).unwrap_or_else(|e| panic!("{e:?}"));
        assert_eq!(
            manifest["semantics"],
            format!("zeno-fcis/transform-check/{version}")
        );
        for vector in manifest["vectors"]
            .as_array()
            .unwrap_or_else(|| panic!("expected a value"))
        {
            let name = |key: &str| {
                vector[key]
                    .as_str()
                    .unwrap_or_else(|| panic!("expected a value"))
            };
            let bytes = [read(name("original")), read(name("candidate"))];
            let programs = bytes.each_ref().map(|data| {
                let program = import_i64_program(data).unwrap_or_else(|e| panic!("{e:?}"));
                assert_eq!(
                    program
                        .value()
                        .unwrap_or_else(|e| panic!("{e:?}"))
                        .zcve_bytes()
                        .unwrap_or_else(|e| panic!("{e:?}")),
                    *data
                );
                program
            });
            let sources = ["original", "candidate"].map(|key| {
                let path = Path::new(name(key)).with_extension("json");
                serde_json::from_slice::<Value>(
                    &std::fs::read(directory.join(path)).unwrap_or_else(|e| panic!("{e:?}")),
                )
                .unwrap_or_else(|e| panic!("{e:?}"))
            });
            let expected: Value =
                serde_json::from_slice(&read(name("expected"))).unwrap_or_else(|e| panic!("{e:?}"));
            for (side, data) in ["original", "candidate"].into_iter().zip(&bytes) {
                assert_eq!(expected[format!("{side}_sha256")], sha256(data));
            }
            if sources[0]["outputs"] != sources[1]["outputs"] {
                assert_eq!(expected["status"], "refused");
                assert_eq!(expected["detail"]["reason"], "output-abi-mismatch");
                continue;
            }
            let bounds: Vec<(i64, i64)> = sources[0]["inputs"]
                .as_array()
                .unwrap_or_else(|| panic!("expected a value"))
                .iter()
                .map(|d| {
                    if d["kind"] == "Bool" {
                        (0, 1)
                    } else {
                        (
                            d["min"]
                                .as_str()
                                .unwrap_or_else(|| panic!("expected a value"))
                                .parse()
                                .unwrap_or_else(|e| panic!("{e:?}")),
                            d["max"]
                                .as_str()
                                .unwrap_or_else(|| panic!("expected a value"))
                                .parse()
                                .unwrap_or_else(|e| panic!("{e:?}")),
                        )
                    }
                })
                .collect();
            let size = bounds.iter().fold(1u128, |n, (min, max)| {
                n * (i128::from(*max) - i128::from(*min) + 1) as u128
            });
            if size
                > u128::from(
                    vector["max_input_tuples"]
                        .as_u64()
                        .unwrap_or_else(|| panic!("expected a value")),
                )
            {
                assert_eq!(expected["status"], "inconclusive");
                assert_eq!(expected["detail"]["cause"], "domain-too-large");
                assert_eq!(expected["detail"]["domain_size"], size.to_string());
                continue;
            }
            assert!(size <= 1000);
            let mut input: Vec<_> = bounds.iter().map(|b| b.0).collect();
            let mut max_steps = [0u64; 2];
            let mut over = [0u64; 2];
            let mut more = 0;
            let mut preserved = true;
            let mut different = None;
            for ordinal in 0..size {
                let observations = programs.each_ref().map(|p| observe(p, &input));
                if different.is_none() && observations[0].result != observations[1].result {
                    different = Some((
                        ordinal,
                        input.clone(),
                        observations.each_ref().map(|o| Observation {
                            result: o.result.clone(),
                            steps: o.steps,
                        }),
                    ));
                }
                for side in 0..2 {
                    max_steps[side] = max_steps[side].max(observations[side].steps);
                    over[side] += u64::from(
                        observations[side].steps
                            > vector["step_limit"]
                                .as_u64()
                                .unwrap_or_else(|| panic!("expected a value")),
                    );
                }
                more += u64::from(observations[1].steps > observations[0].steps);
                preserved &= observations[0].steps == observations[1].steps;
                for (value, (min, max)) in input.iter_mut().zip(&bounds).rev() {
                    if *value == *max {
                        *value = *min;
                    } else {
                        *value += 1;
                        break;
                    }
                }
            }
            if let Some((ordinal, input, observations)) = different {
                assert_eq!(expected["status"], "counterexample");
                assert_eq!(expected["detail"]["ordinal"], ordinal as u64);
                assert_eq!(
                    expected["detail"]["input"],
                    values_json(&input.into_iter().map(i128::from).collect::<Vec<_>>())
                );
                for (side, o) in ["original", "candidate"].into_iter().zip(observations) {
                    assert_eq!(
                        expected["detail"][side],
                        json!({"ok":o.result.unwrap_or_else(|e| panic!("{e:?}")).iter().map(ToString::to_string).collect::<Vec<_>>(),"steps":o.steps})
                    );
                }
            } else {
                let usage = json!({"candidate_uses_more":more,"max_steps":{"original":max_steps[0],"candidate":max_steps[1]},"usage_preserved":preserved});
                if over != [0, 0] {
                    assert_eq!(expected["status"], "inconclusive");
                    assert_eq!(expected["detail"]["cause"], "budget-boundary");
                    assert_eq!(
                        expected["detail"]["over_limit"],
                        json!({"original":over[0],"candidate":over[1]})
                    );
                    assert_eq!(
                        expected["detail"]["minimum_step_limit"],
                        max_steps
                            .into_iter()
                            .max()
                            .unwrap_or_else(|| panic!("expected a value"))
                    );
                    assert_eq!(expected["detail"]["usage"], usage);
                } else {
                    assert_eq!(expected["status"], "equivalent");
                    let receipt = read(name("receipt"));
                    let archived: Value =
                        serde_json::from_slice(&receipt).unwrap_or_else(|e| panic!("{e:?}"));
                    assert_eq!(archived, expected["detail"]["receipt"]);
                    assert_eq!(sha256(&receipt), expected["detail"]["receipt_sha256"]);
                    assert_eq!(archived["checker"]["semantics"], manifest["semantics"]);
                    assert_eq!(archived["inputs_checked"], size as u64);
                    assert_eq!(archived["usage"], usage);
                }
            }
        }
    }
}
