//! Emits retained Solidity and Solana/Anchor fixtures from one reviewed machine.

use std::env;
use std::error::Error;
use std::ffi::OsString;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use zeno_fcis_codegen::{
    GeneratedOnchainBundle, ObservationPolicy, OnchainCapability, OnchainCapabilityKind,
    OnchainEvent, OnchainField, OnchainMachineSpec, OnchainReason, OnchainScalar, RecipientPolicy,
    SolanaAnchorSpec, SolanaFungibleBinding, SolanaTokenProgram, SolidityAdvancedSpec,
    SolidityFungibleBinding, generate_advanced_solidity, generate_solana_anchor,
};

const SOLIDITY_VERSION: &str = "0.8.36";

fn main() -> Result<(), Box<dyn Error>> {
    let output = output_directory(env::args_os().nth(1))?;
    if output.exists() {
        fs::remove_dir_all(&output)?;
    }
    fs::create_dir_all(&output)?;

    let machine = fixture_machine()?;
    let solidity = SolidityAdvancedSpec::try_new(
        machine.clone(),
        vec![SolidityFungibleBinding::try_new(7, [3_u8; 20], [4_u8; 32])?],
    )?;
    let solana = SolanaAnchorSpec::try_new(
        machine,
        [9_u8; 32],
        "treasury_state",
        vec![SolanaFungibleBinding::try_new(
            7,
            [2_u8; 32],
            SolanaTokenProgram::Legacy,
            [4_u8; 32],
        )?],
    )?;

    write_bundle(&output, generate_advanced_solidity(&solidity)?)?;
    write_bundle(&output.join("solana"), generate_solana_anchor(&solana)?)?;
    write_text(
        &output.join("solidity/treasury_machine/TreasuryMachineFixture.sol"),
        solidity_fixture_implementation(),
    )?;
    write_text(
        &output.join("README.md"),
        retained_fixture_readme(solidity.machine().machine_hash()),
    )?;
    Ok(())
}

fn fixture_machine() -> Result<OnchainMachineSpec, Box<dyn Error>> {
    let state = vec![
        field(1, "owner", OnchainScalar::Bytes32)?,
        field(2, "balance", OnchainScalar::U128)?,
    ];
    let command = vec![
        field(1, "recipient", OnchainScalar::Bytes32)?,
        field(2, "amount", OnchainScalar::U128)?,
    ];
    let reasons = vec![
        OnchainReason::try_new(1, "Unauthorized")?,
        OnchainReason::try_new(2, "InsufficientBalance")?,
    ];
    let event = OnchainEvent::try_new(
        1,
        "Paid",
        vec![
            field(1, "recipient", OnchainScalar::Bytes32)?,
            field(2, "amount", OnchainScalar::U128)?,
        ],
    )?;
    let capability = OnchainCapability::try_new(
        7,
        "Payout",
        OnchainCapabilityKind::FungibleTransfer,
        [8_u8; 32],
        RecipientPolicy::CommandField(1),
        1_000_000,
        1,
    )?;
    Ok(OnchainMachineSpec::try_new(
        "TreasuryMachine",
        1,
        state,
        command,
        reasons,
        vec![event],
        vec![capability],
        1,
        1,
        ObservationPolicy::PublicVariableShape,
    )?)
}

fn field(id: u16, name: &str, scalar: OnchainScalar) -> Result<OnchainField, Box<dyn Error>> {
    Ok(OnchainField::try_new(id, name, scalar)?)
}

fn write_bundle(root: &Path, bundle: GeneratedOnchainBundle) -> Result<(), Box<dyn Error>> {
    for file in bundle.files() {
        write_text(&root.join(file.path()), file.content())?;
    }
    Ok(())
}

fn write_text(path: &Path, content: impl AsRef<str>) -> Result<(), Box<dyn Error>> {
    let parent = path.parent().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("generated path has no parent: {}", path.display()),
        )
    })?;
    fs::create_dir_all(parent)?;
    fs::write(path, content.as_ref().as_bytes())?;
    Ok(())
}

fn output_directory(value: Option<OsString>) -> Result<PathBuf, Box<dyn Error>> {
    value.map(PathBuf::from).ok_or_else(|| {
        Box::new(io::Error::new(
            io::ErrorKind::InvalidInput,
            "usage: generate_onchain_fixtures <output-directory>",
        )) as Box<dyn Error>
    })
}

fn solidity_fixture_implementation() -> String {
    format!(
        r#"// SPDX-License-Identifier: MIT OR Apache-2.0
pragma solidity {SOLIDITY_VERSION};

import {{TreasuryMachineFcis}} from "./TreasuryMachineFcis.sol";

contract TreasuryMachineFixture is TreasuryMachineFcis {{
    function _commandAdmissible(
        Command memory command,
        Context memory
    ) internal pure override returns (bool) {{
        return command.amount != 0
            && uint256(command.recipient) <= type(uint160).max;
    }}

    function _invariant(
        State memory stateValue
    ) internal pure override returns (bool) {{
        return stateValue.owner != bytes32(0);
    }}

    function _decide(
        State memory stateValue,
        Command memory command,
        Context memory context
    ) internal pure override returns (Decision memory decision) {{
        if (context.actor != stateValue.owner) {{
            decision.kind = DecisionKind.Reject;
            decision.reasonCode = 1;
            return decision;
        }}
        if (command.amount > stateValue.balance) {{
            decision.kind = DecisionKind.Reject;
            decision.reasonCode = 2;
            return decision;
        }}

        decision.kind = DecisionKind.Accept;
        decision.nextState = stateValue;
        decision.nextState.balance = stateValue.balance - command.amount;
        decision.eventCount = 1;
        decision.events[0] = _eventPaid(command.recipient, command.amount);
        decision.effectCount = 1;
        decision.effects[0] = _effectPayout(
            command.amount,
            stateValue,
            command,
            context
        );
    }}
}}
"#
    )
}

fn retained_fixture_readme(machine_hash: zeno_fcis_codec::Hash32) -> String {
    format!(
        "# Retained generated on-chain fixture\n\nMachine hash: `{machine_hash}`\n\nThis directory is generated byte-for-byte by `generate_onchain_fixtures`. The Solidity and Anchor artifacts come from one shared `OnchainMachineSpec`. Do not edit generated files directly.\n\nThe separately retained `solana/Cargo.lock` pins the exact host-check dependency graph and is copied into a fresh generated tree before the complete byte comparison.\n\nThe permanent gate establishes deterministic regeneration, exact `solc 0.8.36` plus OpenZeppelin `5.6.1` compiler admission, and host Rust `1.97.1` checking of the Anchor workspace with the retained lock. Dedicated `/tmp` paths separate inputs and outputs; they are not a hostile-code sandbox.\n\nThe fixture does not establish EVM execution behavior, Anchor CLI or SBF compilation, Solana instruction execution, cross-chain decision parity, reproducible deployed binaries, deployed bytecode or program-data identity, audit completion, or production authorization.\n"
    )
}
