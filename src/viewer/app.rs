use std::path::PathBuf;

use eframe::egui;
use eframe::egui_wgpu;

use crate::intermediate::types::IrModel;
use crate::vrm;

use super::camera::OrbitCamera;
use super::gpu::GpuRenderer;
use super::mesh::GpuModel;
use super::ui;

/// VRM読み込み結果
pub struct LoadedModel {
    pub ir: IrModel,
    pub gpu_model: GpuModel,
    pub file_path: PathBuf,
}

/// ビューアのメイン状態
pub struct ViewerApp {
    pub loaded: Option<LoadedModel>,
    pub camera: OrbitCamera,
    pub renderer: Option<GpuRenderer>,
    pub convert_message: Option<String>,
    /// 表情モーフのスライダ値
    pub morph_weights: Vec<f32>,
    /// 前フレームのモーフウェイト（変更検知用）
    prev_morph_weights: Vec<f32>,
    /// ライト明るさ (0.0〜2.0)
    pub light_intensity: f32,
    /// 環境光 (0.0〜1.0)
    pub ambient_intensity: f32,
    /// 背景明るさ (0.0〜1.0)
    pub bg_brightness: f32,
    /// PMX変換時にログファイルを出力するか
    pub output_log: bool,
    /// PMX出力パス（テキストボックス編集用）
    pub pmx_output_path: String,
    /// ドラッグオーバー中フラグ
    pub drag_hovering: bool,
    /// ビューポートテクスチャID
    pub viewport_texture_id: Option<egui::TextureId>,
    /// wgpu render state（CreationContext から取得）
    render_state: egui_wgpu::RenderState,
}

impl ViewerApp {
    pub fn new(cc: &eframe::CreationContext) -> Self {
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
            prev_morph_weights: Vec::new(),
            light_intensity: 0.7,
            ambient_intensity: 0.45,
            bg_brightness: 0.19,
            output_log: false,
            pmx_output_path: String::new(),
            drag_hovering: false,
            viewport_texture_id: None,
            render_state,
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

    fn load_vrm(&mut self, path: PathBuf) {
        match self.try_load_vrm(&path) {
            Ok(()) => {
                log::info!("VRM読み込み成功: {}", path.display());
                self.convert_message = None;
            }
            Err(e) => {
                log::error!("VRM読み込み失敗: {e}");
                self.convert_message = Some(format!("読み込み失敗: {e}"));
            }
        }
    }

    fn try_load_vrm(&mut self, path: &PathBuf) -> anyhow::Result<()> {
        let glb = vrm::loader::load_glb(path)?;
        let version = vrm::detect::detect_version(&glb.document);
        let all_extensions = vrm::loader::get_raw_extensions(&glb.document);

        let ir = vrm::extract::extract_ir_model(
            &glb.document,
            &glb.buffers,
            &glb.images,
            &glb.vrm_extension,
            &version,
            &all_extensions,
        )?;

        let device = &self.render_state.device;
        let queue = &self.render_state.queue;

        // GPU リソース構築
        let gpu_model = super::mesh::build_gpu_model(&ir, &glb.images, device, queue)?;

        // レンダラー初期化（まだなければ）
        if self.renderer.is_none() {
            self.renderer = Some(GpuRenderer::new(device, queue, gpu_model.has_alpha));
        }

        // モーフスライダ初期化
        self.morph_weights = vec![0.0; ir.morphs.len()];
        self.prev_morph_weights = vec![0.0; ir.morphs.len()];
        self.camera = OrbitCamera::default();

        // デフォルト出力パス: 入力VRMと同じ場所に .pmx
        self.pmx_output_path = path.with_extension("pmx").to_string_lossy().to_string();

        self.loaded = Some(LoadedModel {
            ir,
            gpu_model,
            file_path: path.clone(),
        });

        Ok(())
    }

    fn open_file_dialog(&mut self) {
        if let Some(path) = rfd::FileDialog::new()
            .add_filter("VRM", &["vrm"])
            .pick_file()
        {
            self.load_vrm(path);
        }
    }
}

impl eframe::App for ViewerApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // ドラッグ＆ドロップ処理
        let dropped = ctx.input(|i| {
            self.drag_hovering = !i.raw.hovered_files.is_empty();
            i.raw.dropped_files.first().and_then(|f| f.path.clone())
        });

        if let Some(path) = dropped {
            self.load_vrm(path);
        }

        // トップバー
        egui::TopBottomPanel::top("top_bar").show(ctx, |bar| {
            egui::menu::bar(bar, |bar| {
                if bar.button("開く").clicked() {
                    self.open_file_dialog();
                }

                if let Some(ref loaded) = self.loaded {
                    bar.separator();
                    bar.label(&loaded.ir.name);
                }
            });
        });

        // 右側パネル
        ui::show_side_panel(ctx, self);

        // 中央ビューポート
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill({
                let b = (self.bg_brightness * 255.0).clamp(0.0, 255.0) as u8;
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
                if self.morph_weights != self.prev_morph_weights {
                    if let Some(ref loaded) = self.loaded {
                        let queue = &self.render_state.queue;
                        loaded.gpu_model.apply_morphs(
                            &loaded.ir,
                            &self.morph_weights,
                            queue,
                        );
                        self.prev_morph_weights = self.morph_weights.clone();
                    }
                }

                // 3D描画
                if let (Some(ref renderer), Some(ref loaded)) =
                    (&self.renderer, &self.loaded)
                {
                    let width = (available.x * ctx.pixels_per_point()) as u32;
                    let height = (available.y * ctx.pixels_per_point()) as u32;
                    if width == 0 || height == 0 {
                        return;
                    }

                    let device = &self.render_state.device;
                    let queue = &self.render_state.queue;

                    // オフスクリーン描画
                    let (texture_id, _) = renderer.render_to_texture(
                        device,
                        queue,
                        &mut self.render_state.renderer.write(),
                        &loaded.gpu_model,
                        &self.camera,
                        width,
                        height,
                        self.light_intensity,
                        self.ambient_intensity,
                        self.bg_brightness,
                        &mut self.viewport_texture_id,
                    );

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

                // ドロップオーバーレイ
                if self.drag_hovering {
                    let rect = response.rect;
                    viewport.painter().rect_filled(
                        rect,
                        0.0,
                        egui::Color32::from_rgba_unmultiplied(0x40, 0x80, 0xFF, 0x60),
                    );
                    viewport.painter().text(
                        rect.center(),
                        egui::Align2::CENTER_CENTER,
                        "VRM ファイルをドロップ",
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

                // 操作ヒント（左下）
                if self.loaded.is_some() {
                    let rect = response.rect;
                    viewport.painter().text(
                        egui::pos2(rect.left() + 8.0, rect.bottom() - 8.0),
                        egui::Align2::LEFT_BOTTOM,
                        "左ドラッグ: 回転  右ドラッグ: パン  ホイール: ズーム",
                        egui::FontId::proportional(12.0),
                        egui::Color32::from_gray(0x80),
                    );
                }
            });
    }
}
