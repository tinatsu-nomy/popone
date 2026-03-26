//! テクスチャ割り当て、プレビュー、pkg テクスチャ処理

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use eframe::egui;
use eframe::wgpu;

use super::helpers::{is_temp_path, TextureSource};
use super::{ConvertMessage, GpuModel, ViewerApp};

/// テクスチャ割り当て・パッケージテクスチャ関連の状態
pub struct TextureState {
    /// 手動テクスチャ割り当て履歴（材質Index → テクスチャソース）
    pub assignments: HashMap<usize, TextureSource>,
    /// パッケージテクスチャ手動割り当て履歴（材質Index → テクスチャ名）
    pub pkg_assignments: HashMap<usize, String>,
    /// テクスチャD&Dプレビュー
    pub pending_preview: Option<PendingTexPreview>,
    /// unitypackageテクスチャ手動割当ダイアログ
    pub pending_match: Option<PendingTexMatch>,
    /// unitypackage内テクスチャ（モデル読み込み中保持）
    pub pkg_textures: Option<Vec<(String, Vec<u8>)>>,
    /// pkg_textures のサムネイル TextureId キャッシュ
    pub pkg_thumb_cache: Vec<Option<egui::TextureId>>,
    /// 同一材質名への同時テクスチャ割り当て
    pub link_same_name: bool,
    /// pkgテクスチャポップアップ用フィルタ
    pub pkg_popup_filter: String,
    /// 最後にテクスチャファイルを開いたディレクトリ
    pub last_dir: Option<PathBuf>,
}

impl Default for TextureState {
    fn default() -> Self {
        Self {
            assignments: HashMap::new(),
            pkg_assignments: HashMap::new(),
            pending_preview: None,
            pending_match: None,
            pkg_textures: None,
            pkg_thumb_cache: Vec::new(),
            link_same_name: true,
            pkg_popup_filter: String::new(),
            last_dir: None,
        }
    }
}

/// テクスチャD&Dプレビュー状態
pub struct PendingTexPreview {
    pub path: PathBuf,
    /// 読み込み済みバイトデータ（一時ファイル消失対策）
    pub cached_data: Vec<u8>,
    /// PSDファイルかどうか
    pub is_psd: bool,
    /// 一時パスから読み込まれたか（消失前に判定済み）
    pub was_temp: bool,
    /// 材質ごとの選択状態（チェックボックス）
    pub selection: Vec<bool>,
    /// 現在プレビュー適用中の材質
    pub(super) previewed: Vec<bool>,
    /// プレビュー用テクスチャビュー（GPU）
    pub(super) texture_view: wgpu::TextureView,
    /// draw_index → 退避した元の bind group
    pub(super) saved_binds: HashMap<usize, Option<wgpu::BindGroup>>,
    /// サムネイル表示用 egui TextureId
    pub preview_tex_id: Option<egui::TextureId>,
}

/// unitypackage テクスチャ手動割当ダイアログの状態
pub struct PendingTexMatch {
    /// 未割当の材質インデックス一覧（ir.materials 内のインデックス）
    pub mat_indices: Vec<usize>,
    /// 材質インデックス → 選択中のテクスチャインデックス (pkg_textures 内)
    pub selections: Vec<Option<usize>>,
    /// テクスチャ名フィルタ
    pub tex_filter: String,
}

/// UI 表示用にキャッシュされた材質情報（借用制約回避 + 毎フレーム clone 回避）
pub struct CachedMaterialInfo {
    /// (draw_index, material_index)
    pub draw_indices: Vec<(usize, usize)>,
    /// 材質名
    pub names: Vec<String>,
    /// テクスチャインデックス
    pub tex_indices: Vec<Option<usize>>,
    /// FBX 元テクスチャファイル名
    pub source_tex_names: Vec<Option<String>>,
    /// テクスチャ設定済みカウント
    pub tex_set_count: usize,
    /// ステータスバー用テクスチャ状況文字列（毎フレーム format! 回避）
    pub tex_status_text: String,
}

