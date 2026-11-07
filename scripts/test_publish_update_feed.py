import base64
import tempfile
import unittest
from pathlib import Path
import xml.etree.ElementTree as ET
from publish_update_feed import publish, verify_existing_archive, SPARKLE


def fixture(version="1.0.15"):
    name = f"zBase-{version}-macos-arm64.dmg"
    digest = "a" * 64
    metadata = dict(version=version, asset=name, architecture="arm64", minimum_system_version="15.0", length=1024, sha256=digest, signature=base64.b64encode(bytes(64)).decode())
    release = dict(tag_name="v" + version, draft=False, prerelease=False, published_at="2026-11-06T12:00:00Z", body="## Improvements\n\n- Fix <&> and Unicode 🙂", assets=[dict(name=name, size=1024, state="uploaded", digest="sha256:" + digest, browser_download_url=f"https://github.com/owner/repo/releases/download/v{version}/{name}")])
    return release, metadata


class FeedTests(unittest.TestCase):
    def test_archive_upload_retries_cannot_replace_advertised_bytes(self):
        release, metadata = fixture()
        verify_existing_archive(release, metadata)
        release["assets"][0]["digest"] = "sha256:" + "b" * 64
        with self.assertRaises(ValueError):
            verify_existing_archive(release, metadata)
        release["assets"] = []
        verify_existing_archive(release, metadata)

    def test_stable_release_is_escaped_and_notes_preserved(self):
        with tempfile.TemporaryDirectory() as temp:
            destination = Path(temp)
            release, metadata = fixture()
            publish(release, metadata, destination, "owner/repo")
            item = ET.parse(destination / "appcast.xml").find("channel/item")
            self.assertEqual(item.findtext(f"{{{SPARKLE}}}version"), "1.0.15")
            self.assertEqual(item.find("enclosure").get(f"{{{SPARKLE}}}edSignature"), metadata["signature"])
            self.assertEqual((destination / "notes/v1.0.15.txt").read_text(), release["body"])
            publish(release, metadata, destination, "owner/repo")
            self.assertEqual(len(ET.parse(destination / "appcast.xml").findall("channel/item")), 1)

    def test_missing_assets_prereleases_and_wrong_digest_leave_feed_unchanged(self):
        for mutation in [lambda r: r.update(prerelease=True), lambda r: r.update(draft=True), lambda r: r.update(assets=[]), lambda r: r["assets"][0].update(size=1), lambda r: r["assets"][0].update(digest="sha256:" + "b" * 64), lambda r: r["assets"][0].update(browser_download_url="https://evil.example/app.dmg")]:
            with self.subTest(mutation=mutation), tempfile.TemporaryDirectory() as temp:
                path = Path(temp)
                release, metadata = fixture()
                publish(release, metadata, path, "owner/repo")
                previous = (path / "appcast.xml").read_bytes()
                mutation(release)
                with self.assertRaises(ValueError):
                    publish(release, metadata, path, "owner/repo")
                self.assertEqual((path / "appcast.xml").read_bytes(), previous)

    def test_out_of_order_builds_do_not_regress_latest_and_replacements_are_rejected(self):
        with tempfile.TemporaryDirectory() as temp:
            path = Path(temp)
            for version in ["1.0.16", "1.0.15", "1.0.9"]:
                publish(*fixture(version), path, "owner/repo")
            items = ET.parse(path / "appcast.xml").findall("channel/item")
            self.assertEqual([i.findtext(f"{{{SPARKLE}}}version") for i in items], ["1.0.16", "1.0.15", "1.0.9"])
            release, metadata = fixture("1.0.16")
            metadata["signature"] = base64.b64encode(bytes([1]) * 64).decode()
            with self.assertRaises(ValueError):
                publish(release, metadata, path, "owner/repo")

    def test_invalid_versions_platforms_and_signatures_are_rejected(self):
        for key, value in [("version", "1.0.16-beta.1"), ("signature", "bad"), ("minimum_system_version", "14.0"), ("architecture", "x86_64")]:
            with self.subTest(key=key), tempfile.TemporaryDirectory() as temp:
                release, metadata = fixture()
                metadata[key] = value
                with self.assertRaises(ValueError):
                    publish(release, metadata, Path(temp), "owner/repo")


if __name__ == "__main__":
    unittest.main()
