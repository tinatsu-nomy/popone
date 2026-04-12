//! ray-mmd 2.0 `.fx` マテリアル生成 (§K)
//!
//! 標準 ray-mmd 2.0 は `CUSTOM_ENABLE` + `customA` + `customB` で材質種別を表現する。
//! 本モジュールはカテゴリ推定 → `.fx` テンプレート生成 → `#include` 相対パス解決を行う。

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use crate::intermediate::types::{IrMaterial, IrModel, TextureData};

// ---------------------------------------------------------------------------
// K.1 型定義
// ---------------------------------------------------------------------------

/// ray-mmd 材質カテゴリ（CUSTOM_ENABLE ベース、TODO-5 で値を固定）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RayMmdMaterialKind {
    /// CUSTOM_ENABLE 0 (plastic/generic) — material_2.0.fx
    Standard,
    /// CUSTOM_ENABLE 1 (subsurface) — Materials/Skin/material_skin.fx
    Skin,
    /// CUSTOM_ENABLE 5 — Materials/Cloth/material_cloth.fx
    Cloth,
    /// CUSTOM_ENABLE 3 (anisotropic) — Materials/Editor/Anisotropic/*.fx
    HairAniso,
    /// CUSTOM_ENABLE 4 — Materials/Transparent/material_glass.fx
    Glass,
    /// CUSTOM_ENABLE 6 — Materials/ClearCoat/material_metal_clearcoat.fx
    ClearCoat,
    /// emissive 専用経路
    Emissive,
}

impl RayMmdMaterialKind {
    pub const ALL: [Self; 7] = [
        Self::Standard,
        Self::Skin,
        Self::Cloth,
        Self::HairAniso,
        Self::Glass,
        Self::ClearCoat,
        Self::Emissive,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Standard => "Standard",
            Self::Skin => "Skin (SSS)",
            Self::Cloth => "Cloth",
            Self::HairAniso => "Hair (Aniso)",
            Self::Glass => "Glass",
            Self::ClearCoat => "ClearCoat",
            Self::Emissive => "Emissive",
        }
    }

    /// `CUSTOM_ENABLE` の値。Emissive は特殊経路なので None。
    pub fn custom_enable(self) -> Option<u32> {
        match self {
            Self::Standard => Some(0),
            Self::Skin => Some(1),
            Self::HairAniso => Some(3),
            Self::Glass => Some(4),
            Self::Cloth => Some(5),
            Self::ClearCoat => Some(6),
            Self::Emissive => None,
        }
    }
}

// ---------------------------------------------------------------------------
// K.3 カテゴリ推定ヒューリスティック
// ---------------------------------------------------------------------------

/// 材質名とパラメータからカテゴリを推定する。
///
/// | キーワード（大小無視） | カテゴリ |
/// |---|---|
/// | skin / body / face / hada / 肌 / 顔 / 体 | Skin |
/// | hair / kami / 髪 | HairAniso |
/// | cloth / fuku / 服 / skirt / shirt / dress | Cloth |
/// | glass / eye / pupil / iris / me / 目 / 瞳 / 水 | Glass |
/// | （emissive_factor が非ゼロ） | Emissive |
/// | （上記未該当） | Standard |
pub fn guess_ray_mmd_kind(mat: &IrMaterial) -> RayMmdMaterialKind {
    let name_lower = mat.name.to_lowercase();

    // キーワードマッチ（優先順位順）
    let skin_keywords = ["skin", "body", "face", "hada", "肌", "顔", "体"];
    let hair_keywords = ["hair", "kami", "髪"];
    let cloth_keywords = ["cloth", "fuku", "服", "skirt", "shirt", "dress"];
    // review_022 [P2-1]: "me" は "metal"/"frame"/"smile" 等に誤爆するため削除。
    let glass_keywords = ["glass", "eye", "pupil", "iris", "目", "瞳", "水"];

    for kw in &skin_keywords {
        if name_lower.contains(kw) {
            return RayMmdMaterialKind::Skin;
        }
    }
    for kw in &hair_keywords {
        if name_lower.contains(kw) {
            return RayMmdMaterialKind::HairAniso;
        }
    }
    for kw in &cloth_keywords {
        if name_lower.contains(kw) {
            return RayMmdMaterialKind::Cloth;
        }
    }
    for kw in &glass_keywords {
        if name_lower.contains(kw) {
            return RayMmdMaterialKind::Glass;
        }
    }

    // emissive_factor が非ゼロ → Emissive
    if mat.emissive_factor.length_squared() > 1e-6 {
        return RayMmdMaterialKind::Emissive;
    }

    RayMmdMaterialKind::Standard
}

// ---------------------------------------------------------------------------
// K.2 #include 相対パス解決
// ---------------------------------------------------------------------------

