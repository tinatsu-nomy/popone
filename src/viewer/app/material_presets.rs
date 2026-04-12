//! 材質編集ドロワー用プリセット (§J / Step 5)
//!
//! 3 種のプリセットを `MaterialParamOverride` として定義する。ドロワーの ComboBox から
//! 選択 → 「適用」で `apply_to(mat)` を呼び、名前・テクスチャインデックス・alpha_mode
//! には触れずにカラー/スカラーパラメータだけを一括変更する。

use glam::{Vec3, Vec4};

use super::material_edit::MaterialParamOverride;
use crate::intermediate::types::OutlineWidthMode;

/// プリセット名の列挙
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

    /// プリセットの `MaterialParamOverride` を返す。`apply_to(mat)` で IR に適用する。
    pub fn to_override(self) -> MaterialParamOverride {
        match self {
            Self::Mtoon10Default => preset_mtoon10_default(),
            Self::LilToonStandard => preset_liltoon_standard(),
            Self::PmxCompat => preset_pmx_compat(),
        }
    }
}

/// MToon 1.0 の仕様書記載デフォルト値。
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
        // alpha_mode / alpha_cutoff / cull_mode は触らない
        ..Default::default()
    }
}

/// lilToon の標準設定を模した値。影は MToon より柔らかめ、リムは控えめ。
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

/// PMX 互換: 非 MToon の典型的なデフォルト値。MToon 有効化を OFF に戻す。
fn preset_pmx_compat() -> MaterialParamOverride {
    MaterialParamOverride {
        enable_mtoon: Some(false),
        diffuse: Some(Vec4::new(1.0, 1.0, 1.0, 1.0)),
        edge_color: Some(Vec4::new(0.0, 0.0, 0.0, 1.0)),
        edge_size: Some(1.0),
        emissive_factor: Some(Vec3::ZERO),
        normal_texture_scale: Some(1.0),
        // MToon 系は触らない（enable_mtoon=false で無効化されるため意味がない）
        ..Default::default()
    }
}
