//! 遅延タスク処理（PendingState, ExportState, process_pending_tasks 等）

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use eframe::egui;

use crate::unitypackage::UnityPackageIndex;

use super::helpers::{FbxLoadMode, PkgModelType, ReloadableSource};
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
}

/// unitypackage モデル遅延読み込み状態
pub struct PendingPkgModelLoad {
    pub assets: Vec<crate::unitypackage::ExtractedAsset>,
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
    pub assets: Vec<crate::unitypackage::ExtractedAsset>,
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
    /// アーカイブモデル遅延読み込み
    pub archive_load: Option<PendingArchiveLoad>,
    /// ファイル読み込み遅延実行 (path, overlay表示済みフラグ)
    pub load: Option<(PathBuf, bool)>,
    /// モデル追加読み込み遅延実行 (path, overlay表示済みフラグ)
    pub append: Option<(PathBuf, bool)>,
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
}

impl Default for PendingState {
    fn default() -> Self {
        Self {
            fbx_choice: None,
            unity_pkg: None,
            pkg_load: None,
            archive: None,
            archive_load: None,
            load: None,
            append: None,
            convert: None,
            rebuild: None,
            reload: None,
            refit: false,
            confirm_save_tex_history: false,
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
        let msg = if self.pending.load.is_some()
            || self.pending.append.is_some()
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
        if let Some((_, ref mut shown)) = self.pending.load {
            if !*shown {
                *shown = true;
                ctx.request_repaint();
            }
        }
        if let Some((_, ref mut shown)) = self.pending.append {
            if !*shown {
                *shown = true;
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

    /// 遅延処理（ファイル読み込み、GPU再構築、PMX変換など）を実行
    pub(super) fn process_pending_tasks(&mut self) {
        if let Some((_, true)) = self.pending.load {
            let (path, _) = self
                .pending
                .load
                .take()
                .expect("pending_load は Some(true) 確認済み");
            self.load_file(path);
        }
        // モデル追加読み込み（アペンド）
        if let Some((_, true)) = self.pending.append {
            let (path, _) = self
                .pending
                .append
                .take()
                .expect("pending_append は Some(true) 確認済み");
            self.append_model(path);
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
            log::info!(
                "Load from archive: {:?} [{}] from {}",
                p.model_type,
                model_pathname,
                source_path.display()
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
                self.append_from_pkg(
                    p.assets,
                    p.fbx_index,
                    p.model_type,
                    &source_path,
                    source_override.clone(),
                );
                self.suppress_tex_match = false;
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
                                    p.assets,
                                    p.fbx_index,
                                    &source_path,
                                    FbxLoadMode::ModelOnly,
                                    source_override,
                                    pkg_index.as_deref(),
                                ) {
                                    Ok(()) => {
                                        self.convert_message = None;
                                    }
                                    Err(e) => {
                                        log::error!("Load failed: {e}");
                                        self.convert_message = Some(ConvertMessage::failure(
                                            format!("ファイルを読み込めませんでした。\n詳細: {e}"),
                                        ));
                                    }
                                }
                            }
                        } else {
                            match self.load_fbx_from_assets(
                                p.assets,
                                p.fbx_index,
                                &source_path,
                                FbxLoadMode::Both,
                                source_override,
                                pkg_index.as_deref(),
                            ) {
                                Ok(()) => {
                                    log::info!("Load success: {}", source_path.display());
                                    self.convert_message = None;
                                }
                                Err(e) => {
                                    log::error!("Load failed: {e}");
                                    self.convert_message = Some(ConvertMessage::failure(format!(
                                        "ファイルを読み込めませんでした。\n詳細: {e}"
                                    )));
                                }
                            }
                        }
                    }
                    PkgModelType::Vrm => {
                        match self.load_vrm_from_assets(
                            p.assets,
                            p.fbx_index,
                            &source_path,
                            source_override,
                        ) {
                            Ok(()) => {
                                log::info!("Load success: {}", source_path.display());
                                self.convert_message = None;
                            }
                            Err(e) => {
                                log::error!("Load failed: {e}");
                                self.convert_message = Some(ConvertMessage::failure(format!(
                                    "ファイルを読み込めませんでした。\n詳細: {e}"
                                )));
                            }
                        }
                    }
                    PkgModelType::Prefab => {
                        match self.load_prefab_from_assets(
                            p.assets,
                            p.fbx_index,
                            &source_path,
                            source_override,
                            pkg_index,
                        ) {
                            Ok(()) => {
                                log::info!("PrefabLoad success: {}", source_path.display());
                                self.convert_message = None;
                            }
                            Err(e) => {
                                log::error!("PrefabLoad failed: {e}");
                                self.convert_message = Some(ConvertMessage::failure(format!(
                                    "Prefabを読み込めませんでした。\n詳細: {e}"
                                )));
                            }
                        }
                    }
                }
            } // else (通常ロード)
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
