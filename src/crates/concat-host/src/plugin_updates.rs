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

#[derive(Clone, Debug)]
struct PreparedPlugin {
    id: String,
    version: u32,
    format: u32,
    staged: std::path::PathBuf,
    target: std::path::PathBuf,
}

/// Verified packages staged away from every installed plugin directory.
#[derive(Debug)]
pub struct PreparedPlugins {
    items: Vec<PreparedPlugin>,
    workspace: std::path::PathBuf,
}

impl PreparedPlugins {
    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }
}

/// Downloads and verifies all candidates before changing any installation.
pub fn prepare_plugins(
    candidates: &[PluginStatus],
    dirs: &crate::AppDirs,
    cancel: &std::sync::atomic::AtomicBool,
    progress: &mut dyn FnMut(u64, u64),
) -> Result<PreparedPlugins, String> {
    let root = dirs.data.join("updates").join("plugins");
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let sequence = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let workspace = root.join(format!(
        "yumocut-{}-{sequence}",
        std::process::id()
    ));
    std::fs::create_dir_all(&workspace)
        .map_err(|error| format!("could not create {}: {error}", workspace.display()))?;
    let result = prepare_into(candidates, &workspace, cancel, progress);
    if result.is_err() {
        let _ = std::fs::remove_dir_all(&workspace);
    }
    result
}

fn prepare_into(
    candidates: &[PluginStatus],
    workspace: &std::path::Path,
    cancel: &std::sync::atomic::AtomicBool,
    progress: &mut dyn FnMut(u64, u64),
) -> Result<PreparedPlugins, String> {
    let total = candidates.iter().fold(0_u64, |sum, status| {
        sum + status.entry.files.iter().map(|file| file.bytes).sum::<u64>()
    });
    let mut completed = 0_u64;
    let mut items = Vec::with_capacity(candidates.len());
    for (index, status) in candidates.iter().enumerate() {
        let downloads = workspace.join("downloads").join(index.to_string());
        let staged = workspace.join("packages").join(index.to_string());
        std::fs::create_dir_all(&downloads)
            .and_then(|()| std::fs::create_dir_all(&staged))
            .map_err(|error| format!("could not stage {}: {error}", status.id))?;
        for (file_index, file) in status.entry.files.iter().enumerate() {
            let partial = downloads.join(format!("{file_index}.part"));
            let offset = completed;
            crate::models::download(
                &file.url,
                &partial,
                file.bytes,
                cancel,
                updates::CANCELLED,
                &mut |received, _| progress(offset + received, total),
            )?;
            let actual = std::fs::metadata(&partial)
                .map_err(|error| format!("could not measure {}: {error}", partial.display()))?
                .len();
            if actual != file.bytes {
                return Err(format!(
                    "{} for {} has {actual} bytes; the catalog declares {}",
                    file.path, status.id, file.bytes
                ));
            }
            crate::models::verify(&partial, &file.sha256)?;
            let destination = file
                .path
                .split('/')
                .fold(staged.clone(), |path, part| path.join(part));
            if let Some(parent) = destination.parent() {
                std::fs::create_dir_all(parent).map_err(|error| {
                    format!("could not create {}: {error}", parent.display())
                })?;
            }
            std::fs::rename(&partial, &destination).map_err(|error| {
                format!("could not stage {}: {error}", destination.display())
            })?;
            completed += file.bytes;
            progress(completed, total);
        }
        let package = concat_effects::Package::from_folder(&staged)
            .map_err(|error| format!("the updated {} package is invalid: {error}", status.id))?;
        if package.manifest.effect.id != status.entry.id
            || package.manifest.effect.version != status.entry.version
            || package.manifest.format != status.entry.format
        {
            return Err(format!(
                "the downloaded package for {} does not match its catalog identity",
                status.id
            ));
        }
        items.push(PreparedPlugin {
            id: status.id.clone(),
            version: status.entry.version,
            format: status.entry.format,
            staged,
            target: status.target.clone(),
        });
    }
    Ok(PreparedPlugins {
        items,
        workspace: workspace.to_path_buf(),
    })
}

