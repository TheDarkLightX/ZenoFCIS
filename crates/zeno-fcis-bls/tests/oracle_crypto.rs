//! Oracle verification and lifecycle regression witnesses.
use zeno_fcis_bls::oracle::OracleVerifier;
use zeno_fcis_synthesis::finite::v2_oracle::*;

fn bytes(s: &str) -> Vec<u8> {
    assert_eq!(s.len() % 2, 0);
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap_or_else(|_| panic!("hex")))
        .collect()
}

#[test]
fn oracle_native_pop_verifies_actual_bytes_and_rejects_suite_encoding_and_subgroup_negatives() {
    let mut valid = 0;
    let mut invalid = 0;
    for row in include_str!("../../zeno-fcis-synthesis/tests/test-data/oracle/crypto.tsv")
        .lines()
        .filter(|r| !r.starts_with('#'))
    {
        let c: Vec<_> = row.split('\t').collect();
        assert_eq!(c.len(), 6);
        let result = verify_bls_pop(
            &OracleVerifier,
            BLS_POP_SCHEME,
            &bytes(c[2]),
            &bytes(c[3]),
            &bytes(c[4]),
            &bytes(c[5]),
        );
        if c[1] == "1" {
            assert_eq!(result, Ok(()), "{}", c[0]);
            valid += 1;
        } else {
            assert_eq!(result, Err(Failure::InvalidEvidence), "{}", c[0]);
            invalid += 1;
        }
    }
    assert_eq!((valid, invalid), (5, 12));
}

#[test]
fn oracle_native_pop_rejects_review_subgroup_encoding_suite_and_domain_negatives() {
    let mut valid = 0;
    let mut invalid = 0;
    for row in
        include_str!("../../zeno-fcis-synthesis/tests/test-data/oracle/review-crypto.tsv").lines()
    {
        let c: Vec<_> = row.split('\t').collect();
        assert_eq!(c.len(), 6);
        let result = verify_bls_pop(
            &OracleVerifier,
            BLS_POP_SCHEME,
            &bytes(c[2]),
            &bytes(c[3]),
            &bytes(c[4]),
            &bytes(c[5]),
        );
        if c[1] == "1" {
            assert_eq!(result, Ok(()), "{}", c[0]);
            valid += 1;
        } else {
            assert_eq!(result, Err(Failure::InvalidEvidence), "{}", c[0]);
            invalid += 1;
        }
    }
    assert_eq!((valid, invalid), (4, 22));
}

#[test]
fn oracle_owner_parameters_and_signature_backends_never_default() {
    assert_eq!(
        Parameters::default().validate(),
        Err(Failure::UnboundParameter)
    );
    assert_eq!(
        verify_bls_pop(
            &OracleVerifier,
            BLS_POP_SCHEME,
            &[0; 48],
            &[0; 96],
            b"message",
            &[0; 96]
        ),
        Err(Failure::InvalidEvidence)
    );
    for scheme in [
        HYBRID_REPORT_SCHEME,
        HYBRID_LONG_LIVED_SCHEME,
        "BLS",
        "G2Basic",
    ] {
        assert_eq!(
            verify_bls_pop(&OracleVerifier, scheme, &[], &[], &[], &[]),
            Err(Failure::UnsupportedScheme)
        );
    }
    assert!(Id::new(&[]).is_err());
    assert!(Id::new(&[1; 65]).is_err());
    let mut bytes = vec![1, 2, 3];
    let id = Id::new(&bytes).unwrap_or_else(|_| panic!("ID"));
    bytes[0] = 9;
    assert_eq!(id.bytes(), &[1, 2, 3]);
}
