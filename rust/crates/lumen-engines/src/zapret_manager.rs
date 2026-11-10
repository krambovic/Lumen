//! Minimal winws2 (zapret2) process manager — preset-based, no orchestrator
//! (port of `windows/xray_fluent/zapret_manager.py`).
//!
//! The Qt signals (`started`, `stopped`, `error`, `log_line`) become a
//! broadcast [`ZapretEvent`] channel; lifecycle calls stay synchronous and are
//! meant to run on a worker thread, like the Xray/sing-box manager ports. The
//! QProcess health timer collapses into the monitor thread, which always
//! observes the child exit and reports it.

use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Child;
use std::process::Stdio;
use std::sync::Arc;
use std::sync::LazyLock;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use parking_lot::Mutex;
use regex::Regex;
use sha1::{Digest, Sha1};
use tokio::sync::broadcast;

use std::sync::atomic::{AtomicBool, Ordering};

use lumen_core::constants::{base_dir, data_dir};
use lumen_core::zapret_server_scope::{ZapretServerScope, build_scoped_args};
use lumen_platform::subprocess::{decode_output, hidden_command, run_text_owned};

// ── Module constants ────────────────────────────────────────────────────────

pub fn zapret_dir() -> PathBuf {
    base_dir().join("zapret")
}

pub fn winws2_exe() -> PathBuf {
    zapret_dir().join("exe").join("winws2.exe")
}

pub fn winws_exe() -> PathBuf {
    zapret_dir().join("exe").join("winws.exe")
}

pub fn presets_dir() -> PathBuf {
    zapret_dir().join("presets")
}

/// User-authored presets live outside the install tree: the portable in-app
/// updater wipes everything there except data/.
pub fn user_presets_dir() -> PathBuf {
    data_dir().join("zapret").join("presets")
}

pub fn program_data_dir() -> PathBuf {
    match std::env::var("ProgramData") {
        Ok(value) if !value.trim().is_empty() => PathBuf::from(value),
        _ => PathBuf::from(r"C:\ProgramData"),
    }
}

pub fn at_config_dir() -> PathBuf {
    program_data_dir()
        .join("Lumen")
        .join("zapret")
        .join("winws2_at_config")
}

static LIST_FILE_ARG_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^--(?:ipset|ipset-exclude|hostlist|hostlist-exclude)=(.+)$")
        .expect("valid list-file regex")
});
static BUNDLED_FILE_ARG_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"@((?:lua|bin|windivert\.filter)/[A-Za-z0-9_.+\-]+)").expect("valid blob regex")
});

const ELEVATION_ERROR_MARKERS: [&str; 7] = [
    "elevation",
    "requires elevation",
    "operation requires elevation",
    "requested operation requires elevation",
    "740",
    "требует повышения",
    "требуются повышенные права",
];

const WINDIVERT_CONFLICT_MARKERS: [&str; 7] = [
    "windivert",
    "another instance",
    "already running",
    "error opening filter",
    "failed to open filter",
    "не удалось открыть",
    "другой экземпляр",
];

fn is_elevation_launch_error(text: &str) -> bool {
    let lowered = text.to_lowercase();
    ELEVATION_ERROR_MARKERS
        .iter()
        .any(|marker| lowered.contains(marker))
}

// ── Data types ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PresetInfo {
    pub name: String,
    pub description: String,
    pub created: String,
    pub modified: String,
    pub arg_count: i64,
    pub file_path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ZapretEvent {
    Started,
    Stopped,
    Log(String),
    Error(String),
}

// ── Preset caches (class-level state in Python) ─────────────────────────────

#[derive(Clone)]
struct PresetArgsCacheEntry {
    mtime_ns: u64,
    size: u64,
    args: Vec<String>,
}

#[derive(Default)]
struct PresetCaches {
    names: Option<(Vec<(String, u64, u64)>, Vec<String>)>,
    infos: Option<(Vec<(String, u64, u64)>, Vec<PresetInfo>)>,
    args: BTreeMap<String, PresetArgsCacheEntry>,
    windivert_services: Option<Vec<String>>,
    standard_cleanup_done: bool,
}

static PRESET_CACHES: LazyLock<Mutex<PresetCaches>> =
    LazyLock::new(|| Mutex::new(PresetCaches::default()));

fn mtime_nanos(meta: &fs::Metadata) -> u64 {
    meta.modified()
        .ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map(|duration| duration.as_nanos() as u64)
        .unwrap_or(0)
}

fn read_text_lossy(path: &Path) -> String {
    match fs::read(path) {
        Ok(bytes) => String::from_utf8_lossy(&bytes).into_owned(),
        Err(_) => String::new(),
    }
}

/// Python text-mode `write_text`: `\n` becomes the platform line separator.
fn write_text_native(path: &Path, text: &str) -> std::io::Result<()> {
    #[cfg(windows)]
    {
        fs::write(path, text.replace('\n', "\r\n"))
    }
    #[cfg(not(windows))]
    {
        fs::write(path, text)
    }
}

// ── Preset files ────────────────────────────────────────────────────────────

/// Preset name -> file; a user copy shadows the builtin of the same name.
fn preset_files_in(builtin: &Path, user: &Path) -> Vec<(String, PathBuf)> {
    let mut found: Vec<(String, PathBuf)> = Vec::new();
    for directory in [builtin, user] {
        if !directory.is_dir() {
            continue;
        }
        let Ok(entries) = fs::read_dir(directory) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let name = path
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_default();
            if !name.ends_with(".txt") || name.starts_with('_') {
                continue;
            }
            let stem = path
                .file_stem()
                .map(|stem| stem.to_string_lossy().into_owned())
                .unwrap_or_default();
            // A dict assignment keeps the first insertion position.
            match found.iter().position(|(existing, _)| *existing == stem) {
                Some(position) => found[position].1 = path,
                None => found.push((stem, path)),
            }
        }
    }
    found
}

fn presets_signature_in(builtin: &Path, user: &Path) -> Vec<(String, u64, u64)> {
    let mut entries: Vec<(String, u64, u64)> = Vec::new();
    for (name, path) in preset_files_in(builtin, user) {
        let Ok(meta) = fs::metadata(&path) else {
            continue;
        };
        entries.push((name, mtime_nanos(&meta), meta.len()));
    }
    entries.sort_by(|left, right| left.0.cmp(&right.0));
    entries
}

/// Drop every cached preset view; call after any preset file mutation.
pub fn invalidate_preset_cache() {
    let mut caches = PRESET_CACHES.lock();
    caches.names = None;
    caches.infos = None;
    caches.args.clear();
}

pub fn list_presets() -> Vec<String> {
    list_presets_in(&presets_dir(), &user_presets_dir())
}

/// Sorted list of available preset names (without .txt).
fn list_presets_in(builtin: &Path, user: &Path) -> Vec<String> {
    let signature = presets_signature_in(builtin, user);
    let mut caches = PRESET_CACHES.lock();
    if let Some((cached_signature, names)) = &caches.names
        && *cached_signature == signature
    {
        return names.clone();
    }
    let names: Vec<String> = signature.iter().map(|(name, _, _)| name.clone()).collect();
    caches.names = Some((signature, names.clone()));
    names
}

pub fn preset_path(name: &str) -> PathBuf {
    preset_path_in(name, &presets_dir(), &user_presets_dir())
}

fn preset_path_in(name: &str, builtin: &Path, user: &Path) -> PathBuf {
    let user_path = user.join(format!("{name}.txt"));
    if user_path.is_file() {
        return user_path;
    }
    let builtin_path = builtin.join(format!("{name}.txt"));
    if builtin_path.is_file() {
        return builtin_path;
    }
    user_path
}

/// `_split_launch_line`: one preset line becomes argv items like the upstream
/// winws2 runner; `--` flags may be written on one line.
fn split_launch_line(raw_line: &str) -> Vec<String> {
    let stripped = raw_line.trim();
    if stripped.is_empty() {
        return Vec::new();
    }
    if !stripped.starts_with("--") {
        return vec![stripped.to_string()];
    }
    split_inline_args(stripped)
        .into_iter()
        .map(|part| part.trim().to_string())
        .filter(|part| !part.is_empty())
        .collect()
}

