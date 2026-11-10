//! Cache the active sing-box editor document by path, mtime and file size.

use std::fs;
use std::io;
use std::path::Path;

use lumen_engines::singbox::document::{SingboxDocumentState, inspect_singbox_document_text};

use crate::config_profiles::read_profile_text;

#[derive(Default)]
pub struct SingboxDocumentCache {
    state: Option<SingboxDocumentState>,
}

impl SingboxDocumentCache {
    pub fn clear(&mut self) {
        self.state = None;
    }

    pub fn cache_state(&mut self, path: &Path, text: &str) -> &SingboxDocumentState {
        let mut state = inspect_singbox_document_text(path, text);
        if let Ok(metadata) = fs::metadata(path) {
            state.file_modified = metadata.modified().ok();
            state.file_size = Some(metadata.len());
        }
        self.state.insert(state)
    }

    pub fn get_state(&mut self, path: &Path) -> io::Result<&SingboxDocumentState> {
        let cached = self.state.as_ref().is_some_and(|state| {
            state.source_path == path
                && fs::metadata(path).is_ok_and(|metadata| {
                    metadata
                        .modified()
                        .ok()
                        .is_some_and(|mtime| Some(mtime) == state.file_modified)
                        && Some(metadata.len()) == state.file_size
                })
        });
        if cached {
            return Ok(self.state.as_ref().expect("cache checked"));
        }
        let text = read_profile_text(path)?;
        Ok(self.cache_state(path, &text))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::Result;
    use lumen_engines::singbox::document::parse_singbox_document;

    #[test]
    fn refreshes_changed_editor_files_and_keeps_inspection_tolerant() -> Result<()> {
        let root = tempfile::tempdir()?;
        let path = root.path().join("profile.json");
        fs::write(&path, "{\"outbounds\":[{\"tag\":\"proxy\"}]}\r\n")?;
        let mut cache = SingboxDocumentCache::default();
        let first = cache.get_state(&path)?.clone();
        assert!(first.has_proxy_outbound);
        assert!(first.text.ends_with("}\n"));
        assert_eq!(
            parse_singbox_document(&path, &first.text)?.text_hash,
            first.text_hash
        );

        fs::write(&path, "[")?;
        let changed = cache.get_state(&path)?;
        assert_eq!(changed.text, "[");
        assert!(!changed.has_proxy_outbound);
        assert_ne!(changed.text_hash, first.text_hash);
        let error = parse_singbox_document(&path, &changed.text)
            .err()
            .expect("invalid JSON");
        assert!(
            error
                .to_string()
                .starts_with("profile.json: Ошибка синтаксиса JSON")
        );
        Ok(())
    }
}
