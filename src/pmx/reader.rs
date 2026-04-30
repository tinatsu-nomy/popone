use crate::error::{PoponeError, Result};
use byteorder::{LittleEndian, ReadBytesExt};
use rust_i18n::t;
use std::io::Read;

use crate::pmx::types::*;

pub struct PmxReader<R: Read> {
    reader: R,
    header: PmxHeader,
}

/// Convert an i32 count to usize. Negative values are treated as a
/// corrupted file and produce a localized error.
///
/// `field_key` is an i18n key (e.g. `"error.pmx.field.vertex_count"`)
/// that resolves to the localized field name shown in the error.
#[inline]
fn checked_count(val: i32, field_key: &str) -> Result<usize> {
    if val < 0 {
        let field = t!(field_key).to_string();
        return Err(PoponeError::PmxParse(
            t!(
                "error.pmx.field_negative",
                field = field,
                val = val.to_string()
            )
            .to_string(),
        ));
    }
    Ok(val as usize)
}

impl<R: Read> PmxReader<R> {
    pub fn new(reader: R) -> Self {
        Self {
            reader,
            header: PmxHeader::default(),
        }
    }

    pub fn read_model(&mut self) -> Result<PmxModel> {
        let header = self.read_header()?;
        self.header = header.clone();
        let model_info = self.read_model_info()?;
        let vertices = self.read_vertices()?;
        let faces = self.read_faces()?;
        let textures = self.read_textures()?;
        let materials = self.read_materials()?;
        let bones = self.read_bones()?;
        let morphs = self.read_morphs()?;
        let display_frames = self.read_display_frames()?;
        let rigid_bodies = self.read_rigid_bodies()?;
        let joints = self.read_joints()?;

        // PMX 2.1: skip the SoftBody section (ignore on EOF)
        if header.version >= 2.1 {
            let _ = self.skip_soft_bodies();
        }

        Ok(PmxModel {
            header,
            model_info,
            vertices,
            faces,
            textures,
            materials,
            bones,
            morphs,
            display_frames,
            rigid_bodies,
            joints,
        })
    }

    fn read_header(&mut self) -> Result<PmxHeader> {
        let mut magic = [0u8; 4];
        self.reader.read_exact(&mut magic)?;
        if &magic != b"PMX " {
            return Err(PoponeError::PmxParse(
                t!("error.pmx.invalid_magic", magic = format!("{magic:?}")).to_string(),
            ));
        }

        let version = self.reader.read_f32::<LittleEndian>()?;
        if !(2.0..=2.1).contains(&version) {
            return Err(PoponeError::PmxParse(
                t!(
                    "error.pmx.unsupported_version",
                    version = version.to_string()
                )
                .to_string(),
            ));
        }

        let globals_count = self.reader.read_u8()?;
        if globals_count != 8 {
            return Err(PoponeError::PmxParse(
                t!(
                    "error.pmx.invalid_globals_count",
                    count = globals_count.to_string()
                )
                .to_string(),
            ));
        }

        let encoding = self.reader.read_u8()?;
        let additional_uvs = self.reader.read_u8()?;
        let vertex_index_size = self.reader.read_u8()?;
        let texture_index_size = self.reader.read_u8()?;
        let material_index_size = self.reader.read_u8()?;
        let bone_index_size = self.reader.read_u8()?;
        let morph_index_size = self.reader.read_u8()?;
        let rigid_body_index_size = self.reader.read_u8()?;

        Ok(PmxHeader {
            version,
            encoding,
            additional_uvs,
            vertex_index_size,
            texture_index_size,
            material_index_size,
            bone_index_size,
            morph_index_size,
            rigid_body_index_size,
        })
    }

