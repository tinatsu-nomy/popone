//! Log viewer that runs in a separate window.
//!
//! `LogViewerModel` reads incremental log data from `SharedLogBuffer` and keeps
//! parsed lines in a ring buffer for UI rendering. It is shared from `ViewerApp`
//! as `SharedLogViewer = Arc<Mutex<LogViewerModel>>` and `Arc::clone`'d into the
//! `show_viewport_deferred` closure.
//!
//! Phase 1 scope: type definitions, parser, `ingest` / `poll`, and their tests.
//! UI rendering and persistence are added in Phase 2 / 3.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use eframe::egui;
use rust_i18n::t;

use crate::SharedLogBuffer;

/// Upper bound on parsed log lines kept in memory.
pub const LINE_LIMIT: usize = 20_000;

/// Model shared between `ViewerApp` and the `show_viewport_deferred` closure.
pub type SharedLogViewer = Arc<Mutex<LogViewerModel>>;

/// Log level (used for UI filters and color coding).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogLevel {
    Error,
    Warn,
    Info,
    Debug,
    Trace,
    /// Header `[ts][LEVEL] msg` matched but the level string is unknown (e.g. FATAL).
    Unknown,
}

impl LogLevel {
    fn from_level_str(s: &str) -> Self {
        match s {
            "ERROR" => Self::Error,
            "WARN" => Self::Warn,
            "INFO" => Self::Info,
            "DEBUG" => Self::Debug,
            "TRACE" => Self::Trace,
            _ => Self::Unknown,
        }
    }
}

/// One parsed log line. `message` is multi-line concatenated (backtraces etc. kept with `\n`).
#[derive(Debug, Clone)]
pub struct LogLine {
    pub level: LogLevel,
    pub timestamp: String,
    pub message: String,
}

/// Per-level visibility flags. Default: only Debug is OFF.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LevelFilters {
    pub show_error: bool,
    pub show_warn: bool,
    pub show_info: bool,
    pub show_debug: bool,
}

impl Default for LevelFilters {
    fn default() -> Self {
        Self {
            show_error: true,
            show_warn: true,
            show_info: true,
            show_debug: false,
        }
    }
}

impl LevelFilters {
    /// Whether the given level passes the filter. Unknown is always shown; Trace tracks the Debug flag.
    pub fn matches(&self, level: LogLevel) -> bool {
        match level {
            LogLevel::Error => self.show_error,
            LogLevel::Warn => self.show_warn,
            LogLevel::Info => self.show_info,
            LogLevel::Debug => self.show_debug,
            LogLevel::Trace => self.show_debug,
            LogLevel::Unknown => true,
        }
    }
}

/// Core state of the log viewer. Shared via `Arc<Mutex<_>>`.
pub struct LogViewerModel {
    /// Whether the log viewer window is visible.
    pub visible: bool,
    /// Last read offset into `SharedLogBuffer::total_written`.
    last_offset: usize,
    /// Ring buffer of parsed log lines (cap = `LINE_LIMIT`).
    pub lines: VecDeque<LogLine>,
    /// Indices into `lines` that pass the filter (for virtualized scrolling).
    pub filter_indices: Vec<usize>,
    /// Set to true when a trim happens or filters change -> forces a `rebuild_filter_indices` next call.
    filters_dirty: bool,
    /// Per-level visibility flags.
    pub filters: LevelFilters,
    /// Auto-follow (stick to bottom).
    pub follow_tail: bool,
    /// Position / size to apply via `ViewportBuilder` next frame (cleared after apply).
    /// Used for both startup restore and same-session reopen position restore.
    pub apply_geometry: Option<([f32; 2], [f32; 2])>,
    /// Latest geometry read from the child viewport every frame (used by on_exit persistence).
    pub last_geometry: Option<([f32; 2], [f32; 2])>,
    /// Trailing fragment without a `\n` (prepended to the next ingest call).
    tail_buffer: String,
    /// True until the first `[HEADER]` is seen. The byte-level drain in `LogBuffer` may
    /// produce a leading fragment with a missing prefix, and this flag is used to discard it.
    seeking_first_header: bool,
}

