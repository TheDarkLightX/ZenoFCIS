//! File adapter for bounded counterexample screening. No qualification receipt.

use super::*;
use serde::Deserialize;
use zeno_fcis_loop_doors::fuzz::{self, Observation, State};

const SCHEMA: &str = "zeno-fcis/loop-fuzz/1";

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Binding {
    input: usize,
    output: usize,
    initial: i64,
}

#[derive(Deserialize)]
#[serde(default, deny_unknown_fields)]
struct Limits {
    seed: u64,
    trials: u32,
    pairs: u32,
    steps: usize,
    corpus: usize,
    uniform: bool,
}

impl Default for Limits {
    fn default() -> Self {
        let x = fuzz::Limits::default();
        Self {
            seed: x.seed,
            trials: x.trials,
            pairs: x.pairs,
            steps: x.steps,
            corpus: x.corpus,
            uniform: x.uniform,
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    schema: String,
    #[serde(default)]
    state: Vec<Binding>,
    #[serde(default)]
    seeds: Vec<fuzz::Trace>,
    #[serde(default)]
    limits: Limits,
}

fn observed(value: &Observation) -> Value {
    match value {
        Observation::Values(values) => json!({"outputs": values}),
        Observation::Arithmetic => json!({"error": "arithmetic"}),
        Observation::OutputDomain => json!({"error": "output-domain"}),
    }
}

pub(super) fn command(original: &Path, candidate: &Path, request: &Path) -> (u8, Value) {
    let load = |path: &Path| -> Result<(Vec<u8>, Program), (u8, Value)> {
        let bytes = read_bytes(path)?;
        let program = import_program(&bytes).map_err(|_| refused("fuzz-program-import"))?;
        Ok((bytes, program))
    };
    let (original_bytes, original) = match load(original) {
        Ok(p) => p,
        Err(e) => return e,
    };
    let (candidate_bytes, candidate) = match load(candidate) {
        Ok(p) => p,
        Err(e) => return e,
    };
    let bytes = match read_bytes(request) {
        Ok(b) => b,
        Err(e) => return e,
    };
    let request: Request = match serde_json::from_slice(&bytes) {
        Ok(r) => r,
        Err(_) => return refused("fuzz-request-shape"),
    };
    if request.schema != SCHEMA {
        return refused("fuzz-request-schema");
    }
    let state: Vec<_> = request
        .state
        .into_iter()
        .map(|s| State {
            input: s.input,
            output: s.output,
            initial: s.initial,
        })
        .collect();
    let l = request.limits;
    let limits = fuzz::Limits {
        seed: l.seed,
        trials: l.trials,
        pairs: l.pairs,
        steps: l.steps,
        corpus: l.corpus,
        uniform: l.uniform,
    };
    let report = match fuzz::screen(&original, &candidate, &state, &request.seeds, limits) {
        Ok(r) => r,
        Err(e) => return refused(&e.0),
    };
    let witness = report.witness.as_ref().map(|w| {
        json!({
            "trace": w.trace, "step": w.step, "input": w.input,
            "reference": observed(&w.reference), "candidate": observed(&w.candidate),
            "replay": "matched", "minimality": "not-claimed",
        })
    });
    let proposer = [
        zeno_fcis_loop_doors::source_fingerprint::FUZZ,
        include_str!("loop_fuzz.rs"),
    ]
    .concat();
    // Both outcomes are screening reports. Neither has a receipt or accepts P.
    (
        0,
        json!({
            "schema": SCHEMA, "authority": "none", "qualification": "not-run",
            "status": if witness.is_some() { "counterexample" } else { "inconclusive" },
            "original_sha256": sha256_hex(&original_bytes),
            "candidate_sha256": sha256_hex(&candidate_bytes),
            "request_sha256": sha256_hex(&bytes), "proposer_sha256": sha256_hex(proposer.as_bytes()),
            "observation_profile": original.profile(),
            "strategy": if limits.uniform { "uniform" } else { "boundary-and-corpus" },
            "work": {"trials": report.trials, "evaluation_pairs": report.pairs,
                "evaluator_calls": u64::from(report.pairs) * 2},
            "counterexample": witness, "seeds": report.corpus,
            "next": "Run transform check and replay before accepting a candidate.",
        }),
    )
}
