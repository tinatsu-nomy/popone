pub mod archive;
pub mod color;
pub mod convert;
pub mod directx;
pub mod error;
pub mod fbx;
pub mod intermediate;
pub mod obj;
pub mod pmd;
pub mod pmx;
pub mod psd;
pub mod stl;
pub mod unity;
pub mod unitypackage;
pub mod vrm;

#[cfg(feature = "viewer")]
pub mod viewer;

use std::path::Path;

/// パスの拡張子を小文字で返す（拡張子なし・非UTF-8の場合は空文字列）
pub fn path_ext_lower(path: &Path) -> String {
    path.extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase()
}

/// ログメモリバッファ（上限付き、累計オフセット追跡）
/// VecDeque を使用し、先頭切り詰め（drain）を O(1) で行う。
pub struct LogBuffer {
    pub data: std::collections::VecDeque<u8>,
    /// 累計書き込みバイト数（drain しても減らない）
    pub total_written: usize,
}

impl LogBuffer {
    pub fn new() -> Self {
        Self {
            data: std::collections::VecDeque::new(),
            total_written: 0,
        }
    }

    /// 累計オフセット `offset` 以降のデータを読み取る。
    /// drain 済み範囲は切り捨て、残存データから可能な限り返す。
    pub fn read_from_offset(&self, offset: usize) -> Option<String> {
        let drained = self.total_written - self.data.len();
        let start = if offset <= drained {
            0 // 要求範囲が drain 済み → 残存データの先頭から
        } else if offset >= self.total_written {
            return None; // まだ何も書かれていない範囲
        } else {
            offset - drained
        };
        let (front, back) = self.data.as_slices();
        let total_len = self.data.len();
        if start >= total_len {
            return None;
        }
        // VecDeque は内部的に2つの連続スライスで構成される
        let bytes: std::borrow::Cow<'_, [u8]> = if start < front.len() {
            if back.is_empty() {
                std::borrow::Cow::Borrowed(&front[start..])
            } else {
                std::borrow::Cow::Owned([&front[start..], back].concat())
            }
        } else {
            std::borrow::Cow::Borrowed(&back[start - front.len()..])
        };
        if bytes.is_empty() {
            None
        } else {
            Some(String::from_utf8_lossy(&bytes).into_owned())
        }
    }
}

/// ログメモリバッファの共有型
pub type SharedLogBuffer = std::sync::Arc<std::sync::Mutex<LogBuffer>>;

use error::Result;
use pmx::build::PmxBuildOptions;
use serde::Serialize;

#[derive(Serialize, Debug)]
pub struct ConvertStats {
    pub output_path: String,
    pub tex_dir: String,
    pub bones: usize,
    pub vertices: usize,
    pub faces: usize,
    pub materials: usize,
    pub textures: usize,
    pub morphs: usize,
}

/// VRM → PMX 変換オプション
#[derive(Debug, Clone)]
pub struct VrmConvertOptions {
    /// 物理（剛体・ジョイント）を出力しない
    pub no_physics: bool,
    /// 剛体回転をボーン方向に揃える
    pub align_rigid_rotation: bool,
    /// Aスタンスへ正規化
    pub normalize_pose: bool,
    /// 標準ボーン挿入をスキップ（元のボーン構造を維持）
    pub raw_structure: bool,
    /// PMX出力倍率（デフォルト: 1.0）
    pub scale: f32,
}

impl Default for VrmConvertOptions {
    fn default() -> Self {
        Self {
            no_physics: false,
            align_rigid_rotation: false,
            normalize_pose: false,
            raw_structure: false,
            scale: 1.0,
        }
    }
}

