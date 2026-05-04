//! Aggregated "IR overwrite values" produced by the material edit drawer (Step 2).
//!
//! Step 1 stored only `diffuse` as a `HashMap<usize, Vec4>`. Step 2 adds the §E
//! sections (shade / outline / rim / matcap / UV animation / emissive / normal /
//! misc), so all fields are aggregated into a single `MaterialParamOverride` struct.
//!
//! ## Why a struct?
//!
//! Adding `material_xxx_overrides: HashMap<usize, Type>` per section in Step 2
//! would balloon to 20+ fields and break the consistency of init / merge /
//! re-apply. Aggregating into `MaterialParamOverride` lets the edit path and the
//! reload re-apply path share a single `apply_to()`.
//!
//! ## Relation to Step 3
//!
//! Step 3's planned `MaterialEditRecord.param_override` (§I) is the future form
//! of this struct, replacing the hand-written diff/apply with one auto-generated
//! by `declarative_macro`. This file is the "minimal hand-written prototype"
//! that lands first; Step 3 macroizes it into a sustainable structure.

use glam::{Vec2, Vec3, Vec4};

use crate::intermediate::types::{
    AlphaMode, CullMode, IrMaterial, IrTextureInfo, MtoonParams, OutlineWidthMode, ShaderFamily,
};

/// Per-slot KHR_texture_transform overwrite values (v0.5.4).
///
/// For each slot that owns an `IrTextureInfo` (BaseColor / Emissive / Normal /
/// Shade / ShadingShift / RimMultiply / OutlineWidth / Matcap / UvAnimMask),
/// store `offset / scale / rotation` as deltas. All-`None` means "still
/// pristine". `rotation` is in **radians** (the UI takes degrees and stores
/// radians).
#[derive(Debug, Clone, Default, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct TextureUvOverride {
    pub offset: Option<[f32; 2]>,
    pub scale: Option<[f32; 2]>,
    pub rotation: Option<f32>,
}

impl TextureUvOverride {
    pub fn is_empty(&self) -> bool {
        self.offset.is_none() && self.scale.is_none() && self.rotation.is_none()
    }

    /// Compare `pristine` and `current` IrTextureInfo and return only the differing fields as `Some(_)`.
    ///
    /// - `(Some, Some)`: compare directly.
    /// - `(None, Some)`: a **newly assigned slot**; compare `current` against the
    ///   default transform (offset = 0 / scale = 1 / rotation = 0) so UV edits
    ///   land in the history. Without this, the "assign a texture to an unbound
    ///   slot then edit its UV" case loses its UV diff at save time
    ///   (review_result_01 [P1]).
    /// - `(Some, None)`: the slot itself was unbound. The slot info is dropped
    ///   on the texture_mgmt side, so the UV diff is meaningless — return `None`.
    /// - `(None, None)`: no change.
    pub fn diff(pristine: Option<&IrTextureInfo>, current: Option<&IrTextureInfo>) -> Option<Self> {
        let c = current?;
        let mut out = Self::default();
        let (p_offset, p_scale, p_rotation) = match pristine {
            Some(p) => (p.offset, p.scale, p.rotation),
            None => (glam::Vec2::ZERO, glam::Vec2::ONE, 0.0),
        };
        if p_offset != c.offset {
            out.offset = Some(c.offset.to_array());
        }
        if p_scale != c.scale {
            out.scale = Some(c.scale.to_array());
        }
        if (p_rotation - c.rotation).abs() > f32::EPSILON {
            out.rotation = Some(c.rotation);
        }
        if out.is_empty() {
            None
        } else {
            Some(out)
        }
    }

    /// Apply the diff to an existing `IrTextureInfo`. Does nothing when the target slot is unbound (`None`).
    pub fn apply(&self, info: &mut Option<IrTextureInfo>) {
        let Some(ti) = info.as_mut() else {
            return;
        };
        if let Some(o) = self.offset {
            ti.offset = Vec2::from_array(o);
        }
        if let Some(s) = self.scale {
            ti.scale = Vec2::from_array(s);
        }
        if let Some(r) = self.rotation {
            ti.rotation = r;
        }
    }
}

