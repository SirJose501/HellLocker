# Builds HellLocker on Windows. Needs Rust (https://rustup.rs) and Visual Studio Build Tools
# with "Desktop development with C++". Double-click "Build HellLocker.bat", or run:
#   powershell -ExecutionPolicy Bypass -File .\Build-Windows.ps1
# The app ends up in the HellLocker folder here, and HellLocker-<version>.zip (that
# folder, zipped, ready to upload) next to it.
$ErrorActionPreference = 'Stop'
Set-Location $PSScriptRoot
if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
    Write-Host 'Rust/Cargo is not installed. Install it from https://rustup.rs/' -ForegroundColor Yellow
    Write-Host 'Also install Visual Studio Build Tools with "Desktop development with C++".'
    exit 1
}
# Keep personal folder paths (your user name, where the source lives) out of the exe:
# rustc replaces them with neutral names. CARGO_ENCODED_RUSTFLAGS takes the flags one
# per field, so paths with spaces work, and it replaces .cargo\config.toml's flags, so
# the static C runtime setting is repeated here.
$source = (Resolve-Path $PSScriptRoot).Path
$cargoHome = if ($env:CARGO_HOME) { $env:CARGO_HOME } else { Join-Path $env:USERPROFILE '.cargo' }
$rustupHome = if ($env:RUSTUP_HOME) { $env:RUSTUP_HOME } else { Join-Path $env:USERPROFILE '.rustup' }
$flags = @(
    '-C', 'target-feature=+crt-static',
    "--remap-path-prefix=$($env:USERPROFILE)=~",
    "--remap-path-prefix=$cargoHome=cargo",
    "--remap-path-prefix=$rustupHome=rustup",
    "--remap-path-prefix=$source=HellLocker"
)
$env:CARGO_ENCODED_RUSTFLAGS = $flags -join [char]0x1f
Write-Host 'Running tests...' -ForegroundColor Cyan
& cargo test --locked
if ($LASTEXITCODE -ne 0) { throw 'Tests failed; nothing was built.' }
Write-Host 'Building HellLocker (release)...' -ForegroundColor Cyan
& cargo build --release --locked
if ($LASTEXITCODE -ne 0) { throw 'Build failed.' }
$dest = Join-Path $PSScriptRoot 'HellLocker'
New-Item -ItemType Directory -Force -Path $dest | Out-Null
Copy-Item '.\target\release\HellLocker.exe' (Join-Path $dest 'HellLocker.exe') -Force
Copy-Item '.\LICENSE', '.\README.md' $dest -Force
# The upload zip: the HellLocker folder, named after the version in Cargo.toml.
$version = (Select-String -Path '.\Cargo.toml' -Pattern '^version = "(.+)"' | Select-Object -First 1).Matches[0].Groups[1].Value
$zip = Join-Path $PSScriptRoot "HellLocker-$version.zip"
if (Test-Path $zip) { Remove-Item $zip -Force }
Compress-Archive -Path $dest -DestinationPath $zip
Write-Host "HellLocker $version is ready: $dest\HellLocker.exe" -ForegroundColor Green
Write-Host "Upload zip: $zip" -ForegroundColor Green
Write-Host 'Its settings and presets are kept in %APPDATA%\HellLocker (created on first run; an old data folder next to it is moved there).'
if (-not $env:HELLLOCKER_NO_EXPLORER) { Start-Process explorer.exe "/select,`"$zip`"" }
