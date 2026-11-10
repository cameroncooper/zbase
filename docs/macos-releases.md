# macOS releases

The Release workflow builds a self-contained **Apple Silicon** DMG for **macOS 15 or newer**. It signs the app and disk image with Developer ID, submits both to Apple's notary service, staples their tickets, and checks signatures and Gatekeeper assessment before uploading assets. Intel Macs are not supported by this initial release job.

## Repository configuration

Under Settings → Secrets and variables → Actions, configure:

| Kind | Name | Value |
| --- | --- | --- |
| Secret | `MACOS_CERTIFICATE_P12_BASE64` | Base64-encoded Developer ID Application certificate **and private key**, exported as a password-protected `.p12` |
| Secret | `MACOS_CERTIFICATE_PASSWORD` | Export password for that `.p12` |
| Secret | `APPLE_API_PRIVATE_KEY` | Complete App Store Connect team API `.p8` private key, including PEM header/footer |
| Variable | `MACOS_SIGNING_IDENTITY` | Developer ID Application identity name or SHA-1 fingerprint |
| Variable | `APPLE_TEAM_ID` | Apple Developer team ID |
| Variable | `APPLE_API_KEY_ID` | App Store Connect API key ID |
| Variable | `APPLE_API_ISSUER_ID` | Issuer ID for the team API key |
| Secret | `TAP_TOKEN` | Token with contents-write permission on `cameroncooper/homebrew-tap` |

The signing certificate must be Developer ID **Application**, not Apple Development or Developer ID Installer. The notary API key must have permission to submit notarization requests. Before compilation, credentials are imported into a temporary keychain and added to the user keychain search list. The workflow validates the configured identity and team, then signs and verifies a disposable executable to check private-key access and timestamping; an unconditional cleanup step removes the keychain and private key file. Artifacts contain no credential files.

## Test before publishing

Merge the workflow to the default branch first so GitHub exposes its manual Run workflow control. Run **Release** from the desired branch. Manual runs only build artifacts; they never attach assets to a release. This performs the full signing and notarization pipeline and uploads a `macos-release` artifact, without creating a GitHub release or changing the Homebrew tap. Download the DMG from that workflow run and test it on a Mac without development dependencies installed.

## Publish

1. Open **Actions → Release → Run workflow**, select `main`, and enter the next stable version without `v` (for example, `1.0.14`).
2. Run the workflow. It bumps `Cargo.toml` and `Cargo.lock`, commits the bump to `main`, pushes the matching annotated tag, and publishes a GitHub release with automatically generated notes. It then builds both platforms and attaches their installers and checksums. macOS signing, notarization, Homebrew and update-feed publication remain automatic.

Leave **version** blank to run the full build/signing pipeline without creating a release, attaching assets, or updating Homebrew/the feed. Branch build tests are allowed; publishing requires `main`.

Version input accepts patch, minor, or major increases and rejects malformed, unchanged, older, or already-tagged versions. Dependencies remain unchanged. The version commit and tag are pushed atomically: if `main` rejects the commit, neither ref is changed. The built-in `GITHUB_TOKEN` needs contents-write access, and repository branch/tag rules must allow this workflow to push. No PR creation permission or extra token is needed. Merge the workflow changes before using the new input.