    fn read_text(&mut self) -> Result<String> {
        let byte_len_i32 = self.reader.read_i32::<LittleEndian>()?;
        if byte_len_i32 < 0 {
            return Err(PoponeError::PmxParse(
                t!(
                    "error.pmx.text_length_negative",
                    len = byte_len_i32.to_string()
                )
                .to_string(),
            ));
        }
        let byte_len = byte_len_i32 as usize;
        let mut buf = vec![0u8; byte_len];
        self.reader.read_exact(&mut buf)?;

        if self.header.encoding == 0 {
            // UTF-16LE
            if !buf.len().is_multiple_of(2) {
                return Err(PoponeError::PmxParse(
                    t!("error.pmx.utf16_length_odd", len = buf.len().to_string()).to_string(),
                ));
            }
            let utf16: Vec<u16> = buf
                .chunks_exact(2)
                .map(|c| u16::from_le_bytes([c[0], c[1]]))
                .collect();
            Ok(String::from_utf16_lossy(&utf16))
        } else {
            // UTF-8
            Ok(String::from_utf8_lossy(&buf).into_owned())
        }
    }

    fn read_model_info(&mut self) -> Result<PmxModelInfo> {
        Ok(PmxModelInfo {
            name: self.read_text()?,
            name_en: self.read_text()?,
            comment: self.read_text()?,
            comment_en: self.read_text()?,
        })
    }

    /// Read an unsigned vertex index.
    fn read_vertex_index(&mut self) -> Result<u32> {
        Ok(match self.header.vertex_index_size {
            1 => self.reader.read_u8()? as u32,
            2 => self.reader.read_u16::<LittleEndian>()? as u32,
            _ => self.reader.read_u32::<LittleEndian>()?,
        })
    }

    /// Read a signed bone index.
    fn read_bone_index(&mut self) -> Result<i32> {
        Ok(match self.header.bone_index_size {
            1 => self.reader.read_i8()? as i32,
            2 => self.reader.read_i16::<LittleEndian>()? as i32,
            _ => self.reader.read_i32::<LittleEndian>()?,
        })
    }

    fn read_texture_index(&mut self) -> Result<i32> {
        Ok(match self.header.texture_index_size {
            1 => self.reader.read_i8()? as i32,
            2 => self.reader.read_i16::<LittleEndian>()? as i32,
            _ => self.reader.read_i32::<LittleEndian>()?,
        })
    }

    fn read_material_index(&mut self) -> Result<i32> {
        Ok(match self.header.material_index_size {
            1 => self.reader.read_i8()? as i32,
            2 => self.reader.read_i16::<LittleEndian>()? as i32,
            _ => self.reader.read_i32::<LittleEndian>()?,
        })
    }

    fn read_morph_index(&mut self) -> Result<i32> {
        Ok(match self.header.morph_index_size {
            1 => self.reader.read_i8()? as i32,
            2 => self.reader.read_i16::<LittleEndian>()? as i32,
            _ => self.reader.read_i32::<LittleEndian>()?,
        })
    }

    fn read_rigid_index(&mut self) -> Result<i32> {
        Ok(match self.header.rigid_body_index_size {
            1 => self.reader.read_i8()? as i32,
            2 => self.reader.read_i16::<LittleEndian>()? as i32,
            _ => self.reader.read_i32::<LittleEndian>()?,
        })
    }

    fn read_vec2(&mut self) -> Result<glam::Vec2> {
        let x = self.reader.read_f32::<LittleEndian>()?;
        let y = self.reader.read_f32::<LittleEndian>()?;
        Ok(glam::Vec2::new(x, y))
    }

    fn read_vec3(&mut self) -> Result<glam::Vec3> {
        let x = self.reader.read_f32::<LittleEndian>()?;
        let y = self.reader.read_f32::<LittleEndian>()?;
        let z = self.reader.read_f32::<LittleEndian>()?;
        Ok(glam::Vec3::new(x, y, z))
    }

    fn read_vec4(&mut self) -> Result<glam::Vec4> {
        let x = self.reader.read_f32::<LittleEndian>()?;
        let y = self.reader.read_f32::<LittleEndian>()?;
        let z = self.reader.read_f32::<LittleEndian>()?;
        let w = self.reader.read_f32::<LittleEndian>()?;
        Ok(glam::Vec4::new(x, y, z, w))
    }

