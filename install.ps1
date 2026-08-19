<#
.SYNOPSIS
Installs Convertalot (image-converter) from the latest GitHub release.

.DESCRIPTION
Downloads the newest (or a pinned) Windows release of Convertalot from
https://github.com/sniffle6/image-converter, verifies its SHA-256 checksum,
extracts it to a per-user install folder, adds that folder to the user PATH,
and creates a Start Menu shortcut for the GUI. The installer records the files
it puts in the install folder, and uninstall removes only those files — it
never deletes anything else, even in a shared folder.

One-line install:
    irm https://raw.githubusercontent.com/sniffle6/image-converter/main/install.ps1 | iex

The piped form cannot receive parameters, so every option also has an
environment-variable override:
    CONVERTALOT_VERSION        Pin a release tag (e.g. v0.1.0 or 0.1.0).
    CONVERTALOT_INSTALL_DIR    Install somewhere other than %LOCALAPPDATA%\Programs\Convertalot.
    CONVERTALOT_NO_PATH        Set to 1 to skip the user PATH update.
    CONVERTALOT_NO_SHORTCUT    Set to 1 to skip the Start Menu shortcut.
    CONVERTALOT_SKIP_CHECKSUM  Set to 1 to install a release that has no checksum file.
    CONVERTALOT_UNINSTALL      Set to 1 to uninstall instead of install.

