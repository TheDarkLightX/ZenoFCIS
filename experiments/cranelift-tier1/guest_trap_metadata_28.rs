// Source-pinned guest-visible Wasmtime trap metadata probe, x86-64 only.
// Run under each compiled Wasmtime revision with the exact same WAT.
// This tests finite values, trap classification and original-Wasm backtrace
// locations. It does NOT prove LLVM/Cranelift pass safety or runtime speed.
use std::num::NonZeroUsize;
use wasmtime::{Config, Engine, Instance, Module, OptLevel, Store, Trap, WasmBacktrace};

fn main() -> wasmtime::Result<()> {
    let source = std::env::args().nth(1)
        .ok_or_else(|| wasmtime::Error::msg("expected one identical WAT source path"))?;
    if std::env::args().len() != 2 {
        return Err(wasmtime::Error::msg("expected exactly one WAT source path"));
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

    // Independent Rust unsigned arithmetic oracle. Same operand bit patterns,
    // wrapping result bitcast to i64. Does not count as exhaustive verification.
    let valid: &[(i64, i64)] = &[
        (0, 1), (1, 1), (7, 3), (i64::MAX, 1), (i64::MIN, 1),
        (-1, -1), (i64::MIN, -1), (i64::MAX, 9), (-7, 3), (1, -1),
    ];
    for &(x, y) in valid {
        let ux = x as u64;
        let uy = y as u64;
        let expected = ((ux / uy) ^ (ux % uy).rotate_left(17)) as i64;
        let actual = work.call(&mut store, (x, y))?;
        if actual != expected {
            return Err(wasmtime::Error::msg(format!(
                "invalid guest result x={x} y={y}: expected={expected} actual={actual}"
            )));
        }
        println!("VALUE x={x} y={y} result={actual}");
    }

    // Trap codes and Wasm-origin offsets are taken from the Wasmtime API,
    // not inferred from a Linux host SIGFPE or stderr text.
    let invalid: &[(i64, i64)] = &[
        (0, 0), (1, 0), (-1, 0), (i64::MIN, 0), (i64::MAX, 0),
    ];
    for &(x, y) in invalid {
        let error = match work.call(&mut store, (x, y)) {
            Ok(value) => return Err(wasmtime::Error::msg(format!(
                "zero-divisor returned normally: x={x} value={value}"
            ))),
            Err(error) => error,
        };
        let trap = error.downcast_ref::<Trap>()
            .ok_or_else(|| wasmtime::Error::msg("missing guest Trap classification"))?;
        if *trap != Trap::IntegerDivisionByZero {
            return Err(wasmtime::Error::msg(format!(
                "wrong guest trap x={x}: {trap:?}"
            )));
        }
        let trace = error.downcast_ref::<WasmBacktrace>()
            .ok_or_else(|| wasmtime::Error::msg("missing guest WasmBacktrace"))?;
        if trace.frames().len() != 1 {
            return Err(wasmtime::Error::msg(format!(
                "wrong guest frame count: {}", trace.frames().len()
            )));
        }
        let frame = &trace.frames()[0];
        if frame.func_index() != 0 {
            return Err(wasmtime::Error::msg("wrong Wasm function index"));
        }
        let func_offset = frame.func_offset()
            .ok_or_else(|| wasmtime::Error::msg("missing original Wasm function offset"))?;
        let module_offset = frame.module_offset()
            .ok_or_else(|| wasmtime::Error::msg("missing original Wasm module offset"))?;
        println!("TRAP x={x} y={y} code=IntegerDivisionByZero func_index=0 func_offset={func_offset} module_offset={module_offset}");
    }
    Ok(())
}