    fn read_vertices(&mut self) -> Result<Vec<PmxVertex>> {
        let count = checked_count(
            self.reader.read_i32::<LittleEndian>()?,
            "error.pmx.field.vertex_count",
        )?;
        let mut vertices = Vec::with_capacity(count);

        for _ in 0..count {
            let position = self.read_vec3()?;
            let normal = self.read_vec3()?;
            let uv = self.read_vec2()?;

            // Skip additional UVs
            for _ in 0..self.header.additional_uvs {
                let _ = self.read_vec4()?;
            }

            let weight_type = self.reader.read_u8()?;
            let weight = match weight_type {
                0 => {
                    // BDEF1
                    let bone = self.read_bone_index()?;
                    PmxWeightType::Bdef1 { bone }
                }
                1 => {
                    // BDEF2
                    let bone1 = self.read_bone_index()?;
                    let bone2 = self.read_bone_index()?;
                    let weight1 = self.reader.read_f32::<LittleEndian>()?;
                    PmxWeightType::Bdef2 {
                        bone1,
                        bone2,
                        weight1,
                    }
                }
                2 | 4 => {
                    // BDEF4 / QDEF (QDEF is treated as BDEF4)
                    let mut bones = [0i32; 4];
                    let mut weights = [0f32; 4];
                    for b in &mut bones {
                        *b = self.read_bone_index()?;
                    }
                    for w in &mut weights {
                        *w = self.reader.read_f32::<LittleEndian>()?;
                    }
                    PmxWeightType::Bdef4 { bones, weights }
                }
                3 => {
                    // SDEF -> BDEF2 fallback
                    let bone1 = self.read_bone_index()?;
                    let bone2 = self.read_bone_index()?;
                    let weight1 = self.reader.read_f32::<LittleEndian>()?;
                    // Skip SDEF-C, R0, R1
                    let _ = self.read_vec3()?;
                    let _ = self.read_vec3()?;
                    let _ = self.read_vec3()?;
                    PmxWeightType::Bdef2 {
                        bone1,
                        bone2,
                        weight1,
                    }
                }
                _ => {
                    return Err(PoponeError::PmxParse(
                        t!(
                            "error.pmx.unsupported_weight_type",
                            weight_type = weight_type.to_string()
                        )
                        .to_string(),
                    ))
                }
            };

            let edge_scale = self.reader.read_f32::<LittleEndian>()?;

            vertices.push(PmxVertex {
                position,
                normal,
                uv,
                weight,
                edge_scale,
            });
        }
        Ok(vertices)
    }

    fn read_faces(&mut self) -> Result<Vec<[u32; 3]>> {
        let index_count = checked_count(
            self.reader.read_i32::<LittleEndian>()?,
            "error.pmx.field.face_index_count",
        )?;
        if !index_count.is_multiple_of(3) {
            return Err(PoponeError::PmxParse(
                t!(
                    "error.pmx.face_index_count_not_multiple_of_3",
                    count = index_count.to_string()
                )
                .to_string(),
            ));
        }
        let face_count = index_count / 3;
        let mut faces = Vec::with_capacity(face_count);
        for _ in 0..face_count {
            let a = self.read_vertex_index()?;
            let b = self.read_vertex_index()?;
            let c = self.read_vertex_index()?;
            faces.push([a, b, c]);
        }
        Ok(faces)
    }

    fn read_textures(&mut self) -> Result<Vec<String>> {
        let count = checked_count(
            self.reader.read_i32::<LittleEndian>()?,
            "error.pmx.field.texture_count",
        )?;
        let mut textures = Vec::with_capacity(count);
        for _ in 0..count {
            textures.push(self.read_text()?);
        }
        Ok(textures)
    }

    fn read_materials(&mut self) -> Result<Vec<PmxMaterial>> {
        let count = checked_count(
            self.reader.read_i32::<LittleEndian>()?,
            "error.pmx.field.material_count",
        )?;
        let mut materials = Vec::with_capacity(count);
        for _ in 0..count {
            let name = self.read_text()?;
            let name_en = self.read_text()?;
            let diffuse = self.read_vec4()?;
            let specular = self.read_vec3()?;
            let specular_power = self.reader.read_f32::<LittleEndian>()?;
            let ambient = self.read_vec3()?;
            let draw_flags = self.reader.read_u8()?;
            let edge_color = self.read_vec4()?;
            let edge_size = self.reader.read_f32::<LittleEndian>()?;

            let tex_idx = self.read_texture_index()?;
            let texture_index = if tex_idx < 0 { None } else { Some(tex_idx) };

            let sphere_idx = self.read_texture_index()?;
            let sphere_texture_index = if sphere_idx < 0 {
                None
            } else {
                Some(sphere_idx)
            };

            let sphere_mode = self.reader.read_u8()?;

            let shared_toon_flag = self.reader.read_u8()?;
            let toon_ref = if shared_toon_flag == 1 {
                PmxToonRef::Shared(self.reader.read_u8()?)
            } else {
                PmxToonRef::Texture(self.read_texture_index()?)
            };

            let memo = self.read_text()?;
            let face_count = self.reader.read_i32::<LittleEndian>()? as u32;

            materials.push(PmxMaterial {
                name,
                name_en,
                diffuse,
                specular,
                specular_power,
                ambient,
                draw_flags,
                edge_color,
                edge_size,
                texture_index,
                sphere_texture_index,
                sphere_mode,
                toon_ref,
                memo,
                face_count,
            });
        }
        Ok(materials)
    }

