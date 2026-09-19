"""P1K_CIRCLES v1: reproduce the frozen S2-B3 40 x 25 circles exactly."""
import hashlib
import json
from pathlib import Path
root = Path(__file__).resolve().parents[1]
destination = root / 'fixtures/synthetic/s2b3_1'
destination.mkdir(exist_ok=True)
source = ('G04 S2-B3 public deterministic 1000 circle selection fixture*\n'
          '%FSLAX26Y26*%\n%MOMM*%\n%ADD10C,0.5*%\nD10*\n')
source += ''.join(f'X{x*1000000}Y{y*1000000}D03*\n' for y in range(1,26) for x in range(1,41))
source += 'M02*\n'
original = root/'evidence-public/s2b3-final-20260920/gui/metrics_1000.gbr'
assert source.encode() == original.read_bytes(), 'must preserve original large-selection sample'
(destination/'P1K_CIRCLES.gbr').write_bytes(source.encode())
(destination/'manifest.json').write_text(json.dumps(dict(generator='generate_p1k.py v1',object_count=1000,bounds_mm=[.75,.75,40.25,25.25],diameter_mm=.5,sha256=hashlib.sha256(source.encode()).hexdigest()),indent=2)+'\n')
