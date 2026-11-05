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

& $bin @ArgsList
