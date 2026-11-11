//! Identical optimized Rust-generated Wasm, baseline versus patched Cranelift.
//! Balanced in-process calls; one virtual host, NOT a general speedup claim.
//! AOT files must pass the Python source/hash preflight before deserialization.
use std::{hint::black_box, time::Instant};
use wasmtime::{Engine, Instance, Module, Store, TypedFunc};

type E = Box<dyn std::error::Error>;
const SCHEMA: &str = "zenofcis/tier1-rust-reconstruction-call-timing/1";
const FUNCS: [&str; 6] = [
    "divrem_pair", "divrem_offset", "divrem_reverse",
    "divrem_swapped", "div_only", "signed_divrem",
];

fn vectors() -> Vec<(i64, i64)> {
    // All nonzero operands, and no signed MIN/-1 pair. Every source
    // function is therefore a value-producing Wasm call in this corpus.
    let mut v = vec![
        (1, 1), (7, 3), (i64::MIN, 1), (i64::MAX, 1),
        (-1, -1), (i64::MAX, i64::MIN), (i64::MIN, i64::MAX),
        (123_456_789, -7), (1, -1), (-1, 1), (i64::MIN, 3),
        (i64::MAX, 7), (7, 2), (i64::MIN, 2),
        (123_456_789, 98_765), (-987_654_321, 31_337),
    ];
    let mut s: u64 = 0x5623_2026_11;
    for _ in 0..256 {
        s ^= s << 13;
        s ^= s >> 7;
        s ^= s << 17;
        let x = if s == 0 { 1 } else { s as i64 };
        s ^= s << 13;
        s ^= s >> 7;
        s ^= s << 17;
        let y = if s == 0 { 1 } else { s as i64 };
        // A pair that can overflow signed division is excluded.
        if x == i64::MIN && y == -1 { continue; }
        v.push((x, y));
    }
    v
}

fn oracle(kind: &str, x: i64, y: i64) -> Result<i64, E> {
    if y == 0 || (kind == "divrem_swapped" && x == 0)
        || (kind == "signed_divrem" && x == i64::MIN && y == -1)
    {
        return Err("outside admitted value domain".into());
    }
    if kind == "signed_divrem" {
        return Ok((x / y) ^ (x % y).rotate_left(17));
    }
    let (a, b) = (x as u64, y as u64);
    let q = a / b;
    let r = match kind {
        "divrem_swapped" => b % a,
        "divrem_pair" | "divrem_offset" | "divrem_reverse" | "div_only" => a % b,
        _ => return Err("unknown function".into()),
    };
    Ok(match kind {
        "div_only" => q as i64,
        "divrem_offset" => (q.wrapping_add(3) ^ r.rotate_left(17)) as i64,
        _ => (q ^ r.rotate_left(17)) as i64,
    })
}

struct Guest {
    store: Store<()>,
    f: TypedFunc<(i64, i64), i64>,
}
impl Guest {
    fn new(engine: &Engine, aot: &str, export: &str) -> Result<Self, E> {
        // SAFETY: file is SHA-256-checked against the archived producer's
        // exact Wasmtime+Rust+Wasm receipt by CI before reaching this loader.
        let module = unsafe { Module::deserialize_file(engine, aot) }?;
        let mut store = Store::new(engine, ());
        let instance = Instance::new(&mut store, &module, &[])?;
        let f = instance.get_typed_func::<(i64, i64), i64>(&mut store, export)?;
        Ok(Self { store, f })
    }
    fn check(&mut self, name: &str, inputs: &[(i64, i64)]) -> Result<(), E> {
        for &(x, y) in inputs {
            let expected = oracle(name, x, y)?;
            let got = self.f.call(&mut self.store, (x, y))?;
            if got != expected {
                return Err(format!("mismatch {name}({x},{y}): {got} != {expected}").into());
            }
        }
        if self.f.call(&mut self.store, (7, 0)).is_ok() {
            return Err("zero divisor did not trap".into());
        }
        if name == "divrem_swapped"
            && self.f.call(&mut self.store, (0, 7)).is_ok()
        {
            return Err("swapped numerator zero divisor did not trap".into());
        }
        if name == "signed_divrem"
            && self.f.call(&mut self.store, (i64::MIN, -1)).is_ok()
        {
            return Err("signed MIN/-1 overflow did not trap".into());
        }
        Ok(())
    }
    fn batch(&mut self, inputs: &[(i64, i64)], calls: usize, expected: u64)
        -> Result<(), E>
    {
        let mut checksum = 0_u64;
        for k in 0..calls {
            let (x, y) = black_box(inputs[k % inputs.len()]);
            let got = self.f.call(&mut self.store, (x, y))?;
            checksum = checksum.wrapping_add(black_box(got as u64));
        }
        if checksum != expected {
            return Err("incorrect timed native Wasm checksum".into());
        }
        Ok(())
    }
}

