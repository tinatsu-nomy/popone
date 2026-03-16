use eframe::egui;
use glam::{Mat4, Vec3};

/// カメラ操作感度定数
const YAW_PITCH_SENSITIVITY: f32 = 0.005;
const PAN_SPEED_FACTOR: f32 = 0.003;
const ZOOM_SENSITIVITY_BASE: f32 = 0.003;
const ZOOM_RADIUS_REF: f32 = 20.0;
const ZOOM_SENSITIVITY_MIN: f32 = 0.5;
const ZOOM_SENSITIVITY_MAX: f32 = 3.0;
const DISTANCE_MIN: f32 = 0.1;
const DISTANCE_MAX: f32 = 5000.0;
const NEAR_FACTOR: f32 = 0.005;
const NEAR_MIN: f32 = 0.01;
const NEAR_MAX: f32 = 1.0;
const FAR_FACTOR: f32 = 50.0;
const FAR_MIN: f32 = 100.0;
const FAR_MAX: f32 = 50000.0;
const FOV_DEGREES: f32 = 45.0;
const FIT_MARGIN: f32 = 1.15;

#[derive(Clone)]
pub struct OrbitCamera {
    pub target: Vec3,
    pub distance: f32,
    pub yaw: f32,   // ラジアン
    pub pitch: f32,  // ラジアン
    /// モデルのバウンディング球の半径（ズーム感度に使用）
    pub model_radius: f32,
    /// 透視投影（true）/ 正射影（false）
    pub perspective: bool,
}

impl Default for OrbitCamera {
    fn default() -> Self {
        Self {
            target: Vec3::new(0.0, 15.0, 0.0),
            distance: 40.0,
            yaw: 0.0,
            pitch: 0.0,
            model_radius: 20.0,
            perspective: true,
        }
    }
}

impl OrbitCamera {
    /// マウス操作を処理
    pub fn handle_input(&mut self, ctx: &egui::Context, response: &egui::Response) {
        // 左ドラッグ: 回転
        if response.dragged_by(egui::PointerButton::Primary) {
            let delta = response.drag_delta();
            self.yaw -= delta.x * YAW_PITCH_SENSITIVITY;
            self.pitch -= delta.y * YAW_PITCH_SENSITIVITY;
            self.pitch = self.pitch.clamp(
                -std::f32::consts::FRAC_PI_2 + 0.01,
                std::f32::consts::FRAC_PI_2 - 0.01,
            );
        }

        // 右ドラッグ / 中ボタンドラッグ: パン（ビュー空間の上・右方向を使用）
        let is_pan = response.dragged_by(egui::PointerButton::Secondary)
            || response.dragged_by(egui::PointerButton::Middle);
        if is_pan {
            let delta = response.drag_delta();
            let speed = self.distance * PAN_SPEED_FACTOR;
            let (right, up) = self.view_axes();
            self.target += -right * delta.x * speed + up * delta.y * speed;
        }

        // ホイール: ズーム（モデルサイズに応じた感度）
        if response.hovered() {
            let scroll = ctx.input(|i| i.smooth_scroll_delta.y);
            if scroll != 0.0 {
                // モデル半径に応じた感度調整
                let sensitivity = ZOOM_SENSITIVITY_BASE * (self.model_radius / ZOOM_RADIUS_REF).clamp(ZOOM_SENSITIVITY_MIN, ZOOM_SENSITIVITY_MAX);
                self.distance *= (-scroll * sensitivity).exp();
                self.distance = self.distance.clamp(DISTANCE_MIN, DISTANCE_MAX);
            }
        }
    }

    /// カメラ位置
    /// PMX 座標系ではモデルは +Z を向く。yaw=0 でカメラを -Z 側に置く。
    pub fn eye(&self) -> Vec3 {
        let x = self.distance * self.pitch.cos() * self.yaw.sin();
        let y = self.distance * self.pitch.sin();
        let z = -self.distance * self.pitch.cos() * self.yaw.cos();
        self.target + Vec3::new(x, y, z)
    }

    /// ビュー空間の右方向と上方向を返す
    fn view_axes(&self) -> (Vec3, Vec3) {
        let forward = (self.target - self.eye()).normalize();
        let world_up = self.up_vector();
        let right = forward.cross(world_up).normalize_or_zero();
        let up = right.cross(forward).normalize_or_zero();
        // pitch ≈ ±90° で right がゼロになる場合のフォールバック
        if right.length_squared() < 1e-6 {
            return (Vec3::X, Vec3::Z);
        }
        (right, up)
    }