#[derive(Debug)]
struct AppliedPlugin {
    id: String,
    target: std::path::PathBuf,
    old: std::path::PathBuf,
    new: std::path::PathBuf,
}

/// A replaced group whose old packages remain available until `finish`.
#[derive(Debug)]
pub struct AppliedPlugins {
    records: Vec<AppliedPlugin>,
    transaction: std::path::PathBuf,
    workspace: std::path::PathBuf,
    armed: bool,
}

impl PreparedPlugins {
    /// Replaces the whole group, restoring every old package on any error.
    pub fn apply(self) -> Result<AppliedPlugins, String> {
        let effects = self
            .items
            .first()
            .and_then(|item| item.target.parent())
            .ok_or_else(|| "there are no plugin updates to apply".to_owned())?;
        if self
            .items
            .iter()
            .any(|item| item.target.parent() != Some(effects))
        {
            return Err("plugin update targets do not share one effects folder".to_owned());
        }
        let transaction = effects.join(format!(".yumocut-update-{}", std::process::id()));
        apply_into(self.items, transaction, self.workspace)
    }
}

impl AppliedPlugins {
    /// IDs whose replacement can make this reload fail.
    pub fn ids(&self) -> HashSet<String> {
        self.records.iter().map(|record| record.id.clone()).collect()
    }

    /// Whether a reload error belongs to this updated group.
    pub fn involves(&self, error: &concat_effects::Error) -> bool {
        match error {
            concat_effects::Error::Invalid { id, .. } => {
                self.records.iter().any(|record| &record.id == id)
            }
            concat_effects::Error::Io { path, .. } => self
                .records
                .iter()
                .any(|record| path.starts_with(&record.target)),
        }
    }

    /// Keeps the new packages and removes their backups and staging files.
    pub fn finish(mut self) {
        self.armed = false;
        for record in &self.records {
            let _ = std::fs::remove_dir_all(&record.old);
        }
        let _ = std::fs::remove_dir_all(&self.workspace);
        let _ = std::fs::remove_dir(&self.transaction);
    }

    /// Restores all old packages in reverse replacement order.
    pub fn rollback(mut self) -> Result<(), String> {
        self.armed = false;
        let result = recover(&self.records);
        let _ = std::fs::remove_dir_all(&self.workspace);
        let _ = std::fs::remove_dir(&self.transaction);
        result
    }
}

impl Drop for AppliedPlugins {
    fn drop(&mut self) {
        if self.armed {
            let _ = recover(&self.records);
            let _ = std::fs::remove_dir_all(&self.workspace);
            let _ = std::fs::remove_dir(&self.transaction);
        }
    }
}

fn copy_tree(from: &std::path::Path, to: &std::path::Path) -> Result<(), String> {
    std::fs::create_dir(to)
        .map_err(|error| format!("could not create {}: {error}", to.display()))?;
    for entry in std::fs::read_dir(from)
        .map_err(|error| format!("could not read {}: {error}", from.display()))?
    {
        let entry = entry.map_err(|error| format!("could not read a staged file: {error}"))?;
        let source = entry.path();
        let destination = to.join(entry.file_name());
        let kind = entry
            .file_type()
            .map_err(|error| format!("could not inspect {}: {error}", source.display()))?;
        if kind.is_dir() {
            copy_tree(&source, &destination)?;
        } else if kind.is_file() {
            std::fs::copy(&source, &destination).map_err(|error| {
                format!("could not copy {}: {error}", source.display())
            })?;
        } else {
            return Err(format!("{} is not a regular package file", source.display()));
        }
    }
    Ok(())
}

