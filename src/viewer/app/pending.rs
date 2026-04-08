//! 遅延タスク処理（PendingState, ExportState, process_pending_tasks 等）

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use eframe::egui;

use crate::unitypackage::UnityPackageIndex;

use crate::intermediate::types::IrModel;

use super::file_io::FileFormat;
use super::helpers::{FbxLoadMode, PkgModelType, PreloadedData, ReloadableSource};
use super::{ConvertMessage, ViewerApp};

/// unitypackage 内に複数FBXがある場合の選択待ち状態
pub struct PendingUnityPackage {
    pub assets: Vec<crate::unitypackage::ExtractedAsset>,
    /// (アセットIndex, ファイル名, モデル種別)
    pub model_list: Vec<(usize, String, PkgModelType)>,
    pub source_path: PathBuf,
    /// アペンドモード（既存モデルに追加）
    pub append: bool,
    /// 一時ファイルからの読み込み時、アーカイブデータのスナップショット
    pub archive_snapshot: Option<Arc<[u8]>>,
    /// アーカイブ(ZIP/7z)内 .unitypackage の場合、リロード用のソース情報
    pub nested_archive_source: Option<ReloadableSource>,
    /// Phase 3: パッケージインデックス（Prefab テクスチャ解決用）
    pub pkg_index: Option<Arc<UnityPackageIndex>>,
    /// 複数選択用チェック状態（model_list と同サイズ）
    pub checked: Vec<bool>,
}

/// unitypackage モデル遅延読み込み状態
pub struct PendingPkgModelLoad {
    pub assets: Arc<Vec<crate::unitypackage::ExtractedAsset>>,
    pub fbx_index: usize,
    pub model_type: PkgModelType,
    pub source_path: PathBuf,
    /// オーバーレイ表示済みフラグ
    pub shown: bool,
    /// アペンドモード（既存モデルに追加）
    pub append: bool,
    /// テクスチャ選択ダイアログを抑制（リロード経由時）
    pub suppress_tex_match: bool,
    /// 一時ファイルからの読み込み時、アーカイブデータのスナップショット
    pub archive_snapshot: Option<Arc<[u8]>>,
    /// アーカイブ(ZIP/7z)内 .unitypackage の場合、リロード用のソース情報
    pub nested_archive_source: Option<ReloadableSource>,
    /// Phase 3: パッケージインデックス（Prefab テクスチャ解決用）
    pub pkg_index: Option<Arc<UnityPackageIndex>>,
    /// Batch progress: (current, total) — carried from PendingMultiLoad at queue pop time
    pub batch_progress: Option<(usize, usize)>,
}

/// アーカイブ内モデル選択待ち
pub struct PendingArchive {
    pub archive_data: Arc<[u8]>,
    pub format: crate::archive::ArchiveFormat,
    pub contents: crate::archive::ArchiveContents,
    pub source_path: PathBuf,
    pub append: bool,
    pub is_temp: bool,
}

/// アーカイブ内モデル遅延読み込み
pub struct PendingArchiveLoad {
    pub archive_data: Arc<[u8]>,
    pub format: crate::archive::ArchiveFormat,
    pub contents: crate::archive::ArchiveContents,
    pub model_index: usize,
    pub source_path: PathBuf,
    pub shown: bool,
    pub append: bool,
    pub is_temp: bool,
}

/// FBX読み込み方法選択ダイアログの状態（モデル+アニメーション両方含むFBX用）
pub struct PendingFbxChoice {
    pub path: PathBuf,
    pub load_model: bool,
    pub load_animation: bool,
    /// unitypackage 経由の場合のデータ
    pub pkg_context: Option<PendingFbxChoicePkg>,
    /// D&D一時ファイルの先読みデータ
    pub preloaded: Option<super::helpers::PreloadedData>,
}

/// unitypackage 経由 FBX 選択時の追加コンテキスト
pub struct PendingFbxChoicePkg {
    pub assets: Arc<Vec<crate::unitypackage::ExtractedAsset>>,
    pub fbx_index: usize,
    pub source_path: PathBuf,
    /// 一時ファイルからの読み込み時、アーカイブデータのスナップショット
    pub archive_snapshot: Option<Arc<[u8]>>,
    /// アーカイブ(ZIP/7z)内 .unitypackage の場合、リロード用のソース情報
    pub nested_archive_source: Option<ReloadableSource>,
    /// Phase 3: パッケージインデックス（Prefab テクスチャ解決用）
    pub pkg_index: Option<Arc<UnityPackageIndex>>,
}

