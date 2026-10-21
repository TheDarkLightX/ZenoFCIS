//! Prints the built-in `shell-effects/1` registry as JSON for the ZAL drift test;
//! authority: none.
//!
//! The output gives the registry's canonical bytes and identity, its check
//! result, the SHA-256 of `registry.rs` (so a stale build is refused), and the
//! data the canonical bytes only number: port, capability, resource and answer
//! names, which capabilities are store capabilities, and the refusal codes and
//! faults the answers carry. Exhaustive matches below stop this example from
//! building when a port type gains or loses a case.

use zeno_fcis_codec::{CommitmentHasher, Hash32, RustCryptoSha256};
use zeno_fcis_shell::effects::port::{Code, Fault, Outcome, Report, Sent};
use zeno_fcis_shell::effects::registry::{
    self, Answer, Capability, Kind, Port, REGISTRY, Registry, RegistryError, Resources, SIGNATURES,
    Signature, V2,
};

const CODES: [Code; 22] = [
    Code::Provider,
    Code::Schema,
    Code::Identity,
    Code::Authority,
    Code::Lineage,
    Code::Checkpoint,
    Code::InvocationKind,
    Code::History,
    Code::Range,
    Code::PreState,
    Code::Replay,
    Code::Concurrent,
    Code::Delivery,
    Code::Interpreter,
    Code::UnknownDelivery,
    Code::AlreadyAcknowledged,
    Code::PayloadMismatch,
    Code::Capacity,
    Code::Upgrade,
    Code::Succession,
    Code::Unsettled,
    Code::Protocol,
];
const FAULTS: [Fault; 3] = [Fault::Busy, Fault::Full, Fault::Io];
const CAPABILITIES: [Capability; 7] = [
    Capability::ReadStore,
    Capability::WriteStore,
    Capability::Relay,
    Capability::OpenStore,
    Capability::Transport,
    Capability::Clock,
    Capability::Jitter,
];
const RESOURCES: [(&str, Resources); 5] = [
    ("durable", Resources::DURABLE),
    ("volatile", Resources::VOLATILE),
    ("transport", Resources::TRANSPORT),
    ("clock", Resources::CLOCK),
    ("jitter", Resources::JITTER),
];

fn code_index(code: Code) -> usize {
    match code {
        Code::Provider => 0,
        Code::Schema => 1,
        Code::Identity => 2,
        Code::Authority => 3,
        Code::Lineage => 4,
        Code::Checkpoint => 5,
        Code::InvocationKind => 6,
        Code::History => 7,
        Code::Range => 8,
        Code::PreState => 9,
        Code::Replay => 10,
        Code::Concurrent => 11,
        Code::Delivery => 12,
        Code::Interpreter => 13,
        Code::UnknownDelivery => 14,
        Code::AlreadyAcknowledged => 15,
        Code::PayloadMismatch => 16,
        Code::Capacity => 17,
        Code::Upgrade => 18,
        Code::Succession => 19,
        Code::Unsettled => 20,
        Code::Protocol => 21,
    }
}

fn fault_index(fault: Fault) -> usize {
    match fault {
        Fault::Busy => 0,
        Fault::Full => 1,
        Fault::Io => 2,
    }
}

fn capability_index(capability: Capability) -> usize {
    match capability {
        Capability::ReadStore => 0,
        Capability::WriteStore => 1,
        Capability::Relay => 2,
        Capability::OpenStore => 3,
        Capability::Transport => 4,
        Capability::Clock => 5,
        Capability::Jitter => 6,
    }
}

/// Each case of `Outcome` with its payload type; the bindings pin the types.
fn outcome_case(outcome: &Outcome<()>) -> &'static str {
    match outcome {
        Outcome::Ok(()) => "T",
        Outcome::Refused(code) => {
            let _: &Code = code;
            "Code"
        }
        Outcome::NotApplied(fault) | Outcome::Unknown(fault) => {
            let _: &Fault = fault;
            "Fault"
        }
    }
}

fn sent_case(sent: &Sent) -> &'static str {
    match sent {
        Sent::Sent(report) => {
            let _: &Report = report;
            "T"
        }
        Sent::NotSent(code) | Sent::Unknown(code) => {
            let _: &Code = code;
            "Code"
        }
    }
}

fn fallible_case(result: &Result<(), Fault>) -> &'static str {
    match result {
        Ok(()) => "T",
        Err(fault) => {
            let _: &Fault = fault;
            "Fault"
        }
    }
}

fn answer_number(answer: Answer) -> u8 {
    match answer {
        Answer::Outcome(_) => 0,
        Answer::Transport(_) => 1,
        Answer::Fallible(_) => 2,
    }
}

