//! Optional shell observations. These records carry no checker authority.
use clap::{ArgMatches, CommandFactory};
use serde_json::{Value, json};
use std::cell::{Cell, RefCell};
use std::fs::OpenOptions;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

thread_local! {
    // One owned observation per invocation, consumed at finish. No decision
    // input or checked result is retained, and no mutable reference escapes.
    static WORK: RefCell<Value> = const { RefCell::new(Value::Null) };
    static VERDICT: Cell<Option<&'static str>> = const { Cell::new(None) };
}

pub(crate) struct Invocation {
    destination: PathBuf,
    started: Instant,
    timestamp: String,
    command: Vec<String>,
    inputs: Vec<Value>,
}

fn collect(
    command: &clap::Command,
    matches: &ArgMatches,
    path: &mut Vec<String>,
    inputs: &mut Vec<Value>,
) {
    for arg in command.get_arguments() {
        let id = arg.get_id().as_str();
        if arg.get_value_parser().type_id() != std::any::TypeId::of::<PathBuf>()
            || matches!(
                id,
                "out" | "candidate_out" | "queries" | "session" | "mutant_programs"
            )
        {
            continue;
        }
        if let Some(values) = matches.get_raw(id) {
            for value in values {
                let file = Path::new(value);
                if file.is_dir() {
                    for name in [
                        "project.zeno",
                        "v2/policy.json",
                        "tests/decision-examples.txt",
                    ] {
                        let child = file.join(name);
                        if child.is_file() {
                            inputs.push(input(id, &child));
                        }
                    }
                } else if file.exists() {
                    inputs.push(input(id, file));
                }
            }
        }
    }
    if let Some((name, child)) = matches.subcommand()
        && let Some(subcommand) = command.find_subcommand(name)
    {
        path.push(name.to_owned());
        collect(subcommand, child, path, inputs);
    }
}

fn input(name: &str, path: &Path) -> Value {
    // Do not block on special files or follow an open-time symlink replacement.
    let digest = (|| {
        let mut options = OpenOptions::new();
        options.read(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.custom_flags(nix::libc::O_NONBLOCK);
        }
        let file = options.open(path).ok()?;
        if !file.metadata().ok()?.is_file() {
            return None;
        }
        let mut bytes = Vec::new();
        file.take(64 * 1024 * 1024 + 1)
            .read_to_end(&mut bytes)
            .ok()?;
        if bytes.len() > 64 * 1024 * 1024 {
            return None;
        }
        Some(crate::transform::sha256_hex(&bytes))
    })();
    json!({"name":name, "sha256":digest})
}

impl Invocation {
    pub(crate) fn start() -> Option<Self> {
        let destination = std::env::var_os("ZENO_FCIS_TELEMETRY")?;
        if destination.is_empty()
            || std::env::var_os("ZENO_FCIS_CHECKER_CONTROL").is_some()
            || std::env::var_os("ZENO_FCIS_POSIX_ROLE").is_some()
        {
            return None;
        }
        WORK.with(|work| *work.borrow_mut() = Value::Null);
        VERDICT.with(|verdict| verdict.set(None));
        let started = Instant::now();
        let timestamp = utc_now();
        let mut command = Vec::new();
        let mut inputs = Vec::new();
        let parser = crate::Cli::command();
        if let Ok(matches) = parser.clone().try_get_matches() {
            collect(&parser, &matches, &mut command, &mut inputs);
        } else {
            // On help and malformed arguments, retain only recognized command
            // words. Arbitrary argument text never enters the record.
            let mut cursor = &parser;
            for word in std::env::args().skip(1) {
                if let Some(child) = cursor.find_subcommand(&word) {
                    command.push(child.get_name().to_owned());
                    cursor = child;
                }
            }
        }
        Some(Self {
            destination: destination.into(),
            started,
            timestamp,
            command,
            inputs,
        })
    }

