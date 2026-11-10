# Builds zBase in release mode and packages it as
# dist\release\zBase-<version>-windows-x64-setup.exe, with a .sha256 file beside it.
#
# Requires Rust (MSVC toolchain), Visual Studio Build Tools with the Windows SDK,
# LLVM (for libclang) and Inno Setup 6. The pinned FFmpeg shared libraries are
# downloaded and checksum-verified into dist\windows unless -FfmpegDir or
# FFMPEG_DIR points at an existing copy. -Test runs the release test suite first.
param(
    [string]$FfmpegDir = $env:FFMPEG_DIR,
    [switch]$Test
)

$ErrorActionPreference = 'Stop'
$repo = Resolve-Path (Join-Path $PSScriptRoot '..\..')
$dist = Join-Path $repo 'dist\windows'
$stage = Join-Path $dist 'stage'
$releaseDir = Join-Path $repo 'dist\release'

# An unmodified LGPL shared build from https://github.com/BtbN/FFmpeg-Builds.
# Month-end autobuilds stay published; daily ones are pruned after about a month.
$ffmpegTag = 'autobuild-2026-10-31-12-59'
$ffmpegAsset = 'ffmpeg-n8.1.3-27-g8dca1586f4-win64-lgpl-shared-8.1'
$ffmpegSha256 = '5cef7bc48e513f3cee7c881e8b2999d835541edb1846c3e89443a67f8f8556ab'

function Find-Tool([string]$name, [string[]]$candidates) {
    $command = Get-Command $name -ErrorAction SilentlyContinue
    if ($command) { return $command.Source }
    foreach ($pattern in $candidates) {
        $found = Resolve-Path $pattern -ErrorAction SilentlyContinue |
            Sort-Object { $_.Path } -Descending | Select-Object -First 1
        if ($found) { return $found.Path }
    }
    throw "Could not find $name"
}

New-Item -ItemType Directory -Force $dist, $releaseDir | Out-Null

if (-not $FfmpegDir) {
    $FfmpegDir = Join-Path $dist "ffmpeg\$ffmpegAsset"
    if (-not (Test-Path $FfmpegDir)) {
        $downloads = New-Item -ItemType Directory -Force (Join-Path $dist 'downloads')
        $zip = Join-Path $downloads "$ffmpegAsset.zip"
        if (-not (Test-Path $zip)) {
            Write-Host "Downloading FFmpeg ($ffmpegAsset)"
            Invoke-WebRequest "https://github.com/BtbN/FFmpeg-Builds/releases/download/$ffmpegTag/$ffmpegAsset.zip" -OutFile $zip
        }
        $actual = (Get-FileHash -Algorithm SHA256 $zip).Hash.ToLowerInvariant()
        if ($actual -ne $ffmpegSha256) {
            Remove-Item $zip
            throw "FFmpeg checksum mismatch: expected $ffmpegSha256, got $actual"
        }
        Expand-Archive $zip (Join-Path $dist 'ffmpeg') -Force
    }
}

if (-not $env:LIBCLANG_PATH) {
    $env:LIBCLANG_PATH = Split-Path (Find-Tool 'clang.exe' @("$env:ProgramFiles\LLVM\bin\clang.exe"))
}
$iscc = Find-Tool 'ISCC.exe' @(
    "$env:LOCALAPPDATA\Programs\Inno Setup 6\ISCC.exe",
    "${env:ProgramFiles(x86)}\Inno Setup 6\ISCC.exe"
)
$env:FFMPEG_DIR = $FfmpegDir

Push-Location $repo
try {
    # Cargo reports progress on stderr, which Windows PowerShell treats as an error under 'Stop'.
    $ErrorActionPreference = 'Continue'
    # The binary embeds the built web view (ui/dist), so build it first.
    npm ci --no-audit --no-fund
    if ($LASTEXITCODE -ne 0) { throw 'npm ci failed' }
    npm run build
    if ($LASTEXITCODE -ne 0) { throw 'web view build failed' }
    if ($Test) {
        # Test binaries load the FFmpeg DLLs from PATH.
        $env:PATH = "$(Join-Path $FfmpegDir 'bin');$env:PATH"
        cargo test --locked --release --all-targets --features custom-protocol
        if ($LASTEXITCODE -ne 0) { throw 'cargo test failed' }
    }
    cargo build --locked --release --features custom-protocol
    $ErrorActionPreference = 'Stop'
    if ($LASTEXITCODE -ne 0) { throw 'cargo build failed' }
    $targetDir = if ($env:CARGO_TARGET_DIR) { $env:CARGO_TARGET_DIR } else { Join-Path $repo 'target' }

    if (Test-Path $stage) { Remove-Item -Recurse -Force $stage }
    New-Item -ItemType Directory $stage | Out-Null
    Copy-Item (Join-Path $targetDir 'release\zbase.exe') $stage
    Copy-Item (Join-Path $FfmpegDir 'bin\*.dll') $stage
    # Windows 10 may lack the WebView2 runtime; the installer runs Microsoft's
    # small online bootstrapper only when it is missing.
    Invoke-WebRequest -UseBasicParsing 'https://go.microsoft.com/fwlink/p/?LinkId=2124703' -OutFile (Join-Path $stage 'MicrosoftEdgeWebview2Setup.exe')

    $licenses = New-Item -ItemType Directory (Join-Path $stage 'licenses\FFmpeg')
    Copy-Item (Join-Path $repo 'assets\emoji\LICENSE.keybase') (Join-Path $stage 'licenses\Keybase.txt')
    Copy-Item (Join-Path $FfmpegDir 'LICENSE.txt') (Join-Path $licenses 'LICENSE.txt')
    Set-Content -Encoding utf8 (Join-Path $licenses 'README.txt') @(
        "zBase uses the FFmpeg libraries (the avcodec, avformat, avutil, swscale and other DLLs"
        "next to zbase.exe) under the GNU Lesser General Public License; see LICENSE.txt."
        "They are the unmodified LGPL shared build $(Split-Path $FfmpegDir -Leaf) from"
        "https://github.com/BtbN/FFmpeg-Builds, whose build scripts list every component."
        "FFmpeg source code: https://ffmpeg.org/download.html"
        "You may replace these DLLs with your own compatible build of FFmpeg."
    )

    $version = (Select-String -Path Cargo.toml -Pattern '^version = "(.+)"' | Select-Object -First 1).Matches.Groups[1].Value
    $name = "zBase-$version-windows-x64-setup"
    & $iscc "/DAppVersion=$version" "/DStageDir=$stage" "/O$releaseDir" "/F$name" (Join-Path $PSScriptRoot 'zbase.iss')
    if ($LASTEXITCODE -ne 0) { throw 'Inno Setup failed' }

    $installer = Join-Path $releaseDir "$name.exe"
    $hash = (Get-FileHash -Algorithm SHA256 $installer).Hash.ToLowerInvariant()
    # The same "<hash>  <file>" line as `shasum -a 256` writes for the macOS DMG.
    [System.IO.File]::WriteAllText("$installer.sha256", "$hash  $name.exe`n")
    Write-Host "Installer: $installer"
}
finally {
    Pop-Location
}
