use eframe::egui;
use glam::{Mat4, Vec3};

/// カメラ操作感度定数
const YAW_PITCH_SENSITIVITY: f32 = 0.005;
const PAN_SPEED_FACTOR: f32 = 0.003;
const ZOOM_SENSITIVITY_BASE: f32 = 0.0025;
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
const FOV_DEGREES: f32 = 30.0;
const FIT_MARGIN: f32 = 1.15;

#[derive(Clone)]
pub struct OrbitCamera {
    pub target: Vec3,
    pub distance: f32,
    pub yaw: f32,   // ラジアン
    pub pitch: f32, // ラジアン
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
        // Shift精密操作（1/3速度）
        let fine = if ctx.input(|i| i.modifiers.shift) {
            1.0 / 3.0
        } else {
            1.0
        };

        // 左ドラッグ: 回転
        if response.dragged_by(egui::PointerButton::Primary) {
            let delta = response.drag_delta();
            self.yaw -= delta.x * YAW_PITCH_SENSITIVITY * fine;
            self.pitch -= delta.y * YAW_PITCH_SENSITIVITY * fine;
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
            let fov_scale = FOV_DEGREES / 45.0;
            let speed = self.distance * PAN_SPEED_FACTOR * fov_scale * fine;
            let (right, up) = self.view_axes();
            self.target += -right * delta.x * speed + up * delta.y * speed;
        }

