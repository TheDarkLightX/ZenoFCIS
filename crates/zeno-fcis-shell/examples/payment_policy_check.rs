//! Line-oriented bounded product runner for an independent checker.
use std::io::{self, BufRead};
use zeno_fcis_shell::effects::policy_product::{self, Profile, Run};

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
fn print_run(run: Run) {
    let trace = run.trace.iter().map(|event| format!(
        "{{\"operation\":\"{}\",\"choice\":{},\"verdicts\":{:?},\"response\":\"{}\",\"states\":[{}]}}",
        hex(&event.operation), event.choice, event.verdicts, hex(&event.response),
        event.states.iter().map(|s| format!("\"{}\"", hex(s))).collect::<Vec<_>>().join(","),
    )).collect::<Vec<_>>().join(",");
    println!(
        "{{\"next\":{},\"end\":{},\"outcome\":\"{}\",\"trace\":[{}],\"commits\":{:?},\"pending\":{:?},\"refused\":{:?},\"violation\":{},\"replayed\":{}}}",
        run.next,
        run.end,
        hex(&run.outcome),
        trace,
        run.commits,
        run.pending,
        run.refused,
        run.violation
            .map(|s| format!("\"{s}\""))
            .unwrap_or_else(|| "null".to_owned()),
        run.replayed
    );
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    match std::env::args().nth(1).as_deref() {
        Some("--identity") => {
            println!("{}", hex(policy_product::source().as_bytes()));
            return Ok(());
        }
        Some("--demo") => {
            let profile = Profile {
                bound: 6,
                retries: 1,
                allowed: true,
                monitored: true,
                provider_idempotent: false,
                duplicate_copies: false,
            };
            print_run(
                policy_product::run(profile, &[0, 3, 0, 0, 0]).map_err(|e| format!("{e:?}"))?,
            );
            return Ok(());
        }
        Some(_) => return Err("use --demo, --identity, or stream input lines".into()),
        None => {}
    }
    for line in io::stdin().lock().lines() {
        let line = line?;
        let words = line.split_whitespace().collect::<Vec<_>>();
        if words.len() != 7 {
            return Err(
                "expected K retries allow monitor idempotent duplicates schedule-or-dash".into(),
            );
        }
        let flags = words[..6]
            .iter()
            .map(|s| s.parse::<u8>())
            .collect::<Result<Vec<_>, _>>()?;
        if flags[2..].iter().any(|v| *v > 1) {
            return Err("flags must be 0 or 1".into());
        }
        let profile = Profile {
            bound: flags[0],
            retries: flags[1],
            allowed: flags[2] != 0,
            monitored: flags[3] != 0,
            provider_idempotent: flags[4] != 0,
            duplicate_copies: flags[5] != 0,
        };
        let schedule = if words[6] == "-" {
            Vec::new()
        } else {
            words[6]
                .bytes()
                .map(|b| {
                    b.checked_sub(b'0')
                        .filter(|v| *v < 9)
                        .ok_or("invalid choice digit")
                })
                .collect::<Result<Vec<_>, _>>()?
        };
        match policy_product::run(profile, &schedule) {
            Ok(run) => print_run(run),
            Err(error) => println!("{{\"error\":\"{error:?}\"}}"),
        }
    }
    Ok(())
}
