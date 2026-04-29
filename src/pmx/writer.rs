use crate::error::Result;
use byteorder::{LittleEndian, WriteBytesExt};
use std::io::Write;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::pmx::types::*;

pub struct PmxWriter<W: Write> {
    writer: W,
    header: PmxHeader,
}

impl<W: Write> PmxWriter<W> {
    pub fn new(writer: W, header: PmxHeader) -> Self {
        Self { writer, header }
    }

    pub fn write_model(&mut self, model: &PmxModel) -> Result<()> {
        self.write_model_opt_cancel(model, None)
    }

    pub fn write_model_opt_cancel(
        &mut self,
        model: &PmxModel,
        cancel: Option<&AtomicBool>,
    ) -> Result<()> {
        let check = || -> Result<()> {
            if let Some(c) = cancel {
                if c.load(Ordering::Relaxed) {
                    return Err(crate::error::PoponeError::Other(
                        "PMX write cancelled".into(),
                    ));
                }
            }
            Ok(())
        };

        self.write_header(&model.header)?;
        self.write_model_info(&model.model_info)?;
        check()?;
        self.write_vertices(&model.vertices)?;
        check()?;
        self.write_faces(&model.faces)?;
        check()?;
        self.write_textures(&model.textures)?;
        self.write_materials(&model.materials)?;
        check()?;
        self.write_bones(&model.bones)?;
        self.write_morphs(&model.morphs)?;
        check()?;
        self.write_display_frames(&model.display_frames)?;
        self.write_rigid_bodies(&model.rigid_bodies)?;
        self.write_joints(&model.joints)?;
        Ok(())
    }

    fn write_header(&mut self, header: &PmxHeader) -> Result<()> {
        // Magic number "PMX "
        self.writer.write_all(b"PMX ")?;
        // Version
        self.writer.write_f32::<LittleEndian>(header.version)?;
        // Trailing byte count = 8
        self.writer.write_u8(8)?;
        // Settings byte sequence
        self.writer.write_u8(header.encoding)?;
        self.writer.write_u8(header.additional_uvs)?;
        self.writer.write_u8(header.vertex_index_size)?;
        self.writer.write_u8(header.texture_index_size)?;
        self.writer.write_u8(header.material_index_size)?;
        self.writer.write_u8(header.bone_index_size)?;
        self.writer.write_u8(header.morph_index_size)?;
        self.writer.write_u8(header.rigid_body_index_size)?;
        Ok(())
    }

    fn write_text(&mut self, text: &str) -> Result<()> {
        let bytes = if self.header.encoding == 0 {
            // UTF16LE
            let utf16: Vec<u16> = text.encode_utf16().collect();
            let mut bytes = Vec::with_capacity(utf16.len() * 2);
            for c in utf16 {
                bytes.extend_from_slice(&c.to_le_bytes());
            }
            bytes
        } else {
            // UTF-8 — write directly (no copy needed)
            self.writer.write_i32::<LittleEndian>(text.len() as i32)?;
            self.writer.write_all(text.as_bytes())?;
            return Ok(());
        };

        self.writer.write_i32::<LittleEndian>(bytes.len() as i32)?;
        self.writer.write_all(&bytes)?;
        Ok(())
    }

    fn write_model_info(&mut self, info: &PmxModelInfo) -> Result<()> {
        self.write_text(&info.name)?;
        self.write_text(&info.name_en)?;
        self.write_text(&info.comment)?;
        self.write_text(&info.comment_en)?;
        Ok(())
    }

    fn write_vertex_index(&mut self, idx: u32) -> Result<()> {
        match self.header.vertex_index_size {
            1 => self.writer.write_u8(idx as u8)?,
            2 => self.writer.write_u16::<LittleEndian>(idx as u16)?,
            _ => self.writer.write_u32::<LittleEndian>(idx)?,
        }
        Ok(())
    }

