//! Native line adapter for the proved abstract payment-model interpreter.
//! Parsing and JSON presentation are test infrastructure, outside the proof.
#[path = "effect_protocol_runtime.rs"]
mod runtime;
use std::io::{self, BufRead};

fn evaluate(line: &str) -> Result<String, String> {
    let numbers: Vec<u64> = line
        .split_whitespace()
        .map(str::parse)
        .collect::<Result<_, _>>()
        .map_err(|e| format!("{e}"))?;
    if numbers.len() < 10 || numbers[9] > 128 || numbers.len() != 10 + numbers[9] as usize {
        return Err("model input length".into());
    }
    if numbers[2..8].iter().any(|n| *n > 1) || numbers[8] > 255 {
        return Err("model input range".into());
    }
    let capacity = usize::try_from(numbers[0]).map_err(|e| format!("{e}"))?;
    let keys = numbers[10..].to_vec();
    let admission = runtime::prepare(
        &keys,
        capacity,
        numbers[1],
        numbers[2] != 0,
        numbers[3] != 0,
        numbers[4] != 0,
        numbers[5] != 0,
    );
    let refused = admission.verdicts.iter().any(|v| *v != 0);
    let (reserved, copies) = runtime::finish(
        &keys,
        admission.reserved,
        refused,
        numbers[6] != 0,
        numbers[7] != 0,
        numbers[8] as u8,
    );
    Ok(format!(
        "{{\"reserved\":{reserved:?},\"verdicts\":{:?},\"copies\":{copies}}}",
        admission.verdicts
    ))
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    for line in io::stdin().lock().lines() {
        let line = line?;
        println!("{}", evaluate(&line)?);
    }
    Ok(())
}
