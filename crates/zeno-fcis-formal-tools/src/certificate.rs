//! Complete Alethe certificate checking, separate from solver attestation.
//!
//! The configured binary and its build remain trusted assumptions. Hash and
//! version admission identify it; neither is a proof of compilation. A witness
//! covers only the exact query and certificate checked by this session.
use super::{
    MAX_TOOL_BINARY_BYTES, PrivateExecutable, RustCryptoSha256, ToolFailure, hash_hex,
    open_untrusted_read, run_fixed,
};
use serde::{Deserialize, Serialize};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use zeno_fcis_codec::CommitmentHasher as _;

/// Qualified upstream source revision.
pub const SOURCE_REVISION: &str = "5f9f77bf83c52748dccc12829ce5e80903de0c99";
/// Exact wrapper protocol and source pin.
pub const WRAPPER_VERSION: &str =
    "zfcis-carcara-root-check 0.1.1 5f9f77bf83c52748dccc12829ce5e80903de0c99";
/// Source of the qualified complete-root and fragment guard.
pub const WRAPPER_SOURCE_SHA256: &str =
    "9ac2fbf00bd95985597ad12d5d87b489c2ad8570314fb90855b67cdb8bae22d2";
/// External archive used by the pinned build.
pub const SOURCE_ARCHIVE_SHA256: &str =
    "cf5ea2addf31fcd17dd4bf783fd6b37eb96d0265b8fb088d936e35369df1dd42";
/// Exact dependency lock for the wrapper build.
pub const LOCK_SHA256: &str = "0034712a8b69302309770525bcbd664ec1f66a07a5b6aa50029f25d6fc44b017";
/// Supported certificate-checker manifest.
pub const MANIFEST_SCHEMA: &str = "zeno-fcis/certificate-checker/1";

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Config {
    schema: String,
    path: PathBuf,
    sha256: String,
    timeout_ms: u64,
    max_output_bytes: usize,
}

/// Identity of the privately admitted checker. Local paths are excluded.
/// Source/archive/lock constants are claimed build inputs, not verified
/// provenance for the executable admitted by its caller-supplied hash.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct Identity {
    build_inputs_status: &'static str,
    build_provenance_verified: bool,
    source_revision: &'static str,
    source_archive_sha256: &'static str,
    wrapper_source_sha256: &'static str,
    dependency_lock_sha256: &'static str,
    version: &'static str,
    binary_sha256: String,
    timeout_ms: u64,
    max_output_bytes: usize,
}

/// A complete check of one exact query/certificate pair.
/// Only an admitted session can construct this witness.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct CompleteCertificate {
    query_sha256: String,
    query_bytes: usize,
    certificate_sha256: String,
    certificate_bytes: usize,
    stdout_sha256: String,
    stderr_sha256: String,
}

/// One pinned checker executable snapshot. Each check uses owned immutable files.
pub struct Session {
    identity: Identity,
    executable: PrivateExecutable,
}

impl Session {
    /// Admits a bounded manifest, exact executable hash and exact version.
    /// This never admits a solver or grants application authority.
    ///
    /// # Errors
    /// Invalid configuration, hash/version mismatches and process failures refuse.
    pub fn open(manifest: &[u8]) -> Result<Self, ToolFailure> {
        if manifest.len() > 65_536 {
            return Err(ToolFailure::UnsupportedEvidence);
        }
        let config: Config =
            serde_json::from_slice(manifest).map_err(|error| ToolFailure::Io(error.to_string()))?;
        if config.schema != MANIFEST_SCHEMA
            || !config.path.is_absolute()
            || config.sha256.len() != 64
            || !config
                .sha256
                .bytes()
                .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
            || !(1..=60_000).contains(&config.timeout_ms)
            || !(1..=1_048_576).contains(&config.max_output_bytes)
        {
            return Err(ToolFailure::UnsupportedEvidence);
        }
        let file = open_untrusted_read(&config.path)
            .map_err(|error| ToolFailure::Io(error.to_string()))?;
        let metadata = file
            .metadata()
            .map_err(|error| ToolFailure::Io(error.to_string()))?;
        if !metadata.is_file() {
            return Err(ToolFailure::NotFile);
        }
        if metadata.len() > MAX_TOOL_BINARY_BYTES {
            return Err(ToolFailure::BinaryTooLarge);
        }
        let mut bytes = Vec::new();
        file.take(MAX_TOOL_BINARY_BYTES + 1)
            .read_to_end(&mut bytes)
            .map_err(|error| ToolFailure::Io(error.to_string()))?;
        if bytes.len() as u64 > MAX_TOOL_BINARY_BYTES {
            return Err(ToolFailure::BinaryTooLarge);
        }
        if hash_hex(RustCryptoSha256::hash(&bytes)) != config.sha256 {
            return Err(ToolFailure::HashMismatch);
        }
        let executable = PrivateExecutable::create_named("Carcara", &bytes)?;
        let output = run_fixed(
            executable.path(),
            &["--version"],
            None,
            config.timeout_ms,
            config.max_output_bytes,
        )?;
        if !output.status.success() {
            return Err(ToolFailure::Crash(output.status.code()));
        }
        if output.stdout != format!("{WRAPPER_VERSION}\n").as_bytes() || !output.stderr.is_empty() {
            return Err(ToolFailure::VersionMismatch);
        }
        Ok(Self {
            identity: Identity {
                build_inputs_status: "claimed-build-inputs",
                build_provenance_verified: false,
                source_revision: SOURCE_REVISION,
                source_archive_sha256: SOURCE_ARCHIVE_SHA256,
                wrapper_source_sha256: WRAPPER_SOURCE_SHA256,
                dependency_lock_sha256: LOCK_SHA256,
                version: WRAPPER_VERSION,
                binary_sha256: config.sha256,
                timeout_ms: config.timeout_ms,
                max_output_bytes: config.max_output_bytes,
            },
            executable,
        })
    }

