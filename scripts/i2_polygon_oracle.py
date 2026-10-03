# Independent analytic oracle retained from I2-A independent review; test-only.
import subprocess, random, json, math
from fractions import Fraction as F
from decimal import Decimal as D, getcontext
from pathlib import Path
getcontext().prec=90
import sys
root=Path(sys.argv[1])
def dec(x): return D(x.numerator)/D(x.denominator) if isinstance(x,F) else D.from_float(x)
def cross(a,b): return a[0]*b[1]-a[1]*b[0]
def sub(a,b): return (a[0]-b[0],a[1]-b[1])
def canon(p):
 p=[(F(x),F(y)) for x,y in p]
 if sum(cross(a,b) for a,b in zip(p,p[1:]+p[:1]))<0: p.reverse()
 return p
def clip(poly,bounds):
 for a,b in zip(bounds,bounds[1:]+bounds[:1]):
  result=[]; v=sub(b,a)
  for p,q in zip(poly,poly[1:]+poly[:1]):
   fp=cross(v,sub(p,a));fq=cross(v,sub(q,a))
   if fp>=0:result.append(p)
   if (fp<0)!=(fq<0):
    t=fp/(fp-fq);result.append((p[0]+t*(q[0]-p[0]),p[1]+t*(q[1]-p[1])))
  poly=result
  if not poly:break
 return poly
def props(p):
 if not p:return F(0),F(0),F(0),D(0)
 z=[cross(a,b) for a,b in zip(p,p[1:]+p[:1])]
 area=sum(z)/2
 mx=sum((a[0]+b[0])*v for a,b,v in zip(p,p[1:]+p[:1],z))/6
 my=sum((a[1]+b[1])*v for a,b,v in zip(p,p[1:]+p[:1],z))/6
 perimeter=sum((dec((b[0]-a[0])**2+(b[1]-a[1])**2)).sqrt() for a,b in zip(p,p[1:]+p[:1]))
 return area,mx,my,perimeter
def para(x,y,w,h,slope,other):
 u=(w,w*slope);v=(h*other,h)
 return [(x,y),(x+u[0],y+u[1]),(x+u[0]+v[0],y+u[1]+v[1]),(x+v[0],y+v[1])]
cases=[];rng=random.Random(5152)
for i in range(1200):
 t=[0,10,1e3,1e6][i%4]; scale=[.001,1,10][i%3]
 s=rng.uniform(-.3,.3);r=rng.uniform(-.3,.3)
 p=para(t,t,scale*rng.uniform(.5,3),scale*rng.uniform(.5,3),s,r)
 q=para(t+scale*rng.uniform(-.5,2),t+scale*rng.uniform(-.5,2),scale*rng.uniform(.5,3),scale*rng.uniform(.5,3),s+rng.uniform(-.2,.2),r+rng.uniform(-.2,.2))
 if i%7==0:p=[(-x,y) for x,y in p];q=[(-x,y) for x,y in q]
 cases.append((f'random-{i}',p,q))
for slope in [0.1,1,2,10,100]:
 for angle in [1e-4,1e-6,1e-8,1e-10,1e-12,1e-14]:
  for size in [1,100,10000]:
   for offset in [0,1000,1e6]:
    p=para(offset,offset,size,1,slope,0)
    q=para(offset+size*.1,offset+size*slope*.1+0.3,size,1,slope+angle,0)
    cases.append((f'nearparallel-{slope}-{angle}-{size}-{offset}',p,q))
inputs=[];truth=[]
for name,p,q in cases:
 p=canon(p);q=canon(q);z=clip(p,q)
 a,x,y,per=props(p);b,u,v,p2=props(q);c,j,k,pc=props(z)
 area=a+b-c;truth.append((name,area,(x+u-j)/area,(y+v-k)/area,per+p2-pc))
 inputs.append('2 D '+str(len(p))+' '+' '.join(repr(float(x))+' '+repr(float(y)) for x,y in p)+' D '+str(len(q))+' '+' '.join(repr(float(x))+' '+repr(float(y)) for x,y in q))
(root/'polygon-input.txt').write_text('\n'.join(inputs)+'\n')
r=subprocess.run([str(root/'probe')],input=('\n'.join(inputs)+'\n').encode(),stdout=subprocess.PIPE,stderr=subprocess.PIPE)
(root/'polygon-results.txt').write_bytes(r.stdout);assert r.returncode==0,r.stderr
assert len(r.stdout.decode().splitlines())==len(cases), 'missing probe rows'
ready=0;errs={};fail=[];margins=[]
for i,(line,t) in enumerate(zip(r.stdout.decode().splitlines(),truth)):
 if not line.startswith('READY '):errs[line]=errs.get(line,0)+1;continue
 ready+=1
 a,p,x,y,ce,ae,pe=map(float,line.split()[1:]);name,ta,tx,ty,tp=t
 for kind,value,oracle,bound in [('area',a,dec(ta),ae),('perimeter',p,tp,pe),('centroid_x',x,dec(tx),ce),('centroid_y',y,dec(ty),ce)]:
  error=abs(dec(value)-oracle);margin=error/dec(bound) if bound else D('Infinity')
  margins.append((float(margin),name,kind))
  if error>dec(bound):fail.append({'index':i,'case':name,'kind':kind,'actual':value,'oracle':str(oracle),'error':str(error),'bound':bound,'ratio':float(margin),'input':inputs[i]})
result={'oracle':'Exact binary-input rational Sutherland-Hodgman clipping; exact Green polygon moments; Decimal90 square-root perimeter. No product hit-test/math used.','case_count':len(cases),'ready':ready,'rejected':errs,'failures':fail,'largest_error_bound_ratios':sorted(margins,reverse=True)[:15],'probe_exit':r.returncode}
(root/'polygon-oracle.json').write_text(json.dumps(result,indent=2));print(json.dumps({k:v for k,v in result.items() if k!='failures'},indent=2));print('FAILURES',len(fail));print(json.dumps(fail[:5],indent=2))

assert len(cases)==1470 and ready>=1200, 'practical polygon admission regression'
assert not fail, 'successful bound does not contain independent truth'
