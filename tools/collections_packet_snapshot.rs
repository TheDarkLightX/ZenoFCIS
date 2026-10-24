//! Replays every shipped collection check and writes its exact packet bytes.
use std::{fs, path::PathBuf};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args_os().skip(1);
    let root = PathBuf::from(args.next().ok_or("missing repository root")?);
    let output = PathBuf::from(args.next().ok_or("missing output directory")?);
    fs::create_dir_all(&output)?;
    let examples = root.join("examples/collections");
    let mut projects: Vec<_> = fs::read_dir(examples)?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.join("project.zeno").is_file())
        .collect();
    projects.sort();
    for project in projects {
        let name = project.file_name().ok_or("example has no name")?.to_string_lossy();
        let checked = zeno_fcis_contract::collections::check(
            &fs::read_to_string(project.join("project.zeno"))?,
            &fs::read_to_string(project.join("v2/policy.json"))?,
            zeno_fcis_contract::collections::Limits::default(),
        )?;
        let packet = checked.packet().to_string();
        fs::write(output.join(format!("{name}.json")), packet.as_bytes())?;
        println!("{name}: {:?}; {} bytes", checked.verdict(), packet.len());
    }
    Ok(())
}