/// ray-mmd ルートと出力先ディレクトリから `material_common_2.0.fxsub` への相対パスを計算。
/// 相対パス計算に失敗した場合（ドライブ跨ぎ・相対/絶対混在等）は絶対パスをフォールバック。
pub fn resolve_include_path(ray_mmd_root: &Path, mme_output_dir: &Path) -> PathBuf {
    let common_fxsub = ray_mmd_root
        .join("Materials")
        .join("material_common_2.0.fxsub");
    // 両方を絶対パスに正規化してから diff を試みる
    let abs_fxsub = dunce::canonicalize(&common_fxsub)
        .or_else(|_| std::fs::canonicalize(&common_fxsub))
        .unwrap_or_else(|_| common_fxsub.clone());
    let abs_output = dunce::canonicalize(mme_output_dir)
        .or_else(|_| std::fs::canonicalize(mme_output_dir))
        .unwrap_or_else(|_| mme_output_dir.to_path_buf());
    pathdiff::diff_paths(&abs_fxsub, &abs_output).unwrap_or(common_fxsub)
}

// ---------------------------------------------------------------------------
// K.4 ファイル名サニタイズ
// ---------------------------------------------------------------------------

/// 材質名から `.fx` ファイル名を生成（サニタイズ + 重複回避 + Windows 予約名弾き）
/// review_022 [P2-2]: `used` を lowercase ベースで衝突判定する。
/// Windows ではファイル名の大小が区別されないため、`"Body"` と `"body"` が同時生成
/// されると上書きになる。`used` には lowercase 化した文字列を入れて判定する。
pub fn make_fx_filename(mat_name: &str, used: &mut HashSet<String>) -> String {
    let base = sanitize_material_name(mat_name);
    let base = if is_windows_reserved(&base) {
        format!("{}_mat", base)
    } else {
        base
    };
    let base = if base.is_empty() {
        "unnamed".to_string()
    } else {
        base
    };
    let mut candidate = format!("material_{}.fx", base);
    let mut suffix = 2u32;
    while used.contains(&candidate.to_ascii_lowercase()) {
        candidate = format!("material_{}_{}.fx", base, suffix);
        suffix += 1;
    }
    used.insert(candidate.to_ascii_lowercase());
    candidate
}

fn sanitize_material_name(name: &str) -> String {
    name.chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '-')
        .take(64)
        .collect()
}

fn is_windows_reserved(name: &str) -> bool {
    let upper = name.to_uppercase();
    matches!(
        upper.as_str(),
        "CON"
            | "PRN"
            | "AUX"
            | "NUL"
            | "COM1"
            | "COM2"
            | "COM3"
            | "COM4"
            | "COM5"
            | "COM6"
            | "COM7"
            | "COM8"
            | "COM9"
            | "LPT1"
            | "LPT2"
            | "LPT3"
            | "LPT4"
            | "LPT5"
            | "LPT6"
            | "LPT7"
            | "LPT8"
            | "LPT9"
    )
}

// ---------------------------------------------------------------------------
// K.4 .fx ジェネレータ
// ---------------------------------------------------------------------------

