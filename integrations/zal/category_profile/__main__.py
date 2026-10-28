"""Standalone optional category Lens tools; operator configuration is not JSON."""
import argparse
import hashlib
import json
from pathlib import Path
import sys
import types

sys.dont_write_bytecode = True
ROOT = Path(__file__).resolve().parent
HOST_SHA256 = '22695b28ba45e76c76b863d1dd34523dc2ba199caa7698a048c3c657c73ae0f0'


def captured_host():
    path = ROOT/'host.py'
    if path.is_symlink() or not path.is_file() or path.stat().st_size > 128*1024:
        raise ValueError('ordinary bounded host source required')
    data = path.read_bytes()
    if hashlib.sha256(data).hexdigest() != HOST_SHA256:
        raise ValueError('category host source changed')
    if 'category_profile' not in sys.modules:
        package = types.ModuleType('category_profile'); package.__path__ = []
        sys.modules['category_profile'] = package
    module = types.ModuleType('category_profile.host')
    module.__file__ = str(path)
    module.__package__ = 'category_profile'
    sys.modules[module.__name__] = module
    exec(compile(data, str(path), 'exec'), module.__dict__)
    return module


def verdict_exit(answer):
    if 'matches_recomputation' in answer:
        return 0 if answer['matches_recomputation'] is True else 1
    if answer.get('schema') in ('zal/library-category-application/1', 'zal/library-category-toolkit/1'):
        return 0  # Data preparation/discovery is not a mathematical positive.
    status = answer.get('status')
    if status in ('CheckedAt', 'Authored', 'Audited', 'Prepared'):
        return 0
    return 1 if status == 'RefutedAt' else 2


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest='action', required=True)
    commands.add_parser('toolkit')
    call = commands.add_parser('call'); call.add_argument('request', help='JSON call file or - for stdin')
    call.add_argument('--runtime-config', type=Path, help='trusted operator approval file; never accepted in tool JSON')
    call.add_argument('--evidence-catalog', type=Path); call.add_argument('--evidence-sha256')
    prepare = commands.add_parser('prepare-runtime'); prepare.add_argument('directory', type=Path)
    args = parser.parse_args()
    try:
        host = captured_host()
        if args.action == 'prepare-runtime':
            answer = host.prepare_runtime(args.directory)
        else:
            adapter = host.load_adapter()
            if args.action == 'toolkit':
                answer = adapter.toolkit()
            else:
                if args.request == '-':
                    raw = sys.stdin.buffer.read(2_000_001)
                else:
                    with Path(args.request).open('rb') as stream:
                        raw = stream.read(2_000_001)
                if len(raw) > 2_000_000:
                    raise OverflowError('tool call byte budget exceeded')
                packet = adapter.core.loads(raw.decode('utf-8'))
                adapter.core.keys(packet, {'tool', 'arguments'}, where='category host call')
                context = host.operator_context(runtime_config=args.runtime_config,
                    evidence_catalog=args.evidence_catalog, evidence_sha256=args.evidence_sha256)
                answer = host.dispatch_tool(packet['tool'], packet['arguments'], context=context)
    except (ValueError, KeyError, TypeError, OSError, ImportError) as error:
        answer = {'status': 'Invalid', 'authority': 'none', 'reason': str(error)[:1000]}
    except (OverflowError, RecursionError) as error:
        answer = {'status': 'UNKNOWN', 'authority': 'none', 'reason': str(error)[:1000]}
    print(json.dumps(answer, sort_keys=True, ensure_ascii=True, allow_nan=False))
    return verdict_exit(answer)


if __name__ == '__main__':
    raise SystemExit(main())
