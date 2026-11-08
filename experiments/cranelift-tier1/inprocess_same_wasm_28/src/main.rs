//! Archived baseline and experimentally patched Wasmtime compilers applied to the EXACT SAME WAT.
//! Paired exploratory in-process call timing, not a proof of speed or compiler correctness.
use std::hint::black_box;
use std::time::Instant;
use wasmtime::{Engine, Instance, Module, Store, TypedFunc};

type ErrBox = Box<dyn std::error::Error>;

const SCHEMA: &str = "zeno-fcis/udivrem-same-wasm-compiler-call-timing/1";

fn reference(x: i64, y: i64) -> Result<i64, ErrBox> {
    if y == 0 {
        return Err("division by zero outside value domain".into());
    }
    let a = x as u64;
    let b = y as u64;
    let q = a / b;
    let r = a % b;
    Ok((q ^ r.rotate_left(17)) as i64)
}

fn inputs() -> Vec<(i64, i64)> {
    let mut result = vec![
        (0, 1), (7, 3), (i64::MIN, -1), (i64::MAX, 1),
        (-1, -1), (i64::MAX, i64::MIN), (i64::MIN, i64::MAX),
        (123_456_789, -7), (0, -1), (-1, 1), (i64::MIN, 3), (i64::MAX, 7),
    ];
    let mut state: u64 = 0x9e3779b97f4a7c15;
    for _ in 0..256 {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        let x = state as i64;
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        let y = if state == 0 { 1 } else { state as i64 };
        result.push((x, y));
    }
    result
}

struct Guest {
    store: Store<()>,
    func: TypedFunc<(i64, i64), i64>,
}

impl Guest {
    fn load(engine: &Engine, path: &str) -> Result<Self, ErrBox> {
        // SAFETY: CI verifies each input .cwasm against the exact
        // producer receipt hash BEFORE running this program.
        let module = unsafe { Module::deserialize_file(engine, path) }
            .map_err(|e| format!("AOT deserialization of {path}: {e:#}"))?;
        let mut store = Store::new(engine, ());
        let instance = Instance::new(&mut store, &module, &[])?;
        let func = instance.get_typed_func::<(i64, i64), i64>(&mut store, "work")?;
        Ok(Self { store, func })
    }

    fn verify(&mut self, data: &[(i64, i64)]) -> Result<usize, ErrBox> {
        for &(x, y) in data {
            let wanted = reference(x, y)?;
            let actual = self.func.call(&mut self.store, (x, y))?;
            if wanted != actual {
                return Err(format!("wrong native Wasm value for {x},{y}").into());
            }
        }
        if self.func.call(&mut self.store, (1, 0)).is_ok() {
            return Err("missing divide-by-zero Wasm trap".into());
        }
        Ok(data.len())
    }

    fn batch(&mut self, data: &[(i64, i64)], count: usize, expected: u64)
        -> Result<u64, ErrBox>
    {
        let mut checksum = 0_u64;
        for k in 0..count {
            let (x, y) = black_box(data[k % data.len()]);
            let got = self.func.call(&mut self.store, (x, y))?;
            checksum = checksum.wrapping_add(black_box(got as u64));
        }
        if checksum != expected {
            return Err("timed batch produced wrong output".into());
        }
        Ok(checksum)
    }
}

fn run() -> Result<(), ErrBox> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 6 {
        return Err("usage: tier1-udivrem-inprocess BASELINE.cwasm PATCHED.cwasm CALLS BLOCKS SEED".into());
    }
    let calls: usize = args[3].parse()?;
    let blocks: usize = args[4].parse()?;
    let seed: usize = args[5].parse()?;
    if !(10_000..=2_000_000).contains(&calls)
        || !(4..=64).contains(&blocks) || blocks % 2 != 0 || seed != 5623 {
        return Err("out-of-range fixed-size sample budget or seed".into());
    }
    let engine = Engine::default();
    let mut baseline = Guest::load(&engine, &args[1])?;
    let mut patched = Guest::load(&engine, &args[2])?;
    let data = inputs();
    let a = baseline.verify(&data)?;
    let b = patched.verify(&data)?;
    if a != b {
        return Err("coverage mismatch".into());
    }

    let mut expected = 0_u64;
    for k in 0..calls {
        let (x, y) = data[k % data.len()];
        expected = expected.wrapping_add(reference(x, y)? as u64);
    }
    for _ in 0..3 {
        baseline.batch(&data, calls, expected)?;
        patched.batch(&data, calls, expected)?;
    }
    let mut samples = Vec::with_capacity(4 * blocks);
    for block in 0..blocks {
        let order = if block % 2 == 0 {
            ["baseline", "patched", "patched", "baseline"]
        } else {
            ["patched", "baseline", "baseline", "patched"]
        };
        for (position, name) in order.iter().enumerate() {
            let guest = if *name == "baseline" { &mut baseline } else { &mut patched };
            let start = Instant::now();
            let observed = guest.batch(&data, calls, expected)?;
            let elapsed_ns = start.elapsed().as_nanos();
            if elapsed_ns == 0 {
                return Err("zero-duration sample".into());
            }
            samples.push((block, position, *name, elapsed_ns, observed));
        }
    }
    println!("META,{SCHEMA},{calls},{blocks},{seed}");
    println!("CHECK,baseline,{a}");
    println!("CHECK,patched,{b}");
    for (block, position, name, elapsed_ns, observed) in samples {
        println!("SAMPLE,{block},{position},{name},{elapsed_ns},{observed}");
    }
    Ok(())
}

fn main() {
    if let Err(err) = run() {
        eprintln!("TIER1 DIVREM CALL TIMING ERROR: {err}");
        std::process::exit(1);
    }
}