impl Default for LogViewerModel {
    fn default() -> Self {
        Self {
            visible: false,
            last_offset: 0,
            lines: VecDeque::new(),
            filter_indices: Vec::new(),
            filters_dirty: false,
            filters: LevelFilters::default(),
            follow_tail: true,
            apply_geometry: None,
            last_geometry: None,
            tail_buffer: String::new(),
            seeking_first_header: true,
        }
    }
}

impl LogViewerModel {
    /// Read incremental data from `SharedLogBuffer` and forward it to `ingest`.
    ///
    /// The lock is held only long enough to call `read_from_offset`. Do not call
    /// while UI is being drawn (so that `log::info!` callers are not blocked).
    pub fn poll(&mut self, log_buffer: &SharedLogBuffer) {
        let new_text = {
            let lb = match log_buffer.lock() {
                Ok(g) => g,
                // Resilient against panics in other threads: read even when poisoned.
                Err(p) => p.into_inner(),
            };
            let text = lb.read_from_offset(self.last_offset);
            self.last_offset = lb.total_written;
            text
        };
        if let Some(text) = new_text {
            self.ingest(&text);
        }
    }

    /// Parse raw log text (multi-line) and append to `lines`.
    ///
    /// - A trailing fragment without `\n` is carried over via `tail_buffer` and prepended next call.
    /// - While `seeking_first_header` is true, lines are dropped until a `[HEADER]` shape is seen.
    /// - A line not starting with `[`, when a previous `LogLine` exists, is appended to its message as multiline.
    /// - When `LINE_LIMIT` is exceeded, the front is drained and `filter_indices` is fully rebuilt.
    pub fn ingest(&mut self, text: &str) {
        if text.is_empty() {
            return;
        }

        // Concatenate the previous tail_buffer with the new text and split on \n.
        let mut combined = std::mem::take(&mut self.tail_buffer);
        combined.push_str(text);

        let mut parts: Vec<&str> = combined.split('\n').collect();
        // The final element is the carried-over fragment (empty if the text ended in \n).
        let new_tail = parts.pop().unwrap_or("").to_string();

        for raw_line in parts {
            // Handle CRLF.
            let line = raw_line.strip_suffix('\r').unwrap_or(raw_line);

            if let Some((level, ts, msg)) = parse_line_header(line) {
                // Canonical header detected -> push as a new LogLine.
                self.seeking_first_header = false;
                self.push_line(LogLine {
                    level,
                    timestamp: ts.to_string(),
                    message: msg.to_string(),
                });
            } else if self.seeking_first_header {
                // Initial header not yet found -> discard the leading fragment (likely truncated by byte-level drain).
                continue;
            } else if let Some(last) = self.lines.back_mut() {
                // Continuation of the previous line (multi-line message, backtrace, etc.).
                last.message.push('\n');
                last.message.push_str(line);
            } else {
                // Rare: no previous line but seeking_first_header is false.
                // Treat as an Unknown standalone line so we do not drop information.
                self.push_line(LogLine {
                    level: LogLevel::Unknown,
                    timestamp: String::new(),
                    message: line.to_string(),
                });
            }
        }

        self.tail_buffer = new_tail;

        // When the cap is exceeded, drain from the front and rebuild filter_indices.
        let len = self.lines.len();
        if len > LINE_LIMIT {
            for _ in 0..(len - LINE_LIMIT) {
                self.lines.pop_front();
            }
            self.filters_dirty = true;
        }
        if self.filters_dirty {
            self.rebuild_filter_indices();
            self.filters_dirty = false;
        }
    }

    /// Push a LogLine; if it passes the filter, update `filter_indices` incrementally.
    fn push_line(&mut self, line: LogLine) {
        let matches = self.filters.matches(line.level);
        self.lines.push_back(line);
        if matches {
            self.filter_indices.push(self.lines.len() - 1);
        }
    }

