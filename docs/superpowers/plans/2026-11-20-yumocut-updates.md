# YumoCut Updates Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the independently branded YumoCut fork with startup update notices, atomic app/plugin updates, a Windows x86_64 release path, and complete upstream licence notices.

**Architecture:** Keep release parsing and installer execution in `concat-host::updates`; add a UI-independent `concat-host::plugin_updates` module for manifest validation, candidate selection, verified staging, and reversible directory replacement. The Slint window orchestrates background checks and GPU trials, while GitHub Actions builds and publishes the matching source, installers, manifests, and checksums.

**Tech Stack:** Rust 1.93+, serde/serde_json, existing `ureq`/`sha2` download stack, `concat-effects` package parser, Slint, Python 3 release tooling, Inno Setup/WiX, GitHub Actions.

## Global Constraints

- Product name is `YumoCut`; the only application update repository is `Beau1314/YumoCut`.
- Preserve internal `concat` crate names, executable name, project/effect formats, and `AppDirs::IDENTIFIER` to avoid an unrelated settings migration.
- Preserve upstream copyright and licence files; modified builds are unofficial and use a new icon and new Windows installer identities.
- Only stable numeric releases are offered. Startup errors are logged, while manual-check errors are visible.
- Only registered, installed, compatible, higher-version user packages are candidates; unregistered packages and built-ins are never replaced by the plugin updater.
- Every candidate download is size- and SHA-256-verified before replacement. Any failed updated plugin rolls back the whole plugin group and prevents app installation.
- Plugin manifest schema is exactly `1`; package format is at most `concat_effects::FORMAT`; at most 64 files and 32 MiB per plugin; every package contains `effect.toml`.
- Production downloads require HTTPS, except loopback URLs used by tests. A digest supplies integrity, not protection against repository compromise.
- The cancellation flag applies only to downloads, never to replacement or rollback.
- Windows x86_64 is the only end-to-end release target promised in this phase.
- This machine has no Rust or CMake executable on `PATH`. Rust red/green results must come from real CI logs; local Python and static checks still run locally. Never record an unrun check as passing.

---

### Task 1: Brand and centralise the update repository

**Files:**
- Modify: `src/crates/concat-host/src/updates.rs:36-45,174-177,711-754,911-915`
- Modify: `src/crates/concat/build.rs:106-125`
- Modify: `src/crates/concat/locales/en.json`
- Modify: `assets/windows/concat.iss`
- Modify: `assets/windows/concat.wxs`
- Create: `assets/icons/yumocut.svg`
- Create: `assets/icons/yumocut.ico`
- Test: `src/crates/concat-host/src/updates.rs` unit tests

**Interfaces:**
- Produces: `pub const REPO: &str = "Beau1314/YumoCut"`.
- Produces: `pub fn releases_url() -> String` and existing `manifest_url(tag: &str) -> String`, both derived from `REPO`.
- Produces: `pub(crate) fn get_text(url: &str) -> Result<String, String>` for plugin manifest downloads.

- [ ] **Step 1: Write failing repository and brand assertions**

```rust
#[test]
fn yumocut_is_the_single_release_source() {
    assert_eq!(REPO, "Beau1314/YumoCut");
    assert_eq!(
        releases_url(),
        "https://api.github.com/repos/Beau1314/YumoCut/releases?per_page=100"
    );
    assert_eq!(
        manifest_url("v0.2.12"),
        "https://github.com/Beau1314/YumoCut/releases/download/v0.2.12/manifest.json"
    );
}
```

Add a Python static assertion that both installer files contain `YumoCut`, neither contains the upstream AppId/UpgradeCode, and `build.rs` names `yumocut.ico`.

- [ ] **Step 2: Run the tests and confirm the old configuration fails**

Run: `cd src && cargo test -p concat-host yumocut_is_the_single_release_source`
Expected: FAIL because `REPO` and the URLs still name `jub0t/Concat`; if the local toolchain is absent, record `cargo: command not found` and run the same test in CI before accepting the task.

- [ ] **Step 3: Implement the single source and visible brand**