/// 遅延処理のオーバーレイ表示状態
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PendingOverlay {
    /// オーバーレイ未表示（次フレームで表示）
    WaitingOverlay,
    /// オーバーレイ表示済み（次フレームで実行）
    Ready,
}

/// ロード投入（全入口統一: ダイアログ / D&D / IPC）
pub struct PendingLoadDispatch {
    pub path: PathBuf,
    pub append: bool,
    pub overlay: PendingOverlay,
    /// D&D temp ファイルの先読みデータ（self.preloaded から移動）
    pub preloaded: Option<PreloadedData>,
}

/// バックグラウンド CPU パースの結果
pub struct BgLoadResult {
    pub ir: IrModel,
    pub source: ReloadableSource,
    pub kind: BgLoadKind,
    pub path: PathBuf,
    /// 発行元 dispatch の世代番号。現世代の `BgLoadHandle.request_id` と一致しない結果は
    /// 「古いロードが完了したがユーザーは既に次のロードに進んでいる」状態として破棄する。
    pub request_id: u64,
}

/// バックグラウンド CPU パースのハンドル（受信チャネル + キャンセルトークン + 世代番号）
pub struct BgLoadHandle {
    pub rx: std::sync::mpsc::Receiver<anyhow::Result<BgLoadResult>>,
    /// 別スレッドのパース処理へのキャンセル通知。新規ロード投入時に `true` に設定する。
    pub cancel: Arc<AtomicBool>,
    pub request_id: u64,
}

/// ロード種別（後処理の分岐に使用）
pub enum BgLoadKind {
    /// 通常ロード（format + FBX自動アニメフラグ）
    Initial {
        format: FileFormat,
        auto_fbx_anim: bool,
    },
    /// 追加読み込み
    Append,
}

/// 非同期ファイルダイアログの種別
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileDialogKind {
    /// モデル/アニメーションを開く
    Open,
    /// モデル追加読み込み
    Append,
}

/// バックグラウンドロードの状態マシン。
///
/// 従来は `load_dispatch: Option<PendingLoadDispatch>` と `bg_load: Option<BgLoadHandle>` の
/// 2 フィールド併存で表現していたが、「両方 Some」「両方 None のはずが片方だけ取り残される」などの
/// 不正状態を型レベルで排除するため enum に統合した。
///
/// 状態遷移:
/// - `Idle` → `PendingDispatch`: ファイルダイアログ結果・D&D・IPC・コマンドライン引数
/// - `PendingDispatch` → `Idle` or `Loading`: `route_load_dispatch` が即時実行 / spawn_bg_load を選ぶ
/// - `Loading` → `Idle`: BG スレッドからの結果受信
/// - `Loading` → `PendingDispatch { prior_loading: Some(..) }`: Loading 中に次の dispatch が投入された場合、
///   先行 handle を `prior_loading` として引き継ぎ、`route_load_dispatch` が intent に応じて
///   キャンセル（モデル要求）か保護（アニメ単体要求）を判断する
pub enum BackgroundLoadState {
    /// 何も走っていない
    Idle,
    /// dispatch 予約あり。次フレームで `route_load_dispatch` が呼ばれる。
    /// `prior_loading` は Loading 中に新 dispatch が投入された場合の先行 handle。
    PendingDispatch {
        dispatch: PendingLoadDispatch,
        prior_loading: Option<BgLoadHandle>,
    },
    /// BG スレッドがパース中。結果受信待ち。
    Loading(BgLoadHandle),
}

impl BackgroundLoadState {
    pub fn is_idle(&self) -> bool {
        matches!(self, BackgroundLoadState::Idle)
    }

    pub fn is_loading(&self) -> bool {
        matches!(self, BackgroundLoadState::Loading(_))
    }

    /// 何らかのロード処理が進行中（dispatch 予約 or BG パース中）
    pub fn is_active(&self) -> bool {
        !self.is_idle()
    }

