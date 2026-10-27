use std::collections::{BTreeSet, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use rusqlite::{OptionalExtension, params};
use serde_json::{Value, json};

use crate::{
    ADVISE_TYPES, AGENT_STATES, ARCHIVE_REFUSAL, ApplyResult, BUDGET_BYTES, Commons, CommonsError,
    LINK_RELATIONS, MAX_LIVE_EVENTS, MAX_TAGS, RECORD_STATUSES, RECORD_TYPES, canonical_json,
    is_falsey, iso, validate_name, validate_text,
};

const SEVERITIES: &[&str] = &["blocker", "high", "medium", "low"];
const STOPWORDS: &[&str] = &[
    "the", "and", "for", "with", "that", "this", "from", "into", "are", "was", "were", "has",
    "have", "not", "but", "its", "can", "each", "one", "per", "when", "what", "which", "while",
    "who", "will", "would", "than", "then", "them", "they", "their", "there", "these", "those",
    "also", "only", "over", "under", "about", "more", "most", "such", "some", "any", "all", "out",
    "use", "used", "using", "via", "does", "did", "may", "might", "must", "should", "could",
    "need", "needs",
];

impl Commons {
    /// Register a name with a store-local hash identity.
    pub fn register(&mut self, name: &str, kind: &str, role: &str) -> Result<Value, CommonsError> {
        let name = validate_name(
            if name.is_empty() { &self.actor } else { name },
            "agent name",
        )?;
        let kind = validate_name(kind, "kind")?;
        let role = validate_text(role, "role", true)?
            .chars()
            .take(200)
            .collect::<String>();
        let nonce: String =
            self.connection
                .query_row("SELECT lower(hex(randomblob(16)))", [], |row| row.get(0))?;
        let actor = self.actor.clone();
        let body = json!({"kind": kind, "role": role, "nonce": nonce});
        self.write_event("identity.register", &name, &body, |transaction, seq, at| {
            if let Some(existing) = transaction
                .query_row("SELECT id FROM identities WHERE name=?1", [&name], |row| {
                    row.get::<_, String>(0)
                })
                .optional()?
            {
                return Err(CommonsError::new(format!(
                    "{name} is already registered as {existing}"
                )));
            }
            let material = canonical_json(&json!([name, kind, role, at, nonce]))?;
            let hash = sha256(&material);
            let id = format!("ag-{}", &hash[..16]);
            transaction.execute(
                "INSERT INTO identities VALUES(?1,?2,?3,?4,?5,?6,?7)",
                params![id, name, kind, role, actor, seq, at],
            )?;
            Ok(ApplyResult::Commit(json!({
                "seq": seq,
                "id": id,
                "name": name,
                "kind": kind,
                "role": role
            })))
        })
    }

    /// Look up the current actor or an identity by its registered name or hash ID.
    pub fn identity(&self, name_or_id: &str) -> Result<Value, CommonsError> {
        let key = if name_or_id.is_empty() {
            self.actor.as_str()
        } else {
            name_or_id
        };
        let row = self
            .connection
            .query_row(
                "SELECT id, name, kind, role, registered_by, registered_at FROM identities WHERE name=?1 OR id=?1",
                [key],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, String>(4)?,
                        row.get::<_, f64>(5)?,
                    ))
                },
            )
            .optional()?;
        let Some((id, name, kind, role, registered_by, registered_at)) = row else {
            return Err(CommonsError::new(format!(
                "no registered identity {key}; register first"
            )));
        };
        Ok(json!({
            "id": id,
            "name": name,
            "kind": kind,
            "role": role,
            "registered_by": registered_by,
            "registered": iso(registered_at)
        }))
    }

    /// Take an exclusive lease over a resource for `ttl_seconds`.
    pub fn lease_take(
        &mut self,
        resource: &str,
        ttl_seconds: f64,
        note: &str,
    ) -> Result<Value, CommonsError> {
        self.lease_take_force(resource, ttl_seconds, note, "")
    }

    /// Take a lease, recording a reason when displacing another actor's stale lease.
    pub fn lease_take_force(
        &mut self,
        resource: &str,
        ttl_seconds: f64,
        note: &str,
        force_reason: &str,
    ) -> Result<Value, CommonsError> {
        let resource = normalize_resource(resource)?;
        if !(60.0..=604_800.0).contains(&ttl_seconds) {
            return Err(CommonsError::new(
                "ttl_seconds must be between 60 and 604800",
            ));
        }
        let note = validate_text(note, "note", true)?;
        let force_reason = validate_text(force_reason, "force reason", true)?;
        let mut body = json!({"ttl_seconds": ttl_seconds, "note": note});
        if !force_reason.trim().is_empty() {
            body["force_reason"] = json!(force_reason);
        }
        let actor = self.actor.clone();
        self.write_event("lease.take", &resource, &body, |transaction, seq, at| {
            require_registered(transaction, &actor)?;
            let mut displaced = Vec::new();
            for (held, owner, _, expires) in unreleased_leases(transaction)? {
                if owner == actor || !covers(&held, &resource) {
                    continue;
                }
                if expires > at {
                    return Err(CommonsError::new(format!(
                        "{resource} overlaps {held}, leased by {owner} until {}",
                        iso(expires)
                    )));
                }
                if force_reason.trim().is_empty() {
                    return Err(CommonsError::new(format!(
                        "{resource} overlaps {held}: {owner}'s lease went stale at {} but was never released. Confirm {owner} has finished, then take it with a force reason",
                        iso(expires)
                    )));
                }
                displaced.push(json!([held, owner]));
            }
            for pair in &displaced {
                transaction.execute("DELETE FROM leases WHERE resource=?1", [pair[0].as_str().unwrap_or_default()])?;
            }
            transaction.execute("DELETE FROM leases WHERE resource=?1", [&resource])?;
            let expires = at + ttl_seconds;
            transaction.execute(
                "INSERT INTO leases VALUES(?1,?2,?3,?4,?5,?6)",
                params![resource, actor, note, seq, at, expires],
            )?;
            Ok(ApplyResult::Commit(json!({
                "seq": seq,
                "resource": resource,
                "owner": actor,
                "expires": iso(expires),
                "displaced": displaced
            })))
        })
    }

    /// Renew an unreleased lease owned by this actor, including a stale lease.
    pub fn lease_renew(&mut self, resource: &str, ttl_seconds: f64) -> Result<Value, CommonsError> {
        let resource = normalize_resource(resource)?;
        let body = json!({"ttl_seconds": ttl_seconds});
        let actor = self.actor.clone();
        self.write_event("lease.renew", &resource, &body, |transaction, seq, at| {
            require_registered(transaction, &actor)?;
            let row = transaction
                .query_row(
                    "SELECT owner, expires_at FROM leases WHERE resource=?1",
                    [&resource],
                    |row| Ok((row.get::<_, String>(0)?, row.get::<_, f64>(1)?)),
                )
                .optional()?;
            let Some((owner, _expires_at)) = row else {
                return Err(CommonsError::new(format!(
                    "no lease on {resource}; take it"
                )));
            };
            if owner != actor {
                return Err(CommonsError::new(format!(
                    "{resource} is leased by {owner}"
                )));
            }
            let expires = at + ttl_seconds;
            transaction.execute(
                "UPDATE leases SET heartbeat_at=?1, expires_at=?2 WHERE resource=?3",
                params![at, expires, resource],
            )?;
            Ok(ApplyResult::Commit(json!({
                "seq": seq,
                "resource": resource,
                "expires": iso(expires)
            })))
        })
    }

    /// Release a lease, requiring an owner or a nonblank force reason.
    pub fn lease_release(
        &mut self,
        resource: &str,
        force_reason: &str,
    ) -> Result<Value, CommonsError> {
        let resource = normalize_resource(resource)?;
        let body = json!({"force_reason": force_reason});
        let actor = self.actor.clone();
        self.write_event("lease.release", &resource, &body, |transaction, seq, _| {
            require_registered(transaction, &actor)?;
            let owner = transaction
                .query_row(
                    "SELECT owner FROM leases WHERE resource=?1",
                    [&resource],
                    |row| row.get::<_, String>(0),
                )
                .optional()?;
            let Some(owner) = owner else {
                return Err(CommonsError::new(format!("no lease on {resource}")));
            };
            if owner != actor && force_reason.trim().is_empty() {
                return Err(CommonsError::new(format!(
                    "{resource} is leased by {owner}; only its owner releases it, or give a force reason"
                )));
            }
            transaction.execute("DELETE FROM leases WHERE resource=?1", [&resource])?;
            Ok(ApplyResult::Commit(json!({
                "seq": seq,
                "resource": resource,
                "released_from": owner
            })))
        })
    }

    /// Return unreleased leases, active or stale, that overlap the resource.
    pub fn lease_check(&self, resource: &str) -> Result<Vec<Value>, CommonsError> {
        let resource = normalize_resource(resource)?;
        let at = (self.clock)();
        let rows = unreleased_leases(&self.connection)?;
        Ok(rows
            .into_iter()
            .filter(|(held, _, _, _)| covers(held, &resource))
            .map(|(resource, owner, note, expires)| lease_view(resource, owner, note, expires, at))
            .collect())
    }

    /// Return all unreleased leases.
    pub fn leases(&self) -> Result<Vec<Value>, CommonsError> {
        let at = (self.clock)();
        Ok(unreleased_leases(&self.connection)?
            .into_iter()
            .map(|(resource, owner, note, expires)| lease_view(resource, owner, note, expires, at))
            .collect())
    }

    /// Post this actor's lifecycle status.
    pub fn status(
        &mut self,
        state: &str,
        step: &str,
        question: &str,
    ) -> Result<Value, CommonsError> {
        if !AGENT_STATES.contains(&state) {
            return Err(CommonsError::new(format!(
                "state must be one of {}",
                AGENT_STATES.join(", ")
            )));
        }
        let step = validate_text(step, "step", true)?;
        let question = validate_text(question, "question", true)?;
        if state == "blocked" && question.trim().is_empty() {
            return Err(CommonsError::new(
                "a blocked status needs the question that blocks it",
            ));
        }
        let body = json!({"state": state, "step": step, "question": question});
        let actor = self.actor.clone();
        self.write_event("agent.status", &actor, &body, |transaction, seq, at| {
            require_registered(transaction, &actor)?;
            if state == "done" {
                let held = unreleased_leases(transaction)?
                    .into_iter()
                    .filter(|(_, owner, _, _)| owner == &actor)
                    .map(|(resource, _, _, _)| resource)
                    .collect::<Vec<_>>();
                if !held.is_empty() {
                    return Err(CommonsError::new(format!(
                        "release your leases before posting done: {}",
                        held.join(", ")
                    )));
                }
            }
            transaction.execute(
                "INSERT INTO agents(name, state, step, question, updated_at) VALUES(?1,?2,?3,?4,?5) \
                 ON CONFLICT(name) DO UPDATE SET state=excluded.state, step=excluded.step, \
                 question=excluded.question, updated_at=excluded.updated_at",
                params![actor, state, step, question, at],
            )?;
            Ok(ApplyResult::Commit(json!({
                "seq": seq,
                "agent": actor,
                "state": state
            })))
        })
    }

    /// Return every agent's latest status ordered by name.
    pub fn agents(&self) -> Result<Vec<Value>, CommonsError> {
        let mut statement = self.connection.prepare(
            "SELECT i.name, i.id, i.kind, i.role, a.state, a.step, a.question, a.updated_at, a.read_through \
             FROM identities i LEFT JOIN agents a ON a.name=i.name ORDER BY i.name",
        )?;
        let rows = statement.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, Option<String>>(4)?,
                row.get::<_, Option<String>>(5)?,
                row.get::<_, Option<String>>(6)?,
                row.get::<_, Option<f64>>(7)?,
                row.get::<_, Option<i64>>(8)?,
            ))
        })?;
        rows.map(|row| {
            let (agent, id, kind, role, state, step, question, updated, read_through) = row?;
            Ok(json!({
                "agent": agent,
                "id": id,
                "kind": kind,
                "role": role,
                "state": state.unwrap_or_else(|| "registered".to_string()),
                "step": step.unwrap_or_default(),
                "question": question.unwrap_or_default(),
                "updated": updated.map(iso).unwrap_or_default(),
                "read_through": read_through.unwrap_or_default()
            }))
        })
        .collect()
    }

    /// Send a numbered message to an actor or to `*` for everyone.
    pub fn send(&mut self, recipient: &str, body: &str) -> Result<Value, CommonsError> {
        let recipient = if recipient == "*" {
            recipient.to_string()
        } else {
            validate_name(recipient, "recipient")?
        };
        let body = validate_text(body, "message", false)?;
        let event_body = json!({"body": body});
        let actor = self.actor.clone();
        self.write_event(
            "message.send",
            &recipient,
            &event_body,
            |transaction, seq, at| {
                require_registered(transaction, &actor)?;
                let to = if recipient == "*" {
                    recipient.clone()
                } else {
                    resolve_agent(transaction, &recipient)?
                };
                transaction.execute(
                    "INSERT INTO messages VALUES(?1,?2,?3,?4,?5)",
                    params![seq, actor, to, body, at],
                )?;
                Ok(ApplyResult::Commit(json!({"seq": seq, "to": to})))
            },
        )
    }

    /// Return this actor's unread messages or all matching messages when requested.
    pub fn inbox(&self, include_read: bool) -> Result<Value, CommonsError> {
        let through = read_through(&self.connection, &self.actor)?;
        let floor = if include_read { 0 } else { through };
        let mut statement = self.connection.prepare(
            "SELECT seq, sender, recipient, body, at FROM messages WHERE seq > ?1 AND (recipient=?2 OR recipient='*') \
             AND sender != ?2 ORDER BY seq",
        )?;
        let rows = statement.query_map(params![floor, self.actor], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, f64>(4)?,
            ))
        })?;
        let messages = rows
            .map(|row| {
                let (seq, sender, recipient, body, at) = row?;
                Ok(json!({
                    "seq": seq,
                    "from": sender,
                    "to": recipient,
                    "body": body,
                    "at": iso(at)
                }))
            })
            .collect::<Result<Vec<_>, rusqlite::Error>>()?;
        Ok(json!({"read_through": through, "messages": messages}))
    }

    /// Advance this actor's read receipt through a message number.
    pub fn ack(&mut self, through: i64) -> Result<Value, CommonsError> {
        if through < 0 {
            return Err(CommonsError::new(
                "through must be a non-negative message number",
            ));
        }
        let body = json!({"through": through});
        let actor = self.actor.clone();
        self.write_event("message.ack", &actor, &body, |transaction, seq, at| {
            require_registered(transaction, &actor)?;
            let top: i64 =
                transaction.query_row("SELECT COALESCE(MAX(seq), 0) FROM events", [], |row| {
                    row.get(0)
                })?;
            if through > top {
                return Err(CommonsError::new(format!(
                    "message {through} does not exist yet"
                )));
            }
            let current = read_through(transaction, &actor)?;
            let new = current.max(through);
            transaction.execute(
                "INSERT INTO agents(name, state, step, question, updated_at, read_through) \
                 VALUES(?1, 'working', '', '', ?2, ?3) \
                 ON CONFLICT(name) DO UPDATE SET read_through=?3",
                params![actor, at, new],
            )?;
            Ok(ApplyResult::Commit(
                json!({"seq": seq, "read_through": new}),
            ))
        })
    }

    /// Add a knowledge record, returning an idempotent result for an existing ID.
    #[allow(clippy::too_many_arguments)]
    pub fn record_add(
        &mut self,
        record_type: &str,
        title: &str,
        body: &str,
        status: &str,
        tags: &[String],
        refs: &[String],
        record_id: &str,
    ) -> Result<Value, CommonsError> {
        if !RECORD_TYPES.contains(&record_type) {
            return Err(CommonsError::new(format!(
                "type must be one of {}",
                RECORD_TYPES.join(", ")
            )));
        }
        if !RECORD_STATUSES.contains(&status) {
            return Err(CommonsError::new(format!(
                "status must be one of {}",
                RECORD_STATUSES.join(", ")
            )));
        }
        let title = validate_text(title, "title", false)?;
        let body = validate_text(body, "body", true)?;
        let tags = normalize_tags(tags)?;
        validate_refs(refs)?;
        let content = canonical_json(&json!([record_type, title, body, tags, refs]))?;
        let record_id = if record_id.is_empty() {
            format!(
                "{}-{}",
                record_type.chars().take(3).collect::<String>(),
                &sha256(&content)[..12]
            )
        } else {
            record_id.to_string()
        };
        let record_id = validate_name(&record_id, "record id")?;
        let event_body = json!({
            "type": record_type,
            "status": status,
            "title": title,
            "body": body,
            "tags": tags,
            "refs": refs
        });
        let actor = self.actor.clone();
        self.write_event(
            "record.add",
            &record_id,
            &event_body,
            |transaction, seq, _| {
                require_registered(transaction, &actor)?;
                if transaction
                    .query_row("SELECT id FROM records WHERE id=?1", [&record_id], |row| {
                        row.get::<_, String>(0)
                    })
                    .optional()?
                    .is_some()
                {
                    return Ok(ApplyResult::Rollback(json!({
                        "seq": Value::Null,
                        "id": record_id,
                        "existing": true
                    })));
                }
                transaction.execute(
                    "INSERT INTO records VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?9)",
                    params![
                        record_id,
                        record_type,
                        status,
                        title,
                        body,
                        canonical_json(&json!(tags))?,
                        canonical_json(&json!(refs))?,
                        actor,
                        seq
                    ],
                )?;
                Ok(ApplyResult::Commit(json!({
                    "seq": seq,
                    "id": record_id,
                    "existing": false
                })))
            },
        )
    }

    /// Update a record's status, append a note, and add references.
    pub fn record_update(
        &mut self,
        record_id: &str,
        status: &str,
        note: &str,
        refs: &[String],
    ) -> Result<Value, CommonsError> {
        self.record_update_title(record_id, status, note, refs, "")
    }

    /// Update a record and optionally correct its title while preserving the previous title in its note history.
    pub fn record_update_title(
        &mut self,
        record_id: &str,
        status: &str,
        note: &str,
        refs: &[String],
        title: &str,
    ) -> Result<Value, CommonsError> {
        self.record_update_full(record_id, status, note, refs, title, "", &[])
    }

    /// Update a record's status, note, references, title, severity, and additional tags.
    #[allow(clippy::too_many_arguments)]
    pub fn record_update_full(
        &mut self,
        record_id: &str,
        status: &str,
        note: &str,
        refs: &[String],
        title: &str,
        severity: &str,
        tags: &[String],
    ) -> Result<Value, CommonsError> {
        if !status.is_empty() && !RECORD_STATUSES.contains(&status) {
            return Err(CommonsError::new(format!(
                "status must be one of {}",
                RECORD_STATUSES.join(", ")
            )));
        }
        let note = validate_text(note, "note", true)?;
        let title = validate_text(title, "title", true)?.trim().to_string();
        if !severity.is_empty() && !SEVERITIES.contains(&severity) {
            return Err(CommonsError::new(format!(
                "severity must be one of {}",
                SEVERITIES.join(", ")
            )));
        }
        let added_tags = normalize_tags(tags)?;
        validate_refs(refs)?;
        let mut event_body = json!({"status": status, "note": note, "refs": refs});
        if !title.is_empty() {
            event_body["title"] = json!(title);
        }
        if !severity.is_empty() {
            event_body["severity"] = json!(severity);
        }
        if !added_tags.is_empty() {
            event_body["tags"] = json!(added_tags);
        }
        let actor = self.actor.clone();
        self.write_event(
            "record.update",
            record_id,
            &event_body,
            |transaction, seq, at| {
                require_registered(transaction, &actor)?;
                let row = transaction
                    .query_row(
                        "SELECT status, refs, body, title, tags FROM records WHERE id=?1",
                        [record_id],
                        |row| {
                            Ok((
                                row.get::<_, String>(0)?,
                                row.get::<_, String>(1)?,
                                row.get::<_, String>(2)?,
                                row.get::<_, String>(3)?,
                                row.get::<_, String>(4)?,
                            ))
                        },
                    )
                    .optional()?;
                let Some((old_status, old_refs, old_body, old_title, old_tags)) = row else {
                    return Err(CommonsError::new(format!("no record {record_id}")));
                };
                let new_status = if status.is_empty() {
                    old_status
                } else {
                    status.to_string()
                };
                let existing_refs: Vec<String> = serde_json::from_str(&old_refs)
                    .map_err(|error| CommonsError::new(error.to_string()))?;
                let new_refs = existing_refs
                    .into_iter()
                    .chain(refs.iter().cloned())
                    .collect::<BTreeSet<_>>()
                    .into_iter()
                    .collect::<Vec<_>>();
                if new_refs.len() > 64 {
                    return Err(CommonsError::new(format!(
                        "{record_id} would have more than 64 references; add a new record and link it"
                    )));
                }
                let mut new_tags: BTreeSet<String> = serde_json::from_str::<Vec<String>>(&old_tags)
                    .map_err(|error| CommonsError::new(error.to_string()))?
                    .into_iter()
                    .chain(added_tags.iter().cloned())
                    .collect();
                if !severity.is_empty() {
                    new_tags.retain(|tag| !tag.starts_with("severity-"));
                    new_tags.insert(format!("severity-{severity}"));
                }
                if new_tags.len() > MAX_TAGS {
                    return Err(CommonsError::new(format!(
                        "{record_id} would have more than {MAX_TAGS} tags"
                    )));
                }
                let new_title = if title.is_empty() {
                    old_title.clone()
                } else {
                    title.clone()
                };
                let lines = [
                    (!title.is_empty() && title != old_title)
                        .then(|| format!("title was: {old_title}")),
                    (!note.trim().is_empty()).then(|| note.clone()),
                ];
                let mut new_body = old_body;
                for line in lines.into_iter().flatten() {
                    new_body.push_str(&format!("\n[{} {}] {line}", iso(at), actor));
                }
                if new_body.chars().count() > crate::MAX_TEXT {
                    return Err(CommonsError::new(format!(
                        "{record_id} is full ({} characters); add a new record and link it",
                        crate::MAX_TEXT
                    )));
                }
                transaction.execute(
                    "UPDATE records SET status=?1, refs=?2, body=?3, title=?4, tags=?5, updated_seq=?6 WHERE id=?7",
                    params![
                        new_status,
                        canonical_json(&json!(new_refs))?,
                        new_body,
                        new_title,
                        canonical_json(&json!(new_tags))?,
                        seq,
                        record_id
                    ],
                )?;
                Ok(ApplyResult::Commit(json!({
                    "seq": seq,
                    "id": record_id,
                    "status": new_status,
                    "title": new_title
                })))
            },
        )
    }

    /// Add a typed relation between two existing records.
    pub fn link(&mut self, src: &str, relation: &str, dst: &str) -> Result<Value, CommonsError> {
        if !LINK_RELATIONS.contains(&relation) {
            return Err(CommonsError::new(format!(
                "relation must be one of {}",
                LINK_RELATIONS.join(", ")
            )));
        }
        let body = json!({"rel": relation, "dst": dst});
        let actor = self.actor.clone();
        self.write_event("link.add", src, &body, |transaction, seq, at| {
            require_registered(transaction, &actor)?;
            for id in [src, dst] {
                if transaction
                    .query_row("SELECT 1 FROM records WHERE id=?1", [id], |_| Ok(()))
                    .optional()?
                    .is_none()
                {
                    return Err(CommonsError::new(format!("no record {id}")));
                }
            }
            if relation == "duplicates"
                && transaction
                    .query_row(
                        "SELECT 1 FROM links WHERE src=?1 AND rel='duplicates' AND dst=?2",
                        params![src, dst],
                        |_| Ok(()),
                    )
                    .optional()?
                    .is_none()
            {
                fold_duplicate(transaction, &actor, src, dst, seq, at)?;
            }
            transaction.execute(
                "INSERT OR IGNORE INTO links VALUES(?1,?2,?3,?4)",
                params![src, relation, dst, seq],
            )?;
            if relation == "supersedes" {
                transaction.execute(
                    "UPDATE records SET status='superseded', updated_seq=?1 WHERE id=?2",
                    params![seq, dst],
                )?;
            }
            Ok(ApplyResult::Commit(
                json!({"seq": seq, "link": [src, relation, dst]}),
            ))
        })
    }

    /// Rank open gaps by severity, independent reporters, then age.
    pub fn gaps(&self, include_closed: bool, limit: i64) -> Result<Vec<Value>, CommonsError> {
        let mut statement = self
            .connection
            .prepare("SELECT * FROM records WHERE type='gap'")?;
        let records = statement
            .query_map([], record_row)?
            .collect::<Result<Vec<_>, _>>()?;
        let mut ranked = Vec::new();
        for record in records {
            let status = record["status"].as_str().unwrap_or_default();
            if status == "superseded"
                || (!include_closed && !matches!(status, "open" | "planned" | "testing"))
            {
                continue;
            }
            let tags = value_strings(&record["tags"]);
            let severity = tags
                .iter()
                .filter_map(|tag| tag.strip_prefix("severity-"))
                .filter(|level| SEVERITIES.contains(level))
                .min_by_key(|level| SEVERITIES.iter().position(|item| item == level))
                .unwrap_or("unrated")
                .to_string();
            let severity_rank = SEVERITIES
                .iter()
                .position(|level| *level == severity.as_str())
                .unwrap_or(SEVERITIES.len());
            let id = record["id"].as_str().unwrap_or_default();
            let reporters = duplicate_reporters(&self.connection, id)?;
            let tags = tags
                .into_iter()
                .filter(|tag| !tag.starts_with("severity-"))
                .collect::<Vec<_>>();
            ranked.push((
                severity_rank,
                std::cmp::Reverse(reporters.len()),
                record["created_seq"].as_i64().unwrap_or_default(),
                json!({
                    "id": id,
                    "severity": severity,
                    "reporters": reporters,
                    "status": status,
                    "title": record["title"],
                    "tags": tags
                }),
            ));
        }
        ranked.sort_by(|a, b| (&a.0, &a.1, &a.2).cmp(&(&b.0, &b.1, &b.2)));
        Ok(ranked
            .into_iter()
            .take(limit.clamp(1, 1_000) as usize)
            .map(|item| item.3)
            .collect())
    }

    /// Propose duplicate pairs among records with overlapping words.
    pub fn similar(
        &self,
        record_type: &str,
        threshold: f64,
        limit: i64,
    ) -> Result<Vec<Value>, CommonsError> {
        if !(threshold > 0.0 && threshold <= 1.0) {
            return Err(CommonsError::new("threshold must be in (0, 1]"));
        }
        let mut statement = self.connection.prepare(
            "SELECT * FROM records WHERE type=?1 AND status NOT IN ('superseded','rejected') ORDER BY created_seq",
        )?;
        let records = statement
            .query_map([record_type], record_row)?
            .collect::<Result<Vec<_>, _>>()?;
        let body_words = records
            .iter()
            .map(|record| {
                (
                    record["id"].as_str().unwrap_or_default().to_string(),
                    dedup_words(&format!(
                        "{} {}",
                        record["title"].as_str().unwrap_or_default(),
                        record["body"].as_str().unwrap_or_default()
                    )),
                )
            })
            .collect::<Vec<_>>();
        let titles = records
            .iter()
            .map(|record| {
                (
                    record["id"].as_str().unwrap_or_default().to_string(),
                    dedup_words(record["title"].as_str().unwrap_or_default()),
                )
            })
            .collect::<Vec<_>>();
        let mut pairs = Vec::new();
        for left in 0..records.len() {
            for right in (left + 1)..records.len() {
                let left_words = &body_words[left].1;
                let right_words = &body_words[right].1;
                if left_words.is_empty() || right_words.is_empty() {
                    continue;
                }
                let body_score = jaccard(left_words, right_words);
                let left_title = &titles[left].1;
                let right_title = &titles[right].1;
                let title_score = if left_title.is_empty() || right_title.is_empty() {
                    0.0
                } else {
                    jaccard(left_title, right_title)
                };
                let score = (body_score.max(title_score) * 1_000.0).round_ties_even() / 1_000.0;
                if score < threshold {
                    continue;
                }
                let shared = left_words
                    .intersection(right_words)
                    .take(12)
                    .cloned()
                    .collect::<Vec<_>>();
                pairs.push((
                    score,
                    records[left]["id"].as_str().unwrap_or_default().to_string(),
                    records[right]["id"]
                        .as_str()
                        .unwrap_or_default()
                        .to_string(),
                    json!({
                        "score": score,
                        "older": records[left]["id"],
                        "newer": records[right]["id"],
                        "older_title": records[left]["title"],
                        "newer_title": records[right]["title"],
                        "shared": shared
                    }),
                ));
            }
        }
        pairs.sort_by(|a, b| {
            b.0.total_cmp(&a.0)
                .then_with(|| a.1.cmp(&b.1))
                .then_with(|| a.2.cmp(&b.2))
        });
        Ok(pairs
            .into_iter()
            .take(limit.clamp(1, 1_000) as usize)
            .map(|item| item.3)
            .collect())
    }

    /// Return one record together with its links.
    pub fn record(&self, record_id: &str) -> Result<Value, CommonsError> {
        let row = self
            .connection
            .query_row("SELECT * FROM records WHERE id=?1", [record_id], record_row)
            .optional()?;
        let Some(mut record) = row else {
            return Err(CommonsError::new(format!("no record {record_id}")));
        };
        let mut statement = self
            .connection
            .prepare("SELECT src, rel, dst FROM links WHERE src=?1 OR dst=?1 ORDER BY seq")?;
        let rows = statement.query_map([record_id], |row| {
            Ok(json!({
                "src": row.get::<_, String>(0)?,
                "rel": row.get::<_, String>(1)?,
                "dst": row.get::<_, String>(2)?
            }))
        })?;
        let links = rows.collect::<Result<Vec<_>, _>>()?;
        record
            .as_object_mut()
            .ok_or_else(|| CommonsError::new("record row was not an object"))?
            .insert("links".to_string(), json!(links));
        Ok(record)
    }

    /// Return records ordered by newest update, with optional type, status, and tag filters.
    pub fn records(
        &self,
        record_type: &str,
        status: &str,
        tag: &str,
        limit: i64,
    ) -> Result<Vec<Value>, CommonsError> {
        let mut query = "SELECT * FROM records WHERE 1=1".to_string();
        let mut values = Vec::new();
        if !record_type.is_empty() {
            query.push_str(" AND type=?");
            values.push(record_type.to_string());
        }
        if !status.is_empty() {
            query.push_str(" AND status=?");
            values.push(status.to_string());
        }
        query.push_str(" ORDER BY updated_seq DESC");
        let mut statement = self.connection.prepare(&query)?;
        let rows = statement.query_map(rusqlite::params_from_iter(values.iter()), record_row)?;
        let mut records = rows.collect::<Result<Vec<_>, _>>()?;
        if !tag.is_empty() {
            records.retain(|record| {
                record["tags"].as_array().is_some_and(|tags| {
                    tags.iter()
                        .any(|value| value.as_str() == Some(tag.to_lowercase().as_str()))
                })
            });
        }
        records.truncate(limit.clamp(1, 1_000) as usize);
        Ok(records)
    }

    /// Return canonical JSON event lines; archive segments are included first when requested.
    pub fn export_events(&self, with_archive: bool) -> Result<Vec<String>, CommonsError> {
        if with_archive {
            return Err(CommonsError::new(ARCHIVE_REFUSAL));
        }
        let mut statement = self
            .connection
            .prepare("SELECT seq,at,actor,kind,subject,body,prev,hash FROM events ORDER BY seq")?;
        let rows = statement.query_map([], |row| {
            let body_text: String = row.get(5)?;
            let body: Value = serde_json::from_str(&body_text).map_err(|error| {
                rusqlite::Error::FromSqlConversionFailure(
                    5,
                    rusqlite::types::Type::Text,
                    Box::new(error),
                )
            })?;
            Ok(json!({
                "seq": row.get::<_, i64>(0)?,
                "at": row.get::<_, f64>(1)?,
                "actor": row.get::<_, String>(2)?,
                "kind": row.get::<_, String>(3)?,
                "subject": row.get::<_, String>(4)?,
                "body": body,
                "prev": row.get::<_, String>(6)?,
                "hash": row.get::<_, String>(7)?
            }))
        })?;
        rows.map(|row| canonical_json(&row?))
            .collect::<Result<Vec<_>, _>>()
    }

    /// Export records and links as canonical JSON lines or grouped Markdown.
    pub fn export_records(&self, format: &str) -> Result<Vec<String>, CommonsError> {
        if !matches!(format, "jsonl" | "md") {
            return Err(CommonsError::new("format must be jsonl or md"));
        }
        let mut statement = self
            .connection
            .prepare("SELECT * FROM records ORDER BY type,created_seq")?;
        let records = statement
            .query_map([], record_row)?
            .collect::<Result<Vec<_>, _>>()?;
        let links = {
            let mut statement = self
                .connection
                .prepare("SELECT src,rel,dst FROM links ORDER BY seq")?;
            statement
                .query_map([], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                    ))
                })?
                .collect::<Result<Vec<_>, _>>()?
        };
        if format == "jsonl" {
            let mut lines = records
                .iter()
                .map(canonical_json)
                .collect::<Result<Vec<_>, _>>()?;
            lines.extend(
                links
                    .iter()
                    .map(|(src, rel, dst)| canonical_json(&json!({"link": [src, rel, dst]})))
                    .collect::<Result<Vec<_>, _>>()?,
            );
            return Ok(lines);
        }
        let mut output = vec![format!(
            "# Zeno Commons records\n\n{} records and {} links.\n",
            records.len(),
            links.len()
        )];
        let mut current = String::new();
        for record in &records {
            let record_type = record["type"].as_str().unwrap_or_default();
            if record_type != current {
                current = record_type.to_string();
                output.push(format!("\n## {current}\n"));
            }
            let mut meta = format!(
                "{} · by {}",
                record["status"].as_str().unwrap_or_default(),
                record["author"].as_str().unwrap_or_default()
            );
            let tags = record["tags"].as_array().cloned().unwrap_or_default();
            if !tags.is_empty() {
                meta.push_str(" · tags: ");
                meta.push_str(
                    &tags
                        .iter()
                        .filter_map(Value::as_str)
                        .collect::<Vec<_>>()
                        .join(", "),
                );
            }
            output.push(format!(
                "\n### {}: {}\n\n{meta}\n",
                record["id"].as_str().unwrap_or_default(),
                record["title"].as_str().unwrap_or_default()
            ));
            if let Some(body) = record["body"]
                .as_str()
                .filter(|body| !body.trim().is_empty())
            {
                output.push(format!("\n{}\n", body.trim_end()));
            }
            let refs = record["refs"].as_array().cloned().unwrap_or_default();
            if !refs.is_empty() {
                output.push(format!(
                    "\nReferences: {}\n",
                    refs.iter()
                        .filter_map(Value::as_str)
                        .collect::<Vec<_>>()
                        .join(", ")
                ));
            }
        }
        if !links.is_empty() {
            output.push(format!(
                "\n## Links\n\n{}",
                links
                    .iter()
                    .map(|(src, rel, dst)| format!("- {src} {rel} {dst}\n"))
                    .collect::<String>()
            ));
        }
        Ok(output)
    }

    /// Return negative-memory matches, covering leases, and unread messages for an action.
    pub fn advise(
        &self,
        action: &str,
        tags: &[String],
        text: &str,
        limit: i64,
    ) -> Result<Value, CommonsError> {
        let action = validate_text(action, "action", false)?;
        let wanted_tags = normalize_tags(tags)?.into_iter().collect::<HashSet<_>>();
        let wanted_words = words(&format!("{action} {text}"));
        let mut scored = Vec::new();
        for record in self.records("", "", "", 1_000)? {
            let record_type = record["type"].as_str().unwrap_or_default();
            if !ADVISE_TYPES.contains(&record_type) {
                continue;
            }
            if ["superseded", "stale", "rejected"]
                .contains(&record["status"].as_str().unwrap_or_default())
            {
                continue;
            }
            let record_tags = record["tags"]
                .as_array()
                .map(|values| {
                    values
                        .iter()
                        .filter_map(Value::as_str)
                        .map(str::to_string)
                        .collect::<HashSet<_>>()
                })
                .unwrap_or_default();
            let tag_score = wanted_tags.intersection(&record_tags).count() * 3;
            let title = record["title"].as_str().unwrap_or_default();
            let body = record["body"].as_str().unwrap_or_default();
            let record_words = words(&format!("{title} {body}"));
            let word_score = wanted_words.intersection(&record_words).count();
            let score = tag_score + word_score;
            if score > 0 {
                scored.push((
                    score,
                    record["updated_seq"].as_i64().unwrap_or_default(),
                    record,
                ));
            }
        }
        scored.sort_by(|left, right| right.0.cmp(&left.0).then_with(|| right.1.cmp(&left.1)));
        let advice = scored
            .into_iter()
            .take(limit.clamp(1, 50) as usize)
            .map(|(score, _, mut record)| {
                if let Some(object) = record.as_object_mut() {
                    object.insert("score".to_string(), json!(score));
                }
                record
            })
            .collect::<Vec<_>>();
        let resources = text
            .split_whitespace()
            .filter(|resource| resource.starts_with('/'))
            .collect::<Vec<_>>();
        let mut leases = Vec::new();
        for resource in resources {
            leases.extend(self.lease_check(resource)?);
        }
        let unread = self.inbox(false)?["messages"].clone();
        Ok(json!({
            "action": action,
            "advice": advice,
            "leases": leases,
            "unread": unread
        }))
    }

    /// Summarize record counts, agents, active leases, and unread messages.
    pub fn dashboard(&self) -> Result<Value, CommonsError> {
        let mut counts = serde_json::Map::new();
        let mut statement = self
            .connection
            .prepare("SELECT type, status, COUNT(*) FROM records GROUP BY type, status")?;
        let rows = statement.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, i64>(2)?,
            ))
        })?;
        for row in rows {
            let (record_type, status, count) = row?;
            let type_counts = counts.entry(record_type).or_insert_with(|| json!({}));
            type_counts
                .as_object_mut()
                .ok_or_else(|| CommonsError::new("record count projection was not an object"))?
                .insert(status, json!(count));
        }
        let top: i64 =
            self.connection
                .query_row("SELECT COALESCE(MAX(seq), 0) FROM events", [], |row| {
                    row.get(0)
                })?;
        let agents = self.agents()?;
        let leases = self.leases()?;
        let unread = self.inbox(false)?["messages"]
            .as_array()
            .map_or(0, Vec::len);
        let storage = self.storage()?;
        Ok(json!({
            "format": crate::FORMAT,
            "events": top,
            "agents": agents,
            "leases": leases,
            "records": counts,
            "unread_for_me": unread,
            "storage": storage
        }))
    }

    /// Return live-store and archive sizes plus the configured size thresholds.
    pub fn storage(&self) -> Result<Value, CommonsError> {
        self.storage_with_limits(BUDGET_BYTES, MAX_LIVE_EVENTS)
    }

    /// Return storage sizes with caller-supplied limits.
    pub fn storage_with_limits(
        &self,
        budget_bytes: u64,
        max_live_events: i64,
    ) -> Result<Value, CommonsError> {
        let live = [
            crate::DB_NAME.to_string(),
            format!("{}-wal", crate::DB_NAME),
        ]
        .iter()
        .filter_map(|name| fs::metadata(self.directory.join(name)).ok())
        .map(|metadata| metadata.len())
        .sum::<u64>();
        let mut segments = Vec::new();
        let archive = self.directory.join("archive");
        if archive.exists() {
            for entry in fs::read_dir(&archive)? {
                let entry = entry?;
                let name = entry.file_name();
                let name = name.to_string_lossy();
                if name.starts_with("events-") && name.ends_with(".jsonl.xz") {
                    segments.push(entry.metadata()?.len());
                }
            }
        }
        let (low, high): (Option<i64>, Option<i64>) =
            self.connection
                .query_row("SELECT MIN(seq),MAX(seq) FROM events", [], |row| {
                    Ok((row.get(0)?, row.get(1)?))
                })?;
        let live_events = match (low, high) {
            (Some(low), Some(high)) => high - low + 1,
            _ => 0,
        };
        let record_characters: i64 = self.connection.query_row(
            "SELECT COALESCE(SUM(LENGTH(title)+LENGTH(body)),0) FROM records",
            [],
            |row| row.get(0),
        )?;
        let checkpoints = self.checkpoints()?;
        let archived_through = checkpoints.last().map_or(0, |checkpoint| checkpoint.1);
        let archive_bytes = segments.iter().sum::<u64>();
        Ok(json!({
            "live_bytes": live,
            "budget_bytes": budget_bytes,
            "live_events": live_events,
            "max_live_events": max_live_events,
            "archived_through": archived_through,
            "archive_segments": segments.len(),
            "archive_bytes": archive_bytes,
            "record_characters": record_characters,
            "over_budget": live > budget_bytes || live_events > max_live_events
        }))
    }

    /// Move older events into one checked compressed segment and record a checkpoint event.
    pub fn archive(
        &mut self,
        _keep_events: Option<i64>,
        _if_over: bool,
        _vacuum: bool,
    ) -> Result<Value, CommonsError> {
        Err(CommonsError::new(ARCHIVE_REFUSAL))
    }

    /// Import a JSON-lines feature-gap log, deriving record IDs from content.
    pub fn import_gaps(&mut self, path: impl AsRef<Path>) -> Result<Value, CommonsError> {
        let contents = fs::read_to_string(path)?;
        let mut added = 0_i64;
        let mut existing = 0_i64;
        for line in contents.lines() {
            if line.trim().is_empty() {
                continue;
            }
            let row: Value =
                serde_json::from_str(line).map_err(|error| CommonsError::new(error.to_string()))?;
            let Some(object) = row.as_object() else {
                continue;
            };
            if object.contains_key("schema") {
                continue;
            }
            let title = python_string(
                object
                    .get("need")
                    .filter(|value| !is_falsey(value))
                    .or_else(|| object.get("task").filter(|value| !is_falsey(value)))
                    .unwrap_or(&json!("gap")),
            )
            .chars()
            .take(300)
            .collect::<String>();
            let body = ["task", "what_zenofcis_lacks", "workaround", "evidence"]
                .into_iter()
                .filter_map(|key| {
                    object
                        .get(key)
                        .filter(|value| !is_falsey(value))
                        .map(|value| (key, value))
                })
                .map(|(key, value)| format!("{key}: {}", python_string(value)))
                .collect::<Vec<_>>()
                .join("\n");
            let agent = clean_import_tag(object.get("agent").unwrap_or(&json!("unknown")));
            let severity = clean_import_tag(object.get("severity").unwrap_or(&json!("unknown")));
            let tags = vec![
                agent,
                format!("severity-{severity}"),
                "feature-gap".to_string(),
            ];
            let reporter_tag = format!(
                "reporter-{}",
                clean_import_tag(object.get("agent").unwrap_or(&json!("unknown")))
            );
            let result = self.record_add("gap", &title, &body, "open", &tags, &[], "")?;
            if result["existing"] == true {
                existing += 1;
            } else {
                added += 1;
            }
            let record = self.record(result["id"].as_str().unwrap_or_default())?;
            if !value_strings(&record["tags"]).contains(&reporter_tag) {
                self.record_update_full(
                    result["id"].as_str().unwrap_or_default(),
                    "",
                    "",
                    &[],
                    "",
                    "",
                    &[reporter_tag],
                )?;
            }
        }
        Ok(json!({"added": added, "existing": existing}))
    }
}