```rust
pub const REPO: &str = "Beau1314/YumoCut";
const RELEASES_PATH: &str = "/releases?per_page=100";

pub fn releases_url() -> String {
    format!("https://api.github.com/repos/{REPO}{RELEASES_PATH}")
}

pub fn fetch_releases() -> Result<Vec<Release>, String> {
    releases_from_json(&get_text(&releases_url())?)
}
```

Change `get_text` to `pub(crate)` and its agent to `YumoCut/{version}`. Set the Windows ProductName/FileDescription, installer AppName/AppVerName/paths/URLs/output names, and locale-facing values to YumoCut. Generate fixed, distinct GUIDs for the new AppId, UpgradeCode, and StartMenu component; do not change `AppDirs::IDENTIFIER` or package-manager database names in this task.

Create an original simple `YC` scissors-free vector mark, export a multi-size ICO, and reference it from Windows resources/installers. Keep `concat.exe` as the internal executable filename.

- [ ] **Step 4: Run focused checks**

Run: `cd src && cargo test -p concat-host updates::tests && cargo fmt --check`
Run: `python3 scripts/locales.py --check`
Expected: repository tests and localisation inventory pass; icon/ICO files exist and installer static assertions pass.

- [ ] **Step 5: Commit the independently branded configuration**

```bash
git add src/crates/concat-host/src/updates.rs src/crates/concat/build.rs src/crates/concat/locales/en.json assets/windows assets/icons
git commit -m "feat: brand YumoCut and configure its releases"
```

### Task 2: Parse and validate the plugin release manifest

**Files:**
- Create: `src/crates/concat-host/src/plugin_updates.rs`
- Modify: `src/crates/concat-host/src/lib.rs`
- Test: `src/crates/concat-host/src/plugin_updates.rs` unit tests

**Interfaces:**
- Produces: `PluginEntry { id: String, version: u32, format: u32, min_app: Version, files: Vec<PluginFile> }`.
- Produces: `PluginFile { path: String, url: String, bytes: u64, sha256: String }`.
- Produces: `pub fn catalog_from_json(json: &str) -> Result<Vec<PluginEntry>, String>`.
- Produces: `pub fn catalog_url(tag: &str) -> String`, derived from `updates::REPO`.
- Produces: `pub fn fetch_catalog(release: &Release) -> Result<Vec<PluginEntry>, String>`.

- [ ] **Step 1: Write failing manifest tests**

```rust
#[test]
fn a_catalog_rejects_ambiguous_or_unsafe_entries() {
    let valid = r#"{"schema":1,"plugins":[{"id":"yumo.glow","version":2,"format":2,"min_app":"0.2.11","files":[{"path":"effect.toml","url":"https://github.com/Beau1314/YumoCut/releases/download/v0.2.12/yumo.glow-2-effect.toml","bytes":1,"sha256":"0000000000000000000000000000000000000000000000000000000000000000"}]}]}"#;
    assert_eq!(catalog_from_json(valid).expect("valid")[0].id, "yumo.glow");
    for path in ["", "/effect.toml", "../effect.toml", "a\\effect.toml", "./effect.toml", "C:/effect.toml"] {
        let bad = valid.replace("\"path\":\"effect.toml\"", &format!("\"path\":\"{path}\""));
        assert!(catalog_from_json(&bad).is_err(), "{path}");
    }
    let object = valid.split("\"plugins\":[").nth(1).expect("entry").split("]}").next().expect("object");
    let duplicate = format!("{{\"schema\":1,\"plugins\":[{object},{object}]}}");
    assert!(catalog_from_json(&duplicate).is_err());
}
```

Add separate concrete cases that duplicate an ID, duplicate a path, use uppercase or short SHA-256, omit `effect.toml`, use version `0`, use schema `2`, exceed 64 files, and exceed 32 MiB; each must return `Err`.

- [ ] **Step 2: Run manifest tests and confirm failure**

Run: `cd src && cargo test -p concat-host plugin_updates::tests`
Expected: FAIL because the module and parser do not exist; CI supplies the authoritative result when local Rust is unavailable.

- [ ] **Step 3: Implement strict parsing and path limits**

