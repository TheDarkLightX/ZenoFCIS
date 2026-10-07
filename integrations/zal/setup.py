"""Project-local MCP and skill setup. No provider call or permission changes."""

import argparse
import hashlib
import json
from pathlib import Path
import sys
import tempfile
import tomllib
import os

from behavior import MAX_SOURCE, Refusal
from shared import SharedWorkspace


ROOT = Path(__file__).resolve().parents[2]
STATE = Path(".zeno-fcis/zal")
SKILL = Path("skills/zal-authoring/SKILL.md")
TOOLS = ["zal_read", "zal_check", "zal_help", "zal_explain",
         "zal_propose", "zal_compare", "zal_trace"]


def safe_path(project, relative):
    """Keep managed files inside the chosen project, refusing local symlinks."""
    current = project
    for part in relative.parts:
        current = current / part
        if current.is_symlink():
            raise Refusal(f"Setup refuses a symlink at {current}")
        if current.exists() and current != project / relative and not current.is_dir():
            raise Refusal(f"Setup needs a directory at {current}")
    if current.exists() and not current.is_file():
        raise Refusal(f"Setup needs a regular file at {current}")
    return current


def read_optional(path):
    if not path.exists():
        return b""
    if path.stat().st_size > 1024 * 1024:
        raise Refusal(f"Configuration exceeds 1 MiB: {path}")
    return path.read_bytes()


def codex_config(before, entry):
    document = tomllib.loads(before.decode("utf-8"))
    servers = document.get("mcp_servers", {})
    if not isinstance(servers, dict):
        raise Refusal("Codex mcp_servers must be a table")
    if "zal" in servers:
        if servers["zal"] != entry:
            raise Refusal("An existing Codex zal registration differs; preserve it and choose its source explicitly")
        return before
    # Parse the finished document as well: an existing inline table cannot be
    # extended this way, and must refuse before any setup writes occur.
    lines = ["", "[mcp_servers.zal]"]
    for name, value in entry.items():
        lines.append(f"{name} = {json.dumps(value, ensure_ascii=False)}")
    candidate = before + ("\n" + "\n".join(lines) + "\n").encode()
    expected = {**document, "mcp_servers": {**servers, "zal": entry}}
    if tomllib.loads(candidate.decode("utf-8")) != expected:
        raise Refusal("Codex setup would change another configuration value")
    return candidate


def claude_config(before, entry):
    document = json.loads(before) if before else {}
    if not isinstance(document, dict) or not isinstance(document.get("mcpServers", {}), dict):
        raise Refusal("Claude MCP configuration must contain an object of servers")
    servers = document.get("mcpServers", {})
    if "zal" in servers:
        if servers["zal"] != entry:
            raise Refusal("An existing Claude zal registration differs; preserve it and choose its source explicitly")
        return before
    return (json.dumps({**document, "mcpServers": {**servers, "zal": entry}}, indent=2) + "\n").encode()


def plan(project, harnesses, source_root=ROOT, python=sys.executable):
    project = Path(project).resolve(strict=True)
    source_root = Path(source_root).resolve(strict=True)
    if not project.is_dir() or not harnesses or not set(harnesses) <= {"codex", "claude"}:
        raise Refusal("Choose an existing project and Codex, Claude, or both")
    server = source_root / "integrations/zal/mcp.py"
    skill = source_root / SKILL
    if not server.is_file() or not skill.is_file():
        raise Refusal("The ZAL source must contain its MCP server and authoring skill")
    workspace = safe_path(project, STATE / "workspace.json")
    if workspace.exists():
        SharedWorkspace(workspace).load()  # Validate without changing it.
    entry = {"command": str(Path(python).resolve()),
             "args": [str(server), "--workspace", str(workspace)]}
    writes, expected = {}, {}
    def prepare(key, transform):
        path = safe_path(project, key)
        before = read_optional(path) if path.exists() else None
        expected[key] = before
        writes[key] = transform(before or b"")
    if "codex" in harnesses:
        key = Path(".codex/config.toml")
        prepare(key, lambda before: codex_config(before, {**entry, "enabled_tools": TOOLS}))
    if "claude" in harnesses:
        key = Path(".mcp.json")
        prepare(key, lambda before: claude_config(before, {"type": "stdio", **entry}))
    for harness in harnesses:
        key = Path(".agents" if harness == "codex" else ".claude") / "skills/zal-authoring/SKILL.md"
        path = safe_path(project, key)
        data = skill.read_bytes()
        if path.exists() and path.read_bytes() != data:
            raise Refusal(f"An existing authoring skill differs; preserve {path}")
        prepare(key, lambda before: data)
    record = {"schema": "zal/harness-setup/1", "project": str(project),
              "source": str(source_root), "workspace": str(workspace),
              "harnesses": sorted(set(harnesses)), "tools": TOOLS,
              "review_command": [entry["command"], str(server.with_name("terminal.py")),
                                 "review", "--workspace", str(workspace)],
              "export_command": [entry["command"], str(server.with_name("workspace.py")),
                                 "export", "--workspace", str(workspace)],
              "authority": "none", "approval": "cooperative local review; not authenticated human approval"}
    prepare(STATE / "install.json", lambda before: (json.dumps(record, indent=2) + "\n").encode())
    prepare(STATE / ".gitignore", lambda before: b"*\n")
    for relative in writes:
        safe_path(project, relative)
    safe_path(project, STATE / "workspace.json.lock")
    return project, writes, record, expected


