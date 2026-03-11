use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Instant;

use eframe::egui;
use eframe::egui_wgpu;
use eframe::wgpu;

use crate::intermediate::types::IrModel;
use crate::vrm;

use super::camera::OrbitCamera;
use super::gpu::{self, DrawMode, GpuRenderer, LightMode, RenderParams};
use super::mesh::GpuModel;
use super::ui;

/// D&D 対応画像拡張子
const IMAGE_EXTENSIONS: &[&str] = &["png", "jpg", "jpeg", "tga", "bmp", "psd"];

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
}

/// ステータスバー用キャッシュ
pub struct CachedStats {
    pub total_vertices: usize,
    pub total_faces: usize,
}

/// VRM読み込み結果
pub struct LoadedModel {
    pub ir: IrModel,
    pub gpu_model: GpuModel,
    pub file_path: PathBuf,
    /// 材質情報キャッシュ（テクスチャ割り当て時に更新）
    pub mat_cache: CachedMaterialInfo,
    /// 統計キャッシュ
    pub stats_cache: CachedStats,
}

/// 変換結果の種類
pub enum ConvertResult {
    Success(String),
    Failure(String),
}

/// 表示・描画関連の設定
pub struct DisplaySettings {
    /// ライト明るさ (0.0〜2.0)
    pub light_intensity: f32,
    /// 環境光 (0.0〜1.0)
    pub ambient_intensity: f32,
    /// 背景明るさ (0.0〜1.0)
    pub bg_brightness: f32,
    /// グリッド表示
    pub show_grid: bool,
    /// ボーン表示
    pub show_bones: bool,
    /// ボーン濃度
    pub bone_opacity: f32,
    /// SpringBone（物理）表示
    pub show_spring_bones: bool,
    /// SpringBone 濃度
    pub spring_bone_opacity: f32,
    /// 描画モード
    pub draw_mode: DrawMode,
    /// ライトモード
    pub light_mode: LightMode,
    /// 剛体回転をボーン方向に揃える（PMX出力 + 物理表示）
    pub align_rigid_rotation: bool,
    /// MSAA アンチエイリアス
    pub msaa: bool,
    /// 法線平滑化（頂点統合 + 法線平均化）
    pub smooth_normals: bool,
    /// カスタム法線クリア（ジオメトリから法線を再計算）
    pub clear_custom_normals: bool,
    /// 法線表示
    pub show_normals: bool,
    /// 法線表示の長さ
    pub normal_length: f32,
}

impl Default for DisplaySettings {
    fn default() -> Self {
        Self {
            light_intensity: 0.7,
            ambient_intensity: 0.45,
            bg_brightness: 0.19,
            show_grid: true,
            show_bones: false,
            bone_opacity: 0.85,
            show_spring_bones: false,
            spring_bone_opacity: 0.75,
            draw_mode: DrawMode::Solid,
            light_mode: LightMode::CameraFollow,
            align_rigid_rotation: false,
            msaa: true,
            smooth_normals: false,
            clear_custom_normals: false,
            show_normals: false,
            normal_length: 0.5,
        }
    }
}

/// テクスチャD&Dプレビュー状態
pub struct PendingTexPreview {
    pub path: PathBuf,
    /// 材質ごとの選択状態（チェックボックス）
    pub selection: Vec<bool>,
    /// 現在プレビュー適用中の材質
    previewed: Vec<bool>,
    /// プレビュー用テクスチャビュー（GPU）
    texture_view: wgpu::TextureView,
    /// draw_index → 退避した元の bind group
    saved_binds: HashMap<usize, Option<wgpu::BindGroup>>,
    /// サムネイル表示用 egui TextureId
    pub preview_tex_id: Option<egui::TextureId>,
}

/// ビューアのメイン状態
pub struct ViewerApp {
    pub loaded: Option<LoadedModel>,
    pub camera: OrbitCamera,
    pub renderer: Option<GpuRenderer>,
    pub convert_message: Option<ConvertResult>,
    /// 表情モーフのスライダ値
    pub morph_weights: Vec<f32>,
    /// モーフウェイト変更フラグ
    pub morph_dirty: bool,
    /// 表示・描画設定
    pub display: DisplaySettings,
    /// PMX変換時にログファイルを出力するか
    pub output_log: bool,
    /// PMX出力パス（テキストボックス編集用）
    pub pmx_output_path: String,
    /// 材質ごとの表示ON/OFF
    pub material_visibility: Vec<bool>,
    /// 材質フィルター文字列
    pub material_filter: String,
    /// ドラッグオーバー中フラグ
    pub drag_hovering: bool,
    /// ビューポートテクスチャID
    pub viewport_texture_id: Option<egui::TextureId>,
    /// wgpu render state（CreationContext から取得）
    render_state: egui_wgpu::RenderState,
    /// PMX上書き確認ダイアログ表示中
    pub confirm_overwrite: bool,
    /// Tポーズ→Aスタンス変換（トグル時に再読み込み）
    pub normalize_pose: bool,
    /// ビューポートの高さ（フィット計算用）
    pub last_viewport_height: f32,
    /// 手動テクスチャ割り当て履歴（材質Index → ファイルパス）
    pub tex_assignments: HashMap<usize, PathBuf>,
    /// テクスチャD&Dプレビュー
    pub pending_tex_preview: Option<PendingTexPreview>,
    /// ファイル読み込み遅延実行 (path, overlay表示済みフラグ)
    pub pending_load: Option<(PathBuf, bool)>,
    /// PMX変換遅延実行 (overlay表示済みフラグ)
    pub pending_convert: Option<bool>,
    /// FPS計測用
    last_frame_time: Instant,
    fps_smoothed: f32,
    /// ログディレクトリパス
    pub logs_dir: PathBuf,
    /// 現在のログファイルパス
    pub log_path: PathBuf,
}

