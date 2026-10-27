$ErrorActionPreference = 'Stop'
Set-Location (Split-Path $PSScriptRoot -Parent)
if ($env:OS -ne 'Windows_NT') { throw 'Run this script on Windows.' }
& npm run build -- --bundles nsis,msi
if ($LASTEXITCODE -ne 0) { throw 'Windows packaging failed.' }
$destination = Join-Path (Get-Location) 'artifacts/windows'
New-Item -ItemType Directory -Force -Path $destination | Out-Null
Get-ChildItem 'src-tauri/target/release/bundle/nsis/*.exe', 'src-tauri/target/release/bundle/msi/*.msi' |
    Copy-Item -Destination $destination -Force
Get-ChildItem $destination -File | Where-Object { $_.Extension -in '.exe', '.msi' } |
    Get-FileHash -Algorithm SHA256 | Format-Table -AutoSize
Write-Output $destination
