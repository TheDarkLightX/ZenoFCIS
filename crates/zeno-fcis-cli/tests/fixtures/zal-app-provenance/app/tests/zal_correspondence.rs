//! ZAL initial state and finite reference decisions versus the library Authority.
#[test]
fn zal_whole_declared_domain_matches_authority() {
    let contract = application::v2_contract::Contract::new();
    let descriptor = contract.descriptor();
    let authority = application::authority(&descriptor).unwrap_or_else(|e| panic!("{e}"));
    let expected_initial: &[i128] = &[150, 0];
    assert_eq!(application::genesis_numbers().unwrap_or_else(|e| panic!("{e}")),
               expected_initial, "contract genesis differs from the ZAL initial state");
    // This literal comes from the ZAL interpreter, independently of the contract emitter.
    // Bind the entire table, including refusals, before claiming whole-domain coverage.
    let reference = "150 0 120 1 0 | reject 200 150 0 | -\n150 0 120 2 0 | reject 200 150 0 | -\n150 0 120 1 1 | accept - 151 1 | -\n150 0 120 2 1 | accept - 151 2 | -\n150 0 121 1 0 | reject 201 150 0 | -\n150 0 121 2 0 | reject 201 150 0 | -\n150 0 121 1 1 | reject 201 150 0 | -\n150 0 121 2 1 | reject 201 150 0 | -\n150 1 120 1 0 | reject 200 150 1 | -\n150 1 120 2 0 | reject 200 150 1 | -\n150 1 120 1 1 | accept - 151 1 | -\n150 1 120 2 1 | accept - 151 2 | -\n150 1 121 1 0 | reject 201 150 1 | -\n150 1 121 2 0 | reject 201 150 1 | -\n150 1 121 1 1 | reject 201 150 1 | -\n150 1 121 2 1 | reject 201 150 1 | -\n150 2 120 1 0 | reject 200 150 2 | -\n150 2 120 2 0 | reject 200 150 2 | -\n150 2 120 1 1 | accept - 151 1 | -\n150 2 120 2 1 | accept - 151 2 | -\n150 2 121 1 0 | reject 201 150 2 | -\n150 2 121 2 0 | reject 201 150 2 | -\n150 2 121 1 1 | reject 201 150 2 | -\n150 2 121 2 1 | reject 201 150 2 | -\n151 0 120 1 0 | reject 201 151 0 | -\n151 0 120 2 0 | reject 201 151 0 | -\n151 0 120 1 1 | reject 201 151 0 | -\n151 0 120 2 1 | reject 201 151 0 | -\n151 0 121 1 0 | reject 201 151 0 | -\n151 0 121 2 0 | reject 201 151 0 | -\n151 0 121 1 1 | accept - 150 0 | -\n151 0 121 2 1 | accept - 150 0 | -\n151 1 120 1 0 | reject 201 151 1 | -\n151 1 120 2 0 | reject 201 151 1 | -\n151 1 120 1 1 | reject 201 151 1 | -\n151 1 120 2 1 | reject 201 151 1 | -\n151 1 121 1 0 | reject 201 151 1 | -\n151 1 121 2 0 | reject 201 151 1 | -\n151 1 121 1 1 | accept - 150 0 | -\n151 1 121 2 1 | accept - 150 0 | -\n151 2 120 1 0 | reject 201 151 2 | -\n151 2 120 2 0 | reject 201 151 2 | -\n151 2 120 1 1 | reject 201 151 2 | -\n151 2 120 2 1 | reject 201 151 2 | -\n151 2 121 1 0 | reject 201 151 2 | -\n151 2 121 2 0 | reject 201 151 2 | -\n151 2 121 1 1 | accept - 150 0 | -\n151 2 121 2 1 | accept - 150 0 | -\n";
    assert_eq!(include_str!("zal-authority-examples.txt"), reference,
               "ZAL reference table changed: missing, repeated or altered decisions");
    let rows = application::examples(reference)
        .unwrap_or_else(|e| panic!("{e}"));
    assert!(!rows.is_empty());
    for row in &rows {
        let checked = application::check(&authority, row);
        assert!(checked.is_ok(), "ZAL decision differs at inputs {:?}: {:?}", row.inputs, checked);
    }
    println!("ZAL whole-domain tuples checked: {}", rows.len());
}
