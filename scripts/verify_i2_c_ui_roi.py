"""Read-only portable surface investigation oracle; never closes a user report.

100ms asynchronous surface sampling is bounded evidence, not compositor/video
or physical input evidence. Legitimate UI state changes are excluded explicitly.
"""
import argparse
from collections import Counter
import hashlib
import json
import math
from pathlib import Path
from verify_s5m2_evidence import load, safe


def need(value, message):
    if not value:
        raise ValueError(message)


def rows(path):
    need(path.stat().st_size <= 128*1024*1024, 'oversize JSONL producer')
    def pairs(items):
        result = {}
        for key, value in items:
            need(key not in result, 'duplicate JSONL key')
            result[key] = value
        return result
    def finite(value):
        if isinstance(value,dict):
            return all(finite(v) for v in value.values())
        if isinstance(value,list): return all(finite(v) for v in value)
        return not isinstance(value,(float,int)) or math.isfinite(value)
    result=[]
    for line in path.read_text().splitlines():
        need(len(line) <= 1024*1024, 'oversize JSONL row')
        value=json.loads(line,object_pairs_hook=pairs,
                         parse_constant=lambda v: (_ for _ in ()).throw(ValueError('nonfinite JSONL')))
        need(finite(value),'nonfinite JSONL number');result.append(value)
    return result


def ppm(data):
    header, dimensions, maximum, pixels = data.split(b'\n', 3)
    width, height = map(int, dimensions.split())
    need(header == b'P6' and maximum == b'255' and width > 0 and height > 0
         and len(pixels) == width * height * 3, 'lossless ROI shape/pixel count')
    return width, height, pixels


def intersects(a, b):
    return a[0] < b[2] and a[2] > b[0] and a[1] < b[3] and a[3] > b[1]


def foreground(pixels):
    """Physical pixel occupancy, independent of RGB hashes and ink totals."""
    return bytes(min(pixels[i:i+3]) < 140 for i in range(0, len(pixels), 3))


