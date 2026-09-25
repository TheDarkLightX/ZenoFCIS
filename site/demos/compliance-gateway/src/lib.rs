//! The compliance-gateway example's authority, synthesized step, rule base,
//! and law checker, run in the browser over the library's in-memory
//! reference shell.
//!
//! The step, the report, and the C ABI are `zeno-fcis-site-common`'s. This
//! crate names the application's types, its exact genesis, and the mapping
//! from the page's request to the template's command and context, and
//! exports the module's `demo_reset`.

use compliance_gateway::{
    authority,
    bindings::GeneratedProject,
    context,
    delivery::Destination,
    generated::{CallerContext, CounterpartyRisk, GatewayCommand, Region, Standing},
    genesis_state,
    laws::GatewayLaws,
    profile,
    program::GatewayProgram,
    reinstate, screen,
};
use zeno_fcis_site_common::{
    Application, Authority, Json, Map, Names, Request, RustCryptoSha256, SchemaAdmittedEnvelope,
    SchemaAdmittedTypeEnvelope, ValidationLimits, Value,
};

/// The template, as `zeno-fcis new --template compliance-gateway` writes it.
pub struct ComplianceGateway;

/// Renders an error as the template renders its own: with `Debug`.
fn checked<T, E: std::fmt::Debug>(value: Result<T, E>) -> Result<T, String> {
    value.map_err(|error| format!("{error:?}"))
}

fn admit_root(standing: &Standing) -> Result<SchemaAdmittedEnvelope, String> {
    let project = checked(GeneratedProject::try_new::<RustCryptoSha256>())?;
    checked(project.admit_root::<RustCryptoSha256>(standing, ValidationLimits::default()))
}

