//! Deterministic decoder-result stream for comparison between two source revisions.
//! Build with the two crates from the revision being measured; no extra dependency.
use std::fmt::Write as _;
use std::io::{self, BufWriter, Write};
use zeno_fcis_codec::{DecodeLimits, canonical_value_bytes_with_limits, decode_value};
use zeno_fcis_value::ValueLimits;

const SEED: u64 = 0xc2b2_ae3d_27d4_eb4f;

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        let mut value = self.0;
        value ^= value << 13;
        value ^= value >> 7;
        value ^= value << 17;
        self.0 = value;
        value
    }
    fn bounded(&mut self, bound: usize) -> usize {
        (self.next() % bound as u64) as usize
    }
}

fn blob(output: &mut Vec<u8>, value: &[u8]) {
    output.extend_from_slice(&(value.len() as u32).to_be_bytes());
    output.extend_from_slice(value);
}

// Independent literal ZCVE grammar: the input generator does not call the encoder.
fn sealed(rng: &mut Rng, depth: usize) -> Vec<u8> {
    let tag = if depth == 4 {
        rng.bounded(8)
    } else {
        rng.bounded(13)
    } as u8;
    let mut bytes = vec![tag];
    match tag {
        0..=2 => {}
        3 | 4 => {
            bytes.extend_from_slice(&rng.next().to_be_bytes());
            bytes.extend_from_slice(&rng.next().to_be_bytes());
        }
        5 | 6 => {
            let length = rng.bounded(25);
            let data: Vec<u8> = (0..length)
                .map(|_| (rng.next() as u8) & if tag == 6 { 127 } else { 255 })
                .collect();
            blob(&mut bytes, &data);
        }
        7 => {
            bytes.extend_from_slice(&(rng.next() as u32).to_be_bytes());
            bytes.extend_from_slice(&(rng.next() as u16).to_be_bytes());
        }
        8 | 11 => {
            let count = rng.bounded(5);
            bytes.extend_from_slice(&(count as u32).to_be_bytes());
            for _ in 0..count {
                bytes.extend_from_slice(&sealed(rng, depth + 1));
            }
        }
        9 => {
            let count = rng.bounded(5);
            bytes.extend_from_slice(&(count as u32).to_be_bytes());
            let first = (rng.next() as u16) & 0xff00;
            for index in 0..count {
                bytes.extend_from_slice(&(first + index as u16).to_be_bytes());
                bytes.extend_from_slice(&sealed(rng, depth + 1));
            }
        }
        10 => {
            bytes.extend_from_slice(&(rng.next() as u32).to_be_bytes());
            bytes.extend_from_slice(&(rng.next() as u16).to_be_bytes());
            let present = rng.bounded(2) as u8;
            bytes.push(present);
            if present != 0 {
                bytes.extend_from_slice(&sealed(rng, depth + 1));
            }
        }
        12 => {
            let count = rng.bounded(5);
            bytes.extend_from_slice(&(count as u32).to_be_bytes());
            let high = rng.next();
            for index in 0..count {
                let mut key = vec![3];
                key.extend_from_slice(&high.to_be_bytes());
                key.extend_from_slice(&(index as u64).to_be_bytes());
                blob(&mut bytes, &key);
                blob(&mut bytes, &sealed(rng, depth + 1));
            }
        }
        _ => unreachable!(),
    }
    bytes
}

