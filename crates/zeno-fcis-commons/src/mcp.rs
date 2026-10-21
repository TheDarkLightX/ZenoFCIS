use std::io::{self, BufRead, Write};

use serde_json::{Value, json};

use crate::{
    AGENT_STATES, Commons, CommonsError, LINK_RELATIONS, MAX_TEXT, RECORD_STATUSES, RECORD_TYPES,
};

const INSTRUCTIONS: &str = "Shared state of the software factory. Register once (commons_register). Read before acting (commons_advise), write when you learn (commons_record_add), acknowledge messages and lease what you use. Records cite evidence; they do not prove anything by themselves.";
const SERVER_NAME: &str = "zenofcis-commons";
const SERVER_VERSION: &str = "0.1.0";

/// Run the newline-delimited JSON-RPC server on standard input and output.
pub fn serve(directory: &str, actor: &str) -> u8 {
    let mut commons = match Commons::open(directory, actor) {
        Ok(commons) => commons,
        Err(error) => {
            eprintln!("{error}");
            return 2;
        }
    };
    let stdin = io::stdin();
    let mut input = stdin.lock();
    let stdout = io::stdout();
    let mut output = stdout.lock();
    let mut line = Vec::new();
    loop {
        line.clear();
        match read_request_line(&mut input, &mut line) {
            Ok(false) => return 0,
            Ok(true) => {}
            Err(error) => {
                eprintln!("{error}");
                return 1;
            }
        }
        if line.len() > 512 * 1024 || !line.ends_with(b"\n") {
            eprintln!("MCP input exceeds 512 KiB or is incomplete");
            return 1;
        }
        let request: Value = match serde_json::from_slice(&line) {
            Ok(request) => request,
            Err(error) => {
                let response = json!({
                    "jsonrpc": "2.0",
                    "id": null,
                    "error": {"code": -32600, "message": truncate(&error.to_string())}
                });
                if write_response(&mut output, &response).is_err() {
                    return 1;
                }
                continue;
            }
        };
        let id = request.get("id").cloned();
        let result = handle(&mut commons, &request);
        let Some(result) = result else {
            continue;
        };
        let response = match result {
            Ok(result) => json!({"jsonrpc": "2.0", "id": id, "result": result}),
            Err(error) => json!({
                "jsonrpc": "2.0",
                "id": id.filter(is_valid_error_id),
                "error": {"code": -32600, "message": truncate(&error.to_string())}
            }),
        };
        if write_response(&mut output, &response).is_err() {
            return 1;
        }
    }
}

fn read_request_line(input: &mut impl BufRead, line: &mut Vec<u8>) -> io::Result<bool> {
    const MAX_LINE: usize = 512 * 1024;
    let mut reached_eof = false;
    loop {
        let available = input.fill_buf()?;
        if available.is_empty() {
            reached_eof = true;
            break;
        }
        let requested = available
            .iter()
            .position(|byte| *byte == b'\n')
            .map_or(available.len(), |newline| newline + 1);
        let remaining = MAX_LINE + 1 - line.len().min(MAX_LINE + 1);
        let taken = requested.min(remaining);
        line.extend_from_slice(&available[..taken]);
        input.consume(taken);
        if taken < requested || line.ends_with(b"\n") || line.len() > MAX_LINE {
            break;
        }
    }
    Ok(!line.is_empty() || !reached_eof)
}

fn write_response(output: &mut impl Write, response: &Value) -> io::Result<()> {
    serde_json::to_writer(&mut *output, response).map_err(io::Error::other)?;
    output.write_all(b"\n")?;
    output.flush()
}

fn handle(commons: &mut Commons, request: &Value) -> Option<Result<Value, CommonsError>> {
    if !request.is_object()
        || request.get("jsonrpc").and_then(Value::as_str) != Some("2.0")
        || !request.get("method").is_some_and(Value::is_string)
    {
        return Some(Err(CommonsError::new("expected a JSON-RPC 2.0 request")));
    }
    let _ = request.get("id")?;
    let method = request["method"].as_str().unwrap_or_default();
    let params = request
        .get("params")
        .filter(|value| !falsey(value))
        .cloned()
        .unwrap_or_else(|| json!({}));
    Some(match method {
        "initialize" => Ok(json!({
            "protocolVersion": "2024-11-05",
            "capabilities": {"tools": {}},
            "serverInfo": {"name": SERVER_NAME, "version": SERVER_VERSION},
            "instructions": INSTRUCTIONS
        })),
        "ping" => Ok(json!({})),
        "tools/list" => Ok(json!({"tools": tools()})),
        "tools/call" => {
            let arguments = params
                .get("arguments")
                .filter(|value| !falsey(value))
                .cloned()
                .unwrap_or_else(|| json!({}));
            if !arguments.is_object() {
                Ok(tool_error("arguments must be an object"))
            } else {
                let name = params
                    .get("name")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                match call(commons, name, &arguments) {
                    Ok(result) => Ok(json!({
                        "content": [{"type": "text", "text": pretty(&result)}],
                        "isError": false
                    })),
                    Err(error) => Ok(tool_error(&error.to_string())),
                }
            }
        }
        _ => Err(CommonsError::new("unknown method")),
    })
}