/// `re.split(r"(?<=\S)\s+(?=--)")` without lookaround support: split before
/// every `--` token preceded by whitespace and a non-space character.
fn split_inline_args(stripped: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut start = 0;
    for (arg_start, _) in stripped.match_indices("--").skip(1) {
        let previous = &stripped[start..arg_start];
        let trimmed = previous.trim_end();
        if trimmed.len() < previous.len() {
            parts.push(trimmed);
            start = arg_start;
        }
    }
    parts.push(&stripped[start..]);
    parts
}

fn mtime_size(meta: &fs::Metadata) -> (u64, u64) {
    (mtime_nanos(meta), meta.len())
}

/// Read a preset file and return argv items like upstream winws2 runner.
pub fn parse_preset_args(preset: &Path) -> Vec<String> {
    let meta = fs::metadata(preset).ok();
    let cache_key = fs::canonicalize(preset)
        .unwrap_or_else(|_| preset.to_path_buf())
        .to_string_lossy()
        .into_owned();
    if let Some(meta) = &meta {
        let caches = PRESET_CACHES.lock();
        if let Some(cached) = caches.args.get(&cache_key) {
            let (mtime_ns, size) = mtime_size(meta);
            if cached.mtime_ns == mtime_ns && cached.size == size {
                return cached.args.clone();
            }
        }
    }
    let bytes = fs::read(preset).unwrap_or_default();
    let text = String::from_utf8_lossy(&bytes);
    let mut args: Vec<String> = Vec::new();
    for line in text.lines() {
        let stripped = line.trim();
        if !stripped.is_empty() && !stripped.starts_with('#') {
            args.extend(split_launch_line(stripped));
        }
    }
    if let Some(meta) = meta {
        let (mtime_ns, size) = mtime_size(&meta);
        PRESET_CACHES.lock().args.insert(
            cache_key,
            PresetArgsCacheEntry {
                mtime_ns,
                size,
                args: args.clone(),
            },
        );
    }
    args
}

/// `_write_at_config`: quote the argv into an @config file under ProgramData.
fn write_at_config(preset: &Path, args: &[String]) -> PathBuf {
    let mut config_text = String::new();
    for arg in args {
        if arg.trim().is_empty() {
            continue;
        }
        config_text.push_str(&shlex_quote(arg));
        config_text.push('\n');
    }
    let resolved = fs::canonicalize(preset).unwrap_or_else(|_| preset.to_path_buf());
    let digest_source = format!("{}\0{config_text}", resolved.to_string_lossy());
    let digest = format!("{:x}", Sha1::digest(digest_source.as_bytes()));
    let digest = &digest[..20.min(digest.len())];
    let _ = fs::create_dir_all(at_config_dir());
    let config_path = at_config_dir().join(format!("winws2_at_{digest}.txt"));
    if let Ok(existing) = fs::read_to_string(&config_path) {
        if existing == config_text {
            prune_at_config_cache(&config_path);
            return config_path;
        }
    }
    // newline="\n": the @config must keep LF endings.
    let _ = fs::write(&config_path, &config_text);
    prune_at_config_cache(&config_path);
    config_path
}

fn prune_at_config_cache(keep_path: &Path) {
    const MAX_FILES: usize = 64;
    let Ok(entries) = fs::read_dir(at_config_dir()) else {
        return;
    };
    let mut files: Vec<(PathBuf, SystemTime)> = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            path.is_file()
                && path
                    .file_name()
                    .map(|name| name.to_string_lossy().starts_with("winws2_at_"))
                    .unwrap_or(false)
                && path.extension().map(|ext| ext == "txt").unwrap_or(false)
        })
        .filter_map(|path| {
            let mtime = fs::metadata(&path).and_then(|meta| meta.modified()).ok()?;
            Some((path, mtime))
        })
        .collect();
    files.sort_by(|left, right| right.1.cmp(&left.1));
    let keep = fs::canonicalize(keep_path).unwrap_or_else(|_| keep_path.to_path_buf());
    for (path, _) in files.iter().skip(MAX_FILES.max(1)) {
        let resolved = fs::canonicalize(path).unwrap_or_else(|_| path.clone());
        if resolved != keep {
            let _ = fs::remove_file(path);
        }
    }
}

/// Python `shlex.quote`.
fn shlex_quote(value: &str) -> String {
    let needs_quoting = value.is_empty()
        || value
            .chars()
            .any(|ch| !(ch.is_alphanumeric() || ch == '_' || "@%+=:,./-".contains(ch)));
    if !needs_quoting {
        return value.to_string();
    }
    format!("'{}'", value.replace('\'', "'\"'\"'"))
}

/// Extract metadata from comment headers.
fn parse_metadata(text: &str) -> BTreeMap<String, String> {
    let mut meta = BTreeMap::new();
    for line in text.lines().take(15) {
        // only check first 15 lines
        let stripped = line.trim();
        if !stripped.starts_with('#') {
            if !stripped.is_empty() {
                // non-empty non-comment = end of headers
                break;
            }
            continue;
        }
        for key in [
            "Preset",
            "Description",
            "Created",
            "Modified",
            "BuiltinVersion",
        ] {
            let prefix = format!("# {key}:");
            if stripped.starts_with(&prefix) {
                meta.insert(key.to_string(), stripped[prefix.len()..].trim().to_string());
                break;
            }
        }
    }
    meta
}

pub fn list_preset_infos() -> Vec<PresetInfo> {
    list_preset_infos_in(&presets_dir(), &user_presets_dir())
}

/// List of PresetInfo for all presets, sorted by name.
fn list_preset_infos_in(builtin: &Path, user: &Path) -> Vec<PresetInfo> {
    let signature = presets_signature_in(builtin, user);
    {
        let caches = PRESET_CACHES.lock();
        if let Some((cached_signature, infos)) = &caches.infos
            && *cached_signature == signature
        {
            return infos.clone();
        }
    }
    let mut files = preset_files_in(builtin, user);
    files.sort_by(|left, right| left.0.cmp(&right.0));
    let mut result = Vec::new();
    for (name, path) in files {
        let text = read_text_lossy(&path);
        let meta = parse_metadata(&text);
        let arg_count = text
            .lines()
            .filter(|line| {
                let stripped = line.trim();
                !stripped.is_empty() && !stripped.starts_with('#')
            })
            .count() as i64;
        result.push(PresetInfo {
            name,
            description: meta.get("Description").cloned().unwrap_or_default(),
            created: meta.get("Created").cloned().unwrap_or_default(),
            modified: meta.get("Modified").cloned().unwrap_or_default(),
            arg_count,
            file_path: path,
        });
    }
    PRESET_CACHES.lock().infos = Some((signature, result.clone()));
    result
}

/// Full text content of a preset file.
pub fn read_preset(name: &str) -> String {
    let path = preset_path(name);
    if !path.is_file() {
        return String::new();
    }
    read_text_lossy(&path)
}

fn remove_preset_files_in(name: &str, builtin: &Path, user: &Path) -> bool {
    let mut removed = false;
    for directory in [user, builtin] {
        let path = directory.join(format!("{name}.txt"));
        if path.is_file() {
            let _ = fs::remove_file(&path);
            removed = true;
        }
    }
    removed
}

/// Write a preset file with updated metadata headers.
pub fn save_preset(name: &str, content: &str, description: &str) -> PresetInfo {
    save_preset_in(
        name,
        content,
        description,
        &presets_dir(),
        &user_presets_dir(),
    )
}