    pub(crate) fn finish(self, code: u8) {
        let mut digests: Vec<_> = self
            .inputs
            .iter()
            .filter_map(|v| v["sha256"].as_str())
            .collect();
        digests.sort_unstable();
        digests.dedup();
        let subject = if digests.is_empty() {
            None
        } else {
            Some(crate::transform::sha256_hex(digests.join("\n").as_bytes()))
        };
        let decision_command = self.command.first().is_some_and(|s| {
            matches!(
                s.as_str(),
                "check"
                    | "prove"
                    | "counterexample"
                    | "contract"
                    | "loop"
                    | "transform"
                    | "optimize"
                    | "purity"
                    | "synth"
            )
        });
        let verdict = decision_command.then(|| {
            VERDICT.with(|v| v.take()).unwrap_or(match code {
                0 => "accepted",
                1 if self.command.iter().any(|s| {
                    matches!(
                        s.as_str(),
                        "prove"
                            | "counterexample"
                            | "lemma"
                            | "scenario"
                            | "distribution"
                            | "check"
                    )
                }) =>
                {
                    "refuted"
                }
                2 => "unknown",
                _ => "refused",
            })
        });
        let work = WORK.with(|work| work.replace(Value::Null));
        let record = json!({"schema":"zeno-fcis/telemetry/1", "timestamp":self.timestamp,
            "actor":std::env::var("ZENO_FCIS_COMMONS_ACTOR").unwrap_or_else(|_| "unknown".into()),
            "command":self.command, "inputs":self.inputs, "subject_sha256":subject,
            "exit_code":code, "verdict":verdict, "work_units":work,
            "duration_ms":self.started.elapsed().as_millis()});
        // A single O_APPEND write: never retry a partial write as another append.
        // Failure is deliberately ignored, including open, encoding and write.
        if let Ok(mut bytes) = serde_json::to_vec(&record) {
            bytes.push(b'\n');
            let mut options = OpenOptions::new();
            options.create(true).append(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                options.mode(0o600).custom_flags(nix::libc::O_NONBLOCK);
            }
            if let Ok(mut file) = options.open(self.destination)
                && file.metadata().is_ok_and(|m| m.is_file())
            {
                let _ = file.write(&bytes);
            }
        }
    }
}

pub(crate) fn observe(value: &Value) {
    if std::env::var_os("ZENO_FCIS_TELEMETRY").is_none() {
        return;
    }
    let status = value
        .get("status")
        .or_else(|| value.get("verdict"))
        .and_then(Value::as_str);
    let class = match status {
        Some("refuted" | "false" | "disagreement" | "law-refusal" | "different") => Some("refuted"),
        Some("inconclusive" | "not-inductive" | "unknown" | "UNKNOWN" | "blocked") => {
            Some("unknown")
        }
        Some("refused" | "invalid" | "error" | "contract-invalid" | "malformed") => Some("refused"),
        _ => None,
    };
    if let Some(class) = class {
        VERDICT.with(|v| v.set(Some(class)));
    }
    fn numbers(value: &Value, path: &str, found: &mut serde_json::Map<String, Value>) {
        if let Some(object) = value.as_object() {
            for (key, child) in object {
                let name = if path.is_empty() {
                    key.clone()
                } else {
                    format!("{path}.{key}")
                };
                if matches!(
                    key.as_str(),
                    "work"
                        | "work_units"
                        | "search_work"
                        | "check_work"
                        | "session_work"
                        | "used_work"
                        | "reserved_work"
                        | "steps"
                ) && child.is_number()
                {
                    found.insert(name, child.clone());
                } else if matches!(
                    key.as_str(),
                    "accounting"
                        | "usage"
                        | "detail"
                        | "report"
                        | "result"
                        | "session"
                        | "totals"
                        | "spent"
                        | "measured"
                ) && child.is_object()
                {
                    numbers(child, &name, found);
                }
            }
        }
    }
    let mut found = serde_json::Map::new();
    numbers(value, "", &mut found);
    if !found.is_empty() {
        WORK.with(|work| *work.borrow_mut() = Value::Object(found));
    }
}

fn utc_now() -> String {
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    // Gregorian civil date from days since the Unix epoch.
    let days = seconds / 86400 + 719468;
    let era = days / 146097;
    let doe = days - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let mut year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    year += u64::from(month <= 2);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
        seconds / 3600 % 24,
        seconds / 60 % 60,
        seconds % 60
    )
}
