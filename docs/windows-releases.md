# Windows releases

The Release workflow builds an **x64** installer for **Windows 10 (1903) or newer** alongside the macOS DMG. Publishing a GitHub release attaches `zBase-<version>-windows-x64-setup.exe` and its `.sha256` file to that release. The installer is **not code-signed** yet, so Windows SmartScreen warns on first run; users choose **More info → Run anyway**.

## What the workflow does

The `build-windows` job runs on `windows-2025` and calls `scripts/windows/build_installer.ps1 -Test`, which:

1. Downloads the pinned FFmpeg LGPL shared build from [BtbN/FFmpeg-Builds](https://github.com/BtbN/FFmpeg-Builds) and verifies its SHA-256. It pins a month-end autobuild because those stay published; daily builds are removed after about a month.
2. Runs the Rust test suite in release mode.
3. Builds the web view (`npm ci`, `npm run build`), then `zbase.exe` with the `custom-protocol` feature, which embeds `ui/dist`. On Windows the C and C++ runtimes are linked statically (`.cargo/config.toml`), so users need no Visual C++ Redistributable. `tauri-build` (in `build.rs`) embeds the app icon and the application manifest.
   It also downloads Microsoft's WebView2 bootstrapper into the stage directory; the installer runs it only when no WebView2 Runtime is registered.
4. Stages `zbase.exe`, the FFmpeg DLLs and a `licenses` folder (Keybase emoji, FFmpeg LGPL text and a note naming the exact FFmpeg build).
5. Packages them with Inno Setup (`scripts/windows/zbase.iss`) into a per-user installer: no administrator prompt, installed under `%LOCALAPPDATA%\Programs\zBase`, with a Start Menu entry and an uninstaller under **Settings → Apps**.

The `attach-windows` job then uploads the installer and checksum to the published release. It is separate from the macOS jobs, so a Windows failure does not hold up the DMG, the Homebrew formula or the update feed. Manual runs with a version publish and attach installers automatically. Leave the version blank for a build-only run that uploads a `windows-release` artifact without changing a release.

There is no automatic updater on Windows; users install each new version over the old one.

## Building locally

Install Rust (MSVC toolchain), Visual Studio 2022 Build Tools with the Windows SDK, LLVM and [Inno Setup 6](https://jrsoftware.org/isinfo.php), then run:

```powershell
.\scripts\windows\build_installer.ps1        # add -Test to run the test suite first
```

The installer and checksum are written to `dist\release`. Pass `-FfmpegDir` (or set `FFMPEG_DIR`) to use an FFmpeg copy you already have. Set `CARGO_TARGET_DIR` outside synced folders such as OneDrive to keep build output out of them.

## Updating FFmpeg

Pick a month-end autobuild from the BtbN releases, then update `$ffmpegTag`, `$ffmpegAsset` and `$ffmpegSha256` in `build_installer.ps1`. The release page lists each asset's SHA-256.

## Signing later

To remove the SmartScreen warning, sign `zbase.exe` before packaging and the installer and uninstaller through Inno Setup's `SignTool` setting, using a code-signing certificate or Azure Trusted Signing. Store the credentials as Actions secrets, as the macOS signing credentials are.
