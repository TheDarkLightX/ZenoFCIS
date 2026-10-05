//! Focused entry to the actual CLI resolved-use checker.

#[path = "../../../crates/zeno-fcis-cli/src/purity.rs"]
mod purity;

use std::path::PathBuf;
use std::process::ExitCode;

fn main() -> ExitCode {
    let mut human = false;
    let mut paths = Vec::new();
    for argument in std::env::args_os().skip(1) {
        if argument == "--human" {
            human = true;
        } else {
            paths.push(PathBuf::from(argument));
        }
    }
    if paths.is_empty() {
        eprintln!("resolved-purity: at least one source/crate path is required");
        return ExitCode::from(2);
    }
    let report = purity::check_resolved_paths(&paths);
    if human {
        print!("{}", report.render());
    } else {
        println!("{}", report.to_json());
    }
    ExitCode::from(match report.status() {
        "unreadable" => 2,
        "violations" => 1,
        _ => 0,
    })
}