fn fold_duplicate(
    transaction: &rusqlite::Transaction<'_>,
    actor: &str,
    src: &str,
    dst: &str,
    seq: i64,
    at: f64,
) -> Result<(), CommonsError> {
    if src == dst {
        return Err(CommonsError::new("a record cannot duplicate itself"));
    }
    let read_record = |record_id: &str| {
        transaction.query_row(
            "SELECT type,status,tags,refs,body,title FROM records WHERE id=?1",
            [record_id],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                ))
            },
        )
    };
    let source = read_record(src)?;
    let canonical = read_record(dst)?;
    if source.0 != canonical.0 {
        return Err(CommonsError::new(
            "only records of the same type can be duplicates",
        ));
    }
    if canonical.1 == "superseded" {
        return Err(CommonsError::new(format!(
            "{dst} is itself superseded; fold into the record that replaced it"
        )));
    }
    let source_tags: Vec<String> =
        serde_json::from_str(&source.2).map_err(|error| CommonsError::new(error.to_string()))?;
    let canonical_tags: Vec<String> =
        serde_json::from_str(&canonical.2).map_err(|error| CommonsError::new(error.to_string()))?;
    let all_tags = source_tags
        .iter()
        .chain(canonical_tags.iter())
        .cloned()
        .collect::<BTreeSet<_>>();
    let severity = all_tags
        .iter()
        .filter_map(|tag| tag.strip_prefix("severity-"))
        .filter_map(|level| SEVERITIES.iter().position(|candidate| *candidate == level))
        .min();
    let mut tags = all_tags
        .into_iter()
        .filter(|tag| !tag.starts_with("severity-"))
        .collect::<BTreeSet<_>>();
    if let Some(level) = severity {
        tags.insert(format!("severity-{}", SEVERITIES[level]));
    }
    let source_refs: Vec<String> =
        serde_json::from_str(&source.3).map_err(|error| CommonsError::new(error.to_string()))?;
    let canonical_refs: Vec<String> =
        serde_json::from_str(&canonical.3).map_err(|error| CommonsError::new(error.to_string()))?;
    let mut refs = source_refs
        .into_iter()
        .chain(canonical_refs)
        .chain(std::iter::once(src.to_string()))
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    refs.truncate(64);
    let mut body = format!(
        "{}\n[{} {}] folded in duplicate {src}: {}",
        canonical.4,
        iso(at),
        actor,
        source.5
    );
    if body.chars().count() > crate::MAX_TEXT {
        body = canonical.4;
    }
    transaction.execute(
        "UPDATE records SET tags=?1,refs=?2,body=?3,updated_seq=?4 WHERE id=?5",
        params![
            canonical_json(&json!(tags.into_iter().take(MAX_TAGS).collect::<Vec<_>>()))?,
            canonical_json(&json!(refs))?,
            body,
            seq,
            dst
        ],
    )?;
    transaction.execute(
        "UPDATE records SET status='superseded',updated_seq=?1 WHERE id=?2",
        params![seq, src],
    )?;
    Ok(())
}

