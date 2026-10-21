//! Canonical receipt, quorum and immutable registry boundaries.
#![allow(dead_code, unreachable_pub)]
#[path = "support/issuer.rs"]
mod issuer;
#[path = "../../../core-apps/zusd-v1/tests/support/step4.rs"]
mod support;
use issuer::backend;
use support::*;
use zeno_fcis_bls::{receipt::*, *};
use zenofcis_zusd_v1::application::*;
fn signed() -> (zenofcis_zusd_v1::application::receipts::Message, Vec<u8>) {
    let p = policy(false);
    let s = genesis(&p);
    let r = raw(&s, 1, Command::AdvanceClock);
    let m = receipt_messages(&p, &s, &r).unwrap().remove(0);
    let proof = issuer::issue(&m.bytes);
    (m, proof)
}
#[test]
fn complete_message_and_canonical_proof_boundaries() {
    let (m, p) = signed();
    assert_eq!(
        backend().verify_detailed(m.domain, m.authority, &m.bytes, &p),
        Ok(())
    );
    for i in 0..m.bytes.len() {
        let mut b = m.bytes.clone();
        b[i] ^= 1;
        assert!(
            backend()
                .verify_detailed(m.domain, m.authority, &b, &p)
                .is_err(),
            "accepted altered byte {i}"
        );
    }
    for i in 0..p.len() {
        let mut b = p.clone();
        b[i] ^= 1;
        assert!(
            backend()
                .verify_detailed(m.domain, m.authority, &m.bytes, &b)
                .is_err(),
            "accepted altered proof byte {i}"
        );
    }
    for end in 0..p.len() {
        assert!(
            backend()
                .verify_detailed(m.domain, m.authority, &m.bytes, &p[..end])
                .is_err()
        );
    }
    let mut extra = p.clone();
    extra.push(0);
    assert_eq!(
        backend().verify_detailed(m.domain, m.authority, &m.bytes, &extra),
        Err(ReceiptRefusal::ProofEncoding)
    );
    assert_eq!(
        backend().verify_detailed(Domain::Clock, m.authority, &m.bytes, &p),
        Err(ReceiptRefusal::CanonicalMessage)
    );
}
#[test]
fn quorum_threshold_distinct_votes_and_owned_registry() {
    let p = policy(false);
    let s = ready(&p);
    let r = price_request(&p, &s, OracleAction::ImportPending, 2, 300_000_000, 111);
    let m = receipt_messages(&p, &s, &r)
        .unwrap()
        .into_iter()
        .find(|m| m.domain == Domain::Oracle)
        .unwrap();
    let proof = issuer::issue(&m.bytes);
    assert_eq!(
        backend().verify_detailed(m.domain, m.authority, &m.bytes, &proof),
        Ok(())
    );
    let mut duplicate = proof.clone();
    duplicate[6] = 0;
    duplicate[7] = 0;
    assert_eq!(
        backend().verify_detailed(m.domain, m.authority, &m.bytes, &duplicate),
        Err(ReceiptRefusal::SignerOrder)
    );
    let mut too_few = vec![1, 1, 0, 1, 0, 0];
    too_few.extend_from_slice(&proof[8..]);
    assert_eq!(
        backend().verify_detailed(m.domain, m.authority, &m.bytes, &too_few),
        Err(ReceiptRefusal::InsufficientQuorum)
    );
    let mut out = proof.clone();
    out[7] = 63;
    assert_eq!(
        backend().verify_detailed(m.domain, m.authority, &m.bytes, &out),
        Err(ReceiptRefusal::ProofEncoding)
    );
    let mut suite = proof.clone();
    suite[1] = 0;
    assert_eq!(
        backend().verify_detailed(m.domain, m.authority, &m.bytes, &suite),
        Err(ReceiptRefusal::WrongSuite)
    );
    let sk = blst::min_pk::SecretKey::key_gen(&[7; 32], &[]).unwrap();
    let pk = sk.sk_to_pk().to_bytes();
    let member = PopKey::admit(&pk, &sk.sign(&pk, POP_PROOF_DST, &[]).to_bytes()).unwrap();
    assert!(
        Authority::quorum(
            Domain::Oracle,
            id(63),
            vec![member.clone(), member.clone()],
            2
        )
        .is_err()
    );
    assert!(Authority::quorum(Domain::Oracle, id(63), vec![member], 0).is_err());
    let a = Authority::basic(m.domain, m.authority, pk).unwrap();
    assert!(BlsReceiptVerifier::new(vec![a.clone(), a], 131072).is_err());
    let mut keys = vec![Authority::basic(m.domain, m.authority, pk).unwrap()];
    let frozen = BlsReceiptVerifier::new(keys.clone(), 131072).unwrap();
    keys.clear();
    let mut basic = vec![1, 0, 0, 1, 0, 0];
    basic.extend_from_slice(&sk.sign(&m.bytes, BASIC_DST, &[]).to_bytes());
    assert_eq!(
        frozen.verify_detailed(m.domain, m.authority, &m.bytes, &basic),
        Ok(())
    );
}