/// 1 材質分の `.fx` ファイル内容を生成する（全パラメータをデフォルト値付きで展開）。
///
/// 戻り値は Shift-JIS + CR+LF エンコード済みバイト列。
pub fn generate_fx(
    mat: &IrMaterial,
    kind: RayMmdMaterialKind,
    include_path: &Path,
    support_textures: &std::collections::HashMap<usize, PathBuf>,
) -> Vec<u8> {
    let mut fx = String::with_capacity(2048);
    let mat_label = if mat.name.is_empty() {
        "(unnamed)"
    } else {
        &mat.name
    };

    // ヘッダコメント
    ln(&mut fx, &format!("// {}", mat_label));
    ln(
        &mut fx,
        &format!(
            "// カテゴリ: {} (CUSTOM_ENABLE {})",
            kind.label(),
            kind.custom_enable()
                .map_or("N/A".to_string(), |v| v.to_string())
        ),
    );
    ln(&mut fx, "// Generated by popone");
    ln(&mut fx, "");

    // ===== Albedo =====
    ln(&mut fx, "// ----- Albedo -----");
    let has_base_tex = mat.texture_index.is_some();
    ln(
        &mut fx,
        &format!(
            "#define ALBEDO_MAP_FROM {}",
            if has_base_tex { 3 } else { 0 }
        ),
    );
    ln(&mut fx, "#define ALBEDO_MAP_UV_FLIP 0");
    ln(&mut fx, "#define ALBEDO_MAP_APPLY_SCALE 0");
    ln(
        &mut fx,
        &format!(
            "#define ALBEDO_MAP_APPLY_DIFFUSE {}",
            if has_base_tex { 1 } else { 0 }
        ),
    );
    ln(&mut fx, "#define ALBEDO_MAP_APPLY_MORPH_COLOR 0");
    ln(
        &mut fx,
        &format!(
            "const float3 albedo = float3({:.4}, {:.4}, {:.4});",
            mat.diffuse.x, mat.diffuse.y, mat.diffuse.z
        ),
    );
    ln(&mut fx, "const float2 albedoMapLoopNum = float2(1.0, 1.0);");
    ln(&mut fx, "");

    // ===== SubAlbedo =====
    ln(&mut fx, "// ----- SubAlbedo -----");
    ln(&mut fx, "#define ALBEDO_SUB_ENABLE 0");
    ln(&mut fx, "#define ALBEDO_SUB_MAP_FROM 0");
    ln(&mut fx, "#define ALBEDO_SUB_MAP_UV_FLIP 0");
    ln(&mut fx, "#define ALBEDO_SUB_MAP_APPLY_SCALE 0");
    ln(&mut fx, "const float3 albedoSub = float3(1.0, 1.0, 1.0);");
    ln(
        &mut fx,
        "const float2 albedoSubMapLoopNum = float2(1.0, 1.0);",
    );
    ln(&mut fx, "");

    // ===== Alpha =====
    ln(&mut fx, "// ----- Alpha -----");
    ln(
        &mut fx,
        &format!("const float alpha = {:.4};", mat.diffuse.w),
    );
    ln(&mut fx, "#define ALPHA_MAP_FROM 0");
    ln(&mut fx, "#define ALPHA_MAP_UV_FLIP 0");
    ln(&mut fx, "#define ALPHA_MAP_SWIZZLE 3");
    ln(&mut fx, "const float2 alphaMapLoopNum = float2(1.0, 1.0);");
    ln(&mut fx, "");

    // ===== Normal =====
    ln(&mut fx, "// ----- Normal -----");
    let (normal_from, normal_file) = resolve_tex_ref(&mat.normal_texture, support_textures);
    ln(&mut fx, &format!("#define NORMAL_MAP_FROM {}", normal_from));
    if let Some(ref f) = normal_file {
        ln(&mut fx, &format!("#define NORMAL_MAP_FILE \"{}\"", f));
    }
    ln(&mut fx, "#define NORMAL_MAP_TYPE 0");
    ln(&mut fx, "#define NORMAL_MAP_UV_FLIP 0");
    let normal_scale = mat
        .normal_texture
        .as_ref()
        .map(|_| mat.normal_texture_scale)
        .unwrap_or(1.0);
    ln(
        &mut fx,
        &format!("const float normalMapScale = {:.4};", normal_scale),
    );
    ln(&mut fx, "const float2 normalMapLoopNum = float2(1.0, 1.0);");
    ln(&mut fx, "");

    // ===== SubNormal =====
    ln(&mut fx, "// ----- SubNormal -----");
    ln(&mut fx, "#define NORMAL_SUB_MAP_FROM 0");
    ln(&mut fx, "#define NORMAL_SUB_MAP_TYPE 0");
    ln(&mut fx, "#define NORMAL_SUB_MAP_UV_FLIP 0");
    ln(&mut fx, "const float normalSubMapScale = 1.0;");
    ln(
        &mut fx,
        "const float2 normalSubMapLoopNum = float2(1.0, 1.0);",
    );
    ln(&mut fx, "");

    // ===== Smoothness =====
    ln(&mut fx, "// ----- Smoothness -----");
    ln(&mut fx, "#define SMOOTHNESS_MAP_FROM 0");
    ln(&mut fx, "#define SMOOTHNESS_MAP_TYPE 0");
    ln(&mut fx, "#define SMOOTHNESS_MAP_UV_FLIP 0");
    ln(&mut fx, "#define SMOOTHNESS_MAP_SWIZZLE 0");
    ln(&mut fx, "#define SMOOTHNESS_MAP_APPLY_SCALE 0");
    ln(&mut fx, "const float smoothness = 0.0;");
    ln(
        &mut fx,
        "const float2 smoothnessMapLoopNum = float2(1.0, 1.0);",
    );
    ln(&mut fx, "");

    // ===== Metalness =====
    ln(&mut fx, "// ----- Metalness -----");
    ln(&mut fx, "#define METALNESS_MAP_FROM 0");
    ln(&mut fx, "#define METALNESS_MAP_UV_FLIP 0");
    ln(&mut fx, "#define METALNESS_MAP_SWIZZLE 0");
    ln(&mut fx, "#define METALNESS_MAP_APPLY_SCALE 0");
    ln(&mut fx, "const float metalness = 0.0;");
    ln(
        &mut fx,
        "const float2 metalnessMapLoopNum = float2(1.0, 1.0);",
    );
    ln(&mut fx, "");

    // ===== Specular =====
    ln(&mut fx, "// ----- Specular -----");
    ln(&mut fx, "#define SPECULAR_MAP_FROM 0");
    ln(&mut fx, "#define SPECULAR_MAP_TYPE 0");
    ln(&mut fx, "#define SPECULAR_MAP_UV_FLIP 0");
    ln(&mut fx, "#define SPECULAR_MAP_SWIZZLE 0");
    ln(&mut fx, "#define SPECULAR_MAP_APPLY_SCALE 0");
    ln(&mut fx, "const float3 specular = float3(0.5, 0.5, 0.5);");
    ln(
        &mut fx,
        "const float2 specularMapLoopNum = float2(1.0, 1.0);",
    );
    ln(&mut fx, "");

    // ===== Occlusion =====
    ln(&mut fx, "// ----- Occlusion -----");
    ln(&mut fx, "#define OCCLUSION_MAP_FROM 0");
    ln(&mut fx, "#define OCCLUSION_MAP_TYPE 0");
    ln(&mut fx, "#define OCCLUSION_MAP_UV_FLIP 0");
    ln(&mut fx, "#define OCCLUSION_MAP_SWIZZLE 0");
    ln(&mut fx, "#define OCCLUSION_MAP_APPLY_SCALE 0");
    ln(&mut fx, "const float occlusion = 1.0;");
    ln(
        &mut fx,
        "const float2 occlusionMapLoopNum = float2(1.0, 1.0);",
    );
    ln(&mut fx, "");

    // ===== Parallax =====
    ln(&mut fx, "// ----- Parallax -----");
    ln(&mut fx, "#define PARALLAX_MAP_FROM 0");
    ln(&mut fx, "#define PARALLAX_MAP_TYPE 0");
    ln(&mut fx, "#define PARALLAX_MAP_UV_FLIP 0");
    ln(&mut fx, "#define PARALLAX_MAP_SWIZZLE 0");
    ln(&mut fx, "const float parallaxMapScale = 1.0;");
    ln(
        &mut fx,
        "const float2 parallaxMapLoopNum = float2(1.0, 1.0);",
    );
    ln(&mut fx, "");

    // ===== Emissive =====
    ln(&mut fx, "// ----- Emissive -----");
    let ef = mat.emissive_factor;
    let (emissive_from, emissive_file) = resolve_tex_ref(&mat.emissive_texture, support_textures);
    let has_emissive = ef.length_squared() > 1e-6;
    ln(
        &mut fx,
        &format!(
            "#define EMISSIVE_ENABLE {}",
            if has_emissive { 1 } else { 0 }
        ),
    );
    ln(
        &mut fx,
        &format!("#define EMISSIVE_MAP_FROM {}", emissive_from),
    );
    if let Some(ref f) = emissive_file {
        ln(&mut fx, &format!("#define EMISSIVE_MAP_FILE \"{}\"", f));
    }
    ln(&mut fx, "#define EMISSIVE_MAP_UV_FLIP 0");
    ln(&mut fx, "#define EMISSIVE_MAP_APPLY_SCALE 0");
    ln(&mut fx, "#define EMISSIVE_MAP_APPLY_MORPH_INTENSITY 0");
    ln(
        &mut fx,
        &format!(
            "const float3 emissive = float3({:.4}, {:.4}, {:.4});",
            ef.x, ef.y, ef.z
        ),
    );
    ln(&mut fx, "const float emissiveIntensity = 1.0;");
    ln(
        &mut fx,
        "const float2 emissiveMapLoopNum = float2(1.0, 1.0);",
    );
    ln(&mut fx, "");

    // ===== Shading Model =====
    ln(&mut fx, "// ----- Shading Model -----");
    let ce = kind.custom_enable().unwrap_or(0);
    ln(&mut fx, &format!("#define CUSTOM_ENABLE {}", ce));
    match kind {
        RayMmdMaterialKind::Skin => {
            ln(&mut fx, "const float customA = 0.35;       // SSS amount");
            ln(
                &mut fx,
                "const float3 customB = float3(0.7, 0.3, 0.2); // SSS transmittance",
            );
        }
        RayMmdMaterialKind::HairAniso => {
            ln(
                &mut fx,
                "const float customA = 0.5;        // Anisotropic shift",
            );
            ln(
                &mut fx,
                "const float3 customB = float3(0.5, 0.5, 0.5); // Aniso specular",
            );
        }
        RayMmdMaterialKind::Cloth => {
            ln(&mut fx, "const float customA = 0.5;        // Cloth sheen");
            ln(
                &mut fx,
                "const float3 customB = float3(0.5, 0.5, 0.5); // Sheen color",
            );
        }
        RayMmdMaterialKind::Glass => {
            ln(&mut fx, "const float customA = 0.9;        // Transparency");
            ln(
                &mut fx,
                "const float3 customB = float3(1.0, 1.0, 1.0); // Refraction color",
            );
        }
        RayMmdMaterialKind::ClearCoat => {
            ln(
                &mut fx,
                "const float customA = 0.5;        // ClearCoat amount",
            );
            ln(
                &mut fx,
                "const float3 customB = float3(0.04, 0.04, 0.04); // ClearCoat F0",
            );
        }
        _ => {
            ln(&mut fx, "const float customA = 0.0;");
            ln(&mut fx, "const float3 customB = float3(0.0, 0.0, 0.0);");
        }
    }
    ln(&mut fx, "");

    // ===== #include =====
    ln(
        &mut fx,
        &format!(
            "#include \"{}\"",
            include_path.to_string_lossy().replace('\\', "/")
        ),
    );

    // LF → CR+LF
    let crlf = fx.replace('\n', "\r\n");

    // Shift-JIS エンコード
    let (encoded, _, _) = encoding_rs::SHIFT_JIS.encode(&crlf);
    encoded.into_owned()
}

