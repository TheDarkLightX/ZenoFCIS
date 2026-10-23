//! Local scalar service authority. Bindings use one immutable pre-state;
//! actual checked per-core publications remain provisional until all laws pass.
//! No I/O, callbacks, channels, committed failure or external fact authority.
use super::{DomainSources, Identity, JointComposition, LeafSources, hex};
use super::{
    formula::{Axis, Formula},
    joint::{delivered, gather},
    leaf::sha256,
};
use crate::internals::policy;
use crate::internals::review::evaluate::Framer;
use serde::Deserialize;
use serde_json::value::RawValue;
use std::cell::Cell;
use zeno_fcis_synthesis::finite::{
    V2Resource, v2_authority::PublicationOutcome, v2_composition as c,
};

/// Versioned, deliberately channel-free service profile.
pub const SERVICE_SCHEMA: &str = "zeno-fcis/service/1";
const RESOURCES: [V2Resource; 8] = [
    V2Resource::Read,
    V2Resource::Write,
    V2Resource::Candidate,
    V2Resource::Effect,
    V2Resource::Byte,
    V2Resource::WitnessByte,
    V2Resource::Depth,
    V2Resource::Step,
];

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    schema: String,
    composition: Box<RawValue>,
    schemas: Vec<String>,
    laws: Vec<Law>,
    max_steps: u64,
}
#[derive(Deserialize, Debug)]
#[serde(deny_unknown_fields)]
struct Law {
    name: String,
    formula: String,
    genesis: bool,
}

/// Owned immutable state in the manifest's fixed core/field order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServiceState {
    values: Box<[i128]>,
}
impl ServiceState {
    /// Read-only scalar view; no mutable aliases escape.
    pub fn values(&self) -> &[i128] {
        &self.values
    }
    /// Canonical big-endian scalar vector, with a version tag and length.
    pub fn bytes(&self) -> Vec<u8> {
        numbers(b"ZFCIS-SERVICE-STATE-1\0", &self.values)
    }
}

/// Canonical local request. Labels scope replay; they do not authenticate a caller.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServiceRequest {
    principal: String,
    key: String,
    values: Box<[i128]>,
}
impl ServiceRequest {
    /// Defensively owns bounded ASCII labels and scalar arguments.
    pub fn new(principal: &str, key: &str, values: &[i128]) -> Result<Self, String> {
        let label = |s: &str| {
            !s.is_empty()
                && s.len() <= 64
                && s.bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"-_./".contains(&b))
        };
        if !label(principal) || !label(key) || values.len() > 64 {
            return Err("request-size-or-label".into());
        }
        Ok(Self {
            principal: principal.into(),
            key: key.into(),
            values: values.into(),
        })
    }
    /// Principal-scoped replay label.
    pub fn principal(&self) -> &str {
        &self.principal
    }
    /// Caller-selected bounded replay key.
    pub fn key(&self) -> &str {
        &self.key
    }
    /// Immutable arguments in manifest order.
    pub fn values(&self) -> &[i128] {
        &self.values
    }
    /// Canonical request bytes; replay rejects every noncanonical encoding.
    pub fn bytes(&self) -> Vec<u8> {
        let mut out = numbers(b"ZFCIS-SERVICE-REQUEST-1\0", &self.values);
        frame(&mut out, self.principal.as_bytes());
        frame(&mut out, self.key.as_bytes());
        out
    }
}