To pass parameters directly, run through a script block instead of iex:
    & ([scriptblock]::Create((irm https://raw.githubusercontent.com/sniffle6/image-converter/main/install.ps1))) -Version v0.1.0

.PARAMETER Version
Release tag to install (with or without the leading "v"). Defaults to the latest release.

.PARAMETER InstallDir
Destination folder. Defaults to %LOCALAPPDATA%\Programs\Convertalot.

.PARAMETER NoPath
Skip adding the install folder to the user PATH.

.PARAMETER NoShortcut
Skip creating the Start Menu shortcut for the GUI.

.PARAMETER SkipChecksum
Install even if the release publishes no SHA256SUMS.txt. Off by default.

.PARAMETER Uninstall
Remove the installed files, the PATH entry, and the Start Menu shortcut.
Only files recorded by the installer are deleted.
#>
[CmdletBinding()]
param(
    [string]$Version,
    [string]$InstallDir,
    [switch]$NoPath,
    [switch]$NoShortcut,
    [switch]$SkipChecksum,
    [switch]$Uninstall
)

$ErrorActionPreference = 'Stop'

$Repo = 'sniffle6/image-converter'
$AppName = 'Convertalot'
$AssetSuffix = 'x86_64-pc-windows-msvc.zip'
$ChecksumAssetName = 'SHA256SUMS.txt'
$ManifestName = '.convertalot-manifest.txt'
$Executables = @('image-converter.exe', 'image-converter-gui.exe')

if (-not $Version -and $env:CONVERTALOT_VERSION) { $Version = $env:CONVERTALOT_VERSION }
if (-not $InstallDir -and $env:CONVERTALOT_INSTALL_DIR) { $InstallDir = $env:CONVERTALOT_INSTALL_DIR }
if (-not $NoPath -and $env:CONVERTALOT_NO_PATH -eq '1') { $NoPath = $true }
if (-not $NoShortcut -and $env:CONVERTALOT_NO_SHORTCUT -eq '1') { $NoShortcut = $true }
if (-not $SkipChecksum -and $env:CONVERTALOT_SKIP_CHECKSUM -eq '1') { $SkipChecksum = $true }
if (-not $Uninstall -and $env:CONVERTALOT_UNINSTALL -eq '1') { $Uninstall = $true }

if (-not $InstallDir) {
    $InstallDir = Join-Path $env:LOCALAPPDATA "Programs\$AppName"
}
# Resolve relative paths against the caller's PowerShell location (the .NET
# process CWD can differ from $PWD); works whether or not the path exists yet.
$InstallDir = $ExecutionContext.SessionState.Path.GetUnresolvedProviderPathFromPSPath($InstallDir)

$StartMenuShortcut = Join-Path ([Environment]::GetFolderPath('Programs')) "$AppName.lnk"

function Send-EnvironmentChange {
    # [Environment]::SetEnvironmentVariable broadcasts this itself; raw registry
    # writes must do it by hand or Explorer never refreshes its environment.
    if (-not ('Convertalot.Native' -as [type])) {
        Add-Type -Namespace Convertalot -Name Native -MemberDefinition @'
[DllImport("user32.dll", SetLastError = true, CharSet = CharSet.Unicode)]
public static extern IntPtr SendMessageTimeout(IntPtr hWnd, uint Msg, UIntPtr wParam, string lParam, uint fuFlags, uint uTimeout, out UIntPtr lpdwResult);
'@
    }
    $result = [UIntPtr]::Zero
    # HWND_BROADCAST=0xFFFF, WM_SETTINGCHANGE=0x1A, SMTO_ABORTIFHUNG=0x2
    [void][Convertalot.Native]::SendMessageTimeout([IntPtr]0xFFFF, 0x1A, [UIntPtr]::Zero, 'Environment', 2, 5000, [ref]$result)
}

# The user Path is usually REG_EXPAND_SZ; [Environment]::GetEnvironmentVariable
# expands %VARS% and SetEnvironmentVariable writes back REG_SZ, permanently
# flattening entries like %JAVA_HOME%\bin. Use the registry directly and
# preserve the existing value kind.
function Remove-FromUserPath([string]$Directory) {
    $key = [Microsoft.Win32.Registry]::CurrentUser.OpenSubKey('Environment', $true)
    if (-not $key) { return }
    try {
        if ($key.GetValueNames() -notcontains 'Path') { return }
        $kind = $key.GetValueKind('Path')
        if ($kind -ne [Microsoft.Win32.RegistryValueKind]::String -and $kind -ne [Microsoft.Win32.RegistryValueKind]::ExpandString) { return }
        $current = [string]$key.GetValue('Path', '', [Microsoft.Win32.RegistryValueOptions]::DoNotExpandEnvironmentNames)
        $segments = $current -split ';'
        $kept = @($segments | Where-Object { $_.TrimEnd('\') -ne $Directory.TrimEnd('\') })
        if ($kept.Count -ne $segments.Count) {
            $key.SetValue('Path', ($kept -join ';'), $kind)
            Send-EnvironmentChange
            Write-Host "Removed $Directory from your user PATH."
        }
    } finally { $key.Close() }
}

function Add-ToUserPath([string]$Directory) {
    $key = [Microsoft.Win32.Registry]::CurrentUser.CreateSubKey('Environment')
    try {
        $kind = [Microsoft.Win32.RegistryValueKind]::ExpandString
        $current = ''
        if ($key.GetValueNames() -contains 'Path') {
            $existingKind = $key.GetValueKind('Path')
            if ($existingKind -eq [Microsoft.Win32.RegistryValueKind]::String -or $existingKind -eq [Microsoft.Win32.RegistryValueKind]::ExpandString) {
                $kind = $existingKind
            }
            $current = [string]$key.GetValue('Path', '', [Microsoft.Win32.RegistryValueOptions]::DoNotExpandEnvironmentNames)
        }
        $already = $false
        foreach ($entry in @($current -split ';' | Where-Object { $_ })) {
            if ($entry.TrimEnd('\') -eq $Directory.TrimEnd('\')) { $already = $true; break }
        }
        if (-not $already) {
            $newValue = $Directory
            if ($current) { $newValue = $current.TrimEnd(';') + ';' + $Directory }
            $key.SetValue('Path', $newValue, $kind)
            Send-EnvironmentChange
            Write-Host "Added $Directory to your user PATH. Open a new terminal to pick it up."
        }
    } finally { $key.Close() }
    $sessionEntries = @($env:Path -split ';' | Where-Object { $_ })
    $inSession = $false
    foreach ($entry in $sessionEntries) {
        if ($entry.TrimEnd('\') -eq $Directory.TrimEnd('\')) { $inSession = $true; break }
    }
    if (-not $inSession) { $env:Path = $env:Path.TrimEnd(';') + ';' + $Directory }
}

function Stop-InstalledProcesses {
    foreach ($exe in $Executables) {
        $name = [IO.Path]::GetFileNameWithoutExtension($exe)
        $target = Join-Path $InstallDir $exe
        $running = @(Get-Process -Name $name -ErrorAction SilentlyContinue | Where-Object {
            $_.Path -and ($_.Path.TrimEnd('\') -eq $target.TrimEnd('\'))
        })
        if ($running.Count -gt 0) {
            Write-Host "Closing running $exe so its file can be replaced..."
            $running | Stop-Process -Force -ErrorAction SilentlyContinue
            $running | Wait-Process -Timeout 10 -ErrorAction SilentlyContinue
        }
    }
}

if ($Uninstall) {
    Write-Host "Uninstalling $AppName..."
    Stop-InstalledProcesses
    if (Test-Path $StartMenuShortcut) {
        Remove-Item $StartMenuShortcut -Force
        Write-Host "Removed Start Menu shortcut."
    }
    Remove-FromUserPath $InstallDir
    if (Test-Path $InstallDir) {
        # Delete only the files this installer put there, never the whole tree:
        # the install dir may be a shared folder holding unrelated programs.
        $manifestPath = Join-Path $InstallDir $ManifestName
        $ownedFiles = @($Executables)
        if (Test-Path $manifestPath) {
            $ownedFiles = @(Get-Content $manifestPath | Where-Object { $_ }) + $ManifestName
        }
        $removedAny = $false
        $dirPrefix = $InstallDir.TrimEnd('\') + '\'
        foreach ($name in $ownedFiles) {
            $resolved = [IO.Path]::GetFullPath((Join-Path $InstallDir $name))
            if (-not $resolved.StartsWith($dirPrefix, [StringComparison]::OrdinalIgnoreCase)) { continue }
            if (Test-Path $resolved) {
                Remove-Item $resolved -Force
                $removedAny = $true
            }
        }
        if (-not (Get-ChildItem $InstallDir -Force | Select-Object -First 1)) {
            Remove-Item $InstallDir -Force
            Write-Host "Removed $InstallDir."
        } elseif ($removedAny) {
            Write-Host "Removed $AppName files from $InstallDir; other files there were left in place."
        } else {
            Write-Host "Nothing installed at $InstallDir."
        }
    } else {
        Write-Host "Nothing installed at $InstallDir."
    }
    Write-Host "$AppName has been uninstalled."
    return
}

# Windows PowerShell 5.1 does not negotiate TLS 1.2 by default.
if ($PSVersionTable.PSVersion.Major -lt 6) {
    [Net.ServicePointManager]::SecurityProtocol = [Net.ServicePointManager]::SecurityProtocol -bor [Net.SecurityProtocolType]::Tls12
}

if ($env:PROCESSOR_ARCHITECTURE -eq 'ARM64' -or $env:PROCESSOR_ARCHITEW6432 -eq 'ARM64') {
    Write-Warning "No native ARM64 build is published; installing the x64 build, which runs under emulation on Windows 11."
} elseif (-not [Environment]::Is64BitOperatingSystem) {
    throw "$AppName requires 64-bit Windows."
}

$apiHeaders = @{
    'User-Agent' = 'convertalot-installer'
    'Accept'     = 'application/vnd.github+json'
}

if ($Version) {
    $Version = 'v' + $Version.TrimStart('v', 'V')
    # A tag is a plain token; anything else could rewrite the API URL
    # (e.g. "v1/../../other/repo") and fetch a different repository's release.
    if ($Version -notmatch '^v[0-9A-Za-z][0-9A-Za-z.+_-]*$') {
        throw "Invalid version tag '$Version'. Expected something like v0.1.0."
    }
    $releaseUri = "https://api.github.com/repos/$Repo/releases/tags/$Version"
    $releaseLabel = "release $Version"
} else {
    $releaseUri = "https://api.github.com/repos/$Repo/releases/latest"
    $releaseLabel = 'the latest release'
}

Write-Host "Looking up $releaseLabel of $AppName..."
try {
    $release = Invoke-RestMethod -Uri $releaseUri -Headers $apiHeaders -UseBasicParsing
} catch {
    $status = 0
    try { $status = [int]$_.Exception.Response.StatusCode } catch { }
    $detail = $_.Exception.Message
    if ($_.ErrorDetails -and $_.ErrorDetails.Message) { $detail = $_.ErrorDetails.Message }
    if ($status -eq 403 -or $status -eq 429) {
        throw "GitHub API request was rate-limited or blocked (HTTP $status) while looking up $releaseLabel. The unauthenticated API allows 60 requests per hour per IP; wait a few minutes and re-run. ($detail)"
    } elseif ($status -eq 404) {
        throw "Could not find $releaseLabel of $Repo. Check https://github.com/$Repo/releases for available versions."
    } elseif ($status -gt 0) {
        throw "GitHub API returned HTTP $status while looking up $releaseLabel of $Repo. ($detail)"
    } else {
        throw "Could not reach the GitHub API to look up $releaseLabel of $Repo. Check your network or proxy and try again. ($detail)"
    }
}

$asset = @($release.assets | Where-Object { $_.name -like "*$AssetSuffix" }) | Select-Object -First 1
if (-not $asset) {
    throw "Release $($release.tag_name) has no $AssetSuffix asset. It may still be building; check https://github.com/$Repo/releases."
}
$checksumAsset = @($release.assets | Where-Object { $_.name -eq $ChecksumAssetName }) | Select-Object -First 1

# Defense in depth: only install assets hosted by this repository.
$expectedPrefix = "https://github.com/$Repo/releases/download/"
foreach ($releaseAsset in @($asset, $checksumAsset)) {
    if ($releaseAsset -and $releaseAsset.browser_download_url -notlike "$expectedPrefix*") {
        throw "Release asset URL '$($releaseAsset.browser_download_url)' is not from $Repo; refusing to install."
    }
}

$tempDir = Join-Path $env:TEMP ("convertalot-install-" + [Guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $tempDir | Out-Null

$previousProgress = $ProgressPreference
$ProgressPreference = 'SilentlyContinue'
try {
    $zipPath = Join-Path $tempDir $asset.name
    Write-Host "Downloading $($asset.name) ($([math]::Round($asset.size / 1MB, 1)) MB)..."
    Invoke-WebRequest -Uri $asset.browser_download_url -OutFile $zipPath -UseBasicParsing -Headers @{ 'User-Agent' = 'convertalot-installer' }

    if ($checksumAsset) {
        $sumsPath = Join-Path $tempDir $ChecksumAssetName
        Invoke-WebRequest -Uri $checksumAsset.browser_download_url -OutFile $sumsPath -UseBasicParsing -Headers @{ 'User-Agent' = 'convertalot-installer' }
        $sumLine = @(Get-Content $sumsPath | Where-Object { $_ -match [regex]::Escape($asset.name) }) | Select-Object -First 1
        if (-not $sumLine) {
            throw "$ChecksumAssetName does not list $($asset.name); refusing to install."
        }
        $expectedHash = ($sumLine.Trim() -split '\s+')[0]
        $actualHash = (Get-FileHash $zipPath -Algorithm SHA256).Hash
        if ($actualHash -ne $expectedHash) {
            throw "SHA-256 mismatch for $($asset.name). Expected $expectedHash but got $actualHash. The download may be corrupt; try again."
        }
        Write-Host "SHA-256 checksum verified."
    } elseif ($SkipChecksum) {
        Write-Warning "Release $($release.tag_name) publishes no $ChecksumAssetName; continuing WITHOUT checksum verification because CONVERTALOT_SKIP_CHECKSUM/-SkipChecksum is set."
    } else {
        throw "Release $($release.tag_name) publishes no $ChecksumAssetName; refusing to install. Pass -SkipChecksum (or set CONVERTALOT_SKIP_CHECKSUM=1) to override."
    }

    # Extract to a staging folder and validate it BEFORE touching an existing
    # install, so a truncated or malformed archive cannot destroy a working app.
    $stageDir = Join-Path $tempDir 'stage'
    Expand-Archive -Path $zipPath -DestinationPath $stageDir -Force
    foreach ($exe in $Executables) {
        if (-not (Test-Path (Join-Path $stageDir $exe))) {
            throw "The release archive is missing $exe; refusing to install. The asset may be malformed."
        }
    }
    $stagedNames = @(Get-ChildItem $stageDir -Recurse -File | ForEach-Object {
        $_.FullName.Substring($stageDir.Length + 1)
    })

    Stop-InstalledProcesses
    New-Item -ItemType Directory -Path $InstallDir -Force | Out-Null
    try {
        Copy-Item -Path (Join-Path $stageDir '*') -Destination $InstallDir -Recurse -Force
    } catch {
        throw "Could not write to $InstallDir - a previous install there may now be damaged. Close any running $AppName windows and re-run this installer. ($($_.Exception.Message))"
    }
    $stagedNames | Set-Content (Join-Path $InstallDir $ManifestName) -Encoding ascii
} finally {
    $ProgressPreference = $previousProgress
    Remove-Item $tempDir -Recurse -Force -ErrorAction SilentlyContinue
}

# Smoke-test the installed binary BEFORE wiring up PATH and the Start Menu,
# so a broken binary is never made discoverable.
$installedVersion = (& (Join-Path $InstallDir 'image-converter.exe') --version | Out-String).Trim()
if ($LASTEXITCODE -ne 0 -or -not $installedVersion) {
    throw "image-converter.exe --version failed (exit $LASTEXITCODE); the installed binary appears to be broken. Nothing was added to your PATH or Start Menu."
}

if (-not $NoPath) {
    Add-ToUserPath $InstallDir
}

if (-not $NoShortcut) {
    $shell = New-Object -ComObject WScript.Shell
    $shortcut = $shell.CreateShortcut($StartMenuShortcut)
    $shortcut.TargetPath = Join-Path $InstallDir 'image-converter-gui.exe'
    $shortcut.WorkingDirectory = $InstallDir
    $shortcut.Description = "$AppName image converter"
    $shortcut.Save()
    Write-Host "Start Menu shortcut created."
}

Write-Host ""
Write-Host "$AppName $($release.tag_name) installed to $InstallDir ($installedVersion)."
Write-Host "  CLI: run 'image-converter --help' (new terminals only, unless this one was updated)."
Write-Host "  GUI: launch '$AppName' from the Start Menu or run image-converter-gui.exe."
Write-Host "  Uninstall: re-run this script with -Uninstall (or CONVERTALOT_UNINSTALL=1)."