impl ViewerApp {
    /// 材質情報キャッシュを構築
    pub(super) fn build_mat_cache(
        ir: &crate::intermediate::types::IrModel,
        gpu_model: &GpuModel,
    ) -> CachedMaterialInfo {
        let draw_indices: Vec<(usize, usize)> = gpu_model
            .draws
            .iter()
            .enumerate()
            .map(|(i, d)| (i, d.material_index))
            .collect();
        let names: Vec<String> = ir.materials.iter().map(|m| m.name.clone()).collect();
        let tex_indices: Vec<Option<usize>> =
            ir.materials.iter().map(|m| m.texture_index).collect();
        let source_tex_names: Vec<Option<String>> = ir
            .materials
            .iter()
            .map(|m| m.source_texture_name.clone())
            .collect();
        let tex_set_count = ir
            .materials
            .iter()
            .filter(|m| m.texture_index.is_some())
            .count();
        let tex_total = ir.materials.len();
        let tex_status_text = format!("Tex:{}/{}", tex_set_count, tex_total);
        CachedMaterialInfo {
            draw_indices,
            names,
            tex_indices,
            source_tex_names,
            tex_set_count,
            tex_status_text,
        }
    }

    /// 材質キャッシュを更新（テクスチャ割り当て後）
    pub(super) fn update_mat_cache(&mut self) {
        if let Some(ref mut loaded) = self.loaded {
            loaded.mat_cache = Self::build_mat_cache(&loaded.ir, &loaded.gpu_model);
        }
    }

    /// pkg_textures のサムネイルを GPU にアップロードしてキャッシュ
    pub fn rebuild_pkg_thumb_cache(&mut self) {
        self.clear_pkg_thumb_cache();
        let Some(ref pkg) = self.tex.pkg_textures else {
            return;
        };
        let device = &self.render_state.device;
        let queue = &self.render_state.queue;
        let mut renderer = self.render_state.renderer.write();
        const THUMB_SIZE: u32 = 64;

        for (name, data) in pkg.iter() {
            let is_psd = super::super::texture::is_psd_filename(name);
            match super::super::texture::create_thumbnail_rgba(data, is_psd, THUMB_SIZE) {
                Ok(rgba) => {
                    let (view, _) = super::super::texture::upload_rgba_to_gpu(
                        device,
                        queue,
                        &rgba,
                        THUMB_SIZE,
                        THUMB_SIZE,
                        Some("pkg_thumb"),
                    );
                    let tex_id = renderer.register_native_texture(
                        device,
                        &view,
                        eframe::wgpu::FilterMode::Linear,
                    );
                    self.tex.pkg_thumb_cache.push(Some(tex_id));
                }
                Err(e) => {
                    log::warn!("サムネイル生成失敗: {} - {}", name, e);
                    self.tex.pkg_thumb_cache.push(None);
                }
            }
        }
    }

    /// サムネイルキャッシュをクリア
    pub(super) fn clear_pkg_thumb_cache(&mut self) {
        let mut renderer = self.render_state.renderer.write();
        for tex_id in self.tex.pkg_thumb_cache.drain(..).flatten() {
            renderer.free_texture(&tex_id);
        }
    }

    /// 指定材質に外部テクスチャを割り当て（ファイルパスから）
    pub fn assign_texture_to_material(&mut self, material_index: usize, path: &Path) {
        // 一時パスの場合はキャッシュ
        let tex_source = if is_temp_path(path) {
            let tex_data = match std::fs::read(path) {
                Ok(d) => d,
                Err(e) => {
                    log::error!("ファイル読み込み失敗: {e}");
                    self.convert_message = Some(ConvertMessage::failure(format!(
                        "テクスチャ読み込み失敗: {e}"
                    )));
                    return;
                }
            };
            let ext_lower = path
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("")
                .to_lowercase();
            TextureSource::Cached {
                original_name: path.to_string_lossy().into_owned(),
                data: Arc::from(tex_data.into_boxed_slice()),
                is_psd: ext_lower == "psd",
            }
        } else {
            TextureSource::File(path.to_path_buf())
        };
        self.assign_texture_source_to_material(material_index, &tex_source);
    }

