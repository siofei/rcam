"""Precommit verifier-only review; preserve the original native observation identity."""
import argparse
import json
import re
from pathlib import Path
import package_source
import source_manifest
import verify_batch_drag_evidence as full
import verify_batch_drag_native as native
from verify_s5m2_evidence import require, load, safe, digest

ALLOWED = {
    'scripts/verify_batch_drag_native.py',
    'scripts/test_verify_batch_drag_native.py',
    'scripts/test_batch_drag_package_negatives.py',
    'scripts/verify_batch_drag_review.py',
    'docs/S5_M2_B_EVIDENCE_REVIEW_FIX.md',
    'docs/S5_M2_B_REVIEW.md',
}


def source_identity(root):
    source_manifest.ROOT = root
    full.coverage(root, 'PACKAGE_MANIFEST.sha256')
    entries = package_source.checked_manifest(root, 'MANIFEST.sha256')
    expected = {p.relative_to(root).as_posix() for p in source_manifest.source_files()}
    require(set(entries) == expected, 'source manifest coverage')
    info = load(root/'PACKAGE_INFO.json')
    require(info['schema_version'] == 1 and info['stage'] == 'S5-M2-B'
            and info['supplemental_only'] is False, 'review package kind')
    require(info['clean_worktree'] is False, 'review requires explicit dirty precommit identity')
    require(re.fullmatch('[a-f0-9]{40}', info['git_commit']) is not None
            and info['commit'] == info['git_commit'], 'review commit identity')
    require(info['source_manifest_sha256'] == digest(root/'MANIFEST.sha256'), 'review source identity')
    included = sorted(expected | {'MANIFEST.sha256'})
    require(info['included_paths'] == included
            and info['source_file_count'] == len(expected)
            and info['manifest_count'] == len(included), 'review package source coverage')
    require(set(full.coverage(root, 'PACKAGE_MANIFEST.sha256'))
            == set(included) | {'PACKAGE_INFO.json'}, 'unexpected package files')
    return info, entries


def verify(candidate, observation, evidence):
    candidate, observation, evidence = (p.resolve() for p in (candidate, observation, evidence))
    require(candidate != observation, 'validator and observation source must be distinct')
    for module in (full, native):
        require(Path(module.__file__).resolve().parent == candidate/'scripts',
                'verifier module not loaded from candidate source')
    require(Path(__file__).resolve().parent == candidate/'scripts', 'review verifier source mismatch')
    new, new_entries = source_identity(candidate)
    old, old_entries = source_identity(observation)
    require(new['git_commit'] == old['git_commit'], 'review base commit differs')
    changed = sorted(p for p in new_entries.keys() | old_entries.keys()
                     if new_entries.get(p) != old_entries.get(p))
    require(changed and set(changed) <= ALLOWED, 'non-verifier source change: '+str(changed))
    require('scripts/verify_batch_drag_native.py' in changed, 'no repaired native verifier')
    verified = full.verify(observation, evidence, True)
    close = load(evidence/'CLOSEOUT.json')
    workers = []
    callbacks = []
    for row in load(safe(evidence, close['native_index'])):
        report = load(safe(evidence, 'native/'+row['directory']+'/observations.json'))
        callbacks.append({'directory': row['directory'], 'samples': len(report['frames']),
                          'painted_samples': sum(f['painted'] for f in report['frames']),
                          'draw': report['counters']['draw'],
                          'uniform_upload_bytes': report['counters']['uniform-upload']})
        for worker in report['worker']:
            workers.append({'directory': row['directory'], 'action': worker['action'],
                            'derived_elapsed_ms': native.worker_elapsed_ms(worker),
                            'legacy_elapsed_ms': worker['elapsed_ms']})
    return {'result': 'PRECOMMIT_REVIEW_PASS', 'clean_worktree': False,
            'base_commit': new['git_commit'], 'changed_files': changed,
            'validator_source_manifest_sha256': new['source_manifest_sha256'],
            'observation_source_manifest_sha256': old['source_manifest_sha256'],
            'observation_binaries': {k: v['sha256'] for k, v in close['binaries'].items()},
            'retained_observation_verification': verified,
            'callback_coverage': callbacks, 'worker_clock_audit': workers,
            'cargo_native_rerun': False,
            'remaining': 'Independent re-review; then clean commit/build/native and same-commit audit'}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--candidate-source', required=True, type=Path)
    parser.add_argument('--observation-source', required=True, type=Path)
    parser.add_argument('--evidence', required=True, type=Path)
    args = parser.parse_args()
    try:
        print(json.dumps(verify(args.candidate_source, args.observation_source, args.evidence), indent=2))
        return 0
    except (OSError, ValueError, KeyError, TypeError) as error:
        print('FAIL:', error)
        return 1


if __name__ == '__main__':
    raise SystemExit(main())