```rust
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PluginFile {
    pub path: String,
    pub url: String,
    pub bytes: u64,
    pub sha256: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PluginEntry {
    pub id: String,
    pub version: u32,
    pub format: u32,
    pub min_app: Version,
    pub files: Vec<PluginFile>,
}

pub fn catalog_url(tag: &str) -> String {
    format!("https://github.com/{}/releases/download/{tag}/plugins.json", updates::REPO)
}
```

Deserialize into private serde structs, then validate into the public types. Require lowercase 64-character hex digests, positive versions, parseable `min_app`, unique IDs/paths, `effect.toml`, safe slash-separated relative paths, and the exact file/byte limits. Validate URL shape with `https://`, or `http://127.0.0.1`/`http://localhost` only under `cfg(test)`. Export the module from `concat-host/src/lib.rs` with `pub mod plugin_updates;`.

- [ ] **Step 4: Run all parser cases**

Run: `cd src && cargo test -p concat-host plugin_updates::tests`
Expected: every valid/invalid manifest case passes.

- [ ] **Step 5: Commit the manifest contract**

```bash
git add src/crates/concat-host/src/lib.rs src/crates/concat-host/src/plugin_updates.rs
git commit -m "feat: validate plugin update catalogs"
```

### Task 3: Discover installed packages and select compatible candidates

**Files:**
- Modify: `src/crates/concat-host/src/plugin_updates.rs`
- Test: `src/crates/concat-host/src/plugin_updates.rs` unit tests

**Interfaces:**
- Produces: `InstalledPlugin { id: String, version: u32, format: u32, folder: PathBuf }`.
- Produces: `PluginState::{Update, Current, NeedsApp(Version), NeedsFormat(u32)}`.
- Produces: `PluginStatus { id, installed, available, state, entry, target }` with concrete public field types matching `PluginEntry` and `PathBuf`.
- Produces: `UpdateOffer { release: Release, app: bool, plugins: Vec<PluginStatus> }`.
- Produces: `pub fn installed_plugins(dir: &Path) -> Vec<InstalledPlugin>`.
- Produces: `pub fn offer(release: Release, catalog: Vec<PluginEntry>, dir: &Path) -> UpdateOffer`.
- Produces: `UpdateOffer::candidates(&self) -> Vec<PluginStatus>` filtered to `PluginState::Update`.

- [ ] **Step 1: Write failing discovery and selection tests**

```rust
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
```

Add cases proving same/lower versions are not updates, `min_app` above current and `format` above `FORMAT` expose their specific states, malformed installed folders are skipped without deleting anything, and a release equal to current sets `app == false`.

- [ ] **Step 2: Run selection tests and confirm failure**

Run: `cd src && cargo test -p concat-host only_registered_newer_compatible_packages_are_candidates`
Expected: FAIL on missing discovery/offer types.

- [ ] **Step 3: Implement read-only discovery and pure selection**

```rust
pub fn installed_plugins(dir: &Path) -> Vec<InstalledPlugin> {
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
```

Implement `offer` without filesystem writes. Match catalog IDs to discovered IDs; set `NeedsApp` before `NeedsFormat` before `Update`/`Current`, and keep status for incompatible registered packages visible. `candidates` clones only `Update` rows.

- [ ] **Step 4: Run discovery and selection tests**

Run: `cd src && cargo test -p concat-host plugin_updates::tests`
Expected: all selection and manifest tests pass.

- [ ] **Step 5: Commit candidate discovery**

```bash
git add src/crates/concat-host/src/plugin_updates.rs
git commit -m "feat: select compatible plugin updates"
```

### Task 4: Download and semantically verify every candidate before replacement

**Files:**
- Modify: `src/crates/concat-host/src/plugin_updates.rs`
- Test: `src/crates/concat-host/src/plugin_updates.rs` unit tests with a loopback static server

**Interfaces:**
- Produces: `PreparedPlugins { items: Vec<PreparedPlugin>, workspace: PathBuf }` with private fields.
- Produces: `pub fn prepare_plugins(candidates: &[PluginStatus], dirs: &AppDirs, cancel: &AtomicBool, progress: &mut dyn FnMut(u64, u64)) -> Result<PreparedPlugins, String>`.
- Produces: `PreparedPlugins::len(&self) -> usize` and `PreparedPlugins::is_empty(&self) -> bool`.

