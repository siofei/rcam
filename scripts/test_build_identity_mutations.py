"""Reproduce the independent review's 19 resolver mutations against current code."""
import argparse
import hashlib
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
    env = {k: v for k, v in os.environ.items() if not k.upper().startswith('GIT_')}
    env['CARGO_NET_OFFLINE'] = 'true'
    rows = []
    with tempfile.TemporaryDirectory(prefix='rcam-identity-mutations-') as tmp:
        base = Path(tmp).resolve()
        probe = base/'probe'
        (probe/'src').mkdir(parents=True)
        (probe/'Cargo.toml').write_text('[package]\nname="infra2-resolver-probe"\nversion="0.0.0"\nedition="2024"\n[dependencies]\nserde_json={version="=1.0.151",features=["float_roundtrip"]}\n')
        (probe/'build_identity.rs').write_bytes((ROOT/'crates/editor-app/build_identity.rs').read_bytes())
        sha = base/'editor-core/src/hash.rs'
        sha.parent.mkdir(parents=True)
        sha.write_bytes((ROOT/'crates/editor-core/src/hash.rs').read_bytes())
        (probe/'src/main.rs').write_text('#[path="../build_identity.rs"] mod identity;\nfn main(){let p=std::env::args_os().nth(1).unwrap(); match identity::resolve(std::path::Path::new(&p)){Ok((c,s))=>println!("OK {c} {s}"),Err(e)=>{eprintln!("ERR {e}");std::process::exit(1)}}}\n')
        env['CARGO_TARGET_DIR'] = str(base/'target')
        built = subprocess.run(['cargo', 'build', '--offline'], cwd=probe, env=env, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
        (out/'probe-build.log').write_text(built.stdout)
        assert built.returncode == 0, built.stdout
        binary = base/'target/debug/infra2-resolver-probe'
        tree = base/'archive'
        tree.mkdir()
        for name in source_manifest.ROOT_FILES:
            (tree/name).write_text('synthetic fixture\n')
        (tree/'rust-toolchain.toml').write_text('[toolchain]\nchannel = "1.89.0"\n')
        sha = tree/'crates/editor-core/src/hash.rs'
        sha.parent.mkdir(parents=True)
        sha.write_bytes((ROOT/'crates/editor-core/src/hash.rs').read_bytes())
        with patch.object(source_manifest, 'ROOT', tree):
            manifest = source_manifest.contents()
            paths = sorted(p.relative_to(tree).as_posix() for p in source_manifest.source_files())
        (tree/'MANIFEST.sha256').write_text(manifest)
        included = sorted(paths+['MANIFEST.sha256'])
        info = dict(schema_version=1, clean_worktree=True, supplemental_only=False,
                    git_commit='8512a7c3d1d9000f7374f1d1484d413bbb9b41cb',
                    commit='8512a7c3d1d9000f7374f1d1484d413bbb9b41cb',
                    source_manifest_sha256=hashlib.sha256(manifest.encode()).hexdigest(),
                    included_paths=included, manifest_count=len(included), source_file_count=len(paths))
        (tree/'PACKAGE_INFO.json').write_text(json.dumps(info))
        (tree/'PACKAGE_MANIFEST.sha256').write_text(''.join(
            f'{hashlib.sha256((tree/name).read_bytes()).hexdigest()}  {name}\n'
            for name in sorted(included+['PACKAGE_INFO.json'])))
        original = {p.relative_to(tree): p.read_bytes() for p in tree.rglob('*') if p.is_file()}
        def restore():
            for path, data in original.items():
                (tree/path).write_bytes(data)
        def check(label, success):
            result = subprocess.run([str(binary), str(tree)], env=env, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
            (out/f'{len(rows):02d}.log').write_text(result.stdout)
            row = dict(label=label, exit_code=result.returncode, expected=success,
                       matched=(result.returncode == 0) == success, output=result.stdout)
            rows.append(row)
            assert row['matched'], row
        check('restored archive', True)
        for name in ['PACKAGE_INFO.json', 'PACKAGE_MANIFEST.sha256', 'MANIFEST.sha256', 'Cargo.lock', 'crates/editor-core/src/hash.rs']:
            (tree/name).unlink()
            check('missing '+name, False)
            restore()
            (tree/name).write_bytes(b'corrupt')
            check('corrupt '+name, False)
            restore()
        new = tree/'crates/unlisted.rs'
        new.write_text('// unlisted')
        check('unlisted source', False)
        new.unlink()
        new.symlink_to('../README.md')
        check('unlisted symlink', False)
        new.unlink()
        p = tree/'PACKAGE_MANIFEST.sha256'
        for label, path in [('traversal', '../escape.rs'), ('absolute', str(base/'escape.rs')), ('backslash', r'crates\demo.rs')]:
            p.write_bytes(original[Path('PACKAGE_MANIFEST.sha256')]+('0'*64+'  '+path+'\n').encode())
            check(label, False)
            restore()
        p.write_bytes(original[Path('PACKAGE_MANIFEST.sha256')]+original[Path('PACKAGE_MANIFEST.sha256')].splitlines(keepends=True)[0])
        check('duplicate', False)
        restore()
        (tree/'.git').write_text('gitdir: missing\n')
        check('broken own git regular marker', False)
        (tree/'.git').unlink()
        check('final restored archive', True)
        assert len(rows) == 19
        (out/'mutation-results.json').write_text(json.dumps(rows, indent=2)+'\n')
        matrix = []
        def compare(label, expected, value=None, field=None, source_crlf=None, separator=None):
            restore()
            metadata = dict(info)
            if field == 'root':
                metadata = value
            elif field is not None:
                metadata[field] = value
            if source_crlf is not None:
                p = tree/'MANIFEST.sha256'
                p.write_bytes(original[Path('MANIFEST.sha256')].replace(b'\n', b'\r\n'))
                if source_crlf:
                    metadata['source_manifest_sha256'] = hashlib.sha256(p.read_bytes()).hexdigest()
            (tree/'PACKAGE_INFO.json').write_text(json.dumps(metadata))
            p = tree/'PACKAGE_MANIFEST.sha256'
            framed = ''.join(f'{hashlib.sha256((tree/name).read_bytes()).hexdigest()}  {name}\n'
                             for name in sorted(included+['PACKAGE_INFO.json']))
            if separator is not None:
                framed = framed.replace('\n', separator)
            p.write_bytes(framed.encode())
            rust = subprocess.run([str(binary), str(tree)], env=env, text=True,
                                  stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
            destination = base/f'matrix-{len(matrix):03d}.zip'
            error = None
            try:
                with patch.object(source_manifest, 'ROOT', tree):
                    package_source.package(destination)
            except (ValueError, OSError) as exc:
                error = str(exc)
            python_ok = error is None
            row = dict(label=label, expected=expected, rust_exit=rust.returncode,
                       python_accepted=python_ok, destination_exists=destination.exists(),
                       python_error=error, rust_output=rust.stdout)
            matrix.append(row)
            (out/'metadata-matrix.json').write_text(json.dumps(matrix, indent=2)+'\n')
            assert (rust.returncode == 0) == python_ok == expected, row
            assert destination.exists() == expected, row
            if expected:
                extracted = base/f'repacked-{len(matrix):03d}'
                with zipfile.ZipFile(destination) as z:
                    z.extractall(extracted)
                    assert type(json.loads(z.read('PACKAGE_INFO.json'))['schema_version']) is int
                rebuilt = subprocess.run([str(binary), str(extracted)], env=env, text=True,
                                         stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
                assert rebuilt.returncode == 0 and 'archive-verified' in rebuilt.stdout, rebuilt.stdout
        compare('valid integer schema and counts', True)
        for v in [True, False, 1.0, '1', None, [], {}, 2, -1]:
            compare('schema '+repr(v), False, v, 'schema_version')
        for f in ['source_file_count', 'manifest_count']:
            for v in [True, False, float(info[f]), str(info[f]), None, [], {}, -1, 1 << 64]:
                compare(f+' '+repr(v), False, v, f)
        for f in ['clean_worktree', 'supplemental_only']:
            for v in [0, 1, 0.0, 1.0, 'true', 'false', None, [], {}]:
                compare(f+' '+repr(v), False, v, f)
        for f in ['git_commit', 'commit', 'source_manifest_sha256']:
            for v in [True, False, 1, 1.0, None, [], {}]:
                compare(f+' '+repr(v), False, v, f)
        for v in [None, True, 1, 1.0, 'paths', {}, [1], [None]]:
            compare('included_paths '+repr(v), False, v, 'included_paths')
        for v in [True, False, 1, 1.0, None, [], 'info']:
            compare('root '+repr(v), False, v, 'root')
        for v in [float('nan'), float('inf'), '\ud800']:
            compare('non-JSON/scalar extra '+repr(v), False, v, 'extra')
        compare('finite large integer extra', True, 1 << 80, 'extra')
        compare('out-of-range integer extra', False, 10 ** 400, 'extra')
        for containers in [126, 127]:
            value = None
            for _ in range(containers):
                value = [value]
            compare('JSON container depth '+str(containers+1), containers == 126, value, 'extra')
        compare('CRLF source with raw byte digest', True, source_crlf=True)
        compare('CRLF source with stale LF digest', False, source_crlf=False)
        for sep in ['\r\n', '\r', '\v', '\x85', '\u2028']:
            compare('package framing '+repr(sep), sep == '\r\n', separator=sep)
        restore()
    assert len(rows) == 19
    (out/'mutation-results.json').write_text(json.dumps(rows, indent=2)+'\n')
    print('PASS 19/19 independent-review mutation scenarios; synthetic fixture')
    print(f'PASS {len(matrix)} Rust/Python metadata boundary cases; rejected targets absent')

if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--out', type=Path, required=True)
    run(parser.parse_args().out.resolve())
