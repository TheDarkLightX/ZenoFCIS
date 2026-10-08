//! The operational command line: `init`, `submit`, `decide`, `state`,
//! `history`, `pending`, `deliver` and `version`.
//!
//! Commands and context fields are written by the names `project.zeno`
//! gives them, `NAME=VALUE`, and each value is checked against its declared
//! domain by the decision-examples grammar of `src/examples.rs`, the parser
//! that reads `tests/decision-examples.txt`. Every command that reads a
//! store opens it through the contract lineage, so the library Authority of
//! the version the store runs decides, and a store of another contract is
//! refused.
//!
//! Each command prints one report, as text or, with `--format json`, as one
//! JSON object, and exits with one of the codes below.

use std::ffi::OsString;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use zeno_fcis_codec::{DecodeLimits, decode_value};
use zeno_fcis_synthesis::finite::{
    V2InputLeaf as InputLeaf, canonical_v2::schema as s, v2_composition as c,
};
use zeno_fcis_value::{Value, ValueRef};

use crate::session::{
    self, Current, Decision, Delivered, Failure, FileDestination, Head, History, Outgoing, Waiting,
};
use crate::{examples, v2_contract};

/// The command completed: a submission committed, a decision computed that
/// would commit, or a report printed.
pub const OK: u8 = 0;
/// Refused: an unknown field, a value outside its domain, a store of another
/// contract or version, or another refusal of the store or the Authority.
pub const INVALID: u8 = 1;
/// The Authority rejected the command; nothing was written.
pub const BLOCKED: u8 = 2;
/// A file beside the store, such as the journal or the delivery file, could
/// not be read or written.
pub const FAILURE: u8 = 3;
/// The command line is malformed.
pub const USAGE: u8 = 64;

/// The ZenoFCIS release this application was generated for; `zeno-fcis new`
/// writes it.
pub const ZENO_FCIS_VERSION: &str = "{zeno_fcis_version}";

/// The operational commands, by name.
pub const COMMANDS: &[&str] = &[
    "init", "submit", "decide", "state", "history", "pending", "deliver", "version",
];

/// The usage text of the operational commands.
#[must_use]
pub fn usage() -> String {
    let name = env!("CARGO_PKG_NAME");
    format!(
        "usage: {name} init DATABASE_PATH [--format human|json]\n       {name} submit DATABASE_PATH NAME=VALUE... [--format human|json]\n       {name} decide DATABASE_PATH NAME=VALUE... [--format human|json]\n       {name} state DATABASE_PATH [--format human|json]\n       {name} history DATABASE_PATH [--format human|json]\n       {name} pending DATABASE_PATH [--format human|json]\n       {name} deliver DATABASE_PATH (--to FILE | --relay CONFIG) [--format human|json]\n       {name} version [--format human|json]"
    )
}

/// What a command printed and its exit code.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Outcome {
    /// The exit code: `OK`, `INVALID`, `BLOCKED`, `FAILURE` or `USAGE`.
    pub exit: u8,
    /// The report.
    pub stdout: String,
    /// The error message, if any.
    pub stderr: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Format {
    Human,
    Json,
}