fn apply_into(
    items: Vec<PreparedPlugin>,
    transaction: std::path::PathBuf,
    workspace: std::path::PathBuf,
) -> Result<AppliedPlugins, String> {
    std::fs::create_dir_all(&transaction).map_err(|error| {
        format!("could not create {}: {error}", transaction.display())
    })?;
    let mut records: Vec<AppliedPlugin> = Vec::with_capacity(items.len());
    for (index, item) in items.into_iter().enumerate() {
        let attempt = || {
            let new = transaction.join(format!("new-{index}"));
            let old = transaction.join(format!("old-{index}"));
            copy_tree(&item.staged, &new)?;
            std::fs::rename(&item.target, &old).map_err(|error| {
                format!("could not move {} aside: {error}", item.target.display())
            })?;
            records.push(AppliedPlugin {
                id: item.id,
                target: item.target,
                old,
                new,
            });
            let record = records.last().expect("just pushed");
            std::fs::rename(&record.new, &record.target).map_err(|error| {
                format!("could not install {}: {error}", record.target.display())
            })?;
            Ok(())
        };
        if let Err(error) = attempt() {
            let recovery = recover(&records);
            let _ = std::fs::remove_dir_all(transaction.join(format!("new-{index}")));
            let _ = std::fs::remove_dir_all(&workspace);
            let _ = std::fs::remove_dir(&transaction);
            return Err(match recovery {
                Ok(()) => format!("{error}; installed plugins were restored"),
                Err(recovery) => format!("{error}; rollback also failed: {recovery}"),
            });
        }
    }
    Ok(AppliedPlugins {
        records,
        transaction,
        workspace,
        armed: true,
    })
}

