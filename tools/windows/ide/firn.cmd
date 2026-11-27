@echo off
powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0firn.ps1" %*
exit /b %ERRORLEVEL%