    fn write_bone_index(&mut self, idx: i32) -> Result<()> {
        match self.header.bone_index_size {
            1 => self.writer.write_i8(idx as i8)?,
            2 => self.writer.write_i16::<LittleEndian>(idx as i16)?,
            _ => self.writer.write_i32::<LittleEndian>(idx)?,
        }
        Ok(())
    }

    fn write_texture_index(&mut self, idx: i32) -> Result<()> {
        match self.header.texture_index_size {
            1 => self.writer.write_i8(idx as i8)?,
            2 => self.writer.write_i16::<LittleEndian>(idx as i16)?,
            _ => self.writer.write_i32::<LittleEndian>(idx)?,
        }
        Ok(())
    }

    fn write_material_index(&mut self, idx: i32) -> Result<()> {
        match self.header.material_index_size {
            1 => self.writer.write_i8(idx as i8)?,
            2 => self.writer.write_i16::<LittleEndian>(idx as i16)?,
            _ => self.writer.write_i32::<LittleEndian>(idx)?,
        }
        Ok(())
    }

    fn write_morph_index(&mut self, idx: i32) -> Result<()> {
        match self.header.morph_index_size {
            1 => self.writer.write_i8(idx as i8)?,
            2 => self.writer.write_i16::<LittleEndian>(idx as i16)?,
            _ => self.writer.write_i32::<LittleEndian>(idx)?,
        }
        Ok(())
    }

    fn write_rigid_index(&mut self, idx: i32) -> Result<()> {
        match self.header.rigid_body_index_size {
            1 => self.writer.write_i8(idx as i8)?,
            2 => self.writer.write_i16::<LittleEndian>(idx as i16)?,
            _ => self.writer.write_i32::<LittleEndian>(idx)?,
        }
        Ok(())
    }

    fn write_vec3(&mut self, v: glam::Vec3) -> Result<()> {
        self.writer.write_f32::<LittleEndian>(v.x)?;
        self.writer.write_f32::<LittleEndian>(v.y)?;
        self.writer.write_f32::<LittleEndian>(v.z)?;
        Ok(())
    }

    fn write_vec4(&mut self, v: glam::Vec4) -> Result<()> {
        self.writer.write_f32::<LittleEndian>(v.x)?;
        self.writer.write_f32::<LittleEndian>(v.y)?;
        self.writer.write_f32::<LittleEndian>(v.z)?;
        self.writer.write_f32::<LittleEndian>(v.w)?;
        Ok(())
    }

    fn write_vertices(&mut self, vertices: &[PmxVertex]) -> Result<()> {
        self.writer
            .write_i32::<LittleEndian>(vertices.len() as i32)?;
        for v in vertices {
            self.write_vec3(v.position)?;
            self.write_vec3(v.normal)?;
            self.writer.write_f32::<LittleEndian>(v.uv.x)?;
            self.writer.write_f32::<LittleEndian>(v.uv.y)?;

            match &v.weight {
                PmxWeightType::Bdef1 { bone } => {
                    self.writer.write_u8(0)?;
                    self.write_bone_index(*bone)?;
                }
                PmxWeightType::Bdef2 {
                    bone1,
                    bone2,
                    weight1,
                } => {
                    self.writer.write_u8(1)?;
                    self.write_bone_index(*bone1)?;
                    self.write_bone_index(*bone2)?;
                    self.writer.write_f32::<LittleEndian>(*weight1)?;
                }
                PmxWeightType::Bdef4 { bones, weights } => {
                    self.writer.write_u8(2)?;
                    for &b in bones {
                        self.write_bone_index(b)?;
                    }
                    for &w in weights {
                        self.writer.write_f32::<LittleEndian>(w)?;
                    }
                }
            }

            self.writer.write_f32::<LittleEndian>(v.edge_scale)?;
        }
        Ok(())
    }