pub fn convert_vrm_to_pmx(
    input_path: &Path,
    output_path: &Path,
    options: &VrmConvertOptions,
) -> Result<ConvertStats> {
    let glb = vrm::loader::load_glb(input_path)?;
    let version = vrm::detect::detect_version(&glb.document);
    let all_extensions = vrm::loader::get_raw_extensions(&glb.document);

    let ir = vrm::extract::extract_ir_model_with_options(
        &glb.document,
        &glb.buffers,
        &glb.images,
        &glb.vrm_extension,
        &version,
        &all_extensions,
        options.normalize_pose,
    )?;

    let output_dir = output_path.parent().unwrap_or(Path::new("."));
    let tex_dir = output_dir.join("textures");
    convert::texture::write_all_textures(&ir.textures, &glb.images, &tex_dir)?;

    let build_options = PmxBuildOptions {
        align_rigid_rotation: options.align_rigid_rotation,
        no_physics: options.no_physics,
        raw_structure: options.raw_structure,
        scale: options.scale,
    };
    let (mut pmx_model, toon_textures) =
        pmx::build::build_pmx_model_with_options(&ir, &build_options)?;
    let toon_written = convert::texture::write_all_textures_from_ir(&toon_textures, &tex_dir)?;
    // 生成トゥーンテクスチャの実ファイル名で PMX パスを補正
    let base_tex_count = ir.textures.len();
    for (i, name) in toon_written.iter().enumerate() {
        let pmx_idx = base_tex_count + i;
        if pmx_idx < pmx_model.textures.len() {
            pmx_model.textures[pmx_idx] = format!("textures\\{}", name);
        }
    }
    write_pmx_and_stats(&pmx_model, output_path, &tex_dir)
}

/// FBX → PMX 変換
pub fn convert_fbx_to_pmx(
    input_path: &Path,
    output_path: &Path,
    options: &VrmConvertOptions,
) -> Result<ConvertStats> {
    let data = std::fs::read(input_path)?;
    let ir = fbx::extract::extract_ir_model_from_fbx_with_options(
        &data,
        Some(input_path),
        options.normalize_pose,
        false,
    )?;
    let build_options = PmxBuildOptions {
        align_rigid_rotation: options.align_rigid_rotation,
        no_physics: options.no_physics,
        raw_structure: options.raw_structure,
        scale: options.scale,
    };
    convert_ir_to_pmx(&ir, output_path, &build_options)
}

/// OBJ → PMX 変換
pub fn convert_obj_to_pmx(
    input_path: &Path,
    output_path: &Path,
    options: &VrmConvertOptions,
) -> Result<ConvertStats> {
    let ir = obj::extract::load_obj(input_path)?;
    let build_options = PmxBuildOptions {
        align_rigid_rotation: options.align_rigid_rotation,
        no_physics: options.no_physics,
        raw_structure: options.raw_structure,
        scale: options.scale,
    };
    convert_ir_to_pmx(&ir, output_path, &build_options)
}

/// STL → PMX 変換
pub fn convert_stl_to_pmx(
    input_path: &Path,
    output_path: &Path,
    options: &VrmConvertOptions,
) -> Result<ConvertStats> {
    let ir = stl::extract::load_stl(input_path)?;
    let build_options = PmxBuildOptions {
        align_rigid_rotation: options.align_rigid_rotation,
        no_physics: options.no_physics,
        raw_structure: options.raw_structure,
        scale: options.scale,
    };
    convert_ir_to_pmx(&ir, output_path, &build_options)
}

/// DirectX .x → PMX 変換
pub fn convert_x_to_pmx(
    input_path: &Path,
    output_path: &Path,
    options: &VrmConvertOptions,
) -> Result<ConvertStats> {
    let ir = directx::extract::load_x(input_path)?;
    let build_options = PmxBuildOptions {
        align_rigid_rotation: options.align_rigid_rotation,
        no_physics: options.no_physics,
        raw_structure: options.raw_structure,
        scale: options.scale,
    };
    convert_ir_to_pmx(&ir, output_path, &build_options)
}

/// IrModel から直接 PMX 変換（ビューアで編集済みの IrModel を使用）
pub fn convert_ir_to_pmx(
    ir: &intermediate::types::IrModel,
    output_path: &Path,
    options: &PmxBuildOptions,
) -> Result<ConvertStats> {
    let output_dir = output_path.parent().unwrap_or(Path::new("."));
    std::fs::create_dir_all(output_dir)?;
    let tex_dir = output_dir.join("textures");
    let written_filenames = convert::texture::write_all_textures_from_ir(&ir.textures, &tex_dir)?;

    let (mut pmx_model, toon_textures) = pmx::build::build_pmx_model_with_options(ir, options)?;
    // PSD→PNG 変換でファイル名が変わった場合、PMX テクスチャパスを補正
    for (i, name) in written_filenames.iter().enumerate() {
        if i < pmx_model.textures.len() {
            pmx_model.textures[i] = format!("textures\\{}", name);
        }
    }
    // 生成トゥーンテクスチャをディスクに書き出し、PMX パスを補正
    let base_tex_count = ir.textures.len();
    let toon_written = convert::texture::write_all_textures_from_ir(&toon_textures, &tex_dir)?;
    for (i, name) in toon_written.iter().enumerate() {
        let pmx_idx = base_tex_count + i;
        if pmx_idx < pmx_model.textures.len() {
            pmx_model.textures[pmx_idx] = format!("textures\\{}", name);
        }
    }
    write_pmx_and_stats(&pmx_model, output_path, &tex_dir)
}

