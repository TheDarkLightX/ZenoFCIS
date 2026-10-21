//! Explicit pinned build orchestration, outside the pure contract generator.
use crate::{BLOCKED, FAILURE, INVALID, binding, transform::sha256_hex};
use std::{
    path::Path,
    process::{Command, Stdio},
};

pub(crate) fn scaffold(dir: &Path, contract: &Path, source: Option<&Path>) -> u8 {
    let binding = match binding::bind(
        &include_str!("../contract-app/Cargo.toml.in")
            .replace("{package}", "app-template")
            .replace("{version}", env!("CARGO_PKG_VERSION")),
        source,
    ) {
        Ok(value) => value,
        Err(error) => return crate::binding_refused(&error),
    };
    for (relative, compiled) in [
        (
            "tools/app_factory.py",
            include_str!("../../../tools/app_factory.py"),
        ),
        ("tools/zenoui.py", include_str!("../../../tools/zenoui.py")),
    ] {
        let Ok(actual) = std::fs::read(binding.tree.join(relative)) else {
            eprintln!("missing pinned build tool: {relative}");
            return BLOCKED;
        };
        if sha256_hex(&actual) != sha256_hex(compiled.as_bytes()) {
            eprintln!("pinned build tool differs: {relative}; rebuild the CLI");
            return INVALID;
        }
    }
    let executable = match std::env::current_exe() {
        Ok(path) => path,
        Err(error) => {
            eprintln!("locate CLI: {error}");
            return FAILURE;
        }
    };
    println!(
        "app build: running the pinned Python app factory and ZenoUI checks; see app-manifest.json"
    );
    match Command::new("python3")
        .arg(binding.tree.join("tools/app_factory.py"))
        .arg("--cli")
        .arg(executable)
        .arg("--contract")
        .arg(contract)
        .arg("--out")
        .arg(dir)
        .stdin(Stdio::null())
        .status()
    {
        Ok(status) if status.success() => 0,
        Ok(_) => FAILURE,
        Err(error) => {
            eprintln!("start pinned Python app factory: {error}");
            BLOCKED
        }
    }
}
