//! Runs the decision examples as one session in a new database, or audits,
//! upgrades or migrates an existing database along the contract lineage.

use std::path::Path;

fn main() {
    let arguments: Vec<std::ffi::OsString> = std::env::args_os().skip(1).collect();
    let result = match arguments.as_slice() {
        [path] if !path.to_string_lossy().starts_with("--") => {
            application::examples(include_str!("../tests/decision-examples.txt"))
                .and_then(|examples| application::journey(Path::new(path), &examples))
                .map(|summary| summary.json())
        }
        [flag, path] if flag == "--audit" => {
            application::audit(Path::new(path)).map(|head| head.json())
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
                "usage: {name} NEW_DATABASE_PATH\n       {name} --audit DATABASE_PATH\n       {name} --upgrade DATABASE_PATH\n       {name} --migrate DATABASE_PATH"
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
