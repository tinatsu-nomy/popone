use crate::error::Result;
use glam::{Mat4, Vec3, Vec4};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::convert::coord::{
    pmx_normal_to_gltf as pmx_normal_to_gltf_full, pmx_pos_to_gltf as pmx_pos_to_gltf_full,
};
use crate::intermediate::types::*;
use crate::pmx::types::*;

/// PMX coords -> glTF coords (inverse of the VRM 1.0 transform; PMX is always is_vrm0=false).
#[inline]
fn pmx_pos_to_gltf(v: Vec3) -> Vec3 {
    pmx_pos_to_gltf_full(v, false)
}

/// PMX normal -> glTF normal (PMX is always is_vrm0=false).
#[inline]
fn pmx_normal_to_gltf(n: Vec3) -> Vec3 {
    pmx_normal_to_gltf_full(n, false)
}

/// Build an `IrModel` from a PMX model.
pub fn pmx_to_ir(pmx: &PmxModel, pmx_dir: &Path) -> Result<IrModel> {
    pmx_to_ir_with_aux(pmx, pmx_dir, None)
}

/// Convert PMX -> `IrModel` with in-memory auxiliary files.
pub fn pmx_to_ir_with_aux(
    pmx: &PmxModel,
    pmx_dir: &Path,
    aux_files: Option<&HashMap<PathBuf, Arc<[u8]>>>,
) -> Result<IrModel> {
    let bones = extract_bones(pmx);
    let textures = extract_textures(pmx, pmx_dir, aux_files);
    let materials = extract_materials(pmx);
    let (meshes, pmx_to_ir_vertex) = extract_meshes(pmx);
    // Pass `pmx_to_ir_vertex` so UV morphs can be ingested (Phase 3 A-2).
    // The mapping is 1:1, so when a PMX vertex was split across several IR vertices only the
    // last mapping survives. UV morphs typically stay within a single material in real models,
    // which keeps the impact small in practice.
    let morphs = extract_morphs(pmx, &meshes, &pmx_to_ir_vertex);
    let physics = extract_physics(pmx);

    Ok(IrModel {
        name: pmx.model_info.name.clone(),
        comment: pmx.model_info.comment.clone(),
        bones,
        meshes,
        materials,
        textures,
        morphs,
        physics,
        node_to_bone: HashMap::new(),
        source_format: SourceFormat::Pmx,
        rig_type: None,
        humanoid_bone_count: 0,
        astance_result: AStanceResult::NotRequested,
    })
}

