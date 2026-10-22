//! Starting-state admission uses the runtime Authority, including its limits
//! and ordered genesis-law diagnostics. No judge reconstructs those laws.

use serde_json::{Value, json};
use zeno_fcis_synthesis::finite::{
    v2_authority::Authority, v2_composition::DescriptorData, v2_laws,
};

use super::ContractError;

/// An owned starting-state refusal; no bound policy or invocation data escapes.
#[derive(Clone, Debug)]
pub struct Refusal {
    /// First refusing law, when law evaluation was reached.
    pub law: Option<u32>,
    /// One-line reason shared by judges and generation.
    pub reason: String,
}

impl Refusal {
    /// The same refusal as a contract error.
    pub fn error(&self) -> ContractError {
        ContractError::new("genesis", &self.reason)
    }

    /// A refused starting state is neither a successful proof nor a reachable state.
    pub fn packet(&self) -> Value {
        json!({
            "reason": self.reason,
            "law": self.law,
            "admissible_starting_states": 0,
            "states": {"reachable": 0},
        })
    }
}

/// Admit the original framed state through the same function used at startup.
/// All checks, rejection order and resource accounting belong to the Authority.
pub fn admit<'p, D: DescriptorData<'p> + 'p>(
    authority: &Authority<'p, D>,
    state: &[u8],
) -> Result<(), Refusal> {
    let evaluated = authority.genesis(state);
    evaluated.result().map(|_| ()).map_err(|refusal| {
        let law = evaluated
            .diagnostics()
            .iter()
            .find(|diagnostic| matches!(diagnostic.verdict, v2_laws::Verdict::Refused(_)))
            .map(|diagnostic| diagnostic.id);
        let detail = law.map_or_else(String::new, |id| format!("law {id}: "));
        Refusal {
            law,
            reason: format!("no admissible starting state: {detail}{refusal:?}"),
        }
    })
}