    fn read_bones(&mut self) -> Result<Vec<PmxBone>> {
        let count = checked_count(
            self.reader.read_i32::<LittleEndian>()?,
            "error.pmx.field.bone_count",
        )?;
        let mut bones = Vec::with_capacity(count);
        for _ in 0..count {
            let name = self.read_text()?;
            let name_en = self.read_text()?;
            let position = self.read_vec3()?;
            let parent_index = self.read_bone_index()?;
            let deform_layer = self.reader.read_i32::<LittleEndian>()?;
            let flags = self.reader.read_u16::<LittleEndian>()?;

            // Tail link
            let tail = if flags & BONE_FLAG_TAIL_IS_BONE != 0 {
                BoneTail::BoneIndex(self.read_bone_index()?)
            } else {
                BoneTail::Offset(self.read_vec3()?)
            };

            // Grant (append parent)
            let grant = if flags & (BONE_FLAG_ROTATION_GRANT | BONE_FLAG_MOVE_GRANT) != 0 {
                let parent = self.read_bone_index()?;
                let ratio = self.reader.read_f32::<LittleEndian>()?;
                Some(PmxGrant {
                    parent_index: parent,
                    ratio,
                })
            } else {
                None
            };

            // Axis-fixed
            if flags & BONE_FLAG_AXIS_FIXED != 0 {
                let _ = self.read_vec3()?; // Axis direction vector
            }

            // Local axes
            if flags & BONE_FLAG_LOCAL_AXIS != 0 {
                let _ = self.read_vec3()?; // X axis
                let _ = self.read_vec3()?; // Z axis
            }

            // External parent deform
            if flags & BONE_FLAG_EXT_PARENT != 0 {
                let _ = self.reader.read_i32::<LittleEndian>()?; // Key value
            }

            // IK
            let ik = if flags & BONE_FLAG_IK != 0 {
                let target_bone = self.read_bone_index()?;
                let loop_count = self.reader.read_i32::<LittleEndian>()?;
                let limit_angle = self.reader.read_f32::<LittleEndian>()?;
                let link_count = checked_count(
                    self.reader.read_i32::<LittleEndian>()?,
                    "error.pmx.field.ik_link_count",
                )?;
                let mut links = Vec::with_capacity(link_count);
                for _ in 0..link_count {
                    let bone_index = self.read_bone_index()?;
                    let has_limit = self.reader.read_u8()? != 0;
                    let (limit_min, limit_max) = if has_limit {
                        (self.read_vec3()?, self.read_vec3()?)
                    } else {
                        (glam::Vec3::ZERO, glam::Vec3::ZERO)
                    };
                    links.push(IkLink {
                        bone_index,
                        angle_limit: has_limit,
                        limit_min,
                        limit_max,
                    });
                }
                Some(PmxIk {
                    target_bone,
                    loop_count,
                    limit_angle,
                    links,
                })
            } else {
                None
            };

            bones.push(PmxBone {
                name,
                name_en,
                position,
                parent_index,
                deform_layer,
                flags,
                tail,
                ik,
                grant,
            });
        }
        Ok(bones)
    }

