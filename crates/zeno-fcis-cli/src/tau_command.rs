//! Files and command dispatch for the optional finite Tau kit.

use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command as Process, Stdio};

use clap::Subcommand;
use serde_json::{Value, json};

use crate::contract::{ContractSources, generate_contract, tau};
use crate::contract_files::{self, read_input};
use crate::{FAILURE, INVALID, JSON_SCHEMA, OK, OutputFormat, USAGE, atomic_create, print_json};

const IMPORTER: &str = include_str!("../tau/importer.py");
const RUNNER: &str = include_str!("../tau/runner.py");
// `valid ` plus the formula and NUL must fit Linux's 128 KiB argv limit.
const MAX_TAU_QUERY_BYTES: usize = (128 << 10) - 8;
const MAX_PARSER_OUTPUT: u64 = 64 << 20;
const METADATA: &str = "v2/tau-import.json";

#[derive(Subcommand)]
pub(super) enum Command {
    /// Export finite per-rule queries and sampled actual Authority decisions. Tau and SMT are run separately, locally.
    Export {
        #[arg(long, required = true)]
        tau: bool,
        #[arg(default_value = ".")]
        dir: PathBuf,
        #[arg(long)]
        out: PathBuf,
        /// Largest native review input set. A larger domain uses its existing boundary construction.
        #[arg(long, default_value_t = 1_000_000)]
        max_tuples: u64,
        /// Deterministic Authority decision samples. This is not whole-domain Tau replay.
        #[arg(long, default_value_t = 64)]
        samples: usize,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        format: OutputFormat,
    },
    /// Import a complete, pointwise finite Tau decision ladder through the native contract compiler. Requires local Python 3.
    Import {
        #[arg(long, required = true)]
        tau: bool,
        spec: PathBuf,
        #[arg(long)]
        out: PathBuf,
        /// Explicitly narrow an original bit width, e.g. 256:3. Scaled evidence is labelled separately.
        #[arg(long)]
        scale_width: Vec<String>,
        /// Restrict an input scalar's finite range, e.g. i2.kind:0:3.
        #[arg(long)]
        domain: Vec<String>,
        /// Explicit finite allowance AST and supplied diagnostic/law annotations; amounts 0..7.
        #[arg(long, value_parser = ["allowance-study-bv3"])]
        profile: Option<String>,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        format: OutputFormat,
    },
}

pub(super) fn run(command: Command) -> u8 {
    match command {
        Command::Export {
            tau: _,
            dir,
            out,
            max_tuples,
            samples,
            format,
        } => {
            if !(1..=1_000_000).contains(&max_tuples) || !(1..=10_000).contains(&samples) {
                eprintln!("--max-tuples must be in 1..1000000; --samples in 1..10000");
                return USAGE;
            }
            report(export(&dir, &out, max_tuples, samples), &out, format)
        }
        Command::Import {
            tau: _,
            spec,
            out,
            scale_width,
            domain,
            profile,
            format,
        } => report(
            import(&spec, &out, &scale_width, &domain, profile.as_deref()),
            &out,
            format,
        ),
    }
}

fn read_text(path: &Path) -> Result<String, String> {
    let bytes = read_input(path).map_err(|error| format!("read {}: {error}", path.display()))?;
    String::from_utf8(bytes).map_err(|_| format!("{} is not UTF-8", path.display()))
}

/// Python parses and serializes only. Policy execution stays in the library.
fn parser(input: &[u8], arguments: &[String]) -> Result<Value, String> {
    if input.len() > 32 << 20 {
        return Err("Tau parser input exceeds 32 MiB".to_owned());
    }
    let mut child = Process::new("python3")
        .args(["-I", "-B", "-c", IMPORTER])
        .args(arguments)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .map_err(|error| format!("start the optional Python 3 parser: {error}"))?;
    let written = child
        .stdin
        .take()
        .ok_or_else(|| "parser stdin unavailable".to_owned())?
        .write_all(input);
    if let Err(error) = written {
        let _ = child.kill();
        let _ = child.wait();
        return Err(format!("write parser source: {error}"));
    }
    let mut output = Vec::new();
    let read = child
        .stdout
        .take()
        .ok_or_else(|| "parser stdout unavailable".to_owned())?
        .take(MAX_PARSER_OUTPUT + 1)
        .read_to_end(&mut output);
    if read.is_err() || output.len() as u64 > MAX_PARSER_OUTPUT {
        let _ = child.kill();
        let _ = child.wait();
        return Err(
            "Tau parser output exceeds its bounded profile or could not be read".to_owned(),
        );
    }
    let status = child
        .wait()
        .map_err(|error| format!("wait for parser: {error}"))?;
    if !status.success() {
        return Err("Tau parser refused the source; see its diagnostic on stderr".to_owned());
    }
    serde_json::from_slice(&output)
        .map_err(|error| format!("Tau parser response is invalid: {error}"))
}

