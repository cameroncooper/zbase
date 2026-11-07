#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
PROFILE="${1:-debug}"
APP_NAME="${APP_NAME:-zBase}"
BUNDLE_IDENTIFIER="${BUNDLE_IDENTIFIER:-com.zbase.app}"
ICON_PATH="${ROOT_DIR}/assets/macos/AppIcon.icns"
VERSION="$(awk -F'"' '/^version = / { print $2; exit }' "${ROOT_DIR}/Cargo.toml")"
SKIP_BUILD="${SKIP_BUILD:-0}"
export MACOSX_DEPLOYMENT_TARGET=15.0
BUILD_FEATURES=gpui/runtime_shaders
if [[ "${ZBASE_OFFICIAL_RELEASE:-0}" == 1 ]]; then
  : "${SPARKLE_DIR:?Run scripts/macos/prepare_sparkle.sh first}" "${SPARKLE_PUBLIC_KEY:?}"
  BUILD_FEATURES="$BUILD_FEATURES,macos-updater"
fi
TARGET_DIR="$(cargo metadata --format-version 1 --no-deps --manifest-path "${ROOT_DIR}/Cargo.toml" | plutil -extract target_directory raw -o - -)"

if [[ "${PROFILE}" == "release" ]]; then
  BIN_PATH="${TARGET_DIR}/release/zbase"
  if [[ "${SKIP_BUILD}" != "1" ]]; then
    cargo build --release --locked --bin zbase --features "$BUILD_FEATURES" --timings --manifest-path "${ROOT_DIR}/Cargo.toml"
  fi
else
  BIN_PATH="${TARGET_DIR}/debug/zbase"
  if [[ "${SKIP_BUILD}" != "1" ]]; then
    cargo build --locked --bin zbase --features "$BUILD_FEATURES" --timings --manifest-path "${ROOT_DIR}/Cargo.toml"
  fi
fi

if [[ ! -f "${BIN_PATH}" ]]; then
  echo "Binary missing at ${BIN_PATH}"
  echo "Build first (or run this script without SKIP_BUILD=1)."
  exit 1
fi

if [[ ! -f "${ICON_PATH}" ]]; then
  echo "Icon missing at ${ICON_PATH}"
  echo "Generate it with iconutil from your source PNG before bundling."
  exit 1
fi

OUT_DIR="${ROOT_DIR}/dist/macos"
APP_DIR="${OUT_DIR}/${APP_NAME}.app"
CONTENTS_DIR="${APP_DIR}/Contents"
MACOS_DIR="${CONTENTS_DIR}/MacOS"
RESOURCES_DIR="${CONTENTS_DIR}/Resources"
PLIST_PATH="${CONTENTS_DIR}/Info.plist"

rm -rf "${APP_DIR}"
mkdir -p "${MACOS_DIR}" "${RESOURCES_DIR}"

cp "${BIN_PATH}" "${MACOS_DIR}/zbase"
chmod +x "${MACOS_DIR}/zbase"
cp "${ICON_PATH}" "${RESOURCES_DIR}/AppIcon.icns"
mkdir -p "${RESOURCES_DIR}/Licenses"
cp "${ROOT_DIR}/assets/emoji/LICENSE.keybase" "${RESOURCES_DIR}/Licenses/Keybase.txt"

cat > "${PLIST_PATH}" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleDevelopmentRegion</key>
  <string>en</string>
  <key>CFBundleDisplayName</key>
  <string>${APP_NAME}</string>
  <key>CFBundleExecutable</key>
  <string>zbase</string>
  <key>CFBundleIconFile</key>
  <string>AppIcon</string>
  <key>CFBundleIdentifier</key>
  <string>${BUNDLE_IDENTIFIER}</string>
  <key>CFBundleInfoDictionaryVersion</key>
  <string>6.0</string>
  <key>CFBundleName</key>
  <string>${APP_NAME}</string>
  <key>CFBundlePackageType</key>
  <string>APPL</string>
  <key>CFBundleShortVersionString</key>
  <string>${VERSION}</string>
  <key>CFBundleVersion</key>
  <string>${VERSION}</string>
  <key>LSMinimumSystemVersion</key>
  <string>15.0</string>
  <key>NSHighResolutionCapable</key>
  <true/>
</dict>
</plist>
EOF

plutil -lint "${PLIST_PATH}" >/dev/null

echo "Built app bundle: ${APP_DIR}"

if [[ "${ZBASE_OFFICIAL_RELEASE:-0}" == 1 ]]; then
  mkdir -p "$CONTENTS_DIR/Frameworks"
  ditto "$SPARKLE_DIR/Sparkle.framework" "$CONTENTS_DIR/Frameworks/Sparkle.framework"
  cp "$SPARKLE_DIR/LICENSE" "$RESOURCES_DIR/Licenses/Sparkle.txt"
  python3 - "$PLIST_PATH" <<'PYCONFIG'
import base64, os, pathlib, plistlib, sys
path = pathlib.Path(sys.argv[1])
key = os.environ["SPARKLE_PUBLIC_KEY"]
assert len(base64.b64decode(key, validate=True)) == 32, "Invalid Sparkle public key"
config = plistlib.loads(path.read_bytes())
config.update(ZBaseDistribution="official", SUFeedURL="https://raw.githubusercontent.com/cameroncooper/zbase/updates/appcast.xml", SUPublicEDKey=key, SUEnableAutomaticChecks=True, SUAutomaticallyUpdate=False, SUSendProfileInfo=False, SUScheduledCheckInterval=21600, SUVerifyUpdateBeforeExtraction=True)
path.write_bytes(plistlib.dumps(config))
PYCONFIG
fi
