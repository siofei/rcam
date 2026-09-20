"""Validate S2-C1 native tool evidence plus unchanged S2-B3.2 P1K gates."""
import json
from pathlib import Path
import sys
from validate_native_bench import validate


def validate_tools(root):
    result = validate(root)
    data = json.loads((root/'s2c1-tools.json').read_text())
    assert data['schema_version'] == 2 and data['status'] == 'PASS'
    rows = data['records']
    assert [r['phase'] for r in rows] == [20, 21, 22, 221, 23, 28, 33]
    a, dynamic, fixed, focus_escape, cleared, moved, reopened = rows
    assert focus_escape["text_escape_preserved_measure"] and focus_escape["info"] == a["info"]
    assert dynamic['values'] == fixed['values'] == [3., 4., 5.]
    assert not dynamic['fixed'] and fixed['fixed']
    assert cleared['values'] is None
    assert a['info'] == dynamic['info'] == fixed['info'] == cleared['info']
    assert all(r['ppp'] == 2. and r['canvas_physical'] == [1600., 900.] for r in [a, dynamic, fixed, cleared])
    assert moved['coordinates_correct'] and reopened['reopen_coordinates_correct']
    assert moved['info']['undo_entries'] == a['info']['undo_entries'] + 1
    assert int(moved['info']['revision']) == int(a['info']['revision']) + 1
    assert moved['info']['dirty']
    assert not reopened['info']['dirty'] and reopened['info']['undo_entries'] == 0
    assert (root/'after-tools.gbr').read_bytes() == (root/'baseline.gbr').read_bytes()
    assert (root/'single-snapped.gbr').read_bytes() != (root/'baseline.gbr').read_bytes()
    result.update(tools='PASS', measurement_mm=[3., 4., 5.], native_tool_records=len(rows))
    return result


if __name__ == '__main__':
    print(json.dumps(validate_tools(Path(sys.argv[1])), indent=2))
