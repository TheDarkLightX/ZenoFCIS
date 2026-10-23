//! Pure loading admission: genesis must inhabit its declared state domains.
//! This does not admit genesis laws or establish reachability or publication.
use super::{ContractError, Declarations, Rules};
use zeno_fcis_contract::internals::{declarations::Kind, rules::Constant, tables};

/// Check scalar input domains without reinterpreting the separate table ABI.
pub(crate) fn check(project: &str, rules: &str) -> Result<(), ContractError> {
    if tables::recognizes(rules)? {
        // tables::compile already decodes genesis through its typed value
        // decoder, including signed/unsigned bounds. Retain that independent gate.
        return Ok(());
    }
    read_rules(project, rules).map(|_| ())
}

/// The existing judge application name, after the same scalar admission.
pub(crate) fn application(project: &str, rules: &str) -> Result<String, ContractError> {
    Ok(read_rules(project, rules)?.template)
}

// Parsed rules are exclusively owned invocation scratch. Only the application
// name or unit result escapes this module; no checked mutable handle is exposed.
fn read_rules(project: &str, text: &str) -> Result<Rules, ContractError> {
    let rules = Rules::read_for_project(project, text)?;
    let declarations = Declarations::read(project, &rules.leaf_bindings)?;
    let fields = declarations.state_fields()?;
    if fields.is_empty() {
        return Err(ContractError::new(
            "project.zeno",
            "the state root, type 100, must be a record",
        ));
    }
    if !rules
        .genesis
        .keys()
        .copied()
        .eq(fields.iter().map(|field| field.id))
    {
        return Err(ContractError::new(
            "v2/policy.json genesis",
            "must set every state field",
        ));
    }
    for field in fields {
        let value = rules.genesis[&field.id];
        let (admitted, domain) = match declarations.kind(field.type_id)? {
            Kind::Bool => (
                matches!(value, Constant::Bool(_)),
                "Bool (false or true)".to_owned(),
            ),
            Kind::I128 { min, max } => (
                matches!(value, Constant::Int(n) if min <= n && n <= max),
                format!("{min}..={max}"),
            ),
            Kind::U128 { min, max } => (
                matches!(value, Constant::U128(n) if min <= n && n <= max),
                format!("{min}..={max}"),
            ),
            Kind::Sum(variants) => (
                matches!(value, Constant::Int(n) if variants.iter().any(|variant| i128::from(variant.id) == n)),
                format!(
                    "variant IDs {:?}",
                    variants
                        .iter()
                        .map(|variant| variant.id)
                        .collect::<Vec<_>>()
                ),
            ),
            Kind::Text => (false, "no scalar genesis value for text".to_owned()),
        };
        if !admitted {
            let value = match value {
                Constant::Bool(value) => value.to_string(),
                Constant::Int(value) => value.to_string(),
                Constant::U128(value) => value.to_string(),
            };
            return Err(ContractError::new(
                format!("v2/policy.json genesis.{}", field.id),
                format!(
                    "field {} ({}) has genesis value {value}; expected {domain}",
                    field.id, field.name
                ),
            ));
        }
    }
    Ok(rules)
}

#[cfg(test)]
mod tests {
    use super::*;
    const PROJECT: &str = include_str!("../../../../examples/guide-counter/project.zeno");
    const RULES: &str = include_str!("../../../../examples/guide-counter/v2/policy.json");

    fn rules(value: i128) -> String {
        let mut rules: serde_json::Value =
            serde_json::from_str(RULES).unwrap_or_else(|error| panic!("fixture: {error}"));
        rules["genesis"]["110"] = serde_json::json!(0);
        rules.to_string().replace(
            "\"genesis\":{\"110\":0}",
            &format!("\"genesis\":{{\"110\":{value}}}"),
        )
    }

    #[test]
    fn signed_range_checks_both_endpoints_without_enumeration_or_overflow() {
        for value in [0, 1, 2] {
            assert!(check(PROJECT, &rules(value)).is_ok());
        }
        for value in [-1, 3] {
            let error = check(PROJECT, &rules(value))
                .err()
                .unwrap_or_else(|| panic!("admitted {value}"));
            assert!(
                error.reason().contains(&format!("value {value}"))
                    && error.reason().contains("0..=2")
            );
        }
        let wide = PROJECT.replace("0..=2;", &format!("{}..={};", i128::MIN, i128::MAX));
        for value in [i128::MIN, i128::MAX] {
            assert!(check(&wide, &rules(value)).is_ok());
        }
    }

    #[test]
    fn domain_admission_does_not_admit_genesis_laws() {
        let project = format!("{PROJECT}\nlaw 550 startup_guard on accept, genesis = 0 == 1;\n");
        let mut rules: serde_json::Value =
            serde_json::from_str(RULES).unwrap_or_else(|error| panic!("fixture: {error}"));
        rules["law_kinds"]["550"] = serde_json::json!("FeeAndRounding");
        assert!(check(&project, &rules.to_string()).is_ok());
        let result = crate::contract::review::profile::profile(&project, &rules.to_string(), 1024)
            .unwrap_or_else(|error| panic!("profile: {error}"));
        assert!(matches!(
            result,
            crate::contract::review::profile::Profile::NoStartingState(_)
        ));
    }

    #[test]
    fn shipped_scalar_contracts_keep_their_declared_genesis_domains() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        for directory in [
            "crates/zeno-fcis-cli/templates/account-lockout",
            "crates/zeno-fcis-cli/templates/order-fulfillment",
            "crates/zeno-fcis-cli/templates/inventory-reservation",
            "crates/zeno-fcis-cli/templates/compliance-gateway",
            "crates/zeno-fcis-cli/templates/withdrawal-queue",
            "crates/zeno-fcis-cli/templates/agent-treasury-guard",
            "examples/apps/library-loans",
            "examples/apps/escrow-deal",
            "site/zenoui/test-projects/wide-counter",
        ] {
            let path = root.join(directory);
            let project = std::fs::read_to_string(path.join("project.zeno"))
                .unwrap_or_else(|error| panic!("{directory}: {error}"));
            let rules = std::fs::read_to_string(path.join("v2/policy.json"))
                .unwrap_or_else(|error| panic!("{directory}: {error}"));
            assert!(check(&project, &rules).is_ok(), "{directory}");
        }
    }
}