fn run() -> Result<(), E> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 6 {
        return Err("usage: bench OPTIMIZED_RESULTS_DIR CALLS BLOCKS SEED ab|aa|bb|ab_rev|aa_rev|bb_rev".into());
    }
    let root = &args[1];
    let calls: usize = args[2].parse()?;
    let blocks: usize = args[3].parse()?;
    let seed: usize = args[4].parse()?;
    // Reverse-order modes invert *which module is loaded first*, not the
    // measured A/B labels. This probes the code-placement confound observed
    // in the same-AOT sham runs; it does NOT pin executable addresses.
    let reverse_load = args[5].ends_with("_rev");
    let (left, right) = match args[5].trim_end_matches("_rev") {
        "ab" => ("baseline", "patched"),
        "aa" => ("baseline", "baseline"),
        "bb" => ("patched", "patched"),
        _ => return Err("unrecognized A/B or sham comparison mode".into()),
    };
    if !(20_000..=500_000).contains(&calls)
        || !(4..=32).contains(&blocks) || blocks % 2 != 0 || seed != 5623
    {
        return Err("invalid bounded timing budget or seed".into());
    }
    let engine = Engine::default();
    let inputs = vectors();
    println!("META,{SCHEMA},{calls},{blocks},{seed},{}", args[5]);
    for name in FUNCS {
        let aot_b = format!("{root}/{left}/native.cwasm");
        let aot_p = format!("{root}/{right}/native.cwasm");
        // Both Guest objects remain alive throughout this function's
        // measurements. Reverse only construction order to counterbalance
        // the first-loaded versus second-loaded module addresses.
        let (mut baseline, mut patched) = if reverse_load {
            let second = Guest::new(&engine, &aot_p, name)?;
            let first = Guest::new(&engine, &aot_b, name)?;
            (first, second)
        } else {
            let first = Guest::new(&engine, &aot_b, name)?;
            let second = Guest::new(&engine, &aot_p, name)?;
            (first, second)
        };
        baseline.check(name, &inputs)?;
        patched.check(name, &inputs)?;
        println!("CHECK,{name},baseline,{}", inputs.len());
        println!("CHECK,{name},patched,{}", inputs.len());
        let mut expected = 0_u64;
        for k in 0..calls {
            let (x, y) = inputs[k % inputs.len()];
            expected = expected.wrapping_add(oracle(name, x, y)? as u64);
        }
        for _ in 0..2 {
            baseline.batch(&inputs, calls, expected)?;
            patched.batch(&inputs, calls, expected)?;
        }
        for block in 0..blocks {
            let order = if block % 2 == 0 {
                ["baseline", "patched", "patched", "baseline"]
            } else {
                ["patched", "baseline", "baseline", "patched"]
            };
            for (position, variant) in order.into_iter().enumerate() {
                let guest = if variant == "baseline" { &mut baseline } else { &mut patched };
                let start = Instant::now();
                guest.batch(&inputs, calls, expected)?;
                let ns = start.elapsed().as_nanos();
                if ns == 0 { return Err("zero-duration sample".into()); }
                println!("SAMPLE,{name},{block},{position},{variant},{ns},{expected}");
            }
        }
    }
    Ok(())
}

fn main() {
    if let Err(e) = run() {
        eprintln!("tier1 Rust reconstruction perf error: {e}");
        std::process::exit(1);
    }
}
