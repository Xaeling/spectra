@echo off
setlocal
pushd "%~dp0"

REM ============================================================
REM  Spectra installer build script  (ASCII only, CRLF, no chcp)
REM  Produces ONE self-contained file: dist\spectra-installer.exe
REM ============================================================

set "LOG=%~dp0build.log"
echo. > "%LOG%"
call :log "=== Spectra installer build ==="
call :log "bat dir  : %~dp0"

echo ==================================================
echo   Spectra Installer - build
echo ==================================================
echo.
echo   Output: ONE self-contained spectra-installer.exe
echo           (main program is embedded at compile time)
echo.

REM ---- locate the main program source dir -------------------
set "MAIN="
if exist "%~dp0..\spectra\Cargo.toml"    set "MAIN=%~dp0..\spectra"
if not defined MAIN if exist "%~dp0..\prism-rs\Cargo.toml" set "MAIN=%~dp0..\prism-rs"
if not defined MAIN if exist "%~dp0..\Spectra\Cargo.toml"  set "MAIN=%~dp0..\Spectra"

set "PAYLOAD=%~1"
if "%PAYLOAD%"=="" (
    if defined MAIN set "PAYLOAD=%MAIN%\target\release"
)

call :log "main dir : %MAIN%"
call :log "payload  : %PAYLOAD%"

echo   main dir : %MAIN%
echo   payload  : %PAYLOAD%
echo.

REM ---- [0/3] check cargo ------------------------------------
echo [0/3] checking cargo
where cargo >nul 2>nul
if errorlevel 1 (
    call :log "ERROR: cargo not found in PATH"
    echo.
    echo [X] cargo.exe was NOT found in PATH.
    echo.
    echo     If cargo works in a normal CMD window but not here,
    echo     run this script FROM that CMD window instead of
    echo     double-clicking it.
    echo.
    echo     Or add this to PATH:  %USERPROFILE%\.cargo\bin
    echo.
    pause
    popd
    exit /b 1
)
cargo --version
call :log "cargo ok"
echo       ok
echo.

REM ---- [1/3] build the main program -------------------------
echo [1/3] build spectra (main program)
if /i "%SKIP_MAIN%"=="1" (
    echo       skipped (SKIP_MAIN=1)
    call :log "main build skipped"
) else (
    if not defined MAIN (
        call :log "ERROR: main project Cargo.toml not found"
        echo.
        echo [X] Could not find the main project.
        echo     Looked for Cargo.toml in:
        echo       %~dp0..\spectra
        echo       %~dp0..\prism-rs
        echo.
        echo     Fix: put this installer folder next to the main
        echo     project folder; or run it as:
        echo       build.bat "D:\project\spectra\target\release"
        echo.
        pause
        popd
        exit /b 1
    )
    pushd "%MAIN%"
    cargo build --release
    if errorlevel 1 (
        popd
        call :log "ERROR: main build failed"
        echo.
        echo [X] main program build failed - see errors above.
        echo     Most common cause: Spectra is still RUNNING.
        echo     Close the program, then re-run this script.
        echo.
        pause
        popd
        exit /b 1
    )
    popd
    call :log "main build ok"
    echo       ok
)

REM ---- [2/3] build the installer ----------------------------
echo.
echo [2/3] cargo build --release (installer, embeds spectra.exe)
if exist "%PAYLOAD%\spectra.exe" (
    set "SPECTRA_EXE=%PAYLOAD%\spectra.exe"
) else if defined MAIN if exist "%MAIN%\target\release\spectra.exe" (
    set "SPECTRA_EXE=%MAIN%\target\release\spectra.exe"
) else if defined MAIN if exist "%MAIN%\target\release\prism-rs.exe" (
    set "SPECTRA_EXE=%MAIN%\target\release\prism-rs.exe"
)
if defined SPECTRA_EXE (
    echo       embedding: %SPECTRA_EXE%
    call :log "embed    : %SPECTRA_EXE%"
) else (
    echo       [!] spectra.exe not found - will embed a stub
    call :log "WARN: spectra.exe not found"
)
cargo build --release
if errorlevel 1 (
    call :log "ERROR: installer build failed"
    echo.
    echo [X] installer build failed - see errors above.
    echo.
    pause
    popd
    exit /b 1
)
call :log "installer build ok"

REM ---- [3/3] publish ----------------------------------------
echo.
echo [3/3] publish dist\
if exist dist rmdir /s /q dist
mkdir dist
copy /y target\release\spectra-installer.exe dist\ >nul
if errorlevel 1 (
    call :log "ERROR: copy to dist failed"
    echo.
    echo [X] could not copy spectra-installer.exe to dist\
    echo.
    pause
    popd
    exit /b 1
)

set "SZ=0"
for %%F in ("dist\spectra-installer.exe") do set "SZ=%%~zF"
if %SZ% LSS 3000000 (
    call :log "ERROR: exe too small - main program NOT embedded"
    echo.
    echo [X] spectra-installer.exe is only %SZ% bytes.
    echo     The main program was NOT embedded - this installer
    echo     would produce a broken install. Do NOT send it.
    echo.
    echo     Build the main program first, then re-run this script.
    echo.
    pause
    popd
    exit /b 1
)
echo       + spectra-installer.exe  (%SZ% bytes, main program embedded)
call :log "published: %SZ% bytes"

echo.
echo   done.
echo.
echo   dist\spectra-installer.exe  - ONE file, send it as-is.
echo.
echo   Right click ^> Run as administrator, or just double
echo   click (it asks for elevation by itself).
echo.

where tar >nul 2>nul
if not errorlevel 1 (
    if exist Spectra-Setup.zip del /q Spectra-Setup.zip
    tar -a -c -f Spectra-Setup.zip -C dist spectra-installer.exe
    if not errorlevel 1 (
        echo   [+] Spectra-Setup.zip  - ready to send (1 file inside)
        call :log "zip      : Spectra-Setup.zip"
    )
)

echo.
echo   Tips:
echo     SKIP_MAIN=1   skip rebuilding the main program
echo     build.bat "D:\project\spectra\target\release"
echo.
echo   A copy of this output is in build.log
echo.
call :log "=== done ==="
pause
popd
endlocal
exit /b 0

:log
echo %~1>> "%LOG%"
goto :eof
