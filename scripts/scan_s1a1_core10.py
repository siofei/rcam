"""Explicit local CORE10 audit. Private paths/logs remain under the given evidence directory."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
from check_s1a_reference import read_png


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--manifest', required=True, type=Path)
    parser.add_argument('--out', required=True, type=Path)
    args = parser.parse_args()
    args.out = args.out.resolve()
    args.out.mkdir(parents=True, exist_ok=False)
    manifest = json.loads(args.manifest.read_text())
    sources = {s['core_id']: (Path(manifest['root'])/s['path'], s['sha256'])
               for s in manifest['samples'] if s.get('core_id')}
    if set(sources) != {f'CORE-{n:02}' for n in range(1, 11)}:
        raise ValueError('frozen CORE10 identities missing or duplicated')
    before = {key: sha(path) for key, (path, _) in sources.items()}
    if any(before[key] != expected for key, (_, expected) in sources.items()):
        raise ValueError('source hash differs from frozen manifest')
    env = dict(os.environ, RCAM_CORE10_MANIFEST=str(args.manifest.resolve()),
               RCAM_CORE10_RESULTS=str(args.out/'raw-results.json'),
               RCAM_CORE10_EXPORT_DIR=str(args.out/'normalized'))
    command = ['cargo', 'test', '--locked', '-p', 'editor-service', '--test', 's1a_semantic_truth',
               'frozen_core10_semantic_scan', '--', '--ignored', '--exact', '--nocapture']
    with (args.out/'cargo.log').open('w') as log:
        result = subprocess.run(command, env=env, stdout=log, stderr=subprocess.STDOUT)
    after = {key: sha(path) for key, (path, _) in sources.items()}
    if after != before:
        raise ValueError('source changed during scan')
    hashes = [{'core_id':key, 'expected_sha256':expected, 'before_sha256':before[key],
               'after_sha256':after[key], 'unchanged':before[key] == after[key] == expected}
              for key, (_, expected) in sources.items()]
    (args.out/'source-hashes.json').write_text(json.dumps(hashes, indent=2))
    raw = json.loads((args.out/'raw-results.json').read_text())
    previous = {'CORE-03':'invalid metadata date', 'CORE-06':'G75 radius/sweep consistency',
                'CORE-07':'coordinate field exceeds FS width', 'CORE-08':'no G74 center',
                'CORE-09':'G75 radius/sweep consistency'}
    rows = []
    for row in raw['results']:
        key = row['core_id']
        text = row.get('diagnostic', '')
        category = ('invalid metadata date' if key == 'CORE-03' else
                    'coordinate field exceeds FS width' if key == 'CORE-07' else
                    'Region topology rejected' if 'region contour' in text.lower() else
                    'Region uncertainty envelope rejected' if 'uncertainty envelope' in text else
                    'Region circular interpretation not supported' if 'Region' in text else
                    'other refusal; inspect private log' if text else None)
        statistics = row.get('arc_deviation')
        if not statistics and 'ArcDeviationSummary' in text:
            fields = re.findall(r'(count|single_quadrant_count|zero_sweep_count|above_roundoff_count|max_deviation_mm): ([0-9.eE+-]+)', text)
            statistics = {k: float(v) if k.endswith('_mm') else int(v) for k, v in fields}
        rows.append({'core_id':key, 'original_first_failure':previous.get(key),
                     'semantic_status':row['semantic_status'], 'current_failure_category':category,
                     'objects':row.get('objects'), 'arc_deviation':statistics,
                     'unedited_roundtrip':row.get('unedited_roundtrip', 'not_executed'),
                     'edited_roundtrip':'not_executed', 'hashes':next(h for h in hashes if h['core_id']==key)})
    reference = []
    binary = shutil.which('gerbv')
    for key in ['CORE-06', 'CORE-08', 'CORE-09']:
        item = {'core_id':key, 'method':'local gerbv 1024x1024 autoscale source/output raster supplement',
                'full_independent_manufacturing_geometry':'not_verified'}
        pixels = []
        if binary:
            for kind, path in [('source', sources[key][0]), ('output', args.out/'normalized'/f'{key}.gbr')]:
                if not path.exists():
                    item[kind] = 'not_available'
                    continue
                image = args.out/f'{key}-{kind}.png'
                run = subprocess.run([binary, '-x', 'png', '-w', '1024x1024', '-B', '5',
                                      '-b', '#000000', '-f', '#FFFFFF', '-o', str(image), str(path)],
                                     capture_output=True, text=True, timeout=60)
                (args.out/f'{key}-{kind}.log').write_text(run.stdout+run.stderr)
                item[kind] = {'exit_code':run.returncode, 'file_sha256':sha(path),
                              'has_diagnostics':bool(run.stdout.strip() or run.stderr.strip())}
                if run.returncode == 0 and image.exists():
                    width, height, channels, data = read_png(image)
                    mask = bytes(max(r[x:x+3]) > 127 for r in data for x in range(0,len(r),channels))
                    pixels.append((width,height,mask))
                    item[kind]['foreground_pixels'] = sum(mask)
            if len(pixels) == 2 and pixels[0][:2] == pixels[1][:2]:
                differences = sum(a != b for a,b in zip(pixels[0][2],pixels[1][2]))
                item['different_pixels'] = differences
                item['same_sampled_raster'] = differences == 0
        else:
            item['status'] = 'blocked: local gerbv missing'
        reference.append(item)
    report = {'schema_version':2, 'kind':'semantic_and_unedited_roundtrip_not_edit_acceptance',
              'cargo_exit_code':result.returncode, 'command':' '.join(command),
              'all_ten_semantic_gate':'passed' if result.returncode == 0 else 'failed',
              'results':rows, 'reference':reference}
    (args.out/'core10-redacted-summary.json').write_text(json.dumps(report,indent=2))
    # This audit does not turn the existing failing all-ten readiness test green.
    print(f'CORE10 scan complete: {sum(r["semantic_status"]=="passed" for r in rows)}/10 semantic; cargo exit {result.returncode}')
    return 0 if result.returncode == 0 else 1


if __name__ == '__main__':
    raise SystemExit(main())
