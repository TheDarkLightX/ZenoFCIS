use super::*;
#[path = "../tests/support/vector_data.rs"]
mod data;
fn compare(name: &str) -> bool {
    std::env::var("BLS_DISABLE_COMPARISON").ok().as_deref() != Some(name)
}
#[test]
fn pk_flag_rejects_torsion() {
    let v = data::named("verify-neg-torsion-pk-order3");
    let pk = PublicKey::from_bytes(&v.a[0]).unwrap();
    let result = verify_parsed(
        &pk,
        &v.b[0],
        &signature(&v.c[0]).unwrap(),
        BASIC_DST,
        Stage::Verification,
    );
    if compare("pk") {
        assert!(result.is_err(), "TORSION_KEY_ACCEPTED");
    }
    // The public boundary has an independent key_validate defence as well.
    assert!(validate_key(&v.a[0]).is_err());
}
#[test]
fn sig_flag_rejects_torsion() {
    let v = data::named("verify-neg-torsion-sig-order13");
    let result = basic_verify(&v.a[0], &v.b[0], &v.c[0]);
    if compare("sig") {
        // The supplied torsion signature also fails the raw pairing with the
        // check disabled. Independently enforce the mandatory call flag.
        assert!(SIGNATURE_GROUP_CHECK, "SIG_GROUPCHECK_DISABLED");
        assert!(result.is_err(), "TORSION_SIGNATURE_ACCEPTED");
    }
}
#[test]
fn rogue_key_gate() {
    let v = data::named("rogue-key-pop-neg-proof-signed-with-a");
    let result = PopKey::admit(&v.a[0], &v.c[0]);
    if compare("pop") {
        assert!(result.is_err(), "ROGUE_KEY_ADMITTED");
    }
}
#[test]
fn basic_message_uniqueness() {
    let v = data::named("basic-aggregate-verify-duplicate-msgs");
    let result = basic_aggregate_verify(
        &v.a.iter().map(Vec::as_slice).collect::<Vec<_>>(),
        &v.b.iter().map(Vec::as_slice).collect::<Vec<_>>(),
        &v.c[0],
    );
    if compare("unique") {
        assert_eq!(
            result,
            Err(Refusal::DuplicateMessage),
            "DUPLICATE_MESSAGES_ACCEPTED"
        );
    }
}
#[test]
fn basic_dst_compatibility() {
    let v = data::named("basic-verify-1");
    let result = basic_verify(&v.a[0], &v.b[0], &v.c[0]);
    if compare("dst") {
        assert_eq!(result, Ok(()), "WRONG_DST");
    }
}
#[test]
fn errors_are_precise() {
    assert_eq!(validate_key(&[0; 47]), Err(Refusal::PublicKeyLength));
    let v = data::named("key-neg-infinity");
    assert_eq!(
        validate_key(&v.a[0]),
        Err(Refusal::Crypto(
            Stage::PublicKey,
            CryptoError::PublicKeyInfinity
        ))
    );
    assert_eq!(
        basic_aggregate_verify(&[], &[], &[]),
        Err(Refusal::EmptyAggregate)
    );
}
