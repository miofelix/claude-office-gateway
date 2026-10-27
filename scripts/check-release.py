#!/usr/bin/env python3
from pathlib import Path
import json
import os
import re
from release_common import app_version, msi_version, parse_version
root = Path(__file__).resolve().parents[1]
version = app_version(root)
parse_version(version)
lock = json.loads((root / 'package-lock.json').read_text())
config = json.loads((root / 'src-tauri/tauri.conf.json').read_text())
for actual in (lock['version'], lock['packages']['']['version'], config['version']):
    if actual != version:
        raise SystemExit('Application version files are out of sync')
for name, pattern in [('src-tauri/Cargo.toml', r'^version = "([^"]+)"'), ('src-tauri/Cargo.lock', r'name = "claude-office-gateway"\nversion = "([^"]+)"')]:
    match = re.search(pattern, (root / name).read_text(), re.M)
    if not match or match[1] != version:
        raise SystemExit(f'{name} application version is out of sync')
windows = json.loads((root / 'src-tauri/tauri.windows.conf.json').read_text())
if windows['bundle']['windows']['wix']['version'] != msi_version(version):
    raise SystemExit('Windows MSI numeric version mapping is out of sync')
if os.getenv('RELEASE_VERSION') and os.environ['RELEASE_VERSION'].removeprefix('v') != version:
    raise SystemExit('Requested release does not match the application version')
if not config['bundle'].get('createUpdaterArtifacts') or not config['plugins']['updater'].get('pubkey'):
    raise SystemExit('Signed updater artifacts and a public key are required')
print(version)
