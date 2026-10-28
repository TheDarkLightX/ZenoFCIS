//! ZAL initial state and finite reference decisions versus the library Authority.
#[test]
fn zal_whole_declared_domain_matches_authority() {
    let contract = application::v2_contract::Contract::new();
    let descriptor = contract.descriptor();
    let authority = application::authority(&descriptor).unwrap_or_else(|e| panic!("{e}"));
    let expected_initial: &[i128] = &[152];
    assert_eq!(application::genesis_numbers().unwrap_or_else(|e| panic!("{e}")),
               expected_initial, "contract genesis differs from the ZAL initial state");
    // This literal comes from the ZAL interpreter, independently of the contract emitter.
    // Bind the entire table, including refusals, before claiming whole-domain coverage.
    let reference = "150 120 0 | reject 200 150 | -\n150 120 1 | reject 200 150 | -\n150 121 0 | reject 200 150 | -\n150 121 1 | reject 200 150 | -\n150 122 0 | reject 200 150 | -\n150 122 1 | reject 200 150 | -\n151 120 0 | reject 200 151 | -\n151 120 1 | reject 200 151 | -\n151 121 0 | reject 200 151 | -\n151 121 1 | reject 200 151 | -\n151 122 0 | reject 200 151 | -\n151 122 1 | reject 200 151 | -\n152 120 0 | reject 200 152 | -\n152 120 1 | reject 200 152 | -\n152 121 0 | reject 200 152 | -\n152 121 1 | reject 200 152 | -\n152 122 0 | accept - 153 | -\n152 122 1 | accept - 153 | -\n153 120 0 | reject 200 153 | -\n153 120 1 | accept - 150 | -\n153 121 0 | accept - 151 | -\n153 121 1 | accept - 151 | -\n153 122 0 | reject 200 153 | -\n153 122 1 | reject 200 153 | -\n";
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