    /// 指定材質に TextureSource を割り当て
    pub fn assign_texture_source_to_material(
        &mut self,
        material_index: usize,
        tex_source: &TextureSource,
    ) {
        // テクスチャデータを取得
        let (tex_data, is_psd, display_name) = match tex_source {
            TextureSource::File(path) => {
                let data = match std::fs::read(path) {
                    Ok(d) => d,
                    Err(e) => {
                        log::error!("ファイル読み込み失敗: {e}");
                        self.convert_message = Some(ConvertMessage::failure(format!(
                            "テクスチャ読み込み失敗: {e}"
                        )));
                        return;
                    }
                };
                let ext_lower = path
                    .extension()
                    .and_then(|e| e.to_str())
                    .unwrap_or("")
                    .to_lowercase();
                (
                    data,
                    ext_lower == "psd",
                    path.to_string_lossy().into_owned(),
                )
            }
            TextureSource::Cached {
                original_name,
                data,
                is_psd,
            } => (data.to_vec(), *is_psd, original_name.clone()),
        };

        let ext_lower = if is_psd {
            "psd".to_string()
        } else {
            // display_name から拡張子を取得
            Path::new(&display_name)
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("")
                .to_lowercase()
        };

        let device = &self.render_state.device;
        let queue = &self.render_state.queue;

        // GPU テクスチャをアップロード（読み込み済みバイト列を使用）
        let (texture_view, _texture_view_unorm) =
            match super::super::texture::upload_texture_from_bytes(&tex_data, is_psd, device, queue)
            {
                Ok(views) => views,
                Err(e) => {
                    log::error!("テクスチャ読み込み失敗: {e}");
                    self.convert_message = Some(ConvertMessage::failure(format!(
                        "テクスチャ読み込み失敗: {e}"
                    )));
                    return;
                }
            };

        // IrModel にテクスチャを追加・材質を更新
        let Some(ref mut loaded) = self.loaded else {
            return;
        };

        let basename = Path::new(&display_name)
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();

        // PSD の場合は PNG に変換して保存
        let (ir_data, ir_filename, ir_mime) = if is_psd {
            match Self::psd_to_png(&tex_data) {
                Ok(png_data) => (
                    png_data,
                    format!("{}.png", basename),
                    "image/png".to_string(),
                ),
                Err(e) => {
                    log::error!("PSD→PNG変換失敗: {e}");
                    self.convert_message =
                        Some(ConvertMessage::failure(format!("PSD→PNG変換失敗: {e}")));
                    return;
                }
            }
        } else {
            let filename = Path::new(&display_name)
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();
            let mime = crate::intermediate::types::mime_for_ext(&ext_lower);
            (tex_data, filename, mime.to_string())
        };

        let tex_idx = loaded
            .ir
            .textures
            .iter()
            .position(|t| {
                t.filename == ir_filename && t.data.len() == ir_data.len() && t.data == ir_data
            })
            .unwrap_or_else(|| {
                let idx = loaded.ir.textures.len();
                loaded
                    .ir
                    .textures
                    .push(crate::intermediate::types::IrTexture {
                        filename: ir_filename,
                        data: ir_data,
                        mime_type: ir_mime,
                    });
                idx
            });
        let mat = &mut loaded.ir.materials[material_index];
        mat.texture_index = Some(tex_idx);
        match mat.base_color_tex_info.as_mut() {
            Some(info) => info.index = tex_idx,
            None => {
                mat.base_color_tex_info = Some(
                    crate::intermediate::types::IrTextureInfo::from_index(tex_idx),
                )
            }
        }
        mat.apply_textured_defaults();

        // GPU DrawCall 更新（材質固有のサンプラー情報を維持）
        let texture_bgl = match self.renderer {
            Some(ref r) => r.texture_bgl(),
            None => return,
        };
        let sampler_info = loaded.ir.materials[material_index]
            .base_color_tex_info
            .as_ref()
            .map(|ti| ti.sampler)
            .unwrap_or_default();
        loaded.gpu_model.assign_texture_to_material(
            material_index,
            &texture_view,
            device,
            texture_bgl,
            &sampler_info,
        );

        log::info!(
            "テクスチャ割り当て: 材質[{}] '{}' ← {}",
            material_index,
            loaded.ir.materials[material_index].name,
            display_name
        );

        // 割り当て履歴を記録（reload_current 時の復元用）
        self.tex
            .assignments
            .insert(material_index, tex_source.clone());

        // 同一材質名への連動割り当て
        if self.tex.link_same_name {
            let target_name = loaded.ir.materials[material_index].name.clone();
            let siblings: Vec<usize> = loaded
                .ir
                .materials
                .iter()
                .enumerate()
                .filter(|(i, m)| *i != material_index && m.name == target_name)
                .map(|(i, _)| i)
                .collect();
            for sib_idx in siblings {
                let sib_mat = &mut loaded.ir.materials[sib_idx];
                sib_mat.texture_index = Some(tex_idx);
                match sib_mat.base_color_tex_info.as_mut() {
                    Some(info) => info.index = tex_idx,
                    None => {
                        sib_mat.base_color_tex_info = Some(
                            crate::intermediate::types::IrTextureInfo::from_index(tex_idx),
                        )
                    }
                }
                sib_mat.apply_textured_defaults();
                let sib_sampler_info = loaded.ir.materials[sib_idx]
                    .base_color_tex_info
                    .as_ref()
                    .map(|ti| ti.sampler)
                    .unwrap_or_default();
                loaded.gpu_model.assign_texture_to_material(
                    sib_idx,
                    &texture_view,
                    device,
                    texture_bgl,
                    &sib_sampler_info,
                );
                self.tex.assignments.insert(sib_idx, tex_source.clone());
                log::info!("  連動割り当て: 材質[{}] '{}'", sib_idx, target_name);
            }
        }

        // 材質キャッシュ更新
        self.update_mat_cache();
    }

