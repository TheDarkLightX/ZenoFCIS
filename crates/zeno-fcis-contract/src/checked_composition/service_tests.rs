//! Service authority refusal, ownership, genesis and order controls.
use super::{
    domain_tests::{Built, budget, counter, fail},
    hex,
    leaf::sha256,
    service::{ServiceAuthority, ServiceRequest},
};
use serde_json::{Value, json};
fn subject() -> (String, Vec<super::LeafSources>) {
    subject_for(vec![counter(), budget()])
}
fn subject_for(parts: Vec<super::domain_tests::Part>) -> (String, Vec<super::LeafSources>) {
    let mut built = Built::new(parts, json!([]));
    for core in built.manifest["components"]
        .as_array_mut()
        .unwrap_or_else(|| panic!("components"))
    {
        core["participates"] = json!("true");
    }
    let schemas = built
        .sources
        .iter()
        .map(|source| {
            let rules = crate::internals::rules::Rules::read(&source.rules).unwrap_or_else(fail);
            let declarations = crate::internals::declarations::Declarations::read(
                &source.project,
                &rules.leaf_bindings,
            )
            .unwrap_or_else(fail);
            hex(&sha256(
                &crate::internals::schema::encode(&declarations).unwrap_or_else(fail),
            ))
        })
        .collect::<Vec<_>>();
    let manifest = json!({"schema":"zeno-fcis/service/1","composition":built.manifest,"schemas":schemas,"laws":[],"max_steps":1000000});
    (manifest.to_string(), built.sources)
}
fn authority() -> ServiceAuthority {
    let (text, sources) = subject();
    ServiceAuthority::from_sources(&text, &sources).unwrap_or_else(fail)
}
#[test]
fn any_refusal_discards_all_provisional_states() {
    let service = authority();
    let state = service.state(&[0, 1]).unwrap_or_else(fail);
    let before = state.bytes();
    let request = ServiceRequest::new("local", "reject", &[0, 2, 1]).unwrap_or_else(fail);
    let result = service.decide(&state, &request, 0);
    assert!(
        result.is_err(),
        "one core refuses: neither state may commit"
    );
    let refusal = result.err().unwrap_or_else(|| panic!("refusal"));
    assert!(refusal.reason.starts_with("reject:budget"));
    assert!(refusal.usage.core()[7] > 0);
    assert!(refusal.usage.wrapper() > 0);
    assert_eq!(state.bytes(), before);
}
#[test]
fn first_refusal_obeys_manifest_order() {
    let service = authority();
    let state = service.state(&[2, 0]).unwrap_or_else(fail);
    let request = ServiceRequest::new("local", "ordered", &[0, 2, 0]).unwrap_or_else(fail);
    let refusal = service
        .decide(&state, &request, 0)
        .err()
        .unwrap_or_else(|| panic!("must refuse"));
    assert!(
        refusal.reason.starts_with("reject:counter"),
        "fixed order selects counter refusal: {}",
        refusal.reason
    );
}
#[test]
fn a_field_with_two_owners_is_refused() {
    let (text, sources) = subject();
    let mut manifest: Value = serde_json::from_str(&text).unwrap_or_else(fail);
    manifest["composition"]["components"][1]["owns"] = json!(["counter.value"]);
    assert!(
        ServiceAuthority::from_sources(&manifest.to_string(), &sources).is_err(),
        "shared owner must be refused"
    );
}
#[test]
fn global_genesis_and_joint_outcome_laws_are_enforced() {
    let (text, sources) = subject();
    let mut manifest: Value = serde_json::from_str(&text).unwrap_or_else(fail);
    manifest["laws"] =
        json!([{"name":"genesis","formula":"post.counter.value == 1","genesis":true}]);
    assert!(ServiceAuthority::from_sources(&manifest.to_string(), &sources).is_err());
    manifest["laws"] = json!([{"name":"outcome","formula":"post.counter.value + post.budget.remaining == 2","genesis":true}]);
    let service =
        ServiceAuthority::from_sources(&manifest.to_string(), &sources).unwrap_or_else(fail);
    let state = service.state(&[0, 2]).unwrap_or_else(fail);
    let request = ServiceRequest::new("local", "joint", &[0, 2, 1]).unwrap_or_else(fail);
    assert_eq!(
        service
            .decide(&state, &request, 0)
            .err()
            .unwrap_or_else(|| panic!("law refusal"))
            .reason,
        "law:outcome"
    );
}
#[test]
fn inputs_and_results_are_owned_and_canonical() {
    let service = authority();
    let genesis = service.genesis().unwrap_or_else(fail);
    assert_eq!(genesis.post().values(), [0, 2]);
    let mut values = vec![0, 2];
    let state = service.state(&values).unwrap_or_else(fail);
    values[0] = 1;
    let mut args = vec![0, 1, 1];
    let request = ServiceRequest::new("local", "owned", &args).unwrap_or_else(fail);
    args[1] = 2;
    let publication = service.decide(&state, &request, 0).unwrap_or_else(fail);
    assert!(publication.matches_receipt(&publication.receipt()));
    let mut truncated = publication.receipt();
    truncated.pop();
    assert!(!publication.matches_receipt(&truncated));
    assert_eq!(publication.post().values(), [1, 1]);
    assert_eq!(state.values(), [0, 2]);
    assert_eq!(
        service
            .decode_request(&request.bytes())
            .unwrap_or_else(fail),
        request
    );
    assert_eq!(
        service.decode_state(&state.bytes()).unwrap_or_else(fail),
        state
    );
    assert_eq!(
        publication.receipt(),
        service
            .decide(&state, &request, 0)
            .unwrap_or_else(fail)
            .receipt()
    );
    let mut bytes = request.bytes();
    bytes.push(0);
    assert!(service.decode_request(&bytes).is_err());
}
#[test]
fn committed_failure_and_budget_exhaustion_cannot_publish() {
    let (text, sources) = subject();
    let mut manifest: Value = serde_json::from_str(&text).unwrap_or_else(fail);
    manifest["max_steps"] = json!(1);
    assert!(ServiceAuthority::from_sources(&manifest.to_string(), &sources).is_err());
    let wire = super::domain_tests::part(
        "wire",
        "faults/tripwire",
        &["tripped", "touches"],
        "true",
        json!({"fire":"qty == 2","authorized":"authorized"}),
        "true",
        "true",
    );
    let (text, sources) = subject_for(vec![wire, budget()]);
    let error = ServiceAuthority::from_sources(&text, &sources)
        .err()
        .unwrap_or_else(|| panic!("CommittedFailure profile"));
    assert!(error.contains("service-profile:wire"), "{error}");
}

#[test]
fn remaining_budget_is_checked_before_invoking_a_core() {
    let mut component = counter();
    assert!(component.sources.project.contains("budget steps 64;"));
    component.sources.project = component
        .sources
        .project
        .replace("budget steps 64;", "budget steps 100000;");
    let [project, policy, program] = super::domain_tests::pins(&component.sources);
    component.entry["project_sha256"] = json!(project);
    component.entry["policy_sha256"] = json!(policy);
    component.entry["program_sha256"] = json!(program);
    let (text, sources) = subject_for(vec![component]);
    let service = ServiceAuthority::from_sources(&text, &sources).unwrap_or_else(fail);
    let genesis = service.genesis().unwrap_or_else(fail);
    let actual = genesis.usage().wrapper() + genesis.usage().core()[7];
    assert!(
        actual < 100000,
        "witness must separate actual work from checked reservation"
    );
    let mut limited: Value = serde_json::from_str(&text).unwrap_or_else(fail);
    limited["max_steps"] = json!(actual);
    let error = ServiceAuthority::from_sources(&limited.to_string(), &sources)
        .err()
        .unwrap_or_else(|| panic!("unaffordable checked core must not start"));
    assert_eq!(error, "genesis:service-budget");
}
