import json
from pathlib import Path
import tempfile
import tomllib
import unittest
from unittest.mock import patch

from behavior import Refusal
from shared import SharedWorkspace
from setup import ROOT, STATE, configure, plan
from test_behavior import SOURCE


class SetupTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.project = Path(self.temp.name) / 'project "quoted" café'
        self.project.mkdir()

    def tearDown(self):
        self.temp.cleanup()

    def test_plan_is_read_only_and_both_harnesses_share_private_workspace(self):
        before = list(self.project.iterdir())
        record = configure(self.project, ["codex", "claude"])
        self.assertEqual(record["status"], "planned")
        self.assertEqual(list(self.project.iterdir()), before)
        result = configure(self.project, ["codex", "claude"], apply=True)
        self.assertTrue(result["workspace_created"])
        codex = tomllib.loads((self.project / ".codex/config.toml").read_text())
        claude = json.loads((self.project / ".mcp.json").read_text())
        self.assertEqual(codex["mcp_servers"]["zal"]["args"], claude["mcpServers"]["zal"]["args"])
        self.assertEqual(codex["mcp_servers"]["zal"]["enabled_tools"], result["tools"])
        workspace = Path(result["workspace"])
        self.assertEqual(workspace.stat().st_mode & 0o777, 0o600)
        self.assertEqual(workspace.parent.stat().st_mode & 0o777, 0o700)
        self.assertEqual((workspace.parent / ".gitignore").read_text(), "*\n")
        self.assertEqual((self.project / ".agents/skills/zal-authoring/SKILL.md").read_bytes(),
                         (self.project / ".claude/skills/zal-authoring/SKILL.md").read_bytes())

    def test_preserves_other_servers_and_pending_work_and_is_idempotent(self):
        (self.project / ".codex").mkdir()
        codex_text = b'# User comment\nmodel = "chosen"\n[mcp_servers.other]\ncommand = "other"\n'
        (self.project / ".codex/config.toml").write_bytes(codex_text)
        claude_data = {"custom": {"preserve": True}, "mcpServers": {"other": {"command": "other"}}}
        (self.project / ".mcp.json").write_text(json.dumps(claude_data))
        first = configure(self.project, ["codex", "claude"], apply=True)
        workspace = Path(first["workspace"])
        store = SharedWorkspace(workspace)
        store.use(lambda s: s.propose_human(s.model.render().replace("not_enabled", "not_allowed"),
                                           "symbolic", s.model.revision))
        before = workspace.read_bytes()
        all_files = {str(p): p.read_bytes() for p in self.project.rglob("*") if p.is_file()}
        second = configure(self.project, ["codex", "claude"], apply=True)
        self.assertTrue(second["workspace_preserved"])
        self.assertEqual(workspace.read_bytes(), before)
        self.assertEqual({str(p): p.read_bytes() for p in self.project.rglob("*") if p.is_file()}, all_files)
        codex = (self.project / ".codex/config.toml").read_bytes()
        self.assertTrue(codex.startswith(codex_text))
        parsed = json.loads((self.project / ".mcp.json").read_text())
        self.assertEqual(parsed["custom"], claude_data["custom"])
        self.assertEqual(parsed["mcpServers"]["other"], claude_data["mcpServers"]["other"])
        self.assertEqual(len(list((workspace.parent / "backups").iterdir())), 2)

    def test_differing_registration_or_skill_refuses_before_any_write(self):
        for relative, data in [(".mcp.json", '{"mcpServers":{"zal":{"command":"another"}}}'),
                               (".codex/config.toml", '[mcp_servers.zal]\ncommand = "another"\n'),
                               (".agents/skills/zal-authoring/SKILL.md", "Existing user skill")]:
            with self.subTest(path=relative), tempfile.TemporaryDirectory() as directory:
                project = Path(directory)
                path = project / relative
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_text(data)
                before = {str(p): p.read_bytes() for p in project.rglob("*") if p.is_file()}
                with self.assertRaises(Refusal):
                    configure(project, ["codex", "claude"], apply=True)
                self.assertEqual({str(p): p.read_bytes() for p in project.rglob("*") if p.is_file()}, before)

    def test_symlinked_managed_file_or_backup_directory_refuses(self):
        for relative in [".mcp.json", ".codex", ".zeno-fcis", ".zeno-fcis/zal/workspace.json.lock",
                         ".zeno-fcis/zal/backups"]:
            with self.subTest(path=relative), tempfile.TemporaryDirectory() as directory:
                project = Path(directory) / "project"
                outside = Path(directory) / "outside"
                project.mkdir()
                outside.mkdir()
                target = project / relative
                target.parent.mkdir(parents=True, exist_ok=True)
                target.symlink_to(outside)
                # Force a configuration backup when testing the backup directory.
                (project / ".mcp.json").write_text("{}") if relative.endswith("backups") else None
                with self.assertRaises(Refusal):
                    configure(project, ["codex", "claude"], apply=True)
                self.assertEqual(list(outside.iterdir()), [])
                self.assertFalse((project / STATE / "workspace.json").exists())

    def test_concurrent_configuration_edit_is_not_overwritten(self):
        path = self.project / ".mcp.json"
        path.write_text("{}")
        def changing_plan(*args, **kwargs):
            result = plan(*args, **kwargs)
            path.write_text('{"mcpServers":{"new":{"command":"keep"}}}')
            return result
        with patch("setup.plan", side_effect=changing_plan), self.assertRaisesRegex(Refusal, "changed during"):
            configure(self.project, ["codex", "claude"], apply=True)
        self.assertEqual(json.loads(path.read_text())["mcpServers"]["new"]["command"], "keep")
        self.assertFalse((self.project / STATE).exists())

    def test_bad_seed_refuses_and_setup_never_calls_a_provider(self):
        invalid = Path(self.temp.name) / "invalid.zal"
        invalid.write_text("not a declaration")
        with self.assertRaises(Refusal):
            configure(self.project, ["codex", "claude"], model=invalid, apply=True)
        self.assertEqual(list(self.project.iterdir()), [])
        with patch("socket.socket", side_effect=AssertionError("Network forbidden")), \
                patch("subprocess.Popen", side_effect=AssertionError("Provider/process forbidden")):
            configure(self.project, ["codex", "claude"], source_root=ROOT, apply=True)


if __name__ == "__main__":
    unittest.main()
