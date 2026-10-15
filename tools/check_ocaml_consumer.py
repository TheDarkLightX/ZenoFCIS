#!/usr/bin/env python3
"""Bounded installed OCaml/Rust correspondence; consumes already built artifacts.

No catalogue certificate is issued. Decisions come only from the actual native
application. Expected labels are frozen data, not a second evaluator.
"""
from __future__ import annotations
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import sqlite3
import subprocess
import time

ROOT = Path(__file__).resolve().parents[1]
PROFILE = 'zeno-fcis/native-scalar-consumer/1'
SCHEMA = 'zeno-fcis/native-scalar-frame/1'


def require(condition, message):
    if not condition:
        raise RuntimeError(message)


def digest(data):
    return hashlib.sha256(data).hexdigest()


def strict(pairs):
    value = {}
    for key, item in pairs:
        require(key not in value, f'duplicate JSON member: {key}')
        value[key] = item
    return value


def parsed(data):
    return json.loads(data, object_pairs_hook=strict)


def files(directory):
    return {str(path.relative_to(directory)): path.read_bytes()
            for path in sorted(directory.rglob('*')) if path.is_file()}


def tables(database):
    with sqlite3.connect(f'file:{database}?mode=ro', uri=True) as connection:
        names = [row[0] for row in connection.execute(
            "SELECT name FROM sqlite_schema WHERE type='table' AND name NOT LIKE 'sqlite_%' ORDER BY name")]
        return {name: sorted(connection.execute('SELECT * FROM "' + name.replace('"', '""') + '"'), key=repr)
                for name in names}


