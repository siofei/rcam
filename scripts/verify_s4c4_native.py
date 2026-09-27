"""Verify captured synthetic native C4 before/after observations independently of service code."""
import argparse
import json
import math
from pathlib import Path


def verify(case):
    before, after = case['before'], case['after']
    assert before['document_id'] == after['document_id']
    assert before['fixture_sha256'] == after['fixture_sha256']
    assert before['block_definitions_sha256'] == after['block_definitions_sha256']
    old, new = before['object_bounds'], after['object_bounds']
    assert [(o['object_id'], o['index'], o['exposure']) for o in old] == [
        (o['object_id'], o['index'], o['exposure']) for o in new]
    old_by_id = {o['object_id']: o for o in old}
    new_by_id = {o['object_id']: o for o in new}
    selected = before['selected_object_ids']
    assert len(selected) == len(set(selected))
    assert len(selected) >= (2 if case['op'] == 'align' else 3)
    expected = {id_: [0.0, 0.0] for id_ in old_by_id}
    fixed = []
    gap = None
    if case['op'] == 'align':
        anchor = before['anchor']['object_id']
        assert anchor == selected[-1]
        assert anchor in selected
        fixed = [anchor]
        mode = case['mode']
        axis = 0 if mode in ['Left', 'Right', 'HCenter'] else 1
        def feature(bounds):
            if mode in ['HCenter', 'VCenter']:
                return bounds[axis] + (bounds[axis + 2] - bounds[axis]) / 2
            return bounds[axis + (2 if mode in ['Right', 'Top'] else 0)]
        target = feature(old_by_id[anchor]['bounds_mm'])
        for id_ in selected:
            expected[id_][axis] = target - feature(old_by_id[id_]['bounds_mm'])
    else:
        assert case['op'] == 'distribute'
        axis = 0 if case['axis'] == 'Horizontal' else 1
        order = sorted(selected, key=lambda id_: (
            old_by_id[id_]['bounds_mm'][axis], old_by_id[id_]['bounds_mm'][axis + 2],
            old_by_id[id_]['index']))
        fixed = [order[0], order[-1]]
        widths = [old_by_id[id_]['bounds_mm'][axis + 2] - old_by_id[id_]['bounds_mm'][axis] for id_ in order]
        first, last = old_by_id[order[0]]['bounds_mm'], old_by_id[order[-1]]['bounds_mm']
        gap = (last[axis + 2] - first[axis] - sum(widths)) / (len(order) - 1)
        cursor = first[axis + 2]
        for i, id_ in enumerate(order[1:-1], 1):
            target = cursor + gap
            expected[id_][axis] = target - old_by_id[id_]['bounds_mm'][axis]
            cursor = target + widths[i]
    for id_, object_ in old_by_id.items():
        actual = new_by_id[id_]
        delta = expected[id_]
        if id_ in fixed or id_ not in selected:
            assert actual['geometry_sha256'] == object_['geometry_sha256'], f'fixed object changed: {id_}'
        for coordinate in range(4):
            target = object_['bounds_mm'][coordinate] + delta[coordinate % 2]
            assert math.isclose(actual['bounds_mm'][coordinate], target, rel_tol=0, abs_tol=1e-8), (id_, coordinate, target, actual)
    changed = sum(o['geometry_sha256'] != new_by_id[o['object_id']]['geometry_sha256'] for o in old)
    step = int(changed > 0)
    assert int(after['revision']) == int(before['revision']) + step
    assert after['undo_entries'] == before['undo_entries'] + step
    if step:
        assert after['dirty']
        assert after['redo_entries'] == 0
    else:
        assert after['dirty'] == before['dirty']
        assert after['redo_entries'] == before['redo_entries']
    return dict(label=case['label'], status='PASS', op=case['op'], mode=case.get('mode'),
                axis=case.get('axis'), selected=len(selected), changed=changed, gap_mm=gap,
                revision_before=before['revision'], revision_after=after['revision'],
                fixed_ids=fixed, definition_unchanged=True, exposure_order_unchanged=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('cases', type=Path)
    parser.add_argument('--out', type=Path, required=True)
    args = parser.parse_args()
    cases = json.loads(args.cases.read_text())
    assert cases, 'No native observations supplied'
    results = []
    for case in cases:
        try:
            results.append(verify(case))
        except (AssertionError, KeyError, TypeError) as error:
            results.append(dict(label=case.get('label'), status='FAIL', error=repr(error)))
    passed = all(case['status'] == 'PASS' for case in results)
    args.out.write_text(json.dumps(dict(schema_version=2, stage='S4-C4', status='PASS' if passed else 'FAIL',
        evidence_kind='native CUA events plus read-only synthetic observations', cases=results), indent=2)+'\n')
    print(f'{sum(case["status"] == "PASS" for case in results)}/{len(results)} native semantic checks passed')
    raise SystemExit(0 if passed else 1)


if __name__ == '__main__':
    main()
