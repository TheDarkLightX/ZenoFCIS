#!/usr/bin/env python3
"""Generate an independent SQLite app, then install its thin native mount."""
import argparse
import json
import subprocess
from pathlib import Path

APP = Path(__file__).resolve().parents[1]
ROOT = APP.parents[1]


def mount(cli, contract, out):
    out.mkdir(parents=True, exist_ok=False)
    subprocess.run([str(cli), '--no-kb', 'new', str(out), '--contract', str(contract),
                    '--source', str(ROOT)], check=True)
    fixture = contract/'TEST_FIXTURE.json'
    if fixture.is_file():
        metadata = json.loads(fixture.read_text())
        (out/'TEST_FIXTURE.json').write_bytes(fixture.read_bytes())
        readme = out/'README.md'
        readme.write_text(metadata['label']+'\n\n'+readme.read_text())
    # Keep generated code and dependency binding intact. The adapter is a child
    # of the generated session so it reuses native framing and reporting helpers.
    (out / 'src/mounted.rs').write_bytes((APP / 'mount/native.rs').read_bytes())
    with (out / 'src/session.rs').open('a') as f:
        f.write('\n#[path = "mounted.rs"]\npub mod mounted;\n')
    with (out / 'src/lib.rs').open('a') as f:
        f.write('\n#[cfg(feature = "sqlite")]\npub use session::mounted;\n')
    manifest = out / 'Cargo.toml'
    manifest.write_text(manifest.read_text().replace(
        '[features]\n', '[features]\nmount-test = []\n'))
    (out / 'src/bin').mkdir(exist_ok=True)
    (out / 'src/bin/mounted.rs').write_text('''fn main() {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    match application::mounted::run(&args) {
        Ok(text) => println!("{text}"),
        Err(error) => { eprintln!("{error}"); std::process::exit(1); }
    }
}
''')
    return {'generated_application': str(out), 'contract': str(contract),
            'native_adapter': 'src/mounted.rs', 'generator_exit': 0,
            'fixture_label': json.loads(fixture.read_text())['label'] if fixture.is_file() else None}


if __name__ == '__main__':
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--cli', type=Path, required=True)
    p.add_argument('--contract', type=Path, default=APP)
    p.add_argument('--out', type=Path, required=True)
    a = p.parse_args()
    print(json.dumps(mount(a.cli.resolve(), a.contract.resolve(), a.out.resolve())))
