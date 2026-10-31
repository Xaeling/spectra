@echo off
REM Wrapper: keeps the window open no matter what build.bat does.
cmd /k ""%~dp0build.bat" %*"