def atomic_write(path, data):
    path.parent.mkdir(mode=0o700, parents=True, exist_ok=True)
    descriptor, name = tempfile.mkstemp(prefix=".zal-setup-", dir=path.parent)
    temporary = Path(name)
    try:
        with os.fdopen(descriptor, "wb") as output:
            output.write(data)
            output.flush()
            os.fsync(output.fileno())
        os.replace(temporary, path)
    finally:
        temporary.unlink(missing_ok=True)


def configure(project, harnesses, source_root=ROOT, model=None, apply=False):
    project, writes, record, expected = plan(project, harnesses, source_root)
    seed = Path(model) if model else Path(record["source"]) / "integrations/zal/examples/order.zal"
    with seed.open(encoding="ascii") as stream:
        source = stream.read(MAX_SOURCE + 1)
    from behavior import parse
    parse(source)  # Refuse a malformed seed before writing configuration.
    record = {**record, "status": "planned", "files": [str(p) for p in writes]}
    if not apply:
        return record
    # Refuse detected concurrent edits and redirected backups before any writes.
    backups = {}
    for relative, data in writes.items():
        path = safe_path(project, relative)
        before = read_optional(path) if path.exists() else None
        if before != expected[relative]:
            raise Refusal(f"Configuration changed during setup; retry after inspecting {path}")
        if before is not None and before != data:
            digest = hashlib.sha256(before).hexdigest()
            backup = STATE / "backups" / (hashlib.sha256(str(relative).encode()).hexdigest()[:16] + "-" + digest)
            backup_path = safe_path(project, backup)
            if backup_path.exists() and backup_path.read_bytes() != before:
                raise Refusal("Existing setup backup differs; preserve it and inspect it")
            backups[relative] = backup
    state = project / STATE
    state.mkdir(mode=0o700, parents=True, exist_ok=True)
    workspace = Path(record["workspace"])
    workspace_created = not workspace.exists()
    if workspace_created:
        SharedWorkspace(workspace).initialize(source)
    for relative, data in writes.items():
        path = safe_path(project, relative)
        before = read_optional(path) if path.exists() else None
        if before != expected[relative]:
            raise Refusal(f"Configuration changed during setup; retry after inspecting {path}")
        if before == data:
            continue
        if path.exists():
            atomic_write(safe_path(project, backups[relative]), before)
        atomic_write(path, data)
    return {**record, "status": "configured", "workspace_created": workspace_created,
            "workspace_preserved": not workspace_created}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--project", type=Path, required=True)
    parser.add_argument("--harness", choices=["codex", "claude", "both"], default="both")
    parser.add_argument("--model", type=Path, help="Symbolic seed for a new workspace; never replaces existing state")
    parser.add_argument("--apply", action="store_true", help="Write the inspected project-local plan")
    options = parser.parse_args()
    try:
        harnesses = ["codex", "claude"] if options.harness == "both" else [options.harness]
        print(json.dumps(configure(options.project, harnesses, model=options.model, apply=options.apply), indent=2))
        return 0
    except (ValueError, OSError) as error:
        print(json.dumps({"status": "refused", "diagnostic": str(error), "authority": "none"}), file=sys.stderr)
        return 2


if __name__ == "__main__":
    raise SystemExit(main())
