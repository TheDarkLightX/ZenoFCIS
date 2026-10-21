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
    V2InputLeaf as InputLeaf,
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
/// The server clock: one context field the server sets at admission, counted
/// in days or minutes since a UTC epoch date, within the field's declared range.
pub struct ServerClock {
    position: usize,
    unit: i128,
    epoch: i128,
    range: (i128, i128),
    source: Option<PathBuf>,
}
impl ServerClock {
    /// The clock `v2/policy.json` declares, if any. `source` names a file holding
    /// the current time in the clock's units, a test and operator tool read at
    /// every admission; without it the server reads the system time.
    pub fn declared(source: Option<PathBuf>) -> AppResult<Option<Self>> {
        let policy: Value = serde_json::from_slice(include_bytes!("../v2/policy.json"))
            .map_err(|_| "policy encoding")?;
        let Some(clock) = policy.get("clock") else {
            return Ok(None);
        };
        let bundle: Value =
            serde_json::from_slice(include_bytes!("../www/app.json")).map_err(|_| "UI bundle")?;
        let fields = bundle["description"]["fields"]
            .as_array()
            .ok_or("UI fields")?;
        let context: Vec<&Value> = fields.iter().filter(|f| f["source"] == "context").collect();
        let position = context
            .iter()
            .position(|f| f["field"] == clock["field"] && f["domain"]["kind"] == "int")
            .ok_or("the clock field must be an integer context field")?;
        let bound = |key: &str| {
            context[position]["domain"][key]
                .as_str()
                .and_then(|text| text.parse::<i128>().ok())
                .ok_or("clock range")
        };
        let unit = match clock["unit"].as_str() {
            Some("days") => 86_400,
            Some("minutes") => 60,
            _ => return Err("clock unit".into()),
        };
        let epoch = clock["epoch"].as_str().ok_or("clock epoch")?;
        let date = |range: std::ops::Range<usize>| {
            epoch
                .get(range)
                .and_then(|part| part.parse::<i128>().ok())
                .ok_or("clock epoch")
        };
        let (year, month, day) = (date(0..4)?, date(5..7)?, date(8..10)?);
        // Days from 1970-01-01 to a proleptic Gregorian date.
        let shifted = if month <= 2 { year - 1 } else { year };
        let era = shifted.div_euclid(400);
        let of_era = shifted - era * 400;
        let of_year = (153 * ((month + 9) % 12) + 2) / 5 + day - 1;
        let of_cycle = of_era * 365 + of_era / 4 - of_era / 100 + of_year;
        Ok(Some(Self {
            position,
            unit,
            epoch: (era * 146_097 + of_cycle - 719_468) * 86_400,
            range: (bound("min")?, bound("max")?),
            source,
        }))
    }
    /// Index of the clock among the context values.
    pub const fn position(&self) -> usize {
        self.position
    }
    fn now(&self) -> Result<i128, Refusal> {
        match &self.source {
            // One canonical decimal, as every other number, and at most one
            // final newline.
            Some(file) => std::str::from_utf8(&bounded(file).map_err(|_| Refusal::Unavailable)?)
                .ok()
                .map(|text| text.strip_suffix('\n').unwrap_or(text))
                .and_then(|text| text.parse::<i128>().ok().filter(|n| n.to_string() == text))
                .ok_or(Refusal::Unavailable),
            None => {
                let seconds = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map_err(|_| Refusal::Unavailable)?
                    .as_secs();
                Ok((i128::from(seconds) - self.epoch).div_euclid(self.unit))
            }
        }
    }
    // Never earlier than the last committed time; refused outside the range.
    fn admitted(&self, last: Option<i128>) -> Result<i128, Refusal> {
        let time = self.now()?.max(last.unwrap_or(i128::MIN));
        if time < self.range.0 || time > self.range.1 {
            return Err(Refusal::Clock);
        }
        Ok(time)
    }
}
fn context_numbers(authority: &Authority<'_>, context: &[u8]) -> Result<Vec<i128>, Refusal> {
    crate::session::root_numbers(authority.descriptor().context, context).map_err(failed)
}
fn clock_of(
    authority: &Authority<'_>,
    clock: &ServerClock,
    context: &[u8],
) -> Result<i128, Refusal> {
    context_numbers(authority, context)?
        .get(clock.position)
        .copied()
        .ok_or(Refusal::Binding)
}
// The configured context with its clock field set to `time`.
fn context_at(
    authority: &Authority<'_>,
    clock: &ServerClock,
    configured: &[u8],
    time: i128,
) -> Result<Vec<u8>, Refusal> {
    let mut values = context_numbers(authority, configured)?;
    *values.get_mut(clock.position).ok_or(Refusal::Binding)? = time;
    let value =
        crate::root_value(authority.descriptor().context, &values).map_err(|_| Refusal::Binding)?;
    crate::frame(&crate::v2_contract::FRAMING.context, value).map_err(|_| Refusal::Binding)
}
// The context a new request is decided with: the clock is read now, never earlier
// than the last committed time.
fn admit(
    authority: &Authority<'_>,
    clock: Option<&ServerClock>,
    last: Option<i128>,
    configured: &[u8],
) -> Result<Vec<u8>, Refusal> {
    match clock {
        Some(clock) => context_at(authority, clock, configured, clock.admitted(last)?),
        None => Ok(configured.to_vec()),
    }
}
// A context belongs to the party when it is the configured one, apart from the
// clock time it records.
fn same_party(
    authority: &Authority<'_>,
    clock: Option<&ServerClock>,
    configured: &[u8],
    stored: &[u8],
) -> bool {
    match clock {
        Some(clock) => clock_of(authority, clock, stored)
            .and_then(|time| context_at(authority, clock, configured, time))
            .is_ok_and(|expected| expected == stored),
        None => configured == stored,
    }
}
// A party's view hides state values as null. This filters what is displayed;
// decisions, refusals and reply packets can still reveal hidden values.
fn hide(view: Option<&[bool]>, values: &mut [Value]) {
    for (value, seen) in values.iter_mut().zip(view.unwrap_or_default()) {
        if !seen {
            *value = Value::Null;
        }
    }
}
fn masked(view: Option<&[bool]>, mut reply: Value) -> Value {
    for key in ["input", "post"] {
        if let Some(values) = reply[key].as_array_mut() {
            hide(view, values);
        }
    }
    reply
}
/// Exclusive shell endpoint. The authenticated principal alone selects its copied
/// context; HTTP cannot select one. With a server clock, the server sets that
/// context's clock field when it admits a new request.
pub struct Application<'a, 'p, H> {
    durable: Durable<'a, 'p, H>,
    authority: &'a Authority<'p>,
    lineage: &'a Lineage<'a, 'p>,
    path: PathBuf,
    clock: Option<ServerClock>,
    last: Option<i128>,
    views: Vec<(Hash32, Vec<bool>)>,
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
            clock: None,
            last: None,
            views: Vec::new(),
        })
    }
    /// Set the clock field at admission. The latest committed time, read back
    /// from the store's audited history, is the earliest time admitted after a
    /// restart.
    pub fn with_clock(mut self, clock: ServerClock) -> Result<Self, Refusal> {
        let history = self
            .lineage
            .audit_history_read_only(&self.path)
            .map_err(failed)?;
        for commit in &history.commits {
            let time = clock_of(self.authority, &clock, &commit.context)?;
            self.last = Some(self.last.map_or(time, |last| last.max(time)));
        }
        self.clock = Some(clock);
        Ok(self)
    }
    /// Show each listed principal only the state fields its mask marks; other
    /// principals see every field.
    #[must_use]
    pub fn with_views(mut self, views: Vec<(Hash32, Vec<bool>)>) -> Self {
        self.views = views;
        self
    }
    /// Return the exclusively owned terminal for restart and test inspection.
    pub fn into_terminal(self) -> H {
        self.durable.into_terminal()
    }
    // The authenticated principal's view; never chosen by the request.
    fn view(&self, principal: Hash32) -> Option<&[bool]> {
        self.views
            .iter()
            .find(|(owner, _)| *owner == principal)
            .map(|(_, mask)| mask.as_slice())
    }
}
impl<H: Terminal> Endpoint for Application<'_, '_, H> {
    fn snapshot(&mut self, principal: Hash32) -> Result<Vec<u8>, Refusal> {
        let configured = self.durable.context(principal).ok_or(Refusal::Binding)?;
        let context = admit(self.authority, self.clock.as_ref(), self.last, configured)?;
        let history = self
            .lineage
            .audit_history_read_only(&self.path)
            .map_err(failed)?;
        let values = crate::session::root_numbers(
            self.authority.descriptor().state,
            history.snapshot.state(),
        )
        .map_err(failed)?;
        let context = context_numbers(self.authority, &context)?;
        let mut values: Vec<Value> = strings(&values).into_iter().map(Value::from).collect();
        hide(self.view(principal), &mut values);
        encoded(
            json!({"contract":hex(RustCryptoSha256::hash(self.authority.identity()).as_bytes()),"principal":hex(principal.as_bytes()),"revision":history.snapshot.version().to_string(),"values":values,"context":strings(&context),"pending":history.snapshot.pending().to_string()}),
        )
    }
    fn preview(&mut self, request: &Command) -> Result<Vec<u8>, Refusal> {
        let configured = self
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
        let mut values: Value =
            serde_json::from_slice(request.bytes()).map_err(|_| Refusal::Malformed)?;
        let view = self.view(request.principal());
        // The server supplies hidden state values, whatever the request holds there.
        let state = crate::session::root_numbers(
            self.authority.descriptor().state,
            history.snapshot.state(),
        )
        .map_err(failed)?;
        let items = values.as_array_mut().ok_or(Refusal::Malformed)?;
        if let Some(view) = view {
            for ((item, seen), actual) in items.iter_mut().zip(view).zip(&state) {
                if !seen {
                    *item = Value::from(actual.to_string());
                }
            }
        }
        let inputs = numbers(&values)?;
        let mut frames =
            crate::wire(self.authority.descriptor(), &inputs).map_err(|_| Refusal::Malformed)?;
        let clock = self.clock.as_ref();
        if frames[0] != history.snapshot.state()
            || !same_party(self.authority, clock, &configured, &frames[2])
        {
            return Err(Refusal::Binding);
        }
        // The server, not the request, sets the clock.
        frames[2] = admit(self.authority, clock, self.last, &configured)?;
        let reply = masked(
            view,
            decision(self.authority, &frames[0], &frames[1], &frames[2])?,
        );
        encoded(json!({"reply":reply,"command":hex(&frames[1])}))
    }
    fn execute(&mut self, request: &Command, sink: &mut dyn TraceSink) -> Result<Vec<u8>, Refusal> {
        let (authority, clock, last) = (self.authority, self.clock.as_ref(), self.last);
        let path = self.path.as_path();
        let mut journaled = None;
        let result = self.durable.execute_admitted(
            request,
            &mut |configured| {
                let context = admit(authority, clock, last, configured)?;
                // A new decision's submission is on disk before its commit.
                let line = Submission {
                    sequence: request.expected().checked_add(1).ok_or(Refusal::Conflict)?,
                    command: crate::session::root_numbers(
                        authority.descriptor().command,
                        request.bytes(),
                    )
                    .map_err(|_| Refusal::Malformed)?,
                    context: context_numbers(authority, &context)?,
                };
                journaled = journal(path, request.expected(), &line).map_err(failed)?;
                Ok(context)
            },
            &|configured, stored| same_party(authority, clock, configured, stored),
            sink,
        );
        let history = self
            .lineage
            .audit_history_read_only(&self.path)
            .map_err(failed)?;
        // A line whose commit did not happen is removed again.
        if let Some((file, kept, sequence)) = journaled
            && history.snapshot.version() < sequence
        {
            file.set_len(kept)
                .and_then(|()| file.sync_all())
                .map_err(failed)?;
        }
        let packet = result?;
        let original = history
            .commits
            .iter()
            .find(|entry| entry.replay_id == request.replay_key())
            .ok_or(Refusal::Binding)?;
        if let Some(clock) = &self.clock {
            let time = clock_of(self.authority, clock, &original.context)?;
            self.last = Some(self.last.map_or(time, |last| last.max(time)));
        }
        let reply = masked(
            self.view(request.principal()),
            decision(
                self.authority,
                &original.prestate,
                &original.command,
                &original.context,
            )?,
        );
        encoded(
            json!({"packet":hex(&packet),"revision":original.sequence.to_string(),"reply":reply}),
        )
    }
}
// One line of the submission journal that the generated command line's
// `history` reads: `SEQUENCE CONTRACT_VERSION | COMMAND | CONTEXT`.
struct Submission {
    sequence: u64,
    command: Vec<i128>,
    context: Vec<i128>,
}
impl Submission {
    fn line(&self) -> String {
        let joined = |numbers: &[i128]| strings(numbers).join(" ");
        format!(
            "{} {} | {} | {}\n",
            self.sequence,
            crate::v2_contract::VERSION,
            joined(&self.command),
            joined(&self.context)
        )
    }
}
// Under the journal's exclusive lock, held until the commit's outcome: remove
// any line past the store's head, left by a stop before a commit, then append
// and synchronize this submission. A store with commits but no journal keeps
// none, since its earlier commits cannot be recorded.
fn journal(
    store: &Path,
    head: u64,
    submission: &Submission,
) -> std::io::Result<Option<(fs::File, u64, u64)>> {
    use std::io::Seek as _;
    let path = crate::journal_path(store);
    let open = || fs::OpenOptions::new().read(true).append(true).open(&path);
    let mut file = match open() {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound && head == 0 => {
            fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&path)?
                .sync_all()?;
            fs::File::open(
                path.parent()
                    .filter(|p| !p.as_os_str().is_empty())
                    .unwrap_or(Path::new(".")),
            )?
            .sync_all()?;
            open()?
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        other => other?,
    };
    file.lock()?;
    let mut text = String::new();
    file.seek(std::io::SeekFrom::Start(0))?;
    file.read_to_string(&mut text)?;
    let mut kept = 0;
    for line in text.split_inclusive('\n') {
        let Some(complete) = line.strip_suffix('\n') else {
            break;
        };
        let sequence: u64 = complete
            .split_whitespace()
            .next()
            .and_then(|word| word.parse().ok())
            .ok_or_else(|| std::io::Error::other("malformed submission journal"))?;
        if sequence > head {
            break;
        }
        kept += line.len();
    }
    let kept = u64::try_from(kept).map_err(std::io::Error::other)?;
    file.set_len(kept)?;
    file.write_all(submission.line().as_bytes())?;
    file.sync_all()?;
    Ok(Some((file, kept, submission.sequence)))
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
/// A delivery handler that runs one configured program per delivery, directly,
/// with no shell, an empty environment apart from `PATH`, and discarded output.
/// Its standard input is the delivery's envelope with `payload` added in
/// hexadecimal. Exit 0 acknowledges; any other exit, a failure to start or the
/// time limit leaves the delivery pending for the outbox's retry, so delivery
/// is at least once and the program must treat the delivery ID as its
/// idempotency key.
pub struct ProgramConnector {
    argv: Vec<String>,
    timeout: Duration,
}
impl ProgramConnector {
    /// Read a `zeno-fcis/app-connector/1` file: `argv`, an absolute program
    /// path and its arguments, at most 64 entries of at most 4,096 bytes, and
    /// `timeout_seconds`, from 1 to 60.
    pub fn load(path: &Path) -> AppResult<Self> {
        let file: Value =
            serde_json::from_slice(&bounded(path)?).map_err(|_| "connector encoding")?;
        let file = file.as_object().ok_or("connector encoding")?;
        let argv: Option<Vec<String>> =
            file.get("argv").and_then(Value::as_array).and_then(|argv| {
                argv.iter()
                    .map(|arg| arg.as_str().map(str::to_owned))
                    .collect()
            });
        let timeout = file.get("timeout_seconds").and_then(Value::as_u64);
        let (3, Some(Some("zeno-fcis/app-connector/1")), Some(argv), Some(timeout @ 1..=60)) = (
            file.len(),
            file.get("schema").map(Value::as_str),
            argv,
            timeout,
        ) else {
            return Err(
                "the connector file has exactly schema zeno-fcis/app-connector/1, argv and timeout_seconds from 1 to 60"
                    .into(),
            );
        };
        if argv.is_empty()
            || argv.len() > 64
            || argv
                .iter()
                .any(|arg| arg.len() > 4096 || arg.contains('\0'))
            || !Path::new(&argv[0]).is_absolute()
        {
            return Err(
                "connector argv is an absolute program path and at most 63 arguments".into(),
            );
        }
        Ok(Self {
            argv,
            timeout: Duration::from_secs(timeout),
        })
    }
    fn run(&self, delivery: &Delivery) -> std::io::Result<bool> {
        let mut value: Value = serde_json::from_slice(&envelope(delivery)?)?;
        value["payload"] = Value::from(hex(&delivery.payload));
        let mut input = serde_json::to_vec(&value)?;
        input.push(b'\n');
        let mut child = std::process::Command::new(&self.argv[0])
            .args(&self.argv[1..])
            .env_clear()
            .env("PATH", "/usr/bin:/bin")
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()?;
        // The input is written beside the time limit: a program that never reads
        // it cannot hold the server past the limit once the pipe is full. A
        // program that stops reading early still has its exit status judged.
        if let Some(mut stdin) = child.stdin.take() {
            std::thread::spawn(move || stdin.write_all(&input));
        }
        let deadline = Instant::now() + self.timeout;
        loop {
            if let Some(status) = child.try_wait()? {
                return Ok(status.success());
            }
            if Instant::now() >= deadline {
                let _ = child.kill();
                child.wait()?;
                return Ok(false);
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }
}
impl Transport for ProgramConnector {
    fn send(&mut self, delivery: &Delivery) -> Sent {
        match self.run(delivery) {
            Ok(true) => Sent::Sent(Report {
                delivery_id: delivery.id,
                payload_sha256: delivery.payload_sha256,
            }),
            // The program may have acted before it failed: retry under the same ID.
            _ => Sent::Unknown(Fault::Io),
        }
    }
}
/// The configured delivery transport: the local file receiver, or a program.
pub enum Receiver {
    /// Files under `ZENO_APP_RECEIVER`.
    File(FileReceiver),
    /// The program `ZENO_APP_CONNECTOR` configures.
    Program(ProgramConnector),
}
impl Transport for Receiver {
    fn send(&mut self, delivery: &Delivery) -> Sent {
        match self {
            Self::File(receiver) => receiver.send(delivery),
            Self::Program(connector) => connector.send(delivery),
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
    /// Names of the state fields this party sees; `None` sees every field.
    pub view: Option<Vec<String>>,
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
        let view = match entry.get("view") {
            None => None,
            Some(names) => Some(
                names
                    .as_array()
                    .and_then(|names| {
                        names
                            .iter()
                            .map(|name| name.as_str().map(str::to_owned))
                            .collect::<Option<Vec<_>>>()
                    })
                    .ok_or("a party's view lists state field names")?,
            ),
        };
        let (Some(name), Some(secret), Some(principal), Some(context), true) = (
            text("name"),
            text("credential"),
            text("principal"),
            text("context"),
            entry.len() == 4 + usize::from(view.is_some()),
        ) else {
            return Err(
                "each party has exactly name, credential, principal and context, and may have a view"
                    .into(),
            );
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
            view,
        });
    }
    Ok(parties)
}
// Whether each number lies in its field's declared domain: a declared integer
// range, a listed variant, or 0 or 1.
fn declared(schema: c::Schema<'_>, numbers: &[i128]) -> bool {
    let within = |leaf: &InputLeaf, number: i128| match leaf {
        InputLeaf::Bool => number == 0 || number == 1,
        InputLeaf::I128 { min, max } => (*min..=*max).contains(&number),
        InputLeaf::U128 { min, max } => {
            u128::try_from(number).is_ok_and(|n| (*min..=*max).contains(&n))
        }
        InputLeaf::Sum { variants, .. } => variants.iter().any(|v| i128::from(v.id) == number),
        _ => false,
    };
    match schema {
        c::Schema::Record(fields) => {
            fields.len() == numbers.len()
                && fields
                    .iter()
                    .zip(numbers)
                    .all(|(field, number)| within(&field.leaf, *number))
        }
        c::Schema::Leaf(leaf) => numbers.len() == 1 && within(leaf, numbers[0]),
        _ => false,
    }
}
/// Each party's principal and canonical configured context. With a server clock,
/// a party's context values omit the clock field, which holds the range's lower
/// bound until admission sets it. A value outside its declared domain refuses
/// start-up, rather than every one of that party's requests.
pub fn party_contexts(
    authority: &Authority<'_>,
    parties: &[Party],
    clock: Option<&ServerClock>,
) -> AppResult<Vec<(Hash32, Vec<u8>)>> {
    let schema = authority.descriptor().context;
    let mut table = Vec::new();
    for party in parties {
        let mut context = party.context.clone();
        if let Some(clock) = clock {
            if clock.position > context.len() {
                return Err(format!("party {}: context width", party.name));
            }
            context.insert(clock.position, clock.range.0);
        }
        if context.len() != crate::width(schema)? {
            return Err(format!("party {}: context width", party.name));
        }
        if !declared(schema, &context) {
            return Err(format!(
                "party {}: context outside its declared domain",
                party.name
            ));
        }
        let framed = crate::root_value(schema, &context)
            .and_then(|value| crate::frame(&crate::v2_contract::FRAMING.context, value))
            .map_err(|_| format!("party {}: context outside its declared domain", party.name))?;
        table.push((party.principal, framed));
    }
    Ok(table)
}
/// Each party's view as a mask over the state fields, in field order. Refuses a
/// name that is not a state field or that repeats.
pub fn party_views(parties: &[Party]) -> AppResult<Vec<(Hash32, Vec<bool>)>> {
    let bundle: Value =
        serde_json::from_slice(include_bytes!("../www/app.json")).map_err(|_| "UI bundle")?;
    let names: Vec<&str> = bundle["description"]["fields"]
        .as_array()
        .ok_or("UI fields")?
        .iter()
        .filter(|f| f["source"] == "state")
        .map(|f| f["name"].as_str().unwrap_or_default())
        .collect();
    let mut views = Vec::new();
    for party in parties {
        let mut mask = vec![party.view.is_none(); names.len()];
        for name in party.view.iter().flatten() {
            let index = names
                .iter()
                .position(|field| field == name)
                .ok_or_else(|| format!("party {}: {name} is not a state field", party.name))?;
            if mask[index] {
                return Err(format!(
                    "party {}: {name} is repeated in its view",
                    party.name
                ));
            }
            mask[index] = true;
        }
        views.push((party.principal, mask));
    }
    Ok(views)
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
/// Parse the public HTTPS origin a trusted TLS proxy serves, such as
/// `https://app.example.com`, exactly as browsers send it: lowercase, with a
/// port only when it is not 443, and no path, query or user information.
pub fn public_origin(text: &str) -> AppResult<String> {
    let refused = || -> AppResult<String> {
        Err("ZENO_APP_ORIGIN is an https origin such as https://app.example.com, lowercase, with a port only when it is not 443 and no path".into())
    };
    let Some(authority) = text.strip_prefix("https://") else {
        return refused();
    };
    let (host, port) = match authority.rsplit_once(':') {
        Some((host, port)) => (host, Some(port)),
        None => (authority, None),
    };
    let host_ok = !host.is_empty()
        && host.len() <= 253
        && host
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'.')
        && !host.starts_with(['-', '.'])
        && !host.ends_with(['-', '.']);
    let port_ok = port.is_none_or(|port| {
        !port.starts_with('0')
            && port
                .parse::<u16>()
                .is_ok_and(|number| number != 0 && number != 443)
    });
    if !host_ok || !port_ok {
        return refused();
    }
    Ok(text.to_owned())
}
/// Every setting the server reads. Another `ZENO_APP_` variable refuses
/// start-up, so a misspelt setting never leaves its default silently in force.
pub const SETTINGS: [&str; 10] = [
    "ZENO_APP_ADDRESS",
    "ZENO_APP_CLOCK_FILE",
    "ZENO_APP_CONNECTOR",
    "ZENO_APP_CONTEXT",
    "ZENO_APP_ORIGIN",
    "ZENO_APP_PARTIES",
    "ZENO_APP_PRINCIPAL",
    "ZENO_APP_RECEIVER",
    "ZENO_APP_SECRET_FILE",
    "ZENO_APP_STORE",
];
fn known_settings() -> AppResult<()> {
    for (name, _) in std::env::vars_os() {
        let name = name.to_string_lossy();
        if name.starts_with("ZENO_APP_") && !SETTINGS.contains(&name.as_ref()) {
            return Err(format!(
                "{name:?} is not a setting; the settings are {}",
                SETTINGS.join(", ")
            ));
        }
    }
    // Deliveries go to one place; the receiver would be ignored silently.
    if std::env::var_os("ZENO_APP_CONNECTOR").is_some()
        && std::env::var_os("ZENO_APP_RECEIVER").is_some()
    {
        return Err("ZENO_APP_CONNECTOR replaces ZENO_APP_RECEIVER; set one".into());
    }
    Ok(())
}
// An optional setting: unset is `None`, so its default applies; a value that is
// set must be nonempty text, never silently replaced by the default.
fn setting(name: &str) -> AppResult<Option<String>> {
    match std::env::var_os(name).map(std::ffi::OsString::into_string) {
        None => Ok(None),
        Some(Ok(text)) if !text.is_empty() => Ok(Some(text)),
        Some(_) => Err(format!(
            "{name} is set but empty or not text; unset it to use its default"
        )),
    }
}
// ZENO_APP_PARTIES, or the single credential, principal and context variables.
fn configured_parties() -> AppResult<Vec<Party>> {
    let Some(file) = setting("ZENO_APP_PARTIES")? else {
        let secret_file =
            std::env::var_os("ZENO_APP_SECRET_FILE").ok_or("set ZENO_APP_SECRET_FILE")?;
        let principal = match setting("ZENO_APP_PRINCIPAL")? {
            Some(value) => Hash32::new(unhex(&value)?.try_into().map_err(|_| "principal width")?),
            None => Hash32::new([7; 32]),
        };
        let context = setting("ZENO_APP_CONTEXT")?.unwrap_or_else(|| "context.json".into());
        return Ok(vec![Party {
            name: "default".into(),
            secret: credential(Path::new(&secret_file))?,
            principal,
            context: context_values(Path::new(&context))?,
            view: None,
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
    known_settings()?;
    let parties = configured_parties()?;
    let path = PathBuf::from(setting("ZENO_APP_STORE")?.unwrap_or_else(|| "app.sqlite".into()));
    let receiver =
        PathBuf::from(setting("ZENO_APP_RECEIVER")?.unwrap_or_else(|| "deliveries".into()));
    let address =
        listen_address(&setting("ZENO_APP_ADDRESS")?.unwrap_or_else(|| "127.0.0.1:8080".into()))?;
    // Behind a trusted TLS proxy, the browser's origin is the proxy's.
    // Unset keeps the loopback origin; a value that is set must be a valid origin.
    let origin = match setting("ZENO_APP_ORIGIN")? {
        None => format!("http://{address}"),
        Some(text) => public_origin(&text)?,
    };
    let clock_file = setting("ZENO_APP_CLOCK_FILE")?.map(PathBuf::from);
    let connector = setting("ZENO_APP_CONNECTOR")?.map(PathBuf::from);
    crate::session::with_lineage(|lineage| {
        let authority = lineage.authorities().last().ok_or("empty lineage")?;
        if !path.exists() {
            let state = crate::genesis()?;
            let PublicationOutcome::Commit(genesis) = authority.publish_genesis(&state) else {
                return Err("genesis refused".into());
            };
            drop(V2SqliteShell::create(&path, authority, genesis).map_err(|_| "create store")?);
        }
        let clock = ServerClock::declared(clock_file)?;
        let table = party_contexts(authority, &parties, clock.as_ref())?;
        let production = Production::open(
            &path,
            lineage,
            1,
            match &connector {
                Some(file) => Receiver::Program(ProgramConnector::load(file)?),
                None => Receiver::File(FileReceiver::new(&receiver).map_err(|_| "receiver")?),
            },
            Some(Clock::default()),
        )
        .map_err(|_| "open store")?;
        let store = production.store();
        let mut endpoint = Application::with_parties(lineage, production, store, &path, table)
            .map_err(|_| "bind shell")?;
        if let Some(clock) = clock {
            endpoint = endpoint
                .with_clock(clock)
                .map_err(|_| "read the clock history")?;
        }
        let endpoint = endpoint.with_views(party_views(&parties)?);
        let config = ingress_config(
            &parties,
            RustCryptoSha256::hash(authority.identity()),
            origin,
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
