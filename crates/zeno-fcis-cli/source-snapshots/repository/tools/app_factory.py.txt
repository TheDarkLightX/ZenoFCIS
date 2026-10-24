#!/usr/bin/env python3
"""Explicit offline app build using the pinned core and ZenoUI generators."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import tomllib
import sys

ROOT = Path(__file__).resolve().parents[1]
sys.dont_write_bytecode = True

def run(*command, **kwargs):
    print("app build:", " ".join(map(str, command)), flush=True)
    return subprocess.run(["nice", "-n", "19", "ionice", "-c3", *map(str, command)], check=True, **kwargs)

def refusal(error):
    """One developer-facing reason, including refusals from a child checker."""
    details = getattr(error, "stderr", None) or str(error)
    if not getattr(error, "stderr", None) and getattr(error, "stdout", None):
        try:
            report = json.loads(error.stdout)
            details = report.get("reason") or f"AppContractLaws:{report.get('status', 'refused')}"
        except (ValueError, TypeError, AttributeError):
            pass
    if isinstance(details, bytes):
        details = details.decode("utf-8", errors="replace")
    if "U128SubjectUnsupported" in details:
        return "U128SubjectUnsupported: the native UI binding does not support U128"
    lines = [line.strip() for line in details.splitlines() if line.strip()]
    return " ".join((lines[-1] if lines else "checker refused").split())

def build_ui(*command, **kwargs):
    # Child tracebacks stay out of the developer output. Its exact refusal is
    # returned as one line; no failed UI check becomes a successful app build.
    try:
        result = run(*command, capture_output=True, text=True, **kwargs)
    except subprocess.CalledProcessError as error:
        raise ValueError(refusal(error)) from None
    if result.stdout:
        print(result.stdout, end="", flush=True)

# Enumeration is exact and, at this size, far faster than the dual-solver
# review, whose queries grow with the declared pre-states (a 1,296,000-input
# template enumerates in under two minutes; the solver review gave no answer in
# 45). Larger domains still need a solver-checked symbolic review.
FACTORY_MAX_TUPLES = 1 << 22

def contract_review(cli, contract, scratch=None):
    """Retain native exhaustive or dual-solver law evidence before emission."""
    scratch = temporary_parent(scratch)
    with tempfile.TemporaryDirectory(prefix="app-review-", dir=scratch) as temporary:
        packet = Path(temporary) / "review.json"
        run(cli.resolve(), "contract", "review", contract, "--max-tuples", FACTORY_MAX_TUPLES,
            "--out", packet, "--format", "json", capture_output=True, text=True)
        review = json.loads(packet.read_text())
        if review.get("mode") == "symbolic" and review.get("status") != "solver-checked":
            raise ValueError("AppContractLawsUnknownOrRefuted")
        return review

def verify_review_subject(review, output):
    # The native scaffold owns one read snapshot. Bind its actual copied
    # subject and generated schema to the review, including concurrent edits.
    for name, key in [("project.zeno", "project_sha256"), ("v2/policy.json", "rules_sha256"),
                      ("v2/schema.zcve", "schema_sha256")]:
        actual = hashlib.sha256((output / name).read_bytes()).hexdigest()
        if actual != review.get("sources", {}).get(key):
            raise ValueError("AppContractChangedAfterReview:" + name)

def external(lock):
    return {(p["name"], p["version"], p["source"], p.get("checksum"))
            for p in tomllib.loads(lock)["package"] if "source" in p}

def hashes(root):
    return {p.relative_to(root).as_posix(): hashlib.sha256(p.read_bytes()).hexdigest()
            for p in sorted(root.rglob("*")) if p.is_file()}

def prepare_library(cli, template, site):
    """Stage reviewed files; reuse only a complete matching library."""
    destination = site / "apps" / template
    destination.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix=".library-", dir=destination.parent) as temporary:
        staged = Path(temporary) / template
        run(cli.resolve(), "new", staged, "--template", template, "--source", ROOT)
        manifest = staged / "Cargo.toml"
        manifest.write_text(manifest.read_text().split("\n[patch.crates-io]\n", 1)[0])
        for name in ["Cargo.lock", "rust-toolchain.toml"]:
            (staged / name).unlink(missing_ok=True)
        expected = hashes(staged)
        if destination.exists():
            for name, digest in expected.items():
                path = destination / name
                if not path.is_file() or hashlib.sha256(path.read_bytes()).hexdigest() != digest:
                    raise ValueError(f"AppLibraryIncompleteOrChanged:{template}:{name}")
            return
        staged.rename(destination)

def outside_source(path):
    """Bind a writable shell path without admitting a source-tree destination."""
    path = path.resolve()
    if path.is_relative_to(ROOT.resolve()):
        raise ValueError("AppBuildPathInsideSource:" + str(path))
    return path

def temporary_parent(supplied=None):
    """Choose a shell scratch parent without probing any source directory."""
    if supplied is not None:
        return outside_source(supplied)
    # tempfile.gettempdir() probes candidates by writing. Filter the standard
    # POSIX candidates before TemporaryDirectory performs its first write.
    for name in [os.environ.get(key) for key in ("TMPDIR", "TEMP", "TMP")] + ["/tmp", "/var/tmp", "/usr/tmp"]:
        if not name:
            continue
        try:
            candidate = outside_source(Path(name))
        except ValueError:
            continue
        if candidate.is_dir() and os.access(candidate, os.W_OK | os.X_OK):
            return candidate
    raise ValueError("AppScratchUnavailable: name --scratch outside the source tree")

def prepare_workspace(cli, work):
    """Copy build inputs; only this invocation's site may receive generated files."""
    site = work / "site"
    shutil.copytree(ROOT / "site", site, copy_function=shutil.copyfile,
                    ignore=shutil.ignore_patterns("apps", "target", "node_modules", "__pycache__"))
    # A read-only source copy must still yield writable invocation scratch.
    for path in [site, *site.rglob("*")]:
        path.chmod(0o755 if path.is_dir() else 0o644)
    # Embedded fixtures use relative paths; the link is an input, never a target.
    (work / "crates").symlink_to(ROOT / "crates", target_is_directory=True)
    manifest = site / "Cargo.toml"
    text = manifest.read_text()
    for binding in tomllib.loads(text)["patch"]["crates-io"].values():
        relative = binding["path"]
        text = text.replace('path = ' + json.dumps(relative),
                            'path = ' + json.dumps(str((ROOT / "site" / relative).resolve())))
    manifest.write_text(text)
    for template in ["account-lockout", "order-fulfillment", "inventory-reservation", "compliance-gateway", "withdrawal-queue", "agent-treasury-guard"]:
        prepare_library(cli, template, site)
    return site

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--cli", required=True, type=Path)
    parser.add_argument("--contract", required=True, type=Path)
    parser.add_argument("--out", required=True, type=Path)
    parser.add_argument("--scratch", type=Path, help="Parent for temporary build files; defaults to the system temporary directory. Must be outside the source tree.")
    args = parser.parse_args()
    node = subprocess.check_output(["node", "--version"], text=True).strip()
    if not node.startswith("v22."):
        raise ValueError("AppRequiresNode22")
    contract = args.contract.resolve()
    if contract.is_file():
        if contract.name != "project.zeno":
            raise ValueError("AppContractMustBeProject")
        contract = contract.parent
    out = args.out.resolve()
    if out.exists() and (not out.is_dir() or any(out.iterdir())):
        raise ValueError("AppOutputMustBeEmpty")
    # Inspect the same standalone model the UI supports before writing the target.
    policy = json.loads((contract / "v2/policy.json").read_text())
    if policy.get("adoptions") or policy.get("evolutions"):
        raise ValueError("AppRequiresStandaloneContract")
    if not (contract / "ui.json").is_file():
        raise ValueError("UiMissingMetadata:ui.json")
    outside_source(out)
    scratch = temporary_parent(args.scratch)
    scratch.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="app-build-", dir=scratch) as temporary:
        work = Path(temporary)
        reviewed = contract_review(args.cli, contract, work)
        if policy.get("schema") == "zeno-fcis/template-declarative-policy/4":
            raise ValueError("AppNumericDurableUnsupported: the native durable server adapter for U128 and numeric contracts is unavailable")
        build(args, contract, out, policy, reviewed, work)