fn save_preset_in(
    name: &str,
    content: &str,
    description: &str,
    builtin: &Path,
    user: &Path,
) -> PresetInfo {
    let existing = preset_path_in(name, builtin, user);
    let path = user.join(format!("{name}.txt"));
    let _ = fs::create_dir_all(user);

    // Preserve original Created date if the file exists
    let mut created = String::new();
    if existing.is_file() {
        let old_text = read_text_lossy(&existing);
        let old_meta = parse_metadata(&old_text);
        created = old_meta.get("Created").cloned().unwrap_or_default();
    }

    let now = chrono::Local::now().format("%Y-%m-%dT%H:%M:%S").to_string();
    if created.is_empty() {
        created = now.clone();
    }

    // Strip existing metadata headers from content
    let mut body_lines: Vec<&str> = Vec::new();
    for line in content.lines() {
        let stripped = line.trim();
        if stripped.starts_with("# Preset:")
            || stripped.starts_with("# Description:")
            || stripped.starts_with("# Created:")
            || stripped.starts_with("# Modified:")
        {
            continue;
        }
        body_lines.push(line);
    }

    // Remove leading blank lines from body
    while body_lines
        .first()
        .map(|line| line.trim().is_empty())
        .unwrap_or(false)
    {
        body_lines.remove(0);
    }

    let header = format!(
        "# Preset: {name}\n# Description: {description}\n# Created: {created}\n# Modified: {now}\n\n"
    );
    let full_text = format!("{header}{}\n", body_lines.join("\n"));
    let _ = write_text_native(&path, &full_text);
    invalidate_preset_cache();

    let arg_count = body_lines
        .iter()
        .filter(|line| {
            let stripped = line.trim();
            !stripped.is_empty() && !stripped.starts_with('#')
        })
        .count() as i64;
    PresetInfo {
        name: name.to_string(),
        description: description.to_string(),
        created,
        modified: now,
        arg_count,
        file_path: path,
    }
}

/// Rename a preset file. Returns the new PresetInfo or None on failure.
pub fn rename_preset(old_name: &str, new_name: &str) -> Option<PresetInfo> {
    rename_preset_in(old_name, new_name, &presets_dir(), &user_presets_dir())
}

fn rename_preset_in(
    old_name: &str,
    new_name: &str,
    builtin: &Path,
    user: &Path,
) -> Option<PresetInfo> {
    let old_path = preset_path_in(old_name, builtin, user);
    let new_path = user.join(format!("{new_name}.txt"));
    if !old_path.is_file()
        || preset_files_in(builtin, user)
            .iter()
            .any(|(name, _)| name == new_name)
    {
        return None;
    }

    // Update the # Preset: header inside the file
    let text = read_text_lossy(&old_path);
    let text = text.replacen(
        &format!("# Preset: {old_name}"),
        &format!("# Preset: {new_name}"),
        1,
    );
    let _ = fs::create_dir_all(user);
    let _ = write_text_native(&new_path, &text);
    remove_preset_files_in(old_name, builtin, user);
    invalidate_preset_cache();

    let meta = parse_metadata(&text);
    let arg_count = text
        .lines()
        .filter(|line| {
            let stripped = line.trim();
            !stripped.is_empty() && !stripped.starts_with('#')
        })
        .count() as i64;
    Some(PresetInfo {
        name: new_name.to_string(),
        description: meta.get("Description").cloned().unwrap_or_default(),
        created: meta.get("Created").cloned().unwrap_or_default(),
        modified: meta.get("Modified").cloned().unwrap_or_default(),
        arg_count,
        file_path: new_path,
    })
}

/// Delete a preset file. Returns true if deleted.
pub fn delete_preset(name: &str) -> bool {
    delete_preset_in(name, &presets_dir(), &user_presets_dir())
}

fn delete_preset_in(name: &str, builtin: &Path, user: &Path) -> bool {
    if !remove_preset_files_in(name, builtin, user) {
        return false;
    }
    invalidate_preset_cache();
    true
}

/// Import a preset file from an external path. Handles name conflicts.
pub fn import_preset(source_path: &Path) -> Option<PresetInfo> {
    import_preset_in(source_path, &presets_dir(), &user_presets_dir())
}

fn import_preset_in(source_path: &Path, builtin: &Path, user: &Path) -> Option<PresetInfo> {
    if !source_path.is_file() {
        return None;
    }
    let _ = fs::create_dir_all(user);

    let taken: BTreeSet<String> = preset_files_in(builtin, user)
        .into_iter()
        .map(|(name, _)| name)
        .collect();
    let base_name = source_path
        .file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
        .unwrap_or_default();
    let mut target = user.join(format!("{base_name}.txt"));
    let mut counter = 1;
    loop {
        let stem = target
            .file_stem()
            .map(|stem| stem.to_string_lossy().into_owned())
            .unwrap_or_default();
        if !taken.contains(&stem) && !target.exists() {
            break;
        }
        target = user.join(format!("{base_name} ({counter}).txt"));
        counter += 1;
    }

    fs::copy(source_path, &target).ok()?;
    invalidate_preset_cache();

    // Read and return info
    let text = read_text_lossy(&target);
    let meta = parse_metadata(&text);
    let arg_count = text
        .lines()
        .filter(|line| {
            let stripped = line.trim();
            !stripped.is_empty() && !stripped.starts_with('#')
        })
        .count() as i64;
    Some(PresetInfo {
        name: target
            .file_stem()
            .map(|stem| stem.to_string_lossy().into_owned())
            .unwrap_or_default(),
        description: meta.get("Description").cloned().unwrap_or_default(),
        created: meta.get("Created").cloned().unwrap_or_default(),
        modified: meta.get("Modified").cloned().unwrap_or_default(),
        arg_count,
        file_path: target,
    })
}

// ── Preflight and referenced files ──────────────────────────────────────────

fn preflight_start(exe: &Path, preset: &Path, preset_name: &str) -> String {
    if preset_name.is_empty() {
        return "Не выбран пресет Zapret.".to_string();
    }
    if !exe.exists() {
        return format!("winws2.exe не найден: {}", exe.display());
    }
    if !preset.exists() {
        return format!("Пресет не найден: {}", preset.display());
    }
    preflight_exe(exe)
}

fn preflight_exe(exe: &Path) -> String {
    if !exe.exists() {
        return format!("winws2.exe не найден: {}", exe.display());
    }
    let exe_dir = exe.parent().map(Path::to_path_buf).unwrap_or_default();
    let missing: Vec<&str> = ["WinDivert.dll", "Monkey64.sys"]
        .into_iter()
        .filter(|name| !exe_dir.join(name).exists())
        .collect();
    if !missing.is_empty() {
        return format!("Не найдены файлы Zapret: {}", missing.join(", "));
    }
    String::new()
}

fn referenced_zapret_files_in(zapret_dir: &Path, args: &[String]) -> Vec<PathBuf> {
    let mut paths: Vec<PathBuf> = Vec::new();
    for arg in args {
        for found in BUNDLED_FILE_ARG_RE.captures_iter(arg) {
            paths.push(zapret_dir.join(&found[1]));
        }
        let stripped = arg.trim();
        let Some(found) = LIST_FILE_ARG_RE.captures(stripped) else {
            continue;
        };
        let value = found[1].trim().trim_matches('"');
        if value.is_empty() || value.contains(',') || value.contains("://") {
            continue;
        }
        let path = Path::new(value);
        if path.is_absolute() {
            continue;
        }
        paths.push(zapret_dir.join(path));
    }
    paths
}

pub fn referenced_zapret_files(args: &[String]) -> Vec<PathBuf> {
    referenced_zapret_files_in(&zapret_dir(), args)
}

/// Create the legacy ipset-base alias without masking missing preset data.
pub fn ensure_compatibility_lists() -> String {
    ensure_compatibility_lists_in(&zapret_dir())
}

fn ensure_compatibility_lists_in(zapret_dir: &Path) -> String {
    let lists_dir = zapret_dir.join("lists");
    let ipset_base = lists_dir.join("ipset-base.txt");
    let ipset_all = lists_dir.join("ipset-all.txt");
    let mut errors: Vec<String> = Vec::new();
    if !ipset_base.exists() && ipset_all.is_file() {
        if let Err(exc) = fs::copy(&ipset_all, &ipset_base) {
            errors.push(format!(
                "Не удалось создать {}: {exc}",
                ipset_base.display()
            ));
        }
    }
    errors.join("; ")
}