    fn write_faces(&mut self, faces: &[[u32; 3]]) -> Result<()> {
        // Face count is the number of vertex references (faces * 3)
        self.writer
            .write_i32::<LittleEndian>((faces.len() * 3) as i32)?;
        for face in faces {
            for &idx in face {
                self.write_vertex_index(idx)?;
            }
        }
        Ok(())
    }

    fn write_textures(&mut self, textures: &[String]) -> Result<()> {
        self.writer
            .write_i32::<LittleEndian>(textures.len() as i32)?;
        for tex in textures {
            self.write_text(tex)?;
        }
        Ok(())
    }

    fn write_materials(&mut self, materials: &[PmxMaterial]) -> Result<()> {
        self.writer
            .write_i32::<LittleEndian>(materials.len() as i32)?;
        for mat in materials {
            self.write_text(&mat.name)?;
            self.write_text(&mat.name_en)?;
            self.write_vec4(mat.diffuse)?;
            self.write_vec3(mat.specular)?;
            self.writer.write_f32::<LittleEndian>(mat.specular_power)?;
            self.write_vec3(mat.ambient)?;
            self.writer.write_u8(mat.draw_flags)?;
            self.write_vec4(mat.edge_color)?;
            self.writer.write_f32::<LittleEndian>(mat.edge_size)?;

            // Texture index
            self.write_texture_index(mat.texture_index.unwrap_or(-1))?;
            // Sphere texture index
            self.write_texture_index(mat.sphere_texture_index.unwrap_or(-1))?;
            // Sphere mode
            self.writer.write_u8(mat.sphere_mode)?;

            // Toon reference
            match &mat.toon_ref {
                PmxToonRef::Texture(idx) => {
                    self.writer.write_u8(0)?;
                    self.write_texture_index(*idx)?;
                }
                PmxToonRef::Shared(idx) => {
                    self.writer.write_u8(1)?;
                    self.writer.write_u8(*idx)?;
                }
            }

            self.write_text(&mat.memo)?;
            self.writer
                .write_i32::<LittleEndian>(mat.face_count as i32)?;
        }
        Ok(())
    }

    fn write_bones(&mut self, bones: &[PmxBone]) -> Result<()> {
        self.writer.write_i32::<LittleEndian>(bones.len() as i32)?;
        for bone in bones {
            self.write_text(&bone.name)?;
            self.write_text(&bone.name_en)?;
            self.write_vec3(bone.position)?;
            self.write_bone_index(bone.parent_index)?;
            self.writer.write_i32::<LittleEndian>(bone.deform_layer)?;
            self.writer.write_u16::<LittleEndian>(bone.flags)?;

            // Tail connection
            match &bone.tail {
                BoneTail::Offset(off) => {
                    // flags bit0 = 0 (coord offset)
                    self.write_vec3(*off)?;
                }
                BoneTail::BoneIndex(idx) => {
                    // flags bit0 = 1 (bone index)
                    self.write_bone_index(*idx)?;
                }
            }

            // Grant (when rotation-grant or move-grant flag is set)
            if bone.flags & (BONE_FLAG_ROTATION_GRANT | BONE_FLAG_MOVE_GRANT) != 0 {
                let grant = bone.grant.as_ref();
                self.write_bone_index(grant.map(|g| g.parent_index).unwrap_or(-1))?;
                self.writer
                    .write_f32::<LittleEndian>(grant.map(|g| g.ratio).unwrap_or(1.0))?;
            }

            // IK
            if let Some(ik) = &bone.ik {
                self.write_bone_index(ik.target_bone)?;
                self.writer.write_i32::<LittleEndian>(ik.loop_count)?;
                self.writer.write_f32::<LittleEndian>(ik.limit_angle)?;
                self.writer
                    .write_i32::<LittleEndian>(ik.links.len() as i32)?;
                for link in &ik.links {
                    self.write_bone_index(link.bone_index)?;
                    self.writer.write_u8(link.angle_limit as u8)?;
                    if link.angle_limit {
                        self.write_vec3(link.limit_min)?;
                        self.write_vec3(link.limit_max)?;
                    }
                }
            }
        }
        Ok(())
    }