Both platform builds check out the exact release commit. Builds run in the same workflow because events created with `GITHUB_TOKEN` do not start another release workflow. See [GitHub's token-trigger rules](https://docs.github.com/en/actions/reference/workflows-and-actions/events-that-trigger-workflows).

The release is public immediately, with generated notes; installers appear as each platform finishes. You can edit the notes afterward in GitHub. Existing externally published releases still trigger the build, provided their tags match the committed package version. Immutable releases must remain disabled because assets are attached after publication.

The optional CLI helper `scripts/release.sh VERSION` starts this same workflow on `main`.

If a build, signing, upload, Homebrew, or feed job fails, use **Re-run failed jobs** on the original run. Do not rerun successful preparation or move the tag. Upload retries preserve release metadata. Rebuilding an already attached macOS version with different bytes is rejected; publish a new version instead. If preparation fails after the atomic push (for example, GitHub's release API is unavailable), create and publish the release for the existing tag in GitHub to trigger the builds; do not move or recreate that tag.

Reruns use the workflow from the original run. To include workflow/script fixes, validate them with a blank-version build and publish a new version.

Ticket stapling and validation retry transient Apple service failures up to four times with 10, 20, and 40 second delays. Every check must still succeed; invalid, missing, or revoked tickets fail immediately. Persistent service outages fail the build and require a rerun.

## Packaging and media libraries

`scripts/macos/build_media_deps.sh` builds pinned, checksum-verified FFmpeg and dav1d sources with a macOS 15 deployment target. FFmpeg uses shared libraries, disables GPL/nonfree components and automatic discovery of installed third-party libraries, and includes dav1d for AV1 decoding. Source archives accompany the release; licenses, configuration and the bundled library list are included in the app's Resources/Licenses/Media directory.

`scripts/macos/bundle_dylibs.py` copies the recursive library graph from that explicit build prefix into Contents/Frameworks, rewrites install names, and rejects unresolved dependencies, external non-system paths, incorrect architectures, or libraries requiring an OS newer than macOS 15. It can be rerun without `--prefix` to verify a prepared bundle. Ordinary local/source Homebrew builds continue to use the developer's installed media libraries; the official DMG is self-contained.

To reproduce the unsigned packaging locally (no installed app is replaced):

```sh
scripts/macos/build_media_deps.sh
export PKG_CONFIG_PATH="$PWD/dist/macos-deps/prefix/lib/pkgconfig"
export PKG_CONFIG_LIBDIR="$PKG_CONFIG_PATH"
scripts/macos/build_app_bundle.sh release
python3 scripts/macos/bundle_dylibs.py dist/macos/zBase.app --prefix "$PWD/dist/macos-deps/prefix"
```

The unsigned output is for development only. `scripts/macos/package_release.sh` requires the signing/notary environment described above and produces the distributable DMG.

## In-app updates

Official bundles include the pinned Sparkle 2 framework and a zBase update
popover. Development and Homebrew builds do not initialize the updater. The
first release containing this feature must be installed manually; subsequent
releases can be installed inside the app.

The app checks the stable HTTPS feed at
`https://raw.githubusercontent.com/cameroncooper/zbase/updates/appcast.xml`.
GitHub Pages is not required. The `updates` branch is generated by Actions and
must not be merged into `main`. The Release workflow creates it on first use;
ordinary CI only runs on PRs and pushes to `main`.

Before publishing the first updater-enabled release:

1. Run `scripts/macos/prepare_sparkle.sh` on a trusted Mac.
2. Run `dist/sparkle/bin/generate_keys --account zbase-updates`. Keep this key
   backed up securely. Do not generate a new key for each release.
3. Set the repository **Actions variable** `SPARKLE_PUBLIC_KEY` to the printed
   public key. The build embeds this public value in `Info.plist`.
4. Export the private key with
   `dist/sparkle/bin/generate_keys --account zbase-updates -x /secure/path/zbase-update-key`.
   Set the repository **Actions secret** `SPARKLE_PRIVATE_KEY` to the exported
   file's contents (already base64 text). Do not commit it or add another layer
   of base64 encoding. Remove the temporary export after securely backing it up.
5. Ensure the Actions token can push the `updates` branch. If branch rules
   cover it, explicitly permit this publishing workflow rather than force-pushing.

A release build validates the key pair, embeds and signs Sparkle's nested
helpers, signs/notarizes the app and DMG, then Ed25519-signs the final DMG. Only
after attachment succeeds does the serialized feed job read the latest
`updates` branch, validate the uploaded asset against signed build metadata,
and commit the appcast plus UTF-8 release notes copied from that GitHub release.
Drafts/prereleases never enter the stable feed. Older builds finishing late
cannot demote a newer version. Homebrew publication runs independently, so a
failed tap update does not delay in-app updates.

Published installer bytes must remain immutable. Retrying attachment with the
same artifact is safe; rebuilding a previously attached version with different
bytes is rejected. Publish a new version instead. Retrying only the feed job
is safe while its build artifact is retained. Feed publication is intentionally
not attempted by build-only workflow runs. Release notes are captured when the
feed job runs; rerun it to republish edited notes while the artifact is available.

The updater uses Sparkle for checks, downloads, pre-extraction signature
verification, installation, and relaunch. It sends no system profile information.
Checks default on, every six hours; background downloads default off. The
popover offers release notes, Later, Skip this version, download/cancel/retry,
and an explicit Restart and update action. Prepared updates can also install
when the user quits normally, without relaunching; the quit hook saves text drafts
too. Scheduled reminders wait for a pause
in typing and are snoozed for 24 hours. Settings and the zBase menu provide
manual checks. Running from a read-only DMG explains that the app must be moved
to Applications.

Before restarting, zBase saves account-scoped text drafts, the conversation,
reading anchor, and supported right panes to a private atomic file under
Application Support. It flushes window preferences, rechecks pending sends,
and only then releases Sparkle's install callback. Pending sends and unsent
attachments block restart with an explanation; attachments are not silently
discarded. Restoration consumes the file after the matching account has loaded.
Settings now live under Application Support too; source-build settings in the
current working directory are migrated when no new settings file exists.

### Validation before rollout

CI compiles the native bridge and exercises its callback/cancellation lifecycle,
runs the Rust tests with `macos-updater`, tests appcast generation/rejection and
packaging with an embedded framework, and checks shell/workflow syntax.

Before the first public release, run a signed upgrade rehearsal with two
increasing app versions and a separate staging feed/bundle identifier. Use a
test signing key, not the production key. Confirm:

- Manual and scheduled checks, stable/pre-release filtering, Later, and Skip.
- Release notes, download progress/cancel/retry, offline and failed verification.
- No restart until explicitly requested; failed authorization leaves the app usable.
- Standard-user `/Applications` authorization and launch from a read-only DMG.
- Drafts across conversations, thread drafts, open pane/window geometry, and a
  scrolled-back reading anchor survive the old-to-new relaunch.
- Homebrew/source builds never replace themselves.

Local native checks (without touching an installed app or production feed):

```sh
scripts/macos/prepare_sparkle.sh
export SPARKLE_DIR="$PWD/dist/sparkle"
export DYLD_FRAMEWORK_PATH="$SPARKLE_DIR"
scripts/macos/test_updater_bridge.sh
npm ci && npm run build
cargo test --locked --all-targets --features custom-protocol,macos-updater
```
