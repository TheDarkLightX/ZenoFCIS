use std::collections::{HashMap, HashSet};
use std::ffi::OsString;
use std::fs::{self, OpenOptions};
use std::io::{self, BufWriter, Write};
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::{Value, json};

use crate::{Commons, CommonsError};

const HELP: &str = "Zeno Commons command line.\n\
Usage: zeno-fcis commons [--store DIR] [--actor NAME] <command>\n\
Commands: init register whoami dashboard verify head anchor storage archive export advise lease status agents send inbox ack add update gaps dedup merge link show records emit events import-gaps mcp\n";

/// Run the Commons CLI arguments after the top-level `commons` command.
pub fn run(arguments: &[OsString]) -> u8 {
    run_with_mcp(arguments, crate::mcp::serve)
}

/// Run CLI commands with an application-supplied MCP server entry point.
/// The callback runs in the effectful shell and supplies no decision authority.
pub fn run_with_mcp(arguments: &[OsString], serve: fn(&str, &str) -> u8) -> u8 {
    let arguments = arguments
        .iter()
        .map(|argument| argument.to_string_lossy().into_owned())
        .collect::<Vec<_>>();
    let mut store = std::env::var("ZENO_FCIS_COMMONS").ok();
    let mut actor = std::env::var("ZENO_FCIS_COMMONS_ACTOR").unwrap_or_default();
    let mut command = Vec::new();
    let mut index = 0;
    while index < arguments.len() {
        match arguments[index].as_str() {
            "--store" => match arguments.get(index + 1) {
                Some(value) => {
                    store = Some(value.clone());
                    index += 2;
                }
                None => return refuse("argument --store: expected one argument"),
            },
            "--actor" => match arguments.get(index + 1) {
                Some(value) => {
                    actor = value.clone();
                    index += 2;
                }
                None => return refuse("argument --actor: expected one argument"),
            },
            _ => {
                command.push(arguments[index].clone());
                index += 1;
            }
        }
    }
    if command
        .first()
        .is_some_and(|word| word == "-h" || word == "--help")
    {
        print!("{HELP}");
        return 0;
    }
    if command.first().is_some_and(|word| word == "mcp") {
        let Some(store) = store else {
            return refuse("give --store or set ZENO_FCIS_COMMONS");
        };
        if actor.is_empty() {
            return refuse("give --actor or set ZENO_FCIS_COMMONS_ACTOR");
        }
        return serve(&store, &actor);
    }
    let Some(store) = store else {
        return refuse("give --store or set ZENO_FCIS_COMMONS");
    };
    if actor.is_empty() {
        return refuse("give --actor or set ZENO_FCIS_COMMONS_ACTOR");
    }
    let mut commons = match Commons::open(&store, &actor) {
        Ok(commons) => commons,
        Err(error) => return refuse(&error.to_string()),
    };
    if command.first().is_some_and(|word| word == "export") {
        return match execute_export(&commons, &command[1..]) {
            Ok(code) => code,
            Err(error) => refuse(&error.to_string()),
        };
    }
    let result = execute(&mut commons, &command);
    if !command
        .first()
        .is_some_and(|word| matches!(word.as_str(), "archive" | "storage" | "anchor"))
        && commons
            .storage()
            .is_ok_and(|storage| storage["over_budget"] == true)
    {
        eprintln!(
            "commons: the live store is over its budget; run `commons archive` (see `commons storage`)"
        );
    }
    match result {
        Ok((code, output)) => {
            match serde_json::to_string_pretty(&output) {
                Ok(output) => println!("{output}"),
                Err(error) => return refuse(&error.to_string()),
            }
            code
        }
        Err(error) => refuse(&error.to_string()),
    }
}

