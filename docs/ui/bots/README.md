# Keybase bots

Native GPUI captures from isolated offline demo fixtures. People, conversations, and messages are fictional. The fixture catalog is separate from the read-only live RPC checks for Keybase’s featured Zoom entry and DM permissions. No live bot membership changes or meetings were created for these captures.

Reproduce the main scenes from the repository root:

```sh
cargo build --locked --features demo-fixtures
scripts/ui_preview/capture.sh --no-build --out docs/ui/bots bots-dm bot-details featured-zoom-review bot-commands
```

The Bots list follows the Members pane’s spacing, row hover treatment, avatar size, and section labels. Bot details follow the Profile pane’s divided sections. Text uses the shared typography settings.

The review dialog has its own scroll handle and blocks wheel input from reaching the conversation beneath it. Wheel-dispatch regressions exercise the production dialog shell at normal and short window heights, including its bottom boundary, header, footer, and backdrop.

## Cache upgrade

`bot_username` preserves the “Shared with @bot” message indicator after a restart. It is appended to positional MessagePack message-cache records. The new reader accepts older records through serde defaults; previously released readers reject the longer records. This cache change supports upgrading, not downgrading to an earlier reader. Keybase’s server-side messages are unaffected.

Development and official app bundles share the default zBase data directory. A different app name or bundle identifier does not isolate it; set `ZBASE_DATA_DIR` to a separate directory when a side-by-side development run needs an independent cache.
