"""Run the opt-in release A2 Metal matrix using explicit local inputs.
No private input is copied. Each case uses isolated settings/recovery/diagnostics.
Synthetic native egui input is disclosed; no physical input/scanout claim.
"""
import argparse
import datetime
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import time

CASES = ('A', 'B', 'C', 'D', 'E-undo', 'E-redo')

def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()

def write(path, value):
    path.write_text(json.dumps(value, ensure_ascii=False, indent=2)+'\n', encoding='utf-8')

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', required=True, type=Path)
    parser.add_argument('--evidence', required=True, type=Path)
    parser.add_argument('--fixture', required=True, type=Path)
    args = parser.parse_args()
    if sys.platform != 'darwin': parser.error('this native acceptance runner requires macOS Metal')
    if not args.binary.is_file() or not args.fixture.is_file(): parser.error('binary and fixture must exist')
    binary, fixture = args.binary.resolve(), args.fixture.resolve()
    out = args.evidence.resolve()
    out.mkdir(parents=True, exist_ok=False)
    binary_sha, fixture_sha = sha(binary), sha(fixture)
    # Keep identifying machine serials/UUIDs out of distributable evidence.
    hardware = json.loads(subprocess.check_output(['system_profiler','SPHardwareDataType','SPDisplaysDataType','-json']))
    hw = hardware['SPHardwareDataType'][0]; gpu = hardware['SPDisplaysDataType'][0]
    environment = {'macos':subprocess.check_output(['sw_vers','-productVersion'],text=True).strip(),
        'machine':hw['machine_model'],'cpu':hw['chip_type'],'ram':hw['physical_memory'],
        'gpu':gpu['sppci_model'],'displays':[{'name':d['_name'],'pixels':d.get('_spdisplays_pixels'),
        'resolution_refresh':d.get('spdisplays_resolution')} for d in gpu['spdisplays_ndrvs']]}
    write(out/'environment.json',environment)
    rows = []
    for round_number in (1,2,3):
        for case in CASES:
            name = f'round-{round_number}-{case}'
            directory = Path(tempfile.mkdtemp(prefix='rcam-a2-native-',dir='/tmp')).resolve()
            request = {'schema_version':2,'case':case,'round':round_number,'run_id':out.name+'-'+name,
                'fixture':str(fixture),'fixture_sha256':fixture_sha,
                'start_utc':datetime.datetime.now(datetime.timezone.utc).isoformat()}
            write(directory/'request.json',request)
            env = {k:v for k,v in os.environ.items() if not k.startswith('RCAM_')}
            env['RCAM_A2_NATIVE_DIR'] = str(directory)
            start = time.monotonic(); code = None; error = None; report = None
            try:
                with (directory/'stdout.log').open('w') as stdout, (directory/'stderr.log').open('w') as stderr:
                    process = subprocess.Popen([str(binary)],env=env,stdout=stdout,stderr=stderr)
                    try: code = process.wait(timeout=100)
                    except subprocess.TimeoutExpired:
                        process.terminate()
                        try: code = process.wait(timeout=10)
                        except subprocess.TimeoutExpired:
                            process.kill(); code = process.wait()
                        raise RuntimeError('native case exceeded timeout')
                report = json.loads((directory/'observations.json').read_text())
                if code != 0 or report['failures']: raise RuntimeError('native process/case failed')
                if sha(binary) != binary_sha or report['binary_sha256'] != binary_sha or sha(fixture) != fixture_sha:
                    raise RuntimeError('binary or fixture changed during native matrix')
            except (OSError, ValueError, KeyError, RuntimeError) as exc:
                error = str(exc)
            finally:
                write(directory/'runner.json',{'exit_code':code,'error':error,'binary_sha256':binary_sha,
                    'duration_seconds':time.monotonic()-start,'fixture_sha256':fixture_sha})
                shutil.copytree(directory,out/name)
                # PPM is the original GPU surface readback; PNG is convenience only.
                for ppm in (out/name).glob('*.ppm'):
                    subprocess.run(['sips','-s','format','png',str(ppm),'--out',str(ppm.with_suffix('.png'))],
                                   stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL,check=True)
                rows.append({'round':round_number,'case':case,'observations':name+'/observations.json'})
                write(out/'native-index.json',rows)
            print(json.dumps({'case':case,'round':round_number,'error':error,
                'feedback_ms':report and report['feedback_upper_bound_ms'],
                'worker_ms':report and report['worker_return_upper_bound_ms']},ensure_ascii=False),flush=True)
            if error: return 1
    return 0

if __name__ == '__main__': sys.exit(main())