fn execute(commons: &mut Commons, arguments: &[String]) -> Result<(u8, Value), CommonsError> {
    let Some(command) = arguments.first().map(String::as_str) else {
        return Err(CommonsError::new("a command is required"));
    };
    match command {
        "init" | "dashboard" => {
            exact_positions(&[], &arguments[1..])?;
            Ok((0, commons.dashboard()?))
        }
        "register" => {
            let (positionals, options, _) =
                parse_options(&arguments[1..], &["--name", "--kind", "--role"], &[], &[])?;
            exact_positions(&[], &positionals)?;
            Ok((
                0,
                commons.register(
                    &option(&options, "--name", ""),
                    &option(&options, "--kind", "agent"),
                    &option(&options, "--role", ""),
                )?,
            ))
        }
        "whoami" => {
            let (positionals, _, _) = parse_options(&arguments[1..], &[], &[], &[])?;
            if positionals.len() > 1 {
                return Err(CommonsError::new("expected at most one argument: who"));
            }
            Ok((
                0,
                commons.identity(positionals.first().map_or("", String::as_str))?,
            ))
        }
        "verify" => {
            let (positionals, options, flags) =
                parse_options(&arguments[1..], &["--anchors"], &[], &["--archives"])?;
            exact_positions(&[], &positionals)?;
            let anchors =
                if let Some(path) = options.get("--anchors").and_then(|paths| paths.last()) {
                    fs::read_to_string(path)?
                        .lines()
                        .filter(|line| !line.trim().is_empty())
                        .map(|line| {
                            serde_json::from_str(line)
                                .map_err(|error| CommonsError::new(error.to_string()))
                        })
                        .collect::<Result<Vec<Value>, _>>()?
                } else {
                    Vec::new()
                };
            let result = commons.verify_with(flags.contains("--archives"), &anchors)?;
            Ok((if result["ok"] == true { 0 } else { 2 }, result))
        }
        "head" => {
            exact_positions(&[], &arguments[1..])?;
            Ok((0, commons.head()?))
        }
        "anchor" => {
            let (positionals, options, _) = parse_options(&arguments[1..], &["--out"], &[], &[])?;
            exact_positions(&[], &positionals)?;
            let out = option(&options, "--out", "");
            if out.is_empty() {
                return Err(CommonsError::new("argument --out: expected one argument"));
            }
            let check = commons.verify()?;
            if check["ok"] != true {
                return Err(CommonsError::new(format!(
                    "the chain is broken at {}; no anchor written",
                    check["broken_at"]
                )));
            }
            let head = commons.head()?;
            let seconds = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_err(|error| CommonsError::new(error.to_string()))?
                .as_secs_f64();
            let at = crate::iso(seconds);
            let line = format!(
                "{{\"at\": {}, \"hash\": {}, \"seq\": {}}}\n",
                serde_json::to_string(&at).map_err(|error| CommonsError::new(error.to_string()))?,
                serde_json::to_string(&head["hash"])
                    .map_err(|error| CommonsError::new(error.to_string()))?,
                head["seq"]
            );
            OpenOptions::new()
                .create(true)
                .append(true)
                .open(out)?
                .write_all(line.as_bytes())?;
            Ok((0, head))
        }
        "storage" => {
            exact_positions(&[], &arguments[1..])?;
            Ok((0, commons.storage()?))
        }
        "archive" => {
            let (positionals, options, flags) = parse_options(
                &arguments[1..],
                &["--keep"],
                &[],
                &["--if-over", "--no-vacuum"],
            )?;
            exact_positions(&[], &positionals)?;
            let keep = options
                .get("--keep")
                .and_then(|values| values.last())
                .map(|value| integer(value))
                .transpose()?;
            Ok((
                0,
                commons.archive(
                    keep,
                    flags.contains("--if-over"),
                    !flags.contains("--no-vacuum"),
                )?,
            ))
        }
        "advise" => {
            let (positionals, options, flags) = parse_options(
                &arguments[1..],
                &["--tag", "--text", "--limit"],
                &["--tag"],
                &["--brief"],
            )?;
            exact_positions(&["action"], &positionals)?;
            let tags = options.get("--tag").cloned().unwrap_or_default();
            let text = option(&options, "--text", "");
            let limit = integer(&option(&options, "--limit", "10"))?;
            let out = commons.advise(&positionals[0], &tags, &text, limit)?;
            if flags.contains("--brief") {
                let advice = out["advice"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .map(|item| {
                        format!(
                            "{} {} score={} {}",
                            item["id"].as_str().unwrap_or_default(),
                            item["type"].as_str().unwrap_or_default(),
                            item["score"].as_i64().unwrap_or_default(),
                            item["title"].as_str().unwrap_or_default()
                        )
                    })
                    .collect::<Vec<_>>();
                let leases = out["leases"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .map(|lease| {
                        format!(
                            "{} leased by {} until {}",
                            lease["resource"].as_str().unwrap_or_default(),
                            lease["owner"].as_str().unwrap_or_default(),
                            lease["expires"].as_str().unwrap_or_default()
                        )
                    })
                    .collect::<Vec<_>>();
                return Ok((
                    0,
                    json!({
                        "action": out["action"],
                        "advice": advice,
                        "leases": leases,
                        "unread": out["unread"].as_array().map_or(0, Vec::len)
                    }),
                ));
            }
            Ok((0, out))
        }
        "lease" => execute_lease(commons, &arguments[1..]),
        "status" => {
            let (positionals, options, _) =
                parse_options(&arguments[1..], &["--step", "--question"], &[], &[])?;
            exact_positions(&["state"], &positionals)?;
            Ok((
                0,
                commons.status(
                    &positionals[0],
                    &option(&options, "--step", ""),
                    &option(&options, "--question", ""),
                )?,
            ))
        }
        "agents" => {
            exact_positions(&[], &arguments[1..])?;
            Ok((0, json!(commons.agents()?)))
        }
        "send" => {
            let (positionals, _, _) = parse_options(&arguments[1..], &[], &[], &[])?;
            exact_positions(&["to", "body"], &positionals)?;
            Ok((0, commons.send(&positionals[0], &positionals[1])?))
        }
        "inbox" => {
            let (positionals, _, flags) = parse_options(&arguments[1..], &[], &[], &["--all"])?;
            exact_positions(&[], &positionals)?;
            Ok((0, commons.inbox(flags.contains("--all"))?))
        }
        "ack" => {
            let (positionals, _, _) = parse_options(&arguments[1..], &[], &[], &[])?;
            exact_positions(&["through"], &positionals)?;
            Ok((0, commons.ack(integer(&positionals[0])?)?))
        }
        "add" => execute_add(commons, &arguments[1..]),
        "update" => execute_update(commons, &arguments[1..]),
        "gaps" => {
            let (positionals, options, flags) =
                parse_options(&arguments[1..], &["--limit"], &[], &["--all", "--brief"])?;
            exact_positions(&[], &positionals)?;
            let gaps = commons.gaps(
                flags.contains("--all"),
                integer(&option(&options, "--limit", "200"))?,
            )?;
            if flags.contains("--brief") {
                let lines = gaps
                    .iter()
                    .map(|gap| {
                        format!(
                            "{:<8} x{} {:<8} {} {}",
                            gap["severity"].as_str().unwrap_or_default(),
                            gap["reporters"].as_array().map_or(0, Vec::len),
                            gap["status"].as_str().unwrap_or_default(),
                            gap["id"].as_str().unwrap_or_default(),
                            gap["title"].as_str().unwrap_or_default()
                        )
                    })
                    .collect::<Vec<_>>();
                return Ok((0, json!(lines)));
            }
            Ok((0, json!(gaps)))
        }
        "dedup" => {
            let (positionals, options, _) = parse_options(
                &arguments[1..],
                &["--type", "--threshold", "--limit"],
                &[],
                &[],
            )?;
            exact_positions(&[], &positionals)?;
            Ok((
                0,
                json!(commons.similar(
                    &option(&options, "--type", "gap"),
                    float(&option(&options, "--threshold", "0.3"))?,
                    integer(&option(&options, "--limit", "100"))?
                )?),
            ))
        }
        "merge" => {
            let (positionals, options, _) = parse_options(&arguments[1..], &["--into"], &[], &[])?;
            exact_positions(&["duplicate"], &positionals)?;
            let canonical = option(&options, "--into", "");
            if canonical.is_empty() {
                return Err(CommonsError::new("argument --into is required"));
            }
            Ok((0, commons.link(&positionals[0], "duplicates", &canonical)?))
        }
        "link" => {
            let (positionals, _, _) = parse_options(&arguments[1..], &[], &[], &[])?;
            exact_positions(&["src", "rel", "dst"], &positionals)?;
            Ok((
                0,
                commons.link(&positionals[0], &positionals[1], &positionals[2])?,
            ))
        }
        "show" => {
            let (positionals, _, _) = parse_options(&arguments[1..], &[], &[], &[])?;
            exact_positions(&["id"], &positionals)?;
            Ok((0, commons.record(&positionals[0])?))
        }
        "records" => {
            let (positionals, options, _) = parse_options(
                &arguments[1..],
                &["--type", "--status", "--tag", "--limit"],
                &[],
                &[],
            )?;
            exact_positions(&[], &positionals)?;
            Ok((
                0,
                json!(commons.records(
                    &option(&options, "--type", ""),
                    &option(&options, "--status", ""),
                    &option(&options, "--tag", ""),
                    integer(&option(&options, "--limit", "50"))?
                )?),
            ))
        }
        "emit" => {
            let (positionals, options, _) = parse_options(&arguments[1..], &["--body"], &[], &[])?;
            exact_positions(&["part", "event", "subject"], &positionals)?;
            let body = serde_json::from_str(&option(&options, "--body", "{}"))
                .map_err(|error| CommonsError::new(error.to_string()))?;
            let seq = commons.emit(&positionals[0], &positionals[1], &positionals[2], body)?;
            Ok((0, json!({"seq": seq})))
        }
        "events" => {
            let (positionals, options, _) =
                parse_options(&arguments[1..], &["--after", "--kind", "--limit"], &[], &[])?;
            exact_positions(&[], &positionals)?;
            Ok((
                0,
                json!(commons.events(
                    integer(&option(&options, "--after", "0"))?,
                    &option(&options, "--kind", ""),
                    integer(&option(&options, "--limit", "100"))?
                )?),
            ))
        }
        "import-gaps" => {
            let (positionals, _, _) = parse_options(&arguments[1..], &[], &[], &[])?;
            exact_positions(&["path"], &positionals)?;
            Ok((0, commons.import_gaps(&positionals[0])?))
        }
        unknown => Err(CommonsError::new(format!("unknown command {unknown}"))),
    }
}

fn execute_lease(commons: &mut Commons, arguments: &[String]) -> Result<(u8, Value), CommonsError> {
    let Some(operation) = arguments.first().map(String::as_str) else {
        return Err(CommonsError::new("a lease operation is required"));
    };
    match operation {
        "take" => {
            let (positionals, options, _) = parse_options(
                &arguments[1..],
                &["--ttl", "--note", "--force-reason"],
                &[],
                &[],
            )?;
            exact_positions(&["resource"], &positionals)?;
            Ok((
                0,
                commons.lease_take_force(
                    &positionals[0],
                    float(&option(&options, "--ttl", "3600"))?,
                    &option(&options, "--note", ""),
                    &option(&options, "--force-reason", ""),
                )?,
            ))
        }
        "renew" => {
            let (positionals, options, _) = parse_options(&arguments[1..], &["--ttl"], &[], &[])?;
            exact_positions(&["resource"], &positionals)?;
            Ok((
                0,
                commons.lease_renew(&positionals[0], float(&option(&options, "--ttl", "3600"))?)?,
            ))
        }
        "release" => {
            let (positionals, options, _) =
                parse_options(&arguments[1..], &["--force-reason"], &[], &[])?;
            exact_positions(&["resource"], &positionals)?;
            Ok((
                0,
                commons.lease_release(&positionals[0], &option(&options, "--force-reason", ""))?,
            ))
        }
        "check" => {
            let (positionals, _, _) = parse_options(&arguments[1..], &[], &[], &[])?;
            exact_positions(&["resource"], &positionals)?;
            let held = commons.lease_check(&positionals[0])?;
            Ok((
                if held.is_empty() { 0 } else { 3 },
                json!({"resource": positionals[0], "leased": held}),
            ))
        }
        "list" => {
            let (positionals, _, _) = parse_options(&arguments[1..], &[], &[], &[])?;
            exact_positions(&[], &positionals)?;
            Ok((0, json!(commons.leases()?)))
        }
        unknown => Err(CommonsError::new(format!(
            "unknown lease operation {unknown}"
        ))),
    }
}

fn execute_add(commons: &mut Commons, arguments: &[String]) -> Result<(u8, Value), CommonsError> {
    let (positionals, options, _) = parse_options(
        arguments,
        &["--body", "--status", "--tag", "--ref", "--id"],
        &["--tag", "--ref"],
        &[],
    )?;
    exact_positions(&["type", "title"], &positionals)?;
    Ok((
        0,
        commons.record_add(
            &positionals[0],
            &positionals[1],
            &option(&options, "--body", ""),
            &option(&options, "--status", "open"),
            options.get("--tag").map(Vec::as_slice).unwrap_or_default(),
            options.get("--ref").map(Vec::as_slice).unwrap_or_default(),
            &option(&options, "--id", ""),
        )?,
    ))
}

fn execute_update(
    commons: &mut Commons,
    arguments: &[String],
) -> Result<(u8, Value), CommonsError> {
    let (positionals, options, _) = parse_options(
        arguments,
        &[
            "--status",
            "--note",
            "--ref",
            "--title",
            "--severity",
            "--tag",
        ],
        &["--ref", "--tag"],
        &[],
    )?;
    exact_positions(&["id"], &positionals)?;
    Ok((
        0,
        commons.record_update_full(
            &positionals[0],
            &option(&options, "--status", ""),
            &option(&options, "--note", ""),
            options.get("--ref").map(Vec::as_slice).unwrap_or_default(),
            &option(&options, "--title", ""),
            &option(&options, "--severity", ""),
            options.get("--tag").map(Vec::as_slice).unwrap_or_default(),
        )?,
    ))
}

fn execute_export(commons: &Commons, arguments: &[String]) -> Result<u8, CommonsError> {
    let (positionals, options, flags) =
        parse_options(arguments, &["--format", "--out"], &[], &["--with-archive"])?;
    exact_positions(&["what"], &positionals)?;
    if !matches!(positionals[0].as_str(), "records" | "events") {
        return Err(CommonsError::new("what must be records or events"));
    }
    let format = option(&options, "--format", "jsonl");
    if !matches!(format.as_str(), "jsonl" | "md") {
        return Err(CommonsError::new("format must be jsonl or md"));
    }
    let lines = if positionals[0] == "events" {
        commons.export_events(flags.contains("--with-archive"))?
    } else {
        commons.export_records(&format)?
    };
    let separator = if positionals[0] == "events" || format == "jsonl" {
        "\n"
    } else {
        ""
    };
    let out = option(&options, "--out", "");
    let write = |stream: &mut dyn Write| -> Result<(), io::Error> {
        for line in &lines {
            stream.write_all(line.as_bytes())?;
            stream.write_all(separator.as_bytes())?;
        }
        Ok(())
    };
    if out.is_empty() {
        let stdout = io::stdout();
        let mut stream = stdout.lock();
        if let Err(error) = write(&mut stream) {
            if error.kind() == io::ErrorKind::BrokenPipe {
                return Ok(0);
            }
            return Err(error.into());
        }
    } else {
        let file = OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .open(&out)?;
        let mut stream = BufWriter::new(file);
        write(&mut stream)?;
        stream.flush()?;
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({"written": lines.len(), "out": out}))
                .map_err(|error| CommonsError::new(error.to_string()))?
        );
    }
    Ok(0)
}

