//! Outbound transport through the bundled, dependency-free reference relay.
//! The store acknowledges only after this destination reports success.

use std::io::Write as _;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use crate::session::{Destination, Failure, Outgoing, Sent};

/// A reviewed destination configuration. Python 3 is required at the shell's edge.
pub struct RelayDestination<'a> {
    config: &'a Path,
}

impl<'a> RelayDestination<'a> {
    /// Uses the bounded reference relay with the settings at `config`.
    #[must_use]
    pub fn new(config: &'a Path) -> Self {
        Self { config }
    }
}

impl Destination for RelayDestination<'_> {
    fn send(&mut self, delivery: &Outgoing) -> Result<Sent, Failure> {
        let hash = zeno_fcis_shell_sqlite::v2::relay::payload_sha256(&delivery.payload);
        let line = format!(
            "{{\"schema\":\"zeno-fcis/relay-export/1\",\"commit\":{},\"lane\":{},\"ordinal\":{},\"delivery_id\":\"{}\",\"channel\":{},\"destination_root\":{},\"payload_root\":{},\"destination\":\"{}\",\"payload\":\"{}\",\"payload_sha256\":\"{}\",\"entry_hash\":\"{}\"}}\n",
            delivery.commit,
            delivery.lane,
            delivery.ordinal,
            delivery.id,
            delivery.channel,
            delivery.destination_type,
            delivery.payload_type,
            zeno_fcis_shell_sqlite::v2::relay::hex(&delivery.destination),
            zeno_fcis_shell_sqlite::v2::relay::hex(&delivery.payload),
            zeno_fcis_shell_sqlite::v2::relay::hex(hash.as_bytes()),
            delivery.entry_hash,
        );
        let mut child = Command::new("python3")
            .arg("-c")
            .arg(include_str!("../tools/relay.py"))
            .arg("--send-one")
            .arg(self.config)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::inherit())
            .spawn()
            .map_err(|error| Failure::Io(format!("start Python 3 relay: {error}")))?;
        let written = child
            .stdin
            .take()
            .ok_or_else(|| std::io::Error::other("relay input pipe missing"))
            .and_then(|mut input| input.write_all(line.as_bytes()));
        if let Err(error) = written {
            let _ = child.kill();
            let _ = child.wait();
            return Err(Failure::Io(format!("write relay input: {error}")));
        }
        // This worker performs no subprocess calls. Killing it also ends its
        // socket timer threads; the typed token is dropped without acknowledgment.
        let deadline = Instant::now() + Duration::from_secs(120);
        loop {
            match child.try_wait() {
                Ok(Some(status)) if status.success() => return Ok(Sent::Accepted),
                Ok(Some(status)) => {
                    return Err(Failure::Io(format!(
                        "relay exited {status}; delivery {} stays pending",
                        delivery.id
                    )));
                }
                Ok(None) if Instant::now() < deadline => {
                    std::thread::sleep(Duration::from_millis(10));
                }
                result => {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(Failure::Io(format!(
                        "relay exceeded its 120-second worker limit or could not be inspected ({result:?}); delivery {} stays pending",
                        delivery.id
                    )));
                }
            }
        }
    }
}
