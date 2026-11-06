param(
    [switch]$Quick
)

$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$SkillDir = Split-Path -Parent $ScriptDir
$RepoRoot = (Resolve-Path (Join-Path $SkillDir "..\..\..")).Path
Set-Location $RepoRoot

$Fails = 0
function Pass($msg) { Write-Host "PASS  $msg" -ForegroundColor Green }
function Fail($msg) { Write-Host "FAIL  $msg" -ForegroundColor Red; $script:Fails++ }
function Warn($msg) { Write-Host "WARN  $msg" -ForegroundColor Yellow }

if (-not (Test-Path (Join-Path $RepoRoot "Cargo.toml"))) {
    Fail "Cargo.toml not found at $RepoRoot"
    exit 1
}

if (-not $Quick) {
    $ErrorActionPreference = "Continue"
    $testOut = (& cargo test 2>&1) | Out-String
    if ($LASTEXITCODE -eq 0 -and $testOut -match "test result: ok") {
        Pass "cargo test passed"
    } else {
        Fail "cargo test failed"
        Write-Host $testOut
    }

    $clippyOut = (& cargo clippy --all-targets -- -D warnings 2>&1) | Out-String
    if ($LASTEXITCODE -eq 0) {
        Pass "cargo clippy --all-targets (0 warnings)"
    } else {
        Fail "cargo clippy reported warnings or errors"
        Write-Host $clippyOut
    }

    $buildOut = (& cargo build --release 2>&1) | Out-String
    if ($LASTEXITCODE -eq 0) {
        Pass "cargo build --release"
    } else {
        Fail "cargo build --release failed"
        Write-Host $buildOut
    }
}

# 1. Safety & Architecture Guards
$gitRs = Get-Content -Raw -Encoding UTF8 (Join-Path $RepoRoot "src\git.rs")
if ($gitRs -match 'GIT_OPTIONAL_LOCKS') {
    Pass "git lock-free guard (GIT_OPTIONAL_LOCKS=0)"
} else {
    Fail "src/git.rs missing GIT_OPTIONAL_LOCKS=0"
}

if ($gitRs -match 'pub fn is_sensitive_file') {
    Pass "git diff sensitive-file guard (is_sensitive_file)"
} else {
    Fail "src/git.rs missing is_sensitive_file guard"
}

$mainRs = Get-Content -Raw -Encoding UTF8 (Join-Path $RepoRoot "src\main.rs")
if ($mainRs -match '\?1000h\\x1b\[\?1006h' -and $mainRs -notmatch 'EnableMouseCapture') {
    Pass "button-only SGR mouse capture guard"
} else {
    Fail "src/main.rs must use button-only SGR mouse capture (?1000h+?1006h), not EnableMouseCapture"
}

$uiRs = Get-Content -Raw -Encoding UTF8 (Join-Path $RepoRoot "src\ui.rs")
if ($uiRs -match 'evaluate_account_for_model') {
    Pass "dynamic multi-model quota evaluation in Integraciones"
} else {
    Fail "src/ui.rs must use evaluate_account_for_model for multi-model quota health"
}

# 2. --once Snapshot & Frame Verification
$binPath = Join-Path $RepoRoot "target\release\hudr2.exe"
if (-not (Test-Path $binPath)) {
    $binPath = Join-Path $RepoRoot "target\debug\hudr2.exe"
}

if (Test-Path $binPath) {
    [Console]::OutputEncoding = [System.Text.Encoding]::UTF8
    $snapLines = & $binPath --once --cwd $RepoRoot 2>$null
    if ($LASTEXITCODE -eq 0 -and $snapLines.Count -gt 10) {
        $snapText = $snapLines -join "`n"
        $expectedCards = @(
            "Estado",
            "Contexto",
            "Proyecto",
            "Integraciones",
            "Cuota de Modelos",
            "Cambios",
            "Engram",
            "Stack",
            "Herramientas",
            "MCP"
        )
        $missingCards = @()
        foreach ($card in $expectedCards) {
            if ($snapText -notmatch [regex]::Escape($card)) {
                $missingCards += $card
            }
        }
        if ($missingCards.Count -eq 0) {
            Pass "--once renders all $($expectedCards.Count) catalog cards"
        } else {
            Fail "--once missing cards: $($missingCards -join ', ')"
        }

        $leftBorders = @(0x256D, 0x2502, 0x251C, 0x2570)
        $rightBorders = @(0x256E, 0x2502, 0x2524, 0x256F)
        $badFrames = @()
        for ($i = 0; $i -lt $snapLines.Count; $i++) {
            $line = $snapLines[$i]
            if ($line.Length -gt 1) {
                $firstCode = [int][char]$line[0]
                $lastCode = [int][char]$line[$line.Length - 1]
                if (($leftBorders -contains $firstCode) -and -not ($rightBorders -contains $lastCode)) {
                    $badFrames += "Line $($i + 1): $line"
                }
            }
        }
        if ($badFrames.Count -eq 0) {
            Pass "--once box-drawing frame integrity"
        } else {
            Fail "--once broken box borders:`n$($badFrames -join "`n")"
        }
    } else {
        Fail "--once exited with code $LASTEXITCODE or produced empty output"
    }
} else {
    Warn "hudr2 binary not built yet ($binPath)"
}

if ($Fails -eq 0) {
    Write-Host "doctor: OK" -ForegroundColor Green
    exit 0
} else {
    Write-Host "doctor: $Fails failing check(s)" -ForegroundColor Red
    exit 1
}
