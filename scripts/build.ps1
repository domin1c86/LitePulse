$ErrorActionPreference = "Stop"

$root = Split-Path -Parent $PSScriptRoot
Push-Location $root
try {
    Write-Host "== format =="
    cargo fmt --all -- --check

    Write-Host "== tests =="
    cargo test

    Write-Host "== clippy =="
    cargo clippy --all-targets --features console -- -D warnings

    Write-Host "== release =="
    cargo build --release

    New-Item -ItemType Directory -Force -Path "dist" | Out-Null
    Copy-Item "target\release\litepulse.exe" "dist\litepulse.exe" -Force
    Write-Host "Built: $root\dist\litepulse.exe"
}
finally {
    Pop-Location
}
