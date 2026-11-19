#!/usr/bin/env python3
"""Build the registered-plugin catalog and its deterministic release assets.

Every package keeps Concat's strict ``effect.toml`` unchanged. YumoCut's own
``release.toml`` carries the minimum compatible app version separately:

    [release]
    min_app = "0.2.11"

The output manifest and all plugin files share one directory, ready to attach
to the same GitHub release. Only Python's standard library is required.
"""

from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path
import re
import shutil
import tomllib


REPOSITORY = re.compile(r"^[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+$")
TAG = re.compile(r"^v[0-9]+\.[0-9]+\.[0-9]+(?:-[0-9A-Za-z.]+)?$")
VERSION = re.compile(r"^[0-9]+\.[0-9]+\.[0-9]+$")
PLUGIN_ID = re.compile(r"^[a-z0-9]+(?:[._-][a-z0-9]+)*$")
ASSET_NAME = re.compile(r"^[A-Za-z0-9._-]+$")


def parse_effect_meta(folder: Path) -> dict:
    """Read package identity and YumoCut compatibility without loosening TOML."""
    manifest_path = folder / "effect.toml"
    release_path = folder / "release.toml"
    try:
        manifest = tomllib.loads(manifest_path.read_text(encoding="utf-8"))
        release = tomllib.loads(release_path.read_text(encoding="utf-8"))
    except (OSError, UnicodeError, tomllib.TOMLDecodeError) as error:
        raise ValueError(f"{folder.name}: invalid package metadata: {error}") from error

    effect = manifest.get("effect")
    release_meta = release.get("release")
    if not isinstance(effect, dict) or not isinstance(release_meta, dict):
        raise ValueError(f"{folder.name}: [effect] and [release] tables are required")

    meta = {
        "id": effect.get("id"),
        "version": effect.get("version", 1),
        "format": manifest.get("format", 1),
        "min_app": release_meta.get("min_app"),
    }
    if not isinstance(meta["id"], str) or not PLUGIN_ID.fullmatch(meta["id"]):
        raise ValueError(f"{folder.name}: the plugin id is missing or not URL-safe")
    for key in ("version", "format"):
        value = meta[key]
        if isinstance(value, bool) or not isinstance(value, int) or value < 1:
            raise ValueError(f"{folder.name}: {key} must be a positive integer")
    if not isinstance(meta["min_app"], str) or not VERSION.fullmatch(meta["min_app"]):
        raise ValueError(f"{folder.name}: release.min_app must be a stable x.y.z version")
    return meta


def file_entry(folder: Path, path: Path, repository: str, tag: str, meta: dict) -> dict:
    """Describe one file and the flat asset name under which it is published."""
    if path.is_symlink() or not path.is_file():
        raise ValueError(f"{folder.name}: {path} is not a regular package file")
    relative = path.relative_to(folder).as_posix()
    asset = f"{meta['id']}-{meta['version']}-" + relative.replace("/", "--")
    if not ASSET_NAME.fullmatch(asset):
        raise ValueError(f"{folder.name}: {relative} does not make a URL-safe asset name")
    payload = path.read_bytes()
    return {
        "path": relative,
        "url": f"https://github.com/{repository}/releases/download/{tag}/{asset}",
        "bytes": len(payload),
        "sha256": hashlib.sha256(payload).hexdigest(),
    }


def build_manifest(root: Path, repository: str, tag: str) -> dict:
    """Build a schema-1 catalog in stable folder/path order."""
    if not REPOSITORY.fullmatch(repository):
        raise ValueError("repository must be one GitHub owner/name pair")
    if not TAG.fullmatch(tag):
        raise ValueError("tag must be v1.2.3 or v1.2.3-alpha.1")
    if not root.is_dir():
        raise ValueError(f"plugin folder does not exist: {root}")

    plugins = []
    ids = set()
    assets = {}
    for folder in sorted((path for path in root.iterdir() if path.is_dir()), key=lambda p: p.name):
        if folder.is_symlink():
            raise ValueError(f"plugin folder may not be a symlink: {folder}")
        meta = parse_effect_meta(folder)
        if meta["id"] in ids:
            raise ValueError(f"plugin {meta['id']} is listed twice")
        ids.add(meta["id"])
        files = []
        paths = sorted(
            (path for path in folder.rglob("*") if path.is_file() or path.is_symlink()),
            key=lambda path: path.relative_to(folder).as_posix(),
        )
        for path in paths:
            entry = file_entry(folder, path, repository, tag, meta)
            asset = entry["url"].rsplit("/", 1)[1]
            if asset in assets:
                raise ValueError(f"asset name collision: {asset} from {assets[asset]} and {path}")
            assets[asset] = path
            files.append(entry)
        plugins.append({**meta, "files": files})
    return {"schema": 1, "plugins": plugins}


def write_bundle(root: Path, repository: str, tag: str, out: Path) -> dict:
    """Write the catalog and copy each named asset beside it."""
    data = build_manifest(root, repository, tag)
    # Resolve sources before creating the output: it may be beside them.
    folders = {plugin["id"]: root / _folder_for_id(root, plugin["id"]) for plugin in data["plugins"]}
    out.parent.mkdir(parents=True, exist_ok=True)
    for plugin in data["plugins"]:
        folder = folders[plugin["id"]]
        for entry in plugin["files"]:
            asset = entry["url"].rsplit("/", 1)[1]
            shutil.copyfile(folder / entry["path"], out.parent / asset)
    out.write_text(json.dumps(data, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")
    return data


def _folder_for_id(root: Path, plugin_id: str) -> str:
    matches = [
        folder.name
        for folder in root.iterdir()
        if folder.is_dir() and parse_effect_meta(folder)["id"] == plugin_id
    ]
    if len(matches) != 1:
        raise ValueError(f"plugin {plugin_id} does not have exactly one source folder")
    return matches[0]


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repository", required=True)
    parser.add_argument("--tag", required=True)
    parser.add_argument("--plugins", type=Path, required=True)
    parser.add_argument("--out", type=Path, required=True)
    args = parser.parse_args()
    try:
        data = write_bundle(args.plugins, args.repository, args.tag, args.out)
    except ValueError as error:
        parser.error(str(error))
    print(f"Wrote {len(data['plugins'])} plugin(s) to {args.out}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
