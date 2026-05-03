use eframe::egui;
use glam::{Mat4, Vec3};

/// Camera input sensitivity constants.
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
    pub yaw: f32,   // radians
    pub pitch: f32, // radians
    /// Bounding-sphere radius of the model (used for zoom sensitivity).
    pub model_radius: f32,
    /// Perspective (true) / orthographic (false).
    pub perspective: bool,
    /// Base vertical FOV (radians). At render time an effective FOV is computed
    /// from this value, additionally corrected for the overlay height.
    pub fov_y_radians: f32,
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
            fov_y_radians: FOV_DEGREES.to_radians(),
        }
    }
}

impl OrbitCamera {
    /// Process mouse input.
    pub fn handle_input(&mut self, ctx: &egui::Context, response: &egui::Response) {
        // Shift = fine control (1/3 speed).
        let fine = if ctx.input(|i| i.modifiers.shift) {
            1.0 / 3.0
        } else {
            1.0
        };

        // Left drag: rotate.
        if response.dragged_by(egui::PointerButton::Primary) {
            let delta = response.drag_delta();
            self.yaw -= delta.x * YAW_PITCH_SENSITIVITY * fine;
            self.pitch -= delta.y * YAW_PITCH_SENSITIVITY * fine;
            self.pitch = self.pitch.clamp(
                -std::f32::consts::FRAC_PI_2 + 0.01,
                std::f32::consts::FRAC_PI_2 - 0.01,
            );
        }

        // Right drag / middle drag: pan (uses the view-space up and right axes).
        let is_pan = response.dragged_by(egui::PointerButton::Secondary)
            || response.dragged_by(egui::PointerButton::Middle);
        if is_pan {
            let delta = response.drag_delta();
            let fov_scale = self.fov_y_radians.to_degrees() / 45.0;
            let speed = self.distance * PAN_SPEED_FACTOR * fov_scale * fine;
            let (right, up) = self.view_axes();
            self.target += -right * delta.x * speed + up * delta.y * speed;
        }

        // Wheel: zoom (sensitivity scales with model size).
        if response.hovered() {
            let scroll = ctx.input(|i| i.smooth_scroll_delta.y);
            if scroll != 0.0 {
                // Sensitivity adjustment based on the model radius.
                let sensitivity = ZOOM_SENSITIVITY_BASE
                    * (self.model_radius / ZOOM_RADIUS_REF)
                        .clamp(ZOOM_SENSITIVITY_MIN, ZOOM_SENSITIVITY_MAX);
                self.distance *= (-scroll * sensitivity * fine).exp();
                self.distance = self.distance.clamp(DISTANCE_MIN, DISTANCE_MAX);
            }
        }
    }

    /// Camera position.
    /// In PMX coordinates the model faces +Z, so yaw=0 places the camera on the -Z side.
    pub fn eye(&self) -> Vec3 {
        let x = self.distance * self.pitch.cos() * self.yaw.sin();
        let y = self.distance * self.pitch.sin();
        let z = -self.distance * self.pitch.cos() * self.yaw.cos();
        self.target + Vec3::new(x, y, z)
    }

    /// Return view-space right and up vectors.
    fn view_axes(&self) -> (Vec3, Vec3) {
        let forward = (self.target - self.eye()).normalize();
        let world_up = self.up_vector();
        let right = forward.cross(world_up).normalize_or_zero();
        let up = right.cross(forward).normalize_or_zero();
        // Fallback when right collapses to zero near pitch ≈ ±90°.
        if right.length_squared() < 1e-6 {
            return (Vec3::X, Vec3::Z);
        }
        (right, up)
    }

    /// Up vector based on pitch (supports full 360° tilt).
    fn up_vector(&self) -> Vec3 {
        // Past pitch ±90° the view flips upside-down, so flip up by the sign of cos(pitch).
        if self.pitch.cos() >= 0.0 {
            Vec3::Y
        } else {
            Vec3::NEG_Y
        }
    }

