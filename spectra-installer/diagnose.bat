@echo off
setlocal
pushd "%~dp0"

echo ==================================================
echo   Spectra installer - diagnostics
echo ==================================================
echo.
echo   [1] this script is at:
echo       %~dp0
echo.
echo   [2] parent folder contains:
dir /b "%~dp0.." 2>&1
echo.
echo   [3] main project Cargo.toml:
if exist "%~dp0..\spectra\Cargo.toml"    echo       FOUND  ..\spectra\Cargo.toml
if exist "%~dp0..\prism-rs\Cargo.toml"   echo       FOUND  ..\prism-rs\Cargo.toml
if not exist "%~dp0..\spectra\Cargo.toml" if not exist "%~dp0..\prism-rs\Cargo.toml" echo       NOT FOUND
echo.
echo   [4] cargo / rustc:
where cargo 2>&1
cargo --version 2>&1
rustc --version 2>&1
echo.
echo   [5] previous build.log (last 25 lines):
if exist "%~dp0build.log" (
    powershell -NoProfile -Command "Get-Content '%~dp0build.log' | Select-Object -Last 25"
) else (
    echo       (no build.log yet)
)
echo.
echo   [6] dist folder:
if exist "%~dp0dist" (
    dir /b "%~dp0dist"
) else (
    echo       (no dist folder yet)
)
echo.
echo ==================================================
echo   Copy everything above and send it to me.
echo ==================================================
echo.
pause
popd
endlocal
