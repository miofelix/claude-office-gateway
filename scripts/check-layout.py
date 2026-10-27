#!/usr/bin/env python3
"""Run real WKWebView layout regressions against an isolated Vite fixture."""
from pathlib import Path
import socket
import subprocess
import sys
import tempfile
import time
from urllib.error import URLError
from urllib.request import urlopen

if sys.platform != 'darwin':
    raise SystemExit('The native WKWebView layout check requires macOS')

root = Path(__file__).resolve().parents[1]
with socket.socket() as reservation:
    reservation.bind(('127.0.0.1', 0))
    port = reservation.getsockname()[1]
base_url = f'http://127.0.0.1:{port}'
with tempfile.TemporaryFile(mode='w+') as log:
    server = subprocess.Popen(
        ['node', 'node_modules/vite/bin/vite.js', '--host', '127.0.0.1', '--port', str(port)],
        cwd=root, stdout=log, stderr=subprocess.STDOUT,
    )
    try:
        for _ in range(100):
            if server.poll() is not None:
                log.seek(0)
                raise RuntimeError(log.read())
            try:
                with urlopen(base_url, timeout=1):
                    break
            except URLError:
                time.sleep(0.1)
        else:
            raise RuntimeError('Timed out waiting for the layout fixture server')
        subprocess.run(['swift', str(root / 'scripts/check-layout.swift'), base_url],
                       check=True, cwd=root, timeout=120)
    finally:
        server.terminate()
        try:
            server.wait(timeout=5)
        except subprocess.TimeoutExpired:
            server.kill()
            server.wait()
