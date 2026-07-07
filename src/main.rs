// When the viewer feature is enabled, build for the Windows GUI subsystem
// so launching from Explorer does not pop up a console window.
#![cfg_attr(
    all(feature = "viewer", target_os = "windows"),
    windows_subsystem = "windows"
)]

use popone::{convert, intermediate, pmx, vrm};

use anyhow::{Context, Result};
use clap::Parser;
use rust_i18n::t;
use std::path::{Path, PathBuf};
use std::sync::Arc;

// Embed translations (`locales/{ja,en,zh}.yml`) for the binary crate.
// `t!()` is resolved per-crate, so the bin needs its own `i18n!()` call
// in addition to the one in `lib.rs`.
rust_i18n::i18n!("locales", fallback = "en");

// Help text on each `///` doc and the `about` line below is the static
// fallback used when locale loading fails. The actual user-facing help
// strings are overridden at runtime in `main()` via `Command::about()`
// and `Command::mut_arg(...).help(...)`, so they always reflect the
// detected (or user-selected) locale.
#[derive(Parser, Debug)]
#[command(name = "popone", about = "Convert VRM files to PMX format")]
struct Args {
    /// Input file path
    input: Option<PathBuf>,

    /// Output PMX file path
    output: Option<PathBuf>,

    /// Print bone/vertex counts only; do not generate a PMX
    #[arg(long)]
    dump: bool,

    /// Skip physics conversion
    #[arg(long)]
    no_physics: bool,

    /// Align rigid body rotation to bone direction
    #[arg(long)]
    align_rigid_rotation: bool,

    /// Convert T-pose arms to A-stance
    #[arg(long)]
    normalize_pose: bool,

    /// Convert A-stance arms to T-stance (FBX only)
    #[arg(long)]
    normalize_to_tstance: bool,

    /// Log level (error, warn, info, debug)
    #[arg(long, default_value = "info")]
    log_level: String,

    /// FBX file name inside a unitypackage
    #[arg(long)]
    fbx_name: Option<String>,

    /// Model file name inside an archive
    #[arg(long)]
    model_name: Option<String>,

    /// List models inside the archive and exit
    #[arg(long)]
    list_models: bool,

    /// Skip standard bone insertion (preserve the original bone structure)
    #[arg(long)]
    raw_structure: bool,

    /// PMX output scale factor
    #[arg(long, default_value = "1.0")]
    scale: f32,
}

/// Build a `clap::Command` whose `about` and per-argument `help` strings
/// are localized via `rust-i18n`. The command derived by `#[derive(Parser)]`
/// is used as the structural base; only the human-readable text is
/// overridden so that `--help` and clap's built-in usage messages reflect
/// the detected locale.
fn build_localized_command() -> clap::Command {
    use clap::CommandFactory;
    Args::command()
        .about(format!(
            "{}\n{}",
            t!("cli.about.summary"),
            t!("cli.about.viewer_hint")
        ))
        .mut_arg("input", |a| a.help(t!("cli.arg.input").to_string()))
        .mut_arg("output", |a| a.help(t!("cli.arg.output").to_string()))
        .mut_arg("dump", |a| a.help(t!("cli.arg.dump").to_string()))
        .mut_arg("no_physics", |a| {
            a.help(t!("cli.arg.no_physics").to_string())
        })
        .mut_arg("align_rigid_rotation", |a| {
            a.help(t!("cli.arg.align_rigid_rotation").to_string())
        })
        .mut_arg("normalize_pose", |a| {
            a.help(t!("cli.arg.normalize_pose").to_string())
        })
        .mut_arg("normalize_to_tstance", |a| {
            a.help(t!("cli.arg.normalize_to_tstance").to_string())
        })
        .mut_arg("log_level", |a| a.help(t!("cli.arg.log_level").to_string()))
        .mut_arg("fbx_name", |a| a.help(t!("cli.arg.fbx_name").to_string()))
        .mut_arg("model_name", |a| {
            a.help(t!("cli.arg.model_name").to_string())
        })
        .mut_arg("list_models", |a| {
            a.help(t!("cli.arg.list_models").to_string())
        })
        .mut_arg("raw_structure", |a| {
            a.help(t!("cli.arg.raw_structure").to_string())
        })
        .mut_arg("scale", |a| a.help(t!("cli.arg.scale").to_string()))
}

use popone::SharedLogBuffer;

/// `Write` wrapper used to feed `fern`.
struct SharedLogBufferWriter(SharedLogBuffer);