    fn write_morphs(&mut self, morphs: &[PmxMorph]) -> Result<()> {
        self.writer.write_i32::<LittleEndian>(morphs.len() as i32)?;
        for morph in morphs {
            self.write_text(&morph.name)?;
            self.write_text(&morph.name_en)?;
            self.writer.write_u8(morph.panel)?;
            self.writer.write_u8(morph.morph_type)?;

            match &morph.offsets {
                PmxMorphOffsets::Vertex(offsets) => {
                    self.writer
                        .write_i32::<LittleEndian>(offsets.len() as i32)?;
                    for off in offsets {
                        self.write_vertex_index(off.vertex_index)?;
                        self.write_vec3(off.offset)?;
                    }
                }
                PmxMorphOffsets::Group(offsets) => {
                    self.writer
                        .write_i32::<LittleEndian>(offsets.len() as i32)?;
                    for off in offsets {
                        self.write_morph_index(off.morph_index)?;
                        self.writer.write_f32::<LittleEndian>(off.weight)?;
                    }
                }
                PmxMorphOffsets::Bone(offsets) => {
                    self.writer
                        .write_i32::<LittleEndian>(offsets.len() as i32)?;
                    for off in offsets {
                        self.write_bone_index(off.bone_index)?;
                        self.write_vec3(off.translation)?;
                        self.writer.write_f32::<LittleEndian>(off.rotation.x)?;
                        self.writer.write_f32::<LittleEndian>(off.rotation.y)?;
                        self.writer.write_f32::<LittleEndian>(off.rotation.z)?;
                        self.writer.write_f32::<LittleEndian>(off.rotation.w)?;
                    }
                }
                PmxMorphOffsets::Material(offsets) => {
                    self.writer
                        .write_i32::<LittleEndian>(offsets.len() as i32)?;
                    for off in offsets {
                        self.write_material_index(off.material_index)?;
                        self.writer.write_u8(off.offset_mode)?;
                        self.write_vec4(off.diffuse)?;
                        self.write_vec3(off.specular)?;
                        self.writer.write_f32::<LittleEndian>(off.specular_power)?;
                        self.write_vec3(off.ambient)?;
                        self.write_vec4(off.edge_color)?;
                        self.writer.write_f32::<LittleEndian>(off.edge_size)?;
                        self.write_vec4(off.texture_factor)?;
                        self.write_vec4(off.sphere_factor)?;
                        self.write_vec4(off.toon_factor)?;
                    }
                }
                PmxMorphOffsets::Uv(offsets) => {
                    self.writer
                        .write_i32::<LittleEndian>(offsets.len() as i32)?;
                    for off in offsets {
                        self.write_vertex_index(off.vertex_index)?;
                        self.write_vec4(off.offset)?;
                    }
                }
            }
        }
        Ok(())
    }

    fn write_display_frames(&mut self, frames: &[PmxDisplayFrame]) -> Result<()> {
        self.writer.write_i32::<LittleEndian>(frames.len() as i32)?;
        for frame in frames {
            self.write_text(&frame.name)?;
            self.write_text(&frame.name_en)?;
            self.writer.write_u8(frame.is_special)?;
            self.writer
                .write_i32::<LittleEndian>(frame.elements.len() as i32)?;
            for elem in &frame.elements {
                match elem {
                    DisplayFrameElement::Bone(idx) => {
                        self.writer.write_u8(0)?;
                        self.write_bone_index(*idx)?;
                    }
                    DisplayFrameElement::Morph(idx) => {
                        self.writer.write_u8(1)?;
                        self.write_morph_index(*idx)?;
                    }
                }
            }
        }
        Ok(())
    }