- [ ] **Step 1: Write failing download integrity tests**

```rust
#[test]
fn a_bad_digest_or_size_leaves_the_installed_folder_untouched() {
    let temp = tempfile::tempdir().expect("temp");
    let installed = temp.path().join("effects/yumo.volume");
    write_audio_package(&installed, "yumo.volume", 1);
    let before = std::fs::read_to_string(installed.join("effect.toml")).expect("old");
    let candidate = status_for_download("yumo.volume", 2, installed.clone(), "wrong");
    let result = prepare_plugins(&[candidate], &AppDirs::under(temp.path()), &AtomicBool::new(false), &mut |_, _| {});
    assert!(result.is_err());
    assert_eq!(std::fs::read_to_string(installed.join("effect.toml")).expect("old"), before);
}
```

Add loopback-server cases for a complete two-file package, a truncated response, and cancellation. Assert total progress uses declared bytes and no installed path changes on failure.

- [ ] **Step 2: Run download tests and confirm failure**

Run: `cd src && cargo test -p concat-host plugin_updates::tests::a_bad_digest_or_size_leaves_the_installed_folder_untouched`
Expected: FAIL on the missing preparation API.

- [ ] **Step 3: Implement verified staging**

```rust
pub fn prepare_plugins(
    candidates: &[PluginStatus],
    dirs: &AppDirs,
    cancel: &AtomicBool,
    progress: &mut dyn FnMut(u64, u64),
) -> Result<PreparedPlugins, String> {
    let workspace = dirs.data.join("updates").join("plugins");
    std::fs::create_dir_all(&workspace).map_err(|e| format!("could not create {}: {e}", workspace.display()))?;
    prepare_into(candidates, workspace, cancel, progress)
}
```

For each plugin create a unique download and reconstructed package directory. Download via `models::download`, compare `metadata().len()` with `PluginFile.bytes`, call `models::verify`, create only validated relative parents, and rename the verified file into the staged package. After all files arrive call `Package::from_folder`; require exact ID, version, and format equality with the catalog. Any error removes only this operation's staging paths.

- [ ] **Step 4: Run integrity and parser suites together**

Run: `cd src && cargo test -p concat-host plugin_updates::tests`
Expected: valid packages stage, all bad downloads fail, installed files remain byte-identical.

- [ ] **Step 5: Commit verified preparation**

```bash
git add src/crates/concat-host/src/plugin_updates.rs
git commit -m "feat: stage verified plugin updates"
```

### Task 5: Apply plugin updates as a reversible group

**Files:**
- Modify: `src/crates/concat-host/src/plugin_updates.rs`
- Test: `src/crates/concat-host/src/plugin_updates.rs` filesystem transaction tests

**Interfaces:**
- Consumes: `PreparedPlugins` from Task 4 and `PluginStatus.target` from Task 3.
- Produces: `AppliedPlugins` with private transaction records.
- Produces: `PreparedPlugins::apply(self) -> Result<AppliedPlugins, String>`.
- Produces: `AppliedPlugins::ids(&self) -> HashSet<String>`.
- Produces: `AppliedPlugins::involves(&self, error: &concat_effects::Error) -> bool`.
- Produces: `AppliedPlugins::rollback(self) -> Result<(), String>` and `AppliedPlugins::finish(self)`.

- [ ] **Step 1: Write failing atomic replacement and rollback tests**

```rust
#[test]
fn a_second_target_conflict_restores_the_first_old_package() {
    let temp = tempfile::tempdir().expect("temp");
    let first = prepared_audio_package(temp.path(), "yumo.one", 1, 2);
    let second = prepared_audio_package(temp.path(), "yumo.two", 1, 2);
    std::fs::create_dir(second.target.join("blocked")).expect("conflict parent");
    std::fs::write(second.target.join("blocked/file"), "keep").expect("conflict");
    let result = prepared_group(vec![first.clone(), second.clone()]).apply();
    assert!(result.is_err());
    assert_eq!(installed_version(&first.target), 1);
    assert_eq!(installed_version(&second.target), 1);
    assert_eq!(std::fs::read_to_string(second.target.join("blocked/file")).expect("kept"), "keep");
}
```

