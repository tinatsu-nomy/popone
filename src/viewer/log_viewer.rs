//! 別ウインドウで動作するログビュアー。
//!
//! `LogViewerModel` は `SharedLogBuffer` から差分読み取りしたログをパース済み行の
//! リングバッファで保持し、UI 描画用に供する。`SharedLogViewer = Arc<Mutex<LogViewerModel>>`
//! として `ViewerApp` から共有し、`show_viewport_deferred` のクロージャに `Arc::clone`
//! で渡す設計。
//!
//! Phase 1 の範囲: 型定義・パーサ・`ingest` / `poll` とそのテスト。UI 描画と永続化は
//! Phase 2 / 3 で追加する。

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use eframe::egui;

use crate::SharedLogBuffer;

/// インメモリに保持するパース済みログ行の上限。
pub const LINE_LIMIT: usize = 20_000;

/// `ViewerApp` と `show_viewport_deferred` クロージャ間で共有するモデル。
pub type SharedLogViewer = Arc<Mutex<LogViewerModel>>;

/// ログレベル（UI フィルタと色分けに使用）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogLevel {
    Error,
    Warn,
    Info,
    Debug,
    Trace,
    /// `[ts][LEVEL] msg` 形式を満たすがレベル文字列が未知（例: FATAL）。
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

/// パース済みのログ 1 行。`message` はマルチライン連結済み（バックトレース等を `\n` で保持）。
#[derive(Debug, Clone)]
pub struct LogLine {
    pub level: LogLevel,
    pub timestamp: String,
    pub message: String,
}

/// レベル別表示フラグ。初期値は Debug のみ OFF。
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
    /// 指定レベルがフィルタを通過するか。Unknown は常に表示、Trace は Debug フラグに追従。
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

