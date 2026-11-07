#!/usr/bin/env bash
# Pinned upstream framework/tools; never resolve "latest" during a release build.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
DEST="${SPARKLE_DIR:-$ROOT/dist/sparkle}"
VERSION=2.10.1
SHA256=1a856d2a9edb8e628de88121d7be04cf0e2167f8641650b78fca00b14244cf27
mkdir -p "$DEST"
ARCHIVE="$DEST/Sparkle-$VERSION.tar.xz"
if [[ ! -f "$ARCHIVE" ]]; then
  curl --fail --location --retry 3 "https://github.com/sparkle-project/Sparkle/releases/download/$VERSION/Sparkle-$VERSION.tar.xz" -o "$ARCHIVE.tmp"
  mv "$ARCHIVE.tmp" "$ARCHIVE"
fi
printf '%s  %s\n' "$SHA256" "$ARCHIVE" | shasum -a 256 --check
# Extract on every invocation so a partially extracted framework is never reused.
tar -xf "$ARCHIVE" -C "$DEST"
echo "Prepared Sparkle $VERSION at $DEST"