/// テクスチャ参照を解決: (MAP_FROM値, ファイルパス文字列)
fn resolve_tex_ref(
    tex_info: &Option<crate::intermediate::types::IrTextureInfo>,
    support_textures: &std::collections::HashMap<usize, PathBuf>,
) -> (u32, Option<String>) {
    if let Some(ref info) = tex_info {
        if let Some(rel) = support_textures.get(&info.index) {
            return (1, Some(rel.to_string_lossy().replace('\\', "/")));
        }
    }
    (0, None)
}

/// 改行なし push（改行は後で一括変換）
fn ln(buf: &mut String, s: &str) {
    buf.push_str(s);
    buf.push('\n');
}

// ---------------------------------------------------------------------------
// K.4.1 補助テクスチャ書き出し
// ---------------------------------------------------------------------------

/// 各材質から参照される補助テクスチャ（normal / emissive 等、BaseColor 以外）を
/// `mme_dir/textures/` にコピーし、`tex_index → 相対パス（mme_dir 基準）` を返す。
///
/// 同一 `tex_idx` を複数材質が参照する場合は 1 度だけ書き出す。
/// `RawRgba` テクスチャは PNG にエンコードして書き出す。
pub fn export_mme_support_textures(
    ir: &IrModel,
    mme_dir: &Path,
) -> anyhow::Result<std::collections::HashMap<usize, PathBuf>> {
    let tex_dir = mme_dir.join("textures");
    std::fs::create_dir_all(&tex_dir)?;
    let mut used_names: HashSet<String> = HashSet::new();
    let mut result: std::collections::HashMap<usize, PathBuf> = std::collections::HashMap::new();

    // 全材質から参照される補助テクスチャインデックスを収集（BaseColor は除外）
    let mut needed: HashSet<usize> = HashSet::new();
    for mat in &ir.materials {
        if let Some(ref info) = mat.normal_texture {
            needed.insert(info.index);
        }
        if let Some(ref info) = mat.emissive_texture {
            needed.insert(info.index);
        }
        if let Some(mtoon) = mat.mtoon.as_ref() {
            if let Some(ref info) = mtoon.shade_texture {
                needed.insert(info.index);
            }
            if let Some(ref info) = mtoon.matcap_texture {
                needed.insert(info.index);
            }
            if let Some(ref info) = mtoon.rim_multiply_texture {
                needed.insert(info.index);
            }
            if let Some(ref info) = mtoon.outline_width_texture {
                needed.insert(info.index);
            }
            if let Some(ref info) = mtoon.shading_shift_texture {
                needed.insert(info.index);
            }
            if let Some(ref info) = mtoon.uv_animation_mask_texture {
                needed.insert(info.index);
            }
        }
    }

    for tex_idx in needed {
        if tex_idx >= ir.textures.len() {
            continue;
        }
        let tex = &ir.textures[tex_idx];

        // RawRgba は PNG エンコードするため拡張子を .png に強制
        let is_raw = matches!(tex.data, TextureData::RawRgba { .. });
        let ext = if is_raw {
            "png"
        } else if tex.filename.contains('.') {
            tex.filename.rsplit('.').next().unwrap_or("png")
        } else {
            "png"
        };
        let stem = sanitize_material_name(
            tex.filename
                .rsplit(&['/', '\\'][..])
                .next()
                .unwrap_or(&tex.filename)
                .trim_end_matches(&format!(".{}", ext)),
        );
        // RawRgba で元の拡張子を trim した後にさらに残る別の拡張子を除去
        // 例: "normal.dds" → stem="normal_dds" (sanitize 後) → trim ".dds" 不要
        // ただし元拡張子と異なる場合のフォールバック
        let stem = if is_raw {
            // 元ファイル名からパスと全拡張子を除いた部分
            let raw_stem = tex
                .filename
                .rsplit(&['/', '\\'][..])
                .next()
                .unwrap_or(&tex.filename);
            let raw_stem = raw_stem.split('.').next().unwrap_or(raw_stem);
            let s = sanitize_material_name(raw_stem);
            if s.is_empty() {
                format!("tex_{}", tex_idx)
            } else {
                s
            }
        } else if stem.is_empty() {
            format!("tex_{}", tex_idx)
        } else {
            stem
        };
        let mut candidate = format!("{}.{}", stem, ext);
        while used_names.contains(&candidate.to_ascii_lowercase()) {
            candidate = format!("{}_{}.{}", stem, tex_idx, ext);
        }
        used_names.insert(candidate.to_ascii_lowercase());

        // テクスチャデータを書き出す
        let out_path = tex_dir.join(&candidate);
        match &tex.data {
            TextureData::Encoded(data) => {
                std::fs::write(&out_path, data.as_ref())?;
            }
            TextureData::RawRgba {
                pixels,
                width,
                height,
            } => {
                let mut png_data = Vec::new();
                let encoder = image::codecs::png::PngEncoder::new(&mut png_data);
                image::ImageEncoder::write_image(
                    encoder,
                    pixels,
                    *width,
                    *height,
                    image::ExtendedColorType::Rgba8,
                )
                .map_err(|e| anyhow::anyhow!("PNG encode failed for tex {}: {}", tex_idx, e))?;
                std::fs::write(&out_path, &png_data)?;
            }
        }
        // .fx からの相対パス: "textures/<filename>"
        result.insert(tex_idx, PathBuf::from("textures").join(&candidate));
    }

    Ok(result)
}

