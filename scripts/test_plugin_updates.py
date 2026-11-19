#!/usr/bin/env python3
"""Tests for deterministic registered-plugin release assets."""

from __future__ import annotations

import hashlib
import json
from pathlib import Path
import tempfile
import unittest

from scripts.models import release_manifest, table
from scripts.plugin_updates import build_manifest, write_bundle


AUDIO_PACKAGE = """format = 1

[effect]
id = "yumo.volume"
name = "Volume"
kind = "audio"
version = 2

[ffmpeg]
chain = "volume=1"
"""

RELEASE = """[release]
min_app = "0.2.11"
"""


def package(root: Path, folder: str = "yumo.volume", source: str = AUDIO_PACKAGE) -> Path:
    target = root / folder
    target.mkdir(parents=True)
    (target / "effect.toml").write_text(source, encoding="utf-8")
    (target / "release.toml").write_text(RELEASE, encoding="utf-8")
    return target


class PluginManifestTests(unittest.TestCase):
    def test_manifest_matches_package_and_hashes_every_file(self):
        with tempfile.TemporaryDirectory() as root:
            folder = package(Path(root))
            data = build_manifest(Path(root), "Beau1314/YumoCut", "v0.2.12")
            self.assertEqual(data["schema"], 1)
            self.assertEqual(len(data["plugins"]), 1)
            plugin = data["plugins"][0]
            self.assertEqual(plugin["id"], "yumo.volume")
            self.assertEqual(plugin["version"], 2)
            self.assertEqual(plugin["format"], 1)
            self.assertEqual(plugin["min_app"], "0.2.11")
            manifest = next(file for file in plugin["files"] if file["path"] == "effect.toml")
            payload = (folder / "effect.toml").read_bytes()
            self.assertEqual(manifest["bytes"], len(payload))
            self.assertEqual(manifest["sha256"], hashlib.sha256(payload).hexdigest())
            self.assertEqual(
                manifest["url"],
                "https://github.com/Beau1314/YumoCut/releases/download/v0.2.12/yumo.volume-2-effect.toml",
            )

    def test_ordering_and_output_are_deterministic(self):
        with tempfile.TemporaryDirectory() as root:
            root_path = Path(root)
            package(root_path, "z-last", AUDIO_PACKAGE.replace("yumo.volume", "yumo.last"))
            first = package(root_path, "a-first")
            (first / "nested").mkdir()
            (first / "nested" / "notes.txt").write_text("notes", encoding="utf-8")
            one = build_manifest(root_path, "Beau1314/YumoCut", "v0.2.12")
            two = build_manifest(root_path, "Beau1314/YumoCut", "v0.2.12")
            self.assertEqual(json.dumps(one, sort_keys=True), json.dumps(two, sort_keys=True))
            self.assertEqual([p["id"] for p in one["plugins"]], ["yumo.volume", "yumo.last"])
            paths = [file["path"] for file in one["plugins"][0]["files"]]
            self.assertEqual(paths, sorted(paths))

    def test_duplicate_plugin_ids_are_rejected(self):
        with tempfile.TemporaryDirectory() as root:
            root_path = Path(root)
            package(root_path, "one")
            package(root_path, "two")
            with self.assertRaisesRegex(ValueError, "listed twice"):
                build_manifest(root_path, "Beau1314/YumoCut", "v0.2.12")

    def test_repository_and_tag_cannot_change_the_download_origin(self):
        with tempfile.TemporaryDirectory() as root:
            root_path = Path(root)
            package(root_path)
            for repository in ("http://example.test/repo", "Beau1314", "Beau1314/YumoCut/issues"):
                with self.subTest(repository=repository), self.assertRaises(ValueError):
                    build_manifest(root_path, repository, "v0.2.12")
            with self.assertRaises(ValueError):
                build_manifest(root_path, "Beau1314/YumoCut", "../../other")

    def test_asset_name_collisions_are_rejected(self):
        with tempfile.TemporaryDirectory() as root:
            folder = package(Path(root))
            (folder / "a--b").write_bytes(b"flat")
            (folder / "a").mkdir()
            (folder / "a" / "b").write_bytes(b"nested")
            with self.assertRaisesRegex(ValueError, "asset name collision"):
                build_manifest(Path(root), "Beau1314/YumoCut", "v0.2.12")

    def test_bundle_contains_the_manifest_and_exact_plugin_files(self):
        with tempfile.TemporaryDirectory() as root:
            root_path = Path(root)
            folder = package(root_path)
            (folder / "nested").mkdir()
            (folder / "nested" / "notes.txt").write_text("notes", encoding="utf-8")
            out = root_path / "bundle" / "plugins.json"
            data = write_bundle(root_path, "Beau1314/YumoCut", "v0.2.12", out)
            self.assertEqual(json.loads(out.read_text(encoding="utf-8")), data)
            for plugin in data["plugins"]:
                for file in plugin["files"]:
                    asset = out.parent / file["url"].rsplit("/", 1)[1]
                    self.assertEqual(asset.read_bytes(), (folder / file["path"]).read_bytes())