/// Per-material parameter overwrite values (only `Some(_)` fields are written into the IR).
///
/// When the IR is rebuilt by an A-stance / T-stance reload etc., `apply_to()`
/// re-applies the overwrites onto the new IR. Currently it holds only colors
/// and scalar values for every section (**including diffuse RGB**); texture
/// assignments (`TextureSlot`) are managed separately via `tex.assignments`.
///
/// `Default::default()` is the "empty overwrite" with every field `None`.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct MaterialParamOverride {
    // ===== MToon enable (§G / Step 2-10) =====
    //
    // `Some(true)` = the user checked "enable MToon"
    //   -> `apply_to` sets `shader_family = Mtoon` + `mtoon = Some(default)`.
    // `Some(false)` = the user unchecked "enable MToon"
    //   -> `apply_to` reverts to `shader_family = Other` + `mtoon = None`.
    // `None` = untouched (preserve the IR-side existing value).
    //
    // **Order matters**: `apply_to` handles `enable_mtoon` **first**, then the
    // other MToon fields (shade_color etc.). Otherwise `mtoon_mut()` runs
    // ahead of time and `shader_family` / `mtoon` go out of sync.
    pub enable_mtoon: Option<bool>,

    // ===== Basic section (§E-1) =====
    pub diffuse: Option<Vec4>,
    pub alpha_mode: Option<AlphaMode>,
    pub alpha_cutoff: Option<f32>,
    pub cull_mode: Option<CullMode>,

    // ===== Shade section (§E-2) =====
    /// `MtoonParams.shade_color` is `Option<Vec3>`, so this is `Option<Vec3>`
    /// (not `Option<Option<Vec3>>`) — meaning "if Some, set
    /// `MtoonParams.shade_color = Some(_)`". A path that explicitly clears
    /// shade (sets it to `None`) is added in Step 3.
    pub shade_color: Option<Vec3>,
    pub shading_toony_factor: Option<f32>,
    pub shading_shift_factor: Option<f32>,
    pub gi_equalization_factor: Option<f32>,

    // ===== Outline section (§E-3) =====
    pub edge_color: Option<Vec4>,
    pub edge_size: Option<f32>,
    pub outline_width_mode: Option<OutlineWidthMode>,
    pub outline_width_factor: Option<f32>,
    pub outline_lighting_mix: Option<f32>,

    // ===== Rim section (§E-4) =====
    pub parametric_rim_color: Option<Vec3>,
    pub parametric_rim_fresnel_power: Option<f32>,
    pub parametric_rim_lift: Option<f32>,
    pub rim_lighting_mix: Option<f32>,

    // ===== MatCap section (§E-5) =====
    pub matcap_factor: Option<Vec3>,

    // ===== UV animation section (§E-6) =====
    pub uv_animation_scroll_x_speed: Option<f32>,
    pub uv_animation_scroll_y_speed: Option<f32>,
    pub uv_animation_rotation_speed: Option<f32>,

    // ===== Emissive / normal section (§E-7) =====
    pub emissive_factor: Option<Vec3>,
    pub normal_texture_scale: Option<f32>,

    // ===== Misc section (§E-8) =====
    pub render_queue_offset: Option<i32>,

    // ===== MME category overwrite (§K.3 / Step 6) =====
    /// User-overridden ray-mmd category. `None` = use the inferred value.
    pub mme_kind: Option<crate::convert::mme::ray_mmd::RayMmdMaterialKind>,

    // ===== Material name (v0.5.3) =====
    /// User-edited material name. `None` = keep the pristine name as-is.
    /// `String` is not Copy, so merge / apply clone it individually.
    pub name: Option<String>,

    // ===== Per-slot UV transform (v0.5.4) =====
    //
    // Only effective for slots that own an `IrTextureInfo`. Apply on an
    // unbound slot is a no-op (we do not silently insert `from_index(0)`).
    // Independent path from Expression-driven UV animation
    // (`IrTextureTransformBind`); the order is "static override -> Expression
    // additive" so they coexist.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base_color_uv: Option<TextureUvOverride>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub emissive_uv: Option<TextureUvOverride>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub normal_uv: Option<TextureUvOverride>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shade_uv: Option<TextureUvOverride>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shading_shift_uv: Option<TextureUvOverride>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rim_multiply_uv: Option<TextureUvOverride>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub outline_width_uv: Option<TextureUvOverride>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub matcap_uv: Option<TextureUvOverride>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub uv_animation_mask_uv: Option<TextureUvOverride>,
}

impl MaterialParamOverride {
    /// Empty overwrite (alias for `Default::default()`).
    pub fn new() -> Self {
        Self::default()
    }

    /// Adopt every `Some(_)` field from `other` into self (overwrites self).
    ///
    /// In ui.rs's `show_material_editor_window`, one frame's worth of edits is
    /// accumulated in a `pending_override` and merged into
    /// `material_overrides[mat_idx]` outside the closure. Writing
    /// `if let Some(v) = ...` 24+ times every frame would be tedious, so a
    /// local `merge` macro processes every field in one pass.
    ///
    /// Will be replaced by Step 3's `declarative_macro` version
    /// (`define_param_override!`); until then this method merges concisely.
    pub fn merge_from(&mut self, other: &Self) {
        macro_rules! merge {
            ($($f:ident),* $(,)?) => {
                $(
                    if other.$f.is_some() {
                        self.$f = other.$f;
                    }
                )*
            };
        }
        merge!(
            // MToon enable
            enable_mtoon,
            // Basic
            diffuse,
            alpha_mode,
            alpha_cutoff,
            cull_mode,
            // Shade
            shade_color,
            shading_toony_factor,
            shading_shift_factor,
            gi_equalization_factor,
            // Outline
            edge_color,
            edge_size,
            outline_width_mode,
            outline_width_factor,
            outline_lighting_mix,
            // Rim
            parametric_rim_color,
            parametric_rim_fresnel_power,
            parametric_rim_lift,
            rim_lighting_mix,
            // MatCap
            matcap_factor,
            // UV animation
            uv_animation_scroll_x_speed,
            uv_animation_scroll_y_speed,
            uv_animation_rotation_speed,
            // Emissive / normal
            emissive_factor,
            normal_texture_scale,
            // Misc
            render_queue_offset,
            // MME
            mme_kind,
        );
        // String is not Copy; clone individually.
        if let Some(ref v) = other.name {
            self.name = Some(v.clone());
        }
        // UV overrides are Option<TextureUvOverride> (not Copy); clone individually.
        macro_rules! merge_uv {
            ($($f:ident),* $(,)?) => {
                $(
                    if let Some(ref v) = other.$f {
                        self.$f = Some(v.clone());
                    }
                )*
            };
        }
        merge_uv!(
            base_color_uv,
            emissive_uv,
            normal_uv,
            shade_uv,
            shading_shift_uv,
            rim_multiply_uv,
            outline_width_uv,
            matcap_uv,
            uv_animation_mask_uv,
        );
    }

