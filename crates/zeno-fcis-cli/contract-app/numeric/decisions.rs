use application::{authority, check, examples, genesis, v2_contract};
use zeno_fcis_synthesis::finite::v2_authority::PublicationOutcome;
#[test]
fn typed_examples_and_original_publications() {
    let c = v2_contract::Contract::new();
    let d = c.descriptor();
    let a = authority(&d).unwrap();
    for e in examples(include_str!("decision-examples.txt")).unwrap() {
        check(&a, &e).unwrap()
    }
}
#[test]
fn original_genesis_publication_replays() {
    let c = v2_contract::Contract::new();
    let d = c.descriptor();
    let a = authority(&d).unwrap();
    let bytes = genesis().unwrap();
    let PublicationOutcome::Commit(p) = a.publish_genesis(&bytes) else {
        panic!("genesis refused")
    };
    assert!(matches!(
        a.replay_genesis_publication(&bytes, p.subject()),
        PublicationOutcome::Commit(_)
    ));
}
