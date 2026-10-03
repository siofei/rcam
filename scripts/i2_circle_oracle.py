# Independent analytic oracle retained from I2-A independent review; test-only.
from decimal import Decimal as D,getcontext
from pathlib import Path
import json,subprocess,random,math
getcontext().prec=80
import sys
root=Path(sys.argv[1])
def atan(x):
 neg=x<0;x=abs(x)
 if x>1:return -(PI/2-atan(1/x)) if neg else PI/2-atan(1/x)
 mul=1
 while x>D('.1'):x=x/(1+(1+x*x).sqrt());mul*=2
 total=x;term=x;n=1
 while abs(term)>D('1e-78'):
  term*=-(x*x);total+=term/(2*n+1);n+=1
 return total*mul*(-1 if neg else 1)
PI=16*atan(D(1)/5)-4*atan(D(1)/239)
def acos(x):
 if x==0:return PI/2
 return atan((1-x*x).sqrt()/x)+(PI if x<0 else 0)
def oracle(r,s,p,q,clear):
 r,s=D.from_float(r),D.from_float(s);px,py=map(D.from_float,p);qx,qy=map(D.from_float,q)
 dx,dy=qx-px,qy-py;d=(dx*dx+dy*dy).sqrt();a1=PI*r*r;a2=PI*s*s
 if d>=r+s:return (a1,px,py,2*PI*r) if clear else (a1+a2,(a1*px+a2*qx)/(a1+a2),(a1*py+a2*qy)/(a1+a2),2*PI*(r+s))
 if d<=abs(r-s):
  if clear:
   if s>=r:return None
   a=a1-a2;return a,(a1*px-a2*qx)/a,(a1*py-a2*qy)/a,2*PI*(r+s)
  return (a1,px,py,2*PI*r) if r>=s else (a2,qx,qy,2*PI*s)
 x=(d*d+r*r-s*s)/(2*d);h=(r*r-x*x).sqrt();b=d-x
 alpha=acos(x/r);beta=acos(b/s)
 cap1=r*r*alpha-x*h;cap2=s*s*beta-b*h;inter=cap1+cap2
 if clear:a=a1-inter;mc=-d*cap2;per=2*PI*r-2*r*alpha+2*s*beta
 else:a=a1+a2-inter;mc=d*(a2-cap2);per=2*PI*(r+s)-2*(r*alpha+s*beta)
 return a,px+mc/a*dx/d,py+mc/a*dy/d,per
rng=random.Random(5212);cases=[]
for i in range(300):
 r=rng.uniform(.1,4);s=rng.uniform(.1,4);d=rng.uniform(abs(r-s)*1.01,r+s*.999);angle=rng.uniform(-math.pi,math.pi);offset=[0,1000,1e6][i%3]
 p=(offset,offset);q=(offset+d*math.cos(angle),offset+d*math.sin(angle));cases.append((f'normal-{i}',r,s,p,q,i%2==0))
for ratio in [.25,.5,1,2,4]:
 for near in [1e-2,1e-4,1e-6,1e-8,1e-10,1e-12,1e-14]:
  for angle in [0,.3,1.1]:
   for clear in [False,True]:
    for typ in ['outer','inner']:
     r=1.;s=ratio;d=(r+s)*(1-near) if typ=='outer' else abs(r-s)*(1+near)
     if not d:continue
     cases.append((f'{typ}-{ratio}-{near}-{angle}-{clear}',r,s,(0.,0.),(d*math.cos(angle),d*math.sin(angle)),clear))
baseline_count=len(cases)
# Exact f64 inputs across scales/translations, separated/contained/tangent and
# resolvable/uncertain overlaps. Acceptance does not demand numeric success for
# unprovable inputs, but every successful bound must enclose independent truth.
for scale in [.001,1.,10.]:
 for offset in [0.,1000.,1e6]:
  for ratio in [.25,1.,4.]:
   for angle in [0.,.3,1.1]:
    for kind,factor in [('separated',1.01),('tangent',1.),('overlap',.9),('near',1-1e-8),('nearer',1-1e-12),('contained',.5)]:
     r=scale;s=scale*ratio
     d=(abs(r-s)*factor if kind=='contained' else (r+s)*factor)
     for clear in [False,True]:
      cases.append((f'extended-{scale}-{offset}-{ratio}-{angle}-{kind}-{clear}',r,s,(offset,offset),(offset+d*math.cos(angle),offset+d*math.sin(angle)),clear))
for offset in [0.,1000.,1e6]:
 for thickness in [1e-3,1e-6,1e-9,1e-11]:
  cases.append((f'annulus-{offset}-{thickness}',1.,1.-thickness,(offset,0.),(offset,0.),True))
inputs=['C 2 D '+repr(r)+' '+repr(p[0])+' '+repr(p[1])+' '+('C' if clear else 'D')+' '+repr(s)+' '+repr(q[0])+' '+repr(q[1]) for _,r,s,p,q,clear in cases]
(root/'circle-input.txt').write_text('\n'.join(inputs)+'\n')
result=subprocess.run([str(root/'probe')],input=('\n'.join(inputs)+'\n').encode(),stdout=subprocess.PIPE,stderr=subprocess.PIPE);assert result.returncode==0,result.stderr
(root/'circle-results.txt').write_bytes(result.stdout);errs={};fail=[];margins=[];ready=0;zero=0;baseline_ready=0;case_rows=[]
assert len(result.stdout.decode().splitlines())==len(cases), 'missing probe rows'
for i,(line,c) in enumerate(zip(result.stdout.decode().splitlines(),cases)):
 name,r,s,p,q,clear=c;case_rows.append({'case':name,'input':inputs[i],'actual':line});t=oracle(r,s,p,q,clear)
 if line=='ZERO':
  zero+=1
  if t is not None:fail.append({'case':name,'actual':'ZERO','oracle':list(map(str,t)),'input':inputs[i]})
  continue
 if not line.startswith('READY '):errs[line]=errs.get(line,0)+1;continue
 ready+=1;baseline_ready+=int(i<baseline_count)
 if t is None:fail.append({'case':name,'actual':line,'oracle':'ZERO','input':inputs[i]});continue
 a,per,x,y,ce,ae,pe=map(float,line.split()[1:]);ta,tx,ty,tp=t
 for kind,value,truth,bound in [('area',a,ta,ae),('perimeter',per,tp,pe),('centroid_x',x,tx,ce),('centroid_y',y,ty,ce)]:
  error=abs(D.from_float(value)-truth);margin=error/D.from_float(bound) if bound else D('Infinity');margins.append((float(margin),name,kind))
  if error>D.from_float(bound):fail.append({'index':i,'case':name,'kind':kind,'actual':value,'oracle':str(truth),'error':str(error),'bound':bound,'ratio':float(margin),'input':inputs[i]})
summary={'oracle':'Decimal80 unequal-circle cap/lens formulas and first moments; Machin pi + range-reduced atan; exact input f64 as Decimal; no product geometry functions.','case_count':len(cases),'baseline_case_count':baseline_count,'baseline_ready':baseline_ready,'rows':case_rows,'ready':ready,'zero':zero,'rejected':errs,'failures':fail,'largest_error_bound_ratios':sorted(margins,reverse=True)[:10]}
(root/'circle-oracle.json').write_text(json.dumps(summary,indent=2));print(json.dumps({k:v for k,v in summary.items() if k not in ('failures','rows')},indent=2));print('FAILURES',len(fail));print(json.dumps(fail[:8],indent=2))

assert baseline_count==678 and baseline_ready>=500, 'practical baseline admission regression'
assert not fail, 'successful bound does not contain independent truth'