    /// 新しい dispatch を投入する。
    /// 先行 Loading がある場合は `prior_loading` として引き継ぎ、`route_load_dispatch` の
    /// intent 判定（モデル要求 vs アニメ単体要求）でキャンセル可否を決定する。
    /// 既に `PendingDispatch` 状態なら古い dispatch は破棄し、その `prior_loading` のみ引き継ぐ。
    pub fn submit_dispatch(&mut self, dispatch: PendingLoadDispatch) {
        let prior_loading = match std::mem::replace(self, BackgroundLoadState::Idle) {
            BackgroundLoadState::Idle => None,
            BackgroundLoadState::Loading(h) => Some(h),
            BackgroundLoadState::PendingDispatch { prior_loading, .. } => prior_loading,
        };
        *self = BackgroundLoadState::PendingDispatch {
            dispatch,
            prior_loading,
        };
    }
}

/// OBJ/STL インポート時の単位選択
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ImportUnit {
    Mm,
    Cm,
    M,
    Inch,
}

impl ImportUnit {
    /// glTF 空間（メートル）へのスケール係数
    pub fn scale(self) -> f32 {
        match self {
            ImportUnit::Mm => 0.001,
            ImportUnit::Cm => 0.01,
            ImportUnit::M => 1.0,
            ImportUnit::Inch => 0.0254,
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            ImportUnit::Mm => "ミリメートル (mm)",
            ImportUnit::Cm => "センチメートル (cm)",
            ImportUnit::M => "メートル (m)",
            ImportUnit::Inch => "インチ (inch)",
        }
    }
}

/// OBJ/STL インポートオプション選択待ち状態
pub struct PendingImportOptions {
    pub path: PathBuf,
    pub format: FileFormat,
    pub append: bool,
    pub preloaded: Option<PreloadedData>,
    pub unit: ImportUnit,
    pub z_up: bool,
}

/// 遅延処理（ペンディング）の集約状態
pub struct PendingState {
    /// FBX読み込み方法選択待ち（モデル+アニメ両方含む場合）
    pub fbx_choice: Option<PendingFbxChoice>,
    /// unitypackage FBX選択待ち
    pub unity_pkg: Option<PendingUnityPackage>,
    /// FBX遅延読み込み
    pub pkg_load: Option<PendingPkgModelLoad>,
    /// アーカイブ内モデル選択待ち
    pub archive: Option<PendingArchive>,
    /// アーカイブモデル遅延読み���み
    pub archive_load: Option<PendingArchiveLoad>,
    /// バックグラウンドロードの状態マシン
    /// （dispatch 予約 + BG パース中のハンドルを型レベルで統合）
    pub bg_state: BackgroundLoadState,
    /// PMX変換遅延実行
    pub convert: Option<PendingOverlay>,
    /// GPU再構築遅延実行
    pub rebuild: Option<PendingOverlay>,
    /// モデル再読み込み遅延実行
    pub reload: Option<PendingOverlay>,
    /// ビューポートサイズ確定後の refit（初回ロード時）
    pub refit: bool,
    /// テクスチャ履歴の上書き保存確認ダイアログ表示フラグ
    pub confirm_save_tex_history: bool,
    /// 非同期ファイルダイアログ（種別, 結果受信チャネル）
    pub file_dialog: Option<(FileDialogKind, std::sync::mpsc::Receiver<Option<PathBuf>>)>,
    /// OBJ/STL インポートオプション選択待ち
    pub import_options: Option<PendingImportOptions>,
    /// 複数モデル一括ロード（assets を1つだけ保持し、デキュー時に clone）
    pub multi_load: Option<PendingMultiLoad>,
}

/// 複数モデル一括ロード用キュー（assets を Arc 共有して clone コストを排除）
pub struct PendingMultiLoad {
    pub assets: Arc<Vec<crate::unitypackage::ExtractedAsset>>,
    /// 残りのモデル (fbx_index, model_type)
    pub remaining: Vec<(usize, PkgModelType)>,
    pub source_path: PathBuf,
    pub archive_snapshot: Option<Arc<[u8]>>,
    pub nested_archive_source: Option<ReloadableSource>,
    pub pkg_index: Option<Arc<UnityPackageIndex>>,
    /// Total number of models in batch (for progress display)
    pub total_count: usize,
}

