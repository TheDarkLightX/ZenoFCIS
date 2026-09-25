//! The inventory-reservation example's authority, synthesized step, and law
//! checker, run in the browser over the library's in-memory reference shell.
//!
//! The step, the report, and the C ABI are `zeno-fcis-site-common`'s. This
//! crate names the application's types, its exact genesis, and the mapping
//! from the page's request to the template's command and context, and
//! exports the module's `demo_reset`.

use inventory_reservation::{
    authority,
    bindings::GeneratedProject,
    delivery::Destination,
    generated::{OperatorFlag, Stock, StockAction, StockCommand, StockContext},
    genesis_state,
    laws::StockLaws,
    profile,
    program::StockProgram,
    request,
};
use zeno_fcis_site_common::{
    Application, Authority, Json, Map, Names, Request, RustCryptoSha256, SchemaAdmittedEnvelope,
    SchemaAdmittedTypeEnvelope, ValidationLimits, Value,
};

/// The template, as `zeno-fcis new --template inventory-reservation` writes it.
pub struct InventoryReservation;

/// Renders an error as the template renders its own: with `Debug`.
fn checked<T, E: std::fmt::Debug>(value: Result<T, E>) -> Result<T, String> {
    value.map_err(|error| format!("{error:?}"))
}

fn admit_root(stock: &Stock) -> Result<SchemaAdmittedEnvelope, String> {
    let project = checked(GeneratedProject::try_new::<RustCryptoSha256>())?;
    checked(project.admit_root::<RustCryptoSha256>(stock, ValidationLimits::default()))
}

impl Application for InventoryReservation {
    const NAME: &'static str = "inventory-reservation";
    type Program = StockProgram;
    type Laws = StockLaws;
    type Destination = Destination;
    type Command = StockCommand;
    type Context = StockContext;

    fn authority() -> Result<Authority<Self>, String> {
        authority()
    }

    fn names() -> Result<Names, String> {
        Ok(Names::new(&profile::project(), &profile::manifest()))
    }

    fn genesis() -> Result<SchemaAdmittedEnvelope, String> {
        admit_root(&genesis_state())
    }

    fn admit_state(state: Value) -> Result<SchemaAdmittedEnvelope, String> {
        admit_root(&checked(Stock::try_from_value(state))?)
    }

    /// `command` is `Reserve`, `Release`, `Ship`, or `Restock`, each with its
    /// `quantity`; the context is `authorized`, whether the operator is.
    fn parse(fields: &Map<String, Json>) -> Result<(StockCommand, StockContext), String> {
        let mut fields = Request::new(fields);
        let action = fields.choice(
            "command",
            &[
                ("Reserve", StockAction::Reserve),
                ("Release", StockAction::Release),
                ("Ship", StockAction::Ship),
                ("Restock", StockAction::Restock),
            ],
        )?;
        let quantity = fields.integer("quantity")?;
        let authorized = OperatorFlag(fields.flag("authorized")?);
        fields.finish()?;
        Ok((request(action, quantity), StockContext { authorized }))
    }

    fn admit(
        command: &StockCommand,
        context: &StockContext,
    ) -> Result<(SchemaAdmittedTypeEnvelope, SchemaAdmittedTypeEnvelope), String> {
        let project = checked(GeneratedProject::try_new::<RustCryptoSha256>())?;
        let limits = ValidationLimits::default();
        let command = checked(project.admit_command::<RustCryptoSha256>(command, limits))?;
        let context = checked(project.admit_context::<RustCryptoSha256>(context, limits))?;
        Ok((command.admitted().clone(), context.admitted().clone()))
    }
}