fn call(commons: &mut Commons, name: &str, arguments: &Value) -> Result<Value, CommonsError> {
    let object = arguments
        .as_object()
        .ok_or_else(|| CommonsError::new("arguments must be an object"))?;
    match name {
        "commons_register" => commons.register(
            "",
            optional_string(object, "kind", "agent")?,
            optional_string(object, "role", "")?,
        ),
        "commons_whoami" => commons.identity(optional_string(object, "who", "")?),
        "commons_advise" => {
            let action = required_string(object, "action")?;
            let tags = tag_values(object.get("tags"))?;
            let text = optional_string(object, "text", "")?;
            Ok(commons.advise(action, &tags, text, 10)?)
        }
        "commons_dashboard" => commons.dashboard(),
        "commons_lease_take" => {
            let resource = required_string(object, "resource")?;
            let ttl = number(object, "ttl_seconds", 3_600.0)?;
            let note = optional_string(object, "note", "")?;
            let force_reason = optional_string(object, "force_reason", "")?;
            commons.lease_take_force(resource, ttl, note, force_reason)
        }
        "commons_lease_renew" => {
            let resource = required_string(object, "resource")?;
            let ttl = number(object, "ttl_seconds", 3_600.0)?;
            commons.lease_renew(resource, ttl)
        }
        "commons_lease_release" => commons.lease_release(required_string(object, "resource")?, ""),
        "commons_lease_check" => commons
            .lease_check(required_string(object, "resource")?)
            .map(|result| json!(result)),
        "commons_status" => commons.status(
            required_string(object, "state")?,
            optional_string(object, "step", "")?,
            optional_string(object, "question", "")?,
        ),
        "commons_agents" => commons.agents().map(|result| json!(result)),
        "commons_send" => commons.send(
            required_string(object, "to")?,
            required_string(object, "body")?,
        ),
        "commons_inbox" => commons.inbox(
            object
                .get("include_read")
                .is_some_and(|value| !falsey(value)),
        ),
        "commons_ack" => commons.ack(integer(object, "through")?),
        "commons_record_add" => {
            let record_type = required_string(object, "type")?;
            let title = required_string(object, "title")?;
            let body = optional_string(object, "body", "")?;
            let status = optional_string(object, "status", "open")?;
            let tags = tag_values(object.get("tags"))?;
            let refs = refs_values(object.get("refs"))?;
            commons.record_add(record_type, title, body, status, &tags, &refs, "")
        }
        "commons_record_update" => {
            let id = required_string(object, "id")?;
            let status = optional_string(object, "status", "")?;
            let note = optional_string(object, "note", "")?;
            let refs = refs_values(object.get("refs"))?;
            let tags = tag_values(object.get("tags"))?;
            commons.record_update_full(
                id,
                status,
                note,
                &refs,
                optional_string(object, "title", "")?,
                optional_string(object, "severity", "")?,
                &tags,
            )
        }
        "commons_gaps" => commons
            .gaps(
                object
                    .get("include_closed")
                    .is_some_and(|value| !falsey(value)),
                integer_or(object, "limit", 200)?,
            )
            .map(|result| json!(result)),
        "commons_dedup" => commons
            .similar(
                optional_string(object, "type", "gap")?,
                number(object, "threshold", 0.3)?,
                integer_or(object, "limit", 100)?,
            )
            .map(|result| json!(result)),
        "commons_link" => commons.link(
            required_string(object, "src")?,
            required_string(object, "rel")?,
            required_string(object, "dst")?,
        ),
        "commons_record" => commons.record(required_string(object, "id")?),
        "commons_records" => commons
            .records(
                optional_string(object, "type", "")?,
                optional_string(object, "status", "")?,
                optional_string(object, "tag", "")?,
                integer_or(object, "limit", 50)?,
            )
            .map(|result| json!(result)),
        "commons_emit" => {
            let seq = commons.emit(
                required_string(object, "part")?,
                required_string(object, "event")?,
                required_string(object, "subject")?,
                object.get("body").cloned().unwrap_or(Value::Null),
            )?;
            Ok(json!({"seq": seq}))
        }
        _ => Err(CommonsError::new(format!("unknown tool {name}"))),
    }
}

