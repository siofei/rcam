"""Frozen S5-M1 public synthetic fixture/protocol v1; preserve historical samples."""
from pathlib import Path
import hashlib,json
ROOT=Path(__file__).resolve().parents[1]
OUT=ROOT/'fixtures/synthetic/s5m1'
P100K=ROOT/'fixtures/synthetic/s2b3_2/P100K_CIRCLES.gbr'
P100K_SHA='8111ecada7a66defe30f614cd3851a328d861bbab4c595cb12ee3131fa465a31'

def write(path,data):
    if path.exists() and path.read_bytes()!=data:
        raise ValueError('frozen fixture differs; do not overwrite '+path.name)
    path.write_bytes(data)

def generate():
    assert hashlib.sha256(P100K.read_bytes()).hexdigest()==P100K_SHA
    OUT.mkdir(parents=True,exist_ok=True)
    header='%FSLAX46Y46*%\n%MOMM*%\n%ADD10C,0.5*%\n%ADD11R,0.5X0.4*%\n%ADD12O,0.6X0.4*%\n%ADD13P,0.5X6X0*%\n'
    body=[]
    for i in range(10000):
        body.extend([f'D{10+i%4}*\n',f'X{(i%100+1)*1000000}Y{(i//100+1)*1000000}D03*\n'])
    data=(header+''.join(body)+'M02*\n').encode()
    write(OUT/'P10K_CROP.gbr',data)
    points=[dict(ordinal=(i*37%100)*1000+(i*499%1000)+1,x_mm=i*499%1000+1,y_mm=i*37%100+1,expected_id_suffix='object-'+str((i*37%100)*1000+(i*499%1000)+1)) for i in range(200)]
    assert len({p['ordinal'] for p in points})==200
    protocol=dict(schema_version=2,protocol_version=1,seed='none; deterministic arithmetic',canvas_physical=[1600,900],display_hz=60,warm_seconds=10,navigation_seconds=60,navigation_repeats=3,trajectory=dict(kind='sinusoidal absolute target through normal scroll/pan and zoom events',pan_x_mm_amplitude=80,pan_x_seconds=11,pan_y_mm_amplitude=8,pan_y_seconds=7,zoom_base=1.05,zoom_amplitude=.35,zoom_seconds=17),point_positions=points,point_end_to_end_origin='app receives injected egui Pointer events; worker result must be consumed and production highlight drawn; GPU complete conservative bound, no physical-input claim',full_marquee=dict(rect_mm=[-40,-20,1040,120],expected_count=100000,expected_order='source row-major object-1..object-100000, source namespace from import ID only',result_ready_ms_max=300),lifecycle=dict(rounds=20,lod_multipliers=[1,2,4,1],close_idle_seconds=5,rss_growth_bytes_max=100*1024**2,process_peak_bytes_max=1024**3,gpu_explicit_bytes_max=512*1024**2),idle_seconds=60,percentile='nearest rank ceil(n*q)-1; all active frames retained',load=dict(cold='new app process/no app caches; OS page cache uncontrolled, not disk cold',warm='close/reopen same process',repeats=3,p10k_max_seconds=3,p100k_max_seconds=10))
    write(OUT/'protocol.json',(json.dumps(protocol,ensure_ascii=False,indent=2,sort_keys=True)+'\n').encode())
    manifest=dict(schema_version=2,generator='scripts/generate_s5m1.py v1',generator_sha256=hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),fixtures=[dict(path='fixtures/synthetic/s5m1/P10K_CROP.gbr',sha256=hashlib.sha256(data).hexdigest(),objects=10000,shapes=dict(C=2500,R=2500,O=2500,P=2500),apertures=4,exposure='all Dark, source row-major order',bounds_mm=[.75,.75,100.25,100.25]),dict(path=P100K.relative_to(ROOT).as_posix(),sha256=P100K_SHA,objects=100000,shapes=dict(C=100000),apertures=1,exposure='all Dark, source row-major order',bounds_mm=[.75,.75,1000.25,100.25])],protocol_sha256=hashlib.sha256((OUT/'protocol.json').read_bytes()).hexdigest(),independent_truth='integer source position arithmetic and analytic aperture extents; IDs verified in source exposure order, no renderer output as truth')
    write(OUT/'manifest.json',(json.dumps(manifest,ensure_ascii=False,indent=2,sort_keys=True)+'\n').encode())
    print(json.dumps(manifest,ensure_ascii=False))
if __name__=='__main__':generate()
