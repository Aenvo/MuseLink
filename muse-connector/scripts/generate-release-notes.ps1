[CmdletBinding()]
param(
    [string]$Tag,
    [string]$OutputPath,
    [string]$Repository = 'Aenvo/MuseLink',
    [string]$GeneratedNotesPath,
    [string]$PreviousTag,
    [switch]$FetchGitHubNotes,
    [switch]$ValidateSync,
    [switch]$AllowMissingChangelog,
    [switch]$StrictPrCoverage
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$project = Split-Path -Parent $PSScriptRoot
$repoRoot = Split-Path -Parent $project

if (-not $ValidateSync -and ([string]::IsNullOrWhiteSpace($Tag) -or [string]::IsNullOrWhiteSpace($OutputPath))) {
    throw 'Provide both -Tag and -OutputPath to generate release notes, or pass -ValidateSync for CI validation.'
}

$version = $null
if (-not [string]::IsNullOrWhiteSpace($Tag)) {
    if ($Tag -notmatch '^v\d+\.\d+\.\d+') {
        throw "Invalid release tag: $Tag"
    }
    $version = $Tag.TrimStart('v')
}

function Normalize-LocaleCode {
    param([Parameter(Mandatory)][string]$Raw)
    $parts = $Raw.Trim() -split '[-_]'
    if ($parts.Count -eq 1) {
        return $parts[0].ToLowerInvariant()
    }
    $lang = $parts[0].ToLowerInvariant()
    $rest = for ($i = 1; $i -lt $parts.Count; $i++) {
        $part = $parts[$i]
        if ($part.Length -eq 2) {
            $part.ToUpperInvariant()
        } elseif ($part.Length -eq 4) {
            $part.Substring(0, 1).ToUpperInvariant() + $part.Substring(1).ToLowerInvariant()
        } else {
            $part
        }
    }
    return (@($lang) + @($rest)) -join '-'
}

function Get-LocalePriority {
    param([Parameter(Mandatory)][string]$Locale)
    switch ($Locale) {
        'zh-CN' { return 0 }
        'en'    { return 1 }
        'ja'    { return 2 }
        default { return 100 }
    }
}

function Get-LocaleMetadata {
    param(
        [Parameter(Mandatory)][string]$Locale,
        [Parameter(Mandatory)][string]$FileName,
        [string]$ReadmeFileName,
        [string]$TitleFromFile,
        [Parameter(Mandatory)][string]$Repository
    )

    $changelogUrl = "https://github.com/$Repository/blob/main/$FileName"
    $readmeUrl = if (-not [string]::IsNullOrWhiteSpace($ReadmeFileName)) {
        "https://github.com/$Repository/blob/main/$ReadmeFileName"
    } else {
        "https://github.com/$Repository"
    }

    switch ($Locale) {
        'zh-CN' {
            return @{
                Heading     = '简体中文'
                Subheading  = '本版本更新（{0}）'
                Footer      = "完整变更见 [$FileName]($changelogUrl)，使用说明见 [中文使用指南](https://github.com/$Repository#使用指南)。"
                MacosNotice = $null # Already included in the top Chinese download/platform header.
            }
        }
        'en' {
            return @{
                Heading     = 'English'
                Subheading  = "What's new in {0}"
                Footer      = "See [$FileName]($changelogUrl) and the [English quick start]($readmeUrl#quick-start)."
                MacosNotice = @"
> ⚠️ **Important note for macOS first launch**: macOS packages are ad-hoc signed and not notarized with an Apple Developer ID. On first launch:
> 1. Try opening ``MuseLink.app`` once, then open **System Settings → Privacy & Security → Security** and click **Open Anyway**.
> 2. If macOS Sequoia (15+) still reports that the app "is damaged and can't be opened", run once in Terminal:
>    ``````bash
>    xattr -dr com.apple.quarantine /Applications/MuseLink.app
>    ``````
"@
            }
        }
        'ja' {
            return @{
                Heading     = '日本語'
                Subheading  = '{0} の変更点'
                Footer      = "すべての変更履歴は [$FileName]($changelogUrl)、利用ガイドは [$ReadmeFileName]($readmeUrl) をご覧ください。"
                MacosNotice = @"
> ⚠️ **macOS 初回起動時のご注意**：macOS 版は Apple Developer ID による公証を行っていないため、初回起動時は **システム設定 → プライバシーとセキュリティ → セキュリティ** から **このまま開く** を選択してください。「壊れているため開けません」と表示される場合は、ターミナルで ``xattr -dr com.apple.quarantine /Applications/MuseLink.app`` を 1 回実行してください。
"@
            }
        }
        'zh-TW' {
            return @{
                Heading     = '繁體中文'
                Subheading  = '本版本更新（{0}）'
                Footer      = "完整變更見 [$FileName]($changelogUrl)，使用說明見 [$ReadmeFileName]($readmeUrl)。"
                MacosNotice = $null
            }
        }
        'ko' {
            return @{
                Heading     = '한국어'
                Subheading  = '{0} 변경 사항'
                Footer      = "전체 변경 내역은 [$FileName]($changelogUrl), 사용 가이드는 [$ReadmeFileName]($readmeUrl)를 참고하세요."
                MacosNotice = $null
            }
        }
        'de' {
            return @{
                Heading     = 'Deutsch'
                Subheading  = 'Neuerungen in {0}'
                Footer      = "Vollständiges Änderungsprotokoll: [$FileName]($changelogUrl) · Dokumentation: [$ReadmeFileName]($readmeUrl)."
                MacosNotice = $null
            }
        }
        'fr' {
            return @{
                Heading     = 'Français'
                Subheading  = 'Nouveautés de {0}'
                Footer      = "Journal complet : [$FileName]($changelogUrl) · Documentation : [$ReadmeFileName]($readmeUrl)."
                MacosNotice = $null
            }
        }
        'es' {
            return @{
                Heading     = 'Español'
                Subheading  = 'Novedades de {0}'
                Footer      = "Registro completo: [$FileName]($changelogUrl) · Documentación: [$ReadmeFileName]($readmeUrl)."
                MacosNotice = $null
            }
        }
        'pt-BR' {
            return @{
                Heading     = 'Português (Brasil)'
                Subheading  = 'Novidades em {0}'
                Footer      = "Histórico completo: [$FileName]($changelogUrl) · Documentação: [$ReadmeFileName]($readmeUrl)."
                MacosNotice = $null
            }
        }
        'ru' {
            return @{
                Heading     = 'Русский'
                Subheading  = 'Изменения в {0}'
                Footer      = "Полный список изменений: [$FileName]($changelogUrl) · Документация: [$ReadmeFileName]($readmeUrl)."
                MacosNotice = $null
            }
        }
        default {
            # Zero-config fallback for any future locale added via CHANGELOG.<locale>.md.
            $displayHeading = if (-not [string]::IsNullOrWhiteSpace($TitleFromFile)) {
                "$TitleFromFile ($Locale)"
            } else {
                $Locale
            }
            $footer = if (-not [string]::IsNullOrWhiteSpace($ReadmeFileName)) {
                "[$FileName]($changelogUrl) · [$ReadmeFileName]($readmeUrl)"
            } else {
                "[$FileName]($changelogUrl)"
            }
            return @{
                Heading     = $displayHeading
                Subheading  = '{0}'
                Footer      = $footer
                MacosNotice = $null
            }
        }
    }
}

function Is-UnreleasedHeading {
    param([Parameter(Mandatory)][string]$Name)
    $lower = $Name.Trim().ToLowerInvariant()
    return $lower -in @('unreleased', '未发布', '未公開', '未リリース', '未發布')
}

function Get-MarkdownRefNumbers {
    param([string]$Text)

    if ([string]::IsNullOrWhiteSpace($Text)) {
        return @()
    }

    # Strip inline code spans so hex colors (e.g. `#48C52C`) or code literals are never misread as PR/Issue refs.
    $withoutCode = [regex]::Replace($Text, '`[^`]*`', ' ')
    $numbers = New-Object 'System.Collections.Generic.HashSet[int]'

    foreach ($m in [regex]::Matches($withoutCode, 'https://github\.com/[^/\s)]+/[^/\s)]+/(?:pull|issues)/(\d+)')) {
        $null = $numbers.Add([int]$m.Groups[1].Value)
    }
    foreach ($m in [regex]::Matches($withoutCode, '(?<![A-Za-z0-9_])#(\d+)(?![A-Za-z0-9_])')) {
        $null = $numbers.Add([int]$m.Groups[1].Value)
    }

    return @($numbers | Sort-Object)
}

function Parse-GitHubGeneratedPrs {
    param([string]$GeneratedNotes)

    if ([string]::IsNullOrWhiteSpace($GeneratedNotes)) {
        return @()
    }

    $prs = @()
    $seen = New-Object 'System.Collections.Generic.HashSet[int]'
    $pattern = '(?m)^[*-]\s+(?<title>.+?)\s+by\s+@(?<author>[A-Za-z0-9-]+(?:\[bot\])?)\s+in\s+(?<url>https://github\.com/[^/\s]+/[^/\s]+/pull/(?<number>\d+))\s*$'
    foreach ($m in [regex]::Matches($GeneratedNotes, $pattern)) {
        $num = [int]$m.Groups['number'].Value
        if ($seen.Add($num)) {
            $prs += [pscustomobject]@{
                Number = $num
                Author = $m.Groups['author'].Value
                Title  = $m.Groups['title'].Value.Trim()
                Url    = $m.Groups['url'].Value
            }
        }
    }
    return @($prs)
}

function Write-CiWarning {
    param([Parameter(Mandatory)][string]$Message)

    if ($env:GITHUB_ACTIONS -eq 'true') {
        $escaped = $Message -replace '%', '%25' -replace "`r", '%0D' -replace "`n", '%0A'
        Write-Host "::warning::$escaped"
    } else {
        Write-Warning $Message
    }
}

function Parse-ChangelogDocument {
    param([Parameter(Mandatory)][string]$Path)

    $text = (Get-Content -Raw -Encoding UTF8 -LiteralPath $Path) -replace "`r`n", "`n"
    $titleMatch = [regex]::Match($text, '(?m)^#\s+(.+)$')
    $title = if ($titleMatch.Success) { $titleMatch.Groups[1].Value.Trim() } else { $null }

    $versions = [ordered]@{}
    $unreleased = ''
    $matches = [regex]::Matches($text, '(?ms)^##\s+\[([^\]]+)\][^\n]*\n(.*?)(?=^##\s+\[|\z)')
    foreach ($m in $matches) {
        $rawVer = $m.Groups[1].Value.Trim()
        $body = $m.Groups[2].Value.Trim()
        if (Is-UnreleasedHeading -Name $rawVer) {
            $unreleased = $body
            continue
        }
        $normalizedVer = $rawVer -replace '^[vV]', ''
        # Demote ### subheadings to #### so they nest cleanly under ### 本版本更新 / ### What's new.
        $demoted = [regex]::Replace($body, '(?m)^###(\s+)', '####$1')
        $versions[$normalizedVer] = $demoted
    }

    return [pscustomobject]@{
        Title      = $title
        Unreleased = $unreleased
        Versions   = $versions
    }
}

function Discover-Changelogs {
    param([Parameter(Mandatory)][string]$RootDir)

    $entries = @()
    $files = Get-ChildItem -LiteralPath $RootDir -File -Filter 'CHANGELOG*.md'
    foreach ($file in $files) {
        $locale = $null
        if ($file.Name -ceq 'CHANGELOG.md') {
            $locale = 'zh-CN'
        } elseif ($file.Name -cmatch '^CHANGELOG\.([A-Za-z0-9_-]+)\.md$') {
            $locale = Normalize-LocaleCode -Raw $Matches[1]
        } else {
            continue
        }

        $readmeName = if ($locale -eq 'zh-CN') {
            'README.md'
        } else {
            $candidate = "README.$locale.md"
            if (Test-Path -LiteralPath (Join-Path $RootDir $candidate) -PathType Leaf) {
                $candidate
            } else {
                # Case-insensitive fallback match (e.g. README.zh-tw.md).
                $ciMatch = Get-ChildItem -LiteralPath $RootDir -File -Filter 'README.*.md' |
                    Where-Object {
                        $_.Name -match '^README\.([A-Za-z0-9_-]+)\.md$' -and
                        (Normalize-LocaleCode -Raw $Matches[1]) -eq $locale
                    } |
                    Select-Object -First 1
                if ($ciMatch) { $ciMatch.Name } else { $null }
            }
        }

        $parsed = Parse-ChangelogDocument -Path $file.FullName
        $entries += [pscustomobject]@{
            Locale         = $locale
            Priority       = Get-LocalePriority -Locale $locale
            FileName       = $file.Name
            FullPath       = $file.FullName
            ReadmeFileName = $readmeName
            Title          = $parsed.Title
            Unreleased     = $parsed.Unreleased
            Versions       = $parsed.Versions
        }
    }

    $sorted = @($entries | Sort-Object -Property Priority, Locale)
    if ($sorted.Count -eq 0) {
        throw "No CHANGELOG*.md files discovered in repository root: $RootDir"
    }
    return $sorted
}

function Discover-ReadmeLocales {
    param([Parameter(Mandatory)][string]$RootDir)

    $locales = @()
    $files = Get-ChildItem -LiteralPath $RootDir -File -Filter 'README*.md'
    foreach ($file in $files) {
        if ($file.Name -ceq 'README.md') {
            $locales += [pscustomobject]@{ Locale = 'zh-CN'; FileName = $file.Name }
        } elseif ($file.Name -cmatch '^README\.([A-Za-z0-9_-]+)\.md$') {
            $locales += [pscustomobject]@{
                Locale   = Normalize-LocaleCode -Raw $Matches[1]
                FileName = $file.Name
            }
        }
    }
    return $locales
}

$changelogs = @(Discover-Changelogs -RootDir $repoRoot)
$readmeLocales = @(Discover-ReadmeLocales -RootDir $repoRoot)
$changelogLocaleSet = @($changelogs | ForEach-Object { $_.Locale })

Write-Host ("Discovered changelog locales ({0}): {1}" -f $changelogs.Count, (($changelogs | ForEach-Object { "$($_.Locale) ($($_.FileName))" }) -join ', '))

# Cross-check: every localized README.<locale>.md should have a matching CHANGELOG.<locale>.md.
$missingLocaleChangelogs = @()
foreach ($r in $readmeLocales) {
    if ($r.Locale -notin $changelogLocaleSet) {
        $missingLocaleChangelogs += "$($r.FileName) -> missing CHANGELOG.$($r.Locale).md"
    }
}
if ($missingLocaleChangelogs.Count -gt 0) {
    $msg = "Localized README files without matching CHANGELOG files: $($missingLocaleChangelogs -join '; ')"
    if ($ValidateSync -and -not $AllowMissingChangelog) {
        throw $msg
    } else {
        Write-Warning $msg
    }
}

# Cross-check: all discovered CHANGELOG*.md files must share the exact same set of published versions.
$baseline = $changelogs[0]
$baselineVersions = @($baseline.Versions.Keys)
if ($baselineVersions.Count -eq 0 -and -not $AllowMissingChangelog) {
    throw "No published version headings ('## [vX.Y.Z]') found in $($baseline.FileName)."
}

$syncErrors = @()
foreach ($entry in $changelogs) {
    $entryVersions = @($entry.Versions.Keys)
    $missing = @($baselineVersions | Where-Object { $_ -notin $entryVersions })
    $extra = @($entryVersions | Where-Object { $_ -notin $baselineVersions })
    if ($missing.Count -gt 0) {
        $syncErrors += "$($entry.FileName) is missing versions present in $($baseline.FileName): $($missing -join ', ')"
    }
    if ($extra.Count -gt 0) {
        $syncErrors += "$($entry.FileName) has extra versions absent from $($baseline.FileName): $($extra -join ', ')"
    }
    foreach ($verKey in $entryVersions) {
        if ([string]::IsNullOrWhiteSpace($entry.Versions[$verKey])) {
            $syncErrors += "$($entry.FileName) has an empty section for version $verKey"
        }
    }
}

# Cross-check: PR / Issue references (#N) in published versions must stay aligned across all discovered locales.
foreach ($verKey in $baselineVersions) {
    $baseRefs = @(Get-MarkdownRefNumbers -Text $baseline.Versions[$verKey])
    foreach ($entry in $changelogs) {
        if ($entry.FileName -eq $baseline.FileName -or -not $entry.Versions.Contains($verKey)) {
            continue
        }
        $entryRefs = @(Get-MarkdownRefNumbers -Text $entry.Versions[$verKey])
        $missingRefs = @($baseRefs | Where-Object { $_ -notin $entryRefs } | ForEach-Object { "#$_" })
        $extraRefs = @($entryRefs | Where-Object { $_ -notin $baseRefs } | ForEach-Object { "#$_" })
        if ($missingRefs.Count -gt 0) {
            $syncErrors += "$($entry.FileName) [v$verKey] is missing PR/Issue reference(s) present in $($baseline.FileName): $($missingRefs -join ', ')"
        }
        if ($extraRefs.Count -gt 0) {
            $syncErrors += "$($entry.FileName) [v$verKey] has extra PR/Issue reference(s) absent from $($baseline.FileName): $($extraRefs -join ', ')"
        }
    }
}

# Advisory check for [Unreleased]: warn if PR/Issue references drift across locales without blocking external PRs.
$baseUnreleasedRefs = @(Get-MarkdownRefNumbers -Text $baseline.Unreleased)
foreach ($entry in $changelogs) {
    if ($entry.FileName -eq $baseline.FileName) {
        continue
    }
    $entryUnreleasedRefs = @(Get-MarkdownRefNumbers -Text $entry.Unreleased)
    $missingUnreleased = @($baseUnreleasedRefs | Where-Object { $_ -notin $entryUnreleasedRefs } | ForEach-Object { "#$_" })
    $extraUnreleased = @($entryUnreleasedRefs | Where-Object { $_ -notin $baseUnreleasedRefs } | ForEach-Object { "#$_" })
    if ($missingUnreleased.Count -gt 0 -or $extraUnreleased.Count -gt 0) {
        $details = @()
        if ($missingUnreleased.Count -gt 0) { $details += "missing $($missingUnreleased -join ', ')" }
        if ($extraUnreleased.Count -gt 0) { $details += "extra $($extraUnreleased -join ', ')" }
        Write-CiWarning "$($entry.FileName) [Unreleased] PR/Issue references differ from $($baseline.FileName) ($($details -join '; ')). Sync all CHANGELOG*.md files before tagging a release."
    }
}

if ($syncErrors.Count -gt 0 -and -not $AllowMissingChangelog) {
    throw ("Multilingual changelog sync check failed:`n- " + ($syncErrors -join "`n- "))
}

if (-not [string]::IsNullOrWhiteSpace($version)) {
    $missingTagFiles = @()
    foreach ($entry in $changelogs) {
        if (-not $entry.Versions.Contains($version) -or [string]::IsNullOrWhiteSpace($entry.Versions[$version])) {
            $missingTagFiles += $entry.FileName
        }
    }
    if ($missingTagFiles.Count -gt 0 -and -not $AllowMissingChangelog) {
        throw "Missing release section '## [$Tag]' in: $($missingTagFiles -join ', '). Move items out of [Unreleased] into '## [$Tag] - YYYY-MM-DD' across all discovered CHANGELOG*.md files before tagging."
    }
}

if ($ValidateSync -and [string]::IsNullOrWhiteSpace($OutputPath)) {
    Write-Host ("Multilingual changelog sync check passed: {0} locale(s), {1} published version(s), PR/Issue references aligned." -f $changelogs.Count, $baselineVersions.Count)
    exit 0
}

$generatedSection = $null
if (-not [string]::IsNullOrWhiteSpace($GeneratedNotesPath) -and (Test-Path -LiteralPath $GeneratedNotesPath -PathType Leaf)) {
    $rawGenerated = (Get-Content -Raw -Encoding UTF8 -LiteralPath $GeneratedNotesPath) -replace "`r`n", "`n"
    if (-not [string]::IsNullOrWhiteSpace($rawGenerated)) {
        $generatedSection = $rawGenerated.Trim()
    }
} elseif ($FetchGitHubNotes -and -not [string]::IsNullOrWhiteSpace($Tag)) {
    $ghArgs = @('api', '--method', 'POST', "repos/$Repository/releases/generate-notes", '-f', "tag_name=$Tag", '--jq', '.body')
    if (-not [string]::IsNullOrWhiteSpace($PreviousTag)) {
        $ghArgs += @('-f', "previous_tag_name=$PreviousTag")
    }
    $prevErrorAction = $ErrorActionPreference
    $ErrorActionPreference = 'Continue'
    $generatedLines = & gh @ghArgs 2>$null
    $apiExit = $LASTEXITCODE
    $ErrorActionPreference = $prevErrorAction
    if ($apiExit -eq 0) {
        $rawGenerated = (($generatedLines | ForEach-Object { "$_" }) -join "`n").Trim()
        if (-not [string]::IsNullOrWhiteSpace($rawGenerated)) {
            $generatedSection = $rawGenerated
        }
    } else {
        Write-CiWarning "Failed to fetch GitHub generated release notes for $Tag (exit $apiExit); continuing with CHANGELOG-based notes only."
    }
}

# Audit merged PR & external contributor credit coverage against all localized CHANGELOG sections for this tag.
$mergedPrs = @(Parse-GitHubGeneratedPrs -GeneratedNotes $generatedSection)
$repoOwner = ($Repository -split '/')[0]
if ($mergedPrs.Count -gt 0 -and -not [string]::IsNullOrWhiteSpace($version)) {
    $prCoverageErrors = @()
    $summaryRows = @()
    foreach ($pr in $mergedPrs) {
        $isExternal = -not [string]::Equals($pr.Author, $repoOwner, [System.StringComparison]::OrdinalIgnoreCase) -and ($pr.Author -notmatch '\[bot\]$')
        $missingPrLocales = @()
        $missingAuthorLocales = @()
        foreach ($entry in $changelogs) {
            $body = if ($entry.Versions.Contains($version)) { [string]$entry.Versions[$version] } else { '' }
            $refs = @(Get-MarkdownRefNumbers -Text $body)
            if ($pr.Number -notin $refs) {
                $missingPrLocales += $entry.FileName
            }
            if ($isExternal -and ($body -notmatch "(?i)@$([regex]::Escape($pr.Author))\b")) {
                $missingAuthorLocales += $entry.FileName
            }
        }

        if ($missingPrLocales.Count -gt 0) {
            $msg = "Merged PR #$($pr.Number) (@$($pr.Author): $($pr.Title)) is not referenced in [$Tag] of: $($missingPrLocales -join ', ')."
            $prCoverageErrors += $msg
            Write-CiWarning $msg
        } elseif ($missingAuthorLocales.Count -gt 0) {
            $msg = "External contributor @$($pr.Author) (PR #$($pr.Number)) is not credited in [$Tag] of: $($missingAuthorLocales -join ', ')."
            $prCoverageErrors += $msg
            Write-CiWarning $msg
        }

        $prStatus = if ($missingPrLocales.Count -eq 0) { 'Referenced' } else { "Missing in $($missingPrLocales -join ', ')" }
        $creditStatus = if (-not $isExternal) { 'Maintainer' } elseif ($missingAuthorLocales.Count -eq 0) { 'Credited' } else { "Missing @$($pr.Author)" }
        $safeTitle = $pr.Title -replace '\|', '\|'
        $summaryRows += "| [#$($pr.Number)]($($pr.Url)) | @$($pr.Author) | $safeTitle | $prStatus | $creditStatus |"
    }

    Write-Host ("PR credit audit for {0}: checked {1} merged PR(s) across {2} locale(s) ({3} warning(s))." -f $Tag, $mergedPrs.Count, $changelogs.Count, $prCoverageErrors.Count)

    if (-not [string]::IsNullOrWhiteSpace($env:GITHUB_STEP_SUMMARY)) {
        $localeSummary = ($changelogs | ForEach-Object { "$($_.Locale) ($($_.FileName))" }) -join ', '
        $summaryBlock = @(
            "### Release Notes & PR Credit Audit ($Tag)"
            ''
            "- Locales: $localeSummary"
            "- Merged PRs checked: $($mergedPrs.Count)"
            ''
            '| PR | Author | Title | Changelog Ref | Contributor Credit |'
            '| --- | --- | --- | --- | --- |'
        ) + $summaryRows + @('')
        Add-Content -LiteralPath $env:GITHUB_STEP_SUMMARY -Value ($summaryBlock -join "`n") -Encoding UTF8
    }

    if ($StrictPrCoverage -and $prCoverageErrors.Count -gt 0) {
        throw ("PR credit coverage check failed:`n- " + ($prCoverageErrors -join "`n- "))
    }
}

$blocks = @()

# Top download matrix & platform instructions (Chinese primary header, consistent with previous releases).
$blocks += @"
## 下载文件（Windows x64 / Linux x64 / macOS Universal）

| 平台与文件 | 说明 |
| --- | --- |
| ``MuseLink-$version-windows-x64-setup.exe`` | **Windows 安装版**（NSIS）。安装后支持「关于 → 检查更新」应用内 Minisign 验签更新与自动重启。 |
| ``MuseLink-$version-windows-x64-portable.zip`` | **Windows 免安装便携版**。完整解压后运行 ``muse-connector.exe``（支持新版本提醒与直达下载）。 |
| ``MuseLink-$version-linux-x64.AppImage`` | **Linux x64 AppImage**。赋予执行权限后直接运行（支持应用内 Minisign 验签更新与自动重启）。 |
| ``MuseLink-$version-linux-x64.deb`` | **Linux x64 ``.deb``**。通过包管理器安装（支持新版本提醒与更新详情）。 |
| ``MuseLink-$version-macos-universal.dmg`` | **macOS 安装镜像（Apple Silicon & Intel 通用版）**。打开后将 ``MuseLink.app`` 拖入 ``Applications``。 |
| ``MuseLink-$version-macos-universal.app.zip`` | **macOS 便携包（Apple Silicon & Intel 通用版）**。直接解压即可得到 ``MuseLink.app``。 |
| ``SHA256SUMS.txt`` | 核对上述全部发布产物的 SHA-256 校验和。 |
| ``latest.json`` / ``*.sig`` | 供应用内自动更新器使用的双平台静态更新清单与 Minisign 签名文件。 |

> ⚠️ **macOS 首次启动特别注意（重要）**：
> - 本版本 macOS 包同时原生支持 **Apple Silicon（M 系列）与 Intel Mac**，兼容 **macOS 14（Sonoma）及更早版本**（最低 macOS 10.13 / 11.0，无需 macOS 15）。
> - 由于 macOS 包尚未经过 Apple 开发者签名与公证（Notarization），从浏览器下载后首次打开会被 Gatekeeper 拦截，请按以下两步放行：
>   1. 先双击尝试打开一次 ``MuseLink.app``，被拦截后进入 **系统设置 → 隐私与安全性**，滚动到「安全性」区域点击 **仍要打开**；
>   2. 若在 macOS Sequoia（15+）上提示「已损坏，无法打开」，在终端执行一次以下命令清除下载隔离属性即可正常打开：
>      ``````bash
>      xattr -dr com.apple.quarantine /Applications/MuseLink.app
>      ``````
> - 沙箱命令执行（``exec``）目前仅支持 Windows；macOS 与 Linux 支持完整文件 API（搜索 / 列表 / 读取 / 写入 / 建目录）及 Tailscale / Cloudflare 隧道。

Windows 版本需要 WebView2；由于未进行商业代码签名，Windows 可能提示未知发布者。
"@

$isFirstLocale = $true
foreach ($entry in $changelogs) {
    $meta = Get-LocaleMetadata `
        -Locale $entry.Locale `
        -FileName $entry.FileName `
        -ReadmeFileName $entry.ReadmeFileName `
        -TitleFromFile $entry.Title `
        -Repository $Repository

    $sectionBody = if ($entry.Versions.Contains($version) -and -not [string]::IsNullOrWhiteSpace($entry.Versions[$version])) {
        $entry.Versions[$version]
    } else {
        "- See [$($entry.FileName)](https://github.com/$Repository/blob/main/$($entry.FileName))."
    }

    $subheading = [string]::Format($meta.Subheading, $Tag)
    $localeParts = @()
    if ($isFirstLocale) {
        $localeParts += "### $subheading"
    } else {
        $blocks += '---'
        $localeParts += "## $($meta.Heading)"
        $localeParts += "### $subheading"
    }
    $localeParts += $sectionBody
    if (-not [string]::IsNullOrWhiteSpace($meta.MacosNotice)) {
        $localeParts += $meta.MacosNotice.Trim()
    }
    if (-not [string]::IsNullOrWhiteSpace($meta.Footer)) {
        $localeParts += $meta.Footer
    }

    $blocks += ($localeParts -join "`n`n")
    $isFirstLocale = $false
}

if (-not [string]::IsNullOrWhiteSpace($generatedSection)) {
    $blocks += '---'
    $blocks += $generatedSection
} else {
    $blocks += '---'
    $blocks += "**Release Tag**: https://github.com/$Repository/releases/tag/$Tag"
}

$content = (($blocks -join "`n`n") -replace "`r`n", "`n").TrimEnd() + "`n"
$resolvedOutput = $ExecutionContext.SessionState.Path.GetUnresolvedProviderPathFromPSPath($OutputPath)
$parentDir = Split-Path -Parent $resolvedOutput
if (-not [string]::IsNullOrWhiteSpace($parentDir)) {
    $null = [IO.Directory]::CreateDirectory($parentDir)
}

$utf8NoBom = [System.Text.UTF8Encoding]::new($false)
[IO.File]::WriteAllText($resolvedOutput, $content, $utf8NoBom)
Write-Host ("Generated multilingual release notes for {0} ({1}) at {2} ({3} chars)" -f $Tag, ($changelogLocaleSet -join ', '), $resolvedOutput, $content.Length)
