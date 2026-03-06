use eframe::egui;
use glam::{Mat4, Vec3};

pub struct OrbitCamera {
    pub target: Vec3,
    pub distance: f32,
    pub yaw: f32,   // ラジアン
    pub pitch: f32,  // ラジアン
    /// モデルのバウンディング球の半径（ズーム感度に使用）
    pub model_radius: f32,
}

impl Default for OrbitCamera {
    fn default() -> Self {
        Self {
            target: Vec3::new(0.0, 15.0, 0.0),
            distance: 40.0,
            yaw: 0.0,
            pitch: 0.0,
            model_radius: 20.0,
        }
    }
}

impl OrbitCamera {
    /// マウス操作を処理
    pub fn handle_input(&mut self, ctx: &egui::Context, response: &egui::Response) {
        // 左ドラッグ: 回転
        if response.dragged_by(egui::PointerButton::Primary) {
            let delta = response.drag_delta();
            self.yaw -= delta.x * 0.005;
            self.pitch -= delta.y * 0.005;
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
            let speed = self.distance * 0.003;
            let (right, up) = self.view_axes();
            self.target += -right * delta.x * speed + up * delta.y * speed;
        }

        // ホイール: ズーム（モデルサイズに応じた感度）
        if response.hovered() {
            let scroll = ctx.input(|i| i.smooth_scroll_delta.y);
            if scroll != 0.0 {
                // モデル半径に応じた感度調整
                let sensitivity = 0.003 * (self.model_radius / 20.0).clamp(0.5, 3.0);
                self.distance *= (-scroll * sensitivity).exp();
                self.distance = self.distance.clamp(0.1, 5000.0);
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
        let right = forward.cross(Vec3::Y).normalize_or_zero();
        let up = right.cross(forward).normalize_or_zero();
        // pitch ≈ ±90° で right がゼロになる場合のフォールバック
        if right.length_squared() < 1e-6 {
            return (Vec3::X, Vec3::Z);
        }
        (right, up)
    }

    /// View-Projection 行列（左手系、wgpu NDC: Z∈[0,1]）
    /// near/far はカメラ距離に応じて動的調整
    pub fn view_proj(&self, aspect: f32) -> Mat4 {
        let view = Mat4::look_at_lh(self.eye(), self.target, Vec3::Y);
        let near = (self.distance * 0.005).clamp(0.01, 1.0);
        let far = (self.distance * 50.0).clamp(100.0, 50000.0);
        let proj = Mat4::perspective_lh(
            45.0_f32.to_radians(),
            aspect,
            near,
            far,
        );
        proj * view
    }

    /// バウンディングボックスにフィットさせる
    pub fn fit_to_bbox(&mut self, bbox_min: Vec3, bbox_max: Vec3) {
        let center = (bbox_min + bbox_max) * 0.5;
        let extent = (bbox_max - bbox_min).length();
        self.target = center;
        self.distance = (extent * 0.8).max(2.0);
        self.model_radius = extent * 0.5;
        self.yaw = 0.0;
        self.pitch = 0.0;
    }

    /// ライト方向 — カメラ追従モード
    pub fn camera_following_light_dir(&self) -> Vec3 {
        let forward = (self.target - self.eye()).normalize();
        let right = forward.cross(Vec3::Y).normalize_or_zero();
        let up = right.cross(forward);
        (forward + right * 0.3 + up * 0.5).normalize()
    }

    /// ライト方向 — 固定モード（上方45°前方から）
    pub fn fixed_light_dir() -> Vec3 {
        Vec3::new(0.3, 0.7, -0.5).normalize()
    }
}