fn duplicate_reporters(
    connection: &rusqlite::Connection,
    record_id: &str,
) -> Result<Vec<String>, CommonsError> {
    let mut seen = HashSet::from([record_id.to_string()]);
    let mut frontier = vec![record_id.to_string()];
    let mut reporters = BTreeSet::new();
    while let Some(current) = frontier.pop() {
        let row = connection
            .query_row(
                "SELECT author,tags FROM records WHERE id=?1",
                [&current],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
            )
            .optional()?;
        if let Some((author, tags)) = row {
            let tags: Vec<String> = serde_json::from_str(&tags)
                .map_err(|error| CommonsError::new(error.to_string()))?;
            let named = tags
                .iter()
                .filter_map(|tag| tag.strip_prefix("reporter-"))
                .collect::<Vec<_>>();
            if named.is_empty() {
                reporters.insert(author);
            } else {
                reporters.extend(named.into_iter().map(str::to_string));
            }
        }
        let mut statement =
            connection.prepare("SELECT src FROM links WHERE rel='duplicates' AND dst=?1")?;
        let rows = statement.query_map([&current], |row| row.get::<_, String>(0))?;
        for source in rows.collect::<Result<Vec<_>, _>>()? {
            if seen.insert(source.clone()) {
                frontier.push(source);
            }
        }
    }
    Ok(reporters.into_iter().collect())
}