/// Maximum log-buffer size (16 MB). The head is trimmed when it overflows.
const LOG_BUFFER_MAX_BYTES: usize = 16 * 1024 * 1024;

impl std::io::Write for SharedLogBufferWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        if let Ok(mut lb) = self.0.lock() {
            lb.data.extend(buf.iter().copied());
            lb.total_written += buf.len();
            // Trim the head once the cap is exceeded (keep the most recent logs)
            if lb.data.len() > LOG_BUFFER_MAX_BYTES {
                let excess = lb.data.len() - LOG_BUFFER_MAX_BYTES;
                lb.data.drain(..excess);
            }
        }
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// Flush the in-memory buffer to a file in one shot.
#[cfg(feature = "viewer")]
fn flush_log_buffer(buffer: &SharedLogBuffer, path: &std::path::Path) {
    if let Ok(mut lb) = buffer.lock() {
        if !lb.data.is_empty() {
            let _ = std::fs::write(path, lb.data.make_contiguous());
        }
    }
}

/// Logger setup.
/// Logs up to `stderr_level` are written to stderr.
/// When `log_file` is Some, every record up to DEBUG is written to that path.
/// When `log_buffer` is Some, records go into an in-memory buffer instead of a file (used by the viewer).
fn setup_logging(
    stderr_level: log::LevelFilter,
    log_file: Option<&std::path::Path>,
    log_buffer: Option<SharedLogBuffer>,
) -> Result<()> {
    let mut base = fern::Dispatch::new().level(log::LevelFilter::Debug); // Global floor

    // stderr: user-specified level
    base = base.chain(
        fern::Dispatch::new()
            .level(stderr_level)
            .format(|out, msg, rec| {
                let now = chrono::Local::now().format("%H:%M:%S%.3f");
                out.finish(format_args!("[{}][{}] {}", now, rec.level(), msg))
            })
            .chain(std::io::stderr()),
    );

    // DEBUG sink: in-memory buffer (viewer) or file (CLI)
    if let Some(buffer) = log_buffer {
        base = base.chain(
            fern::Dispatch::new()
                .level(log::LevelFilter::Debug)
                .format(|out, msg, rec| {
                    let now = chrono::Local::now().format("%H:%M:%S%.3f");
                    out.finish(format_args!("[{}][{}] {}", now, rec.level(), msg))
                })
                .chain(Box::new(SharedLogBufferWriter(buffer)) as Box<dyn std::io::Write + Send>),
        );
    } else if let Some(path) = log_file {
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
        .map_err(|e| anyhow::anyhow!("{}: {}", t!("cli.error.logger_init_failed"), e))
}

/// On the Windows GUI subsystem, attach to the parent console
/// so stdout/stdin/stderr are usable.
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

        // Open CONIN$ / CONOUT$ and swap the process's standard handles
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

        // Handle for stdout
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

        // Open a separate handle for stderr (sharing with stdout would double-free on close)
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

        // No need to call std::io::set_output_capture or reset Rust's internal buffers --
        // SetStdHandle updates the handle at the OS level, so subsequent Rust print! / stdin
        // calls automatically pick up the new handle.
    }
}

/// Detach from the console (called before launching the viewer).
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

