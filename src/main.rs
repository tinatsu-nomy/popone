// viewer feature 有効時は Windows GUI サブシステムでビルドし、
// Explorer からの起動時にコンソールウィンドウを表示しない
#![cfg_attr(
    all(feature = "viewer", target_os = "windows"),
    windows_subsystem = "windows"
)]

use popone::{convert, intermediate, pmx, vrm};

use anyhow::{Context, Result};
use clap::Parser;
use std::path::{Path, PathBuf};

#[derive(Parser, Debug)]
#[command(
    name = "popone",
    about = "VRMファイルをPMX形式に変換します\n引数なしで起動するとビューアが開きます"
)]
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

    /// Aスタンスの腕をTスタンスに変換する（FBX用、デフォルト: off）
    #[arg(long)]
    normalize_to_tstance: bool,

    /// ログレベル (error, warn, info, debug)
    #[arg(long, default_value = "info")]
    log_level: String,

    /// unitypackage内のFBXファイル名を指定（省略時は最初のFBXを使用）
    #[arg(long)]
    fbx_name: Option<String>,

    /// アーカイブ内のモデルファイル名を指定（省略時: 1つなら自動、複数ならエラー）
    #[arg(long)]
    model_name: Option<String>,

    /// アーカイブ内のモデル一覧を表示して終了
    #[arg(long)]
    list_models: bool,

    /// 標準ボーン挿入をスキップ（元のボーン構造を維持）
    #[arg(long)]
    raw_structure: bool,

    /// PMX出力倍率（デフォルト: 1.0）
    #[arg(long, default_value = "1.0")]
    scale: f32,
}

/// ロガーセットアップ。
/// stderr には `stderr_level` までのログを出力する。
/// `log_file` が Some の場合、そのパスに DEBUG レベルまで全て書き出す。
fn setup_logging(stderr_level: log::LevelFilter, log_file: Option<&std::path::Path>) -> Result<()> {
    let mut base = fern::Dispatch::new().level(log::LevelFilter::Debug); // グローバル最小フィルター

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
            .write(true)
            .create(true)
            .truncate(true)
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

    base.apply()
        .map_err(|e| anyhow::anyhow!("ロガー初期化失敗: {}", e))
}

/// Windows GUI サブシステムの場合、親コンソールにアタッチして
/// stdout/stdin/stderr を使えるようにする
#[cfg(all(feature = "viewer", target_os = "windows"))]
fn attach_parent_console() {
    extern "system" {
        fn AttachConsole(dw_process_id: u32) -> i32;
        fn CreateFileA(
            name: *const u8,
            access: u32,
            share: u32,
            sa: *mut std::ffi::c_void,
            disp: u32,
            flags: u32,
            template: *mut std::ffi::c_void,
        ) -> *mut std::ffi::c_void;
        fn SetStdHandle(std_handle: u32, handle: *mut std::ffi::c_void) -> i32;
    }

    const GENERIC_READ: u32 = 0x80000000;
    const GENERIC_WRITE: u32 = 0x40000000;
    const FILE_SHARE_READ: u32 = 1;
    const FILE_SHARE_WRITE: u32 = 2;
    const OPEN_EXISTING: u32 = 3;
    const STD_INPUT_HANDLE: u32 = 0xFFFF_FFF6;
    const STD_OUTPUT_HANDLE: u32 = 0xFFFF_FFF5;
    const STD_ERROR_HANDLE: u32 = 0xFFFF_FFF4;
    const INVALID: *mut std::ffi::c_void = -1isize as *mut std::ffi::c_void;

    // SAFETY: All Win32 calls receive valid arguments — string pointers come from
    // null-terminated C literals ("CONIN$"/"CONOUT$"), handle validity is checked
    // against INVALID before use, and null pointers are passed where the API permits.
    // This block only runs once at startup before any I/O occurs.
    unsafe {
        if AttachConsole(0xFFFFFFFF) == 0 {
            return;
        }

        // CONIN$ / CONOUT$ を開いてプロセスの標準ハンドルを差し替え
        let h_in = CreateFileA(
            c"CONIN$".as_ptr().cast(),
            GENERIC_READ,
            FILE_SHARE_READ,
            std::ptr::null_mut(),
            OPEN_EXISTING,
            0,
            std::ptr::null_mut(),
        );
        if h_in != INVALID {
            SetStdHandle(STD_INPUT_HANDLE, h_in);
        }

        // stdout 用ハンドル
        let h_out = CreateFileA(
            c"CONOUT$".as_ptr().cast(),
            GENERIC_WRITE,
            FILE_SHARE_WRITE,
            std::ptr::null_mut(),
            OPEN_EXISTING,
            0,
            std::ptr::null_mut(),
        );
        if h_out != INVALID {
            SetStdHandle(STD_OUTPUT_HANDLE, h_out);
        }

        // stderr 用に別ハンドルを開く（stdout と共有するとクローズ時に二重解放になる）
        let h_err = CreateFileA(
            c"CONOUT$".as_ptr().cast(),
            GENERIC_WRITE,
            FILE_SHARE_WRITE,
            std::ptr::null_mut(),
            OPEN_EXISTING,
            0,
            std::ptr::null_mut(),
        );
        if h_err != INVALID {
            SetStdHandle(STD_ERROR_HANDLE, h_err);
        }

        // Rust の std::io が新しいハンドルを使うよう、
        // 内部バッファをリセットするために std::io::set_output_capture 等は不要
        // — SetStdHandle で OS レベルのハンドルが更新されるため、
        // 以降の Rust print!/stdin は新しいハンドルを使用する
    }
}

