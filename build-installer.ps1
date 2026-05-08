[CmdletBinding()]
param(
    [switch]$DebugBuild
)

$ErrorActionPreference = "Stop"
$Root = Split-Path -Parent $MyInvocation.MyCommand.Path
Set-Location $Root

function Write-Step($msg) {
    Write-Host ""
    Write-Host "==> $msg" -ForegroundColor Cyan
}

Write-Step "Build Rust (tray + cli)"
$cargo = Get-Command cargo -ErrorAction SilentlyContinue
if (-not $cargo) {
    throw "cargo introuvable. Installe Rust : https://rustup.rs/"
}

$buildArgs = @("build", "-p", "obsidian-indexer", "--features", "tray")
if (-not $DebugBuild) { $buildArgs += "--release" }
& cargo @buildArgs
if ($LASTEXITCODE -ne 0) { throw "cargo build a échoué (code $LASTEXITCODE)" }

Write-Step "Compilation installateur Inno Setup"
$iscc = Get-Command iscc -ErrorAction SilentlyContinue
$isccExe = $null
if ($iscc) {
    $isccExe = $iscc.Source
}
else {
    $candidates = @(
        "C:\Program Files (x86)\Inno Setup 6\ISCC.exe",
        "C:\Program Files\Inno Setup 6\ISCC.exe",
        "C:\Program Files (x86)\Inno Setup 5\ISCC.exe",
        "C:\Program Files\Inno Setup 5\ISCC.exe"
    )
    foreach ($candidate in $candidates) {
        if (Test-Path $candidate) {
            $isccExe = $candidate
            break
        }
    }
}
if (-not $isccExe) {
    throw "ISCC.exe introuvable. Installe Inno Setup 6 (ou ajoute iscc au PATH)."
}

$issPath = Join-Path $Root "installer\obsidian-indexer.iss"
if (-not (Test-Path $issPath)) {
    throw "Script Inno introuvable : $issPath"
}

& $isccExe $issPath
if ($LASTEXITCODE -ne 0) { throw "iscc a échoué (code $LASTEXITCODE)" }

Write-Step "Terminé"
Write-Host "Installeur généré dans : $(Join-Path $Root "dist")" -ForegroundColor Green