Also test successful two-package replacement, `involves` matching only updated IDs or IO paths beneath their targets, `finish` retaining new versions, and explicit rollback restoring old versions while leaving an unregistered sibling unchanged.

- [ ] **Step 2: Run transaction tests and confirm failure**

Run: `cd src && cargo test -p concat-host a_second_target_conflict_restores_the_first_old_package`
Expected: FAIL on missing `AppliedPlugins` and `apply`.

- [ ] **Step 3: Implement same-volume replacement and reverse-order recovery**

```rust
impl PreparedPlugins {
    pub fn apply(self) -> Result<AppliedPlugins, String> {
        let effects = self.items.first().and_then(|item| item.target.parent())
            .ok_or_else(|| "there are no plugin updates to apply".to_owned())?;
        let transaction = effects.join(format!(".yumocut-update-{}", std::process::id()));
        apply_into(self.items, transaction)
    }
}
```

Create the transaction beside the effects folder. Copy each verified staged folder into `transaction/new-N`, rename each existing target to `transaction/old-N`, then rename new-N to the target. On any failure, remove installed new folders and rename old folders back in reverse order. `rollback` performs the same recovery and reload-safe cleanup; `finish` disarms recovery and deletes only transaction/backup files. `Drop` calls rollback while armed.

- [ ] **Step 4: Run the full transaction suite**

Run: `cd src && cargo test -p concat-host plugin_updates::tests`
Expected: success persists, induced failure restores both old packages, unrelated packages and files remain unchanged.

- [ ] **Step 5: Commit the reversible transaction**

```bash
git add src/crates/concat-host/src/plugin_updates.rs
git commit -m "feat: make plugin updates atomic"
```

### Task 6: Add startup notices and the one-click window workflow

**Files:**
- Modify: `src/crates/concat/src/panes/settings.rs`
- Modify: `src/crates/concat/src/studio.rs:2823-2842`
- Modify: `src/crates/concat/src/studio/edit.rs:73-166`
- Modify: `src/crates/concat/src/studio/speech.rs:81-155`
- Modify: `src/crates/concat/src/lib.rs:488-503,2248-2325`
- Modify: `src/crates/concat/ui/dialogs/settings.slint:67-130,1543-1643`
- Modify: `src/crates/concat/ui/app.slint:409-420,1055-1067`
- Modify: `src/crates/concat/locales/en.json`
- Test: `src/crates/concat/src/panes/settings.rs` state tests

**Interfaces:**
- Consumes: `UpdateOffer`, `PreparedPlugins`, `AppliedPlugins`, `updates::{PackageKind, Installed}`.
- Produces: `Studio::reload_packages_report(&mut self, announce: bool) -> Vec<concat_effects::Error>`; existing `reload_packages` returns its length.
- Produces: `SettingsMsg::{CheckStartupUpdates, UpdateAll, UpdateAllPrepared(Result<PreparedUpdate, String>), UpdateAllFinished(Result<Installed, String>)}` and matching callbacks.
- Produces: `Confirm::UpdatesAvailable`.
- Produces: `PreparedUpdate { release: Release, app: Option<(PathBuf, PackageKind)>, plugins: PreparedPlugins }` in `concat-host::plugin_updates`.

- [ ] **Step 1: Write failing state and error-attribution tests**

```rust
#[test]
fn update_all_is_enabled_for_either_kind_and_blocked_while_busy() {
    assert!(can_update_all(true, 0, false));
    assert!(can_update_all(false, 2, false));
    assert!(!can_update_all(false, 0, false));
    assert!(!can_update_all(true, 2, true));
}

#[test]
fn only_errors_from_the_updated_group_force_rollback() {
    let updated: HashSet<String> = ["yumo.one".to_owned()].into();
    assert!(error_is_updated(&invalid("yumo.one"), &updated));
    assert!(!error_is_updated(&invalid("local.broken"), &updated));
}
```

- [ ] **Step 2: Run state tests and confirm failure**

Run: `cd src && cargo test -p concat panes::settings::tests`
Expected: FAIL on missing one-click state helpers.