// ---------------------------------------------------------------------------
// K.5 README 生成
// ---------------------------------------------------------------------------

/// `mme/README.txt` を生成する。
/// MaterialMap タブへの割当手順と注意事項を記載。
pub fn write_mme_readme(
    mme_dir: &Path,
    fx_files: &[(usize, String, RayMmdMaterialKind)],
) -> anyhow::Result<()> {
    use std::fmt::Write;

    let mut txt = String::with_capacity(2048);
    writeln!(txt, "=== popone MME (ray-mmd) マテリアル ===")?;
    writeln!(txt)?;
    writeln!(txt, "このフォルダは popone が自動生成した ray-mmd 2.0 用")?;
    writeln!(txt, "マテリアルファイル (.fx) を含んでいます。")?;
    writeln!(txt)?;
    writeln!(txt, "【使い方】")?;
    writeln!(txt, "1. MMEffect の MaterialMap タブを開く")?;
    writeln!(
        txt,
        "2. 各材質の行をダブルクリックし、対応する .fx ファイルを割り当てる"
    )?;
    writeln!(txt, "3. 表示を確認し、必要に応じてパラメータを調整する")?;
    writeln!(txt)?;
    writeln!(txt, "【材質一覧】")?;
    for (mat_idx, fx_name, kind) in fx_files {
        writeln!(txt, "  材質{:>3}: {} ({})", mat_idx, fx_name, kind.label())?;
    }
    writeln!(txt)?;
    writeln!(txt, "【注意事項】")?;
    writeln!(
        txt,
        "- edge_size: PMX 1.0 では 0.0〜1.0 にクランプされます。"
    )?;
    writeln!(
        txt,
        "  元モデルのエッジ幅が大きい場合、表示が異なることがあります。"
    )?;
    writeln!(
        txt,
        "- textures/ フォルダには法線マップ等の補助テクスチャが含まれます。"
    )?;
    writeln!(
        txt,
        "  .fx ファイルから相対パスで参照しているため、移動しないでください。"
    )?;
    writeln!(txt)?;
    writeln!(txt, "Generated by popone")?;

    // CR+LF + Shift-JIS
    let crlf = txt.replace('\n', "\r\n");
    let (encoded, _, _) = encoding_rs::SHIFT_JIS.encode(&crlf);
    std::fs::write(mme_dir.join("README.txt"), encoded.as_ref())?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use glam::Vec3;

    #[test]
    fn test_guess_skin() {
        let mut mat = IrMaterial::default();
        mat.name = "body_skin".to_string();
        assert_eq!(guess_ray_mmd_kind(&mat), RayMmdMaterialKind::Skin);
    }

    #[test]
    fn test_guess_hair() {
        let mut mat = IrMaterial::default();
        mat.name = "Hair_front".to_string();
        assert_eq!(guess_ray_mmd_kind(&mat), RayMmdMaterialKind::HairAniso);
    }

    #[test]
    fn test_guess_emissive() {
        let mut mat = IrMaterial::default();
        mat.name = "unknown".to_string();
        mat.emissive_factor = Vec3::new(1.0, 0.5, 0.0);
        assert_eq!(guess_ray_mmd_kind(&mat), RayMmdMaterialKind::Emissive);
    }

    #[test]
    fn test_guess_default() {
        let mat = IrMaterial::default();
        assert_eq!(guess_ray_mmd_kind(&mat), RayMmdMaterialKind::Standard);
    }

    #[test]
    fn test_guess_japanese_keywords() {
        let mut mat = IrMaterial::default();
        mat.name = "顔の肌".to_string();
        assert_eq!(guess_ray_mmd_kind(&mat), RayMmdMaterialKind::Skin);

        mat.name = "前髪".to_string();
        assert_eq!(guess_ray_mmd_kind(&mat), RayMmdMaterialKind::HairAniso);
    }

    #[test]
    fn test_make_fx_filename_basic() {
        let mut used = HashSet::new();
        let name = make_fx_filename("Body", &mut used);
        assert_eq!(name, "material_Body.fx");
    }

    #[test]
    fn test_make_fx_filename_collision() {
        let mut used = HashSet::new();
        let n1 = make_fx_filename("Body", &mut used);
        let n2 = make_fx_filename("Body", &mut used);
        assert_eq!(n1, "material_Body.fx");
        assert_eq!(n2, "material_Body_2.fx");
    }

    /// review_022 [P2-2]: 大小無視の衝突判定
    #[test]
    fn test_make_fx_filename_case_insensitive() {
        let mut used = HashSet::new();
        let n1 = make_fx_filename("Body", &mut used);
        let n2 = make_fx_filename("body", &mut used);
        assert_eq!(n1, "material_Body.fx");
        // Windows では同一ファイル名になるので _2 が付く
        assert_eq!(n2, "material_body_2.fx");
    }

    #[test]
    fn test_make_fx_filename_reserved() {
        let mut used = HashSet::new();
        let name = make_fx_filename("CON", &mut used);
        assert_eq!(name, "material_CON_mat.fx");
    }

    #[test]
    fn test_resolve_include_path() {
        let root = Path::new("E:/mme/ray-mmd");
        let output = Path::new("E:/output/mme");
        let result = resolve_include_path(root, output);
        // material_common_2.0.fxsub を含むパスが返されること
        assert!(result
            .to_string_lossy()
            .contains("material_common_2.0.fxsub"));
    }

    #[test]
    fn test_resolve_include_path_relative_fallback() {
        // 相対パスでも失敗せずフォールバックする
        let root = Path::new(".");
        let output = Path::new("E:/output/mme");
        let result = resolve_include_path(root, output);
        assert!(result
            .to_string_lossy()
            .contains("material_common_2.0.fxsub"));
    }

    // ===== Step 7-32: カテゴリ推定テスト拡充 =====

    #[test]
    fn test_guess_cloth() {
        let mut mat = IrMaterial::default();
        mat.name = "制服_shirt".to_string();
        assert_eq!(guess_ray_mmd_kind(&mat), RayMmdMaterialKind::Cloth);
    }

    #[test]
    fn test_guess_glass() {
        let mut mat = IrMaterial::default();
        mat.name = "eye_L".to_string();
        assert_eq!(guess_ray_mmd_kind(&mat), RayMmdMaterialKind::Glass);
    }

    #[test]
    fn test_guess_mixed_case() {
        let mut mat = IrMaterial::default();
        mat.name = "BODY_SKIN_01".to_string();
        assert_eq!(guess_ray_mmd_kind(&mat), RayMmdMaterialKind::Skin);

        mat.name = "HaIr_BaCk".to_string();
        assert_eq!(guess_ray_mmd_kind(&mat), RayMmdMaterialKind::HairAniso);
    }

    #[test]
    fn test_guess_prefixed_name() {
        let mut mat = IrMaterial::default();
        // プレフィックス付きでもキーワードが含まれれば検出
        mat.name = "mat_02_face_blush".to_string();
        assert_eq!(guess_ray_mmd_kind(&mat), RayMmdMaterialKind::Skin);
    }

    #[test]
    fn test_guess_japanese_cloth() {
        let mut mat = IrMaterial::default();
        mat.name = "上着の服".to_string();
        assert_eq!(guess_ray_mmd_kind(&mat), RayMmdMaterialKind::Cloth);
    }

    #[test]
    fn test_guess_japanese_glass() {
        let mut mat = IrMaterial::default();
        mat.name = "瞳ハイライト".to_string();
        assert_eq!(guess_ray_mmd_kind(&mat), RayMmdMaterialKind::Glass);
    }

    #[test]
    fn test_guess_priority_skin_over_cloth() {
        // "skin" と "dress" 両方含む場合、skin が先（優先順位順）
        let mut mat = IrMaterial::default();
        mat.name = "skin_dress_overlay".to_string();
        assert_eq!(guess_ray_mmd_kind(&mat), RayMmdMaterialKind::Skin);
    }

    #[test]
    fn test_guess_emissive_only_when_no_keyword() {
        // キーワードが先に一致すれば emissive にはならない
        let mut mat = IrMaterial::default();
        mat.name = "hair_glow".to_string();
        mat.emissive_factor = Vec3::new(1.0, 1.0, 1.0);
        assert_eq!(guess_ray_mmd_kind(&mat), RayMmdMaterialKind::HairAniso);
    }

    // ===== Step 7-32: custom_enable 値の検証 =====

    #[test]
    fn test_custom_enable_values() {
        assert_eq!(RayMmdMaterialKind::Standard.custom_enable(), Some(0));
        assert_eq!(RayMmdMaterialKind::Skin.custom_enable(), Some(1));
        assert_eq!(RayMmdMaterialKind::HairAniso.custom_enable(), Some(3));
        assert_eq!(RayMmdMaterialKind::Glass.custom_enable(), Some(4));
        assert_eq!(RayMmdMaterialKind::Cloth.custom_enable(), Some(5));
        assert_eq!(RayMmdMaterialKind::ClearCoat.custom_enable(), Some(6));
        assert_eq!(RayMmdMaterialKind::Emissive.custom_enable(), None);
    }

    // ===== Step 7-34: generate_fx 出力検証 =====

    #[test]
    fn test_generate_fx_contains_all_sections() {
        let mat = IrMaterial::default();
        let include = Path::new("../Materials/material_common_2.0.fxsub");
        let support = std::collections::HashMap::new();
        let fx = generate_fx(&mat, RayMmdMaterialKind::Standard, include, &support);
        let content = encoding_rs::SHIFT_JIS.decode(&fx).0;

        // 全セクションヘッダが含まれること
        assert!(content.contains("// ----- Albedo -----"));
        assert!(content.contains("// ----- SubAlbedo -----"));
        assert!(content.contains("// ----- Alpha -----"));
        assert!(content.contains("// ----- Normal -----"));
        assert!(content.contains("// ----- SubNormal -----"));
        assert!(content.contains("// ----- Smoothness -----"));
        assert!(content.contains("// ----- Metalness -----"));
        assert!(content.contains("// ----- Specular -----"));
        assert!(content.contains("// ----- Occlusion -----"));
        assert!(content.contains("// ----- Parallax -----"));
        assert!(content.contains("// ----- Emissive -----"));
        assert!(content.contains("// ----- Shading Model -----"));
        assert!(content.contains("#include"));
    }

    #[test]
    fn test_generate_fx_crlf_encoding() {
        let mat = IrMaterial::default();
        let include = Path::new("test.fxsub");
        let support = std::collections::HashMap::new();
        let fx = generate_fx(&mat, RayMmdMaterialKind::Standard, include, &support);

        // CR+LF が含まれること
        assert!(fx.windows(2).any(|w| w == b"\r\n"));
        // 孤立 LF がないこと（全 LF は CR+LF の一部）
        for (i, &b) in fx.iter().enumerate() {
            if b == b'\n' {
                assert!(i > 0 && fx[i - 1] == b'\r', "bare LF at byte {}", i);
            }
        }
    }

    #[test]
    fn test_generate_fx_skin_custom_params() {
        let mat = IrMaterial::default();
        let include = Path::new("test.fxsub");
        let support = std::collections::HashMap::new();
        let fx = generate_fx(&mat, RayMmdMaterialKind::Skin, include, &support);
        let content = encoding_rs::SHIFT_JIS.decode(&fx).0;

        assert!(content.contains("#define CUSTOM_ENABLE 1"));
        assert!(content.contains("SSS amount"));
    }

    #[test]
    fn test_make_fx_filename_empty_name() {
        let mut used = HashSet::new();
        let name = make_fx_filename("", &mut used);
        assert_eq!(name, "material_unnamed.fx");
    }

    #[test]
    fn test_make_fx_filename_sanitize_special_chars() {
        let mut used = HashSet::new();
        let name = make_fx_filename("体/肌@テスト", &mut used);
        // 非 ASCII 英数字は除去される
        assert!(name.starts_with("material_"));
        assert!(name.ends_with(".fx"));
    }
}