fn dedup_words(text: &str) -> BTreeSet<String> {
    let mut result = BTreeSet::new();
    let mut current = String::new();
    let flush = |current: &mut String, result: &mut BTreeSet<String>| {
        if current.chars().count() > 2 && !STOPWORDS.contains(&current.as_str()) {
            result.insert(current.clone());
        }
        current.clear();
    };
    for character in text.chars().flat_map(char::to_lowercase) {
        if character.is_ascii_alphanumeric() {
            current.push(character);
        } else {
            flush(&mut current, &mut result);
        }
    }
    flush(&mut current, &mut result);
    result
}

fn jaccard(left: &BTreeSet<String>, right: &BTreeSet<String>) -> f64 {
    let intersection = left.intersection(right).count();
    let union = left.union(right).count();
    intersection as f64 / union as f64
}

fn value_strings(value: &Value) -> Vec<String> {
    value
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::to_string)
        .collect()
}

fn record_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Value> {
    let tags: String = row.get(5)?;
    let refs: String = row.get(6)?;
    let tags: Value = serde_json::from_str(&tags).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(5, rusqlite::types::Type::Text, Box::new(error))
    })?;
    let refs: Value = serde_json::from_str(&refs).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(6, rusqlite::types::Type::Text, Box::new(error))
    })?;
    Ok(json!({
        "id": row.get::<_, String>(0)?,
        "type": row.get::<_, String>(1)?,
        "status": row.get::<_, String>(2)?,
        "title": row.get::<_, String>(3)?,
        "body": row.get::<_, String>(4)?,
        "tags": tags,
        "refs": refs,
        "author": row.get::<_, String>(7)?,
        "created_seq": row.get::<_, i64>(8)?,
        "updated_seq": row.get::<_, i64>(9)?
    }))
}