pub fn missing_referenced_files(args: &[String]) -> Vec<PathBuf> {
    missing_referenced_files_in(&zapret_dir(), args)
}

fn missing_referenced_files_in(zapret_dir: &Path, args: &[String]) -> Vec<PathBuf> {
    ensure_compatibility_lists_in(zapret_dir);
    referenced_zapret_files_in(zapret_dir, args)
        .into_iter()
        .filter(|path| !path.is_file())
        .collect()
}

// ── Manager lifecycle ───────────────────────────────────────────────────────

struct ProcessHandle {
    child: Mutex<Child>,
    /// Python disconnects `finished` in `stop()`; this flag plays that role.
    report_exit: AtomicBool,
}

#[derive(Default)]
struct ZapretState {
    process: Option<Arc<ProcessHandle>>,
    current_preset: String,
    /// Bound preset restricted to one server (runs next to the manual one).
    server_scope: Option<ZapretServerScope>,
    start_args: Vec<String>,
    start_retry_count: i64,
    current_at_config: Option<PathBuf>,
    launch_mode: &'static str,
    output_tail: Vec<String>,
    /// WinDivert needs a moment after a full stop before it is opened again;
    /// the next start waits out the rest instead of the stop blocking the UI.
    settle_until: Option<Instant>,
}

struct Shared {
    state: Mutex<ZapretState>,
    events: broadcast::Sender<ZapretEvent>,
}

impl Shared {
    fn emit(&self, event: ZapretEvent) {
        let _ = self.events.send(event);
    }
}

/// Start / stop winws2.exe with a preset file.
pub struct ZapretManager {
    shared: Arc<Shared>,
}

impl Default for ZapretManager {
    fn default() -> Self {
        Self::new()
    }
}

impl ZapretManager {
    pub fn new() -> Self {
        let (events, _) = broadcast::channel(512);
        Self {
            shared: Arc::new(Shared {
                state: Mutex::new(ZapretState {
                    launch_mode: "direct",
                    ..Default::default()
                }),
                events,
            }),
        }
    }

    pub fn subscribe(&self) -> broadcast::Receiver<ZapretEvent> {
        self.shared.events.subscribe()
    }

    pub fn running(&self) -> bool {
        let state = self.shared.state.lock();
        state
            .process
            .as_ref()
            .map(|handle| child_exited(handle).is_none())
            .unwrap_or(false)
    }

    /// Name of the preset that is currently running ("" when stopped).
    pub fn current_preset(&self) -> String {
        let state = self.shared.state.lock();
        if state.process.is_some() {
            state.current_preset.clone()
        } else {
            String::new()
        }
    }

    /// Server-only part of the running process (None when absent).
    pub fn server_scope(&self) -> Option<ZapretServerScope> {
        let state = self.shared.state.lock();
        if state.process.is_some() {
            state.server_scope.clone()
        } else {
            None
        }
    }

    /// `start(preset_name)`: launch winws2 with the preset argv.
    pub fn start(&self, preset_name: &str, server_scope: Option<ZapretServerScope>) {
        self.start_with(preset_name, server_scope, 0, false, "direct");
    }

    pub fn start_with(
        &self,
        preset_name: &str,
        server_scope: Option<ZapretServerScope>,
        retry_count: i64,
        force_cleanup: bool,
        launch_mode: &str,
    ) {
        start_on(
            Arc::clone(&self.shared),
            preset_name,
            server_scope,
            retry_count,
            force_cleanup,
            launch_mode,
        );
    }

    pub fn stop(&self) {
        self.stop_with(false, true, true);
    }

    pub fn stop_with(&self, fast: bool, cleanup: bool, emit_stopped: bool) {
        stop_on(Arc::clone(&self.shared), fast, cleanup, emit_stopped);
    }
}