        // ホイール: ズーム（モデルサイズに応じた感度）
        if response.hovered() {
            let scroll = ctx.input(|i| i.smooth_scroll_delta.y);
            if scroll != 0.0 {
                // モデル半径に応じた感度調整
                let sensitivity = ZOOM_SENSITIVITY_BASE
                    * (self.model_radius / ZOOM_RADIUS_REF)
                        .clamp(ZOOM_SENSITIVITY_MIN, ZOOM_SENSITIVITY_MAX);
                self.distance *= (-scroll * sensitivity * fine).exp();
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

    /// ビュー行列（左手系）
    pub fn view_matrix(&self) -> Mat4 {
        Mat4::look_at_lh(self.eye(), self.target, self.up_vector())
    }

    /// 射影行列の [1][1] 成分（= 1/tan(fov_y/2)）
    /// MToon ScreenCoordinates アウトラインの距離クランプ用
    pub fn proj_11(&self) -> f32 {
        1.0 / (FOV_DEGREES.to_radians() * 0.5).tan()
    }

    /// View-Projection 行列（左手系、wgpu NDC: Z∈[0,1]）
    /// near/far はカメラ距離に応じて動的調整
    pub fn view_proj(&self, aspect: f32) -> Mat4 {
        let view = self.view_matrix();
        let near = (self.distance * NEAR_FACTOR).clamp(NEAR_MIN, NEAR_MAX);
        let far = (self.distance * FAR_FACTOR).clamp(FAR_MIN, FAR_MAX);
        let proj = if self.perspective {
            Mat4::perspective_lh(FOV_DEGREES.to_radians(), aspect, near, far)
        } else {
            // 正射影: 透視投影と同じ距離でのビュー高さに合わせる
            let fov_half = (FOV_DEGREES / 2.0).to_radians();
            let half_h = self.distance * fov_half.tan();
            let half_w = half_h * aspect;
            Mat4::orthographic_lh(-half_w, half_w, -half_h, half_h, near, far)
        };
        proj * view
    }

    /// バウンディングボックスにフィットさせる（yaw/pitchをリセット）
    pub fn fit_to_bbox(
        &mut self,
        bbox_min: Vec3,
        bbox_max: Vec3,
        viewport_w: f32,
        viewport_h: f32,
    ) {
        self.yaw = 0.0;
        self.pitch = 0.0;
        let (target, distance, model_radius) =
            self.compute_fit(bbox_min, bbox_max, viewport_w, viewport_h);
        self.target = target;
        self.distance = distance;
        self.model_radius = model_radius;
    }

    /// フィット: 現在のyaw/pitchを保持し、距離とターゲットだけ調整
    pub fn fit_to_bbox_with_margin(
        &mut self,
        bbox_min: Vec3,
        bbox_max: Vec3,
        viewport_w: f32,
        viewport_h: f32,
    ) {
        let (target, distance, model_radius) =
            self.compute_fit(bbox_min, bbox_max, viewport_w, viewport_h);
        self.target = target;
        self.distance = distance;
        self.model_radius = model_radius;
    }

    /// リセット: yaw/pitchを正面に戻してフィット
    pub fn reset_to_bbox_with_margin(
        &mut self,
        bbox_min: Vec3,
        bbox_max: Vec3,
        viewport_w: f32,
        viewport_h: f32,
    ) {
        self.yaw = 0.0;
        self.pitch = 0.0;
        let (target, distance, model_radius) =
            self.compute_fit(bbox_min, bbox_max, viewport_w, viewport_h);
        self.target = target;
        self.distance = distance;
        self.model_radius = model_radius;
    }

    /// bbox 8頂点を現在のview軸に投影し、投影半幅・半高・半奥行きを返す
    /// half_depth は透視投影で手前面のスケーリングを考慮するために必要
    fn projected_half_extents(&self, bbox_min: Vec3, bbox_max: Vec3) -> (f32, f32, f32) {
        let (right, up) = self.view_axes();
        let forward = (self.target - self.eye()).normalize();
        let center = (bbox_min + bbox_max) * 0.5;
        let corners = [
            Vec3::new(bbox_min.x, bbox_min.y, bbox_min.z),
            Vec3::new(bbox_min.x, bbox_min.y, bbox_max.z),
            Vec3::new(bbox_min.x, bbox_max.y, bbox_min.z),
            Vec3::new(bbox_min.x, bbox_max.y, bbox_max.z),
            Vec3::new(bbox_max.x, bbox_min.y, bbox_min.z),
            Vec3::new(bbox_max.x, bbox_min.y, bbox_max.z),
            Vec3::new(bbox_max.x, bbox_max.y, bbox_min.z),
            Vec3::new(bbox_max.x, bbox_max.y, bbox_max.z),
        ];
        let (mut half_w, mut half_h, mut half_d) = (0.0f32, 0.0f32, 0.0f32);
        for p in corners {
            let v = p - center;
            half_w = half_w.max(v.dot(right).abs());
            half_h = half_h.max(v.dot(up).abs());
            half_d = half_d.max(v.dot(forward).abs());
        }
        (half_w, half_h, half_d)
    }

    /// フィット計算の共通処理（視点依存: 現在のyaw/pitchでの投影幅・高を使用）
    fn compute_fit(
        &self,
        bbox_min: Vec3,
        bbox_max: Vec3,
        viewport_w: f32,
        viewport_h: f32,
    ) -> (Vec3, f32, f32) {
        let center = (bbox_min + bbox_max) * 0.5;
        let model_radius = (bbox_max - bbox_min).length() * 0.5;

        let (half_w, half_h, half_d) = self.projected_half_extents(bbox_min, bbox_max);
        let aspect = viewport_w.max(1.0) / viewport_h.max(1.0);
        let fov_y_half = (FOV_DEGREES / 2.0).to_radians();

        // 上部オーバーレイ + 下部ヒントで約60px分の余白が必要
        let margin_px = 60.0;
        let effective_h = (viewport_h - margin_px).max(100.0);
        let effective_fov_y_half = (effective_h / viewport_h.max(1.0)) * fov_y_half;

        // 高さ基準・幅基準の距離
        // 透視投影: 手前面が frustum 内に収まるよう half_depth を加算
        // 正射影: 見かけの幅・高さは深度に依存しないため half_depth 不要
        let depth_offset = if self.perspective { half_d } else { 0.0 };
        let dist_h = half_h / effective_fov_y_half.tan() + depth_offset;
        let fov_x_half = (fov_y_half.tan() * aspect).atan();
        let dist_w = half_w / fov_x_half.tan() + depth_offset;

        let distance = (dist_h.max(dist_w) * FIT_MARGIN).max(2.0);

        // ターゲットを少し下げてオーバーレイ下にモデル中心を配置
        let world_per_px = 2.0 * distance * fov_y_half.tan() / viewport_h.max(1.0);
        let mut target = center;
        target.y -= world_per_px * margin_px * 0.25;

        (target, distance, model_radius)
    }

    /// ライト方向 — カメラ追従モード（MMD風: やや左上から）
    pub fn camera_following_light_dir(&self) -> Vec3 {
        let forward = (self.target - self.eye()).normalize();
        let world_up = self.up_vector();
        let right = forward.cross(world_up).normalize_or_zero();
        let up = right.cross(forward);
        (forward + right * (-0.3) + up * 0.7).normalize()
    }

    /// ライト方向 — 固定モード（MMD準拠: (-0.5,-1.0,0.5) の反転）
    pub fn fixed_light_dir() -> Vec3 {
        // -light_dir = (0.5, 1.0, -0.5) で正面法線(0,0,-1)が照らされる
        Vec3::new(-0.5, -1.0, 0.5).normalize()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixed_light_dir_normalized_and_correct_sign() {
        let dir = OrbitCamera::fixed_light_dir();
        assert!((dir.length() - 1.0).abs() < 1e-5);
        assert!(dir.y < 0.0); // -light_dir が上方を向く
    }

    #[test]
    fn camera_following_light_biases_left_up() {
        let cam = OrbitCamera::default();
        let dir = cam.camera_following_light_dir();
        assert!(dir.y > 0.0); // 上方成分あり
    }

    #[test]
    fn compute_fit_respects_aspect_ratio() {
        let cam = OrbitCamera::default();
        let min = Vec3::new(-10.0, 0.0, -1.0);
        let max = Vec3::new(10.0, 20.0, 1.0);
        let (_, dist_wide, _) = cam.compute_fit(min, max, 1920.0, 1080.0);
        let (_, dist_tall, _) = cam.compute_fit(min, max, 600.0, 1080.0);
        assert!(dist_tall > dist_wide);
    }

    #[test]
    fn compute_fit_accounts_for_forward_depth() {
        let cam = OrbitCamera::default();
        // 正面視点で X/Y は小さく Z だけ大きい bbox
        let thin = Vec3::new(-1.0, 0.0, -1.0);
        let thin_max = Vec3::new(1.0, 5.0, 1.0);
        let deep = Vec3::new(-1.0, 0.0, -20.0);
        let deep_max = Vec3::new(1.0, 5.0, 20.0);
        let (_, dist_thin, _) = cam.compute_fit(thin, thin_max, 1920.0, 1080.0);
        let (_, dist_deep, _) = cam.compute_fit(deep, deep_max, 1920.0, 1080.0);
        // 奥行きが大きい方がカメラを引く必要がある
        assert!(dist_deep > dist_thin);
    }

    #[test]
    fn compute_fit_ortho_ignores_depth() {
        let mut cam = OrbitCamera::default();
        cam.perspective = false;
        let thin = Vec3::new(-1.0, 0.0, -1.0);
        let thin_max = Vec3::new(1.0, 5.0, 1.0);
        let deep = Vec3::new(-1.0, 0.0, -20.0);
        let deep_max = Vec3::new(1.0, 5.0, 20.0);
        let (_, dist_thin, _) = cam.compute_fit(thin, thin_max, 1920.0, 1080.0);
        let (_, dist_deep, _) = cam.compute_fit(deep, deep_max, 1920.0, 1080.0);
        // 正射影では奥行きが distance に影響しない
        assert!((dist_thin - dist_deep).abs() < 0.01);
    }

    #[test]
    fn compute_fit_side_view_uses_depth() {
        let mut cam = OrbitCamera::default();
        cam.yaw = std::f32::consts::FRAC_PI_2; // 側面ビュー
                                               // Y高さを小さくし、Z奥行きを大きくして幅基準が支配的になるケース
        let min = Vec3::new(-1.0, 0.0, -10.0);
        let max = Vec3::new(1.0, 5.0, 10.0);
        let (_, dist_side, _) = cam.compute_fit(min, max, 1920.0, 1080.0);
        cam.yaw = 0.0; // 正面ビュー
        let (_, dist_front, _) = cam.compute_fit(min, max, 1920.0, 1080.0);
        assert!(dist_side > dist_front); // 側面からはZ奥行き20が画面幅に映るため距離増
    }
}