/// IrModel dump output (shared between CLI flows). All user-facing
/// labels are localized; numeric values and identifier names (bone names,
/// morph names, etc.) are passed through unchanged so that automation that
/// reads names still works.
fn dump_ir(ir: &intermediate::types::IrModel) {
    println!(
        "{}",
        t!("cli.dump.header", format = ir.source_format.label())
    );
    println!("{}", t!("cli.dump.model_name", name = ir.name.clone()));
    println!(
        "{}",
        t!("cli.dump.bones", count = ir.bones.len().to_string())
    );
    println!(
        "{}",
        t!("cli.dump.meshes", count = ir.meshes.len().to_string())
    );
    println!(
        "{}",
        t!(
            "cli.dump.vertices_total",
            count = ir.total_vertices().to_string()
        )
    );
    println!(
        "{}",
        t!("cli.dump.faces_total", count = ir.total_faces().to_string())
    );
    println!(
        "{}",
        t!("cli.dump.materials", count = ir.materials.len().to_string())
    );
    println!(
        "{}",
        t!("cli.dump.textures", count = ir.textures.len().to_string())
    );
    println!(
        "{}",
        t!("cli.dump.morphs", count = ir.morphs.len().to_string())
    );
    println!(
        "{}",
        t!(
            "cli.dump.rigidbodies",
            count = ir.physics.rigid_bodies.len().to_string()
        )
    );
    println!(
        "{}",
        t!(
            "cli.dump.joints",
            count = ir.physics.joints.len().to_string()
        )
    );
    if let Some(ref rig) = ir.rig_type {
        println!(
            "{}",
            t!(
                "cli.dump.rig_type",
                rig = rig.to_string(),
                count = ir.humanoid_bone_count.to_string()
            )
        );
    }

    println!("\n{}", t!("cli.dump.bone_list_header"));
    for (i, bone) in ir.bones.iter().enumerate() {
        let vrm_name = bone.vrm_bone_name.as_deref().unwrap_or("-");
        println!(
            "{}",
            t!(
                "cli.dump.bone_entry",
                index = format!("{:3}", i),
                name = bone.name.clone(),
                vrm = vrm_name.to_string()
            )
        );
    }

    println!("\n{}", t!("cli.dump.morph_list_header"));
    for morph in &ir.morphs {
        println!(
            "{}",
            t!(
                "cli.dump.morph_entry",
                panel = morph.panel.to_string(),
                name = morph.name.clone()
            )
        );
    }
}

fn main() {
    // Apply OS-detected locale before any user-visible string is generated.
    popone::i18n::init_default_locale();

    // Even on the GUI subsystem, when CLI args are present we want console output
    #[cfg(all(feature = "viewer", target_os = "windows"))]
    if std::env::args().len() > 1 {
        attach_parent_console();
    }

    use clap::FromArgMatches;
    let matches = match build_localized_command().try_get_matches() {
        Ok(m) => m,
        Err(e) => {
            let _ = e.print();
            std::process::exit(e.exit_code());
        }
    };
    let args = match Args::from_arg_matches(&matches) {
        Ok(a) => a,
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(2);
        }
    };

    if let Err(e) = run_main(args) {
        eprintln!("{}: {e:#}", t!("cli.output.error_prefix"));
        std::process::exit(1);
    }
}