    /// Returns `true` when at least one field is `Some(_)`.
    /// An empty overwrite is wasteful to store, so callers gate HashMap inserts on this.
    pub fn is_empty(&self) -> bool {
        self.enable_mtoon.is_none()
            && self.diffuse.is_none()
            && self.alpha_mode.is_none()
            && self.alpha_cutoff.is_none()
            && self.cull_mode.is_none()
            && self.shade_color.is_none()
            && self.shading_toony_factor.is_none()
            && self.shading_shift_factor.is_none()
            && self.gi_equalization_factor.is_none()
            && self.edge_color.is_none()
            && self.edge_size.is_none()
            && self.outline_width_mode.is_none()
            && self.outline_width_factor.is_none()
            && self.outline_lighting_mix.is_none()
            && self.parametric_rim_color.is_none()
            && self.parametric_rim_fresnel_power.is_none()
            && self.parametric_rim_lift.is_none()
            && self.rim_lighting_mix.is_none()
            && self.matcap_factor.is_none()
            && self.uv_animation_scroll_x_speed.is_none()
            && self.uv_animation_scroll_y_speed.is_none()
            && self.uv_animation_rotation_speed.is_none()
            && self.emissive_factor.is_none()
            && self.normal_texture_scale.is_none()
            && self.render_queue_offset.is_none()
            && self.mme_kind.is_none()
            && self.name.is_none()
            && self.base_color_uv.is_none()
            && self.emissive_uv.is_none()
            && self.normal_uv.is_none()
            && self.shade_uv.is_none()
            && self.shading_shift_uv.is_none()
            && self.rim_multiply_uv.is_none()
            && self.outline_width_uv.is_none()
            && self.matcap_uv.is_none()
            && self.uv_animation_mask_uv.is_none()
    }