fn normalize_tags(tags: &[String]) -> Result<Vec<String>, CommonsError> {
    if tags.len() > MAX_TAGS {
        return Err(CommonsError::new(format!(
            "tags must be a list of at most {MAX_TAGS} names"
        )));
    }
    tags.iter()
        .map(|tag| validate_name(&tag.to_lowercase(), "tag"))
        .collect::<Result<BTreeSet<_>, _>>()
        .map(|tags| tags.into_iter().collect())
}

fn validate_refs(refs: &[String]) -> Result<(), CommonsError> {
    if refs.len() > 64
        || refs
            .iter()
            .any(|reference| reference.chars().count() > 4_096)
    {
        return Err(CommonsError::new(
            "refs must be a list of at most 64 references (paths, commits, record IDs, hashes)",
        ));
    }
    Ok(())
}

fn sha256(value: &str) -> String {
    use sha2::{Digest, Sha256};
    let digest = Sha256::digest(value.as_bytes());
    let mut output = String::with_capacity(64);
    for byte in digest {
        use std::fmt::Write as _;
        let _ = write!(output, "{byte:02x}");
    }
    output
}

/// Normalize resources with the same absolute-path behavior as Python `os.path`.
pub fn normalize_resource(resource: &str) -> Result<String, CommonsError> {
    if resource.is_empty() || resource.chars().count() > 4_096 {
        return Err(CommonsError::new(
            "resource must be non-empty text of at most 4096 characters",
        ));
    }
    if resource.starts_with('/')
        || resource.starts_with('~')
        || resource.starts_with("./")
        || resource.starts_with("../")
    {
        let expanded = if resource == "~" || resource.starts_with("~/") {
            let home = std::env::var_os("HOME")
                .map(PathBuf::from)
                .or_else(|| {
                    std::env::var_os("USER").and_then(|name| user_home(&name.to_string_lossy()))
                })
                .unwrap_or_else(|| PathBuf::from("~"));
            let mut path = home;
            if resource.len() > 2 {
                path.push(&resource[2..]);
            }
            path
        } else if let Some(rest) = resource.strip_prefix("~") {
            if let Some((username, suffix)) = rest.split_once('/') {
                let home =
                    user_home(username).unwrap_or_else(|| PathBuf::from(format!("~{username}")));
                home.join(suffix)
            } else {
                PathBuf::from(resource)
            }
        } else {
            PathBuf::from(resource)
        };
        let absolute = if expanded.is_absolute() {
            expanded
        } else {
            std::env::current_dir()?.join(expanded)
        };
        Ok(lexical_normalize(&absolute).to_string_lossy().into_owned())
    } else {
        Ok(resource.to_string())
    }
}

