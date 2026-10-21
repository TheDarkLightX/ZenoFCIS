//! Authenticated shell. Immutable Authority inputs decide; this module owns I/O.
use crate::AppResult;
use serde_json::{Value, json};
use std::{
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    time::{Duration, Instant},
};
use zeno_fcis_codec::{CommitmentHasher, Hash32, RustCryptoSha256};
use zeno_fcis_ingress::{
    Asset, Command, Config, Durable, Endpoint, Refusal, Server, Trace, TraceSink,
};
use zeno_fcis_shell::effects::{
    handler::Terminal,
    port::{Delivery, EpochTick, Fault, Report, Sent, WaitError},
};
use zeno_fcis_shell_sqlite::{
    effects::production::{Production, Time, Transport},
    v2::{Lineage, V2SqliteShell},
};
use zeno_fcis_synthesis::finite::{
    v2_authority::{Authority, PublicationOutcome},
    v2_composition as c,
};

fn failed<T>(_: T) -> Refusal {
    Refusal::Unavailable
}
fn encoded(value: Value) -> Result<Vec<u8>, Refusal> {
    serde_json::to_vec(&value).map_err(failed)
}
/// Encode canonical hexadecimal bytes for the HTTP transport and retained packets.
pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
/// Decode canonical lowercase hexadecimal bytes.
pub fn unhex(text: &str) -> AppResult<Vec<u8>> {
    if !text.len().is_multiple_of(2)
        || !text
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err("invalid hexadecimal value".into());
    }
    text.as_bytes()
        .chunks_exact(2)
        .map(|p| {
            let d = |b: u8| if b <= b'9' { b - b'0' } else { b - b'a' + 10 };
            Ok(d(p[0]) * 16 + d(p[1]))
        })
        .collect()
}
fn strings(numbers: &[i128]) -> Vec<String> {
    numbers.iter().map(ToString::to_string).collect()
}
fn numbers(value: &Value) -> Result<Vec<i128>, Refusal> {
    value
        .as_array()
        .ok_or(Refusal::Malformed)?
        .iter()
        .map(|v| {
            let text = v.as_str().ok_or(Refusal::Malformed)?;
            let n = text.parse::<i128>().map_err(|_| Refusal::Malformed)?;
            if n.to_string() != text {
                return Err(Refusal::Malformed);
            }
            Ok(n)
        })
        .collect()
}
fn decision(
    authority: &Authority<'_>,
    state: &[u8],
    command: &[u8],
    context: &[u8],
) -> Result<Value, Refusal> {
    let d = authority.descriptor();
    let mut input = crate::session::root_numbers(d.state, state).map_err(failed)?;
    input.extend(crate::session::root_numbers(d.command, command).map_err(failed)?);
    input.extend(crate::session::root_numbers(d.context, context).map_err(failed)?);
    let report = authority.evaluate(c::Raw {
        state,
        command,
        context,
    });
    let Ok(candidate) = report.result() else {
        return Ok(
            json!({"input":strings(&input),"class":"Refused","reason":null,"post":[],"checked":"1","total":"1"}),
        );
    };
    let post = if candidate.class() == c::Class::Reject {
        Vec::new()
    } else {
        candidate
            .post()
            .iter()
            .map(|f| match f.value {
                c::Atom::Bool(v) => Ok(i128::from(v)),
                c::Atom::I128(v) => Ok(v),
                c::Atom::Sum { variant, .. } | c::Atom::Enum { variant, .. } => {
                    Ok(i128::from(variant))
                }
                _ => Err(Refusal::Binding),
            })
            .collect::<Result<Vec<_>, _>>()?
    };
    Ok(
        json!({"input":strings(&input),"class":format!("{:?}",candidate.class()),"reason":candidate.reason().map(|v|v.to_string()),"post":strings(&post),"checked":"1","total":"1"}),
    )
}
/// Exclusive shell endpoint. The authenticated principal alone selects its copied
/// context; HTTP cannot select one.
pub struct Application<'a, 'p, H> {
    durable: Durable<'a, 'p, H>,
    authority: &'a Authority<'p>,
    lineage: &'a Lineage<'a, 'p>,
    path: PathBuf,
}
impl<'a, 'p, H: Terminal> Application<'a, 'p, H> {
    /// Bind the immutable configuration and the exclusive terminal once.
    pub fn new(
        lineage: &'a Lineage<'a, 'p>,
        terminal: H,
        store: zeno_fcis_shell::effects::port::Store,
        path: &Path,
        principal: Hash32,
        context: &[u8],
    ) -> Result<Self, Refusal> {
        Self::with_parties(
            lineage,
            terminal,
            store,
            path,
            vec![(principal, context.to_vec())],
        )
    }
    /// Bind several parties, each a distinct principal with its own canonical context.
    pub fn with_parties(
        lineage: &'a Lineage<'a, 'p>,
        terminal: H,
        store: zeno_fcis_shell::effects::port::Store,
        path: &Path,
        parties: Vec<(Hash32, Vec<u8>)>,
    ) -> Result<Self, Refusal> {
        let authority = lineage.authorities().last().ok_or(Refusal::Binding)?;
        Ok(Self {
            durable: Durable::with_parties(authority, terminal, store, parties)?,
            authority,
            lineage,
            path: path.to_owned(),
        })
    }
    /// Return the exclusively owned terminal for restart and test inspection.
    pub fn into_terminal(self) -> H {
        self.durable.into_terminal()
    }
}
impl<H: Terminal> Endpoint for Application<'_, '_, H> {
    fn snapshot(&mut self, principal: Hash32) -> Result<Vec<u8>, Refusal> {
        let context = self
            .durable
            .context(principal)
            .ok_or(Refusal::Binding)?
            .to_vec();
        let history = self
            .lineage
            .audit_history_read_only(&self.path)
            .map_err(failed)?;
        let values = crate::session::root_numbers(
            self.authority.descriptor().state,
            history.snapshot.state(),
        )
        .map_err(failed)?;
        let context = crate::session::root_numbers(self.authority.descriptor().context, &context)
            .map_err(failed)?;
        encoded(
            json!({"contract":hex(RustCryptoSha256::hash(self.authority.identity()).as_bytes()),"principal":hex(principal.as_bytes()),"revision":history.snapshot.version().to_string(),"values":strings(&values),"context":strings(&context),"pending":history.snapshot.pending().to_string()}),
        )
    }
    fn preview(&mut self, request: &Command) -> Result<Vec<u8>, Refusal> {
        let context = self
            .durable
            .context(request.principal())
            .ok_or(Refusal::Binding)?
            .to_vec();
        let history = self
            .lineage
            .audit_history_read_only(&self.path)
            .map_err(failed)?;
        if request.expected() != history.snapshot.version() {
            return Err(Refusal::Conflict);
        }
        let values: Value =
            serde_json::from_slice(request.bytes()).map_err(|_| Refusal::Malformed)?;
        let inputs = numbers(&values)?;
        let frames =
            crate::wire(self.authority.descriptor(), &inputs).map_err(|_| Refusal::Malformed)?;
        if frames[0] != history.snapshot.state() || frames[2] != context {
            return Err(Refusal::Binding);
        }
        encoded(
            json!({"reply":decision(self.authority, &frames[0], &frames[1], &context)?,"command":hex(&frames[1])}),
        )
    }
    fn execute(&mut self, request: &Command, sink: &mut dyn TraceSink) -> Result<Vec<u8>, Refusal> {
        let packet = self.durable.execute(request, sink)?;
        let history = self
            .lineage
            .audit_history_read_only(&self.path)
            .map_err(failed)?;
        let original = history
            .commits
            .iter()
            .find(|entry| entry.replay_id == request.replay_key())
            .ok_or(Refusal::Binding)?;
        encoded(
            json!({"packet":hex(&packet),"revision":original.sequence.to_string(),"reply":decision(self.authority, &original.prestate, &original.command, &original.context)?}),
        )
    }
}
/// Supplied real time for the checked bounded retry programs, in milliseconds.
pub struct Clock(Instant);
impl Default for Clock {
    fn default() -> Self {
        Self(Instant::now())
    }
}
impl Time for Clock {
    fn clock(&mut self, epoch: u64) -> Result<EpochTick, Fault> {
        Ok(EpochTick {
            epoch,
            tick: u64::try_from(self.0.elapsed().as_millis()).map_err(|_| Fault::Io)?,
        })
    }
    fn random(&mut self, _: u64) -> Result<u64, Fault> {
        Ok(0)
    }
    fn wait(&mut self, deadline: EpochTick) -> Result<EpochTick, WaitError> {
        let now = self.clock(deadline.epoch).map_err(WaitError::Failed)?;
        std::thread::sleep(Duration::from_millis(
            deadline.tick.saturating_sub(now.tick),
        ));
        self.clock(deadline.epoch).map_err(WaitError::Failed)
    }
}
/// Local file receiver. One trusted writer; durable IDs deduplicate exact payloads.
pub struct FileReceiver {
    directory: PathBuf,
    sequence: u64,
}
impl FileReceiver {
    /// Open a dedicated receiver directory.
    pub fn new(directory: &Path) -> std::io::Result<Self> {
        fs::create_dir_all(directory)?;
        Ok(Self {
            directory: directory.to_owned(),
            sequence: 0,
        })
    }
}
impl FileReceiver {
    // Write one file atomically: a private temporary file, synced, then renamed.
    fn write(&mut self, path: &Path, bytes: &[u8]) -> std::io::Result<()> {
        self.sequence = self
            .sequence
            .checked_add(1)
            .ok_or_else(|| std::io::Error::other("receiver exhausted"))?;
        let temporary =
            self.directory
                .join(format!(".pending-{}-{}", std::process::id(), self.sequence));
        let mut file = fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&temporary)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        drop(file);
        fs::rename(&temporary, path)?;
        fs::File::open(&self.directory)?.sync_all()
    }
}
/// The receiver's description of one delivery, kept beside its payload as
/// `<delivery ID>.json`: commit position, lane, ordinal and destination. The
/// effect port's record carries no channel; `deliver --to FILE` reports it.
pub fn envelope(delivery: &Delivery) -> std::io::Result<Vec<u8>> {
    // A canonical text destination is its tag, a 32-bit big-endian length and ASCII.
    let text = delivery.destination.split_first().and_then(|(tag, rest)| {
        let (length, text) = rest.split_at_checked(4)?;
        let length = usize::try_from(u32::from_be_bytes(length.try_into().ok()?)).ok()?;
        (*tag == zeno_fcis_value::zcve::TAG_TEXT
            && length == text.len()
            && text.iter().all(|b| b.is_ascii_graphic() || *b == b' '))
        .then(|| String::from_utf8_lossy(text).into_owned())
    });
    let mut bytes = serde_json::to_vec(&json!({
        "format": "zeno-fcis/file-receiver-envelope/1",
        "delivery": hex(delivery.id.as_bytes()),
        "commit": delivery.version,
        "lane": delivery.lane,
        "ordinal": delivery.ordinal,
        "destination": hex(&delivery.destination),
        "destination_text": text,
        "payload_sha256": hex(delivery.payload_sha256.as_bytes()),
    }))
    .map_err(std::io::Error::other)?;
    bytes.push(b'\n');
    Ok(bytes)
}
impl Transport for FileReceiver {
    fn send(&mut self, delivery: &Delivery) -> Sent {
        let name = hex(delivery.id.as_bytes());
        let destination = self.directory.join(&name);
        let described = self.directory.join(format!("{name}.json"));
        let result = (|| -> std::io::Result<()> {
            let envelope = envelope(delivery)?;
            if destination.exists() {
                if fs::read(&destination)? != delivery.payload {
                    return Err(std::io::Error::other("delivery collision"));
                }
                // A payload kept before envelopes existed gains its envelope.
                return match fs::read(&described) {
                    Ok(kept) if kept == envelope => Ok(()),
                    Ok(_) => Err(std::io::Error::other("delivery collision")),
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                        self.write(&described, &envelope)
                    }
                    Err(error) => Err(error),
                };
            }
            // The envelope lands first; the payload file marks a complete delivery.
            self.write(&described, &envelope)?;
            self.write(&destination, &delivery.payload)
        })();
        match result {
            Ok(()) => Sent::Sent(Report {
                delivery_id: delivery.id,
                payload_sha256: delivery.payload_sha256,
            }),
            Err(_) => Sent::Unknown(Fault::Io),
        }
    }
}
struct Log;
impl TraceSink for Log {
    fn record(&mut self, trace: Trace) {
        eprintln!("{trace}");
    }
}
fn bounded(path: &Path) -> AppResult<Vec<u8>> {
    let mut data = Vec::new();
    fs::File::open(path)
        .and_then(|f| f.take(65537).read_to_end(&mut data))
        .map_err(|_| "configuration could not be read")?;
    if data.len() > 65536 {
        return Err("configuration exceeds limit".into());
    }
    Ok(data)
}
fn credential(path: &Path) -> AppResult<[u8; 32]> {
    let text = bounded(path)?;
    let secret: [u8; 32] = unhex(
        std::str::from_utf8(&text)
            .map_err(|_| "credential encoding")?
            .trim_end_matches('\n'),
    )?
    .try_into()
    .map_err(|_| "credential width")?;
    if secret == [0; 32] {
        return Err("credential is zero".into());
    }
    Ok(secret)
}
fn context_values(path: &Path) -> AppResult<Vec<i128>> {
    let context: Value = serde_json::from_slice(&bounded(path)?).map_err(|_| "context encoding")?;
    numbers(&context).map_err(|_| "context values".into())
}
/// Most parties one server binds, as the ingress allows.
pub const MAX_PARTIES: usize = zeno_fcis_ingress::MAX_PARTIES;
/// One configured party: its credential authenticates its principal, and its
/// context values decide every request it makes. Deliberately no Debug.
pub struct Party {
    /// Unique name from the parties file.
    pub name: String,
    secret: [u8; 32],
    /// Principal bound to the credential.
    pub principal: Hash32,
    /// Context values, in the form of `context.json`.
    pub context: Vec<i128>,
}
/// Read a `zeno-fcis/app-parties/1` file. Each entry names a party, its private
/// credential file, its 64-digit principal and its context file; relative files
/// resolve against the parties file's directory. Refuses 0 or more than 64
/// parties, unknown keys, unreadable or oversized files and duplicate names,
/// credentials or principals.
pub fn load_parties(path: &Path) -> AppResult<Vec<Party>> {
    let file: Value = serde_json::from_slice(&bounded(path)?).map_err(|_| "parties encoding")?;
    let file = file.as_object().ok_or("parties encoding")?;
    if file.len() != 2 || file.get("schema") != Some(&json!("zeno-fcis/app-parties/1")) {
        return Err("parties file needs schema zeno-fcis/app-parties/1 and parties".into());
    }
    let entries = file
        .get("parties")
        .and_then(Value::as_array)
        .ok_or("parties list")?;
    if entries.is_empty() || entries.len() > MAX_PARTIES {
        return Err(format!("parties file lists 1 to {MAX_PARTIES} parties"));
    }
    let base = path.parent().unwrap_or_else(|| Path::new("."));
    let mut parties: Vec<Party> = Vec::new();
    for entry in entries {
        let entry = entry.as_object().ok_or("party entry")?;
        let text = |key: &str| entry.get(key).and_then(Value::as_str);
        let (Some(name), Some(secret), Some(principal), Some(context), 4) = (
            text("name"),
            text("credential"),
            text("principal"),
            text("context"),
            entry.len(),
        ) else {
            return Err("each party has exactly name, credential, principal and context".into());
        };
        if name.is_empty()
            || name.len() > 32
            || !name
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
        {
            return Err("party names are 1 to 32 letters, digits, - or _".into());
        }
        let secret = credential(&base.join(secret)).map_err(|e| format!("party {name}: {e}"))?;
        let principal: [u8; 32] = unhex(principal)
            .ok()
            .and_then(|bytes| bytes.try_into().ok())
            .ok_or_else(|| format!("party {name}: principal is 64 lowercase hexadecimal digits"))?;
        let context =
            context_values(&base.join(context)).map_err(|e| format!("party {name}: {e}"))?;
        for old in &parties {
            if old.name == name {
                return Err(format!("party name {name} is repeated"));
            }
            if old.principal.as_bytes() == &principal {
                return Err(format!("parties {} and {name} share a principal", old.name));
            }
            if old.secret == secret {
                return Err(format!(
                    "parties {} and {name} share a credential",
                    old.name
                ));
            }
        }
        parties.push(Party {
            name: name.to_owned(),
            secret,
            principal: Hash32::new(principal),
            context,
        });
    }
    Ok(parties)
}
/// The ingress configuration for these parties at a loopback origin.
pub fn ingress_config(parties: &[Party], contract: Hash32, origin: String) -> AppResult<Config> {
    Config::with_parties(
        parties.iter().map(|p| (p.secret, p.principal)).collect(),
        contract,
        origin,
        Duration::from_secs(10),
    )
    .map_err(|_| "bind ingress".into())
}
/// Parse the listening address; cleartext HTTP is confined to loopback.
pub fn listen_address(text: &str) -> AppResult<std::net::SocketAddr> {
    let address: std::net::SocketAddr = text.parse().map_err(|_| "address")?;
    if !address.ip().is_loopback() {
        return Err("loopback required".into());
    }
    Ok(address)
}
// ZENO_APP_PARTIES, or the single credential, principal and context variables.
fn configured_parties() -> AppResult<Vec<Party>> {
    let Some(file) = std::env::var_os("ZENO_APP_PARTIES") else {
        let secret_file =
            std::env::var_os("ZENO_APP_SECRET_FILE").ok_or("set ZENO_APP_SECRET_FILE")?;
        let principal = match std::env::var("ZENO_APP_PRINCIPAL") {
            Ok(value) => Hash32::new(unhex(&value)?.try_into().map_err(|_| "principal width")?),
            Err(_) => Hash32::new([7; 32]),
        };
        let context = std::env::var_os("ZENO_APP_CONTEXT").unwrap_or_else(|| "context.json".into());
        return Ok(vec![Party {
            name: "default".into(),
            secret: credential(Path::new(&secret_file))?,
            principal,
            context: context_values(Path::new(&context))?,
        }]);
    };
    if [
        "ZENO_APP_SECRET_FILE",
        "ZENO_APP_PRINCIPAL",
        "ZENO_APP_CONTEXT",
    ]
    .iter()
    .any(|name| std::env::var_os(name).is_some())
    {
        return Err("ZENO_APP_PARTIES replaces ZENO_APP_SECRET_FILE, ZENO_APP_PRINCIPAL and ZENO_APP_CONTEXT; set one form".into());
    }
    load_parties(Path::new(&file))
}
/// Start the loopback application. A private credential file, or a parties file,
/// is required. The existing store is reopened; a new store receives checked genesis.
pub fn run() -> AppResult<()> {
    let parties = configured_parties()?;
    let path =
        PathBuf::from(std::env::var_os("ZENO_APP_STORE").unwrap_or_else(|| "app.sqlite".into()));
    let receiver =
        PathBuf::from(std::env::var_os("ZENO_APP_RECEIVER").unwrap_or_else(|| "deliveries".into()));
    let address = listen_address(
        &std::env::var("ZENO_APP_ADDRESS").unwrap_or_else(|_| "127.0.0.1:8080".into()),
    )?;
    crate::session::with_lineage(|lineage| {
        let authority = lineage.authorities().last().ok_or("empty lineage")?;
        if !path.exists() {
            let state = crate::genesis()?;
            let PublicationOutcome::Commit(genesis) = authority.publish_genesis(&state) else {
                return Err("genesis refused".into());
            };
            drop(V2SqliteShell::create(&path, authority, genesis).map_err(|_| "create store")?);
        }
        let bundle: Value =
            serde_json::from_slice(include_bytes!("../www/app.json")).map_err(|_| "UI bundle")?;
        let fields = bundle["description"]["fields"]
            .as_array()
            .ok_or("UI fields")?;
        let mut input = Vec::new();
        for f in fields {
            input.push(
                f["initial"]
                    .as_str()
                    .ok_or("initial field")?
                    .parse::<i128>()
                    .map_err(|_| "initial number")?,
            );
        }
        let context_width = fields.iter().filter(|f| f["source"] == "context").count();
        if context_width > input.len() {
            return Err("context width".into());
        }
        let offset = input.len() - context_width;
        let mut table = Vec::new();
        for party in &parties {
            if party.context.len() != context_width {
                return Err(format!("party {}: context width", party.name));
            }
            input[offset..].copy_from_slice(&party.context);
            let framed = crate::wire(authority.descriptor(), &input).map_err(|_| {
                format!("party {}: context outside its declared domain", party.name)
            })?;
            table.push((party.principal, framed[2].clone()));
        }
        let production = Production::open(
            &path,
            lineage,
            1,
            FileReceiver::new(&receiver).map_err(|_| "receiver")?,
            Some(Clock::default()),
        )
        .map_err(|_| "open store")?;
        let store = production.store();
        let endpoint = Application::with_parties(lineage, production, store, &path, table)
            .map_err(|_| "bind shell")?;
        let config = ingress_config(
            &parties,
            RustCryptoSha256::hash(authority.identity()),
            format!("http://{address}"),
        )?;
        let assets = (|| -> Result<Vec<Asset>, Refusal> { Ok(include!("app_assets.rs")) })()
            .map_err(|_| "assets")?;
        let mut server = Server::new(config, endpoint, Log)
            .with_assets(assets)
            .map_err(|_| "asset routes")?;
        let runtime = zeno_fcis_ingress::runtime().map_err(|_| "runtime")?;
        runtime.block_on(async {
            let listener = tokio::net::TcpListener::bind(address)
                .await
                .map_err(|_| "listen")?;
            println!("Application available at http://{address}");
            loop {
                if server.serve_one(&listener).await.is_err() {
                    eprintln!("connection refused or interrupted");
                }
            }
        })
    })
}
