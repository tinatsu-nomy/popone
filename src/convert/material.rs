use glam::Vec3;

use crate::intermediate::types::IrMaterial;
use crate::pmx::types::{PmxMaterial, PmxToonRef};

/// Rec. 709 に基づく相対輝度
fn luminance(v: Vec3) -> f32 {
    v.x * 0.2126 + v.y * 0.7152 + v.z * 0.0722
}

/// shade/diffuse 輝度比に基づいてトゥーンテクスチャを選択する。
/// 非 MToon は Shared(0) を維持（回帰防止）。
fn select_toon(ir: &IrMaterial) -> PmxToonRef {
    if !ir.is_mtoon {
        return PmxToonRef::Shared(0); // 非MToon: 現行動作を維持
    }
    let Some(shade) = ir.shade_color else {
        return PmxToonRef::Shared(2); // shade_color無し: toon03（中間）
    };
    // shade/diffuse 輝度比でトゥーンの硬さを決定
    let base = ir.diffuse.truncate(); // Vec4 → Vec3 (RGB)
    let ratio = (luminance(shade) / luminance(base).max(0.05)).clamp(0.0, 1.2);
    match () {
        _ if ratio < 0.25 => PmxToonRef::Shared(0), // toon01: 硬い影（shade << diffuse）
        _ if ratio < 0.45 => PmxToonRef::Shared(1), // toon02
        _ if ratio < 0.65 => PmxToonRef::Shared(2), // toon03: 中間
        _ if ratio < 0.85 => PmxToonRef::Shared(4), // toon05: 柔らかめ
        _ => PmxToonRef::Shared(6),                 // toon07: 最も柔らかい（shade ≈ diffuse）
    }
}

