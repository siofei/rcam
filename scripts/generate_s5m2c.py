"""Frozen PMIX v1 input/protocol. Refuse changed existing fixture bytes."""
from pathlib import Path
import hashlib
import json
import math

ROOT = Path(__file__).resolve().parents[1]
OUT = ROOT/'fixtures/synthetic/s5m2c'
CONVEX = [(320,0),(296,122),(226,226),(122,296),(0,320),(-122,296),(-226,226),(-296,122),(-320,0),(-296,-122),(-226,-226),(-122,-296),(0,-320),(122,-296),(226,-226),(296,-122)]
CONCAVE = [(-300,-300),(-100,-300),(-100,-200),(100,-200),(100,-300),(300,-300),(300,300),(100,300),(100,200),(-100,200),(-100,300),(-300,300),(-300,100),(-200,100),(-200,-100),(-300,-100)]
HEADER = '%FSLAX46Y46*%\n%MOMM*%\n%ADD10C,0.5*%\n%ADD11R,0.6X0.4*%\n%ADD12O,0.6X0.3*%\n%ADD13P,0.6X6X0*%\n%ADD14C,0.1*%\nG75*\n%LPD*%\n'


def put(path, data):
    if path.exists() and path.read_bytes() != data:
        raise ValueError('frozen bytes differ; preserve and investigate: '+str(path))
    path.write_bytes(data)


def encoded(value):
    return (json.dumps(value, ensure_ascii=False, sort_keys=True, indent=2)+'\n').encode()


