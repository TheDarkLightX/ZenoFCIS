#!/usr/bin/env python3
"""Check public release downloads against a separately reviewed fixed inventory.

Only anonymous GETs and public-key SSH verification are performed. No downloaded
code is executed. A new output directory retains bytes and private verification
logs; only result.json is intended for the hosted evidence artifact. Expected
identities and all 53 byte hashes must be frozen before this checker can run.
"""
from pathlib import Path
from datetime import datetime, timezone
from urllib.parse import quote, urlsplit
import argparse
from urllib.request import Request, HTTPRedirectHandler, build_opener
import base64
import hashlib
import json
import re
import stat
import subprocess

VERSION = '1.1.0'
TAG = 'v1.1.0'
REPOSITORY = 'https://github.com/TheDarkLightX/ZenoFCIS'
API = 'https://api.github.com/repos/TheDarkLightX/ZenoFCIS'
KEY_FILE = 'ZenoFCIS-SSH-SIGNING-KEY.pub'
FINGERPRINT = 'SHA256:uPpB0mTOYACtRhOWrMqKcQL6EfNcSOmQReDXyCALaYE'
SIGNER = 'TheDarkLightX'
NAMESPACE = 'zeno-fcis-release'
PAYLOADS = ('SHA256SUMS', 'RELEASE-SHA256SUMS')
MAX_JSON_BYTES = 2 * 1024 * 1024
MAX_INPUT_BYTES = 128 * 1024
CHUNK_BYTES = 1024 * 1024


def require(condition, message):
    if not condition:
        raise ValueError(message)


def regular(path):
    require(not any(parent.is_symlink() for parent in path.parents)
            and stat.S_ISREG(path.lstat().st_mode), 'Linked or nonregular input file')
    return path


def digest(path):
    result = hashlib.sha256()
    with regular(path).open('rb') as stream:
        while block := stream.read(CHUNK_BYTES):
            result.update(block)
    return result.hexdigest()


def pairs(items):
    result = {}
    for key, value in items:
        require(key not in result, 'Duplicate JSON field')
        result[key] = value
    return result


def json_bytes(data):
    return json.loads(data, object_pairs_hook=pairs)


def read_json(path):
    with regular(path).open('rb') as stream:
        data = stream.read(MAX_JSON_BYTES + 1)
    require(len(data) <= MAX_JSON_BYTES, 'Downloaded identity JSON exceeds the fixed byte cap')
    return json_bytes(data)


def encode(value):
    return (json.dumps(value, indent=2, sort_keys=True) + '\n').encode()


def asset_name(name):
    require(isinstance(name, str) and re.fullmatch('[A-Za-z0-9][A-Za-z0-9._-]{0,199}', name),
            'Unsafe or unsupported asset name')
    return name


def load_expected(path):
    with regular(path).open('rb') as stream:
        data = stream.read(MAX_INPUT_BYTES + 1)
    require(len(data) <= MAX_INPUT_BYTES, 'Expected input exceeds the fixed byte cap')
    pins = json_bytes(data)
    fields = {'schema', 'repository', 'version', 'tag', 'source_commit', 'tag_object',
              'release_id', 'signature_namespace', 'signing_key_fingerprint', 'assets'}
    require(type(pins) is dict and set(pins) == fields, 'Expected input fields are not the closed schema')
    require(pins['schema'] == 'zeno-fcis/public-release-download-input/1'
            and pins['repository'] == REPOSITORY and pins['version'] == VERSION and pins['tag'] == TAG
            and pins['signature_namespace'] == NAMESPACE and pins['signing_key_fingerprint'] == FINGERPRINT,
            'Expected release or signature profile differs')
    require(all(isinstance(pins[name], str) and re.fullmatch('[0-9a-f]{40}', pins[name])
                for name in ('source_commit', 'tag_object'))
            and type(pins['release_id']) is int and pins['release_id'] > 0,
            'Freeze source commit, tag object and release ID first')
    require(type(pins['assets']) is list and len(pins['assets']) == 53, 'Exactly 53 expected assets are required')
    assets = {}
    for item in pins['assets']:
        require(type(item) is dict and set(item) == {'name', 'size', 'sha256'}, 'Expected asset fields are not the closed schema')
        name = asset_name(item['name'])
        require(name not in assets and type(item['size']) is int and item['size'] >= 0
                and isinstance(item['sha256'], str) and re.fullmatch('[0-9a-f]{64}', item['sha256']),
                'Duplicate asset, invalid size or unfrozen SHA256')
        assets[name] = {'size': item['size'], 'sha256': item['sha256']}
    mandatory = {KEY_FILE, 'RELEASE-VERIFICATION.json', 'SOURCE-MANIFEST.json',
                 'SHA256SUMS', 'SHA256SUMS.sig', 'RELEASE-SHA256SUMS', 'RELEASE-SHA256SUMS.sig'}
    require(mandatory <= assets.keys() and {name for name in assets if name.endswith('.sig')}
            == {'SHA256SUMS.sig', 'RELEASE-SHA256SUMS.sig'}, 'Required signature or source assets differ')
    return {**pins, 'assets': assets, 'input_sha256': hashlib.sha256(data).hexdigest()}