fn capitalize_first(text: &str) -> String {
    let mut chars = text.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

fn start_on(
    shared: Arc<Shared>,
    preset_name: &str,
    server_scope: Option<ZapretServerScope>,
    retry_count: i64,
    force_cleanup: bool,
    requested_launch_mode: &str,
) {
    let preset_name = preset_name.trim();
    let exe = winws2_exe();
    let scope = server_scope.filter(|scope| !scope.ips.is_empty());
    let mut preset = preset_path(preset_name);
    let mut args: Vec<String> = Vec::new();
    if !preset_name.is_empty() || scope.is_none() {
        let preflight_error = preflight_start(&exe, &preset, preset_name);
        if !preflight_error.is_empty() {
            shared.emit(ZapretEvent::Error(preflight_error));
            return;
        }
        args = parse_preset_args(&preset);
        if args.is_empty() {
            shared.emit(ZapretEvent::Error(format!("Пресет пустой: {preset_name}")));
            return;
        }
    }
    if let Some(scope) = &scope {
        let server_preset;
        let preflight_error;
        if scope.uses_strategy() {
            server_preset = zapret_dir()
                .join("strategy_catalogs")
                .join("winws2")
                .join(format!(
                    "{}-server.txt",
                    if scope.kind.is_empty() {
                        "tcp"
                    } else {
                        &scope.kind
                    }
                ));
            preflight_error = preflight_exe(&exe);
        } else {
            server_preset = preset_path(&scope.preset);
            preflight_error = preflight_start(&exe, &server_preset, &scope.preset);
        }
        if !preflight_error.is_empty() {
            shared.emit(ZapretEvent::Error(preflight_error));
            return;
        }
        let server_args = if scope.uses_strategy() {
            Vec::new()
        } else {
            parse_preset_args(&server_preset)
        };
        args = build_scoped_args(&server_args, scope, &args);
        if args.is_empty() {
            if scope.uses_strategy() {
                shared.emit(ZapretEvent::Error(format!(
                    "Стратегия {} не подходит для сервера",
                    if scope.strategy.is_empty() {
                        "?"
                    } else {
                        &scope.strategy
                    }
                )));
            } else {
                shared.emit(ZapretEvent::Error(format!(
                    "В пресете {} нет стратегий, которые можно применить к серверу",
                    scope.preset
                )));
            }
            return;
        }
        if preset_name.is_empty() {
            preset = server_preset;
        }
        let title = capitalize_first(&scope.title());
        let mut message = format!(
            "[zapret] {title} применяется только к серверу {}",
            scope.describe()
        );
        if !preset_name.is_empty() {
            message.push_str(&format!("; остальной трафик: {preset_name}"));
        }
        shared.emit(ZapretEvent::Log(message));
    }

    let lists_error = ensure_compatibility_lists();
    let missing_files = missing_referenced_files(&args);
    if !missing_files.is_empty() {
        let mut preview_items: Vec<String> = Vec::new();
        for path in missing_files.iter().take(6) {
            match path.strip_prefix(zapret_dir()) {
                Ok(relative) => preview_items.push(relative.to_string_lossy().into_owned()),
                Err(_) => preview_items.push(path.to_string_lossy().into_owned()),
            }
        }
        let mut preview = preview_items.join(", ");
        if missing_files.len() > 6 {
            preview += &format!(" ... (+{})", missing_files.len() - 6);
        }
        let mut message = format!(
            "Не найдены файлы Zapret: {preview}. Запуск остановлен, чтобы пресет не работал частично."
        );
        if !lists_error.is_empty() {
            message.push_str(&format!(". {lists_error}"));
        }
        shared.emit(ZapretEvent::Error(message));
        return;
    }

    let was_running = running_of(&shared);
    if was_running {
        let current = shared.state.lock().current_preset.clone();
        if current == preset_name {
            shared.emit(ZapretEvent::Log(format!(
                "[zapret] Перезапуск текущего пресета: {preset_name}"
            )));
        } else {
            shared.emit(ZapretEvent::Log(format!(
                "[zapret] Переключение пресета: {current} -> {preset_name}"
            )));
        }
        stop_on(Arc::clone(&shared), true, false, false);
    }

    let killed = if was_running && !force_cleanup {
        Vec::new()
    } else {
        kill_orphaned(
            if force_cleanup { 5.0 } else { 0.8 },
            if force_cleanup { 1.25 } else { 0.1 },
        )
    };
    for name in killed {
        shared.emit(ZapretEvent::Log(format!(
            "[zapret] Завершён сторонний процесс: {name}"
        )));
    }

    if force_cleanup {
        shared.emit(ZapretEvent::Log(
            "[zapret] WinDivert конфликт: выполняется глубокая очистка перед повторным запуском"
                .into(),
        ));
        cleanup_windivert(2.5);
        std::thread::sleep(Duration::from_millis(150));
    } else if !was_running {
        standard_windivert_cleanup(false);
        let settle = {
            let state = shared.state.lock();
            state
                .settle_until
                .map(|deadline| deadline.saturating_duration_since(Instant::now()))
                .unwrap_or_default()
        };
        if settle > Duration::ZERO {
            std::thread::sleep(settle);
        }
    } else {
        std::thread::sleep(Duration::from_millis(50));
    }

    let launch_mode: &'static str = if requested_launch_mode == "at_config" {
        "at_config"
    } else {
        "direct"
    };
    let at_config = if launch_mode == "at_config" {
        Some(write_at_config(&preset, &args))
    } else {
        None
    };
    let start_args = match &at_config {
        Some(path) => vec![format!("@{}", path.display())],
        None => args.clone(),
    };

    {
        let mut state = shared.state.lock();
        state.current_preset = preset_name.to_string();
        state.server_scope = scope.clone();
        state.start_args = start_args.clone();
        state.start_retry_count = retry_count;
        state.current_at_config = at_config.clone();
        state.launch_mode = launch_mode;
        state.output_tail.clear();
    }

    let args_count = args.len();
    let label = if !preset_name.is_empty() {
        preset_name.to_string()
    } else {
        scope
            .as_ref()
            .map(|scope| scope.title())
            .unwrap_or_default()
    };

    let mut command = hidden_command(&exe);
    command
        .args(&start_args)
        .current_dir(zapret_dir())
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(path) = &at_config {
        shared.emit(ZapretEvent::Log(format!(
            "[zapret] Запуск: {label} ({args_count} аргументов через @{})",
            path.file_name()
                .map(|name| name.to_string_lossy())
                .unwrap_or_default()
        )));
    } else {
        shared.emit(ZapretEvent::Log(format!(
            "[zapret] Запуск: {label} ({args_count} аргументов напрямую)"
        )));
    }
    let spawn = command.spawn();
    match spawn {
        Err(error) => {
            let launch_error = error.to_string();
            if is_elevation_launch_error(&launch_error) {
                shared.emit(ZapretEvent::Log(format!(
                    "[zapret] Запуск winws2.exe отклонён Windows: {launch_error}"
                )));
                shared.emit(ZapretEvent::Error(
                    "Для запуска Zapret нужны права администратора. Перезапустите Lumen от имени администратора."
                        .into(),
                ));
                manager_release(&shared, None);
                set_launch_mode(&shared, "direct");
                return;
            }
            if launch_mode == "direct" {
                shared.emit(ZapretEvent::Log(
                    "[zapret] Прямой запуск не стартовал, пробую fallback через @config из ProgramData"
                        .into(),
                ));
                manager_release(&shared, None);
                schedule_retry(&shared, preset_name, scope.clone(), 2, false, "at_config");
                return;
            }
            shared.emit(ZapretEvent::Error(format!(
                "Не удалось запустить winws2.exe: {launch_error}"
            )));
            manager_release(&shared, None);
            set_launch_mode(&shared, "direct");
        }
        Ok(mut child) => {
            let stdout = child.stdout.take();
            let stderr = child.stderr.take();
            let handle = Arc::new(ProcessHandle {
                child: Mutex::new(child),
                report_exit: AtomicBool::new(true),
            });
            {
                let mut state = shared.state.lock();
                state.process = Some(Arc::clone(&handle));
            }
            spawn_monitor(Arc::clone(&shared), Arc::clone(&handle), stdout, stderr);
            shared.emit(ZapretEvent::Started);
        }
    }
}

fn running_of(shared: &Shared) -> bool {
    let state = shared.state.lock();
    state
        .process
        .as_ref()
        .map(|handle| child_exited(handle).is_none())
        .unwrap_or(false)
}

fn set_launch_mode(shared: &Shared, launch_mode: &'static str) {
    shared.state.lock().launch_mode = launch_mode;
}

fn manager_release(shared: &Shared, process: Option<&Arc<ProcessHandle>>) {
    let mut state = shared.state.lock();
    match (process, state.process.as_ref()) {
        (Some(released), Some(active)) if Arc::ptr_eq(released, active) => {
            state.process = None;
        }
        (None, _) => state.process = None,
        _ => {}
    }
}

/// QTimer.singleShot(150, ...) retry: rerun start through a fresh thread.
fn schedule_retry(
    shared: &Arc<Shared>,
    preset_name: &str,
    scope: Option<ZapretServerScope>,
    retry_count: i64,
    force_cleanup: bool,
    launch_mode: &'static str,
) {
    let shared = Arc::clone(shared);
    let preset_name = preset_name.to_string();
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(150));
        start_on(
            shared,
            &preset_name,
            scope,
            retry_count,
            force_cleanup,
            launch_mode,
        );
    });
}

fn child_exited(handle: &ProcessHandle) -> Option<std::process::ExitStatus> {
    handle.child.lock().try_wait().ok().flatten()
}

/// Read stdout/stderr until EOF and feed every stripped line to the log.
fn pump_lines<R: Read>(mut reader: R, shared: &Shared) {
    let mut buffer: Vec<u8> = Vec::new();
    let mut chunk = [0u8; 4096];
    loop {
        match reader.read(&mut chunk) {
            Ok(0) | Err(_) => break,
            Ok(read) => buffer.extend_from_slice(&chunk[..read]),
        }
        while let Some(position) = buffer.iter().position(|byte| *byte == b'\n') {
            let mut line: Vec<u8> = buffer.drain(..=position).collect();
            line.pop();
            if line.last() == Some(&b'\r') {
                line.pop();
            }
            emit_line(shared, &decode_output(&line));
        }
    }
    if !buffer.is_empty() {
        emit_line(shared, &decode_output(&buffer));
    }
}

fn emit_line(shared: &Shared, text: &str) {
    let stripped = text.trim();
    if stripped.is_empty() {
        return;
    }
    {
        let mut state = shared.state.lock();
        state.output_tail.push(stripped.to_string());
        if state.output_tail.len() > 80 {
            let excess = state.output_tail.len() - 80;
            state.output_tail.drain(..excess);
        }
    }
    shared.emit(ZapretEvent::Log(format!("[zapret] {stripped}")));
}

fn spawn_monitor(
    shared: Arc<Shared>,
    handle: Arc<ProcessHandle>,
    stdout: Option<std::process::ChildStdout>,
    stderr: Option<std::process::ChildStderr>,
) {
    let out_shared = Arc::clone(&shared);
    let out_reader = stdout.map(|pipe| std::thread::spawn(move || pump_lines(pipe, &out_shared)));
    let err_shared = Arc::clone(&shared);
    let err_reader = stderr.map(|pipe| std::thread::spawn(move || pump_lines(pipe, &err_shared)));
    std::thread::spawn(move || {
        // Poll with short lock holds so stop() can acquire the child.
        let status = loop {
            if let Some(status) = child_exited(&handle) {
                break status;
            }
            std::thread::sleep(Duration::from_millis(50));
        };
        if let Some(reader) = out_reader {
            let _ = reader.join();
        }
        if let Some(reader) = err_reader {
            let _ = reader.join();
        }
        if handle.report_exit.load(Ordering::SeqCst) {
            on_finished(&shared, &handle, status.code().unwrap_or(-1));
        }
    });
}

