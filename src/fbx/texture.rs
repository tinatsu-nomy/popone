use std::path::Path;
use super::scene::FbxScene;

pub struct TextureData {
    pub name: String,
    pub rgba: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

/// Extract diffuse texture data for a material via scene graph connections
pub fn extract_texture_for_material(
    scene: &FbxScene,
    mat_id: i64,
    fbx_path: Option<&Path>,
) -> Option<TextureData> {
    let textures = scene.textures_for_material(mat_id);

    // Prefer diffuse texture, fall back to first available
    let (tex_obj, _prop) = textures
        .iter()
        .find(|(_, prop)| {
            prop.as_ref()
                .map(|p| p.contains("Diffuse") || p.contains("diffuse"))
                .unwrap_or(false)
        })
        .or_else(|| textures.first())?;

    let tex_name = tex_obj.name.clone();

    // Try embedded Video content first
    if let Some(video) = scene.video_for_texture(tex_obj.id) {
        if let Some(content) = video.node.child("Content") {
            if let Some(data) = content.properties.first().and_then(|p| p.as_binary()) {
                if !data.is_empty() {
                    if let Some(tex) = decode_image_data(data, &tex_name) {
                        return Some(tex);
                    }
                }
            }
        }
    }

    // Fallback: external file
    let fbx_dir = fbx_path.and_then(|p| p.parent())?;

    // Try RelativeFilename
    if let Some(rel_node) = tex_obj.node.child("RelativeFilename") {
        if let Some(filename) = rel_node.properties.first().and_then(|p| p.as_string()) {
            let path = fbx_dir.join(filename.replace('\\', "/"));
            if let Ok(data) = std::fs::read(&path) {
                if let Some(tex) = decode_image_data(&data, &tex_name) {
                    return Some(tex);
                }
            }
        }
    }

    // Try FileName (extract basename, look in fbx dir)
    if let Some(abs_node) = tex_obj.node.child("FileName") {
        if let Some(filename) = abs_node.properties.first().and_then(|p| p.as_string()) {
            let normalized = filename.replace('\\', "/");
            let basename = Path::new(&normalized)
                .file_name()
                .unwrap_or_default();
            let path = fbx_dir.join(basename);
            if let Ok(data) = std::fs::read(&path) {
                if let Some(tex) = decode_image_data(&data, &tex_name) {
                    return Some(tex);
                }
            }
        }
    }

    None
}

fn decode_image_data(data: &[u8], name: &str) -> Option<TextureData> {
    match image::load_from_memory(data) {
        Ok(img) => {
            let rgba = img.to_rgba8();
            let width = rgba.width();
            let height = rgba.height();
            Some(TextureData {
                name: name.to_string(),
                rgba: rgba.into_raw(),
                width,
                height,
            })
        }
        Err(e) => {
            log::warn!("Failed to decode texture '{}': {}", name, e);
            None
        }
    }
}
