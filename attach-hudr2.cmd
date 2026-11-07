@echo off
setlocal
chcp 65001 >nul

set "HUDR2_BIN=%~dp0target\release\hudr2.exe"
if not exist "%HUDR2_BIN%" set "HUDR2_BIN=%~dp0target\debug\hudr2.exe"

if not exist "%HUDR2_BIN%" (
    echo [ERROR] No se encontro el binario compilado en: %~dp0target\release\hudr2.exe
    echo Ejecuta primero: cargo build --release
    exit /b 1
)

"%HUDR2_BIN%" --attach %*
