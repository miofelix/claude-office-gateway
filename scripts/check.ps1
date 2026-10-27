$ErrorActionPreference = 'Stop'
Set-Location (Split-Path $PSScriptRoot -Parent)
function Invoke-Checked {
    param([string]$Program, [string[]]$Arguments)
    & $Program @Arguments
    if ($LASTEXITCODE -ne 0) { throw "$Program failed with exit code $LASTEXITCODE" }
}
Invoke-Checked python @('scripts/check-release.py')
Invoke-Checked python @('-m', 'unittest', 'discover', '-s', 'scripts', '-p', 'test_*.py')
Invoke-Checked npm @('run', 'format:check')
Invoke-Checked npm @('run', 'check')
Invoke-Checked cargo @('fmt', '--manifest-path', 'src-tauri/Cargo.toml', '--', '--check')
Invoke-Checked cargo @('test', '--locked', '--manifest-path', 'src-tauri/Cargo.toml')
Invoke-Checked cargo @('clippy', '--locked', '--manifest-path', 'src-tauri/Cargo.toml', '--all-targets', '--', '-D', 'warnings')
