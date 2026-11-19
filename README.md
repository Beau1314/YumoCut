<p align="center">
  <img src="assets/icons/yumocut.svg" alt="YumoCut" width="120" />
</p>

<h1 align="center">YumoCut</h1>
<h3 align="center">A local, open-source video editor for Windows.</h3>

<p align="center">
  <a href="https://github.com/Beau1314/YumoCut/actions/workflows/ci.yml"><img src="https://img.shields.io/github/actions/workflow/status/Beau1314/YumoCut/ci.yml?style=flat&logo=githubactions&logoColor=F8F8F8&label=Build&labelColor=000000" alt="Build status" /></a>
  <a href="https://github.com/Beau1314/YumoCut/releases"><img src="https://img.shields.io/github/downloads/Beau1314/YumoCut/total?style=flat&logo=github&logoColor=F8F8F8&label=Downloads&labelColor=212123&color=0568FD" alt="Total downloads" /></a>
  <a href="LICENSE"><img src="https://img.shields.io/badge/License-AGPL%20v3-0568FD?style=flat&logo=gnu&logoColor=F8F8F8&labelColor=212123" alt="License: AGPL-3.0-or-later" /></a>
</p>

[简体中文](docs/README.zh-Hans.md)

## About this fork

**YumoCut is an unofficial, modified version of Concat.** It is based on
[Concat v0.2.11](https://github.com/jub0t/Concat) and is not maintained or
endorsed by the upstream project. YumoCut has its own name, icon, update
channel, and Windows installers.

See [FORK_NOTICES.md](FORK_NOTICES.md) for the exact upstream baseline,
corresponding source, modification summary, and distribution notices.

## What it does

YumoCut retains Concat's local editing engine: multi-track cutting, keyframes,
effects and titles, auto-captions, text-to-speech, background removal, and
video export. Footage is processed on your machine. Optional AI models are
downloaded on demand and can be used offline afterwards.

This fork adds startup update notices and a combined update action for the
application and registered user plugin packages. Downloads are checked
against their published SHA-256 digests. Registered plugins are replaced as a
group and restored if the new group cannot be loaded or tested; unregistered
plugin folders are left alone. Built-in effects travel with the application.

## Download and support

**[GitHub Releases](https://github.com/Beau1314/YumoCut/releases)** carries the
Windows x86_64 setup and MSI installers, update manifests, registered plugin
assets, and checksums. Releases are created as drafts for inspection before
publication. This fork currently promises Windows x86_64 only; the upstream
project's other platform packages are not YumoCut releases.

YumoCut is beta software. Report YumoCut defects at
**[Beau1314/YumoCut Issues](https://github.com/Beau1314/YumoCut/issues)**, not
in the upstream Concat tracker. Upstream Concat support remains at
[jub0t/Concat](https://github.com/jub0t/Concat).

Unsigned Windows builds can trigger SmartScreen, and Smart App Control may
refuse them. The release notes state whether a particular build is signed.

## Build from source

You need Rust 1.93 or newer (see `rust-version` in `src/Cargo.toml`), FFmpeg
7+ development libraries, cmake, and a C++ compiler. On Windows, point
`FFMPEG_DIR` at an LGPL shared FFmpeg build with headers and import libraries.

```sh
cd src
cargo run -p concat
```

Other useful commands:

```sh
cargo build --profile app -p concat
cargo test
cargo fmt --check
cargo clippy --all-targets -- -D warnings
```

Run `python3 scripts/locales.py --check` after changing interface text, and
`python3 -m unittest scripts.test_plugin_updates -v` after changing release
or registered-plugin tooling. The internal crate names, executable name, and
project/effect formats deliberately retain `concat` for compatibility.

## Licence and notices

YumoCut remains free software under [AGPL-3.0-or-later](LICENSE). Distributing
it, or offering a modified version for network use, carries the licence's
corresponding-source obligations. YumoCut's source is published at
[Beau1314/YumoCut](https://github.com/Beau1314/YumoCut).

Upstream copyright and licence notices are preserved. Third-party software,
fonts, and models retain their own licences; see
[THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md). No additional codec patent
licence is granted. The Concat name and logo are not granted by the AGPL; see
[TRADEMARK.md](TRADEMARK.md) and [LICENSING.md](LICENSING.md).
