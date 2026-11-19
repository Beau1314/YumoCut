// SPDX-License-Identifier: AGPL-3.0-or-later
// SPDX-FileCopyrightText: 2026 Jareer Abdullah and Concat contributors

//! Registered user-effect updates, from catalog validation to installation.
//!
//! Nothing in this module knows about a window. The release catalog is
//! untrusted input: paths, sizes, URLs and digests are checked before a
//! download starts, and the reconstructed package is parsed before it can
//! replace an installed one.

use std::collections::HashSet;

use serde::Deserialize;

use crate::updates::{self, Release, Version};

const SCHEMA: u32 = 1;
const MAX_FILES: usize = 64;
const MAX_BYTES: u64 = 32 * 1024 * 1024;

/// One file belonging to a plugin release.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PluginFile {
    pub path: String,
    pub url: String,
    pub bytes: u64,
    pub sha256: String,
}

/// A registered plugin in a YumoCut release catalog.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PluginEntry {
    pub id: String,
    pub version: u32,
    pub format: u32,
    pub min_app: Version,
    pub files: Vec<PluginFile>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawCatalog {
    schema: u32,
    plugins: Vec<RawPlugin>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawPlugin {
    id: String,
    version: u32,
    format: u32,
    min_app: String,
    files: Vec<RawFile>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawFile {
    path: String,
    url: String,
    bytes: u64,
    sha256: String,
}

/// Where a release's registered-plugin catalog lives.
pub fn catalog_url(tag: &str) -> String {
    format!(
        "https://github.com/{}/releases/download/{tag}/plugins.json",
        updates::REPO
    )
}

/// Fetches and validates the catalog attached to `release`.
pub fn fetch_catalog(release: &Release) -> Result<Vec<PluginEntry>, String> {
    catalog_from_json(&updates::get_text(&catalog_url(&release.tag))?)
}

/// Parses the strict, versioned plugin catalog attached to a release.
pub fn catalog_from_json(json: &str) -> Result<Vec<PluginEntry>, String> {
    let raw: RawCatalog = serde_json::from_str(json)
        .map_err(|error| format!("the plugin catalog does not read as JSON: {error}"))?;
    if raw.schema != SCHEMA {
        return Err(format!(
            "the plugin catalog uses schema {}, but this build reads schema {SCHEMA}",
            raw.schema
        ));
    }
    let mut ids = HashSet::new();
    let mut plugins = Vec::with_capacity(raw.plugins.len());
    for plugin in raw.plugins {
        if plugin.id.trim().is_empty() {
            return Err("a plugin catalog entry has an empty id".to_owned());
        }
        if !ids.insert(plugin.id.clone()) {
            return Err(format!("plugin {} is listed twice", plugin.id));
        }
        if plugin.version == 0 {
            return Err(format!("plugin {} has version 0", plugin.id));
        }
        if plugin.format == 0 {
            return Err(format!("plugin {} has package format 0", plugin.id));
        }
        let min_app = Version::parse(&plugin.min_app).ok_or_else(|| {
            format!(
                "plugin {} has an invalid minimum app version {:?}",
                plugin.id, plugin.min_app
            )
        })?;
        if plugin.files.is_empty() || plugin.files.len() > MAX_FILES {
            return Err(format!(
                "plugin {} has {} files; the limit is 1..={MAX_FILES}",
                plugin.id,
                plugin.files.len()
            ));
        }
        let mut paths = HashSet::new();
        let mut total = 0_u64;
        let mut files = Vec::with_capacity(plugin.files.len());
        for file in plugin.files {
            if !safe_relative_path(&file.path) {
                return Err(format!(
                    "plugin {} names unsafe path {:?}",
                    plugin.id, file.path
                ));
            }
            if !paths.insert(file.path.clone()) {
                return Err(format!(
                    "plugin {} lists {} twice",
                    plugin.id, file.path
                ));
            }
            if !valid_download_url(&file.url) {
                return Err(format!(
                    "plugin {} uses a non-HTTPS download URL for {}",
                    plugin.id, file.path
                ));
            }
            if file.sha256.len() != 64
                || !file
                    .sha256
                    .chars()
                    .all(|c| c.is_ascii_digit() || ('a'..='f').contains(&c))
            {
                return Err(format!(
                    "plugin {} has an invalid SHA-256 for {}",
                    plugin.id, file.path
                ));
            }
            total = total.checked_add(file.bytes).ok_or_else(|| {
                format!("plugin {} has an unrepresentable total size", plugin.id)
            })?;
            if total > MAX_BYTES {
                return Err(format!(
                    "plugin {} exceeds the {MAX_BYTES}-byte size limit",
                    plugin.id
                ));
            }
            files.push(PluginFile {
                path: file.path,
                url: file.url,
                bytes: file.bytes,
                sha256: file.sha256,
            });
        }
        if !paths.contains("effect.toml") {
            return Err(format!("plugin {} has no effect.toml", plugin.id));
        }
        plugins.push(PluginEntry {
            id: plugin.id,
            version: plugin.version,
            format: plugin.format,
            min_app,
            files,
        });
    }
    Ok(plugins)
}

fn safe_relative_path(path: &str) -> bool {
    !path.is_empty()
        && !path.starts_with('/')
        && !path.contains('\\')
        && !path.contains(':')
        && path
            .split('/')
            .all(|part| !part.is_empty() && part != "." && part != "..")
}

fn valid_download_url(url: &str) -> bool {
    if url.strip_prefix("https://").is_some_and(|rest| !rest.is_empty()) {
        return true;
    }
    #[cfg(test)]
    {
        return ["127.0.0.1", "localhost"].into_iter().any(|host| {
            url.strip_prefix(&format!("http://{host}"))
                .and_then(|rest| rest.chars().next())
                .is_some_and(|next| next == ':' || next == '/')
        });
    }
    #[cfg(not(test))]
    {
        false
    }
}

/// A readable installed user package, discovered without changing it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InstalledPlugin {
    pub id: String,
    pub version: u32,
    pub format: u32,
    pub folder: std::path::PathBuf,
}

/// Why a registered package can or cannot be updated by this build.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PluginState {
    Update,
    Current,
    NeedsApp(Version),
    NeedsFormat(u32),
}