/// Actual per-resource core usage plus explicitly charged wrapper steps.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ServiceUsage {
    core: [u64; 8],
    wrapper: u64,
}
impl ServiceUsage {
    /// Core counters in Read, Write, Candidate, Effect, Byte, WitnessByte, Depth, Step order.
    pub fn core(&self) -> &[u64; 8] {
        &self.core
    }
    /// Wrapper logical work (input bytes, binding/predicate source bytes, framing).
    pub fn wrapper(&self) -> u64 {
        self.wrapper
    }
    fn charge(&mut self, count: usize) -> Result<(), String> {
        self.wrapper = self
            .wrapper
            .checked_add(u64::try_from(count).map_err(|_| "usage-overflow")?)
            .ok_or("usage-overflow")?;
        Ok(())
    }
    fn add(&mut self, usage: zeno_fcis_synthesis::finite::V2Usage) -> Result<(), String> {
        for (at, resource) in RESOURCES.iter().enumerate() {
            self.core[at] = self.core[at]
                .checked_add(usage.used(*resource))
                .ok_or("usage-overflow")?;
        }
        Ok(())
    }
    fn steps(&self) -> Option<u64> {
        self.wrapper.checked_add(self.core[7])
    }
}
/// Whole-request refusal; no committing capability or successor is exposed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServiceRefusal {
    /// First refusal in manifest/check order.
    pub reason: String,
    /// Actual measured work, including discarded provisional accepts.
    pub usage: ServiceUsage,
}
impl std::fmt::Display for ServiceRefusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.reason)
    }
}
impl std::error::Error for ServiceRefusal {}

#[derive(Debug)]
struct CoreRecord {
    id: String,
    program: [u8; 32],
    schema: [u8; 32],
    pre: [u8; 32],
    post: [u8; 32],
    decision: [u8; 32],
}

/// Non-Clone, non-deserializable capability. Only this authority constructs it
/// after every actual core publication and global law has passed.
#[must_use]
#[derive(Debug)]
pub struct ServicePublication {
    identity: [u8; 32],
    pre: ServiceState,
    post: ServiceState,
    request: Option<ServiceRequest>,
    version: u64,
    records: Vec<CoreRecord>,
    usage: ServiceUsage,
    pre_root: [u8; 32],
    post_root: [u8; 32],
}
impl ServicePublication {
    /// Exact source-, manifest-, policy- and program-bound service identity.
    pub fn identity(&self) -> &[u8; 32] {
        &self.identity
    }
    /// Immutable expected pre-state.
    pub fn pre(&self) -> &ServiceState {
        &self.pre
    }
    /// Immutable complete successor.
    pub fn post(&self) -> &ServiceState {
        &self.post
    }
    /// None only for checked service genesis.
    pub fn request(&self) -> Option<&ServiceRequest> {
        self.request.as_ref()
    }
    /// Expected global version (genesis uses zero).
    pub fn version(&self) -> u64 {
        self.version
    }
    /// Root of the complete ordered pre-state vector.
    pub fn pre_root(&self) -> &[u8; 32] {
        &self.pre_root
    }
    /// Root of the complete ordered post-state vector.
    pub fn post_root(&self) -> &[u8; 32] {
        &self.post_root
    }
    /// Retained actual invocation accounting.
    pub fn usage(&self) -> ServiceUsage {
        self.usage
    }
    /// Independently checks the receipt's fields against this genuine publication.
    /// This data-format utility grants no decision, usage or publication authority.
    #[must_use]
    pub fn matches_receipt(&self, bytes: &[u8]) -> bool {
        let mut remaining = bytes;
        let request = self
            .request
            .as_ref()
            .map(ServiceRequest::bytes)
            .unwrap_or_default();
        if !receipt_part(&mut remaining, b"ZFCIS-SERVICE-RECEIPT-1\0")
            || !receipt_part(&mut remaining, &self.identity)
            || !receipt_part(&mut remaining, &self.version.to_be_bytes())
            || !receipt_part(&mut remaining, &[u8::from(self.request.is_some())])
            || read_frame(&mut remaining).ok() != Some(request.as_slice())
            || !receipt_part(&mut remaining, &self.pre_root)
            || !receipt_part(&mut remaining, &self.post_root)
            || !receipt_part(&mut remaining, &(self.records.len() as u64).to_be_bytes())
        {
            return false;
        }
        for record in &self.records {
            if read_frame(&mut remaining).ok() != Some(record.id.as_bytes())
                || !receipt_part(&mut remaining, &record.program)
                || !receipt_part(&mut remaining, &record.schema)
                || !receipt_part(&mut remaining, &record.pre)
                || !receipt_part(&mut remaining, &record.post)
                || !receipt_part(&mut remaining, &record.decision)
            {
                return false;
            }
        }
        for value in self.usage.core {
            if !receipt_part(&mut remaining, &value.to_be_bytes()) {
                return false;
            }
        }
        receipt_part(&mut remaining, &self.usage.wrapper.to_be_bytes())
            && receipt_part(&mut remaining, &sha256(b"ZFCIS-SERVICE-EMPTY-OUTBOX-1\0"))
            && remaining.is_empty()
    }

