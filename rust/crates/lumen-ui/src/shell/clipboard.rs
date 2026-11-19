//! Native clipboard access for explicit paste actions.

/// Read plain text from the system clipboard.
pub(crate) fn read_text() -> Option<String> {
    let mut clipboard = arboard::Clipboard::new().ok()?;
    clipboard.get_text().ok()
}
