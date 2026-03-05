use vrm2pmx::{vrm, pmx, convert, intermediate};

use anyhow::{Context, Result};
use clap::Parser;
use std::path::{Path, PathBuf};

#[derive(Parser, Debug)]
#[command(name = "vrm2pmx", about = "VRMファイルをPMX形式に変換します\n引数なしで起動するとビューアが開きます")]
struct Args {
    /// 入力VRMファイルパス
    input: Option<PathBuf>,

    /// 出力PMXファイルパス
    output: Option<PathBuf>,

    /// ボーン・頂点数のみ出力してPMX生成しない
    #[arg(long)]
    dump: bool,

    /// 物理変換をスキップ
    #[arg(long)]
    no_physics: bool,

    /// ログレベル (error, warn, info, debug)
    #[arg(long, default_value = "info")]
    log_level: String,
}

/// ロガーセットアップ。
/// stderr には `stderr_level` までのログを出力する。
/// `log_file` が Some の場合、そのパスに DEBUG レベルまで全て書き出す。
fn setup_logging(stderr_level: log::LevelFilter, log_file: Option<&std::path::Path>) -> Result<()> {
    let mut base = fern::Dispatch::new()
        .level(log::LevelFilter::Debug); // グローバル最小フィルター

    // stderr: ユーザー指定レベル
    base = base.chain(
        fern::Dispatch::new()
            .level(stderr_level)
            .format(|out, msg, rec| {
                let now = chrono::Local::now().format("%H:%M:%S%.3f");
                out.finish(format_args!("[{}][{}] {}", now, rec.level(), msg))
            })
            .chain(std::io::stderr()),
    );

    // ファイル: DEBUG まで全件（上書き）
    if let Some(path) = log_file {
        let file = std::fs::OpenOptions::new()
            .write(true).create(true).truncate(true)
            .open(path)?;
        base = base.chain(
            fern::Dispatch::new()
                .level(log::LevelFilter::Debug)
                .format(|out, msg, rec| {
                    let now = chrono::Local::now().format("%H:%M:%S%.3f");
                    out.finish(format_args!("[{}][{}] {}", now, rec.level(), msg))
                })
                .chain(Box::new(file) as Box<dyn std::io::Write + Send>),
        );
    }

    base.apply().map_err(|e| anyhow::anyhow!("ロガー初期化失敗: {}", e))
}

fn main() -> Result<()> {
    let args = Args::parse();

    // 引数なし → ビューア起動
    if args.input.is_none() {
        #[cfg(feature = "viewer")]
        {
            return run_viewer();
        }
        #[cfg(not(feature = "viewer"))]
        {
            anyhow::bail!(
                "ビューアは viewer feature 付きでビルドする必要があります。\n\
                 使い方: vrm2pmx <入力.vrm> <出力.pmx>\n\
                 ビューア: cargo build --features viewer"
            );
        }
    }

    let input = args.input.unwrap();
    let output = args.output.context(
        "出力ファイルパスを指定してください。\n使い方: vrm2pmx <入力.vrm> <出力.pmx>"
    )?;

    // ロガー初期化（dump 時はファイルログなし）
    let log_level = args.log_level.parse::<log::LevelFilter>()
        .unwrap_or(log::LevelFilter::Info);
    let log_path = if args.dump { None } else { Some(output.with_extension("log")) };
    setup_logging(log_level, log_path.as_deref())
        .context("ロガー初期化失敗")?;
    if let Some(ref p) = log_path {
        log::info!("ログファイル: {}", p.display());
    }

    log::info!("入力ファイル: {}", input.display());

    // GLB読み込み
    let glb = vrm::loader::load_glb(&input)
        .with_context(|| format!("GLB読み込み失敗: {}", input.display()))?;

    // バージョン判定
    let version = vrm::detect::detect_version(&glb.document);
    log::info!("VRMバージョン: {:?}", version);

    // 全拡張取得
    let all_extensions = vrm::loader::get_raw_extensions(&glb.document);

    // 中間表現抽出
    let mut ir = vrm::extract::extract_ir_model(
        &glb.document,
        &glb.buffers,
        &glb.images,
        &glb.vrm_extension,
        &version,
        &all_extensions,
    ).context("VRM中間表現の抽出に失敗")?;

    if args.no_physics {
        ir.physics = intermediate::types::IrPhysics::default();
        log::info!("物理変換をスキップ（--no-physics）");
    }

    if args.dump {
        println!("=== VRM dump ===");
        println!("モデル名: {}", ir.name);
        println!("ボーン数: {}", ir.bones.len());
        println!("メッシュ数: {}", ir.meshes.len());
        println!("頂点数(合計): {}", ir.total_vertices());
        println!("面数(合計): {}", ir.total_faces());
        println!("材質数: {}", ir.materials.len());
        println!("テクスチャ数: {}", ir.textures.len());
        println!("モーフ数: {}", ir.morphs.len());
        println!("剛体数: {}", ir.physics.rigid_bodies.len());
        println!("ジョイント数: {}", ir.physics.joints.len());

        println!("\n--- ボーン一覧 ---");
        for (i, bone) in ir.bones.iter().enumerate() {
            let vrm_name = bone.vrm_bone_name.as_deref().unwrap_or("-");
            println!("  [{:3}] {} (vrm: {})", i, bone.name, vrm_name);
        }

        println!("\n--- モーフ一覧 ---");
        for morph in &ir.morphs {
            println!("  [panel{}] {}", morph.panel, morph.name);
        }
        return Ok(());
    }

    // 出力ディレクトリ確定
    let output_dir = output.parent()
        .unwrap_or(Path::new("."))
        .to_path_buf();

    // テクスチャ書き出し
    let tex_dir = output_dir.join("textures");
    convert::texture::write_all_textures(&ir.textures, &glb.images, &tex_dir)
        .context("テクスチャ書き出し失敗")?;

    // PMXモデル構築
    let pmx_model = pmx::build::build_pmx_model(&ir)
        .context("PMXモデル構築失敗")?;

    // PMX書き出し
    let output_file = std::fs::File::create(&output)
        .with_context(|| format!("出力ファイル作成失敗: {}", output.display()))?;
    let writer = std::io::BufWriter::new(output_file);

    let header = pmx_model.header.clone();
    let mut pmx_writer = pmx::writer::PmxWriter::new(writer, header);
    pmx_writer.write_model(&pmx_model)
        .context("PMX書き出し失敗")?;

    log::info!("変換完了: {}", output.display());
    println!("変換完了: {} → {}", input.display(), output.display());

    Ok(())
}

#[cfg(feature = "viewer")]
fn run_viewer() -> Result<()> {
    env_logger::init();

    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size([1280.0, 800.0])
            .with_title("VRM Viewer")
            .with_drag_and_drop(true),
        renderer: eframe::Renderer::Wgpu,
        ..Default::default()
    };

    eframe::run_native(
        "VRM Viewer",
        options,
        Box::new(|cc| Ok(Box::new(vrm2pmx::viewer::app::ViewerApp::new(cc)))),
    )
    .map_err(|e| anyhow::anyhow!("ビューア起動失敗: {e}"))
}