fn recover(records: &[AppliedPlugin]) -> Result<(), String> {
    let mut errors = Vec::new();
    for record in records.iter().rev() {
        if record.target.exists()
            && let Err(error) = std::fs::remove_dir_all(&record.target)
        {
            errors.push(format!("could not remove {}: {error}", record.target.display()));
            continue;
        }
        if let Err(error) = std::fs::rename(&record.old, &record.target) {
            errors.push(format!("could not restore {}: {error}", record.target.display()));
        }
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors.join("; "))
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

    fn audio_source(id: &str, version: u32) -> Vec<u8> {
        format!("[effect]\nid = {id:?}\nname = \"Test\"\nkind = \"audio\"\nversion = {version}\n[ffmpeg]\nchain = \"volume=1\"\n").into_bytes()
    }

    fn digest(bytes: &[u8]) -> String {
        use sha2::{Digest, Sha256};
        Sha256::digest(bytes).iter().map(|byte| format!("{byte:02x}")).collect()
    }

    fn static_server(
        files: Vec<(&str, Vec<u8>)>,
    ) -> (String, std::thread::JoinHandle<()>) {
        use std::io::{Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("listen");
        let base = format!("http://{}", listener.local_addr().expect("address"));
        let handle = std::thread::spawn(move || {
            for (path, body) in files {
                let (mut stream, _) = listener.accept().expect("request");
                let mut request = [0_u8; 2048];
                let _ = stream.read(&mut request).expect("read request");
                assert!(request.starts_with(format!("GET {path} ").as_bytes()));
                let head = format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", body.len());
                stream.write_all(head.as_bytes()).expect("headers");
                stream.write_all(&body).expect("body");
            }
        });
        (base, handle)
    }

    fn download_candidate(
        id: &str,
        target: std::path::PathBuf,
        base: &str,
        files: Vec<(&str, Vec<u8>)>,
        wrong_digest: bool,
    ) -> PluginStatus {
        PluginStatus {
            id: id.to_owned(),
            installed: 1,
            available: 2,
            state: PluginState::Update,
            entry: PluginEntry {
                id: id.to_owned(),
                version: 2,
                format: 1,
                min_app: Version::new(0, 2, 11),
                files: files.into_iter().map(|(path, body)| PluginFile {
                    path: path.to_owned(),
                    url: format!("{base}/{path}"),
                    bytes: body.len() as u64,
                    sha256: if wrong_digest { "0".repeat(64) } else { digest(&body) },
                }).collect(),
            },
            target,
        }
    }

    #[test]
    fn a_bad_digest_or_size_leaves_the_installed_folder_untouched() {
        let temp = tempfile::tempdir().expect("temp");
        let installed = temp.path().join("effects/yumo.volume");
        write_audio_package(&installed, "yumo.volume", 1);
        let before = std::fs::read_to_string(installed.join("effect.toml")).expect("old");
        let body = audio_source("yumo.volume", 2);
        let (base, server) = static_server(vec![("effect.toml", body.clone())]);
        let candidate = download_candidate("yumo.volume", installed.clone(), &base, vec![("effect.toml", body)], true);
        let result = prepare_plugins(&[candidate], &crate::AppDirs::under(temp.path()), &std::sync::atomic::AtomicBool::new(false), &mut |_, _| {});
        server.join().expect("server");
        assert!(result.is_err());
        assert_eq!(std::fs::read_to_string(installed.join("effect.toml")).expect("old"), before);
    }

    #[test]
    fn a_truncated_response_is_rejected_before_replacement() {
        use std::io::{Read, Write};
        let temp = tempfile::tempdir().expect("temp");
        let installed = temp.path().join("effects/yumo.volume");
        write_audio_package(&installed, "yumo.volume", 1);
        let before = std::fs::read_to_string(installed.join("effect.toml")).expect("old");
        let body = audio_source("yumo.volume", 2);
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("listen");
        let base = format!("http://{}", listener.local_addr().expect("address"));
        let served = body.clone();
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("request");
            let mut request = [0_u8; 2048];
            let _ = stream.read(&mut request).expect("read request");
            let head = format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", served.len());
            stream.write_all(head.as_bytes()).expect("headers");
            stream.write_all(&served[..served.len() - 1]).expect("truncated body");
        });
        let candidate = download_candidate("yumo.volume", installed.clone(), &base, vec![("effect.toml", body)], false);
        let result = prepare_plugins(&[candidate], &crate::AppDirs::under(temp.path()), &std::sync::atomic::AtomicBool::new(false), &mut |_, _| {});
        server.join().expect("server");
        assert!(result.is_err());
        assert_eq!(std::fs::read_to_string(installed.join("effect.toml")).expect("old"), before);
    }

    #[test]
    fn every_verified_file_is_staged_and_semantically_checked() {
        let temp = tempfile::tempdir().expect("temp");
        let installed = temp.path().join("effects/yumo.volume");
        write_audio_package(&installed, "yumo.volume", 1);
        let manifest = audio_source("yumo.volume", 2);
        let fixtures = b"[[case]]\nname = \"default\"\nchain = \"volume=1\"\n".to_vec();
        let total = manifest.len() as u64 + fixtures.len() as u64;
        let (base, server) = static_server(vec![("effect.toml", manifest.clone()), ("fixtures.toml", fixtures.clone())]);
        let candidate = download_candidate("yumo.volume", installed.clone(), &base, vec![("effect.toml", manifest), ("fixtures.toml", fixtures)], false);
        let progress = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let seen = progress.clone();
        let prepared = prepare_plugins(&[candidate], &crate::AppDirs::under(temp.path()), &std::sync::atomic::AtomicBool::new(false), &mut move |received, total| seen.borrow_mut().push((received, total))).expect("prepared");
        server.join().expect("server");
        assert_eq!(prepared.len(), 1);
        assert!(!prepared.is_empty());
        assert_eq!(progress.borrow().last(), Some(&(total, total)));
        assert_eq!(installed_plugins(&installed.parent().expect("effects"))[0].version, 1);
        drop(prepared);
    }

    #[test]
    fn cancellation_and_a_package_identity_mismatch_do_not_touch_installations() {
        let temp = tempfile::tempdir().expect("temp");
        let installed = temp.path().join("effects/yumo.volume");
        write_audio_package(&installed, "yumo.volume", 1);
        let before = std::fs::read_to_string(installed.join("effect.toml")).expect("old");
        let body = audio_source("other.identity", 2);
        let (base, server) = static_server(vec![("effect.toml", body.clone())]);
        let candidate = download_candidate("yumo.volume", installed.clone(), &base, vec![("effect.toml", body)], false);
        let result = prepare_plugins(&[candidate], &crate::AppDirs::under(temp.path()), &std::sync::atomic::AtomicBool::new(false), &mut |_, _| {});
        server.join().expect("server");
        assert!(result.is_err());
        assert_eq!(std::fs::read_to_string(installed.join("effect.toml")).expect("old"), before);

        let body = audio_source("yumo.volume", 2);
        let (base, server) = static_server(vec![("effect.toml", body.clone())]);
        let candidate = download_candidate("yumo.volume", installed.clone(), &base, vec![("effect.toml", body)], false);
        let result = prepare_plugins(&[candidate], &crate::AppDirs::under(temp.path()), &std::sync::atomic::AtomicBool::new(true), &mut |_, _| {});
        server.join().expect("server");
        assert_eq!(result.expect_err("cancelled"), updates::CANCELLED);
        assert_eq!(std::fs::read_to_string(installed.join("effect.toml")).expect("old"), before);
    }

    fn prepared_audio_package(
        root: &std::path::Path,
        id: &str,
        old_version: u32,
        new_version: u32,
    ) -> PreparedPlugin {
        let target = root.join("effects").join(id);
        let staged = root.join("operation/packages").join(id);
        write_audio_package(&target, id, old_version);
        write_audio_package(&staged, id, new_version);
        PreparedPlugin {
            id: id.to_owned(),
            version: new_version,
            format: 1,
            staged,
            target,
        }
    }

    fn prepared_group(items: Vec<PreparedPlugin>) -> PreparedPlugins {
        let workspace = items[0]
            .staged
            .parent()
            .and_then(std::path::Path::parent)
            .expect("operation")
            .to_path_buf();
        PreparedPlugins { items, workspace }
    }

    fn installed_version(target: &std::path::Path) -> u32 {
        let source = std::fs::read_to_string(target.join("effect.toml")).expect("manifest");
        concat_effects::Manifest::parse(&source).expect("parse").effect.version
    }

    #[test]
    fn a_second_target_conflict_restores_the_first_old_package() {
        let temp = tempfile::tempdir().expect("temp");
        let first = prepared_audio_package(temp.path(), "yumo.one", 1, 2);
        let second = prepared_audio_package(temp.path(), "yumo.two", 1, 2);
        let transaction = temp.path().join("effects").join(format!(".yumocut-update-{}", std::process::id()));
        std::fs::create_dir_all(&transaction).expect("transaction");
        std::fs::write(transaction.join("old-1"), "keep").expect("conflict");
        let result = prepared_group(vec![first.clone(), second.clone()]).apply();
        assert!(result.is_err());
        assert_eq!(installed_version(&first.target), 1);
        assert_eq!(installed_version(&second.target), 1);
        assert_eq!(std::fs::read_to_string(transaction.join("old-1")).expect("kept"), "keep");
    }

    #[test]
    fn successful_groups_finish_and_failed_trials_can_roll_back() {
        let temp = tempfile::tempdir().expect("temp");
        let first = prepared_audio_package(temp.path(), "yumo.one", 1, 2);
        let second = prepared_audio_package(temp.path(), "yumo.two", 1, 2);
        let applied = prepared_group(vec![first.clone(), second.clone()]).apply().expect("applied");
        assert_eq!(installed_version(&first.target), 2);
        assert_eq!(installed_version(&second.target), 2);
        assert_eq!(applied.ids().len(), 2);
        assert!(applied.involves(&concat_effects::Error::Invalid { id: "yumo.one".to_owned(), message: "bad".to_owned() }));
        assert!(applied.involves(&concat_effects::Error::Io { path: second.target.join("effect.toml"), message: "bad".to_owned() }));
        assert!(!applied.involves(&concat_effects::Error::Invalid { id: "local.broken".to_owned(), message: "bad".to_owned() }));
        applied.finish();
        assert_eq!(installed_version(&first.target), 2);

        let applied = prepared_group(vec![
            prepared_audio_package(temp.path(), "yumo.one", 1, 2),
            prepared_audio_package(temp.path(), "yumo.two", 1, 2),
        ]).apply().expect("applied again");
        applied.rollback().expect("rollback");
        assert_eq!(installed_version(&first.target), 1);
        assert_eq!(installed_version(&second.target), 1);
    }

    #[test]
    fn rollback_leaves_an_unregistered_sibling_unchanged() {
        let temp = tempfile::tempdir().expect("temp");
        let first = prepared_audio_package(temp.path(), "yumo.one", 1, 2);
        let sibling = temp.path().join("effects/local.private");
        write_audio_package(&sibling, "local.private", 7);
        prepared_group(vec![first.clone()]).apply().expect("applied").rollback().expect("rollback");
        assert_eq!(installed_version(&first.target), 1);
        assert_eq!(installed_version(&sibling), 7);
    }
}