    /// Complete canonical internal receipt, including every core's roots and decision.
    pub fn receipt(&self) -> Vec<u8> {
        let mut out = b"ZFCIS-SERVICE-RECEIPT-1\0".to_vec();
        out.extend(self.identity);
        out.extend(self.version.to_be_bytes());
        out.push(u8::from(self.request.is_some()));
        frame(
            &mut out,
            &self
                .request
                .as_ref()
                .map(ServiceRequest::bytes)
                .unwrap_or_default(),
        );
        out.extend(self.pre_root);
        out.extend(self.post_root);
        out.extend((self.records.len() as u64).to_be_bytes());
        for record in &self.records {
            frame(&mut out, record.id.as_bytes());
            out.extend(record.program);
            out.extend(record.schema);
            out.extend(record.pre);
            out.extend(record.post);
            out.extend(record.decision);
        }
        for value in self.usage.core {
            out.extend(value.to_be_bytes());
        }
        out.extend(self.usage.wrapper.to_be_bytes());
        // Channels are refused at binding; this profile has exactly the empty outbox.
        out.extend(sha256(b"ZFCIS-SERVICE-EMPTY-OUTBOX-1\0"));
        out
    }
}

/// Immutable bound local service. Preparation tables accelerate bindings only;
/// each decision calls the actual checked Program's publication API again.
#[derive(Debug)]
pub struct ServiceAuthority {
    joint: JointComposition,
    identity: [u8; 32],
    laws: Vec<(Law, Formula)>,
    max_steps: u64,
}
impl ServiceAuthority {
    /// Binds exact scalar sources and fixed order. Refuses channels, partial or
    /// shared ownership, skipping participants and CommittedFailure-capable cores.
    pub fn from_sources(text: &str, sources: &[LeafSources]) -> Result<Self, String> {
        if text.len() > 1 << 20 {
            return Err("manifest-too-large".into());
        }
        let manifest: Manifest = serde_json::from_str(text).map_err(|e| e.to_string())?;
        if manifest.schema != SERVICE_SCHEMA || manifest.max_steps == 0 || manifest.laws.len() > 64
        {
            return Err("service-profile".into());
        }
        let joint = JointComposition::from_sources(&DomainSources {
            manifest: manifest.composition.get(),
            components: sources,
        })
        .map_err(|e| e.to_string())?;
        if manifest.schemas.len() != joint.composition.components.len() {
            return Err("schema-pins".into());
        }
        for (core, schema) in joint.composition.components.iter().zip(&manifest.schemas) {
            if !core.owns || core.participates.formula.text != "true" || core.committed_failure {
                return Err(format!("service-profile:{}", core.name));
            }
            if *schema != hex(&sha256(&core.bound.schema)) {
                return Err(format!("schema-pin:{}", core.name));
            }
        }
        let mut names = std::collections::BTreeSet::new();
        let mut laws = Vec::new();
        for law in manifest.laws {
            if law.name.is_empty() || law.name.len() > 64 || !names.insert(law.name.clone()) {
                return Err("law-name".into());
            }
            let formula = Formula::parse(&law.formula).map_err(|e| match e {
                super::formula::Intake::Syntax(s) | super::formula::Intake::TooLarge(s) => s,
            })?;
            for name in &formula.names {
                if name == "accepted" {
                    continue;
                }
                if let Some(field) = name.strip_prefix("request.")
                    && !law.genesis
                    && joint.request_layout().iter().any(|s| s.name == field)
                {
                    continue;
                }
                if let Some(field) = name
                    .strip_prefix("pre.")
                    .or_else(|| name.strip_prefix("post."))
                    && joint
                        .state_layout()
                        .iter()
                        .any(|s| format!("{}.{}", s.component, s.field) == field)
                {
                    continue;
                }
                return Err(format!("law-observation:{name}"));
            }
            formula.check_predicate(&joint.composition.components[0].bound.declarations)?;
            laws.push((law, formula));
        }
        let mut subject = b"ZFCIS-SERVICE-IDENTITY-1\0".to_vec();
        frame(&mut subject, text.as_bytes());
        subject.extend(Identity::current().digest());
        let authority = Self {
            joint,
            identity: sha256(&subject),
            laws,
            max_steps: manifest.max_steps,
        };
        drop(authority.genesis().map_err(|e| format!("genesis:{e}"))?);
        Ok(authority)
    }
    /// Exact identity for whole-version activation and historical replay.
    pub fn identity(&self) -> &[u8; 32] {
        &self.identity
    }
    /// Admit a scalar snapshot by the manifest's complete layout and domains.
    pub fn state(&self, values: &[i128]) -> Result<ServiceState, String> {
        if values.len() != self.joint.state_layout().len()
            || values
                .iter()
                .zip(self.joint.state_layout())
                .any(|(v, s)| !s.domain.contains(*v))
        {
            return Err("state-domain".into());
        }
        Ok(ServiceState {
            values: values.into(),
        })
    }
    /// Decode only this profile's exact canonical state format.
    pub fn decode_state(&self, bytes: &[u8]) -> Result<ServiceState, String> {
        let (values, rest) = read_numbers(bytes, b"ZFCIS-SERVICE-STATE-1\0")?;
        if !rest.is_empty() {
            return Err("state-trailing".into());
        }
        self.state(&values)
    }
    /// Decode exact canonical request bytes, rejecting trailing or alternative forms.
    pub fn decode_request(&self, bytes: &[u8]) -> Result<ServiceRequest, String> {
        let (values, mut rest) = read_numbers(bytes, b"ZFCIS-SERVICE-REQUEST-1\0")?;
        let principal = std::str::from_utf8(read_frame(&mut rest)?).map_err(|_| "request-utf8")?;
        let key = std::str::from_utf8(read_frame(&mut rest)?).map_err(|_| "request-utf8")?;
        let request = ServiceRequest::new(principal, key, &values)?;
        if !rest.is_empty() || request.bytes() != bytes {
            return Err("request-noncanonical".into());
        }
        Ok(request)
    }
    /// Recompute every per-core canonical root and the length-framed global root.
    pub fn root(&self, state: &ServiceState) -> Result<[u8; 32], String> {
        self.state(state.values())?;
        let states = self.split(state);
        let mut out = b"ZFCIS-SERVICE-ROOT-1\0".to_vec();
        out.extend(self.identity);
        out.extend((states.len() as u64).to_be_bytes());
        for (core, values) in self.joint.composition.components.iter().zip(states) {
            frame(&mut out, core.name.as_bytes());
            out.extend(self.core_root(core, &values)?);
        }
        Ok(sha256(&out))
    }
    /// Check every core's real genesis publication and the global genesis laws.
    pub fn genesis(&self) -> Result<ServicePublication, ServiceRefusal> {
        let state = ServiceState {
            values: self.joint.genesis().into(),
        };
        self.run(&state, None, 0)
    }
    /// Pure whole-request decision. First refusal discards all provisional work.
    /// No implicit retry occurs; each returned refusal retains its measured usage.
    pub fn decide(
        &self,
        state: &ServiceState,
        request: &ServiceRequest,
        version: u64,
    ) -> Result<ServicePublication, ServiceRefusal> {
        self.run(state, Some(request), version)
    }