/// コンソールを切り離す（ビューア起動前に呼び出す）
#[cfg(all(feature = "viewer", target_os = "windows"))]
fn detach_console() {
    extern "system" {
        fn FreeConsole() -> i32;
    }
    // SAFETY: FreeConsole has no preconditions; it detaches the calling process
    // from its console. Called once before the GUI event loop starts.
    unsafe {
        FreeConsole();
    }
}

/// IrModel のダンプ出力（共通処理）
fn dump_ir(ir: &intermediate::types::IrModel) {
    println!("=== {} dump ===", ir.source_format.label());
    println!("Model name: {}", ir.name);
    println!("Bones: {}", ir.bones.len());
    println!("Meshes: {}", ir.meshes.len());
    println!("Vertices (total): {}", ir.total_vertices());
    println!("Faces (total): {}", ir.total_faces());
    println!("Materials: {}", ir.materials.len());
    println!("Textures: {}", ir.textures.len());
    println!("Morphs: {}", ir.morphs.len());
    println!("Rigidbodies: {}", ir.physics.rigid_bodies.len());
    println!("Joints: {}", ir.physics.joints.len());
    if let Some(ref rig) = ir.rig_type {
        println!(
            "Rig type: {} (Humanoid: {} bones)",
            rig, ir.humanoid_bone_count
        );
    }

    println!("\n--- Bone list ---");
    for (i, bone) in ir.bones.iter().enumerate() {
        let vrm_name = bone.vrm_bone_name.as_deref().unwrap_or("-");
        println!("  [{:3}] {} (vrm: {})", i, bone.name, vrm_name);
    }

    println!("\n--- Morph list ---");
    for morph in &ir.morphs {
        println!("  [panel{}] {}", morph.panel, morph.name);
    }
}

fn main() {
    // GUI サブシステムでも CLI 引数がある場合はコンソール出力を有効にする
    #[cfg(all(feature = "viewer", target_os = "windows"))]
    if std::env::args().len() > 1 {
        attach_parent_console();
    }

    let args = match Args::try_parse() {
        Ok(args) => args,
        Err(e) => {
            let _ = e.print();
            std::process::exit(e.exit_code());
        }
    };

    if let Err(e) = run_main(args) {
        eprintln!("Error: {e:#}");
        std::process::exit(1);
    }
}