    /// Fully rebuild the filter index. Called after a trim or after filters change.
    fn rebuild_filter_indices(&mut self) {
        self.filter_indices.clear();
        self.filter_indices
            .extend(self.lines.iter().enumerate().filter_map(|(i, line)| {
                if self.filters.matches(line.level) {
                    Some(i)
                } else {
                    None
                }
            }));
    }

    /// Replace the level filter and rebuild the index immediately.
    pub fn set_filters(&mut self, filters: LevelFilters) {
        self.filters = filters;
        self.rebuild_filter_indices();
        self.filters_dirty = false;
    }

    /// Initialize from the `LogViewerConfig` loaded out of `popone.toml`.
    ///
    /// `apply_geometry` is set only when both position and size are Some, so it is
    /// passed to `ViewportBuilder` next frame. A half-set state is treated as default.
    ///
    /// `last_geometry` is initialized from the same value. Without this, a session
    /// where the viewer is never opened would lose the configured position when
    /// `export_config` runs (P2 fix).
    pub fn from_config(cfg: &crate::viewer::app::persistence::LogViewerConfig) -> Self {
        // Position / size are persisted as four scalar fields. Adopt the geometry only
        // when all four are Some; if any is missing, fall back to defaults.
        let geometry = match (cfg.x, cfg.y, cfg.width, cfg.height) {
            (Some(x), Some(y), Some(w), Some(h)) => Some(([x, y], [w, h])),
            _ => None,
        };
        Self {
            visible: cfg.visible,
            filters: LevelFilters {
                show_error: cfg.show_error,
                show_warn: cfg.show_warn,
                show_info: cfg.show_info,
                show_debug: cfg.show_debug,
            },
            follow_tail: cfg.follow_tail,
            apply_geometry: geometry,
            last_geometry: geometry,
            ..Self::default()
        }
    }

    /// Serialize the current state into `LogViewerConfig`. Called from `ViewerApp::on_exit`.
    ///
    /// Position / size come from `last_geometry`. `last_geometry` is initialized in
    /// `from_config` from config values and updated every frame from the child viewport
    /// inputs while the viewer is visible. As a result, all of the following return
    /// sensible values:
    /// - never opened: the config value installed by from_config flows through
    /// - opened then closed: the actual position at close time
    /// - visible since startup: the latest position from the child viewport
    pub fn export_config(&self) -> crate::viewer::app::persistence::LogViewerConfig {
        let (x, y, width, height) = match self.last_geometry {
            Some(([gx, gy], [gw, gh])) => (Some(gx), Some(gy), Some(gw), Some(gh)),
            None => (None, None, None, None),
        };
        crate::viewer::app::persistence::LogViewerConfig {
            visible: self.visible,
            x,
            y,
            width,
            height,
            show_error: self.filters.show_error,
            show_warn: self.filters.show_warn,
            show_info: self.filters.show_info,
            show_debug: self.filters.show_debug,
            follow_tail: self.follow_tail,
        }
    }

    /// Turn on visibility. To re-apply the position from the previous hide on the next
    /// frame, restore from `last_geometry` when `apply_geometry` is None.
    ///
    /// If `apply_geometry` is already Some, it is not overwritten (resilient against multiple show calls).
    pub fn show(&mut self) {
        self.visible = true;
        if self.apply_geometry.is_none() {
            self.apply_geometry = self.last_geometry;
        }
    }

    /// Turn off visibility. Snapshot the current `last_geometry` into `apply_geometry`
    /// so the next `show` reopens at the same position.
    ///
    /// Both the top-bar toggle button and the X close button funnel through this,
    /// so "close and reopen within the same session" preserves the position.
    pub fn hide(&mut self) {
        if self.visible {
            self.apply_geometry = self.last_geometry;
        }
        self.visible = false;
    }

