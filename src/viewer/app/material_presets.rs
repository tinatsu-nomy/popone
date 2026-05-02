//! Presets for the material edit drawer (§J / Step 5).
//!
//! Defines three presets as `MaterialParamOverride`. Selected from the drawer's ComboBox
//! and applied via the "Apply" button calling `apply_to(mat)`, which bulk-updates only
//! color / scalar parameters without touching the name, texture indices, or alpha_mode.

use glam::{Vec3, Vec4};

use super::material_edit::MaterialParamOverride;
use crate::intermediate::types::OutlineWidthMode;

/// Enumeration of preset names.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MaterialPreset {
    Mtoon10Default,
    LilToonStandard,
    PmxCompat,
}

impl MaterialPreset {
    pub const ALL: [Self; 3] = [Self::Mtoon10Default, Self::LilToonStandard, Self::PmxCompat];

    pub fn label(self) -> &'static str {
        match self {
            Self::Mtoon10Default => "MToon 1.0 既定値",
            Self::LilToonStandard => "lilToon 標準",
            Self::PmxCompat => "PMX 互換",
        }
    }

    /// Return the preset as `MaterialParamOverride`. Apply to the IR via `apply_to(mat)`.
    pub fn to_override(self) -> MaterialParamOverride {
        match self {
            Self::Mtoon10Default => preset_mtoon10_default(),
            Self::LilToonStandard => preset_liltoon_standard(),
            Self::PmxCompat => preset_pmx_compat(),
        }
    }
}

/// MToon 1.0 spec-defined default values.
/// <https://github.com/vrm-c/vrm-specification/blob/master/specification/VRMC_materials_mtoon-1.0/README.md>
fn preset_mtoon10_default() -> MaterialParamOverride {
    MaterialParamOverride {
        enable_mtoon: Some(true),
        diffuse: Some(Vec4::new(1.0, 1.0, 1.0, 1.0)),
        shade_color: Some(Vec3::new(0.0, 0.0, 0.0)),
        shading_toony_factor: Some(0.9),
        shading_shift_factor: Some(0.0),
        gi_equalization_factor: Some(0.9),
        edge_color: Some(Vec4::new(0.0, 0.0, 0.0, 1.0)),
        edge_size: Some(0.0),
        outline_width_mode: Some(OutlineWidthMode::None),
        outline_width_factor: Some(0.0),
        outline_lighting_mix: Some(1.0),
        parametric_rim_color: Some(Vec3::ZERO),
        parametric_rim_fresnel_power: Some(5.0),
        parametric_rim_lift: Some(0.0),
        rim_lighting_mix: Some(1.0),
        matcap_factor: Some(Vec3::ONE),
        uv_animation_scroll_x_speed: Some(0.0),
        uv_animation_scroll_y_speed: Some(0.0),
        uv_animation_rotation_speed: Some(0.0),
        emissive_factor: Some(Vec3::ZERO),
        normal_texture_scale: Some(1.0),
        render_queue_offset: Some(0),
        // Do not touch alpha_mode / alpha_cutoff / cull_mode.
        ..Default::default()
    }
}

/// Values that mimic lilToon's standard settings. Softer shading than MToon, subtler rim.
fn preset_liltoon_standard() -> MaterialParamOverride {
    MaterialParamOverride {
        enable_mtoon: Some(true),
        diffuse: Some(Vec4::new(1.0, 1.0, 1.0, 1.0)),
        shade_color: Some(Vec3::new(0.75, 0.75, 0.85)),
        shading_toony_factor: Some(0.5),
        shading_shift_factor: Some(-0.1),
        gi_equalization_factor: Some(0.5),
        edge_color: Some(Vec4::new(0.0, 0.0, 0.0, 1.0)),
        edge_size: Some(0.0),
        outline_width_mode: Some(OutlineWidthMode::None),
        outline_width_factor: Some(0.0),
        outline_lighting_mix: Some(1.0),
        parametric_rim_color: Some(Vec3::new(1.0, 1.0, 1.0)),
        parametric_rim_fresnel_power: Some(3.0),
        parametric_rim_lift: Some(0.0),
        rim_lighting_mix: Some(1.0),
        matcap_factor: Some(Vec3::ONE),
        emissive_factor: Some(Vec3::ZERO),
        normal_texture_scale: Some(1.0),
        render_queue_offset: Some(0),
        ..Default::default()
    }
}

/// PMX compatible: typical defaults for non-MToon materials. Turns MToon back off.
fn preset_pmx_compat() -> MaterialParamOverride {
    MaterialParamOverride {
        enable_mtoon: Some(false),
        diffuse: Some(Vec4::new(1.0, 1.0, 1.0, 1.0)),
        edge_color: Some(Vec4::new(0.0, 0.0, 0.0, 1.0)),
        edge_size: Some(1.0),
        emissive_factor: Some(Vec3::ZERO),
        normal_texture_scale: Some(1.0),
        // Skip MToon-specific fields (enable_mtoon=false disables them anyway).
        ..Default::default()
    }
}