impl ViewerApp {
    pub fn new(cc: &eframe::CreationContext, logs_dir: PathBuf, log_path: PathBuf) -> Self {
        let render_state = cc
            .wgpu_render_state
            .clone()
            .expect("wgpu render state required");

        // 日本語フォント読み込み
        Self::setup_japanese_font(&cc.egui_ctx);

        Self {
            loaded: None,
            camera: OrbitCamera::default(),
            renderer: None,
            convert_message: None,
            morph_weights: Vec::new(),
            morph_dirty: false,
            display: DisplaySettings::default(),
            material_visibility: Vec::new(),
            material_filter: String::new(),
            output_log: false,
            pmx_output_path: String::new(),
            drag_hovering: false,
            viewport_texture_id: None,
            render_state,
            confirm_overwrite: false,
            normalize_pose: false,
            last_viewport_height: 720.0,
            tex_assignments: HashMap::new(),
            pending_tex_preview: None,
            pending_load: None,
            pending_convert: None,
            last_frame_time: Instant::now(),
            fps_smoothed: 0.0,
            logs_dir,
            log_path,
        }
    }

    fn setup_japanese_font(ctx: &egui::Context) {
        // Noto Sans JP（OFL ライセンス）をバイナリに組み込み
        const NOTO_SANS_JP: &[u8] =
            include_bytes!("../../assets/NotoSansJP-Regular.ttf");

        let mut fonts = egui::FontDefinitions::default();
        fonts.font_data.insert(
            "noto_jp".to_owned(),
            egui::FontData::from_static(NOTO_SANS_JP).into(),
        );
        fonts
            .families
            .get_mut(&egui::FontFamily::Proportional)
            .unwrap()
            .insert(0, "noto_jp".to_owned());
        fonts
            .families
            .get_mut(&egui::FontFamily::Monospace)
            .unwrap()
            .push("noto_jp".to_owned());
        ctx.set_fonts(fonts);
    }

    fn load_file(&mut self, path: PathBuf) {
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();

        let result = match ext.as_str() {
            "fbx" => self.try_load_fbx(&path),
            _ => self.try_load_vrm(&path),
        };

        match result {
            Ok(()) => {
                log::info!("読み込み成功: {}", path.display());
                self.convert_message = None;
            }
            Err(e) => {
                log::error!("読み込み失敗: {e}");
                self.convert_message = Some(ConvertResult::Failure(format!(
                    "読み込み失敗: {e}\n対応形式: VRM 0.0 / 1.0 (.vrm), FBX (.fbx)\n別のファイルを試してください。"
                )));
            }
        }
    }

    fn try_load_fbx(&mut self, path: &std::path::Path) -> anyhow::Result<()> {
        let data = std::fs::read(path)?;
        let ir = crate::fbx::extract::extract_ir_model_from_fbx_with_options(
            &data, Some(path), self.normalize_pose,
        )?;
        self.finish_load(ir, path)
    }

    fn try_load_vrm(&mut self, path: &std::path::Path) -> anyhow::Result<()> {
        let glb = vrm::loader::load_glb(path)?;
        let version = vrm::detect::detect_version(&glb.document);
        let all_extensions = vrm::loader::get_raw_extensions(&glb.document);

        let mut ir = vrm::extract::extract_ir_model_with_options(
            &glb.document,
            &glb.buffers,
            &glb.images,
            &glb.vrm_extension,
            &version,
            &all_extensions,
            self.normalize_pose,
        )?;

        let device = &self.render_state.device;
        let queue = &self.render_state.queue;
        let gpu_model = super::mesh::build_gpu_model(&ir, &glb.images, device, queue, self.display.smooth_normals, self.display.clear_custom_normals)?;

        // IrTexture を PNG エンコード済みに変換（convert_ir_to_pmx で統一的に使えるように）
        Self::encode_ir_textures_as_png(&mut ir, &glb.images);

        self.finish_load_with_gpu(ir, gpu_model, path)
    }

    fn finish_load(&mut self, ir: IrModel, path: &std::path::Path) -> anyhow::Result<()> {
        let device = &self.render_state.device;
        let queue = &self.render_state.queue;

        // GPU リソース構築（IrTexture から直接アップロード）
        let gpu_model = super::mesh::build_gpu_model_from_ir(&ir, device, queue, self.display.smooth_normals, self.display.clear_custom_normals)?;
        self.finish_load_with_gpu(ir, gpu_model, path)
    }

    fn finish_load_with_gpu(&mut self, ir: IrModel, gpu_model: super::mesh::GpuModel, path: &std::path::Path) -> anyhow::Result<()> {
        // レンダラー初期化（まだなければ）
        if self.renderer.is_none() {
            let device = &self.render_state.device;
            let queue = &self.render_state.queue;
            self.renderer = Some(GpuRenderer::new(device, queue, gpu_model.has_alpha));
        }

        // テクスチャ割り当て履歴クリア（別モデル読み込み時）
        self.tex_assignments.clear();
        self.pending_tex_preview = None;

        // モーフスライダ初期化
        self.morph_weights = vec![0.0; ir.morphs.len()];
        self.morph_dirty = false;
        // 材質表示フラグ初期化（DrawCall数 = 材質数ではない場合があるのでdraws数に合わせる）
        self.material_visibility = vec![true; gpu_model.draws.len()];
        self.material_filter.clear();
        // カメラをモデルのバウンディングボックスにフィット
        let (bbox_min, bbox_max) = gpu_model.bbox();
        self.camera.reset_to_bbox_with_margin(bbox_min, bbox_max, self.last_viewport_height);

        // デフォルト出力パス: 入力VRMと同じ場所に .pmx
        self.pmx_output_path = path.with_extension("pmx").to_string_lossy().to_string();

        // キャッシュ構築
        let mat_cache = Self::build_mat_cache(&ir, &gpu_model);
        let stats_cache = CachedStats {
            total_vertices: ir.total_vertices(),
            total_faces: ir.total_faces(),
        };

        self.loaded = Some(LoadedModel {
            ir,
            gpu_model,
            file_path: path.to_path_buf(),
            mat_cache,
            stats_cache,
        });

        if let Some(ref mut renderer) = self.renderer {
            renderer.invalidate_normal_cache();
        }

        Ok(())
    }