/// Runs an operational command, or returns `None` when the first argument
/// names none, so the caller can read the other forms.
#[must_use]
pub fn run(arguments: &[OsString]) -> Option<Outcome> {
    let (first, rest) = arguments.split_first()?;
    let command = COMMANDS
        .iter()
        .copied()
        .find(|command| first.to_str() == Some(command))?;
    let mut format = Format::Human;
    let mut to = None;
    let mut relay = None;
    let mut words = Vec::new();
    let mut rest = rest.iter();
    while let Some(word) = rest.next() {
        match word.to_str() {
            Some("--format") => match rest.next().and_then(|value| value.to_str()) {
                Some("human") => format = Format::Human,
                Some("json") => format = Format::Json,
                _ => return Some(usage_error(format, "--format takes human or json")),
            },
            Some("--to") if command == "deliver" => match rest.next() {
                Some(file) => to = Some(PathBuf::from(file)),
                None => return Some(usage_error(format, "--to takes a file path")),
            },
            Some("--relay") if command == "deliver" => match rest.next() {
                Some(file) => relay = Some(PathBuf::from(file)),
                None => return Some(usage_error(format, "--relay takes a configuration path")),
            },
            Some(flag) if flag.starts_with("--") => {
                return Some(usage_error(format, &format!("unknown option {flag}")));
            }
            _ => words.push(word.clone()),
        }
    }
    if to.is_some() && relay.is_some() {
        return Some(usage_error(
            format,
            "deliver takes either --to FILE or --relay CONFIG",
        ));
    }
    let database = words.first().map(PathBuf::from);
    let outcome = match (command, database, words.len()) {
        ("version", None, 0) => version(),
        ("init", Some(path), 1) => {
            session::init(&path).map(|head| report_head("initialized", &head))
        }
        ("submit", Some(path), _) => decide_command(&path, &words[1..], true),
        ("decide", Some(path), _) => decide_command(&path, &words[1..], false),
        ("state", Some(path), 1) => session::current(&path).map(|current| report_state(&current)),
        ("history", Some(path), 1) => {
            session::history(&path).and_then(|history| report_history(&history))
        }
        ("pending", Some(path), 1) => {
            session::pending(&path).map(|waiting| report_pending(&waiting))
        }
        ("deliver", Some(path), 1) => match (to, relay) {
            (Some(file), None) => session::deliver_to(&path, &mut FileDestination::new(&file))
                .map(|delivered| report_delivered(&delivered, &file)),
            (None, Some(config)) => {
                session::deliver_to(&path, &mut crate::relay::RelayDestination::new(&config))
                    .map(|delivered| report_relay(&delivered, &config))
            }
            _ => {
                return Some(usage_error(
                    format,
                    "deliver needs --to FILE or --relay CONFIG",
                ));
            }
        },
        _ => return Some(usage_error(format, &usage())),
    };
    Some(match outcome {
        Ok(report) => report.finish(format),
        Err(failure) => {
            let (exit, code) = match failure {
                Failure::Refused(_) => (INVALID, "refused"),
                Failure::Io(_) => (FAILURE, "io-error"),
            };
            error(format, exit, code, &failure.to_string())
        }
    })
}

