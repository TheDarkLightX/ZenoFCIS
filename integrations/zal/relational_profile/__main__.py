"""Prepare, lower, or query the optional finite relational profile."""
import argparse
import hashlib
import json
import sys
import types
from pathlib import Path

if not __package__:
    # The qualified direct entry point loads only these captured source bytes;
    # importing the checkout's ancestor packages first would execute unpinned
    # __init__.py files before the provider boundary could check anything.
    expected = {
        'profile': 'c9e65c94caed9a03c4b58e8d39c2fc99a720d25022c856965dd71e7085443133',
        'bridge': '97110ed5f68946386256ccf2f63f9beaec603a5def0763721dcf536cb381028f',
    }
    try:
        root = Path(__file__).resolve().parent
        captured = {}
        for name, digest in expected.items():
            path = root / (name + '.py')
            if path.is_symlink():
                raise ValueError('symlink in standalone source: ' + name)
            with path.open('rb') as handle:
                source = handle.read(1024**2 + 1)
            if len(source) > 1024**2 or hashlib.sha256(source).hexdigest() != digest:
                raise ValueError('standalone source changed: ' + name)
            captured[name] = (path, source, digest)
        package_name = '_zenofcis_relational_standalone'
        package = types.ModuleType(package_name)
        package.__path__ = []
        sys.modules[package_name] = package
        for name, (path, source, digest) in captured.items():
            module = types.ModuleType(package_name + '.' + name)
            module.__file__, module.__package__ = str(path), package_name
            module.__verified_source_sha256__ = digest
            sys.modules[module.__name__] = module
            exec(compile(source, str(path), 'exec'), module.__dict__)  # noqa: S102 - pinned local bytes
            setattr(package, name, module)
        profile, bridge = package.profile, package.bridge
    except (OSError, ValueError, SyntaxError, ImportError) as exc:
        print(json.dumps({'schema': 'zal/relational-profile-result/1',
                          'status': 'Error', 'detail': str(exc)[:1000]}, sort_keys=True))
        raise SystemExit(2) from exc
else:
    from . import bridge, profile


def exit_status(answer):
    status = answer.get('status')
    if status in ('Prepared', 'Lowered', 'ExactCount', 'Evaluated', 'EquivalentAt', 'ImpliesAt'):
        return 0
    return 1 if status == 'RefutedAt' else 2


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest='command', required=True)
    prepare = commands.add_parser('prepare', help='copy pinned provider sources to a new runtime directory')
    prepare.add_argument('runtime', type=Path)
    run = commands.add_parser('run', help='execute one bounded relational JSON request')
    run.add_argument('runtime', type=Path)
    run.add_argument('request', help='request JSON file or - for stdin')
    run.add_argument('--seconds', type=int, default=60)
    run.add_argument('--max-models', type=int, default=int(profile.DEFAULT_LIMITS['candidates']))
    run.add_argument('--max-work', type=int, default=int(profile.DEFAULT_LIMITS['work']))
    run.add_argument('--scratch', type=Path, help='directory for bounded response capture')
    lower = commands.add_parser('lower', help='lower supported current-ladder source without executing a checker')
    lower.add_argument('--context', required=True, type=Path)
    lower.add_argument('--text', required=True)
    lower.add_argument('--rung', default='typed')
    lower.add_argument('--domain-size', required=True, type=int)
    ladder = commands.add_parser('run-ladder', help='retain the current-ladder reading and run its finite count')
    ladder.add_argument('runtime', type=Path)
    ladder.add_argument('--context', required=True, type=Path)
    ladder.add_argument('--text', required=True)
    ladder.add_argument('--rung', default='typed')
    ladder.add_argument('--domain-size', required=True, type=int)
    ladder.add_argument('--seconds', type=int, default=60)
    ladder.add_argument('--scratch', type=Path)
    args = parser.parse_args()
    try:
        if args.command == 'prepare':
            answer = profile.prepare_runtime(args.runtime)
        elif args.command == 'lower':
            answer = bridge.lower_text(args.context, args.text, args.rung, args.domain_size)
        elif args.command == 'run-ladder':
            answer = bridge.lower_and_execute(args.context, args.text, args.rung, args.domain_size,
                                              args.runtime, seconds=args.seconds, scratch=args.scratch)
        else:
            if args.request == '-':
                data = sys.stdin.buffer.read(profile.MAX_BYTES + 1)
            else:
                with Path(args.request).open('rb') as handle:
                    data = handle.read(profile.MAX_BYTES + 1)
            limits = dict(profile.DEFAULT_LIMITS, candidates=str(args.max_models), work=str(args.max_work))
            answer = profile.execute(data, args.runtime, limits=limits, seconds=args.seconds, scratch=args.scratch)
    except profile.ProfileFailure as exc:
        answer = {'schema': profile.RESULT_SCHEMA, 'status': exc.status, 'detail': exc.detail}
    except (ValueError, TypeError, OSError) as exc:
        answer = {'schema': profile.RESULT_SCHEMA, 'status': 'Error', 'detail': str(exc)[:1000]}
    print(json.dumps(answer, sort_keys=True, indent=2))
    raise SystemExit(exit_status(answer))


if __name__ == '__main__':
    main()