    /// Compare `pristine` (the IR material right after load) and `current`
    /// (the IR material after the user edits) and return a
    /// `MaterialParamOverride` with `Some(_)` only on differing fields.
    /// Returns `None` if every field matches.
    ///
    /// **Persistence usage**: `MaterialEditRecord.param_override` saved in
    /// `popone_history.json` v2 contains only the diff computed here. Fields
    /// matching the load-time value stay `None` and are skip_serialized, so
    /// the file size is the minimum possible.
    ///
    /// **enable_mtoon diff**: when `shader_family` differs between pristine
    /// and current, set `enable_mtoon = Some(current == Mtoon)`.
    pub fn diff_from(pristine: &IrMaterial, current: &IrMaterial) -> Option<Self> {
        let mut out = Self::default();

        // enable_mtoon: shader_family difference.
        if pristine.shader_family != current.shader_family {
            out.enable_mtoon = Some(matches!(current.shader_family, ShaderFamily::Mtoon));
        }

        // Material name diff (String is not Copy, so it cannot use the diff_field macro).
        if pristine.name != current.name {
            out.name = Some(current.name.clone());
        }

        // Basic.
        macro_rules! diff_field {
            ($field:ident, $get:expr) => {
                if $get(pristine) != $get(current) {
                    out.$field = Some($get(current));
                }
            };
        }

        diff_field!(diffuse, |m: &IrMaterial| m.diffuse);
        diff_field!(alpha_mode, |m: &IrMaterial| m.alpha_mode);
        diff_field!(alpha_cutoff, |m: &IrMaterial| m.alpha_cutoff);
        diff_field!(cull_mode, |m: &IrMaterial| m.cull_mode);
        diff_field!(edge_color, |m: &IrMaterial| m.edge_color);
        diff_field!(edge_size, |m: &IrMaterial| m.edge_size);
        diff_field!(emissive_factor, |m: &IrMaterial| m.emissive_factor);
        diff_field!(normal_texture_scale, |m: &IrMaterial| m
            .normal_texture_scale);

        // MToon-related fields (mtoon() falls back to defaults; side-effect free).
        //
        // review_009 [P2] fix: when enable_mtoon = Some(false) (the user turned MToon off),
        // skip diffing every MToon-related field. Reasons:
        // - current.mtoon() returns MTOON_DEFAULT even when mtoon = None, so a
        //   "turned-off" material would still produce diffs against pristine and
        //   save MToon-related overrides.
        // - On apply_to restore, after enable_mtoon = false sets mtoon = None,
        //   any MToon override would re-enter via has_mtoon_override = true,
        //   call mtoon_mut() -> Some(default), and break the round-trip.
        //
        // shade_color is Option<Vec3>, so the diff_field! macro cannot be used
        // (it would become Some(Option<Vec3>) = Option<Option<Vec3>>). Handle by direct assignment.
        let diff_mtoon = out.enable_mtoon != Some(false);
        if diff_mtoon {
            let p = pristine.mtoon().shade_color;
            let c = current.mtoon().shade_color;
            if p != c {
                out.shade_color = c;
            }
            diff_field!(shading_toony_factor, |m: &IrMaterial| m
                .mtoon()
                .shading_toony_factor);
            diff_field!(shading_shift_factor, |m: &IrMaterial| m
                .mtoon()
                .shading_shift_factor);
            diff_field!(gi_equalization_factor, |m: &IrMaterial| m
                .mtoon()
                .gi_equalization_factor);
            diff_field!(outline_width_mode, |m: &IrMaterial| m
                .mtoon()
                .outline_width_mode);
            diff_field!(outline_width_factor, |m: &IrMaterial| m
                .mtoon()
                .outline_width_factor);
            diff_field!(outline_lighting_mix, |m: &IrMaterial| m
                .mtoon()
                .outline_lighting_mix);
            diff_field!(parametric_rim_color, |m: &IrMaterial| m
                .mtoon()
                .parametric_rim_color);
            diff_field!(parametric_rim_fresnel_power, |m: &IrMaterial| m
                .mtoon()
                .parametric_rim_fresnel_power);
            diff_field!(parametric_rim_lift, |m: &IrMaterial| m
                .mtoon()
                .parametric_rim_lift);
            diff_field!(rim_lighting_mix, |m: &IrMaterial| m
                .mtoon()
                .rim_lighting_mix);
            diff_field!(matcap_factor, |m: &IrMaterial| m.mtoon().matcap_factor);
            diff_field!(uv_animation_scroll_x_speed, |m: &IrMaterial| m
                .mtoon()
                .uv_animation_scroll_x_speed);
            diff_field!(uv_animation_scroll_y_speed, |m: &IrMaterial| m
                .mtoon()
                .uv_animation_scroll_y_speed);
            diff_field!(uv_animation_rotation_speed, |m: &IrMaterial| m
                .mtoon()
                .uv_animation_rotation_speed);
            diff_field!(render_queue_offset, |m: &IrMaterial| m
                .mtoon()
                .render_queue_offset);
        } // end if diff_mtoon

        // ===== Per-slot UV (v0.5.4) =====
        //
        // BaseColor / Emissive / Normal are accessible even on non-MToon, while
        // the 6 MToon-only slots are handled separately. Skip every MToon-only
        // slot when enable_mtoon == Some(false) so the round-trip never leaves
        // unwanted diffs on the mtoon = None side.
        out.base_color_uv = TextureUvOverride::diff(
            pristine.base_color_tex_info.as_ref(),
            current.base_color_tex_info.as_ref(),
        );
        out.emissive_uv = TextureUvOverride::diff(
            pristine.emissive_texture.as_ref(),
            current.emissive_texture.as_ref(),
        );
        out.normal_uv = TextureUvOverride::diff(
            pristine.normal_texture.as_ref(),
            current.normal_texture.as_ref(),
        );
        if diff_mtoon {
            let p = pristine.mtoon();
            let c = current.mtoon();
            out.shade_uv =
                TextureUvOverride::diff(p.shade_texture.as_ref(), c.shade_texture.as_ref());
            out.shading_shift_uv = TextureUvOverride::diff(
                p.shading_shift_texture.as_ref(),
                c.shading_shift_texture.as_ref(),
            );
            out.rim_multiply_uv = TextureUvOverride::diff(
                p.rim_multiply_texture.as_ref(),
                c.rim_multiply_texture.as_ref(),
            );
            out.outline_width_uv = TextureUvOverride::diff(
                p.outline_width_texture.as_ref(),
                c.outline_width_texture.as_ref(),
            );
            out.matcap_uv =
                TextureUvOverride::diff(p.matcap_texture.as_ref(), c.matcap_texture.as_ref());
            out.uv_animation_mask_uv = TextureUvOverride::diff(
                p.uv_animation_mask_texture.as_ref(),
                c.uv_animation_mask_texture.as_ref(),
            );
        }

        if out.is_empty() {
            None
        } else {
            Some(out)
        }
    }