/// A report: the JSON object's members, the human text and the exit code.
struct Report {
    exit: u8,
    members: Vec<(&'static str, String)>,
    human: String,
}

impl Report {
    fn finish(self, format: Format) -> Outcome {
        let stdout = match format {
            Format::Json => format!("{}\n", object(&self.members)),
            Format::Human => self.human,
        };
        Outcome {
            exit: self.exit,
            stdout,
            stderr: String::new(),
        }
    }
}

fn error(format: Format, exit: u8, code: &str, message: &str) -> Outcome {
    let stdout = match format {
        Format::Json => format!(
            "{}\n",
            object(&[
                ("status", quote("error")),
                (
                    "error",
                    object(&[("code", quote(code)), ("message", quote(message))])
                ),
            ])
        ),
        Format::Human => String::new(),
    };
    Outcome {
        exit,
        stdout,
        stderr: format!("{message}\n"),
    }
}

fn usage_error(format: Format, message: &str) -> Outcome {
    error(format, USAGE, "usage", message)
}

/// A JSON string.
fn quote(text: &str) -> String {
    let mut quoted = String::from("\"");
    for character in text.chars() {
        match character {
            '"' => quoted.push_str("\\\""),
            '\\' => quoted.push_str("\\\\"),
            '\n' => quoted.push_str("\\n"),
            '\r' => quoted.push_str("\\r"),
            '\t' => quoted.push_str("\\t"),
            control if u32::from(control) < 0x20 => {
                let _ = write!(quoted, "\\u{:04x}", u32::from(control));
            }
            other => quoted.push(other),
        }
    }
    quoted.push('"');
    quoted
}

fn object(members: &[(&str, String)]) -> String {
    let members: Vec<String> = members
        .iter()
        .map(|(key, value)| format!("{}:{value}", quote(key)))
        .collect();
    format!("{{{}}}", members.join(","))
}

fn array(items: &[String]) -> String {
    format!("[{}]", items.join(","))
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// How a value of a declared type is written.
#[derive(Clone, Debug, Eq, PartialEq)]
enum Kind {
    Bool,
    Int,
    /// Each variant's ID and name.
    Sum(Vec<(u16, String)>),
    /// A value with no command-line form, such as text.
    Other,
}

fn definition_in<'a>(
    description: &'a s::Description<'a>,
    type_id: u32,
) -> Option<&'a s::Definition<'a>> {
    description
        .definitions
        .iter()
        .find(|definition| definition.id == type_id)
}

fn kind_of_type(type_id: u32) -> Kind {
    kind_of_type_in(&v2_contract::DESCRIPTION, type_id)
}

fn kind_of_type_in(description: &s::Description<'_>, type_id: u32) -> Kind {
    match definition_in(description, type_id).map(|definition| &definition.kind) {
        Some(s::Kind::Bool) => Kind::Bool,
        Some(s::Kind::I128 { .. } | s::Kind::U128 { .. }) => Kind::Int,
        Some(s::Kind::Sum(variants)) => Kind::Sum(
            variants
                .iter()
                .map(|variant| (variant.id, text(variant.name)))
                .collect(),
        ),
        _ => Kind::Other,
    }
}

fn kind_of_leaf(description: &s::Description<'_>, leaf: &InputLeaf) -> Kind {
    match leaf {
        InputLeaf::Bool => Kind::Bool,
        InputLeaf::I128 { .. } => Kind::Int,
        InputLeaf::Sum { type_id, .. } => kind_of_type_in(description, *type_id),
        _ => Kind::Other,
    }
}

/// The fields of a record type: each one's ID, name and type.
fn record_fields(type_id: u32) -> Vec<(u16, String, u32)> {
    record_fields_in(&v2_contract::DESCRIPTION, type_id)
}

fn record_fields_in(description: &s::Description<'_>, type_id: u32) -> Vec<(u16, String, u32)> {
    match definition_in(description, type_id).map(|definition| &definition.kind) {
        Some(s::Kind::Record(fields)) => fields
            .iter()
            .map(|field| (field.id, text(field.name), field.type_id))
            .collect(),
        _ => Vec::new(),
    }
}

/// One program input a command line names: a field of a root record, or a
/// whole scalar root, named after the root.
#[derive(Clone, Debug)]
struct Input {
    root: &'static str,
    name: String,
    kind: Kind,
}

/// Every program input, in program order: the state fields, then the
/// command, then the context.
fn inputs() -> Vec<Input> {
    let contract = v2_contract::Contract::new();
    let descriptor = contract.descriptor();
    inputs_for(
        &descriptor,
        &v2_contract::DESCRIPTION,
        &v2_contract::FRAMING,
    )
}

fn inputs_for(
    descriptor: &c::Descriptor<'_>,
    description: &s::Description<'_>,
    framing: &c::Framing,
) -> Vec<Input> {
    let mut inputs = Vec::new();
    for (root, schema, type_id) in [
        ("state", descriptor.state, framing.state.root),
        ("command", descriptor.command, framing.command.root),
        ("context", descriptor.context, framing.context.root),
    ] {
        match schema {
            c::Schema::Record(fields) => {
                let names = record_fields_in(description, type_id);
                inputs.extend(fields.iter().map(|field| Input {
                    root,
                    name: names.iter().find(|(id, _, _)| *id == field.id).map_or_else(
                        || format!("field_{}", field.id),
                        |(_, name, _)| name.clone(),
                    ),
                    kind: kind_of_leaf(description, &field.leaf),
                }));
            }
            c::Schema::Leaf(leaf) => inputs.push(Input {
                root,
                name: root.to_owned(),
                kind: kind_of_leaf(description, leaf),
            }),
            _ => {}
        }
    }
    inputs
}

/// A value written on the command line, as the number the examples grammar
/// reads: `true` or `false` (or 1 or 0) for a boolean, a decimal integer,
/// or a variant's name (or ID) for a sum.
fn number(kind: &Kind, value: &str) -> Result<i128, String> {
    let parsed = value.parse::<i128>().ok();
    match kind {
        Kind::Bool => match value {
            "true" => Ok(1),
            "false" => Ok(0),
            _ => parsed.ok_or_else(|| format!("`{value}` is not true or false")),
        },
        Kind::Int => parsed.ok_or_else(|| format!("`{value}` is not an integer")),
        Kind::Sum(variants) => variants
            .iter()
            .find(|(_, name)| name == value)
            .map(|(id, _)| i128::from(*id))
            .or(parsed)
            .ok_or_else(|| {
                let names: Vec<&str> = variants.iter().map(|(_, name)| name.as_str()).collect();
                format!("`{value}` is not one of {}", names.join(", "))
            }),
        Kind::Other => Err("this field has no command-line form".to_owned()),
    }
}

/// A number as JSON: a boolean, an integer or a variant's name.
fn json_number(kind: &Kind, number: i128) -> String {
    match kind {
        Kind::Bool if number == 0 || number == 1 => (number == 1).to_string(),
        Kind::Sum(variants) => variants
            .iter()
            .find(|(id, _)| i128::from(*id) == number)
            .map_or_else(|| number.to_string(), |(_, name)| quote(name)),
        _ => number.to_string(),
    }
}

/// A number as text: as JSON, without the quotes around a variant name.
fn human_number(kind: &Kind, number: i128) -> String {
    json_number(kind, number).trim_matches('"').to_owned()
}

/// The command and context numbers, in program order, from `NAME=VALUE`
/// words. A name may be qualified, `command.NAME` or `context.NAME`, and
/// must be when both roots have a field of that name.
fn command_numbers(words: &[OsString]) -> Result<(Vec<i128>, Vec<i128>), String> {
    let contract = v2_contract::Contract::new();
    let shape = examples::Shape::of(&contract.descriptor())?;
    let inputs = inputs();
    let mut values: Vec<Option<i128>> = vec![None; inputs.len()];
    for word in words {
        let word = word
            .to_str()
            .ok_or_else(|| format!("{} is not UTF-8", word.to_string_lossy()))?;
        let (name, value) = word
            .split_once('=')
            .ok_or_else(|| format!("`{word}` is not NAME=VALUE"))?;
        let (root, field) = match name.split_once('.') {
            Some((root @ ("command" | "context"), field)) => (Some(root), field),
            _ => (None, name),
        };
        let matches: Vec<usize> = inputs
            .iter()
            .enumerate()
            .filter(|(_, input)| {
                input.root != "state"
                    && input.name == field
                    && root.is_none_or(|root| root == input.root)
            })
            .map(|(index, _)| index)
            .collect();
        let index = match matches.as_slice() {
            [index] => *index,
            [] => {
                let known: Vec<String> = inputs
                    .iter()
                    .filter(|input| input.root != "state")
                    .map(|input| format!("{}.{}", input.root, input.name))
                    .collect();
                return Err(format!(
                    "unknown field `{name}`: the contract reads {}",
                    known.join(", ")
                ));
            }
            _ => {
                return Err(format!(
                    "`{name}` names a command and a context field: write command.{field} or context.{field}"
                ));
            }
        };
        if values[index].is_some() {
            return Err(format!("`{name}` is given twice"));
        }
        let input = &inputs[index];
        let checked = number(&input.kind, value)
            .and_then(|number| shape.check_input(index, number))
            .map_err(|reason| format!("{}.{}={value}: {reason}", input.root, input.name))?;
        values[index] = Some(checked);
    }
    let mut command = Vec::new();
    let mut context = Vec::new();
    for (input, value) in inputs.iter().zip(values) {
        let target = match input.root {
            "command" => &mut command,
            "context" => &mut context,
            _ => continue,
        };
        target.push(value.ok_or_else(|| format!("missing field {}.{}", input.root, input.name))?);
    }
    Ok((command, context))
}

/// The `NAME=VALUE` words that submit `command` and `context`, numbers in
/// program order as the decision examples write them; each name is
/// qualified, `command.NAME` or `context.NAME`.
///
/// # Errors
/// Returns a count that does not fit the command and context roots.
pub fn assignments(command: &[i128], context: &[i128]) -> Result<Vec<String>, String> {
    let inputs = inputs();
    let mut words = Vec::new();
    for (root, numbers) in [("command", command), ("context", context)] {
        let named = root_inputs(&inputs, root);
        if named.len() != numbers.len() {
            return Err(format!(
                "{} {root} numbers; the contract reads {}",
                numbers.len(),
                named.len()
            ));
        }
        words.extend(named.iter().zip(numbers).map(|(input, number)| {
            format!(
                "{root}.{}={}",
                input.name,
                human_number(&input.kind, *number)
            )
        }));
    }
    Ok(words)
}

/// Named fields as a JSON object and as `name=value` text.
fn named(inputs: &[&Input], numbers: &[i128]) -> (String, String) {
    let members: Vec<(&str, String)> = inputs
        .iter()
        .zip(numbers)
        .map(|(input, number)| (input.name.as_str(), json_number(&input.kind, *number)))
        .collect();
    let human: Vec<String> = inputs
        .iter()
        .zip(numbers)
        .map(|(input, number)| format!("{}={}", input.name, human_number(&input.kind, *number)))
        .collect();
    (object(&members), human.join(" "))
}

fn root_inputs<'a>(inputs: &'a [Input], root: &str) -> Vec<&'a Input> {
    inputs.iter().filter(|input| input.root == root).collect()
}