fn tools() -> Vec<Value> {
    let text = json!({"type": "string", "maxLength": MAX_TEXT});
    let names =
        json!({"type": "array", "items": {"type": "string", "maxLength": 128}, "maxItems": 32});
    let refs =
        json!({"type": "array", "items": {"type": "string", "maxLength": 4096}, "maxItems": 64});
    vec![
        tool(
            "commons_register",
            "Register your identity once: your server's actor name plus kind and role. Returns your hash ID. Others address you by name or ID.",
            schema(json!({"kind": text, "role": text}), &[]),
        ),
        tool(
            "commons_whoami",
            "Your identity, or another agent's by name or hash ID.",
            schema(json!({"who": text}), &[]),
        ),
        tool(
            "commons_advise",
            "Read before acting: lessons, refutations, gaps and risks relevant to an action, plus leases on any paths named in text and your unread messages.",
            schema(
                json!({"action": text, "tags": names, "text": text}),
                &["action"],
            ),
        ),
        tool(
            "commons_dashboard",
            "Agents and their states, active leases, record counts and your unread message count.",
            schema(json!({}), &[]),
        ),
        tool(
            "commons_lease_take",
            "Take a lease on a resource (a path, branch, port or name) before using it. Refused if another actor holds it or anything inside it. A stale lease (its holder stopped renewing) still blocks: confirm the holder finished, then pass force_reason.",
            schema(
                json!({"resource": text, "ttl_seconds": {"type": "number"}, "note": text, "force_reason": text}),
                &["resource"],
            ),
        ),
        tool(
            "commons_lease_renew",
            "Renew your lease (heartbeat).",
            schema(
                json!({"resource": text, "ttl_seconds": {"type": "number"}}),
                &["resource"],
            ),
        ),
        tool(
            "commons_lease_release",
            "Release your lease when you no longer need the resource.",
            schema(json!({"resource": text}), &["resource"]),
        ),
        tool(
            "commons_lease_check",
            "Unreleased leases, active or stale, that cover a resource or lie inside it. Never change or delete a leased resource; a stale lease means ask its holder, not that it is free.",
            schema(json!({"resource": text}), &["resource"]),
        ),
        tool(
            "commons_status",
            "Post your lifecycle state: working (with step), blocked (with question), idle, or done (only after releasing leases).",
            schema(
                json!({"state": {"type": "string", "enum": AGENT_STATES}, "step": text, "question": text}),
                &["state"],
            ),
        ),
        tool(
            "commons_agents",
            "Every agent's latest state and read-through message number.",
            schema(json!({}), &[]),
        ),
        tool(
            "commons_send",
            "Send a numbered message to an agent by name or hash ID, or '*' for everyone.",
            schema(json!({"to": text, "body": text}), &["to", "body"]),
        ),
        tool(
            "commons_inbox",
            "Your unread messages. Report 'read through N' when you report.",
            schema(json!({"include_read": {"type": "boolean"}}), &[]),
        ),
        tool(
            "commons_ack",
            "Mark your messages read through a message number.",
            schema(json!({"through": {"type": "integer"}}), &["through"]),
        ),
        tool(
            "commons_record_add",
            "Write when you learn: add a hypothesis, claim, evidence, refutation, counterexample, result, gap, lesson, decision, question, answer, risk or note. Cite evidence in refs.",
            schema(
                json!({"type": {"type": "string", "enum": RECORD_TYPES}, "title": text, "body": text, "status": {"type": "string", "enum": RECORD_STATUSES}, "tags": names, "refs": refs}),
                &["type", "title"],
            ),
        ),
        tool(
            "commons_record_update",
            "Change a record's status (for example supported or refuted) with a note and evidence references, or correct its title (the old title is kept in a dated note).",
            schema(
                json!({"id": text, "status": {"type": "string", "enum": RECORD_STATUSES}, "note": text, "refs": refs, "title": text, "severity": {"type": "string", "enum": ["blocker", "high", "medium", "low"]}, "tags": {"type": "array", "items": text}}),
                &["id"],
            ),
        ),
        tool(
            "commons_gaps",
            "Feature gaps ranked by severity (blocker first), then by independent reporters, then age.",
            schema(
                json!({"include_closed": {"type": "boolean"}, "limit": {"type": "integer"}}),
                &[],
            ),
        ),
        tool(
            "commons_dedup",
            "Proposed duplicate pairs among records of one type, by word overlap. Confirm each with commons_link <duplicate> duplicates <canonical>, which folds the duplicate into the canonical record.",
            schema(json!({"type": text, "threshold": {"type": "number"}}), &[]),
        ),
        tool(
            "commons_link",
            &format!(
                "Relate two records: {}. 'A duplicates B' closes A and folds its references, tags, reporters and higher severity into B.",
                LINK_RELATIONS.join(", ")
            ),
            schema(
                json!({"src": text, "rel": {"type": "string", "enum": LINK_RELATIONS}, "dst": text}),
                &["src", "rel", "dst"],
            ),
        ),
        tool(
            "commons_record",
            "One record with its links.",
            schema(json!({"id": text}), &["id"]),
        ),
        tool(
            "commons_records",
            "Search records by type, status or tag, newest first.",
            schema(
                json!({"type": text, "status": text, "tag": text, "limit": {"type": "integer"}}),
                &[],
            ),
        ),
        tool(
            "commons_emit",
            "Report a factory event (part, event, subject, body).",
            schema(
                json!({"part": text, "event": text, "subject": text, "body": {"type": "object"}}),
                &["part", "event", "subject"],
            ),
        ),
    ]
}