fn input(rng: &mut Rng, index: usize) -> Vec<u8> {
    match index % 4 {
        0 => sealed(rng, 0),
        1 => {
            let mut bytes = sealed(rng, 0);
            match rng.bounded(8) {
                0 => {
                    let at = rng.bounded(bytes.len());
                    bytes[at] ^= 1 << rng.bounded(8);
                }
                1 => {
                    bytes.truncate(rng.bounded(bytes.len()));
                }
                2 => {
                    bytes.push(rng.next() as u8);
                }
                3 => {
                    bytes[0] = rng.next() as u8;
                }
                4 => {
                    let at = rng.bounded(bytes.len());
                    bytes.insert(at, rng.next() as u8);
                }
                5 => {
                    if bytes.len() >= 5 {
                        bytes[1..5].copy_from_slice(&u32::MAX.to_be_bytes());
                    }
                }
                6 => {
                    bytes = vec![6, 0, 0, 0, 1, 0x80 | rng.next() as u8];
                }
                _ => {
                    bytes = vec![10, 0, 0, 0, 0, 0, 0, 2 + rng.bounded(254) as u8];
                }
            }
            bytes
        }
        2 => {
            const EDGES: [u32; 10] = [
                0,
                1,
                2,
                255,
                256,
                65535,
                65536,
                0x7fff_ffff,
                0x8000_0000,
                u32::MAX,
            ];
            const TAGS: [u8; 6] = [5, 6, 8, 9, 11, 12];
            let mut bytes = vec![TAGS[rng.bounded(TAGS.len())]];
            bytes.extend_from_slice(&EDGES[rng.bounded(EDGES.len())].to_be_bytes());
            let length = rng.bounded(33);
            bytes.extend((0..length).map(|_| rng.next() as u8));
            bytes
        }
        _ => {
            let length = rng.bounded(97);
            (0..length).map(|_| rng.next() as u8).collect()
        }
    }
}

fn limits(rng: &mut Rng, index: usize) -> DecodeLimits {
    if index % 8 < 4 {
        return DecodeLimits::default();
    }
    const DEPTHS: [u32; 6] = [0, 1, 4, 64, 1024, u32::MAX];
    const NODES: [u64; 6] = [0, 1, 2, 7, 1_000_000, u64::MAX];
    const PAYLOADS: [u64; 6] = [0, 1, 8, 32, 67_108_864, u64::MAX];
    const COLLECTIONS: [u32; 6] = [0, 1, 2, 4, 1_000_000, u32::MAX];
    const INPUTS: [u64; 6] = [0, 1, 5, 32, 67_108_864, u64::MAX];
    DecodeLimits {
        max_input_bytes: INPUTS[rng.bounded(INPUTS.len())],
        value: ValueLimits {
            max_depth: DEPTHS[rng.bounded(DEPTHS.len())],
            max_nodes: NODES[rng.bounded(NODES.len())],
            max_payload_bytes: PAYLOADS[rng.bounded(PAYLOADS.len())],
            max_collection_len: COLLECTIONS[rng.bounded(COLLECTIONS.len())],
        },
    }
}

fn hex(bytes: &[u8]) -> String {
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        write!(&mut output, "{byte:02x}").expect("string formatting");
    }
    output
}

fn main() -> io::Result<()> {
    let count: usize = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "1000000".to_owned())
        .parse()
        .expect("positive input count");
    assert!(count > 0);
    let mut rng = Rng(SEED);
    let stdout = io::stdout();
    let mut output = BufWriter::new(stdout.lock());
    writeln!(
        output,
        "ZCVE-differential-v1 seed={SEED:016x} count={count}"
    )?;
    for index in 0..count {
        let bytes = input(&mut rng, index);
        let limits = limits(&mut rng, index);
        write!(
            output,
            "{index:07} {} {}/{}/{}/{}/{} ",
            hex(&bytes),
            limits.max_input_bytes,
            limits.value.max_depth,
            limits.value.max_nodes,
            limits.value.max_payload_bytes,
            limits.value.max_collection_len
        )?;
        match decode_value(&bytes, limits) {
            Ok(value) => {
                let canonical = canonical_value_bytes_with_limits(&value, limits.value)
                    .expect("an accepted value remains encodable under the same limits");
                assert_eq!(
                    canonical, bytes,
                    "accepted input must equal its canonical encoding"
                );
                writeln!(output, "Ok({})", hex(&canonical))?;
            }
            Err(error) => writeln!(output, "Err({error:?})")?,
        }
    }
    output.flush()
}
