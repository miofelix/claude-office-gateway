#!/usr/bin/env python3
"""Build a signed official update; never print private key material."""
from pathlib import Path
import argparse
import os
import subprocess
import sys
from release_common import PLATFORMS
root=Path(__file__).resolve().parents[1]
parser=argparse.ArgumentParser(description=__doc__)
parser.add_argument('--target',required=True,choices=PLATFORMS)
args=parser.parse_args()
env=os.environ.copy()
if not env.get('TAURI_SIGNING_PRIVATE_KEY'):
    if sys.platform!='darwin':raise SystemExit('Official release signing credentials are required in the environment')
    key=Path.home()/'Library/Application Support/Claude Office Gateway Release/updater.key'
    if not key.exists():raise SystemExit('Local release signing key is not available')
    result=subprocess.run(['/usr/bin/security','find-generic-password','-s','com.miofelix.office-gateway.release','-a','updater-key-password','-w'],text=True,capture_output=True)
    if result.returncode:raise SystemExit('Cannot read release signing password from login Keychain')
    env['TAURI_SIGNING_PRIVATE_KEY']=key.read_text().strip()
    env['TAURI_SIGNING_PRIVATE_KEY_PASSWORD']=result.stdout.rstrip('\n')
subprocess.run([sys.executable,str(root/'scripts/check-release.py')],check=True,cwd=root)
bundles='app,dmg' if args.target.endswith('apple-darwin') else 'nsis,msi'
command=['npm.cmd' if os.name=='nt' else 'npm','run','tauri','--','build','--ci','--target',args.target,'--bundles',bundles]
subprocess.run(command,env=env,cwd=root,check=True)
subprocess.run([sys.executable,str(root/'scripts/collect-release.py'),'--target',args.target],check=True,cwd=root)
