#!/usr/bin/env bash
# Screenshots the offline demo build in fixed UI scenes, for design review.
#
#   scripts/ui_preview/capture.sh [--no-build] [--out DIR] [--theme light|dark] [--text-size 100..200] [SCENE...]
#
# Scenes: team-details chat chat-grouped chat-previews chat-previews-grouped chat-grouped-collapsed chat-previews-live
# identity-menu order-menu chat-states
# thread thread-empty profile preferences
# quick-switcher quick-switcher-search search search-empty dm members
# members-filter-empty new-chat offline inactive composer-format composer-autocomplete
# reaction-tooltip reaction-tooltip-long (default: all).
# Themes: light and dark (default: both). Output: <out>/<scene>-<theme>.png,
# default out dir target/ui_preview/shots. Each run uses a fresh ZBASE_DATA_DIR, so
# no remembered window size or settings leak in and the user's real settings are
# never touched. Every scene is shot with the window active (key), except
# `inactive`, which hands focus to Finder first; `preferences` shoots the separate
# Settings window. `chat-previews*` scenes run with ZBASE_DEMO_SIDEBAR_STYLE=previews.
# macOS only (screencapture + CoreGraphics window lookup).
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
ALL_SCENES=(dm-details bots-dm featured-zoom featured-zoom-review bot-add bot-add-review bot-remove bots bot-details bot-access bot-commands team-details files-media files-documents files-empty chat chat-grouped chat-previews chat-previews-grouped chat-grouped-collapsed chat-previews-live identity-menu
  order-menu chat-states details details-dropdown thread thread-empty profile preferences quick-switcher
  quick-switcher-search search search-empty dm members members-filter-empty new-chat offline
  inactive composer-format composer-autocomplete reaction-tooltip reaction-tooltip-long)
THEMES=(light dark)
OUT="$ROOT/target/ui_preview/shots"
BUILD=1
TEXT_SIZE=100
RENDER_WAIT_SECS="${ZBASE_PREVIEW_RENDER_WAIT:-2.5}"
WINDOW_TIMEOUT_SECS=30
SCENES=()

