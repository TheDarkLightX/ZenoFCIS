//! Same-engine, in-process Wasmtime AOT runtime experiment.
//! This is an exploratory guest-loop timing, NOT a CPU-latency/throughput proof.
//! Inputs are SHA-256 checked archived .cwasm modules from an earlier pinned build.
use std::hint::black_box;
use std::time::Instant;
use wasmtime::{Engine, Instance, Module, Store, TypedFunc};

type E = Box<dyn std::error::Error>;

const TEST_PAIRS: [(i64, i64); 12] = [
    (0, 0), (1, 2), (-1, -2), (i64::MAX, 1), (i64::MIN, -1),
    (i64::MAX, i64::MAX), (i64::MIN, i64::MIN), (i64::MAX, i64::MIN),
    (i64::MIN, i64::MAX), (i64::MAX, -1), (i64::MIN, 1),
    (12_345_678_901_234, -12_345_678_901_234),
];
const COUNTS: [i32; 8] = [0, 1, 2, 3, 7, 17, 257, 4096];
const SCHEMA: &str = "zeno-fcis/tier1-inprocess-wasmtime/1";

struct Guest {
    store: Store<()>,
    sat: TypedFunc<(i64, i64), i64>,
    workload: TypedFunc<i32, i64>,
}

impl Guest {
    fn load(engine: &Engine, module_path: &str) -> Result<Self, E> {
        // SAFETY: the caller must verify the archived .cwasm SHA-256 against
        // the pinned CI-generated object; never deserialize untrusted modules.
        let module = unsafe { Module::deserialize_file(engine, module_path) }.map_err(|err| {
            // Preserve chained Wasmtime compatibility diagnostics. The outer
            // "failed deserialization" message by itself is not actionable.
            let message = format!("AOT deserialization failed for {module_path}: {err:#}");
            std::io::Error::new(std::io::ErrorKind::InvalidData, message)
        })?;
        let mut store = Store::new(engine, ());
        let instance = Instance::new(&mut store, &module, &[])?;
        let sat = instance.get_typed_func::<(i64, i64), i64>(&mut store, "sat")?;
        let workload = instance.get_typed_func::<i32, i64>(&mut store, "workload")?;
        Ok(Self { store, sat, workload })
    }

    fn verify(&mut self) -> Result<(usize, usize), E> {
        for (x, y) in TEST_PAIRS {
            let want = sat_reference(x, y);
            let got = self.sat.call(&mut self.store, (x, y))?;
            if want != got {
                return Err(format!("sat mismatch for {x}, {y}: wanted {want}, got {got}").into());
            }
        }
        // A deterministic seeded oracle set independent from Wasm/Cranelift.
        let mut state: u64 = 0x9e37_79b9_7f4a_7c15;
        for _ in 0..1024 {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            let x = state as i64;
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            let y = state as i64;
            let want = sat_reference(x, y);
            let got = self.sat.call(&mut self.store, (x, y))?;
            if want != got {
                return Err(format!("seeded mismatch for {x}, {y}").into());
            }
        }
        for count in COUNTS {
            let want = workload_reference(count);
            let got = self.workload.call(&mut self.store, count)?;
            if want != got {
                return Err(format!("workload mismatch for n={count}: {want} != {got}").into());
            }
        }
        Ok((TEST_PAIRS.len() + 1024, COUNTS.len()))
    }
}

fn sat_reference(x: i64, y: i64) -> i64 {
    ((x as i128) + (y as i128)).clamp(i64::MIN as i128, i64::MAX as i128) as i64
}

fn workload_reference(count: i32) -> i64 {
    let mut value = 1_i64;
    for i in 0..count {
        let rhs = if i & 1 == 0 { i64::MIN + 9 } else { i64::MAX - 7 };
        value = sat_reference(value, rhs);
    }
    value
}

struct Sample {
    block: usize,
    pos: usize,
    which: &'static str,
    ns: u128,
    checksum: u64,
}

fn run() -> Result<(), E> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 6 {
        return Err("usage: tier1-wasmtime-inprocess BASE.cwasm PATCH.cwasm LOOPS REPS BLOCKS".into());
    }
    let loops: i32 = args[3].parse()?;
    let reps: usize = args[4].parse()?;
    let blocks: usize = args[5].parse()?;
    if !(1000..=2_000_000).contains(&loops) || !(1..=64).contains(&reps)
        || !(2..=32).contains(&blocks) || blocks % 2 != 0 {
        return Err("bounded timing arguments required".into());
    }

    let engine = Engine::default();
    let mut baseline = Guest::load(&engine, &args[1])?;
    let mut patched = Guest::load(&engine, &args[2])?;
    let b_checks = baseline.verify()?;
    let p_checks = patched.verify()?;
    if b_checks != p_checks { return Err("baseline/patch coverage differs".into()); }

    let expected = workload_reference(loops);
    for _ in 0..4 {
        if baseline.workload.call(&mut baseline.store, loops)? != expected
            || patched.workload.call(&mut patched.store, loops)? != expected {
            return Err("warmup oracle mismatch".into());
        }
    }

    let mut samples = Vec::with_capacity(blocks * 4);
    for block in 0..blocks {
        let order = if block % 2 == 0 {
            ["baseline", "patched", "patched", "baseline"]
        } else {
            ["patched", "baseline", "baseline", "patched"]
        };
        for (pos, which) in order.iter().enumerate() {
            let guest = if *which == "baseline" { &mut baseline } else { &mut patched };
            let begin = Instant::now();
            let mut checksum: u64 = 0;
            for _ in 0..reps {
                let result = guest.workload.call(&mut guest.store, black_box(loops))?;
                if result != expected { return Err("timed guest oracle mismatch".into()); }
                checksum = checksum.wrapping_add(black_box(result as u64));
            }
            let ns = begin.elapsed().as_nanos();
            if ns == 0 { return Err("zero-duration timing invalid".into()); }
            samples.push(Sample { block, pos, which, ns, checksum: black_box(checksum) });
        }
    }

    // No output during timed regions.
    println!("META,{SCHEMA},{loops},{reps},{blocks}");
    println!("CHECK,baseline,{},{}", b_checks.0, b_checks.1);
    println!("CHECK,patched,{},{}", p_checks.0, p_checks.1);
    for x in samples {
        println!("SAMPLE,{},{},{},{},{}", x.block, x.pos, x.which, x.ns, x.checksum);
    }
    Ok(())
}

fn main() {
    if let Err(err) = run() {
        eprintln!("TIER1 IN-PROCESS BENCH FAILED: {err}");
        std::process::exit(1);
    }
}