- [ ] **Step 3: Implement shared checking and prepared app downloads**

Add `PreparedUpdate::download(offer: UpdateOffer, dirs: AppDirs, app_kind: Option<PackageKind>, cancel: Arc<AtomicBool>, progress: impl FnMut(u64, u64)) -> Result<PreparedUpdate, String>`. When `offer.app` is true require `app_kind`, fetch/download the installer using existing update functions; independently call `prepare_plugins` for all candidates. No installed directory is modified here.

Change `check_updates` to fetch the newest stable release plus `plugins.json`, build `UpdateOffer`, and return the same result type for startup and manual checks. Guard both with `checking`. Startup failure logs only; manual failure sets `update_status`. Startup success with no candidates does nothing; otherwise set `Confirm::UpdatesAvailable` only when no other confirmation is open.

- [ ] **Step 4: Implement group application, GPU gate, and app handoff**

```rust
let applied = prepared.plugins.apply()?;
let errors = studio.reload_packages_report(false);
if errors.iter().any(|error| applied.involves(error)) {
    applied.rollback()?;
    studio.reload_packages_report(false);
    return Err(t("settings.pluginsRolledBack"));
}
applied.finish();
```

Run this sequence on the UI thread after background preparation. Do not start the installer when rollback occurs. If only plugins updated, clear busy state, refresh offer rows, and show success. If an app package is present, call existing `updates::install` and route its `Installed` result through the existing save/close/relaunch flow. Keep manual version switching unchanged.

Add `Confirm::UpdatesAvailable` acceptance as `SettingsMsg::UpdateAll`; dismissal only clears the confirmation. After `Shell::install`, send `CheckStartupUpdates` for non-smoke desktop runs.

- [ ] **Step 5: Add version-page plugin rows and update controls**

Extend `SettingsData` with `plugin-status: string`, `updates-available: int`, `can-update-all: bool`, and `updating-all: bool`. Add a `settings-update-all` callback through `app.slint`. Render each registered plugin as `id · installed → available` plus compatibility text, and show the existing progress bar for the combined download. Disable repeat actions while checking, downloading, replacing, or installing. Add English source strings for update-now/later, update-all, rollback, success, and compatibility; run the locale generator rather than editing generated locale inventories inconsistently.

- [ ] **Step 6: Run window and localisation checks**

Run: `cd src && cargo test -p concat panes::settings::tests && cargo fmt --check`
Run: `python3 scripts/locales.py --check`
Expected: state tests pass, Slint compiles in CI, locale inventory is complete.

- [ ] **Step 7: Commit the complete user workflow**

```bash
git add src/crates/concat src/crates/concat-host/src/plugin_updates.rs
git commit -m "feat: offer one-click app and plugin updates"
```

### Task 7: Generate plugin assets and publish a Windows-only release

**Files:**
- Create: `scripts/plugin_updates.py`
- Create: `scripts/test_plugin_updates.py`
- Create: `plugins/yumo.volume/effect.toml`
- Create: `.github/workflows/release-windows.yml`
- Modify: `.github/workflows/build-app.yml` with an optional target-selector input and conditional matrix rows
- Modify: `.github/workflows/release.yml` only to point legacy all-platform users at the Windows workflow without changing upstream recovery history
- Test: `scripts/test_plugin_updates.py`

**Interfaces:**
- Produces: `python3 scripts/plugin_updates.py --repository Beau1314/YumoCut --tag v0.2.12 --plugins plugins --out bundles/plugins.json`.
- Produces: release assets `plugins.json`, one deterministic asset per plugin file, `manifest.json`, `SHA256SUMS`, and YumoCut setup/MSI installers.
- Consumes: Task 2 manifest schema and Task 1 `REPO`.

- [ ] **Step 1: Write failing release-script tests**

