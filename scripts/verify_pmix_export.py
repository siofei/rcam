"""Independent decoder of the bounded normalized PMIX export subset only.

No product parser/writer, UI, subprocess or external dependency. Unknown commands
are rejected. Compare every flattened manufacturing primitive, aperture hole,
exposure and f64 coordinate using the frozen project's quantization policy.
This is evidence tooling, not general Gerber/CircuitCAM compatibility support.
"""
import copy
import math
import re
from pathlib import Path


def require(ok, message):
    if not ok:
        raise ValueError(message)


def decode(path):
    data = Path(path).read_bytes()
    require(0 < len(data) <= 8*1024*1024, 'export byte budget')
    lines = data.decode('ascii').splitlines()
    require(lines[:3] == ['G04 RCam normalized S1 output*', '%FSLAX66Y66*%', '%MOMM*%']
            and lines[-1:] == ['M02*'], 'normalized export framing')
    apertures = {}
    result = []
    current = None
    aperture = None
    exposure = 'dark'
    transform = {'mirror': 'None', 'rotation_deg': 0., 'scale': 1.}
    region = None
    multiquadrant = False
    for line in lines[3:-1]:
        match = re.fullmatch(r'%ADD(\d+)([CROP]),([\d.X-]+)\*%', line)
        if match:
            code, kind, raw = match.groups()
            values = [float(x) for x in raw.split('X')]
            require(code not in apertures and all(math.isfinite(x) for x in values), 'aperture definition')
            require(len(values) in ({'C': (1, 2), 'R': (2, 3), 'O': (2, 3), 'P': (3, 4)}[kind]), 'aperture parameters')
            names = {'C': ('diameter_mm',), 'R': ('width_mm', 'height_mm'),
                     'O': ('width_mm', 'height_mm'), 'P': ('diameter_mm', 'vertices', 'rotation_deg')}[kind]
            shape = dict(zip(names, values))
            shape['hole_diameter_mm'] = values[len(names)] if len(values) > len(names) else None
            if kind == 'P':
                require(shape['vertices'].is_integer(), 'polygon vertex count')
                shape['vertices'] = int(shape['vertices'])
            apertures[code] = {dict(C='Circle', R='Rectangle', O='Obround', P='Polygon')[kind]: shape}
            continue
        if re.fullmatch(r'D\d+\*', line):
            aperture = line[1:-1]
            require(aperture in apertures, 'undeclared aperture')
            continue
        if line in ('%LPC*%', '%LPD*%'):
            require(region is None, 'polarity inside Region')
            exposure = 'clear' if line == '%LPC*%' else 'dark'
            continue
        match = re.fullmatch(r'%LM(N|X|Y|XY)\*%', line)
        if match:
            require(region is None,'transform inside frozen Region')
            transform['mirror'] = {'N': 'None', 'X': 'X', 'Y': 'Y', 'XY': 'XY'}[match[1]]
            continue
        match = re.fullmatch(r'%L([RS])(-?\d+(?:\.\d+)?)\*%', line)
        if match:
            require(region is None and math.isfinite(float(match[2])),'invalid transform')
            transform['rotation_deg' if match[1] == 'R' else 'scale'] = float(match[2])
            continue
        if line == 'G75*':
            multiquadrant = True
            continue
        if line == 'G36*':
            require(region is None, 'nested Region')
            region = []
            continue
        if line == 'G37*':
            require(region and len(region) >= 4 and region[0] == region[-1], 'unclosed/empty Region')
            result.append({'exposure': exposure, 'geometry': {'Region': region[:-1]}})
            region = None
            continue
        match = re.fullmatch(r'(G0[123])?X(-?\d+)Y(-?\d+)(?:I(-?\d+)J(-?\d+))?D0([123])\*', line)
        require(match is not None, 'unknown/unsafe PMIX export command: '+line)
        interpolation, x, y, offset_x, offset_y, op = match.groups()
        end = [int(x)/1e6, int(y)/1e6]
        if op == '2':
            require(interpolation is None and offset_x is None, 'move interpolation')
            current = end
            if region is not None:
                require(not region, 'multiple Region contours outside frozen subset')
                region.append(end)
            continue
        require((aperture in apertures and region is None)
                or (op == '1' and region is not None), 'missing aperture')
        if op == '3':
            require(interpolation is None and offset_x is None and region is None, 'flash syntax')
            result.append({'exposure': exposure, 'geometry': {'Flash': {'center': end, 'shape': copy.deepcopy(apertures[aperture]), 'transform': copy.deepcopy(transform)}}})
        else:
            require(current is not None, 'draw without starting point')
            if region is not None:
                require(interpolation == 'G01' and offset_x is None, 'nonlinear frozen Region')
                region.append(end)
            else:
                shape = apertures[aperture]
                require(set(shape) == {'Circle'} and shape['Circle']['hole_diameter_mm'] is None, 'stroke aperture')
                width = shape['Circle']['diameter_mm']
                if interpolation == 'G01':
                    require(offset_x is None, 'line with center')
                    geometry = {'Line': {'start': current, 'end': end, 'width_mm': width}}
                else:
                    require(multiquadrant and interpolation in ('G02', 'G03') and offset_x is not None, 'arc syntax/quadrant mode')
                    geometry = {'Arc': {'start': current, 'end': end, 'center': [current[0]+int(offset_x)/1e6, current[1]+int(offset_y)/1e6], 'direction': 'Clockwise' if interpolation == 'G02' else 'CounterClockwise', 'width_mm': width}}
                result.append({'exposure': exposure, 'geometry': geometry})
        current = end
    require(region is None and result, 'incomplete/empty export')
    return result


