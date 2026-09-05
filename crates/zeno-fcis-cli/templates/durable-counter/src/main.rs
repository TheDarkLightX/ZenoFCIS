fn main() {
    let Some(path) = std::env::args_os().nth(1) else {
        eprintln!("usage: durable-counter NEW_DATABASE_PATH");
        std::process::exit(2);
    };
    match durable_counter::journey(std::path::Path::new(&path)) {
        Ok(summary) => println!("{summary}"),
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(1);
        }
    }
}
