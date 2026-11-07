#!/usr/bin/env python3
"""Generate the updates branch from a completed, signed GitHub release.

No API credentials or private signing keys are written into the feed. The caller
serializes publication and pushes only after this script validates every input.
"""
import argparse
import base64
import json
import re
from pathlib import Path
from urllib.parse import quote
import xml.etree.ElementTree as ET

SPARKLE = "http://www.andymatuschak.org/xml-namespaces/sparkle"
ET.register_namespace("sparkle", SPARKLE)


def version_tuple(version):
    if not re.fullmatch(r"[0-9]+\.[0-9]+\.[0-9]+", version):
        raise ValueError("Expected a stable semantic version")
    return tuple(map(int, version.split(".")))


def verify_existing_archive(release, metadata):
    for asset in release.get("assets", []):
        if asset["name"] == metadata["asset"]:
            if asset.get("digest") != "sha256:" + metadata["sha256"] or asset.get("size") != metadata["length"]:
                raise ValueError("Refusing to overwrite an existing release installer with different bytes; publish a new version")


def publish(release, metadata, destination, repository):
    version = metadata["version"]
    version_tuple(version)
    if release.get("draft") or release.get("prerelease"):
        raise ValueError("Only published stable releases belong in the update feed")
    if not release.get("published_at") or release["tag_name"] != "v" + version:
        raise ValueError("Published release and signed bundle version must match")
    expected_name = f"zBase-{version}-macos-arm64.dmg"
    if metadata["asset"] != expected_name or metadata["architecture"] != "arm64" or metadata["minimum_system_version"] != "15.0":
        raise ValueError("Unexpected update platform or asset name")
    signature = base64.b64decode(metadata["signature"], validate=True)
    if len(signature) != 64:
        raise ValueError("Invalid Ed25519 signature")
    if not re.fullmatch(r"[0-9a-f]{64}", metadata["sha256"]):
        raise ValueError("Invalid archive digest")
    assets = [asset for asset in release.get("assets", []) if asset["name"] == expected_name]
    if len(assets) != 1:
        raise ValueError("Verified installer has not been uploaded")
    asset = assets[0]
    expected_url = f"https://github.com/{repository}/releases/download/v{version}/{expected_name}"
    if asset.get("state") != "uploaded" or asset["size"] != metadata["length"] or asset["size"] <= 0 or asset["browser_download_url"] != expected_url:
        raise ValueError("Uploaded installer does not match signed build metadata")
    # GitHub includes a server-computed digest on new uploaded release assets.
    if asset.get("digest") and asset["digest"] != "sha256:" + metadata["sha256"]:
        raise ValueError("Uploaded installer checksum mismatch")
    feed = destination / "appcast.xml"
    if feed.exists():
        root = ET.fromstring(feed.read_bytes())
        channel = root.find("channel")
        if root.tag != "rss" or channel is None:
            raise ValueError("Invalid existing appcast")
    else:
        root = ET.Element("rss", version="2.0")
        channel = ET.SubElement(root, "channel")
        ET.SubElement(channel, "title").text = "zBase updates"
        ET.SubElement(channel, "link").text = f"https://github.com/{repository}/releases"
        ET.SubElement(channel, "description").text = "Verified stable macOS updates"
    for existing in list(channel.findall("item")):
        existing_version = existing.findtext(f"{{{SPARKLE}}}version")
        if existing_version == version:
            # A version's signed archive must be immutable after announcement.
            enclosure = existing.find("enclosure")
            if enclosure is None or enclosure.get(f"{{{SPARKLE}}}edSignature") != metadata["signature"]:
                raise ValueError("Refusing to replace an already advertised update archive")
            channel.remove(existing)
    item = ET.SubElement(channel, "item")
    ET.SubElement(item, "title").text = f"zBase {version}"
    ET.SubElement(item, f"{{{SPARKLE}}}version").text = version
    ET.SubElement(item, f"{{{SPARKLE}}}shortVersionString").text = version
    ET.SubElement(item, f"{{{SPARKLE}}}minimumSystemVersion").text = "15.0"
    ET.SubElement(item, "link").text = f"https://github.com/{repository}/releases/tag/v{version}"
    ET.SubElement(item, f"{{{SPARKLE}}}releaseNotesLink").text = f"https://raw.githubusercontent.com/{repository}/updates/notes/v{quote(version)}.txt"
    ET.SubElement(item, "enclosure", {
        "url": expected_url, "length": str(metadata["length"]), "type": "application/octet-stream",
        f"{{{SPARKLE}}}edSignature": metadata["signature"],
    })
    # Concurrent releases may finish in reverse order. Never regress latest.
    items = sorted(channel.findall("item"), key=lambda entry: version_tuple(entry.findtext(f"{{{SPARKLE}}}version")), reverse=True)
    for entry in channel.findall("item"):
        channel.remove(entry)
    for entry in items[:20]:
        channel.append(entry)
    notes = destination / "notes"
    notes.mkdir(parents=True, exist_ok=True)
    (notes / f"v{version}.txt").write_text(release.get("body") or "See this release on GitHub for details.", encoding="utf-8")
    ET.indent(root)
    output = ET.tostring(root, encoding="utf-8", xml_declaration=True)
    temporary = feed.with_suffix(".xml.tmp")
    temporary.write_bytes(output)
    temporary.replace(feed)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--release", type=Path, required=True)
    parser.add_argument("--metadata", type=Path, required=True)
    parser.add_argument("--destination", type=Path)
    parser.add_argument("--verify-existing-archive", action="store_true")
    parser.add_argument("--repository", required=True)
    args = parser.parse_args()
    if not re.fullmatch(r"[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+", args.repository):
        parser.error("Invalid repository")
    release, metadata = json.loads(args.release.read_text()), json.loads(args.metadata.read_text())
    if args.verify_existing_archive:
        verify_existing_archive(release, metadata)
    else:
        if args.destination is None:
            parser.error("--destination is required when publishing")
        publish(release, metadata, args.destination, args.repository)


if __name__ == "__main__":
    main()
