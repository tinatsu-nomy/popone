use crate::error::Result;
use crate::intermediate::types::{
    AStanceResult, IrBone, IrMaterial, IrMesh, IrModel, IrPhysics, IrVertex, SourceFormat,
};
use glam::{Mat4, Vec2, Vec3, Vec4};
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use super::reader;

/// Read an STL file and convert it to an `IrModel` (defaults: mm, Z-Up).
pub fn load_stl(path: &Path) -> Result<IrModel> {
    load_stl_with_params(path, 0.001, true)
}

/// Read an STL file and convert it to an `IrModel` with custom parameters.
pub fn load_stl_with_params(path: &Path, scale: f32, z_up: bool) -> Result<IrModel> {
    let stl = reader::read_stl(path)?;
    stl_to_ir(&stl, scale, z_up)
}

/// Read STL data from memory and convert it to an `IrModel` (defaults: mm, Z-Up).
pub fn load_stl_from_data(data: &[u8], name: &str) -> Result<IrModel> {
    load_stl_from_data_with_params(data, name, 0.001, true)
}

/// Read STL data from memory and convert it to an `IrModel` with custom parameters.
pub fn load_stl_from_data_with_params(
    data: &[u8],
    name: &str,
    scale: f32,
    z_up: bool,
) -> Result<IrModel> {
    let stl = reader::read_stl_from_data(data, name)?;
    stl_to_ir(&stl, scale, z_up)
}

fn stl_to_ir(stl: &reader::StlModel, scale: f32, z_up: bool) -> Result<IrModel> {
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

    // STL is flat-shaded: each triangle owns its own 3 vertices.
    let mut vertices = Vec::with_capacity(stl.triangles.len() * 3);
    let mut indices = Vec::with_capacity(stl.triangles.len() * 3);

    // Coordinate transform: apply user-specified unit scale and Z-Up conversion.
    let pos_to_gltf: fn(Vec3, f32) -> Vec3 = if z_up {
        |v: Vec3, s: f32| Vec3::new(v.x * s, v.z * s, v.y * s)
    } else {
        |v: Vec3, s: f32| Vec3::new(v.x * s, v.y * s, v.z * s)
    };

    for (i, tri) in stl.triangles.iter().enumerate() {
        let base = (i * 3) as u32;
        // Normal: swap Y<->Z when source is Z-Up
        let raw_normal = if z_up {
            Vec3::new(tri.normal.x, tri.normal.z, tri.normal.y)
        } else {
            Vec3::new(tri.normal.x, tri.normal.y, tri.normal.z)
        };
        let face_normal = if raw_normal.length_squared() < 1e-8 {
            let p0 = pos_to_gltf(tri.vertices[0], scale);
            let p1 = pos_to_gltf(tri.vertices[1], scale);
            let p2 = pos_to_gltf(tri.vertices[2], scale);
            if z_up {
                // Recompute the normal to match the winding after b<->c swap
                (p2 - p0).cross(p1 - p0).normalize_or_zero()
            } else {
                (p1 - p0).cross(p2 - p0).normalize_or_zero()
            }
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
                position: pos_to_gltf(*v, scale),
                normal,
                uv: Vec2::ZERO,
                tangent: Vec4::new(1.0, 0.0, 0.0, 1.0),
                weights: [(0, 1.0), (0, 0.0), (0, 0.0), (0, 0.0)],
                weight_count: 1,
                edge_scale: 1.0,
            });
        }
        if z_up {
            // Y<->Z swap has determinant -1, so flip face winding (b<->c swap).
            indices.push(base);
            indices.push(base + 2);
            indices.push(base + 1);
        } else {
            indices.push(base);
            indices.push(base + 1);
            indices.push(base + 2);
        }
    }

    let mesh = IrMesh {
        name: stl.name.clone(),
        vertices: vertices.into(),
        indices: indices.into(),
        material_index: 0,
        morph_targets: Arc::new(Vec::new()),
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
