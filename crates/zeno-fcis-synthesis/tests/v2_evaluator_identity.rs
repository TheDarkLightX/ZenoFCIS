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
        33_u8, 32_u8, 253_u8, 180_u8, 74_u8, 191_u8, 1_u8, 121_u8, 16_u8, 242_u8, 156_u8, 182_u8,
        179_u8, 74_u8, 116_u8, 251_u8, 39_u8, 202_u8, 103_u8, 166_u8, 54_u8, 244_u8, 243_u8,
        140_u8, 88_u8, 241_u8, 36_u8, 198_u8, 105_u8, 108_u8, 241_u8, 15_u8,
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
    rows.push(("@pin/core-profile".into(), b"ZenoFCIS V2 closed generic scalar producer, flat records and scalar roots. No application callbacks or supplied meter. Exact original schema and full reviewed policy binding.\n".to_vec()));
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