    fn read_morphs(&mut self) -> Result<Vec<PmxMorph>> {
        let count = checked_count(
            self.reader.read_i32::<LittleEndian>()?,
            "error.pmx.field.morph_count",
        )?;
        let mut morphs = Vec::with_capacity(count);
        for _ in 0..count {
            let name = self.read_text()?;
            let name_en = self.read_text()?;
            let panel = self.reader.read_u8()?;
            let morph_type = self.reader.read_u8()?;
            let offset_count = checked_count(
                self.reader.read_i32::<LittleEndian>()?,
                "error.pmx.field.morph_offset_count",
            )?;

            let offsets = match morph_type {
                0 | 9 => {
                    // Group / Flip (Flip is treated as Group)
                    let mut v = Vec::with_capacity(offset_count);
                    for _ in 0..offset_count {
                        let morph_index = self.read_morph_index()?;
                        let weight = self.reader.read_f32::<LittleEndian>()?;
                        v.push(GroupMorphOffset {
                            morph_index,
                            weight,
                        });
                    }
                    PmxMorphOffsets::Group(v)
                }
                1 => {
                    // Vertex
                    let mut v = Vec::with_capacity(offset_count);
                    for _ in 0..offset_count {
                        let vertex_index = self.read_vertex_index()?;
                        let offset = self.read_vec3()?;
                        v.push(VertexMorphOffset {
                            vertex_index,
                            offset,
                        });
                    }
                    PmxMorphOffsets::Vertex(v)
                }
                2 => {
                    // Bone
                    let mut v = Vec::with_capacity(offset_count);
                    for _ in 0..offset_count {
                        let bone_index = self.read_bone_index()?;
                        let translation = self.read_vec3()?;
                        let x = self.reader.read_f32::<LittleEndian>()?;
                        let y = self.reader.read_f32::<LittleEndian>()?;
                        let z = self.reader.read_f32::<LittleEndian>()?;
                        let w = self.reader.read_f32::<LittleEndian>()?;
                        let rotation = glam::Quat::from_xyzw(x, y, z, w);
                        v.push(BoneMorphOffset {
                            bone_index,
                            translation,
                            rotation,
                        });
                    }
                    PmxMorphOffsets::Bone(v)
                }
                3..=7 => {
                    // UV / additional UV1..UV4
                    let mut v = Vec::with_capacity(offset_count);
                    for _ in 0..offset_count {
                        let vertex_index = self.read_vertex_index()?;
                        let offset = self.read_vec4()?;
                        v.push(UvMorphOffset {
                            vertex_index,
                            offset,
                        });
                    }
                    PmxMorphOffsets::Uv(v)
                }
                8 => {
                    // Material
                    let mut v = Vec::with_capacity(offset_count);
                    for _ in 0..offset_count {
                        let material_index = self.read_material_index()?;
                        let offset_mode = self.reader.read_u8()?;
                        let diffuse = self.read_vec4()?;
                        let specular = self.read_vec3()?;
                        let specular_power = self.reader.read_f32::<LittleEndian>()?;
                        let ambient = self.read_vec3()?;
                        let edge_color = self.read_vec4()?;
                        let edge_size = self.reader.read_f32::<LittleEndian>()?;
                        let texture_factor = self.read_vec4()?;
                        let sphere_factor = self.read_vec4()?;
                        let toon_factor = self.read_vec4()?;
                        v.push(MaterialMorphOffset {
                            material_index,
                            offset_mode,
                            diffuse,
                            specular,
                            specular_power,
                            ambient,
                            edge_color,
                            edge_size,
                            texture_factor,
                            sphere_factor,
                            toon_factor,
                        });
                    }
                    PmxMorphOffsets::Material(v)
                }
                10 => {
                    // Impulse (PMX 2.1): skip, store as an empty group
                    for _ in 0..offset_count {
                        let _ = self.read_rigid_index()?; // Rigid body index
                        let _ = self.reader.read_u8()?; // Local flag
                        let _ = self.read_vec3()?; // Linear velocity
                        let _ = self.read_vec3()?; // Angular torque
                    }
                    PmxMorphOffsets::Group(Vec::new())
                }
                _ => {
                    return Err(PoponeError::PmxParse(
                        t!(
                            "error.pmx.unsupported_morph_type",
                            morph_type = morph_type.to_string()
                        )
                        .to_string(),
                    ))
                }
            };

            morphs.push(PmxMorph {
                name,
                name_en,
                panel,
                morph_type,
                offsets,
            });
        }
        Ok(morphs)
    }