/// The Debug name of a value up to its first parenthesis.
fn head(value: &impl core::fmt::Debug) -> String {
    let text = format!("{value:?}");
    text.split('(').next().unwrap_or_default().to_owned()
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn strings(items: &[String]) -> String {
    let quoted: Vec<String> = items.iter().map(|item| format!("\"{item}\"")).collect();
    format!("[{}]", quoted.join(","))
}

/// Whether the registry check requires this capability's ports to map to V2
/// calls: give the first signature this capability and an opt-in mapping.
fn store(capability: Capability) -> bool {
    let mut signatures: Vec<Signature> = SIGNATURES.to_vec();
    signatures[0].capability = capability;
    signatures[0].footprint = capability.allows();
    signatures[0].v2 = V2::OptIn;
    let probe = Registry {
        signatures: Box::leak(signatures.into_boxed_slice()),
        ..REGISTRY
    };
    registry::check(&probe) == Err(RegistryError::Unmapped(Port::ReadState))
}

fn main() {
    let complete = CODES
        .iter()
        .enumerate()
        .all(|(index, code)| code_index(*code) == index && *code as usize == index)
        && FAULTS
            .iter()
            .enumerate()
            .all(|(index, fault)| fault_index(*fault) == index && *fault as usize == index)
        && CAPABILITIES
            .iter()
            .enumerate()
            .all(|(index, capability)| capability_index(*capability) == index);
    let canonical = registry::canonical_bytes(&REGISTRY);
    let ports: Vec<String> = Port::ALL
        .iter()
        .map(|port| format!("[\"{port:?}\",{}]", port.tag()))
        .collect();
    let capabilities: Vec<String> = CAPABILITIES
        .iter()
        .map(|capability| {
            let allowed = capability.allows();
            format!(
                "[\"{capability:?}\",{},{},{},{}]",
                *capability as u8,
                store(*capability),
                allowed.reads.bits(),
                allowed.writes.bits()
            )
        })
        .collect();
    let resources: Vec<String> = RESOURCES
        .iter()
        .map(|(name, bits)| format!("[\"{name}\",{}]", bits.bits()))
        .collect();
    let report = Report {
        delivery_id: Hash32::ZERO,
        payload_sha256: Hash32::ZERO,
    };
    let outcomes = [
        Outcome::Ok(()),
        Outcome::Refused(Code::Provider),
        Outcome::NotApplied(Fault::Busy),
        Outcome::Unknown(Fault::Busy),
    ];
    let sends = [
        Sent::Sent(report),
        Sent::NotSent(Code::Provider),
        Sent::Unknown(Code::Provider),
    ];
    let fallibles = [Ok(()), Err(Fault::Busy)];
    let case = |name: String, payload: &str| format!("[\"{name}\",\"{payload}\"]");
    let answers = [
        (
            Answer::Outcome(Kind::Unit),
            outcomes
                .iter()
                .map(|o| case(head(o), outcome_case(o)))
                .collect::<Vec<_>>(),
        ),
        (
            Answer::Transport(Kind::Unit),
            sends
                .iter()
                .map(|s| case(head(s), sent_case(s)))
                .collect::<Vec<_>>(),
        ),
        (
            Answer::Fallible(Kind::Unit),
            fallibles
                .iter()
                .map(|r| case(head(r), fallible_case(r)))
                .collect::<Vec<_>>(),
        ),
    ];
    let answers: Vec<String> = answers
        .iter()
        .map(|(answer, cases)| {
            format!(
                "[\"{}\",{},[{}]]",
                head(answer),
                answer_number(*answer),
                cases.join(",")
            )
        })
        .collect();
    let codes: Vec<String> = CODES.iter().map(|code| format!("{code:?}")).collect();
    let faults: Vec<String> = FAULTS.iter().map(|fault| format!("{fault:?}")).collect();
    println!(
        "{{\"schema\":\"{}\",\"check\":\"{:?}\",\"complete\":{complete},\"canonical\":\"{}\",\
         \"identity\":\"{}\",\"registry_source_sha256\":\"{}\",\"ports\":[{}],\
         \"capabilities\":[{}],\"resources\":[{}],\"answers\":[{}],\"codes\":{},\"faults\":{},\
         \"authority\":\"none\"}}",
        REGISTRY.schema,
        registry::check(&REGISTRY),
        hex(&canonical),
        hex(registry::identity(&REGISTRY).as_bytes()),
        hex(RustCryptoSha256::hash(include_bytes!("../src/effects/registry.rs")).as_bytes()),
        ports.join(","),
        capabilities.join(","),
        resources.join(","),
        answers.join(","),
        strings(&codes),
        strings(&faults),
    );
}