usage() {
  sed -n '2,17p' "$0" | sed 's/^# \{0,1\}//'
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --no-build) BUILD=0 ;;
    --out)
      [[ $# -ge 2 ]] || { echo "--out needs a directory" >&2; exit 2; }
      OUT="$2"
      shift
      ;;
    --text-size)
      [[ $# -ge 2 ]] || { echo "--text-size needs a percentage from 100 to 200 (steps of 5)" >&2; exit 2; }
      if [[ "$2" =~ ^[0-9]{3}$ ]] && (( 10#$2 >= 100 && 10#$2 <= 200 && 10#$2 % 5 == 0 )); then
        TEXT_SIZE="$2"
      else
        echo "invalid text size: $2 (expected 100..200 in steps of 5)" >&2
        exit 2
      fi
      shift
      ;;
    --theme)
      [[ $# -ge 2 ]] || { echo "--theme needs light or dark" >&2; exit 2; }
      case "$2" in light|dark) THEMES=("$2") ;; *) echo "unknown theme: $2" >&2; exit 2 ;; esac
      shift
      ;;
    -h|--help) usage; exit 0 ;;
    -*) echo "unknown option: $1" >&2; usage >&2; exit 2 ;;
    *)
      known=0
      for scene in "${ALL_SCENES[@]}"; do [[ "$scene" == "$1" ]] && known=1; done
      [[ $known == 1 ]] || { echo "unknown scene: $1 (known: ${ALL_SCENES[*]})" >&2; exit 2; }
      SCENES+=("$1")
      ;;
  esac
  shift
done
[[ ${#SCENES[@]} -gt 0 ]] || SCENES=("${ALL_SCENES[@]}")

TARGET_DIR="$(cd "$ROOT" && cargo metadata --no-deps --format-version 1 | python3 -c 'import json, sys; print(json.load(sys.stdin)["target_directory"])')"
BIN="$TARGET_DIR/debug/zbase"
HELPER_DIR="$ROOT/target/ui_preview"
WINDOW_ID="$HELPER_DIR/window_id"
WINDOW_ID_SRC="$ROOT/scripts/ui_preview/window_id.swift"

if [[ $BUILD == 1 ]]; then
  echo "==> Building demo binary"
  # ffmpeg-next needs ffmpeg 8; Homebrew's default ffmpeg may be newer.
  (cd "$ROOT" && PKG_CONFIG_PATH=/opt/homebrew/opt/ffmpeg@8/lib/pkgconfig \
    cargo build --locked --features demo-fixtures)
fi
[[ -x "$BIN" ]] || { echo "missing $BIN; run without --no-build" >&2; exit 1; }

mkdir -p "$HELPER_DIR" "$OUT"
if [[ ! -x "$WINDOW_ID" || "$WINDOW_ID_SRC" -nt "$WINDOW_ID" ]]; then
  echo "==> Compiling window_id helper"
  swiftc -O -o "$WINDOW_ID" "$WINDOW_ID_SRC"
fi

APP_PID=""
DATA_DIR=""
cleanup() {
  if [[ -n "$APP_PID" ]] && kill -0 "$APP_PID" 2>/dev/null; then
    kill "$APP_PID" 2>/dev/null || true
    wait "$APP_PID" 2>/dev/null || true
  fi
  [[ -n "$DATA_DIR" && -d "$DATA_DIR" ]] && rm -rf "$DATA_DIR"
  APP_PID=""
  DATA_DIR=""
}
trap cleanup EXIT INT TERM

failures=()
for scene in "${SCENES[@]}"; do
  for theme in "${THEMES[@]}"; do
    shot="$OUT/$scene-$theme.png"
    log="$OUT/$scene-$theme.log"
    DATA_DIR="$(mktemp -d "${TMPDIR:-/tmp}/zbase-ui-preview.XXXXXX")"
    sidebar_style=compact
    [[ "$scene" == chat-previews* ]] && sidebar_style=previews
    (cd "$ROOT" && exec env \
      ZBASE_DATA_DIR="$DATA_DIR" \
      ZBASE_BENCH_USE_DEMO=1 \
      ZBASE_BENCH_SKIP_BACKEND=1 \
      ZBASE_DEMO_SCENE="$scene" \
      ZBASE_DEMO_THEME="$theme" \
      ZBASE_DEMO_TEXT_SIZE="$TEXT_SIZE" \
      ZBASE_DEMO_SIDEBAR_STYLE="$sidebar_style" \
      "$BIN") >"$log" 2>&1 &
    APP_PID=$!

    # The Settings scene is its own window; everything else is the main window.
    title_args=()
    [[ "$scene" == preferences ]] && title_args=(Settings)
    wid=""
    deadline=$((SECONDS + WINDOW_TIMEOUT_SECS))
    while [[ $SECONDS -lt $deadline ]]; do
      kill -0 "$APP_PID" 2>/dev/null || break
      if wid="$("$WINDOW_ID" "$APP_PID" ${title_args[@]+"${title_args[@]}"} 2>/dev/null)"; then break; fi
      wid=""
      sleep 0.2
    done

    if [[ -z "$wid" ]]; then
      echo "!! $scene/$theme: no window (see $log)" >&2
      failures+=("$scene-$theme")
      cleanup
      continue
    fi

    # The demo build activates itself; also ask System Events, in case macOS
    # refused self-activation (best effort if not permitted).
    osascript -e "tell application \"System Events\" to set frontmost of (first process whose unix id is $APP_PID) to true" \
      >/dev/null 2>&1 || true
    sleep "$RENDER_WAIT_SECS"
    if [[ "$scene" == inactive ]]; then
      # Once the scene has fully rendered, hand focus to Finder so the window
      # resigns key and redraws in its background style.
      open -a Finder
      sleep 1
    else
      # Someone using the Mac meanwhile may have taken focus; take it back so
      # the shot shows the active window.
      for _ in 1 2 3 4 5; do
        front="$(osascript -e 'tell application "System Events" to get unix id of first process whose frontmost is true' 2>/dev/null || true)"
        [[ -z "$front" || "$front" == "$APP_PID" ]] && break
        osascript -e "tell application \"System Events\" to set frontmost of (first process whose unix id is $APP_PID) to true" \
          >/dev/null 2>&1 || true
        sleep 0.7
      done
    fi
    if kill -0 "$APP_PID" 2>/dev/null && screencapture -x -o -l "$wid" "$shot"; then
      rm -f "$log"
      echo "    $shot"
    else
      echo "!! $scene/$theme: capture failed (see $log)" >&2
      failures+=("$scene-$theme")
    fi
    cleanup
  done
done

if [[ ${#failures[@]} -gt 0 ]]; then
  echo "Failed: ${failures[*]}" >&2
  exit 1
fi