    /// smooth_normals 切り替え時に GPU モデルを再構築
    pub fn rebuild_gpu_model(&mut self) {
        let Some(loaded) = &self.loaded else { return };
        let device = &self.render_state.device;
        let queue = &self.render_state.queue;
        let smooth = self.display.smooth_normals;
        let clear_normals = self.display.clear_custom_normals;

        match super::mesh::build_gpu_model_from_ir(&loaded.ir, device, queue, smooth, clear_normals) {
            Ok(new_model) => {
                let mat_cache = Self::build_mat_cache(&loaded.ir, &new_model);
                self.material_visibility = vec![true; new_model.draws.len()];
                if let Some(loaded) = &mut self.loaded {
                    loaded.gpu_model = new_model;
                    loaded.mat_cache = mat_cache;
                }
                if let Some(ref mut renderer) = self.renderer {
                    renderer.invalidate_normal_cache();
                }
                log::info!("GPU モデル再構築完了 (smooth_normals={})", smooth);
            }
            Err(e) => log::error!("GPU モデル再構築失敗: {}", e),
        }
    }

    /// 材質情報キャッシュを構築
    fn build_mat_cache(ir: &IrModel, gpu_model: &GpuModel) -> CachedMaterialInfo {
        let draw_indices: Vec<(usize, usize)> = gpu_model.draws.iter()
            .enumerate()
            .map(|(i, d)| (i, d.material_index))
            .collect();
        let names: Vec<String> = ir.materials.iter().map(|m| m.name.clone()).collect();
        let tex_indices: Vec<Option<usize>> = ir.materials.iter().map(|m| m.texture_index).collect();
        let source_tex_names: Vec<Option<String>> = ir.materials.iter()
            .map(|m| m.source_texture_name.clone()).collect();
        let tex_set_count = ir.materials.iter().filter(|m| m.texture_index.is_some()).count();
        CachedMaterialInfo { draw_indices, names, tex_indices, source_tex_names, tex_set_count }
    }

    /// 材質キャッシュを更新（テクスチャ割り当て後）
    fn update_mat_cache(&mut self) {
        if let Some(ref loaded) = self.loaded {
            let cache = Self::build_mat_cache(&loaded.ir, &loaded.gpu_model);
            // loaded を再借用して書き込み
            if let Some(ref mut loaded) = self.loaded {
                loaded.mat_cache = cache;
            }
        }
    }

    /// 指定材質に外部テクスチャファイルを割り当て
    pub fn assign_texture_to_material(&mut self, material_index: usize, path: &std::path::Path) {
        // ファイルを1回だけ読み込み
        let tex_data = match std::fs::read(path) {
            Ok(d) => d,
            Err(e) => {
                log::error!("ファイル読み込み失敗: {e}");
                self.convert_message = Some(ConvertResult::Failure(format!(
                    "テクスチャ読み込み失敗: {e}"
                )));
                return;
            }
        };

        let ext_lower = path.extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();
        let is_psd = ext_lower == "psd";

        let device = &self.render_state.device;
        let queue = &self.render_state.queue;

        // GPU テクスチャをアップロード（読み込み済みバイト列を使用）
        let texture_view = match super::texture::upload_texture_from_bytes(&tex_data, is_psd, device, queue) {
            Ok(view) => view,
            Err(e) => {
                log::error!("テクスチャ読み込み失敗: {e}");
                self.convert_message = Some(ConvertResult::Failure(format!(
                    "テクスチャ読み込み失敗: {e}"
                )));
                return;
            }
        };

        // IrModel にテクスチャを追加・材質を更新
        let Some(ref mut loaded) = self.loaded else { return };

        let basename = path.file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();

        // PSD の場合は PNG に変換して保存
        let (ir_data, ir_filename, ir_mime) = if is_psd {
            match Self::psd_to_png(&tex_data) {
                Ok(png_data) => (png_data, format!("{}.png", basename), "image/png".to_string()),
                Err(e) => {
                    log::error!("PSD→PNG変換失敗: {e}");
                    self.convert_message = Some(ConvertResult::Failure(format!(
                        "PSD→PNG変換失敗: {e}"
                    )));
                    return;
                }
            }
        } else {
            let filename = path.file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();
            let mime = match ext_lower.as_str() {
                "png" => "image/png",
                "tga" => "image/x-tga",
                "bmp" => "image/bmp",
                _ => "image/jpeg",
            };
            (tex_data, filename, mime.to_string())
        };

        let tex_idx = loaded.ir.textures.len();
        loaded.ir.textures.push(crate::intermediate::types::IrTexture {
            filename: ir_filename,
            data: ir_data,
            mime_type: ir_mime,
        });
        let mat = &mut loaded.ir.materials[material_index];
        mat.texture_index = Some(tex_idx);
        mat.apply_textured_defaults();

        // GPU DrawCall 更新
        let (texture_bgl, sampler) = match self.renderer {
            Some(ref r) => (r.texture_bgl(), r.sampler()),
            None => return,
        };
        loaded.gpu_model.assign_texture_to_material(material_index, &texture_view, device, texture_bgl, sampler);

        log::info!(
            "テクスチャ割り当て: 材質[{}] '{}' ← {}",
            material_index,
            loaded.ir.materials[material_index].name,
            path.display()
        );

        // 割り当て履歴を記録（reload_current 時の復元用）
        self.tex_assignments.insert(material_index, path.to_path_buf());

        // 材質キャッシュ更新
        self.update_mat_cache();
    }

