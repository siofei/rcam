"""Check public S1-A physical coverage points with local gerbv, never upload.

PNG sampling supplements exact manufacturing assertions; it is not a writer.
The small PNG reader handles the noninterlaced 8-bit RGB/RGBA files gerbv emits.
"""
import argparse
import hashlib
import json
from pathlib import Path
import shutil
import struct
import subprocess
import zlib


def read_png(path):
    data = path.read_bytes()
    if data[:8] != b'\x89PNG\r\n\x1a\n':
        raise ValueError('not PNG')
    offset, packed = 8, bytearray()
    while offset < len(data):
        size = struct.unpack('>I', data[offset:offset+4])[0]
        kind, chunk = data[offset+4:offset+8], data[offset+8:offset+8+size]
        if kind == b'IHDR':
            width, height, depth, color, compression, filtering, interlace = struct.unpack('>IIBBBBB', chunk)
            if (depth, color, compression, filtering, interlace) not in [(8, 2, 0, 0, 0), (8, 6, 0, 0, 0)]:
                raise ValueError('unsupported PNG layout')
        if kind == b'IDAT':
            packed.extend(chunk)
        offset += size + 12
    channels = 3 if color == 2 else 4
    stride, raw = width * channels, zlib.decompress(packed)
    if len(raw) != height * (stride + 1):
        raise ValueError('invalid PNG size')
    rows, previous = [], bytearray(stride)
    for y in range(height):
        start = y * (stride + 1)
        filter_type, row = raw[start], bytearray(raw[start+1:start+1+stride])
        for x in range(stride):
            left = row[x-channels] if x >= channels else 0
            up, corner = previous[x], previous[x-channels] if x >= channels else 0
            if filter_type == 0:
                prediction = 0
            elif filter_type == 1:
                prediction = left
            elif filter_type == 2:
                prediction = up
            elif filter_type == 3:
                prediction = (left + up) // 2
            elif filter_type == 4:
                p = left + up - corner
                distances = [abs(p-left), abs(p-up), abs(p-corner)]
                prediction = [left, up, corner][distances.index(min(distances))]
            else:
                raise ValueError('unsupported PNG filter')
            row[x] = (row[x] + prediction) & 255
        rows.append(row)
        previous = row
    return width, height, channels, rows


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--out', type=Path, required=True)
    parser.add_argument('--normalized', type=Path)
    args = parser.parse_args()
    args.out.mkdir(parents=True, exist_ok=False)
    binary = shutil.which('gerbv')
    if not binary:
        raise SystemExit('BLOCKED: local gerbv missing')
    manifest = json.loads(Path('fixtures/synthetic/s1a/manifest.json').read_text())
    results = []
    for case in manifest['cases']:
        if not case['accept'] or not case['coverage_mm']:
            continue
        source = (args.normalized / (case['name'] + '.gbr')) if args.normalized else Path(case['path'])
        output = args.out / (case['name'] + '.png')
        command = [binary, '-x', 'png', '-D', '600', '-O', '-0.4x-0.4', '-W', '1.2x1.2',
                   '-B', '0', '-b', '#000000', '-f', '#FFFFFF', '-o', str(output), str(source)]
        run = subprocess.run(command, capture_output=True, text=True, timeout=30)
        (args.out / (case['name'] + '.log')).write_text(json.dumps(command) + '\n' + run.stdout + run.stderr)
        item = {'case': case['name'], 'exit_code': run.returncode, 'diagnostics': run.stderr,
                'input_sha256': hashlib.sha256(source.read_bytes()).hexdigest(), 'points': []}
        if run.returncode:
            item['status'] = 'failed'
        else:
            width, height, channels, rows = read_png(output)
            for x, y, expected in case['coverage_mm']:
                px, py = int((x/25.4 + .4) * 600), height - 1 - int((y/25.4 + .4) * 600)
                if not (0 <= px < width and 0 <= py < height):
                    raise ValueError('probe outside fixed window')
                actual = max(rows[py][px*channels:px*channels+3]) > 127
                item['points'].append(dict(x_mm=x, y_mm=y, expected=expected, actual=actual, passed=actual == expected))
            item['status'] = 'passed' if all(p['passed'] for p in item['points']) else 'failed'
        results.append(item)
        print(item['case'], item['status'], flush=True)
    version = subprocess.run([binary, '--version'], capture_output=True, text=True)
    report = {'schema_version': 2, 'kind': 'independent_raster_probes_not_manufacturing_geometry',
              'binary': str(Path(binary).resolve()), 'version': version.stdout + version.stderr,
              'binary_sha256': hashlib.sha256(Path(binary).read_bytes()).hexdigest(),
              'dpi': 600, 'origin_inches': [-.4, -.4], 'window_inches': [1.2, 1.2], 'results': results}
    (args.out/'results.json').write_text(json.dumps(report, indent=2))
    return int(any(r['status'] != 'passed' for r in results))


if __name__ == '__main__':
    raise SystemExit(main())