    /// Toggle visibility (used by the top-bar button).
    pub fn toggle_visible(&mut self) {
        if self.visible {
            self.hide();
        } else {
            self.show();
        }
    }

    /// Render the log viewer UI. Called against `&mut LogViewerModel` from inside the
    /// `show_viewport_deferred` closure.
    ///
    /// - Toolbar: level checkboxes / auto-follow / open folder / save log
    /// - Body: virtualized scroll + per-level coloring
    ///
    /// `log_buffer` is used by the "save log" button to take a byte snapshot.
    /// `logs_dir` is the initial directory for both the "open folder" button and the save dialog.
    pub fn draw(
        &mut self,
        child_ctx: &egui::Context,
        log_buffer: &SharedLogBuffer,
        logs_dir: &std::path::Path,
    ) {
        egui::CentralPanel::default().show(child_ctx, |ui| {
            // Toolbar.
            ui.horizontal(|ui| {
                let prev = self.filters;
                ui.checkbox(&mut self.filters.show_error, "Error");
                ui.checkbox(&mut self.filters.show_warn, "Warn");
                ui.checkbox(&mut self.filters.show_info, "Info");
                ui.checkbox(&mut self.filters.show_debug, "Debug");
                if self.filters != prev {
                    self.rebuild_filter_indices();
                }
                ui.separator();
                ui.checkbox(&mut self.follow_tail, t!("viewer.log_viewer.follow_tail"))
                    .on_hover_text(t!("viewer.log_viewer.follow_tail_hover"));
                ui.separator();
                if ui
                    .button(t!("viewer.log_viewer.open_folder"))
                    .on_hover_text(t!("viewer.log_viewer.open_folder_hover"))
                    .clicked()
                {
                    open_logs_directory(logs_dir);
                }
                if ui
                    .button(t!("viewer.log_viewer.save_log"))
                    .on_hover_text(t!("viewer.log_viewer.save_log_hover"))
                    .clicked()
                {
                    save_log_to_file(log_buffer, logs_dir);
                }
            });

            ui.separator();

            // Row header: total count and hidden count.
            ui.horizontal(|ui| {
                let total = self.lines.len();
                let shown = self.filter_indices.len();
                let suffix: std::borrow::Cow<'static, str> = if total >= LINE_LIMIT {
                    t!("viewer.log_viewer.limit_reached")
                } else {
                    std::borrow::Cow::Borrowed("")
                };
                ui.small(t!(
                    "viewer.log_viewer.line_count",
                    shown = shown,
                    total = total,
                    suffix = suffix,
                ));
            });

            ui.separator();

            // Body: virtualized scroll.
            let row_height = ui.text_style_height(&egui::TextStyle::Monospace);
            let total_rows = self.filter_indices.len();
            egui::ScrollArea::vertical()
                .stick_to_bottom(self.follow_tail)
                .auto_shrink([false, false])
                .show_rows(ui, row_height, total_rows, |ui, row_range| {
                    for row in row_range {
                        let Some(&line_idx) = self.filter_indices.get(row) else {
                            continue;
                        };
                        let Some(line) = self.lines.get(line_idx) else {
                            continue;
                        };
                        draw_log_row(ui, line);
                    }
                });
        });
    }
}

