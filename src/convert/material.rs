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
        edge_size: ir.edge_size,
        texture_index,
        sphere_texture_index: None,
        sphere_mode: 0,
        toon_ref: PmxToonRef::Shared(0),
        memo: String::new(),
        face_count: 0, // build.rsで設定
    }
}
