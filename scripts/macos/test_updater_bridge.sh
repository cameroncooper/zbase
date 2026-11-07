#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
: "${SPARKLE_DIR:?Run scripts/macos/prepare_sparkle.sh first}"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT
clang -fobjc-arc -fblocks -Wall -Wextra -Werror=protocol -mmacosx-version-min=15.0 \
  -F "$SPARKLE_DIR" -framework AppKit -framework Sparkle \
  "$ROOT/native/updater_tests.m" -o "$WORK/updater-tests"
DYLD_FRAMEWORK_PATH="$SPARKLE_DIR" "$WORK/updater-tests"