    fn read_display_frames(&mut self) -> Result<Vec<PmxDisplayFrame>> {
        let count = checked_count(
            self.reader.read_i32::<LittleEndian>()?,
            "error.pmx.field.display_frame_count",
        )?;
        let mut frames = Vec::with_capacity(count);
        for _ in 0..count {
            let name = self.read_text()?;
            let name_en = self.read_text()?;
            let is_special = self.reader.read_u8()?;
            let elem_count = checked_count(
                self.reader.read_i32::<LittleEndian>()?,
                "error.pmx.field.display_frame_element_count",
            )?;
            let mut elements = Vec::with_capacity(elem_count);
            for _ in 0..elem_count {
                let elem_type = self.reader.read_u8()?;
                let elem = if elem_type == 0 {
                    DisplayFrameElement::Bone(self.read_bone_index()?)
                } else {
                    DisplayFrameElement::Morph(self.read_morph_index()?)
                };
                elements.push(elem);
            }
            frames.push(PmxDisplayFrame {
                name,
                name_en,
                is_special,
                elements,
            });
        }
        Ok(frames)
    }

    fn read_rigid_bodies(&mut self) -> Result<Vec<PmxRigidBody>> {
        let count = checked_count(
            self.reader.read_i32::<LittleEndian>()?,
            "error.pmx.field.rigid_body_count",
        )?;
        let mut bodies = Vec::with_capacity(count);
        for _ in 0..count {
            let name = self.read_text()?;
            let name_en = self.read_text()?;
            let bone_index = self.read_bone_index()?;
            let group = self.reader.read_u8()?;
            let no_collision_mask = self.reader.read_u16::<LittleEndian>()?;
            let shape = self.reader.read_u8()?;
            let size = self.read_vec3()?;
            let position = self.read_vec3()?;
            let rotation = self.read_vec3()?;
            let mass = self.reader.read_f32::<LittleEndian>()?;
            let linear_damping = self.reader.read_f32::<LittleEndian>()?;
            let angular_damping = self.reader.read_f32::<LittleEndian>()?;
            let restitution = self.reader.read_f32::<LittleEndian>()?;
            let friction = self.reader.read_f32::<LittleEndian>()?;
            let physics_mode = self.reader.read_u8()?;
            bodies.push(PmxRigidBody {
                name,
                name_en,
                bone_index,
                group,
                no_collision_mask,
                shape,
                size,
                position,
                rotation,
                mass,
                linear_damping,
                angular_damping,
                restitution,
                friction,
                physics_mode,
            });
        }
        Ok(bodies)
    }

    fn read_joints(&mut self) -> Result<Vec<PmxJoint>> {
        let count = checked_count(
            self.reader.read_i32::<LittleEndian>()?,
            "error.pmx.field.joint_count",
        )?;
        let mut joints = Vec::with_capacity(count);
        for _ in 0..count {
            let name = self.read_text()?;
            let name_en = self.read_text()?;
            let joint_type = self.reader.read_u8()?;
            let rigid_a = self.read_rigid_index()?;
            let rigid_b = self.read_rigid_index()?;
            let position = self.read_vec3()?;
            let rotation = self.read_vec3()?;
            let move_limit_lo = self.read_vec3()?;
            let move_limit_hi = self.read_vec3()?;
            let rot_limit_lo = self.read_vec3()?;
            let rot_limit_hi = self.read_vec3()?;
            let spring_move = self.read_vec3()?;
            let spring_rot = self.read_vec3()?;
            joints.push(PmxJoint {
                name,
                name_en,
                joint_type,
                rigid_a,
                rigid_b,
                position,
                rotation,
                move_limit_lo,
                move_limit_hi,
                rot_limit_lo,
                rot_limit_hi,
                spring_move,
                spring_rot,
            });
        }
        Ok(joints)
    }