/// The installed and available state of one registered package.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PluginStatus {
    pub id: String,
    pub installed: u32,
    pub available: u32,
    pub state: PluginState,
    pub entry: PluginEntry,
    pub target: std::path::PathBuf,
}

/// Everything a release offers this installation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UpdateOffer {
    pub release: Release,
    pub app: bool,
    pub plugins: Vec<PluginStatus>,
}

impl UpdateOffer {
    /// Only compatible, newer versions of packages already installed.
    pub fn candidates(&self) -> Vec<PluginStatus> {
        self.plugins
            .iter()
            .filter(|status| status.state == PluginState::Update)
            .cloned()
            .collect()
    }
}

/// Reads installed packages and skips malformed folders without deleting them.
pub fn installed_plugins(dir: &std::path::Path) -> Vec<InstalledPlugin> {
    let mut found = Vec::new();
    for folder in concat_effects::package_folders(dir).unwrap_or_default() {
        if let Ok(package) = concat_effects::Package::from_folder(&folder) {
            found.push(InstalledPlugin {
                id: package.manifest.effect.id.clone(),
                version: package.manifest.effect.version,
                format: package.manifest.format,
                folder,
            });
        }
    }
    found.sort_by(|a, b| a.id.cmp(&b.id));
    found
}

/// Matches this release's registered packages against the installed ones.
pub fn offer(
    release: Release,
    catalog: Vec<PluginEntry>,
    dir: &std::path::Path,
) -> UpdateOffer {
    let current = Version::current();
    let mut plugins = Vec::new();
    for installed in installed_plugins(dir) {
        let Some(entry) = catalog.iter().find(|entry| entry.id == installed.id) else {
            continue;
        };
        let state = if entry.min_app > current {
            PluginState::NeedsApp(entry.min_app)
        } else if entry.format > concat_effects::FORMAT {
            PluginState::NeedsFormat(entry.format)
        } else if entry.version > installed.version {
            PluginState::Update
        } else {
            PluginState::Current
        };
        plugins.push(PluginStatus {
            id: installed.id,
            installed: installed.version,
            available: entry.version,
            state,
            entry: entry.clone(),
            target: installed.folder,
        });
    }
    UpdateOffer {
        app: release.version > current,
        release,
        plugins,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const VALID: &str = r#"{"schema":1,"plugins":[{"id":"yumo.glow","version":2,"format":2,"min_app":"0.2.11","files":[{"path":"effect.toml","url":"https://github.com/Beau1314/YumoCut/releases/download/v0.2.12/yumo.glow-2-effect.toml","bytes":1,"sha256":"0000000000000000000000000000000000000000000000000000000000000000"}]}]}"#;

    #[test]
    fn a_catalog_rejects_ambiguous_or_unsafe_entries() {
        assert_eq!(catalog_from_json(VALID).expect("valid")[0].id, "yumo.glow");
        for path in ["", "/effect.toml", "../effect.toml", "a\\effect.toml", "./effect.toml", "C:/effect.toml"] {
            let bad = VALID.replace("\"path\":\"effect.toml\"", &format!("\"path\":\"{path}\""));
            assert!(catalog_from_json(&bad).is_err(), "{path}");
        }
        let object = VALID.split("\"plugins\":[").nth(1).expect("entry").split("]}").next().expect("object");
        let duplicate = format!("{{\"schema\":1,\"plugins\":[{object},{object}]}}");
        assert!(catalog_from_json(&duplicate).is_err());
    }

    #[test]
    fn a_catalog_rejects_bad_digests_duplicates_and_missing_manifests() {
        for digest in ["0", "A000000000000000000000000000000000000000000000000000000000000000", "g000000000000000000000000000000000000000000000000000000000000000"] {
            assert!(catalog_from_json(&VALID.replace(&"0".repeat(64), digest)).is_err(), "{digest}");
        }
        let duplicate_path = VALID.replace(
            "]}]}",
            ",{\"path\":\"effect.toml\",\"url\":\"https://example.test/other\",\"bytes\":1,\"sha256\":\"0000000000000000000000000000000000000000000000000000000000000000\"}]}]}",
        );
        assert!(catalog_from_json(&duplicate_path).is_err());
        assert!(catalog_from_json(&VALID.replace("\"path\":\"effect.toml\"", "\"path\":\"shader.wgsl\"")).is_err());
    }

    #[test]
    fn a_catalog_enforces_versions_schema_and_limits() {
        assert!(catalog_from_json(&VALID.replace("\"version\":2", "\"version\":0")).is_err());
        assert!(catalog_from_json(&VALID.replace("\"schema\":1", "\"schema\":2")).is_err());
        assert!(catalog_from_json(&VALID.replace("\"min_app\":\"0.2.11\"", "\"min_app\":\"next\"")).is_err());
        assert!(catalog_from_json(&VALID.replace("\"bytes\":1", &format!("\"bytes\":{}", 32 * 1024 * 1024 + 1))).is_err());

        let file = r#"{"path":"effect.toml","url":"https://example.test/effect","bytes":1,"sha256":"0000000000000000000000000000000000000000000000000000000000000000"}"#;
        let files = std::iter::repeat_n(file, 65).collect::<Vec<_>>().join(",");
        let many = format!("{{\"schema\":1,\"plugins\":[{{\"id\":\"yumo.glow\",\"version\":2,\"format\":2,\"min_app\":\"0.2.11\",\"files\":[{files}]}}]}}");
        assert!(catalog_from_json(&many).is_err());
    }

    #[test]
    fn catalog_urls_come_from_the_yumocut_release() {
        assert_eq!(catalog_url("v0.2.12"), "https://github.com/Beau1314/YumoCut/releases/download/v0.2.12/plugins.json");
    }

    fn entry(id: &str, version: u32, format: u32, min_app: &str) -> PluginEntry {
        PluginEntry {
            id: id.to_owned(),
            version,
            format,
            min_app: Version::parse(min_app).expect("version"),
            files: vec![PluginFile {
                path: "effect.toml".to_owned(),
                url: "https://example.test/effect.toml".to_owned(),
                bytes: 1,
                sha256: "0".repeat(64),
            }],
        }
    }

    fn release(version: &str) -> Release {
        Release {
            version: Version::parse(version).expect("version"),
            tag: format!("v{version}"),
            published: String::new(),
        }
    }

    fn write_audio_package(folder: &std::path::Path, id: &str, version: u32) {
        std::fs::create_dir_all(folder).expect("package folder");
        std::fs::write(
            folder.join("effect.toml"),
            format!("[effect]\nid = {id:?}\nname = \"Test\"\nkind = \"audio\"\nversion = {version}\n[ffmpeg]\nchain = \"volume=1\"\n"),
        )
        .expect("package manifest");
    }

    #[test]
    fn only_registered_newer_compatible_packages_are_candidates() {
        let temp = tempfile::tempdir().expect("temp");
        write_audio_package(&temp.path().join("yumo.volume"), "yumo.volume", 1);
        write_audio_package(&temp.path().join("local.private"), "local.private", 1);
        let entries = vec![entry("yumo.volume", 2, 2, "0.2.11"), entry("other.new", 2, 2, "0.2.11")];
        let result = offer(release("0.2.12"), entries, temp.path());
        assert!(result.app);
        let candidates = result.candidates();
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].id, "yumo.volume");
        assert_eq!(candidates[0].target, temp.path().join("yumo.volume"));
    }

    #[test]
    fn selection_explains_compatibility_and_never_downgrades() {
        let temp = tempfile::tempdir().expect("temp");
        write_audio_package(&temp.path().join("yumo.volume"), "yumo.volume", 2);
        let same = offer(release("0.2.11"), vec![entry("yumo.volume", 2, 2, "0.2.11")], temp.path());
        assert!(!same.app);
        assert!(same.candidates().is_empty());
        assert_eq!(same.plugins[0].state, PluginState::Current);

        let lower = offer(release("0.2.12"), vec![entry("yumo.volume", 1, 2, "0.2.11")], temp.path());
        assert!(lower.candidates().is_empty());
        assert_eq!(lower.plugins[0].state, PluginState::Current);

        let app = offer(release("0.2.12"), vec![entry("yumo.volume", 3, 2, "0.2.13")], temp.path());
        assert_eq!(app.plugins[0].state, PluginState::NeedsApp(Version::new(0, 2, 13)));
        assert!(app.candidates().is_empty());

        let format = offer(release("0.2.12"), vec![entry("yumo.volume", 3, concat_effects::FORMAT + 1, "0.2.11")], temp.path());
        assert_eq!(format.plugins[0].state, PluginState::NeedsFormat(concat_effects::FORMAT + 1));
        assert!(format.candidates().is_empty());
    }

    #[test]
    fn malformed_installed_packages_are_left_alone() {
        let temp = tempfile::tempdir().expect("temp");
        let broken = temp.path().join("broken.plugin");
        std::fs::create_dir_all(&broken).expect("folder");
        std::fs::write(broken.join("effect.toml"), "not a manifest").expect("manifest");
        assert!(installed_plugins(temp.path()).is_empty());
        assert_eq!(std::fs::read_to_string(broken.join("effect.toml")).expect("untouched"), "not a manifest");
    }
}