/// Bone extraction: `PmxBone` -> `IrBone`.
fn extract_bones(pmx: &PmxModel) -> Vec<IrBone> {
    let mut bones: Vec<IrBone> = pmx
        .bones
        .iter()
        .enumerate()
        .map(|(i, b)| {
            let parent = if b.parent_index >= 0 {
                Some(b.parent_index as usize)
            } else {
                None
            };

            let vrm_bone_name =
                crate::convert::bone_map::pmx_name_to_vrm_bone(&b.name).map(|s| s.to_string());

            // Compute the tail position and bone index (glTF coords)
            let (tail_position, tail_bone_index) = match &b.tail {
                BoneTail::BoneIndex(idx) => {
                    let ci = *idx as usize;
                    if ci < pmx.bones.len() {
                        (Some(pmx_pos_to_gltf(pmx.bones[ci].position)), Some(ci))
                    } else {
                        (None, None)
                    }
                }
                BoneTail::Offset(off) => {
                    if off.length_squared() > 0.0001 {
                        (Some(pmx_pos_to_gltf(b.position + *off)), None)
                    } else {
                        (None, None)
                    }
                }
            };

            IrBone {
                name: b.name.clone(),
                name_en: b.name_en.clone(),
                original_name: b.name.clone(),
                vrm_bone_name,
                position: pmx_pos_to_gltf(b.position),
                global_mat: Mat4::IDENTITY,
                parent,
                children: Vec::new(),
                node_index: i,
                is_physics: b.flags & BONE_FLAG_PHYS_AFTER != 0,
                tail_position,
                tail_bone_index,
                is_ik: false, // IK target/link is filled in later
                is_ik_bone: b.flags & BONE_FLAG_IK != 0,
                is_translatable: b.flags & BONE_FLAG_TRANSLATABLE != 0,
                is_axis_fixed: b.flags & BONE_FLAG_AXIS_FIXED != 0,
                is_visible: b.flags & BONE_FLAG_VISIBLE != 0,
                grant: b.grant.as_ref().and_then(|g| {
                    if g.parent_index >= 0 && (g.parent_index as usize) < pmx.bones.len() {
                        Some(IrGrant {
                            parent_index: g.parent_index as usize,
                            ratio: g.ratio,
                            is_rotation: b.flags & BONE_FLAG_ROTATION_GRANT != 0,
                            is_move: b.flags & BONE_FLAG_MOVE_GRANT != 0,
                            is_local: b.flags & BONE_FLAG_LOCAL_GRANT != 0,
                        })
                    } else {
                        None
                    }
                }),
            }
        })
        .collect();

    // Build the children lists
    let parents: Vec<Option<usize>> = bones.iter().map(|b| b.parent).collect();
    for (i, parent) in parents.iter().enumerate() {
        if let Some(p) = parent {
            if *p < bones.len() {
                bones[*p].children.push(i);
            }
        }
    }

    // Compute global matrices (root-first)
    for i in 0..bones.len() {
        let pos = bones[i].position;
        let local = Mat4::from_translation(pos);
        if let Some(parent_idx) = bones[i].parent {
            if parent_idx < i {
                // Derive the offset from the parent's global matrix
                let parent_pos = bones[parent_idx].position;
                let offset = pos - parent_pos;
                let parent_mat = bones[parent_idx].global_mat;
                bones[i].global_mat = parent_mat * Mat4::from_translation(offset);
            } else {
                bones[i].global_mat = local;
            }
        } else {
            bones[i].global_mat = local;
        }
    }

    // Mark bones inside an IK chain (links only) -- targets are drawn blue
    for b in &pmx.bones {
        if let Some(ref ik) = b.ik {
            for link in &ik.links {
                let li = link.bone_index as usize;
                if li < bones.len() {
                    bones[li].is_ik = true;
                }
            }
        }
    }

    bones
}

/// Case-insensitive `aux_files` lookup.
///
/// Windows/macOS filesystems are case-insensitive, so a PMX authored on such a system
/// can reference `foo.PNG` while the archive entry is `foo.png`. The exact `HashMap`
/// lookup is case-sensitive and misses these, causing a spurious "format error" (the
/// texture ends up empty). `normalized` uses forward slashes; aux keys may render with
/// `\` on Windows, so both sides are slash-normalized before comparing.
fn aux_get_ignore_case<'a>(
    aux: &'a HashMap<PathBuf, Arc<[u8]>>,
    normalized: &str,
) -> Option<&'a Arc<[u8]>> {
    let target = normalized.to_lowercase();
    aux.iter()
        .find(|(k, _)| k.to_string_lossy().replace('\\', "/").to_lowercase() == target)
        .map(|(_, v)| v)
}

