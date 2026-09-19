"""Create an unsigned macOS test app from an already built release executable."""
import argparse
from pathlib import Path
import plistlib
import shutil
import sys

ROOT = Path(__file__).resolve().parents[1]

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--out', type=Path, required=True)
    parser.add_argument('--binary', type=Path, default=ROOT / 'target/release/editor-app')
    args = parser.parse_args()
    if sys.platform != 'darwin':
        raise SystemExit('This package targets macOS only')
    bundle = args.out / 'RCam.app'
    if bundle.exists():
        raise SystemExit('Refusing to replace an existing package')
    executable = bundle / 'Contents/MacOS/editor-app'
    executable.parent.mkdir(parents=True)
    shutil.copy2(args.binary, executable)
    info = dict(CFBundleName='RCam', CFBundleDisplayName='RCam',
                CFBundleIdentifier='local.rcam.editor', CFBundleExecutable='editor-app',
                CFBundlePackageType='APPL', CFBundleShortVersionString='0.1.0',
                CFBundleVersion='212', NSHighResolutionCapable=True,
                LSMinimumSystemVersion='15.0', NSPrincipalClass='NSApplication')
    (bundle / 'Contents/Info.plist').write_bytes(plistlib.dumps(info))
    print(bundle.resolve())

if __name__ == '__main__':
    main()