fn user_home(username: &str) -> Option<PathBuf> {
    let passwd = fs::read_to_string("/etc/passwd").ok()?;
    passwd.lines().find_map(|line| {
        let mut fields = line.split(':');
        if fields.next()? == username {
            fields.nth(4).map(PathBuf::from)
        } else {
            None
        }
    })
}

fn lexical_normalize(path: &Path) -> PathBuf {
    let path = path.to_string_lossy();
    let absolute = path.starts_with('/');
    let prefix = if path.starts_with("//") && !path.starts_with("///") {
        "//"
    } else if absolute {
        "/"
    } else {
        ""
    };
    let mut components = Vec::new();
    for component in path.split('/') {
        match component {
            "" | "." => {}
            ".." => {
                if components.last().is_some_and(|last| *last != "..") {
                    components.pop();
                } else if !absolute {
                    components.push("..");
                }
            }
            other => components.push(other),
        }
    }
    let tail = components.join("/");
    let normalized = if tail.is_empty() {
        if prefix.is_empty() { "." } else { prefix }
    } else {
        // Python's POSIX normpath preserves exactly two leading slashes.
        return PathBuf::from(format!("{prefix}{tail}"));
    };
    PathBuf::from(normalized)
}

fn covers(parent_or_child: &str, resource: &str) -> bool {
    if parent_or_child == resource {
        return true;
    }
    if parent_or_child.starts_with('/') && resource.starts_with('/') {
        let a = parent_or_child.trim_end_matches('/');
        let b = resource.trim_end_matches('/');
        resource.starts_with(&format!("{a}/")) || parent_or_child.starts_with(&format!("{b}/"))
    } else {
        false
    }
}