def validate_downloaded_identity(directory, pins):
    source = read_json(directory / 'SOURCE-MANIFEST.json')
    require(source['format'] == 'zeno-fcis/source-manifest/1'
            and source['commit'] == pins['source_commit'] and source['clean'] is True,
            'Downloaded source manifest belongs to another source')
    release = read_json(directory / 'RELEASE-VERIFICATION.json')
    require(release['schema'] == 'zeno-fcis/owner-release-verification/2'
            and release['source_commit'] == pins['source_commit'] and release['version'] == VERSION
            and release['tag'] == TAG and release['tag_object'] == pins['tag_object']
            and release['status'] == 'qualified-for-stable-github-release',
            'Downloaded release qualification differs')


def checksum_entries(data, expected):
    actual = {}
    for line in data.decode('utf-8').splitlines():
        require('  ' in line, 'Malformed outer checksum line')
        checksum, name = line.split('  ', 1)
        asset_name(name)
        require(re.fullmatch('[0-9a-f]{64}', checksum) and name not in actual,
                'Malformed or duplicate outer checksum entry')
        actual[name] = checksum
    names = set(expected) - {'RELEASE-SHA256SUMS', 'RELEASE-SHA256SUMS.sig'}
    require(set(actual) == names and len(actual) == 51, 'Outer checksum inventory differs')
    require(all(actual[name] == expected[name]['sha256'] for name in names), 'Outer checksum differs from reviewed bytes')
    return actual


def public_key(path):
    words = regular(path).read_text().split()
    require(len(words) >= 2 and words[0] == 'ssh-rsa', 'Pinned RSA public key missing')
    blob = base64.b64decode(words[1], validate=True)
    fingerprint = 'SHA256:' + base64.b64encode(hashlib.sha256(blob).digest()).decode().rstrip('=')
    require(fingerprint == FINGERPRINT, 'Release public key fingerprint differs')
    return words[0] + ' ' + words[1]


def verify_signatures(directory, logs, label):
    key = public_key(directory / KEY_FILE)
    allowed = logs / (label + '-allowed-signers')
    with allowed.open('x') as stream:
        stream.write(SIGNER + ' ' + key + '\n')
    results = []
    for payload in PAYLOADS:
        signature = payload + '.sig'
        with regular(directory / payload).open('rb') as message:
            result = subprocess.run(['ssh-keygen', '-Y', 'verify', '-f', str(allowed), '-I', SIGNER,
                                     '-n', NAMESPACE, '-s', str(regular(directory / signature))],
                                    stdin=message, stdout=subprocess.PIPE, stderr=subprocess.PIPE, check=False)
        (logs / (label + '-' + payload + '.stdout')).write_bytes(result.stdout)
        (logs / (label + '-' + payload + '.stderr')).write_bytes(result.stderr)
        require(result.returncode == 0, 'Detached public-key signature verification failed')
        results.append({'payload': payload, 'signature': signature, 'signer': SIGNER, 'namespace': NAMESPACE,
                        'key_fingerprint': FINGERPRINT, 'verified': True})
    return results


class PublicRedirects(HTTPRedirectHandler):
    def redirect_request(self, request, response, code, message, headers, new_url):
        target = urlsplit(new_url)
        require(urlsplit(request.full_url).hostname != 'api.github.com', 'GitHub API redirect is not expected')
        require(target.scheme == 'https' and target.username is None and target.password is None
                and target.port in (None, 443)
                and target.hostname in ('release-assets.githubusercontent.com', 'objects.githubusercontent.com'),
                'Public asset redirected outside the expected GitHub download hosts')
        return super().redirect_request(request, response, code, message, headers, new_url)


