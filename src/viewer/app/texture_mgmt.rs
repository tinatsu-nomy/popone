//! テクスチャ割り当て、プレビュー、pkg テクスチャ処理

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use eframe::egui;
use eframe::wgpu;

/// PSD→PNG バックグラウンド変換の結果型
type PsdConversionResult = anyhow::Result<Vec<u8>>;

/// PSD→PNG バックグラウンド変換の保留状態
pub struct PendingPsdConversion {
    /// 変換結果の受信チャネル
    pub rx: std::sync::mpsc::Receiver<PsdConversionResult>,
    /// 変換完了後に差し替える IrTexture のインデックス
    pub tex_idx: usize,
    /// 変換完了後に設定する PNG ファイル名（例: "foo.png"）
    pub png_filename: String,
    /// 元の表示名（ログ出力用）
    pub display_name: String,
}

use super::helpers::{is_temp_path, TextureSource};
use super::{ConvertMessage, GpuModel, ViewerApp};
use crate::intermediate::types::{TextureData, TextureSlot};

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
    pub pkg_textures: Option<Vec<(String, Arc<[u8]>)>>,
    /// pkg_textures のサムネイル TextureId キャッシュ
    pub pkg_thumb_cache: Vec<Option<egui::TextureId>>,
    /// 同一材質名への同時テクスチャ割り当て
    pub link_same_name: bool,
    /// pkgテクスチャポップアップ用フィルタ
    pub pkg_popup_filter: String,
    /// 最後にテクスチャファイルを開いたディレクトリ
    pub last_dir: Option<PathBuf>,
    /// 非同期テクスチャファイルダイアログ（材質Index, TextureSlot, 結果受信チャネル）
    /// Step 4-16b: slot 情報を追加し、各セクションのテクスチャ選択ボタンから起動可能にした。
    pub pending_file_dialog: Option<(
        usize,
        TextureSlot,
        std::sync::mpsc::Receiver<Option<PathBuf>>,
    )>,
    /// PSD→PNG バックグラウンド変換の保留リスト
    pub pending_psd_conversions: Vec<PendingPsdConversion>,
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
            pending_file_dialog: None,
            pending_psd_conversions: Vec::new(),
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
    pub previewed: Vec<bool>,
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
    /// 現在プレビュー適用中の選択状態
    pub previewed: Vec<Option<usize>>,
    /// draw_index → 退避した元の (texture_bind_group, mmd_texture_bind_group)
    pub saved_binds: HashMap<usize, (Option<wgpu::BindGroup>, Option<wgpu::BindGroup>)>,
    /// pkg テクスチャの GPU TextureView（インデックス対応）
    pub texture_views: Vec<Option<wgpu::TextureView>>,
    /// アップロード失敗済みテクスチャインデックス（再試行防止）
    pub failed_uploads: std::collections::HashSet<usize>,
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
                    log::warn!("Thumbnail generation failed: {} - {}", name, e);
                    self.tex.pkg_thumb_cache.push(None);
                }
            }
        }
    }

    /// pkg_textures の新規追加分のみサムネイルを生成して追記する（差分更新）。
    /// `start_index` 以降のエントリが新規追加分。
    pub fn append_pkg_thumb_cache(&mut self, start_index: usize) {
        let Some(ref pkg) = self.tex.pkg_textures else {
            return;
        };
        if start_index >= pkg.len() {
            return;
        }
        let device = &self.render_state.device;
        let queue = &self.render_state.queue;
        let mut renderer = self.render_state.renderer.write();
        const THUMB_SIZE: u32 = 64;

        for (name, data) in pkg[start_index..].iter() {
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
                    log::warn!("Thumbnail generation failed: {} - {}", name, e);
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
        // ファイルを1回だけ読み込む（二重読み込み回避）
        let tex_data = match std::fs::read(path) {
            Ok(d) => d,
            Err(e) => {
                log::error!("File read failed: {e}");
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
        let data_arc = Arc::from(tex_data.into_boxed_slice());
        // Cached として渡し、assign_texture_source_to_material 内での再読み込みを回避
        let cached_source = TextureSource::Cached {
            original_name: path.to_string_lossy().into_owned(),
            data: Arc::clone(&data_arc),
            is_psd: ext_lower == "psd",
        };
        // 履歴用: 一時パスは Cached、通常パスは File で保存（reload 時に再読み込み可能に）
        let history_source = if is_temp_path(path) {
            TextureSource::Cached {
                original_name: path.to_string_lossy().into_owned(),
                data: data_arc,
                is_psd: ext_lower == "psd",
            }
        } else {
            TextureSource::File(path.to_path_buf())
        };
        self.assign_texture_source_to_material(material_index, &cached_source);
        // 履歴を上書き（通常ファイルパスの場合は File で保存し、メモリ使用量を抑える）
        self.tex
            .assignments
            .insert(material_index, history_source.clone());
        if self.tex.link_same_name {
            if let Some(ref loaded) = self.loaded {
                let siblings = loaded.same_name_siblings(material_index);
                for sib_idx in siblings {
                    self.tex.assignments.insert(sib_idx, history_source.clone());
                }
            }
        }
    }

    /// 指定材質に TextureSource を割り当て
    pub fn assign_texture_source_to_material(
        &mut self,
        material_index: usize,
        tex_source: &TextureSource,
    ) {
        // TextureSource からバイト列を取得
        let (tex_data, is_psd, display_name) = match tex_source {
            TextureSource::File(path) => {
                let data = match std::fs::read(path) {
                    Ok(d) => d,
                    Err(e) => {
                        log::error!("File read failed: {e}");
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

        if !self.assign_texture_core(
            material_index,
            TextureSlot::BaseColor,
            &tex_data,
            is_psd,
            &display_name,
        ) {
            return;
        }

        // File source: assign_texture_to_material that wraps this handles history override
        self.tex
            .assignments
            .insert(material_index, tex_source.clone());
        if self.tex.link_same_name {
            if let Some(ref loaded) = self.loaded {
                let siblings = loaded.same_name_siblings(material_index);
                for sib_idx in siblings {
                    self.tex.assignments.insert(sib_idx, tex_source.clone());
                }
            }
        }

        self.update_mat_cache();
    }

    /// パッケージ内テクスチャデータを材質に割り当て（バイト列から直接）
    /// 成功時は true、デコード/アップロード失敗時は false を返す
    pub fn assign_texture_data_to_material(
        &mut self,
        material_index: usize,
        tex_name: &str,
        tex_data: &[u8],
    ) -> bool {
        let is_psd = super::super::texture::is_psd_filename(tex_name);
        if !self.assign_texture_core(
            material_index,
            TextureSlot::BaseColor,
            tex_data,
            is_psd,
            tex_name,
        ) {
            return false;
        }
        self.update_mat_cache();
        true
    }

    /// GPU upload, IrTexture registration, material update, PSD BG conversion,
    /// linked sibling assignment -- shared by both file-path and raw-byte callers.
    /// Returns false on upload failure or missing loaded model.
    ///
    /// `slot` 引数は §B で導入された `TextureSlot` enum に対応する。Step 4-16a で
    /// 全 11 スロットの書き込み経路を実装。BaseColor は既存の texture_bind_group
    /// 即時更新、他のスロットは IrMaterial フィールド書き換え + `mark_material_dirty`
    /// → `rebuild_material_bind_groups` で bind group 再生成。
    pub(crate) fn assign_texture_core(
        &mut self,
        material_index: usize,
        slot: TextureSlot,
        tex_data: &[u8],
        is_psd: bool,
        display_name: &str,
    ) -> bool {
        let device = &self.render_state.device;
        let queue = &self.render_state.queue;

        let (texture_view, texture_view_unorm) =
            match super::super::texture::upload_texture_from_bytes(tex_data, is_psd, device, queue)
            {
                Ok(views) => views,
                Err(e) => {
                    log::error!("Texture upload failed: {e}");
                    self.convert_message = Some(ConvertMessage::failure(format!(
                        "テクスチャ読み込み失敗: {e}"
                    )));
                    return false;
                }
            };

        let Some(ref mut loaded) = self.loaded else {
            return false;
        };

        let basename = Path::new(display_name)
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();

        let ext_lower = if is_psd {
            "psd".to_string()
        } else {
            Path::new(display_name)
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("")
                .to_lowercase()
        };

        // PSD: keep raw PSD data temporarily; non-PSD: derive filename/mime from display_name
        let (ir_data, ir_filename, ir_mime, spawn_psd_bg) = if is_psd {
            let psd_filename = format!("{}.psd", basename);
            (
                tex_data.to_vec(),
                psd_filename,
                "image/vnd.adobe.photoshop".to_string(),
                true,
            )
        } else {
            let filename = Path::new(display_name)
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();
            let mime = crate::intermediate::types::mime_for_ext(&ext_lower);
            (tex_data.to_vec(), filename, mime.to_string(), false)
        };

        // Reuse existing IrTexture with same name+content (dedup)
        // review_012 [P1] 対応: 新規テクスチャの場合は gpu_texture_views にも push する。
        // 既存テクスチャ再利用時は push しない（TODO-1: dedup 条件で整合維持）。
        let existing_idx = loaded.ir.textures.iter().position(|t| {
            t.filename == ir_filename
                && t.data.len() == ir_data.len()
                && t.data.as_bytes() == ir_data
        });
        let tex_idx = if let Some(idx) = existing_idx {
            idx
        } else {
            let idx = loaded.ir.textures.len();
            loaded
                .ir
                .textures
                .push(crate::intermediate::types::IrTexture {
                    filename: ir_filename,
                    data: TextureData::Encoded(Arc::from(ir_data)),
                    mime_type: ir_mime,
                    source_path: display_name.to_string(),
                    mip_chain: None,
                });
            // [P1] 新規テクスチャ: GPU view 配列にも追加（rebuild 時に tex_idx から引けるように）
            loaded
                .gpu_model
                .push_gpu_texture_view(texture_view.clone(), texture_view_unorm.clone());
            idx
        };

        // PSD BG conversion
        if spawn_psd_bg {
            let psd_data = loaded.ir.textures[tex_idx].data.as_bytes().to_vec();
            let png_filename = format!("{}.png", basename);
            let (tx, rx) = std::sync::mpsc::channel();
            std::thread::spawn(move || {
                let result = crate::psd::psd_to_png(&psd_data);
                let _ = tx.send(result);
            });
            log::info!("PSD->PNG background conversion started: {}", display_name);
            self.tex.pending_psd_conversions.push(PendingPsdConversion {
                rx,
                tex_idx,
                png_filename,
                display_name: display_name.to_string(),
            });
        }

        // Step 4-16a: slot に応じて IrMaterial の対応フィールドにテクスチャを設定
        Self::apply_texture_to_slot(&mut loaded.ir.materials[material_index], slot, tex_idx);

        // GPU DrawCall update:
        // - BaseColor: 既存の texture_bind_group 即時更新（遅延なし）
        // - 他のスロット: mark_material_dirty で次フレームに rebuild_material_bind_groups
        //   が全 bind group を再生成（mtoon_aux_bind_group 含む）
        let needs_immediate_gpu_update = slot == TextureSlot::BaseColor;
        if needs_immediate_gpu_update {
            let texture_bgl = match self.renderer {
                Some(ref r) => r.texture_bgl(),
                None => return false,
            };
            Self::update_gpu_bind(
                &mut loaded.gpu_model,
                material_index,
                &texture_view,
                device,
                texture_bgl,
                &loaded.ir.materials[material_index],
            );
        }

        log::info!(
            "Texture assignment: mat[{}] '{}' slot={:?} <- {}",
            material_index,
            loaded.ir.materials[material_index].name,
            slot,
            display_name
        );

        // Linked sibling assignment
        if self.tex.link_same_name {
            let siblings = loaded.same_name_siblings(material_index);
            for sib_idx in siblings {
                Self::apply_texture_to_slot(&mut loaded.ir.materials[sib_idx], slot, tex_idx);
                if needs_immediate_gpu_update {
                    let texture_bgl = match self.renderer {
                        Some(ref r) => r.texture_bgl(),
                        None => continue,
                    };
                    Self::update_gpu_bind(
                        &mut loaded.gpu_model,
                        sib_idx,
                        &texture_view,
                        device,
                        texture_bgl,
                        &loaded.ir.materials[sib_idx],
                    );
                }
                log::info!("  Linked assignment: mat[{}] slot={:?}", sib_idx, slot);
            }
        }

        // 非 BaseColor スロットは mark_material_dirty で rebuild を予約。
        // loaded の mut borrow を先に終わらせるため、ここで true を返してから
        // 呼び出し元で mark_material_dirty を呼ぶ設計にするか、あるいは
        // ここで直接呼ぶ（loaded は上の if 分岐後は使わないので NLL で許容される）。
        if !needs_immediate_gpu_update {
            // loaded の borrow は上のブロックで終了しているため、self を mut で借りられる
            self.mark_material_dirty(material_index);
            if self.tex.link_same_name {
                if let Some(ref loaded) = self.loaded {
                    let siblings = loaded.same_name_siblings(material_index);
                    for sib_idx in siblings {
                        self.mark_material_dirty(sib_idx);
                    }
                }
            }
        }

        true
    }

    /// Step 4-16a: slot に応じて IrMaterial の対応フィールドにテクスチャインデックスを設定。
    ///
    /// - BaseColor: 既存の texture_index + base_color_tex_info + apply_textured_defaults
    /// - Emissive / Normal: IrMaterial 直接フィールド
    /// - Shade / ShadingShift / Rim / OutlineWidth / Matcap / UvAnimMask: MtoonParams フィールド
    ///   + review_012 [P2]: `shader_family = Mtoon` に同期（描画側が MToon パスで aux bind
    ///     group を組むための前提条件）
    /// - Sphere / Toon: MMD 専用フィールド
    fn apply_texture_to_slot(
        mat: &mut crate::intermediate::types::IrMaterial,
        slot: TextureSlot,
        tex_idx: usize,
    ) {
        use crate::intermediate::types::{IrTextureInfo, ShaderFamily};

        // review_012 [P2]: MToon 系スロットを割り当てた場合、shader_family を Mtoon に同期する。
        // 描画側が shader_family 主軸判定 (review_007) で MToon パスを選択するために必要。
        // §G の方針「ユーザーの明示的操作」にテクスチャスロット割当も含む。
        let is_mtoon_slot = matches!(
            slot,
            TextureSlot::ShadeMultiply
                | TextureSlot::ShadingShift
                | TextureSlot::RimMultiply
                | TextureSlot::OutlineWidth
                | TextureSlot::Matcap
                | TextureSlot::UvAnimMask
        );
        if is_mtoon_slot {
            mat.shader_family = ShaderFamily::Mtoon;
        }

        match slot {
            TextureSlot::BaseColor => {
                mat.texture_index = Some(tex_idx);
                match mat.base_color_tex_info.as_mut() {
                    Some(info) => info.index = tex_idx,
                    None => {
                        mat.base_color_tex_info = Some(IrTextureInfo::from_index(tex_idx));
                    }
                }
                mat.apply_textured_defaults();
            }
            TextureSlot::Emissive => {
                mat.emissive_texture = Some(IrTextureInfo::from_index(tex_idx));
            }
            TextureSlot::Normal => {
                mat.normal_texture = Some(IrTextureInfo::from_index(tex_idx));
            }
            TextureSlot::ShadeMultiply => {
                mat.mtoon_mut().shade_texture = Some(IrTextureInfo::from_index(tex_idx));
            }
            TextureSlot::ShadingShift => {
                mat.mtoon_mut().shading_shift_texture = Some(IrTextureInfo::from_index(tex_idx));
            }
            TextureSlot::RimMultiply => {
                mat.mtoon_mut().rim_multiply_texture = Some(IrTextureInfo::from_index(tex_idx));
            }
            TextureSlot::OutlineWidth => {
                mat.mtoon_mut().outline_width_texture = Some(IrTextureInfo::from_index(tex_idx));
            }
            TextureSlot::Matcap => {
                mat.mtoon_mut().matcap_texture = Some(IrTextureInfo::from_index(tex_idx));
            }
            TextureSlot::UvAnimMask => {
                mat.mtoon_mut().uv_animation_mask_texture =
                    Some(IrTextureInfo::from_index(tex_idx));
            }
            TextureSlot::Sphere => {
                mat.sphere_texture_index = Some(tex_idx);
                if mat.sphere_mode == 0 {
                    mat.sphere_mode = 1; // 乗算がデフォルト
                }
            }
            TextureSlot::Toon => {
                mat.toon_texture_index = Some(tex_idx);
                mat.toon_shared_index = None; // 個別トゥーンに切替
            }
        }
    }

    /// Update GPU bind group for a material and clear stale MMD bind group.
    fn update_gpu_bind(
        gpu_model: &mut GpuModel,
        material_index: usize,
        texture_view: &wgpu::TextureView,
        device: &wgpu::Device,
        texture_bgl: &wgpu::BindGroupLayout,
        mat: &crate::intermediate::types::IrMaterial,
    ) {
        let sampler_info = mat
            .base_color_tex_info
            .as_ref()
            .map(|ti| ti.sampler)
            .unwrap_or_default();
        gpu_model.assign_texture_to_material(
            material_index,
            texture_view,
            device,
            texture_bgl,
            &sampler_info,
        );
        for draw in &mut gpu_model.draws {
            if draw.material_index == material_index {
                draw.mmd_texture_bind_group = None;
            }
        }
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

        // PSD の場合は一時的に PSD 生データで IrTexture を作成し、BG スレッドで PNG 変換
        let (ir_data, ir_filename, ir_mime, spawn_psd_bg) = if is_psd {
            // 変換完了まで実データと一致するメタ情報を保持
            let psd_filename = format!("{}.psd", basename);
            (
                tex_data.clone(),
                psd_filename,
                "image/vnd.adobe.photoshop".to_string(),
                true,
            )
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
            (tex_data, filename, mime.to_string(), false)
        };

        // BG PSD 変換完了後に設定する PNG ファイル名
        let png_filename_for_bg = if spawn_psd_bg {
            Some(format!("{}.png", basename))
        } else {
            None
        };

        let tex_idx = loaded.ir.textures.len();
        loaded
            .ir
            .textures
            .push(crate::intermediate::types::IrTexture {
                filename: ir_filename,
                data: TextureData::Encoded(Arc::from(ir_data)),
                mime_type: ir_mime,
                source_path: path.display().to_string(),
                mip_chain: None,
            });

        // PSD の場合は BG スレッドで PNG 変換を開始
        if spawn_psd_bg {
            let psd_data = loaded.ir.textures[tex_idx].data.as_bytes().to_vec();
            let (tx, rx) = std::sync::mpsc::channel();
            std::thread::spawn(move || {
                let result = crate::psd::psd_to_png(&psd_data);
                let _ = tx.send(result);
            });
            let display = path.display().to_string();
            log::info!("PSD->PNG background conversion started: {}", display);
            self.tex.pending_psd_conversions.push(PendingPsdConversion {
                rx,
                tex_idx,
                png_filename: png_filename_for_bg.unwrap(),
                display_name: display,
            });
        }

        // 選択した材質の texture_index を更新
        let path_buf = path.clone();
        for &mat_idx in &selected {
            let mat = &mut loaded.ir.materials[mat_idx];
            mat.texture_index = Some(tex_idx);
            mat.apply_textured_defaults();
            log::info!(
                "Texture assignment: mat[{}] '{}' <- {}",
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

    /// pkg テクスチャの TextureView スロットを初期化（遅延ロード用）
    /// 実際の GPU アップロードは sync_tex_match_preview 内で選択時にオンデマンドで行う
    pub fn prepare_tex_match_views(&mut self) {
        let Some(ref mut pending) = self.tex.pending_match else {
            return;
        };
        if !pending.texture_views.is_empty() {
            return; // 既に初期化済み
        }
        let pkg_count = self.tex.pkg_textures.as_ref().map(|p| p.len()).unwrap_or(0);
        if pkg_count > 0 {
            pending.texture_views = vec![None; pkg_count];
        }
    }

    /// テクスチャ手動割当のリアルタイムプレビュー同期
    /// selections と previewed の差分を GPU bind group に反映
    /// テクスチャは選択時にオンデマンドで GPU アップロード（VRAM スパイク防止）
    pub fn sync_tex_match_preview(&mut self) {
        let Some(ref mut pending) = self.tex.pending_match else {
            return;
        };
        let Some(ref mut loaded) = self.loaded else {
            return;
        };
        let Some(ref renderer) = self.renderer else {
            return;
        };
        let device = &self.render_state.device;
        let queue = &self.render_state.queue;
        let texture_bgl = renderer.texture_bgl();

        for i in 0..pending.mat_indices.len() {
            let mat_idx = pending.mat_indices[i];
            let sel = pending.selections[i];
            let prev = pending.previewed[i];

            if sel == prev {
                continue;
            }

            if let Some(tex_idx) = sel {
                // オンデマンドアップロード: 未アップロードなら今アップロード
                if tex_idx < pending.texture_views.len()
                    && pending.texture_views[tex_idx].is_none()
                    && !pending.failed_uploads.contains(&tex_idx)
                {
                    if let Some(ref pkg) = self.tex.pkg_textures {
                        if let Some((name, data)) = pkg.get(tex_idx) {
                            let is_psd = super::super::texture::is_psd_filename(name);
                            match super::super::texture::upload_texture_from_bytes(
                                data, is_psd, device, queue,
                            ) {
                                Ok((view, _unorm)) => {
                                    pending.texture_views[tex_idx] = Some(view);
                                }
                                Err(e) => {
                                    log::warn!("pkg texture upload failed ({}): {e}", name);
                                    pending.failed_uploads.insert(tex_idx);
                                }
                            }
                        }
                    }
                }

                // テクスチャビュー取得（失敗時は既存プレビューを復元 — 同名兄弟含む）
                let Some(Some(ref view)) = pending.texture_views.get(tex_idx) else {
                    if prev.is_some() {
                        let fail_targets: Vec<usize> = if self.tex.link_same_name {
                            let mut t = vec![mat_idx];
                            t.extend(loaded.same_name_siblings(mat_idx));
                            t
                        } else {
                            vec![mat_idx]
                        };
                        for (draw_idx, draw) in loaded.gpu_model.draws.iter_mut().enumerate() {
                            if fail_targets.contains(&draw.material_index) {
                                if let Some((orig_tex, orig_mmd)) =
                                    pending.saved_binds.remove(&draw_idx)
                                {
                                    draw.texture_bind_group = orig_tex;
                                    draw.mmd_texture_bind_group = orig_mmd;
                                }
                            }
                        }
                        pending.previewed[i] = None;
                    }
                    continue;
                };

                // link_same_name 時は同名材質にも横展開（同一 MaterialGroup 内）
                let target_mats: Vec<usize> = if self.tex.link_same_name {
                    let mut targets = vec![mat_idx];
                    targets.extend(loaded.same_name_siblings(mat_idx));
                    targets
                } else {
                    vec![mat_idx]
                };

                for &target in &target_mats {
                    let sampler_info = loaded
                        .ir
                        .materials
                        .get(target)
                        .and_then(|m| m.base_color_tex_info.as_ref())
                        .map(|ti| ti.sampler)
                        .unwrap_or_default();
                    let sampler =
                        super::super::mesh::create_sampler_from_info(device, &sampler_info);

                    for (draw_idx, draw) in loaded.gpu_model.draws.iter_mut().enumerate() {
                        if draw.material_index == target {
                            if let std::collections::hash_map::Entry::Vacant(e) =
                                pending.saved_binds.entry(draw_idx)
                            {
                                e.insert((
                                    draw.texture_bind_group.take(),
                                    draw.mmd_texture_bind_group.take(),
                                ));
                            }
                            let new_bg = super::super::gpu::create_texture_bind_group(
                                device,
                                texture_bgl,
                                view,
                                &sampler,
                            );
                            draw.texture_bind_group = Some(new_bg);
                            // MMD パスでも texture_bind_group を参照させるため mmd 側を None に
                            draw.mmd_texture_bind_group = None;
                        }
                    }
                }
            } else {
                // 選択解除 → 元の bind group を復元（同一 MaterialGroup 内の同名材質含む）
                let target_mats: Vec<usize> = if self.tex.link_same_name {
                    let mut targets = vec![mat_idx];
                    targets.extend(loaded.same_name_siblings(mat_idx));
                    targets
                } else {
                    vec![mat_idx]
                };
                for &target in &target_mats {
                    for (draw_idx, draw) in loaded.gpu_model.draws.iter_mut().enumerate() {
                        if draw.material_index == target {
                            if let Some((orig_tex, orig_mmd)) =
                                pending.saved_binds.remove(&draw_idx)
                            {
                                draw.texture_bind_group = orig_tex;
                                draw.mmd_texture_bind_group = orig_mmd;
                            }
                        }
                    }
                }
            }
            pending.previewed[i] = sel;
        }
    }

    /// テクスチャ手動割当プレビューをキャンセル（元の bind group を復元）
    pub fn cancel_tex_match_preview(&mut self) {
        let Some(pending) = self.tex.pending_match.take() else {
            return;
        };
        let Some(ref mut loaded) = self.loaded else {
            return;
        };
        for (draw_idx, (orig_tex, orig_mmd)) in pending.saved_binds.into_iter() {
            if draw_idx < loaded.gpu_model.draws.len() {
                loaded.gpu_model.draws[draw_idx].texture_bind_group = orig_tex;
                loaded.gpu_model.draws[draw_idx].mmd_texture_bind_group = orig_mmd;
            }
        }
        // D&D プレビューが併存していた場合、bind group 復元で表示がずれるため
        // previewed をリセットして次フレームの sync_tex_preview で再適用させる
        if let Some(ref mut preview) = self.tex.pending_preview {
            preview.previewed.iter_mut().for_each(|v| *v = false);
        }
    }

    // -----------------------------------------------------------------------
    // テクスチャ割り当て履歴 (popone_history.json)
    // -----------------------------------------------------------------------

    /// 現在のモデルが履歴対象かどうか判定し、キーを返す
    pub fn texture_history_key(&self) -> Option<String> {
        use super::helpers::ReloadableSource;
        let loaded = self.loaded.as_ref()?;
        if !loaded.appended_models.is_empty() {
            return None;
        }
        // v0.5.0: 材質パラメータ編集の永続化のため、全フォーマット（VRM / PMX / FBX /
        // OBJ 等）で履歴キーを返すよう拡張。旧版では FBX / OBJ のみに制限していたが、
        // テクスチャ履歴だけでなく MaterialParamOverride も同じキーで保存するため、
        // File ソースなら全フォーマットでキーを返す。
        match &loaded.source {
            ReloadableSource::File(path) => Some(super::persistence::normalize_path(path)),
            _ => None,
        }
    }

    /// 現在のテクスチャ割り当て + 材質パラメータ編集差分を履歴に保存
    pub fn do_save_texture_history(&mut self) {
        let Some(key) = self.texture_history_key() else {
            return;
        };
        let Some(loaded) = self.loaded.as_ref() else {
            return;
        };

        // テクスチャ割当エントリ（BaseColor: v1 互換）
        let mut entries: Vec<super::persistence::TextureHistoryEntry> = self
            .tex
            .assignments
            .iter()
            .filter_map(|(mat_idx, src)| {
                if let TextureSource::File(path) = src {
                    let mat_name = loaded
                        .ir
                        .materials
                        .get(*mat_idx)
                        .map(|m| m.name.clone())
                        .unwrap_or_default();
                    Some(super::persistence::TextureHistoryEntry {
                        material_index: *mat_idx,
                        material_name: mat_name,
                        texture_path: path.to_string_lossy().into_owned(),
                        slot: crate::intermediate::types::TextureSlot::BaseColor,
                    })
                } else {
                    None
                }
            })
            .collect();

        // v0.5.1 追加 (M5): 補助スロットの割当エントリを追加
        for ((mat_idx, slot), path) in &self.slot_texture_paths {
            let mat_name = loaded
                .ir
                .materials
                .get(*mat_idx)
                .map(|m| m.name.clone())
                .unwrap_or_default();
            entries.push(super::persistence::TextureHistoryEntry {
                material_index: *mat_idx,
                material_name: mat_name,
                texture_path: path.to_string_lossy().into_owned(),
                slot: *slot,
            });
        }

        // v0.5.0 追加: 材質パラメータ編集差分（§I 最小永続化）
        // pristine_materials との diff を計算して保存する。
        // review_025 [P2]: mme_kind は IrMaterial にないため diff_from では取れない。
        // material_overrides から転写して保存に含める。
        let param_entries: Vec<super::persistence::MaterialParamOverrideEntry> = loaded
            .ir
            .materials
            .iter()
            .enumerate()
            .filter_map(|(mat_idx, mat)| {
                let pristine = self.pristine_materials.get(mat_idx)?;
                let mut diff =
                    super::material_edit::MaterialParamOverride::diff_from(pristine, mat);
                // mme_kind を material_overrides から転写
                let mme_kind = self
                    .material_overrides
                    .get(&mat_idx)
                    .and_then(|o| o.mme_kind);
                if mme_kind.is_some() {
                    diff.get_or_insert_with(Default::default).mme_kind = mme_kind;
                }
                let diff = diff?;
                Some(super::persistence::MaterialParamOverrideEntry {
                    material_index: mat_idx,
                    material_name: mat.name.clone(),
                    overrides: diff,
                })
            })
            .collect();

        if entries.is_empty() && param_entries.is_empty() {
            self.convert_message = Some(ConvertMessage::failure(String::from(
                "保存対象のテクスチャ割り当て・編集がありません",
            )));
            return;
        }

        let tex_count = entries.len();
        let param_count = param_entries.len();
        self.texture_history.history.insert(key.clone(), entries);
        // review_011 [P2] 対応: param_entries が空なら古い param_overrides を明示的に消す。
        // 空のまま残すと、ユーザーが編集を「初期値に戻す」で全消しした後でも古い override が
        // 「履歴呼出」で再適用されてしまう。
        if !param_entries.is_empty() {
            self.texture_history
                .param_overrides
                .insert(key, param_entries);
        } else {
            self.texture_history.param_overrides.remove(&key);
        }
        super::persistence::save_texture_history(&self.data_dir, &self.texture_history);
        self.convert_message = Some(ConvertMessage::success(format!(
            "履歴を保存しました (テクスチャ{tex_count}件, パラメータ{param_count}件)"
        )));
    }

    /// 履歴からテクスチャ割り当てを呼び出し
    pub fn do_recall_texture_history(&mut self) {
        let Some(key) = self.texture_history_key() else {
            return;
        };
        // review_011 [P1] 対応: テクスチャ履歴が空でも param_overrides があれば続行する。
        // パラメータ編集だけ保存したケースで「このモデルの履歴がありません」即 return を防ぐ。
        let has_tex_entries = self.texture_history.history.contains_key(&key);
        let has_param_entries = self.texture_history.param_overrides.contains_key(&key);
        if !has_tex_entries && !has_param_entries {
            self.convert_message = Some(ConvertMessage::failure(String::from(
                "このモデルの履歴がありません",
            )));
            return;
        }
        let entries = self
            .texture_history
            .history
            .get(&key)
            .cloned()
            .unwrap_or_default();

        // 照合結果を先に収集（loaded の不変借用を閉じるため）
        // v0.5.1 M5: (mat_idx, slot, path) の 3 タプルに拡張。同一材質の複数スロットを許容。
        let resolved: Vec<(usize, crate::intermediate::types::TextureSlot, PathBuf)>;
        let mut skipped = 0usize;
        {
            let Some(loaded) = self.loaded.as_ref() else {
                return;
            };
            // v0.5.1 M5: (mat_idx, slot) を重複検出キーに
            let mut seen = std::collections::HashSet::new();
            let mut tmp = Vec::new();
            for entry in &entries {
                let Some(mat_idx) =
                    super::persistence::resolve_material(&loaded.ir.materials, entry)
                else {
                    skipped += 1;
                    continue;
                };
                if !seen.insert((mat_idx, entry.slot)) {
                    continue;
                }
                let tex_path = PathBuf::from(&entry.texture_path);
                if !tex_path.is_file() {
                    log::warn!(
                        "Texture history: file not found, skipping: {} (slot={:?})",
                        entry.texture_path,
                        entry.slot
                    );
                    skipped += 1;
                    continue;
                }
                tmp.push((mat_idx, entry.slot, tex_path));
            }
            resolved = tmp;
        }

        // v0.5.1 レビュー [P1] 対応: 順序修正 — 先に pristine 復元してからテクスチャ/param を適用。
        //
        // 旧実装ではテクスチャ復元 → pristine 復元 → param 復元の順で、pristine 復元時に
        // 補助スロットのテクスチャ参照（IrMaterial.emissive_texture 等）がクリアされ、
        // 復元した補助スロットが全て消える不具合があった。
        //
        // review_012 [P2] 対応: 保存差分を適用する前に、全材質を pristine に戻して
        // material_overrides をクリアする。これにより「呼出前の unsaved 編集が残る」
        // 問題を解消し、「呼出 = 保存時点の状態を完全再現」を保証する。
        {
            let mat_count = if let Some(loaded) = self.loaded.as_mut() {
                for (i, mat) in loaded.ir.materials.iter_mut().enumerate() {
                    if let Some(pristine) = self.pristine_materials.get(i) {
                        *mat = pristine.clone();
                    }
                }
                loaded.ir.materials.len()
            } else {
                0
            };
            // dirty は loaded borrow 解放後に一括で立てる
            for i in 0..mat_count {
                self.mark_material_dirty(i);
            }
        }
        self.material_overrides.clear();
        // pristine 復元で補助スロットの参照も消えるため、slot_texture_paths もクリア。
        // 直後のテクスチャ復元ループで resolved 経由で再設定される。
        self.slot_texture_paths.clear();
        // v0.5.1 レビュー 02 [P1] 対応: BaseColor の tex.assignments / pkg_assignments もクリア。
        // 履歴に BaseColor エントリがない場合、旧実装では古いパスが残留し「次回保存で
        // 古い BaseColor が混入」「GPU 側に古い bind が表示される」不具合があった。
        // pristine 復元 = 保存時点の再現なので、assignments もベースラインに戻す。
        self.tex.assignments.clear();
        self.tex.pkg_assignments.clear();

        // link_same_name を一時的に無効化（reload_current と同じパターン）
        let saved_link = self.tex.link_same_name;
        self.tex.link_same_name = false;

        let mut applied = 0usize;
        for (mat_idx, slot, tex_path) in &resolved {
            self.convert_message = None;
            // v0.5.1 M5: slot に応じて BaseColor と補助スロットで経路を分ける
            if *slot == crate::intermediate::types::TextureSlot::BaseColor {
                self.assign_texture_to_material(*mat_idx, tex_path);
            } else {
                // 補助スロット: ファイル読み込み + assign_texture_core を直接呼ぶ
                let data = match std::fs::read(tex_path) {
                    Ok(d) => d,
                    Err(e) => {
                        log::warn!(
                            "Texture history (aux slot): failed to read {}: {}",
                            tex_path.display(),
                            e
                        );
                        skipped += 1;
                        continue;
                    }
                };
                let is_psd = tex_path
                    .extension()
                    .and_then(|e| e.to_str())
                    .map(|e| e.eq_ignore_ascii_case("psd"))
                    .unwrap_or(false);
                let display_name = tex_path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("")
                    .to_string();
                self.assign_texture_core(*mat_idx, *slot, &data, is_psd, &display_name);
                // 補助スロットのパスを slot_texture_paths にも記録
                self.slot_texture_paths
                    .insert((*mat_idx, *slot), tex_path.clone());
            }
            // assign_texture_to_material は失敗時に convert_message に Failure を設定する
            let failed = self
                .convert_message
                .as_ref()
                .is_some_and(|m| matches!(m.result, super::ConvertResult::Failure(_)));
            if failed {
                skipped += 1;
            } else {
                applied += 1;
            }
        }

        self.tex.link_same_name = saved_link;

        // テクスチャ復元と同じ「resolve → apply」2 フェーズ分離パターンで
        // immutable borrow (resolve) と mutable borrow (apply) の衝突を避ける。
        let mut param_applied = 0usize;
        let resolved_params: Vec<(usize, super::material_edit::MaterialParamOverride)> = {
            let param_entries = self
                .texture_history
                .param_overrides
                .get(&key)
                .cloned()
                .unwrap_or_default();
            let Some(loaded) = self.loaded.as_ref() else {
                return;
            };
            param_entries
                .into_iter()
                .filter_map(|entry| {
                    let dummy = super::persistence::TextureHistoryEntry {
                        material_index: entry.material_index,
                        material_name: entry.material_name,
                        texture_path: String::new(),
                        slot: crate::intermediate::types::TextureSlot::BaseColor,
                    };
                    let mat_idx =
                        super::persistence::resolve_material(&loaded.ir.materials, &dummy)?;
                    Some((mat_idx, entry.overrides))
                })
                .collect()
        };
        for (mat_idx, overrides) in resolved_params {
            self.material_overrides.insert(mat_idx, overrides.clone());
            if let Some(loaded) = self.loaded.as_mut() {
                if let Some(mat) = loaded.ir.materials.get_mut(mat_idx) {
                    overrides.apply_to(mat);
                }
            }
            self.mark_material_dirty(mat_idx);
            param_applied += 1;
        }

        let msg = if skipped > 0 || param_applied > 0 {
            let parts: Vec<String> = [
                if applied > 0 {
                    Some(format!("テクスチャ{applied}件"))
                } else {
                    None
                },
                if param_applied > 0 {
                    Some(format!("パラメータ{param_applied}件"))
                } else {
                    None
                },
                if skipped > 0 {
                    Some(format!("スキップ{skipped}件"))
                } else {
                    None
                },
            ]
            .into_iter()
            .flatten()
            .collect();
            format!("履歴呼出: {}", parts.join(", "))
        } else if applied > 0 {
            format!("テクスチャ履歴: {applied}件適用")
        } else {
            "履歴がありません".to_string()
        };
        self.convert_message = Some(if applied > 0 || param_applied > 0 {
            ConvertMessage::success(msg)
        } else {
            ConvertMessage::failure(msg)
        });
    }

    /// PSD→PNG バックグラウンド変換の結果をポーリングし、IrTexture を差し替え
    pub(super) fn poll_pending_psd_conversions(&mut self) {
        if self.tex.pending_psd_conversions.is_empty() {
            return;
        }

        let loaded = match self.loaded.as_mut() {
            Some(l) => l,
            None => {
                // モデルがアンロードされた場合は全て破棄
                self.tex.pending_psd_conversions.clear();
                return;
            }
        };

        // 完了した変換を逆順に処理（インデックスをずらさないため）
        let mut i = 0;
        while i < self.tex.pending_psd_conversions.len() {
            match self.tex.pending_psd_conversions[i].rx.try_recv() {
                Ok(Ok(png_data)) => {
                    let conv = self.tex.pending_psd_conversions.remove(i);
                    // IrTexture のデータ・ファイル名・MIME を PSD から PNG に差し替え
                    if conv.tex_idx < loaded.ir.textures.len() {
                        let tex = &mut loaded.ir.textures[conv.tex_idx];
                        tex.data = TextureData::Encoded(Arc::from(png_data));
                        tex.filename = conv.png_filename;
                        tex.mime_type = "image/png".to_string();
                        log::info!(
                            "PSD->PNG background conversion completed: {} (tex_idx={})",
                            conv.display_name,
                            conv.tex_idx,
                        );
                    } else {
                        log::warn!(
                            "PSD->PNG conversion result discarded (tex_idx {} out of range): {}",
                            conv.tex_idx,
                            conv.display_name,
                        );
                    }
                    // i は進めない（remove でずれたため）
                }
                Ok(Err(e)) => {
                    let conv = self.tex.pending_psd_conversions.remove(i);
                    log::warn!(
                        "PSD->PNG background conversion failed: {} - {}",
                        conv.display_name,
                        e,
                    );
                    // 変換失敗時は PSD 生データのまま IrTexture に残る
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {
                    // まだ変換中
                    i += 1;
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    let conv = self.tex.pending_psd_conversions.remove(i);
                    log::warn!(
                        "PSD->PNG background conversion thread disconnected: {}",
                        conv.display_name,
                    );
                }
            }
        }
    }
}
