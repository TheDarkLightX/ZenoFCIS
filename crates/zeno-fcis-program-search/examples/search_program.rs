//! Measurement driver around the pure search library: canonical program
//! files in, candidate files and one JSON report line out. Nothing it writes
//! carries authority; `zeno-fcis transform check` judges every candidate.
//!
//! ```text
//! search_program search --program ORIGINAL --out CANDIDATE [--seed PROGRAM]...
//!                [--work-units N] [--max-tuples N] [--max-cells N]
//! search_program plant --program PROGRAM --out PLANTED [--ordinal N] [--output J]
//! ```
//!
//! `search` creates `CANDIDATE` only when the search returns a strictly
//! smaller program. `plant` is a control: it writes a copy of `PROGRAM` whose
//! output `J` (default 0) differs on exactly the input tuple at ordinal `N`
//! (default: the middle tuple the program accepts), and reports that tuple.
//! Exit status: 0 for a finished search or a planted copy, 2 when a budget or
//! refusal stopped the search, 1 for a usage, input or output error.

use std::fmt::Write as _;
use std::fs::OpenOptions;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use zeno_fcis_codec::CanonicalEncode;
use zeno_fcis_program_search::{Outcome, Provenance, Refusal, Settings, Stop, search};
use zeno_fcis_synthesis::finite::{Domain, Error, Op, Program};
use zeno_fcis_synthesis::finite_runtime::import_program;

const PROGRAM_LIMIT: u64 = 64 * 1024;

fn main() -> ExitCode {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    let result = match arguments.first().map(String::as_str) {
        Some("search") => run_search(&arguments[1..]),
        Some("plant") => run_plant(&arguments[1..]),
        _ => Err("usage: search_program (search|plant) --program FILE --out FILE ...".to_owned()),
    };
    match result {
        Ok((code, report)) => {
            println!("{report}");
            ExitCode::from(code)
        }
        Err(message) => {
            println!(
                "{{\"schema\":\"zeno-fcis/program-search-report/1\",\"authority\":\"none\",\"status\":\"error\",\"message\":{}}}",
                text(&message)
            );
            ExitCode::from(1)
        }
    }
}

struct Options {
    program: PathBuf,
    out: PathBuf,
    seeds: Vec<PathBuf>,
    settings: Settings,
    ordinal: Option<usize>,
    output: usize,
}

fn options(arguments: &[String]) -> Result<Options, String> {
    let mut program = None;
    let mut out = None;
    let mut seeds = Vec::new();
    let mut settings = Settings::default();
    let mut ordinal = None;
    let mut output = 0;
    let mut rest = arguments.iter();
    while let Some(flag) = rest.next() {
        let value = rest.next().ok_or_else(|| format!("{flag} needs a value"))?;
        let number = || {
            value
                .parse::<u64>()
                .map_err(|_| format!("{flag} needs an unsigned integer"))
        };
        match flag.as_str() {
            "--program" => program = Some(PathBuf::from(value)),
            "--out" => out = Some(PathBuf::from(value)),
            "--seed" => seeds.push(PathBuf::from(value)),
            "--work-units" => settings.budget.work_units = number()?,
            "--max-tuples" => settings.budget.max_tuples = number()?,
            "--max-cells" => settings.budget.max_cells = number()?,
            "--ordinal" => {
                ordinal = Some(usize::try_from(number()?).map_err(|_| "--ordinal is too large")?);
            }
            "--output" => {
                output = usize::try_from(number()?).map_err(|_| "--output is too large")?
            }
            _ => return Err(format!("unknown flag {flag}")),
        }
    }
    Ok(Options {
        program: program.ok_or("--program is required")?,
        out: out.ok_or("--out is required")?,
        seeds,
        settings,
        ordinal,
        output,
    })
}

fn read(path: &Path) -> Result<Vec<u8>, String> {
    let length = std::fs::metadata(path)
        .map_err(|error| format!("{}: {error}", path.display()))?
        .len();
    if length > PROGRAM_LIMIT {
        return Err(format!("{} is larger than 64 KiB", path.display()));
    }
    std::fs::read(path).map_err(|error| format!("{}: {error}", path.display()))
}