    /// pitch に応じた up ベクトル（360° チルト対応）
    fn up_vector(&self) -> Vec3 {
        // pitch が ±90° を超えると逆さまになるので、cos(pitch) の符号で up を反転
        if self.pitch.cos() >= 0.0 {
            Vec3::Y
        } else {
            Vec3::NEG_Y
        }
    }

    /// View-Projection 行列（左手系、wgpu NDC: Z∈[0,1]）
    /// near/far はカメラ距離に応じて動的調整
    pub fn view_proj(&self, aspect: f32) -> Mat4 {
        let view = Mat4::look_at_lh(self.eye(), self.target, self.up_vector());
        let near = (self.distance * NEAR_FACTOR).clamp(NEAR_MIN, NEAR_MAX);
        let far = (self.distance * FAR_FACTOR).clamp(FAR_MIN, FAR_MAX);
        let proj = if self.perspective {
            Mat4::perspective_lh(
                FOV_DEGREES.to_radians(),
                aspect,
                near,
                far,
            )
        } else {
            // 正射影: 透視投影と同じ距離でのビュー高さに合わせる
            let fov_half = (FOV_DEGREES / 2.0).to_radians();
            let half_h = self.distance * fov_half.tan();
            let half_w = half_h * aspect;
            Mat4::orthographic_lh(-half_w, half_w, -half_h, half_h, near, far)
        };
        proj * view
    }

    /// バウンディングボックスにフィットさせる
    /// 高さ基準でFOVから必要な距離を算出（人型モデル向け）
    pub fn fit_to_bbox(&mut self, bbox_min: Vec3, bbox_max: Vec3) {
        let center = (bbox_min + bbox_max) * 0.5;
        let height = bbox_max.y - bbox_min.y;
        let fov_half = (FOV_DEGREES / 2.0).to_radians();
        let required = (height * 0.5) / fov_half.tan();
        self.target = center;
        self.distance = (required * FIT_MARGIN).max(2.0);
        self.model_radius = (bbox_max - bbox_min).length() * 0.5;
        self.yaw = 0.0;
        self.pitch = 0.0;
    }

    /// フィット: 現在のyaw/pitchを保持し、距離とターゲットだけ調整
    pub fn fit_to_bbox_with_margin(&mut self, bbox_min: Vec3, bbox_max: Vec3, viewport_height: f32) {
        let (target, distance, model_radius) = Self::compute_fit(bbox_min, bbox_max, viewport_height);
        self.target = target;
        self.distance = distance;
        self.model_radius = model_radius;
    }

    /// リセット: yaw/pitchを正面に戻してフィット
    pub fn reset_to_bbox_with_margin(&mut self, bbox_min: Vec3, bbox_max: Vec3, viewport_height: f32) {
        let (target, distance, model_radius) = Self::compute_fit(bbox_min, bbox_max, viewport_height);
        self.target = target;
        self.distance = distance;
        self.model_radius = model_radius;
        self.yaw = 0.0;
        self.pitch = 0.0;
    }

    /// フィット計算の共通処理
    fn compute_fit(bbox_min: Vec3, bbox_max: Vec3, viewport_height: f32) -> (Vec3, f32, f32) {
        let center = (bbox_min + bbox_max) * 0.5;
        let height = bbox_max.y - bbox_min.y;
        let fov_half = (FOV_DEGREES / 2.0).to_radians();

        // 上部オーバーレイ + 下部ヒントで約60px分の余白が必要
        let margin_px = 60.0;
        let effective_height = (viewport_height - margin_px).max(100.0);
        let effective_fov_half = (effective_height / viewport_height.max(1.0)) * fov_half;
        let required = (height * 0.5) / effective_fov_half.tan();

        let distance = (required * FIT_MARGIN).max(2.0);
        let model_radius = (bbox_max - bbox_min).length() * 0.5;

        // ターゲットを少し下げてオーバーレイ下にモデル中心を配置
        let world_per_px = 2.0 * distance * fov_half.tan() / viewport_height.max(1.0);
        let mut target = center;
        target.y -= world_per_px * margin_px * 0.25;

        (target, distance, model_radius)
    }

    /// ライト方向 — カメラ追従モード
    pub fn camera_following_light_dir(&self) -> Vec3 {
        let forward = (self.target - self.eye()).normalize();
        let world_up = self.up_vector();
        let right = forward.cross(world_up).normalize_or_zero();
        let up = right.cross(forward);
        (forward + right * 0.3 + up * 0.5).normalize()
    }

    /// ライト方向 — 固定モード（上方45°前方から）
    pub fn fixed_light_dir() -> Vec3 {
        Vec3::new(0.3, 0.7, -0.5).normalize()
    }
}