/// ログビュアーのコア状態。`Arc<Mutex<_>>` 経由で共有される。
pub struct LogViewerModel {
    /// ログビュアーウインドウを表示するか。
    pub visible: bool,
    /// `SharedLogBuffer::total_written` の前回読み取り位置。
    last_offset: usize,
    /// パース済みログ行のリングバッファ（上限 `LINE_LIMIT`）。
    pub lines: VecDeque<LogLine>,
    /// フィルタを通過した `lines` のインデックス（仮想化スクロール用）。
    pub filter_indices: Vec<usize>,
    /// trim 発生時やフィルタ変更時に true → 次回 `rebuild_filter_indices` を強制。
    filters_dirty: bool,
    /// レベル表示フラグ。
    pub filters: LevelFilters,
    /// 自動追尾（末尾スクロール）。
    pub follow_tail: bool,
    /// 次フレームの `ViewportBuilder` に渡す位置・サイズ（反映後 None）。
    /// 起動時の復元値、および同セッション reopen 時の位置復元に使う。
    pub apply_geometry: Option<([f32; 2], [f32; 2])>,
    /// 毎フレーム子 viewport から読み取った最新 geometry（on_exit 永続化用）。
    pub last_geometry: Option<([f32; 2], [f32; 2])>,
    /// `\n` 未完了の末尾断片（次回 ingest の先頭に連結）。
    tail_buffer: String,
    /// 初回 `[HEADER]` を見るまで `true`。`LogBuffer` のバイト単位 drain で先頭断片が
    /// 欠損している可能性があるため、それを捨てるためのフラグ。
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
    /// `SharedLogBuffer` から差分を読み取り、`ingest` へ渡す。
    ///
    /// ロック保持は `read_from_offset` の結果を取得する最短時間のみ。UI 描画中は
    /// 呼ばないこと（`log::info!` 側スレッドをブロックしないため）。
    pub fn poll(&mut self, log_buffer: &SharedLogBuffer) {
        let new_text = {
            let lb = match log_buffer.lock() {
                Ok(g) => g,
                // 他スレッド panic 耐性: poisoned でも中身は読む
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

    /// 生のログテキスト（複数行）をパースして `lines` に追加する。
    ///
    /// - 改行未完了の末尾断片は `tail_buffer` に繰り越し、次回 ingest の先頭に連結する
    /// - `seeking_first_header` の間は `[HEADER]` 形式が見つかるまで行を捨てる
    /// - `[` で始まらない行で直前の `LogLine` が存在する場合は message にマルチライン連結
    /// - `LINE_LIMIT` 超過時は先頭から drain し、`filter_indices` は全再構築
    pub fn ingest(&mut self, text: &str) {
        if text.is_empty() {
            return;
        }

        // 前回の tail_buffer と新規テキストを連結し、\n で分割
        let mut combined = std::mem::take(&mut self.tail_buffer);
        combined.push_str(text);

        let mut parts: Vec<&str> = combined.split('\n').collect();
        // 最後の要素は次回までの繰越断片（text が \n で終わっていれば空文字列）
        let new_tail = parts.pop().unwrap_or("").to_string();

        for raw_line in parts {
            // CRLF 対応
            let line = raw_line.strip_suffix('\r').unwrap_or(raw_line);

            if let Some((level, ts, msg)) = parse_line_header(line) {
                // 正規ヘッダを検出 → 新しい LogLine として push
                self.seeking_first_header = false;
                self.push_line(LogLine {
                    level,
                    timestamp: ts.to_string(),
                    message: msg.to_string(),
                });
            } else if self.seeking_first_header {
                // 初回ヘッダ未発見 → 先頭断片（バイト単位 drain で欠損した可能性）を捨てる
                continue;
            } else if let Some(last) = self.lines.back_mut() {
                // 直前行の継続（マルチラインメッセージ、バックトレース等）
                last.message.push('\n');
                last.message.push_str(line);
            } else {
                // 直前行が無いのに seeking_first_header が false の稀なケース
                // → Unknown 独立行として扱い、情報欠落を防ぐ
                self.push_line(LogLine {
                    level: LogLevel::Unknown,
                    timestamp: String::new(),
                    message: line.to_string(),
                });
            }
        }

        self.tail_buffer = new_tail;

        // 上限超過時は先頭から drain し、filter_indices を全再構築
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

    /// LogLine を push し、フィルタ通過ならインクリメンタルに `filter_indices` を更新する。
    fn push_line(&mut self, line: LogLine) {
        let matches = self.filters.matches(line.level);
        self.lines.push_back(line);
        if matches {
            self.filter_indices.push(self.lines.len() - 1);
        }
    }

    /// フィルタ通過インデックスを全再構築。trim 後またはフィルタ変更後に呼ぶ。
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

    /// レベルフィルタ設定を差し替え、インデックスを即座に再構築する。
    pub fn set_filters(&mut self, filters: LevelFilters) {
        self.filters = filters;
        self.rebuild_filter_indices();
        self.filters_dirty = false;
    }

    /// `popone.toml` から読み込んだ `LogViewerConfig` で初期化する。
    ///
    /// 位置/サイズの両方が Some の場合だけ `apply_geometry` を設定し、次フレームで
    /// `ViewportBuilder` に渡される。片方だけ Some の半端な状態はデフォルト扱い。
    ///
    /// `last_geometry` も同じ値で初期化する。これにより「ビュアーを一度も開かずに
    /// アプリを終了したセッション」でも `export_config` が config 由来の位置を保ったまま
    /// 戻せるようになり、永続化が壊れない（P2 修正）。
    pub fn from_config(cfg: &crate::viewer::app::persistence::LogViewerConfig) -> Self {
        let geometry = match (cfg.position, cfg.size) {
            (Some(p), Some(s)) => Some((p, s)),
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

    /// 現在の状態を `LogViewerConfig` にシリアライズする。`ViewerApp::on_exit` から呼ぶ。
    ///
    /// 位置/サイズは `last_geometry` から取得する。`last_geometry` は `from_config` で
    /// config 由来の値で初期化され、ビュアーが表示されている間は子 viewport の入力から
    /// 毎フレーム更新される。よって以下の全ケースで適切な値が返る:
    /// - 一度も開いていない: from_config が入れた config 値がそのまま返る
    /// - 開いてから閉じた: 閉じる時点での実際の位置が返る
    /// - 起動時から表示中: 子 viewport の最新位置が返る
    pub fn export_config(&self) -> crate::viewer::app::persistence::LogViewerConfig {
        let (position, size) = match self.last_geometry {
            Some((p, s)) => (Some(p), Some(s)),
            None => (None, None),
        };
        crate::viewer::app::persistence::LogViewerConfig {
            visible: self.visible,
            position,
            size,
            show_error: self.filters.show_error,
            show_warn: self.filters.show_warn,
            show_info: self.filters.show_info,
            show_debug: self.filters.show_debug,
            follow_tail: self.follow_tail,
        }
    }

    /// 表示を ON にする。前回 hide 時に保存した位置を次フレームで適用するため、
    /// `apply_geometry` が None なら `last_geometry` から復元する。
    ///
    /// `apply_geometry` が既に Some の場合は上書きしない（複数回連続呼出への耐性）。
    pub fn show(&mut self) {
        self.visible = true;
        if self.apply_geometry.is_none() {
            self.apply_geometry = self.last_geometry;
        }
    }

    /// 表示を OFF にする。次回 `show` 時に同じ位置で開けるよう、現在の `last_geometry`
    /// を `apply_geometry` にスナップショットして保存する。
    ///
    /// トップバーのトグルボタン経由でも、× ボタン経由でも、両方の経路でこれを呼ぶことで
    /// 「同セッション内で閉じてから再度開く」ケースの位置維持を実現する。
    pub fn hide(&mut self) {
        if self.visible {
            self.apply_geometry = self.last_geometry;
        }
        self.visible = false;
    }

    /// 表示状態をトグルする（トップバーのボタン用）。
    pub fn toggle_visible(&mut self) {
        if self.visible {
            self.hide();
        } else {
            self.show();
        }
    }

    /// ログビュアーウインドウの UI を描画する。`show_viewport_deferred` の
    /// クロージャから `&mut LogViewerModel` に対して呼ぶ。
    ///
    /// - ツールバー: レベルチェックボックス / 自動追尾 / フォルダを開く / ログ保存
    /// - 本体: 仮想化スクロール + レベル別カラーリング
    ///
    /// `log_buffer` は「ログ保存」ボタンでバイト列スナップショットを取るために使う。
    /// `logs_dir` は「フォルダを開く」ボタンと保存ダイアログの初期ディレクトリとして使う。
    pub fn draw(
        &mut self,
        child_ctx: &egui::Context,
        log_buffer: &SharedLogBuffer,
        logs_dir: &std::path::Path,
    ) {
        egui::CentralPanel::default().show(child_ctx, |ui| {
            // ツールバー
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
                ui.checkbox(&mut self.follow_tail, "自動追尾")
                    .on_hover_text("新しいログが来たら自動で最下部へスクロール");
                ui.separator();
                if ui
                    .button("フォルダを開く")
                    .on_hover_text("ログディレクトリをエクスプローラで開く")
                    .clicked()
                {
                    open_logs_directory(logs_dir);
                }
                if ui
                    .button("ログ保存")
                    .on_hover_text("現在のメモリ上のログを .log ファイルとして保存")
                    .clicked()
                {
                    save_log_to_file(log_buffer, logs_dir);
                }
            });

            ui.separator();

            // 行ヘッダ: 件数と非表示件数の表示
            ui.horizontal(|ui| {
                let total = self.lines.len();
                let shown = self.filter_indices.len();
                ui.small(format!(
                    "{} 件表示 / 全 {} 件{}",
                    shown,
                    total,
                    if total >= LINE_LIMIT {
                        "（上限到達）"
                    } else {
                        ""
                    }
                ));
            });

            ui.separator();

            // 本体: 仮想化スクロール
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

/// 1 行分の描画。マルチラインメッセージは先頭行のみ表示し、残りは hover ツールチップに出す。
fn draw_log_row(ui: &mut egui::Ui, line: &LogLine) {
    let color = match line.level {
        LogLevel::Error => egui::Color32::from_rgb(0xFF, 0x60, 0x60),
        LogLevel::Warn => egui::Color32::from_rgb(0xE0, 0xC0, 0x40),
        LogLevel::Info => egui::Color32::WHITE,
        LogLevel::Debug => egui::Color32::from_rgb(0x90, 0x90, 0x90),
        LogLevel::Trace => egui::Color32::from_rgb(0x70, 0x70, 0x70),
        LogLevel::Unknown => egui::Color32::from_rgb(0xB0, 0xB0, 0xB0),
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

/// `logs_dir` をエクスプローラ/Finder で開く。失敗は無視。
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

/// 「ログ保存」ボタンのハンドラ。ロック最短化パターンで
/// `log_buffer` のバイト列をスナップショットしてから `rfd::FileDialog` で保存先を尋ねる。
fn save_log_to_file(log_buffer: &SharedLogBuffer, logs_dir: &std::path::Path) {
    // 1. ロック下ではスナップショットだけ取る
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

    // 2. アンロック後にダイアログ（UI スレッドはブロックするが初版では許容）
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

/// `[HH:MM:SS.mmm][LEVEL] message` 形式を解析する。
///
/// 成功時は `(level, timestamp, message)` を返す。`LEVEL` が未知の文字列でも
/// フォーマットが合っていれば `LogLevel::Unknown` で返す（継続行との区別のため）。
/// 2 つの `[...]` を含まない行は `None`。
fn parse_line_header(line: &str) -> Option<(LogLevel, &str, &str)> {
    let rest = line.strip_prefix('[')?;
    let close1 = rest.find(']')?;
    let timestamp = &rest[..close1];
    let after_first = &rest[close1 + 1..];
    let rest2 = after_first.strip_prefix('[')?;
    let close2 = rest2.find(']')?;
    let level_str = &rest2[..close2];
    let tail = &rest2[close2 + 1..];
    // main.rs の fern フォーマットは `] ` 後にメッセージが続く（半角スペース 1 つ）
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
            "マルチライン (バックトレース) は 1 件の LogLine に連結されるべき"
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
        // LogBuffer がバイト単位で drain した結果、先頭断片が [HEADER] で始まっていないケース
        m.ingest("garbage fragment\nanother garbage line\n[12:34:56.789][INFO] real\n");
        assert_eq!(
            m.lines.len(),
            1,
            "初回 [HEADER] を見るまでの非ヘッダ行は捨てられるべき"
        );
        assert_eq!(m.lines[0].message, "real");
        assert!(
            !m.seeking_first_header,
            "正規ヘッダ検出後は seeking_first_header が false"
        );
    }

    #[test]
    fn parser_unknown_level() {
        let mut m = make_model();
        // [ts][LEVEL] フォーマットは合っているがレベル文字列が既知集合に無いケース
        m.ingest("[12:34:56.789][FATAL] boom\n");
        assert_eq!(m.lines.len(), 1);
        assert_eq!(m.lines[0].level, LogLevel::Unknown);
        assert_eq!(m.lines[0].timestamp, "12:34:56.789");
        assert_eq!(m.lines[0].message, "boom");
    }

    #[test]
    fn parser_incomplete_trailing_fragment() {
        let mut m = make_model();
        // \n 無しの断片は tail_buffer に繰り越し、lines には push されない
        m.ingest("[12:34:56.789][INFO] part");
        assert_eq!(m.lines.len(), 0);
        assert_eq!(m.tail_buffer, "[12:34:56.789][INFO] part");

        // 次の ingest で結合され 1 行として完成する
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
        m.filters.show_debug = true; // 全レベル通過させる

        // LINE_LIMIT を超えて push
        for i in 0..(LINE_LIMIT + 100) {
            m.ingest(&format!("[00:00:00.000][INFO] line {i}\n"));
        }

        assert_eq!(
            m.lines.len(),
            LINE_LIMIT,
            "lines は LINE_LIMIT で cap される"
        );

        // filter_indices に stale なインデックスが残っていないこと
        for &idx in &m.filter_indices {
            assert!(
                idx < m.lines.len(),
                "stale filter index {} >= lines.len() {}",
                idx,
                m.lines.len()
            );
        }
        // 全件フィルタ通過する前提なので、filter_indices は lines と同じ長さ
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
        // 先頭に残っているのは 25000 - 20000 = 5000 番目以降
        assert_eq!(m.lines[0].message, "line 5000");
        assert_eq!(m.lines.back().unwrap().message, "line 24999");
    }

    #[test]
    fn poll_incremental_read() {
        let buf: SharedLogBuffer = Arc::new(Mutex::new(LogBuffer::new()));
        let mut m = make_model();

        // 初回ログを書き込み
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

        // 追加ログ
        {
            let mut lb = buf.lock().unwrap();
            let text = "[12:34:57.123][WARN] second\n";
            lb.data.extend(text.as_bytes().iter().copied());
            lb.total_written += text.len();
        }

        m.poll(&buf);
        assert_eq!(m.lines.len(), 2);
        assert_eq!(m.lines[1].message, "second");
        assert!(m.last_offset > offset_after_first, "last_offset は単調増加");
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

        // Debug を有効化
        let mut f = m.filters;
        f.show_debug = true;
        m.set_filters(f);
        assert_eq!(m.filter_indices, vec![0, 1, 2]);

        // Info を無効化
        let mut f = m.filters;
        f.show_info = false;
        m.set_filters(f);
        assert_eq!(m.filter_indices, vec![1]);
    }

    // ------------------------------------------------------------------
    // P1 / P2 修正の回帰防止テスト
    // ------------------------------------------------------------------

    fn make_cfg(visible: bool) -> crate::viewer::app::persistence::LogViewerConfig {
        crate::viewer::app::persistence::LogViewerConfig {
            visible,
            position: Some([100.0, 200.0]),
            size: Some([800.0, 600.0]),
            show_error: true,
            show_warn: true,
            show_info: true,
            show_debug: false,
            follow_tail: true,
        }
    }

    #[test]
    fn from_config_initializes_last_geometry() {
        // P2 修正検証: from_config は last_geometry も config 値で初期化する。
        // これにより export_config が「一度も開いていない」セッションでも config を保持する。
        let cfg = make_cfg(false);
        let m = LogViewerModel::from_config(&cfg);
        assert_eq!(m.apply_geometry, Some(([100.0, 200.0], [800.0, 600.0])));
        assert_eq!(m.last_geometry, Some(([100.0, 200.0], [800.0, 600.0])));
    }

    #[test]
    fn export_config_preserves_geometry_when_never_opened() {
        // P2 修正検証: from_config → export_config の往復で position/size が消えない
        let cfg = make_cfg(false);
        let m = LogViewerModel::from_config(&cfg);
        // ビュアーを一度も開かないまま export
        let exported = m.export_config();
        assert_eq!(exported.position, Some([100.0, 200.0]));
        assert_eq!(exported.size, Some([800.0, 600.0]));
        assert!(!exported.visible);
    }

    #[test]
    fn hidden_startup_then_show_keeps_apply_geometry() {
        // P1 修正検証: visible=false のフレームで apply_geometry を消費しない。
        // 隠したまま起動 → ボタンで開く、というシーケンスで apply_geometry が残ること。
        let cfg = make_cfg(false);
        let mut m = LogViewerModel::from_config(&cfg);
        assert!(!m.visible);
        // 「show_log_viewer の visible-false 早期 return 後」と同じ状態
        // → apply_geometry には触れていないので Some のまま
        assert!(m.apply_geometry.is_some());

        // ユーザが「ログ」ボタンを押して show
        m.show();
        assert!(m.visible);
        // show は apply_geometry が Some の間は何も上書きしないので、config 値が残る
        assert_eq!(m.apply_geometry, Some(([100.0, 200.0], [800.0, 600.0])));
    }

    #[test]
    fn hide_then_show_round_trip_preserves_geometry() {
        // 同セッション reopen の位置維持: × 閉じ後 toggle 開きで last_geometry が復元される
        let cfg = make_cfg(true);
        let mut m = LogViewerModel::from_config(&cfg);
        // 1 回目の show_log_viewer 相当: apply_geometry を take して使う
        let _ = m.apply_geometry.take();
        assert!(m.apply_geometry.is_none());
        // ユーザがウインドウを動かしたとして last_geometry を更新
        m.last_geometry = Some(([300.0, 400.0], [900.0, 700.0]));
        // × 閉じる
        m.hide();
        assert!(!m.visible);
        // hide は apply_geometry を last_geometry でスナップショットする
        assert_eq!(m.apply_geometry, Some(([300.0, 400.0], [900.0, 700.0])));
        // 再度開く
        m.show();
        assert!(m.visible);
        // 次の show_log_viewer で take される値はユーザが動かした位置
        assert_eq!(m.apply_geometry, Some(([300.0, 400.0], [900.0, 700.0])));
    }

    #[test]
    fn toggle_close_then_toggle_open_preserves_geometry() {
        // トップバートグル経由（× ではなく）でも閉じ→開きで位置が維持される
        let cfg = make_cfg(true);
        let mut m = LogViewerModel::from_config(&cfg);
        let _ = m.apply_geometry.take();
        m.last_geometry = Some(([500.0, 600.0], [1000.0, 800.0]));

        // toggle_visible で閉じる
        m.toggle_visible();
        assert!(!m.visible);
        assert_eq!(m.apply_geometry, Some(([500.0, 600.0], [1000.0, 800.0])));

        // toggle_visible で再度開く
        m.toggle_visible();
        assert!(m.visible);
        assert_eq!(m.apply_geometry, Some(([500.0, 600.0], [1000.0, 800.0])));
    }

    #[test]
    fn export_after_session_uses_latest_geometry() {
        // ビュアーを開いてユーザが動かしてから閉じた場合、export_config は最新位置を返す
        let cfg = make_cfg(true);
        let mut m = LogViewerModel::from_config(&cfg);
        let _ = m.apply_geometry.take();
        // ユーザが動かした位置
        m.last_geometry = Some(([700.0, 800.0], [1100.0, 900.0]));
        m.hide();
        let exported = m.export_config();
        assert_eq!(exported.position, Some([700.0, 800.0]));
        assert_eq!(exported.size, Some([1100.0, 900.0]));
    }
}