fn unavailable(out: &Path) -> Result<(), String> {
    if out
        .try_exists()
        .map_err(|error| format!("inspect destination: {error}"))?
    {
        return Err("destination already exists; choose a new directory".to_owned());
    }
    Ok(())
}

fn write_new(out: &Path, files: &[(String, Vec<u8>)]) -> Result<(), String> {
    fs::create_dir(out).map_err(|error| format!("create {}: {error}", out.display()))?;
    for (name, bytes) in files {
        let path = out.join(name);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .map_err(|error| format!("create {}: {error}", parent.display()))?;
        }
        atomic_create(&path, bytes)
            .map_err(|error| format!("write {}: {error}", path.display()))?;
    }
    Ok(())
}

fn json_bytes(value: &Value) -> Result<Vec<u8>, String> {
    let mut bytes = serde_json::to_vec_pretty(value).map_err(|error| error.to_string())?;
    bytes.push(b'\n');
    Ok(bytes)
}

fn import(
    spec: &Path,
    out: &Path,
    scaling: &[String],
    domains: &[String],
    profile: Option<&str>,
) -> Result<Value, String> {
    unavailable(out)?;
    let source = read_text(spec)?;
    let arguments = scaling
        .iter()
        .flat_map(|value| ["--scale-width".to_owned(), value.clone()])
        .chain(
            domains
                .iter()
                .flat_map(|value| ["--domain".to_owned(), value.clone()]),
        )
        .chain(
            profile
                .into_iter()
                .flat_map(|value| ["--profile".to_owned(), value.to_owned()]),
        )
        .collect::<Vec<_>>();
    let parsed = parser(source.as_bytes(), &arguments)?;
    let project = parsed["project"].as_str().ok_or("parser omitted project")?;
    let rules = parsed["rules"].as_str().ok_or("parser omitted rules")?;
    let generated = generate_contract(ContractSources {
        project,
        rules,
        schema_origin: None,
        adoptions: &[],
        replayed: &[],
        evolutions: &[],
    })
    .map_err(|error| format!("{}: {}", error.place(), error.reason()))?;
    let mut files = contract_files::outputs(&generated)
        .into_iter()
        .map(|(name, bytes)| (name, bytes.to_vec()))
        .collect::<Vec<_>>();
    files.extend([
        (
            contract_files::PROJECT.to_owned(),
            project.as_bytes().to_vec(),
        ),
        (contract_files::RULES.to_owned(), rules.as_bytes().to_vec()),
        (METADATA.to_owned(), json_bytes(&parsed["metadata"])?),
    ]);
    write_new(out, &files)?;
    Ok(
        json!({"status": "imported-finite-profile", "scope": parsed["metadata"]["scope"], "metadata": METADATA,
        "qualification": "native-generation-only; review and paired Tau/SMT checks required"}),
    )
}