/// Each delivery of a decision, as JSON objects and as text.
fn deliveries(
    outbox: &[(u32, Vec<(u16, i128)>)],
    description: &s::Description<'_>,
    channel_roots: &[(u32, u32, u32)],
) -> (Vec<String>, Vec<String>) {
    outbox
        .iter()
        .map(|(channel, payload)| {
            let fields = channel_roots
                .iter()
                .find(|(id, _, _)| id == channel)
                .map(|(_, _, payload)| record_fields_in(description, *payload))
                .unwrap_or_default();
            let mut members = Vec::new();
            let mut human = Vec::new();
            for (id, number) in payload {
                let (name, kind) = fields.iter().find(|(field, _, _)| field == id).map_or_else(
                    || (format!("field_{id}"), Kind::Int),
                    |(_, name, type_id)| (name.clone(), kind_of_type_in(description, *type_id)),
                );
                human.push(format!("{name}={}", human_number(&kind, *number)));
                members.push((name, json_number(&kind, *number)));
            }
            let members: Vec<(&str, String)> = members
                .iter()
                .map(|(name, value)| (name.as_str(), value.clone()))
                .collect();
            (
                object(&[
                    ("channel", channel.to_string()),
                    ("payload", object(&members)),
                ]),
                format!("channel {channel}: {}", human.join(" ")),
            )
        })
        .unzip()
}

