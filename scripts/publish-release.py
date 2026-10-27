#!/usr/bin/env python3
"""Publish verified native assets, then atomically advance the update feeds."""
from pathlib import Path
from datetime import datetime, timezone
import argparse
import base64
import json
import os
import subprocess
import tempfile
from release_common import app_version, build_manifest, compare_versions, parse_version, sha256

root = Path(__file__).resolve().parents[1]

def gh(*args, data=None, missing=False):
    command = ['gh', *args]
    if data is not None:
        command += ['--input', '-']
    result = subprocess.run(command, input=json.dumps(data) if data is not None else None,
                            text=True, capture_output=True, cwd=root)
    if result.returncode:
        if missing and 'HTTP 404' in result.stderr:
            return None
        raise RuntimeError(result.stderr.strip() or 'GitHub operation failed')
    return result.stdout

def api(path, data=None, missing=False, method=None):
    arguments = ["--method", method] if method else []
    result = gh('api', *arguments, path, data=data, missing=missing)
    return json.loads(result) if result else None

def existing_feed(repo, name):
    response = api(f'repos/{repo}/contents/{name}.json?ref=updates', missing=True)
    return json.loads(base64.b64decode(response['content'])) if response else None

def advance_feeds(repo, manifest):
    current = api(f'repos/{repo}/git/ref/heads/updates', missing=True)
    files = {}
    channels = ['beta'] if parse_version(manifest['version'])[1] else ['beta', 'stable']
    for name in channels:
        previous = existing_feed(repo, name) if current else None
        if previous:
            order = compare_versions(manifest['version'], previous['version'])
            if order < 0:
                print(f'Preserving newer {name} channel {previous["version"]}')
                continue
            if order == 0:
                if previous != manifest:
                    raise RuntimeError(f'Refusing to change the published bytes of {name} {manifest["version"]}')
                continue
        files[name + '.json'] = json.dumps(manifest, indent=2) + '\n'
    if not files:
        return
    payload = {'tree': [{'path': name, 'mode': '100644', 'type': 'blob', 'content': content}
                        for name, content in files.items()]}
    if current:
        commit = api(f'repos/{repo}/git/commits/{current["object"]["sha"]}')
        payload['base_tree'] = commit['tree']['sha']
    tree = api(f'repos/{repo}/git/trees', payload)
    commit = api(f'repos/{repo}/git/commits', {
        'message': f'Publish signed update feeds for v{manifest["version"]}',
        'tree': tree['sha'], 'parents': [current['object']['sha']] if current else [],
    })
    if current:
        # Non-forced update fails if another publisher advanced the branch.
        api(f'repos/{repo}/git/refs/heads/updates', {'sha': commit['sha'], 'force': False}, method='PATCH')
    else:
        api(f'repos/{repo}/git/refs', {'ref': 'refs/heads/updates', 'sha': commit['sha']})

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--source', required=True)
    parser.add_argument('--assets', default='artifacts/release')
    args = parser.parse_args()
    if len(args.source) != 40 or any(char not in '0123456789abcdef' for char in args.source):
        raise ValueError('An exact source commit SHA is required')
    repo = os.environ.get('GITHUB_REPOSITORY', 'miofelix/claude-office-gateway')
    if repo != 'miofelix/claude-office-gateway':
        raise ValueError('Release publishing is restricted to the configured repository')
    subprocess.run(['python3', 'scripts/check-release.py'], check=True, cwd=root)
    version = app_version(root)
    tag = 'v' + version
    notes_path = root / 'docs/releases' / (tag + '.md')
    notes = notes_path.read_text()
    records = []
    assets = {}
    for record_path in (root / args.assets).rglob('platform-info-*.json'):
        record = json.loads(record_path.read_text())
        records.append(record)
        for name, expected_hash in record['assets'].items():
            if Path(name).name != name or name in assets:
                raise ValueError('Invalid or duplicate release asset name')
            file = record_path.parent / name
            if sha256(file) != expected_hash:
                raise ValueError(f'Artifact checksum mismatch: {name}')
            assets[name] = file
        for entry in record['updates'].values():
            file = assets[entry['file']]
            signature = assets[entry['file'] + '.sig']
            if signature.read_text().strip() != entry['signature'].strip():
                raise ValueError('Metadata signature differs from signature asset')
            subprocess.run(['cargo', 'run', '--quiet', '--locked', '--manifest-path',
                            str(root / 'tools/update-verifier/Cargo.toml'), '--',
                            str(root / 'src-tauri/tauri.conf.json'), str(signature), str(file)], check=True)
    manifest = build_manifest(version, repo, records, notes, datetime.now(timezone.utc).isoformat())
    release = api(f'repos/{repo}/releases/tags/{tag}', missing=True)
    tag_ref = api(f'repos/{repo}/git/ref/tags/{tag}', missing=True)
    if tag_ref:
        obj = tag_ref['object']
        while obj['type'] == 'tag':
            obj = api(f'repos/{repo}/git/tags/{obj["sha"]}')['object']
        if obj['sha'] != args.source:
            raise ValueError('Release tag points at a different source commit')
    checksums = ''.join(f'{sha256(file)}  {name}\n' for name, file in sorted(assets.items()))
    with tempfile.TemporaryDirectory() as temporary:
        temporary = Path(temporary)
        if release and not release['draft']:
            if not tag_ref:
                raise ValueError('Published release has no verifiable source tag')
            gh('release', 'download', tag, '--repo', repo, '--pattern', 'latest.json', '--pattern', 'SHA256SUMS', '--dir', str(temporary))
            published = json.loads((temporary / 'latest.json').read_text())
            if any(published.get(key) != manifest[key] for key in ('version', 'notes', 'platforms')) or (temporary / 'SHA256SUMS').read_text() != checksums:
                raise ValueError('Published release is immutable; use a new version')
            manifest = published
        else:
            if release and release['target_commitish'] != args.source:
                raise ValueError('Draft belongs to a different source commit')
            manifest_file = temporary / 'latest.json'
            manifest_file.write_text(json.dumps(manifest, indent=2) + '\n')
            checksum_file = temporary / 'SHA256SUMS'
            checksum_file.write_text(checksums)
            if not release:
                flags = ['--prerelease'] if parse_version(version)[1] else []
                gh('release', 'create', tag, '--repo', repo, '--target', args.source, '--draft',
                   '--title', f'Claude Office Gateway {version}', '--notes-file', str(notes_path), *flags)
            gh('release', 'upload', tag, '--repo', repo, '--clobber',
               *(str(file) for file in assets.values()), str(manifest_file), str(checksum_file))
            gh('release', 'edit', tag, '--repo', repo, '--draft=false',
               '--latest=false' if parse_version(version)[1] else '--latest')
    advance_feeds(repo, manifest)
    print(f'Published https://github.com/{repo}/releases/tag/{tag}')

if __name__ == '__main__':
    main()
