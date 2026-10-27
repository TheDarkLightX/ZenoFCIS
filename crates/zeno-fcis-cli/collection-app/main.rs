fn main() {
    if let Err(error) = application::app::run() {
        eprintln!("application stopped: {error}");
        std::process::exit(1);
    }
}
