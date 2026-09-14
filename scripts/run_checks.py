"""Record raw command output without hiding nonzero exits. No toolchain changes."""
import argparse
import json
import os
from pathlib import Path
import subprocess
import time

parser = argparse.ArgumentParser()
parser.add_argument('--out', type=Path, required=True)
parser.add_argument('commands', nargs='+')
args = parser.parse_args()
args.out.mkdir(parents=True, exist_ok=False)
results = []
for index, command in enumerate(args.commands, 1):
    import shlex
    start = time.monotonic()
    log = args.out / f'{index:02}.log'
    with log.open('w') as stream:
        stream.write(command + '\n')
        stream.flush()
        result = subprocess.run(shlex.split(command), stdout=stream, stderr=subprocess.STDOUT, env=os.environ)
    results.append(dict(command=command, exit_code=result.returncode, seconds=time.monotonic()-start, log=log.name))
    print(f'{result.returncode}: {command}', flush=True)
    (args.out/'commands.json').write_text(json.dumps(results, indent=2))
raise SystemExit(1 if any(r['exit_code'] for r in results) else 0)