fn on_finished(shared: &Arc<Shared>, handle: &Arc<ProcessHandle>, exit_code: i32) {
    let output_for_diagnostics = {
        let state = shared.state.lock();
        state.output_tail.clone()
    };
    let (preset, launch_mode, retry_count, has_context) = {
        let state = shared.state.lock();
        (
            if state.current_preset.is_empty() {
                "?".to_string()
            } else {
                state.current_preset.clone()
            },
            state.launch_mode,
            state.start_retry_count,
            !state.current_preset.is_empty() || state.server_scope.is_some(),
        )
    };

    if exit_code != 0 {
        if launch_mode == "direct" && retry_count == 0 && has_context && exit_code == 2 {
            let (preset_name, scope) = {
                let state = shared.state.lock();
                (state.current_preset.clone(), state.server_scope.clone())
            };
            shared.emit(ZapretEvent::Log(
                "[zapret] Ошибка аргументов, пробую fallback через @config из ProgramData".into(),
            ));
            manager_release(shared, Some(handle));
            schedule_retry(shared, &preset_name, scope, 2, false, "at_config");
            return;
        }

        if retry_count < 1
            && has_context
            && looks_like_windivert_conflict(exit_code, &output_for_diagnostics)
        {
            let (preset_name, scope) = {
                let state = shared.state.lock();
                (state.current_preset.clone(), state.server_scope.clone())
            };
            shared.emit(ZapretEvent::Log(
                "[zapret] winws2 завершился с кодом 1, пробую повторный запуск после очистки WinDivert"
                    .into(),
            ));
            manager_release(shared, Some(handle));
            schedule_retry(shared, &preset_name, scope, 1, true, "direct");
            return;
        }

        if launch_mode == "direct"
            && retry_count == 1
            && has_context
            && (exit_code == 1 || exit_code == 2)
        {
            let (preset_name, scope) = {
                let state = shared.state.lock();
                (state.current_preset.clone(), state.server_scope.clone())
            };
            shared.emit(ZapretEvent::Log(
                "[zapret] Прямой запуск не прошёл, пробую fallback через @config из ProgramData"
                    .into(),
            ));
            manager_release(shared, Some(handle));
            schedule_retry(shared, &preset_name, scope, 2, false, "at_config");
            return;
        }

        // Подробности в лог
        let hint = exit_code_hint(exit_code);
        if !hint.is_empty() {
            shared.emit(ZapretEvent::Log(format!(
                "[zapret] Код {exit_code}: {hint}"
            )));
        }
        shared.emit(ZapretEvent::Log(format!("[zapret] Пресет: {preset}")));
        let start_args = shared.state.lock().start_args.clone();
        if !start_args.is_empty() {
            let mut preview = start_args
                .iter()
                .take(6)
                .cloned()
                .collect::<Vec<_>>()
                .join(" ");
            if start_args.len() > 6 {
                preview += &format!(" ... (+{} аргументов)", start_args.len() - 6);
            }
            shared.emit(ZapretEvent::Log(format!(
                "[zapret] Команда: winws2.exe {preview}"
            )));
        }
        if output_for_diagnostics.is_empty() {
            shared.emit(ZapretEvent::Log(
                "[zapret] Процесс не вывел ничего в stdout/stderr".into(),
            ));
        }

        // Краткое сообщение для InfoBar и status_label
        let mut short = format!("winws2 завершился с кодом {exit_code}");
        if !hint.is_empty() {
            short += &format!(" — {hint}");
        }
        shared.emit(ZapretEvent::Error(short));
    }

    manager_release(shared, Some(handle));
    {
        let mut state = shared.state.lock();
        state.current_preset.clear();
        state.server_scope = None;
        state.start_args.clear();
        state.start_retry_count = 0;
        state.current_at_config = None;
        state.launch_mode = "direct";
        state.output_tail.clear();
    }
    shared.emit(ZapretEvent::Stopped);
}

fn stop_on(shared: Arc<Shared>, fast: bool, cleanup: bool, emit_stopped: bool) {
    let handle = {
        let state = shared.state.lock();
        if let Some(handle) = state.process.as_ref() {
            handle.report_exit.store(false, Ordering::SeqCst);
        }
        state.process.clone()
    };
    let Some(handle) = handle else {
        return;
    };

    let still_running = child_exited(&handle).is_none();
    if still_running {
        // winws2 is a windowless console process: terminate() only posts
        // WM_CLOSE, which it never receives, so it used to hold the UI for
        // the whole timeout before the kill. WinDivert releases its filters
        // when the handle closes with the process.
        let _ = handle.child.lock().kill();
        wait_for_process_exit(&handle, if fast { 800 } else { 3000 });
    }

    manager_release(&shared, Some(&handle));
    {
        let mut state = shared.state.lock();
        state.current_preset.clear();
        state.server_scope = None;
        state.start_args.clear();
        state.start_retry_count = 0;
        state.current_at_config = None;
        state.launch_mode = "direct";
        state.output_tail.clear();
    }
    let killed = kill_orphaned(if fast { 1.0 } else { 5.0 }, if fast { 0.0 } else { 1.25 });
    for name in killed {
        shared.emit(ZapretEvent::Log(format!(
            "[zapret] Завершён оставшийся процесс: {name}"
        )));
    }
    if cleanup {
        standard_windivert_cleanup(false);
    }
    if cleanup && !fast {
        shared.state.lock().settle_until = Some(Instant::now() + Duration::from_millis(500));
    }
    if emit_stopped {
        shared.emit(ZapretEvent::Stopped);
    }
}