    fn write_rigid_bodies(&mut self, bodies: &[PmxRigidBody]) -> Result<()> {
        self.writer.write_i32::<LittleEndian>(bodies.len() as i32)?;
        for body in bodies {
            self.write_text(&body.name)?;
            self.write_text(&body.name_en)?;
            self.write_bone_index(body.bone_index)?;
            self.writer.write_u8(body.group)?;
            self.writer
                .write_u16::<LittleEndian>(body.no_collision_mask)?;
            self.writer.write_u8(body.shape)?;
            self.write_vec3(body.size)?;
            self.write_vec3(body.position)?;
            self.write_vec3(body.rotation)?;
            self.writer.write_f32::<LittleEndian>(body.mass)?;
            self.writer.write_f32::<LittleEndian>(body.linear_damping)?;
            self.writer
                .write_f32::<LittleEndian>(body.angular_damping)?;
            self.writer.write_f32::<LittleEndian>(body.restitution)?;
            self.writer.write_f32::<LittleEndian>(body.friction)?;
            self.writer.write_u8(body.physics_mode)?;
        }
        Ok(())
    }

    fn write_joints(&mut self, joints: &[PmxJoint]) -> Result<()> {
        self.writer.write_i32::<LittleEndian>(joints.len() as i32)?;
        for joint in joints {
            self.write_text(&joint.name)?;
            self.write_text(&joint.name_en)?;
            self.writer.write_u8(joint.joint_type)?;
            self.write_rigid_index(joint.rigid_a)?;
            self.write_rigid_index(joint.rigid_b)?;
            self.write_vec3(joint.position)?;
            self.write_vec3(joint.rotation)?;
            self.write_vec3(joint.move_limit_lo)?;
            self.write_vec3(joint.move_limit_hi)?;
            self.write_vec3(joint.rot_limit_lo)?;
            self.write_vec3(joint.rot_limit_hi)?;
            self.write_vec3(joint.spring_move)?;
            self.write_vec3(joint.spring_rot)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pmx_write_read_roundtrip() {
        let Some(sample) = crate::test_util::try_test_file(crate::test_util::seed_san_pmx()) else {
            return;
        };
        let original = crate::pmx::reader::read_pmx(&sample).expect("PMX読み込み失敗");

        let temp_path = std::env::temp_dir().join("popone_test_roundtrip.pmx");
        {
            let file = std::fs::File::create(&temp_path).expect("一時ファイル作成失敗");
            let writer = std::io::BufWriter::new(file);
            let header = original.header.clone();
            let mut pmx_writer = PmxWriter::new(writer, header);
            pmx_writer.write_model(&original).expect("PMX書き込み失敗");
        }

        let reloaded = crate::pmx::reader::read_pmx(&temp_path).expect("再読み込み失敗");

        // Verify basic property match
        assert_eq!(original.bones.len(), reloaded.bones.len(), "ボーン数不一致");
        assert_eq!(
            original.vertices.len(),
            reloaded.vertices.len(),
            "頂点数不一致"
        );
        assert_eq!(
            original.materials.len(),
            reloaded.materials.len(),
            "材質数不一致"
        );
        assert_eq!(
            original.morphs.len(),
            reloaded.morphs.len(),
            "モーフ数不一致"
        );
        assert_eq!(
            original.textures.len(),
            reloaded.textures.len(),
            "テクスチャ数不一致"
        );
        assert_eq!(
            original.rigid_bodies.len(),
            reloaded.rigid_bodies.len(),
            "剛体数不一致"
        );
        assert_eq!(
            original.joints.len(),
            reloaded.joints.len(),
            "ジョイント数不一致"
        );

        // Bone name match
        for (i, (a, b)) in original.bones.iter().zip(reloaded.bones.iter()).enumerate() {
            assert_eq!(a.name, b.name, "ボーン[{i}]名不一致");
        }

        let _ = std::fs::remove_file(&temp_path);
    }
}