/// PMX モデルをファイルに書き出して ConvertStats を返す（共通処理）
fn write_pmx_and_stats(
    pmx_model: &pmx::types::PmxModel,
    output_path: &Path,
    tex_dir: &Path,
) -> Result<ConvertStats> {
    let stats = ConvertStats {
        output_path: output_path.to_string_lossy().into_owned(),
        tex_dir: tex_dir.to_string_lossy().into_owned(),
        bones: pmx_model.bones.len(),
        vertices: pmx_model.vertices.len(),
        faces: pmx_model.faces.len(),
        materials: pmx_model.materials.len(),
        textures: pmx_model.textures.len(),
        morphs: pmx_model.morphs.len(),
    };

    let file = std::fs::File::create(output_path)?;
    let writer = std::io::BufWriter::new(file);
    let header = pmx_model.header.clone();
    let mut pmx_writer = pmx::writer::PmxWriter::new(writer, header);
    pmx_writer.write_model(pmx_model)?;

    Ok(stats)
}

/// base_dir が既に converted_modelXX 配下なら親ディレクトリを返す（入れ子防止）
pub fn resolve_converted_base(dir: &Path) -> &Path {
    if let Some(name) = dir.file_name().and_then(|n| n.to_str()) {
        if name.starts_with("converted_model")
            && name["converted_model".len()..]
                .chars()
                .all(|c| c.is_ascii_digit())
        {
            return dir.parent().unwrap_or(dir);
        }
    }
    dir
}

/// converted_modelXX ディレクトリの次の空き番号を検索（上限なし）
pub fn next_converted_dir(base_dir: &Path) -> std::path::PathBuf {
    let base_dir = resolve_converted_base(base_dir);
    for i in 1.. {
        let dir = base_dir.join(format!("converted_model{:02}", i));
        if !dir.exists() {
            return dir;
        }
    }
    unreachable!()
}

/// モデル名をファイル名に安全な文字列にサニタイズ
/// Windows の不正文字・予約名を処理し、空の場合は None を返す
pub fn sanitize_filename(name: &str) -> Option<String> {
    const INVALID_CHARS: &[char] = &['/', '\\', ':', '*', '?', '"', '<', '>', '|'];
    const RESERVED: &[&str] = &[
        "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
        "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
    ];
    if name.is_empty() {
        return None;
    }
    // 不正文字を _ に置換
    let sanitized: String = name
        .chars()
        .map(|c| {
            if INVALID_CHARS.contains(&c) || c.is_control() {
                '_'
            } else {
                c
            }
        })
        .collect();
    // 末尾の空白・ピリオドを除去
    let trimmed = sanitized.trim_end_matches(|c: char| c == ' ' || c == '.');
    if trimmed.is_empty() {
        return None;
    }
    // Windows 予約名チェック（ベース名 = 最初の '.' より前で判定）
    let base = match trimmed.find('.') {
        Some(pos) => &trimmed[..pos],
        None => trimmed,
    };
    let base_upper = base.to_uppercase();
    if RESERVED.contains(&base_upper.as_str()) {
        return Some(format!("_{}", trimmed));
    }
    Some(trimmed.to_string())
}

/// 拡張子で VRM/FBX を自動判定して PMX 変換
pub fn convert_to_pmx(
    input_path: &Path,
    output_path: &Path,
    options: &VrmConvertOptions,
) -> Result<ConvertStats> {
    let ext = path_ext_lower(input_path);
    match ext.as_str() {
        "fbx" => convert_fbx_to_pmx(input_path, output_path, options),
        "obj" => convert_obj_to_pmx(input_path, output_path, options),
        "stl" => convert_stl_to_pmx(input_path, output_path, options),
        "x" => convert_x_to_pmx(input_path, output_path, options),
        _ => convert_vrm_to_pmx(input_path, output_path, options),
    }
}

