#!/usr/bin/env python3
"""Solve or independently check explicit finite arenas (no native acquisition)."""
import argparse
import hashlib
import json
from pathlib import Path
from arena_spec import load, need
from arena_check import check

HERE = Path(__file__).resolve().parent
SOURCES = ('arena.py','arena_spec.py','arena_check.py','knowledge.py','game_spec.py')


def identities():
    return {name: hashlib.sha256((HERE/name).read_bytes()).hexdigest() for name in SOURCES}


def write(path, value):
    path.write_text(json.dumps(value, sort_keys=True, indent=2) + '\n')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('mode', choices=('solve','check'))
    parser.add_argument('--game', type=Path, help='explicit zeno-fcis/arena/1 JSON (required for solve)')
    parser.add_argument('--out', type=Path, required=True, help='fresh evidence directory for solve; existing directory for check')
    parser.add_argument('--max-states', type=int, default=10000)
    parser.add_argument('--max-edges', type=int, default=100000)
    parser.add_argument('--work', type=int, default=1000000)
    parser.add_argument('--seconds', type=float, default=30)
    args = parser.parse_args()
    try:
        if args.mode == 'solve':
            from knowledge import solve, Budget
            need(args.game is not None, '--game required')
            root = HERE.parents[1]
            need(not args.out.resolve().is_relative_to(root), 'evidence must be outside source')
            arena = load(args.game); before = identities()
            args.out.mkdir(parents=True, exist_ok=False)
            write(args.out/'game.json', arena.wire())
            packet = solve(arena, Budget(args.max_states,args.max_edges,args.work,args.seconds))
            write(args.out/'certificate.json', packet)
            if packet.get('status') == 'UNKNOWN':
                print(json.dumps(packet)); return 2
            verdict = check(packet, arena)
            need(before == identities(), 'source drift during solve/check')
            write(args.out/'verdict.json', verdict)
            write(args.out/'arena-binding.json', dict(sources=before, artifacts={name:hashlib.sha256((args.out/name).read_bytes()).hexdigest()
                  for name in ('game.json','certificate.json')}))
        else:
            binding = json.loads((args.out/'arena-binding.json').read_text())
            need(binding['sources'] == identities(), 'arena source drift')
            need(set(binding['artifacts']) == {'game.json','certificate.json'}, 'artifact coverage')
            for name, sha in binding['artifacts'].items():
                need(hashlib.sha256((args.out/name).read_bytes()).hexdigest() == sha, 'artifact drift')
            arena = load(args.game or args.out/'game.json')
            verdict = check(json.loads((args.out/'certificate.json').read_text()), arena)
        print(json.dumps(verdict, sort_keys=True)); return 0
    except (ValueError,KeyError,TypeError,IndexError,OSError,RecursionError) as error:
        print(json.dumps(dict(evidence='UNKNOWN', stop=str(error), claim='no certificate established'))); return 2


if __name__ == '__main__': raise SystemExit(main())
