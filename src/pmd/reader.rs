use anyhow::{bail, Result};
use byteorder::{LittleEndian, ReadBytesExt};
use glam::{Vec2, Vec3, Vec4};
use std::io::Read;

use super::types::*;

/// Shift_JIS (cp932) の固定長バイト列を UTF-8 文字列に変換
fn decode_sjis(buf: &[u8]) -> String {
    // NUL終端を探す
    let len = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());
    let (cow, _, _) = encoding_rs::SHIFT_JIS.decode(&buf[..len]);
    cow.into_owned()
}

fn read_vec3<R: Read>(r: &mut R) -> Result<Vec3> {
    let x = r.read_f32::<LittleEndian>()?;
    let y = r.read_f32::<LittleEndian>()?;
    let z = r.read_f32::<LittleEndian>()?;
    Ok(Vec3::new(x, y, z))
}

/// バイト列から PMD を読み込む（オンメモリキャッシュ用）
pub fn read_pmd_from_data(data: &[u8]) -> Result<PmdModel> {
    let cursor = std::io::Cursor::new(data);
    let mut r = std::io::BufReader::new(cursor);
    read_pmd_inner(&mut r)
}

pub fn read_pmd(path: &std::path::Path) -> Result<PmdModel> {
    let file = std::fs::File::open(path)?;
    let mut r = std::io::BufReader::new(file);
    read_pmd_inner(&mut r)
}