/// テスト用ユーティリティ（テストデータのパス解決）
///
/// パス解決の優先順位（ファイルごと）:
///   1. ファイル個別の環境変数（例: `POPONE_TEST_VRM_SEED_SAN`）
///   2. ルート環境変数 `POPONE_TEST_DATA` + 相対パス
///   3. `CARGO_MANIFEST_DIR/..` + 相対パス（ローカル開発デフォルト）
///
/// CI 設定例:
/// ```sh
/// # ルート指定（全ファイル共通ベース）
/// export POPONE_TEST_DATA=/path/to/test-fixtures
///
/// # または個別指定（特定ファイルだけ別の場所にある場合）
/// export POPONE_TEST_VRM_SEED_SAN=/data/models/Seed-san.vrm
/// export POPONE_TEST_PMX_SEED_SAN=/data/converted/Seed-san.pmx
/// export POPONE_TEST_PMD_MIKU_V2=/data/mmd/初音ミクVer2.pmd
/// ```
#[cfg(test)]
pub mod test_util {
    use std::path::PathBuf;

    /// テストデータのルートディレクトリ
    fn test_data_root() -> PathBuf {
        if let Ok(dir) = std::env::var("POPONE_TEST_DATA") {
            return PathBuf::from(dir);
        }
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..")
    }

    /// ファイル個別の環境変数 → ルート相対パスの順で解決
    fn resolve(env_key: &str, relative: &str) -> PathBuf {
        if let Ok(path) = std::env::var(env_key) {
            return PathBuf::from(path);
        }
        test_data_root().join(relative)
    }

    /// VRM サンプル (vrm-c/vrm-specification リポジトリ内)
    /// 環境変数: `POPONE_TEST_VRM_SEED_SAN`
    pub fn seed_san_vrm() -> PathBuf {
        resolve(
            "POPONE_TEST_VRM_SEED_SAN",
            "vrm-spec/vrm-specification/samples/Seed-san/vrm/Seed-san.vrm",
        )
    }

    /// PMX テストファイル (Seed-san.vrm を popone で変換済み)
    /// 環境変数: `POPONE_TEST_PMX_SEED_SAN`
    pub fn seed_san_pmx() -> PathBuf {
        resolve("POPONE_TEST_PMX_SEED_SAN", "tmp/Seed-san.pmx")
    }

    /// PMD テストファイル (MikuMikuDance_v932x64.zip 同梱)
    /// 環境変数: `POPONE_TEST_PMD_MIKU_V2`
    pub fn miku_v2_pmd() -> PathBuf {
        resolve("POPONE_TEST_PMD_MIKU_V2", "tmp/pmd/初音ミクVer2.pmd")
    }

    /// テストファイルが存在すれば Some(path)、なければ None を返す
    pub fn try_test_file(path: PathBuf) -> Option<PathBuf> {
        if path.exists() {
            Some(path)
        } else {
            eprintln!(
                "テストファイルが存在しません（スキップ）: {}",
                path.display()
            );
            None
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_vrm_to_pmx_end_to_end() {
        let Some(input) = crate::test_util::try_test_file(crate::test_util::seed_san_vrm()) else {
            return;
        };
        let output = std::env::temp_dir().join("popone_test_e2e.pmx");

        // VRM → PMX 変換
        let stats =
            crate::convert_vrm_to_pmx(&input, &output, &crate::VrmConvertOptions::default())
                .expect("VRM→PMX変換失敗");

        // 統計値の確認
        assert!(stats.bones > 100, "ボーン数が少なすぎる: {}", stats.bones);
        assert!(
            stats.vertices > 1000,
            "頂点数が少なすぎる: {}",
            stats.vertices
        );
        assert!(stats.materials > 0, "材質数がゼロ");

        // 出力ファイルの存在とサイズ確認
        let metadata = std::fs::metadata(&output).expect("出力ファイルなし");
        assert!(
            metadata.len() > 1000,
            "PMXファイルが小さすぎる: {} bytes",
            metadata.len()
        );

        // 読み戻して検証
        let pmx = crate::pmx::reader::read_pmx(&output).expect("PMX再読み込み失敗");
        assert!(
            pmx.bones.len() > 100,
            "ボーン数が少なすぎる: {}",
            pmx.bones.len()
        );
        assert!(
            pmx.vertices.len() > 1000,
            "頂点数が少なすぎる: {}",
            pmx.vertices.len()
        );

        // テンポラリファイルのクリーンアップ
        let _ = std::fs::remove_file(&output);
        let tex_dir = output.parent().unwrap().join("textures");
        let _ = std::fs::remove_dir_all(&tex_dir);
    }
}