    /// Returns the admitted binary identity and explicitly claimed build inputs.
    #[must_use]
    pub const fn identity(&self) -> &Identity {
        &self.identity
    }

    /// Checks bounded query/certificate bytes using owned files and the private
    /// executable. Exit success alone, holes, UNKNOWN or extra output never pass.
    ///
    /// # Errors
    /// Refuses unsupported sizes/platforms, rejected proofs and process failures.
    pub fn check(
        &self,
        query: &[u8],
        certificate: &[u8],
    ) -> Result<CompleteCertificate, ToolFailure> {
        if query.len() > 1_048_576 || certificate.len() > 33_554_432 {
            return Err(ToolFailure::UnsupportedEvidence);
        }
        #[cfg(not(unix))]
        {
            let _ = (query, certificate);
            Err(ToolFailure::ProcessContainmentUnavailable)
        }
        #[cfg(unix)]
        {
            let inputs = Inputs::new(query, certificate)?;
            let query_path = inputs
                .query
                .to_str()
                .ok_or(ToolFailure::UnsupportedEvidence)?;
            let certificate_path = inputs
                .certificate
                .to_str()
                .ok_or(ToolFailure::UnsupportedEvidence)?;
            let output = run_fixed(
                self.executable.path(),
                &[query_path, certificate_path],
                None,
                self.identity.timeout_ms,
                self.identity.max_output_bytes,
            )?;
            if !output.status.success() {
                return Err(ToolFailure::Crash(output.status.code()));
            }
            if output.stdout != b"checked-complete-root\n" || !output.stderr.is_empty() {
                return Err(ToolFailure::Unknown);
            }
            Ok(CompleteCertificate {
                query_sha256: hash_hex(RustCryptoSha256::hash(query)),
                query_bytes: query.len(),
                certificate_sha256: hash_hex(RustCryptoSha256::hash(certificate)),
                certificate_bytes: certificate.len(),
                stdout_sha256: hash_hex(RustCryptoSha256::hash(&output.stdout)),
                stderr_sha256: hash_hex(RustCryptoSha256::hash(&output.stderr)),
            })
        }
    }
}

#[cfg(unix)]
struct Inputs {
    query: PathBuf,
    certificate: PathBuf,
    directory: super::PrivateWorkingDirectory,
}
#[cfg(unix)]
impl Inputs {
    fn new(query: &[u8], certificate: &[u8]) -> Result<Self, ToolFailure> {
        let directory = super::PrivateWorkingDirectory::create()?;
        let inputs = Self {
            query: directory.path().join("query.smt2"),
            certificate: directory.path().join("certificate.alethe"),
            directory,
        };
        let write = |path: &Path, bytes: &[u8]| {
            use std::os::unix::fs::OpenOptionsExt as _;
            let mut file = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .open(path)?;
            file.write_all(bytes)?;
            file.sync_all()
        };
        write(&inputs.query, query)
            .and_then(|()| write(&inputs.certificate, certificate))
            .map_err(|error: std::io::Error| ToolFailure::Io(error.to_string()))?;
        Ok(inputs)
    }
}
#[cfg(unix)]
impl Drop for Inputs {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.query);
        let _ = std::fs::remove_file(&self.certificate);
        let _ = self.directory.path();
    }
}
