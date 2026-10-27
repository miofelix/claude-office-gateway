#!/usr/bin/env python3
from pathlib import Path
import argparse
import json
import shutil
import subprocess
from release_common import PLATFORMS, app_version, sha256
root = Path(__file__).resolve().parents[1]
parser=argparse.ArgumentParser()
parser.add_argument('--target',required=True,choices=PLATFORMS)
parser.add_argument('--output',default='artifacts/release')
args=parser.parse_args()
platform,target_key=PLATFORMS[args.target]
version=app_version(root)
bundle=root/'src-tauri/target'/args.target/'release/bundle'
output=root/args.output/platform
output.mkdir(parents=True,exist_ok=True)
prefix=f'Claude-Office-Gateway_{version}_{platform}'
assets=[];updates={}
def unique(folder,pattern):
    matches=list((bundle/folder).glob(pattern))
    if len(matches)!=1:raise SystemExit(f'Expected one {folder}/{pattern}, got {len(matches)}')
    return matches[0]
def copy(source,name,*,signed=False):
    destination=output/name
    shutil.copy2(source,destination);assets.append(name)
    if signed:
        signature=Path(str(source)+'.sig')
        if not signature.exists():raise SystemExit('Missing mandatory updater signature')
        subprocess.run(['cargo','run','--quiet','--locked','--manifest-path',str(root/'tools/update-verifier/Cargo.toml'),'--',str(root/'src-tauri/tauri.conf.json'),str(signature),str(source)],check=True)
        shutil.copy2(signature,Path(str(destination)+'.sig'));assets.append(name+'.sig')
        return {'file':name,'signature':signature.read_text().strip()}
if platform.startswith('macos-'):
    app=unique('macos','*.app')
    subprocess.run(['codesign','--verify','--deep','--strict',str(app)],check=True)
    copy(unique('dmg',f'*{version}*.dmg'),prefix+'.dmg')
    updates[target_key]=copy(unique('macos','*.app.tar.gz'),prefix+'.app.tar.gz',signed=True)
else:
    updates[target_key+'-nsis']=copy(unique('nsis',f'*{version}*.exe'),prefix+'-setup.exe',signed=True)
    updates[target_key+'-msi']=copy(unique('msi',f'*{version}*.msi'),prefix+'.msi',signed=True)
    updates[target_key]=updates[target_key+'-nsis']
record={'version':version,'platform':platform,'target':args.target,'updates':updates,'assets':{name:sha256(output/name) for name in assets}}
(output/f'platform-info-{platform}.json').write_text(json.dumps(record,indent=2)+'\n')
print(output)