    /// VRM の IrTexture（raw ピクセル）を PNG エンコード済みに変換
    fn encode_ir_textures_as_png(ir: &mut IrModel, images: &[gltf::image::Data]) {
        use image::ImageEncoder;
        for (i, tex) in ir.textures.iter_mut().enumerate() {
            if let Some(img_data) = images.get(i) {
                let (w, h) = (img_data.width, img_data.height);
                // RGBA 画像を構築
                let rgba_img: Option<image::RgbaImage> = if tex.data.len() == (w * h * 4) as usize {
                    image::ImageBuffer::from_raw(w, h, tex.data.clone())
                } else if tex.data.len() == (w * h * 3) as usize {
                    let mut rgba = Vec::with_capacity((w * h * 4) as usize);
                    for chunk in tex.data.chunks(3) {
                        rgba.extend_from_slice(chunk);
                        rgba.push(255);
                    }
                    image::ImageBuffer::from_raw(w, h, rgba)
                } else {
                    None
                };
                if let Some(img) = rgba_img {
                    let mut png_data = Vec::new();
                    if image::codecs::png::PngEncoder::new(&mut png_data)
                        .write_image(img.as_raw(), w, h, image::ExtendedColorType::Rgba8)
                        .is_ok()
                    {
                        tex.data = png_data;
                        if !tex.filename.ends_with(".png") {
                            tex.filename = tex.filename.replace(".jpg", ".png")
                                .replace(".jpeg", ".png");
                            if !tex.filename.ends_with(".png") {
                                tex.filename.push_str(".png");
                            }
                        }
                        tex.mime_type = "image/png".to_string();
                    }
                }
            }
        }
    }

    /// PSD データを PNG に変換（decode_psd を共有）
    fn psd_to_png(psd_data: &[u8]) -> anyhow::Result<Vec<u8>> {
        let (rgba, width, height) = super::texture::decode_psd(psd_data)?;

        let mut png_data = Vec::new();
        {
            let encoder = image::codecs::png::PngEncoder::new(&mut png_data);
            use image::ImageEncoder;
            encoder.write_image(&rgba, width, height, image::ExtendedColorType::Rgba8)
                .map_err(|e| anyhow::anyhow!("PNG エンコード失敗: {}", e))?;
        }
        Ok(png_data)
    }

    /// 現在読み込み中のVRMを再読み込みする（オプション変更時）
    /// カメラ・モーフ・材質表示などの状態は保持する
    pub fn reload_current(&mut self) {
        let Some(ref loaded) = self.loaded else { return };
        let path = loaded.file_path.clone();
        let saved_camera = self.camera.clone();
        let saved_morphs = self.morph_weights.clone();
        let saved_visibility = self.material_visibility.clone();
        let saved_filter = self.material_filter.clone();
        let saved_pmx_path = self.pmx_output_path.clone();
        let saved_tex_assignments = self.tex_assignments.clone();

        self.load_file(path);

        // 状態を復元（モーフ数・材質数が変わらなければそのまま使う）
        self.camera = saved_camera;
        if saved_morphs.len() == self.morph_weights.len() {
            self.morph_weights = saved_morphs;
            self.morph_dirty = true; // 強制更新
        }
        if saved_visibility.len() == self.material_visibility.len() {
            self.material_visibility = saved_visibility;
        }
        self.material_filter = saved_filter;
        self.pmx_output_path = saved_pmx_path;

        // テクスチャ割り当てを復元
        self.tex_assignments = HashMap::new(); // assign 時に再記録されるのでクリア
        for (mat_idx, tex_path) in &saved_tex_assignments {
            self.assign_texture_to_material(*mat_idx, tex_path);
        }
    }

