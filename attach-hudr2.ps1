[CmdletBinding()]
param(
    [Parameter(ValueFromRemainingArguments = $true)]
    [string[]]$ArgsList
)

$scriptDir = $PSScriptRoot
$bin = Join-Path $scriptDir "target\release\hudr2.exe"
if (-not (Test-Path $bin)) {
    $bin = Join-Path $scriptDir "target\debug\hudr2.exe"
}

if (-not (Test-Path $bin)) {
    Write-Host "[ERROR] No se encontro el binario compilado de hudr2 en target\release o target\debug." -ForegroundColor Red
    Write-Host "Ejecuta primero: cargo build --release" -ForegroundColor Yellow
    exit 1
}

# Intentar acoplar a un split lateral si herdr esta corriendo
if (Get-Command herdr -ErrorAction SilentlyContinue) {
    try {
        & herdr pane split --current --direction right --ratio 0.80 --no-focus 2>$null
    } catch {
        # herdr no activo o split fallo
    }
}

& $bin @ArgsList
