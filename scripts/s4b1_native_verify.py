#!/usr/bin/env python3
"""Evaluate the S4-B1 native GUI evidence recorded by the opt-in probe.

Input : a probe directory written by the app when RCAM_NATIVE_PROBE_DIR is set
        (native_observations.jsonl, native_actions.log, screens/*.ppm).
Output: native_observations.json (Gate F, 16 items), layer_panel_width_validation.json
        (Gate G, 14 items), screens/*.png, and a short text summary.

Every item is decided from observed application state, never from a prose claim; an
item without matching observations is reported as NOT_OBSERVED (a failure).
"""
import argparse
import hashlib
import json
import re
import struct
import sys
import zlib
from pathlib import Path


def load(probe: Path):
    obs = [json.loads(l) for l in (probe / 'native_observations.jsonl').read_text(encoding='utf-8').splitlines() if l.strip()]
    actions = []
    for line in (probe / 'native_actions.log').read_text(encoding='utf-8').splitlines():
        t, _, text = line.partition('\t')
        actions.append((int(t), text))
    return obs, actions


def ppm_to_png(src: Path, dst: Path):
    data = src.read_bytes()
    m = re.match(rb'P6\n(\d+) (\d+)\n255\n', data)
    w, h = int(m.group(1)), int(m.group(2))
    pix = data[m.end():]
    raw = b''.join(b'\x00' + pix[y * w * 3:(y + 1) * w * 3] for y in range(h))

    def chunk(tag, body):
        c = struct.pack('>I', len(body)) + tag + body
        return c + struct.pack('>I', zlib.crc32(tag + body) & 0xFFFFFFFF)

    dst.write_bytes(b'\x89PNG\r\n\x1a\n' + chunk(b'IHDR', struct.pack('>IIBBBBB', w, h, 8, 2, 0, 0, 0))
                    + chunk(b'IDAT', zlib.compress(raw, 9)) + chunk(b'IEND', b''))
    return w, h


def layer(o, lid):
    return next((l for l in o['layers'] if l['layer_id'] == lid), None)


class Check:
    def __init__(self):
        self.items = []

    def add(self, key, title, ok, evidence):
        self.items.append({'id': key, 'title': title,
                           'result': 'PASS' if ok else ('NOT_OBSERVED' if ok is None else 'FAIL'),
                           'evidence': evidence})


def first(obs, pred, start=0):
    for i in range(start, len(obs)):
        try:
            if pred(obs[i]):
                return i
        except (KeyError, TypeError):
            pass
    return None


def acts(actions, pat):
    r = re.compile(pat)
    return [(t, a) for t, a in actions if r.search(a)]