fn export(dir: &Path, out: &Path, max_tuples: u64, samples: usize) -> Result<Value, String> {
    unavailable(out)?;
    let inputs = contract_files::Inputs::read(dir)
        .map_err(|_| "application inputs could not be read".to_owned())?;
    inputs
        .generate()
        .map_err(|_| "native contract generation refused the application".to_owned())?;
    let mut bundle = tau::export(&inputs.project, &inputs.rules, max_tuples, samples)
        .map_err(|error| format!("{}: {}", error.place(), error.reason()))?;
    match read_input(&dir.join(METADATA)) {
        Ok(bytes) => {
            let metadata: Value = serde_json::from_slice(&bytes)
                .map_err(|error| format!("read import metadata: {error}"))?;
            let input = serde_json::to_vec(
                &json!({"metadata": metadata, "decisions": bundle["decisions"]}),
            )
            .map_err(|error| error.to_string())?;
            let parsed = parser(&input, &["--correspondence".to_owned()])?;
            if parsed["project"].as_str() != Some(&inputs.project)
                || parsed["rules"].as_str() != Some(&inputs.rules)
            {
                return Err(
                    "import metadata no longer describes the exact application source".to_owned(),
                );
            }
            let queries = parsed["queries"]
                .as_array()
                .ok_or("parser omitted correspondence queries")?;
            bundle["queries"]
                .as_array_mut()
                .ok_or("export omitted queries")?
                .extend(queries.iter().cloned());
            let reference =
                tau::reference_queries(&inputs.project, &inputs.rules, &parsed["metadata"])
                    .map_err(|error| format!("{}: {}", error.place(), error.reason()))?;
            bundle["queries"]
                .as_array_mut()
                .ok_or("export omitted queries")?
                .extend(reference);
            bundle["import"] = parsed["metadata"].clone();
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => (),
        Err(error) => return Err(format!("read import metadata: {error}")),
    }
    let count = validate_export_queries(&bundle)?;
    let files = vec![
        ("queries.json".to_owned(), json_bytes(&bundle)?),
        ("mapping.json".to_owned(), json_bytes(&bundle["mapping"])?),
        (
            "decisions.json".to_owned(),
            json_bytes(&bundle["decisions"])?,
        ),
        ("runner.py".to_owned(), RUNNER.as_bytes().to_vec()),
    ];
    write_new(out, &files)?;
    Ok(
        json!({"status": "exported-unrun", "queries": count, "review": bundle["review"], "runner": "runner.py",
        "qualification": "no external Tau or SMT result has been obtained by export"}),
    )
}

fn validate_export_queries(bundle: &Value) -> Result<usize, String> {
    let queries = bundle["queries"]
        .as_array()
        .ok_or("export omitted queries")?;
    let count = queries.len();
    // The runner always adds its independent false-T diagnostic.
    if !(1..20_000).contains(&count) {
        return Err("query count must be in 1..19999; one of the runner's 20000 slots is reserved for its diagnostic".to_owned());
    }
    for query in queries {
        let formula = query["tau"]
            .as_str()
            .ok_or("export omitted a Tau formula")?;
        if formula.is_empty() || formula.len() > MAX_TAU_QUERY_BYTES {
            return Err(format!(
                "Tau query exceeds {MAX_TAU_QUERY_BYTES} bytes; valid-command argv transport is bounded below 128 KiB"
            ));
        }
    }
    Ok(count)
}

fn report(result: Result<Value, String>, out: &Path, format: OutputFormat) -> u8 {
    match result {
        Ok(mut result) => {
            result["schema"] = JSON_SCHEMA.into();
            result["path"] = out.display().to_string().into();
            result["authority"] = "none".into();
            match format {
                OutputFormat::Json => print_json(&result),
                OutputFormat::Human => println!(
                    "{}: {} (authority: none)",
                    result["status"].as_str().unwrap_or("exported"),
                    out.display()
                ),
            }
            OK
        }
        Err(reason) => {
            match format {
                OutputFormat::Json => print_json(
                    &json!({"schema": JSON_SCHEMA, "status": "refused", "authority": "none", "reason": reason}),
                ),
                OutputFormat::Human => eprintln!("Tau finite profile refused: {reason}"),
            }
            // The optional workflow never interprets a refusal as corroboration.
            if out.exists() { FAILURE } else { INVALID }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn export_reserves_one_query_for_the_runner_diagnostic() {
        let query = json!({"tau": "T"});
        assert_eq!(
            validate_export_queries(&json!({"queries": vec![query.clone(); 19_999]})),
            Ok(19_999)
        );
        assert!(validate_export_queries(&json!({"queries": vec![query; 20_000]})).is_err());
    }

    #[test]
    fn export_refuses_a_tau_formula_above_the_argv_boundary() {
        assert_eq!(
            validate_export_queries(
                &json!({"queries": [{"tau": "T".repeat(MAX_TAU_QUERY_BYTES)}]})
            ),
            Ok(1)
        );
        assert!(
            validate_export_queries(
                &json!({"queries": [{"tau": "T".repeat(MAX_TAU_QUERY_BYTES + 1)}]})
            )
            .is_err()
        );
    }
}
