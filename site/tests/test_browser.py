#!/usr/bin/env python3
"""The browser capture must wait for asynchronous results and refuse a pending page."""
import argparse
import json
from pathlib import Path
import tempfile
import threading
import time
import unittest
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

from deploy_check import DeployCheckError, dump_dom, element_text, find_chrome


class BrowserCompletionTests(unittest.TestCase):
    requested_chrome = None

    @classmethod
    def setUpClass(cls):
        cls.chrome = find_chrome(cls.requested_chrome)
        cls.requests = []

        class Handler(BaseHTTPRequestHandler):
            def do_GET(self):
                cls.requests.append(self.path)
                if self.path == '/reply':
                    time.sleep(0.5)
                    body = b'{"done":true}'
                else:
                    body = b'<!doctype html><pre id="results">pending</pre>'
                    if self.path == '/delayed':
                        body += (b'<script>fetch("/reply").then(r=>r.text()).then(text=>'
                                 b'document.getElementById("results").textContent=text)</script>')
                self.send_response(200)
                self.send_header('Content-Type', 'text/html; charset=utf-8')
                self.send_header('Content-Length', str(len(body)))
                self.end_headers()
                self.wfile.write(body)

            def log_message(self, *args):
                pass

        cls.server = ThreadingHTTPServer(('127.0.0.1', 0), Handler)
        cls.thread = threading.Thread(target=cls.server.serve_forever, daemon=True)
        cls.thread.start()
        cls.base = f'http://127.0.0.1:{cls.server.server_port}'

    @classmethod
    def tearDownClass(cls):
        cls.server.shutdown()
        cls.server.server_close()
        cls.thread.join()

    def test_waits_for_asynchronous_completion(self):
        with tempfile.TemporaryDirectory(prefix='zeno-browser-ready-') as profile:
            dom = dump_dom(self.chrome, Path(profile), self.base + '/delayed', timeout_ms=15000)
        self.assertEqual(json.loads(element_text(dom, 'results')), {'done': True})
        self.assertIn('/reply', self.requests)

    def test_pending_page_fails_at_wall_clock_deadline(self):
        with tempfile.TemporaryDirectory(prefix='zeno-browser-pending-') as profile:
            with self.assertRaisesRegex(DeployCheckError, 'page not ready|timed out waiting for'):
                dump_dom(self.chrome, Path(profile), self.base + '/pending', timeout_ms=5000)
        self.assertIn('/pending', self.requests)


if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('--chrome')
    options, arguments = parser.parse_known_args()
    BrowserCompletionTests.requested_chrome = options.chrome
    unittest.main(argv=[__file__, *arguments])