fn tool(name: &str, description: &str, input_schema: Value) -> Value {
    json!({"name": name, "description": description, "inputSchema": input_schema})
}

fn schema(properties: Value, required: &[&str]) -> Value {
    json!({"type": "object", "additionalProperties": false, "properties": properties, "required": required})
}

fn tool_error(message: &str) -> Value {
    json!({"content": [{"type": "text", "text": truncate(message)}], "isError": true})
}

fn pretty(value: &Value) -> String {
    serde_json::to_string_pretty(value).unwrap_or_else(|_| "null".to_string())
}

fn required_string<'a>(
    object: &'a serde_json::Map<String, Value>,
    key: &str,
) -> Result<&'a str, CommonsError> {
    object
        .get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| CommonsError::new(format!("'{key}'")))
}

fn optional_string<'a>(
    object: &'a serde_json::Map<String, Value>,
    key: &str,
    default: &'a str,
) -> Result<&'a str, CommonsError> {
    object.get(key).map_or(Ok(default), |value| {
        value
            .as_str()
            .ok_or_else(|| CommonsError::new("expected string"))
    })
}

fn number(
    object: &serde_json::Map<String, Value>,
    key: &str,
    default: f64,
) -> Result<f64, CommonsError> {
    object.get(key).map_or(Ok(default), |value| {
        value
            .as_f64()
            .ok_or_else(|| CommonsError::new("expected number"))
    })
}

fn integer(object: &serde_json::Map<String, Value>, key: &str) -> Result<i64, CommonsError> {
    object
        .get(key)
        .and_then(Value::as_i64)
        .ok_or_else(|| CommonsError::new(key.to_string()))
}

fn integer_or(
    object: &serde_json::Map<String, Value>,
    key: &str,
    default: i64,
) -> Result<i64, CommonsError> {
    object.get(key).map_or(Ok(default), |value| {
        value
            .as_i64()
            .ok_or_else(|| CommonsError::new("expected integer"))
    })
}

fn tag_values(value: Option<&Value>) -> Result<Vec<String>, CommonsError> {
    match value {
        None | Some(Value::Null) => Ok(Vec::new()),
        Some(Value::Array(values)) if values.len() <= 32 => {
            Ok(values.iter().map(python_string).collect())
        }
        Some(Value::Array(_)) => Err(CommonsError::new("tags must be a list of at most 32 names")),
        Some(_) => Err(CommonsError::new("tags must be a list of at most 32 names")),
    }
}

fn refs_values(value: Option<&Value>) -> Result<Vec<String>, CommonsError> {
    match value {
        None | Some(Value::Null) => Ok(Vec::new()),
        Some(Value::Array(values)) if values.len() <= 64 => values
            .iter()
            .map(|value| {
                value
                    .as_str()
                    .map(str::to_string)
                    .ok_or_else(|| CommonsError::new("refs must be a list of at most 64 references (paths, commits, record IDs, hashes)"))
            })
            .collect(),
        Some(_) => Err(CommonsError::new("refs must be a list of at most 64 references (paths, commits, record IDs, hashes)")),
    }
}

fn python_string(value: &Value) -> String {
    match value {
        Value::Null => "None".to_string(),
        Value::Bool(true) => "True".to_string(),
        Value::Bool(false) => "False".to_string(),
        Value::String(value) => value.clone(),
        Value::Number(value) => value.to_string(),
        Value::Array(values) => format!(
            "[{}]",
            values
                .iter()
                .map(python_string)
                .collect::<Vec<_>>()
                .join(", ")
        ),
        Value::Object(_) => serde_json::to_string(value).unwrap_or_default(),
    }
}

fn falsey(value: &Value) -> bool {
    match value {
        Value::Null => true,
        Value::Bool(value) => !value,
        Value::Number(value) => value.as_f64() == Some(0.0),
        Value::String(value) => value.is_empty(),
        Value::Array(value) => value.is_empty(),
        Value::Object(value) => value.is_empty(),
    }
}

fn is_valid_error_id(value: &Value) -> bool {
    value.is_string() || value.is_number() || value.is_boolean()
}

fn truncate(value: &str) -> String {
    value.chars().take(2_000).collect()
}
