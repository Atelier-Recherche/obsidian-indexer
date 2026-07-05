#Requires -Version 5.1
param(
    [ValidateSet('Patch', 'Minor', 'Major')]
    [string] $BumpKind = 'Patch',
    [switch] $SkipBuild,
    [switch] $NoPush,
    [string] $Remote = 'origin'
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$PluginSubdir = 'plugin'
$ReleaseNotesFile = 'vault-index-search-release-notes.md'
$ReleaseNotesTitle = 'Vault Index Search'
$GhActionsUrl = 'https://github.com/Atelier-Recherche/obsidian-indexer/actions'
$IncludeMainJsInCommit = $true

function Test-CommandExists {
    param([Parameter(Mandatory)][string] $Name)
    return [bool](Get-Command $Name -ErrorAction SilentlyContinue)
}

function Set-Utf8NoBomFile {
    param(
        [Parameter(Mandatory)][string] $Path,
        [Parameter(Mandatory)][string] $Content
    )
    $utf8 = New-Object System.Text.UTF8Encoding $false
    [System.IO.File]::WriteAllText($Path, $Content, $utf8)
}

function Get-NextSemVer {
    param(
        [Parameter(Mandatory)][string] $Version,
        [Parameter(Mandatory)][ValidateSet('Patch', 'Minor', 'Major')][string] $Kind
    )
    if ($Version -notmatch '^(\d+)\.(\d+)\.(\d+)$') {
        throw "Version non semver: $Version"
    }
    $major = [int]$Matches[1]
    $minor = [int]$Matches[2]
    $patch = [int]$Matches[3]
    switch ($Kind) {
        'Major' { return "$($major + 1).0.0" }
        'Minor' { return "$major.$($minor + 1).0" }
        'Patch' { return "$major.$minor.$($patch + 1)" }
    }
}

function Get-PluginRepoRelativePath {
    param([Parameter(Mandatory)][string] $FileName)
    return "$PluginSubdir/$FileName"
}

$repoRoot = $PSScriptRoot
$pluginDir = Join-Path $repoRoot $PluginSubdir
Set-Location -LiteralPath $repoRoot

foreach ($cmd in @('git', 'node', 'npm')) {
    if (-not (Test-CommandExists $cmd)) { throw "Commande introuvable: $cmd" }
}

foreach ($p in @(
    (Join-Path $pluginDir 'package.json'),
    (Join-Path $pluginDir 'manifest.json'),
    (Join-Path $pluginDir 'versions.json'),
    (Join-Path $pluginDir 'version-bump.mjs'),
    (Join-Path $pluginDir 'styles.css'),
    (Join-Path $repoRoot '.github/workflows/obsidian-plugin-release.yml')
)) {
    if (-not (Test-Path $p)) { throw "Fichier requis absent: $p" }
}

$dirty = (& git status --porcelain 2>&1 | Out-String).Trim()
if ($dirty) { throw "Arbre Git non propre.`n$dirty" }

$packagePath = Join-Path $pluginDir 'package.json'
$packageRaw = Get-Content -LiteralPath $packagePath -Raw -Encoding UTF8
$currentVersion = [string](($packageRaw | ConvertFrom-Json).version)
$newVersion = Get-NextSemVer -Version $currentVersion -Kind $BumpKind

$updatedPackage = [regex]::Replace(
    $packageRaw,
    '("version"\s*:\s*")' + [regex]::Escape($currentVersion) + '(")',
    '${1}' + $newVersion + '$2',
    1
)
Set-Utf8NoBomFile -Path $packagePath -Content $updatedPackage

$env:npm_package_version = $newVersion
try {
    Push-Location $pluginDir
    & node version-bump.mjs
    if ($LASTEXITCODE -ne 0) { throw "version-bump.mjs a échoué." }
} finally {
    Pop-Location
    Remove-Item Env:\npm_package_version -ErrorAction SilentlyContinue
}

$lastTag = $null
$describeResult = & git describe --tags --abbrev=0 --match '[0-9]*.[0-9]*.[0-9]*' 2>&1
if ($LASTEXITCODE -eq 0) { $lastTag = ($describeResult | Out-String).Trim() }
$logLines = if ($lastTag) { & git log "$lastTag..HEAD" --oneline 2>&1 } else { & git log --oneline -n 30 2>&1 }
Set-Utf8NoBomFile -Path (Join-Path $repoRoot $ReleaseNotesFile) -Content ("# $ReleaseNotesTitle $newVersion`n`n" + ($logLines | Out-String).Trim() + "`n")

if (-not $SkipBuild) {
    Push-Location $pluginDir
    try {
        & npm run build
        if ($LASTEXITCODE -ne 0) { throw "npm run build a échoué." }
    } finally {
        Pop-Location
    }
}

if (-not (Test-Path (Join-Path $pluginDir 'main.js'))) { throw 'plugin/main.js absent après build.' }

& git rev-parse --verify --quiet "refs/tags/$newVersion" 2>$null | Out-Null
if ($LASTEXITCODE -eq 0) { throw "Tag $newVersion existe déjà." }

& git add `
    (Get-PluginRepoRelativePath 'package.json') `
    (Get-PluginRepoRelativePath 'manifest.json') `
    (Get-PluginRepoRelativePath 'versions.json') `
    (Get-PluginRepoRelativePath 'styles.css') `
    $ReleaseNotesFile
if ($IncludeMainJsInCommit) { & git add (Get-PluginRepoRelativePath 'main.js') }

& git commit -m "release(plugin): $newVersion"
& git tag -a $newVersion -m $newVersion

if ($NoPush) { Write-Host "Release $newVersion préparée (-NoPush)." -ForegroundColor Green; exit 0 }

& git push $Remote HEAD
& git push $Remote "refs/tags/$newVersion"
Write-Host "Release $newVersion poussée." -ForegroundColor Green
Write-Host "Suivi : $GhActionsUrl" -ForegroundColor Gray
