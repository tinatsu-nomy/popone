use std::collections::HashSet;
use std::sync::Arc;

use glam::Vec3;

use crate::intermediate::types::{IrMaterial, IrTexture, ShaderFamily, TextureData};
use crate::pmx::types::{PmxMaterial, PmxToonRef};

/// Return a 256x16 shade-to-diffuse gradient image as a PNG byte buffer.
/// Left edge is `shade_color`, right edge is `diffuse_color`.
fn generate_toon_gradient(shade: Vec3, diffuse: Vec3) -> Vec<u8> {
    const W: u32 = 256;
    const H: u32 = 16;
    let mut pixels = Vec::with_capacity((W * H * 4) as usize);
    for _ in 0..H {
        for x in 0..W {
            let t = x as f32 / (W - 1) as f32;
            let r = (shade.x + (diffuse.x - shade.x) * t).clamp(0.0, 1.0);
            let g = (shade.y + (diffuse.y - shade.y) * t).clamp(0.0, 1.0);
            let b = (shade.z + (diffuse.z - shade.z) * t).clamp(0.0, 1.0);
            pixels.push((r * 255.0 + 0.5) as u8);
            pixels.push((g * 255.0 + 0.5) as u8);
            pixels.push((b * 255.0 + 0.5) as u8);
            pixels.push(255u8);
        }
    }
    // PNG encode
    let mut png_buf = Vec::new();
    {
        let encoder = image::codecs::png::PngEncoder::new(&mut png_buf);
        image::ImageEncoder::write_image(encoder, &pixels, W, H, image::ExtendedColorType::Rgba8)
            .expect("PNG encode should not fail for valid RGBA data");
    }
    png_buf
}

/// Generate a toon texture for an MToon material.
/// Pushes the generated texture into `toon_textures` and returns `PmxToonRef::Texture(index)`.
/// `base_tex_count` is the number of pre-existing textures (used for offset computation).
/// `used_names` contains existing texture names plus already-generated toon names (collision avoidance).
/// Non-MToon materials and materials without `shade_color` return `PmxToonRef::Shared` instead.
///
/// Section G (Step 2-9): The decision axis is now `shader_family`. `ir.is_mtoon()`
/// (= `mtoon.is_some()`) becomes true as a side effect when the material editor drawer
/// touches `mat.mtoon_mut()` (e.g. simply expanding the Shade section), so it is unstable
/// as a branch criterion for PMX conversion. By keying on `shader_family` instead, PMX
/// export behavior stays the same until the user explicitly toggles "Enable MToon".
pub fn generate_toon(
    ir: &IrMaterial,
    toon_textures: &mut Vec<IrTexture>,
    base_tex_count: usize,
    used_names: &mut HashSet<String>,
) -> PmxToonRef {
    let is_mtoon_like = matches!(
        ir.shader_family,
        ShaderFamily::Mtoon | ShaderFamily::Uts2 | ShaderFamily::LilToon | ShaderFamily::Poiyomi
    );
    if !is_mtoon_like {
        return PmxToonRef::Shared(0);
    }
    let Some(shade) = ir.mtoon().shade_color else {
        return PmxToonRef::Shared(2);
    };
    let diffuse = ir.diffuse.truncate();

    // Generate the gradient PNG
    let png_data = generate_toon_gradient(shade, diffuse);

    // Filename: toon_{material_name}_{serial}.png (the serial keeps it ASCII-safe)
    // Avoid collision with existing texture names
    let idx = toon_textures.len();
    let safe_name = ir
        .name
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '-')
        .take(32)
        .collect::<String>();
    let base_name = if safe_name.is_empty() {
        format!("toon_{idx:03}")
    } else {
        format!("toon_{safe_name}_{idx:03}")
    };
    let mut filename = format!("{base_name}.png");
    let mut suffix = 1u32;
    while used_names.contains(&filename) {
        filename = format!("{base_name}_{suffix}.png");
        suffix += 1;
    }
    used_names.insert(filename.clone());

    let tex = IrTexture {
        filename: filename.clone(),
        data: TextureData::Encoded(Arc::from(png_data)),
        mime_type: "image/png".to_string(),
        source_path: format!("generated(toon: {})", ir.name),
        mip_chain: None,
    };
    toon_textures.push(tex);

    let texture_index = (base_tex_count + idx) as i32;
    log::info!(
        "Toon texture generated: {} (index={}, shade=({:.2},{:.2},{:.2}), diffuse=({:.2},{:.2},{:.2}))",
        filename, texture_index,
        shade.x, shade.y, shade.z,
        diffuse.x, diffuse.y, diffuse.z
    );
    PmxToonRef::Texture(texture_index)
}

