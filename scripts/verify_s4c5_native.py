"""Independent array checks over read-only native synthetic observations, never UI control."""
import argparse
import json
import math
from pathlib import Path


def verify(case):
    before, after = case['before'], case['after']
    assert before['document_id'] == after['document_id']
    assert before['fixture_sha256'] == after['fixture_sha256']
    assert before['block_definitions_sha256'] == after['block_definitions_sha256']
    selected = set(before['selected_object_ids'])
    old, new = before['object_bounds'], after['object_bounds']
    positions = [i for i, obj in enumerate(old) if obj['object_id'] in selected]
    assert positions == list(range(positions[0], positions[-1] + 1))
    source = old[positions[0]:positions[-1] + 1]
    cells = case['rows'] * case['columns']
    assert len(new) == len(old) + len(source) * (cells - 1)
    assert len({o['object_id'] for o in new}) == len(new)
    inserted_end = positions[-1] + 1 + len(source) * (cells - 1)
    unchanged = new[:positions[-1] + 1] + new[inserted_end:]
    for left, right in zip(old, unchanged):
        for key in ['object_id', 'geometry_sha256', 'exposure', 'bounds_mm']:
            assert left[key] == right[key], ('source/tail changed', key)
    for cell in range(1, cells):
        dx = (cell % case['columns']) * case['pitch_x_mm']
        dy = (cell // case['columns']) * case['pitch_y_mm']
        start = positions[-1] + 1 + (cell - 1) * len(source)
        for src, dst in zip(source, new[start:start + len(source)]):
            assert dst['exposure'] == src['exposure']
            assert dst['object_id'] not in {o['object_id'] for o in old}
            for coordinate in range(4):
                assert math.isclose(dst['bounds_mm'][coordinate], src['bounds_mm'][coordinate] + [dx,dy][coordinate % 2], rel_tol=0, abs_tol=1e-8)
    step = int(cells > 1)
    assert int(after['revision']) == int(before['revision']) + step
    assert after['undo_entries'] == before['undo_entries'] + step
    assert after['selected_object_ids'] == before['selected_object_ids']
    if 'undo' in case:
        assert case['undo']['semantic_objects_sha256'] == before['semantic_objects_sha256']
        assert case['redo']['semantic_objects_sha256'] == after['semantic_objects_sha256']
        assert case['undo']['block_definitions_sha256'] == before['block_definitions_sha256']
        assert case['redo']['block_definitions_sha256'] == before['block_definitions_sha256']
    return dict(label=case['label'],status='PASS',cells=cells,source_objects=len(source),created_objects=len(source)*(cells-1),source_unchanged=True,definition_shared=True,exposure_order_preserved=True,undo_redo_checked='undo' in case)


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('cases',type=Path)
    parser.add_argument('--out',type=Path,required=True)
    args=parser.parse_args()
    cases=json.loads(args.cases.read_text())
    assert cases
    results=[]
    for case in cases:
        try: results.append(verify(case))
        except (AssertionError,KeyError,ValueError,TypeError,IndexError) as error: results.append(dict(label=case.get('label'),status='FAIL',error=repr(error)))
    passed=all(r['status']=='PASS' for r in results)
    args.out.write_text(json.dumps(dict(schema_version=2,stage='S4-C5',status='PASS' if passed else 'FAIL',evidence_kind='native CUA plus read-only synthetic observations',cases=results),indent=2)+'\n')
    print(f'{sum(r["status"]=="PASS" for r in results)}/{len(results)} native array checks passed')
    raise SystemExit(0 if passed else 1)

if __name__=='__main__': main()