fn run_main(mut args: Args) -> Result<()> {
    // No args -> launch the viewer
    if args.input.is_none() {
        #[cfg(feature = "viewer")]
        {
            return run_viewer();
        }
        #[cfg(not(feature = "viewer"))]
        {
            anyhow::bail!("{}", t!("cli.error.viewer_feature_required"));
        }
    }

    // unwrap safe: is_none() was checked above
    let input = args.input.take().expect("input は is_none チェック済み");

    let ext = popone::path_ext_lower(&input);

    // Reject `--list-models` on non-archive inputs
    if args.list_models && !matches!(ext.as_str(), "zip" | "7z") {
        anyhow::bail!("{}", t!("cli.error.list_models_archive_only"));
    }

    // Archives: handle `--list-models` before the viewer-mode branch
    if args.list_models && matches!(ext.as_str(), "zip" | "7z") {
        let data = std::fs::read(&input).with_context(|| {
            t!(
                "cli.error.archive_load_failed",
                path = input.display().to_string()
            )
            .to_string()
        })?;
        let format = popone::archive::archive_format_from_ext(&ext).with_context(|| {
            t!("cli.error.unsupported_archive_format", ext = ext.clone()).to_string()
        })?;
        let contents = popone::archive::list_models(&data, format)
            .with_context(|| t!("cli.error.archive_list_failed").to_string())?;
        if contents.models.is_empty() {
            println!("{}", t!("cli.output.no_models_in_archive"));
        } else {
            for (_, path, _, kind) in &contents.models {
                println!(
                    "{}",
                    t!(
                        "cli.output.archive_entry",
                        kind = kind.label().to_string(),
                        path = path.display().to_string()
                    )
                );
            }
        }
        return Ok(());
    }

    // `--model-name` is for CLI conversion only (not allowed in viewer mode)
    #[cfg(feature = "viewer")]
    if args.model_name.is_some() && args.output.is_none() && !args.dump {
        anyhow::bail!("{}", t!("cli.error.model_name_cli_only"));
    }

    // viewer feature: output not specified -> open in viewer mode
    #[cfg(feature = "viewer")]
    {
        if args.output.is_none() && !args.dump {
            return run_viewer_with_file(input);
        }
    }

    // PMX conversion via an archive
    if matches!(ext.as_str(), "zip" | "7z") {
        let output = args
            .output
            .as_ref()
            .with_context(|| t!("cli.error.output_required_zip").to_string())?;
        return run_archive_convert(&input, output, &ext, &args);
    }

    let output = args
        .output
        .as_ref()
        .with_context(|| t!("cli.error.output_required_vrm").to_string())?
        .clone();

    // Initialize the logger (no file log on `--dump`)
    let log_level = args
        .log_level
        .parse::<log::LevelFilter>()
        .unwrap_or(log::LevelFilter::Info);
    let log_path = if args.dump {
        None
    } else {
        Some(output.with_extension("log"))
    };
    setup_logging(log_level, log_path.as_deref(), None)
        .with_context(|| t!("cli.error.logger_init_failed").to_string())?;
    if let Some(ref p) = log_path {
        log::info!("Log file: {}", p.display());
    }

    log::info!("Input file: {}", input.display());

    // IR extraction (branches on VRM / FBX / OBJ / STL / unitypackage).
    // For VRM, keep the glb so texture writing can reuse it (avoids a second load).
    let (ir, glb_for_tex) = match ext.as_str() {
        "obj" => {
            let ir = popone::obj::extract::load_obj(&input)
                .with_context(|| t!("cli.error.obj_extract_failed").to_string())?;
            (ir, None)
        }
        "stl" => {
            let ir = popone::stl::extract::load_stl(&input)
                .with_context(|| t!("cli.error.stl_extract_failed").to_string())?;
            (ir, None)
        }
        "x" => {
            let ir = popone::directx::extract::load_x(&input)
                .with_context(|| t!("cli.error.directx_extract_failed").to_string())?;
            (ir, None)
        }
        "fbx" => {
            let data = std::fs::read(&input).with_context(|| {
                t!(
                    "cli.error.fbx_read_failed",
                    path = input.display().to_string()
                )
                .to_string()
            })?;
            let ir = popone::fbx::extract::extract_ir_model_from_fbx_with_options(
                &data,
                Some(&input),
                args.normalize_pose,
                args.normalize_to_tstance,
            )
            .with_context(|| t!("cli.error.fbx_extract_failed").to_string())?;
            (ir, None)
        }
        "unitypackage" => {
            let archive_data = std::fs::read(&input).with_context(|| {
                t!(
                    "cli.error.unitypackage_read_failed",
                    path = input.display().to_string()
                )
                .to_string()
            })?;
            let pkg = popone::unitypackage::build_unity_package_index(&archive_data)
                .with_context(|| t!("cli.error.unitypackage_extract_failed").to_string())?;

            // Collect the FBX entries
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
                return Err(anyhow::anyhow!("{}", t!("cli.error.unitypackage_no_fbx")));
            }

            if fbx_indices.len() > 1 {
                log::info!("Found {} FBX files in .unitypackage:", fbx_indices.len());
                for (_, name) in &fbx_indices {
                    log::info!("  FBX: {}", name);
                }
            }

            // Pick an FBX
            let selected_idx = if let Some(ref target) = args.fbx_name {
                let target_lower = target.to_lowercase();
                fbx_indices
                    .iter()
                    .find(|(_, name)| name.to_lowercase().contains(&target_lower))
                    .map(|(idx, _)| *idx)
                    .ok_or_else(|| {
                        let candidates = fbx_indices
                            .iter()
                            .map(|(_, n)| n.as_str())
                            .collect::<Vec<_>>()
                            .join(", ");
                        anyhow::anyhow!(
                            "{}",
                            t!(
                                "cli.error.fbx_not_found",
                                name = target.clone(),
                                candidates = candidates
                            )
                        )
                    })?
            } else {
                popone::unitypackage::select_best_fbx_index(&pkg, &fbx_indices)
            };

            let prepared = popone::unitypackage::prepare_pkg_fbx(&pkg, selected_idx)
                .with_context(|| t!("cli.error.prefab_resolve_failed").to_string())?;
            log::info!("FBX in unitypackage: {}", prepared.model.pathname);

            // Via unitypackage: passing fbx_path=None disables the texture lookup near the FBX
            let mut ir = popone::fbx::extract::extract_ir_model_from_fbx_with_options(
                &prepared.fbx_data,
                None,
                args.normalize_pose,
                args.normalize_to_tstance,
            )
            .with_context(|| t!("cli.error.fbx_extract_failed").to_string())?;

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
                // Fallback: legacy filename-based matching
                let textures: Vec<(String, Arc<[u8]>)> = prepared
                    .textures
                    .iter()
                    .map(|t| (t.display_name.to_string(), Arc::clone(&t.data)))
                    .collect();
                popone::unitypackage::embed_textures_into_ir(&mut ir, &textures);
            }
            (ir, None)
        }
        _ => {
            let glb = vrm::loader::load_glb(&input).with_context(|| {
                t!(
                    "cli.error.glb_load_failed",
                    path = input.display().to_string()
                )
                .to_string()
            })?;
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
            .with_context(|| t!("cli.error.vrm_extract_failed").to_string())?;
            (ir, Some(glb))
        }
    };

    // Log the texture assignments
    ir.log_texture_assignments();

    if args.dump {
        dump_ir(&ir);
        return Ok(());
    }

    // Resolve the output directory
    let output_dir = output.parent().unwrap_or(Path::new(".")).to_path_buf();
    std::fs::create_dir_all(&output_dir).with_context(|| {
        t!(
            "cli.error.output_dir_create_failed",
            path = output_dir.display().to_string()
        )
        .to_string()
    })?;

    // Write textures (for VRM, reuse the previously loaded glb)
    let tex_dir = output_dir.join("textures");
    let written_filenames = if let Some(ref glb) = glb_for_tex {
        convert::texture::write_all_textures(&ir.textures, &glb.images, &tex_dir)
            .with_context(|| t!("cli.error.texture_write_failed").to_string())?
    } else {
        convert::texture::write_all_textures_from_ir(&ir.textures, &tex_dir)
            .with_context(|| t!("cli.error.texture_write_failed").to_string())?
    };

    // Build the PMX model
    let build_options = pmx::build::PmxBuildOptions {
        align_rigid_rotation: args.align_rigid_rotation,
        no_physics: args.no_physics,
        raw_structure: args.raw_structure,
        scale: args.scale,
    };
    let (mut pmx_model, toon_textures) =
        pmx::build::build_pmx_model_with_options(&ir, &build_options)
            .with_context(|| t!("cli.error.pmx_build_failed").to_string())?;
    // Patch PMX texture paths when PSD->PNG conversion changed the filename
    for (i, name) in written_filenames.iter().enumerate() {
        if i < pmx_model.textures.len() {
            pmx_model.textures[i] = format!("textures\\{}", name);
        }
    }
    // Write generated toon textures to disk and patch the PMX paths
    let base_tex_count = ir.textures.len();
    let toon_written =
        popone::convert::texture::write_all_textures_from_ir(&toon_textures, &tex_dir)?;
    for (i, name) in toon_written.iter().enumerate() {
        let pmx_idx = base_tex_count + i;
        if pmx_idx < pmx_model.textures.len() {
            pmx_model.textures[pmx_idx] = format!("textures\\{}", name);
        }
    }

    // Write the PMX
    let output_file = std::fs::File::create(&output).with_context(|| {
        t!(
            "cli.error.output_file_create_failed",
            path = output.display().to_string()
        )
        .to_string()
    })?;
    let writer = std::io::BufWriter::new(output_file);

    let header = pmx_model.header.clone();
    let mut pmx_writer = pmx::writer::PmxWriter::new(writer, header);
    pmx_writer
        .write_model(&pmx_model)
        .with_context(|| t!("cli.error.pmx_write_failed").to_string())?;

    log::info!("Conversion complete: {}", output.display());
    println!(
        "{}",
        t!(
            "cli.output.conversion_complete",
            input = input.display().to_string(),
            output = output.display().to_string()
        )
    );

    Ok(())
}