impl Default for PendingState {
    fn default() -> Self {
        Self {
            fbx_choice: None,
            unity_pkg: None,
            pkg_load: None,
            archive: None,
            archive_load: None,
            bg_state: BackgroundLoadState::Idle,
            convert: None,
            rebuild: None,
            reload: None,
            refit: false,
            confirm_save_tex_history: false,
            file_dialog: None,
            import_options: None,
            multi_load: None,
        }
    }
}

/// PMXエクスポート関連の状態
pub struct ExportState {
    /// PMX変換時にログファイルを出力するか
    pub output_log: bool,
    /// PMX出力パス（テキストボックス編集用）
    pub pmx_output_path: String,
    /// 表示材質のみPMX出力（デフォルト: false）
    pub export_visible_only: bool,
    /// UVマップ出力解像度
    pub uv_map_size: u32,
    /// 物理（剛体・ジョイント）なしで出力
    pub no_physics: bool,
    /// 元のボーン構造のまま出力（標準ボーン挿入スキップ）
    pub raw_structure: bool,
    /// PMX出力倍率（デフォルト: 1.0）
    pub scale: f32,
    /// converted_modelXX の作成先ベースディレクトリ（None = ソースファイルと同じ場所）
    pub output_base_dir: Option<std::path::PathBuf>,
}

impl Default for ExportState {
    fn default() -> Self {
        Self {
            output_log: false,
            pmx_output_path: String::new(),
            export_visible_only: false,
            uv_map_size: crate::convert::uvmap::DEFAULT_UV_SIZE,
            no_physics: false,
            raw_structure: false,
            scale: 1.0,
            output_base_dir: None,
        }
    }
}

impl ViewerApp {
    /// プログレスオーバーレイ描画
    pub(super) fn paint_progress_overlay(
        &self,
        viewport: &egui::Ui,
        rect: egui::Rect,
        ctx: &egui::Context,
    ) {
        let msg = if self.pending.bg_state.is_active()
            || self.pending.pkg_load.is_some()
            || self.pending.archive_load.is_some()
        {
            Some("読み込み中...")
        } else if self.pending.rebuild.is_some() || self.pending.reload.is_some() {
            Some("処理中...")
        } else if self.pending.convert.is_some() {
            Some("PMX変換中...")
        } else {
            None
        };
        let Some(msg) = msg else { return };

        let color = egui::Color32::from_rgb(0x60, 0xA0, 0xFF);
        let bar_h = 36.0_f32;
        let center = rect.center();
        let bar_rect = egui::Rect::from_center_size(
            egui::pos2(center.x, center.y),
            egui::vec2(rect.width(), bar_h),
        );
        // 背景帯
        viewport.painter().rect_filled(
            bar_rect,
            0.0,
            egui::Color32::from_rgba_unmultiplied(0, 0, 0, 0xC0),
        );
        // テキスト（中央揃え）
        viewport.painter().text(
            center,
            egui::Align2::CENTER_CENTER,
            msg,
            egui::FontId::proportional(16.0),
            color,
        );
        ctx.request_repaint();
    }

    /// プログレスフラグ更新（次フレームで処理を実行するためのトリガー）
    pub(super) fn update_progress_flags(&mut self, ctx: &egui::Context) {
        if let BackgroundLoadState::PendingDispatch {
            ref mut dispatch, ..
        } = self.pending.bg_state
        {
            if dispatch.overlay == PendingOverlay::WaitingOverlay {
                dispatch.overlay = PendingOverlay::Ready;
                ctx.request_repaint();
            }
        }
        if let Some(ref mut p) = self.pending.pkg_load {
            if !p.shown {
                p.shown = true;
                ctx.request_repaint();
            }
        }
        if let Some(ref mut p) = self.pending.archive_load {
            if !p.shown {
                p.shown = true;
                ctx.request_repaint();
            }
        }
        if self.pending.rebuild == Some(PendingOverlay::WaitingOverlay) {
            self.pending.rebuild = Some(PendingOverlay::Ready);
            ctx.request_repaint();
        }
        if self.pending.reload == Some(PendingOverlay::WaitingOverlay) {
            self.pending.reload = Some(PendingOverlay::Ready);
            ctx.request_repaint();
        }
        if self.pending.convert == Some(PendingOverlay::WaitingOverlay) {
            self.pending.convert = Some(PendingOverlay::Ready);
            ctx.request_repaint();
        }
    }