def build(args, contract, out, policy, reviewed, work):
    target = outside_source(Path(os.environ.get("CARGO_TARGET_DIR", work / "target")))
    site = prepare_workspace(args.cli, work)
    environment = dict(os.environ, CARGO_TARGET_DIR=str(target))
    cargo = ("cargo", "+1.97.1", "build", "--locked", "--offline", "--release", "--manifest-path", site / "Cargo.toml", "-p", "zeno-fcis-site-factory")
    run(*cargo, "--bin", "ui_contract", env=environment)
    registry = Path(os.environ.get("CARGO_HOME", Path.home() / ".cargo")) / "registry/src"
    remaps = [f"--remap-path-prefix={ROOT}=/zeno-fcis", f"--remap-path-prefix={work}=/zeno-fcis", f"--remap-path-prefix={target}=/target", f"--remap-path-prefix={registry}=/registry"]
    wasm_environment = dict(environment, CARGO_ENCODED_RUSTFLAGS="\x1f".join(remaps))
    run(*cargo, "--lib", "--target", "wasm32-unknown-unknown", env=wasm_environment)
    # Use the existing checked transformation: the raw linker export includes memory.
    sys.path.insert(0, str(ROOT / "site"))
    from wasm import private_module
    # Every law and mounted-runtime check completes before scaffold emission.
    runtime = work / "core.wasm"
    runtime.write_bytes(private_module((target / "wasm32-unknown-unknown/release/zeno_fcis_site_factory.wasm").read_bytes()))
    build_ui("python3", "-B", ROOT / "tools/zenoui.py", contract, "--out", work / "www", "--evidence", work / "checks",
        "--oracle", target / "release/ui_contract", "--runtime", runtime, env=environment)
    run(args.cli.resolve(), "new", out, "--contract", contract, "--source", ROOT)
    verify_review_subject(reviewed, out)
    manifest = (out / "Cargo.toml").read_text()
    manifest = manifest.replace('default = ["sqlite"]', 'default = ["app"]\n' +
        'shell-effects = ["sqlite"]\napp = ["shell-effects", "dep:zeno-fcis-ingress", "dep:serde_json", "dep:tokio"]')
    manifest = manifest.replace('[dependencies]', '[dependencies]\n' +
        'zeno-fcis-ingress = { version = "=2.6.0", optional = true }\n' +
        'serde_json = { version = "=1.0.151", optional = true }\n' +
        'tokio = { version = "=1.53.2", default-features = false, features = ["rt", "net", "time"], optional = true }')
    manifest = manifest.replace('publish = false', 'publish = false\ndefault-run = "app-server"', 1)
    manifest += '\nzeno-fcis-ingress = { path = ' + json.dumps(str(ROOT / "crates/zeno-fcis-ingress")) + ' }\n'
    manifest += '\n[[bin]]\nname = "app-server"\npath = "src/app_main.rs"\nrequired-features = ["app"]\n'
    (out / "Cargo.toml").write_text(manifest)
    library = out / "src/lib.rs"
    library.write_text(library.read_text() + '\n/// Authenticated durable application shell.\n#[cfg(feature = "app")]\npub mod app;\n')
    session = out / "src/session.rs"
    session.write_text(session.read_text().replace("fn root_numbers(schema:", "pub(crate) fn root_numbers(schema:"))
    shutil.copyfile(ROOT / "crates/zeno-fcis-cli/app-template/app.rs", out / "src/app.rs")
    (out / "src/app_main.rs").write_text('fn main() { if let Err(error) = application::app::run() { eprintln!("application stopped: {error}; check configuration and retained storage"); std::process::exit(1); } }\n')
    shutil.copytree(work / "www", out / "www")
    shutil.copytree(work / "checks", out / "app-checks")
    (out / "app-checks/contract-review.json").write_text(json.dumps(reviewed, indent=2) + "\n")
    for name in ["durable.js", "durable-bootstrap.js"]:
        shutil.copyfile(ROOT / "site/public/zenoui" / name, out / "www/zenoui" / name)
    html = out / "www/index.html"
    html.write_text(html.read_text().replace('zenoui/app-bootstrap.js', 'zenoui/durable-bootstrap.js'))
    # Fixed reviewed context is shell configuration, never selected by a request.
    bundle = json.loads((out / "www/app.json").read_text())
    # A declared server clock is set by the server, never configured.
    context = [f["initial"] for f in bundle["description"]["fields"] if f["source"] == "context" and not f.get("clock")]
    (out / "context.json").write_text(json.dumps(context) + "\n")
    shutil.copyfile(ROOT / "crates/zeno-fcis-cli/app-template/journey.rs", out / "tests/app_journey.rs")
    fixture = ROOT / "test-data/app-journeys" / (policy["template"] + ".json")
    if fixture.is_file():
        shutil.copyfile(fixture, out / "app-checks/journey.json")
    else:
        samples = json.loads((out / "app-checks/samples.json").read_text())
        fields = bundle["description"]["fields"]
        sample = next((s for s in samples if s["reply"]["class"] == "Accept" and all(s["input"][i] == f["initial"] for i,f in enumerate(fields) if f["source"] == "state")), None)
        if sample is None:
            raise ValueError("AppNeedsGenesisJourney")
        step = {"class":"Accept", "command":[sample["input"][i] for i,f in enumerate(fields) if f["source"] == "command"], "context":[sample["input"][i] for i,f in enumerate(fields) if f["source"] == "context" and not f.get("clock")]}
        for i, field in enumerate(fields):
            if field["source"] == "context" and field.get("clock"):
                step["clock"] = sample["input"][i]
        (out / "app-checks/journey.json").write_text(json.dumps({"steps":[step],"deliveries":None}))
    # Reconcile only the copied lock, admitting no external identity outside it.
    admitted = (ROOT / "Cargo.lock").read_text()
    (out / "Cargo.lock").write_text(admitted)
    run("cargo", "+1.97.1", "metadata", "--offline", "--format-version", "1", cwd=out, stdout=subprocess.DEVNULL)
    if not external((out / "Cargo.lock").read_text()).issubset(external(admitted)):
        raise ValueError("AppLockOutsideApprovedSet")
    assets = hashes(out / "www")
    # Compile in only the closed asset list; no runtime path traversal or file replacement.
    records = []
    types = {".js":"text/javascript; charset=utf-8", ".css":"text/css; charset=utf-8", ".json":"application/json", ".html":"text/html; charset=utf-8", ".wasm":"application/wasm"}
    for name in assets:
        records.append(f'Asset::new({json.dumps("/"+name)}, {json.dumps(types[Path(name).suffix])}, include_bytes!({json.dumps("../www/"+name)}))?')
    records.append('Asset::new("/", "text/html; charset=utf-8", include_bytes!("../www/index.html"))?')
    (out / "src/app_assets.rs").write_text('vec![\n' + ',\n'.join(records) + '\n]\n')
    with (out / "README.md").open("a") as guide:
        guide.write("\n## Run the durable application\n\nReview context.json for the configured principal. Create a private file containing 64 lowercase hexadecimal credential digits. Set ZENO_APP_SECRET_FILE to that file and run `cargo +1.97.1 run --locked --offline --bin app-server`. It serves http://127.0.0.1:8080. Sign in through the credential form; commands appear only after an authenticated snapshot. The credential stays in private transport memory, never in assets, URLs or browser storage. Reload requires sign-in again. Every state, preview and command request carries the credential; the server binds its principal. ZENO_APP_CONTEXT, ZENO_APP_STORE, ZENO_APP_RECEIVER and ZENO_APP_ADDRESS optionally select the context file, database, file receiver and loopback address. The UI retains exact requests without credentials for original-key recovery; clear a saved request only after resolving it. The local file receiver deduplicates delivery IDs. It keeps each payload as `<delivery ID>` and, beside it, `<delivery ID>.json`, an envelope with the commit, lane, ordinal and destination; `deliver --to FILE` also reports the channel. ZENO_APP_CONNECTOR instead names a `zeno-fcis/app-connector/1` file whose program, run directly with no shell, receives each delivery on standard input; exit 0 acknowledges it, and anything else leaves it pending, so delivery is at least once and the delivery ID is the idempotency key. Delivery elsewhere needs a reviewed transport.\n\nTo serve several parties from one server, set ZENO_APP_PARTIES to a `zeno-fcis/app-parties/1` file instead of ZENO_APP_SECRET_FILE, ZENO_APP_PRINCIPAL and ZENO_APP_CONTEXT. Each party has a name, its own private credential file, its principal and its context file; the credential a request presents selects that party's principal and context. `docs/BUILD_AN_APP.md` in the ZenoFCIS source tree describes the file and the HTTP API.\n\napp-server writes the same submission journal as `submit`, so `history` reads its store: each new request's line is on disk before its commit and is removed when the commit does not happen. A store that already has commits but no journal, such as one an earlier app-server wrote, gets none; `state` and `--audit` still read it.\n\nWhen `v2/policy.json` declares a `clock`, the server sets that context field at admission, in days or minutes since the epoch date, never earlier than the last committed time; context files omit it. A time outside the field's declared range is refused (503, `Clock`). ZENO_APP_CLOCK_FILE names a file holding the current time in those units: a test and operator tool, not a security boundary.\n\nThe explicit Python build checks all eight ZenoUI laws, uses the copied approved Cargo lock, and records source and asset digests in app-manifest.json. This manifest is build provenance, not authorization.\n")
    receipt = {"format":"zeno-fcis/app-manifest/1", "authority":"none", "ui":bundle["contract"], "assets":assets,
               "contract_review":{"mode":reviewed.get("mode", "exhaustive"), "evidence":reviewed.get("evidence", "native exhaustive review"), "sha256":hashlib.sha256((out / "app-checks/contract-review.json").read_bytes()).hexdigest()},
               "files":{k:v for k,v in hashes(out).items()},
               "generator":hashes(ROOT / "crates/zeno-fcis-cli/app-template"),
               "cli":hashlib.sha256(args.cli.resolve().read_bytes()).hexdigest(),
               "oracle":hashlib.sha256((target / "release/ui_contract").read_bytes()).hexdigest(),
               "generator_sources":{str(p.relative_to(ROOT)):hashlib.sha256(p.read_bytes()).hexdigest() for directory in ["site/factory/src", "site/common/src", "site/public/zenoui", "site/zenoui"] for p in sorted((ROOT/directory).rglob("*")) if p.is_file()},
               "tools":{name:hashlib.sha256((ROOT/name).read_bytes()).hexdigest() for name in ["tools/app_factory.py", "tools/zenoui.py", "site/wasm.py"]}}
    (out / "app-manifest.json").write_text(json.dumps(receipt, indent=2) + "\n")
    print("Application ready. Review context.json, then start app-server as documented in README.md.")

def entrypoint():
    from commons_hook import failure, report
    subject = Path(sys.argv[1]).name if len(sys.argv) > 1 else "unnamed"
    try:
        main()
    except (ValueError, OSError, subprocess.CalledProcessError) as error:
        report("app-factory", "failed", subject, failure(error))
        print(f"app build refused: {refusal(error)}", file=sys.stderr)
        return 1
    except BaseException as error:
        report("app-factory", "failed", subject, failure(error))
        raise
    report("app-factory", "built", subject, {})
    return 0

if __name__ == "__main__":
    sys.exit(entrypoint())
