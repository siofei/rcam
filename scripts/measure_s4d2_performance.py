"""Measure the actual release test process RSS, excluding Cargo/compiler memory (macOS)."""
import json
import os
from pathlib import Path
import re
import subprocess
import sys
ROOT=Path(__file__).resolve().parents[1]
command=['cargo','test','--release','--locked','-p','editor-service','--test','component_candidates','--no-run','--message-format=json']
r=subprocess.run(command,cwd=ROOT,text=True,capture_output=True)
print(r.stderr,end='')
if r.returncode: raise SystemExit(r.returncode)
artifacts=[json.loads(line) for line in r.stdout.splitlines() if line.startswith('{')]
exe=next(v['executable'] for v in artifacts if v.get('reason')=='compiler-artifact' and v.get('target',{}).get('name')=='component_candidates' and v.get('executable'))
r=subprocess.run(['/usr/bin/time','-l',exe,'performance_100k_sparse_and_5000_dense','--ignored','--exact','--nocapture'],cwd=ROOT,text=True,capture_output=True)
print(r.stdout,end='');print(r.stderr,end='')
if r.returncode: raise SystemExit(r.returncode)
match=re.search(r'(\d+)\s+maximum resident set size',r.stderr)
assert match, 'macOS resident-set observation missing'
rss=int(match.group(1));assert rss <=512*1024*1024,(rss,'512 MiB RSS budget exceeded')
path=Path(os.environ['RCAM_S4D2_EVIDENCE_DIR'])/'candidate-performance.json'
report=json.loads(path.read_text());report.update(observed_peak_rss_bytes=rss,rss_budget_bytes=512*1024*1024,memory_measurement='macOS /usr/bin/time -l over standalone release test process, Cargo/compiler excluded')
path.write_text(json.dumps(report,indent=2)+'\n');print('PASS actual 100k release RSS',rss)