/// Builds the demo at the exact genesis and returns its state.
// SAFETY: the `demo_` prefix keeps every exported symbol unique.
#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn demo_reset() -> u32 {
    zeno_fcis_site_common::abi::reset::<InventoryReservation>()
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use zeno_fcis_site_common::serde_json::json;
    use zeno_fcis_site_common::{Demo, Stage};

    fn request(command: &str, quantity: i64, authorized: bool) -> String {
        format!(r#"{{"command":"{command}","quantity":{quantity},"authorized":{authorized}}}"#)
    }

    fn stock(available: i64, reserved: i64) -> Json {
        json!({ "available": available, "reserved": reserved })
    }

    fn law_ids(report: &Json) -> Vec<u64> {
        report["laws"]
            .as_array()
            .unwrap()
            .iter()
            .map(|law| {
                assert_eq!(law["status"], "Satisfied", "{law}");
                law["id"].as_u64().unwrap()
            })
            .collect()
    }

    #[test]
    fn a_new_demo_holds_the_exact_genesis() {
        let state = Demo::<InventoryReservation>::new()
            .unwrap()
            .state()
            .unwrap();
        assert_eq!(state["state"], stock(0, 0));
        assert_eq!(state["bundles"], 0);
        assert_eq!(state["outbox"], json!([]));
    }

    #[test]
    fn stock_moves_as_the_table_says_and_a_shipment_is_queued() {
        let mut demo = Demo::<InventoryReservation>::new().unwrap();
        let restocked = demo.step(&request("Restock", 3, true)).unwrap();
        assert_eq!(restocked["decision"], "Accept");
        assert_eq!(restocked["after"], stock(3, 0));
        assert_eq!(restocked["outbox"], json!([]));
        let reserved = demo.step(&request("Reserve", 2, true)).unwrap();
        assert_eq!(reserved["after"], stock(1, 2));
        let shipped = demo.step(&request("Ship", 2, true)).unwrap();
        assert_eq!(shipped["after"], stock(1, 0));
        let queued = shipped["outbox"].as_array().unwrap();
        assert_eq!(queued.len(), 1);
        assert_eq!(queued[0]["channel"], 300);
        assert_eq!(queued[0]["channel_name"], "shipment");
        assert_eq!(queued[0]["destination"], "warehouse");
        assert_eq!(queued[0]["payload"], json!({ "shipped_units": 2 }));
    }

    #[test]
    fn an_unauthorized_operator_is_rejected_first() {
        let mut demo = Demo::<InventoryReservation>::new().unwrap();
        let report = demo.step(&request("Ship", 1, false)).unwrap();
        assert_eq!(report["decision"], "Reject");
        assert_eq!(
            report["reason"],
            json!({ "id": 200, "name": "not_authorized" })
        );
        assert_eq!(report["after"], stock(0, 0));
        let insufficient = demo.step(&request("Ship", 1, true)).unwrap();
        assert_eq!(
            insufficient["reason"],
            json!({ "id": 202, "name": "insufficient_reserved" })
        );
        assert_eq!(demo.state().unwrap()["bundles"], 0);
    }

    #[test]
    fn law_statuses_follow_the_manifest_scopes() {
        let mut demo = Demo::<InventoryReservation>::new().unwrap();
        let accept = demo.step(&request("Restock", 1, true)).unwrap();
        assert_eq!(law_ids(&accept), [500, 501, 502]);
        assert_eq!(accept["laws"][1]["name"], "units_conserved");
        let reject = demo.step(&request("Restock", 1, false)).unwrap();
        assert_eq!(law_ids(&reject), [509]);
    }

    #[test]
    fn a_malformed_request_is_refused_before_any_decision() {
        let mut demo = Demo::<InventoryReservation>::new().unwrap();
        for (input, stage, message) in [
            (
                r#"{"command":"Ship","quantity":1}"#,
                Stage::Input,
                "\"authorized\" is required",
            ),
            (
                r#"{"command":"Ship","quantity":1,"authorized":true,"lane":1}"#,
                Stage::Input,
                "unexpected field \"lane\"",
            ),
            (
                r#"{"command":"Ship","quantity":0,"authorized":true}"#,
                Stage::Admission,
                "IntegerRange",
            ),
            (
                r#"{"command":"Restock","quantity":4,"authorized":true}"#,
                Stage::Admission,
                "IntegerRange",
            ),
        ] {
            let refusal = demo.step(input).unwrap_err();
            assert_eq!(refusal.stage(), stage, "{input}: {refusal:?}");
            assert!(
                refusal.to_json()["error"]
                    .as_str()
                    .unwrap()
                    .contains(message),
                "{input}: {refusal:?}"
            );
        }
        assert_eq!(demo.state().unwrap()["steps"], 0);
    }
}
