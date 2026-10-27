#!/usr/bin/env bash
set -euo pipefail
PROJECT_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$PROJECT_ROOT"
if [[ "$(uname -s)" != Darwin ]]; then
    printf '%s\n' 'Run this script on macOS.' >&2
    exit 1
fi
npm run build -- --bundles app,dmg
python3 scripts/collect-macos.py "$@"
