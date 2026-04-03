use crate::error::Result;
use crate::intermediate::types::{
    AStanceResult, IrBone, IrMaterial, IrMesh, IrModel, IrPhysics, IrVertex, SourceFormat,
};
use glam::{Mat4, Vec2, Vec3, Vec4};
use std::collections::HashMap;
use std::path::Path;

use super::reader;

/// STL ファイルを読み込んで IrModel に変換する
pub fn load_stl(path: &Path) -> Result<IrModel> {
    let stl = reader::read_stl(path)?;
    stl_to_ir(&stl)
}

/// STL データをメモリから読み込んで IrModel に変換する
pub fn load_stl_from_data(data: &[u8], name: &str) -> Result<IrModel> {
    let stl = reader::read_stl_from_data(data, name)?;
    stl_to_ir(&stl)
}

fn stl_to_ir(stl: &reader::StlModel) -> Result<IrModel> {
    let root_bone = IrBone {
        name: "全ての親".to_string(),
        name_en: "Root".to_string(),
        original_name: "Root".to_string(),
        vrm_bone_name: None,
        position: Vec3::ZERO,
        global_mat: Mat4::IDENTITY,
        parent: None,
        children: vec![],
        node_index: 0,
        is_physics: false,
        tail_position: None,
        tail_bone_index: None,
        is_ik: false,
        is_ik_bone: false,
        is_translatable: true,
        is_axis_fixed: false,
        is_visible: true,
        grant: None,
    };

    // STL はフラットシェーディング: 各三角形が独立した3頂点を持つ
    let mut vertices = Vec::with_capacity(stl.triangles.len() * 3);
    let mut indices = Vec::with_capacity(stl.triangles.len() * 3);

    // STL の座標を mm 単位・Z-Up と仮定し、glTF 空間（Y-Up、メートル）に正規化。
    // 3Dプリンタ系ツールの慣習: mm → m (÷1000)、Z-Up → Y-Up (Y↔Z入替)
    const MM_TO_M: f32 = 0.001;
    let pos_to_gltf = |v: Vec3| Vec3::new(v.x * MM_TO_M, v.z * MM_TO_M, v.y * MM_TO_M);

    for (i, tri) in stl.triangles.iter().enumerate() {
        let base = (i * 3) as u32;
        // ゼロ法線・不正法線の場合は面法線を再計算
        let raw_normal = Vec3::new(tri.normal.x, tri.normal.z, tri.normal.y);
        let face_normal = if raw_normal.length_squared() < 1e-8 {
            let p0 = pos_to_gltf(tri.vertices[0]);
            let p1 = pos_to_gltf(tri.vertices[1]);
            let p2 = pos_to_gltf(tri.vertices[2]);
            // b↔c swap 後の巻き順に合わせて法線を計算
            (p2 - p0).cross(p1 - p0).normalize_or_zero()
        } else {
            raw_normal.normalize_or_zero()
        };
        let normal = if face_normal == Vec3::ZERO {
            Vec3::Y
        } else {
            face_normal
        };
        for v in &tri.vertices {
            vertices.push(IrVertex {
                position: pos_to_gltf(*v),
                normal,
                uv: Vec2::ZERO,
                tangent: Vec4::new(1.0, 0.0, 0.0, 1.0),
                weights: [(0, 1.0), (0, 0.0), (0, 0.0), (0, 0.0)],
                weight_count: 1,
                edge_scale: 1.0,
            });
        }
        // Y↔Z 入替は行列式 -1 → 面の巻き順を反転 (b↔c swap)
        indices.push(base);
        indices.push(base + 2);
        indices.push(base + 1);
    }

    let mesh = IrMesh {
        name: stl.name.clone(),
        vertices,
        indices,
        material_index: 0,
        morph_targets: vec![],
        node_index: 0,
        uvs1: vec![],
    };

    let material = IrMaterial {
        name: "default".to_string(),
        source_format: SourceFormat::Stl,
        ..Default::default()
    };

    Ok(IrModel {
        name: stl.name.clone(),
        comment: String::new(),
        bones: vec![root_bone],
        meshes: vec![mesh],
        materials: vec![material],
        textures: vec![],
        morphs: vec![],
        physics: IrPhysics::default(),
        node_to_bone: HashMap::new(),
        source_format: SourceFormat::Stl,
        rig_type: None,
        humanoid_bone_count: 0,
        astance_result: AStanceResult::NotRequested,
    })
}
