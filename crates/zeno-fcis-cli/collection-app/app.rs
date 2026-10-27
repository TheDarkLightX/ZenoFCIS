//! Collection application shell. Only the bound Authority decides.
mod journal;
mod operations;
mod previous;
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    fs,
    io::Read as _,
    path::{Path, PathBuf},
    time::Duration,
};
use zeno_fcis_codec::{CommitmentHasher, Hash32, RustCryptoSha256};
use zeno_fcis_contract::collections::runtime::{Entries, Framed, Input, Key, Layout, Writes};
use zeno_fcis_contract::internals::declarations::Source;
use zeno_fcis_ingress::{Asset, Command, Config, Endpoint, Refusal, Server, Trace, TraceSink};
use zeno_fcis_shell_sqlite::collections::{CollectionStore, Commit, RequestBinding};
use zeno_fcis_synthesis::finite::{
    v2_authority::{Authority, PublicationOutcome},
    v2_composition as c,
};

const PROJECT: &str = include_str!("../project.zeno");
const POLICY: &str = include_str!("../v2/policy.json");

fn unavailable<T>(_: T) -> Refusal {
    Refusal::Unavailable
}
fn encoded(value: &Value) -> Result<Vec<u8>, Refusal> {
    serde_json::to_vec(value).map_err(unavailable)
}
/// Canonical lowercase hexadecimal transport.
pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
fn numbers(values: &[i128]) -> Vec<String> {
    values.iter().map(ToString::to_string).collect()
}
fn scalars(bytes: &[u8]) -> Result<BTreeMap<u16, i128>, Refusal> {
    let fields: BTreeMap<u16, String> = serde_json::from_slice(bytes).map_err(unavailable)?;
    fields
        .into_iter()
        .map(|(id, text)| text.parse().map(|v| (id, v)).map_err(unavailable))
        .collect()
}
fn scalar_bytes(fields: &BTreeMap<u16, i128>) -> Result<Vec<u8>, Refusal> {
    serde_json::to_vec(
        &fields
            .iter()
            .map(|(id, value)| (*id, value.to_string()))
            .collect::<BTreeMap<_, _>>(),
    )
    .map_err(unavailable)
}
/// Source and checked-catalog binding for this collection application.
pub fn identity(authority: &Authority<'_>) -> Hash32 {
    let source = RustCryptoSha256::hash_parts(&[
        include_bytes!("app.rs"),
        include_bytes!("app/operations.rs"),
        include_bytes!("app/journal.rs"),
        include_bytes!("app/previous.rs"),
        include_bytes!("lib.rs"),
    ]);
    let layout = zeno_fcis_contract::collections::runtime::source_digest();
    let storage = zeno_fcis_shell_sqlite::collections::source_digest();
    RustCryptoSha256::hash_parts(&[
        source.as_bytes(),
        &layout,
        &storage,
        b"ZFCIS-COLLECTION-APP/2\0",
        authority.identity(),
        PROJECT.as_bytes(),
        POLICY.as_bytes(),
    ])
}
struct Loaded(BTreeMap<(u32, Key), Option<Vec<u8>>>);
impl Entries for Loaded {
    fn record(&self, map: u32, key: &Key) -> Option<Vec<u8>> {
        self.0.get(&(map, *key)).cloned().flatten()
    }
}
struct Prepared {
    pre_scalars: Vec<u8>,
    framed: Framed,
    writes: Option<Writes>,
    subject: Option<Vec<u8>>,
    reply: Value,
    loads: u64,
}
/// Exclusive storage preparation and publication shell; no mutable core inputs escape.
pub struct Application<'a, 'p> {
    authority: &'a Authority<'p>,
    layout: Layout,
    store: CollectionStore,
    parties: Vec<(Hash32, Input)>,
}
impl<'a, 'p> Application<'a, 'p> {
    /// Binds a store and copied, authenticated-principal contexts. Keys use fixed hex.
    pub fn open(
        authority: &'a Authority<'p>,
        path: &Path,
        parties: Vec<(Hash32, Vec<String>)>,
    ) -> Result<Self, Refusal> {
        let store = CollectionStore::open(path).map_err(unavailable)?;
        let app = Self::with_store(authority, store, parties)?;
        app.audit()?;
        Ok(app)
    }
    /// Binds an exclusively owned store; useful for isolated application journeys.
    pub fn with_store(
        authority: &'a Authority<'p>,
        mut store: CollectionStore,
        parties: Vec<(Hash32, Vec<String>)>,
    ) -> Result<Self, Refusal> {
        if parties.is_empty()
            || parties.len() > zeno_fcis_ingress::MAX_PARTIES
            || parties
                .iter()
                .enumerate()
                .any(|(i, (owner, _))| parties[..i].iter().any(|(old, _)| old == owner))
        {
            return Err(Refusal::Binding);
        }
        // This profile does not silently discard deliveries.
        if !authority.descriptor().channels.is_empty() {
            return Err(Refusal::Binding);
        }
        let layout = Layout::read(PROJECT, POLICY).map_err(unavailable)?;
        let parties = parties
            .into_iter()
            .map(|(owner, fields)| {
                Ok((
                    owner,
                    layout
                        .decode_input(Source::Context, &fields)
                        .map_err(|_| Refusal::Malformed)?,
                ))
            })
            .collect::<Result<Vec<_>, Refusal>>()?;
        let initial = crate::genesis().map_err(unavailable)?;
        if !matches!(
            authority.publish_genesis(&initial),
            PublicationOutcome::Commit(_)
        ) {
            return Err(Refusal::Decision);
        }
        let all = crate::v2_contract::GENESIS
            .iter()
            .map(|field| Ok((field.id, atom(field.value)?)))
            .collect::<Result<BTreeMap<_, _>, Refusal>>()?;
        store
            .bind(
                identity(authority).as_bytes(),
                &scalar_bytes(&layout.scalar_state(&all))?,
                layout.capacities(),
            )
            .map_err(unavailable)?;
        Ok(Self {
            authority,
            layout,
            store,
            parties,
        })
    }
    /// Immutable storage access for audits; this is not a decision API.
    pub fn store(&self) -> &CollectionStore {
        &self.store
    }
    /// Return the exclusive storage handle after the application is stopped.
    pub fn into_store(self) -> CollectionStore {
        self.store
    }
    /// The original command/context interface, including all opaque key fields.
    pub fn interface(&self) -> Value {
        self.layout.interface()
    }
    fn context(&self, owner: Hash32) -> Result<&Input, Refusal> {
        self.parties
            .iter()
            .find(|(p, _)| *p == owner)
            .map(|(_, c)| c)
            .ok_or(Refusal::Binding)
    }
    fn prepare(&self, command: &[String], context: &Input) -> Result<Prepared, Refusal> {
        let command = self
            .layout
            .decode_input(Source::Command, command)
            .map_err(|_| Refusal::Malformed)?;
        let named = self
            .layout
            .named(&command, context)
            .map_err(|_| Refusal::Malformed)?;
        let pre_scalars = self.store.scalar_state().map_err(unavailable)?;
        let state = scalars(&pre_scalars)?;
        let mut loaded = Loaded(BTreeMap::new());
        let mut loads = 0;
        for (map, keys) in &named {
            for key in keys {
                // The only stored-record load in decision preparation.
                let record = self.store.get(*map, key).map_err(unavailable)?;
                loaded.0.insert((*map, *key), record);
                loads += 1;
            }
        }
        let framed = self
            .layout
            .frame(&state, &command, context, &loaded)
            .map_err(unavailable)?;
        let raw = crate::wire(self.authority.descriptor(), &framed.tuple)
            .map_err(|_| Refusal::Malformed)?;
        let outcome = self.authority.publish(c::Raw {
            state: &raw[0],
            command: &raw[1],
            context: &raw[2],
        });
        let candidate = outcome
            .evaluation()
            .result()
            .map_err(|_| Refusal::Decision)?;
        let post = candidate
            .post()
            .iter()
            .map(|f| Ok((f.id, atom(f.value)?)))
            .collect::<Result<BTreeMap<_, _>, Refusal>>()?;
        let reply = json!({"input":numbers(&framed.tuple),"class":format!("{:?}",candidate.class()),"reason":candidate.reason(),"post":post.iter().map(|(id,v)| (id.to_string(),v.to_string())).collect::<BTreeMap<_,_>>(),"loads":loads});
        let (writes, subject) = match outcome {
            PublicationOutcome::Commit(publication) => {
                let writes = self
                    .layout
                    .writes(&framed, &post)
                    .map_err(|_| Refusal::Decision)?;
                let checked = self
                    .layout
                    .check_successor(
                        self.authority.descriptor(),
                        &framed,
                        &writes,
                        publication.evaluation(),
                    )
                    .map_err(|_| Refusal::Decision)?;
                if checked.into_parts().0.is_err() {
                    return Err(Refusal::Decision);
                }
                (Some(writes), Some(publication.subject().to_vec()))
            }
            PublicationOutcome::Reject(_) => (None, None),
            _ => return Err(Refusal::Decision),
        };
        Ok(Prepared {
            pre_scalars,
            framed,
            writes,
            subject,
            reply,
            loads,
        })
    }
    /// Decide and optionally commit one command; callers supply only transport data.
    pub fn decide(
        &mut self,
        owner: Hash32,
        expected: u64,
        command: &[String],
        binding: Option<RequestBinding>,
    ) -> Result<Value, Refusal> {
        let context = self.context(owner)?.clone();
        let wire = serde_json::to_vec(command).map_err(unavailable)?;
        self.apply(owner, expected, command, &context, binding, &wire)
    }
    fn apply(
        &mut self,
        owner: Hash32,
        expected: u64,
        command: &[String],
        context: &Input,
        binding: Option<RequestBinding>,
        wire: &[u8],
    ) -> Result<Value, Refusal> {
        if expected != self.store.version().map_err(unavailable)? {
            return Err(Refusal::Conflict);
        }
        let prepared = self.prepare(command, context)?;
        let mut report = json!({"reply":prepared.reply,"revision":expected.to_string(),"key_fields":self.interface()["fields"],"loads":prepared.loads});
        if let (Some(writes), Some(binding)) = (prepared.writes, binding) {
            let context = self
                .layout
                .encode_input(Source::Context, context)
                .map_err(unavailable)?;
            let decision = encoded(
                &json!({"format":"zeno-fcis/collection-submission/2","principal":hex(owner.as_bytes()),"expected":expected.to_string(),"key":hex(&binding.key),"wire":hex(wire),"command":command,"context":context,"pre_scalars":hex(&prepared.pre_scalars),"subject":hex(prepared.subject.as_deref().ok_or(Refusal::Decision)?),"reply":report["reply"],"request":hex(&binding.request)}),
            )?;
            let commit = self
                .store
                .commit_request(
                    expected,
                    &prepared.framed.named,
                    &writes.entries,
                    &decision,
                    &scalar_bytes(&writes.scalars)?,
                    binding,
                )
                .map_err(|error| match error {
                    zeno_fcis_shell_sqlite::collections::Error::Conflict => Refusal::Conflict,
                    zeno_fcis_shell_sqlite::collections::Error::Replay => Refusal::Replay,
                    _ => Refusal::Unavailable,
                })?;
            report["revision"] = json!(commit.sequence.to_string());
            report["receipt"] = receipt(&commit);
        }
        Ok(report)
    }
    /// Export original submissions and complete receipts in committed order.
    /// This convenience API materializes history; CLI recovery uses `journal_entry`.
    pub fn journal(&self) -> Result<Vec<Value>, Refusal> {
        self.store
            .commits()
            .map_err(unavailable)?
            .iter()
            .map(|commit| {
                let submission: Value =
                    serde_json::from_slice(&commit.decision).map_err(unavailable)?;
                Ok(json!({"submission":submission,"receipt":receipt(commit)}))
            })
            .collect()
    }

