//! The order-fulfillment example's authority, program, and law checker, run
//! in the browser over the library's in-memory reference shell.
//!
//! The step, the report, and the C ABI are `zeno-fcis-site-common`'s. This
//! crate names the application's types, its exact genesis, and the mapping
//! from the page's request to the template's command and context, and
//! exports the module's `demo_reset`.

use order_fulfillment::{
    authority,
    bindings::GeneratedProject,
    command,
    delivery::Destination,
    generated::{Caller, CallerContext, Order, OrderAction, OrderCommand},
    genesis_state,
    laws::OrderLaws,
    profile,
    program::OrderProgram,
};
use zeno_fcis_site_common::{
    Application, Authority, Json, Map, Names, Request, RustCryptoSha256, SchemaAdmittedEnvelope,
    SchemaAdmittedTypeEnvelope, ValidationLimits, Value,
};

/// The template, as `zeno-fcis new --template order-fulfillment` writes it.
pub struct OrderFulfillment;

/// Renders an error as the template renders its own: with `Debug`.
fn checked<T, E: std::fmt::Debug>(value: Result<T, E>) -> Result<T, String> {
    value.map_err(|error| format!("{error:?}"))
}

fn admit_root(order: &Order) -> Result<SchemaAdmittedEnvelope, String> {
    let project = checked(GeneratedProject::try_new::<RustCryptoSha256>())?;
    checked(project.admit_root::<RustCryptoSha256>(order, ValidationLimits::default()))
}