fn words(text: &str) -> HashSet<String> {
    let mut found = HashSet::new();
    let mut word = String::new();
    for character in text.chars().flat_map(char::to_lowercase) {
        if character.is_ascii_alphanumeric() {
            word.push(character);
        } else {
            if word.chars().count() > 2 {
                found.insert(std::mem::take(&mut word));
            } else {
                word.clear();
            }
        }
    }
    if word.chars().count() > 2 {
        found.insert(word);
    }
    found
}

fn python_string(value: &Value) -> String {
    match value {
        Value::Null => "None".to_string(),
        Value::Bool(true) => "True".to_string(),
        Value::Bool(false) => "False".to_string(),
        Value::String(value) => value.clone(),
        Value::Array(values) => format!(
            "[{}]",
            values
                .iter()
                .map(python_string)
                .collect::<Vec<_>>()
                .join(", ")
        ),
        Value::Object(_) => canonical_json(value).unwrap_or_default(),
        Value::Number(value) => value.to_string(),
    }
}

fn clean_import_tag(value: &Value) -> String {
    let mut cleaned = String::new();
    let mut replacing = false;
    for character in python_string(value).to_lowercase().chars() {
        if character.is_ascii_alphanumeric() || "._:@/-".contains(character) {
            cleaned.push(character);
            replacing = false;
        } else if !replacing {
            cleaned.push('-');
            replacing = true;
        }
    }
    let trimmed = cleaned.trim_matches(['-', '.', '_', ':', '@', '/']);
    let result = trimmed.chars().take(100).collect::<String>();
    if result.is_empty() {
        "unknown".to_string()
    } else {
        result
    }
}

