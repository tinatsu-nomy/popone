pub mod error;
pub mod vrm;
pub mod intermediate;
pub mod pmx;
pub mod pmd;
pub mod convert;
pub mod fbx;
pub mod unity;
pub mod unitypackage;
pub mod archive;

#[cfg(feature = "viewer")]
pub mod viewer;

use std::path::Path;
use error::Result;
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

pub fn convert_vrm_to_pmx(
    input_path: &Path,
    output_path: &Path,
    no_physics: bool,
) -> Result<ConvertStats> {
    convert_vrm_to_pmx_with_options(input_path, output_path, no_physics, false)
}

pub fn convert_vrm_to_pmx_with_options(
    input_path: &Path,
    output_path: &Path,
    no_physics: bool,
    align_rigid_rotation: bool,
) -> Result<ConvertStats> {
    convert_vrm_to_pmx_full(input_path, output_path, no_physics, align_rigid_rotation, false)
}

pub fn convert_vrm_to_pmx_full(
    input_path: &Path,
    output_path: &Path,
    no_physics: bool,
    align_rigid_rotation: bool,
    normalize_pose: bool,
) -> Result<ConvertStats> {
    let glb = vrm::loader::load_glb(input_path)?;
    let version = vrm::detect::detect_version(&glb.document);
    let all_extensions = vrm::loader::get_raw_extensions(&glb.document);

    let mut ir = vrm::extract::extract_ir_model_with_options(
        &glb.document,
        &glb.buffers,
        &glb.images,
        &glb.vrm_extension,
        &version,
        &all_extensions,
        normalize_pose,
    )?;

    if no_physics {
        ir.physics = intermediate::types::IrPhysics::default();
    }

    let output_dir = output_path.parent().unwrap_or(Path::new("."));
    let tex_dir = output_dir.join("textures");
    convert::texture::write_all_textures(&ir.textures, &glb.images, &tex_dir)?;

    let pmx_model = pmx::build::build_pmx_model_with_options(&ir, align_rigid_rotation)?;
    write_pmx_and_stats(&pmx_model, output_path, &tex_dir)
}

/// FBX → PMX 変換
pub fn convert_fbx_to_pmx(
    input_path: &Path,
    output_path: &Path,
) -> Result<ConvertStats> {
    let data = std::fs::read(input_path)?;
    let ir = fbx::extract::extract_ir_model_from_fbx(&data, Some(input_path))?;
    convert_ir_to_pmx(&ir, output_path, false)
}

/// IrModel から直接 PMX 変換（ビューアで編集済みの IrModel を使用）
pub fn convert_ir_to_pmx(
    ir: &intermediate::types::IrModel,
    output_path: &Path,
    align_rigid_rotation: bool,
) -> Result<ConvertStats> {
    let output_dir = output_path.parent().unwrap_or(Path::new("."));
    let tex_dir = output_dir.join("textures");
    convert::texture::write_all_textures_from_ir(&ir.textures, &tex_dir)?;

    let pmx_model = pmx::build::build_pmx_model_with_options(ir, align_rigid_rotation)?;
    write_pmx_and_stats(&pmx_model, output_path, &tex_dir)
}

/// PMX モデルをファイルに書き出して ConvertStats を返す（共通処理）
fn write_pmx_and_stats(
    pmx_model: &pmx::types::PmxModel,
    output_path: &Path,
    tex_dir: &Path,
) -> Result<ConvertStats> {
    let stats = ConvertStats {
        output_path: output_path.to_string_lossy().to_string(),
        tex_dir: tex_dir.to_string_lossy().to_string(),
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

/// 拡張子で VRM/FBX を自動判定して PMX 変換
pub fn convert_to_pmx(
    input_path: &Path,
    output_path: &Path,
    no_physics: bool,
) -> Result<ConvertStats> {
    let ext = input_path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_lowercase());
    match ext.as_deref() {
        Some("fbx") => convert_fbx_to_pmx(input_path, output_path),
        _ => convert_vrm_to_pmx(input_path, output_path, no_physics),
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
            eprintln!("テストファイルが存在しません（スキップ）: {}", path.display());
            None
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_vrm_to_pmx_end_to_end() {
        let Some(input) = crate::test_util::try_test_file(crate::test_util::seed_san_vrm()) else { return; };
        let output = std::env::temp_dir().join("popone_test_e2e.pmx");

        // VRM → PMX 変換
        let stats = crate::convert_vrm_to_pmx(&input, &output, false)
            .expect("VRM→PMX変換失敗");

        // 統計値の確認
        assert!(stats.bones > 100, "ボーン数が少なすぎる: {}", stats.bones);
        assert!(stats.vertices > 1000, "頂点数が少なすぎる: {}", stats.vertices);
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
        assert!(pmx.bones.len() > 100, "ボーン数が少なすぎる: {}", pmx.bones.len());
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
