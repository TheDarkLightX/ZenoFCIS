//! The operational command line of `src/cli.rs`: `init`, `submit`, `decide`,
//! `state`, `history`, `pending`, `deliver` and `version`. Without one of
//! those commands, it runs the decision examples as one session in a new
//! database, or audits, decides the examples on, delivers, upgrades or
//! migrates an existing database along the contract lineage.

use std::io::Write as _;
use std::path::Path;

fn main() {
    let arguments: Vec<std::ffi::OsString> = std::env::args_os().skip(1).collect();
    if let Some(outcome) = application::cli::run(&arguments) {
        print!("{}", outcome.stdout);
        eprint!("{}", outcome.stderr);
        let _ = std::io::stdout().flush();
        std::process::exit(i32::from(outcome.exit));
    }
    let examples = || application::examples(include_str!("../tests/decision-examples.txt"));
    let result = match arguments.as_slice() {
        [path] if !path.to_string_lossy().starts_with("--") => examples()
            .and_then(|examples| application::journey(Path::new(path), &examples))
            .map(|summary| summary.json()),
        [flag, path] if flag == "--decide" => examples()
            .and_then(|examples| application::decide(Path::new(path), &examples))
            .map(|summary| summary.json()),
        [flag, path] if flag == "--audit" => {
            application::audit(Path::new(path)).map(|head| head.json())
        }
        [flag, path] if flag == "--deliver" => {
            application::deliver(Path::new(path)).map(|delivered| delivered.json())
        }
        [flag, path] if flag == "--upgrade" => {
            application::upgrade(Path::new(path)).map(|upgraded| upgraded.json())
        }
        [flag, path] if flag == "--migrate" => {
            application::migrate(Path::new(path)).map(|head| head.json())
        }
        _ => {
            let name = env!("CARGO_PKG_NAME");
            eprintln!(
                "{}\n       {name} NEW_DATABASE_PATH\n       {name} --decide DATABASE_PATH\n       {name} --audit DATABASE_PATH\n       {name} --deliver DATABASE_PATH\n       {name} --upgrade DATABASE_PATH\n       {name} --migrate DATABASE_PATH",
                application::cli::usage()
            );
            std::process::exit(2);
        }
    };
    match result {
        Ok(report) => println!("{report}"),
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(1);
        }
    }
}