fn read_through(connection: &rusqlite::Connection, actor: &str) -> Result<i64, CommonsError> {
    connection
        .query_row(
            "SELECT read_through FROM agents WHERE name=?1",
            [actor],
            |row| row.get(0),
        )
        .optional()
        .map(|value| value.unwrap_or(0))
        .map_err(CommonsError::from)
}

fn unreleased_leases(
    connection: &rusqlite::Connection,
) -> Result<Vec<(String, String, String, f64)>, CommonsError> {
    let mut statement =
        connection.prepare("SELECT resource, owner, note, expires_at FROM leases")?;
    let rows = statement.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, f64>(3)?,
        ))
    })?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(CommonsError::from)
}

fn lease_view(resource: String, owner: String, note: String, expires: f64, at: f64) -> Value {
    json!({
        "resource": resource,
        "owner": owner,
        "note": note,
        "expires": iso(expires),
        "state": if expires > at { "active" } else { "stale" }
    })
}

fn require_registered(connection: &rusqlite::Connection, actor: &str) -> Result<(), CommonsError> {
    let registered = connection
        .query_row(
            "SELECT 1 FROM identities WHERE name=?1",
            [actor],
            |_| Ok(()),
        )
        .optional()?
        .is_some();
    if !registered {
        return Err(CommonsError::new(format!(
            "{actor} is not registered; run register first so others can address you"
        )));
    }
    Ok(())
}

fn resolve_agent(
    connection: &rusqlite::Connection,
    name_or_id: &str,
) -> Result<String, CommonsError> {
    connection
        .query_row(
            "SELECT name FROM identities WHERE name=?1 OR id=?1",
            [name_or_id],
            |row| row.get(0),
        )
        .optional()?
        .ok_or_else(|| CommonsError::new(format!("no registered agent {name_or_id}")))
}