fn create(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|error| format!("{}: {error}", path.display()))?;
    file.write_all(bytes)
        .and_then(|()| file.sync_all())
        .map_err(|error| format!("{}: {error}", path.display()))
}

fn text(value: &str) -> String {
    let mut out = String::from("\"");
    for character in value.chars() {
        match character {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            c if u32::from(c) < 0x20 || !c.is_ascii() => {
                let _ = write!(out, "\\u{:04x}", u32::from(c) & 0xffff);
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn decimals(values: &[i64]) -> String {
    let items: Vec<String> = values
        .iter()
        .map(|value| text(&value.to_string()))
        .collect();
    format!("[{}]", items.join(","))
}

fn import(bytes: &[u8]) -> Result<Program, String> {
    import_program(bytes).map_err(|error| match error {
        Error::Invalid(code) | Error::Limit(code) => format!("program not admitted: {code}"),
        _ => "program not admitted".to_owned(),
    })
}

fn provenance_json(provenance: &Provenance) -> String {
    let trail: Vec<String> = provenance
        .trail
        .iter()
        .map(|step| {
            format!(
                "{{\"proposer\":{},\"round\":{},\"nodes\":{},\"bytes\":{}}}",
                text(step.proposer),
                step.round,
                step.nodes,
                step.bytes
            )
        })
        .collect();
    let offers: Vec<String> = provenance
        .offers
        .iter()
        .map(|offer| {
            format!(
                "{{\"proposer\":{},\"offered\":{},\"empty\":{},\"duplicate\":{},\"admitted\":{},\"refused\":{}}}",
                text(offer.proposer),
                offer.offered,
                offer.empty,
                offer.duplicate,
                offer.admitted,
                offer.refused
            )
        })
        .collect();
    let refused: Vec<String> = provenance
        .refused
        .iter()
        .map(|refused| {
            let reason = match refused.refusal {
                Refusal::Differs { ordinal } => format!("\"differs\",\"ordinal\":{ordinal}"),
                Refusal::Abi => "\"abi\"".to_owned(),
                Refusal::NotAdmitted => "\"not-admitted\"".to_owned(),
            };
            format!(
                "{{\"proposer\":{},\"round\":{},\"refusal\":{reason}}}",
                text(refused.proposer),
                refused.round
            )
        })
        .collect();
    let diagram = match &provenance.diagram {
        None => "null".to_owned(),
        Some(report) => {
            let list = |items: &[usize]| {
                items
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join(",")
            };
            format!(
                "{{\"skipped\":{},\"orders_tried\":{},\"orders_completed\":{},\"orders_over_size\":{},\"resynthesized_outputs\":[{}],\"copied_outputs\":[{}]}}",
                report.skipped.map_or_else(|| "null".to_owned(), text),
                report.orders_tried,
                report.orders_completed,
                report.orders_over_size,
                list(&report.resynthesized_outputs),
                list(&report.copied_outputs)
            )
        }
    };
    format!(
        "{{\"domain_tuples\":{},\"trapping_tuples\":{},\"rounds\":{},\"work_used\":{},\"winner\":{},\"winner_round\":{},\"trail\":[{}],\"offers\":[{}],\"refused\":[{}],\"diagram\":{diagram}}}",
        provenance.domain_tuples,
        provenance.trapping_tuples,
        provenance.rounds,
        provenance.work_used,
        text(provenance.winner),
        provenance.winner_round,
        trail.join(","),
        offers.join(","),
        refused.join(",")
    )
}

fn stop_json(stop: Stop) -> String {
    match stop {
        Stop::NotAdmitted { code } => {
            format!("{{\"reason\":\"not-admitted\",\"code\":{}}}", text(code))
        }
        Stop::EmptyInputDomain => "{\"reason\":\"empty-input-domain\"}".to_owned(),
        Stop::DomainOverBudget { tuples, limit } => format!(
            "{{\"reason\":\"domain-over-budget\",\"tuples\":\"{tuples}\",\"limit\":{limit}}}"
        ),
        Stop::CellsOverBudget { cells, limit } => {
            format!("{{\"reason\":\"cells-over-budget\",\"cells\":\"{cells}\",\"limit\":{limit}}}")
        }
        Stop::WorkBudget { limit } => {
            format!("{{\"reason\":\"work-budget\",\"limit\":{limit}}}")
        }
        Stop::TooManySeeds { limit } => {
            format!("{{\"reason\":\"too-many-seeds\",\"limit\":{limit}}}")
        }
        Stop::Inconsistent { code } => {
            format!("{{\"reason\":\"inconsistent\",\"code\":{}}}", text(code))
        }
        _ => "{\"reason\":\"unclassified\"}".to_owned(),
    }
}

fn run_search(arguments: &[String]) -> Result<(u8, String), String> {
    let options = options(arguments)?;
    if options.out.symlink_metadata().is_ok() {
        return Err(format!("{} exists", options.out.display()));
    }
    let original = read(&options.program)?;
    let nodes = import(&original)?.nodes().len();
    let seeds = options
        .seeds
        .iter()
        .map(PathBuf::as_path)
        .map(read)
        .collect::<Result<Vec<_>, _>>()?;
    let seed_slices: Vec<&[u8]> = seeds.iter().map(Vec::as_slice).collect();
    let head = format!(
        "\"schema\":\"zeno-fcis/program-search-report/1\",\"authority\":\"none\",\"original\":{{\"nodes\":{nodes},\"bytes\":{}}}",
        original.len()
    );
    let bounds = format!(
        "\"max_tuples\":{},\"max_cells\":{},\"work_units\":{},\"rounds\":{},\"depth\":{},\"width\":{},\"solve_limit\":{},\"diagram_nodes\":{}",
        options.settings.budget.max_tuples,
        options.settings.budget.max_cells,
        options.settings.budget.work_units,
        options.settings.budget.rounds,
        options.settings.shape.depth.map_or_else(
            || "null".into(),
            |d| d.min(zeno_fcis_program_search::MAX_DEPTH).to_string()
        ),
        options.settings.shape.width,
        options.settings.shape.solve_limit,
        options.settings.shape.diagram_nodes,
    );
    let request = format!(
        "{{\"bound\":\"configured-portfolio-envelope\",\"limits\":{{{bounds}}},\"action\":\"request-better-primitive\",\"request\":\"Provide a supported lowerable primitive or seed for the same requirements; keep the declared domain and checker limits unchanged.\",\"scope\":\"bounded search only; no impossibility claim\"}}"
    );
    Ok(match search(&original, &seed_slices, &options.settings) {
        Outcome::Improved(candidate) => {
            create(&options.out, &candidate.bytes)?;
            (
                0,
                format!(
                    "{{{head},\"status\":\"improved\",\"candidate\":{{\"nodes\":{},\"bytes\":{}}},\"provenance\":{}}}",
                    candidate.nodes,
                    candidate.bytes.len(),
                    provenance_json(&candidate.provenance)
                ),
            )
        }
        Outcome::Unimproved(provenance) => (
            0,
            format!(
                "{{{head},\"status\":\"unimproved\",\"candidate\":null,\"search_limit\":{request},\"provenance\":{}}}",
                provenance_json(&provenance)
            ),
        ),
        Outcome::Stopped(stop) => (
            2,
            format!(
                "{{{head},\"status\":\"stopped\",\"candidate\":null,\"search_limit\":{request},\"stop\":{}}}",
                stop_json(stop)
            ),
        ),
    })
}

/// Every tuple of `inputs`, last input fastest.
fn tuples(inputs: &[Domain]) -> Vec<Vec<i64>> {
    let mut all = vec![Vec::new()];
    for domain in inputs {
        let (min, max) = domain.bounds();
        let mut next = Vec::new();
        for prefix in &all {
            let mut value = min;
            loop {
                let mut tuple = prefix.clone();
                tuple.push(value);
                next.push(tuple);
                if value == max {
                    break;
                }
                value += 1;
            }
        }
        all = next;
    }
    all
}

fn run_plant(arguments: &[String]) -> Result<(u8, String), String> {
    let options = options(arguments)?;
    if options.out.symlink_metadata().is_ok() {
        return Err(format!("{} exists", options.out.display()));
    }
    let program = import(&read(&options.program)?)?;
    let size: u128 = program
        .inputs()
        .iter()
        .map(|domain| {
            let (min, max) = domain.bounds();
            u128::try_from(i128::from(max) - i128::from(min) + 1).unwrap_or(u128::MAX)
        })
        .product();
    if size > 1 << 22 {
        return Err("domain too large to plant a control".to_owned());
    }
    let all = tuples(program.inputs());
    let ok: Vec<usize> = (0..all.len())
        .filter(|ordinal| program.evaluate(&all[*ordinal]).is_ok())
        .collect();
    let ordinal = match options.ordinal {
        Some(ordinal) if ok.contains(&ordinal) => ordinal,
        Some(ordinal) => {
            return Err(format!(
                "the program refuses the tuple at ordinal {ordinal}"
            ));
        }
        None => *ok
            .get(ok.len() / 2)
            .ok_or("the program refuses every tuple")?,
    };
    let tuple = &all[ordinal];
    let before = program
        .evaluate(tuple)
        .map_err(|_| "evaluation changed".to_owned())?;
    let (Some(output), Some(first)) = (
        program.outputs().get(options.output),
        before.get(options.output),
    ) else {
        return Err(format!("the program has no output {}", options.output));
    };
    let replacement = match *output {
        Domain::Bool => Op::Bool(*first == 0),
        _ => {
            let (min, max) = output.bounds();
            let value = if *first < max {
                first + 1
            } else if *first > min {
                first - 1
            } else {
                // A one-value domain: plant an output-domain refusal.
                first
                    .checked_add(1)
                    .or_else(|| first.checked_sub(1))
                    .unwrap_or(0)
            };
            Op::Int(value)
        }
    };
    let mut nodes = program.nodes().to_vec();
    let mut push = |op: Op| {
        nodes.push(op);
        u16::try_from(nodes.len() - 1).map_err(|_| "too many nodes".to_owned())
    };
    let mut condition = None;
    for (k, (domain, value)) in program.inputs().iter().zip(tuple).enumerate() {
        let input = push(Op::Input(u16::try_from(k).map_err(|_| "too many inputs")?))?;
        let constant = match domain {
            Domain::Bool => push(Op::Bool(*value == 1))?,
            _ => push(Op::Int(*value))?,
        };
        let equal = push(Op::Eq(input, constant))?;
        condition = Some(match condition {
            Some(previous) => push(Op::And(previous, equal))?,
            None => equal,
        });
    }
    let condition = match condition {
        Some(condition) => condition,
        None => push(Op::Bool(true))?,
    };
    let alternative = push(replacement)?;
    let root = program.roots()[options.output];
    let planted_root = push(Op::Select(condition, alternative, root))?;
    let mut roots = program.roots().to_vec();
    roots[options.output] = planted_root;
    let planted = Program::try_new(
        program.inputs().to_vec(),
        program.outputs().to_vec(),
        nodes,
        roots,
    )
    .map_err(|error| format!("planted program not admitted: {error:?}"))?;
    let differing: Vec<usize> = (0..all.len())
        .filter(|ordinal| {
            let a = program.evaluate(&all[*ordinal]);
            let b = planted.evaluate(&all[*ordinal]);
            a != b
        })
        .collect();
    if differing != [ordinal] {
        return Err(format!(
            "planted copy differs on {} tuples",
            differing.len()
        ));
    }
    let bytes = planted
        .value()
        .and_then(|value| value.canonical_bytes())
        .map_err(|error| format!("planted program did not encode: {error:?}"))?;
    create(&options.out, &bytes)?;
    let after = match planted.evaluate(tuple) {
        Ok(values) => decimals(&values),
        Err(_) => "\"refused\"".to_owned(),
    };
    Ok((
        0,
        format!(
            "{{\"schema\":\"zeno-fcis/program-search-plant/1\",\"authority\":\"none\",\"status\":\"planted\",\"output\":{},\"ordinal\":{ordinal},\"input\":{},\"original_output\":{},\"planted_output\":{after},\"nodes\":{}}}",
            options.output,
            decimals(tuple),
            decimals(&before),
            planted.nodes().len()
        ),
    ))
}
