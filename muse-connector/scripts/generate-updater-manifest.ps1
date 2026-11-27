[CmdletBinding()]
param(
    [Parameter(Mandatory)][string]$Tag,
    [Parameter(Mandatory)][string]$InstallerPath,
    [Parameter(Mandatory)][string]$SignaturePath,
    [Parameter(Mandatory)][string]$OutputDirectory,
    [string]$PortableZipPath,
    [string]$LinuxAppImagePath,
    [string]$LinuxSignaturePath,
    [string]$LinuxDebPath,
    [string]$MacosDmgPath,
    [string]$MacosZipPath,
    [string]$Repository = 'Aenvo/MuseLink',
    # 可选覆盖下载基址（例如第二阶段接入 Cloudflare R2 自定义域名镜像时传入）。
    [string]$BaseDownloadUrl
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$project = Split-Path -Parent $PSScriptRoot
$repoRoot = Split-Path -Parent $project
$tauriConfigPath = Join-Path $project 'src-tauri/tauri.conf.json'
$packageJsonPath = Join-Path $project 'package.json'
$cargoTomlPath = Join-Path $project 'src-tauri/Cargo.toml'

$tauriConfig = Get-Content -Raw -Encoding UTF8 -LiteralPath $tauriConfigPath | ConvertFrom-Json
$packageJson = Get-Content -Raw -Encoding UTF8 -LiteralPath $packageJsonPath | ConvertFrom-Json
$cargoToml = Get-Content -Raw -Encoding UTF8 -LiteralPath $cargoTomlPath
$cargoVersionMatch = [regex]::Match($cargoToml, '(?m)^version\s*=\s*"([^"]+)"')
if (-not $cargoVersionMatch.Success) {
    throw 'Cannot parse version from Cargo.toml.'
}

$version = [string]$tauriConfig.version
$tagVersion = $Tag.TrimStart('v')
if ([string]::IsNullOrWhiteSpace($version)) {
    throw 'tauri.conf.json is missing version.'
}
if ($tagVersion -ne $version) {
    throw "Tag $Tag does not match tauri.conf.json version $version."
}
if ([string]$packageJson.version -ne $version -or $cargoVersionMatch.Groups[1].Value -ne $version) {
    throw "Version mismatch across tauri.conf.json ($version), package.json ($($packageJson.version)), and Cargo.toml ($($cargoVersionMatch.Groups[1].Value))."
}

$installer = Get-Item -LiteralPath $InstallerPath
$sigFile = Get-Item -LiteralPath $SignaturePath
if ($installer.PSIsContainer -or $installer.Extension -ne '.exe') {
    throw 'InstallerPath must point to an .exe installer.'
}
if ($sigFile.PSIsContainer -or $sigFile.Extension -ne '.sig') {
    throw 'SignaturePath must point to a .sig updater signature.'
}
$signature = (Get-Content -Raw -Encoding UTF8 -LiteralPath $sigFile.FullName).Trim()
if ([string]::IsNullOrWhiteSpace($signature)) {
    throw "Signature file is empty: $($sigFile.FullName)"
}

$hasLinuxUpdater = (-not [string]::IsNullOrWhiteSpace($LinuxAppImagePath)) -or (-not [string]::IsNullOrWhiteSpace($LinuxSignaturePath))
if ($hasLinuxUpdater -and ([string]::IsNullOrWhiteSpace($LinuxAppImagePath) -or [string]::IsNullOrWhiteSpace($LinuxSignaturePath))) {
    throw 'LinuxAppImagePath and LinuxSignaturePath must be provided together.'
}

$outDir = $ExecutionContext.SessionState.Path.GetUnresolvedProviderPathFromPSPath($OutputDirectory)
$null = [IO.Directory]::CreateDirectory($outDir)

$winAssetName = "MuseLink-$version-windows-x64-setup.exe"
$winSigAssetName = "$winAssetName.sig"
$baseUrl = if ([string]::IsNullOrWhiteSpace($BaseDownloadUrl)) {
    "https://github.com/$Repository/releases/download/$Tag"
} else {
    $BaseDownloadUrl.TrimEnd('/')
}
if ($baseUrl -notmatch '^https://') {
    throw "BaseDownloadUrl must use HTTPS: $baseUrl"
}
$winDownloadUrl = "$baseUrl/$winAssetName"
$pubDate = ([DateTimeOffset]::UtcNow).ToString("yyyy-MM-dd'T'HH:mm:ss'Z'")

$stagedWinInstaller = Join-Path $outDir $winAssetName
$stagedWinSig = Join-Path $outDir $winSigAssetName
if ($installer.FullName -ne $stagedWinInstaller) {
    Copy-Item -LiteralPath $installer.FullName -Destination $stagedWinInstaller -Force
}
if ($sigFile.FullName -ne $stagedWinSig) {
    Copy-Item -LiteralPath $sigFile.FullName -Destination $stagedWinSig -Force
}

$checksumTargets = @($stagedWinInstaller, $stagedWinSig)
if (-not [string]::IsNullOrWhiteSpace($PortableZipPath)) {
    $zipItem = Get-Item -LiteralPath $PortableZipPath
    $zipAssetName = "MuseLink-$version-windows-x64-portable.zip"
    $stagedZip = Join-Path $outDir $zipAssetName
    if ($zipItem.FullName -ne $stagedZip) {
        Copy-Item -LiteralPath $zipItem.FullName -Destination $stagedZip -Force
    }
    $checksumTargets += $stagedZip
}

$platforms = [ordered]@{
    'windows-x86_64' = [ordered]@{
        url = $winDownloadUrl
        signature = $signature
    }
}

$linuxDownloadUrl = $null
$linuxSignature = $null
if ($hasLinuxUpdater) {
    $linuxAppImage = Get-Item -LiteralPath $LinuxAppImagePath
    $linuxSigFile = Get-Item -LiteralPath $LinuxSignaturePath
    if ($linuxAppImage.PSIsContainer -or $linuxAppImage.Extension -ne '.AppImage') {
        throw 'LinuxAppImagePath must point to an .AppImage file.'
    }
    if ($linuxSigFile.PSIsContainer -or $linuxSigFile.Extension -ne '.sig') {
        throw 'LinuxSignaturePath must point to a .sig updater signature.'
    }
    $linuxSignature = (Get-Content -Raw -Encoding UTF8 -LiteralPath $linuxSigFile.FullName).Trim()
    if ([string]::IsNullOrWhiteSpace($linuxSignature)) {
        throw "Linux signature file is empty: $($linuxSigFile.FullName)"
    }
    $linuxAssetName = "MuseLink-$version-linux-x64.AppImage"
    $linuxSigAssetName = "$linuxAssetName.sig"
    $linuxDownloadUrl = "$baseUrl/$linuxAssetName"
    $stagedLinuxAppImage = Join-Path $outDir $linuxAssetName
    $stagedLinuxSig = Join-Path $outDir $linuxSigAssetName
    if ($linuxAppImage.FullName -ne $stagedLinuxAppImage) {
        Copy-Item -LiteralPath $linuxAppImage.FullName -Destination $stagedLinuxAppImage -Force
    }
    if ($linuxSigFile.FullName -ne $stagedLinuxSig) {
        Copy-Item -LiteralPath $linuxSigFile.FullName -Destination $stagedLinuxSig -Force
    }
    $checksumTargets += @($stagedLinuxAppImage, $stagedLinuxSig)
    $platforms['linux-x86_64'] = [ordered]@{
        url = $linuxDownloadUrl
        signature = $linuxSignature
    }
}

if (-not [string]::IsNullOrWhiteSpace($LinuxDebPath)) {
    $debItem = Get-Item -LiteralPath $LinuxDebPath
    if ($debItem.PSIsContainer -or $debItem.Extension -ne '.deb') {
        throw 'LinuxDebPath must point to a .deb package.'
    }
    $debAssetName = "MuseLink-$version-linux-x64.deb"
    $stagedDeb = Join-Path $outDir $debAssetName
    if ($debItem.FullName -ne $stagedDeb) {
        Copy-Item -LiteralPath $debItem.FullName -Destination $stagedDeb -Force
    }
    $checksumTargets += $stagedDeb
}

if (-not [string]::IsNullOrWhiteSpace($MacosDmgPath)) {
    $dmgItem = Get-Item -LiteralPath $MacosDmgPath
    if ($dmgItem.PSIsContainer -or $dmgItem.Extension -ne '.dmg') {
        throw 'MacosDmgPath must point to a .dmg disk image.'
    }
    $dmgAssetName = "MuseLink-$version-macos-universal.dmg"
    $stagedDmg = Join-Path $outDir $dmgAssetName
    if ($dmgItem.FullName -ne $stagedDmg) {
        Copy-Item -LiteralPath $dmgItem.FullName -Destination $stagedDmg -Force
    }
    $checksumTargets += $stagedDmg
}

if (-not [string]::IsNullOrWhiteSpace($MacosZipPath)) {
    $macZipItem = Get-Item -LiteralPath $MacosZipPath
    if ($macZipItem.PSIsContainer -or $macZipItem.Extension -ne '.zip') {
        throw 'MacosZipPath must point to a .zip archive.'
    }
    $macZipAssetName = "MuseLink-$version-macos-universal.app.zip"
    $stagedMacZip = Join-Path $outDir $macZipAssetName
    if ($macZipItem.FullName -ne $stagedMacZip) {
        Copy-Item -LiteralPath $macZipItem.FullName -Destination $stagedMacZip -Force
    }
    $checksumTargets += $stagedMacZip
}

$changelogPath = Join-Path $repoRoot 'CHANGELOG.md'
$releaseNotes = "MuseLink $Tag"
if (Test-Path -LiteralPath $changelogPath -PathType Leaf) {
    $changelogText = (Get-Content -Raw -Encoding UTF8 -LiteralPath $changelogPath) -replace "`r`n", "`n"
    $pattern = "(?ms)^##\s+\[v?$([regex]::Escape($version))\][^\n]*\n(.*?)(?=^##\s+\[|\z)"
    $match = [regex]::Match($changelogText, $pattern)
    if ($match.Success -and -not [string]::IsNullOrWhiteSpace($match.Groups[1].Value)) {
        $releaseNotes = $match.Groups[1].Value.Trim()
    }
}

$manifest = [ordered]@{
    version = $version
    notes = $releaseNotes
    pub_date = $pubDate
    platforms = $platforms
}

$utf8NoBom = [System.Text.UTF8Encoding]::new($false)
$manifestPath = Join-Path $outDir 'latest.json'
[IO.File]::WriteAllText($manifestPath, ($manifest | ConvertTo-Json -Depth 6) + "`n", $utf8NoBom)

$parsed = Get-Content -Raw -Encoding UTF8 -LiteralPath $manifestPath | ConvertFrom-Json
if (
    $parsed.version -ne $version -or
    $parsed.platforms.'windows-x86_64'.url -ne $winDownloadUrl -or
    $parsed.platforms.'windows-x86_64'.signature -ne $signature
) {
    throw "latest.json round-trip validation failed for windows-x86_64: $manifestPath"
}
if (
    $hasLinuxUpdater -and (
        $parsed.platforms.'linux-x86_64'.url -ne $linuxDownloadUrl -or
        $parsed.platforms.'linux-x86_64'.signature -ne $linuxSignature
    )
) {
    throw "latest.json round-trip validation failed for linux-x86_64: $manifestPath"
}

$checksumLines = foreach ($file in $checksumTargets) {
    $hash = (Get-FileHash -Algorithm SHA256 -LiteralPath $file).Hash.ToLowerInvariant()
    $name = [IO.Path]::GetFileName($file)
    "$hash  $name"
}
$checksumPath = Join-Path $outDir 'SHA256SUMS.txt'
[IO.File]::WriteAllText($checksumPath, ($checksumLines -join "`n") + "`n", $utf8NoBom)

Write-Host "Generated updater manifest: $manifestPath"
Write-Host "Windows URL: $winDownloadUrl"
if ($linuxDownloadUrl) {
    Write-Host "Linux URL: $linuxDownloadUrl"
}
Write-Host "Generated checksums: $checksumPath"
