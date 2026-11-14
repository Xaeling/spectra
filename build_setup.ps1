# Build single-file app + offline installer.
$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $MyInvocation.MyCommand.Path
Set-Location $root

$env:CARGO_HOME = Join-Path (Split-Path $root -Parent) ".cargo-home"
if (-not (Test-Path $env:CARGO_HOME)) {
    throw "Missing offline dependency cache: $env:CARGO_HOME"
}

cargo build --release --offline
if ($LASTEXITCODE -ne 0) { throw "App build failed" }
$app = (Resolve-Path "target\release\stataura.exe").Path
$env:STATAURA_APP_EXE = $app

# Same static CRT for installer; run from project root to inherit .cargo/config.toml.
$env:RUSTFLAGS = "-C target-feature=+crt-static"
Remove-Item Env:STATAURA_SETUP_AS_INVOKER -ErrorAction SilentlyContinue
cargo build --release --offline --manifest-path installer/Cargo.toml
if ($LASTEXITCODE -ne 0) { throw "Installer build failed" }

$versionLine = Get-Content "Cargo.toml" | Where-Object { $_ -match '^version = "([^"]+)"' } | Select-Object -First 1
$version = [regex]::Match($versionLine, '"([^"]+)"').Groups[1].Value
$dist = Join-Path $root "dist"
New-Item -ItemType Directory -Force -Path $dist | Out-Null

$appOut = Join-Path $dist "StatAura-$version.exe"
$setupOut = Join-Path $dist "StatAura-Setup-$version.exe"
Copy-Item -Force $app $appOut
Copy-Item -Force "installer\target\release\stataura-setup.exe" $setupOut

Get-ChildItem $dist -Filter "*.exe" |
    ForEach-Object {
        $hash = (Get-FileHash -Algorithm SHA256 $_.FullName).Hash.ToLower()
        "$hash  $($_.Name)"
    } | Set-Content -Encoding ASCII (Join-Path $dist "SHA256SUMS.txt")

Get-ChildItem $dist -Filter "*.exe" | Select-Object Name, Length
Write-Host "Done: $dist"
