//! Identical local benchmark executable linked to two independently built crates.
use std::hint::black_box;
use std::time::Instant;
use zeno_fcis_spec::{SourceLimits, parse_project};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let manifest = std::env::args()
        .nth(1)
        .ok_or("missing benchmark manifest")?;
    let digest = zeno_fcis_spec::source_digest()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    println!("{{\"metadata\":{{\"spec_source_digest\":\"{digest}\"}}}}");
    for line in std::fs::read_to_string(manifest)?.lines() {
        let fields = line.split('\t').collect::<Vec<_>>();
        if fields.len() != 3 {
            return Err("invalid benchmark manifest row".into());
        }
        let name = fields[0];
        let source = std::fs::read_to_string(fields[1])?;
        let rounds = fields[2].parse::<usize>()?;
        if rounds == 0 || !name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_') {
            return Err("invalid benchmark case or iteration count".into());
        }
        let accepted = parse_project(&source, SourceLimits::default()).is_ok();
        for _ in 0..5 {
            let _ = black_box(parse_project(black_box(&source), SourceLimits::default()));
        }
        let mut samples = Vec::with_capacity(7);
        for _ in 0..7 {
            let begin = Instant::now();
            for _ in 0..rounds {
                let _ = black_box(parse_project(black_box(&source), SourceLimits::default()));
            }
            samples.push(begin.elapsed().as_nanos() as f64 / rounds as f64);
        }
        samples.sort_by(f64::total_cmp);
        println!(
            "{{\"case\":\"{name}\",\"bytes\":{},\"accepted\":{accepted},\"iterations_per_sample\":{rounds},\"median_ns\":{},\"samples_ns\":{samples:?}}}",
            source.len(),
            samples[3]
        );
    }
    Ok(())
}
