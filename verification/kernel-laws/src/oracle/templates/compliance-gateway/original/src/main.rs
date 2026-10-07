fn main() {
    let Some(path) = std::env::args_os().nth(1) else {
        eprintln!("usage: compliance-gateway NEW_DATABASE_PATH");
        std::process::exit(2);
    };
    match compliance_gateway::journey(std::path::Path::new(&path)) {
        Ok(summary) => println!("{summary}"),
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(1);
        }
    }
}
