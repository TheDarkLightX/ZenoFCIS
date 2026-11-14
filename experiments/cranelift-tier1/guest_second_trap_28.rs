// Experimental guest-visible trap-site identity check for TWO quotient/remainder pairs.
// Identical WAT on pristine and detached experimental Wasmtime builds, x86-64 only.
// This establishes finite guest trap locations, NOT machine-PC or compiler-wide proof.
use std::num::NonZeroUsize;
use wasmtime::{Config, Engine, Instance, Module, OptLevel, Store, Trap, WasmBacktrace};

fn main() -> wasmtime::Result<()> {
    let mut args = std::env::args().skip(1);
    let source = args.next().ok_or_else(|| wasmtime::Error::msg("expected WAT path"))?;
    if args.next().is_some() {
        return Err(wasmtime::Error::msg("expected exactly one WAT path"));
    }
    let mut config = Config::new();
    config.cranelift_opt_level(OptLevel::SpeedAndSize);
    config.generate_address_map(true);
    config.wasm_backtrace_max_frames(NonZeroUsize::new(8));
    let engine = Engine::new(&config)?;
    let module = Module::from_file(&engine, &source)?;
    let mut store = Store::new(&engine, ());
    let instance = Instance::new(&mut store, &module, &[])?;
    let work = instance.get_typed_func::<(i64, i64), i64>(&mut store, "work")?;

    let valid: &[(i64, i64)] = &[
        (0, 1), (1, 1), (7, 3), (-1, 1), (i64::MIN, 1),
        (i64::MAX, i64::MAX), (-1, -2), (17, 5),
    ];
    for &(x, y) in valid {
        let ux = x as u64;
        let uy = y as u64;
        let (q, r) = (ux / uy, ux % uy);
        let shifted_x = ux.wrapping_add(1);
        let shifted_y = uy.wrapping_add(1);
        assert_ne!(shifted_y, 0, "invalid nominally finite second divisor");
        let (q2, r2) = (shifted_x / shifted_y, shifted_x % shifted_y);
        let expected = ((q ^ r.rotate_left(17)) ^ (q2 ^ r2.rotate_left(17))) as i64;
        let actual = work.call(&mut store, (x, y))?;
        if actual != expected {
            return Err(wasmtime::Error::msg(format!(
                "guest arithmetic mismatch x={x} y={y}: expected={expected} got={actual}"
            )));
        }
        println!("VALUE x={x} y={y} result={actual}");
    }

    let invalid: &[(&str, i64, i64)] = &[
        ("first", 0, 0), ("first", 1, 0), ("first", -1, 0),
        ("second", 0, -1), ("second", 1, -1),
        ("second", -1, -1), ("second", i64::MIN, -1),
    ];
    let mut first_site = None;
    let mut second_site = None;
    for &(site, x, y) in invalid {
        let error = match work.call(&mut store, (x, y)) {
            Ok(value) => return Err(wasmtime::Error::msg(format!(
                "missing {site} zero-divisor trap x={x} y={y}, returned {value}"
            ))),
            Err(error) => error,
        };
        let trap = error.downcast_ref::<Trap>()
            .ok_or_else(|| wasmtime::Error::msg("missing Wasmtime Trap type"))?;
        if *trap != Trap::IntegerDivisionByZero {
            return Err(wasmtime::Error::msg(format!("wrong {site} trap: {trap:?}")));
        }
        let trace = error.downcast_ref::<WasmBacktrace>()
            .ok_or_else(|| wasmtime::Error::msg("missing WasmBacktrace"))?;
        if trace.frames().len() != 1 || trace.frames()[0].func_index() != 0 {
            return Err(wasmtime::Error::msg("wrong guest trap frame"));
        }
        let frame = &trace.frames()[0];
        let func_offset = frame.func_offset()
            .ok_or_else(|| wasmtime::Error::msg("missing guest function offset"))?;
        let module_offset = frame.module_offset()
            .ok_or_else(|| wasmtime::Error::msg("missing guest module offset"))?;
        let current = (func_offset, module_offset);
        let slot = if site == "first" { &mut first_site } else { &mut second_site };
        if let Some(previous) = *slot {
            if previous != current {
                return Err(wasmtime::Error::msg("input-dependent trap location within same site"));
            }
        } else {
            *slot = Some(current);
        }
        println!("TRAP site={site} x={x} y={y} code=IntegerDivisionByZero func_index=0 func_offset={func_offset} module_offset={module_offset}");
    }
    if first_site.is_none() || second_site.is_none() || first_site == second_site {
        return Err(wasmtime::Error::msg("distinct guest div-by-zero sites collapsed"));
    }
    Ok(())
}