    /// Set progress toast for batch model loading
    fn set_batch_progress_message(
        &mut self,
        batch_progress: &Option<(usize, usize)>,
        model_name: &str,
    ) {
        if let Some((current, total)) = *batch_progress {
            let msg = format!("読み込み完了 ({}/{}): {}", current, total, model_name);
            self.convert_message = Some(ConvertMessage::success(msg));
        } else {
            self.convert_message = None;
        }
    }

    /// 遅延処理（ファイル読み込み、GPU再構築、PMX変換など）を実行
    pub(super) fn process_pending_tasks(&mut self) {
        // 非同期ファイルダイアログの結果をポーリング
        if let Some((kind, ref rx)) = self.pending.file_dialog {
            match rx.try_recv() {
                Ok(Some(path)) => {
                    if let Some(dir) = path.parent() {
                        self.last_model_dir = Some(dir.to_path_buf());
                    }
                    let append = kind == FileDialogKind::Append;
                    self.pending.bg_state.submit_dispatch(PendingLoadDispatch {
                        path,
                        append,
                        overlay: PendingOverlay::WaitingOverlay,
                        preloaded: None,
                    });
                    self.pending.file_dialog = None;
                }
                Ok(None) => {
                    // ユーザーがキャンセル
                    self.pending.file_dialog = None;
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {
                    // まだダイアログ表示中
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    self.pending.file_dialog = None;
                }
            }
        }

        // PendingDispatch が Ready → メインスレッドルーティング
        let dispatch_ready = matches!(
            &self.pending.bg_state,
            BackgroundLoadState::PendingDispatch { dispatch, .. }
                if dispatch.overlay == PendingOverlay::Ready
        );
        if dispatch_ready {
            let (dispatch, prior_loading) =
                match std::mem::replace(&mut self.pending.bg_state, BackgroundLoadState::Idle) {
                    BackgroundLoadState::PendingDispatch {
                        dispatch,
                        prior_loading,
                    } => (dispatch, prior_loading),
                    _ => unreachable!("dispatch_ready で PendingDispatch 確認済み"),
                };
            self.route_load_dispatch(dispatch, prior_loading);
        }
        // バックグラウンド CPU パース結果をポーリング
        if let BackgroundLoadState::Loading(ref handle) = self.pending.bg_state {
            let current_id = handle.request_id;
            match handle.rx.try_recv() {
                Ok(Ok(result)) => {
                    if result.request_id != current_id {
                        // 古い世代の結果が後から届いたケース。ハンドル自体は現世代のものなので保持し、
                        // 次の try_recv で現世代の結果を待つ。
                        log::info!(
                            "Discarding stale bg load result (req={}, current={})",
                            result.request_id,
                            current_id
                        );
                    } else {
                        self.pending.bg_state = BackgroundLoadState::Idle;
                        if let Err(e) = self.apply_bg_load_result(result) {
                            self.convert_message =
                                Some(ConvertMessage::failure(format!("{:#}", e)));
                        }
                    }
                }
                Ok(Err(e)) => {
                    self.pending.bg_state = BackgroundLoadState::Idle;
                    // キャンセル由来のエラーはユーザーが意図した中断なので UI に出さない
                    let msg = format!("{:#}", e);
                    if msg.contains("bg load cancelled") {
                        log::info!("Bg load cancelled (req={})", current_id);
                    } else {
                        self.convert_message = Some(ConvertMessage::failure(msg));
                    }
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {
                    // まだパース中
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    self.pending.bg_state = BackgroundLoadState::Idle;
                    self.convert_message = Some(ConvertMessage::failure(
                        "Background load thread panicked".to_string(),
                    ));
                }
            }
        }
        // unitypackage モデル遅延読み込み
        if self.pending.pkg_load.as_ref().is_some_and(|p| p.shown) {
            let p = self
                .pending
                .pkg_load
                .take()
                .expect("pending_pkg_load は shown 確認済み");
            let source_path = p.source_path.clone();
            let model_pathname = p
                .assets
                .get(p.fbx_index)
                .map(|a| a.pathname.as_str())
                .unwrap_or("?");

            // Batch progress info (e.g. "2/5") — stored in PendingPkgModelLoad itself
            // so it survives multi_load being set to None after the last pop
            let batch_progress = p.batch_progress;

            let model_display_name = std::path::Path::new(model_pathname)
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();

            // Show loading progress toast for batch mode
            if let Some((current, total)) = batch_progress {
                let msg = format!("読み込み中 ({}/{})：{}", current, total, model_display_name);
                self.convert_message = Some(ConvertMessage::success(msg));
            }

            log::info!(
                "Load from archive: {:?} [{}] from {}{}",
                p.model_type,
                model_pathname,
                source_path.display(),
                batch_progress
                    .map(|(c, t)| format!(" ({}/{})", c, t))
                    .unwrap_or_default()
            );

            // source_override を構築（nested_archive_source > archive_snapshot > None）
            let source_override = if let Some(nested) = p.nested_archive_source {
                Some(nested)
            } else if let Some(ref snap) = p.archive_snapshot {
                Some(ReloadableSource::Snapshot {
                    original_path: source_path.clone(),
                    main_bytes: Arc::clone(snap),
                    aux_files: HashMap::new(),
                })
            } else {
                None
            };

            // アペンドモード: unitypackage内モデルを既存モデルに追加
            if p.append {
                // リロード経由の場合はテクスチャ選択ダイアログを抑制
                if p.suppress_tex_match {
                    self.suppress_tex_match = true;
                }
                let ok = self.append_from_pkg(
                    &p.assets,
                    p.fbx_index,
                    p.model_type,
                    &source_path,
                    source_override.clone(),
                    p.pkg_index,
                );
                self.suppress_tex_match = false;
                if !ok {
                    self.pending.multi_load = None;
                }
                // 以下の通常ロードをスキップ
            } else {
                let pkg_index = p.pkg_index;
                match p.model_type {
                    PkgModelType::Fbx => {
                        if self.loaded.is_some() {
                            let has_anim = if let Some(asset) = p.assets.get(p.fbx_index) {
                                crate::fbx::animation::load_fbx_animation_from_data(&asset.data)
                                    .is_ok_and(|a| !a.is_empty())
                            } else {
                                false
                            };
                            if has_anim {
                                let fbx_name = p
                                    .assets
                                    .get(p.fbx_index)
                                    .map(|a| a.filename())
                                    .unwrap_or_default();
                                self.pending.fbx_choice = Some(PendingFbxChoice {
                                    path: std::path::PathBuf::from(&fbx_name),
                                    load_model: true,
                                    load_animation: true,
                                    pkg_context: Some(PendingFbxChoicePkg {
                                        assets: p.assets,
                                        fbx_index: p.fbx_index,
                                        source_path,
                                        archive_snapshot: p.archive_snapshot,
                                        nested_archive_source: source_override,
                                        pkg_index,
                                    }),
                                    preloaded: None,
                                });
                            } else {
                                match self.load_fbx_from_assets(
                                    &p.assets,
                                    p.fbx_index,
                                    &source_path,
                                    FbxLoadMode::ModelOnly,
                                    source_override,
                                    pkg_index.as_deref(),
                                ) {
                                    Ok(()) => {
                                        self.set_batch_progress_message(
                                            &batch_progress,
                                            &model_display_name,
                                        );
                                    }
                                    Err(e) => {
                                        log::error!("Load failed: {e}");
                                        self.convert_message = Some(ConvertMessage::failure(
                                            format!("ファイルを読み込めませんでした。\n詳細: {e}"),
                                        ));
                                        self.pending.multi_load = None;
                                    }
                                }
                            }
                        } else {
                            match self.load_fbx_from_assets(
                                &p.assets,
                                p.fbx_index,
                                &source_path,
                                FbxLoadMode::Both,
                                source_override,
                                pkg_index.as_deref(),
                            ) {
                                Ok(()) => {
                                    log::info!("Load success: {}", source_path.display());
                                    self.set_batch_progress_message(
                                        &batch_progress,
                                        &model_display_name,
                                    );
                                }
                                Err(e) => {
                                    log::error!("Load failed: {e}");
                                    self.convert_message = Some(ConvertMessage::failure(format!(
                                        "ファイルを読み込めませんでした。\n詳細: {e}"
                                    )));
                                    self.pending.multi_load = None;
                                }
                            }
                        }
                    }
                    PkgModelType::Vrm => {
                        match self.load_vrm_from_assets(
                            &p.assets,
                            p.fbx_index,
                            &source_path,
                            source_override,
                        ) {
                            Ok(()) => {
                                log::info!("Load success: {}", source_path.display());
                                self.set_batch_progress_message(
                                    &batch_progress,
                                    &model_display_name,
                                );
                            }
                            Err(e) => {
                                log::error!("Load failed: {e}");
                                self.convert_message = Some(ConvertMessage::failure(format!(
                                    "ファイルを読み込めませんでした。\n詳細: {e}"
                                )));
                                self.pending.multi_load = None;
                            }
                        }
                    }
                    PkgModelType::Prefab => {
                        match self.load_prefab_from_assets(
                            &p.assets,
                            p.fbx_index,
                            &source_path,
                            source_override,
                            pkg_index,
                        ) {
                            Ok(()) => {
                                log::info!("PrefabLoad success: {}", source_path.display());
                                self.set_batch_progress_message(
                                    &batch_progress,
                                    &model_display_name,
                                );
                            }
                            Err(e) => {
                                log::error!("PrefabLoad failed: {e}");
                                self.convert_message = Some(ConvertMessage::failure(format!(
                                    "Prefabを読み込めませんでした。\n詳細: {e}"
                                )));
                                self.pending.multi_load = None;
                            }
                        }
                    }
                }
            } // else (通常ロード)
        }
        // 複数モデル一括ロードキューの処理
        // pkg_load と fbx_choice の両方が空のときのみ次を投入
        // （FBX読み込みモード選択中にキューが進むのを防ぐ）
        if self.pending.pkg_load.is_none() && self.pending.fbx_choice.is_none() {
            if let Some(ref mut ml) = self.pending.multi_load {
                if let Some((fbx_index, model_type)) = ml.remaining.pop() {
                    // Calculate progress AFTER pop (remaining is now smaller)
                    let current = ml.total_count - ml.remaining.len();
                    self.pending.pkg_load = Some(PendingPkgModelLoad {
                        assets: ml.assets.clone(),
                        fbx_index,
                        model_type,
                        source_path: ml.source_path.clone(),
                        shown: false,
                        append: true,
                        suppress_tex_match: false,
                        archive_snapshot: ml.archive_snapshot.clone(),
                        nested_archive_source: ml.nested_archive_source.clone(),
                        pkg_index: ml.pkg_index.clone(),
                        batch_progress: Some((current, ml.total_count)),
                    });
                }
            }
            // remaining が空になったら multi_load を破棄
            if self
                .pending
                .multi_load
                .as_ref()
                .is_some_and(|ml| ml.remaining.is_empty())
            {
                self.pending.multi_load = None;
            }
        }
        // アーカイブモデル遅延読み込み
        if self.pending.archive_load.as_ref().is_some_and(|p| p.shown) {
            let p = self
                .pending
                .archive_load
                .take()
                .expect("pending_archive_load は shown 確認済み");
            let source_path = p.source_path.clone();
            match self.load_model_from_archive(p) {
                Ok(()) => {
                    log::info!("Model loaded from archive: {}", source_path.display());
                    self.convert_message = None;
                    self.anim.state = None;
                    self.anim.library.clear();
                    self.anim.active_index = None;
                }
                Err(e) => {
                    log::error!("Archive load failed: {e}");
                    self.convert_message = Some(ConvertMessage::failure(format!(
                        "アーカイブからの読み込みに失敗しました。\n詳細: {e}"
                    )));
                }
            }
        }
        if self.pending.rebuild == Some(PendingOverlay::Ready) {
            self.pending.rebuild = None;
            self.rebuild_gpu_model();
        }
        if self.pending.reload == Some(PendingOverlay::Ready) {
            self.pending.reload = None;
            self.reload_current();
        }
        if self.pending.convert == Some(PendingOverlay::Ready) {
            self.pending.convert = None;
            super::super::ui::execute_conversion(self);
        }
    }
}
