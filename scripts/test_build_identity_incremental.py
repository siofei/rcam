"""Actual Cargo incremental identity checks in synthetic Git/archive workspaces."""
import json
import os
from pathlib import Path
import subprocess
import tempfile
from unittest.mock import patch
import zipfile
import package_source
import source_manifest

ROOT = source_manifest.ROOT

def run(out):
    out.mkdir(parents=True, exist_ok=False)
    env = {key: value for key, value in os.environ.items()
           if not key.upper().startswith('GIT_')}
    env['CARGO_NET_OFFLINE'] = 'true'
    results = []
    with tempfile.TemporaryDirectory(prefix='rcam-build-incremental-') as temporary:
        base = Path(temporary).resolve()
        tree = base/'git-source'
        tree.mkdir()
        def write(name, data):
            path = tree/name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(data)
        for name in source_manifest.ROOT_FILES:
            write(name, 'fixture\n')
        write('rust-toolchain.toml', '[toolchain]\nchannel = "1.89.0"\n')
        write('Cargo.toml', '[workspace]\nresolver = "2"\nmembers = ["crates/editor-app"]\n')
        write('crates/editor-app/Cargo.toml', '[package]\nname = "identity-probe"\nversion = "0.0.0"\nedition = "2024"\n[build-dependencies]\nserde_json = { version = "=1.0.151", features = ["float_roundtrip"] }\n')
        write('crates/editor-app/src/main.rs', 'fn main() { println!("{} {}", env!("RCAM_BUILD_COMMIT"), env!("RCAM_BUILD_SOURCE")); }\n')
        for name in ['crates/editor-app/build.rs', 'crates/editor-app/build_identity.rs', 'crates/editor-core/src/hash.rs']:
            write(name, (ROOT/name).read_text())
        def command(c, cwd=tree, expected=0, label=None):
            r = subprocess.run(c, cwd=cwd, env=env, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
            label = label or str(len(results))
            (out/(label+'.log')).write_text(r.stdout)
            assert (r.returncode == 0) == (expected == 0), (label, r.returncode, r.stdout[-4000:])
            results.append(dict(label=label, command=c, exit_code=r.returncode, expected_success=expected == 0))
            return r.stdout.strip()
        command(['cargo', 'generate-lockfile', '--offline'])
        with patch.object(source_manifest, 'ROOT', tree):
            write('MANIFEST.sha256', source_manifest.contents())
        command(['git', 'init', '-q'])
        command(['git', 'add', '.'])
        command(['git', '-c', 'user.name=Test', '-c', 'user.email=test@example.invalid', 'commit', '-qm', 'synthetic source'])
        commit = command(['git', 'rev-parse', 'HEAD'])
        archive = base/'source.zip'
        with patch.object(source_manifest, 'ROOT', tree):
            package_source.package(archive, stage='synthetic Cargo identity test', commit=commit)
        target = base/'target'
        env['CARGO_TARGET_DIR'] = str(target)
        def build(cwd, label, expected=0):
            command(['cargo', 'build', '--offline', '--locked'], cwd=cwd, expected=expected, label=label)
            if expected == 0:
                return command([str(target/'debug/identity-probe')], cwd=cwd, label=label+'-identity')
        assert build(tree, 'git-clean') == commit+' git-clean'
        write('README.md', 'changed\n')
        assert build(tree, 'git-dirty') == commit+'-dirty git-dirty'
        with patch.object(source_manifest, 'ROOT', tree):
            write('MANIFEST.sha256', source_manifest.contents())
        command(['git', 'add', '.'])
        command(['git', '-c', 'user.name=Test', '-c', 'user.email=test@example.invalid', 'commit', '-qm', 'second synthetic source'])
        second = command(['git', 'rev-parse', 'HEAD'])
        assert second != commit and build(tree, 'git-new-head') == second+' git-clean'
        parent = base/'unrelated'
        parent.mkdir()
        command(['git', 'init', '-q'], cwd=parent)
        command(['git', '-c', 'user.name=Test', '-c', 'user.email=test@example.invalid', 'commit', '--allow-empty', '-qm', 'unrelated parent'], cwd=parent)
        foreign_index = base/'foreign-index'
        foreign_index.write_text('invalid index must not be used')
        vectors = [
            dict(GIT_DIR=str(parent/'.git'), GIT_WORK_TREE=str(tree)),
            dict(GIT_INDEX_FILE=str(foreign_index)),
            dict(GIT_COMMON_DIR=str(parent/'.git')),
            dict(GIT_CONFIG_COUNT='1', GIT_CONFIG_KEY_0='core.worktree',
                 GIT_CONFIG_VALUE_0=str(parent)),
        ]
        for i, vector in enumerate(vectors):
            baseline_env = dict(env)
            env.update(vector)
            assert build(tree, f'isolated-git-env-{i}') == second+' git-clean'
            with patch.dict(os.environ, vector), patch.object(source_manifest, 'ROOT', tree):
                repacked = base/f'isolated-git-env-{i}.zip'
                package_source.package(repacked, commit=second)
            with zipfile.ZipFile(repacked) as z:
                info = json.loads(z.read('PACKAGE_INFO.json'))
                assert info['git_commit'] == second and info['clean_worktree'] is True
            results.append(dict(label=f'isolated-package-env-{i}', status='PASS', source_commit=second))
            env.clear()
            env.update(baseline_env)
        extracted = parent/'extracted'
        with zipfile.ZipFile(archive) as z:
            z.extractall(extracted)
        sentinel = extracted/'.rcam-identity-always-recheck'
        sentinel.write_text('preexisting root sentinel')
        os.utime(sentinel, (1, 1))
        assert build(extracted, 'archive-pristine-parent-git') == commit+' archive-verified'
        main = extracted/'crates/editor-app/src/main.rs'
        original_main = main.read_bytes()
        for mode in ['preexisting-old-mtime', 'added-after-build', 'added-after-build-old-mtime']:
            if mode != 'preexisting-old-mtime':
                sentinel.unlink()
                assert build(extracted, 'sentinel-absent-'+mode) == commit+' archive-verified'
                sentinel.write_text('added root sentinel')
                if mode.endswith('old-mtime'):
                    os.utime(sentinel, (1, 1))
            main.write_bytes(original_main+b'// changed source with root sentinel\n')
            build(extracted, 'sentinel-changed-source-'+mode, 1)
            main.write_bytes(original_main)
            assert build(extracted, 'sentinel-restored-'+mode) == commit+' archive-verified'
        (extracted/'.git').symlink_to('missing-git-dir', target_is_directory=True)
        build(extracted, 'broken-git-symlink-build', 1)
        with patch.object(source_manifest, 'ROOT', extracted):
            try:
                package_source.package(base/'broken-git-symlink.zip')
            except ValueError as error:
                assert 'Git marker symlink' in str(error)
            else:
                raise AssertionError('broken .git symlink package accepted')
        results.append(dict(label='broken-git-symlink-package', status='PASS'))
        (extracted/'.git').unlink()
        original = (extracted/'README.md').read_bytes()
        (extracted/'README.md').write_bytes(b'changed')
        build(extracted, 'archive-source-change', 1)
        (extracted/'README.md').write_bytes(original)
        assert build(extracted, 'archive-restored') == commit+' archive-verified'
        path = extracted/'PACKAGE_INFO.json'
        original = path.read_bytes()
        info = json.loads(original)
        info['git_commit'] = info['commit'] = 'a'*40
        path.write_text(json.dumps(info))
        build(extracted, 'archive-stale-info', 1)
        path.write_bytes(original)
        (extracted/'crates/new.rs').write_text('// added')
        build(extracted, 'archive-added-source', 1)
        (extracted/'crates/new.rs').unlink()
        for name in ['PACKAGE_INFO.json', 'PACKAGE_MANIFEST.sha256', 'MANIFEST.sha256']:
            p = extracted/name
            original = p.read_bytes()
            p.unlink()
            build(extracted, 'archive-missing-'+name.replace('.', '-'), 1)
            p.write_bytes(original)
        assert build(extracted, 'archive-final-restored') == commit+' archive-verified'
    (out/'results.json').write_text(json.dumps(dict(status='PASS', synthetic=True, results=results), indent=2)+'\n')
    print(f'PASS {len(results)} actual commands; same target reused across identity changes')

if __name__ == '__main__':
    import argparse
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument('--out', type=Path, required=True)
    run(p.parse_args().out.resolve())
