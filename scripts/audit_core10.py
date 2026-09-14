"""Read only the frozen CORE10 paths; keep token/use evidence in a private output.
This is a lexical/state usage audit, NOT a geometric Gerber interpreter.
"""
import argparse
from decimal import Decimal, InvalidOperation
from collections import Counter
import hashlib
import json
from pathlib import Path
import re


def tokenize(source):
    """Account for every non-whitespace character, preserving AM blocks."""
    position = 0
    while position < len(source):
        if source[position].isspace():
            position += 1
            continue
        start = position
        extended = source[position] == '%'
        if extended:
            end = source.find('%', position + 1)
            if end < 0:
                raise ValueError(f'unclosed extended block at {position}')
            body = source[position + 1:end]
            if body.lstrip().startswith('AM'):
                if not body.rstrip().endswith('*'):
                    raise ValueError(f'unterminated macro command at {start}')
                yield start, 'AM', body.strip()
            else:
                cursor = position + 1
                for command in body.split('*')[:-1]:
                    if command.strip():
                        yield cursor, 'extended', command.strip()
                    cursor += len(command) + 1
                if body.split('*')[-1].strip():
                    raise ValueError(f'unterminated extended command at {cursor}')
            position = end + 1
        else:
            end = source.find('*', position)
            if end < 0:
                raise ValueError(f'unterminated command at {position}')
            yield start, 'normal', source[position:end].strip()
            position = end + 1


def identity_candidate(token):
    if token in {'IPPOS', 'ASAXBY'}:
        return True
    if token.startswith('IR'):
        try:
            return Decimal(token[2:]) == 0
        except InvalidOperation:
            return False
    match = re.fullmatch(r'(OF|MI|SF)A([+-]?[0-9.]+)B([+-]?[0-9.]+)', token)
    if match:
        try:
            expected = 1 if match[1] == 'SF' else 0
            return Decimal(match[2]) == expected and Decimal(match[3]) == expected
        except InvalidOperation:
            return False
    return False


