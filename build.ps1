<#
.SYNOPSIS
  Télécharge les dépendances et compile l’indexeur Rust et le plugin Obsidian.

.PARAMETER DebugBuild
  Compile Rust en mode debug (sans --release). Par défaut : release.

.PARAMETER SkipRust
  Ne pas exécuter cargo (plugin uniquement).

.PARAMETER SkipPlugin
  Ne pas exécuter npm build (Rust uniquement).

.PARAMETER NoTray
  Ne pas compiler la fonctionnalité tray (pas de binaire obsidian-indexer-tray).
#>
[CmdletBinding()]
param(
    [switch]$DebugBuild,
    [switch]$SkipRust,
    [switch]$SkipPlugin,
    [switch]$NoTray
)

$ErrorActionPreference = "Stop"
$Root = Split-Path -Parent $MyInvocation.MyCommand.Path
Set-Location $Root

function Write-Step($msg) {
    Write-Host ""
    Write-Host "==> $msg" -ForegroundColor Cyan
}

if (-not $SkipRust) {
    Write-Step "Rust : cargo fetch + build (workspace)"
    $cargo = Get-Command cargo -ErrorAction SilentlyContinue
    if (-not $cargo) {
        throw "cargo introuvable. Installe Rust : https://rustup.rs/"
    }

    Push-Location $Root
    try {
        $runningTray = Get-Process -Name "obsidian-indexer-tray" -ErrorAction SilentlyContinue
        if ($runningTray) {
            Write-Host "Processus tray détecté, arrêt pour libérer le binaire..." -ForegroundColor Yellow
            $runningTray | Stop-Process -Force
            Start-Sleep -Milliseconds 500
        }
        cargo fetch
        $buildArgs = @("build", "-p", "obsidian-indexer")
        if (-not $DebugBuild) { $buildArgs += "--release" }
        if (-not $NoTray) {
            $buildArgs += "--features", "tray"
        }
        & cargo @buildArgs
        if ($LASTEXITCODE -ne 0) { throw "cargo build a échoué (code $LASTEXITCODE)" }
    }
    finally {
        Pop-Location
    }

    $out = if ($DebugBuild) { "target\debug" } else { "target\release" }
    Write-Host "Binaires Rust : $(Join-Path $Root $out)" -ForegroundColor Green
}

if (-not $SkipPlugin) {
    Write-Step "Plugin : npm ci + npm run build"
    $npm = Get-Command npm -ErrorAction SilentlyContinue
    if (-not $npm) {
        throw "npm introuvable. Installe Node.js LTS : https://nodejs.org/"
    }

    $pluginDir = Join-Path $Root "plugin"
    if (-not (Test-Path $pluginDir)) { throw "Dossier plugin introuvable : $pluginDir" }

    Push-Location $pluginDir
    try {
        if (Test-Path "package-lock.json") {
            npm ci
        }
        else {
            npm install
        }
        if ($LASTEXITCODE -ne 0) { throw "npm install a échoué (code $LASTEXITCODE)" }

        npm run build
        if ($LASTEXITCODE -ne 0) { throw "npm run build a échoué (code $LASTEXITCODE)" }
    }
    finally {
        Pop-Location
    }

    Write-Host "Plugin compilé : $pluginDir (main.js inclut le WASM sql.js)" -ForegroundColor Green

    $deployDir = "D:\Notes\.obsidian\plugins\vault-index-search"
    Write-Step "Déploiement plugin vers $deployDir"
    New-Item -ItemType Directory -Force -Path $deployDir | Out-Null
    Copy-Item -Path (Join-Path $pluginDir "main.js") -Destination (Join-Path $deployDir "main.js") -Force
    Copy-Item -Path (Join-Path $pluginDir "manifest.json") -Destination (Join-Path $deployDir "manifest.json") -Force
    if (Test-Path (Join-Path $pluginDir "styles.css")) {
        Copy-Item -Path (Join-Path $pluginDir "styles.css") -Destination (Join-Path $deployDir "styles.css") -Force
    }
    Write-Host "Plugin déployé : $deployDir" -ForegroundColor Green
}

Write-Step "Terminé."
if (-not $SkipRust) {
    Write-Host "  Indexeur CLI : obsidian-indexer.exe (ou sans .exe sous Unix)"
    if (-not $NoTray) {
        Write-Host "  Tray          : obsidian-indexer-tray.exe"
    }
}