/// Texture extraction: load from disk or `aux_files`.
fn extract_textures(
    pmx: &PmxModel,
    pmx_dir: &Path,
    aux_files: Option<&HashMap<PathBuf, Arc<[u8]>>>,
) -> Vec<IrTexture> {
    pmx.textures
        .iter()
        .map(|tex_path| {
            // Normalize path separators and sanitize to prevent path traversal
            let normalized = tex_path.replace('\\', "/");
            let sanitized = crate::sanitize_rel_path(&normalized);
            let full_path = pmx_dir.join(&sanitized);
            let filename = Path::new(&normalized)
                .file_name()
                .map(|f| f.to_string_lossy().into_owned())
                .unwrap_or_else(|| normalized.clone());

            let ext = crate::path_ext_lower(Path::new(&normalized));
            let mime = match ext.as_str() {
                "png" => "image/png",
                "jpg" | "jpeg" => "image/jpeg",
                "bmp" => "image/bmp",
                "tga" => "image/tga",
                _ => "application/octet-stream",
            };

            // Read from aux_files when available, otherwise fall back to the filesystem
            let data = if let Some(aux) = aux_files {
                let key = PathBuf::from(&normalized);
                if let Some(cached) = aux.get(&key) {
                    cached.to_vec()
                } else if let Some(cached) = aux_get_ignore_case(aux, &normalized) {
                    // Case-insensitive fallback: a PMX built on a case-insensitive
                    // filesystem (Windows/macOS) may reference `foo.PNG` while the archive
                    // stores `foo.png`. Disk loads resolve this via the OS, but the
                    // case-sensitive `aux.get` above misses it, so match here explicitly.
                    cached.to_vec()
                } else {
                    log::warn!("Texture not found in aux_files: {:?}", key);
                    Vec::new()
                }
            } else if full_path.exists() {
                std::fs::read(&full_path).unwrap_or_default()
            } else {
                log::warn!("Texture file not found: {:?}", full_path);
                Vec::new()
            };

            IrTexture {
                filename: filename.clone(),
                data: TextureData::Encoded(Arc::from(data)),
                mime_type: mime.to_string(),
                source_path: normalized.clone(),
                mip_chain: None,
            }
        })
        .collect()
}

/// Material extraction: `PmxMaterial` -> `IrMaterial`.
fn extract_materials(pmx: &PmxModel) -> Vec<IrMaterial> {
    pmx.materials
        .iter()
        .map(|m| {
            let texture_index = m.texture_index.map(|i| i as usize);
            let cull_mode = if m.draw_flags & 0x01 != 0 {
                CullMode::None
            } else {
                CullMode::Back
            };
            let has_edge = m.draw_flags & 0x10 != 0;

            // Sphere mode 3 (sub-texture) is not supported
            let sphere_mode = if m.sphere_mode == 3 {
                log::warn!(
                    "Material '{}': sphere_mode=3 (sub-texture) not supported, disabling",
                    m.name
                );
                0
            } else {
                m.sphere_mode
            };
            let sphere_texture_index = if sphere_mode > 0 {
                m.sphere_texture_index.map(|i| i as usize)
            } else {
                None
            };

            // Toon reference (-1 means unset, i.e. no toon)
            let (toon_texture_index, toon_shared_index) = match &m.toon_ref {
                PmxToonRef::Texture(i) if *i >= 0 => (Some(*i as usize), None),
                PmxToonRef::Texture(_) => (None, None),
                PmxToonRef::Shared(i) => (None, Some(*i)),
            };

            IrMaterial {
                name: m.name.clone(),
                diffuse: m.diffuse,
                specular: m.specular,
                specular_power: m.specular_power,
                ambient: m.ambient,
                texture_index,
                base_color_tex_info: None,
                cull_mode,
                edge_color: if has_edge { m.edge_color } else { Vec4::ZERO },
                edge_size: if has_edge { m.edge_size } else { 0.0 },
                mtoon: None,
                shader_family: ShaderFamily::Other,
                source_texture_name: None,
                source_format: SourceFormat::Pmx,
                sphere_texture_index,
                sphere_mode,
                toon_texture_index,
                toon_shared_index,
                alpha_mode: AlphaMode::Opaque,
                alpha_cutoff: 0.5,
                emissive_factor: Vec3::ZERO,
                emissive_texture: None,
                normal_texture: None,
                normal_texture_scale: 1.0,
                source_material: None,
            }
        })
        .collect()
}

