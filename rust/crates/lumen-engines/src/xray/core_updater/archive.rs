use std::fs::{self, File};
use std::io::{self, Read, Seek};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail, ensure};
use lumen_core::component_compatibility::ensure_component_compatible;
use lumen_core::zip_utils::safe_extract_zip;
use sha2::{Digest, Sha256};
use tempfile::{Builder, TempDir};
use zip::ZipArchive;

use super::UpdateCancellation;
use super::release::extract_version;
use crate::xray::manager::get_xray_version;

pub fn sha256_file(path: &Path, cancellation: &UpdateCancellation) -> Result<String> {
    let mut file = File::open(path)?;
    let mut digest = Sha256::new();
    let mut buffer = vec![0; 1024 * 1024];
    loop {
        cancellation.check()?;
        let read = file.read(&mut buffer)?;
        if read == 0 {
            return Ok(format!("{:x}", digest.finalize()));
        }
        digest.update(&buffer[..read]);
    }
}

pub(super) struct ExtractedArchive {
    _directory: TempDir,
    xray: PathBuf,
    wintun: Option<PathBuf>,
}

impl ExtractedArchive {
    pub(super) fn open<R: Read + Seek>(reader: R) -> Result<Self> {
        let directory = Builder::new().prefix("xray_core_extract_").tempdir()?;
        let mut archive = ZipArchive::new(reader)?;
        safe_extract_zip(&mut archive, directory.path())?;
        let xray =
            find_file(directory.path(), "xray.exe")?.context("xray.exe не найден в архиве")?;
        let wintun = find_file(directory.path(), "wintun.dll")?;
        Ok(Self {
            _directory: directory,
            xray,
            wintun,
        })
    }

    pub(super) fn validate(&self, expected_version: &str) -> Result<()> {
        let version =
            extract_version(&get_xray_version(&self.xray.to_string_lossy()).unwrap_or_default());
        ensure_component_compatible("xray", &version)?;
        ensure!(
            expected_version.is_empty() || extract_version(expected_version) == version,
            "архив содержит Xray {version}, ожидалась версия {expected_version}"
        );
        Ok(())
    }

    pub(super) fn install(&self, target: &Path) -> Result<()> {
        let directory = target
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        self.stage(directory, target)?.commit()
    }

    pub(super) fn stage(&self, directory: &Path, target: &Path) -> Result<StagedInstall> {
        fs::create_dir_all(directory)?;
        let mut stage = StagedInstall::new(directory)?;
        let original = stage.add(&self.xray, target.to_path_buf())?;
        // Lumen manages geoip.dat/geosite.dat independently of the core.
        if let Some(wintun) = &self.wintun {
            stage.add(wintun, directory.join("wintun.dll"))?;
        }
        if let Some(original) = original {
            stage.add(&original, target.with_extension("exe.bak"))?;
        }
        Ok(stage)
    }
}

fn find_file(directory: &Path, name: &str) -> io::Result<Option<PathBuf>> {
    let mut subdirs = Vec::new();
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        let kind = entry.file_type()?;
        if kind.is_file()
            && entry
                .file_name()
                .to_string_lossy()
                .eq_ignore_ascii_case(name)
        {
            return Ok(Some(entry.path()));
        }
        if kind.is_dir() {
            subdirs.push(entry.path());
        }
    }
    for subdir in subdirs {
        if let Some(path) = find_file(&subdir, name)? {
            return Ok(Some(path));
        }
    }
    Ok(None)
}

struct StagedFile {
    destination: PathBuf,
    replacement: PathBuf,
    backup: Option<PathBuf>,
}

pub(super) struct StagedInstall {
    directory: TempDir,
    files: Vec<StagedFile>,
}

impl StagedInstall {
    fn new(parent: &Path) -> Result<Self> {
        Ok(Self {
            directory: Builder::new().prefix(".xray_install_").tempdir_in(parent)?,
            files: Vec::new(),
        })
    }

    fn add(&mut self, source: &Path, destination: PathBuf) -> Result<Option<PathBuf>> {
        let index = self.files.len();
        let replacement = self.directory.path().join(format!("new-{index}"));
        fs::copy(source, &replacement)?;
        let backup = match fs::symlink_metadata(&destination) {
            Ok(metadata) => {
                ensure!(
                    metadata.is_file(),
                    "ожидался файл: {}",
                    destination.display()
                );
                let backup = self.directory.path().join(format!("old-{index}"));
                fs::copy(&destination, &backup)?;
                Some(backup)
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => None,
            Err(error) => return Err(error.into()),
        };
        self.files.push(StagedFile {
            destination,
            replacement,
            backup: backup.clone(),
        });
        Ok(backup)
    }

    pub(super) fn commit(self) -> Result<()> {
        for (index, file) in self.files.iter().enumerate() {
            if let Err(error) = fs::rename(&file.replacement, &file.destination) {
                let failure = format!("{}: {error}", file.destination.display());
                let mut rollback_errors = Vec::new();
                for previous in self.files[..index].iter().rev() {
                    let restored = match &previous.backup {
                        Some(backup) => fs::rename(backup, &previous.destination),
                        None => fs::remove_file(&previous.destination),
                    };
                    if let Err(error) = restored {
                        rollback_errors
                            .push(format!("{}: {error}", previous.destination.display()));
                    }
                }
                if !rollback_errors.is_empty() {
                    let preserved = self.directory.keep();
                    bail!(
                        "{failure}. Ошибка отката: {}. Резервные копии: {}",
                        rollback_errors.join(", "),
                        preserved.display()
                    );
                }
                bail!("{failure}");
            }
        }
        Ok(())
    }
}