def response(url, pins, *, api=False):
    target = urlsplit(url)
    if api:
        allowed = {API + '/releases/' + str(pins['release_id']), API + '/releases/tags/' + TAG,
                   API + '/releases/' + str(pins['release_id']) + '/assets?per_page=100',
                   API + '/git/ref/tags/' + TAG, API + '/git/tags/' + pins['tag_object']}
        require(url in allowed, 'Unexpected API request target')
    else:
        prefix = REPOSITORY + '/releases/download/' + TAG + '/'
        require(url.startswith(prefix) and target.hostname == 'github.com'
                and not target.query and not target.fragment, 'Unexpected public asset request target')
        require(asset_name(url.removeprefix(prefix)) in pins['assets'], 'Download name is outside the independent input')
    require(target.scheme == 'https' and target.username is None and target.password is None
            and target.port in (None, 443), 'Unexpected request credentials or transport')
    headers = {'User-Agent': 'ZenoFCIS 1.1.0 public release verification', 'Accept-Encoding': 'identity', 'Cache-Control': 'no-cache'}
    headers['Accept'] = 'application/vnd.github+json' if api else 'application/octet-stream'
    # No Authorization header, credential lookup, cookies, or mutation method.
    result = build_opener(PublicRedirects()).open(Request(url, headers=headers, method='GET'), timeout=30)
    if result.status != 200:
        result.close()
        raise ValueError('Public endpoint did not return HTTP 200')
    return result


def fetch_json(url, pins):
    with response(url, pins, api=True) as stream:
        require(not any(name.lower() == 'link' and value for name, value in stream.headers.items()),
                'Unexpected Link header for a nonpaginated endpoint')
        data = stream.read(MAX_JSON_BYTES + 1)
    require(len(data) <= MAX_JSON_BYTES, 'GitHub metadata exceeds the fixed byte cap')
    return json_bytes(data)


def validate_assets(items, expected):
    require(isinstance(items, list) and len(items) == len(expected), 'Public asset inventory count differs')
    result, ids = {}, set()
    for item in items:
        name = asset_name(item['name'])
        require(name in expected and name not in result, 'Unexpected or duplicate public asset name')
        asset_id = item['id']
        url = REPOSITORY + '/releases/download/' + TAG + '/' + quote(name, safe='')
        require(type(asset_id) is int and asset_id > 0 and asset_id not in ids
                and item['state'] == 'uploaded' and type(item['size']) is int
                and item['size'] == expected[name]['size']
                and item['browser_download_url'] == url
                and item['url'] == API + '/releases/assets/' + str(asset_id), 'Public asset identity, size or URL differs')
        ids.add(asset_id)
        remote_digest = item.get('digest')
        require(remote_digest is None or remote_digest == 'sha256:' + expected[name]['sha256'],
                'GitHub asset digest differs from reviewed bytes')
        result[name] = {'asset_id': asset_id, 'url': url, 'size': item['size'],
                        'sha256': expected[name]['sha256'], 'github_digest': remote_digest}
    require(set(result) == set(expected), 'Public asset name inventory differs')
    return result


def validate_release(record, pins):
    expected = pins['assets']
    require(record['id'] == pins['release_id'] and type(record['id']) is int and record['tag_name'] == TAG
            and record['draft'] is False and record['prerelease'] is False
            and isinstance(record['published_at'], str)
            and re.fullmatch(r'\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}Z', record['published_at'])
            and record['url'] == API + '/releases/' + str(pins['release_id'])
            and record['html_url'] == REPOSITORY + '/releases/tag/' + TAG
            and record['assets_url'] == API + '/releases/' + str(pins['release_id']) + '/assets',
            'Expected release is not publicly published and stable')
    if 'immutable' in record:
        require(type(record['immutable']) is bool, 'Malformed GitHub immutable flag')
    return {'release_id': pins['release_id'], 'tag': TAG, 'draft': False, 'prerelease': False,
            'published_at': record['published_at'], 'url': record['html_url'],
            'github_immutable': record.get('immutable'), 'assets': validate_assets(record['assets'], expected)}


def validate_tag(reference, tag, pins):
    require(reference['ref'] == 'refs/tags/' + TAG and reference['object']['type'] == 'tag'
            and reference['object']['sha'] == pins['tag_object']
            and reference['object']['url'] == API + '/git/tags/' + pins['tag_object'],
            'Remote ref does not match the pinned annotated tag object')
    require(tag['sha'] == pins['tag_object'] and tag['tag'] == TAG and tag['object']['type'] == 'commit'
            and tag['object']['sha'] == pins['source_commit'] and tag['object']['url'] == API + '/git/commits/' + pins['source_commit'],
            'Remote annotated tag does not peel directly to the pinned source commit')


