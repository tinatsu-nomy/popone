use super::scene::FbxScene;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

pub struct TextureData {
    pub name: String,
    pub rgba: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

/// FBX 近傍テクスチャ検索キャッシュ
/// 初回アクセス時に FBX 親ディレクトリ以下を走査し、basename(小文字)→パスのマップを構築
pub struct TextureSearchCache {
    map: Option<HashMap<String, PathBuf>>,
}

impl TextureSearchCache {
    pub fn new() -> Self {
        Self { map: None }
    }

    fn get_or_build(&mut self, fbx_dir: &Path) -> &HashMap<String, PathBuf> {
        if self.map.is_none() {
            let search_root = fbx_dir.parent().unwrap_or(fbx_dir);
            let mut map = HashMap::new();
            collect_files(search_root, &mut map, 0);
            log::debug!(
                "Texture search cache built: {} files (root={})",
                map.len(),
                search_root.display()
            );
            self.map = Some(map);
        }
        self.map
            .as_ref()
            .expect("map は直前の is_none 分岐で必ず初期化済み")
    }

    fn lookup(&mut self, fbx_dir: &Path, basename: &str) -> Option<PathBuf> {
        let key = basename.to_lowercase();
        self.get_or_build(fbx_dir).get(&key).cloned()
    }
}

fn collect_files(dir: &Path, map: &mut HashMap<String, PathBuf>, depth: u8) {
    if depth > 3 {
        return;
    }
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let ft = match entry.file_type() {
            Ok(ft) => ft,
            Err(_) => continue,
        };
        if ft.is_file() {
            if let Some(name) = entry.file_name().to_str() {
                let key = name.to_lowercase();
                // 画像ファイルのみキャッシュ
                if matches!(
                    Path::new(&key).extension().and_then(|e| e.to_str()),
                    Some("png" | "jpg" | "jpeg" | "tga" | "bmp" | "dds" | "psd" | "tif" | "tiff")
                ) {
                    map.entry(key).or_insert_with(|| entry.path());
                }
            }
        } else if ft.is_dir() {
            collect_files(&entry.path(), map, depth + 1);
        }
    }
}

/// テクスチャリストから Diffuse テクスチャを選択（共通ロジック）
fn find_diffuse_texture<'a>(
    textures: &[(&'a super::scene::FbxObject<'a>, Option<String>)],
) -> Option<&'a super::scene::FbxObject<'a>> {
    textures
        .iter()
        .find(|(_, prop)| {
            prop.as_ref()
                .map(|p| p.contains("Diffuse") || p.contains("diffuse"))
                .unwrap_or(false)
        })
        .or_else(|| textures.first())
        .map(|(obj, _)| *obj)
}

/// テクスチャノードからファイル名（ベースネーム）を取得
fn extract_basename_from_texture(tex_obj: &super::scene::FbxObject) -> Option<String> {
    for child_name in &["RelativeFilename", "FileName"] {
        if let Some(node) = tex_obj.node.child(child_name) {
            if let Some(filename) = node.properties.first().and_then(|p| p.as_string()) {
                let normalized = filename.replace('\\', "/");
                if let Some(basename) = Path::new(&normalized).file_name() {
                    return Some(basename.to_string_lossy().into_owned());
                }
            }
        }
    }
    None
}

