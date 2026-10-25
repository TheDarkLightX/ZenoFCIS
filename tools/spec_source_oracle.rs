//! Process-isolated public-parser observer for revision-bound differential runs.

use std::env;
use std::fs;
use std::process::ExitCode;

use zeno_fcis_spec::{
    LawApplicability, LawScope, ProjectLimits, SourceLimits, elaborate_project, expand_tables,
    parse_formula, parse_law_formula, parse_project, parse_table_expression, parse_table_read,
    table_index_sets,
};

fn emit<T: std::fmt::Debug, E: std::fmt::Debug>(result: Result<T, E>) {
    match result {
        Ok(value) => println!("OK\n{value:?}"),
        Err(error) => println!("ERR\n{error:?}"),
    }
}

fn run() -> Result<(), String> {
    let arguments: Vec<_> = env::args().skip(1).collect();
    if arguments.len() != 6 {
        return Err("expected ACTION SOURCE PAYLOAD MAX_BYTES MAX_TOKENS MAX_DIAGNOSTICS".into());
    }
    let source = fs::read_to_string(&arguments[1]).map_err(|error| error.to_string())?;
    let payload = fs::read_to_string(&arguments[2]).map_err(|error| error.to_string())?;
    let number = |index: usize| {
        arguments[index]
            .parse::<usize>()
            .map_err(|error| error.to_string())
    };
    let limits =
        SourceLimits::try_new(number(3)?, number(4)?, number(5)?).ok_or("invalid source limits")?;
    match arguments[0].as_str() {
        "source_identity" => emit::<_, ()>(Ok(zeno_fcis_spec::source_digest())),
        "project" => emit(parse_project(&source, limits)),
        "formula" => emit(parse_formula(&source, &payload, limits)),
        "law_formula" => emit(parse_law_formula(
            &source,
            &payload,
            LawApplicability::new(LawScope::Always, true),
            limits,
        )),
        "table_expression" => emit(parse_table_expression(&source, &payload, limits)),
        "table_read" => emit(parse_table_read(&source, &payload, limits)),
        "index_sets" => emit(table_index_sets(&source, limits)),
        "expand_tables" => emit(expand_tables(&source, limits)),
        "elaborate" => emit(
            parse_project(&source, limits)
                .and_then(|parsed| elaborate_project(parsed, ProjectLimits::default())),
        ),
        _ => return Err("unknown fixed action".into()),
    }
    Ok(())
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("observer error: {error}");
            ExitCode::from(64)
        }
    }
}