class Check:
    def __init__(self, options):
        self.options = options
        self.work = options.work_dir.resolve()
        require(not self.work.exists(), 'work directory must be fresh')
        self.work.mkdir(parents=True)
        self.deadline = time.monotonic() + options.budget_seconds
        self.records = []
        self.consumer = options.consumer.resolve()
        self.expected = parsed((ROOT / 'integrations/ocaml/tests/expected.json').read_bytes())
        self.report = {'schema': 'zeno-fcis/ocaml-consumer-check/1', 'status': 'unfinished',
                       'profile': PROFILE, 'authority': 'actual-native-only',
                       'catalogue_certificate_installation': 'unclaimed',
                       'independent_review': False, 'expected_sha256': digest(
                           (ROOT / 'integrations/ocaml/tests/expected.json').read_bytes()),
                       'records': self.records}

    def run(self, arguments, expected=None):
        remaining = self.deadline - time.monotonic()
        require(remaining > 0, 'global checker budget exhausted')
        result = subprocess.run([str(value) for value in arguments], capture_output=True,
                                timeout=min(45, remaining), env={**os.environ, 'PYTHONDONTWRITEBYTECODE': '1'})
        require(len(result.stdout) <= 32 * 1024 * 1024 and len(result.stderr) <= 32 * 1024 * 1024,
                'checker output cap exceeded')
        self.records.append({'argv': [str(value) for value in arguments], 'exit': result.returncode,
                             'stdout_sha256': digest(result.stdout), 'stderr_sha256': digest(result.stderr)})
        if expected is not None:
            require(result.returncode == expected, f'exit {result.returncode}, expected {expected}: {arguments}; {result.stderr[:2048]!r}')
        return result

    def native(self, native, command, *arguments, expected=None):
        return self.run([native, command, *arguments, '--format', 'json'], expected)

    def host(self, native, version, command, *arguments, expected=None):
        return self.run([self.consumer, native, version, command, *arguments], expected)

    @staticmethod
    def typed(words):
        return [name + '=' + ('bool:' if value in ('true', 'false') else
                             'i64:' if value.lstrip('-').isdigit() else 'enum:') + value
                for name, value in (word.split('=', 1) for word in words)]

    def installed(self, name, executable):
        directory = self.work / 'install' / name
        directory.mkdir(parents=True)
        native = directory / 'native'
        shutil.copy2(executable, native)
        version = directory / 'version.json'
        response = self.native(native, 'version', expected=0)
        binding = parsed(response.stdout)
        require(binding['host_abi'] == SCHEMA and binding['host_profile'] == PROFILE, 'unsupported native profile')
        version.write_bytes(response.stdout)
        return native, version, directory

    def frame(self, native, version, directory, name, words):
        path = directory / (name + '.frame')
        direct = self.native(native, 'frame', *words, expected=0)
        report = parsed(direct.stdout)
        require(list(report) == ['schema', 'profile', 'frame'] and report['schema'] == SCHEMA
                and report['profile'] == PROFILE, 'frame report identity/shape')
        canonical = bytes.fromhex(report['frame'])
        require(canonical.hex() == report['frame'], 'native frame hex alias')
        self.host(native, version, 'frame', path, *self.typed(words), expected=0)
        require(path.read_bytes() == canonical, 'native/OCaml request bytes differ')
        return path, direct.stdout

    def checkpoints(self, name, native, directory):
        stores = []
        for index in range(4 if name == 'c3' else 2):
            store = directory / ('state-' + str(index))
            if index == 0:
                store.mkdir(); self.native(native, 'init', store / 'state.sqlite', expected=0)
            else:
                shutil.copytree(stores[-1], store)
                arguments = ['command.action=Inc', 'context.authorized=true'] if name == 'c3' else [
                    'command.action=Up', 'command.probe=9223372036854775807', 'context.authorized=true']
                self.native(native, 'submit', store / 'state.sqlite', *arguments, expected=0)
            stores.append(store)
        return stores

    def corpus(self, name, native, version, directory):
        stores = self.checkpoints(name, native, directory)
        corpus = self.expected[name if name == 'c3' else 'endpoints']
        for index, row in enumerate(corpus):
            case = directory / ('case-' + str(index)); case.mkdir()
            state_index = int(row['state'][0]) if name == 'c3' else int(row['state'][0] == '-9223372036854775807')
            store = case / 'store'; shutil.copytree(stores[state_index], store)
            database = store / 'state.sqlite'
            state = parsed(self.native(native, 'state', database, expected=0).stdout)['state']
            require(list(map(str, state.values())) == row['state'], 'checkpoint is not the declared actual native state')
            frame, _ = self.frame(native, version, case, 'request', row['arguments'])
            before = files(store)
            exit_code = 2 if row['expected']['class'] == 'Reject' else 0
            direct = self.native(native, 'decide', database, *row['arguments'], expected=exit_code)
            host = self.host(native, version, 'decide', database, frame, expected=exit_code)
            require((direct.stdout, direct.stderr) == (host.stdout, host.stderr), 'full native/host decision differs')
            observed = parsed(host.stdout)
            require({key: observed[key] for key in row['expected']} == row['expected'], 'independent expected decision differs')
            require(observed['commit'] is None and files(store) == before, 'preview changed actual store/journal bytes')
        return stores[0]

    def negatives(self, native, version, directory, genesis, endpoint, endpoint_version):
        store = directory / 'negative-store'; shutil.copytree(genesis, store)
        database = store / 'state.sqlite'; before = files(store)
        word_cases = {
            'unknown-word': ['unknown=true', 'command.action=Inc', 'context.authorized=true'],
            'duplicate-word': ['command.action=Inc', 'command.action=Inc', 'context.authorized=true'],
            'missing-word': [],
            'bool-domain': ['command.action=Inc', 'context.authorized=2'],
        }
        for row in self.expected['native_negatives']:
            name = row['case']
            if name in word_cases:
                words = word_cases[name]
                direct = self.native(native, 'frame', *words, expected=row['exit'])
                # The public client requires qualified names; unknown-name refusal is local.
                host_exit = 65 if name == 'unknown-word' else row['exit']
                self.host(native, version, 'frame', directory / (name + '.out'), *self.typed(words), expected=host_exit)
            elif name == 'integer-outside-i64':
                words = ['command.action=Up', 'command.probe=9223372036854775808', 'context.authorized=true']
                self.native(endpoint, 'frame', *words, expected=1)
                self.host(endpoint, endpoint_version, 'frame', directory / 'outside.out', *self.typed(words), expected=65)
            elif name == 'frame-and-words':
                self.native(native, 'submit', database, '--frame', directory / 'missing.frame',
                            'command.action=Inc', 'context.authorized=true', expected=64)
            else:
                frame = self.options.negatives.resolve() / (name + '.frame')
                require(frame.is_file(), f'missing native-codec negative artifact: {name}')
                direct = self.native(native, 'submit', database, '--frame', frame, expected=1)
                host = self.host(native, version, 'submit', database, frame, expected=65 if name == 'oversized-frame' else 1)
                if name != 'oversized-frame':
                    require((host.stdout, host.stderr) == (direct.stdout, direct.stderr), f'{name}: refusal changed')
            require(files(store) == before, f'{name}: native refusal wrote store/journal bytes')

    def decoder_negatives(self, report, directory):
        text = report.decode('ascii')
        marker = '"frame":"'
        index = text.index(marker) + len(marker)
        hex_text = text[index:-3]
        uppercase = text[:index] + hex_text.upper() + text[-3:]
        require(uppercase != text, 'supplied native frame needs an a-f digit for uppercase check')
        aliases = {
            'uppercase-hex': uppercase, 'odd-hex': text[:index] + '0' + text[index:],
            'nonhex': text[:index] + 'g' + text[index + 1:],
            'duplicate-frame-field': text[:-2] + ',"frame":"00"}\n',
            'wrong-frame-schema': text.replace(SCHEMA, SCHEMA + '-other'),
            'wrong-frame-profile': text.replace(PROFILE, PROFILE + '-other'),
            'missing-newline': text[:-1], 'extra-newline': text + '\n',
        }
        for name, text in aliases.items():
            source, destination = directory / (name + '.json'), directory / (name + '.bin')
            source.write_text(text); destination.write_bytes(b'preserved')
            self.run([self.consumer, 'decode-report', source, destination], 65)
            require(destination.read_bytes() == b'preserved', f'{name}: decoder wrote rejected output')

    def journey(self, native, version, directory):
        direct, host = directory / 'direct', directory / 'ocaml'
        direct.mkdir(); host.mkdir()
        self.native(native, 'init', direct / 'state.sqlite', expected=0)
        self.host(native, version, 'init', host / 'state.sqlite', expected=0)
        operations = [('Dec', True, 'Reject', 202, 0), ('Inc', True, 'Accept', None, 1),
                      ('Inc', False, 'Reject', 200, 1), ('Inc', True, 'Accept', None, 2),
                      ('Inc', True, 'Accept', None, 3), ('Inc', True, 'Reject', 201, 3),
                      ('Dec', True, 'Accept', None, 2)]
        for index, (action, authorized, cls, reason, state) in enumerate(operations):
            words = ['command.action=' + action, 'context.authorized=' + str(authorized).lower()]
            frame, _ = self.frame(native, version, directory, 'step-' + str(index), words)
            before_direct, before_host = files(direct), files(host)
            expected = 2 if cls == 'Reject' else 0
            a = self.native(native, 'submit', direct / 'state.sqlite', *words, expected=expected)
            b = self.host(native, version, 'submit', host / 'state.sqlite', frame, expected=expected)
            require((a.stdout, a.stderr) == (b.stdout, b.stderr), 'installed submit reports differ')
            result = parsed(b.stdout)
            require((result['class'], result['reason'], result['state'], result['deliveries']) ==
                    (cls, reason, {'value': state}, []), 'installed journey differs from supplied labels')
            if expected == 2:
                require(files(direct) == before_direct and files(host) == before_host, 'reject changed bytes')
            require(tables(direct / 'state.sqlite') == tables(host / 'state.sqlite'), 'canonical SQLite rows/tokens differ')
        a = self.native(native, 'history', direct / 'state.sqlite', expected=0)
        b = self.host(native, version, 'history', host / 'state.sqlite', expected=0)
        require(a.stdout == b.stdout, 'complete audited history/reopen differs')
        return host / 'state.sqlite'

    def transport_faults(self, native, version, directory):
        wrong = directory / 'wrong-version.json'; wrong.write_bytes(version.read_bytes().replace(b'"identity":"', b'"identity":"0', 1))
        self.host(native, wrong, 'state', directory / 'never-created.sqlite', expected=65)
        require(not (directory / 'never-created.sqlite').exists(), 'binding mismatch opened a store')
        # Fault wrappers run the actual owner first. They are transport controls,
        # not a substitute evaluator or claimed authenticated native binary.
        wrapper = directory / 'transport-fault.py'
        wrapper.write_text('#!/usr/bin/python3\nimport os,subprocess,sys,time\n'
            + 'native=' + repr(str(native)) + '\n'
            + 'if sys.argv[1]=="version": os.execv(native,[native]+sys.argv[1:])\n'
            + 'result=subprocess.run([native]+sys.argv[1:])\n'
            + 'if sys.argv[1]=="frame":\n'
            + ' for _ in range(513): sys.stdout.buffer.write(b"x"*65536); sys.stdout.buffer.flush()\n'
            + 'else: time.sleep(40)\n'
            + 'sys.exit(result.returncode)\n')
        wrapper.chmod(0o755)
        self.host(wrapper, version, 'frame', directory / 'capped.frame',
                  'command.action=enum:Inc', 'context.authorized=bool:true', expected=66)
        database = directory / 'unknown.sqlite'; self.native(native, 'init', database, expected=0)
        frame, _ = self.frame(native, version, directory, 'unknown-request', ['command.action=Inc', 'context.authorized=true'])
        self.host(wrapper, version, 'submit', database, frame, expected=66)
        state = parsed(self.native(native, 'state', database, expected=0).stdout)
        history = parsed(self.native(native, 'history', database, expected=0).stdout)
        require(state['state'] == {'value': 1} and state['commits'] == 1, 'unknown commit recovery state differs')
        require(history['commits'] == 1, 'unknown recovery did not audit actual committed history')
        self.report['unknown_submission_recovered'] = 'actual native commit 1; no automatic retry'

    def main(self):
        sdk = self.work / 'install' / 'sdk'; sdk.mkdir(parents=True)
        for name in ['zeno_scalar.ml', 'zeno_scalar.mli', 'counter.ml', 'README.md']:
            shutil.copy2(ROOT / 'integrations/ocaml' / name, sdk / name)
        shutil.copy2(self.consumer, sdk / 'counter'); self.consumer = sdk / 'counter'
        c3, c3_version, c3_dir = self.installed('c3', self.options.c3)
        endpoint, endpoint_version, endpoint_dir = self.installed('endpoints', self.options.endpoints)
        genesis = self.corpus('c3', c3, c3_version, c3_dir)
        self.corpus('endpoints', endpoint, endpoint_version, endpoint_dir)
        self.negatives(c3, c3_version, c3_dir, genesis, endpoint, endpoint_version)
        _, report = self.frame(c3, c3_version, c3_dir, 'decoder-source', ['command.action=Inc', 'context.authorized=true'])
        self.decoder_negatives(report, c3_dir)
        self.journey(c3, c3_version, c3_dir)
        self.transport_faults(c3, c3_version, c3_dir)
        self.report.update(status='passed', c3_tuples=16, endpoint_tuples=16,
                           native_negatives=24, client_negative_groups=11,
                           installed_files_sha256={str(path.relative_to(self.work / 'install')): digest(path.read_bytes())
                                                   for path in sorted((self.work / 'install').rglob('*')) if path.is_file()},
                           assumptions=['trusted installed filesystem/loader/native executable',
                                        'supplied authorization Bool is not authenticated identity'],
                           formal_foreign_client_proof=False, release_qualified=False)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ('c3', 'endpoints', 'consumer', 'negatives', 'work-dir', 'report'):
        parser.add_argument('--' + name, type=Path, required=True)
    parser.add_argument('--budget-seconds', type=int, default=600)
    options = parser.parse_args()
    require(0 < options.budget_seconds <= 600, 'checker budget must be in 1..600')
    check = Check(options)
    try:
        check.main()
    except (RuntimeError, OSError, subprocess.SubprocessError, ValueError) as error:
        check.report.update(status='failed', error=str(error))
        raise
    finally:
        options.report.parent.mkdir(parents=True, exist_ok=True)
        options.report.write_text(json.dumps(check.report, indent=2, sort_keys=True) + '\n')


if __name__ == '__main__':
    main()