impl Application for OrderFulfillment {
    const NAME: &'static str = "order-fulfillment";
    type Program = OrderProgram;
    type Laws = OrderLaws;
    type Destination = Destination;
    type Command = OrderCommand;
    type Context = CallerContext;

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
        admit_root(&checked(Order::try_from_value(state))?)
    }

    /// `command` is one of the six actions; a payment provider's callback,
    /// `PaymentCaptured` or `PaymentDeclined`, also carries
    /// `callback_attempt`, the attempt it answers. The context is `caller`:
    /// `Customer`, `PaymentProvider`, or `Carrier`.
    fn parse(request: &Map<String, Json>) -> Result<(OrderCommand, CallerContext), String> {
        let mut request = Request::new(request);
        let action = request.choice(
            "command",
            &[
                ("Checkout", OrderAction::Checkout),
                ("PaymentCaptured", OrderAction::PaymentCaptured),
                ("PaymentDeclined", OrderAction::PaymentDeclined),
                ("ParcelDispatched", OrderAction::ParcelDispatched),
                ("ParcelDelivered", OrderAction::ParcelDelivered),
                ("CancelOrder", OrderAction::CancelOrder),
            ],
        )?;
        let callback_attempt = match action {
            OrderAction::PaymentCaptured | OrderAction::PaymentDeclined => {
                request.integer("callback_attempt")?
            }
            _ => 0,
        };
        let caller = request.choice(
            "caller",
            &[
                ("Customer", Caller::Customer),
                ("PaymentProvider", Caller::PaymentProvider),
                ("Carrier", Caller::Carrier),
            ],
        )?;
        request.finish()?;
        Ok((command(action, callback_attempt), CallerContext { caller }))
    }

    fn admit(
        command: &OrderCommand,
        context: &CallerContext,
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
    zeno_fcis_site_common::abi::reset::<OrderFulfillment>()
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use zeno_fcis_site_common::serde_json::json;
    use zeno_fcis_site_common::{Demo, Stage};

    fn request(command: &str, caller: &str) -> String {
        format!(r#"{{"command":"{command}","caller":"{caller}"}}"#)
    }

    fn callback(command: &str, attempt: i64, caller: &str) -> String {
        format!(r#"{{"command":"{command}","callback_attempt":{attempt},"caller":"{caller}"}}"#)
    }

    fn order(status: &str, payment_attempts: i64) -> Json {
        json!({ "status": status, "payment_attempts": payment_attempts })
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
        let state = Demo::<OrderFulfillment>::new().unwrap().state().unwrap();
        assert_eq!(state["state"], order("Placed", 0));
        assert_eq!(state["bundles"], 0);
        assert_eq!(state["outbox"], json!([]));
    }

    #[test]
    fn a_checkout_queues_a_numbered_capture_request() {
        let mut demo = Demo::<OrderFulfillment>::new().unwrap();
        let report = demo.step(&request("Checkout", "Customer")).unwrap();
        assert_eq!(report["decision"], "Accept");
        assert_eq!(report["after"], order("AwaitingPayment", 1));
        let queued = report["outbox"].as_array().unwrap();
        assert_eq!(queued.len(), 1);
        assert_eq!(queued[0]["channel"], 300);
        assert_eq!(queued[0]["channel_name"], "payment_request");
        assert_eq!(queued[0]["destination"], "payment-provider");
        assert_eq!(
            queued[0]["payload"],
            json!({ "request_attempt": 1, "request_action": "Capture" })
        );
        let paid = demo
            .step(&callback("PaymentCaptured", 1, "PaymentProvider"))
            .unwrap();
        assert_eq!(paid["after"], order("Paid", 1));
        assert_eq!(paid["outbox"][0]["channel_name"], "shipping_request");
        assert_eq!(paid["outbox"][0]["destination"], "carrier");
        assert_eq!(paid["outbox"][0]["payload"], json!({ "paid_attempt": 1 }));
    }

    #[test]
    fn a_decline_is_a_committed_failure_and_a_wrong_caller_is_rejected() {
        let mut demo = Demo::<OrderFulfillment>::new().unwrap();
        demo.step(&request("Checkout", "Customer")).unwrap();
        let declined = demo
            .step(&callback("PaymentDeclined", 1, "PaymentProvider"))
            .unwrap();
        assert_eq!(declined["decision"], "CommittedFailure");
        assert_eq!(
            declined["reason"],
            json!({ "id": 204, "name": "payment_declined" })
        );
        assert_eq!(declined["after"], order("Placed", 1));
        assert_eq!(declined["outbox"], json!([]));
        demo.step(&request("Checkout", "Customer")).unwrap();
        let rejected = demo
            .step(&callback("PaymentCaptured", 2, "Customer"))
            .unwrap();
        assert_eq!(rejected["decision"], "Reject");
        assert_eq!(
            rejected["reason"],
            json!({ "id": 200, "name": "wrong_caller" })
        );
        assert_eq!(rejected["after"], rejected["before"]);
        assert_eq!(rejected["commit"], Json::Null);
    }

    #[test]
    fn law_statuses_follow_the_manifest_scopes() {
        let mut demo = Demo::<OrderFulfillment>::new().unwrap();
        let accept = demo.step(&request("Checkout", "Customer")).unwrap();
        assert_eq!(law_ids(&accept), [500, 501, 502, 503, 504, 505]);
        let failure = demo
            .step(&callback("PaymentDeclined", 1, "PaymentProvider"))
            .unwrap();
        assert_eq!(law_ids(&failure), [500, 506]);
        let reject = demo.step(&request("ParcelDelivered", "Carrier")).unwrap();
        assert_eq!(law_ids(&reject), [509]);
    }

    #[test]
    fn a_malformed_request_is_refused_before_any_decision() {
        let mut demo = Demo::<OrderFulfillment>::new().unwrap();
        for (input, stage, message) in [
            (
                r#"{"command":"Checkout","callback_attempt":1,"caller":"Customer"}"#,
                Stage::Input,
                "unexpected field \"callback_attempt\"",
            ),
            (
                r#"{"command":"PaymentCaptured","caller":"PaymentProvider"}"#,
                Stage::Input,
                "\"callback_attempt\" is required",
            ),
            (
                r#"{"command":"Checkout","caller":"Nobody"}"#,
                Stage::Input,
                "\"caller\" must be one of Customer, PaymentProvider, Carrier",
            ),
            (
                r#"{"command":"PaymentCaptured","callback_attempt":4,"caller":"PaymentProvider"}"#,
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