def audit(source):
    commands = []
    definitions = {}
    macros = {}
    selections = Counter()
    imaging = Counter()
    region_ops = 0
    selected = None
    operation = None
    interpolation = 'G01'
    quadrant = None
    region = False
    polarity = 'D'
    sr = None
    legacy = []
    unknown = []
    arcs = Counter()
    categories = Counter()
    formats = []
    coordinate_mode = None
    unit = None
    mode_uses = Counter()
    draw_shapes = Counter()
    compatibility = []
    issues = []
    width_counts = Counter()
    current_format = None
    previous_xy = {'X': None, 'Y': None}
    rectangular_draws = Counter()
    for offset, kind, token in tokenize(source):
        entry = dict(offset=offset, kind=kind, token=token)
        commands.append(entry)
        if kind == 'AM':
            parts = [part.strip() for part in token.split('*') if part.strip()]
            name = parts[0][2:]
            if name in macros:
                issues.append(dict(offset=offset, code='duplicate_macro'))
            primitives = [part.split(',')[0].strip() for part in parts[1:] if not part.startswith('$') and not part.startswith('0 ')]
            macros[name] = dict(body=parts[1:], primitives=primitives)
            macros[name]['variables'] = sorted(set(re.findall(r'\$\d+', token)))
            macros[name]['assignments'] = sum(part.startswith('$') for part in parts[1:])
            macros[name]['operators'] = sorted(set(re.findall(r'[+\-/xX]', ''.join(parts[1:]))))
            categories['AM'] += 1
        elif kind == 'extended':
            category = token[:2]
            categories[category] += 1
            if category == 'AD':
                match = re.fullmatch(r'ADD(\d+)([^,]+)(?:,(.*))?', token, re.S)
                if match:
                    if str(int(match[1])) in definitions:
                        issues.append(dict(offset=offset, code='duplicate_aperture'))
                    if match[2] not in {'C', 'R', 'O', 'P'} and match[2] not in macros:
                        issues.append(dict(offset=offset, code='macro_before_definition'))
                    definitions[str(int(match[1]))] = dict(shape=match[2], parameters=match[3])
                else:
                    unknown.append(entry)
            elif category == 'LP':
                polarity = token[2:]
            elif category == 'SR':
                sr = token[2:] or None
                entry['repeat_scope'] = sr
                match = re.fullmatch(r'SRX1Y1I([+-]?[0-9.]+)J([+-]?[0-9.]+)', token)
                try:
                    identity = token == 'SR' or bool(match and Decimal(match[1]) == Decimal(match[2]) == 0)
                except InvalidOperation:
                    identity = False
                compatibility.append(dict(offset=offset, category='SR', identity_candidate=identity))
            elif category == 'FS':
                match = re.fullmatch(r'FS([LTD])([AI])X([1-6])([1-6])Y([1-6])([1-6])', token)
                if not match:
                    unknown.append(entry)
                else:
                    coordinate_mode = match[2]
                    current_format = dict(zero_mode=match[1], coordinate_mode=match[2],
                                          x_integer=int(match[3]), x_decimal=int(match[4]),
                                          y_integer=int(match[5]), y_decimal=int(match[6]))
                    formats.append(dict(offset=offset, token=token, **current_format))
            elif category == 'MO':
                if token not in {'MOMM', 'MOIN'}:
                    unknown.append(entry)
                else:
                    unit = token[2:]
            elif category == 'IC':
                if token != 'ICAS':
                    unknown.append(entry)
                compatibility.append(dict(offset=offset, category='IC', ascii_declaration=token == 'ICAS'))
            elif category == 'IO':
                match = re.fullmatch(r'IOA([+-]?(?:\d+(?:\.\d*)?|\.\d+))B([+-]?(?:\d+(?:\.\d*)?|\.\d+))', token)
                if not match:
                    unknown.append(entry)
                compatibility.append(dict(offset=offset, category='IO', unit=unit,
                                          nonzero=bool(match and (Decimal(match[1]) != 0 or Decimal(match[2]) != 0)),
                                          semantic_status='blocked_independent_offset_validation'))
            elif category in {'IP','MI','OF','SF','IR','AS'}:
                identity = identity_candidate(token)
                legacy.append(dict(offset=offset, token=token, identity_candidate=identity))
            elif category not in {'FS','MO','TF','TA','TO','TD','LM','LR','LS','AB','IN','LN'}:
                unknown.append(entry)
        else:
            if token.startswith('G04'):
                categories['G04'] += 1
                continue
            g_codes = re.findall(r'G0*(\d+)', token)
            for code in g_codes:
                number = int(code)
                categories[f'G{number:02}'] += 1
                if number in {1,2,3}: interpolation = f'G{number:02}'
                elif number in {74,75}: quadrant = f'G{number}'
                elif number in {36,37}: region = number == 36
                elif number in {90,91}: coordinate_mode = 'A' if number == 90 else 'I'
                elif number in {70,71}: unit = 'IN' if number == 70 else 'MM'
                elif number not in {54,70,71,90,91}: unknown.append(entry)
            ds = re.findall(r'D0*(\d+)', token)
            explicit_operation = False
            for d in ds:
                number = int(d)
                if number >= 10:
                    selected = str(number)
                    selections[selected] += 1
                    if selected not in definitions:
                        issues.append(dict(offset=offset, code='selection_before_definition'))
                elif number in {1,2,3}:
                    operation = number
                    explicit_operation = True
                else: unknown.append(entry)
            coordinate = bool(re.search(r'[XYIJ][+-]?\d', token))
            delta = {}
            if current_format and coordinate_mode == 'A':
                # Only an audit of axis changes, not sweep geometry or bounds.
                for axis, number in re.findall(r'([XY])([+-]?\d+)', token):
                    prefix = axis.lower()
                    width = current_format[prefix + '_integer'] + current_format[prefix + '_decimal']
                    digits = number.lstrip('+-')
                    if current_format['zero_mode'] == 'T':
                        digits = digits.ljust(width, '0')
                    value = Decimal(('-' if number.startswith('-') else '') + digits).scaleb(-current_format[prefix + '_decimal'])
                    delta[axis] = None if previous_xy[axis] is None else value - previous_xy[axis]
                    previous_xy[axis] = value
            for axis, number in re.findall(r'([XYIJ])([+-]?\d+)', token):
                if current_format and current_format['zero_mode'] == 'D':
                    prefix = 'x' if axis in 'XI' else 'y'
                    expected = current_format[prefix + '_integer'] + current_format[prefix + '_decimal']
                    width_counts['full' if len(number.lstrip('+-')) == expected else 'non_full'] += 1
            if (coordinate or explicit_operation) and operation in {1,3}:
                entry['imaging'] = dict(aperture=selected, operation=operation, region=region, interpolation=interpolation, quadrant=quadrant, polarity=polarity, repeat_scope=sr, coordinate_mode=coordinate_mode, unit=unit)
                mode_uses[coordinate_mode or 'undefined'] += 1
                if region and operation == 1:
                    region_ops += 1
                else:
                    imaging[selected or 'undefined'] += 1
                    shape = definitions.get(selected, {}).get('shape', 'undefined')
                    draw_shapes[f'D0{operation}:' + (shape if shape in {'C','R','O','P','undefined'} else 'AM')] += 1
                    if operation == 1 and shape == 'R':
                        dx, dy = delta.get('X', 0), delta.get('Y', 0)
                        direction = 'unknown' if coordinate_mode != 'A' or dx is None or dy is None else 'zero' if dx == dy == 0 else 'vertical' if dx == 0 else 'horizontal' if dy == 0 else 'diagonal'
                        rectangular_draws[interpolation + ':' + direction] += 1
                    if selected not in definitions:
                        issues.append(dict(offset=offset, code='imaging_before_definition'))
                if operation == 1 and interpolation in {'G02','G03'}:
                    arcs[quadrant or 'undefined'] += 1
            remainder = re.sub(r'(?:G\d+|D\d+|M0?[02]|[XYIJ][+-]?\d+)', '', token)
            if remainder.strip(): unknown.append(entry)
    aperture_report = {code: dict(**definition, selections=selections[code], imaging_uses=imaging[code]) for code,definition in definitions.items()}
    used_macro_names = {d['shape'] for d in aperture_report.values() if d['imaging_uses'] and d['shape'] not in {'C','R','O','P'}}
    return dict(categories=dict(categories), apertures=aperture_report, macros=macros,
                formats=formats, imaging_by_coordinate_mode=dict(mode_uses), imaging_by_operation_shape=dict(draw_shapes),
                explicit_format_width_counts=dict(width_counts), compatibility_commands=compatibility, usage_issues=issues,
                rectangular_draw_directions=dict(rectangular_draws),
                used_macros=sorted(used_macro_names), unused_macros=sorted(set(macros)-used_macro_names), unreferenced_apertures=[code for code in definitions if not selections[code]],
                selected_without_imaging=[code for code in definitions if selections[code] and not imaging[code]],
                undefined_imaging_apertures=[code for code in imaging if code not in definitions],
                region_draw_operations=region_ops, arc_operations_by_quadrant=dict(arcs), legacy_commands=legacy,
                unknown_commands=unknown, commands=commands)


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--manifest', type=Path, required=True)
    parser.add_argument('--out', type=Path, required=True)
    args=parser.parse_args()
    manifest_bytes=args.manifest.read_bytes()
    manifest=json.loads(manifest_bytes)
    samples=[s for s in manifest['samples'] if s.get('core_id')]
    if len(samples)!=10 or len({s['core_id'] for s in samples})!=10:
        raise ValueError('frozen CORE10 must contain ten unique identities')
    args.out.mkdir(parents=True,exist_ok=False)
    summary=[]
    for sample in samples:
        item=dict(core_id=sample['core_id'], expected_sha256=sample['sha256'])
        try:
            path=Path(manifest['root'])/sample['path']
            if path.stat().st_size>32*1024*1024: raise ValueError('audit input exceeds 32 MiB')
            data=path.read_bytes()
            digest=hashlib.sha256(data).hexdigest()
            if digest!=sample['sha256']: raise ValueError('frozen source hash mismatch')
            report=audit(data.decode('ascii'))
            # Independently re-read after audit; source never opened for writing.
            if hashlib.sha256(path.read_bytes()).hexdigest()!=digest: raise ValueError('source changed during audit')
            (args.out/f"{sample['core_id']}.json").write_text(json.dumps(report,indent=2))
            item.update(status='audited' if not report['unknown_commands'] and not report['undefined_imaging_apertures'] and not report['usage_issues'] else 'needs_review', sha256=digest,
                        semantic_validation='not_run', edit_roundtrip_safe=False,
                        source_bytes=len(data), formats=report['formats'],
                        imaging_by_coordinate_mode=report['imaging_by_coordinate_mode'],
                        imaging_by_operation_shape=report['imaging_by_operation_shape'],
                        explicit_format_width_counts=report['explicit_format_width_counts'],
                        compatibility_commands=report['compatibility_commands'],
                        region_draw_operations=report['region_draw_operations'], usage_issue_count=len(report['usage_issues']),
                        rectangular_draw_directions=report['rectangular_draw_directions'],
                        command_count=len(report['commands']), categories=report['categories'],
                        aperture_count=len(report['apertures']), unreferenced_count=len(report['unreferenced_apertures']),
                        unused_macro_count=len(report['unused_macros']), used_macro_count=len(report['used_macros']), used_macro_primitives=sorted({p for name in report['used_macros'] for p in report['macros'].get(name,{}).get('primitives',[])}),
                        arcs=report['arc_operations_by_quadrant'], legacy_identity_candidates=all(x['identity_candidate'] for x in report['legacy_commands']),
                        undefined_imaging_apertures=report['undefined_imaging_apertures'], unknown_count=len(report['unknown_commands']))
        except (OSError,ValueError,UnicodeError) as error:
            item.update(status='blocked', diagnostic=str(error))
        summary.append(item)
        print(item['core_id'], item['status'], flush=True)
    (args.out/'summary.json').write_text(json.dumps(dict(schema_version=2, kind='private_lexical_usage_audit', manifest_sha256=hashlib.sha256(manifest_bytes).hexdigest(), samples=summary),indent=2))
    return 2 if any(s['status']=='blocked' for s in summary) else 1 if any(s['status']=='needs_review' for s in summary) else 0

if __name__=='__main__':
    raise SystemExit(main())