/// Mesh extraction: split by each material's `face_count`.
/// Returns (meshes, mapping from PMX global vertex index to IR global vertex index).
fn extract_meshes(pmx: &PmxModel) -> (Vec<IrMesh>, HashMap<u32, usize>) {
    let mut meshes = Vec::new();
    let mut face_offset = 0usize;
    // PMX global vertex index -> IrModel running index (mesh0 vert0=0, vert1=1, ..., mesh1 vert0=N, ...)
    let mut pmx_to_ir_vertex: HashMap<u32, usize> = HashMap::new();
    let mut ir_vertex_offset = 0usize;

    for (mat_idx, mat) in pmx.materials.iter().enumerate() {
        let face_count = (mat.face_count / 3) as usize;
        if face_count == 0 {
            meshes.push(IrMesh {
                name: mat.name.clone(),
                vertices: Arc::new(Vec::new()),
                indices: Arc::new(Vec::new()),
                material_index: mat_idx,
                morph_targets: Arc::new(Vec::new()),
                node_index: 0,
                uvs1: Vec::new(),
            });
            face_offset += face_count;
            continue;
        }

        // Collect vertex indices from faces referenced by this material
        let mut vertex_map: HashMap<u32, u32> = HashMap::new();
        let mut local_vertices = Vec::new();
        let mut local_indices = Vec::new();

        for fi in face_offset..face_offset + face_count {
            let face = &pmx.faces[fi];
            // Flip the face winding (PMX -> glTF: swap b and c)
            let reordered = [face[0], face[2], face[1]];
            for &global_idx in &reordered {
                let local_idx = if let Some(&existing) = vertex_map.get(&global_idx) {
                    existing
                } else {
                    let new_idx = local_vertices.len() as u32;
                    vertex_map.insert(global_idx, new_idx);

                    // Record PMX global -> IrModel running index
                    pmx_to_ir_vertex.insert(global_idx, ir_vertex_offset + new_idx as usize);

                    let v = &pmx.vertices[global_idx as usize];
                    let (w_arr, w_cnt) = match &v.weight {
                        PmxWeightType::Bdef1 { bone } => {
                            ([(*bone as usize, 1.0), (0, 0.0), (0, 0.0), (0, 0.0)], 1u8)
                        }
                        PmxWeightType::Bdef2 {
                            bone1,
                            bone2,
                            weight1,
                        } => {
                            if *weight1 < 1.0 {
                                (
                                    [
                                        (*bone1 as usize, *weight1),
                                        (*bone2 as usize, 1.0 - weight1),
                                        (0, 0.0),
                                        (0, 0.0),
                                    ],
                                    2,
                                )
                            } else {
                                (
                                    [(*bone1 as usize, *weight1), (0, 0.0), (0, 0.0), (0, 0.0)],
                                    1,
                                )
                            }
                        }
                        PmxWeightType::Bdef4 { bones, weights } => {
                            let mut arr = [(0usize, 0.0f32); 4];
                            let mut cnt = 0u8;
                            for (&b, &w) in bones.iter().zip(weights.iter()) {
                                if w > 0.0 && cnt < 4 {
                                    arr[cnt as usize] = (b as usize, w);
                                    cnt += 1;
                                }
                            }
                            (arr, cnt)
                        }
                    };

                    local_vertices.push(IrVertex {
                        position: pmx_pos_to_gltf(v.position),
                        normal: pmx_normal_to_gltf(v.normal),
                        uv: v.uv,
                        tangent: Vec4::ZERO, // Generated later via MikkTSpace
                        weights: w_arr,
                        weight_count: w_cnt,
                        edge_scale: v.edge_scale,
                    });
                    new_idx
                };
                local_indices.push(local_idx);
            }
        }

        ir_vertex_offset += local_vertices.len();

        meshes.push(IrMesh {
            name: mat.name.clone(),
            vertices: local_vertices.into(),
            indices: local_indices.into(),
            material_index: mat_idx,
            morph_targets: Arc::new(Vec::new()),
            node_index: 0,
            uvs1: Vec::new(),
        });

        face_offset += face_count;
    }

    // Distribute vertex morphs to meshes BEFORE generate_tangents,
    // so that any vertex split inside generate_tangents can also clone morph_targets.
    distribute_vertex_morphs(pmx, &mut meshes);

    // Tangent generation (vertices are split when tangent w disagrees, and morph_targets follow)
    for mesh in &mut meshes {
        crate::intermediate::tangent::generate_tangents(mesh, 0);
    }

    (meshes, pmx_to_ir_vertex)
}

