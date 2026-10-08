use std::{ffi::OsString, path::Path};

fn main() {
    let arguments: Vec<OsString> = std::env::args_os().skip(1).collect();
    let flag = |index: usize| arguments.get(index).and_then(|word| word.to_str());
    let result = match (arguments.len(), flag(0)) {
        (2, Some("--payouts")) => withdrawal_queue::payouts(Path::new(&arguments[1])),
        (2, Some("--relay-export")) => withdrawal_queue::relay_export(Path::new(&arguments[1])),
        (4, Some("--relay-acknowledge")) => match (flag(2), flag(3)) {
            (Some(id), Some(hash)) => {
                withdrawal_queue::relay_acknowledge(Path::new(&arguments[1]), id, hash)
            }
            _ => usage(),
        },
        (1, _) => withdrawal_queue::journey(Path::new(&arguments[0])),
        _ => usage(),
    };
    match result {
        Ok(summary) => println!("{summary}"),
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(1);
        }
    }
}

fn usage() -> ! {
    eprintln!(
        "usage: withdrawal-queue NEW_DATABASE_PATH\n       withdrawal-queue --payouts NEW_DATABASE_PATH\n       withdrawal-queue --relay-export DATABASE_PATH\n       withdrawal-queue --relay-acknowledge DATABASE_PATH DELIVERY_ID PAYLOAD_SHA256"
    );
    std::process::exit(2);
}
