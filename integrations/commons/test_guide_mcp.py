"""Exercise the actual standard-library stdio MCP guide tools."""
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

from commons import Commons


@unittest.skipUnless(os.environ.get("ZENO_FCIS_CLI"), "set ZENO_FCIS_CLI")
class GuideMcp(unittest.TestCase):
    def test_three_readonly_tools_over_stdio_leave_event_head_unchanged(self):
        with tempfile.TemporaryDirectory() as temporary:
            store = Commons(temporary, "reader")
            head = store.head()
            store.close()
            requests = [
                {"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {}},
                {"jsonrpc": "2.0", "id": 2, "method": "tools/list"},
                {"jsonrpc": "2.0", "id": 3, "method": "tools/call", "params": {"name": "guide_list"}},
                {"jsonrpc": "2.0", "id": 4, "method": "tools/call", "params": {"name": "guide_show", "arguments": {"name": "check"}}},
                {"jsonrpc": "2.0", "id": 5, "method": "tools/call", "params": {"name": "guide_find", "arguments": {"task": "find modulo bias"}}},
            ]
            result = subprocess.run([sys.executable, "-I", str(Path(__file__).with_name("mcp.py")), "--store", temporary, "--actor", "reader"],
                input="".join(json.dumps(row) + "\n" for row in requests), capture_output=True,
                text=True, timeout=30, check=False)
            self.assertEqual(result.returncode, 0, result.stderr)
            replies = [json.loads(line)["result"] for line in result.stdout.splitlines()]
            exposed = {tool["name"]: tool for tool in replies[1]["tools"] if tool["name"].startswith("guide_")}
            self.assertEqual(set(exposed), {"guide_list", "guide_show", "guide_find"})
            self.assertTrue(all(tool["annotations"]["readOnlyHint"] for tool in exposed.values()))
            self.assertTrue(all(not reply["isError"] for reply in replies[2:]))
            found = json.loads(replies[4]["content"][0]["text"])
            self.assertEqual(found["cards"][0]["name"], "contract-distribution")
            store = Commons(temporary, "reader")
            try:
                self.assertEqual(store.head(), head)
            finally:
                store.close()


if __name__ == "__main__":
    unittest.main()