fn run_main(mut args: Args) -> Result<()> {
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
                 使い方: popone <入力.vrm> <出力.pmx>\n\
                 ビューア: cargo build --features viewer"
            );
        }
    }

    // unwrap 安全: 上で is_none() チェック済み
    let input = args.input.take().expect("input は is_none チェック済み");

    let ext = input
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();

    // --list-models が非アーカイブファイルに使われた場合はエラー
    if args.list_models && !matches!(ext.as_str(), "zip" | "7z") {
        anyhow::bail!("--list-models はアーカイブファイル（.zip / .7z）専用です");
    }

    // アーカイブ: --list-models はビューアモードより先に処理
    if args.list_models && matches!(ext.as_str(), "zip" | "7z") {
        let data = std::fs::read(&input)
            .with_context(|| format!("アーカイブ読み込み失敗: {}", input.display()))?;
        let format = popone::archive::archive_format_from_ext(&ext)
            .ok_or_else(|| anyhow::anyhow!("未対応のアーカイブ形式: {ext}"))?;
        let contents = popone::archive::list_models(&data, format)
            .context("アーカイブ内モデル一覧取得失敗")?;
        if contents.models.is_empty() {
            println!("No model files found in archive");
        } else {
            for (_, path, _, kind) in &contents.models {
                println!("[{}] {}", kind.label(), path.display());
            }
        }
        return Ok(());
    }

    // --model-name はCLI変換専用（ビューアモードでは使用不可）
    #[cfg(feature = "viewer")]
    if args.model_name.is_some() && args.output.is_none() && !args.dump {
        anyhow::bail!("--model-name はCLI変換時のみ有効です。出力ファイルを指定してください");
    }

    // viewer feature: 出力未指定 → ビューアモードで開く
    #[cfg(feature = "viewer")]
    {
        if args.output.is_none() && !args.dump {
            return run_viewer_with_file(input);
        }
    }

    // アーカイブ経由のPMX変換
    if matches!(ext.as_str(), "zip" | "7z") {
        let output = args.output.as_ref().context(
            "出力ファイルパスを指定してください。\n使い方: popone <入力.zip> <出力.pmx>",
        )?;
        return run_archive_convert(&input, output, &ext, &args);
    }

    let output = args
        .output
        .context("出力ファイルパスを指定してください。\n使い方: popone <入力.vrm> <出力.pmx>")?;

    // ロガー初期化（dump 時はファイルログなし）
    let log_level = args
        .log_level
        .parse::<log::LevelFilter>()
        .unwrap_or(log::LevelFilter::Info);
    let log_path = if args.dump {
        None
    } else {
        Some(output.with_extension("log"))
    };
    setup_logging(log_level, log_path.as_deref()).context("ロガー初期化失敗")?;
    if let Some(ref p) = log_path {
        log::info!("Log file: {}", p.display());
    }

    log::info!("Input file: {}", input.display());

    // 中間表現抽出（VRM / FBX / OBJ / STL / unitypackage 分岐）
    // VRM の場合は glb を保持してテクスチャ書き出しに再利用（二重読み込み回避）
    let (ir, glb_for_tex) = match ext.as_str() {
        "obj" => {
            let ir = popone::obj::extract::load_obj(&input).context("OBJ中間表現の抽出に失敗")?;
            (ir, None)
        }
        "stl" => {
            let ir = popone::stl::extract::load_stl(&input).context("STL中間表現の抽出に失敗")?;
            (ir, None)
        }
        "x" => {
            let ir =
                popone::directx::extract::load_x(&input).context("DirectX中間表現の抽出に失敗")?;
            (ir, None)
        }
        "fbx" => {
            let data = std::fs::read(&input)
                .with_context(|| format!("FBXファイル読み込み失敗: {}", input.display()))?;
            let ir = popone::fbx::extract::extract_ir_model_from_fbx_with_options(
                &data,
                Some(&input),
                args.normalize_pose,
                args.normalize_to_tstance,
            )
            .context("FBX中間表現の抽出に失敗")?;
            (ir, None)
        }
        "unitypackage" => {
            let archive_data = std::fs::read(&input)
                .with_context(|| format!("unitypackage読み込み失敗: {}", input.display()))?;
            let pkg = popone::unitypackage::build_unity_package_index(&archive_data)
                .context("unitypackage展開失敗")?;

            // FBX 一覧を取得
            let fbx_indices: Vec<(usize, String)> = pkg
                .entries
                .iter()
                .enumerate()
                .filter(|(_, e)| e.pathname.to_lowercase().ends_with(".fbx"))
                .map(|(i, e)| {
                    let name = std::path::Path::new(&e.pathname)
                        .file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .to_string();
                    (i, name)
                })
                .collect();

            if fbx_indices.is_empty() {
                return Err(anyhow::anyhow!(
                    ".unitypackage 内に FBX ファイルが見つかりません"
                ));
            }

            if fbx_indices.len() > 1 {
                log::info!("Found {} FBX files in .unitypackage:", fbx_indices.len());
                for (_, name) in &fbx_indices {
                    log::info!("  FBX: {}", name);
                }
            }

            // FBX 選択
            let selected_idx = if let Some(ref target) = args.fbx_name {
                let target_lower = target.to_lowercase();
                fbx_indices
                    .iter()
                    .find(|(_, name)| name.to_lowercase().contains(&target_lower))
                    .map(|(idx, _)| *idx)
                    .ok_or_else(|| {
                        anyhow::anyhow!(
                            "指定された FBX '{}' が見つかりません。利用可能: {}",
                            target,
                            fbx_indices
                                .iter()
                                .map(|(_, n)| n.as_str())
                                .collect::<Vec<_>>()
                                .join(", ")
                        )
                    })?
            } else {
                popone::unitypackage::select_best_fbx_index(&pkg, &fbx_indices)
            };

            let prepared = popone::unitypackage::prepare_pkg_fbx(&pkg, selected_idx)
                .context("Prefab テクスチャ解決失敗")?;
            log::info!("FBX in unitypackage: {}", prepared.model.pathname);

            // unitypackage 経由: fbx_path=None で FBX 近傍テクスチャ検索を無効化
            let mut ir = popone::fbx::extract::extract_ir_model_from_fbx_with_options(
                &prepared.fbx_data,
                None,
                args.normalize_pose,
                args.normalize_to_tstance,
            )
            .context("FBX中間表現の抽出に失敗")?;

            if !prepared.resolved.is_empty() {
                let prefab_label = format!(
                    "prefab({})",
                    std::path::Path::new(&*prepared.model.pathname)
                        .file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                );
                popone::unitypackage::embed_textures_with_prefab(
                    &mut ir,
                    &prepared.textures,
                    &prepared.resolved,
                    &prefab_label,
                );
            } else {
                // フォールバック: 既存のファイル名マッチング
                let textures: Vec<(String, Vec<u8>)> = prepared
                    .textures
                    .iter()
                    .map(|t| (t.display_name.to_string(), t.data.to_vec()))
                    .collect();
                popone::unitypackage::embed_textures_into_ir(&mut ir, &textures);
            }
            (ir, None)
        }
        _ => {
            let glb = vrm::loader::load_glb(&input)
                .with_context(|| format!("GLB読み込み失敗: {}", input.display()))?;
            let version = vrm::detect::detect_version(&glb.document);
            log::info!("VRM version: {:?}", version);
            let all_extensions = vrm::loader::get_raw_extensions(&glb.document);
            let ir = vrm::extract::extract_ir_model_with_options(
                &glb.document,
                &glb.buffers,
                &glb.images,
                &glb.vrm_extension,
                &version,
                &all_extensions,
                args.normalize_pose,
            )
            .context("VRM中間表現の抽出に失敗")?;
            (ir, Some(glb))
        }
    };

    // テクスチャ割当ログ出力
    ir.log_texture_assignments();

    if args.dump {
        dump_ir(&ir);
        return Ok(());
    }

    // 出力ディレクトリ確定
    let output_dir = output.parent().unwrap_or(Path::new(".")).to_path_buf();
    std::fs::create_dir_all(&output_dir)
        .with_context(|| format!("出力ディレクトリ作成失敗: {}", output_dir.display()))?;

    // テクスチャ書き出し（VRM は保持済み glb を再利用）
    let tex_dir = output_dir.join("textures");
    let written_filenames = if let Some(ref glb) = glb_for_tex {
        convert::texture::write_all_textures(&ir.textures, &glb.images, &tex_dir)
            .context("テクスチャ書き出し失敗")?
    } else {
        convert::texture::write_all_textures_from_ir(&ir.textures, &tex_dir)
            .context("テクスチャ書き出し失敗")?
    };

    // PMXモデル構築
    let build_options = pmx::build::PmxBuildOptions {
        align_rigid_rotation: args.align_rigid_rotation,
        no_physics: args.no_physics,
        raw_structure: args.raw_structure,
        scale: args.scale,
    };
    let mut pmx_model = pmx::build::build_pmx_model_with_options(&ir, &build_options)
        .context("PMXモデル構築失敗")?;
    // PSD→PNG 変換でファイル名が変わった場合、PMX テクスチャパスを補正
    for (i, name) in written_filenames.iter().enumerate() {
        if i < pmx_model.textures.len() {
            pmx_model.textures[i] = format!("textures\\{}", name);
        }
    }

    // PMX書き出し
    let output_file = std::fs::File::create(&output)
        .with_context(|| format!("出力ファイル作成失敗: {}", output.display()))?;
    let writer = std::io::BufWriter::new(output_file);

    let header = pmx_model.header.clone();
    let mut pmx_writer = pmx::writer::PmxWriter::new(writer, header);
    pmx_writer
        .write_model(&pmx_model)
        .context("PMX書き出し失敗")?;

    log::info!("Conversion complete: {}", output.display());
    println!(
        "Conversion complete: {} -> {}",
        input.display(),
        output.display()
    );

    Ok(())
}