    /// Replay a journal into this fresh prepared store. On failure discard this target.
    /// Every decision is recomputed by Authority and every named root is checked separately.
    pub fn replay(&mut self, journal: &[Value]) -> Result<(), Refusal> {
        if self.store.version().map_err(unavailable)? != 0 {
            return Err(Refusal::Conflict);
        }
        for row in journal {
            self.replay_entry(row)?;
        }
        Ok(())
    }

    /// One indexed export page, owned by the caller. A missing position refuses.
    pub fn journal_entry(&self, sequence: u64) -> Result<Value, Refusal> {
        let commit = self
            .store
            .commit_at(sequence)
            .map_err(unavailable)?
            .ok_or(Refusal::Binding)?;
        let submission: Value = serde_json::from_slice(&commit.decision).map_err(unavailable)?;
        Ok(json!({"submission":submission,"receipt":receipt(&commit)}))
    }

    /// Replay one owned page at the current position. Discard this target on error.
    /// This shell utility recomputes Authority's decision; it grants no authority.
    pub fn replay_entry(&mut self, row: &Value) -> Result<(), Refusal> {
        let submission = &row["submission"];
        if row.as_object().map(|o| o.len()) != Some(2) {
            return Err(Refusal::Binding);
        }
        if submission["format"] != "zeno-fcis/collection-submission/2" {
            return Err(Refusal::Binding);
        }
        let expected = self.store.version().map_err(unavailable)?;
        let expected_text = expected.to_string();
        if submission["expected"].as_str() != Some(expected_text.as_str()) {
            return Err(Refusal::Binding);
        }
        let owner = Hash32::new(key_field(submission, "principal")?);
        let binding = RequestBinding {
            key: key_field(submission, "key")?,
            request: key_field(submission, "request")?,
        };
        let wire = unhex(submission["wire"].as_str().ok_or(Refusal::Binding)?)?;
        let command: Vec<String> = serde_json::from_slice(&wire).map_err(|_| Refusal::Binding)?;
        if serde_json::to_value(&command).map_err(unavailable)? != submission["command"] {
            return Err(Refusal::Binding);
        }
        let context_values: Vec<String> =
            serde_json::from_value(submission["context"].clone()).map_err(|_| Refusal::Binding)?;
        let context = self
            .layout
            .decode_input(Source::Context, &context_values)
            .map_err(|_| Refusal::Binding)?;
        let decoded = self
            .layout
            .decode_input(Source::Command, &command)
            .map_err(|_| Refusal::Binding)?;
        let named = self
            .layout
            .named(&decoded, &context)
            .map_err(|_| Refusal::Binding)?;
        let pre = self.store.scalar_state().map_err(unavailable)?;
        if row["receipt"]["pre_scalars"] != hex(&pre) || submission["pre_scalars"] != hex(&pre) {
            return Err(Refusal::Binding);
        }
        self.check_roots(&row["receipt"]["pre_roots"], &named)?;
        let result = self.apply(owner, expected, &command, &context, Some(binding), &wire)?;
        if result["reply"]["class"] == "Reject" {
            return Err(Refusal::Binding);
        }
        // This acceptance check is independent of the receipt producer.
        // Missing or additional roots must fail even if both encoders omit them.
        self.check_roots(&row["receipt"]["post_roots"], &named)?;
        if row["receipt"]["post_scalars"] != hex(&self.store.scalar_state().map_err(unavailable)?) {
            return Err(Refusal::Binding);
        }
        let (_, commit) = self
            .store
            .request(&binding.key)
            .map_err(unavailable)?
            .ok_or(Refusal::Binding)?;
        let actual: Value = serde_json::from_slice(&commit.decision).map_err(unavailable)?;
        if actual != *submission || result["receipt"] != row["receipt"] {
            return Err(Refusal::Binding);
        }
        Ok(())
    }