/// Distribute PMX vertex morphs to each mesh.
fn distribute_vertex_morphs(pmx: &PmxModel, meshes: &mut [IrMesh]) {
    // Build the mapping global vertex index -> (mesh_idx, local_vertex_idx)
    let mut global_to_local: HashMap<u32, Vec<(usize, u32)>> = HashMap::new();
    let mut face_offset = 0usize;
    for (mesh_idx, mat) in pmx.materials.iter().enumerate() {
        let face_count = (mat.face_count / 3) as usize;
        let mut vertex_map: HashMap<u32, u32> = HashMap::new();
        let mut next_local = 0u32;

        for fi in face_offset..face_offset + face_count {
            let face = &pmx.faces[fi];
            // Walk faces with the same winding (b<->c swap) as extract_meshes
            // so the local vertex assignment order matches.
            let reordered = [face[0], face[2], face[1]];
            for &global_idx in &reordered {
                if let std::collections::hash_map::Entry::Vacant(e) = vertex_map.entry(global_idx) {
                    e.insert(next_local);
                    next_local += 1;
                }
            }
        }

        for (&global_idx, &local_idx) in &vertex_map {
            global_to_local
                .entry(global_idx)
                .or_default()
                .push((mesh_idx, local_idx));
        }

        face_offset += face_count;
    }

    // Distribute each vertex morph to mesh.morph_targets
    for morph in &pmx.morphs {
        if let PmxMorphOffsets::Vertex(offsets) = &morph.offsets {
            // Build a sparse morph target per mesh
            let mesh_count = meshes.len();
            let mut mesh_offsets: Vec<Vec<(u32, Vec3)>> =
                (0..mesh_count).map(|_| Vec::new()).collect();

            for off in offsets {
                let gltf_offset = pmx_pos_to_gltf(off.offset); // Displacement: flip Z + scale by 1/12.5
                if let Some(targets) = global_to_local.get(&off.vertex_index) {
                    for &(mesh_idx, local_idx) in targets {
                        mesh_offsets[mesh_idx].push((local_idx, gltf_offset));
                    }
                }
            }

            for (mesh_idx, mut offsets) in mesh_offsets.into_iter().enumerate() {
                // Only push when this mesh is actually affected
                if !offsets.is_empty() {
                    offsets.sort_by_key(|&(vi, _)| vi);
                    meshes[mesh_idx].morph_targets_mut().push(IrMorphTarget {
                        name: morph.name.clone(),
                        position_offsets: offsets,
                        normal_offsets: Vec::new(),
                        tangent_offsets: Vec::new(),
                    });
                }
            }
        }
    }
}

