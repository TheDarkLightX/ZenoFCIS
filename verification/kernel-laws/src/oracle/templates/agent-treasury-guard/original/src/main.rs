fn main() {
    let Some(path) = std::env::args_os().nth(1) else {
        eprintln!("usage: agent-treasury-guard NEW_DATABASE_PATH");
        std::process::exit(2);
    };
    match agent_treasury_guard::journey(std::path::Path::new(&path)) {
        Ok(summary) => println!("{summary}"),
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(1);
        }
    }
}
