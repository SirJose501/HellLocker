@echo off
rem HellLocker: double-click to test and build HellLocker, then make the upload zip.
rem Runs Build-Windows.ps1 in this folder; the window stays open so you can read the result.
title Building HellLocker
cd /d "%~dp0"
powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0Build-Windows.ps1"
if errorlevel 1 (
    echo.
    echo The build did not finish. The message above says why.
) else (
    echo.
    echo Done. The upload zip is selected in the window that just opened.
)
echo.
pause