/// アーカイブ（ZIP/7z）→ PMX 変換
fn run_archive_convert(input: &Path, output: &Path, ext: &str, args: &Args) -> Result<()> {
    let log_level = args
        .log_level
        .parse::<log::LevelFilter>()
        .unwrap_or(log::LevelFilter::Info);
    let log_path = if args.dump {
        None
    } else {
        Some(output.with_extension("log"))
    };
    setup_logging(log_level, log_path.as_deref()).context("ロガー初期化失敗")?;

    log::info!("Input file (archive): {}", input.display());

    let data = std::fs::read(input)
        .with_context(|| format!("アーカイブ読み込み失敗: {}", input.display()))?;
    let format = popone::archive::archive_format_from_ext(ext)
        .ok_or_else(|| anyhow::anyhow!("未対応のアーカイブ形式: {ext}"))?;
    let contents =
        popone::archive::list_models(&data, format).context("アーカイブ内モデル一覧取得失敗")?;

    if contents.models.is_empty() {
        anyhow::bail!("アーカイブ内にモデルファイルが見つかりません");
    }

    // モデル選択
    let selected = match (&args.model_name, contents.models.len()) {
        (Some(name), _) => {
            // 完全一致 → 前方一致 → 部分一致（各段階で一意のみ採用）
            let exact: Vec<usize> = contents
                .models
                .iter()
                .enumerate()
                .filter(|(_, (_, p, _, _))| {
                    p.file_name().and_then(|f| f.to_str()) == Some(name.as_str())
                })
                .map(|(i, _)| i)
                .collect();
            if exact.len() == 1 {
                exact[0]
            } else if exact.len() > 1 {
                let candidates: Vec<String> = exact
                    .iter()
                    .map(|&i| contents.models[i].1.display().to_string())
                    .collect();
                anyhow::bail!(
                    "\"{}\" に完全一致するモデルが {} 個あります:\n  {}\n--list-models で確認し、パスで指定してください。",
                    name, exact.len(), candidates.join("\n  ")
                );
            } else {
                let prefix: Vec<usize> = contents
                    .models
                    .iter()
                    .enumerate()
                    .filter(|(_, (_, p, _, _))| p.to_string_lossy().starts_with(name.as_str()))
                    .map(|(i, _)| i)
                    .collect();
                if prefix.len() == 1 {
                    prefix[0]
                } else if prefix.len() > 1 {
                    let candidates: Vec<String> = prefix
                        .iter()
                        .map(|&i| contents.models[i].1.display().to_string())
                        .collect();
                    anyhow::bail!(
                        "\"{}\" に前方一致するモデルが {} 個あります:\n  {}\nより具体的に指定してください。",
                        name, prefix.len(), candidates.join("\n  ")
                    );
                } else {
                    let substr: Vec<usize> = contents
                        .models
                        .iter()
                        .enumerate()
                        .filter(|(_, (_, p, _, _))| p.to_string_lossy().contains(name.as_str()))
                        .map(|(i, _)| i)
                        .collect();
                    if substr.len() == 1 {
                        substr[0]
                    } else if substr.len() > 1 {
                        let candidates: Vec<String> = substr
                            .iter()
                            .map(|&i| contents.models[i].1.display().to_string())
                            .collect();
                        anyhow::bail!(
                            "\"{}\" に部分一致するモデルが {} 個あります:\n  {}\nより具体的に指定してください。",
                            name, substr.len(), candidates.join("\n  ")
                        );
                    } else {
                        anyhow::bail!(
                            "アーカイブ内に \"{}\" に一致するモデルが見つかりません。\n--list-models で一覧を確認してください。",
                            name
                        );
                    }
                }
            }
        }
        (None, 1) => 0,
        (None, n) => {
            anyhow::bail!(
                "{n} 個のモデルが見つかりました。--model-name で指定するか --list-models で一覧を確認してください"
            );
        }
    };

    log::info!("Selected model: {}", contents.models[selected].1.display());

    let bundle = popone::archive::extract_model_bundle(&data, format, contents, selected)
        .context("モデル展開失敗")?;

    // 種別で分岐して中間表現を構築
    use popone::archive::ArchiveModelKind;
    let ir = match bundle.kind {
        ArchiveModelKind::Pmx => {
            let pmx_model = popone::pmx::reader::read_pmx_from_data(&bundle.model.data)
                .context("PMX読み込み失敗")?;
            popone::pmx::extract::pmx_to_ir_with_aux(
                &pmx_model,
                Path::new("."),
                Some(&bundle.aux_files),
            )
            .context("PMX中間表現の抽出に失敗")?
        }
        ArchiveModelKind::Pmd => {
            let pmd_model = popone::pmd::reader::read_pmd_from_data(&bundle.model.data)
                .context("PMD読み込み失敗")?;
            popone::pmd::extract::pmd_to_ir_with_aux(
                &pmd_model,
                &bundle.model.path,
                Some(&bundle.aux_files),
            )
            .context("PMD中間表現の抽出に失敗")?
        }
        ArchiveModelKind::Fbx => {
            // アーカイブ経由: fbx_path=None で FBX 近傍テクスチャ検索を無効化
            let mut ir = popone::fbx::extract::extract_ir_model_from_fbx_with_options(
                &bundle.model.data,
                None,
                args.normalize_pose,
                args.normalize_to_tstance,
            )
            .context("FBX中間表現の抽出に失敗")?;
            let label = format!(
                "archive({})",
                input.file_name().unwrap_or_default().to_string_lossy()
            );
            popone::unitypackage::embed_textures_into_ir_with_label(
                &mut ir,
                &bundle.textures,
                &label,
            );
            ir
        }
        ArchiveModelKind::Vrm | ArchiveModelKind::Glb => {
            let glb = popone::vrm::loader::load_glb_from_data(&bundle.model.data)
                .context("VRM/GLB読み込み失敗")?;
            let version = popone::vrm::detect::detect_version(&glb.document);
            log::info!("VRM version: {:?}", version);
            let all_extensions = popone::vrm::loader::get_raw_extensions(&glb.document);
            popone::vrm::extract::extract_ir_model_with_options(
                &glb.document,
                &glb.buffers,
                &glb.images,
                &glb.vrm_extension,
                &version,
                &all_extensions,
                args.normalize_pose,
            )
            .context("VRM中間表現の抽出に失敗")?
        }
        ArchiveModelKind::Obj => {
            let base_dir = bundle
                .model
                .path
                .parent()
                .unwrap_or(std::path::Path::new("."));
            let name = bundle
                .model
                .path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("Model");
            popone::obj::extract::load_obj_from_data(
                &bundle.model.data,
                name,
                base_dir,
                Some(&bundle.aux_files),
            )
            .context("OBJ中間表現の抽出に失敗")?
        }
        ArchiveModelKind::Stl => {
            let name = bundle
                .model
                .path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("Model");
            popone::stl::extract::load_stl_from_data(&bundle.model.data, name)
                .context("STL中間表現の抽出に失敗")?
        }
        ArchiveModelKind::DirectX => {
            let base_dir = bundle
                .model
                .path
                .parent()
                .unwrap_or(std::path::Path::new("."));
            let name = bundle
                .model
                .path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("Model");
            popone::directx::extract::load_x_from_data(
                &bundle.model.data,
                name,
                base_dir,
                Some(&bundle.aux_files),
            )
            .context("DirectX中間表現の抽出に失敗")?
        }
        ArchiveModelKind::UnityPackage => {
            // アーカイブ内 .unitypackage を二重展開
            let pkg = popone::unitypackage::build_unity_package_index(&bundle.model.data)
                .context("アーカイブ内 unitypackage 展開失敗")?;

            // FBX 一覧を取得
            let fbx_indices: Vec<(usize, String)> = pkg
                .entries
                .iter()
                .enumerate()
                .filter(|(_, e)| e.pathname.to_lowercase().ends_with(".fbx"))
                .map(|(i, e)| {
                    let name = std::path::Path::new(&e.pathname)
                        .file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .to_string();
                    (i, name)
                })
                .collect();

            if fbx_indices.is_empty() {
                anyhow::bail!("アーカイブ内 .unitypackage に FBX が見つかりません");
            }

            let selected_idx = if let Some(ref target) = args.fbx_name {
                let target_lower = target.to_lowercase();
                fbx_indices
                    .iter()
                    .find(|(_, name)| name.to_lowercase().contains(&target_lower))
                    .map(|(idx, _)| *idx)
                    .ok_or_else(|| {
                        anyhow::anyhow!("指定された FBX '{}' が見つかりません", target)
                    })?
            } else {
                popone::unitypackage::select_best_fbx_index(&pkg, &fbx_indices)
            };

            let prepared = popone::unitypackage::prepare_pkg_fbx(&pkg, selected_idx)
                .context("Prefab テクスチャ解決失敗")?;
            log::info!(
                "FBX in unitypackage: {} textures: {}",
                prepared.model.pathname,
                prepared.textures.len()
            );

            // unitypackage 経由: fbx_path=None で FBX 近傍テクスチャ検索を無効化
            let mut ir = popone::fbx::extract::extract_ir_model_from_fbx_with_options(
                &prepared.fbx_data,
                None,
                args.normalize_pose,
                args.normalize_to_tstance,
            )
            .context("FBX中間表現の抽出に失敗")?;

            if !prepared.resolved.is_empty() {
                let prefab_label = format!(
                    "prefab({})",
                    std::path::Path::new(&*prepared.model.pathname)
                        .file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                );
                popone::unitypackage::embed_textures_with_prefab(
                    &mut ir,
                    &prepared.textures,
                    &prepared.resolved,
                    &prefab_label,
                );
            } else {
                let textures: Vec<(String, Vec<u8>)> = prepared
                    .textures
                    .iter()
                    .map(|t| (t.display_name.to_string(), t.data.to_vec()))
                    .collect();
                popone::unitypackage::embed_textures_into_ir(&mut ir, &textures);
            }
            ir
        }
    };

    // テクスチャ割当ログ出力
    ir.log_texture_assignments();

    if args.dump {
        dump_ir(&ir);
        return Ok(());
    }

    // テクスチャ書き出し（アーカイブ経由は常に write_all_textures_from_ir を使用）
    let output_dir = output.parent().unwrap_or(Path::new(".")).to_path_buf();
    std::fs::create_dir_all(&output_dir)
        .with_context(|| format!("出力ディレクトリ作成失敗: {}", output_dir.display()))?;
    let tex_dir = output_dir.join("textures");
    let written_filenames = convert::texture::write_all_textures_from_ir(&ir.textures, &tex_dir)
        .context("テクスチャ書き出し失敗")?;

    // PMXモデル構築 & 書き出し
    let build_options = pmx::build::PmxBuildOptions {
        align_rigid_rotation: args.align_rigid_rotation,
        no_physics: args.no_physics,
        raw_structure: args.raw_structure,
        scale: args.scale,
    };
    let mut pmx_model = pmx::build::build_pmx_model_with_options(&ir, &build_options)
        .context("PMXモデル構築失敗")?;
    // PSD→PNG 変換でファイル名が変わった場合、PMX テクスチャパスを補正
    for (i, name) in written_filenames.iter().enumerate() {
        if i < pmx_model.textures.len() {
            pmx_model.textures[i] = format!("textures\\{}", name);
        }
    }
    let output_file = std::fs::File::create(output)
        .with_context(|| format!("出力ファイル作成失敗: {}", output.display()))?;
    let writer = std::io::BufWriter::new(output_file);
    let header = pmx_model.header.clone();
    let mut pmx_writer = pmx::writer::PmxWriter::new(writer, header);
    pmx_writer
        .write_model(&pmx_model)
        .context("PMX書き出し失敗")?;

    log::info!("Conversion complete: {}", output.display());
    println!(
        "Conversion complete: {} -> {}",
        input.display(),
        output.display()
    );
    Ok(())
}

