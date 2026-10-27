//! Private construction experiment; no installation, adoption or publication.

/// Recorded source-tree setting shared with application binding.
#[path = "../src/build_tree.rs"]
pub(crate) mod build_tree;

#[path = "construction_pilot/common.rs"]
mod common;
#[path = "construction_pilot/compilation.rs"]
mod compilation;
#[path = "construction_pilot/controls.rs"]
mod controls;
#[path = "construction_pilot/files.rs"]
mod files;
#[path = "construction_pilot/layout.rs"]
mod layout;
#[path = "construction_pilot/native.rs"]
mod native;
#[path = "construction_pilot/phases.rs"]
mod phases;
#[path = "construction_pilot/proposal.rs"]
mod proposal;
#[path = "construction_pilot/qualification.rs"]
mod qualification;
#[path = "construction_pilot/reference.rs"]
mod reference;
#[path = "construction_pilot/search.rs"]
mod search;
#[path = "construction_pilot/summary.rs"]
mod summary;

use clap::{Parser, Subcommand};
use std::path::{Path, PathBuf};

#[derive(Parser)]
#[command(
    name = "construction_pilot",
    about = "Private bounded construction experiment"
)]
struct Arguments {
    #[command(subcommand)]
    phase: Phase,
}
#[derive(Subcommand)]
enum Phase {
    Prepare {
        #[arg(long)]
        reference: PathBuf,
        #[arg(long)]
        f51_evidence: PathBuf,
        #[arg(long)]
        out: PathBuf,
    },
    Preflight {
        #[arg(long)]
        bundle: PathBuf,
        #[arg(long)]
        out: PathBuf,
    },
    Search {
        #[arg(long)]
        bundle: PathBuf,
        #[arg(long)]
        owner_approval: PathBuf,
        #[arg(long,value_parser=["T","I"])]
        layout: String,
        #[arg(long)]
        cli: PathBuf,
        #[arg(long)]
        out: PathBuf,
    },
    Replay {
        #[arg(long)]
        bundle: PathBuf,
        #[arg(long)]
        run: PathBuf,
        #[arg(long)]
        cli: PathBuf,
        #[arg(long)]
        out: PathBuf,
    },
    Controls {
        #[arg(long)]
        bundle: PathBuf,
        #[arg(long)]
        t_run: PathBuf,
        #[arg(long)]
        i_run: PathBuf,
        #[arg(long)]
        cli: PathBuf,
        #[arg(long)]
        out: PathBuf,
    },
    Summarize {
        #[arg(long)]
        bundle: PathBuf,
        #[arg(long)]
        t_run: PathBuf,
        #[arg(long)]
        i_run: PathBuf,
        #[arg(long)]
        replays: PathBuf,
        #[arg(long)]
        controls: PathBuf,
        #[arg(long)]
        meta_log: PathBuf,
        #[arg(long)]
        review: Option<PathBuf>,
        #[arg(long)]
        out: PathBuf,
    },
}
impl Phase {
    fn out(&self) -> &Path {
        match self {
            Self::Prepare { out, .. }
            | Self::Preflight { out, .. }
            | Self::Search { out, .. }
            | Self::Replay { out, .. }
            | Self::Controls { out, .. }
            | Self::Summarize { out, .. } => out,
        }
    }
    fn name(&self) -> &'static str {
        match self {
            Self::Prepare { .. } => "prepare",
            Self::Preflight { .. } => "preflight",
            Self::Search { .. } => "search",
            Self::Replay { .. } => "replay",
            Self::Controls { .. } => "controls",
            Self::Summarize { .. } => "summarize",
        }
    }
    fn evidence(&self) -> common::Result<PathBuf> {
        let root = match self {
            Self::Prepare { out, .. } => out.parent(),
            Self::Preflight { bundle, .. }
            | Self::Search { bundle, .. }
            | Self::Replay { bundle, .. }
            | Self::Controls { bundle, .. }
            | Self::Summarize { bundle, .. } => bundle.parent(),
        };
        root.map(Path::to_path_buf)
            .ok_or_else(|| common::malformed("missing-evidence-root"))
    }
}
#[cfg(not(test))]
#[allow(dead_code)] // Standalone example entry; the authoring CLI calls construct.
fn main() {
    let args = match Arguments::try_parse() {
        Ok(args) => args,
        Err(error) => {
            let help = matches!(
                error.kind(),
                clap::error::ErrorKind::DisplayHelp | clap::error::ErrorKind::DisplayVersion
            );
            let _ = error.print();
            std::process::exit(if help { 0 } else { 65 });
        }
    };
    let result = phases::execute(args.phase);
    std::process::exit(result);
}

#[cfg(test)]
#[path = "../tests/construction_pilot/cases.rs"]
mod tests;

#[allow(dead_code)]
/// Construct from an approved frozen requirements bundle using the pilot gate.
#[derive(clap::Args)]
pub(crate) struct Construct {
    /// Supported requirements family. Other families refuse by name.
    #[arg(long, default_value = "priority-collection")]
    family: String,
    /// Frozen requirements bundle directory, including reference and grammar pins.
    #[arg(long)]
    requirement: PathBuf,
    /// Owner approval bound to the exact bundle manifest.
    #[arg(long)]
    owner_approval: PathBuf,
    /// Arrival-indexed T or identity-indexed I layout.
    #[arg(long, value_parser = ["T", "I"])]
    layout: String,
    /// Fresh output directory for candidates, counterexamples and receipts.
    #[arg(long)]
    out: PathBuf,
}

#[allow(dead_code)]
pub(crate) fn construct(args: Construct) -> u8 {
    if args.family != "priority-collection" {
        eprintln!("unsupported construction family: {}", args.family);
        return 65;
    }
    if let Err(error) = files::repository() {
        eprintln!("{}", error.reason);
        return 65;
    }
    let cwd = match std::env::current_dir() {
        Ok(path) => path,
        Err(error) => {
            eprintln!("{error}");
            return 3;
        }
    };
    // Resolve existing parents once, retaining the final component so the
    // normal intake still sees and refuses symlink artifacts and existing output.
    let paths = (|| -> std::io::Result<_> {
        Ok((
            resolve_parent(&cwd.join(args.requirement))?,
            resolve_parent(&cwd.join(args.owner_approval))?,
            resolve_parent(&cwd.join(args.out))?,
        ))
    })();
    let (requirement, owner_approval, out) = match paths {
        Ok(paths) => paths,
        Err(error) => {
            eprintln!("{error}");
            return 3;
        }
    };
    let cli = match std::env::current_exe() {
        Ok(path) => path,
        Err(error) => {
            eprintln!("{error}");
            return 3;
        }
    };
    let result = phases::execute(Phase::Search {
        bundle: requirement,
        owner_approval,
        layout: args.layout,
        cli,
        out,
    });
    u8::try_from(result).unwrap_or(3)
}

fn resolve_parent(path: &Path) -> std::io::Result<PathBuf> {
    let invalid = || {
        std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "path needs a final component",
        )
    };
    let mut suffix = vec![path.file_name().ok_or_else(invalid)?.to_os_string()];
    let mut parent = path.parent().ok_or_else(invalid)?;
    loop {
        match parent.canonicalize() {
            Ok(mut resolved) => {
                for component in suffix.into_iter().rev() {
                    resolved.push(component);
                }
                return Ok(resolved);
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                suffix.push(parent.file_name().ok_or_else(invalid)?.to_os_string());
                parent = parent.parent().ok_or_else(invalid)?;
            }
            Err(error) => return Err(error),
        }
    }
}
