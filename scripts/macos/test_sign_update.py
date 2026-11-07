"""Sign a fixture with a throwaway test key; never access a developer's Keychain."""
import base64
import hashlib
import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest


@unittest.skipUnless(os.environ.get("SPARKLE_DIR"), "Requires the pinned Sparkle tools")
class UpdateSigningTests(unittest.TestCase):
    def test_signature_metadata_and_key_pair_validation(self):
        # Deterministic fixture-only seed, not a production credential.
        seed = base64.b64encode(bytes(range(32))).decode()
        code = f'''import Foundation
import CryptoKit
let key = try Curve25519.Signing.PrivateKey(rawRepresentation: Data(base64Encoded: "{seed}")!)
print(key.publicKey.rawRepresentation.base64EncodedString())
'''
        public = subprocess.check_output(["swift", "-"], input=code, text=True).strip()
        with tempfile.TemporaryDirectory(prefix="zbase-update-sign-test-") as temp:
            archive = Path(temp) / "zBase-1.0.15-macos-arm64.dmg"
            archive.write_bytes(b"Fixture archive, never an installable application")
            env = dict(os.environ, SPARKLE_PRIVATE_KEY=seed, SPARKLE_PUBLIC_KEY=public)
            script = Path(__file__).with_name("sign_update.sh")
            subprocess.run([str(script), str(archive), "1.0.15"], env=env, check=True, capture_output=True)
            metadata = json.loads((archive.parent / "update.json").read_text())
            self.assertEqual(metadata["sha256"], hashlib.sha256(archive.read_bytes()).hexdigest())
            self.assertEqual(metadata["length"], archive.stat().st_size)
            self.assertEqual(len(base64.b64decode(metadata["signature"])), 64)
            env["SPARKLE_PUBLIC_KEY"] = base64.b64encode(bytes(32)).decode()
            result = subprocess.run([str(script), str(archive), "1.0.15"], env=env, capture_output=True)
            self.assertNotEqual(result.returncode, 0)


if __name__ == "__main__":
    unittest.main()