def remote_snapshot(pins):
    expected = pins['assets']
    release = validate_release(fetch_json(API + '/releases/' + str(pins['release_id']), pins), pins)
    by_tag = validate_release(fetch_json(API + '/releases/tags/' + TAG, pins), pins)
    require(release == by_tag, 'Release-ID and release-tag views disagree')
    assets = validate_assets(fetch_json(API + '/releases/' + str(pins['release_id']) + '/assets?per_page=100', pins), expected)
    require(assets == release['assets'], 'Dedicated public asset listing disagrees with the release')
    validate_tag(fetch_json(API + '/git/ref/tags/' + TAG, pins),
                 fetch_json(API + '/git/tags/' + pins['tag_object'], pins), pins)
    return release


def copy_download(stream, output, expected):
    require(stream.status == 200, 'Asset download did not return HTTP 200')
    length = stream.headers.get('Content-Length')
    if length is not None:
        require(length.isdecimal() and int(length) == expected['size'], 'Public Content-Length differs')
    total, checksum = 0, hashlib.sha256()
    while True:
        # At most one extra byte is read to detect an oversized response.
        block = stream.read(min(CHUNK_BYTES, expected['size'] - total + 1))
        if not block:
            break
        total += len(block)
        require(total <= expected['size'], 'Public download exceeds the independently pinned byte bound')
        checksum.update(block)
        output.write(block)
    require(total == expected['size'] and checksum.hexdigest() == expected['sha256'],
            'Public size or SHA256 differs from the independent input')


def check(input_path, work):
    pins = load_expected(input_path)
    require(not work.exists() and not work.is_symlink()
            and not any(parent.is_symlink() for parent in work.parents),
            'Existing or linked verification directory must be preserved before retry')
    work.mkdir(mode=0o700)
    downloads, logs = work / 'downloads', work / 'signature-checks'
    downloads.mkdir(mode=0o700)
    logs.mkdir(mode=0o700)
    snapshot = remote_snapshot(pins)
    for name, details in sorted(snapshot['assets'].items()):
        with response(details['url'], pins) as stream, (downloads / name).open('xb') as output:
            copy_download(stream, output, pins['assets'][name])
    require({path.name for path in downloads.iterdir()} == set(pins['assets']), 'Downloaded asset inventory differs')
    for name, expected in pins['assets'].items():
        path = regular(downloads / name)
        require(path.stat().st_size == expected['size'] and digest(path) == expected['sha256'],
                'Downloaded asset bytes changed during verification')
    checksum_entries(regular(downloads / 'RELEASE-SHA256SUMS').read_bytes(), pins['assets'])
    validate_downloaded_identity(downloads, pins)
    signatures = verify_signatures(downloads, logs, 'downloaded')
    require(remote_snapshot(pins) == snapshot, 'Release, asset identities or remote tag changed during verification')
    record = {'schema': 'zeno-fcis/public-release-download-check/1', 'status': 'passed',
              'source_commit': pins['source_commit'], 'tag_object': pins['tag_object'], 'version': VERSION,
              'checked_at': datetime.now(timezone.utc).isoformat(),
              'expected_input_sha256': pins['input_sha256'], 'checker_sha256': digest(Path(__file__)),
              'release': {key: value for key, value in snapshot.items() if key != 'assets'},
              'asset_count': len(pins['assets']),
              'assets': [{'name': name, **details} for name, details in sorted(snapshot['assets'].items())],
              'outer_checksum_entries': 51, 'signatures': signatures,
              'qualification': 'Public downloads match an independently supplied expected inventory. This unsigned result checks delivery and detached owner signatures. Separate-machine execution must be established from the hosted run and job metadata, not from this JSON alone; no separate rebuild or continuing immutability guarantee is claimed.',
              'checksum_scope': 'RELEASE-SHA256SUMS covers the other 51 flat assets. Original SHA256SUMS and its signature retain their bytes; its paths describe the unpacked RC bundle, not this flat download directory.'}
    with (work / 'result.json').open('xb') as output:
        output.write(encode(record))
    return record


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--input', required=True, type=Path)
    parser.add_argument('--out', required=True, type=Path)
    args = parser.parse_args()
    result = check(args.input, args.out)
    print(json.dumps({'status': result['status'], 'source_commit': result['source_commit'],
                      'release_id': result['release']['release_id'], 'assets_verified': result['asset_count']}))
