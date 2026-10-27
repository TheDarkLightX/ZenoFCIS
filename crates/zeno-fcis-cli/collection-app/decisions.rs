//! Contract examples still decide through the exact emitted Authority.
#[test]
fn every_example_is_the_authority_decision() {
    let contract = application::v2_contract::Contract::new();
    let descriptor = contract.descriptor();
    let authority = application::authority(&descriptor).unwrap_or_else(|error| panic!("{error}"));
    let examples = application::examples(include_str!("decision-examples.txt"))
        .unwrap_or_else(|error| panic!("{error}"));
    for example in &examples {
        application::check(&authority, example).unwrap_or_else(|error| panic!("{error}"));
    }
    println!("decision examples checked: {}", examples.len());
}