    fn split(&self, state: &ServiceState) -> Vec<Vec<i128>> {
        let mut at = 0;
        self.joint
            .composition
            .components
            .iter()
            .map(|c| {
                let values = state.values[at..at + c.state_width].to_vec();
                at += c.state_width;
                values
            })
            .collect()
    }
    fn core_root(
        &self,
        core: &super::domain::Component,
        state: &[i128],
    ) -> Result<[u8; 32], String> {
        let contract = core.contract().map_err(|e| e.to_string())?;
        let mut tuple = state.to_vec();
        tuple.extend(
            core.fields[core.state_width..]
                .iter()
                .map(|f| f.domain.values()[0]),
        );
        Ok(sha256(
            &Framer::new(&core.bound.positions, &contract).frame(&tuple)[0],
        ))
    }
    fn invariants(&self, states: &[Vec<i128>], usage: &mut ServiceUsage) -> Result<(), String> {
        for (core, tables) in self
            .joint
            .composition
            .components
            .iter()
            .zip(&self.joint.tables.components)
        {
            usage.charge(core.invariant.formula.text.len())?;
            let own = &states[self
                .joint
                .composition
                .components
                .iter()
                .position(|c| c.name == core.name)
                .ok_or("core-missing")?];
            if !tables
                .invariant
                .holds(&gather(&core.invariant.reads, &[], states, own))
            {
                return Err(format!("invariant:{}", core.name));
            }
        }
        for (coupling, table) in self
            .joint
            .composition
            .couplings
            .iter()
            .zip(&self.joint.tables.couplings)
        {
            usage.charge(coupling.invariant.formula.text.len())?;
            if !table.holds(&gather(&coupling.invariant.reads, &[], states, &[])) {
                return Err(format!("coupling:{}", coupling.name));
            }
        }
        Ok(())
    }
    fn run(
        &self,
        state: &ServiceState,
        request: Option<&ServiceRequest>,
        version: u64,
    ) -> Result<ServicePublication, ServiceRefusal> {
        let mut usage = ServiceUsage::default();
        let result = (|| -> Result<ServicePublication, String> {
            usage.charge(state.bytes().len() + request.map_or(0, |r| r.bytes().len()))?;
            self.state(state.values())?;
            if version >= i64::MAX as u64 {
                return Err("version-exhausted".into());
            }
            if let Some(request) = request
                && (request.values.len() != self.joint.request_layout().len()
                    || request
                        .values
                        .iter()
                        .zip(self.joint.request_layout())
                        .any(|(v, s)| !s.domain.contains(*v)))
            {
                return Err("request-domain".into());
            }
            let pre = self.split(state);
            self.invariants(&pre, &mut usage)?;
            let mut after = pre.clone();
            let mut records = Vec::new();
            for (at, core) in self.joint.composition.components.iter().enumerate() {
                if usage.steps().is_none_or(|s| s > self.max_steps) {
                    return Err("service-budget".into());
                }
                let tables = &self.joint.tables.components[at];
                usage.charge(
                    core.bindings
                        .iter()
                        .map(|f| f.formula.text.len())
                        .sum::<usize>()
                        + core.assume.formula.text.len(),
                )?;
                let tuple = if let Some(request) = request {
                    delivered(core, tables, request.values(), &pre, &pre[at])
                        .map_err(|c| format!("binding:{}:{}", core.name, c.name()))?
                } else {
                    let mut tuple = core.genesis.clone();
                    tuple.extend(
                        core.fields[core.state_width..]
                            .iter()
                            .map(|f| f.domain.values()[0]),
                    );
                    tuple
                };
                if request.is_some()
                    && !tables
                        .assume
                        .holds(&gather(&core.assume.reads, &[], &pre, &tuple))
                {
                    return Err(format!("assumption:{}", core.name));
                }
                let contract = core.contract().map_err(|e| e.to_string())?;
                let framer = Framer::new(&core.bound.positions, &contract);
                let frames = framer.frame(&tuple);
                usage.charge(frames.iter().map(Vec::len).sum())?;
                let record = policy::with_authority(
                    &contract,
                    &core.bound.schema,
                    |authority| -> Result<CoreRecord, String> {
                        // Reserve from the actual checked descriptor, never a preparation handle.
                        let bound = authority.descriptor().limits.limit(V2Resource::Step);
                        if usage
                            .steps()
                            .and_then(|used| used.checked_add(bound))
                            .is_none_or(|reserved| reserved > self.max_steps)
                        {
                            return Err("service-budget".into());
                        }
                        let outcome = if request.is_some() {
                            authority.publish(c::Raw {
                                state: &frames[0],
                                command: &frames[1],
                                context: &frames[2],
                            })
                        } else {
                            authority.publish_genesis(&frames[0])
                        };
                        usage.add(outcome.evaluation().usage())?;
                        match outcome {
                            PublicationOutcome::Commit(publication) => {
                                if publication
                                    .evaluation()
                                    .result()
                                    .map_err(|_| "committed-evaluation-refused")?
                                    .class()
                                    != c::Class::Accept
                                    || !publication.outbox().is_empty()
                                    || !publication.effects().is_empty()
                                {
                                    return Err(format!("publication-profile:{}", core.name));
                                }
                                let candidate = publication
                                    .evaluation()
                                    .result()
                                    .map_err(|_| "committed-evaluation-refused")?;
                                // Field IDs are the exact contract state positions, never output order guesses.
                                let post = core
                                    .bound
                                    .positions
                                    .iter()
                                    .take(core.state_width)
                                    .map(|position| {
                                        let value = candidate
                                            .post()
                                            .iter()
                                            .find(|f| Some(f.id) == position.field)
                                            .ok_or("post-field")?
                                            .value;
                                        match value {
                                            c::Atom::Bool(v) => Ok(i128::from(v)),
                                            c::Atom::I128(v) => Ok(v),
                                            c::Atom::Sum { variant, .. }
                                            | c::Atom::Enum { variant, .. } => {
                                                Ok(i128::from(variant))
                                            }
                                            _ => Err("post-type"),
                                        }
                                    })
                                    .collect::<Result<Vec<_>, &str>>()?;
                                after[at] = post;
                                Ok(CoreRecord {
                                    id: core.name.clone(),
                                    program: core.program_sha256,
                                    schema: sha256(&core.bound.schema),
                                    pre: sha256(&frames[0]),
                                    post: sha256(publication.poststate()),
                                    decision: sha256(publication.subject()),
                                })
                            }
                            PublicationOutcome::Reject(evaluation) => Err(format!(
                                "reject:{}:{}",
                                core.name,
                                evaluation
                                    .result()
                                    .ok()
                                    .and_then(|c| c.reason())
                                    .map_or("none".into(), |r| r.to_string())
                            )),
                            PublicationOutcome::Refused { error, evaluation } => Err(format!(
                                "refused:{}:{}",
                                core.name,
                                super::AuthorityRefusal::of(&error, evaluation.diagnostics())
                                    .json()
                            )),
                            _ => Err("unknown-publication".into()),
                        }
                    },
                )
                .map_err(|e| e.to_string())??;
                records.push(record);
            }
            self.invariants(&after, &mut usage)?;
            let post = ServiceState {
                values: after.iter().flatten().copied().collect(),
            };
            for (law, formula) in &self.laws {
                if request.is_none() && !law.genesis {
                    continue;
                }
                usage.charge(formula.text.len())?;
                let values: Vec<i128> = formula
                    .names
                    .iter()
                    .map(|name| {
                        if name == "accepted" {
                            return Ok(1);
                        }
                        if let Some(field) = name.strip_prefix("request.") {
                            let at = self
                                .joint
                                .request_layout()
                                .iter()
                                .position(|s| s.name == field)
                                .ok_or("law-request")?;
                            return request.map(|r| r.values[at]).ok_or("genesis-request");
                        }
                        let (prefix, field) = name.split_once('.').ok_or("law-observation")?;
                        let at = self
                            .joint
                            .state_layout()
                            .iter()
                            .position(|s| format!("{}.{}", s.component, s.field) == field)
                            .ok_or("law-field")?;
                        Ok(if prefix == "pre" {
                            state.values[at]
                        } else {
                            post.values[at]
                        })
                    })
                    .collect::<Result<_, &str>>()?;
                let axes = values.iter().map(|v| Axis { values: vec![*v] }).collect();
                let table = formula.truth_table(
                    &self.joint.composition.components[0].bound.declarations,
                    axes,
                    &Cell::new(0),
                )?;
                if !table.holds(&values) {
                    return Err(format!("law:{}", law.name));
                }
            }
            if usage.steps().is_none_or(|s| s > self.max_steps) {
                return Err("service-budget".into());
            }
            let pre_root = self.root(state)?;
            let post_root = self.root(&post)?;
            // Independently recompute every successor root from the exact frozen scalar state.
            for (core, (record, values)) in self
                .joint
                .composition
                .components
                .iter()
                .zip(records.iter().zip(&after))
            {
                if self.core_root(core, values)? != record.post {
                    return Err("post-root-mismatch".into());
                }
            }
            Ok(ServicePublication {
                identity: self.identity,
                pre: state.clone(),
                post,
                request: request.cloned(),
                version,
                records,
                usage,
                pre_root,
                post_root,
            })
        })();
        result.map_err(|reason| ServiceRefusal { reason, usage })
    }
}
fn frame(out: &mut Vec<u8>, bytes: &[u8]) {
    out.extend((bytes.len() as u64).to_be_bytes());
    out.extend(bytes);
}
fn numbers(tag: &[u8], values: &[i128]) -> Vec<u8> {
    let mut out = tag.to_vec();
    out.extend((values.len() as u64).to_be_bytes());
    for v in values {
        out.extend(v.to_be_bytes());
    }
    out
}
fn read_frame<'a>(rest: &mut &'a [u8]) -> Result<&'a [u8], String> {
    let width = rest.get(..8).ok_or("short-length")?;
    let len = u64::from_be_bytes(width.try_into().map_err(|_| "length")?);
    let len = usize::try_from(len).map_err(|_| "length-overflow")?;
    let data = rest
        .get(8..)
        .and_then(|b| b.get(..len))
        .ok_or("short-frame")?;
    *rest = &rest[8 + len..];
    Ok(data)
}
fn read_numbers<'a>(bytes: &'a [u8], tag: &[u8]) -> Result<(Vec<i128>, &'a [u8]), String> {
    let mut rest = bytes.strip_prefix(tag).ok_or("wire-tag")?;
    let width = rest.get(..8).ok_or("short-length")?;
    let len = u64::from_be_bytes(width.try_into().map_err(|_| "length")?);
    if len > 4096 {
        return Err("wire-count".into());
    }
    rest = &rest[8..];
    let mut values = Vec::new();
    for _ in 0..len {
        values.push(i128::from_be_bytes(
            rest.get(..16)
                .ok_or("short-number")?
                .try_into()
                .map_err(|_| "number")?,
        ));
        rest = &rest[16..];
    }
    Ok((values, rest))
}

// The cursor is exclusively owned invocation scratch; the input bytes stay immutable.
fn receipt_part(remaining: &mut &[u8], expected: &[u8]) -> bool {
    match remaining.strip_prefix(expected) {
        Some(rest) => {
            *remaining = rest;
            true
        }
        None => false,
    }
}