def cell(i, x, y):
    """x/y integer micrometers; serialized FS4.6 integers are nanometers."""
    x *= 1000
    y *= 1000
    kind = i % 10
    if kind < 4:
        return f'D{10+kind}*\nX{x}Y{y}D03*\n'
    if kind < 7:
        dx,dy = [(300000,0),(0,300000),(250000,200000)][kind-4]
        return f'D14*\nG01*\nX{x-dx}Y{y-dy}D02*\nX{x+dx}Y{y+dy}D01*\n'
    if kind < 9:
        return f'D14*\nX{x+300000}Y{y}D02*\nG0{3 if kind==7 else 2}X{x}Y{y+300000}I-300000J0D01*\n'
    points = CONVEX if (i//10)%2==0 else CONCAVE
    points = [(x+a*1000,y+b*1000) for a,b in points]
    return 'G01*\nG36*\n'+f'X{points[0][0]}Y{points[0][1]}D02*\n'+''.join(f'X{a}Y{b}D01*\n' for a,b in points[1:]+points[:1])+'G37*\n'


def generate():
    OUT.mkdir(parents=True,exist_ok=True)
    pmix=(HEADER+''.join(cell(i,(i%400+1)*1000,(i//400+1)*1000) for i in range(100000))+'M02*\n').encode()
    # These two small files only seed the additional project workflow.
    extra='%ADD15R,4X4*%\n%ADD16C,3*%\n%ADD17C,1*%\n%ADD18C,3X1*%\nD15*\nX10000000Y12000000D03*\n%LPC*%\nD16*\nX10000000Y12000000D03*\n%LPD*%\nD17*\nX10000000Y12000000D03*\nD14*\nG01*\nX17000000Y12000000D02*\nX23000000Y12000000D01*\nD18*\nX20000000Y12000000D03*\n'
    base=(HEADER+''.join(cell(i,(i+1)*3000,3000) for i in range(10))+extra+'M02*\n').encode()
    upper=(HEADER+'%ADD15R,4X4*%\nD15*\nX45000000Y12000000D03*\n%LPC*%\nX10000000Y12000000D03*\nM02*\n').encode()
    fixtures=[]
    for name,data in [('PMIX.gbr',pmix),('MIX_BASE.gbr',base),('MIX_UPPER.gbr',upper)]:
        put(OUT/name,data)
        fixtures.append({'path':'fixtures/synthetic/s5m2c/'+name,'sha256':hashlib.sha256(data).hexdigest(),'bytes':len(data)})
    points=[]
    for i in range(200):
        ordinal=(i*137%10000)*10+i%10+1
        x=(ordinal-1)%400+1.;y=(ordinal-1)//400+1.;kind=(ordinal-1)%10
        inside=i<150
        if inside:
            if kind==7:x+=.3/math.sqrt(2);y+=.3/math.sqrt(2)
            elif kind==8:x-=.3
        elif kind not in (7,8):x+=.45;y+=.45
        points.append({'ordinal':ordinal,'position_mm':[x,y],'expected_id':f'object-{ordinal}' if inside else None,'kind_slot':kind})
    selected=[row*400+col+1 for row in range(50) for col in range(20)]
    protocol={'schema_version':2,'stage':'S5-M2-C','version':1,'generator':'generate_s5m2c.py v1; deterministic integer arithmetic, no PRNG',
        'fixture':fixtures[0], 'objects':100000,'counts':{'flash':40000,'line':30000,'arc':20000,'region':10000},'flash_shapes':{'circle':10000,'rectangle':10000,'obround':10000,'polygon':10000},'region_edges':160000,'region_templates':{'convex':5000,'concave':5000},'bounds_mm':[.75,.65,400.3,250.35],
        'canvas_physical':[1600,900],'reference_hz':60,'camera':{'center_mm':[200.5,125.5],'logical_scale':1.656},'warm_seconds':10,'navigation_seconds':60,'repeats':3,
        'trajectory':{'pan_x_mm':80,'pan_x_seconds':11,'pan_y_mm':8,'pan_y_seconds':7,'zoom_base':1.05,'zoom_amplitude':.35,'zoom_seconds':17},
        'drag':{'seconds':10,'selected':1000,'rect_mm':[.5,.5,20.5,50.5],'ordinals':selected,'type_counts':{'flash':400,'line':300,'arc':200,'region':100},'press_mm':[1,1],'pointer_delta_logical':'(36*t,-18*t+8*sin(2*pi*t)); t normalized [0,1]','snap':False},
        'points':points,'marquee':{'rect_mm':[.5,.5,400.5,250.5],'count':100000},
        'budgets':{'frame_p95_ms':50,'frame_p99_ms':100,'frame_max_ms':200,'input_gpu_p95_ms':100,'commit_ms':300,'undo_ms':300,'redo_ms':300,'point_cpu_p95_ms':20,'point_highlight_p95_ms':100,'box_ready_ms':300,'rss_bytes':1073741824,'custom_gpu_bytes':536870912},
        'measurement':'All input frames and GPU callbacks, actual shared clocks; synthetic input/GPU completion upper bounds, no scanout claim'}
    put(OUT/'protocol.json',encoded(protocol))
    recipe={'schema_version':2,'source_files':[x for x in fixtures if not x['path'].endswith('/PMIX.gbr')], 'first_layer_source_objects':15,'second_layer_source_objects':2,'block_source_ordinals':list(range(1,11)),'block_origin_mm':[0,0],'instances':[{'translation_mm':[40,0],'rotation_deg':0,'mirror':False},{'translation_mm':[80,0],'rotation_deg':37,'mirror':False},{'translation_mm':[120,0],'rotation_deg':90,'mirror':True}], 'text':{'font':'builtin:rcam-stroke-v1','string':'PMIX 8B','height_mm':3,'stroke_width_mm':.15,'position_mm':[0,18]}, 'canonical_fixture_metadata':{'project_id':'project-0000000000000000000000000000c053','imported_at':'2000-01-01T00:00:00Z'},'geometry_source':'real ApplicationService actions; only detached fixture project ID/import timestamps canonicalized','scope':'bounded two-layer correctness/workflow, not PPOL or performance replacement'}
    put(OUT/'workflow-recipe.json',encoded(recipe))
    manifest={'schema_version':2,'generator_version':1,'fixtures':fixtures,'protocol_sha256':hashlib.sha256(encoded(protocol)).hexdigest(),'recipe_sha256':hashlib.sha256(encoded(recipe)).hexdigest(),'region_offset_units':'micrometers','region_convex_offsets':CONVEX,'region_concave_offsets':CONCAVE}
    put(OUT/'manifest.json',encoded(manifest))
    print(json.dumps({'fixtures':fixtures,'protocol_sha256':manifest['protocol_sha256']},indent=2))


if __name__=='__main__':
    generate()
