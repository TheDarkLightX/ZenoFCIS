//! Artifact profiles: which admitted finite programs a request may hold and
//! propose (NSC-002, NSC-003, NSC-005).
//!
//! Both profiles start from the library importer's complete admission (shape,
//! types, references, canonical re-encoding). `FunctionalBoolV1` then narrows
//! to the total Boolean subset; `CheckedI64V1` keeps the full instruction set
//! with its eager checked arithmetic, bounded by F3's domain cap. Neither
//! profile is enabled by the other's qualification.

use crate::transform::{DEFAULT_MAX_INPUT_TUPLES, domain_size};
use serde_json::{Value, json};
use zeno_fcis_synthesis::finite::{Domain, Op, Program};

/// Named, versioned artifact profile.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Profile {
    /// 0–6 Boolean inputs, 1–16 Boolean outputs, 1–256 Input/Bool/And/Not/
    /// Select nodes; at most 64 input tuples. Total after admission.
    FunctionalBoolV1,
    /// The full finite-i64 instruction set with eager traps, on a complete
    /// declared domain of at most F3's tuple cap.
    CheckedI64V1,
}

/// Why a program is outside a profile.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum ProfileRefusal {
    /// More inputs than the profile admits.
    Inputs { count: usize, max: usize },
    /// Fewer or more outputs than the profile admits.
    Outputs {
        count: usize,
        min: usize,
        max: usize,
    },
    /// A non-Boolean input position in the Boolean profile.
    NonBooleanInput { position: usize },
    /// A non-Boolean output position in the Boolean profile.
    NonBooleanOutput { position: usize },
    /// A stored instruction, used or not, outside the profile's opcodes.
    Opcode { node: usize, opcode: &'static str },
    /// An input domain with no values.
    EmptyInputDomain { position: usize },
    /// The complete product is larger than the profile admits; `size` is
    /// `None` above `u128::MAX`.
    DomainTooLarge { size: Option<u128>, max: u64 },
}

impl Profile {
    pub(crate) const ALL: [Profile; 2] = [Profile::FunctionalBoolV1, Profile::CheckedI64V1];

    /// Stable profile name.
    pub(crate) fn name(self) -> &'static str {
        match self {
            Profile::FunctionalBoolV1 => "functional-bool-v1",
            Profile::CheckedI64V1 => "checked-i64-v1",
        }
    }

    pub(crate) fn parse(name: &str) -> Option<Profile> {
        Self::ALL.into_iter().find(|profile| profile.name() == name)
    }

    /// Largest complete input domain the profile admits, in tuples.
    pub(crate) fn max_tuples(self) -> u64 {
        match self {
            Profile::FunctionalBoolV1 => 64,
            Profile::CheckedI64V1 => DEFAULT_MAX_INPUT_TUPLES,
        }
    }

    /// Checks an already admitted program against the profile. Every stored
    /// node is checked, including unused nodes and unselected arms; the domain
    /// is the complete ordered product of the declared inputs.
    pub(crate) fn admit(self, program: &Program) -> Result<u64, ProfileRefusal> {
        if let Profile::FunctionalBoolV1 = self {
            let inputs = program.inputs();
            if inputs.len() > 6 {
                return Err(ProfileRefusal::Inputs {
                    count: inputs.len(),
                    max: 6,
                });
            }
            let outputs = program.outputs();
            if outputs.is_empty() || outputs.len() > 16 {
                return Err(ProfileRefusal::Outputs {
                    count: outputs.len(),
                    min: 1,
                    max: 16,
                });
            }
            if let Some(position) = inputs.iter().position(|domain| *domain != Domain::Bool) {
                return Err(ProfileRefusal::NonBooleanInput { position });
            }
            if let Some(position) = outputs.iter().position(|domain| *domain != Domain::Bool) {
                return Err(ProfileRefusal::NonBooleanOutput { position });
            }
            for (node, op) in program.nodes().iter().enumerate() {
                let opcode = match op {
                    Op::Input(_) | Op::Bool(_) | Op::And(..) | Op::Not(_) | Op::Select(..) => {
                        continue;
                    }
                    other => opcode_name(other),
                };
                return Err(ProfileRefusal::Opcode { node, opcode });
            }
        }
        let size = domain_size(program.inputs())
            .map_err(|position| ProfileRefusal::EmptyInputDomain { position })?;
        let max = self.max_tuples();
        match size.and_then(|size| u64::try_from(size).ok()) {
            Some(size) if size <= max => Ok(size),
            _ => Err(ProfileRefusal::DomainTooLarge { size, max }),
        }
    }
}

/// The library's opcode names, for diagnostics and cost reports.
pub(crate) fn opcode_name(op: &Op) -> &'static str {
    match op {
        Op::Input(_) => "Input",
        Op::Int(_) => "Int",
        Op::Bool(_) => "Bool",
        Op::Add(..) => "Add",
        Op::Sub(..) => "Sub",
        Op::Eq(..) => "Eq",
        Op::Lt(..) => "Lt",
        Op::And(..) => "And",
        Op::Not(_) => "Not",
        Op::Select(..) => "Select",
        _ => "Unclassified",
    }
}

impl ProfileRefusal {
    pub(crate) fn json(&self) -> Value {
        match self {
            ProfileRefusal::Inputs { count, max } => {
                json!({"reason": "profile-inputs", "count": count, "max": max})
            }
            ProfileRefusal::Outputs { count, min, max } => {
                json!({"reason": "profile-outputs", "count": count, "min": min, "max": max})
            }
            ProfileRefusal::NonBooleanInput { position } => {
                json!({"reason": "profile-non-boolean-input", "position": position})
            }
            ProfileRefusal::NonBooleanOutput { position } => {
                json!({"reason": "profile-non-boolean-output", "position": position})
            }
            ProfileRefusal::Opcode { node, opcode } => {
                json!({"reason": "profile-opcode", "node": node, "opcode": opcode})
            }
            ProfileRefusal::EmptyInputDomain { position } => {
                json!({"reason": "empty-input-domain", "position": position})
            }
            ProfileRefusal::DomainTooLarge { size, max } => json!({
                "reason": "domain-too-large",
                "domain_size": size.map(|size| size.to_string()),
                "max_input_tuples": max
            }),
        }
    }
}
