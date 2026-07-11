#Requires -Version 5.1
<#
.SYNOPSIS
    sloth リリースビルド＆パッケージスクリプト
.DESCRIPTION
    1. cargo build --release でワークスペース全体をビルド
    2. dist/ に実行ファイル + data/ を配置
    3. ZIP アーカイブを生成 (sloth-v{VERSION}-win-x64.zip)
.EXAMPLE
    .\build-release.ps1
    .\build-release.ps1 -SkipBuild   # ビルド済みなら配布物だけ再生成
#>
param(
    [switch]$SkipBuild
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$root = $PSScriptRoot
$target = Join-Path $root 'target\release'
$dist   = Join-Path $root 'dist\sloth'

# --- Version from Cargo.toml ---
$cargoToml = Get-Content (Join-Path $root 'Cargo.toml') -Raw
if ($cargoToml -match 'version\s*=\s*"([^"]+)"') {
    $version = $Matches[1]
} else {
    $version = '0.0.0'
}
Write-Host "=== sloth v$version release build ===" -ForegroundColor Cyan

# --- Build ---
if (-not $SkipBuild) {
    Write-Host "`n[1/3] cargo build --release ..." -ForegroundColor Yellow
    Push-Location $root
    cargo build --release
    if ($LASTEXITCODE -ne 0) {
        Pop-Location
        Write-Error 'cargo build failed'
    }
    Pop-Location
} else {
    Write-Host "`n[1/3] build skipped (-SkipBuild)" -ForegroundColor DarkGray
}

# --- Assemble dist ---
Write-Host "`n[2/3] assembling dist/ ..." -ForegroundColor Yellow

if (Test-Path $dist) {
    try {
        Remove-Item $dist -Recurse -Force -Confirm:$false -ErrorAction Stop
    } catch {
        Write-Host "  warning: could not remove old dist, retrying..." -ForegroundColor DarkYellow
        Start-Sleep -Milliseconds 500
        Remove-Item $dist -Recurse -Force -Confirm:$false -ErrorAction Stop
    }
}
New-Item -ItemType Directory -Path $dist -Force | Out-Null

# Executables
foreach ($bin in @('sloth.exe', 'sloth-config.exe')) {
    $src = Join-Path $target $bin
    if (-not (Test-Path $src)) {
        Write-Error "missing: $src"
    }
    Copy-Item $src $dist
    Write-Host "  + $bin ($('{0:N0}' -f ((Get-Item $src).Length / 1KB)) KB)"
}

# data/ directory (config + layouts)
$dataSrc  = Join-Path $root 'data'
$dataDest = Join-Path $dist 'data'
New-Item -ItemType Directory -Path $dataDest -Force | Out-Null
New-Item -ItemType Directory -Path (Join-Path $dataDest 'layouts') -Force | Out-Null
New-Item -ItemType Directory -Path (Join-Path $dataDest 'layouts\samples') -Force | Out-Null

Copy-Item (Join-Path $dataSrc 'config.json') $dataDest
Write-Host '  + data/config.json'

Get-ChildItem (Join-Path $dataSrc 'layouts') -Filter '*.txt' -File | ForEach-Object {
    Copy-Item $_.FullName (Join-Path $dataDest 'layouts')
    Write-Host "  + data/layouts/$($_.Name)"
}
Get-ChildItem (Join-Path $dataSrc 'layouts\samples') -Filter '*.txt' -File | ForEach-Object {
    Copy-Item $_.FullName (Join-Path $dataDest 'layouts\samples')
    Write-Host "  + data/layouts/samples/$($_.Name)"
}

# --- ZIP ---
Write-Host "`n[3/3] creating ZIP ..." -ForegroundColor Yellow

$zipName = "sloth-v$version-win-x64.zip"
$zipPath = Join-Path $root "dist\$zipName"
if (Test-Path $zipPath) {
    Remove-Item $zipPath -Force -Confirm:$false
}
Compress-Archive -Path $dist -DestinationPath $zipPath -CompressionLevel Optimal
$zipSize = (Get-Item $zipPath).Length
Write-Host "  => $zipName ($('{0:N0}' -f ($zipSize / 1KB)) KB)"

# --- Release to GitHub ---
Write-Host "`n[4/4] creating GitHub release ..." -ForegroundColor Yellow

$tag = "v$version"
Push-Location $root

# Check if tag already exists and delete it
$tags = @(git tag -l $tag 2>$null)
if ($tags.Count -gt 0) {
    Write-Host "  tag $tag already exists, deleting..." -ForegroundColor DarkGray
    git tag -d $tag | Out-Null
    git push origin --delete $tag 2>$null | Out-Null
}

# Create and push tag
Write-Host "  creating tag $tag..." -ForegroundColor Gray
git tag $tag
git push origin $tag

# Create release with ZIP
Write-Host "  creating release on GitHub..." -ForegroundColor Gray
gh release create $tag `
    --title "sloth $version" `
    --notes "リマッパー配布版 v$version" `
    "$zipPath" `
    --repo "cet-t/sloth"

Write-Host "  ✓ Released to GitHub!" -ForegroundColor Green

Pop-Location

# --- Summary ---
Write-Host "`n=== done ===" -ForegroundColor Green
Write-Host "dist folder : $dist"
Write-Host "ZIP archive : $zipPath"
Write-Host "GitHub      : https://github.com/cet-t/sloth/releases/tag/$tag"
Write-Host "`ncontents:"
Get-ChildItem $dist -Recurse -File | ForEach-Object {
    $rel = $_.FullName.Substring($dist.Length + 1)
    Write-Host "  $rel"
}
