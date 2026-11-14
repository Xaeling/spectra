@echo off
setlocal
set "CARGO_HOME=%~dp0..\.cargo-home"
cargo %*