    /// 1枚のテクスチャをプレビューダイアログで開く
    fn open_texture_preview(&mut self, path: PathBuf) {
        let ext = path.extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();
        let is_psd = ext == "psd";
        match std::fs::read(&path).and_then(|data|
            super::texture::upload_texture_from_bytes(
                &data, is_psd,
                &self.render_state.device, &self.render_state.queue,
            ).map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))
        ) {
            Ok(texture_view) => {
                let num_mats = self.loaded.as_ref()
                    .map_or(0, |l| l.ir.materials.len());
                let preview_tex_id = {
                    let mut renderer = self.render_state.renderer.write();
                    Some(renderer.register_native_texture(
                        &self.render_state.device,
                        &texture_view,
                        wgpu::FilterMode::Linear,
                    ))
                };
                self.pending_tex_preview = Some(PendingTexPreview {
                    path,
                    selection: vec![false; num_mats],
                    previewed: vec![false; num_mats],
                    texture_view,
                    saved_binds: HashMap::new(),
                    preview_tex_id,
                });
            }
            Err(e) => {
                self.convert_message = Some(ConvertResult::Failure(
                    format!("テクスチャ読み込み失敗: {e}")
                ));
            }
        }
    }

    /// 複数テクスチャの自動割り当て（ファイル名と材質名のマッチング）
    fn auto_assign_textures(&mut self, image_files: Vec<PathBuf>) {
        let Some(ref loaded) = self.loaded else { return };
        let mat_names: Vec<String> = loaded.ir.materials.iter()
            .map(|m| m.name.to_lowercase())
            .collect();

        let mut assigned = 0usize;
        let mut unmatched: Vec<String> = Vec::new();

        // ファイル名 → マッチする材質インデックスを収集
        let mut assignments: Vec<(PathBuf, Vec<usize>)> = Vec::new();
        for path in &image_files {
            let stem = path.file_stem()
                .unwrap_or_default()
                .to_string_lossy()
                .to_lowercase();
            if stem.is_empty() {
                continue;
            }
            // 材質名にファイル名（拡張子なし）を含む材質を検索
            let matched: Vec<usize> = mat_names.iter()
                .enumerate()
                .filter(|(_, name)| name.contains(&stem) || stem.contains(name.as_str()))
                .map(|(i, _)| i)
                .collect();
            if matched.is_empty() {
                unmatched.push(path.file_name().unwrap_or_default().to_string_lossy().to_string());
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
            self.convert_message = Some(ConvertResult::Success(msg));
        } else {
            self.convert_message = Some(ConvertResult::Failure(
                format!("マッチする材質が見つかりませんでした\nファイル: {}", unmatched.join(", "))
            ));
        }
    }

    fn open_file_dialog(&mut self) {
        if let Some(path) = rfd::FileDialog::new()
            .add_filter("3D Models", &["vrm", "fbx"])
            .add_filter("VRM", &["vrm"])
            .add_filter("FBX", &["fbx"])
            .pick_file()
        {
            self.pending_load = Some((path, false));
        }
    }

    /// テクスチャプレビューの同期（selection と previewed の差分を GPU に反映）
    pub fn sync_tex_preview(&mut self) {
        let Some(ref mut preview) = self.pending_tex_preview else { return };
        let Some(ref mut loaded) = self.loaded else { return };
        let Some(ref renderer) = self.renderer else { return };
        let device = &self.render_state.device;
        let texture_bgl = renderer.texture_bgl();
        let sampler = renderer.sampler();

        for mat_idx in 0..preview.selection.len() {
            if preview.selection[mat_idx] && !preview.previewed[mat_idx] {
                // プレビュー適用: 元の bind group を退避し、プレビュー用に差し替え
                for (draw_idx, draw) in loaded.gpu_model.draws.iter_mut().enumerate() {
                    if draw.material_index == mat_idx {
                        if !preview.saved_binds.contains_key(&draw_idx) {
                            preview.saved_binds.insert(draw_idx, draw.texture_bind_group.take());
                        }
                        draw.texture_bind_group = Some(
                            gpu::create_texture_bind_group(device, texture_bgl, &preview.texture_view, sampler),
                        );
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
        let Some(preview) = self.pending_tex_preview.take() else { return };
        let path = &preview.path;

        // 選択された材質のインデックスを収集
        let selected: Vec<usize> = preview.selection.iter()
            .enumerate()
            .filter_map(|(i, &v)| if v { Some(i) } else { None })
            .collect();

        if selected.is_empty() {
            // 何も選択されていなければ元に戻す
            self.cancel_tex_preview_inner(preview);
            return;
        }

        // IrModel にテクスチャを追加（1回だけ）
        let Some(ref mut loaded) = self.loaded else { return };

        let ext_lower = path.extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();
        let is_psd = ext_lower == "psd";

        let tex_data = match std::fs::read(path) {
            Ok(d) => d,
            Err(e) => {
                log::error!("ファイル読み込み失敗: {e}");
                return;
            }
        };

        let basename = path.file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();

        let (ir_data, ir_filename, ir_mime) = if is_psd {
            match Self::psd_to_png(&tex_data) {
                Ok(png_data) => (png_data, format!("{}.png", basename), "image/png".to_string()),
                Err(e) => {
                    log::error!("PSD→PNG変換失敗: {e}");
                    return;
                }
            }
        } else {
            let filename = path.file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();
            let mime = match ext_lower.as_str() {
                "png" => "image/png",
                "tga" => "image/x-tga",
                "bmp" => "image/bmp",
                _ => "image/jpeg",
            };
            (tex_data, filename, mime.to_string())
        };

        let tex_idx = loaded.ir.textures.len();
        loaded.ir.textures.push(crate::intermediate::types::IrTexture {
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
                mat_idx, mat.name, path_buf.display()
            );
        }

        // 割り当て履歴を記録（reload_current 時の復元用）
        for &mat_idx in &selected {
            self.tex_assignments.insert(mat_idx, path_buf.clone());
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
        let Some(preview) = self.pending_tex_preview.take() else { return };
        self.cancel_tex_preview_inner(preview);
    }

    fn cancel_tex_preview_inner(&mut self, preview: PendingTexPreview) {
        // サムネイル用 egui テクスチャを解放
        if let Some(tex_id) = preview.preview_tex_id {
            let mut renderer = self.render_state.renderer.write();
            renderer.free_texture(&tex_id);
        }
        let Some(ref mut loaded) = self.loaded else { return };
        // 退避した全 bind group を復元
        for (draw_idx, orig) in preview.saved_binds.into_iter() {
            if draw_idx < loaded.gpu_model.draws.len() {
                loaded.gpu_model.draws[draw_idx].texture_bind_group = orig;
            }
        }
    }
}

impl eframe::App for ViewerApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // FPS計測（指数移動平均）
        let now = Instant::now();
        let dt = now.duration_since(self.last_frame_time).as_secs_f32();
        self.last_frame_time = now;
        if dt > 0.0 {
            let fps = 1.0 / dt;
            self.fps_smoothed = if self.fps_smoothed == 0.0 {
                fps
            } else {
                self.fps_smoothed * 0.9 + fps * 0.1
            };
        }

        // 遅延処理: オーバーレイ表示済みなら実行
        if let Some((_, true)) = self.pending_load {
            let (path, _) = self.pending_load.take().unwrap();
            self.load_file(path);
        }
        if self.pending_convert == Some(true) {
            self.pending_convert = None;
            ui::execute_conversion(self);
        }

        // ドラッグ＆ドロップ処理
        let (dropped_files, hover_ext) = ctx.input(|i| {
            let hover_ext = i.raw.hovered_files.first()
                .and_then(|f| f.path.as_ref())
                .and_then(|p| p.extension())
                .and_then(|e| e.to_str())
                .map(|e| e.to_lowercase())
                .unwrap_or_default();
            self.drag_hovering = !i.raw.hovered_files.is_empty();
            let paths: Vec<PathBuf> = i.raw.dropped_files.iter()
                .filter_map(|f| f.path.clone())
                .collect();
            (paths, hover_ext)
        });
        let is_hover_image = IMAGE_EXTENSIONS.contains(&hover_ext.as_str());

        if !dropped_files.is_empty() {
            // 画像とモデルに分類
            let mut image_files: Vec<PathBuf> = Vec::new();
            let mut model_file: Option<PathBuf> = None;
            for path in dropped_files {
                let ext = path.extension()
                    .and_then(|e| e.to_str())
                    .unwrap_or("")
                    .to_lowercase();
                if IMAGE_EXTENSIONS.contains(&ext.as_str()) {
                    image_files.push(path);
                } else {
                    model_file = Some(path);
                }
            }

            let has_loaded_model = self.loaded.is_some();

            if let Some(model_path) = model_file {
                // モデルファイル → 読み込み
                self.pending_load = Some((model_path, false));
            }

            if !image_files.is_empty() && has_loaded_model {
                if image_files.len() == 1 {
                    // 1枚 → テクスチャプレビューダイアログ（従来動作）
                    let path = image_files.into_iter().next().unwrap();
                    self.open_texture_preview(path);
                } else {
                    // 複数枚 → 材質名マッチングで自動割り当て
                    self.auto_assign_textures(image_files);
                }
            }
        }

        // キーボードショートカット
        let wants_kb = ctx.wants_keyboard_input();
        ctx.input(|i| {
            // Ctrl+O: ファイルを開く
            if i.modifiers.ctrl && i.key_pressed(egui::Key::O) {
                self.open_file_dialog();
            }
            // テキスト入力中はシングルキーショートカットを無効化
            if !wants_kb {
                // R: カメラリセット
                if !i.modifiers.ctrl && i.key_pressed(egui::Key::R) {
                    if let Some(ref loaded) = self.loaded {
                        let (bbox_min, bbox_max) = loaded.gpu_model.bbox();
                        self.camera.reset_to_bbox_with_margin(bbox_min, bbox_max, self.last_viewport_height);
                    }
                }
                // F: モデルにフィット
                if !i.modifiers.ctrl && i.key_pressed(egui::Key::F) {
                    if let Some(ref loaded) = self.loaded {
                        let (bbox_min, bbox_max) = loaded.gpu_model.bbox();
                        self.camera.fit_to_bbox_with_margin(bbox_min, bbox_max, self.last_viewport_height);
                    }
                }
                // G: グリッド表示切り替え
                if !i.modifiers.ctrl && i.key_pressed(egui::Key::G) {
                    self.display.show_grid = !self.display.show_grid;
                }
                // B: ボーン表示切り替え
                if !i.modifiers.ctrl && i.key_pressed(egui::Key::B) {
                    self.display.show_bones = !self.display.show_bones;
                }
                // P: SpringBone物理表示切り替え
                if !i.modifiers.ctrl && i.key_pressed(egui::Key::P) {
                    self.display.show_spring_bones = !self.display.show_spring_bones;
                }
                // W: ワイヤーフレーム切り替え
                if !i.modifiers.ctrl && i.key_pressed(egui::Key::W) {
                    self.display.draw_mode = match self.display.draw_mode {
                        DrawMode::Solid => DrawMode::Wireframe,
                        DrawMode::Wireframe => DrawMode::Solid,
                    };
                }
                // N: 法線表示切り替え
                if !i.modifiers.ctrl && i.key_pressed(egui::Key::N) {
                    self.display.show_normals = !self.display.show_normals;
                }
                // L: ライトモード切り替え
                if !i.modifiers.ctrl && i.key_pressed(egui::Key::L) {
                    self.display.light_mode = match self.display.light_mode {
                        LightMode::CameraFollow => LightMode::Fixed,
                        LightMode::Fixed => LightMode::CameraFollow,
                    };
                }
            }
        });

        // トップバー
        egui::TopBottomPanel::top("top_bar").show(ctx, |bar| {
            egui::menu::bar(bar, |bar| {
                if bar.button("開く").clicked() {
                    self.open_file_dialog();
                }

                if bar.button("ログ").clicked() {
                    open_directory(&self.logs_dir);
                }

                if let Some(ref loaded) = self.loaded {
                    bar.separator();
                    bar.label(
                        egui::RichText::new(&loaded.ir.name)
                            .color(egui::Color32::from_gray(0x20)),
                    );
                }
            });
        });

        // 右側パネル
        ui::show_side_panel(ctx, self);

        // テクスチャD&Dダイアログ + プレビュー同期
        ui::show_texture_drop_dialog(ctx, self);
        self.sync_tex_preview();

        // ステータスバー
        egui::TopBottomPanel::bottom("status_bar")
            .exact_height(22.0)
            .show(ctx, |ui| {
                ui.horizontal_centered(|ui| {
                    if let Some(ref loaded) = self.loaded {
                        let ir = &loaded.ir;
                        let font = egui::FontId::proportional(11.0);
                        let color = egui::Color32::from_gray(0x30);

                        // ファイルパス
                        ui.label(egui::RichText::new(
                            loaded.file_path.to_string_lossy().as_ref()
                        ).font(font.clone()).color(color));

                        ui.separator();

                        // モデル統計（キャッシュ利用）
                        let stats = format!(
                            "頂点:{} 面:{} 材質:{} テクスチャ:{} ボーン:{} モーフ:{}",
                            loaded.stats_cache.total_vertices,
                            loaded.stats_cache.total_faces,
                            ir.materials.len(),
                            ir.textures.len(),
                            ir.bones.len(),
                            ir.morphs.len(),
                        );
                        ui.label(egui::RichText::new(stats).font(font.clone()).color(color));

                        // FBXの場合、テクスチャ設定状況（キャッシュ利用）
                        if ir.source_format == crate::intermediate::types::SourceFormat::Fbx {
                            let tex_set = loaded.mat_cache.tex_set_count;
                            let tex_total = ir.materials.len();
                            ui.separator();
                            let tex_status = format!("Tex:{}/{}", tex_set, tex_total);
                            let tex_color = if tex_set == tex_total {
                                egui::Color32::from_rgb(0x40, 0xC0, 0x40)
                            } else {
                                egui::Color32::from_rgb(0xD0, 0xA0, 0x40)
                            };
                            ui.label(egui::RichText::new(tex_status).font(font).color(tex_color));
                        }
                    } else {
                        ui.label(
                            egui::RichText::new("VRM/FBX ファイルを読み込んでください")
                                .font(egui::FontId::proportional(11.0))
                                .color(egui::Color32::from_gray(0x60))
                        );
                    }

                    // FPS表示（右端に配置）
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let fps_text = format!("{:.0} fps", self.fps_smoothed);
                        ui.label(
                            egui::RichText::new(fps_text)
                                .font(egui::FontId::proportional(11.0))
                                .color(egui::Color32::from_gray(0x60))
                        );
                    });
                });
            });

        // 中央ビューポート
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill({
                let b = (self.display.bg_brightness * 255.0).clamp(0.0, 255.0) as u8;
                egui::Color32::from_rgb(b, b, b)
            }))
            .show(ctx, |viewport| {
                let available = viewport.available_size();
                if available.x < 1.0 || available.y < 1.0 {
                    return;
                }

                // カメラ操作
                let response = viewport.allocate_rect(
                    egui::Rect::from_min_size(viewport.cursor().min, available),
                    egui::Sense::click_and_drag(),
                );
                self.camera.handle_input(ctx, &response);

                // モーフウェイト変更検知 → 頂点バッファ更新
                if self.morph_dirty {
                    if let Some(ref mut loaded) = self.loaded {
                        let queue = &self.render_state.queue;
                        loaded.gpu_model.apply_morphs(
                            &loaded.ir,
                            &self.morph_weights,
                            queue,
                        );
                        self.morph_dirty = false;
                    }
                }

                // 3D描画（renderer を take して &mut で使い、戻す）
                if let Some(ref loaded) = self.loaded {
                    let width = (available.x * ctx.pixels_per_point()) as u32;
                    let height = (available.y * ctx.pixels_per_point()) as u32;
                    if width > 0 && height > 0 {
                        if let Some(mut renderer) = self.renderer.take() {
                            let device = &self.render_state.device;
                            let queue = &self.render_state.queue;

                            let render_params = RenderParams {
                                camera: &self.camera,
                                width,
                                height,
                                material_visibility: &self.material_visibility,
                                display: &self.display,
                            };

                            let (texture_id, _) = renderer.render_to_texture(
                                device,
                                queue,
                                &mut self.render_state.renderer.write(),
                                &loaded.gpu_model,
                                &loaded.ir,
                                &render_params,
                                &mut self.viewport_texture_id,
                            );

                            self.renderer = Some(renderer);

                            // egui に表示
                            let uv = egui::Rect::from_min_max(
                                egui::pos2(0.0, 0.0),
                                egui::pos2(1.0, 1.0),
                            );
                            viewport.painter().image(
                                texture_id,
                                response.rect,
                                uv,
                                egui::Color32::WHITE,
                            );
                        }
                    }
                }

                // ドロップオーバーレイ
                if self.drag_hovering {
                    let rect = response.rect;
                    let is_fbx_loaded = self.loaded.as_ref()
                        .map_or(false, |l| l.ir.source_format == crate::intermediate::types::SourceFormat::Fbx);
                    let (overlay_color, overlay_text) = if is_hover_image && is_fbx_loaded {
                        (
                            egui::Color32::from_rgba_unmultiplied(0x40, 0xC0, 0x40, 0x60),
                            "テクスチャを割り当て",
                        )
                    } else {
                        (
                            egui::Color32::from_rgba_unmultiplied(0x40, 0x80, 0xFF, 0x60),
                            "VRM ファイルをドロップ",
                        )
                    };
                    viewport.painter().rect_filled(
                        rect,
                        0.0,
                        overlay_color,
                    );
                    viewport.painter().text(
                        rect.center(),
                        egui::Align2::CENTER_CENTER,
                        overlay_text,
                        egui::FontId::proportional(28.0),
                        egui::Color32::WHITE,
                    );
                } else if self.loaded.is_none() {
                    let rect = response.rect;
                    viewport.painter().text(
                        rect.center(),
                        egui::Align2::CENTER_CENTER,
                        "VRM ファイルをドロップ、または「開く」をクリック",
                        egui::FontId::proportional(20.0),
                        egui::Color32::from_gray(0xA0),
                    );
                }

                // ビューポートの高さを記録（フィット計算用）
                self.last_viewport_height = response.rect.height();

                // カメラ情報（左上、テキスト直接描画）
                if self.loaded.is_some() {
                    let rect = response.rect;
                    let cam_info = format!(
                        "({:.1},{:.1},{:.1}) D:{:.1} Y:{:.0}° P:{:.0}°",
                        self.camera.target.x,
                        self.camera.target.y,
                        self.camera.target.z,
                        self.camera.distance,
                        self.camera.yaw.to_degrees(),
                        self.camera.pitch.to_degrees(),
                    );
                    viewport.painter().text(
                        egui::pos2(rect.left() + 10.0, rect.top() + 10.0),
                        egui::Align2::LEFT_TOP,
                        &cam_info,
                        egui::FontId::monospace(11.0),
                        egui::Color32::from_gray(0xC0),
                    );

                    // フィット・リセットボタン（右上）
                    let margin = 8.0;
                    let btn_pos = egui::pos2(rect.right() - margin, rect.top() + margin);
                    let btn_area = egui::Area::new(egui::Id::new("camera_btn_overlay"))
                        .fixed_pos(btn_pos)
                        .constrain(false)
                        .interactable(true)
                        .anchor(egui::Align2::RIGHT_TOP, egui::Vec2::ZERO);
                    btn_area.show(ctx, |ui| {
                        ui.horizontal(|ui| {
                            if ui.small_button("フィット(F)").on_hover_text("モデルにフィット").clicked() {
                                if let Some(ref loaded) = self.loaded {
                                    let (bbox_min, bbox_max) = loaded.gpu_model.bbox();
                                    self.camera.fit_to_bbox_with_margin(bbox_min, bbox_max, rect.height());
                                }
                            }
                            if ui.small_button("リセット(R)").on_hover_text("カメラをリセット").clicked() {
                                if let Some(ref loaded) = self.loaded {
                                    let (bbox_min, bbox_max) = loaded.gpu_model.bbox();
                                    self.camera.reset_to_bbox_with_margin(bbox_min, bbox_max, rect.height());
                                }
                            }
                        });
                    });
                }

                // 操作ヒント（左下、常時表示）
                {
                    let rect = response.rect;
                    let hint = if self.loaded.is_some() {
                        "左ドラッグ:回転  右/中ドラッグ:パン  ホイール:ズーム  Ctrl+O:開く  R:リセット  F:フィット  G:グリッド  B:ボーン  P:物理  W:ワイヤー  L:ライト"
                    } else {
                        "Ctrl+O:開く  ドラッグ&ドロップ:VRM/FBXファイル読込"
                    };
                    viewport.painter().text(
                        egui::pos2(rect.left() + 8.0, rect.bottom() - 8.0),
                        egui::Align2::LEFT_BOTTOM,
                        hint,
                        egui::FontId::proportional(12.0),
                        egui::Color32::from_gray(0xC0),
                    );
                }

                // プログレスオーバーレイ（読み込み中 / 変換中）
                let processing_msg = if self.pending_load.is_some() {
                    Some("読み込み中...")
                } else if self.pending_convert.is_some() {
                    Some("PMX変換中...")
                } else {
                    None
                };
                if let Some(msg) = processing_msg {
                    let rect = response.rect;
                    // 半透明背景
                    viewport.painter().rect_filled(
                        rect,
                        0.0,
                        egui::Color32::from_rgba_unmultiplied(0, 0, 0, 0xA0),
                    );
                    // テキスト
                    let center = rect.center();
                    viewport.painter().text(
                        egui::pos2(center.x, center.y - 20.0),
                        egui::Align2::CENTER_CENTER,
                        msg,
                        egui::FontId::proportional(24.0),
                        egui::Color32::WHITE,
                    );
                    // プログレスバー（不定型アニメーション）
                    let bar_width = 300.0_f32.min(rect.width() - 40.0);
                    let bar_rect = egui::Rect::from_center_size(
                        egui::pos2(center.x, center.y + 16.0),
                        egui::vec2(bar_width, 6.0),
                    );
                    let t = ctx.input(|i| i.time) as f32;
                    let phase = (t * 1.5).sin() * 0.5 + 0.5; // 0..1 oscillation
                    let indicator_w = bar_width * 0.3;
                    let indicator_x = bar_rect.left() + phase * (bar_width - indicator_w);
                    // バー背景
                    viewport.painter().rect_filled(
                        bar_rect,
                        3.0,
                        egui::Color32::from_gray(0x40),
                    );
                    // インジケータ
                    viewport.painter().rect_filled(
                        egui::Rect::from_min_size(
                            egui::pos2(indicator_x, bar_rect.top()),
                            egui::vec2(indicator_w, 6.0),
                        ),
                        3.0,
                        egui::Color32::from_rgb(0x50, 0xA0, 0xFF),
                    );

                    // フラグ更新: 次フレームで処理実行
                    if let Some((_, ref mut shown)) = self.pending_load {
                        if !*shown {
                            *shown = true;
                            ctx.request_repaint();
                        }
                    }
                    if self.pending_convert == Some(false) {
                        self.pending_convert = Some(true);
                        ctx.request_repaint();
                    }
                }
            });
    }
}

/// ディレクトリをOSのファイルマネージャで開く
fn open_directory(path: &std::path::Path) {
    #[cfg(target_os = "windows")]
    {
        let _ = std::process::Command::new("explorer").arg(path).spawn();
    }
    #[cfg(target_os = "macos")]
    {
        let _ = std::process::Command::new("open").arg(path).spawn();
    }
    #[cfg(target_os = "linux")]
    {
        let _ = std::process::Command::new("xdg-open").arg(path).spawn();
    }
}
