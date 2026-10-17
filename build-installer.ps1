# Builds ZenProbe and packs it into dist\ZenProbe-Setup-<version>.exe
$ErrorActionPreference = 'Stop'
Set-Location $PSScriptRoot
cargo build --release
if ($LASTEXITCODE -ne 0) { throw 'ZenProbe build failed' }
cargo build --release --manifest-path installer\Cargo.toml
if ($LASTEXITCODE -ne 0) { throw 'installer build failed' }
$version = (Select-String -Path installer\Cargo.toml -Pattern '^version = "(.+)"').Matches[0].Groups[1].Value
New-Item -ItemType Directory -Force dist | Out-Null
$out = "dist\ZenProbe-Setup-$version.exe"
Copy-Item installer\target\release\zenprobe-setup.exe $out -Force
"Installer: $(Resolve-Path $out) ($([math]::Round((Get-Item $out).Length / 1MB, 1)) MB)"
