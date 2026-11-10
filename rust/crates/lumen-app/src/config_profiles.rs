//! Normalize profile paths and read or write the Windows JSON editor format.

use std::fs;
use std::io::{self, Write};
use std::path::{Component, Path, PathBuf};

use anyhow::{Result, ensure};
use lumen_core::config::json_document::format_json_error_message;
use serde_json::Value;

pub fn normalize_relative_json_path(value: Option<&Path>, default_name: &str) -> PathBuf {
    let raw = value
        .map(|value| value.to_string_lossy())
        .unwrap_or_default();
    let normalized = raw.trim().replace('\\', "/");
    let mut path = PathBuf::new();
    for part in Path::new(&normalized).components() {
        match part {
            Component::CurDir | Component::ParentDir => {}
            Component::RootDir if !cfg!(windows) => {}
            part => path.push(part),
        }
    }
    if path.as_os_str().is_empty() {
        path.push(default_name);
    }
    with_default_extension(path)
}

fn with_default_extension(mut path: PathBuf) -> PathBuf {
    if path
        .extension()
        .is_none_or(|extension| extension.is_empty())
        && let Some(name) = path.file_name()
    {
        let mut name = name.to_owned();
        name.push(".json");
        path.set_file_name(name);
    }
    path
}

/// Resolve an existing prefix, including junctions and symlinks, even when
/// the profile file itself has not been created yet.
fn resolve_existing_prefix(path: &Path) -> io::Result<PathBuf> {
    let absolute = std::path::absolute(path)?;
    match absolute.canonicalize() {
        Ok(path) => return Ok(path),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(error),
    }
    let mut resolved = PathBuf::new();
    for component in absolute.components() {
        match component {
            Component::ParentDir => {
                resolved.pop();
            }
            Component::Normal(_) => {
                resolved.push(component);
                match fs::symlink_metadata(&resolved) {
                    Ok(_) => resolved = resolved.canonicalize()?,
                    Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                    Err(error) => return Err(error),
                }
            }
            _ => resolved.push(component),
        }
    }
    Ok(resolved)
}

pub fn resolve_profile_path(
    base: &Path,
    value: Option<&Path>,
    default_name: &str,
    label: &str,
) -> Result<PathBuf> {
    let base = resolve_existing_prefix(base)?;
    let candidate = match value {
        Some(path) if path.is_absolute() => path.to_path_buf(),
        _ => base.join(normalize_relative_json_path(value, default_name)),
    };
    let outside = || {
        format!(
            "Файл {label} должен находиться в {}/",
            relative_display(&base)
        )
    };
    // A colon in a Windows component denotes a stream, not a profile file.
    ensure!(
        !cfg!(windows)
            || !candidate.components().any(|part| matches!(
                part, Component::Normal(name) if name.to_string_lossy().contains(':')
            )),
        outside()
    );
    let resolved = resolve_existing_prefix(&candidate)?;
    let resolved = resolve_existing_prefix(&with_default_extension(resolved))?;
    ensure!(resolved != base && resolved.starts_with(&base), outside());
    Ok(resolved)
}

pub(crate) fn relative_display(path: &Path) -> String {
    let display = path.to_string_lossy().replace('\\', "/");
    if let Some(unc) = display.strip_prefix("//?/UNC/") {
        format!("//{unc}")
    } else {
        display.strip_prefix("//?/").unwrap_or(&display).to_owned()
    }
}

pub(crate) fn read_profile_text(path: &Path) -> io::Result<String> {
    let text = fs::read_to_string(path)?;
    if text.contains('\r') {
        Ok(text.replace("\r\n", "\n").replace('\r', "\n"))
    } else {
        Ok(text)
    }
}

pub(crate) fn write_profile_text(path: &Path, text: &str) -> io::Result<()> {
    let directory = path
        .parent()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "missing profile directory"))?;
    fs::create_dir_all(directory)?;
    let mut staged = tempfile::NamedTempFile::new_in(directory)?;
    if cfg!(windows) {
        staged.write_all(text.replace('\n', "\r\n").as_bytes())?;
    } else {
        staged.write_all(text.as_bytes())?;
    }
    staged.persist(path)?;
    Ok(())
}

pub(crate) fn default_template_text(
    base: &Path,
    default_name: &str,
    label: &str,
) -> Result<String> {
    let path = resolve_profile_path(base, None, default_name, label)?;
    ensure!(
        path.is_file(),
        "Не найден шаблон {label}: {}",
        relative_display(&path)
    );
    Ok(read_profile_text(&path)?)
}

pub fn validate_json_text(text: &str) -> (bool, String) {
    match serde_json::from_str::<Value>(text) {
        Ok(_) => (true, "JSON корректен.".into()),
        Err(error) => (false, format_json_error_message(text, &error)),
    }
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;

    #[test]
    fn rejects_a_missing_profile_below_a_junction_pointing_outside() -> Result<()> {
        let temp = tempfile::tempdir()?;
        let profiles = temp.path().join("profiles");
        let outside = temp.path().join("outside");
        fs::create_dir_all(&profiles)?;
        fs::create_dir_all(&outside)?;
        let junction = profiles.join("junction");
        let output = lumen_platform::subprocess::hidden_command("powershell")
            .args(["-NoProfile", "-NonInteractive", "-Command",
                   "New-Item -ItemType Junction -Path $env:LUMEN_TEST_JUNCTION -Target $env:LUMEN_TEST_TARGET -ErrorAction Stop | Out-Null"])
            .env("LUMEN_TEST_JUNCTION", &junction)
            .env("LUMEN_TEST_TARGET", &outside)
            .output()?;
        assert!(
            output.status.success(),
            "{}",
            lumen_platform::decode_output(&output.stderr)
        );
        let error = resolve_profile_path(
            &profiles,
            Some(Path::new("junction/missing")),
            "default.json",
            "xray",
        )
        .unwrap_err();
        assert!(
            error.to_string().contains("Файл xray должен находиться в"),
            "{error}"
        );
        Ok(())
    }
}
