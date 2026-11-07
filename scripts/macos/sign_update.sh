#!/usr/bin/env bash
# The key is a Sparkle generate_keys -x export stored as a GitHub Actions secret.
set -euo pipefail
: "${SPARKLE_DIR:?}" "${SPARKLE_PRIVATE_KEY:?}" "${SPARKLE_PUBLIC_KEY:?}"
ARCHIVE="${1:?Usage: sign_update.sh archive.dmg version}"
VERSION="${2:?}"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT
chmod 700 "$WORK"
printf '%s' "$SPARKLE_PRIVATE_KEY" > "$WORK/key"
chmod 600 "$WORK/key"
# Fail before publishing if the secret doesn't match the public key embedded in the app.
cat > "$WORK/check.swift" <<'SWIFT'
import Foundation
import CryptoKit
let raw = Data(base64Encoded: try String(contentsOfFile: CommandLine.arguments[1], encoding: .utf8).trimmingCharacters(in: .whitespacesAndNewlines))!
let key = try Curve25519.Signing.PrivateKey(rawRepresentation: raw.prefix(32))
guard key.publicKey.rawRepresentation.base64EncodedString() == ProcessInfo.processInfo.environment["SPARKLE_PUBLIC_KEY"] else {
    FileHandle.standardError.write(Data("Sparkle private/public key mismatch\n".utf8))
    exit(1)
}
SWIFT
swift "$WORK/check.swift" "$WORK/key"
"$SPARKLE_DIR/bin/sign_update" --ed-key-file "$WORK/key" -p "$ARCHIVE" > "$WORK/signature"
"$SPARKLE_DIR/bin/sign_update" --verify --ed-key-file "$WORK/key" "$ARCHIVE" "$(cat "$WORK/signature")"
python3 - "$ARCHIVE" "$VERSION" "$WORK/signature" <<'PY'
import hashlib, json, pathlib, sys
archive = pathlib.Path(sys.argv[1])
metadata = dict(version=sys.argv[2], asset=archive.name, architecture="arm64", minimum_system_version="15.0", length=archive.stat().st_size, sha256=hashlib.sha256(archive.read_bytes()).hexdigest(), signature=pathlib.Path(sys.argv[3]).read_text().strip())
(archive.parent / "update.json").write_text(json.dumps(metadata, indent=2) + "\n")
PY