    /// パッケージ内テクスチャデータを材質に割り当て（バイト列から直接）
    pub fn assign_texture_data_to_material(
        &mut self,
        material_index: usize,
        tex_name: &str,
        tex_data: &[u8],
    ) {
        let is_psd = super::super::texture::is_psd_filename(tex_name);

        let device = &self.render_state.device;
        let queue = &self.render_state.queue;

        let (texture_view, _texture_view_unorm) =
            match super::super::texture::upload_texture_from_bytes(tex_data, is_psd, device, queue)
            {
                Ok(views) => views,
                Err(e) => {
                    log::error!("テクスチャデコード失敗: {e}");
                    self.convert_message = Some(ConvertMessage::failure(format!(
                        "テクスチャデコード失敗: {e}"
                    )));
                    return;
                }
            };

        let Some(ref mut loaded) = self.loaded else {
            return;
        };

        // IrModel にテクスチャを追加
        let basename = std::path::Path::new(tex_name)
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        let (ir_data, ir_filename, ir_mime) = if is_psd {
            match Self::psd_to_png(tex_data) {
                Ok(png_data) => (
                    png_data,
                    format!("{}.png", basename),
                    "image/png".to_string(),
                ),
                Err(e) => {
                    log::warn!("PSD→PNG変換失敗 (IrTexture用): {e}");
                    (tex_data.to_vec(), tex_name.to_string(), String::new())
                }
            }
        } else {
            (tex_data.to_vec(), tex_name.to_string(), String::new())
        };

        let tex_idx = loaded.ir.textures.len();
        loaded
            .ir
            .textures
            .push(crate::intermediate::types::IrTexture {
                filename: ir_filename,
                data: ir_data,
                mime_type: ir_mime,
            });
        let mat = &mut loaded.ir.materials[material_index];
        mat.texture_index = Some(tex_idx);
        match mat.base_color_tex_info.as_mut() {
            Some(info) => info.index = tex_idx,
            None => {
                mat.base_color_tex_info = Some(
                    crate::intermediate::types::IrTextureInfo::from_index(tex_idx),
                )
            }
        }
        mat.apply_textured_defaults();

        // GPU DrawCall 更新（材質固有のサンプラー情報を維持）
        let texture_bgl = match self.renderer {
            Some(ref r) => r.texture_bgl(),
            None => return,
        };
        let sampler_info = loaded.ir.materials[material_index]
            .base_color_tex_info
            .as_ref()
            .map(|ti| ti.sampler)
            .unwrap_or_default();
        loaded.gpu_model.assign_texture_to_material(
            material_index,
            &texture_view,
            device,
            texture_bgl,
            &sampler_info,
        );

        log::info!(
            "パッケージテクスチャ割り当て: 材質[{}] '{}' ← {}",
            material_index,
            loaded.ir.materials[material_index].name,
            tex_name,
        );

        // 同一材質名への連動割り当て
        if self.tex.link_same_name {
            let target_name = loaded.ir.materials[material_index].name.clone();
            let siblings: Vec<usize> = loaded
                .ir
                .materials
                .iter()
                .enumerate()
                .filter(|(i, m)| *i != material_index && m.name == target_name)
                .map(|(i, _)| i)
                .collect();
            for sib_idx in siblings {
                let sib_mat = &mut loaded.ir.materials[sib_idx];
                sib_mat.texture_index = Some(tex_idx);
                match sib_mat.base_color_tex_info.as_mut() {
                    Some(info) => info.index = tex_idx,
                    None => {
                        sib_mat.base_color_tex_info = Some(
                            crate::intermediate::types::IrTextureInfo::from_index(tex_idx),
                        )
                    }
                }
                sib_mat.apply_textured_defaults();
                let sib_sampler_info = loaded.ir.materials[sib_idx]
                    .base_color_tex_info
                    .as_ref()
                    .map(|ti| ti.sampler)
                    .unwrap_or_default();
                loaded.gpu_model.assign_texture_to_material(
                    sib_idx,
                    &texture_view,
                    device,
                    texture_bgl,
                    &sib_sampler_info,
                );
                log::info!("  連動割り当て: 材質[{}] '{}'", sib_idx, target_name);
            }
        }

        self.update_mat_cache();
    }