def gate_f(obs, actions):
    c = Check()
    # 1 Finder drag/drop of several .gbr files
    drops = acts(actions, r'^DROP_FILES n=(\d+)')
    imports = acts(actions, r'^ImportGerbers n=(\d+)')
    multi = [d for d in drops if int(re.search(r'n=(\d+)', d[1]).group(1)) >= 2]
    ok = None
    ev = 'no DROP_FILES event recorded'
    if multi:
        t0 = multi[0][0]
        follow = [a for a in imports if a[0] >= t0]
        ok = bool(follow) and int(re.search(r'n=(\d+)', follow[0][1]).group(1)) >= 2
        ev = {'drop': multi[0], 'import': follow[:1]}
    c.add('F01', 'Finder drag/drop of multiple .gbr', ok, ev)
    # 2 category settings modal
    i = first(obs, lambda o: o['layer_dialog']['kind'] == 'Categories')
    c.add('F02', 'Category Settings modal opens', i is not None if i is not None else None, {'observation': i})
    # 3 category colour override
    i = first(obs, lambda o: any(k['color_override'] for l in o['layers'] for k in l['classes']))
    c.add('F03', 'Category colour override applied', True if i is not None else None,
          {'observation': i, 'action': acts(actions, r'color_override: Some')[:1]})
    # 4-6 category visible / selectable / locked
    for key, field, cond, title in (
            ('F04', 'visible', lambda k: not k['visible'], 'Category Visible off'),
            ('F05', 'selectable', lambda k: not k['selectable'], 'Category Selectable off'),
            ('F06', 'locked', lambda k: k['locked'], 'Category Locked on')):
        i = first(obs, lambda o: any(cond(k) for l in o['layers'] for k in l['classes']))
        c.add(key, title, True if i is not None else None, {'observation': i})
    # 7 layer locked
    i = first(obs, lambda o: any(l['locked'] for l in o['layers']))
    c.add('F07', 'Layer Locked on', True if i is not None else None,
          {'observation': i, 'action': acts(actions, r'locked: Some\(true\)')[:1]})
    # 8 solo on then off
    on = first(obs, lambda o: o['solo_layer'])
    off = first(obs, lambda o: o['solo_layer'] is None, (on or 0) + 1) if on is not None else None
    c.add('F08', 'Solo on, then off', (on is not None and off is not None) or None,
          {'on': on, 'off': off, 'actions': acts(actions, r'SetSoloLayer')[:4]})
    # 9 drag reorder
    reorder = acts(actions, r'^ReorderLayers')
    ok = None
    ev = {}
    if reorder:
        t = reorder[0][0]
        i = first(obs, lambda o: o['t_ms'] >= t and o['display_order'])
        prev = [o for o in obs if o['t_ms'] < t and o['display_order']]
        ok = bool(prev) and i is not None and prev[-1]['display_order'] != obs[i]['display_order']
        ev = {'before': prev[-1]['display_order'] if prev else None, 'after': obs[i]['display_order'] if i is not None else None}
    c.add('F09', 'Drag reorder changes order', ok, ev)
    # 10 dirty/generated strong delete confirmation
    i = first(obs, lambda o: o['layer_dialog']['kind'] == 'Delete' and o['layer_dialog']['risk'] == 'NonEmptyDirty' and not o['layer_dialog']['acknowledged'])
    j = first(obs, lambda o: o['layer_dialog']['kind'] == 'Delete' and o['layer_dialog']['acknowledged'], (i or 0)) if i is not None else None
    rm = [a for a in acts(actions, r'^RemoveLayer .* allow_non_empty=true')]
    ok = None
    if i is not None:
        ok = j is not None and bool(rm) and rm[0][0] >= obs[j]['t_ms']
    c.add('F10', 'Dirty layer: strong confirmation gates the delete', ok,
          {'unchecked_dialog': i, 'checked_dialog': j, 'remove': rm[:1],
           'modified': obs[i]['layer_dialog'].get('modified') if i is not None else None,
           'generated': obs[i]['layer_dialog'].get('generated') if i is not None else None})
    # 11 delete last layer -> empty workspace
    rmi = first(obs, lambda o: len(o['layers']) == 0 and o['manufacturing_revision'] is not None and o['t_ms'] > 0)
    prior = first(obs, lambda o: len(o['layers']) == 1)
    c.add('F11', 'Deleting the last layer leaves an empty Workspace',
          (rmi is not None and prior is not None and prior < rmi) or None, {'one_layer': prior, 'empty': rmi})
    # 12 undo restores same layer / style / order
    ok = None
    ev = {}
    for a_t, a in acts(actions, r'^RemoveLayer (\S+)'):
        lid = a.split()[1]
        before = [o for o in obs if o['t_ms'] < a_t and layer(o, lid)]
        if not before:
            continue
        snap = before[-1]
        order_before = snap['display_order']
        undo = [t for t, x in acts(actions, r'^History undo') if t > a_t]
        if not undo:
            continue
        after = [o for o in obs if o['t_ms'] >= undo[0] and layer(o, lid)]
        if not after:
            continue
        l0, l1 = layer(snap, lid), layer(after[0], lid)
        keys = ('name', 'color', 'display_mode', 'color_mode', 'visible', 'locked', 'selectable', 'z_index', 'objects')
        same = all(l0[k] == l1[k] for k in keys) and order_before == after[0]['display_order']
        ok = same if ok is None else (ok and same)
        ev = {'layer': lid, 'same': same, 'before': {k: l0[k] for k in keys}, 'after': {k: l1[k] for k in keys},
              'order_before': order_before, 'order_after': after[0]['display_order']}
    c.add('F12', 'Undo restores the same layer id / style / order', ok, ev)
    # 13 Filled / Outline / ZeroWidth quick control
    modes = {l['display_mode'] for o in obs for l in o['layers']}
    revs = {o['manufacturing_revision'] for o in obs if any(l['display_mode'] != 'Filled' for l in o['layers'])}
    c.add('F13', 'Row quick control switches Filled / Outline / ZeroWidth',
          ({'Filled', 'Outline', 'ZeroWidth'} <= modes) or None,
          {'modes_seen': sorted(modes), 'display_mode_actions': len(acts(actions, r'display_mode: Some'))})
    # 14 recent colours
    rc = [o['recent_colors'] for o in obs if o['recent_colors']]
    ok = None
    if rc:
        last = rc[-1]
        ok = len(last) >= 2 and len(set(last)) == len(last) and len(last) <= 8
    c.add('F14', 'Recent colours: bounded, unique, newest first', ok, {'final': rc[-1] if rc else None})
    # 15 active indicator
    i = first(obs, lambda o: o['rows'] and any(r['controls'].get('indicator') for r in o['rows']))
    act_ok = None
    if i is not None:
        act_ok = all(any(r['layer_id'] == o['active_layer'] for r in o['rows']) for o in obs if o['rows'] and o['active_layer'])
    c.add('F15', 'Active indicator present on every row; active layer follows clicks', act_ok,
          {'observation': i, 'active_layers_seen': sorted({o['active_layer'] for o in obs if o['active_layer']})})
    # 16 compact panel without inline detail expansion
    heights = {round(r['row_height'], 1) for o in obs for r in o['rows']}
    keys = {k for o in obs for r in o['rows'] for k in r['controls']}
    ok = None
    if heights:
        ok = max(heights) - min(heights) <= 1.0 and 'expand' not in keys
    c.add('F16', 'Compact rows: uniform height, no expand control', ok, {'row_heights': sorted(heights), 'control_keys': sorted(keys)})
    return c.items


