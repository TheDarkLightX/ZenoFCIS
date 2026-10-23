//! Independent RustCrypto check of the evaluator compiled into the authority.
#[cfg(not(miri))]
use std::{fs, path::Path};
use zeno_fcis_codec::{CommitmentHasher, RustCryptoSha256};
#[cfg(not(miri))]
use zeno_fcis_synthesis::finite::v2_authority::EVALUATOR;

fn encoding(mut rows: Vec<(String, Vec<u8>)>) -> Vec<u8> {
    rows.sort_by(|a, b| a.0.cmp(&b.0));
    assert!(rows.windows(2).all(|w| w[0].0 < w[1].0));
    let mut out = b"ZENO-FCIS-EVALUATOR\0".to_vec();
    out.extend(1_u32.to_be_bytes());
    out.extend((rows.len() as u64).to_be_bytes());
    for (path, payload) in rows {
        out.extend((path.len() as u64).to_be_bytes());
        out.extend(path.as_bytes());
        out.extend(RustCryptoSha256::hash(&payload).as_bytes());
    }
    out
}

#[test]
fn fixed_encoding_vector() {
    let encoded = encoding(vec![
        ("b.rs".into(), vec![]),
        ("a.rs".into(), b"abc".to_vec()),
    ]);
    let actual: String = encoded.iter().map(|v| format!("{v:02x}")).collect();
    assert_eq!(
        actual,
        include_str!("../../../verification/verus/evaluator_encoding_vector.txt").trim()
    );
}

#[test]
#[cfg(not(miri))]
fn workspace_digest_equals_compiled_authority() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let manifest = include_str!("../../../verification/verus/authority_v2_sources.json");
    // Independently pinned membership: omission must refuse even if regenerated.
    let approved_manifest = [
        60_u8, 66_u8, 164_u8, 70_u8, 56_u8, 231_u8, 139_u8, 138_u8, 141_u8, 59_u8, 184_u8, 131_u8,
        116_u8, 183_u8, 153_u8, 0_u8, 160_u8, 169_u8, 97_u8, 251_u8, 40_u8, 207_u8, 58_u8, 73_u8,
        78_u8, 206_u8, 102_u8, 28_u8, 38_u8, 38_u8, 177_u8, 103_u8,
    ];
    assert_eq!(
        RustCryptoSha256::hash(manifest.as_bytes()).as_bytes(),
        &approved_manifest
    );
    let paths = manifest
        .split_once("\"paths\": [")
        .unwrap_or_else(|| panic!("missing paths array"))
        .1
        .split_once(']')
        .unwrap_or_else(|| panic!("missing paths end"))
        .0;
    let mut rows: Vec<_> = paths
        .lines()
        .map(str::trim)
        .filter(|s| s.starts_with('"'))
        .map(|s| {
            let name = s.trim_end_matches(',').trim_matches('"');
            let payload = fs::read(root.join(name))
                .unwrap_or_else(|error| panic!("registered source must exist: {error}"));
            (name.to_owned(), payload)
        })
        .collect();
    rows.push(("@pin/core-profile".into(), b"ZenoFCIS V2 closed generic scalar producer, flat records and scalar roots. No application callbacks or supplied meter. Exact original schema and full reviewed policy binding.\nOwned native tables/1 policy v3: bounded maps/sets, I128 sums and full-scan work tariff. Finite checked at C=0..3 plus tested journeys; no table Verus proof. Scalar-only consumers refuse table subjects.\nOwned native tables-ordered/1 policy v4: closed I128 priority/ID and committed V1 NICR cuts, bounded ID vectors, full-view admission, independent ordered and redemption relations, stable metered V1-cut redistribution repair. Typed redemption plans require authenticated consensus/oracle inputs and cannot publish custody or authenticate a host. Finite evidence and tested journeys, no ordered-table Verus or full-zUSD proof.\nU128 tables extension: exact unsigned schema/key/value reconstruction, kind-preserving point/update/frame laws, law-local checked U256 sums with established static capacity bounds, unsigned ordered priorities and U128 redemption quantities using exact wide NICR. Existing I128 table schema/policy/value formats and tariffs retained. Finite alphabets, independent integer oracles and compiled defect controls only; final shared-source Verus qualification remains pending.\n".to_vec()));
    rows.push(("@pin/runtime-toolchain".into(), b"Rust 1.97.1; Cargo --locked. Verus-shared execution source under ordinary Rust. Supported assurance target Linux x86_64.\n".to_vec()));
    rows.push((
        "@pin/verifier-toolchain".into(),
        fs::read(root.join("verification/verus/toolchain.json"))
            .unwrap_or_else(|error| panic!("missing pinned verifier: {error}")),
    ));
    assert_eq!(
        RustCryptoSha256::hash(&encoding(rows)).as_bytes(),
        &EVALUATOR
    );
}