pub fn ir_material_to_pmx(ir: &IrMaterial, texture_index: Option<i32>) -> PmxMaterial {
    let draw_flags: u8 = {
        let mut f = 0u8;
        if ir.cull_mode != crate::intermediate::types::CullMode::Back {
            f |= 0x01; // 両面描画（None, Front ともに PMX では両面扱い）
        }
        f |= 0x02; // 地面影
        f |= 0x04; // セルフシャドウマップへの描画
        f |= 0x08; // セルフシャドウの描画
        if ir.edge_size > 0.0 {
            f |= 0x10;
        } // エッジ描画
        f
    };

    // MToon の場合: shade_color → ambient、specular 抑制
    let (ambient, specular, specular_power) = if ir.is_mtoon {
        let amb = if let Some(sc) = ir.shade_color {
            sc * 0.5
        } else {
            Vec3::new(ir.diffuse.x * 0.4, ir.diffuse.y * 0.4, ir.diffuse.z * 0.4)
        };
        (amb, Vec3::ZERO, 0.0)
    } else {
        (ir.ambient, ir.specular, ir.specular_power)
    };

    PmxMaterial {
        name: ir.name.clone(),
        name_en: ir.name.clone(),
        diffuse: ir.diffuse,
        specular,
        specular_power,
        ambient,
        draw_flags,
        edge_color: ir.edge_color,
        edge_size: ir.edge_size.min(1.0),
        texture_index,
        sphere_texture_index: None,
        sphere_mode: 0,
        toon_ref: select_toon(ir),
        memo: String::new(),
        face_count: 0, // build.rsで設定
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::Vec4;

    fn make_test_material() -> IrMaterial {
        IrMaterial {
            name: "テスト材質".to_string(),
            diffuse: Vec4::new(1.0, 0.8, 0.6, 1.0),
            specular: glam::Vec3::new(0.5, 0.5, 0.5),
            specular_power: 10.0,
            ambient: glam::Vec3::new(0.3, 0.3, 0.3),
            edge_color: Vec4::new(0.0, 0.0, 0.0, 1.0),
            edge_size: 0.5,
            texture_index: Some(0),
            cull_mode: crate::intermediate::types::CullMode::None,
            is_mtoon: true,
            shade_color: None,
            shade_texture: None,
            outline_width_texture: None,
            source_texture_name: None,
            ..Default::default()
        }
    }

    #[test]
    fn test_basic_material_conversion() {
        let ir = make_test_material();
        let pmx = ir_material_to_pmx(&ir, Some(0));
        assert_eq!(pmx.name, "テスト材質");
        assert_eq!(pmx.texture_index, Some(0));
        assert_eq!(pmx.face_count, 0); // build.rs で後から設定
    }

    #[test]
    fn test_double_sided_flag() {
        let ir = make_test_material();
        let pmx = ir_material_to_pmx(&ir, None);
        assert_ne!(pmx.draw_flags & 0x01, 0, "両面描画フラグが立つべき");
    }

    #[test]
    fn test_single_sided_no_flag() {
        let mut ir = make_test_material();
        ir.cull_mode = crate::intermediate::types::CullMode::Back;
        let pmx = ir_material_to_pmx(&ir, None);
        assert_eq!(pmx.draw_flags & 0x01, 0, "片面描画はフラグなし");
    }

    #[test]
    fn test_edge_flag_when_edge_size_positive() {
        let ir = make_test_material();
        let pmx = ir_material_to_pmx(&ir, None);
        assert_ne!(
            pmx.draw_flags & 0x10,
            0,
            "edge_size > 0 ならエッジフラグが立つ"
        );
    }

    #[test]
    fn test_no_edge_flag_when_edge_size_zero() {
        let mut ir = make_test_material();
        ir.edge_size = 0.0;
        let pmx = ir_material_to_pmx(&ir, None);
        assert_eq!(
            pmx.draw_flags & 0x10,
            0,
            "edge_size == 0 ならエッジフラグなし"
        );
    }

    #[test]
    fn test_edge_size_clamped_to_max_1() {
        let mut ir = make_test_material();
        ir.edge_size = 5.0;
        let pmx = ir_material_to_pmx(&ir, None);
        assert!(
            (pmx.edge_size - 1.0).abs() < 1e-6,
            "edge_size は最大 1.0 にクランプ"
        );
    }

    #[test]
    fn test_no_texture_index() {
        let ir = make_test_material();
        let pmx = ir_material_to_pmx(&ir, None);
        assert_eq!(pmx.texture_index, None);
    }

    #[test]
    fn test_select_toon_shade_diffuse_ratio() {
        let mut mat = make_test_material();
        mat.is_mtoon = true;
        mat.diffuse = Vec4::new(0.8, 0.8, 0.8, 1.0);

        // shade << diffuse → 硬い影 (Shared(0))
        mat.shade_color = Some(glam::Vec3::new(0.1, 0.1, 0.1));
        assert_eq!(select_toon(&mat), PmxToonRef::Shared(0));

        // shade ≈ diffuse → 柔らかい影 (Shared(6))
        mat.shade_color = Some(glam::Vec3::new(0.75, 0.75, 0.75));
        assert_eq!(select_toon(&mat), PmxToonRef::Shared(6));

        // shade_color 中間 → 中間トゥーン
        mat.shade_color = Some(glam::Vec3::new(0.4, 0.4, 0.4));
        assert_eq!(select_toon(&mat), PmxToonRef::Shared(2));

        // 非MToon → Shared(0)（現行動作維持）
        mat.is_mtoon = false;
        assert_eq!(select_toon(&mat), PmxToonRef::Shared(0));
    }

    #[test]
    fn test_mtoon_specular_suppression() {
        let mut mat = make_test_material();
        mat.is_mtoon = true;
        mat.specular = glam::Vec3::ONE;
        mat.specular_power = 25.0;
        let pmx = ir_material_to_pmx(&mat, None);
        assert_eq!(pmx.specular, glam::Vec3::ZERO);
        assert_eq!(pmx.specular_power, 0.0);
    }

    #[test]
    fn test_non_mtoon_unchanged() {
        let mut mat = make_test_material();
        mat.is_mtoon = false;
        mat.specular = glam::Vec3::new(0.5, 0.5, 0.5);
        mat.specular_power = 25.0;
        mat.ambient = glam::Vec3::new(0.3, 0.3, 0.3);
        let pmx = ir_material_to_pmx(&mat, None);
        // 非 MToon は ambient/specular/toon すべて変更なし
        assert_eq!(pmx.specular, mat.specular);
        assert_eq!(pmx.specular_power, mat.specular_power);
        assert_eq!(pmx.ambient, mat.ambient);
        assert_eq!(pmx.toon_ref, PmxToonRef::Shared(0));
    }

    #[test]
    fn test_mtoon_no_shade_color() {
        let mut mat = make_test_material();
        mat.is_mtoon = true;
        mat.shade_color = None;
        let pmx = ir_material_to_pmx(&mat, None);
        // shade_color無し → toon03（中間）
        assert_eq!(pmx.toon_ref, PmxToonRef::Shared(2));
        // ambient は diffuse ベース
        let expected_amb = glam::Vec3::new(
            mat.diffuse.x * 0.4,
            mat.diffuse.y * 0.4,
            mat.diffuse.z * 0.4,
        );
        assert!((pmx.ambient - expected_amb).length() < 1e-5);
    }
}