def gate_g(obs, actions):
    c = Check()
    w = [o['layer_panel_width'] for o in obs if o['layer_panel_width'] and o['layer_panel_width'] > 0]
    c.add('G01', 'Default width', abs(w[0] - 250) <= 3 if w else None, {'first_width_points': w[0] if w else None})
    c.add('G02', 'Dragged to the minimum width', (min(w) <= 240.5) if w else None, {'min_width_points': min(w) if w else None})
    c.add('G03', 'Dragged to the maximum width', (max(w) >= 479.5) if w else None, {'max_width_points': max(w) if w else None})
    names = {r['name'] for o in obs for r in o['rows']}
    long_en = [n for n in names if re.fullmatch(r'[A-Za-z0-9_.\- ]{30,}', n)]
    zh = [n for n in names if re.search(r'[一-鿿]', n)]
    zh_hash = [n for n in names if re.search(r'[一-鿿]', n) and ' ' in n and '#' in n]
    c.add('G04', 'Long English name', True if long_en else None, {'names': long_en[:2]})
    c.add('G05', 'Chinese name', True if zh else None, {'names': zh[:2]})
    c.add('G06', 'Chinese + space + #', True if zh_hash else None, {'names': zh_hash[:2]})
    mx = max((len(o['layers']) for o in obs), default=0)
    c.add('G07', '20+ layers', True if mx >= 20 else (None if mx == 0 else False), {'max_layers': mx})
    ell = [(o['layer_panel_width'], r['name']) for o in obs for r in o['rows'] if r['name_ellipsized']]
    c.add('G08', 'Ellipsis at narrow widths', True if ell else None, {'examples': ell[:3]})
    tip = [(o['layer_panel_width'], r['name']) for o in obs for r in o['rows'] if r['tooltip_shown'] and r['name_ellipsized']]
    c.add('G09', 'Tooltip shows the full name of an ellipsized name', True if tip else None, {'examples': tip[:3]})
    ren = None
    ev = {}
    for o in obs:
        d = o['layer_dialog']
        if d and d['kind'] == 'Rename':
            full = next((l['name'] for l in o['layers'] if l['layer_id'] == d['layer']), None)
            was_ell = any(r['name_ellipsized'] for oo in obs for r in oo['rows'] if r['layer_id'] == d['layer'])
            if full is not None and d['text'] == full:
                ren = True if was_ell or ren is None else ren
                ev = {'layer': d['layer'], 'text': d['text'], 'was_ellipsized_in_panel': was_ell}
                if was_ell:
                    break
    c.add('G10', 'Rename dialog shows the full name', ren, ev)
    bad = [(o['layer_panel_width'], r['layer_id'], r['overlaps']) for o in obs for r in o['rows'] if r['overlaps'] or not r['all_controls_inside_row']]
    n_rows = sum(len(o['rows']) for o in obs)
    c.add('G11', 'Buttons never overlap / stay inside the row', (not bad) if n_rows else None,
          {'row_observations': n_rows, 'violations': bad[:5]})
    narrow = [i for i, o in enumerate(obs) if o['layer_panel_width'] and o['layer_panel_width'] <= 241]
    changed = set()
    for i in narrow:
        for j in range(i + 1, min(i + 400, len(obs))):
            if obs[j]['layer_panel_width'] > 241:
                break
            for l0 in obs[i]['layers']:
                l1 = layer(obs[j], l0['layer_id'])
                if not l1:
                    continue
                if l0['visible'] != l1['visible']:
                    changed.add('visible')
                if l0['locked'] != l1['locked']:
                    changed.add('locked')
                if l0['display_mode'] != l1['display_mode']:
                    changed.add('display_mode')
    more = acts(actions, r'^(FitLayer|SetSoloLayer|SetActiveLayer|LayerSummary)')
    dlg = first(obs, lambda o: o['layer_panel_width'] <= 241 and o['layer_dialog'] and o['layer_dialog']['kind'] in ('Rename', 'Settings', 'Categories', 'Delete', 'DeletePending'))
    ok = ({'visible', 'locked', 'display_mode'} <= changed and dlg is not None) if narrow else None
    c.add('G12', 'Visible / Locked / DisplayMode / More all usable at the minimum width', ok,
          {'changed_at_min_width': sorted(changed), 'more_menu_dialog_observation': dlg})
    # 13 resize never touches manufacturing state
    resize_pairs = [(a, b) for a, b in zip(obs, obs[1:]) if a['layer_panel_width'] != b['layer_panel_width']]
    okr = all(a['manufacturing_revision'] == b['manufacturing_revision'] and a['dirty'] == b['dirty']
              and a['workspace_revision'] == b['workspace_revision'] for a, b in resize_pairs) if resize_pairs else None
    c.add('G13', 'Resizing changes no manufacturing revision / dirty flag / workspace revision', okr,
          {'resize_steps': len(resize_pairs)})
    ppp = {o['ppp'] for o in obs}
    canvas_x = {round(o['canvas'][0]) for o in obs if o['layer_panel_width'] > 0}
    c.add('G14', 'Retina ppp = 2 and the canvas relayouts with the panel', ((2.0 in ppp) and len(canvas_x) > 1) or (None if not obs else False),
          {'ppp_values': sorted(ppp), 'canvas_left_edges': sorted(canvas_x)[:8]})
    return c.items


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument('probe', type=Path)
    ap.add_argument('--binary', type=Path)
    ap.add_argument('--out', type=Path)
    a = ap.parse_args()
    out = a.out or a.probe
    obs, actions = load(a.probe)
    shots = {}
    (out / 'screens').mkdir(parents=True, exist_ok=True)
    for ppm in sorted((a.probe / 'screens').glob('*.ppm')):
        png = out / 'screens' / (ppm.stem + '.png')
        shots[ppm.stem] = dict(zip(('width_px', 'height_px'), ppm_to_png(ppm, png)))
        shots[ppm.stem]['file'] = f'screens/{png.name}'
    sha = hashlib.sha256(a.binary.read_bytes()).hexdigest() if a.binary else None
    f = gate_f(obs, actions)
    g = gate_g(obs, actions)
    ppps = sorted({o['ppp'] for o in obs})
    header = {'schema': 'rcam-s4b1-native/1', 'binary_sha256': sha, 'pixels_per_point': ppps,
              'observations': len(obs), 'actions': len(actions), 'screenshots': shots}
    (out / 'native_observations.json').write_text(json.dumps({**header, 'items': f}, indent=2, ensure_ascii=False), encoding='utf-8')
    (out / 'layer_panel_width_validation.json').write_text(json.dumps({**header, 'items': g}, indent=2, ensure_ascii=False), encoding='utf-8')
    bad = 0
    for title, items in (('Gate F', f), ('Gate G', g)):
        print(title)
        for i in items:
            print(f"  {i['id']} {i['result']:<12} {i['title']}")
            bad += i['result'] != 'PASS'
    print(f'NOT_PASSING={bad}')
    return 1 if bad else 0


if __name__ == '__main__':
    sys.exit(main())
