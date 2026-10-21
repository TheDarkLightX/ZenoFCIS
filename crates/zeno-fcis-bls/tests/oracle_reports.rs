//! Oracle verification and lifecycle regression witnesses.
use zeno_fcis_bls::oracle::OracleVerifier;
use zeno_fcis_synthesis::finite::v2_oracle::*;
fn id(n: u8) -> Id {
    Id::new(&[n]).unwrap_or_else(|_| panic!("ID"))
}
fn limits() -> Limits {
    zero_limits()
        .with_limit(Resource::Step, 10000)
        .with_limit(Resource::Byte, 100000)
        .with_limit(Resource::WitnessByte, 10000)
}
fn payload() -> ReportPayload {
    ReportPayload {
        chain: id(1),
        reporter: id(2),
        key_version: 7,
        feed: [1; 32],
        profile: [2; 32],
        query: [3; 32],
        source: id(3),
        round: 3,
        value: PriceE8::new(u128::MAX).unwrap_or_else(|_| panic!("price")),
        observed_epoch: 4,
        observation_seconds: 12,
        sequence: 11,
        previous: Some([9; 32]),
    }
}
fn hex(s: &str) -> Vec<u8> {
    s.as_bytes()
        .chunks_exact(2)
        .map(|b| {
            let digit = |x: u8| match x {
                b'0'..=b'9' => x - b'0',
                b'a'..=b'f' => x - b'a' + 10,
                _ => panic!("hex"),
            };
            digit(b[0]) * 16 + digit(b[1])
        })
        .collect()
}
#[test]
fn oracle_report_json_and_hashes_match_separate_bls_reference_bytes() {
    let columns: Vec<_> =
        include_str!("../../zeno-fcis-synthesis/tests/test-data/oracle/signature.tsv")
            .trim()
            .split('\t')
            .collect();
    let p = payload();
    assert_eq!(p.canonical_json(), hex(columns[0]));
    assert_eq!(p.message().as_slice(), hex(columns[2]));
    let signature: [u8; 96] = hex(columns[5])
        .try_into()
        .unwrap_or_else(|_| panic!("signature"));
    assert_eq!(p.report_id(&signature).as_slice(), hex(columns[6]));
    let key = KeyRegistration {
        reporter: id(2),
        owner: id(2),
        version: 7,
        public_key: hex(columns[3]).try_into().unwrap_or_else(|_| panic!("key")),
        possession: hex(columns[4]).try_into().unwrap_or_else(|_| panic!("pop")),
    };
    let report = RawReport {
        id: hex(columns[6]).try_into().unwrap_or_else(|_| panic!("id")),
        payload_hash: hex(columns[1])
            .try_into()
            .unwrap_or_else(|_| panic!("hash")),
        signature,
        payload: p,
    };
    let chain = ReportChain {
        next_sequence: 11,
        previous: Some([9; 32]),
        last_seconds: Some(10),
    };
    let native = admit_signed_report(&OracleVerifier, &key, &chain, &report, 4, 4, limits());
    assert_eq!(native.result(), Err(&Failure::UnboundParameter));
    assert_eq!(
        register_key(&OracleVerifier, &key, limits()).result(),
        Ok(&())
    );
    let mut mutated = report.clone();
    mutated.payload.sequence += 1;
    assert_eq!(
        admit_signed_report(&OracleVerifier, &key, &chain, &mutated, 4, 4, limits()).result(),
        Err(&Failure::InvalidEvidence)
    );
    assert_eq!(chain.next_sequence, 11);
}

#[test]
fn oracle_signing_preparation_refuses_before_rendering_and_preserves_retry_usage() {
    let key = KeyRegistration {
        reporter: id(2),
        owner: id(2),
        version: 7,
        public_key: [0; 48],
        possession: [0; 96],
    };
    let mut next = key.clone();
    next.version += 1;
    let report = RawReport {
        payload: payload(),
        payload_hash: [0; 32],
        id: [0; 32],
        signature: [0; 96],
    };
    let chain = ReportChain::default();
    let original = (key.clone(), next.clone(), report.clone(), chain.clone());
    for bytes in [0, 4095] {
        let bound = limits().with_limit(Resource::Byte, bytes);
        let first =
            admit_signed_report(&OracleVerifier, &key, &chain, &report, 4, 4, bound).into_parts();
        let retry =
            admit_signed_report(&OracleVerifier, &key, &chain, &report, 4, 4, bound).into_parts();
        assert_eq!(first, retry);
        assert!(matches!(&first.0, Err(Failure::Budget(f))
            if f.resource == Resource::Byte && f.attempted == 4096));
        assert_eq!(first.1.used(Resource::Step), 1);
        assert_eq!(first.1.used(Resource::Byte), 0);
        let first =
            rotate_key(&OracleVerifier, &key, &next, &[0; 96], &[0; 96], bound).into_parts();
        let retry =
            rotate_key(&OracleVerifier, &key, &next, &[0; 96], &[0; 96], bound).into_parts();
        assert_eq!(first, retry);
        assert!(matches!(&first.0, Err(Failure::Budget(f))
            if f.resource == Resource::Byte && f.attempted == 4096));
        assert_eq!(first.1.used(Resource::Step), 1);
        assert_eq!(first.1.used(Resource::Byte), 0);
    }
    assert_eq!((key, next, report, chain), original);
}

#[test]
fn oracle_linked_sequence_complete_small_domain_and_precedence() {
    let mut rows = 0;
    let mut accepts = 0;
    for next in 0..=2 {
        for previous in [None, Some([1; 32])] {
            for old_time in [None, Some(0), Some(1), Some(2)] {
                let chain = ReportChain {
                    next_sequence: next,
                    previous,
                    last_seconds: old_time,
                };
                for sequence in 0..=3 {
                    for link in [None, Some([1; 32]), Some([2; 32])] {
                        for observed in 0..=2 {
                            for now in 0..=2 {
                                for seconds in 0..=2 {
                                    let mut p = payload();
                                    p.sequence = sequence;
                                    p.previous = link;
                                    p.observed_epoch = observed;
                                    p.observation_seconds = seconds;
                                    let expected = if sequence != next || link != previous {
                                        Some(Reason::BadSequence)
                                    } else if observed > now {
                                        Some(Reason::FutureObservation)
                                    } else if old_time.is_some_and(|t| seconds < t) {
                                        Some(Reason::BackwardObservation)
                                    } else if now - observed > 1 {
                                        Some(Reason::StaleObservation)
                                    } else {
                                        None
                                    };
                                    let out = report_chain_relation(
                                        &chain,
                                        &p,
                                        [7; 32],
                                        now,
                                        1,
                                        limits(),
                                    );
                                    let decision = out.result().unwrap_or_else(|e| panic!("{e:?}"));
                                    match expected {
                                        Some(reason) => assert_eq!(decision, &Err(reason)),
                                        None => {
                                            accepts += 1;
                                            assert_eq!(
                                                decision,
                                                &Ok(ReportChain {
                                                    next_sequence: next + 1,
                                                    previous: Some([7; 32]),
                                                    last_seconds: Some(seconds)
                                                })
                                            );
                                        }
                                    }
                                    rows += 1;
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    assert_eq!(rows, 7776);
    assert!(accepts > 0);
    let chain = ReportChain {
        next_sequence: u64::MAX,
        previous: None,
        last_seconds: None,
    };
    assert_eq!(
        report_chain_relation(&chain, &payload(), [7; 32], 4, 4, limits()).result(),
        Ok(&Err(Reason::CounterExhausted))
    );
}