fn read_pmd_inner<R: Read>(mut r: &mut R) -> Result<PmdModel> {

    // ヘッダ: "Pmd" + version(float) + name(20) + comment(256)
    let mut magic = [0u8; 3];
    r.read_exact(&mut magic)?;
    if &magic != b"Pmd" {
        bail!("PMDマジックナンバーが不正: {:?}", magic);
    }
    let version = r.read_f32::<LittleEndian>()?;
    if version < 1.0 {
        bail!("未対応のPMDバージョン: {}", version);
    }

    let mut name_buf = [0u8; 20];
    r.read_exact(&mut name_buf)?;
    let mut comment_buf = [0u8; 256];
    r.read_exact(&mut comment_buf)?;
    let header = PmdHeader {
        name: decode_sjis(&name_buf),
        comment: decode_sjis(&comment_buf),
    };

    // 頂点
    let vertex_count = r.read_u32::<LittleEndian>()? as usize;
    let mut vertices = Vec::with_capacity(vertex_count);
    for _ in 0..vertex_count {
        let position = read_vec3(&mut r)?;
        let normal = read_vec3(&mut r)?;
        let u = r.read_f32::<LittleEndian>()?;
        let v = r.read_f32::<LittleEndian>()?;
        let bone1 = r.read_u16::<LittleEndian>()?;
        let bone2 = r.read_u16::<LittleEndian>()?;
        let weight = r.read_u8()?;
        let edge_flag = r.read_u8()?;
        vertices.push(PmdVertex {
            position,
            normal,
            uv: Vec2::new(u, v),
            bone1,
            bone2,
            weight,
            edge_flag,
        });
    }

    // 面
    let face_index_count = r.read_u32::<LittleEndian>()? as usize;
    if !face_index_count.is_multiple_of(3) {
        bail!("PMD面インデックス数が3の倍数でない: {}", face_index_count);
    }
    let face_count = face_index_count / 3;
    let mut faces = Vec::with_capacity(face_count);
    for _ in 0..face_count {
        let a = r.read_u16::<LittleEndian>()?;
        let b = r.read_u16::<LittleEndian>()?;
        let c = r.read_u16::<LittleEndian>()?;
        faces.push([a, b, c]);
    }

    // 材質
    let material_count = r.read_u32::<LittleEndian>()? as usize;
    let mut materials = Vec::with_capacity(material_count);
    for _ in 0..material_count {
        let dr = r.read_f32::<LittleEndian>()?;
        let dg = r.read_f32::<LittleEndian>()?;
        let db = r.read_f32::<LittleEndian>()?;
        let da = r.read_f32::<LittleEndian>()?;
        let specular_power = r.read_f32::<LittleEndian>()?;
        let specular = read_vec3(&mut r)?;
        let ambient = read_vec3(&mut r)?;
        let toon_index = r.read_u8()?;
        let edge_flag = r.read_u8()?;
        let face_count = r.read_u32::<LittleEndian>()?;
        let mut tex_buf = [0u8; 20];
        r.read_exact(&mut tex_buf)?;
        let texture_name = decode_sjis(&tex_buf);

        materials.push(PmdMaterial {
            diffuse: Vec4::new(dr, dg, db, da),
            specular_power,
            specular,
            ambient,
            toon_index,
            edge_flag,
            face_count,
            texture_name,
        });
    }

    // ボーン
    let bone_count = r.read_u16::<LittleEndian>()? as usize;
    let mut bones = Vec::with_capacity(bone_count);
    for _ in 0..bone_count {
        let mut name_buf = [0u8; 20];
        r.read_exact(&mut name_buf)?;
        let parent = r.read_u16::<LittleEndian>()?;
        let child = r.read_u16::<LittleEndian>()?;
        let bone_type = r.read_u8()?;
        let ik_parent = r.read_u16::<LittleEndian>()?;
        let position = read_vec3(&mut r)?;
        bones.push(PmdBone {
            name: decode_sjis(&name_buf),
            parent,
            child,
            bone_type,
            ik_parent,
            position,
        });
    }

    // IK
    let ik_count = r.read_u16::<LittleEndian>()? as usize;
    let mut ik_list = Vec::with_capacity(ik_count);
    for _ in 0..ik_count {
        let bone_index = r.read_u16::<LittleEndian>()?;
        let target_bone = r.read_u16::<LittleEndian>()?;
        let chain_length = r.read_u8()?;
        let iterations = r.read_u16::<LittleEndian>()?;
        let limit_angle = r.read_f32::<LittleEndian>()?;
        let mut chain = Vec::with_capacity(chain_length as usize);
        for _ in 0..chain_length {
            chain.push(r.read_u16::<LittleEndian>()?);
        }
        ik_list.push(PmdIk {
            bone_index,
            target_bone,
            chain_length,
            iterations,
            limit_angle,
            chain,
        });
    }

    // モーフ（表情）
    let morph_count = r.read_u16::<LittleEndian>()? as usize;
    let mut morphs = Vec::with_capacity(morph_count);
    for _ in 0..morph_count {
        let mut name_buf = [0u8; 20];
        r.read_exact(&mut name_buf)?;
        let vertex_count = r.read_u32::<LittleEndian>()?;
        let morph_type = r.read_u8()?;
        let mut verts = Vec::with_capacity(vertex_count as usize);
        for _ in 0..vertex_count {
            let index = r.read_u32::<LittleEndian>()?;
            let offset = read_vec3(&mut r)?;
            verts.push(PmdMorphVertex { index, offset });
        }
        morphs.push(PmdMorph {
            name: decode_sjis(&name_buf),
            vertex_count,
            morph_type,
            vertices: verts,
        });
    }

    // 表情表示枠
    let morph_display_count = r.read_u8()? as usize;
    let mut morph_display = Vec::with_capacity(morph_display_count);
    for _ in 0..morph_display_count {
        morph_display.push(r.read_u16::<LittleEndian>()?);
    }

    // ボーン表示枠名
    let bone_display_name_count = r.read_u8()? as usize;
    let mut bone_display_names = Vec::with_capacity(bone_display_name_count);
    for _ in 0..bone_display_name_count {
        let mut buf = [0u8; 50];
        r.read_exact(&mut buf)?;
        bone_display_names.push(decode_sjis(&buf));
    }

    // ボーン表示枠
    let bone_display_count = r.read_u32::<LittleEndian>()? as usize;
    let mut bone_display = Vec::with_capacity(bone_display_count);
    for _ in 0..bone_display_count {
        let bone_idx = r.read_u16::<LittleEndian>()?;
        let frame_idx = r.read_u8()?;
        bone_display.push((bone_idx, frame_idx));
    }

    // 英語ヘッダ（オプション）
    let english_header = read_english_header(&mut r, bone_count, morph_count, bone_display_name_count).ok();

    // トゥーンテクスチャ
    let mut toon_textures = std::array::from_fn(|_| String::new());
    if read_toon_textures(&mut r, &mut toon_textures).is_err() {
        // トゥーンテクスチャがない古いPMDファイルもある
    }

    // 剛体（物理拡張、ファイル末尾にオプション）
    let mut rigid_bodies = Vec::new();
    let mut joints = Vec::new();

    if let Ok(rb_count) = r.read_u32::<LittleEndian>() {
        rigid_bodies.reserve(rb_count as usize);
        for _ in 0..rb_count {
            let mut name_buf = [0u8; 20];
            r.read_exact(&mut name_buf)?;
            let bone_index = r.read_u16::<LittleEndian>()?;
            let group = r.read_u8()?;
            let no_collision_mask = r.read_u16::<LittleEndian>()?;
            let shape = r.read_u8()?;
            let size = read_vec3(&mut r)?;
            let position = read_vec3(&mut r)?;
            let rotation = read_vec3(&mut r)?;
            let mass = r.read_f32::<LittleEndian>()?;
            let linear_damping = r.read_f32::<LittleEndian>()?;
            let angular_damping = r.read_f32::<LittleEndian>()?;
            let restitution = r.read_f32::<LittleEndian>()?;
            let friction = r.read_f32::<LittleEndian>()?;
            let physics_mode = r.read_u8()?;
            rigid_bodies.push(PmdRigidBody {
                name: decode_sjis(&name_buf),
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

        // ジョイント
        if let Ok(joint_count) = r.read_u32::<LittleEndian>() {
            joints.reserve(joint_count as usize);
            for _ in 0..joint_count {
                let mut name_buf = [0u8; 20];
                r.read_exact(&mut name_buf)?;
                let rigid_a = r.read_u32::<LittleEndian>()?;
                let rigid_b = r.read_u32::<LittleEndian>()?;
                let position = read_vec3(&mut r)?;
                let rotation = read_vec3(&mut r)?;
                let move_limit_lo = read_vec3(&mut r)?;
                let move_limit_hi = read_vec3(&mut r)?;
                let rot_limit_lo = read_vec3(&mut r)?;
                let rot_limit_hi = read_vec3(&mut r)?;
                let spring_move = read_vec3(&mut r)?;
                let spring_rot = read_vec3(&mut r)?;
                joints.push(PmdJoint {
                    name: decode_sjis(&name_buf),
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
        }
    }

    Ok(PmdModel {
        header,
        vertices,
        faces,
        materials,
        bones,
        ik_list,
        morphs,
        morph_display,
        bone_display_names,
        bone_display,
        toon_textures,
        rigid_bodies,
        joints,
        english_header,
    })
}

fn read_english_header<R: Read>(
    r: &mut R,
    bone_count: usize,
    morph_count: usize,
    display_count: usize,
) -> Result<PmdEnglishHeader> {
    let flag = r.read_u8()?;
    if flag == 0 {
        bail!("英語ヘッダなし");
    }

    let mut name_buf = [0u8; 20];
    r.read_exact(&mut name_buf)?;
    let mut comment_buf = [0u8; 256];
    r.read_exact(&mut comment_buf)?;

    let mut bone_names = Vec::with_capacity(bone_count);
    for _ in 0..bone_count {
        let mut buf = [0u8; 20];
        r.read_exact(&mut buf)?;
        bone_names.push(decode_sjis(&buf));
    }

    // base モーフを除いた数
    let en_morph_count = if morph_count > 0 { morph_count - 1 } else { 0 };
    let mut morph_names = Vec::with_capacity(en_morph_count);
    for _ in 0..en_morph_count {
        let mut buf = [0u8; 20];
        r.read_exact(&mut buf)?;
        morph_names.push(decode_sjis(&buf));
    }

    let mut display_names = Vec::with_capacity(display_count);
    for _ in 0..display_count {
        let mut buf = [0u8; 50];
        r.read_exact(&mut buf)?;
        display_names.push(decode_sjis(&buf));
    }

    Ok(PmdEnglishHeader {
        name: decode_sjis(&name_buf),
        comment: decode_sjis(&comment_buf),
        bone_names,
        morph_names,
        display_names,
    })
}

fn read_toon_textures<R: Read>(r: &mut R, out: &mut [String; 10]) -> Result<()> {
    for item in out.iter_mut() {
        let mut buf = [0u8; 100];
        r.read_exact(&mut buf)?;
        *item = decode_sjis(&buf);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_read_miku_v2_pmd() {
        let Some(path) = crate::test_util::try_test_file(crate::test_util::miku_v2_pmd()) else { return; };

        let model = read_pmd(&path).expect("PMD読み込みに失敗");

        assert_eq!(model.header.name, "初音ミク");
        assert_eq!(model.bones.len(), 140);
        assert_eq!(model.vertices.len(), 12354);
        assert!(!model.morphs.is_empty());
        assert_eq!(model.rigid_bodies.len(), 45);
        assert_eq!(model.joints.len(), 27);

        // 面数合計 = 材質のface_count合計 / 3
        let total_face_verts: u32 = model.materials.iter().map(|m| m.face_count).sum();
        assert_eq!(total_face_verts as usize / 3, model.faces.len());
    }
}
