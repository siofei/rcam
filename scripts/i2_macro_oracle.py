"""Independent exact-f64 lens truth through uniform affine Flash transforms.
Retains all900 rereview cases; extends scale/rotation/mirror/cancellation.
"""
from pathlib import Path
import subprocess,json,math,sys
from decimal import Decimal as D

ROOT=Path(__file__).resolve().parents[1]
root=Path(sys.argv[1])
ns={}
exec((ROOT/'scripts/i2_circle_oracle.py').read_text().split('rng=random.Random')[0],ns)
oracle=ns['oracle'];PI=ns['PI']

def sincos(degrees):
    x=(D.from_float(degrees)*PI/180)%(2*PI)
    if x>PI:x-=2*PI
    s=term=x;c=cterm=D(1);n=1
    while abs(term)>D('1e-78') or abs(cterm)>D('1e-78'):
        term*=-(x*x)/((2*n)*(2*n+1));s+=term
        cterm*=-(x*x)/((2*n-1)*(2*n));c+=cterm;n+=1
    return s,c

cases=[];practical=[]
for s in [.25,.5,1,2,4]:
 for near in [1e-2,1e-4,1e-6,1e-8,1e-10]:
  for angle in [0,.3,1.1]:
   for scale in [1.,10.,100.,1000.,10000.,100000.]:
    for world in [0.,1000.]:
     d=(1+s)*(1-near)
     cases.append((1.,s,d*math.cos(angle),d*math.sin(angle),scale,world,world,0.))
     practical.append(near>=1e-4 and scale<=1000.)
assert len(cases)==900
for scale in [.001,.1,1.,10.,1000.]:
 for world in [0.,1000.,1e6]:
  for rotation in [0.,37.,90.]:
   for mirror in range(4):
    for near in [.01,1e-8,1e-10]:
     cases.append((1.,.25,1.25*(1-near),0.,scale,world,-world,rotation,float(mirror),0.,0.))
     practical.append(near==.01 and world<=1000.)
for scale in [.001,1.,100.]:
 for rotation in [0.,37.,90.]:
  for mirror in range(4):
   for near in [.01,1e-10]:
    cases.append((1.,.25,1.25*(1-near),0.,scale,-1e6*scale,-1e6*scale,rotation,float(mirror),1e6,1e6))
    practical.append(False)
assert len(cases)==1512
inputs=[' '.join(map(repr,c)) for c in cases]
r=subprocess.run([str(root/'macro_probe')],input='\n'.join(inputs)+'\n',text=True,capture_output=True)
(root/'macro-input.txt').write_text('\n'.join(inputs)+'\n');(root/'macro-results.txt').write_text(r.stdout)
assert r.returncode==0,r.stderr
lines=r.stdout.splitlines();assert len(lines)==len(cases)
fail=[];rejected={};ready=0;baseline_ready=0;maxratio=0;rows=[];practical_ready=0
for i,(line,c) in enumerate(zip(lines,cases)):
 rows.append(dict(index=i,input=inputs[i],actual=line))
 if not line.startswith('READY '):
  rejected[line]=rejected.get(line,0)+1
  if not line.startswith('ERR PrecisionUncertain('):fail.append(dict(index=i,unexpected=line,input=inputs[i]))
  continue
 ready+=1;baseline_ready+=int(i<900);practical_ready+=int(practical[i])
 r,s,dx,dy,scale,wx,wy,rot=c[:8];mirror=int(c[8]) if len(c)>8 else 0
 lx,ly=c[9:] if len(c)>9 else (0.,0.)
 # Float additions reproduce only the semantic input construction. All union
 # and transform truth calculations below are independent Decimal arithmetic.
 a,x,y,p=oracle(r,s,(lx,ly),(lx+dx,ly+dy),False)
 sc=D.from_float(scale);a*=sc*sc;p*=sc
 if mirror in (1,3):x=-x
 if mirror in (2,3):y=-y
 sine,cosine=sincos(rot);x,y=sc*(cosine*x-sine*y)+D.from_float(wx),sc*(sine*x+cosine*y)+D.from_float(wy)
 va,vp,vx,vy,ce,ae,pe=map(float,line.split()[1:])
 for kind,value,truth,bound in [('area',va,a,ae),('perimeter',vp,p,pe),('centroid_x',vx,x,ce),('centroid_y',vy,y,ce)]:
  error=abs(D.from_float(value)-truth);ratio=float(error/D.from_float(bound));maxratio=max(maxratio,ratio)
  if error>D.from_float(bound):fail.append(dict(index=i,kind=kind,input=inputs[i],actual=value,oracle=str(truth),error=str(error),bound=bound,ratio=ratio,reply=line))
counterexamples=json.loads((ROOT/'fixtures/synthetic/s5i2/macro_transform_counterexamples.json').read_text())
by_input={r['input']:r for r in rows};regressions=[by_input[s] for s in counterexamples]
assert len(regressions)==32
summary=dict(oracle='Decimal80 exact input f64 lens/first moments; Machin pi/atan and independent Taylor sin/cos affine transform; no product geometry math',cases=len(cases),baseline_cases=900,ready=ready,baseline_ready=baseline_ready,rejected=rejected,practical_cases=sum(practical),practical_ready=practical_ready,max_error_bound_ratio=maxratio,failures=fail,original_32_replayed=regressions)
(root/'macro-oracle.json').write_text(json.dumps(summary,indent=2)+'\n')
print(json.dumps({k:v for k,v in summary.items() if k not in ('failures','original_32_replayed')},indent=2));print('FAILURES',len(fail));print(json.dumps(fail[:8],indent=2))
assert practical_ready==sum(practical),'ordinary practical transforms must remain Ready'
assert not fail,'successful transformed error bound does not enclose independent truth'
