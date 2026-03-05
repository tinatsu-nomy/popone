use eframe::egui;
use glam::{Mat4, Vec3};

pub struct OrbitCamera {
    pub target: Vec3,
    pub distance: f32,
    pub yaw: f32,   // ラジアン
    pub pitch: f32,  // ラジアン
}

impl Default for OrbitCamera {
    fn default() -> Self {
        Self {
            target: Vec3::new(0.0, 15.0, 0.0),
            distance: 40.0,
            yaw: 0.0,
            pitch: 0.0,
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

        // 右ドラッグ: パン
        if response.dragged_by(egui::PointerButton::Secondary) {
            let delta = response.drag_delta();
            let speed = self.distance * 0.003;
            let right = self.right();
            let up = Vec3::Y;
            self.target += -right * delta.x * speed + up * delta.y * speed;
        }

        // ホイール: ズーム
        if response.hovered() {
            let scroll = ctx.input(|i| i.smooth_scroll_delta.y);
            if scroll != 0.0 {
                self.distance *= (-scroll * 0.003).exp();
                self.distance = self.distance.clamp(1.0, 500.0);
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

    /// カメラ右方向
    fn right(&self) -> Vec3 {
        let forward = (self.target - self.eye()).normalize();
        forward.cross(Vec3::Y).normalize()
    }

    /// View-Projection 行列（左手系、wgpu NDC: Z∈[0,1]）
    pub fn view_proj(&self, aspect: f32) -> Mat4 {
        let view = Mat4::look_at_lh(self.eye(), self.target, Vec3::Y);
        let proj = Mat4::perspective_lh(
            45.0_f32.to_radians(),
            aspect,
            0.1,
            1000.0,
        );
        proj * view
    }

    /// ライト方向（ビュー空間）— カメラ方向から若干ずらした方向
    pub fn light_dir(&self) -> Vec3 {
        let forward = (self.target - self.eye()).normalize();
        let right = forward.cross(Vec3::Y).normalize_or_zero();
        let up = right.cross(forward);
        (forward + right * 0.3 + up * 0.5).normalize()
    }
}