    fn check_roots(
        &self,
        value: &Value,
        named: &BTreeMap<u32, std::collections::BTreeSet<Key>>,
    ) -> Result<(), Refusal> {
        let roots = value.as_object().ok_or(Refusal::Binding)?;
        if roots.len() != named.len() {
            return Err(Refusal::Binding);
        }
        for map in named.keys() {
            let root = self.store.root(*map).map_err(unavailable)?;
            if roots.get(&map.to_string()) != Some(&json!(hex(&root))) {
                return Err(Refusal::Binding);
            }
        }
        Ok(())
    }

    /// Startup/audit work: replay all submissions and compare every stored map and index.
    /// This map-wide recovery check does not run in request preparation.
    pub fn audit(&self) -> Result<(), Refusal> {
        let parties = self
            .parties
            .iter()
            .map(|(owner, context)| {
                Ok((
                    *owner,
                    self.layout
                        .encode_input(Source::Context, context)
                        .map_err(unavailable)?,
                ))
            })
            .collect::<Result<Vec<_>, Refusal>>()?;
        let mut replay = Self::with_store(
            self.authority,
            CollectionStore::temporary().map_err(unavailable)?,
            parties,
        )?;
        let version = self.store.version().map_err(unavailable)?;
        for sequence in 1..=version {
            let row = self.journal_entry(sequence)?;
            replay.replay_entry(&row)?;
            let submission = &row["submission"];
            let binding = key_field(submission, "key")?;
            let (request, commit) = self
                .store
                .request(&binding)
                .map_err(unavailable)?
                .ok_or(Refusal::Binding)?;
            if request != key_field(submission, "request")? || receipt(&commit) != row["receipt"] {
                return Err(Refusal::Binding);
            }
        }
        if self.store.scalar_state().map_err(unavailable)?
            != replay.store.scalar_state().map_err(unavailable)?
        {
            return Err(Refusal::Binding);
        }
        for map in self.layout.capacities().keys() {
            if self.store.root(*map).map_err(unavailable)?
                != replay.store.root(*map).map_err(unavailable)?
                || self.store.entries(*map).map_err(unavailable)?
                    != replay.store.entries(*map).map_err(unavailable)?
            {
                return Err(Refusal::Binding);
            }
        }
        self.store.audit_application().map_err(unavailable)?;
        if self.store.version().map_err(unavailable)? != version {
            return Err(Refusal::Conflict);
        }
        Ok(())
    }
}
fn key_field(value: &Value, name: &str) -> Result<Key, Refusal> {
    zeno_fcis_contract::collections::runtime::decode_key(
        value[name].as_str().ok_or(Refusal::Binding)?,
    )
    .ok_or(Refusal::Binding)
}
fn unhex(text: &str) -> Result<Vec<u8>, Refusal> {
    if !text.len().is_multiple_of(2)
        || !text
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err(Refusal::Binding);
    }
    text.as_bytes()
        .chunks_exact(2)
        .map(|p| {
            let digit = |b: u8| if b <= b'9' { b - b'0' } else { b - b'a' + 10 };
            Ok(digit(p[0]) * 16 + digit(p[1]))
        })
        .collect()
}
fn atom(value: c::Atom<'_>) -> Result<i128, Refusal> {
    match value {
        c::Atom::Bool(v) => Ok(i128::from(v)),
        c::Atom::I128(v) => Ok(v),
        c::Atom::Sum { variant, .. } => Ok(i128::from(variant)),
        _ => Err(Refusal::Binding),
    }
}
/// A collection commit's complete scalar and named-map root binding.
pub fn receipt(commit: &Commit) -> Value {
    let roots = |r: &BTreeMap<u32, [u8; 32]>| {
        r.iter()
            .map(|(id, root)| (id.to_string(), hex(root)))
            .collect::<BTreeMap<_, _>>()
    };
    let submission: Value = serde_json::from_slice(&commit.decision).unwrap_or(Value::Null);
    json!({"format":"zeno-fcis/collection-receipt/2","sequence":commit.sequence.to_string(),"pre_scalars":submission["pre_scalars"],"post_scalars":hex(&commit.scalars),"pre_roots":roots(&commit.pre_roots),"post_roots":roots(&commit.post_roots),"chain":hex(&commit.chain)})
}
impl Endpoint for Application<'_, '_> {
    fn snapshot(&mut self, principal: Hash32) -> Result<Vec<u8>, Refusal> {
        let context = self.context(principal)?;
        encoded(
            &json!({"contract":hex(identity(self.authority).as_bytes()),"revision":self.store.version().map_err(unavailable)?.to_string(),"scalars":scalars(&self.store.scalar_state().map_err(unavailable)?)?.iter().map(|(id,v)|(id.to_string(),v.to_string())).collect::<BTreeMap<_,_>>(),"context":self.layout.encode_input(Source::Context,context).map_err(unavailable)?,"interface":self.interface()}),
        )
    }
    fn preview(&mut self, request: &Command) -> Result<Vec<u8>, Refusal> {
        let command: Vec<String> =
            serde_json::from_slice(request.bytes()).map_err(|_| Refusal::Malformed)?;
        let report = self.decide(request.principal(), request.expected(), &command, None)?;
        encoded(&report)
    }
    fn execute(&mut self, request: &Command, _: &mut dyn TraceSink) -> Result<Vec<u8>, Refusal> {
        let binding = RequestBinding {
            key: *request.replay_key().as_bytes(),
            request: *request.id().as_bytes(),
        };
        if let Some((original, commit)) = self.store.request(&binding.key).map_err(unavailable)? {
            if original != binding.request {
                return Err(Refusal::Replay);
            }
            let data: Value = serde_json::from_slice(&commit.decision).map_err(unavailable)?;
            let context = self
                .layout
                .encode_input(Source::Context, self.context(request.principal())?)
                .map_err(unavailable)?;
            if data["context"] != json!(context) {
                return Err(Refusal::Replay);
            }
            return encoded(
                &json!({"reply":data["reply"],"revision":commit.sequence.to_string(),"receipt":receipt(&commit)}),
            );
        }
        let command: Vec<String> =
            serde_json::from_slice(request.bytes()).map_err(|_| Refusal::Malformed)?;
        let context = self.context(request.principal())?.clone();
        let report = self.apply(
            request.principal(),
            request.expected(),
            &command,
            &context,
            Some(binding),
            request.bytes(),
        )?;
        encoded(&report)
    }
}
struct Log;
impl TraceSink for Log {
    fn record(&mut self, trace: Trace) {
        eprintln!("{trace}");
    }
}
fn setting(name: &str) -> Result<Option<String>, String> {
    match std::env::var_os(name).map(std::ffi::OsString::into_string) {
        None => Ok(None),
        Some(Ok(value)) if !value.is_empty() => Ok(Some(value)),
        Some(_) => Err(format!("{name} must be nonempty text")),
    }
}
fn bounded(path: &Path) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    fs::File::open(path)
        .and_then(|file| file.take(65_537).read_to_end(&mut bytes))
        .map_err(|_| "configuration file".to_owned())?;
    if bytes.len() > 65_536 {
        return Err("configuration size".into());
    }
    Ok(bytes)
}
/// Owned startup inputs. Ingress and Layout perform the binding checks.
pub struct PartySettings {
    /// The transport principal selected by its private credential.
    pub principal: Hash32,
    /// Owned context strings, checked by Layout during application binding.
    pub context: Vec<String>,
    secret_file: PathBuf,
}
/// Read bounded party settings; relative credential paths use the settings directory.
pub fn load_parties(path: &Path) -> Result<Vec<PartySettings>, String> {
    let rows: Vec<Value> =
        serde_json::from_slice(&bounded(path)?).map_err(|_| "party configuration")?;
    if rows.is_empty() || rows.len() > zeno_fcis_ingress::MAX_PARTIES {
        return Err("party count".into());
    }
    let parent = path.parent().unwrap_or(Path::new("."));
    rows.into_iter()
        .map(|row| {
            if row.as_object().map(|fields| fields.len()) != Some(3) {
                return Err("party configuration fields".into());
            }
            let principal = row["principal"]
                .as_str()
                .and_then(zeno_fcis_contract::collections::runtime::decode_key)
                .map(Hash32::new)
                .ok_or("party principal")?;
            let context = serde_json::from_value::<Vec<String>>(row["context"].clone())
                .map_err(|_| "party context")?;
            let file = row["secret_file"]
                .as_str()
                .filter(|text| !text.is_empty())
                .ok_or("party credential file")?;
            Ok(PartySettings {
                principal,
                context,
                secret_file: parent.join(file),
            })
        })
        .collect()
}
fn read_secret(path: &Path) -> Result<[u8; 32], String> {
    let text = String::from_utf8(bounded(path)?).map_err(|_| "credential file")?;
    zeno_fcis_contract::collections::runtime::decode_key(text.strip_suffix('\n').unwrap_or(&text))
        .ok_or_else(|| "credential encoding".into())
}
/// Copy private credentials once for server startup; no file is retained by ingress.
pub fn read_credentials(parties: &[PartySettings]) -> Result<Vec<([u8; 32], Hash32)>, String> {
    parties
        .iter()
        .map(|party| read_secret(&party.secret_file).map(|secret| (secret, party.principal)))
        .collect()
}
fn selected_parties() -> Result<Option<Vec<PartySettings>>, String> {
    let Some(path) = setting("ZENO_APP_PARTIES")? else {
        return Ok(None);
    };
    if setting("ZENO_APP_SECRET_FILE")?.is_some() || setting("ZENO_APP_CONTEXT")?.is_some() {
        return Err("party configuration conflicts with single-party settings".into());
    }
    load_parties(Path::new(&path)).map(Some)
}
fn context_file() -> Result<Vec<String>, String> {
    serde_json::from_slice(&bounded(Path::new(
        &setting("ZENO_APP_CONTEXT")?.unwrap_or_else(|| "context.json".into()),
    ))?)
    .map_err(|_| "context encoding".into())
}