    /// Skip the PMX 2.1 SoftBody section.
    fn skip_soft_bodies(&mut self) -> Result<()> {
        let count = self.reader.read_i32::<LittleEndian>()?;
        for _ in 0..count {
            let _ = self.read_text()?; // Name (JP)
            let _ = self.read_text()?; // Name (EN)
            let _ = self.reader.read_u8()?; // Shape
            let _ = self.read_material_index()?; // Material index
            let _ = self.reader.read_u8()?; // Group
            let _ = self.reader.read_u16::<LittleEndian>()?; // No-collision group
            let _ = self.reader.read_u8()?; // Flags
            let _ = self.reader.read_i32::<LittleEndian>()?; // B-Link distance
            let _ = self.reader.read_i32::<LittleEndian>()?; // Cluster count
            let _ = self.reader.read_f32::<LittleEndian>()?; // Total mass
            let _ = self.reader.read_f32::<LittleEndian>()?; // Margin
            let _ = self.reader.read_i32::<LittleEndian>()?; // AeroModel
                                                             // Config: 12 floats
            for _ in 0..12 {
                let _ = self.reader.read_f32::<LittleEndian>()?;
            }
            // Cluster: 6 floats
            for _ in 0..6 {
                let _ = self.reader.read_f32::<LittleEndian>()?;
            }
            // Iteration: 4 ints
            for _ in 0..4 {
                let _ = self.reader.read_i32::<LittleEndian>()?;
            }
            // Material: 3 floats
            for _ in 0..3 {
                let _ = self.reader.read_f32::<LittleEndian>()?;
            }
            // Anchor rigid bodies
            let anchor_count = checked_count(
                self.reader.read_i32::<LittleEndian>()?,
                "error.pmx.field.anchor_count",
            )?;
            for _ in 0..anchor_count {
                let _ = self.read_rigid_index()?;
                let _ = self.read_vertex_index()?;
                let _ = self.reader.read_u8()?;
            }
            // Pin vertices
            let pin_count = checked_count(
                self.reader.read_i32::<LittleEndian>()?,
                "error.pmx.field.pin_vertex_count",
            )?;
            for _ in 0..pin_count {
                let _ = self.read_vertex_index()?;
            }
        }
        Ok(())
    }
}

/// Read a PMX file and return a `PmxModel`.
pub fn read_pmx(path: &std::path::Path) -> Result<PmxModel> {
    let file = std::fs::File::open(path)?;
    let reader = std::io::BufReader::new(file);
    let mut pmx_reader = PmxReader::new(reader);
    pmx_reader.read_model()
}

/// Read a PMX from a byte slice (used by the in-memory cache).
pub fn read_pmx_from_data(data: &[u8]) -> Result<PmxModel> {
    let cursor = std::io::Cursor::new(data);
    let mut pmx_reader = PmxReader::new(cursor);
    pmx_reader.read_model()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_read_seed_san_pmx() {
        let Some(path) = crate::test_util::try_test_file(crate::test_util::seed_san_pmx()) else {
            return;
        };

        let model = read_pmx(&path).expect("PMX読み込みに失敗");

        // Compare against known values for Seed-san
        assert_eq!(model.header.version, 2.0);
        assert_eq!(model.header.encoding, 0); // UTF-16LE
        assert_eq!(model.vertices.len(), 34261);
        assert_eq!(model.faces.len(), 45058);
        assert_eq!(model.textures.len(), 15);
        assert_eq!(model.materials.len(), 17);
        assert_eq!(model.bones.len(), 179);
        assert_eq!(model.morphs.len(), 17);
        assert_eq!(model.display_frames.len(), 7);
        assert_eq!(model.rigid_bodies.len(), 36);
        assert_eq!(model.joints.len(), 19);

        // Model name
        assert!(!model.model_info.name.is_empty(), "モデル名が空");

        // Bones: the first bone must be "全ての親" (root parent)
        assert_eq!(model.bones[0].name, "全ての親");
        assert_eq!(model.bones[0].parent_index, -1);

        // Total material face-vertex count = face count * 3
        let total_face_verts: u32 = model.materials.iter().map(|m| m.face_count).sum();
        assert_eq!(total_face_verts as usize, model.faces.len() * 3);

        println!("PMX load test success: {} bones={}, vertices={}, faces={}, materials={}, morphs={}, rigidbodies={}, joints={}",
            model.model_info.name,
            model.bones.len(),
            model.vertices.len(),
            model.faces.len(),
            model.materials.len(),
            model.morphs.len(),
            model.rigid_bodies.len(),
            model.joints.len(),
        );
    }
}
