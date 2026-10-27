//! Offline descriptor and full-domain evidence exporter.
use serde_json::{Value, json};
use std::{env, fs, path::Path, process::ExitCode};
fn run() -> Result<Value, String> {
    let args: Vec<String> = env::args().collect();
    let dir = Path::new(args.get(1).ok_or("contract directory required")?);
    let read = |name| fs::read_to_string(dir.join(name)).map_err(|e| e.to_string());
    let source = json!({"project":read("project.zeno")?,"policy":read("v2/policy.json")?});
    match args.get(2).map(String::as_str) {
        Some("finite") => zeno_fcis_site_factory::ui::finite(&source),
        Some("decisions") => {
            let input: Value =
                serde_json::from_reader(std::io::stdin()).map_err(|e| e.to_string())?;
            let inputs = input.as_array().ok_or("UiInvalidTuples")?;
            let replies = inputs
                .iter()
                .map(|v| zeno_fcis_site_factory::ui::decide(&source, v))
                .collect::<Result<Vec<_>, _>>()?;
            Ok(json!(replies))
        }
        _ => {
            Ok(json!({"source":source,"descriptor":zeno_fcis_site_factory::ui::describe(&source)?}))
        }
    }
}
fn main() -> ExitCode {
    match run() {
        Ok(v) => {
            println!("{v}");
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("{e}");
            ExitCode::FAILURE
        }
    }
}