pub fn ir_material_to_pmx(
    ir: &IrMaterial,
    texture_index: Option<i32>,
    toon_textures: &mut Vec<IrTexture>,
    base_tex_count: usize,
    used_names: &mut HashSet<String>,
) -> PmxMaterial {
    let draw_flags: u8 = {
        let mut f = 0u8;
        if ir.cull_mode != crate::intermediate::types::CullMode::Back {
            f |= 0x01; // Double-sided (PMX treats both None and Front as double-sided)
        }
        f |= 0x02; // Ground shadow
        f |= 0x04; // Render to self-shadow map
        f |= 0x08; // Receive self-shadow
        if ir.edge_size > 0.0 {
            f |= 0x10;
        } // Edge drawing
        f
    };

    // For toon shaders: adjust ambient / specular.
    //
    // Section G (Step 2-9): The decision axis is now `shader_family`. Relying on
    // `ir.is_mtoon()` alone would incorrectly classify non-MToon materials as MToon
    // when the material editor drawer triggers `mat.mtoon_mut()` as a side effect, so
    // `shader_family` is checked explicitly.
    let is_mtoon_like = matches!(
        ir.shader_family,
        ShaderFamily::Mtoon | ShaderFamily::Uts2 | ShaderFamily::LilToon | ShaderFamily::Poiyomi
    );
    let (ambient, specular, specular_power) = if is_mtoon_like {
        match ir.shader_family {
            ShaderFamily::Uts2 | ShaderFamily::LilToon | ShaderFamily::Poiyomi => {
                // UTS2 / lilToon / Poiyomi: ambient and specular are already set during extraction
                (ir.ambient, ir.specular, ir.specular_power)
            }
            _ => {
                // MToon: derive ambient from shade_color and add a light specular highlight for lighting response
                let amb = if let Some(sc) = ir.mtoon().shade_color {
                    sc * 0.5
                } else {
                    Vec3::new(ir.diffuse.x * 0.4, ir.diffuse.y * 0.4, ir.diffuse.z * 0.4)
                };
                let diff_rgb = ir.diffuse.truncate();
                (amb, diff_rgb * 0.2, 10.0)
            }
        }
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
        toon_ref: generate_toon(ir, toon_textures, base_tex_count, used_names),
        memo: String::new(),
        face_count: 0, // Set later in build.rs
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::Vec4;

    fn make_test_material() -> IrMaterial {
        // Step 2-9 (Section G): explicitly set `shader_family = Mtoon`.
        // Previously `mtoon: Some(_)` + `shader_family: Other` (default) was enough because the PMX
        // converter branched via `is_mtoon()`. Now that the decision axis is `shader_family`, test
        // materials that should be treated as MToon must declare `shader_family` explicitly.
        IrMaterial {
            name: "test_mat".to_string(),
            diffuse: Vec4::new(1.0, 0.8, 0.6, 1.0),
            specular: glam::Vec3::new(0.5, 0.5, 0.5),
            specular_power: 10.0,
            ambient: glam::Vec3::new(0.3, 0.3, 0.3),
            edge_color: Vec4::new(0.0, 0.0, 0.0, 1.0),
            edge_size: 0.5,
            texture_index: Some(0),
            cull_mode: crate::intermediate::types::CullMode::None,
            mtoon: Some(crate::intermediate::types::MtoonParams::default()),
            shader_family: ShaderFamily::Mtoon,
            source_texture_name: None,
            ..Default::default()
        }
    }

    /// Test helper: call `ir_material_to_pmx` with default test arguments.
    fn to_pmx(ir: &IrMaterial, tex_idx: Option<i32>) -> PmxMaterial {
        let mut toon_textures = Vec::new();
        let mut used_names = HashSet::new();
        ir_material_to_pmx(ir, tex_idx, &mut toon_textures, 0, &mut used_names)
    }

    #[test]
    fn test_basic_material_conversion() {
        let ir = make_test_material();
        let pmx = to_pmx(&ir, Some(0));
        assert_eq!(pmx.name, "test_mat");
        assert_eq!(pmx.texture_index, Some(0));
        assert_eq!(pmx.face_count, 0);
    }

    #[test]
    fn test_double_sided_flag() {
        let ir = make_test_material();
        let pmx = to_pmx(&ir, None);
        assert_ne!(pmx.draw_flags & 0x01, 0, "両面描画フラグが立つべき");
    }

    #[test]
    fn test_single_sided_no_flag() {
        let mut ir = make_test_material();
        ir.cull_mode = crate::intermediate::types::CullMode::Back;
        let pmx = to_pmx(&ir, None);
        assert_eq!(pmx.draw_flags & 0x01, 0, "片面描画はフラグなし");
    }

    #[test]
    fn test_edge_flag_when_edge_size_positive() {
        let ir = make_test_material();
        let pmx = to_pmx(&ir, None);
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
        let pmx = to_pmx(&ir, None);
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
        let pmx = to_pmx(&ir, None);
        assert!(
            (pmx.edge_size - 1.0).abs() < 1e-6,
            "edge_size は最大 1.0 にクランプ"
        );
    }

    #[test]
    fn test_no_texture_index() {
        let ir = make_test_material();
        let pmx = to_pmx(&ir, None);
        assert_eq!(pmx.texture_index, None);
    }

    #[test]
    fn test_generate_toon_mtoon_produces_texture() {
        let mut mat = make_test_material();
        mat.diffuse = Vec4::new(0.8, 0.8, 0.8, 1.0);
        mat.mtoon_mut().shade_color = Some(glam::Vec3::new(0.2, 0.2, 0.2));

        let mut toon_textures = Vec::new();
        let mut used_names = HashSet::new();
        let toon_ref = generate_toon(&mat, &mut toon_textures, 10, &mut used_names);

        // MToon + shade_color present -> Texture reference
        assert!(matches!(toon_ref, PmxToonRef::Texture(10)));
        assert_eq!(toon_textures.len(), 1);
        assert!(toon_textures[0].filename.starts_with("toon_"));
        assert!(toon_textures[0].filename.ends_with(".png"));
        // PNG data must be generated
        assert!(!toon_textures[0].data.is_empty());
    }

    #[test]
    fn test_generate_toon_non_mtoon_shared() {
        let mut mat = make_test_material();
        mat.mtoon = None;
        // Step 2-9: `make_test_material()` returns shader_family = Mtoon, so when the test wants to
        // assert "non-MToon material -> Shared(0)" we must reset it to Other explicitly.
        mat.shader_family = ShaderFamily::Other;

        let mut toon_textures = Vec::new();
        let mut used_names = HashSet::new();
        let toon_ref = generate_toon(&mat, &mut toon_textures, 5, &mut used_names);

        assert_eq!(toon_ref, PmxToonRef::Shared(0));
        assert!(toon_textures.is_empty());
    }

    #[test]
    fn test_generate_toon_no_shade_color_shared() {
        let mut mat = make_test_material();
        mat.mtoon_mut().shade_color = None;

        let mut toon_textures = Vec::new();
        let mut used_names = HashSet::new();
        let toon_ref = generate_toon(&mat, &mut toon_textures, 5, &mut used_names);

        assert_eq!(toon_ref, PmxToonRef::Shared(2));
        assert!(toon_textures.is_empty());
    }

    #[test]
    fn test_generate_toon_multiple_materials() {
        let mut mat1 = make_test_material();
        mat1.name = "mat_a".to_string();
        mat1.diffuse = Vec4::new(0.9, 0.9, 0.9, 1.0);
        mat1.mtoon_mut().shade_color = Some(glam::Vec3::new(0.3, 0.1, 0.1));

        let mut mat2 = make_test_material();
        mat2.name = "mat_b".to_string();
        mat2.diffuse = Vec4::new(0.5, 0.5, 0.8, 1.0);
        mat2.mtoon_mut().shade_color = Some(glam::Vec3::new(0.1, 0.1, 0.4));

        let mut toon_textures = Vec::new();
        let mut used_names = HashSet::new();
        let ref1 = generate_toon(&mat1, &mut toon_textures, 10, &mut used_names);
        let ref2 = generate_toon(&mat2, &mut toon_textures, 10, &mut used_names);

        assert_eq!(ref1, PmxToonRef::Texture(10));
        assert_eq!(ref2, PmxToonRef::Texture(11));
        assert_eq!(toon_textures.len(), 2);
    }

    #[test]
    fn test_generate_toon_name_collision_avoidance() {
        let mut mat = make_test_material();
        mat.diffuse = Vec4::new(0.8, 0.8, 0.8, 1.0);
        mat.mtoon_mut().shade_color = Some(glam::Vec3::new(0.2, 0.2, 0.2));

        let mut toon_textures = Vec::new();
        // When an existing texture has the same name, avoid the collision with a suffix
        let mut used_names: HashSet<String> =
            ["toon_test_mat_000.png".to_string()].into_iter().collect();
        let toon_ref = generate_toon(&mat, &mut toon_textures, 5, &mut used_names);

        assert!(matches!(toon_ref, PmxToonRef::Texture(5)));
        assert_eq!(toon_textures.len(), 1);
        // Confirm the collision avoidance appended a `_1` suffix
        assert_eq!(toon_textures[0].filename, "toon_test_mat_000_1.png");
    }

    #[test]
    fn test_generate_toon_gradient_png_valid() {
        let shade = Vec3::new(0.2, 0.1, 0.3);
        let diffuse = Vec3::new(0.9, 0.8, 0.7);
        let png_data = generate_toon_gradient(shade, diffuse);

        // Check the PNG header
        assert!(png_data.len() > 8);
        assert_eq!(&png_data[0..4], &[0x89, b'P', b'N', b'G']);

        // Confirm the data decodes correctly
        let img = image::load_from_memory(&png_data).expect("PNG decode");
        assert_eq!(img.width(), 256);
        assert_eq!(img.height(), 16);
    }

    #[test]
    fn test_mtoon_specular_light_reactive() {
        let mut mat = make_test_material();
        mat.specular = glam::Vec3::ONE;
        mat.specular_power = 25.0;
        let pmx = to_pmx(&mat, None);
        let expected = mat.diffuse.truncate() * 0.2;
        assert!((pmx.specular - expected).length() < 1e-5);
        assert!((pmx.specular_power - 10.0).abs() < 1e-5);
    }

    #[test]
    fn test_non_mtoon_unchanged() {
        let mut mat = make_test_material();
        mat.mtoon = None;
        // Step 2-9: `make_test_material()` returns shader_family = Mtoon, so when a test exercises
        // non-MToon paths we must reset it to Other explicitly.
        mat.shader_family = ShaderFamily::Other;
        mat.specular = glam::Vec3::new(0.5, 0.5, 0.5);
        mat.specular_power = 25.0;
        mat.ambient = glam::Vec3::new(0.3, 0.3, 0.3);
        let pmx = to_pmx(&mat, None);
        assert_eq!(pmx.specular, mat.specular);
        assert_eq!(pmx.specular_power, mat.specular_power);
        assert_eq!(pmx.ambient, mat.ambient);
        assert_eq!(pmx.toon_ref, PmxToonRef::Shared(0));
    }

    /// Edge case for review_005 [P1]: a material with `mtoon = Some(_)` but `shader_family`
    /// other than `Mtoon` must take the non-MToon path through PMX conversion (Section G axis).
    ///
    /// Mimics the case where the material editor drawer "merely expanded the Shade section"
    /// and `mtoon_mut()` injected `MtoonParams::default()` as a side effect. When `shader_family`
    /// stays `Other`, the PMX export must continue to use the non-MToon ambient / specular / toon
    /// adjustments until the user explicitly toggles "Enable MToon".
    #[test]
    fn test_mtoon_some_but_shader_family_other_behaves_non_mtoon() {
        let mut mat = make_test_material();
        // Keep mtoon = Some(default) but lower shader_family to Other
        mat.shader_family = ShaderFamily::Other;
        mat.specular = glam::Vec3::new(0.5, 0.5, 0.5);
        mat.specular_power = 25.0;
        mat.ambient = glam::Vec3::new(0.3, 0.3, 0.3);
        assert!(
            mat.mtoon.is_some(),
            "この境界ケースでは mtoon は Some のまま"
        );
        let pmx = to_pmx(&mat, None);
        // shader_family-based dispatch routes this through the non-MToon code path
        assert_eq!(pmx.specular, mat.specular, "specular が変更されないこと");
        assert_eq!(
            pmx.specular_power, mat.specular_power,
            "specular_power が変更されないこと"
        );
        assert_eq!(pmx.ambient, mat.ambient, "ambient が変更されないこと");
        assert_eq!(
            pmx.toon_ref,
            PmxToonRef::Shared(0),
            "toon も非 MToon と同じ Shared(0) 経路"
        );
    }

    #[test]
    fn test_mtoon_no_shade_color() {
        let mut mat = make_test_material();
        mat.mtoon_mut().shade_color = None;
        let pmx = to_pmx(&mat, None);
        assert_eq!(pmx.toon_ref, PmxToonRef::Shared(2));
        let expected_amb = glam::Vec3::new(
            mat.diffuse.x * 0.4,
            mat.diffuse.y * 0.4,
            mat.diffuse.z * 0.4,
        );
        assert!((pmx.ambient - expected_amb).length() < 1e-5);
    }

    #[test]
    fn test_uts2_specular_preserved() {
        let mut mat = make_test_material();
        mat.shader_family = crate::intermediate::types::ShaderFamily::Uts2;
        mat.specular = glam::Vec3::new(0.8, 0.6, 0.4);
        mat.specular_power = 15.0;
        mat.ambient = glam::Vec3::new(0.2, 0.15, 0.1);
        let pmx = to_pmx(&mat, None);
        assert_eq!(pmx.specular, mat.specular);
        assert_eq!(pmx.specular_power, mat.specular_power);
        assert_eq!(pmx.ambient, mat.ambient);
    }

    #[test]
    fn test_uts2_toon_generates_texture() {
        let mut mat = make_test_material();
        mat.shader_family = crate::intermediate::types::ShaderFamily::Uts2;
        mat.diffuse = Vec4::new(0.8, 0.8, 0.8, 1.0);
        mat.mtoon_mut().shade_color = Some(glam::Vec3::new(0.1, 0.1, 0.1));

        let mut toon_textures = Vec::new();
        let mut used_names = HashSet::new();
        let toon_ref = generate_toon(&mat, &mut toon_textures, 5, &mut used_names);
        assert!(matches!(toon_ref, PmxToonRef::Texture(5)));
        assert_eq!(toon_textures.len(), 1);
    }

    #[test]
    fn test_mtoon_specular_light_reactive_explicit_family() {
        let mut mat = make_test_material();
        mat.shader_family = crate::intermediate::types::ShaderFamily::Mtoon;
        mat.specular = glam::Vec3::ONE;
        mat.specular_power = 25.0;
        let pmx = to_pmx(&mat, None);
        let expected = mat.diffuse.truncate() * 0.2;
        assert!((pmx.specular - expected).length() < 1e-5);
        assert!((pmx.specular_power - 10.0).abs() < 1e-5);
    }

    #[test]
    fn test_liltoon_specular_preserved() {
        let mut mat = make_test_material();
        mat.shader_family = crate::intermediate::types::ShaderFamily::LilToon;
        mat.specular = glam::Vec3::new(0.6, 0.5, 0.4);
        mat.specular_power = 10.0;
        mat.ambient = glam::Vec3::new(0.25, 0.2, 0.15);
        let pmx = to_pmx(&mat, None);
        assert_eq!(pmx.specular, mat.specular);
        assert_eq!(pmx.specular_power, mat.specular_power);
        assert_eq!(pmx.ambient, mat.ambient);
    }

    #[test]
    fn test_liltoon_toon_generates_texture() {
        let mut mat = make_test_material();
        mat.shader_family = crate::intermediate::types::ShaderFamily::LilToon;
        mat.diffuse = Vec4::new(0.9, 0.8, 0.7, 1.0);
        mat.mtoon_mut().shade_color = Some(glam::Vec3::new(0.3, 0.2, 0.1));

        let mut toon_textures = Vec::new();
        let mut used_names = HashSet::new();
        let toon_ref = generate_toon(&mat, &mut toon_textures, 8, &mut used_names);
        assert!(matches!(toon_ref, PmxToonRef::Texture(8)));
        assert_eq!(toon_textures.len(), 1);
    }

    #[test]
    fn test_poiyomi_specular_preserved() {
        let mut mat = make_test_material();
        mat.shader_family = crate::intermediate::types::ShaderFamily::Poiyomi;
        mat.specular = glam::Vec3::new(0.5, 0.4, 0.3);
        mat.specular_power = 10.0;
        mat.ambient = glam::Vec3::new(0.2, 0.15, 0.1);
        let pmx = to_pmx(&mat, None);
        assert_eq!(pmx.specular, mat.specular);
        assert_eq!(pmx.specular_power, mat.specular_power);
        assert_eq!(pmx.ambient, mat.ambient);
    }

    #[test]
    fn test_poiyomi_toon_generates_texture() {
        let mut mat = make_test_material();
        mat.shader_family = crate::intermediate::types::ShaderFamily::Poiyomi;
        mat.diffuse = Vec4::new(0.7, 0.7, 0.9, 1.0);
        mat.mtoon_mut().shade_color = Some(glam::Vec3::new(0.2, 0.2, 0.4));

        let mut toon_textures = Vec::new();
        let mut used_names = HashSet::new();
        let toon_ref = generate_toon(&mat, &mut toon_textures, 3, &mut used_names);
        assert!(matches!(toon_ref, PmxToonRef::Texture(3)));
        assert_eq!(toon_textures.len(), 1);
    }
}
