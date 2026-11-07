<#
.SYNOPSIS
    Установщик Lumen для Windows.

.DESCRIPTION
    Два режима:
      * по умолчанию — скачивает готовый установщик из последнего GitHub Release
        (собранный Tauri: *-setup.exe или *.msi), проверяет SHA-256 и запускает его;
      * -FromSource — клонирует репозиторий и собирает проект локально
        (нужны Git, Rust/Cargo, Node.js 18+ и MSVC Build Tools).

.EXAMPLE
    # Одной строкой (последний релиз):
    irm https://raw.githubusercontent.com/lsdasy/lumen/main/install.ps1 | iex

.EXAMPLE
    # С параметрами (через scriptblock, т.к. "| iex" параметры не принимает):
    & ([scriptblock]::Create((irm https://raw.githubusercontent.com/lsdasy/lumen/main/install.ps1))) -FromSource

.EXAMPLE
    .\install.ps1 -Version v0.2.0 -Silent
#>
[CmdletBinding()]
param(
    [string]$Repo       = $(if ($env:LUMEN_REPO) { $env:LUMEN_REPO } else { "lsdasy/lumen" }),
    [string]$Version    = "latest",          # тег релиза, например v0.1.0
    [switch]$FromSource,                     # собрать из исходников вместо скачивания релиза
    [string]$Branch     = "main",
    [string]$InstallDir = "$env:LOCALAPPDATA\Lumen\src",   # куда клонировать при -FromSource
    [switch]$Silent                          # тихая установка (NSIS /S, MSI /qn)
)

$ErrorActionPreference = "Stop"
$ProgressPreference    = "SilentlyContinue"   # в PS 5.1 прогресс-бар сильно замедляет загрузку
[Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12

function Write-Step($msg) { Write-Host "==> $msg" -ForegroundColor Cyan }
function Write-Ok($msg)   { Write-Host "  ✓ $msg" -ForegroundColor Green }
function Fail($msg)       { Write-Host "  ✗ $msg" -ForegroundColor Red; exit 1 }

function Test-Command($name) { [bool](Get-Command $name -ErrorAction SilentlyContinue) }

$Headers = @{ "User-Agent" = "lumen-installer"; "Accept" = "application/vnd.github+json" }
if ($env:GITHUB_TOKEN) { $Headers["Authorization"] = "Bearer $env:GITHUB_TOKEN" }  # для приватного репо / лимитов API

# ---------------------------------------------------------------------------
# Режим 1: готовый релиз
# ---------------------------------------------------------------------------
function Install-FromRelease {
    $api = if ($Version -eq "latest") {
        "https://api.github.com/repos/$Repo/releases/latest"
    } else {
        "https://api.github.com/repos/$Repo/releases/tags/$Version"
    }

    Write-Step "Ищу релиз $Version в $Repo"
    try {
        $release = Invoke-RestMethod -Uri $api -Headers $Headers
    } catch {
        Fail "Релиз не найден ($api). Проверь, что в репозитории есть опубликованный Release, или используй -FromSource."
    }
    Write-Ok "Найден $($release.tag_name)"

    $arch = if ([Environment]::Is64BitOperatingSystem) {
        if ($env:PROCESSOR_ARCHITECTURE -eq "ARM64") { "arm64" } else { "x64" }
    } else { "x86" }

    # Tauri называет файлы примерно так: Lumen_0.1.0_x64-setup.exe, Lumen_0.1.0_x64_en-US.msi
    $assets = $release.assets
    $asset  = $assets | Where-Object { $_.name -match "_$arch-setup\.exe$" } | Select-Object -First 1
    if (-not $asset) { $asset = $assets | Where-Object { $_.name -match "_$arch.*\.msi$" } | Select-Object -First 1 }
    if (-not $asset) { $asset = $assets | Where-Object { $_.name -match "\.(exe|msi)$" } | Select-Object -First 1 }
    if (-not $asset) { Fail "В релизе нет установщика для Windows ($arch)." }

    $tmp  = Join-Path $env:TEMP "lumen-install"
    New-Item -ItemType Directory -Force -Path $tmp | Out-Null
    $file = Join-Path $tmp $asset.name

    Write-Step "Скачиваю $($asset.name) ($([math]::Round($asset.size / 1MB, 1)) МБ)"
    Invoke-WebRequest -Uri $asset.browser_download_url -OutFile $file -Headers @{ "User-Agent" = "lumen-installer" }
    Write-Ok "Скачано"

    # Проверка целостности, если в релизе есть SHA256SUMS(.txt)
    $sums = $assets | Where-Object { $_.name -match "^SHA256SUMS" } | Select-Object -First 1
    if ($sums) {
        Write-Step "Проверяю SHA-256"
        $sumsText = (Invoke-WebRequest -Uri $sums.browser_download_url -Headers @{ "User-Agent" = "lumen-installer" } -UseBasicParsing).Content
        if ($sumsText -is [byte[]]) { $sumsText = [Text.Encoding]::UTF8.GetString($sumsText) }
        $line = ($sumsText -split "`n") | Where-Object { $_ -match [regex]::Escape($asset.name) } | Select-Object -First 1
        if (-not $line) { Fail "Файл $($asset.name) не найден в $($sums.name)." }
        $expected = ($line -split "\s+")[0].ToLower()
        $actual   = (Get-FileHash -Algorithm SHA256 -Path $file).Hash.ToLower()
        if ($expected -ne $actual) { Remove-Item $file -Force; Fail "Контрольная сумма не совпала — файл повреждён или подменён." }
        Write-Ok "Контрольная сумма совпала"
    } else {
        Write-Host "  ! В релизе нет SHA256SUMS — проверка целостности пропущена" -ForegroundColor Yellow
    }

    Write-Step "Запускаю установщик"
    # ($args — автоматическая переменная PowerShell, поэтому своё имя)
    if ($file -like "*.msi") {
        $instArgs = @("/i", "`"$file`"")
        if ($Silent) { $instArgs += "/qn" }
        $p = Start-Process msiexec.exe -ArgumentList $instArgs -Wait -PassThru
    } elseif ($Silent) {
        $p = Start-Process -FilePath $file -ArgumentList "/S" -Wait -PassThru
    } else {
        $p = Start-Process -FilePath $file -Wait -PassThru
    }
    if ($p.ExitCode -ne 0) { Fail "Установщик завершился с кодом $($p.ExitCode)." }

    Remove-Item $tmp -Recurse -Force -ErrorAction SilentlyContinue
    Write-Ok "Lumen $($release.tag_name) установлен. Открывай по Alt + Space."
}

# ---------------------------------------------------------------------------
# Режим 2: сборка из исходников
# ---------------------------------------------------------------------------
function Install-FromSource {
    Write-Step "Проверяю зависимости"
    $missing = @()
    if (-not (Test-Command git))   { $missing += "Git           → winget install Git.Git" }
    if (-not (Test-Command cargo)) { $missing += "Rust (cargo)  → winget install Rustlang.Rustup" }
    if (-not (Test-Command node))  { $missing += "Node.js 18+   → winget install OpenJS.NodeJS.LTS" }
    if ($missing.Count -gt 0) {
        Write-Host "  Не хватает:" -ForegroundColor Red
        $missing | ForEach-Object { Write-Host "    $_" }
        Write-Host "  Также нужны MSVC Build Tools (C++): winget install Microsoft.VisualStudio.2022.BuildTools" -ForegroundColor Yellow
        exit 1
    }
    $nodeMajor = [int]((node --version).TrimStart("v").Split(".")[0])
    if ($nodeMajor -lt 18) { Fail "Нужен Node.js 18+, установлен $(node --version)." }
    Write-Ok "git, cargo, node $(node --version)"

    if (Test-Path (Join-Path $InstallDir ".git")) {
        Write-Step "Обновляю исходники в $InstallDir"
        git -C $InstallDir fetch --depth 1 origin $Branch
        git -C $InstallDir checkout -B $Branch FETCH_HEAD
    } else {
        Write-Step "Клонирую https://github.com/$Repo ($Branch)"
        New-Item -ItemType Directory -Force -Path (Split-Path $InstallDir) | Out-Null
        git clone --depth 1 --branch $Branch "https://github.com/$Repo.git" $InstallDir
    }
    if ($LASTEXITCODE -ne 0) { Fail "Не удалось получить исходники." }

    Push-Location $InstallDir
    try {
        Write-Step "Устанавливаю npm-зависимости"
        if (Test-Path "package-lock.json") { npm ci } else { npm install }
        if ($LASTEXITCODE -ne 0) { Fail "npm install завершился с ошибкой." }

        Write-Step "Собираю (npm run tauri build) — первый раз это 5–15 минут"
        npm run tauri build -- --bundles nsis
        if ($LASTEXITCODE -ne 0) { Fail "Сборка не удалась. Проверь, что установлены MSVC Build Tools." }

        $bundle = Get-ChildItem -Path "target\release\bundle" -Recurse -Include "*-setup.exe", "*.msi" |
                  Sort-Object LastWriteTime -Descending | Select-Object -First 1
        if (-not $bundle) { Fail "Сборка прошла, но установщик не найден в target\release\bundle." }
        Write-Ok "Собрано: $($bundle.FullName)"

        Write-Step "Запускаю установщик"
        $p = if ($bundle.Extension -eq ".msi") {
            Start-Process msiexec.exe -ArgumentList @("/i", "`"$($bundle.FullName)`"") -Wait -PassThru
        } else {
            Start-Process -FilePath $bundle.FullName -Wait -PassThru
        }
        if ($p.ExitCode -ne 0) { Fail "Установщик завершился с кодом $($p.ExitCode)." }
        Write-Ok "Lumen собран и установлен из исходников."
    } finally {
        Pop-Location
    }
}

Write-Host ""
Write-Host "  Lumen installer" -ForegroundColor White
Write-Host "  репозиторий: $Repo" -ForegroundColor DarkGray
Write-Host ""

if ($FromSource) { Install-FromSource } else { Install-FromRelease }