def expected(snapshot, layer_id):
    apertures = {a['id']: a['shape'] for a in snapshot['apertures']}
    blocks = {b['id']: b for b in snapshot['block_definitions']}
    layer = next(l for l in snapshot['layers'] if l['id'] == layer_id)
    result = []
    def primitive(g, polarity, transform=None):
        def point(p):
            x, y = p['x_mm'], p['y_mm']
            if transform:
                if transform['mirror']: x = -x
                angle = math.radians(transform['rotation_deg'])
                x, y = math.cos(angle)*x-math.sin(angle)*y, math.sin(angle)*x+math.cos(angle)*y
                x += transform['translation']['x_mm']; y += transform['translation']['y_mm']
            return [x, y]
        kind = next(iter(g)); value = g[kind]
        if kind == 'Flash':
            local = copy.deepcopy(value['transform'])
            if transform:
                require(local == {'mirror': 'None', 'rotation_deg': 0., 'scale': 1.}, 'unfrozen local Flash transform')
                local.update(mirror='X' if transform['mirror'] else 'None', rotation_deg=transform['rotation_deg'] % 360)
            geometry = {'Flash': {'center': point(value['center']), 'shape': apertures[value['aperture_id']], 'transform': local}}
        elif kind == 'Line':
            geometry = {'Line': {'start': point(value['start']), 'end': point(value['end']), 'width_mm': value['width_mm']}}
        elif kind == 'Arc':
            arc = value['path']; direction = arc['direction']
            if transform and transform['mirror']: direction = 'Clockwise' if direction == 'CounterClockwise' else 'CounterClockwise'
            require(not arc['full_circle'], 'unfrozen full circle')
            geometry = {'Arc': {k: point(arc[k]) for k in ('start', 'end', 'center')}}
            geometry['Arc'].update(direction=direction, width_mm=value['width_mm'])
        elif kind == 'Region':
            contours = value['contours']; require(len(contours) == 1 and contours[0]['role'] == 'Solid', 'unfrozen Region topology')
            edges = contours[0]['edges']; require(all(set(e) == {'Line'} for e in edges), 'unfrozen Region curve')
            geometry = {'Region': [point(e['Line']['start']) for e in edges]}
        else:
            raise ValueError('unfrozen primitive '+kind)
        result.append({'exposure': polarity, 'geometry': geometry})
    for obj in layer['objects']:
        g = obj['geometry']
        if 'BlockInstance' in g:
            block = g['BlockInstance']; definition = blocks[block['definition_id']]
            for child in definition['objects']: primitive(child['geometry'], child['exposure'], block['transform'])
        else:
            primitive(g, obj['exposure'])
    return result


def verify_export(path, snapshot, layer_id, resolution):
    require(resolution == .0001, 'frozen export precision')
    observed, wanted = decode(path), expected(snapshot, layer_id)
    require(len(observed) == len(wanted), 'missing/extra flattened export primitive')
    tolerance = 1e-9
    def equal(a, b, field=None):
        if type(a) is bool or type(b) is bool:return type(a) is type(b) and a==b
        if type(a) in (int, float) and type(b) in (int, float):
            if field in ('vertices', 'scale', 'rotation_deg'):
                return type(a) is type(b) and a == b
            grid = math.copysign(math.floor(abs(b/resolution)+.5), b) * resolution
            return math.isfinite(a) and math.isfinite(b) and abs(a-grid) <= tolerance
        if isinstance(a, dict) and isinstance(b, dict): return set(a) == set(b) and all(equal(a[k], b[k], k) for k in a)
        if isinstance(a, list) and isinstance(b, list): return len(a) == len(b) and all(equal(x, y) for x, y in zip(a, b))
        return a == b
    for index, (a, b) in enumerate(zip(observed, wanted)):
        if 'Region' in a['geometry'] and 'Region' in b['geometry']:
            actual, target = a['geometry']['Region'], b['geometry']['Region']
            require(a['exposure'] == b['exposure'] and len(actual) == len(target) and any(equal(actual, seq[i:]+seq[:i]) for seq in (target, list(reversed(target))) for i in range(len(target))), 'Region geometry/exposure '+str(index))
        else:
            require(equal(a, b), 'export geometry/aperture/hole/exposure/transform '+str(index))
    return {'layer_id': layer_id, 'flattened_primitives': len(observed), 'coordinate_error_budget_mm': tolerance, 'all_primitives_checked': True}