/// Start the authenticated loopback server, using the collection-specific plain form.
pub fn run() -> Result<(), String> {
    let contract = crate::v2_contract::Contract::new();
    let descriptor = contract.descriptor();
    let authority = crate::authority(&descriptor)?;
    if std::env::args().nth(1).as_deref() == Some("--interface") {
        println!(
            "{}",
            Layout::read(PROJECT, POLICY)
                .map_err(|e| e.to_string())?
                .interface()
        );
        return Ok(());
    }
    let arguments: Vec<std::ffi::OsString> = std::env::args_os().skip(1).collect();
    if operations::run(&authority, &arguments)?.is_some() {
        return Ok(());
    }
    const SETTINGS: &[&str] = &[
        "ZENO_APP_PARTIES",
        "ZENO_APP_SECRET_FILE",
        "ZENO_APP_CONTEXT",
        "ZENO_APP_STORE",
        "ZENO_APP_ADDRESS",
    ];
    for (name, _) in std::env::vars_os() {
        let name = name.to_string_lossy();
        if name.starts_with("ZENO_APP_") && !SETTINGS.contains(&name.as_ref()) {
            return Err(format!("unsupported setting: {name}"));
        }
    }
    if std::env::args_os().len() != 1 {
        return Err("usage: app-server [--interface | --journal STORE | --audit STORE | --replay JOURNAL NEW_STORE]".into());
    }
    let (credentials, parties) = if let Some(settings) = selected_parties()? {
        let credentials = read_credentials(&settings)?;
        let parties = settings
            .into_iter()
            .map(|party| (party.principal, party.context))
            .collect();
        (credentials, parties)
    } else {
        let path = setting("ZENO_APP_SECRET_FILE")?.ok_or("set ZENO_APP_SECRET_FILE")?;
        let secret = read_secret(Path::new(&path))?;
        let principal = Hash32::new([7; 32]);
        let context = context_file()?;
        (vec![(secret, principal)], vec![(principal, context)])
    };
    let path = setting("ZENO_APP_STORE")?.unwrap_or_else(|| "app.sqlite".into());
    let address: std::net::SocketAddr = setting("ZENO_APP_ADDRESS")?
        .unwrap_or_else(|| "127.0.0.1:8080".into())
        .parse()
        .map_err(|_| "address")?;
    if !address.ip().is_loopback() {
        return Err("loopback required".into());
    }
    let config = Config::with_parties(
        credentials,
        identity(&authority),
        format!("http://{address}"),
        Duration::from_secs(10),
    )
    .map_err(|_| "ingress configuration")?;
    let endpoint = Application::open(&authority, Path::new(&path), parties)
        .map_err(|e| format!("bind application: {e:?}"))?;
    let assets = vec![
        Asset::new(
            "/",
            "text/html; charset=utf-8",
            include_bytes!("../www/index.html"),
        )
        .map_err(|_| "form asset")?,
        Asset::new(
            "/form.js",
            "text/javascript; charset=utf-8",
            include_bytes!("../www/form.js"),
        )
        .map_err(|_| "form script")?,
        Asset::new(
            "/interface.json",
            "application/json",
            include_bytes!("../www/interface.json"),
        )
        .map_err(|_| "interface asset")?,
    ];
    let mut server = Server::new(config, endpoint, Log)
        .with_assets(assets)
        .map_err(|_| "assets")?;
    zeno_fcis_ingress::runtime()
        .map_err(|_| "runtime")?
        .block_on(async {
            let listener = tokio::net::TcpListener::bind(address)
                .await
                .map_err(|_| "listen")?;
            println!("Application available at http://{address}");
            loop {
                if server.serve_one(&listener).await.is_err() {
                    eprintln!("request interrupted");
                }
            }
        })
}
