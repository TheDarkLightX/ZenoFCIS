//! Runs the decision examples as one session in a new database.

fn main() {
    let Some(path) = std::env::args_os().nth(1) else {
        eprintln!("usage: {} NEW_DATABASE_PATH", env!("CARGO_PKG_NAME"));
        std::process::exit(2);
    };
    let result = application::examples(include_str!("../tests/decision-examples.txt"))
        .and_then(|examples| application::journey(std::path::Path::new(&path), &examples));
    match result {
        Ok(summary) => println!("{}", summary.json()),
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(1);
        }
    }
}
