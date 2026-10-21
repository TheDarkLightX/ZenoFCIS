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
/// Exclusive shell endpoint. The copied principal context cannot be selected by HTTP.
pub struct Application<'a, 'p, H> {
    durable: Durable<'a, 'p, H>,
    authority: &'a Authority<'p>,
    lineage: &'a Lineage<'a, 'p>,
    path: PathBuf,
    principal: Hash32,
    context: Vec<u8>,
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
        let authority = lineage.authorities().last().ok_or(Refusal::Binding)?;
        Ok(Self {
            durable: Durable::new(authority, terminal, store, principal, context)?,
            authority,
            lineage,
            path: path.to_owned(),
            principal,
            context: context.to_vec(),
        })
    }
    /// Return the exclusively owned terminal for restart and test inspection.
    pub fn into_terminal(self) -> H {
        self.durable.into_terminal()
    }
}
impl<H: Terminal> Endpoint for Application<'_, '_, H> {
    fn snapshot(&mut self, principal: Hash32) -> Result<Vec<u8>, Refusal> {
        if principal != self.principal {
            return Err(Refusal::Binding);
        }
        let history = self
            .lineage
            .audit_history_read_only(&self.path)
            .map_err(failed)?;
        let values = crate::session::root_numbers(
            self.authority.descriptor().state,
            history.snapshot.state(),
        )
        .map_err(failed)?;
        let context =
            crate::session::root_numbers(self.authority.descriptor().context, &self.context)
                .map_err(failed)?;
        encoded(
            json!({"contract":hex(RustCryptoSha256::hash(self.authority.identity()).as_bytes()),"principal":hex(principal.as_bytes()),"revision":history.snapshot.version().to_string(),"values":strings(&values),"context":strings(&context),"pending":history.snapshot.pending().to_string()}),
        )
    }
    fn preview(&mut self, request: &Command) -> Result<Vec<u8>, Refusal> {
        if request.principal() != self.principal {
            return Err(Refusal::Binding);
        }
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
        if frames[0] != history.snapshot.state() || frames[2] != self.context {
            return Err(Refusal::Binding);
        }
        encoded(
            json!({"reply":decision(self.authority, &frames[0], &frames[1], &self.context)?,"command":hex(&frames[1])}),
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
impl Transport for FileReceiver {
    fn send(&mut self, delivery: &Delivery) -> Sent {
        let destination = self.directory.join(hex(delivery.id.as_bytes()));
        let result = (|| -> std::io::Result<()> {
            if destination.exists() {
                if fs::read(&destination)? == delivery.payload {
                    return Ok(());
                }
                return Err(std::io::Error::other("delivery collision"));
            }
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
            file.write_all(&delivery.payload)?;
            file.sync_all()?;
            drop(file);
            fs::rename(&temporary, &destination)?;
            fs::File::open(&self.directory)?.sync_all()
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
/// Start the loopback application. A private credential file is required.
/// The existing store is reopened; a new store receives checked genesis.
pub fn run() -> AppResult<()> {
    let secret_file = std::env::var_os("ZENO_APP_SECRET_FILE").ok_or("set ZENO_APP_SECRET_FILE")?;
    let secret_text = bounded(Path::new(&secret_file))?;
    let secret: [u8; 32] = unhex(
        std::str::from_utf8(&secret_text)
            .map_err(|_| "credential encoding")?
            .trim_end_matches('\n'),
    )?
    .try_into()
    .map_err(|_| "credential width")?;
    let principal = match std::env::var("ZENO_APP_PRINCIPAL") {
        Ok(value) => Hash32::new(unhex(&value)?.try_into().map_err(|_| "principal width")?),
        Err(_) => Hash32::new([7; 32]),
    };
    let path =
        PathBuf::from(std::env::var_os("ZENO_APP_STORE").unwrap_or_else(|| "app.sqlite".into()));
    let receiver =
        PathBuf::from(std::env::var_os("ZENO_APP_RECEIVER").unwrap_or_else(|| "deliveries".into()));
    let context_path = PathBuf::from(
        std::env::var_os("ZENO_APP_CONTEXT").unwrap_or_else(|| "context.json".into()),
    );
    let context: Value =
        serde_json::from_slice(&bounded(&context_path)?).map_err(|_| "context encoding")?;
    let context = numbers(&context).map_err(|_| "context values")?;
    let address: std::net::SocketAddr = std::env::var("ZENO_APP_ADDRESS")
        .unwrap_or_else(|_| "127.0.0.1:8080".into())
        .parse()
        .map_err(|_| "address")?;
    if !address.ip().is_loopback() {
        return Err("loopback required".into());
    }
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
        if context.len() != context_width {
            return Err("context width".into());
        }
        if context_width > input.len() {
            return Err("context width".into());
        }
        let offset = input.len() - context_width;
        input[offset..].copy_from_slice(&context);
        let framed = crate::wire(authority.descriptor(), &input)?;
        let production = Production::open(
            &path,
            lineage,
            1,
            FileReceiver::new(&receiver).map_err(|_| "receiver")?,
            Some(Clock::default()),
        )
        .map_err(|_| "open store")?;
        let store = production.store();
        let endpoint = Application::new(lineage, production, store, &path, principal, &framed[2])
            .map_err(|_| "bind shell")?;
        let origin = format!("http://{address}");
        let config = Config::new(
            secret,
            principal,
            RustCryptoSha256::hash(authority.identity()),
            origin,
            Duration::from_secs(10),
        )
        .map_err(|_| "bind ingress")?;
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