```python
class PluginManifestTests(unittest.TestCase):
    def test_manifest_matches_package_and_hashes_every_file(self):
        with tempfile.TemporaryDirectory() as root:
            package = Path(root) / "yumo.volume"
            package.mkdir()
            (package / "effect.toml").write_text(AUDIO_PACKAGE, encoding="utf-8")
            data = build_manifest(Path(root), "Beau1314/YumoCut", "v0.2.12")
            self.assertEqual(data["schema"], 1)
            self.assertEqual(data["plugins"][0]["id"], "yumo.volume")
            self.assertEqual(data["plugins"][0]["files"][0]["bytes"], len(AUDIO_PACKAGE.encode()))
            self.assertEqual(len(data["plugins"][0]["files"][0]["sha256"]), 64)
```

Add concrete tests for deterministic ordering, subpath assets, duplicate IDs, non-HTTPS production URLs, and an asset-name collision. Use only Python standard-library `unittest`, `hashlib`, `json`, `pathlib`, and `tempfile`.

- [ ] **Step 2: Run script tests and confirm failure**

Run: `python3 -m unittest scripts.test_plugin_updates -v`
Expected: FAIL with `ModuleNotFoundError: No module named 'scripts.plugin_updates'` before implementation.

- [ ] **Step 3: Implement deterministic manifest and asset generation**

```python
def build_manifest(root: Path, repository: str, tag: str) -> dict:
    plugins = []
    for folder in sorted(path for path in root.iterdir() if path.is_dir()):
        source = (folder / "effect.toml").read_text(encoding="utf-8")
        meta = parse_effect_meta(source)
        files = [file_entry(folder, path, repository, tag, meta["id"], meta["version"])
                 for path in sorted(p for p in folder.rglob("*") if p.is_file())]
        plugins.append({"id": meta["id"], "version": meta["version"],
                        "format": meta["format"], "min_app": meta["min_app"], "files": files})
    return {"schema": 1, "plugins": plugins}
```

`file_entry` uses the URL-safe deterministic asset name `<id>-<version>-<relative path with / replaced by -->`, records bytes and lowercase SHA-256, and rejects collisions. The CLI writes UTF-8 JSON and copies each source into the bundle asset directory. Validate packages through `concat-cli check` in CI before manifest generation.

Add one minimal audio package with an FFmpeg `volume` chain, explicit `min_app = "0.2.11"` release metadata, and no native executable content.

- [ ] **Step 4: Restrict the reusable build and add the release workflow**

Add `build-app.yml` input `targets: string` default `all`; include existing matrix rows only when `targets` is `all` or contains that platform. Update Windows stage copies to include `LICENSE`, `LICENSING.md`, `THIRD_PARTY_NOTICES.md`, `TRADEMARK.md`, and `FORK_NOTICES.md`.

Create `release-windows.yml` with tag/version validation, CI on the tagged ref, `build-app.yml` called with `targets: windows`, plugin checks, manifest/asset generation, checksum generation, and a draft release named `YumoCut <tag>`. Remove phone and non-Windows dependencies from this new workflow. Generate `manifest.json` using the existing `scripts/models.py` without changing its separate model mirror repository.

- [ ] **Step 5: Run local release checks**

Run: `python3 -m unittest scripts.test_plugin_updates -v`
Run: `python3 scripts/plugin_updates.py --repository Beau1314/YumoCut --tag v0.2.12 --plugins plugins --out bundles/plugins.json`
Run: `python3 -c "import json; d=json.load(open('bundles/plugins.json')); assert d['schema'] == 1 and d['plugins'][0]['id'] == 'yumo.volume'"`
Expected: all Python tests pass and the generated manifest validates. YAML parse and full Rust/package checks run in CI.

- [ ] **Step 6: Commit release production**

```bash
git add scripts/plugin_updates.py scripts/test_plugin_updates.py plugins .github/workflows
git commit -m "ci: release YumoCut for Windows with plugin updates"
```

### Task 8: Preserve licence provenance, publish source, and verify real updates

**Files:**
- Create: `FORK_NOTICES.md`
- Modify: `README.md`
- Modify: `docs/README.zh-Hans.md` with a short link to the authoritative English fork notice
- Verify unchanged: `LICENSE`, `LICENSING.md`, `THIRD_PARTY_NOTICES.md`, `TRADEMARK.md`
- Verify: all Tasks 1-7 and generated release assets

