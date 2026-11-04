@echo off
setlocal
chcp 65001 >nul

set "HUDR2_BIN=%~dp0target\release\hudr2.exe"

if not exist "%HUDR2_BIN%" (
    echo [ERROR] No se encontró el binario compilado en: %HUDR2_BIN%
    echo Ejecuta primero: cargo build --release
    exit /b 1
)

:: Intentar acoplar a un split lateral si herdr está corriendo
where herdr >nul 2>nul
if %ERRORLEVEL% EQU 0 (
    for /f "delims=" %%i in ('herdr pane split --current --direction right --ratio 0.80 --no-focus 2^>nul') do (
        set "SPLIT_OUT=%%i"
    )
)

:: Si no se pudo hacer split automático o se ejecuta en su propio pane:
"%HUDR2_BIN%" %*