fn head_members(head: &Head) -> Vec<(&'static str, String)> {
    vec![
        ("contract_version", head.contract_version.to_string()),
        ("commits", head.commits.to_string()),
        ("pending", head.pending.to_string()),
        ("upgrades", head.upgrades.to_string()),
    ]
}

fn head_human(head: &Head) -> String {
    format!(
        "contract version {}, {} commits, {} pending deliveries, {} upgrades\n",
        head.contract_version, head.commits, head.pending, head.upgrades
    )
}

fn report_head(status: &'static str, head: &Head) -> Report {
    let mut members = vec![("status", quote(status))];
    members.extend(head_members(head));
    Report {
        exit: OK,
        members,
        human: format!("{status}: {}", head_human(head)),
    }
}

fn version() -> Result<Report, Failure> {
    let identity = session::identity()?;
    let name = env!("CARGO_PKG_NAME");
    Ok(Report {
        exit: OK,
        members: vec![
            ("status", quote("version")),
            ("application", quote(name)),
            ("contract_version", v2_contract::VERSION.to_string()),
            ("identity", quote(&identity)),
            ("zeno_fcis", quote(ZENO_FCIS_VERSION)),
        ],
        human: format!(
            "{name}: contract version {}, identity {identity}, ZenoFCIS {ZENO_FCIS_VERSION}\n",
            v2_contract::VERSION
        ),
    })
}

fn decide_command(path: &Path, words: &[OsString], commit: bool) -> Result<Report, Failure> {
    let (command, context) = command_numbers(words)?;
    let decision = if commit {
        session::submit(path, &command, &context)?
    } else {
        session::preview(path, &command, &context)?
    };
    let status = match (decision.class, commit) {
        ("Reject", _) => "rejected",
        (_, true) => "committed",
        (_, false) => "decided",
    };
    Ok(report_decision(status, &decision))
}

