"""Frozen S2-B3.2 display stress fixtures v1; no private manufacturing inputs."""
from pathlib import Path
import hashlib,json
root=Path(__file__).resolve().parents[1]/'fixtures/synthetic/s2b3_2'
root.mkdir(exist_ok=True)
header='%FSLAX46Y46*%\n%MOMM*%\n%ADD10C,0.5*%\nD10*\n'
def write(name,points):
    data=(header+''.join(f'X{x*1000000}Y{y*1000000}D03*\n' for x,y in points)+'M02*\n').encode()
    (root/(name+'.gbr')).write_bytes(data)
    return dict(name=name,object_count=len(points),sha256=hashlib.sha256(data).hexdigest(),generator='generate_s2b32.py v1')
rows=[]
rows.append(write('P10K_CIRCLES',[(x+1,y+1) for y in range(100) for x in range(100)]))
rows.append(write('P100K_CIRCLES',[(x+1,y+1) for y in range(100) for x in range(1000)]))
rows.append(write('SPARSE_DENSE',[(x+1,y+1) for y in range(10) for x in range(10)]+[(1000,1000)]*1000))
(root/'manifest.json').write_text(json.dumps(rows,indent=2)+'\n')