type ParsedArguments = (Vec<String>, HashMap<String, Vec<String>>, HashSet<String>);

fn parse_options(
    arguments: &[String],
    valued: &[&str],
    repeated: &[&str],
    flags: &[&str],
) -> Result<ParsedArguments, CommonsError> {
    let mut positionals = Vec::new();
    let mut options = HashMap::<String, Vec<String>>::new();
    let mut found_flags = HashSet::new();
    let mut index = 0;
    while index < arguments.len() {
        let argument = &arguments[index];
        if flags.contains(&argument.as_str()) {
            found_flags.insert(argument.clone());
            index += 1;
            continue;
        }
        if valued.contains(&argument.as_str()) {
            let Some(value) = arguments.get(index + 1) else {
                return Err(CommonsError::new(format!(
                    "argument {argument}: expected one argument"
                )));
            };
            let values = options.entry(argument.clone()).or_default();
            if !repeated.contains(&argument.as_str()) {
                values.clear();
            }
            values.push(value.clone());
            index += 2;
            continue;
        }
        if argument.starts_with('-') {
            return Err(CommonsError::new(format!(
                "unrecognized arguments: {argument}"
            )));
        }
        positionals.push(argument.clone());
        index += 1;
    }
    Ok((positionals, options, found_flags))
}

