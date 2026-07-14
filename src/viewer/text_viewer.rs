//! Text viewer for archive-bundled documents (readme, terms of use, etc.).
//!
//! `TextViewerModel` holds the list of text files found in the most recently
//! listed archive plus the documents the user opened from that list. It is
//! shared from `ViewerApp` as `SharedTextViewer = Arc<Mutex<TextViewerModel>>`
//! because each open document renders in its own deferred viewport (a separate
//! OS window, same pattern as `log_viewer`) whose closure cannot borrow
//! `&mut ViewerApp`.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use eframe::egui;
use rust_i18n::t;

/// Model shared between `ViewerApp` and the `show_viewport_deferred` closures.
pub type SharedTextViewer = Arc<Mutex<TextViewerModel>>;

/// Cap on characters kept per opened document (guards the UI against
/// pathological multi-MB text files; the extraction side has its own cap).
const MAX_DISPLAY_CHARS: usize = 1_000_000;

/// One text file found in an archive. Raw bytes are kept as extracted and
/// decoded lazily when the user opens the file.
pub struct TextFileEntry {
    /// Internal archive path (unique key within the list).
    pub path: PathBuf,
    /// File name used for list display and window titles.
    pub name: String,
    pub data: Vec<u8>,
}

/// One opened document (backing state for a text-viewer window).
pub struct TextDoc {
    /// Internal archive path (also the viewport-id seed).
    pub path: PathBuf,
    pub title: String,
    pub content: String,
    /// True when `content` was cut off at `MAX_DISPLAY_CHARS`.
    pub truncated: bool,
    /// Cleared when the user closes the window; the parent then stops
    /// registering the viewport on its next frame.
    pub open: bool,
}

/// Core state of the text viewer. Shared via `Arc<Mutex<_>>`.
#[derive(Default)]
pub struct TextViewerModel {
    /// Text files found in the current archive (list-window contents).
    pub files: Vec<TextFileEntry>,
    /// Documents opened from the list (one viewport each while `open`).
    pub docs: Vec<TextDoc>,
    /// Whether the file-list window is shown.
    pub list_visible: bool,
}

impl TextViewerModel {
    /// Replace the file list (fresh archive listing). Open documents are
    /// closed because they belong to the previous archive.
    pub fn replace_files(&mut self, files: Vec<TextFileEntry>) {
        self.files = files;
        self.docs.clear();
        if self.files.is_empty() {
            self.list_visible = false;
        }
    }

    /// Merge additional files (append load). Entries whose path is already
    /// listed are skipped; open documents stay open.
    pub fn extend_files(&mut self, files: Vec<TextFileEntry>) {
        for f in files {
            if !self.files.iter().any(|e| e.path == f.path) {
                self.files.push(f);
            }
        }
    }

    /// Open the document for `files[index]`, decoding it on first open.
    /// Returns the viewport id of the (existing or new) document window so the
    /// caller can focus it.
    pub fn open_doc(&mut self, index: usize) -> Option<egui::ViewportId> {
        let file = self.files.get(index)?;
        let vp_id = doc_viewport_id(&file.path);
        if let Some(doc) = self.docs.iter_mut().find(|d| d.path == file.path) {
            doc.open = true;
            return Some(vp_id);
        }
        let decoded = crate::archive::decode_text_bytes(&file.data);
        let (content, truncated) = match decoded.char_indices().nth(MAX_DISPLAY_CHARS) {
            Some((byte_idx, _)) => (decoded[..byte_idx].to_string(), true),
            None => (decoded, false),
        };
        self.docs.push(TextDoc {
            path: file.path.clone(),
            title: file.name.clone(),
            content,
            truncated,
            open: true,
        });
        Some(vp_id)
    }

    /// Render the clickable file list. Returns the index clicked, if any.
    /// Shared by the list window and the archive model-selection dialog.
    pub fn list_ui(&self, ui: &mut egui::Ui) -> Option<usize> {
        let mut clicked = None;
        for (i, f) in self.files.iter().enumerate() {
            let opened = self.docs.iter().any(|d| d.open && d.path == f.path);
            let label = if opened {
                format!("📖 {}", f.path.display())
            } else {
                format!("📄 {}", f.path.display())
            };
            if ui
                .button(label)
                .on_hover_text(t!("viewer.text_viewer.open_hover"))
                .clicked()
            {
                clicked = Some(i);
            }
        }
        clicked
    }
}

/// Stable viewport id for one document window.
pub fn doc_viewport_id(path: &std::path::Path) -> egui::ViewportId {
    egui::ViewportId::from_hash_of(("popone_text_doc", path))
}

/// Draw one document's window body (called inside its deferred viewport).
pub fn draw_doc_window(child_ctx: &egui::Context, doc: &mut TextDoc) {
    egui::CentralPanel::default().show(child_ctx, |ui| {
        if doc.truncated {
            ui.colored_label(
                egui::Color32::from_rgb(0xE0, 0xA0, 0x40),
                t!("viewer.text_viewer.truncated"),
            );
            ui.separator();
        }
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                // Immutable `&str` buffer: selectable / copyable but read-only.
                let mut text = doc.content.as_str();
                ui.add_sized(
                    ui.available_size(),
                    egui::TextEdit::multiline(&mut text).desired_width(f32::INFINITY),
                );
            });
    });
    if child_ctx.input(|i| i.viewport().close_requested()) {
        doc.open = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(path: &str, data: &[u8]) -> TextFileEntry {
        TextFileEntry {
            path: PathBuf::from(path),
            name: PathBuf::from(path)
                .file_name()
                .unwrap()
                .to_string_lossy()
                .into_owned(),
            data: data.to_vec(),
        }
    }

    #[test]
    fn test_open_doc_decodes_and_dedupes() {
        let mut m = TextViewerModel::default();
        m.replace_files(vec![entry("readme.txt", "日本語".as_bytes())]);
        let id1 = m.open_doc(0).unwrap();
        assert_eq!(m.docs.len(), 1);
        assert_eq!(m.docs[0].content, "日本語");
        assert!(m.docs[0].open);

        // Re-opening the same file reuses the existing doc (same viewport id).
        m.docs[0].open = false;
        let id2 = m.open_doc(0).unwrap();
        assert_eq!(id1, id2);
        assert_eq!(m.docs.len(), 1);
        assert!(m.docs[0].open);
    }

    #[test]
    fn test_replace_closes_docs_extend_keeps() {
        let mut m = TextViewerModel::default();
        m.replace_files(vec![entry("a.txt", b"a")]);
        m.open_doc(0);
        assert_eq!(m.docs.len(), 1);

        // Append merge: duplicate paths are skipped, docs stay.
        m.extend_files(vec![entry("a.txt", b"dup"), entry("b.txt", b"b")]);
        assert_eq!(m.files.len(), 2);
        assert_eq!(m.docs.len(), 1);

        // Fresh listing: docs are dropped.
        m.replace_files(vec![entry("c.txt", b"c")]);
        assert!(m.docs.is_empty());
        assert_eq!(m.files.len(), 1);

        // Empty listing also hides the list window.
        m.list_visible = true;
        m.replace_files(Vec::new());
        assert!(!m.list_visible);
    }
}