    /// 1枚のテクスチャをプレビューダイアログで開く
    pub(super) fn open_texture_preview(&mut self, path: PathBuf) {
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();
        let is_psd = ext == "psd";
        // ファイル消失前に一時パス判定を確定（canonicalize がファイル存在を前提とするため）
        let was_temp = is_temp_path(&path);
        let data = match std::fs::read(&path) {
            Ok(d) => d,
            Err(e) => {
                self.convert_message = Some(ConvertMessage::failure(format!(
                    "テクスチャ読み込み失敗: {e}"
                )));
                return;
            }
        };
        match super::super::texture::upload_texture_from_bytes(
            &data,
            is_psd,
            &self.render_state.device,
            &self.render_state.queue,
        ) {
            Ok((texture_view, _texture_view_unorm)) => {
                let num_mats = self.loaded.as_ref().map_or(0, |l| l.ir.materials.len());
                let preview_tex_id = {
                    let mut renderer = self.render_state.renderer.write();
                    Some(renderer.register_native_texture(
                        &self.render_state.device,
                        &texture_view,
                        wgpu::FilterMode::Linear,
                    ))
                };
                self.tex.pending_preview = Some(PendingTexPreview {
                    path,
                    cached_data: data,
                    is_psd,
                    was_temp,
                    selection: vec![false; num_mats],
                    previewed: vec![false; num_mats],
                    texture_view,
                    saved_binds: HashMap::new(),
                    preview_tex_id,
                });
            }
            Err(e) => {
                self.convert_message = Some(ConvertMessage::failure(format!(
                    "テクスチャ読み込み失敗: {e}"
                )));
            }
        }
    }