/// Morph extraction: vertex / group / UV morphs -> `IrMorph`.
/// Vertex morphs are rebuilt from `mesh.morph_targets` (so the splits done in
/// `generate_tangents` are honored). Group morphs are built directly from PMX data
/// (with sub-morph indices remapped). UV morphs (Phase 3 A-2) resolve PMX vertex
/// indices to IR global vertex indices via `pmx_to_ir_vertex`. Bone and material
/// morphs are still skipped.
fn extract_morphs(
    pmx: &PmxModel,
    meshes: &[IrMesh],
    pmx_to_ir_vertex: &HashMap<u32, usize>,
) -> Vec<IrMorph> {
    // Global vertex offsets per mesh (after splitting; based on the final vertex count)
    let mesh_global_offsets: Vec<usize> = {
        let mut offsets = Vec::with_capacity(meshes.len());
        let mut cum = 0usize;
        for m in meshes {
            offsets.push(cum);
            cum += m.vertices.len();
        }
        offsets
    };

    // Pass 1: build the PMX index -> IrModel index mapping.
    // Skipped morphs (bone / material) become None.
    let mut pmx_to_ir_morph: Vec<Option<usize>> = Vec::with_capacity(pmx.morphs.len());
    let mut ir_idx = 0usize;
    for m in &pmx.morphs {
        match &m.offsets {
            PmxMorphOffsets::Vertex(_) | PmxMorphOffsets::Group(_) | PmxMorphOffsets::Uv(_) => {
                pmx_to_ir_morph.push(Some(ir_idx));
                ir_idx += 1;
            }
            _ => {
                pmx_to_ir_morph.push(None);
            }
        }
    }

    // Pass 2: convert morphs (using the remapped indices)
    pmx.morphs
        .iter()
        .filter_map(|m| {
            let kind = match &m.offsets {
                PmxMorphOffsets::Vertex(_) => {
                    // Built from mesh.morph_targets (which includes split vertices from generate_tangents)
                    let mut entries: Vec<(usize, Vec3)> = Vec::new();
                    for (mi, mesh) in meshes.iter().enumerate() {
                        let global_offset = mesh_global_offsets[mi];
                        if let Some(mt) = mesh.morph_targets.iter().find(|t| t.name == m.name) {
                            for &(local_vi, offset) in &mt.position_offsets {
                                entries.push((global_offset + local_vi as usize, offset));
                            }
                        }
                    }
                    IrMorphKind::Vertex {
                        positions: entries,
                        normals: Vec::new(),
                        tangents: Vec::new(),
                    }
                }
                PmxMorphOffsets::Group(offsets) => {
                    let entries: Vec<(usize, f32)> = offsets
                        .iter()
                        .filter_map(|off| {
                            let pmx_idx = off.morph_index as usize;
                            match pmx_to_ir_morph.get(pmx_idx).copied().flatten() {
                                Some(ir_idx) => Some((ir_idx, off.weight)),
                                None => {
                                    log::warn!(
                                        "Group morph: sub-morph[{}] has unsupported morph type, skipping",
                                        pmx_idx
                                    );
                                    None
                                }
                            }
                        })
                        .collect();
                    IrMorphKind::Group(entries)
                }
                PmxMorphOffsets::Uv(offsets) => {
                    // PMX morph_type: 3=UV0, 4..=7=UV1..UV4 -> channel 0..=4
                    let channel = match m.morph_type {
                        3 => 0u8,
                        4..=7 => m.morph_type - 3,
                        _ => 0,
                    };
                    let entries: Vec<(usize, [f32; 4])> = offsets
                        .iter()
                        .filter_map(|off| {
                            pmx_to_ir_vertex
                                .get(&off.vertex_index)
                                .map(|&ir_vi| (ir_vi, off.offset.to_array()))
                        })
                        .collect();
                    IrMorphKind::Uv {
                        channel,
                        offsets: entries,
                    }
                }
                _ => return None, // Bone / material morphs are skipped
            };

            Some(IrMorph {
                name: m.name.clone(),
                name_en: m.name_en.clone(),
                panel: m.panel,
                kind,
            })
        })
        .collect()
}

