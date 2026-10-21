#!/usr/bin/env python3
"""Replay a native public-example report; no process execution or private defaults."""
import argparse
import json
from pathlib import Path
import usage_knowledge_reference as U


def main():
    p=argparse.ArgumentParser();p.add_argument('--report',type=Path,required=True);p.add_argument('--task',default='')
    args=p.parse_args();report=json.loads(args.report.read_text())
    count=U.verify_native_explanations(report,args.task)
    print(json.dumps({'status':'passed','native_traces_replayed':count,'authority':'none'},sort_keys=True))


if __name__ == "__main__":
    try:
        from telemetry import run_cli
    except ImportError:
        # Reporting is optional, including for a standalone copy of this tool.
        run_cli = lambda function: function()
    def _entrypoint():
        main()
    run_cli(_entrypoint)
