//! Extract ZIP contents after checking all member destinations.

use std::fs::{self, File, Metadata};
use std::io::{self, Read, Seek};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, ensure};
use zip::ZipArchive;

pub fn safe_extract_zip<R: Read + Seek>(archive: &mut ZipArchive<R>, target: &Path) -> Result<()> {
    fs::create_dir_all(target)?;
    let root = target.canonicalize()?;
    let mut members = Vec::with_capacity(archive.len());
    for index in 0..archive.len() {
        let member = archive.by_index(index)?;
        let name = member.name();
        let path = member_path(name).with_context(|| format!("unsafe zip member path: {name}"))?;
        let mut dest = root.clone();
        for part in path.components() {
            dest.push(part);
            match fs::symlink_metadata(&dest) {
                Ok(metadata) => ensure!(!is_link(&metadata), "unsafe zip member path: {name}"),
                Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                Err(error) => return Err(error.into()),
            }
        }
        members.push((dest, member.is_dir() || name.ends_with('\\')));
    }

    for (index, (destination, is_dir)) in members.into_iter().enumerate() {
        if is_dir {
            fs::create_dir_all(destination)?;
        } else {
            if let Some(parent) = destination.parent() {
                fs::create_dir_all(parent)?;
            }
            // Python zipfile extracts link entries as ordinary files.
            io::copy(
                &mut archive.by_index(index)?,
                &mut File::create(destination)?,
            )?;
        }
    }
    Ok(())
}

fn member_path(name: &str) -> Option<PathBuf> {
    if name.starts_with(['/', '\\']) {
        return None;
    }
    let mut path = PathBuf::new();
    for component in name
        .split(['/', '\\'])
        .filter(|part| !matches!(*part, "" | "."))
    {
        if component.ends_with([' ', '.'])
            || component.contains(['<', '>', ':', '"', '|', '?', '*'])
            || component.chars().any(|ch| ch <= '\u{1f}')
            || is_dos_device(component)
        {
            return None;
        }
        path.push(component);
    }
    (!path.as_os_str().is_empty()).then_some(path)
}

fn is_dos_device(component: &str) -> bool {
    let stem = component
        .split('.')
        .next()
        .unwrap_or_default()
        .trim_end()
        .to_ascii_uppercase();
    matches!(
        stem.as_str(),
        "CON" | "PRN" | "AUX" | "NUL" | "CONIN$" | "CONOUT$"
    ) || stem
        .strip_prefix("COM")
        .or_else(|| stem.strip_prefix("LPT"))
        .is_some_and(|suffix| {
            matches!(
                suffix,
                "0" | "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" | "¹" | "²" | "³"
            )
        })
}

fn is_link(metadata: &Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        metadata.file_attributes() & 0x400 != 0 // FILE_ATTRIBUTE_REPARSE_POINT
    }
    #[cfg(not(windows))]
    {
        metadata.file_type().is_symlink()
    }
}

#[cfg(test)]
mod tests {
    use std::io::{Cursor, Write};

    use super::*;
    use zip::write::SimpleFileOptions;

    fn archive(names: &[&str]) -> anyhow::Result<ZipArchive<Cursor<Vec<u8>>>> {
        let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
        for name in names {
            writer.start_file(*name, SimpleFileOptions::default())?;
            writer.write_all(name.as_bytes())?;
        }
        Ok(ZipArchive::new(writer.finish()?)?)
    }

    #[test]
    fn extracts_relative_entries_with_windows_separators() -> anyhow::Result<()> {
        let dir = tempfile::tempdir()?;
        safe_extract_zip(
            &mut archive(&[r"nested\xray.exe", "wintun.dll"])?,
            dir.path(),
        )?;
        assert_eq!(
            std::fs::read(dir.path().join("nested").join("xray.exe"))?,
            br"nested\xray.exe"
        );
        Ok(())
    }

    #[test]
    fn preflights_every_entry_before_writing_any_files() -> anyhow::Result<()> {
        for unsafe_name in [
            "safe/../escaped",
            r"..\escape",
            "C:/xray.exe",
            "/root",
            "file:stream",
            "NUL.exe",
        ] {
            let dir = tempfile::tempdir()?;
            let error = safe_extract_zip(&mut archive(&["good.txt", unsafe_name])?, dir.path())
                .unwrap_err();
            assert!(error.to_string().contains("unsafe zip member path"));
            assert!(!dir.path().join("good.txt").exists());
        }
        Ok(())
    }
}