fn decision_members(decision: &Decision) -> (Vec<(&'static str, String)>, String) {
    let inputs = inputs();
    decision_members_for(
        decision,
        &inputs,
        &v2_contract::DESCRIPTION,
        v2_contract::CHANNEL_ROOTS,
    )
}

fn decision_members_for(
    decision: &Decision,
    inputs: &[Input],
    description: &s::Description<'_>,
    channel_roots: &[(u32, u32, u32)],
) -> (Vec<(&'static str, String)>, String) {
    let (state, state_human) = named(&root_inputs(inputs, "state"), &decision.post);
    let (outbox, outbox_human) = deliveries(&decision.outbox, description, channel_roots);
    let members = vec![
        ("class", quote(decision.class)),
        (
            "reason",
            decision
                .reason
                .map_or_else(|| "null".to_owned(), |reason| reason.to_string()),
        ),
        (
            "commit",
            decision
                .commit
                .map_or_else(|| "null".to_owned(), |commit| commit.to_string()),
        ),
        ("state", state),
        ("deliveries", array(&outbox)),
    ];
    let mut human = decision.class.to_owned();
    if let Some(reason) = decision.reason {
        let _ = write!(human, " (reason {reason})");
    }
    if let Some(commit) = decision.commit {
        let _ = write!(human, ", commit {commit}");
    }
    let _ = write!(human, "\n  state: {state_human}\n");
    for delivery in outbox_human {
        let _ = writeln!(human, "  delivery {delivery}");
    }
    (members, human)
}

fn report_decision(status: &'static str, decision: &Decision) -> Report {
    let (members, human) = decision_members(decision);
    let mut all = vec![("status", quote(status))];
    all.extend(members);
    Report {
        exit: if decision.class == "Reject" {
            BLOCKED
        } else {
            OK
        },
        members: all,
        human: format!("{status}: {human}"),
    }
}

fn report_state(current: &Current) -> Report {
    let inputs = inputs();
    let (state, human) = named(&root_inputs(&inputs, "state"), &current.state);
    let mut members = vec![("status", quote("state"))];
    members.extend(head_members(&current.head));
    members.push(("state", state));
    Report {
        exit: OK,
        members,
        human: format!("state: {human}\n{}", head_human(&current.head)),
    }
}

fn report_history(history: &History) -> Result<Report, Failure> {
    session::with_lineage(|lineage| {
        let metadata = |version: usize| -> Result<_, String> {
            let catalog = version
                .checked_sub(1)
                .and_then(|index| lineage.catalogs().get(index))
                .ok_or_else(|| format!("history: absent contract version {version}"))?;
            let description = v2_contract::schema_description(version)
                .ok_or_else(|| format!("history: absent schema labels for version {version}"))?;
            if !s::encoding_matches(
                catalog.original_schema(),
                description,
                catalog.original_schema().len() as u64,
            ) {
                return Err(format!(
                    "history: schema labels differ from checked version {version}"
                ));
            }
            Ok((*catalog, description))
        };
        let (catalog, description) = metadata(history.genesis_contract_version)?;
        let inputs = inputs_for(catalog.descriptor(), description, catalog.framing());
        check_number_count(&root_inputs(&inputs, "state"), &history.genesis)?;
        let (genesis, genesis_human) = named(&root_inputs(&inputs, "state"), &history.genesis);
        let mut entries = Vec::new();
        let mut human = format!("genesis: {genesis_human}\n");
        for entry in &history.entries {
            let (catalog, description) = metadata(entry.contract_version)?;
            let inputs = inputs_for(catalog.descriptor(), description, catalog.framing());
            check_number_count(&root_inputs(&inputs, "command"), &entry.command)?;
            check_number_count(&root_inputs(&inputs, "context"), &entry.context)?;
            check_number_count(&root_inputs(&inputs, "state"), &entry.decision.post)?;
            let (command, command_human) = named(&root_inputs(&inputs, "command"), &entry.command);
            let (context, context_human) = named(&root_inputs(&inputs, "context"), &entry.context);
            let (members, decision_human) = decision_members_for(
                &entry.decision,
                &inputs,
                description,
                catalog.channel_roots(),
            );
            let mut all = vec![
                ("contract_version", entry.contract_version.to_string()),
                ("command", command),
                ("context", context),
            ];
            all.extend(members);
            entries.push(object(&all));
            let _ = write!(
                human,
                "commit {}: {command_human} | {context_human}\n  {decision_human}",
                entry.decision.commit.unwrap_or_default()
            );
        }
        let mut members = vec![("status", quote("history"))];
        members.extend(head_members(&history.head));
        members.push(("genesis", genesis));
        members.push(("entries", array(&entries)));
        human.push_str(&head_human(&history.head));
        Ok(Report {
            exit: OK,
            members,
            human,
        })
    })
    .map_err(Failure::from)
}

fn check_number_count(inputs: &[&Input], numbers: &[i128]) -> Result<(), String> {
    if inputs.len() != numbers.len() {
        return Err(format!(
            "history: {} fields but {} recorded numbers",
            inputs.len(),
            numbers.len()
        ));
    }
    Ok(())
}

fn report_pending(waiting: &Waiting) -> Report {
    let mut members = vec![("status", quote("pending"))];
    members.extend(head_members(&waiting.head));
    members.push((
        "next",
        waiting
            .next
            .as_ref()
            .map_or_else(|| "null".to_owned(), delivery_json),
    ));
    let next = waiting.next.as_ref().map_or_else(
        || "none".to_owned(),
        |next| format!("{}\n", delivery_json(next)),
    );
    Report {
        exit: OK,
        members,
        human: format!("{}next: {next}", head_human(&waiting.head)),
    }
}

fn report_delivered(delivered: &Delivered, file: &Path) -> Report {
    let ids: Vec<String> = delivered.deliveries.iter().map(|id| quote(id)).collect();
    let mut members = vec![
        ("status", quote("delivered")),
        ("destination", quote("file")),
        ("file", quote(&file.display().to_string())),
        ("deliveries", array(&ids)),
    ];
    members.extend(head_members(&delivered.head));
    Report {
        exit: OK,
        members,
        human: format!(
            "delivered {} to {}\n{}",
            delivered.deliveries.len(),
            file.display(),
            head_human(&delivered.head)
        ),
    }
}

fn report_relay(delivered: &Delivered, config: &Path) -> Report {
    let mut report = report_head("delivered", &delivered.head);
    report.members.push(("destination", quote("relay")));
    report
        .members
        .push(("config", quote(&config.display().to_string())));
    report.members.push((
        "deliveries",
        array(
            &delivered
                .deliveries
                .iter()
                .map(|id| quote(id))
                .collect::<Vec<_>>(),
        ),
    ));
    report.human = format!(
        "relayed {} deliveries\n{}",
        delivered.deliveries.len(),
        head_human(&delivered.head)
    );
    report
}

/// A stored value as JSON, by the names its type declares: a record as an
/// object, a sum as its variant's name, text as a string and bytes as
/// hexadecimal.
fn value_json(value: &Value, type_id: u32) -> String {
    match value.view() {
        ValueRef::Record(fields) => {
            let names = record_fields(type_id);
            let members: Vec<(String, String)> = fields
                .iter()
                .map(|field| {
                    names
                        .iter()
                        .find(|(id, _, _)| *id == field.id())
                        .map_or_else(
                            || {
                                (
                                    format!("field_{}", field.id()),
                                    value_json(field.value(), 0),
                                )
                            },
                            |(_, name, field_type)| {
                                (name.clone(), value_json(field.value(), *field_type))
                            },
                        )
                })
                .collect();
            let members: Vec<(&str, String)> = members
                .iter()
                .map(|(name, value)| (name.as_str(), value.clone()))
                .collect();
            object(&members)
        }
        ValueRef::Bool(value) => value.to_string(),
        ValueRef::I128(value) => value.to_string(),
        ValueRef::U128(value) => value.to_string(),
        ValueRef::Text(text) => quote(text),
        ValueRef::Bytes(bytes) => quote(&hex(bytes)),
        ValueRef::Sum { variant, .. } | ValueRef::Enum { variant, .. } => {
            json_number(&kind_of_type(type_id), i128::from(variant))
        }
        _ => "null".to_owned(),
    }
}

fn stored_json(bytes: &[u8], type_id: u32) -> String {
    decode_value(bytes, DecodeLimits::default()).map_or_else(
        |_| object(&[("undecoded", quote(&hex(bytes)))]),
        |value| value_json(&value, type_id),
    )
}

/// A delivery as one JSON object on one line, its ID first.
pub(crate) fn delivery_json(delivery: &Outgoing) -> String {
    object(&[
        ("delivery_id", quote(&delivery.id)),
        ("entry_hash", quote(&delivery.entry_hash)),
        ("commit", delivery.commit.to_string()),
        ("lane", delivery.lane.to_string()),
        ("ordinal", delivery.ordinal.to_string()),
        ("channel", delivery.channel.to_string()),
        (
            "destination",
            stored_json(&delivery.destination, delivery.destination_type),
        ),
        (
            "payload",
            stored_json(&delivery.payload, delivery.payload_type),
        ),
    ])
}