    /// 複数テクスチャの自動割り当て（ファイル名と材質名のマッチング）
    pub(super) fn auto_assign_textures(&mut self, image_files: Vec<PathBuf>) {
        let Some(ref loaded) = self.loaded else {
            return;
        };
        let mat_names: Vec<String> = loaded
            .ir
            .materials
            .iter()
            .map(|m| m.name.to_lowercase())
            .collect();

        let mut assigned = 0usize;
        let mut unmatched: Vec<String> = Vec::new();

        // ファイル名 → マッチする材質インデックスを収集
        let mut assignments: Vec<(PathBuf, Vec<usize>)> = Vec::new();
        for path in &image_files {
            let stem = path
                .file_stem()
                .unwrap_or_default()
                .to_string_lossy()
                .to_lowercase();
            if stem.is_empty() {
                continue;
            }
            // 材質名にファイル名（拡張子なし）を含む材質を検索
            let matched: Vec<usize> = mat_names
                .iter()
                .enumerate()
                .filter(|(_, name)| name.contains(&stem) || stem.contains(name.as_str()))
                .map(|(i, _)| i)
                .collect();
            if matched.is_empty() {
                unmatched.push(
                    path.file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .to_string(),
                );
            } else {
                assignments.push((path.clone(), matched));
            }
        }

        // 割り当て実行
        for (path, mat_indices) in assignments {
            for &mat_idx in &mat_indices {
                self.assign_texture_to_material(mat_idx, &path);
                assigned += 1;
            }
        }

        // 結果メッセージ
        let mut msg = format!("テクスチャ自動割り当て: {}材質に適用", assigned);
        if !unmatched.is_empty() {
            msg += &format!("\nマッチなし: {}", unmatched.join(", "));
        }
        if assigned > 0 {
            self.convert_message = Some(ConvertMessage::success(msg));
        } else {
            self.convert_message = Some(ConvertMessage::failure(format!(
                "マッチする材質が見つかりませんでした\nファイル: {}",
                unmatched.join(", ")
            )));
        }
    }

    /// テクスチャプレビューの同期（selection と previewed の差分を GPU に反映）
    pub fn sync_tex_preview(&mut self) {
        let Some(ref mut preview) = self.tex.pending_preview else {
            return;
        };
        let Some(ref mut loaded) = self.loaded else {
            return;
        };
        let Some(ref renderer) = self.renderer else {
            return;
        };
        let device = &self.render_state.device;
        let texture_bgl = renderer.texture_bgl();
        let sampler = renderer.sampler();

        for mat_idx in 0..preview.selection.len() {
            if preview.selection[mat_idx] && !preview.previewed[mat_idx] {
                // プレビュー適用: 元の bind group を退避し、プレビュー用に差し替え
                for (draw_idx, draw) in loaded.gpu_model.draws.iter_mut().enumerate() {
                    if draw.material_index == mat_idx {
                        if let std::collections::hash_map::Entry::Vacant(e) =
                            preview.saved_binds.entry(draw_idx)
                        {
                            e.insert(draw.texture_bind_group.take());
                        }
                        draw.texture_bind_group =
                            Some(super::super::gpu::create_texture_bind_group(
                                device,
                                texture_bgl,
                                &preview.texture_view,
                                sampler,
                            ));
                    }
                }
                preview.previewed[mat_idx] = true;
            } else if !preview.selection[mat_idx] && preview.previewed[mat_idx] {
                // プレビュー解除: 退避した元の bind group を復元
                for (draw_idx, draw) in loaded.gpu_model.draws.iter_mut().enumerate() {
                    if draw.material_index == mat_idx {
                        if let Some(orig) = preview.saved_binds.remove(&draw_idx) {
                            draw.texture_bind_group = orig;
                        }
                    }
                }
                preview.previewed[mat_idx] = false;
            }
        }
    }