#[cfg(feature = "viewer")]
fn run_viewer() -> Result<()> {
    run_viewer_with_initial(None)
}

#[cfg(feature = "viewer")]
fn run_viewer_with_file(input: PathBuf) -> Result<()> {
    run_viewer_with_initial(Some(input))
}

/// ビューア共通起動（ログ・パニックフック・NativeOptions 設定）
#[cfg(feature = "viewer")]
fn run_viewer_with_initial(initial_file: Option<PathBuf>) -> Result<()> {
    // シングルインスタンス: ログ初期化前に判定（不要なログファイル生成・ローテーション防止）
    #[cfg(target_os = "windows")]
    let can_rotate = {
        use popone::viewer::single_instance::InstanceCheck;
        match popone::viewer::single_instance::try_send_to_existing(initial_file.as_deref()) {
            InstanceCheck::Forwarded => return Ok(()),
            InstanceCheck::Primary => true,
            InstanceCheck::FallbackStart => false, // 既存検出済み→ログ削除しない
        }
    };
    #[cfg(not(target_os = "windows"))]
    let can_rotate = true;

    let logs_dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.join("logs")))
        .unwrap_or_else(|| std::path::PathBuf::from("logs"));
    let _ = std::fs::create_dir_all(&logs_dir);
    if can_rotate {
        rotate_logs(&logs_dir, 5);
    }

    let timestamp = chrono::Local::now().format("%Y%m%d_%H%M%S");
    let log_path = logs_dir.join(format!("popone_{timestamp}.log"));
    setup_logging(log::LevelFilter::Debug, Some(&log_path))?;

    {
        let panic_log = log_path.clone();
        std::panic::set_hook(Box::new(move |info| {
            let bt = std::backtrace::Backtrace::force_capture();
            let msg = format!("[PANIC] {info}\n{bt}");
            log::error!("{msg}");
            if let Ok(mut f) = std::fs::OpenOptions::new().append(true).open(&panic_log) {
                use std::io::Write;
                let _ = writeln!(f, "\n{msg}");
            }
            // パニックログを panic_yyyymmdd_hhmmss.log としてコピー
            if let Some(name) = panic_log.file_name().and_then(|n| n.to_str()) {
                if let Some(rest) = name.strip_prefix("popone_") {
                    let panic_name = format!("panic_{rest}");
                    let panic_path = panic_log.with_file_name(panic_name);
                    let _ = std::fs::copy(&panic_log, &panic_path);
                }
            }
        }));
    }

    if let Some(ref path) = initial_file {
        log::info!("Viewer mode: {}", path.display());
    }

    let png = include_bytes!("../assets/popone_icon.png");
    let img = image::load_from_memory(png).context("アイコン画像の読み込み失敗")?;
    let rgba = img.to_rgba8();
    let (w, h) = rgba.dimensions();
    let icon = eframe::egui::IconData {
        rgba: rgba.into_raw(),
        width: w,
        height: h,
    };

    // セッション設定の読み込み
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.to_path_buf()))
        .unwrap_or_else(|| std::path::PathBuf::from("."));
    let app_config = popone::viewer::app::persistence::load_config(&exe_dir);

    // NativeOptions: 保存済み設定があればサイズを適用（位置は初回フレームで適用）
    let inner_size = app_config
        .as_ref()
        .and_then(|c| c.window.as_ref())
        .map(|w| [w.width, w.height])
        .unwrap_or([1280.0, 720.0]);
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size(inner_size)
            .with_title(format!(
                "POPONE Model Viewer v{}",
                env!("CARGO_PKG_VERSION")
            ))
            .with_drag_and_drop(true)
            .with_icon(icon),
        renderer: eframe::Renderer::Wgpu,
        wgpu_options: eframe::egui_wgpu::WgpuConfiguration {
            wgpu_setup: eframe::egui_wgpu::WgpuSetup::CreateNew(
                eframe::egui_wgpu::WgpuSetupCreateNew {
                    device_descriptor: std::sync::Arc::new(|adapter| {
                        let mut features = eframe::wgpu::Features::default();
                        if adapter
                            .features()
                            .contains(eframe::wgpu::Features::POLYGON_MODE_LINE)
                        {
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

    // ビューア起動前にコンソールを切り離す
    #[cfg(target_os = "windows")]
    detach_console();

    run_viewer_inner(
        options,
        logs_dir,
        log_path,
        initial_file,
        exe_dir,
        app_config,
    )
}

#[cfg(feature = "viewer")]
fn run_viewer_inner(
    options: eframe::NativeOptions,
    logs_dir: PathBuf,
    log_path: PathBuf,
    initial_file: Option<PathBuf>,
    exe_dir: PathBuf,
    app_config: Option<popone::viewer::app::persistence::AppConfig>,
) -> Result<()> {
    eframe::run_native(
        "Viewer",
        options,
        Box::new(move |cc| {
            let mut app =
                popone::viewer::app::ViewerApp::new(cc, logs_dir, log_path, exe_dir, app_config);
            if let Some(path) = initial_file {
                app.pending.bg_state.submit_dispatch(
                    popone::viewer::app::pending::PendingLoadDispatch {
                        path,
                        append: false,
                        overlay: popone::viewer::app::pending::PendingOverlay::WaitingOverlay,
                        preloaded: None,
                    },
                );
            }
            Ok(Box::new(app))
        }),
    )
    .map_err(|e| anyhow::anyhow!("ビューア起動失敗: {e}"))
}

/// logs ディレクトリ内の古いログファイルを削除（最新 keep 件を保持）
#[cfg(feature = "viewer")]
fn rotate_logs(logs_dir: &std::path::Path, keep: usize) {
    let mut entries: Vec<_> = std::fs::read_dir(logs_dir)
        .into_iter()
        .flatten()
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.path()
                .file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with("popone_") && n.ends_with(".log"))
        })
        .collect();
    // ファイル名でソート（タイムスタンプ順）→ 降順
    entries.sort_by_key(|e| std::cmp::Reverse(e.file_name()));
    // keep 件より古いものを削除
    for entry in entries.into_iter().skip(keep) {
        let _ = std::fs::remove_file(entry.path());
    }
}