/// Archive (ZIP / 7z) -> PMX conversion.
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
    setup_logging(log_level, log_path.as_deref(), None)
        .with_context(|| t!("cli.error.logger_init_failed").to_string())?;

    log::info!("Input file (archive): {}", input.display());

    let data = std::fs::read(input).with_context(|| {
        t!(
            "cli.error.archive_load_failed",
            path = input.display().to_string()
        )
        .to_string()
    })?;
    let format = popone::archive::archive_format_from_ext(ext).with_context(|| {
        t!(
            "cli.error.unsupported_archive_format",
            ext = ext.to_string()
        )
        .to_string()
    })?;
    let contents = popone::archive::list_models(&data, format)
        .with_context(|| t!("cli.error.archive_list_failed").to_string())?;

    if contents.models.is_empty() {
        anyhow::bail!("{}", t!("cli.error.archive_no_models_found"));
    }

    // Pick a model
    let selected = match (&args.model_name, contents.models.len()) {
        (Some(name), _) => {
            // Exact -> prefix -> substring (only accept when uniquely matched at each stage)
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
                    "{}",
                    t!(
                        "cli.error.model_name_exact_multiple",
                        name = name.clone(),
                        count = exact.len().to_string(),
                        candidates = candidates.join("\n  ")
                    )
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
                        "{}",
                        t!(
                            "cli.error.model_name_prefix_multiple",
                            name = name.clone(),
                            count = prefix.len().to_string(),
                            candidates = candidates.join("\n  ")
                        )
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
                            "{}",
                            t!(
                                "cli.error.model_name_substr_multiple",
                                name = name.clone(),
                                count = substr.len().to_string(),
                                candidates = candidates.join("\n  ")
                            )
                        );
                    } else {
                        anyhow::bail!(
                            "{}",
                            t!("cli.error.model_name_no_match", name = name.clone())
                        );
                    }
                }
            }
        }
        (None, 1) => 0,
        (None, n) => {
            anyhow::bail!(
                "{}",
                t!("cli.error.archive_multiple_models", count = n.to_string())
            );
        }
    };

    log::info!("Selected model: {}", contents.models[selected].1.display());

    let bundle = popone::archive::extract_model_bundle(&data, format, contents, selected)
        .with_context(|| t!("cli.error.archive_model_extract_failed").to_string())?;

    // Build the IR by branching on the model kind
    use popone::archive::ArchiveModelKind;
    let ir = match bundle.kind {
        ArchiveModelKind::Pmx => {
            let pmx_model = popone::pmx::reader::read_pmx_from_data(&bundle.model.data)
                .with_context(|| t!("cli.error.pmx_read_failed").to_string())?;
            popone::pmx::extract::pmx_to_ir_with_aux(
                &pmx_model,
                Path::new("."),
                Some(&bundle.aux_files),
            )
            .with_context(|| t!("cli.error.pmx_extract_failed").to_string())?
        }
        ArchiveModelKind::Pmd => {
            let pmd_model = popone::pmd::reader::read_pmd_from_data(&bundle.model.data)
                .with_context(|| t!("cli.error.pmd_read_failed").to_string())?;
            popone::pmd::extract::pmd_to_ir_with_aux(
                &pmd_model,
                &bundle.model.path,
                Some(&bundle.aux_files),
            )
            .with_context(|| t!("cli.error.pmd_extract_failed").to_string())?
        }
        ArchiveModelKind::Fbx => {
            // Via archive: passing fbx_path=None disables the texture lookup near the FBX
            let mut ir = popone::fbx::extract::extract_ir_model_from_fbx_with_options(
                &bundle.model.data,
                None,
                args.normalize_pose,
                args.normalize_to_tstance,
            )
            .with_context(|| t!("cli.error.fbx_extract_failed").to_string())?;
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
                .with_context(|| t!("cli.error.vrm_glb_read_failed").to_string())?;
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
            .with_context(|| t!("cli.error.vrm_extract_failed").to_string())?
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
            .with_context(|| t!("cli.error.obj_extract_failed").to_string())?
        }
        ArchiveModelKind::Stl => {
            let name = bundle
                .model
                .path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("Model");
            popone::stl::extract::load_stl_from_data(&bundle.model.data, name)
                .with_context(|| t!("cli.error.stl_extract_failed").to_string())?
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
            .with_context(|| t!("cli.error.directx_extract_failed").to_string())?
        }
        ArchiveModelKind::UnityPackage => {
            // Re-extract a `.unitypackage` nested inside the archive
            let pkg = popone::unitypackage::build_unity_package_index(&bundle.model.data)
                .with_context(|| t!("cli.error.archive_unitypackage_extract_failed").to_string())?;

            // Collect the FBX entries
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
                anyhow::bail!("{}", t!("cli.error.archive_unitypackage_no_fbx"));
            }

            let selected_idx = if let Some(ref target) = args.fbx_name {
                let target_lower = target.to_lowercase();
                fbx_indices
                    .iter()
                    .find(|(_, name)| name.to_lowercase().contains(&target_lower))
                    .map(|(idx, _)| *idx)
                    .ok_or_else(|| {
                        anyhow::anyhow!(
                            "{}",
                            t!("cli.error.fbx_not_found_simple", name = target.clone())
                        )
                    })?
            } else {
                popone::unitypackage::select_best_fbx_index(&pkg, &fbx_indices)
            };

            let prepared = popone::unitypackage::prepare_pkg_fbx(&pkg, selected_idx)
                .with_context(|| t!("cli.error.prefab_resolve_failed").to_string())?;
            log::info!(
                "FBX in unitypackage: {} textures: {}",
                prepared.model.pathname,
                prepared.textures.len()
            );

            // Via unitypackage: passing fbx_path=None disables the texture lookup near the FBX
            let mut ir = popone::fbx::extract::extract_ir_model_from_fbx_with_options(
                &prepared.fbx_data,
                None,
                args.normalize_pose,
                args.normalize_to_tstance,
            )
            .with_context(|| t!("cli.error.fbx_extract_failed").to_string())?;

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
                let textures: Vec<(String, Arc<[u8]>)> = prepared
                    .textures
                    .iter()
                    .map(|t| (t.display_name.to_string(), Arc::clone(&t.data)))
                    .collect();
                popone::unitypackage::embed_textures_into_ir(&mut ir, &textures);
            }
            ir
        }
    };

    // Log the texture assignments
    ir.log_texture_assignments();

    if args.dump {
        dump_ir(&ir);
        return Ok(());
    }

    // Write textures (for archive flows, always use `write_all_textures_from_ir`)
    let output_dir = output.parent().unwrap_or(Path::new(".")).to_path_buf();
    std::fs::create_dir_all(&output_dir).with_context(|| {
        t!(
            "cli.error.output_dir_create_failed",
            path = output_dir.display().to_string()
        )
        .to_string()
    })?;
    let tex_dir = output_dir.join("textures");
    let written_filenames = convert::texture::write_all_textures_from_ir(&ir.textures, &tex_dir)
        .with_context(|| t!("cli.error.texture_write_failed").to_string())?;

    // Build and write the PMX model
    let build_options = pmx::build::PmxBuildOptions {
        align_rigid_rotation: args.align_rigid_rotation,
        no_physics: args.no_physics,
        raw_structure: args.raw_structure,
        scale: args.scale,
    };
    let (mut pmx_model, toon_textures) =
        pmx::build::build_pmx_model_with_options(&ir, &build_options)
            .with_context(|| t!("cli.error.pmx_build_failed").to_string())?;
    // Patch PMX texture paths when PSD->PNG conversion changed the filename
    for (i, name) in written_filenames.iter().enumerate() {
        if i < pmx_model.textures.len() {
            pmx_model.textures[i] = format!("textures\\{}", name);
        }
    }
    let base_tex_count = ir.textures.len();
    let toon_written =
        popone::convert::texture::write_all_textures_from_ir(&toon_textures, &tex_dir)?;
    for (i, name) in toon_written.iter().enumerate() {
        let pmx_idx = base_tex_count + i;
        if pmx_idx < pmx_model.textures.len() {
            pmx_model.textures[pmx_idx] = format!("textures\\{}", name);
        }
    }
    let output_file = std::fs::File::create(output).with_context(|| {
        t!(
            "cli.error.output_file_create_failed",
            path = output.display().to_string()
        )
        .to_string()
    })?;
    let writer = std::io::BufWriter::new(output_file);
    let header = pmx_model.header.clone();
    let mut pmx_writer = pmx::writer::PmxWriter::new(writer, header);
    pmx_writer
        .write_model(&pmx_model)
        .with_context(|| t!("cli.error.pmx_write_failed").to_string())?;

    log::info!("Conversion complete: {}", output.display());
    println!(
        "{}",
        t!(
            "cli.output.conversion_complete",
            input = input.display().to_string(),
            output = output.display().to_string()
        )
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

/// Shared viewer launch (logging, panic hook, NativeOptions setup).
#[cfg(feature = "viewer")]
fn run_viewer_with_initial(initial_file: Option<PathBuf>) -> Result<()> {
    // App data directory (%LOCALAPPDATA%\popone)
    let data_dir = popone::viewer::app::persistence::data_dir();
    popone::viewer::app::persistence::migrate_from_exe_dir(&data_dir);

    // Load session config (loaded before the log config is applied; also gates the
    // single-instance check below via the hidden `[behavior] disable_single_instance` flag)
    let app_config = popone::viewer::app::persistence::load_config(&data_dir);

    // Single-instance: try to forward to an existing instance. Hidden option
    // `[behavior] disable_single_instance = true` in popone.toml skips this entirely.
    #[cfg(target_os = "windows")]
    {
        let disable_single_instance = app_config
            .as_ref()
            .is_some_and(|c| c.behavior.disable_single_instance);
        if !disable_single_instance {
            use popone::viewer::single_instance::InstanceCheck;
            match popone::viewer::single_instance::try_send_to_existing(initial_file.as_deref()) {
                InstanceCheck::Forwarded => return Ok(()),
                // Both Primary and FallbackStart continue execution. v0.4.0 onwards no longer
                // performs log rotation, so the two cases need no further distinction.
                InstanceCheck::Primary | InstanceCheck::FallbackStart => {}
            }
        }
    }

    let log_config = app_config
        .as_ref()
        .map(|c| c.log.clone())
        .unwrap_or_default();

    let logs_dir = data_dir.join("logs");
    let _ = std::fs::create_dir_all(&logs_dir);
    // v0.4.0 changed the policy to "only persist panic logs"; saving a normal log is now
    // an explicit user action via the "Save log" button inside the log viewer. Automatic
    // rotation was therefore removed -- every generated file is user-intentional, so we never
    // delete it on our own.

    let timestamp = chrono::Local::now().format("%Y%m%d_%H%M%S");
    let log_path = logs_dir.join(format!("popone_{timestamp}.log"));
    let log_buffer: SharedLogBuffer =
        std::sync::Arc::new(std::sync::Mutex::new(popone::LogBuffer::new()));
    setup_logging(log_config.level_filter(), None, Some(log_buffer.clone()))?;

    {
        // Panic dumps are written directly to `panic_<ts>.log` without bouncing through
        // `popone_<ts>.log` (one crash = one file). Since v0.4.0 also dropped log rotation,
        // generated panic dumps stay on disk until the user removes them manually.
        let panic_dump_path = match log_path.file_name().and_then(|n| n.to_str()) {
            Some(name) => match name.strip_prefix("popone_") {
                Some(rest) => log_path.with_file_name(format!("panic_{rest}")),
                None => log_path.clone(),
            },
            None => log_path.clone(),
        };
        let panic_buffer = log_buffer.clone();
        std::panic::set_hook(Box::new(move |info| {
            let bt = std::backtrace::Backtrace::force_capture();
            let msg = format!("[PANIC] {info}\n{bt}");
            log::error!("{msg}");
            // Flush the in-memory buffer directly into panic_<ts>.log (no copy needed)
            flush_log_buffer(&panic_buffer, &panic_dump_path);
        }));
    }

    if let Some(ref path) = initial_file {
        log::info!("Viewer mode: {}", path.display());
    }

    let png = include_bytes!("../assets/popone_icon.png");
    let img = image::load_from_memory(png)
        .with_context(|| t!("cli.error.viewer_icon_load_failed").to_string())?;
    let rgba = img.to_rgba8();
    let (w, h) = rgba.dimensions();
    let icon = eframe::egui::IconData {
        rgba: rgba.into_raw(),
        width: w,
        height: h,
    };

    // NativeOptions: apply the saved size if one exists (position is applied on the first frame)
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

    // Detach from the console before launching the viewer
    #[cfg(target_os = "windows")]
    detach_console();

    run_viewer_inner(
        options,
        logs_dir,
        log_path,
        log_buffer,
        initial_file,
        data_dir,
        app_config,
    )
}

#[cfg(feature = "viewer")]
fn run_viewer_inner(
    options: eframe::NativeOptions,
    logs_dir: PathBuf,
    log_path: PathBuf,
    log_buffer: SharedLogBuffer,
    initial_file: Option<PathBuf>,
    data_dir: PathBuf,
    app_config: Option<popone::viewer::app::persistence::AppConfig>,
) -> Result<()> {
    eframe::run_native(
        "Viewer",
        options,
        Box::new(move |cc| {
            let mut app = popone::viewer::app::ViewerApp::new(
                cc, logs_dir, log_path, log_buffer, data_dir, app_config,
            );
            if let Some(path) = initial_file {
                app.pending.bg_state.submit_dispatch(
                    popone::viewer::app::pending::PendingLoadDispatch {
                        path,
                        append: false,
                        overlay: popone::viewer::app::pending::PendingOverlay::WaitingOverlay,
                        preloaded: None,
                        is_reload: false,
                    },
                );
            }
            Ok(Box::new(app))
        }),
    )
    .map_err(|e| {
        anyhow::anyhow!(
            "{}",
            t!("cli.error.viewer_launch_failed", detail = e.to_string())
        )
    })
}
