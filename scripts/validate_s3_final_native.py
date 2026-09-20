"""Validate the same-build macOS S3 native flow and P1K performance evidence."""
import argparse
import hashlib
import json
from pathlib import Path
from validate_s2c1_native import validate_tools


ROOT = Path(__file__).resolve().parents[1]


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def validate(root):
    result = validate_tools(root)
    tools = json.loads((root/'s2c1-tools.json').read_text())
    named = [row for row in tools['records'] if isinstance(row['phase'], str)]
    expected = [
        'window_selection', 'crossing_selection', 'duplicate', 'delete',
        'delete_undo_redo', 'rotate', 'mirror', 'flash_size_cow', 's3_save_reopen',
    ]
    assert [row['phase'] for row in named] == expected
    by_phase = {row['phase']: row for row in named}
    assert by_phase['window_selection']['selected'] == 1000
    assert by_phase['crossing_selection']['selected'] == 1000
    assert by_phase['duplicate']['scene_total'] == 1001
    assert by_phase['delete']['scene_total'] == 1000
    assert by_phase['delete_undo_redo']['scene_total'] == 1000
    assert by_phase['rotate']['coordinates_correct']
    assert by_phase['mirror']['coordinates_correct']
    assert by_phase['flash_size_cow']['cow_correct']
    assert by_phase['s3_save_reopen']['reopened_geometry_correct']
    source = ROOT/'fixtures/synthetic/s2b3_1/P1K_CIRCLES.gbr'
    hashes = {
        'input_p1k_sha256': digest(source),
        'baseline_sha256': digest(root/'baseline.gbr'),
        'single_snapped_sha256': digest(root/'single-snapped.gbr'),
        's3_final_sha256': digest(root/'s3-final.gbr'),
    }
    normalized = [root/'after-navigation.gbr', root/'after-tools.gbr'] + [
        root/f'after-undo-{index}.gbr' for index in range(3)
    ]
    assert all(digest(path) == hashes['baseline_sha256'] for path in normalized)
    result.update(
        status='PASS',
        stage='S3-FINAL',
        s3_native_phases=expected,
        input_output_hashes=hashes,
        normalized_writer_outputs_stable=True,
        windows='deferred / not executed',
    )
    return result


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('root', type=Path)
    parser.add_argument('--out', type=Path)
    args = parser.parse_args()
    data = json.dumps(validate(args.root.resolve()), indent=2, sort_keys=True)+'\n'
    if args.out:
        args.out.write_text(data, encoding='utf-8')
    print(data, end='')