def ink_geometry(mask, width):
    points = [(n % width, n // width) for n, ink in enumerate(mask) if ink]
    if not points:
        return {'bbox_px': None, 'centroid_px': None}
    xs, ys = zip(*points)
    return {'bbox_px': [min(xs), min(ys), max(xs)+1, max(ys)+1],
            'centroid_px': [sum(xs)/len(xs), sum(ys)/len(ys)]}


def analyze(root, binary=None, manifest=None):
    root = Path(root)
    identity = load(root / 'identity.json')
    need(type(identity['schema_version']) is int and identity['schema_version'] == 2
         and identity['profile'] == 'release' and identity['user_flicker_report'] == 'OPEN'
         and identity['sample_interval_ms'] == 100 and identity['max_frames'] == 40000
         and identity['max_samples'] == 1200, 'ROI release/bounded scope weakened')
    if binary is not None:
        need(identity['binary_sha256'] == binary, 'ROI binary binding')
    if manifest is not None:
        need(identity['source_manifest_sha256'] == manifest, 'ROI source binding')
    frames = rows(root / 'frames.jsonl')
    paints = rows(root / 'paint.jsonl')
    samples = rows(root / 'samples.jsonl')
    need(frames and samples and paints, 'missing UI/surface/paint producers')
    need(len(frames) <= 40000 and len(samples) <= 1200, 'ROI bounds exceeded')
    indexed = {f['frame']: f for f in frames}
    need(len(indexed) == len(frames) and list(indexed) == list(range(1, len(frames) + 1)),
         'continuous UI frame producer coverage')
    last = 0
    for f in frames:
        need(type(f['frame']) is int and type(f['t_ns']) is int and f['t_ns'] > last
             and 0 <= f['input']['t_ns'] <= f['t_ns'], 'UI input/update clock')
        last = f['t_ns']
    for p in paints:
        need(p['frame'] in indexed and p['t_ns'] >= indexed[p['frame']]['t_ns'],
             'paint frame/update causality')
        v = p['viewport_px']; c = p['clip_px']
        expected = [max(v[0], c[0], 0), max(v[1], c[1], 0),
                    max(min(v[0] + v[2], c[0] + c[2]), 0),
                    max(min(v[1] + v[3], c[1] + c[3]), 0)]
        need(p['scissor_xyxy'] == expected, 'actual GPU scissor derivation')
        canvas = [round(x * indexed[p['frame']]['ppp'])
                  for x in indexed[p['frame']]['canvas']]
        need(all(abs(a-b) <= 1 for a,b in zip(expected, canvas)),
             'GPU callback escapes actual canvas')
    entries = []; groups = {}; outliers = []; previous_request = None
    for number, sample in enumerate(samples):
        q = sample['request']; end = sample['callback_last_ui']
        frame_snapshot = {k:v for k,v in q.items() if k != 'request_ns'}
        need(type(sample['sample']) is int and sample['sample'] == number
             and type(q['request_ns']) is int and frame_snapshot == indexed.get(q['frame'])
             and end == indexed.get(sample['callback_after_ui_frame']),
             'surface request/callback UI producer binding')
        need(q['t_ns'] <= q['request_ns'] <= sample['callback_ns']
             and q['t_ns'] <= end['t_ns'] <= sample['callback_ns'], 'surface clock causality')
        if previous_request is not None:
            need(q['request_ns'] - previous_request >= 100_000_000, 'surface sample interval weakened')
        previous_request = q['request_ns']
        need(len(sample['crops']) == len(q['menus']) == 8,
             'static menu ROI omitted')
        need({c['name'] for c in sample['crops']} == set(q['menus']), 'menu labels mismatch')
        for crop in sample['crops']:
            name = crop['name']; metadata = crop['metadata']; rect = crop['rect_px']
            need(metadata == q['menus'][name], 'ROI menu producer binding')
            data = safe(root, crop['path']).read_bytes()
            width, height, pixels = ppm(data)
            need(hashlib.sha256(data).hexdigest() == crop['sha256'], 'ROI pixel hash')
            expected = [max(round(x*q['ppp']), 0) for x in metadata['rect']]
            expected[2] = min(expected[2], sample['surface_size_px'][0])
            expected[3] = min(expected[3], sample['surface_size_px'][1])
            need(rect == expected and width == rect[2]-rect[0] and height == rect[3]-rect[1],
                 'actual menu crop spatial binding')
            for state in (q, end):
                canvas = [v * state['ppp'] for v in state['canvas']]
                need(not intersects(rect, canvas), 'menu ROI intersects canvas')
            # Same state over the entire asynchronous request/readback interval.
            interval = [indexed[n] for n in range(q['frame'], end['frame'] + 1)]
            stable = all(not f['popup'] and f['ppp'] == q['ppp']
                         and f['menus'][name] == metadata and metadata['enabled']
                         and not metadata['hovered'] for f in interval)
            mask = foreground(pixels)
            dark = sum(mask)
            mean = sum(pixels) / len(pixels)
            item = {'sample': number, 'name': name, 'rect_px': rect,
                    'sha256': crop['sha256'], 'dark_pixels': dark,
                    'mean_channel': mean, 'eligible_static': stable,
                    'request_frame': q['frame'], 'callback_frame': end['frame'],
                    'path': crop['path'], 'ppp': q['ppp'],
                    'foreground_geometry': ink_geometry(mask, width)}
            entries.append(item)
            if stable:
                groups.setdefault((name, tuple(rect), q['ppp']), []).append((item, mask))
    for (name, rect, ppp), observations in groups.items():
        items = [item for item, _ in observations]
        median = sorted(e['dark_pixels'] for e in items)[len(items)//2]
        # A modal observed pixel layout is a temporal reference, not a claim
        # about ideal font rendering. Ties still flag every differing layout.
        # Compare actual occupancy bytes at fixed physical coordinates: totals,
        # bbox/centroid, row/column sums or a new hash alone cannot detect every
        # equal-count glyph deformation. RGB variation with unchanged occupancy
        # is allowed; changed rect/DPI is a different recorded state group.
        reference = Counter(mask for _, mask in observations).most_common(1)[0][0]
        reference_sample = next(item['sample'] for item, mask in observations if mask == reference)
        for item, mask in observations:
            changed = sum(a != b for a, b in zip(mask, reference))
            reasons = []
            if (item['dark_pixels'] < 10 or item['dark_pixels'] < median*.75
                    or item['dark_pixels'] > median*1.25 or item['mean_channel'] < 100):
                reasons.append('ink count/intensity')
            if changed:
                reasons.append('foreground physical position/shape')
            if reasons:
                outliers.append(dict(item, median_dark=median, reasons=reasons,
                                     changed_foreground_pixels=changed,
                                     reference_sample=reference_sample,
                                     reference_geometry=ink_geometry(reference, rect[2]-rect[0])))
    labels = {}
    for name in sorted({k[0] for k in groups}):
        items = [e for k, es in groups.items() if k[0] == name for e, _ in es]
        labels[name] = {'eligible':len(items),
                        'distinct_pixel_hashes':len({e['sha256'] for e in items}),
                        'dark_min':min(e['dark_pixels'] for e in items),
                        'dark_max':max(e['dark_pixels'] for e in items)}
    need(len(labels) == 8 and all(v['eligible'] >= 10 for v in labels.values()),
         'insufficient stable static-menu evidence')
    return {'schema_version':2, 'identity':identity,
            'result':'ROI_OUTLIERS_REQUIRE_INSPECTION' if outliers else
                     'NOT_REPRODUCED_IN_MEASURED_SURFACE_SCOPE',
            'user_flicker_report':'OPEN', 'samples':len(samples), 'frames':len(frames),
            'paint_callbacks':len(paints), 'static_menu_labels':labels,
            'outliers':outliers, 'entries':entries,
            'position_shape_method':'exact threshold140 foreground occupancy at physical pixel coordinates; modal temporal reference within identical menu/rect/DPI stable state; RGB-only variation allowed; not absolute font/OCR truth',
            'boundary':'100ms asynchronous postpaint surface ROI; all intervening UI states checked; no physical input, OS compositor, scanout, continuous video or unknown user reproduction claim'}


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('directory', type=Path)
    p.add_argument('--out', required=True, type=Path)
    a = p.parse_args()
    report = analyze(a.directory)
    a.out.write_text(json.dumps(report, ensure_ascii=False, indent=2)+'\n')
    print(json.dumps({k:v for k,v in report.items()
                      if k not in ('identity','entries','outliers')}, ensure_ascii=False))
    return int(bool(report['outliers']))


if __name__ == '__main__':
    raise SystemExit(main())
