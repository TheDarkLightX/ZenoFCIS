//! Canonical real-key inputs, including context-named entries.
use zeno_fcis_contract::collections::runtime::{Layout, decode_key};
use zeno_fcis_contract::internals::declarations::Source;
const PROJECT: &str = include_str!("../../../examples/collections/token-balances/project.zeno");
const POLICY: &str = include_str!("../../../examples/collections/token-balances/v2/policy.json");
fn ok<T, E: std::fmt::Debug>(r: Result<T, E>) -> T {
    r.unwrap_or_else(|e| panic!("{e:?}"))
}
#[test]
fn command_keys_have_one_fixed_wire_form() {
    let layout = ok(Layout::read(PROJECT, POLICY));
    let values = vec!["150".into(), "ab".repeat(32), "cd".repeat(32), "2".into()];
    let input = ok(layout.decode_input(Source::Command, &values));
    assert_eq!(ok(layout.encode_input(Source::Command, &input)), values);
    for text in [
        "0",
        &"ab".repeat(31),
        &"ab".repeat(33),
        &"AB".repeat(32),
        &"gg".repeat(32),
    ] {
        assert_eq!(decode_key(text), None);
    }
    for text in ["+2", "02", "-0", "4"] {
        let mut changed = values.clone();
        changed[3] = text.into();
        assert!(layout.decode_input(Source::Command, &changed).is_err());
    }
    assert!(layout.decode_input(Source::Command, &values[..3]).is_err());
}
#[test]
fn context_keys_use_the_same_decoder_and_name_storage() {
    let project=PROJECT.replace("type 102 context Desk;","type 102 context Desk;\ntype 108 data Role;\nfield 141 102 role 108;\nfield 142 102 account 103;").replace("180 102","180 108").replace("181 102","181 108").replace("at command.101.131","at context.102.142").replace("context.102 == 180","context.102.141 == 180");
    let policy = POLICY.replace("context.102\"", "context.102.141\"");
    let layout = ok(Layout::read(&project, &policy));
    let command = ok(layout.decode_input(
        Source::Command,
        &["150".into(), "11".repeat(32), "22".repeat(32), "0".into()],
    ));
    let values = vec!["180".into(), "33".repeat(32)];
    let context = ok(layout.decode_input(Source::Context, &values));
    assert_eq!(ok(layout.encode_input(Source::Context, &context)), values);
    let named = ok(layout.named(&command, &context));
    assert!(named[&110].contains(&[0x33; 32]));
    assert!(!named[&110].contains(&[0x11; 32]));
    assert_eq!(named[&110].len(), 2);
    let mut bad = values;
    bad[1] = "33".repeat(31);
    assert!(layout.decode_input(Source::Context, &bad).is_err());
    assert!(
        layout.interface()["fields"]
            .as_array()
            .is_some_and(|fields| fields
                .iter()
                .any(|f| f["source"] == "context" && f["domain"]["kind"] == "key"))
    );
}