/// Physics extraction.
fn extract_physics(pmx: &PmxModel) -> IrPhysics {
    let rigid_bodies = pmx
        .rigid_bodies
        .iter()
        .map(|r| {
            // bone_index = -1 (no associated bone) -> follow bone 0 (center) instead
            let bone_index = if r.bone_index >= 0 {
                Some(r.bone_index as usize)
            } else if !pmx.bones.is_empty() {
                Some(0)
            } else {
                None
            };

            let shape = match r.shape {
                0 => RigidShape::Sphere { radius: r.size.x },
                1 => RigidShape::Box { size: r.size },
                2 => RigidShape::Capsule {
                    radius: r.size.x,
                    height: r.size.y,
                },
                _ => RigidShape::Sphere { radius: r.size.x },
            };

            IrRigidBody {
                name: r.name.clone(),
                bone_index,
                group: r.group,
                no_collision_mask: r.no_collision_mask,
                shape,
                position: r.position, // Keep in PMX coords (the viewer renders in PMX coords)
                rotation: r.rotation,
                mass: r.mass,
                linear_damping: r.linear_damping,
                angular_damping: r.angular_damping,
                restitution: r.restitution,
                friction: r.friction,
                physics_mode: r.physics_mode,
            }
        })
        .collect();

    let joints = pmx
        .joints
        .iter()
        .map(|j| IrJoint {
            name: j.name.clone(),
            rigid_a: if j.rigid_a >= 0 {
                j.rigid_a as usize
            } else {
                0
            },
            rigid_b: if j.rigid_b >= 0 {
                j.rigid_b as usize
            } else {
                0
            },
            position: j.position, // Keep in PMX coords
            rotation: j.rotation,
            move_limit_lo: j.move_limit_lo,
            move_limit_hi: j.move_limit_hi,
            rot_limit_lo: j.rot_limit_lo,
            rot_limit_hi: j.rot_limit_hi,
            spring_move: j.spring_move,
            spring_rot: j.spring_rot,
        })
        .collect();

    IrPhysics {
        rigid_bodies,
        joints,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pmx_to_ir_seed_san() {
        let Some(pmx_path) = crate::test_util::try_test_file(crate::test_util::seed_san_pmx())
        else {
            return;
        };

        let pmx = crate::pmx::reader::read_pmx(&pmx_path).expect("PMX読み込み失敗");
        let pmx_dir = pmx_path.parent().unwrap();
        let ir = pmx_to_ir(&pmx, pmx_dir).expect("PMX→IrModel変換失敗");

        assert_eq!(ir.source_format, SourceFormat::Pmx);
        assert_eq!(ir.bones.len(), 179);
        assert_eq!(ir.materials.len(), 17);
        assert_eq!(ir.meshes.len(), 17); // Equals the number of materials
        assert!(!ir.name.is_empty());

        // Verify the total vertex count
        let total_verts: usize = ir.meshes.iter().map(|m| m.vertices.len()).sum();
        assert!(total_verts > 0, "頂点数が0");

        // Verify the total face count
        let total_faces: usize = ir.meshes.iter().map(|m| m.indices.len() / 3).sum();
        assert_eq!(total_faces, 45058);

        // Physics info
        assert_eq!(ir.physics.rigid_bodies.len(), 36);
        assert_eq!(ir.physics.joints.len(), 19);

        // Verify bone parent/child consistency
        for (i, bone) in ir.bones.iter().enumerate() {
            if let Some(parent) = bone.parent {
                assert!(parent < ir.bones.len(), "ボーン{}の親{}が範囲外", i, parent);
                assert!(
                    ir.bones[parent].children.contains(&i),
                    "ボーン{}が親{}のchildrenに含まれていない",
                    i,
                    parent
                );
            }
        }

        // Verify texture data
        for tex in &ir.textures {
            assert!(
                !tex.data.is_empty(),
                "テクスチャ '{}' のデータが空",
                tex.filename
            );
        }
    }

    #[test]
    fn test_aux_get_ignore_case() {
        let mut aux: HashMap<PathBuf, Arc<[u8]>> = HashMap::new();
        // Archive stored the file with a lowercase extension.
        aux.insert(
            PathBuf::from("textures/body_d.png"),
            Arc::from(vec![1u8, 2, 3].into_boxed_slice()),
        );

        // Exact match still works through the caller's `aux.get`; here we verify the
        // case-insensitive fallback resolves a PMX reference that uses `.PNG`.
        let hit = aux_get_ignore_case(&aux, "textures/body_d.PNG");
        assert!(hit.is_some(), "大文字拡張子の参照が解決できていない");
        assert_eq!(&**hit.unwrap(), &[1u8, 2, 3]);

        // A differing-case directory component must also resolve.
        let hit_dir = aux_get_ignore_case(&aux, "Textures/BODY_D.png");
        assert!(
            hit_dir.is_some(),
            "ディレクトリ名の大文字小文字差が解決できていない"
        );

        // A genuinely different file must not match.
        assert!(aux_get_ignore_case(&aux, "textures/other.png").is_none());
    }

    #[test]
    fn test_extract_textures_case_insensitive_aux() {
        // A PMX referencing `foo.PNG` while the archive stores `foo.png` must still
        // resolve the bytes (regression for the ZIP/7z texture "format error").
        let mut pmx = PmxModel::default();
        pmx.textures = vec!["textures\\Body_D.PNG".to_string()];

        let mut aux: HashMap<PathBuf, Arc<[u8]>> = HashMap::new();
        aux.insert(
            PathBuf::from("textures/body_d.png"),
            Arc::from(vec![9u8, 8, 7, 6].into_boxed_slice()),
        );

        let textures = extract_textures(&pmx, Path::new(""), Some(&aux));
        assert_eq!(textures.len(), 1);
        assert_eq!(
            textures[0].data.as_bytes(),
            &[9u8, 8, 7, 6],
            "大文字小文字が異なる参照でテクスチャデータが空になっている"
        );
        assert_eq!(textures[0].mime_type, "image/png");
    }
}