fn exact_positions(expected: &[&str], found: &[String]) -> Result<(), CommonsError> {
    if expected.len() == found.len() {
        Ok(())
    } else {
        Err(CommonsError::new(format!(
            "expected {} argument(s): {}",
            expected.len(),
            expected.join(" ")
        )))
    }
}

fn option(options: &HashMap<String, Vec<String>>, name: &str, default: &str) -> String {
    options
        .get(name)
        .and_then(|values| values.last())
        .cloned()
        .unwrap_or_else(|| default.to_string())
}

fn integer(value: &str) -> Result<i64, CommonsError> {
    value
        .parse::<i64>()
        .map_err(|_| CommonsError::new(format!("invalid int value: '{value}'")))
}

fn float(value: &str) -> Result<f64, CommonsError> {
    value
        .parse::<f64>()
        .map_err(|_| CommonsError::new(format!("invalid float value: '{value}'")))
}

fn refuse(message: &str) -> u8 {
    let message = message.chars().take(2_000).collect::<String>();
    let mut stderr = io::stderr().lock();
    let response = json!({"refused": message});
    match serde_json::to_string(&response) {
        Ok(response) => {
            let _ = writeln!(stderr, "{response}");
        }
        Err(_) => {
            let _ = writeln!(stderr, "{{\"refused\":\"serialization error\"}}");
        }
    }
    2
}
