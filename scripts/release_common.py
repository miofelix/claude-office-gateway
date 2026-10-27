"""Shared, offline release validation and metadata construction."""
from pathlib import Path
import hashlib
import json
import re

VERSION_PATTERN = re.compile(r'^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)(?:-((?:0|[1-9]\d*|[A-Za-z-][0-9A-Za-z-]*)(?:\.(?:0|[1-9]\d*|[A-Za-z-][0-9A-Za-z-]*))*))?$')
PLATFORMS = {
    'aarch64-apple-darwin': ('macos-aarch64', 'darwin-aarch64'),
    'x86_64-pc-windows-msvc': ('windows-x86_64', 'windows-x86_64'),
}

def parse_version(value):
    match = VERSION_PATTERN.fullmatch(value)
    if not match:
        raise ValueError(f'Invalid release version: {value}')
    return tuple(int(match[i]) for i in (1, 2, 3)), tuple((match[4] or '').split('.')) if match[4] else ()

def compare_versions(a, b):
    core_a, pre_a = parse_version(a)
    core_b, pre_b = parse_version(b)
    if core_a != core_b:
        return (core_a > core_b) - (core_a < core_b)
    if not pre_a or not pre_b:
        return (not pre_a) - (not pre_b)
    for x, y in zip(pre_a, pre_b):
        if x == y:
            continue
        if x.isdigit() and y.isdigit():
            return (int(x) > int(y)) - (int(x) < int(y))
        if x.isdigit() != y.isdigit():
            return -1 if x.isdigit() else 1
        return (x > y) - (x < y)
    return (len(pre_a) > len(pre_b)) - (len(pre_a) < len(pre_b))

def msi_version(version):
    """MSI compares only three numeric components; reserve channel ordering in patch."""
    (major, minor, patch), pre = parse_version(version)
    if major > 255 or minor > 255 or patch > 255:
        raise ValueError('MSI mapping requires major, minor and patch <= 255')
    if not pre:
        slot = 255
    elif len(pre) == 2 and pre[0] in ('beta', 'rc') and pre[1].isdigit() and 1 <= int(pre[1]) <= 127:
        slot = int(pre[1]) + (127 if pre[0] == 'rc' else 0)
    else:
        raise ValueError('Releases support beta.N, rc.N (1..127), or stable versions')
    return f'{major}.{minor}.{patch * 256 + slot}'

def app_version(root):
    return json.loads((root / 'package.json').read_text())['version']

def sha256(path):
    digest = hashlib.sha256()
    with path.open('rb') as file:
        for chunk in iter(lambda: file.read(1024 * 1024), b''):
            digest.update(chunk)
    return digest.hexdigest()

def build_manifest(version, repo, records, notes, published_at):
    parse_version(version)
    expected = {item[0] for item in PLATFORMS.values()}
    if {record['platform'] for record in records} != expected or len(records) != len(expected):
        raise ValueError('A release requires exactly macOS ARM64 and Windows x64')
    platforms = {}
    for record in records:
        if record['version'] != version:
            raise ValueError('Mixed application versions in release assets')
        for target, entry in record['updates'].items():
            if target in platforms or not entry['signature'].strip():
                raise ValueError('Duplicate platform or missing updater signature')
            filename = entry['file']
            if Path(filename).name != filename:
                raise ValueError('Updater asset must be a basename')
            platforms[target] = {'url': f'https://github.com/{repo}/releases/download/v{version}/{filename}', 'signature': entry['signature'].strip()}
    required = {'darwin-aarch64', 'windows-x86_64', 'windows-x86_64-nsis', 'windows-x86_64-msi'}
    if set(platforms) != required:
        raise ValueError('Missing signed updater installer variants')
    return {'version': version, 'notes': notes, 'pub_date': published_at, 'platforms': platforms}