/// Draw a single row. For multi-line messages, only the first line is rendered;
/// the rest is shown in the hover tooltip.
fn draw_log_row(ui: &mut egui::Ui, line: &LogLine) {
    let dark = ui.visuals().dark_mode;
    let color = match line.level {
        LogLevel::Error => {
            super::theme::accent_text(dark, egui::Color32::from_rgb(0xFF, 0x60, 0x60))
        }
        LogLevel::Warn => {
            super::theme::accent_text(dark, egui::Color32::from_rgb(0xE0, 0xC0, 0x40))
        }
        LogLevel::Info => super::theme::strong_text(dark),
        LogLevel::Debug => super::theme::gray_text(dark, 0x90),
        LogLevel::Trace => super::theme::gray_text(dark, 0x70),
        LogLevel::Unknown => super::theme::gray_text(dark, 0xB0),
    };

    let first_line = line.message.lines().next().unwrap_or("");
    let extra_lines = line.message.lines().count().saturating_sub(1);

    let display = if line.timestamp.is_empty() {
        if extra_lines > 0 {
            format!("{first_line}  [+{extra_lines} lines]")
        } else {
            first_line.to_string()
        }
    } else if extra_lines > 0 {
        format!(
            "[{}] {}  [+{extra_lines} lines]",
            line.timestamp, first_line
        )
    } else {
        format!("[{}] {}", line.timestamp, first_line)
    };

    let response = ui.add(egui::Label::new(
        egui::RichText::new(display).monospace().color(color),
    ));
    if extra_lines > 0 {
        response.on_hover_text(&line.message);
    }
}

/// Open `logs_dir` in Explorer / Finder. Failures are ignored.
fn open_logs_directory(logs_dir: &std::path::Path) {
    #[cfg(target_os = "windows")]
    {
        let _ = std::process::Command::new("explorer").arg(logs_dir).spawn();
    }
    #[cfg(target_os = "macos")]
    {
        let _ = std::process::Command::new("open").arg(logs_dir).spawn();
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let _ = std::process::Command::new("xdg-open").arg(logs_dir).spawn();
    }
}

/// Handler for the "save log" button. Take a byte snapshot of `log_buffer` under
/// the lock-shortest pattern, then ask `rfd::FileDialog` for the destination.
fn save_log_to_file(log_buffer: &SharedLogBuffer, logs_dir: &std::path::Path) {
    // 1. Only take a snapshot while the lock is held.
    let bytes: Vec<u8> = {
        let mut lb = match log_buffer.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        lb.data.make_contiguous().to_vec()
    };
    if bytes.is_empty() {
        log::info!("Log save skipped: buffer is empty");
        return;
    }

    // 2. Show the dialog after releasing the lock (it blocks the UI thread; acceptable for v1).
    let default_name = format!(
        "popone_{}.log",
        chrono::Local::now().format("%Y%m%d_%H%M%S")
    );
    let picked = rfd::FileDialog::new()
        .set_directory(logs_dir)
        .set_file_name(&default_name)
        .add_filter("Log file", &["log"])
        .save_file();

    if let Some(path) = picked {
        match std::fs::write(&path, &bytes) {
            Ok(_) => log::info!("Log saved: {}", path.display()),
            Err(e) => log::warn!("Log save failed: {e}"),
        }
    }
}