fn wait_for_process_exit(handle: &ProcessHandle, timeout_ms: u64) -> bool {
    let deadline = Instant::now() + Duration::from_millis(timeout_ms);
    loop {
        if child_exited(handle).is_some() {
            return true;
        }
        if Instant::now() >= deadline {
            return false;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

// ── Orphans and WinDivert services ──────────────────────────────────────────

/// Kill orphaned winws.exe / winws2.exe processes started from ZAPRET_DIR.
///
/// Scoped by image path: winws.exe is the standard zapret binary name, and a
/// name-based taskkill would tear down an unrelated third-party instance.
fn kill_orphaned(timeout: f64, settle_delay: f64) -> Vec<String> {
    let mut killed: Vec<String> = Vec::new();
    #[cfg(windows)]
    for (exe_name, exe_path) in [("winws2.exe", winws2_exe()), ("winws.exe", winws_exe())] {
        if kill_processes_by_path(exe_name, &exe_path, timeout) {
            killed.push(exe_name.to_string());
        }
    }
    if !killed.is_empty() {
        std::thread::sleep(Duration::from_secs_f64(settle_delay.max(0.0)));
    }
    killed
}

/// `subprocess_utils.kill_processes_by_path` compatibility shim: a path/name
/// does not establish process ownership. Runtime managers already
/// terminate/wait on their retained handles. Never kills anything.
fn kill_processes_by_path(_process_name: &str, _executable_path: &Path, _timeout: f64) -> bool {
    false
}

/// Driver services registered from Lumen's own bundled winws2 only.
///
/// WinDivert service names are shared with GoodbyeDPI / ByeDPI / standalone
/// zapret, so they are matched by image path instead of by name.
#[cfg(windows)]
fn find_windivert_services() -> Vec<String> {
    {
        let caches = PRESET_CACHES.lock();
        if let Some(services) = &caches.windivert_services {
            return services.clone();
        }
    }
    let owned_dir = match fs::canonicalize(zapret_dir().join("exe")) {
        Ok(path) => path.to_string_lossy().to_lowercase().replace('/', "\\"),
        Err(_) => zapret_dir()
            .join("exe")
            .to_string_lossy()
            .to_lowercase()
            .replace('/', "\\"),
    };
    let mut found: BTreeSet<String> = BTreeSet::new();
    use winreg::enums::{HKEY_LOCAL_MACHINE, KEY_READ};
    if let Ok(root) = winreg::RegKey::predef(HKEY_LOCAL_MACHINE)
        .open_subkey_with_flags(r"SYSTEM\CurrentControlSet\Services", KEY_READ)
    {
        for name in root.enum_keys() {
            let Ok(name) = name else {
                break;
            };
            let Ok(service_key) = root.open_subkey(&name) else {
                continue;
            };
            let Ok(image_path) = service_key.get_value::<String, _>("ImagePath") else {
                continue;
            };
            let image_text = image_path.to_lowercase().replace('/', "\\");
            if !image_text.contains(&owned_dir) {
                continue;
            }
            if ["monkey64", "monkey32", "windivert"]
                .iter()
                .any(|marker| image_text.contains(marker))
            {
                found.insert(name);
            }
        }
    }
    let services: Vec<String> = found.into_iter().collect();
    PRESET_CACHES.lock().windivert_services = Some(services.clone());
    services
}

#[cfg(not(windows))]
fn find_windivert_services() -> Vec<String> {
    Vec::new()
}

fn standard_windivert_cleanup(force: bool) {
    #[cfg(windows)]
    {
        {
            let caches = PRESET_CACHES.lock();
            if caches.standard_cleanup_done && !force {
                return;
            }
        }
        for service in find_windivert_services() {
            let command: Vec<String> = vec![
                "sc".into(),
                "config".into(),
                service,
                "start=".into(),
                "demand".into(),
            ];
            let _ = run_text_owned(&command, 0.8, false);
        }
        PRESET_CACHES.lock().standard_cleanup_done = true;
        std::thread::sleep(Duration::from_secs_f64(if force { 0.2 } else { 0.05 }));
    }
    #[cfg(not(windows))]
    {
        let _ = force;
    }
}

fn cleanup_windivert(timeout: f64) {
    #[cfg(windows)]
    {
        {
            let mut caches = PRESET_CACHES.lock();
            caches.windivert_services = None;
            caches.standard_cleanup_done = false;
        }
        for service in find_windivert_services() {
            for action in ["stop", "delete"] {
                let command: Vec<String> = vec!["sc".into(), action.into(), service.clone()];
                let _ = run_text_owned(&command, timeout, false);
            }
        }
    }
    #[cfg(not(windows))]
    {
        let _ = timeout;
    }
}

fn looks_like_windivert_conflict(exit_code: i32, lines: &[String]) -> bool {
    if exit_code != 1 {
        return false;
    }
    let text = lines.join("\n").to_lowercase();
    if text.is_empty() {
        return true;
    }
    WINDIVERT_CONFLICT_MARKERS
        .iter()
        .any(|marker| text.contains(marker))
}

/// Human-readable hint for common winws2 exit codes.
fn exit_code_hint(code: i32) -> String {
    match code {
        1 => "общая ошибка (другой экземпляр / не удалось открыть WinDivert)".to_string(),
        2 => "ошибка аргументов командной строки".to_string(),
        3 => "не удалось загрузить WinDivert драйвер (нужны права администратора)".to_string(),
        _ => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(path: &Path, text: &str) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, text).unwrap();
    }

    #[test]
    fn list_presets_cache_tracks_added_files() {
        let builtin = tempfile::tempdir().unwrap();
        let user = tempfile::tempdir().unwrap();
        invalidate_preset_cache();

        write(&builtin.path().join("first.txt"), "--hostlist list.txt\n");
        assert_eq!(list_presets_in(builtin.path(), user.path()), ["first"]);

        write(&builtin.path().join("_hidden.txt"), "--skip\n");
        write(&builtin.path().join("second.txt"), "--filter-tcp=443\n");

        assert_eq!(
            list_presets_in(builtin.path(), user.path()),
            ["first", "second"]
        );
    }

    #[test]
    fn user_presets_shadow_builtins() {
        let builtin = tempfile::tempdir().unwrap();
        let user = tempfile::tempdir().unwrap();
        write(&builtin.path().join("alpha.txt"), "--builtin\n");
        write(&user.path().join("alpha.txt"), "--user\n");
        write(&builtin.path().join("beta.txt"), "--builtin\n");
        let files = preset_files_in(builtin.path(), user.path());
        let lookup: BTreeMap<String, PathBuf> = files.into_iter().collect();
        assert_eq!(lookup["alpha"], user.path().join("alpha.txt"));
        assert_eq!(lookup["beta"], builtin.path().join("beta.txt"));
    }

    #[test]
    fn parse_preset_args_cache_invalidates_after_file_change() {
        let dir = tempfile::tempdir().unwrap();
        let preset = dir.path().join("preset.txt");
        invalidate_preset_cache();

        write(&preset, "--hostlist list.txt\n# ignored\n");
        assert_eq!(parse_preset_args(&preset), ["--hostlist list.txt"]);

        let meta = fs::metadata(&preset).unwrap();
        let (mtime_ns, _) = mtime_size(&meta);
        write(&preset, "--filter-tcp=443\n--dpi-desync=fake\n");
        // Force a different mtime like os.utime in the Python test.
        let new_time =
            UNIX_EPOCH + Duration::from_nanos(mtime_ns + 1_000_000_000) + Duration::from_secs(1);
        let file = fs::OpenOptions::new().write(true).open(&preset).unwrap();
        file.set_modified(new_time).unwrap();

        assert_eq!(
            parse_preset_args(&preset),
            ["--filter-tcp=443", "--dpi-desync=fake"]
        );
    }

    #[test]
    fn inline_flags_split_before_double_dash_tokens() {
        assert_eq!(
            split_launch_line("--filter-tcp=443 --dpi-desync=fake,ipset --blob=tls:@bin/x.bin"),
            [
                "--filter-tcp=443",
                "--dpi-desync=fake,ipset",
                "--blob=tls:@bin/x.bin"
            ]
        );
        assert_eq!(split_launch_line("--blob=a--b"), ["--blob=a--b"]);
        assert_eq!(split_launch_line("single line"), ["single line"]);
        assert_eq!(split_launch_line("   "), Vec::<String>::new());
        assert_eq!(
            split_launch_line("--hostlist=lists/x.txt"),
            ["--hostlist=lists/x.txt"]
        );
        assert_eq!(
            split_launch_line("--wf-tcp-out=443 --new --dpi-desync=fake"),
            ["--wf-tcp-out=443", "--new", "--dpi-desync=fake"]
        );
    }

    #[test]
    fn missing_ipset_base_is_created_from_ipset_all() {
        let root = tempfile::tempdir().unwrap();
        let zapret_root = root.path().join("zapret");
        let lists = zapret_root.join("lists");
        fs::create_dir_all(&lists).unwrap();
        write(&lists.join("ipset-all.txt"), "1.1.1.0/24\n");

        let missing = missing_referenced_files_in(
            &zapret_root,
            &["--ipset=lists/ipset-base.txt".to_string()],
        );

        assert!(missing.is_empty());
        assert_eq!(
            fs::read_to_string(lists.join("ipset-base.txt")).unwrap(),
            "1.1.1.0/24\n"
        );
    }

    #[test]
    fn missing_preset_lists_are_reported_without_creating_placeholders() {
        let root = tempfile::tempdir().unwrap();
        let zapret_root = root.path().join("zapret");
        let lists = zapret_root.join("lists");
        fs::create_dir_all(&lists).unwrap();
        write(&lists.join("custom.txt"), "example.org\n");

        let args: Vec<String> = [
            "--hostlist=lists/cloudflare.txt",
            "--ipset-exclude=lists/ipset-akamai.txt",
            "--hostlist=lists/custom.txt",
        ]
        .iter()
        .map(|arg| arg.to_string())
        .collect();
        let missing = missing_referenced_files_in(&zapret_root, &args);

        assert_eq!(
            missing,
            vec![lists.join("cloudflare.txt"), lists.join("ipset-akamai.txt")]
        );
        assert!(!lists.join("cloudflare.txt").exists());
        assert!(!lists.join("ipset-akamai.txt").exists());
        assert_eq!(
            fs::read_to_string(lists.join("custom.txt")).unwrap(),
            "example.org\n"
        );
    }

    #[test]
    fn missing_preset_list_paths_outside_lists_are_not_created() {
        let root = tempfile::tempdir().unwrap();
        let zapret_root = root.path().join("zapret");
        fs::create_dir_all(zapret_root.join("lists")).unwrap();

        let args: Vec<String> = [
            "--hostlist=../outside.txt",
            "--ipset=lists/nested/inside.txt",
        ]
        .iter()
        .map(|arg| arg.to_string())
        .collect();
        let missing = missing_referenced_files_in(&zapret_root, &args);

        assert_eq!(missing.len(), 2);
        assert!(!root.path().join("outside.txt").exists());
        assert!(!zapret_root.join("lists").join("nested").exists());
    }

    #[test]
    fn missing_bundled_blob_is_detected_before_start() {
        let root = tempfile::tempdir().unwrap();
        let zapret_root = root.path().join("zapret");
        fs::create_dir_all(zapret_root.join("bin")).unwrap();

        let args: Vec<String> = ["--blob=tls:@bin/missing.bin", "--lua-init=@lua/missing.lua"]
            .iter()
            .map(|arg| arg.to_string())
            .collect();
        let missing = missing_referenced_files_in(&zapret_root, &args);

        assert_eq!(
            missing,
            vec![
                zapret_root.join("bin").join("missing.bin"),
                zapret_root.join("lua").join("missing.lua")
            ]
        );
    }

    #[test]
    fn ipset_registration_error_is_not_windivert_conflict() {
        assert!(!looks_like_windivert_conflict(
            1,
            &["failed to register ipset 'lists/ipset-base.txt'".to_string()]
        ));
        assert!(looks_like_windivert_conflict(1, &[]));
        assert!(looks_like_windivert_conflict(
            1,
            &["error opening filter".to_string()]
        ));
        assert!(!looks_like_windivert_conflict(2, &[]));
    }

    #[test]
    fn exit_code_hints_cover_common_codes() {
        assert_eq!(
            exit_code_hint(1),
            "общая ошибка (другой экземпляр / не удалось открыть WinDivert)"
        );
        assert_eq!(exit_code_hint(2), "ошибка аргументов командной строки");
        assert_eq!(
            exit_code_hint(3),
            "не удалось загрузить WinDivert драйвер (нужны права администратора)"
        );
        assert_eq!(exit_code_hint(7), "");
    }

    #[test]
    fn preset_headers_roundtrip_through_save_and_rename() {
        let builtin = tempfile::tempdir().unwrap();
        let user = tempfile::tempdir().unwrap();
        let info = save_preset_in(
            "custom",
            "# Preset: old\n# Modified: 2000-01-01T00:00:00\n--filter-tcp=443\n\n--dpi-desync=fake\n",
            "описание",
            builtin.path(),
            user.path(),
        );
        assert_eq!(info.name, "custom");
        assert_eq!(info.description, "описание");
        assert_eq!(info.arg_count, 2);
        assert!(!info.modified.is_empty());
        let text = fs::read_to_string(user.path().join("custom.txt")).unwrap();
        assert!(text.contains("# Preset: custom"));
        assert!(text.contains("# Description: описание"));
        assert!(text.contains("--filter-tcp=443"));
        assert!(!text.contains("# Modified: 2000-01-01T00:00:00"));

        let renamed = rename_preset_in("custom", "renamed", builtin.path(), user.path());
        let renamed = renamed.expect("rename succeeds");
        assert_eq!(renamed.name, "renamed");
        assert!(!user.path().join("custom.txt").exists());
        assert!(user.path().join("renamed.txt").exists());
        let renamed_text = fs::read_to_string(user.path().join("renamed.txt")).unwrap();
        assert!(renamed_text.contains("# Preset: renamed"));
        assert!(renamed_text.contains("--filter-tcp=443"));
    }

    #[test]
    fn import_preset_resolves_name_conflicts() {
        let builtin = tempfile::tempdir().unwrap();
        let user = tempfile::tempdir().unwrap();
        let source = tempfile::tempdir().unwrap();
        write(&source.path().join("imported.txt"), "--filter-tcp=443\n");
        write(&user.path().join("imported.txt"), "--existing\n");

        let info = import_preset_in(
            &source.path().join("imported.txt"),
            builtin.path(),
            user.path(),
        )
        .expect("import succeeds");
        assert_eq!(info.name, "imported (1)");
        assert!(user.path().join("imported (1).txt").exists());
        assert_eq!(read_preset_content(&info.file_path), "--filter-tcp=443\n");
    }

    fn read_preset_content(path: &Path) -> String {
        fs::read_to_string(path).unwrap()
    }

    #[test]
    fn delete_preset_removes_user_copy() {
        let builtin = tempfile::tempdir().unwrap();
        let user = tempfile::tempdir().unwrap();
        write(&user.path().join("gone.txt"), "--filter-tcp=443\n");
        assert!(delete_preset_in("gone", builtin.path(), user.path()));
        assert!(!user.path().join("gone.txt").exists());
        assert!(!delete_preset_in("gone", builtin.path(), user.path()));
    }

    #[test]
    fn metadata_headers_are_parsed_from_the_first_lines() {
        let text = "# Preset: name\n\n# Description: desc\n--arg\n# Created: never\n";
        let meta = parse_metadata(text);
        assert_eq!(meta.get("Preset").map(String::as_str), Some("name"));
        assert_eq!(meta.get("Description").map(String::as_str), Some("desc"));
        assert!(!meta.contains_key("Created"));
    }

    #[test]
    fn elevation_errors_are_recognized() {
        assert!(is_elevation_launch_error(
            "CreateProcess failed: The requested operation requires elevation. (os error 740)"
        ));
        assert!(is_elevation_launch_error("требует повышения прав"));
        assert!(!is_elevation_launch_error("file not found"));
    }

    #[cfg(windows)]
    #[test]
    fn stop_kills_windowless_winws2_without_blocking() {
        let manager = ZapretManager::new();
        let child = hidden_command("cmd")
            .args(["/c", "ping -n 60 127.0.0.1 > NUL"])
            .stdin(Stdio::null())
            .spawn()
            .expect("test child spawns");
        let handle = Arc::new(ProcessHandle {
            child: Mutex::new(child),
            report_exit: AtomicBool::new(true),
        });
        manager.shared.state.lock().process = Some(Arc::clone(&handle));
        assert!(manager.running());

        manager.stop();

        assert!(manager.shared.state.lock().process.is_none());
        assert!(!manager.running());
        // The child was killed promptly (terminate() never appears here).
        let exited = handle.child.lock().try_wait();
        assert!(matches!(exited, Ok(Some(_))));
        // cleanup without fast sets the WinDivert settle window.
        assert!(manager.shared.state.lock().settle_until.is_some());
    }

    #[test]
    fn at_config_quotes_and_hashes_stably() {
        let dir = tempfile::tempdir().unwrap();
        let preset = dir.path().join("preset.txt");
        write(&preset, "--filter-tcp=443\n");
        let args = vec![
            "--filter-tcp=443".to_string(),
            "--hostlist=a b.txt".to_string(),
        ];
        let path = write_at_config(&preset, &args);
        assert!(
            path.file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("winws2_at_")
        );
        let text = fs::read_to_string(&path).unwrap();
        assert_eq!(text, "--filter-tcp=443\n'--hostlist=a b.txt'\n");
        // Same inputs land on the same cached file.
        assert_eq!(write_at_config(&preset, &args), path);
    }

    #[test]
    fn shlex_quote_matches_python() {
        assert_eq!(shlex_quote("simple"), "simple");
        assert_eq!(shlex_quote(""), "''");
        assert_eq!(shlex_quote("a b"), "'a b'");
        assert_eq!(shlex_quote("it's"), "'it'\"'\"'s'");
        assert_eq!(shlex_quote("--flag=1"), "--flag=1");
    }
}