class ReleaseManifestTests(unittest.TestCase):
    def test_windows_binaries_use_the_fork_but_models_keep_their_mirror(self):
        with tempfile.TemporaryDirectory() as root:
            bundles = Path(root)
            for name in (
                "YumoCut-0.2.12-windows-x86_64-setup.exe",
                "YumoCut-0.2.12-windows-x86_64.msi",
            ):
                (bundles / name).write_bytes(b"installer")
            data = release_manifest(
                "0.2.12",
                "v0.2.12",
                bundles,
                repository="Beau1314/YumoCut",
                product="YumoCut",
            )
            windows = data["binaries"]["windows"]["x86_64"]
            self.assertEqual(data["product"], "YumoCut")
            self.assertEqual(
                windows["setup"]["url"],
                "https://github.com/Beau1314/YumoCut/releases/download/v0.2.12/YumoCut-0.2.12-windows-x86_64-setup.exe",
            )
            self.assertEqual(
                windows["msi"]["url"],
                "https://github.com/Beau1314/YumoCut/releases/download/v0.2.12/YumoCut-0.2.12-windows-x86_64.msi",
            )
            model = next(iter(data["models"].values()))
            mirror = f"https://github.com/jub0t/Concat/releases/download/{table()['release']}/{model['file']}"
            self.assertEqual(model["url"], mirror)
            self.assertEqual(model["sources"][0], mirror)


class ProvenanceTests(unittest.TestCase):
    def test_fork_notice_names_upstream_baseline_and_source(self):
        text = Path("FORK_NOTICES.md").read_text(encoding="utf-8")
        for required in (
            "YumoCut is an unofficial, modified version of Concat",
            "https://github.com/jub0t/Concat",
            "v0.2.11",
            "6584d34971ae5e168f12cb475bdf58055b67d780",
            "https://github.com/Beau1314/YumoCut",
            "2026-11-20",
            "AGPL-3.0-or-later",
            "Third-party components retain their own licences",
            "No additional codec patent licence",
        ):
            self.assertIn(required, text)

    def test_upstream_licence_files_and_windows_notice_staging_remain(self):
        for name in ("LICENSE", "LICENSING.md", "THIRD_PARTY_NOTICES.md", "TRADEMARK.md"):
            self.assertTrue(Path(name).is_file(), name)
        build = Path(".github/workflows/build-app.yml").read_text(encoding="utf-8")
        for name in ("LICENSE", "LICENSING.md", "THIRD_PARTY_NOTICES.md", "TRADEMARK.md", "FORK_NOTICES.md"):
            self.assertIn(name, build)

    def test_readme_does_not_present_the_fork_as_upstream(self):
        text = Path("README.md").read_text(encoding="utf-8")
        self.assertIn("YumoCut is an unofficial, modified version of Concat", text)
        self.assertIn("https://github.com/Beau1314/YumoCut/releases", text)
        self.assertIn("https://github.com/Beau1314/YumoCut/issues", text)
        self.assertIn("FORK_NOTICES.md", text)


if __name__ == "__main__":
    unittest.main()