/// Parse the `[HH:MM:SS.mmm][LEVEL] message` form.
///
/// On success returns `(level, timestamp, message)`. If the LEVEL string is not in
/// the known set, the format is still accepted and `LogLevel::Unknown` is returned
/// (so the line is distinguished from a continuation line).
/// Lines without two `[...]` segments return None.
fn parse_line_header(line: &str) -> Option<(LogLevel, &str, &str)> {
    let rest = line.strip_prefix('[')?;
    let close1 = rest.find(']')?;
    let timestamp = &rest[..close1];
    let after_first = &rest[close1 + 1..];
    let rest2 = after_first.strip_prefix('[')?;
    let close2 = rest2.find(']')?;
    let level_str = &rest2[..close2];
    let tail = &rest2[close2 + 1..];
    // The fern formatter in main.rs follows `]` with a single space before the message.
    let message = tail.strip_prefix(' ').unwrap_or(tail);
    Some((LogLevel::from_level_str(level_str), timestamp, message))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::LogBuffer;

    fn make_model() -> LogViewerModel {
        LogViewerModel::default()
    }

    #[test]
    fn parser_basic() {
        let mut m = make_model();
        m.ingest("[12:34:56.789][INFO] hello\n");
        assert_eq!(m.lines.len(), 1);
        let line = &m.lines[0];
        assert_eq!(line.level, LogLevel::Info);
        assert_eq!(line.timestamp, "12:34:56.789");
        assert_eq!(line.message, "hello");
        assert!(!m.seeking_first_header);
    }

    #[test]
    fn parser_all_levels() {
        let mut m = make_model();
        m.filters.show_debug = true;
        m.ingest("[00:00:00.000][ERROR] e\n");
        m.ingest("[00:00:00.000][WARN] w\n");
        m.ingest("[00:00:00.000][INFO] i\n");
        m.ingest("[00:00:00.000][DEBUG] d\n");
        m.ingest("[00:00:00.000][TRACE] t\n");
        assert_eq!(m.lines.len(), 5);
        assert_eq!(m.lines[0].level, LogLevel::Error);
        assert_eq!(m.lines[1].level, LogLevel::Warn);
        assert_eq!(m.lines[2].level, LogLevel::Info);
        assert_eq!(m.lines[3].level, LogLevel::Debug);
        assert_eq!(m.lines[4].level, LogLevel::Trace);
    }

    #[test]
    fn parser_multiline_concat() {
        let mut m = make_model();
        m.ingest("[12:34:56.789][ERROR] panic\n  at src/foo.rs:42\n  at src/bar.rs:88\n");
        assert_eq!(
            m.lines.len(),
            1,
            "multi-line (backtrace) should be concatenated into a single LogLine"
        );
        assert_eq!(m.lines[0].level, LogLevel::Error);
        assert_eq!(
            m.lines[0].message,
            "panic\n  at src/foo.rs:42\n  at src/bar.rs:88"
        );
    }

    #[test]
    fn parser_leading_fragment_discarded() {
        let mut m = make_model();
        // After byte-level drain in LogBuffer, the leading fragment may not start with [HEADER].
        m.ingest("garbage fragment\nanother garbage line\n[12:34:56.789][INFO] real\n");
        assert_eq!(
            m.lines.len(),
            1,
            "non-header lines before the first [HEADER] should be discarded"
        );
        assert_eq!(m.lines[0].message, "real");
        assert!(
            !m.seeking_first_header,
            "seeking_first_header should be false after canonical header detection"
        );
    }

    #[test]
    fn parser_unknown_level() {
        let mut m = make_model();
        // The [ts][LEVEL] format matches but the level string is not in the known set.
        m.ingest("[12:34:56.789][FATAL] boom\n");
        assert_eq!(m.lines.len(), 1);
        assert_eq!(m.lines[0].level, LogLevel::Unknown);
        assert_eq!(m.lines[0].timestamp, "12:34:56.789");
        assert_eq!(m.lines[0].message, "boom");
    }

    #[test]
    fn parser_incomplete_trailing_fragment() {
        let mut m = make_model();
        // A fragment without \n is carried over in tail_buffer; nothing is pushed to lines.
        m.ingest("[12:34:56.789][INFO] part");
        assert_eq!(m.lines.len(), 0);
        assert_eq!(m.tail_buffer, "[12:34:56.789][INFO] part");

        // The next ingest concatenates and completes one line.
        m.ingest("ial\n");
        assert_eq!(m.lines.len(), 1);
        assert_eq!(m.lines[0].message, "partial");
        assert!(m.tail_buffer.is_empty());
    }

    #[test]
    fn parser_crlf_stripped() {
        let mut m = make_model();
        m.ingest("[12:34:56.789][INFO] crlf line\r\n");
        assert_eq!(m.lines.len(), 1);
        assert_eq!(m.lines[0].message, "crlf line");
    }

    #[test]
    fn parser_drain_and_filter_indices_consistency() {
        let mut m = make_model();
        m.filters.show_debug = true; // let every level through

        // Push past LINE_LIMIT.
        for i in 0..(LINE_LIMIT + 100) {
            m.ingest(&format!("[00:00:00.000][INFO] line {i}\n"));
        }

        assert_eq!(
            m.lines.len(),
            LINE_LIMIT,
            "lines は LINE_LIMIT で cap される"
        );

        // No stale indices remain in filter_indices.
        for &idx in &m.filter_indices {
            assert!(
                idx < m.lines.len(),
                "stale filter index {} >= lines.len() {}",
                idx,
                m.lines.len()
            );
        }
        // Every line passes the filter, so filter_indices has the same length as lines.
        assert_eq!(m.filter_indices.len(), m.lines.len());
    }

    #[test]
    fn parser_line_limit_cut() {
        let mut m = make_model();
        m.filters.show_debug = true;
        for i in 0..25_000 {
            m.ingest(&format!("[00:00:00.000][INFO] line {i}\n"));
        }
        assert_eq!(m.lines.len(), LINE_LIMIT);
        // The remaining front entry is the (25000 - 20000 = 5000)-th line.
        assert_eq!(m.lines[0].message, "line 5000");
        assert_eq!(m.lines.back().unwrap().message, "line 24999");
    }

    #[test]
    fn poll_incremental_read() {
        let buf: SharedLogBuffer = Arc::new(Mutex::new(LogBuffer::new()));
        let mut m = make_model();

        // Write the initial log.
        {
            let mut lb = buf.lock().unwrap();
            let text = "[12:34:56.789][INFO] first\n";
            lb.data.extend(text.as_bytes().iter().copied());
            lb.total_written += text.len();
        }

        m.poll(&buf);
        assert_eq!(m.lines.len(), 1);
        assert_eq!(m.lines[0].message, "first");
        let offset_after_first = m.last_offset;
        assert!(offset_after_first > 0);

        // Append more log data.
        {
            let mut lb = buf.lock().unwrap();
            let text = "[12:34:57.123][WARN] second\n";
            lb.data.extend(text.as_bytes().iter().copied());
            lb.total_written += text.len();
        }

        m.poll(&buf);
        assert_eq!(m.lines.len(), 2);
        assert_eq!(m.lines[1].message, "second");
        assert!(
            m.last_offset > offset_after_first,
            "last_offset should be monotonically increasing"
        );
    }

    #[test]
    fn filter_toggles_rebuild_indices() {
        let mut m = make_model();
        m.filters.show_debug = false;
        m.ingest("[00:00:00.000][INFO] shown\n");
        m.ingest("[00:00:00.000][DEBUG] hidden\n");
        m.ingest("[00:00:00.000][INFO] shown2\n");

        assert_eq!(m.lines.len(), 3);
        assert_eq!(m.filter_indices.len(), 2);
        assert_eq!(m.filter_indices, vec![0, 2]);

        // Enable Debug.
        let mut f = m.filters;
        f.show_debug = true;
        m.set_filters(f);
        assert_eq!(m.filter_indices, vec![0, 1, 2]);

        // Disable Info.
        let mut f = m.filters;
        f.show_info = false;
        m.set_filters(f);
        assert_eq!(m.filter_indices, vec![1]);
    }

    // ------------------------------------------------------------------
    // Regression tests for the P1 / P2 fixes
    // ------------------------------------------------------------------

    fn make_cfg(visible: bool) -> crate::viewer::app::persistence::LogViewerConfig {
        crate::viewer::app::persistence::LogViewerConfig {
            visible,
            x: Some(100.0),
            y: Some(200.0),
            width: Some(800.0),
            height: Some(600.0),
            show_error: true,
            show_warn: true,
            show_info: true,
            show_debug: false,
            follow_tail: true,
        }
    }

    #[test]
    fn from_config_initializes_last_geometry() {
        // P2 fix verification: from_config initializes last_geometry from config too.
        // This way export_config preserves the config even in a session where the viewer is never opened.
        let cfg = make_cfg(false);
        let m = LogViewerModel::from_config(&cfg);
        assert_eq!(m.apply_geometry, Some(([100.0, 200.0], [800.0, 600.0])));
        assert_eq!(m.last_geometry, Some(([100.0, 200.0], [800.0, 600.0])));
    }

    #[test]
    fn export_config_preserves_geometry_when_never_opened() {
        // P2 fix verification: a from_config -> export_config round trip does not lose position/size.
        let cfg = make_cfg(false);
        let m = LogViewerModel::from_config(&cfg);
        // Export without ever opening the viewer.
        let exported = m.export_config();
        assert_eq!(exported.x, Some(100.0));
        assert_eq!(exported.y, Some(200.0));
        assert_eq!(exported.width, Some(800.0));
        assert_eq!(exported.height, Some(600.0));
        assert!(!exported.visible);
    }

    #[test]
    fn hidden_startup_then_show_keeps_apply_geometry() {
        // P1 fix verification: do not consume apply_geometry on a frame where visible=false.
        // Start hidden -> open by button: apply_geometry must still be present.
        let cfg = make_cfg(false);
        let mut m = LogViewerModel::from_config(&cfg);
        assert!(!m.visible);
        // Same state as after the visible-false early return inside show_log_viewer.
        // -> apply_geometry was never touched, so it stays Some.
        assert!(m.apply_geometry.is_some());

        // User clicks the "Log" button to show.
        m.show();
        assert!(m.visible);
        // show does not overwrite while apply_geometry is Some, so the config value remains.
        assert_eq!(m.apply_geometry, Some(([100.0, 200.0], [800.0, 600.0])));
    }

    #[test]
    fn hide_then_show_round_trip_preserves_geometry() {
        // Same-session reopen position retention: after closing with X, toggling open restores last_geometry.
        let cfg = make_cfg(true);
        let mut m = LogViewerModel::from_config(&cfg);
        // Equivalent to the first call to show_log_viewer: take and use apply_geometry.
        let _ = m.apply_geometry.take();
        assert!(m.apply_geometry.is_none());
        // Pretend the user moved the window, updating last_geometry.
        m.last_geometry = Some(([300.0, 400.0], [900.0, 700.0]));
        // X close.
        m.hide();
        assert!(!m.visible);
        // hide snapshots last_geometry into apply_geometry.
        assert_eq!(m.apply_geometry, Some(([300.0, 400.0], [900.0, 700.0])));
        // Reopen.
        m.show();
        assert!(m.visible);
        // The next show_log_viewer takes the value the user moved to.
        assert_eq!(m.apply_geometry, Some(([300.0, 400.0], [900.0, 700.0])));
    }

    #[test]
    fn toggle_close_then_toggle_open_preserves_geometry() {
        // Position is also preserved across close/reopen via the top-bar toggle (not just via X).
        let cfg = make_cfg(true);
        let mut m = LogViewerModel::from_config(&cfg);
        let _ = m.apply_geometry.take();
        m.last_geometry = Some(([500.0, 600.0], [1000.0, 800.0]));

        // Close via toggle_visible.
        m.toggle_visible();
        assert!(!m.visible);
        assert_eq!(m.apply_geometry, Some(([500.0, 600.0], [1000.0, 800.0])));

        // Reopen via toggle_visible.
        m.toggle_visible();
        assert!(m.visible);
        assert_eq!(m.apply_geometry, Some(([500.0, 600.0], [1000.0, 800.0])));
    }

    #[test]
    fn export_after_session_uses_latest_geometry() {
        // After the user moves the viewer and closes it, export_config returns the latest position.
        let cfg = make_cfg(true);
        let mut m = LogViewerModel::from_config(&cfg);
        let _ = m.apply_geometry.take();
        // Position the user moved to.
        m.last_geometry = Some(([700.0, 800.0], [1100.0, 900.0]));
        m.hide();
        let exported = m.export_config();
        assert_eq!(exported.x, Some(700.0));
        assert_eq!(exported.y, Some(800.0));
        assert_eq!(exported.width, Some(1100.0));
        assert_eq!(exported.height, Some(900.0));
    }
}