**Interfaces:**
- Produces: traceable source at `https://github.com/Beau1314/YumoCut`, upstream baseline `6584d34971ae5e168f12cb475bdf58055b67d780`, and per-release source commit/tag.
- Produces: explicit unofficial-fork, modification, licence, third-party, trademark, and no-extra-patent-grant notices.

- [ ] **Step 1: Add failing provenance checks**

```python
def test_fork_notice_names_upstream_baseline_and_source(self):
    text = Path("FORK_NOTICES.md").read_text(encoding="utf-8")
    for required in ["jub0t/Concat", "6584d34971ae5e168f12cb475bdf58055b67d780",
                     "Beau1314/YumoCut", "AGPL-3.0-or-later", "unofficial"]:
        self.assertIn(required, text)
```

Add assertions that the four upstream licence/policy files still exist, README says YumoCut is a modified unofficial Concat fork, and Windows staging includes all five notice files.

- [ ] **Step 2: Run provenance checks and confirm failure**

Run: `python3 -m unittest scripts.test_plugin_updates -v`
Expected: FAIL because `FORK_NOTICES.md` is absent and README still presents the upstream product.

- [ ] **Step 3: Write the actual fork and licence documents**

```markdown
# YumoCut fork notices

YumoCut is an unofficial, modified version of Concat. It is not maintained or
endorsed by the upstream Concat project.

Upstream: https://github.com/jub0t/Concat
Baseline: v0.2.11, commit 6584d34971ae5e168f12cb475bdf58055b67d780
Corresponding source: https://github.com/Beau1314/YumoCut
Modified: 2026-11-20 — independent branding, startup update notices,
plugin catalog downloads, reversible plugin updates, Windows release tooling.

The source remains AGPL-3.0-or-later. Copyright and licence notices are
preserved. Third-party components retain their own licences. No additional
codec patent licence is granted by this fork.
```

Rewrite README's product identity, screenshots/badges/download links, and support direction so users are not sent to upstream for YumoCut defects. Keep factual upstream attribution and build instructions. Do not change the upstream licence text.

- [ ] **Step 4: Run all locally available checks and inspect the diff**

Run: `python3 -m unittest scripts.test_plugin_updates -v && python3 scripts/locales.py --check && git diff --check`
Run: `cd src && cargo fmt --check && cargo test -p concat-host -p concat-effects -p concat && cargo clippy --all-targets -- -D warnings`
Expected: local Python/static checks pass; Rust checks pass in CI. Any red CI result is fixed before release.

- [ ] **Step 5: Commit notices and reviewed implementation**

```bash
git add FORK_NOTICES.md README.md docs/README.zh-Hans.md scripts/test_plugin_updates.py .github/workflows/build-app.yml
git commit -m "docs: preserve Concat provenance and licence notices"
git status --short
```

Expected: no unintended uncommitted source files.

- [ ] **Step 6: Verify GitHub identity and publish the source repository**

Load the GitHub workflow before remote operations. Read the authenticated account, check whether `Beau1314/YumoCut` exists, create it only if absent, set local `origin` to `https://github.com/Beau1314/YumoCut.git`, retain upstream as `upstream`, then push `main` and tags explicitly requested for test releases.

```bash
git remote set-url origin https://github.com/Beau1314/YumoCut.git
git remote add upstream https://github.com/jub0t/Concat.git
git push -u origin main
```

Read back the remote default branch, HEAD SHA, README, `FORK_NOTICES.md`, and licence files. A successful local push alone is not the verification.

- [ ] **Step 7: Build two real test releases and exercise updates end to end**

Create two sequential numeric versions with real installer/plugin changes. Run `release-windows.yml` for the older version, install it, publish its draft; then run and publish the newer release. Verify both releases contain YumoCut setup/MSI, `manifest.json`, `plugins.json`, plugin assets, and `SHA256SUMS`.

On Windows x86_64 verify: startup notice; later suppresses repeat notice for that run; manual errors are visible; app-only, plugin-only, combined, and no-update states; a corrupt second plugin restores all old plugin files and prevents installer launch; successful update preserves unregistered plugins/settings/projects; installer closes through the unsaved-project gate and the restarted app reports the new version. Record actual versions, Actions run URLs, and any unverified item rather than claiming completion without evidence.