    /// View matrix (left-handed).
    pub fn view_matrix(&self) -> Mat4 {
        Mat4::look_at_lh(self.eye(), self.target, self.up_vector())
    }

    /// Effective vertical FOV (radians), corrected for the overlay.
    ///
    /// Even when the central viewport is shrunk by `overlay_h` pixels by a
    /// bottom panel (such as the material edit panel), the FOV is scaled down
    /// so that the model's on-screen pixel size matches the no-overlay case.
    ///
    /// `tan(fov_eff/2) = tan(fov_y/2) * viewport_h / (viewport_h + overlay_h)`
    pub fn effective_fov_y_radians(&self, viewport_h: f32, overlay_h: f32) -> f32 {
        if overlay_h > 0.0 && viewport_h > 0.0 {
            let scale = viewport_h / (viewport_h + overlay_h);
            2.0 * ((self.fov_y_radians * 0.5).tan() * scale).atan()
        } else {
            self.fov_y_radians
        }
    }

    /// The [1][1] component of the projection matrix (= 1/tan(fov_y/2)).
    /// Used by the MToon ScreenCoordinates outline distance clamp.
    /// Uses the same effective FOV as `view_proj` so that outline thickness stays consistent.
    pub fn proj_11(&self, viewport_h: f32, overlay_h: f32) -> f32 {
        let fov_eff = self.effective_fov_y_radians(viewport_h, overlay_h);
        1.0 / (fov_eff * 0.5).tan()
    }

    /// View-Projection matrix (left-handed, Reverse-Z: near→1.0, far→0.0).
    /// near/far are adjusted dynamically based on camera distance.
    ///
    /// Pass the actual pixel height of the bottom overlay panel as `overlay_h`
    /// to keep the model's on-screen pixel size constant when the panel opens or closes.
    pub fn view_proj(&self, viewport_w: f32, viewport_h: f32, overlay_h: f32) -> Mat4 {
        let view = self.view_matrix();
        let near = (self.distance * NEAR_FACTOR).clamp(NEAR_MIN, NEAR_MAX);
        let far = (self.distance * FAR_FACTOR).clamp(FAR_MIN, FAR_MAX);
        let aspect = viewport_w.max(1.0) / viewport_h.max(1.0);
        let fov_eff = self.effective_fov_y_radians(viewport_h, overlay_h);
        let proj = if self.perspective {
            // Reverse-Z: swap near and far so depth maps 0 → far, 1 → near.
            Mat4::perspective_lh(fov_eff, aspect, far, near)
        } else {
            // Orthographic Reverse-Z: near/far are also swapped.
            let fov_half = fov_eff * 0.5;
            let half_h = self.distance * fov_half.tan();
            let half_w = half_h * aspect;
            Mat4::orthographic_lh(-half_w, half_w, -half_h, half_h, far, near)
        };
        proj * view
    }

    /// Fit to the bounding box (resets yaw/pitch).
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

    /// Fit: keep the current yaw/pitch and adjust only distance and target.
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

    /// Reset: snap yaw/pitch to face front and fit.
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

    /// Project the 8 bbox corners onto the current view axes and return half width / height / depth.
    /// half_depth is required so that perspective projection accounts for near-plane scaling.
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

    /// Shared core of the fit calculation (view-dependent: uses the projected width/height for the current yaw/pitch).
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
        let fov_y_half = self.fov_y_radians * 0.5;

        // Reserve about 60 px for the top overlay + bottom hint margin.
        let margin_px = 60.0;
        let effective_h = (viewport_h - margin_px).max(100.0);
        let effective_fov_y_half = (effective_h / viewport_h.max(1.0)) * fov_y_half;

