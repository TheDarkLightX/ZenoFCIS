"""Offline hostile-input tests for the public download checker.

Every HTTP call and process is blocked unless a test supplies a simulated result.
The key below is the established public release key, never a private key.
"""
from pathlib import Path
from unittest.mock import patch
from urllib.request import Request
from email.message import Message
import copy
import hashlib
import importlib.util
import io
import json
import subprocess
import tempfile
import types
import unittest

spec = importlib.util.spec_from_file_location('public_check', Path(__file__).with_name('check_public_release.py'))
m = importlib.util.module_from_spec(spec)
spec.loader.exec_module(m)
REAL_RESPONSE = m.response
PUBLIC_KEY = 'ssh-rsa AAAAB3NzaC1yc2EAAAADAQABAAAEAQC88/Bf5/96423UhtoSiGA3+0oWjUgoc7Lx0K8gMbTJekTCIRfstfuYbDr0nKilAE6dcnDia2rdmP3s9kihQi0z5WISy/z2EClrSEwMiKr/3b/oVk9bXwAsGtyD/dLJsGyZWewyP9Hzjs4vBkIQFCCYFNXD9bAo/udLB44xU/MfWDcp7NDsitXo7r6zFdkttQPSp+OILtGucir7JBzEu/uUuPNO6N6MyfzINw83tBfWGcTuUafObQEBmuzFmRcSpB+6KxMRzbQbmV7hFzpQot2Aqp/J88wZK+FwRk4dI8X/fUvSLXzYirgDIbJ0YI6Ve6yVDDvxcqhy4C/IhZ5yjMQbM0uP4/L8Y2AfQeA23xui5vBT7Sy7Ml0xA6X8wjUJcFCt9mYtHlQMzAlZP9cHVt5hlHQgcFnAxDf3x7TLyw7Ly4+1Db+q7cUo6WUCrtilZGU6IiJfftA5u+EMXy5O7hPh6jqSPKmWHhnxjatmRaUPvZK+qFNUfc+TGQWJ6+JLhqxBIDMmjDJPzJWhLcNIRq+E2EwQZkzyKFvya1RzXe/fnOXkpMl1CRtjJp3oK/HFg9U4tkEVP6IfmFkaExwdOH+aYka5hpC/7+2FK4HfuWyzPjUuJCJ6/wKcHKXF/B5V5eh7a8l/e7zGyE+36mICGkaCWwIuI/ADQYtv4V9noXXbrCxv2uqdk5EKxn+oRoJ8tHFfwd6ALNeuk4/i78WP00cAjggwwNpKyHtiNPTSJV9numa1RXGIVtAgrWYm4O7/y/2Fcg5qvGyF06hBciO4PQqqLFHcaaI1yrDeI1TbfyTHZBUfWPssysEr6sqUrkwcj2CLQ0PDgrvw22j5zlFYZAqOI/A0TnQ9vs7Ky9oaQHHQn7qVa92TuryCLr3cWa7uCSGpf0zI6fSZ9yXgvF1FGEgfxZiKjrIlG7t5ANcl5h3jgOdjHHKvry6gh3RrrZkMmJoUgVXHPqj5V4YIeDryb0nvMgaLL/8zWW9SnH2YecWp3AP4vE5aP7rOHUP3V8Crly5c6XtS/BWRsii1gWGqmwDoA+tJWNrYwde20pWRKuits9JPfqbcMcr5luevqNs5nhG1vgO/3y13q4B9LoiyaFtbWQfNE1jRL+wFRwsWe/3CIDpclDKZtQkU7ILjloKH+U7EkhSBo2fHB6rQYQaWOokr23t8IOHlU7CSPALSYSLgHzFVndE/Tpu2jHbDw3Bo+G68vS2YMZxIDX+1WG6EyI0FbcYpPqqocKdGUHdmZ8Kljh3jMmRDVSp5VxOgdEirjxc7yVwgRnsF1vsu7XpAYBVoUKvNk6+778M5sn2OyQXUWvsqzrazpx6pvKTKMeVHNPhnLLpgQR8Pllx3egD7dZYj'
SOURCE = '3b2224d5a081e7ab3e2e282ad6704bcb177d25ac'
TAG_OBJECT = 'f3f833f9eff158cae925405fc7f573311fc234b4'


class Response(io.BytesIO):
    status = 200
    def __init__(self, data, headers=None):
        super().__init__(data)
        self.headers = headers or {}
        self.read_sizes = []
    def read(self, size=-1):
        self.read_sizes.append(size)
        return super().read(size)


class PublicReleaseTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory(prefix='zenofcis-public-check-test-')
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        self.path = self.root / 'expected.json'
        self.work = self.root / 'checked'
        self.network = self.enterContext(patch.object(m, 'response', side_effect=AssertionError('Network forbidden in tests')))
        self.process = self.enterContext(patch.object(m.subprocess, 'run', side_effect=AssertionError('Unexpected subprocess')))
        names = [m.KEY_FILE, 'SOURCE-MANIFEST.json', 'RELEASE-VERIFICATION.json', 'SHA256SUMS',
                 'SHA256SUMS.sig', 'RELEASE-SHA256SUMS', 'RELEASE-SHA256SUMS.sig']
        self.contents = {name: ('sample ' + name).encode() for name in names + [f'asset-{i:02}.bin' for i in range(46)]}
        self.contents[m.KEY_FILE] = (PUBLIC_KEY + '\n').encode()
        self.contents['SOURCE-MANIFEST.json'] = m.encode({'format': 'zeno-fcis/source-manifest/1', 'commit': SOURCE, 'clean': True})
        self.contents['RELEASE-VERIFICATION.json'] = m.encode({'schema': 'zeno-fcis/owner-release-verification/2',
            'source_commit': SOURCE, 'tag_object': TAG_OBJECT, 'tag': m.TAG, 'version': m.VERSION,
            'status': 'qualified-for-stable-github-release'})
        self.contents['SHA256SUMS'] = ('a' * 64 + '  packages/example-1.1.0.crate\n').encode()
        self.refresh()

    def refresh(self):
        self.contents['RELEASE-SHA256SUMS'] = ''.join(hashlib.sha256(data).hexdigest() + '  ' + name + '\n'
            for name, data in sorted(self.contents.items()) if name not in ('RELEASE-SHA256SUMS', 'RELEASE-SHA256SUMS.sig')).encode()
        self.input = {'schema': 'zeno-fcis/public-release-download-input/1', 'repository': m.REPOSITORY,
                      'version': m.VERSION, 'tag': m.TAG, 'source_commit': SOURCE, 'tag_object': TAG_OBJECT,
                      'release_id': 123, 'signature_namespace': m.NAMESPACE, 'signing_key_fingerprint': m.FINGERPRINT,
                      'assets': [{'name': name, 'size': len(data), 'sha256': hashlib.sha256(data).hexdigest()}
                                 for name, data in sorted(self.contents.items())]}
        self.path.write_bytes(m.encode(self.input))
        self.pins = m.load_expected(self.path)

    def assets(self):
        return [{'id': i, 'name': name, 'state': 'uploaded', 'size': entry['size'],
                 'digest': 'sha256:' + entry['sha256'], 'url': m.API + '/releases/assets/' + str(i),
                 'browser_download_url': m.REPOSITORY + '/releases/download/' + m.TAG + '/' + name,
                 'uploader': {'unneeded_metadata': str(self.root)}}
                for i, (name, entry) in enumerate(sorted(self.pins['assets'].items()), 1)]

    def release(self):
        return {'id': 123, 'tag_name': m.TAG, 'draft': False, 'prerelease': False, 'immutable': False,
                'published_at': '2026-09-14T12:00:00Z', 'url': m.API + '/releases/123',
                'html_url': m.REPOSITORY + '/releases/tag/' + m.TAG, 'assets_url': m.API + '/releases/123/assets',
                'assets': self.assets(), 'author': {'unneeded_metadata': str(self.root)}}

    def tag(self):
        return ({'ref': 'refs/tags/' + m.TAG, 'object': {'type': 'tag', 'sha': TAG_OBJECT, 'url': m.API + '/git/tags/' + TAG_OBJECT}},
                {'sha': TAG_OBJECT, 'tag': m.TAG, 'object': {'type': 'commit', 'sha': SOURCE, 'url': m.API + '/git/commits/' + SOURCE}})

    def successful_calls(self, snapshots=None):
        snapshot = m.validate_release(self.release(), self.pins)
        self.enterContext(patch.object(m, 'remote_snapshot', side_effect=snapshots or [snapshot, snapshot]))
        download = self.enterContext(patch.object(m, 'response', side_effect=lambda url, pins: Response(self.contents[url.rsplit('/', 1)[1]])))
        process = self.enterContext(patch.object(m.subprocess, 'run', return_value=types.SimpleNamespace(returncode=0, stdout=b'verified', stderr=b'')))
        return download, process

    def test_closed_input_and_frozen_pins_reject_before_network_or_output(self):
        for field, value in [('unknown', 'value'), ('source_commit', 'UNFROZEN'), ('tag_object', 'x' * 40),
                             ('release_id', 'UNFROZEN'), ('release_id', True), ('release_id', 0),
                             ('repository', 'https://example.invalid/repo'), ('signature_namespace', 'file'),
                             ('signing_key_fingerprint', 'SHA256:wrong'), ('tag', 'v1.0.0')]:
            data = copy.deepcopy(self.input)
            data[field] = value
            self.path.write_bytes(m.encode(data))
            with self.assertRaises(ValueError):
                m.check(self.path, self.work)
        self.assertFalse(self.work.exists())
        self.network.assert_not_called()
        self.process.assert_not_called()

    def test_input_requires_exact_unique_safe_asset_names_sizes_and_hashes(self):
        for field, value in [('name', '../escape'), ('name', 'asset/name'), ('name', self.input['assets'][1]['name']),
                             ('size', True), ('size', -1), ('sha256', 'UNFROZEN'), ('unknown', 'value')]:
            data = copy.deepcopy(self.input)
            data['assets'][0][field] = value
            self.path.write_bytes(m.encode(data))
            with self.assertRaises(ValueError):
                m.load_expected(self.path)
        self.input['assets'].pop()
        self.path.write_bytes(m.encode(self.input))
        with self.assertRaisesRegex(ValueError, 'Exactly 53'):
            m.load_expected(self.path)

    def test_input_cap_duplicate_json_and_symlink_reject(self):
        for data in [b'x' * (m.MAX_INPUT_BYTES + 1), b'{"schema":1,"schema":2}']:
            self.path.write_bytes(data)
            with self.assertRaises(ValueError):
                m.load_expected(self.path)
        self.path.unlink()
        self.path.symlink_to(self.root / 'missing')
        with self.assertRaises(ValueError):
            m.load_expected(self.path)

    def test_remote_inventory_rejects_duplicate_unknown_wrong_bytes_and_urls(self):
        original = self.assets()
        for field, value in [('name', 'unknown.bin'), ('size', original[0]['size'] + 1), ('digest', 'sha256:' + '0' * 64),
                             ('state', 'new'), ('id', True), ('browser_download_url', 'https://example.invalid/asset')]:
            changed = copy.deepcopy(original)
            changed[0][field] = value
            with self.assertRaises(ValueError):
                m.validate_assets(changed, self.pins['assets'])
        for values in [original[:-1], [original[0], *original[:-1]]]:
            with self.assertRaises(ValueError):
                m.validate_assets(values, self.pins['assets'])
        changed = copy.deepcopy(original)
        changed[0]['id'] = changed[1]['id']
        with self.assertRaises(ValueError):
            m.validate_assets(changed, self.pins['assets'])

    def test_absent_api_digest_is_reported_and_supplied_unknown_digest_rejects(self):
        items = self.assets()
        items[0].pop('digest')
        checked = m.validate_assets(items, self.pins['assets'])
        self.assertIsNone(checked[items[0]['name']]['github_digest'])
        items[1]['digest'] = 'sha512:' + 'a' * 128
        with self.assertRaises(ValueError):
            m.validate_assets(items, self.pins['assets'])

    def test_release_is_stable_exact_id_and_tag_without_exporting_api_actors(self):
        original = self.release()
        checked = m.validate_release(original, self.pins)
        self.assertFalse(checked['github_immutable'])
        self.assertNotIn(str(self.root).encode(), m.encode(checked))
        for field, value in [('id', 124), ('tag_name', 'v1.0.0'), ('draft', True), ('draft', 0),
                             ('prerelease', True), ('published_at', None), ('immutable', 'true')]:
            changed = copy.deepcopy(original)
            changed[field] = value
            with self.assertRaises(ValueError):
                m.validate_release(changed, self.pins)

    def test_remote_ref_is_annotated_and_peels_to_independently_expected_commit(self):
        reference, tag = self.tag()
        m.validate_tag(reference, tag, self.pins)
        for target, field, value in [('ref', 'type', 'commit'), ('ref', 'sha', 'a' * 40),
                                     ('tag', 'type', 'tag'), ('tag', 'sha', 'a' * 40)]:
            ref, obj = copy.deepcopy(reference), copy.deepcopy(tag)
            (ref if target == 'ref' else obj)['object'][field] = value
            with self.assertRaises(ValueError):
                m.validate_tag(ref, obj, self.pins)

    def test_metadata_views_agree_and_use_only_fixed_api_paths(self):
        release, (reference, tag) = self.release(), self.tag()
        values = {m.API + '/releases/123': release, m.API + '/releases/tags/' + m.TAG: release,
                  m.API + '/releases/123/assets?per_page=100': release['assets'],
                  m.API + '/git/ref/tags/' + m.TAG: reference, m.API + '/git/tags/' + TAG_OBJECT: tag}
        with patch.object(m, 'fetch_json', side_effect=lambda url, pins: values[url]) as fetch:
            m.remote_snapshot(self.pins)
            self.assertEqual({call.args[0] for call in fetch.call_args_list}, set(values))
            changed = copy.deepcopy(release)
            changed['assets'][0]['id'] = 999
            changed['assets'][0]['url'] = m.API + '/releases/assets/999'
            values[m.API + '/releases/tags/' + m.TAG] = changed
            with self.assertRaisesRegex(ValueError, 'views disagree'):
                m.remote_snapshot(self.pins)

    def test_json_read_cap_extra_pages_and_duplicate_fields_reject(self):
        for response in [Response(b'x' * (m.MAX_JSON_BYTES + 1)), Response(b'[]', {'Link': '<https://example.invalid>; rel="next"'}),
                         Response(b'{"id":1,"id":2}')]:
            with patch.object(m, 'response', return_value=response):
                with self.assertRaises(ValueError):
                    m.fetch_json(m.API + '/releases/123', self.pins)

    def test_any_nonempty_link_header_rejects_before_reading_json(self):
        duplicate = Message()
        duplicate['Link'] = ''
        duplicate['Link'] = '<https://example.invalid>; rel="next"'
        headers = [{'Link': value} for value in ['<https://example.invalid>; rel="prev next"',
                   '<https://example.invalid>; rel = "next"', '<https://example.invalid>; rel="help"']]
        headers.extend([{'link': 'unexpected'}, duplicate])
        for values in headers:
            stream = Response(b'[]', values)
            with patch.object(m, 'response', return_value=stream):
                with self.assertRaises(ValueError):
                    m.fetch_json(m.API + '/releases/123', self.pins)
            self.assertEqual(stream.read_sizes, [])
        for values in [{}, {'Link': ''}]:
            with patch.object(m, 'response', return_value=Response(b'[]', values)):
                self.assertEqual(m.fetch_json(m.API + '/releases/123', self.pins), [])

    def test_request_adapter_is_anonymous_get_and_rejects_unpinned_urls(self):
        opener = types.SimpleNamespace(open=lambda request, timeout: Response(b'{}'))
        with patch.object(m, 'build_opener', return_value=opener) as build, patch.object(opener, 'open', wraps=opener.open) as opened:
            REAL_RESPONSE(m.API + '/releases/123', self.pins, api=True).close()
            REAL_RESPONSE(m.REPOSITORY + '/releases/download/' + m.TAG + '/asset-00.bin', self.pins).close()
            for call in opened.call_args_list:
                self.assertEqual(call.args[0].get_method(), 'GET')
                self.assertIsNone(call.args[0].data)
                self.assertNotIn('authorization', {name.lower() for name, _ in call.args[0].header_items()})
            before = build.call_count
            for url, api in [(m.API + '/releases/124', True), (m.API + '/releases/123?token=secret', True),
                             (m.REPOSITORY + '/releases/download/' + m.TAG + '/unknown.bin', False),
                             (m.REPOSITORY + '/releases/download/v1.0.0/asset-00.bin', False),
                             (m.REPOSITORY + '/releases/download/' + m.TAG + '/../escape', False),
                             ('https://example.invalid/asset.bin', False)]:
                with self.assertRaises(ValueError):
                    REAL_RESPONSE(url, self.pins, api=api)
            self.assertEqual(build.call_count, before)

    def test_redirects_admit_only_expected_https_github_download_hosts(self):
        handler = m.PublicRedirects()
        request = Request(m.REPOSITORY + '/releases/download/' + m.TAG + '/asset-00.bin')
        for url in ['http://release-assets.githubusercontent.com/a', 'https://example.invalid/a',
                    'https://github.com/login', 'https://user:password@objects.githubusercontent.com/a', 'file:///unrelated']:
            with self.assertRaises(ValueError):
                handler.redirect_request(request, None, 302, '', {}, url)
        redirect = handler.redirect_request(request, None, 302, '', {}, 'https://release-assets.githubusercontent.com/public?temporary=token')
        self.assertEqual(redirect.get_method(), 'GET')
        with self.assertRaises(ValueError):
            handler.redirect_request(Request(m.API + '/releases/123'), None, 302, '', {}, 'https://objects.githubusercontent.com/a')

    def test_stream_bounds_and_hash_compare_reject_corruption_truncation_and_overflow(self):
        expected = {'size': 4, 'sha256': hashlib.sha256(b'good').hexdigest()}
        response, output = Response(b'good', {'Content-Length': '4'}), io.BytesIO()
        m.copy_download(response, output, expected)
        self.assertEqual(output.getvalue(), b'good')
        self.assertTrue(all(0 < count <= m.CHUNK_BYTES for count in response.read_sizes))
        for data, headers in [(b'bad!', {}), (b'goo', {}), (b'goodX', {}), (b'good', {'Content-Length': '5'})]:
            out = io.BytesIO()
            with self.assertRaises(ValueError):
                m.copy_download(Response(data, headers), out, expected)
            self.assertLessEqual(len(out.getvalue()), 4)

    def test_outer_checksum_list_requires_all_51_exact_flat_names_and_hashes(self):
        data = self.contents['RELEASE-SHA256SUMS']
        lines = data.decode().splitlines()
        for changed in ['\n'.join(lines[:-1]), '\n'.join(lines + [lines[0]]),
                        '\n'.join(['0' * 64 + lines[0][64:], *lines[1:]]),
                        '\n'.join([lines[0].split('  ')[0] + '  ../escape', *lines[1:]])]:
            with self.assertRaises(ValueError):
                m.checksum_entries(changed.encode(), self.pins['assets'])
        self.assertEqual(len(m.checksum_entries(data, self.pins['assets'])), 51)

    def test_full_simulation_downloads_all_53_without_any_local_prior_asset_bytes(self):
        self.assertEqual(list(self.root.iterdir()), [self.path])
        download, process = self.successful_calls()
        result = m.check(self.path, self.work)
        self.assertEqual(download.call_count, 53)
        self.assertEqual(process.call_count, 2)
        for call in process.call_args_list:
            self.assertEqual(call.args[0][1:3], ['-Y', 'verify'])
            self.assertIn('zeno-fcis-release', call.args[0])
            self.assertIn('TheDarkLightX', call.args[0])
        self.assertEqual(result['asset_count'], 53)
        self.assertEqual(result['expected_input_sha256'], hashlib.sha256(self.path.read_bytes()).hexdigest())
        self.assertNotIn(str(self.root).encode(), m.encode(result))
        self.assertIn('unsigned', result['qualification'])
        self.assertIn('job metadata', result['qualification'])
        self.assertFalse((self.work / 'downloads/packages').exists())
        for name, data in self.contents.items():
            self.assertEqual((self.work / 'downloads' / name).read_bytes(), data)
        with self.assertRaisesRegex(ValueError, 'Existing or linked'):
            m.check(self.path, self.work)

    def test_failed_download_or_signature_has_no_pass_receipt_and_no_implicit_retry(self):
        self.successful_calls()
        with patch.object(m, 'response', return_value=Response(b'wrong')):
            with self.assertRaises(ValueError):
                m.check(self.path, self.work)
        self.assertFalse((self.work / 'result.json').exists())
        with self.assertRaisesRegex(ValueError, 'Existing or linked'):
            m.check(self.path, self.work)
        with patch.object(m.subprocess, 'run', return_value=types.SimpleNamespace(returncode=1, stdout=b'', stderr=b'bad signature')):
            with self.assertRaisesRegex(ValueError, 'signature verification failed'):
                m.check(self.path, self.root / 'bad-signature')
        self.assertFalse((self.root / 'bad-signature/result.json').exists())

    def test_metadata_change_after_downloads_blocks_pass_receipt(self):
        snapshot = m.validate_release(self.release(), self.pins)
        changed = copy.deepcopy(snapshot)
        changed['assets'][next(iter(changed['assets']))]['asset_id'] += 1000
        self.successful_calls(snapshots=[snapshot, changed])
        with self.assertRaisesRegex(ValueError, 'changed during verification'):
            m.check(self.path, self.work)
        self.assertFalse((self.work / 'result.json').exists())

    def test_matching_byte_pins_do_not_excuse_wrong_embedded_source_binding(self):
        wrong = json.loads(self.contents['SOURCE-MANIFEST.json'])
        wrong['commit'] = 'a' * 40
        self.contents['SOURCE-MANIFEST.json'] = m.encode(wrong)
        self.refresh()
        self.successful_calls()
        with self.assertRaisesRegex(ValueError, 'another source'):
            m.check(self.path, self.work)
        self.assertFalse((self.work / 'result.json').exists())


if __name__ == '__main__':
    unittest.main(verbosity=2)
