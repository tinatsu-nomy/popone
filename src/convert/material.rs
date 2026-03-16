use crate::intermediate::types::IrMaterial;
use crate::pmx::types::{PmxMaterial, PmxToonRef};

pub fn ir_material_to_pmx(ir: &IrMaterial, texture_index: Option<i32>) -> PmxMaterial {
    let draw_flags: u8 = {
        let mut f = 0u8;
        if ir.is_double_sided { f |= 0x01; }
        f |= 0x02; // 地面影
        f |= 0x04; // セルフシャドウマップへの描画
        f |= 0x08; // セルフシャドウの描画
        if ir.edge_size > 0.0 { f |= 0x10; } // エッジ描画
        f
    };

    PmxMaterial {
        name: ir.name.clone(),
        name_en: ir.name.clone(),
        diffuse: ir.diffuse,
        specular: ir.specular,
        specular_power: ir.specular_power,
        ambient: ir.ambient,
        draw_flags,
        edge_color: ir.edge_color,
        edge_size: ir.edge_size.min(1.0),
        texture_index,
        sphere_texture_index: None,
        sphere_mode: 0,
        toon_ref: PmxToonRef::Shared(0),
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
            is_double_sided: true,
            is_mtoon: true,
            shade_color: None,
            shade_texture_index: None,
            outline_width_texture_index: None,
            source_texture_name: None,
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
        ir.is_double_sided = false;
        let pmx = ir_material_to_pmx(&ir, None);
        assert_eq!(pmx.draw_flags & 0x01, 0, "片面描画はフラグなし");
    }

    #[test]
    fn test_edge_flag_when_edge_size_positive() {
        let ir = make_test_material();
        let pmx = ir_material_to_pmx(&ir, None);
        assert_ne!(pmx.draw_flags & 0x10, 0, "edge_size > 0 ならエッジフラグが立つ");
    }

    #[test]
    fn test_no_edge_flag_when_edge_size_zero() {
        let mut ir = make_test_material();
        ir.edge_size = 0.0;
        let pmx = ir_material_to_pmx(&ir, None);
        assert_eq!(pmx.draw_flags & 0x10, 0, "edge_size == 0 ならエッジフラグなし");
    }

    #[test]
    fn test_edge_size_clamped_to_max_1() {
        let mut ir = make_test_material();
        ir.edge_size = 5.0;
        let pmx = ir_material_to_pmx(&ir, None);
        assert!((pmx.edge_size - 1.0).abs() < 1e-6, "edge_size は最大 1.0 にクランプ");
    }

    #[test]
    fn test_no_texture_index() {
        let ir = make_test_material();
        let pmx = ir_material_to_pmx(&ir, None);
        assert_eq!(pmx.texture_index, None);
    }
}