/// Extract diffuse texture data for a material via scene graph connections
pub fn extract_texture_for_material(
    scene: &FbxScene,
    mat_id: i64,
    fbx_path: Option<&Path>,
    search_cache: &mut TextureSearchCache,
) -> Option<TextureData> {
    let textures = scene.textures_for_material(mat_id);
    let tex_obj = find_diffuse_texture(&textures)?;

    // テクスチャ名: 実ファイル名を優先し、無ければ FBX オブジェクト名
    let file_basename = extract_basename_from_texture(tex_obj);
    let tex_name = file_basename
        .as_deref()
        .map(|b| {
            Path::new(b)
                .file_stem()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned()
        })
        .unwrap_or_else(|| tex_obj.name.clone());

    // 拡張子ヒント（ファイル名から抽出、埋め込み・外部ファイル両方で使用）
    let ext_owned: Option<String> = file_basename
        .as_deref()
        .and_then(|b| Path::new(b).extension())
        .and_then(|e| e.to_str())
        .map(|s| s.to_owned());
    let ext_hint = ext_owned.as_deref();

    // Try embedded Video content first (binary FBX only)
    // ASCII FBX の Content はテキスト表現のため画像デコードできない → 外部ファイルフォールバックに委ねる
    if let Some(video) = scene.video_for_texture(tex_obj.id) {
        if let Some(content) = video.node.child("Content") {
            if let Some(data) = content.properties.first().and_then(|p| p.as_binary()) {
                if !data.is_empty() {
                    if let Some(tex) = decode_image_data_with_ext(data, &tex_name, ext_hint) {
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
                if let Some(tex) = decode_image_data_with_ext(&data, &tex_name, ext_hint) {
                    return Some(tex);
                }
            }
        }
    }

    // Try FileName (extract basename, look in fbx dir)
    if let Some(abs_node) = tex_obj.node.child("FileName") {
        if let Some(filename) = abs_node.properties.first().and_then(|p| p.as_string()) {
            let normalized = filename.replace('\\', "/");
            let basename = Path::new(&normalized).file_name().unwrap_or_default();
            let path = fbx_dir.join(basename);
            if let Ok(data) = std::fs::read(&path) {
                if let Some(tex) = decode_image_data_with_ext(&data, &tex_name, ext_hint) {
                    return Some(tex);
                }
            }
        }
    }

    // Fallback: basename で FBX 近傍ディレクトリをキャッシュ検索
    // Unity/Blender エクスポート FBX で RelativeFilename のパスが実際のディレクトリ構造と異なる場合に対応
    let basename = file_basename?;
    if let Some(found) = search_cache.lookup(fbx_dir, &basename) {
        log::info!(
            "Texture '{}' found by proximity search: {}",
            basename,
            found.display()
        );
        if let Ok(data) = std::fs::read(&found) {
            if let Some(tex) = decode_image_data_with_ext(&data, &tex_name, ext_hint) {
                return Some(tex);
            }
        }
        log::warn!("Texture '{}' exists as file but decoding failed", basename);
        return None;
    }

    log::warn!(
        "Texture '{}' not found (searched near FBX directory)",
        basename
    );
    None
}

/// Extract the texture reference filename for a material (without loading the file)
pub fn extract_texture_name_for_material(scene: &FbxScene, mat_id: i64) -> Option<String> {
    let textures = scene.textures_for_material(mat_id);
    let tex_obj = find_diffuse_texture(&textures)?;
    extract_basename_from_texture(tex_obj).or_else(|| Some(tex_obj.name.clone()))
}

fn decode_image_data_with_ext(
    data: &[u8],
    name: &str,
    ext_hint: Option<&str>,
) -> Option<TextureData> {
    // PSD: image crate は PSD 未対応のため、自前デコーダーで先に処理
    if crate::psd::is_psd_filename(name)
        || ext_hint
            .map(|e| e.eq_ignore_ascii_case("psd"))
            .unwrap_or(false)
    {
        match crate::psd::decode_psd(data) {
            Ok((rgba, width, height)) => {
                return Some(TextureData {
                    name: name.to_string(),
                    rgba,
                    width,
                    height,
                });
            }
            Err(e) => {
                log::warn!("PSD decode failed '{}': {}", name, e);
                return None;
            }
        }
    }

    // まず自動判別を試行
    match image::load_from_memory(data) {
        Ok(img) => {
            let rgba = img.to_rgba8();
            let width = rgba.width();
            let height = rgba.height();
            return Some(TextureData {
                name: name.to_string(),
                rgba: rgba.into_raw(),
                width,
                height,
            });
        }
        Err(_) => {}
    }

    // TGA 等マジックナンバーのない形式は拡張子からフォーマットを推定してリトライ
    let ext = ext_hint.or_else(|| Path::new(name).extension().and_then(|e| e.to_str()));
    if let Some(ext) = ext {
        if let Some(format) = image::ImageFormat::from_extension(ext) {
            match image::load_from_memory_with_format(data, format) {
                Ok(img) => {
                    let rgba = img.to_rgba8();
                    let width = rgba.width();
                    let height = rgba.height();
                    return Some(TextureData {
                        name: name.to_string(),
                        rgba: rgba.into_raw(),
                        width,
                        height,
                    });
                }
                Err(e) => {
                    log::warn!(
                        "Texture '{}' decode failed (format={:?}): {}",
                        name,
                        format,
                        e
                    );
                    return None;
                }
            }
        }
    }

    log::warn!("Cannot determine format of texture '{}'", name);
    None
}
