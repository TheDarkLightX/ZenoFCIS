//! Independent py_ecc reference parity; secrets and raw attack math are TEST ONLY.
#[path = "support/vector_data.rs"]
mod data;
use blst::{
    BLST_ERROR,
    min_pk::{AggregatePublicKey, AggregateSignature, PublicKey, SecretKey, Signature},
};
use data::*;
use zeno_fcis_bls::*;
fn hx(b: &[u8]) -> String {
    format!(
        "0x{}",
        b.iter().map(|b| format!("{b:02x}")).collect::<String>()
    )
}
fn admitted(pk: &[u8]) -> Result<PopKey, String> {
    // Independent PoP bytes for the first three keys. Fourth key is only a
    // negative extra-member fixture and derives its test proof from its test SK.
    for i in 0..3 {
        let row = named(&format!("sign-parity-poptag-{i}"));
        if row.b[0] == pk {
            return PopKey::admit(pk, &hex(row.expected)).map_err(|e| format!("{e:?}"));
        }
    }
    let row = named("sk-to-pk-parity-3");
    if hex(row.expected) == pk {
        let sk = SecretKey::from_bytes(&row.a[0])
            .unwrap_or_else(|error| panic!("test fixture result: {error:?}"));
        return PopKey::admit(pk, &sk.sign(pk, POP_PROOF_DST, &[]).to_bytes())
            .map_err(|e| format!("{e:?}"));
    }
    Err("not a registered PoP fixture".into())
}
fn run(v: &Vector) -> Result<String, String> {
    let verify = |r: Result<(), Refusal>| r.map(|_| "ACCEPT".into()).map_err(|e| format!("{e:?}"));
    match v.op {
        "verify" => {
            if v.dst == BASIC_DST {
                verify(basic_verify(&v.a[0], &v.b[0], &v.c[0]))
            } else if v.dst == POP_DST {
                verify(pop_verify(&admitted(&v.a[0])?, &v.b[0], &v.c[0]))
            } else {
                Err("unsupported ciphersuite".into())
            }
        }
        "pop_verify" => {
            assert_eq!(v.dst, POP_PROOF_DST);
            PopKey::admit(&v.a[0], &v.c[0])
                .map(|_| "ACCEPT".into())
                .map_err(|e| format!("{e:?}"))
        }
        "key_validate" => verify(validate_key(&v.a[0])),
        "aggregate_verify_basic" => {
            assert_eq!(v.dst, BASIC_DST);
            verify(basic_aggregate_verify(
                &v.a.iter().map(Vec::as_slice).collect::<Vec<_>>(),
                &v.b.iter().map(Vec::as_slice).collect::<Vec<_>>(),
                &v.c[0],
            ))
        }
        "aggregate_verify_pop" | "fast_aggregate_verify" => {
            assert_eq!(v.dst, POP_DST);
            if v.name == "rogue-key-fast-aggregate-math-accepts" {
                // Raw group arithmetic intentionally accepts the attack. This
                // observation is not a registry acceptance; the paired proof
                // vectors and rogue_key_gate test require public admission refusal.
                let keys =
                    v.a.iter()
                        .map(|p| {
                            PublicKey::key_validate(p)
                                .unwrap_or_else(|error| panic!("test fixture result: {error:?}"))
                        })
                        .collect::<Vec<_>>();
                let agg = AggregatePublicKey::aggregate(&keys.iter().collect::<Vec<_>>(), true)
                    .unwrap_or_else(|error| panic!("test fixture result: {error:?}"))
                    .to_public_key();
                assert_eq!(
                    Signature::from_bytes(&v.c[0])
                        .unwrap_or_else(|error| panic!("test fixture result: {error:?}"))
                        .verify(true, &v.b[0], POP_DST, &[], &agg, true),
                    BLST_ERROR::BLST_SUCCESS
                );
                return Ok("ACCEPT".into());
            }
            let keys =
                v.a.iter()
                    .map(|p| admitted(p))
                    .collect::<Result<Vec<_>, _>>()?;
            let refs = keys.iter().collect::<Vec<_>>();
            if v.op == "aggregate_verify_pop" {
                verify(pop_aggregate_verify(
                    &refs,
                    &v.b.iter().map(Vec::as_slice).collect::<Vec<_>>(),
                    &v.c[0],
                ))
            } else {
                verify(pop_fast_aggregate_verify(&refs, &v.b[0], &v.c[0]))
            }
        }
        "keygen" => SecretKey::key_gen(&v.a[0], &v.b[0])
            .map(|s| hx(&s.to_bytes()))
            .map_err(|e| format!("{e:?}")),
        "sk_to_pk" => SecretKey::from_bytes(&v.a[0])
            .map(|s| hx(&s.sk_to_pk().to_bytes()))
            .map_err(|e| format!("{e:?}")),
        "sign" => SecretKey::from_bytes(&v.a[0])
            .map(|s| hx(&s.sign(&v.b[0], &v.dst, &[]).to_bytes()))
            .map_err(|e| format!("{e:?}")),
        "agg_sigs" => {
            let sigs =
                v.a.iter()
                    .map(|s| Signature::sig_validate(s, true))
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(|e| format!("{e:?}"))?;
            AggregateSignature::aggregate(&sigs.iter().collect::<Vec<_>>(), true)
                .map(|s| hx(&s.to_signature().to_bytes()))
                .map_err(|e| format!("{e:?}"))
        }
        "agg_pks" => {
            let keys =
                v.a.iter()
                    .map(|s| PublicKey::key_validate(s))
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(|e| format!("{e:?}"))?;
            AggregatePublicKey::aggregate(&keys.iter().collect::<Vec<_>>(), true)
                .map(|s| hx(&s.to_public_key().to_bytes()))
                .map_err(|e| format!("{e:?}"))
        }
        _ => panic!("unknown vector operation"),
    }
}
#[test]
fn all_82_py_ecc_vectors() {
    let vectors = vectors();
    assert_eq!(vectors.len(), 82);
    for v in &vectors {
        let got = run(v);
        let pass = match &got {
            Ok(s) => s == v.expected,
            Err(_) => v.expected == "REJECT",
        };
        assert!(pass, "{}: expected {}, got {:?}", v.name, v.expected, got);
        println!("PASS\t{}\t{}\t{}", v.name, v.op, v.expected);
    }
    println!("TOTAL py_ecc pass=82 fail=0");
}
