//! Post-Souper in-process qualification of the SAME AOTs from the successful CLI A/B.
//! No Wasm compilation, AOT loading, printing or process startup inside timed spans.
//! This is still a noisy one-host exploratory measurement, NOT a speedup theorem.
use std::hint::black_box;
use std::time::Instant;
use wasmtime::{Engine, Instance, Module, Store, TypedFunc};

type Failure = Box<dyn std::error::Error>;
const SCHEMA: &str = "zenofcis/tier1-post-souper-inprocess/1";
const SEED: u64 = 781234567891;
const STEP: u64 = 0x9e3779b97f4a7c15; // -7046029254386353131 modulo 2^64.

fn oracle(pair: bool, n: u64, seed: u64) -> i64 {
    let mut x = seed;
    let mut y = 3_u64;
    let mut acc = 0_u64;
    for _ in 0..n {
        let (q, r) = (x / y, x % y);
        let term = if pair { q ^ r.rotate_left(17) } else { q };
        acc ^= term;
        x = x.wrapping_add(STEP);
        y = y.wrapping_add(2);
    }
    acc as i64
}

struct Guest {
    store: Store<()>,
    work: TypedFunc<(i64, i64), i64>,
}
impl Guest {
    fn load(engine: &Engine, path: &str) -> Result<Self, Failure> {
        // SAFETY: the workflow MUST validate the *exact* original artifact
        // receipt and every AOT SHA-256 before invoking this executable.
        let module = unsafe { Module::deserialize_file(engine, path) }?;
        let mut store = Store::new(engine, ());
        let instance = Instance::new(&mut store, &module, &[])?;
        let work = instance.get_typed_func::<(i64, i64), i64>(&mut store, "work")?;
        Ok(Self { store, work })
    }
    fn call(&mut self, n: u64) -> Result<i64, Failure> {
        Ok(self.work.call(&mut self.store, (n as i64, SEED as i64))?)
    }
}

fn main_run() -> Result<(), Failure> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 7 {
        return Err("usage: B_PAIR.cwasm P_PAIR.cwasm B_CTRL.cwasm P_CTRL.cwasm ITERS CYCLES".into());
    }
    let iterations: u64 = args[5].parse()?;
    let cycles: usize = args[6].parse()?;
    if !(10_000..=8_000_000).contains(&iterations) || !(2..=12).contains(&cycles) {
        return Err("outside finite timing budget".into());
    }
    let engine = Engine::default();
    let mut guests = [
        Guest::load(&engine, &args[1])?,
        Guest::load(&engine, &args[2])?,
        Guest::load(&engine, &args[3])?,
        Guest::load(&engine, &args[4])?,
    ];
    let kinds = ["pair", "quotient_control"];
    let mut observed = Vec::with_capacity(cycles * 8);
    let mut checks = Vec::new();

    for (kind_i, kind) in kinds.iter().enumerate() {
        let pair = kind_i == 0;
        let small = oracle(pair, 37, SEED);
        let expected = oracle(pair, iterations, SEED);
        let base = 2 * kind_i;

        // Finite value oracle and both original zero/nonzero loop limits.
        for &i in &[base, base + 1] {
            for (n, want) in [(0, 0), (1, oracle(pair, 1, SEED)), (37, small),
                              (iterations, expected)] {
                if guests[i].call(n)? != want {
                    return Err(format!("{kind} variant {i}: n={n} value mismatch").into());
                }
            }
            for _ in 0..2 {
                if guests[i].call(iterations)? != expected {
                    return Err("warmup value mismatch".into());
                }
            }
        }
        checks.push(format!("CHECK,{kind},37,{small},{iterations},{expected}"));

        // Alternating ABBA/BAAB samples. Retain BOTH variant Stores and
        // deserialized Modules throughout; no process or AOT load in span.
        for cycle in 0..cycles {
            let order: [usize; 4] = if cycle % 2 == 0 {
                [0, 1, 1, 0]
            } else {
                [1, 0, 0, 1]
            };
            for (position, local) in order.iter().enumerate() {
                let mode = if *local == 0 { "baseline" } else { "patched" };
                let guest = &mut guests[base + local];
                let start = Instant::now();
                let result = black_box(guest.call(black_box(iterations))?);
                let elapsed = start.elapsed().as_nanos();
                if elapsed == 0 || result != expected {
                    return Err("timed value mismatch or invalid duration".into());
                }
                observed.push(format!("SAMPLE,{kind},{cycle},{position},{mode},{elapsed},{result}"));
            }
        }
    }
    println!("META,{SCHEMA},{iterations},{cycles},{SEED}");
    for row in checks { println!("{row}"); }
    for row in observed { println!("{row}"); }
    Ok(())
}

fn main() {
    if let Err(e) = main_run() {
        eprintln!("TIER1 POST-SOUPER IN-PROCESS ERROR: {e}");
        std::process::exit(1);
    }
}
