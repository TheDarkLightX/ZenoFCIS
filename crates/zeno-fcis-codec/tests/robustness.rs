//! Generated-input robustness tests for the value codec, in pure Rust with no
//! dependency: a seeded generator builds structured values, encodes them, and
//! mutates the encodings. Every input must decode without a panic; a decoded
//! input must be exactly canonical and round-trip. Set
//! `ZENO_FCIS_ROBUSTNESS_ITERATIONS` for a longer run; a failure prints its seed.

use std::panic::{AssertUnwindSafe, catch_unwind};

use zeno_fcis_codec::{CanonicalEncode, DecodeLimits, decode_value};
use zeno_fcis_value::{Field, MapEntry, Value};

/// SplitMix64: small, fast and reproducible from its seed.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n.max(1)
    }
    fn chance(&mut self, percent: u64) -> bool {
        self.below(100) < percent
    }
}

fn iterations() -> u64 {
    std::env::var("ZENO_FCIS_ROBUSTNESS_ITERATIONS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(2_000)
}

fn edge_u128(rng: &mut Rng) -> u128 {
    match rng.below(6) {
        0 => 0,
        1 => 1,
        2 => u128::MAX,
        3 => u128::from(rng.next()),
        4 => (u128::from(rng.next()) << 64) | u128::from(rng.next()),
        _ => 1u128 << rng.below(128),
    }
}

fn edge_i128(rng: &mut Rng) -> i128 {
    match rng.below(6) {
        0 => 0,
        1 => -1,
        2 => i128::MIN,
        3 => i128::MAX,
        4 => i128::from(rng.next() as i64),
        _ => edge_u128(rng) as i128,
    }
}

/// A random value; containers only below `depth`.
fn value(rng: &mut Rng, depth: u32) -> Value {
    let kinds = if depth == 0 { 8 } else { 12 };
    match rng.below(kinds) {
        0 => Value::unit(),
        1 => Value::boolean(rng.chance(50)),
        2 => Value::unsigned(edge_u128(rng)),
        3 => Value::signed(edge_i128(rng)),
        4 => Value::enumeration(rng.below(1 << 20) as u32, rng.below(1 << 16) as u16),
        5 => {
            let len = rng.below(48) as usize;
            Value::bytes((0..len).map(|_| rng.next() as u8).collect())
                .unwrap_or_else(|_| Value::unit())
        }
        6 => {
            let len = rng.below(24) as usize;
            let text: String = (0..len)
                .map(|_| char::from(b' ' + rng.below(95) as u8))
                .collect();
            Value::text_ascii(text).unwrap_or_else(|_| Value::unit())
        }
        7 => {
            let payload = if rng.chance(50) {
                Some(Value::unsigned(edge_u128(rng)))
            } else {
                None
            };
            Value::sum(
                rng.below(1 << 20) as u32,
                rng.below(1 << 16) as u16,
                payload,
            )
        }
        8 => {
            let items = (0..rng.below(4)).map(|_| value(rng, depth - 1)).collect();
            Value::tuple(items).unwrap_or_else(|_| Value::unit())
        }
        9 => {
            let items = (0..rng.below(4)).map(|_| value(rng, depth - 1)).collect();
            Value::vector(items).unwrap_or_else(|_| Value::unit())
        }
        10 => {
            let fields = (0..rng.below(4))
                .map(|_| Field::new(rng.below(64) as u16, value(rng, depth - 1)))
                .collect();
            Value::normalize_record(fields).unwrap_or_else(|_| Value::unit())
        }
        _ => {
            let entries = (0..rng.below(3))
                .filter_map(|_| {
                    MapEntry::try_new(Value::unsigned(rng.below(16).into()), value(rng, depth - 1))
                        .ok()
                })
                .collect();
            Value::normalize_map(entries).unwrap_or_else(|_| Value::unit())
        }
    }
}

/// One byte-level mutation of `bytes`, sometimes splicing in `other`.
fn mutate(rng: &mut Rng, mut bytes: Vec<u8>, other: &[u8]) -> Vec<u8> {
    match rng.below(7) {
        0 if !bytes.is_empty() => {
            let i = rng.below(bytes.len() as u64) as usize;
            bytes[i] ^= 1 << rng.below(8);
        }
        1 => {
            let i = rng.below(bytes.len() as u64 + 1) as usize;
            bytes.insert(i, rng.next() as u8);
        }
        2 if !bytes.is_empty() => {
            let i = rng.below(bytes.len() as u64) as usize;
            bytes.remove(i);
        }
        3 => {
            let keep = rng.below(bytes.len() as u64 + 1) as usize;
            bytes.truncate(keep);
        }
        4 if !other.is_empty() => {
            let i = rng.below(bytes.len() as u64 + 1) as usize;
            let j = rng.below(other.len() as u64) as usize;
            bytes.truncate(i);
            bytes.extend_from_slice(&other[j..]);
        }
        5 if !bytes.is_empty() => {
            let i = rng.below(bytes.len() as u64) as usize;
            let n = (rng.below(8) as usize).min(bytes.len() - i);
            let slice = bytes[i..i + n].to_vec();
            bytes.splice(i..i, slice);
        }
        _ if !bytes.is_empty() => {
            let i = rng.below(bytes.len() as u64) as usize;
            bytes[i] = [0x00, 0xFF, 0x7F, 0x80][rng.below(4) as usize];
        }
        _ => bytes.push(rng.next() as u8),
    }
    bytes
}

/// The decoder's contract on any input: no panic; a decoded input is exactly
/// canonical, and its value round-trips to the same bytes.
fn check(input: &[u8], seed: u64) {
    let outcome = catch_unwind(AssertUnwindSafe(|| {
        decode_value(input, DecodeLimits::default())
    }));
    let decoded = outcome
        .unwrap_or_else(|_| panic!("decode_value panicked; seed {seed}, input {input:02x?}"));
    if let Ok(value) = decoded {
        let canonical = value
            .canonical_bytes()
            .unwrap_or_else(|_| panic!("decoded value not encodable; seed {seed}"));
        assert_eq!(
            canonical, input,
            "a non-canonical input decoded; seed {seed}, input {input:02x?}"
        );
        let again = decode_value(&canonical, DecodeLimits::default())
            .unwrap_or_else(|error| panic!("canonical bytes decode: {error:?}"));
        assert_eq!(again, value, "round trip changed the value; seed {seed}");
    }
}

#[test]
fn generated_values_round_trip_exactly() {
    for seed in 0..iterations() {
        let mut rng = Rng(seed);
        let v = value(&mut rng, 3);
        let bytes = v
            .canonical_bytes()
            .unwrap_or_else(|_| panic!("generated value not encodable; seed {seed}"));
        let decoded = decode_value(&bytes, DecodeLimits::default())
            .unwrap_or_else(|e| panic!("canonical bytes refused: {e:?}; seed {seed}"));
        assert_eq!(decoded, v, "seed {seed}");
        check(&bytes, seed);
    }
}

#[test]
fn mutated_encodings_never_panic_and_decode_only_when_canonical() {
    for seed in 0..iterations() {
        let mut rng = Rng(seed ^ 0xA5A5_A5A5);
        let a = value(&mut rng, 3).canonical_bytes().unwrap_or_default();
        let b = value(&mut rng, 3).canonical_bytes().unwrap_or_default();
        let mut input = a;
        for _ in 0..=rng.below(4) {
            input = mutate(&mut rng, input, &b);
        }
        check(&input, seed);
    }
}

#[test]
fn random_bytes_never_panic() {
    for seed in 0..iterations() {
        let mut rng = Rng(seed.wrapping_mul(31) ^ 0x5EED);
        let len = rng.below(96) as usize;
        let input: Vec<u8> = (0..len).map(|_| rng.next() as u8).collect();
        check(&input, seed);
    }
}
#[test]
fn sha256_bounded_streaming_vectors() {
    use zeno_fcis_codec::{CommitmentHasher, Hash32, RustCryptoSha256 as H};

    let vectors: &[(&[u8], Hash32)] = &[
        (
            b"",
            Hash32::new([
                0xe3, 0xb0, 0xc4, 0x42, 0x98, 0xfc, 0x1c, 0x14, 0x9a, 0xfb, 0xf4, 0xc8, 0x99, 0x6f,
                0xb9, 0x24, 0x27, 0xae, 0x41, 0xe4, 0x64, 0x9b, 0x93, 0x4c, 0xa4, 0x95, 0x99, 0x1b,
                0x78, 0x52, 0xb8, 0x55,
            ]),
        ),
        (
            b"abc",
            Hash32::new([
                0xba, 0x78, 0x16, 0xbf, 0x8f, 0x01, 0xcf, 0xea, 0x41, 0x41, 0x40, 0xde, 0x5d, 0xae,
                0x22, 0x23, 0xb0, 0x03, 0x61, 0xa3, 0x96, 0x17, 0x7a, 0x9c, 0xb4, 0x10, 0xff, 0x61,
                0xf2, 0x00, 0x15, 0xad,
            ]),
        ),
        (
            b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq",
            Hash32::new([
                0x24, 0x8d, 0x6a, 0x61, 0xd2, 0x06, 0x38, 0xb8, 0xe5, 0xc0, 0x26, 0x93, 0x0c, 0x3e,
                0x60, 0x39, 0xa3, 0x3c, 0xe4, 0x59, 0x64, 0xff, 0x21, 0x67, 0xf6, 0xec, 0xed, 0xd4,
                0x19, 0xdb, 0x06, 0xc1,
            ]),
        ),
    ];
    for &(message, expected) in vectors {
        assert_eq!(H::hash(message), expected);
        for split in 0..=message.len() {
            assert_eq!(
                H::hash_parts(&[&message[..split], b"", &message[split..]]),
                expected
            );
        }
    }
    for length in [0, 1, 55, 56, 63, 64, 65, 127, 128, 129, 255, 256, 257] {
        let bytes: Vec<u8> = (0_u8..=255).cycle().take(length).collect();
        for chunk in [1, 55, 56, 63, 64, 65, 257] {
            let parts: Vec<&[u8]> = bytes.chunks(chunk).collect();
            assert_eq!(H::hash_parts(&parts), H::hash(&bytes));
        }
    }
}