        // Distance based on height vs. width.
        // Perspective: add half_depth so the near plane stays inside the frustum.
        // Orthographic: apparent width / height does not depend on depth, so half_depth is unnecessary.
        let depth_offset = if self.perspective { half_d } else { 0.0 };
        let dist_h = half_h / effective_fov_y_half.tan() + depth_offset;
        let fov_x_half = (fov_y_half.tan() * aspect).atan();
        let dist_w = half_w / fov_x_half.tan() + depth_offset;

        let distance = (dist_h.max(dist_w) * FIT_MARGIN).max(2.0);

        // Slide the target down a bit so the model center sits below the overlay.
        let world_per_px = 2.0 * distance * fov_y_half.tan() / viewport_h.max(1.0);
        let mut target = center;
        target.y -= world_per_px * margin_px * 0.25;

        (target, distance, model_radius)
    }

    /// Light direction — camera-following mode (MMD-like, slightly upper-left).
    pub fn camera_following_light_dir(&self) -> Vec3 {
        let forward = (self.target - self.eye()).normalize();
        let world_up = self.up_vector();
        let right = forward.cross(world_up).normalize_or_zero();
        let up = right.cross(forward);
        (forward + right * (-0.3) + up * 0.7).normalize()
    }

    /// Light direction — fixed mode (MMD compatible: negation of (-0.5,-1.0,0.5)).
    pub fn fixed_light_dir() -> Vec3 {
        // -light_dir = (0.5, 1.0, -0.5) lights the front-facing normal (0, 0, -1).
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
        assert!(dir.y < 0.0); // -light_dir points upward
    }

    #[test]
    fn camera_following_light_biases_left_up() {
        let cam = OrbitCamera::default();
        let dir = cam.camera_following_light_dir();
        assert!(dir.y > 0.0); // upward bias
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
        // Front-facing view: small X/Y, large Z bbox.
        let thin = Vec3::new(-1.0, 0.0, -1.0);
        let thin_max = Vec3::new(1.0, 5.0, 1.0);
        let deep = Vec3::new(-1.0, 0.0, -20.0);
        let deep_max = Vec3::new(1.0, 5.0, 20.0);
        let (_, dist_thin, _) = cam.compute_fit(thin, thin_max, 1920.0, 1080.0);
        let (_, dist_deep, _) = cam.compute_fit(deep, deep_max, 1920.0, 1080.0);
        // The deeper bbox needs the camera pulled back further.
        assert!(dist_deep > dist_thin);
    }

    #[test]
    fn compute_fit_ortho_ignores_depth() {
        let cam = OrbitCamera {
            perspective: false,
            ..Default::default()
        };
        let thin = Vec3::new(-1.0, 0.0, -1.0);
        let thin_max = Vec3::new(1.0, 5.0, 1.0);
        let deep = Vec3::new(-1.0, 0.0, -20.0);
        let deep_max = Vec3::new(1.0, 5.0, 20.0);
        let (_, dist_thin, _) = cam.compute_fit(thin, thin_max, 1920.0, 1080.0);
        let (_, dist_deep, _) = cam.compute_fit(deep, deep_max, 1920.0, 1080.0);
        // Under orthographic, depth does not affect distance.
        assert!((dist_thin - dist_deep).abs() < 0.01);
    }

    #[test]
    fn compute_fit_side_view_uses_depth() {
        let mut cam = OrbitCamera {
            yaw: std::f32::consts::FRAC_PI_2, // side view
            ..Default::default()
        };
        // Small Y height, large Z depth — the width-driven distance dominates.
        let min = Vec3::new(-1.0, 0.0, -10.0);
        let max = Vec3::new(1.0, 5.0, 10.0);
        let (_, dist_side, _) = cam.compute_fit(min, max, 1920.0, 1080.0);
        cam.yaw = 0.0; // front view
        let (_, dist_front, _) = cam.compute_fit(min, max, 1920.0, 1080.0);
        assert!(dist_side > dist_front); // From the side, Z depth = 20 fills the screen width, so distance grows.
    }
}
