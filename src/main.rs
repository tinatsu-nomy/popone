// viewer feature 有効時は Windows GUI サブシステムでビルドし、
// Explorer からの起動時にコンソールウィンドウを表示しない
#![cfg_attr(all(feature = "viewer", target_os = "windows"), windows_subsystem = "windows")]

use vrm2pmx::{vrm, pmx, convert, intermediate};

use anyhow::{Context, Result};
use clap::Parser;
use std::path::{Path, PathBuf};

#[derive(Parser, Debug)]
#[command(name = "vrm2pmx", about = "VRMファイルをPMX形式に変換します\n引数なしで起動するとビューアが開きます")]
struct Args {
    /// 入力ファイルパス（VRM/FBX）
    input: Option<PathBuf>,

    /// 出力PMXファイルパス
    output: Option<PathBuf>,

    /// ボーン・頂点数のみ出力してPMX生成しない
    #[arg(long)]
    dump: bool,

    /// 物理変換をスキップ
    #[arg(long)]
    no_physics: bool,

    /// 剛体の回転をボーン方向に揃える（デフォルト: off）
    #[arg(long)]
    align_rigid_rotation: bool,

    /// Tポーズの腕をAスタンスに変換する（デフォルト: off）
    #[arg(long)]
    normalize_pose: bool,

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

/// Windows GUI サブシステムの場合、親コンソールにアタッチして
/// stdout/stderr を使えるようにする
#[cfg(all(feature = "viewer", target_os = "windows"))]
fn attach_parent_console() {
    extern "system" {
        fn AttachConsole(dw_process_id: u32) -> i32;
    }
    unsafe {
        AttachConsole(0xFFFFFFFF); // ATTACH_PARENT_PROCESS
    }
}

fn main() -> Result<()> {
    // GUI サブシステムでも CLI 引数がある場合はコンソール出力を有効にする
    #[cfg(all(feature = "viewer", target_os = "windows"))]
    if std::env::args().len() > 1 {
        attach_parent_console();
    }

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

    // unwrap 安全: 上で is_none() チェック済み
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

    let ext = input
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();
    let is_fbx = ext == "fbx";

    // 中間表現抽出（VRM / FBX 分岐）
    let mut ir = if is_fbx {
        let data = std::fs::read(&input)
            .with_context(|| format!("FBXファイル読み込み失敗: {}", input.display()))?;
        vrm2pmx::fbx::extract::extract_ir_model_from_fbx(&data, Some(&input))
            .context("FBX中間表現の抽出に失敗")?
    } else {
        let glb = vrm::loader::load_glb(&input)
            .with_context(|| format!("GLB読み込み失敗: {}", input.display()))?;
        let version = vrm::detect::detect_version(&glb.document);
        log::info!("VRMバージョン: {:?}", version);
        let all_extensions = vrm::loader::get_raw_extensions(&glb.document);
        vrm::extract::extract_ir_model_with_options(
            &glb.document,
            &glb.buffers,
            &glb.images,
            &glb.vrm_extension,
            &version,
            &all_extensions,
            args.normalize_pose,
        ).context("VRM中間表現の抽出に失敗")?
    };

    if args.no_physics {
        ir.physics = intermediate::types::IrPhysics::default();
        log::info!("物理変換をスキップ（--no-physics）");
    }

    if args.dump {
        println!("=== {} dump ===", ir.source_format.label());
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
        if let Some(ref rig) = ir.rig_type {
            println!("リグ種別: {} (Humanoid: {}本)", rig, ir.humanoid_bone_count);
        }

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
    if is_fbx {
        convert::texture::write_all_textures_from_ir(&ir.textures, &tex_dir)
            .context("テクスチャ書き出し失敗")?;
    } else {
        let glb = vrm::loader::load_glb(&input)?;
        convert::texture::write_all_textures(&ir.textures, &glb.images, &tex_dir)
            .context("テクスチャ書き出し失敗")?;
    }

    // PMXモデル構築
    let pmx_model = pmx::build::build_pmx_model_with_options(&ir, args.align_rigid_rotation)
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
    // exe と同じディレクトリに vrm2pmx.log を出力
    let log_path = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.join("vrm2pmx.log")))
        .unwrap_or_else(|| std::path::PathBuf::from("vrm2pmx.log"));
    setup_logging(log::LevelFilter::Debug, Some(&log_path))?;

    // パニック時にログファイルへバックトレースを書き出す
    {
        let panic_log = log_path.clone();
        std::panic::set_hook(Box::new(move |info| {
            let bt = std::backtrace::Backtrace::force_capture();
            let msg = format!("[PANIC] {info}\n{bt}");
            log::error!("{msg}");
            // log が flush されない場合に備えて直接書き込み
            if let Ok(mut f) = std::fs::OpenOptions::new().append(true).open(&panic_log) {
                use std::io::Write;
                let _ = writeln!(f, "\n{msg}");
            }
        }));
    }

    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size([1280.0, 720.0])
            .with_title("VRM Viewer")
            .with_drag_and_drop(true),
        renderer: eframe::Renderer::Wgpu,
        wgpu_options: eframe::egui_wgpu::WgpuConfiguration {
            wgpu_setup: eframe::egui_wgpu::WgpuSetup::CreateNew(
                eframe::egui_wgpu::WgpuSetupCreateNew {
                    device_descriptor: std::sync::Arc::new(|adapter| {
                        let mut features = eframe::wgpu::Features::default();
                        // ワイヤーフレーム表示用（対応ハードウェアのみ）
                        if adapter.features().contains(eframe::wgpu::Features::POLYGON_MODE_LINE) {
                            features |= eframe::wgpu::Features::POLYGON_MODE_LINE;
                        }
                        eframe::wgpu::DeviceDescriptor {
                            required_features: features,
                            ..Default::default()
                        }
                    }),
                    ..Default::default()
                },
            ),
            ..Default::default()
        },
        ..Default::default()
    };

    eframe::run_native(
        "VRM Viewer",
        options,
        Box::new(|cc| Ok(Box::new(vrm2pmx::viewer::app::ViewerApp::new(cc)))),
    )
    .map_err(|e| anyhow::anyhow!("ビューア起動失敗: {e}"))
}
