#!/usr/bin/env python3
"""Collect and verify generated macOS bundles without touching user app data."""
from pathlib import Path
import hashlib
import shutil
import subprocess
import tempfile

root = Path(__file__).resolve().parents[1]
bundles = root / "src-tauri/target/release/bundle"
source = bundles / "macos/Claude Office Gateway.app"
subprocess.run(["codesign", "--verify", "--deep", "--strict", str(source)], check=True)
output = root / "artifacts/macos"
output.mkdir(parents=True, exist_ok=True)
with tempfile.TemporaryDirectory(prefix=".collect-", dir=output) as temporary:
    staging = Path(temporary)
    app = staging / source.name
    shutil.copytree(source, app, symlinks=True)
    previous = output / source.name
    if previous.exists():
        previous.rename(staging / "previous.app")
    app.rename(previous)
    subprocess.run(["codesign", "--verify", "--deep", "--strict", str(previous)], check=True)
checksums = []
for dmg in sorted((bundles / "dmg").glob("*.dmg")):
    destination = output / dmg.name
    shutil.copy2(dmg, destination)
    checksums.append(f"{hashlib.sha256(destination.read_bytes()).hexdigest()}  {dmg.name}")
(output / "SHA256SUMS").write_text("\n".join(checksums) + "\n")
print(output)