impl Application for ComplianceGateway {
    const NAME: &'static str = "compliance-gateway";
    type Program = GatewayProgram;
    type Laws = GatewayLaws;
    type Destination = Destination;
    type Command = GatewayCommand;
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
        admit_root(&checked(Standing::try_from_value(state))?)
    }

    /// `command` is `Screen`, with the transfer's `region`, `amount_band`,
    /// and `counterparty_risk`, or `Reinstate`, which carries no transfer.
    /// The context is `identity_tier` and `reviewer`.
    fn parse(request: &Map<String, Json>) -> Result<(GatewayCommand, CallerContext), String> {
        let mut request = Request::new(request);
        let screening = request.choice("command", &[("Screen", true), ("Reinstate", false)])?;
        let command = if screening {
            screen(
                request.choice(
                    "region",
                    &[
                        ("Allowed", Region::Allowed),
                        ("Restricted", Region::Restricted),
                        ("Sanctioned", Region::Sanctioned),
                    ],
                )?,
                request.integer("amount_band")?,
                request.choice(
                    "counterparty_risk",
                    &[
                        ("Low", CounterpartyRisk::Low),
                        ("Medium", CounterpartyRisk::Medium),
                        ("High", CounterpartyRisk::High),
                    ],
                )?,
            )
        } else {
            reinstate()
        };
        let context = context(request.integer("identity_tier")?, request.flag("reviewer")?);
        request.finish()?;
        Ok((command, context))
    }

    fn admit(
        command: &GatewayCommand,
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
    zeno_fcis_site_common::abi::reset::<ComplianceGateway>()
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use zeno_fcis_site_common::serde_json::json;
    use zeno_fcis_site_common::{Demo, Stage};

    fn screening(region: &str, band: i64, risk: &str, tier: i64, reviewer: bool) -> String {
        format!(
            r#"{{"command":"Screen","region":"{region}","amount_band":{band},"counterparty_risk":"{risk}","identity_tier":{tier},"reviewer":{reviewer}}}"#
        )
    }

    fn reinstatement(tier: i64, reviewer: bool) -> String {
        format!(r#"{{"command":"Reinstate","identity_tier":{tier},"reviewer":{reviewer}}}"#)
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
        let state = Demo::<ComplianceGateway>::new().unwrap().state().unwrap();
        assert_eq!(state["state"], json!({ "strikes": 0 }));
        assert_eq!(state["bundles"], 0);
        assert_eq!(state["outbox"], json!([]));
    }

    #[test]
    fn a_held_transfer_queues_a_ticket_naming_its_rule() {
        let mut demo = Demo::<ComplianceGateway>::new().unwrap();
        let allowed = demo
            .step(&screening("Allowed", 1, "Low", 2, false))
            .unwrap();
        assert_eq!(allowed["decision"], "Accept");
        assert_eq!(allowed["outbox"], json!([]));
        let held = demo
            .step(&screening("Restricted", 2, "Low", 2, false))
            .unwrap();
        assert_eq!(held["decision"], "Accept");
        assert_eq!(held["after"], json!({ "strikes": 0 }));
        let queued = held["outbox"].as_array().unwrap();
        assert_eq!(queued.len(), 1);
        assert_eq!(queued[0]["channel"], 300);
        assert_eq!(queued[0]["channel_name"], "review_ticket");
        assert_eq!(queued[0]["destination"], "review-queue");
        assert_eq!(
            queued[0]["payload"],
            json!({ "ticket_rule": "RestrictedRegion", "ticket_amount_band": 2 })
        );
    }

    #[test]
    fn a_blocked_transfer_is_a_committed_failure_with_an_alert() {
        let mut demo = Demo::<ComplianceGateway>::new().unwrap();
        let blocked = demo
            .step(&screening("Sanctioned", 0, "Low", 3, false))
            .unwrap();
        assert_eq!(blocked["decision"], "CommittedFailure");
        assert_eq!(
            blocked["reason"],
            json!({ "id": 210, "name": "sanctioned_region" })
        );
        assert_eq!(blocked["after"], json!({ "strikes": 1 }));
        let queued = blocked["outbox"].as_array().unwrap();
        assert_eq!(queued[0]["channel"], 301);
        assert_eq!(queued[0]["channel_name"], "block_alert");
        assert_eq!(queued[0]["destination"], "compliance-team");
        assert_eq!(
            queued[0]["payload"],
            json!({ "alert_rule": "SanctionedRegion", "alert_strikes": 1 })
        );
        let rejected = demo.step(&reinstatement(3, false)).unwrap();
        assert_eq!(rejected["decision"], "Reject");
        assert_eq!(
            rejected["reason"],
            json!({ "id": 200, "name": "not_reviewer" })
        );
        let reinstated = demo.step(&reinstatement(3, true)).unwrap();
        assert_eq!(reinstated["after"], json!({ "strikes": 0 }));
    }

    #[test]
    fn law_statuses_follow_the_manifest_scopes() {
        let mut demo = Demo::<ComplianceGateway>::new().unwrap();
        let failure = demo
            .step(&screening("Sanctioned", 0, "Low", 3, false))
            .unwrap();
        assert_eq!(law_ids(&failure), [500, 503]);
        let accept = demo.step(&reinstatement(3, true)).unwrap();
        assert_eq!(law_ids(&accept), [500, 501, 502]);
        let reject = demo.step(&reinstatement(3, true)).unwrap();
        assert_eq!(law_ids(&reject), [509]);
    }

    #[test]
    fn a_malformed_request_is_refused_before_any_decision() {
        let mut demo = Demo::<ComplianceGateway>::new().unwrap();
        for (input, stage, message) in [
            (
                r#"{"command":"Reinstate","region":"Allowed","identity_tier":3,"reviewer":true}"#,
                Stage::Input,
                "unexpected field \"region\"",
            ),
            (
                r#"{"command":"Screen","region":"Allowed","amount_band":1,"identity_tier":2,"reviewer":false}"#,
                Stage::Input,
                "\"counterparty_risk\" is required",
            ),
            (
                &screening("Allowed", 5, "Low", 2, false),
                Stage::Admission,
                "IntegerRange",
            ),
            (
                &screening("Allowed", 1, "Low", 4, false),
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
