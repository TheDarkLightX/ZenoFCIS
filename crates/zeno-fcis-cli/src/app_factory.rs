//! Explicit pinned build orchestration, outside the pure contract generator.
use crate::{BLOCKED, FAILURE, INVALID, binding, transform::sha256_hex};
use std::{
    path::Path,
    process::{Command, Stdio},
};

pub(crate) fn scaffold(
    dir: &Path,
    contract: &Path,
    source: Option<&Path>,
    scratch: Option<&Path>,
) -> u8 {
    // Collection applications use the same checked producer and a plain form.
    // Their original key inputs are not the finite scalar UI's class inputs.
    if let Ok(inputs) = crate::contract_files::Inputs::read(contract)
        && let Ok(spec) = zeno_fcis_spec::parse_project(&inputs.project, Default::default())
            .and_then(|p| zeno_fcis_spec::elaborate_project(p, Default::default()))
        && !spec.collections().maps().is_empty()
    {
        return crate::contract_files::scaffold(dir, contract, source);
    }
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
            include_str!("../source-snapshots/repository/tools/app_factory.py.txt"),
        ),
        (
            "tools/zenoui.py",
            include_str!("../source-snapshots/repository/tools/zenoui.py.txt"),
        ),
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
    let mut command = Command::new("python3");
    command
        .arg("-B")
        .arg(binding.tree.join("tools/app_factory.py"))
        .arg("--cli")
        .arg(executable)
        .arg("--contract")
        .arg(contract)
        .arg("--out")
        .arg(dir)
        .stdin(Stdio::null());
    if let Some(scratch) = scratch {
        command.arg("--scratch").arg(scratch);
    }
    match command.status() {
        Ok(status) if status.success() => 0,
        Ok(_) => FAILURE,
        Err(error) => {
            eprintln!("start pinned Python app factory: {error}");
            BLOCKED
        }
    }
}

/// Offline assembly adds admission and installed-app tests around the factory.
pub(crate) fn assemble(
    description: &Path,
    out: &Path,
    source: Option<&Path>,
    scratch: &Path,
) -> u8 {
    let binding = match binding::bind(
        &include_str!("../contract-app/Cargo.toml.in")
            .replace("{package}", "app-template")
            .replace("{version}", env!("CARGO_PKG_VERSION")),
        source,
    ) {
        Ok(value) => value,
        Err(error) => return crate::binding_refused(&error),
    };
    let compiled = include_str!("../source-snapshots/repository/tools/runtime_assembler.py.txt");
    let script = binding.tree.join("tools/runtime_assembler.py");
    match std::fs::read(&script) {
        Ok(actual) if sha256_hex(&actual) == sha256_hex(compiled.as_bytes()) => {}
        _ => {
            eprintln!("pinned runtime assembler differs; rebuild the CLI");
            return INVALID;
        }
    }
    let executable = match std::env::current_exe() {
        Ok(path) => path,
        Err(_) => return FAILURE,
    };
    match Command::new("python3")
        .arg("-B")
        .arg(script)
        .arg("--cli")
        .arg(executable)
        .arg("--assembly")
        .arg(description)
        .arg("--out")
        .arg(out)
        .arg("--scratch")
        .arg(scratch)
        .stdin(Stdio::null())
        .status()
    {
        Ok(status) if status.success() => 0,
        Ok(_) => FAILURE,
        Err(_) => BLOCKED,
    }
}