    /// Apply self's overwrites to `mat`. Only `Some(_)` fields are written.
    ///
    /// **Apply order**:
    /// 1. Process `enable_mtoon` first (settles `shader_family` and the presence of `mtoon`).
    /// 2. Apply basic fields (diffuse etc.) and non-MToon fields.
    /// 3. Write MToon fields via `mtoon_mut()` (since step 1 settled
    ///    `mtoon = Some(_)`, the result is the intended state).
    ///
    /// **Relation to §G**: applying ordinary MToon fields does not change
    /// `shader_family`; only the explicit `enable_mtoon` toggle switches it.
    pub fn apply_to(&self, mat: &mut IrMaterial) {
        // Material name (v0.5.3).
        if let Some(ref v) = self.name {
            mat.name = v.clone();
        }

        // ===== MToon enable (highest priority) =====
        // Process **before** any other MToon field application so that
        // subsequent `mtoon_mut()` calls do not produce inconsistencies.
        if let Some(enable) = self.enable_mtoon {
            if enable {
                mat.shader_family = ShaderFamily::Mtoon;
                if mat.mtoon.is_none() {
                    mat.mtoon = Some(MtoonParams::default());
                }
            } else {
                mat.shader_family = ShaderFamily::Other;
                mat.mtoon = None;
            }
        }

        // Basic.
        if let Some(v) = self.diffuse {
            mat.diffuse = v;
        }
        if let Some(m) = self.alpha_mode {
            mat.alpha_mode = m;
        }
        if let Some(c) = self.alpha_cutoff {
            mat.alpha_cutoff = c;
        }
        if let Some(c) = self.cull_mode {
            mat.cull_mode = c;
        }

        // Fields accessible even on non-MToon materials.
        if let Some(v) = self.edge_color {
            mat.edge_color = v;
        }
        if let Some(v) = self.edge_size {
            mat.edge_size = v;
        }
        if let Some(v) = self.emissive_factor {
            mat.emissive_factor = v;
        }
        if let Some(v) = self.normal_texture_scale {
            mat.normal_texture_scale = v;
        }

        // UV overrides for non-MToon slots (v0.5.4): no-op when the texture is unbound.
        if let Some(ref uv) = self.base_color_uv {
            uv.apply(&mut mat.base_color_tex_info);
        }
        if let Some(ref uv) = self.emissive_uv {
            uv.apply(&mut mat.emissive_texture);
        }
        if let Some(ref uv) = self.normal_uv {
            uv.apply(&mut mat.normal_texture);
        }

        // MToon fields: if any one is set, initialize mtoon and then write the values.
        //
        // review_009 [P2] fix: when enable_mtoon = Some(false), skip every
        // MToon override. This prevents the round-trip mismatch where we just
        // set mtoon = None but mtoon_mut() runs here and re-inserts Some(default).
        if self.enable_mtoon == Some(false) {
            return;
        }
        let has_mtoon_override = self.shade_color.is_some()
            || self.shading_toony_factor.is_some()
            || self.shading_shift_factor.is_some()
            || self.gi_equalization_factor.is_some()
            || self.outline_width_mode.is_some()
            || self.outline_width_factor.is_some()
            || self.outline_lighting_mix.is_some()
            || self.parametric_rim_color.is_some()
            || self.parametric_rim_fresnel_power.is_some()
            || self.parametric_rim_lift.is_some()
            || self.rim_lighting_mix.is_some()
            || self.matcap_factor.is_some()
            || self.uv_animation_scroll_x_speed.is_some()
            || self.uv_animation_scroll_y_speed.is_some()
            || self.uv_animation_rotation_speed.is_some()
            || self.render_queue_offset.is_some();

        if has_mtoon_override {
            let mp = mat.mtoon_mut();
            // Shade.
            if let Some(v) = self.shade_color {
                mp.shade_color = Some(v);
            }
            if let Some(v) = self.shading_toony_factor {
                mp.shading_toony_factor = v;
            }
            if let Some(v) = self.shading_shift_factor {
                mp.shading_shift_factor = v;
            }
            if let Some(v) = self.gi_equalization_factor {
                mp.gi_equalization_factor = v;
            }
            // Outline.
            if let Some(v) = self.outline_width_mode {
                mp.outline_width_mode = v;
            }
            if let Some(v) = self.outline_width_factor {
                mp.outline_width_factor = v;
            }
            if let Some(v) = self.outline_lighting_mix {
                mp.outline_lighting_mix = v;
            }
            // Rim.
            if let Some(v) = self.parametric_rim_color {
                mp.parametric_rim_color = v;
            }
            if let Some(v) = self.parametric_rim_fresnel_power {
                mp.parametric_rim_fresnel_power = v;
            }
            if let Some(v) = self.parametric_rim_lift {
                mp.parametric_rim_lift = v;
            }
            if let Some(v) = self.rim_lighting_mix {
                mp.rim_lighting_mix = v;
            }
            // MatCap.
            if let Some(v) = self.matcap_factor {
                mp.matcap_factor = v;
            }
            // UV animation.
            if let Some(v) = self.uv_animation_scroll_x_speed {
                mp.uv_animation_scroll_x_speed = v;
            }
            if let Some(v) = self.uv_animation_scroll_y_speed {
                mp.uv_animation_scroll_y_speed = v;
            }
            if let Some(v) = self.uv_animation_rotation_speed {
                mp.uv_animation_rotation_speed = v;
            }
            // Misc.
            if let Some(v) = self.render_queue_offset {
                mp.render_queue_offset = v;
            }
        }

        // UV overrides for MToon slots (v0.5.4):
        //
        // Avoid `mtoon_mut()` (would silently insert a default mtoon on a
        // non-MToon material). Only when `mat.mtoon` is already Some, write
        // into the existing slot's IrTextureInfo. `TextureUvOverride::apply()`
        // is a no-op for slots without a bound texture.
        if let Some(ref mut mp) = mat.mtoon {
            if let Some(ref uv) = self.shade_uv {
                uv.apply(&mut mp.shade_texture);
            }
            if let Some(ref uv) = self.shading_shift_uv {
                uv.apply(&mut mp.shading_shift_texture);
            }
            if let Some(ref uv) = self.rim_multiply_uv {
                uv.apply(&mut mp.rim_multiply_texture);
            }
            if let Some(ref uv) = self.outline_width_uv {
                uv.apply(&mut mp.outline_width_texture);
            }
            if let Some(ref uv) = self.matcap_uv {
                uv.apply(&mut mp.matcap_texture);
            }
            if let Some(ref uv) = self.uv_animation_mask_uv {
                uv.apply(&mut mp.uv_animation_mask_texture);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::intermediate::types::MtoonParams;

    /// MToon material diff -> apply round-trip: the material is equivalent before and after save.
    #[test]
    fn test_diff_apply_roundtrip_mtoon() {
        let pristine = IrMaterial::default();
        let mut current = pristine.clone();
        current.diffuse = Vec4::new(1.0, 0.0, 0.0, 1.0);
        current.shader_family = ShaderFamily::Mtoon;
        current.mtoon = Some(MtoonParams {
            shade_color: Some(Vec3::new(0.5, 0.0, 0.0)),
            shading_toony_factor: 0.7,
            ..MtoonParams::default()
        });

        let diff = MaterialParamOverride::diff_from(&pristine, &current);
        assert!(diff.is_some(), "diff should be Some when there are changes");

        // apply: applying diff to pristine should yield the same as current.
        let mut restored = pristine.clone();
        diff.unwrap().apply_to(&mut restored);

        assert_eq!(restored.diffuse, current.diffuse);
        assert_eq!(restored.shader_family, current.shader_family);
        assert!(restored.mtoon.is_some());
        assert_eq!(
            restored.mtoon.as_ref().unwrap().shade_color,
            current.mtoon.as_ref().unwrap().shade_color,
        );
        assert!(
            (restored.mtoon.as_ref().unwrap().shading_toony_factor
                - current.mtoon.as_ref().unwrap().shading_toony_factor)
                .abs()
                < 1e-6,
        );
    }

    /// MToon ON -> OFF round-trip (review_009 [P2]): diffing then applying the
    /// OFF state must preserve `mtoon = None` + `shader_family = Other`.
    #[test]
    fn test_diff_apply_roundtrip_mtoon_off() {
        // pristine: an MToon material (state right after VRM load).
        let mut pristine = IrMaterial::default();
        pristine.shader_family = ShaderFamily::Mtoon;
        pristine.mtoon = Some(MtoonParams::default());

        // The user unchecked "enable MToon".
        let mut current = pristine.clone();
        current.shader_family = ShaderFamily::Other;
        current.mtoon = None;

        let diff = MaterialParamOverride::diff_from(&pristine, &current);
        assert!(
            diff.is_some(),
            "diff should appear when shader_family changes"
        );

        let diff = diff.unwrap();
        assert_eq!(
            diff.enable_mtoon,
            Some(false),
            "MToon disable should be recorded"
        );

        // apply: applying diff to pristine should yield the same as current.
        let mut restored = pristine.clone();
        diff.apply_to(&mut restored);

        assert_eq!(restored.shader_family, ShaderFamily::Other);
        assert!(
            restored.mtoon.is_none(),
            "mtoon = None が保持されるべき（mtoon_mut() が再挿入してはならない）"
        );
    }

    /// No change -> diff is None.
    #[test]
    fn test_diff_from_no_change() {
        let mat = IrMaterial::default();
        let diff = MaterialParamOverride::diff_from(&mat, &mat);
        assert!(
            diff.is_none(),
            "diff should be None when there are no changes"
        );
    }

    // ===== Step 7-35: expanded diff_from / apply_to tests =====

    /// Per-field diff: only diffuse changed.
    #[test]
    fn test_diff_from_diffuse_only() {
        let pristine = IrMaterial::default();
        let mut current = pristine.clone();
        current.diffuse = Vec4::new(0.5, 0.3, 0.1, 0.9);

        let diff = MaterialParamOverride::diff_from(&pristine, &current).unwrap();
        assert_eq!(diff.diffuse, Some(Vec4::new(0.5, 0.3, 0.1, 0.9)));
        // Other fields stay None.
        assert!(diff.emissive_factor.is_none());
        assert!(diff.enable_mtoon.is_none());
    }

    /// Only emissive_factor changed.
    #[test]
    fn test_diff_from_emissive_only() {
        let pristine = IrMaterial::default();
        let mut current = pristine.clone();
        current.emissive_factor = Vec3::new(1.0, 0.5, 0.0);

        let diff = MaterialParamOverride::diff_from(&pristine, &current).unwrap();
        assert_eq!(diff.emissive_factor, Some(Vec3::new(1.0, 0.5, 0.0)));
        assert!(diff.diffuse.is_none());
    }

    /// mme_kind is not part of IrMaterial, so diff_from never emits it.
    #[test]
    fn test_diff_from_never_sets_mme_kind() {
        let pristine = IrMaterial::default();
        let mut current = pristine.clone();
        current.diffuse = Vec4::new(1.0, 0.0, 0.0, 1.0);

        let diff = MaterialParamOverride::diff_from(&pristine, &current).unwrap();
        assert!(
            diff.mme_kind.is_none(),
            "mme_kind は diff_from では設定されない"
        );
    }

    /// is_empty: every field None -> true.
    #[test]
    fn test_is_empty_default() {
        let ov = MaterialParamOverride::default();
        assert!(ov.is_empty());
    }

    /// is_empty: at least one Some field -> false.
    #[test]
    fn test_is_empty_with_mme_kind() {
        let mut ov = MaterialParamOverride::default();
        ov.mme_kind = Some(crate::convert::mme::ray_mmd::RayMmdMaterialKind::Skin);
        assert!(!ov.is_empty());
    }

    /// merge_from: only Some fields are overwritten.
    #[test]
    fn test_merge_from_selective() {
        let mut base = MaterialParamOverride::default();
        base.diffuse = Some(Vec4::new(1.0, 0.0, 0.0, 1.0));

        let patch = MaterialParamOverride {
            emissive_factor: Some(Vec3::new(0.5, 0.5, 0.5)),
            ..Default::default()
        };

        base.merge_from(&patch);
        // diffuse is unchanged.
        assert_eq!(base.diffuse, Some(Vec4::new(1.0, 0.0, 0.0, 1.0)));
        // emissive_factor was merged in.
        assert_eq!(base.emissive_factor, Some(Vec3::new(0.5, 0.5, 0.5)));
    }

    /// When enable_mtoon = false, MToon fields are excluded from the diff.
    #[test]
    fn test_diff_from_mtoon_off_skips_mtoon_fields() {
        let mut pristine = IrMaterial::default();
        pristine.shader_family = ShaderFamily::Mtoon;
        pristine.mtoon = Some(MtoonParams {
            shading_toony_factor: 0.9,
            ..MtoonParams::default()
        });

        let mut current = pristine.clone();
        current.shader_family = ShaderFamily::Other;
        current.mtoon = None;

        let diff = MaterialParamOverride::diff_from(&pristine, &current).unwrap();
        assert_eq!(diff.enable_mtoon, Some(false));
        // MToon fields are skipped.
        assert!(diff.shade_color.is_none());
        assert!(diff.shading_toony_factor.is_none());
    }

    // ===== v0.5.4: per-slot UV transform =====

    /// TextureUvOverride::default() is is_empty == true and produces no spurious diff.
    #[test]
    fn test_uv_override_default_is_empty() {
        let ov = TextureUvOverride::default();
        assert!(ov.is_empty());
        assert!(MaterialParamOverride::default().is_empty());
    }

    /// BaseColor UV offset / scale / rotation round-trip: diff -> apply restores the values.
    #[test]
    fn test_diff_apply_roundtrip_base_color_uv() {
        let mut pristine = IrMaterial::default();
        pristine.base_color_tex_info = Some(IrTextureInfo::from_index(0));

        let mut current = pristine.clone();
        {
            let ti = current.base_color_tex_info.as_mut().unwrap();
            ti.offset = Vec2::new(0.25, -0.5);
            ti.scale = Vec2::new(2.0, 0.5);
            ti.rotation = std::f32::consts::FRAC_PI_4;
        }

        let diff = MaterialParamOverride::diff_from(&pristine, &current)
            .expect("diff should be Some because of UV changes");
        let uv = diff
            .base_color_uv
            .as_ref()
            .expect("base_color_uv should be Some");
        assert_eq!(uv.offset, Some([0.25, -0.5]));
        assert_eq!(uv.scale, Some([2.0, 0.5]));
        assert_eq!(uv.rotation, Some(std::f32::consts::FRAC_PI_4));

        let mut restored = pristine.clone();
        diff.apply_to(&mut restored);
        let ti = restored.base_color_tex_info.as_ref().unwrap();
        assert_eq!(ti.offset, Vec2::new(0.25, -0.5));
        assert_eq!(ti.scale, Vec2::new(2.0, 0.5));
        assert!((ti.rotation - std::f32::consts::FRAC_PI_4).abs() < 1e-6);
    }

    /// MToon slot (shade) UV round-trip.
    #[test]
    fn test_diff_apply_roundtrip_mtoon_slot_uv() {
        let mut pristine = IrMaterial::default();
        pristine.shader_family = ShaderFamily::Mtoon;
        let mut mp = MtoonParams::default();
        mp.shade_texture = Some(IrTextureInfo::from_index(3));
        pristine.mtoon = Some(mp);

        let mut current = pristine.clone();
        {
            let ti = current
                .mtoon
                .as_mut()
                .unwrap()
                .shade_texture
                .as_mut()
                .unwrap();
            ti.offset = Vec2::new(0.1, 0.2);
            ti.scale = Vec2::new(1.5, 1.5);
        }

        let diff = MaterialParamOverride::diff_from(&pristine, &current).unwrap();
        let uv = diff.shade_uv.as_ref().expect("shade_uv should be Some");
        assert_eq!(uv.offset, Some([0.1, 0.2]));
        assert_eq!(uv.scale, Some([1.5, 1.5]));

        let mut restored = pristine.clone();
        diff.apply_to(&mut restored);
        let ti = restored
            .mtoon
            .as_ref()
            .unwrap()
            .shade_texture
            .as_ref()
            .unwrap();
        assert_eq!(ti.offset, Vec2::new(0.1, 0.2));
        assert_eq!(ti.scale, Vec2::new(1.5, 1.5));
    }

    /// Apply on an unbound slot is a no-op (does not crash and does not silently insert an IrTextureInfo).
    #[test]
    fn test_uv_apply_to_unassigned_slot_is_noop() {
        let pristine = IrMaterial::default();
        assert!(pristine.base_color_tex_info.is_none());
        assert!(pristine.emissive_texture.is_none());

        let mut ov = MaterialParamOverride::default();
        ov.base_color_uv = Some(TextureUvOverride {
            offset: Some([1.0, 1.0]),
            scale: Some([2.0, 2.0]),
            rotation: Some(1.0),
        });
        ov.emissive_uv = Some(TextureUvOverride {
            offset: Some([-0.5, 0.3]),
            ..Default::default()
        });

        let mut restored = pristine.clone();
        ov.apply_to(&mut restored);
        // Slots stay None (no silent from_index(0) insertion).
        assert!(restored.base_color_tex_info.is_none());
        assert!(restored.emissive_texture.is_none());
    }

    /// When enable_mtoon = Some(false), MToon slot UVs are also excluded from the diff.
    #[test]
    fn test_diff_from_mtoon_off_skips_mtoon_slot_uv() {
        let mut pristine = IrMaterial::default();
        pristine.shader_family = ShaderFamily::Mtoon;
        let mut mp = MtoonParams::default();
        mp.shade_texture = Some(IrTextureInfo::from_index(0));
        pristine.mtoon = Some(mp);

        let mut current = pristine.clone();
        current.shader_family = ShaderFamily::Other;
        current.mtoon = None;

        let diff = MaterialParamOverride::diff_from(&pristine, &current).unwrap();
        assert_eq!(diff.enable_mtoon, Some(false));
        assert!(
            diff.shade_uv.is_none(),
            "MToon OFF 時は shade_uv が diff に含まれない"
        );
    }

    /// UV overrides apply without calling mtoon_mut() when MToon is enabled
    /// (a non-MToon material must not silently get mtoon = Some(default) inserted).
    #[test]
    fn test_apply_uv_does_not_upgrade_to_mtoon() {
        let pristine = IrMaterial::default();
        assert!(pristine.mtoon.is_none());

        let mut ov = MaterialParamOverride::default();
        ov.shade_uv = Some(TextureUvOverride {
            offset: Some([0.5, 0.5]),
            ..Default::default()
        });

        let mut restored = pristine.clone();
        ov.apply_to(&mut restored);
        // It is critical that mtoon stays None (do not create mtoon for the sake of UV alone).
        assert!(
            restored.mtoon.is_none(),
            "UV override 単体では mtoon を生成してはならない"
        );
    }

    /// review_result_01 [P1]: a "newly assign + edit UV" diff appears in the diff.
    ///
    /// pristine: slot=None, current: slot=Some(IrTextureInfo{offset=(0.5,0)})
    /// -> Save `{offset: Some([0.5, 0.0])}` as the UV override.
    /// Without this, the "assign new texture -> edit UV" history is lost.
    #[test]
    fn test_diff_from_newly_assigned_slot_uv_is_saved() {
        let pristine = IrMaterial::default();
        assert!(pristine.base_color_tex_info.is_none());

        let mut current = pristine.clone();
        let mut ti = IrTextureInfo::from_index(5);
        ti.offset = Vec2::new(0.5, 0.0);
        ti.rotation = 0.25;
        current.base_color_tex_info = Some(ti);

        let diff =
            MaterialParamOverride::diff_from(&pristine, &current).expect("Some because of UV diff");
        let uv = diff
            .base_color_uv
            .as_ref()
            .expect("UV diff for newly assigned slot should be saved");
        assert_eq!(uv.offset, Some([0.5, 0.0]));
        assert_eq!(uv.rotation, Some(0.25));
        // default scale = 1.0 unchanged, so scale stays None.
        assert_eq!(uv.scale, None);
    }

    /// Reverse direction: pristine = Some, current = None (slot unbinding) yields no UV diff
    /// (slot info disappears on the texture_mgmt side anyway).
    #[test]
    fn test_diff_from_removed_slot_no_uv_diff() {
        let mut pristine = IrMaterial::default();
        let mut ti = IrTextureInfo::from_index(0);
        ti.offset = Vec2::new(0.3, 0.3);
        pristine.base_color_tex_info = Some(ti);

        let mut current = pristine.clone();
        current.base_color_tex_info = None;

        // Without other diffs against base_color_tex_info, UV alone is None.
        let diff = MaterialParamOverride::diff_from(&pristine, &current);
        // If current's slot is None, the UV diff is None.
        if let Some(d) = diff {
            assert!(d.base_color_uv.is_none());
        }
    }

    /// merge_from overwrites UV overrides as a merge.
    #[test]
    fn test_merge_from_uv_override() {
        let mut base = MaterialParamOverride::default();
        base.base_color_uv = Some(TextureUvOverride {
            offset: Some([0.0, 0.0]),
            scale: Some([1.0, 1.0]),
            rotation: None,
        });

        let patch = MaterialParamOverride {
            base_color_uv: Some(TextureUvOverride {
                offset: Some([0.5, 0.0]),
                scale: None,
                rotation: Some(0.5),
            }),
            ..Default::default()
        };
        base.merge_from(&patch);
        // base values are overwritten by patch (replacement at the `Option<TextureUvOverride>` level).
        let uv = base.base_color_uv.as_ref().unwrap();
        assert_eq!(uv.offset, Some([0.5, 0.0]));
        assert_eq!(uv.rotation, Some(0.5));
    }
}