    /// テクスチャプレビューを確定適用
    pub fn apply_tex_preview(&mut self) {
        let Some(preview) = self.tex.pending_preview.take() else {
            return;
        };
        let path = &preview.path;

        // 選択された材質のインデックスを収集
        let selected: Vec<usize> = preview
            .selection
            .iter()
            .enumerate()
            .filter_map(|(i, &v)| if v { Some(i) } else { None })
            .collect();

        if selected.is_empty() {
            // 何も選択されていなければ元に戻す
            self.cancel_tex_preview_inner(preview);
            return;
        }

        // IrModel にテクスチャを追加（1回だけ）
        let Some(ref mut loaded) = self.loaded else {
            return;
        };

        let is_psd = preview.is_psd;
        let tex_data = preview.cached_data.clone();

        // 一時パスの場合はキャッシュ用にバイト列を保持（消失前に判定済みのフラグを使用）
        let cached_data = if preview.was_temp {
            Some(tex_data.clone())
        } else {
            None
        };

        let basename = path
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();

        let (ir_data, ir_filename, ir_mime) = if is_psd {
            match Self::psd_to_png(&tex_data) {
                Ok(png_data) => (
                    png_data,
                    format!("{}.png", basename),
                    "image/png".to_string(),
                ),
                Err(e) => {
                    log::error!("PSD→PNG変換失敗: {e}");
                    return;
                }
            }
        } else {
            let filename = path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();
            let ext_l = path
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("")
                .to_lowercase();
            let mime = crate::intermediate::types::mime_for_ext(&ext_l);
            (tex_data, filename, mime.to_string())
        };

        let tex_idx = loaded.ir.textures.len();
        loaded
            .ir
            .textures
            .push(crate::intermediate::types::IrTexture {
                filename: ir_filename,
                data: ir_data,
                mime_type: ir_mime,
            });

        // 選択した材質の texture_index を更新
        let path_buf = path.clone();
        for &mat_idx in &selected {
            let mat = &mut loaded.ir.materials[mat_idx];
            mat.texture_index = Some(tex_idx);
            mat.apply_textured_defaults();
            log::info!(
                "テクスチャ割り当て: 材質[{}] '{}' ← {}",
                mat_idx,
                mat.name,
                path_buf.display()
            );
        }

        // 割り当て履歴を記録（reload_current 時の復元用）
        let tex_src = if let Some(data) = cached_data {
            TextureSource::Cached {
                original_name: path_buf.to_string_lossy().into_owned(),
                data: Arc::from(data.into_boxed_slice()),
                is_psd,
            }
        } else {
            TextureSource::File(path_buf.clone())
        };
        for &mat_idx in &selected {
            self.tex.assignments.insert(mat_idx, tex_src.clone());
        }

        // サムネイル用 egui テクスチャを解放
        if let Some(tex_id) = preview.preview_tex_id {
            let mut renderer = self.render_state.renderer.write();
            renderer.free_texture(&tex_id);
        }

        // GPU は既にプレビュー状態 → saved_binds を捨てて確定
        // saved_binds 内の未プレビュー分は復元
        for (draw_idx, orig) in preview.saved_binds.into_iter() {
            let draw = &mut loaded.gpu_model.draws[draw_idx];
            if !selected.contains(&draw.material_index) {
                draw.texture_bind_group = orig;
            }
        }

        // 材質キャッシュ更新
        self.update_mat_cache();
    }

    /// テクスチャプレビューをキャンセル（元に戻す）
    pub fn cancel_tex_preview(&mut self) {
        let Some(preview) = self.tex.pending_preview.take() else {
            return;
        };
        self.cancel_tex_preview_inner(preview);
    }

    pub(super) fn cancel_tex_preview_inner(&mut self, preview: PendingTexPreview) {
        // サムネイル用 egui テクスチャを解放
        if let Some(tex_id) = preview.preview_tex_id {
            let mut renderer = self.render_state.renderer.write();
            renderer.free_texture(&tex_id);
        }
        let Some(ref mut loaded) = self.loaded else {
            return;
        };
        // 退避した全 bind group を復元
        for (draw_idx, orig) in preview.saved_binds.into_iter() {
            if draw_idx < loaded.gpu_model.draws.len() {
                loaded.gpu_model.draws[draw_idx].texture_bind_group = orig;
            }
        }
    }
}
