//! ファイル読み込み、D&D処理、reload、append、アニメーション読み込み

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use eframe::egui;

use crate::intermediate::types::{IrModel, TextureData};
use crate::unitypackage::UnityPackageIndex;

use super::MaterialDisplayState;
use crate::vrm;

use super::pending::PendingOverlay;

use super::super::animation::AnimationState;
use super::helpers::{
    build_pkg_model_list, collect_image_files_recursive, is_temp_path, FbxLoadMode, PkgModelType,
    PreloadedData, ReloadableSource, IMAGE_EXTENSIONS, MODEL_EXTENSIONS,
};
use super::pending::{
    PendingArchiveLoad, PendingFbxChoice, PendingPkgModelLoad, PendingUnityPackage,
};
use super::texture_mgmt::PendingTexMatch;
use super::{
    AppendedModel, CachedStats, ConvertMessage, ConvertResult, DisplaySettings, MaterialGroup,
    ViewerApp,
};

/// FBX ファイルのメッシュ・アニメーション有無
struct FbxContentInfo {
    has_mesh: bool,
    has_anim: bool,
}

use super::helpers::TextureSource;
use super::OrbitCamera;

/// ファイル拡張子から判定するファイル形式。
/// 拡張子判定を1箇所に集約し、3箇所の分岐での漏れを防止する。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileFormat {
    Vrm,
    Fbx,
    Pmx,
    Pmd,
    Obj,
    Stl,
    DirectX,
    UnityPackage,
    SevenZ,
    Zip,
    /// .vrma, .gltf, .glb (animation), .anim
    Animation,
    Unknown,
}

/// 拡張子文字列（小文字化済み）から `FileFormat` を判定する。
pub(super) fn detect_format(ext: &str) -> FileFormat {
    match ext {
        "vrm" | "glb" | "gltf" => FileFormat::Vrm,
        "fbx" => FileFormat::Fbx,
        "pmx" => FileFormat::Pmx,
        "pmd" => FileFormat::Pmd,
        "obj" => FileFormat::Obj,
        "stl" => FileFormat::Stl,
        "x" => FileFormat::DirectX,
        "unitypackage" => FileFormat::UnityPackage,
        "7z" => FileFormat::SevenZ,
        "zip" => FileFormat::Zip,
        "vrma" | "anim" => FileFormat::Animation,
        _ => FileFormat::Unknown,
    }
}

/// バックグラウンドロードの入力ソースを表す enum。
/// 将来 `ArchiveEntry` / `Reload` バリアントを追加して
/// アーカイブ内パースやリロードの BG 化を統一する。
pub(super) enum CpuParseInput {
    /// 通常のファイルロード（temp ファイルの場合は preloaded 付き）
    File {
        path: PathBuf,
        format: FileFormat,
        preloaded: Option<super::helpers::PreloadedData>,
    },
}

/// バックグラウンドスレッドで実行する CPU パース（`&self` 不要のフリー関数）。
/// ファイル読込 + パース → `(IrModel, ReloadableSource)` を返す。
/// GPU リソース構築は行わない。
pub(super) fn cpu_parse_source(
    input: CpuParseInput,
    normalize_pose: bool,
    normalize_to_tstance: bool,
    cancel: &std::sync::Arc<std::sync::atomic::AtomicBool>,
) -> anyhow::Result<(IrModel, super::helpers::ReloadableSource)> {
    use super::helpers::ReloadableSource;

    let CpuParseInput::File {
        ref path,
        format,
        ref preloaded,
    } = input;

    // キャンセルチェック：関数冒頭、各フォーマット分岐の直前、重い I/O の後、extract 呼び出しの前後、
    // そして関数末尾で粗く行う。パーサ本体（VRM/FBX/PMX extract）の内部までは潜らせず、
    // ディスパッチ境界でチェックすることで旧スレッドの無駄な CPU/I/O を段階的に打ち切る。
    let check_cancel = |cancel: &Arc<std::sync::atomic::AtomicBool>| -> anyhow::Result<()> {
        if cancel.load(std::sync::atomic::Ordering::Relaxed) {
            anyhow::bail!("bg load cancelled");
        }
        Ok(())
    };
    check_cancel(cancel)?;

    let is_temp = is_temp_path(path) || preloaded.as_ref().is_some_and(|pl| pl.path == *path);

    // preloaded があればそこからバイト列を取得、なければディスクから読む
    let read_data = |p: &Path| -> anyhow::Result<Arc<[u8]>> {
        if let Some(ref pl) = preloaded {
            if pl.path == *p {
                return Ok(Arc::clone(&pl.main_bytes));
            }
            if let Some(data) = pl.aux_files.get(p) {
                return Ok(Arc::clone(data));
            }
        }
        Ok(std::fs::read(p)?.into())
    };
    let collect_aux = |p: &Path| -> HashMap<PathBuf, Arc<[u8]>> {
        if let Some(ref pl) = preloaded {
            if pl.path == *p {
                return pl.aux_files.clone();
            }
        }
        let mut aux = HashMap::new();
        if let Some(dir) = p.parent() {
            collect_image_files_recursive(dir, dir, &mut aux);
        }
        aux
    };

    let make_source = |data: Arc<[u8]>, aux: HashMap<PathBuf, Arc<[u8]>>| -> ReloadableSource {
        if is_temp {
            ReloadableSource::Snapshot {
                original_path: path.to_path_buf(),
                main_bytes: data,
                aux_files: aux,
            }
        } else {
            ReloadableSource::File(path.to_path_buf())
        }
    };

    check_cancel(cancel)?;
    match format {
        FileFormat::Vrm => {
            let glb = if is_temp {
                let data = read_data(path)?;
                crate::vrm::loader::load_glb_from_data(&data)?
            } else {
                crate::vrm::loader::load_glb(path)?
            };
            check_cancel(cancel)?;
            let version = crate::vrm::detect::detect_version(&glb.document);
            let all_extensions = crate::vrm::loader::get_raw_extensions(&glb.document);
            let ir = crate::vrm::extract::extract_ir_model_with_options(
                &glb.document,
                &glb.buffers,
                &glb.images,
                &glb.vrm_extension,
                &version,
                &all_extensions,
                normalize_pose,
            )?;
            check_cancel(cancel)?;
            // 生 RGBA のまま IrTexture に格納（vrm::extract で設定済み）。
            // PNG エンコードは GPU アップロード後に必要になった時点（PMX エクスポート等）で行う。
            let source = if is_temp {
                let data = read_data(path)?;
                ReloadableSource::Snapshot {
                    original_path: path.to_path_buf(),
                    main_bytes: data,
                    aux_files: HashMap::new(),
                }
            } else {
                ReloadableSource::File(path.to_path_buf())
            };
            Ok((ir, source))
        }
        FileFormat::Fbx => {
            let data = read_data(path)?;
            check_cancel(cancel)?;
            let ir = crate::fbx::extract::extract_ir_model_from_fbx_with_options(
                &data,
                Some(path),
                normalize_pose,
                normalize_to_tstance,
            )?;
            check_cancel(cancel)?;
            let aux = collect_aux(path);
            let source = make_source(data, aux);
            Ok((ir, source))
        }
        FileFormat::Pmx => {
            // 非 temp: ディスク直読み（pmx_to_ir が sph/spa 等も含め全拡張子を読める）
            // temp: preloaded.aux_files を使って pmx_to_ir_with_aux
            let pmx_dir = path.parent().unwrap_or(Path::new("."));
            let mut ir = if is_temp {
                let data = read_data(path)?;
                check_cancel(cancel)?;
                let pmx_model = crate::pmx::reader::read_pmx_from_data(&data)?;
                check_cancel(cancel)?;
                let aux = preloaded
                    .as_ref()
                    .filter(|pl| pl.path == *path)
                    .map(|pl| pl.aux_files.clone())
                    .unwrap_or_default();
                crate::pmx::extract::pmx_to_ir_with_aux(&pmx_model, pmx_dir, Some(&aux))?
            } else {
                let pmx_model = crate::pmx::reader::read_pmx(path)?;
                check_cancel(cancel)?;
                crate::pmx::extract::pmx_to_ir(&pmx_model, pmx_dir)?
            };
            check_cancel(cancel)?;
            if normalize_pose {
                ir.astance_result = crate::intermediate::pose::normalize_pose_to_tstance_full(
                    &mut ir.bones,
                    &mut ir.meshes,
                    &mut ir.morphs,
                    &mut ir.physics,
                    crate::convert::coord::gltf_pos_to_pmx,
                );
            }
            let source = if is_temp {
                let data = read_data(path).unwrap_or_default();
                let aux = preloaded
                    .as_ref()
                    .filter(|pl| pl.path == *path)
                    .map(|pl| pl.aux_files.clone())
                    .unwrap_or_default();
                ReloadableSource::Snapshot {
                    original_path: path.to_path_buf(),
                    main_bytes: data,
                    aux_files: aux,
                }
            } else {
                ReloadableSource::File(path.to_path_buf())
            };
            Ok((ir, source))
        }
        FileFormat::Pmd => {
            // 非 temp: ディスク直読み（pmd_to_ir が sph/spa 等も含め全拡張子を読める）
            // temp: preloaded.aux_files を使って pmd_to_ir_with_aux
            let mut ir = if is_temp {
                let data = read_data(path)?;
                check_cancel(cancel)?;
                let pmd_model = crate::pmd::reader::read_pmd_from_data(&data)?;
                check_cancel(cancel)?;
                let aux = preloaded
                    .as_ref()
                    .filter(|pl| pl.path == *path)
                    .map(|pl| pl.aux_files.clone())
                    .unwrap_or_default();
                crate::pmd::extract::pmd_to_ir_with_aux(&pmd_model, path, Some(&aux))?
            } else {
                let pmd_model = crate::pmd::reader::read_pmd(path)?;
                check_cancel(cancel)?;
                crate::pmd::extract::pmd_to_ir(&pmd_model, path)?
            };
            check_cancel(cancel)?;
            if normalize_pose {
                ir.astance_result = crate::intermediate::pose::normalize_pose_to_tstance_full(
                    &mut ir.bones,
                    &mut ir.meshes,
                    &mut ir.morphs,
                    &mut ir.physics,
                    crate::convert::coord::gltf_pos_to_pmx,
                );
            }
            let source = if is_temp {
                let data = read_data(path).unwrap_or_default();
                let aux = preloaded
                    .as_ref()
                    .filter(|pl| pl.path == *path)
                    .map(|pl| pl.aux_files.clone())
                    .unwrap_or_default();
                ReloadableSource::Snapshot {
                    original_path: path.to_path_buf(),
                    main_bytes: data,
                    aux_files: aux,
                }
            } else {
                ReloadableSource::File(path.to_path_buf())
            };
            Ok((ir, source))
        }
        FileFormat::Obj => {
            let ir = if is_temp {
                let data = read_data(path)?;
                check_cancel(cancel)?;
                let obj_dir = path.parent().unwrap_or(Path::new("."));
                let aux = collect_aux(path);
                let name = path.file_stem().and_then(|s| s.to_str()).unwrap_or("Model");
                crate::obj::extract::load_obj_from_data(&data, name, obj_dir, Some(&aux))?
            } else {
                crate::obj::extract::load_obj(path)?
            };
            check_cancel(cancel)?;
            let data = read_data(path).unwrap_or_default();
            let aux = collect_aux(path);
            let source = make_source(data, aux);
            Ok((ir, source))
        }
        FileFormat::Stl => {
            let ir = if is_temp {
                let data = read_data(path)?;
                check_cancel(cancel)?;
                let name = path.file_stem().and_then(|s| s.to_str()).unwrap_or("Model");
                crate::stl::extract::load_stl_from_data(&data, name)?
            } else {
                crate::stl::extract::load_stl(path)?
            };
            check_cancel(cancel)?;
            let data = read_data(path).unwrap_or_default();
            let source = make_source(data, HashMap::new());
            Ok((ir, source))
        }
        FileFormat::DirectX => {
            let ir = if is_temp {
                let data = read_data(path)?;
                check_cancel(cancel)?;
                let x_dir = path.parent().unwrap_or(Path::new("."));
                let aux = collect_aux(path);
                let name = path.file_stem().and_then(|s| s.to_str()).unwrap_or("Model");
                crate::directx::extract::load_x_from_data(&data, name, x_dir, Some(&aux))?
            } else {
                crate::directx::extract::load_x(path)?
            };
            check_cancel(cancel)?;
            let data = read_data(path).unwrap_or_default();
            let aux = collect_aux(path);
            let source = make_source(data, aux);
            Ok((ir, source))
        }
        _ => anyhow::bail!("Unsupported format for background loading: {:?}", format),
    }
}

/// `reload_current` で退避・復元するフィールドをまとめた構造体。
/// 新フィールド追加時の漏れを防止する。
struct ReloadSnapshot {
    appended_models: Vec<AppendedModel>,
    camera: OrbitCamera,
    morph_weights: Vec<f32>,
    material_visibility: Vec<bool>,
    material_display: Vec<MaterialDisplayState>,
    material_filter: String,
    pmx_output_path: String,
    export_visible_only: bool,
    tex_assignments: HashMap<usize, TextureSource>,
    pkg_tex_assignments: HashMap<usize, String>,
    pkg_textures: Option<Vec<(String, Vec<u8>)>>,
    vrma_library: Vec<(
        String,
        PathBuf,
        Arc<crate::intermediate::animation::VrmaAnimation>,
    )>,
    vrma_active_index: Option<usize>,
    display: DisplaySettings,
}

impl ViewerApp {
    /// preloaded の aux_files があればそれを移動（clone回避）、なければディスクから再帰収集する
    pub(super) fn take_or_collect_aux(&mut self, path: &Path) -> HashMap<PathBuf, Arc<[u8]>> {
        if let Some(ref pl) = self.preloaded {
            if pl.path == path {
                // preloaded から aux_files を移動（HashMap の再割り当て回避）
                let pl = self.preloaded.take().expect("preloaded は Some 確認済み");
                self.preloaded = Some(PreloadedData {
                    path: pl.path,
                    main_bytes: pl.main_bytes,
                    aux_files: HashMap::new(),
                });
                return pl.aux_files;
            }
        }
        let mut aux = HashMap::new();
        if let Some(dir) = path.parent() {
            collect_image_files_recursive(dir, dir, &mut aux);
        }
        aux
    }

    /// temp先読みデータがあればそれを、なければファイルから読む
    pub(super) fn read_or_preloaded(&self, path: &Path) -> anyhow::Result<Arc<[u8]>> {
        if let Some(ref pl) = self.preloaded {
            if pl.path == path {
                return Ok(Arc::clone(&pl.main_bytes));
            }
            // aux_files も確認（サブファイル参照用）
            if let Some(data) = pl.aux_files.get(path) {
                return Ok(Arc::clone(data));
            }
        }
        Ok(std::fs::read(path)?.into())
    }

    /// FBX ファイルのメッシュ・アニメーション有無を判定
    fn inspect_fbx(&self, path: &Path) -> FbxContentInfo {
        let data = match self.read_or_preloaded(path) {
            Ok(d) => d,
            Err(_) => {
                return FbxContentInfo {
                    has_mesh: false,
                    has_anim: false,
                }
            }
        };
        FbxContentInfo {
            has_mesh: crate::fbx::extract::fbx_has_mesh(&data),
            has_anim: crate::fbx::animation::load_fbx_animation_from_data(&data)
                .is_ok_and(|a| !a.is_empty()),
        }
    }

    /// ロード dispatch のメインスレッドルーティング。
    /// アニメーション判定、FBX choice、archive/pkg は既存の同期パスに振り分け、
    /// モデルパースのみバックグラウンドスレッドに送る。
    pub(super) fn route_load_dispatch(
        &mut self,
        dispatch: super::pending::PendingLoadDispatch,
        prior_loading: Option<super::pending::BgLoadHandle>,
    ) {
        use super::pending::{BackgroundLoadState, BgLoadKind};

        let path = dispatch.path;
        let append = dispatch.append;
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();
        let format = detect_format(&ext);

        // 先に dispatch 種別を判定する。
        // アニメーション単体の要求（既存モデルに適用する vrma/.anim/gltf-anim/anim-only FBX）は
        // 進行中モデルロードに依存するため、ここでキャンセルしてはいけない。
        // 代わりに bg_load 進行中なら拒否する（アニメ適用先モデルが未確定のため）。
        let is_anim_only_request = !append
            && match ext.as_str() {
                "vrma" => true,
                "anim" => self.loaded.is_some(),
                "glb" | "gltf" if self.loaded.is_some() => {
                    vrm::animation::load_gltf_animation(&path)
                        .map(|a| !a.is_empty())
                        .unwrap_or(false)
                }
                _ => {
                    format == FileFormat::Fbx && self.loaded.is_some() && {
                        let info = self.inspect_fbx(&path);
                        !info.has_mesh && info.has_anim
                    }
                }
            };

        if is_anim_only_request {
            if let Some(prior) = prior_loading {
                // モデルロード進行中にアニメ要求が来た場合、キャンセルしてしまうと
                // アニメ適用先のモデルが消えて両方失敗する。拒否して現行ロードを守る。
                log::warn!(
                    "Cannot load animation while model load is in progress: {}",
                    path.display()
                );
                self.convert_message = Some(ConvertMessage::failure(
                    "モデル読み込み中はアニメーションを開けません。完了してから再試行してください。"
                        .to_string(),
                ));
                // prior Loading を bg_state に戻して現行ロードを保護
                self.pending.bg_state = BackgroundLoadState::Loading(prior);
                return;
            }
            // bg_load 非進行中: アニメ要求を既存モデルに適用する通常フローへ進む（キャンセル不要）
        } else {
            // モデルロード要求: 進行中の bg_load があればキャンセルして新規を受け入れる。
            if let Some(old) = prior_loading {
                old.cancel.store(true, std::sync::atomic::Ordering::Relaxed);
                log::info!(
                    "Cancelling previous bg load (req={}) for new dispatch: {}",
                    old.request_id,
                    path.display()
                );
                // old はここで drop、受信チャネルもクローズされる
            }
        }

        // dispatch.preloaded を self.preloaded に一時セット（既存メソッドとの互換性）
        self.preloaded = dispatch.preloaded;

        // append モード
        if append {
            // unitypackage / archive は同期パスにフォールバック
            if format == FileFormat::UnityPackage
                || format == FileFormat::Zip
                || format == FileFormat::SevenZ
            {
                self.append_model(path);
                return;
            }
            // その他のフォーマットはバ��クグラウンドパース
            self.spawn_bg_load(path, BgLoadKind::Append, format);
            return;
        }

        // --- 以下、通常ロード ---

        // 読み込み時はプレビュー中の bind group を復元してからクリア
        self.cancel_tex_match_preview();
        // unitypackage以外の読み込み時はパッケージテクスチャをクリア
        if format != FileFormat::UnityPackage {
            self.tex.pkg_textures = None;
            self.clear_pkg_thumb_cache();
        }

        // アニメーションファイルの判定（即時実行、BG 不要）
        if ext == "vrma" {
            self.try_load_vrma(&path);
            return;
        }
        if (ext == "glb" || ext == "gltf") && self.loaded.is_some() {
            if let Ok(anims) = vrm::animation::load_gltf_animation(&path) {
                if !anims.is_empty() {
                    self.try_load_gltf_animation(&path);
                    return;
                }
            }
        }
        if ext == "anim" && self.loaded.is_some() {
            self.try_load_unity_animation(&path);
            return;
        }

        // FBX: メッシュ+アニメーション両方含むなら選択ダイアログ
        if format == FileFormat::Fbx {
            let info = self.inspect_fbx(&path);
            if info.has_mesh && info.has_anim {
                self.pending.fbx_choice = Some(PendingFbxChoice {
                    path: path.clone(),
                    load_model: true,
                    load_animation: true,
                    pkg_context: None,
                    preloaded: self.preloaded.take(),
                });
                return;
            } else if !info.has_mesh && info.has_anim {
                if self.loaded.is_some() {
                    self.try_load_fbx_animation(&path);
                } else {
                    self.convert_message = Some(ConvertMessage::failure(String::from(
                        "先にモデルを読み込んでください",
                    )));
                }
                return;
            }
        }

        // archive / unitypackage は同期パスにフォールバック
        if matches!(
            format,
            FileFormat::UnityPackage | FileFormat::Zip | FileFormat::SevenZ
        ) {
            self.load_file_as_model(path);
            return;
        }

        // FBX auto-animation 判定（BG 完了後に自動適用するかどうか）
        let auto_fbx_anim = format == FileFormat::Fbx && self.inspect_fbx(&path).has_anim;

        // スタンスのみ事前リセット（シェーダーは finish_load_with_gpu 成功時にリセット）
        self.normalize_pose = false;
        self.normalize_to_tstance = false;

        self.spawn_bg_load(
            path,
            BgLoadKind::Initial {
                format,
                auto_fbx_anim,
            },
            format,
        );
    }

    /// バックグラウンドスレッドで CPU パースを実行する。
    fn spawn_bg_load(
        &mut self,
        path: PathBuf,
        kind: super::pending::BgLoadKind,
        format: FileFormat,
    ) {
        use super::pending::BackgroundLoadState;
        use std::sync::atomic::AtomicBool;
        use std::sync::Arc;

        // 残存している旧 Loading があればここでもキャンセル（route_load_dispatch 以外の経路保険）
        if let BackgroundLoadState::Loading(old) =
            std::mem::replace(&mut self.pending.bg_state, BackgroundLoadState::Idle)
        {
            old.cancel.store(true, std::sync::atomic::Ordering::Relaxed);
        }

        let request_id = self.fresh_request_id();
        let cancel = Arc::new(AtomicBool::new(false));
        let normalize_pose = self.normalize_pose;
        let normalize_to_tstance = self.normalize_to_tstance;
        let preloaded = self.preloaded.take();

        let (tx, rx) = std::sync::mpsc::channel();
        self.pending.bg_state = BackgroundLoadState::Loading(super::pending::BgLoadHandle {
            rx,
            cancel: Arc::clone(&cancel),
            request_id,
        });

        let path_clone = path.clone();
        std::thread::spawn(move || {
            let input = CpuParseInput::File {
                path: path_clone.clone(),
                format,
                preloaded,
            };
            let result = cpu_parse_source(input, normalize_pose, normalize_to_tstance, &cancel);
            let result = result.map(|(ir, source)| super::pending::BgLoadResult {
                ir,
                source,
                kind,
                path: path_clone,
                request_id,
            });
            let _ = tx.send(result);
        });
    }

    /// バックグラウンドパース結果の後処理（basic path: direct / append）。
    pub(super) fn apply_bg_load_result(
        &mut self,
        result: super::pending::BgLoadResult,
    ) -> anyhow::Result<()> {
        use super::pending::BgLoadKind;
        match result.kind {
            BgLoadKind::Initial {
                format: _,
                auto_fbx_anim,
            } => {
                self.finish_load(result.ir, result.source)?;
                log::info!("Model loaded (bg): {}", result.path.display());
                self.convert_message = None;
                self.anim.state = None;
                self.anim.library.clear();
                self.anim.active_index = None;
                if auto_fbx_anim {
                    self.try_load_fbx_animation(&result.path);
                }
            }
            BgLoadKind::Append => {
                // 座標系互換チェック
                if let Some(ref loaded) = self.loaded {
                    let host_fmt = loaded.ir.source_format;
                    let other_fmt = result.ir.source_format;
                    if host_fmt.is_vrm0() != other_fmt.is_vrm0() {
                        log::warn!(
                            "Appending model with different coordinate system: {} + {}",
                            host_fmt.label(),
                            other_fmt.label()
                        );
                        anyhow::bail!(
                            "座標系が異なるモデルは追加できません。\nホスト: {}, 追加: {}",
                            host_fmt.label(),
                            other_fmt.label()
                        );
                    }
                }
                self.finish_append_with_source(result.ir, result.source, None);
            }
        }
        Ok(())
    }

    /// 旧同期ロード経路（archive/reload 等の同期フォールバック用に残存）。
    /// 新規 direct load は route_load_dispatch → spawn_bg_load を使用する。
    #[allow(dead_code)]
    pub(super) fn load_file(&mut self, path: PathBuf) {
        log::info!("Open file: {}", path.display());
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();
        let format = detect_format(&ext);

        // 読み込み時はプレビュー中の bind group を復元してからクリア
        self.cancel_tex_match_preview();
        // unitypackage以外の読み込み時はパッケージテクスチャをクリア
        if format != FileFormat::UnityPackage {
            self.tex.pkg_textures = None;
            self.clear_pkg_thumb_cache();
        }

        // アニメーションファイルの判定
        if ext == "vrma" {
            self.try_load_vrma(&path);
            return;
        }
        // GLB/glTF: モデルが読み込み済みの場合、アニメーションとして開くか確認
        if (ext == "glb" || ext == "gltf") && self.loaded.is_some() {
            // アニメーションが含まれるか先に確認
            if let Ok(anims) = vrm::animation::load_gltf_animation(&path) {
                if !anims.is_empty() {
                    self.try_load_gltf_animation(&path);
                    return;
                }
            }
        }
        // Unity .anim: アニメーションとして読み込む
        if ext == "anim" && self.loaded.is_some() {
            self.try_load_unity_animation(&path);
            return;
        }

        // FBX: メッシュ+アニメーション両方含むなら選択ダイアログ（初回ロード時も対象）
        if format == FileFormat::Fbx {
            let info = self.inspect_fbx(&path);
            if info.has_mesh && info.has_anim {
                // 両方含む → 選択ダイアログを表示
                self.pending.fbx_choice = Some(PendingFbxChoice {
                    path: path.clone(),
                    load_model: true,
                    load_animation: true,
                    pkg_context: None,
                    preloaded: self.preloaded.take(),
                });
                return;
            } else if !info.has_mesh && info.has_anim {
                // アニメーションのみ
                if self.loaded.is_some() {
                    self.try_load_fbx_animation(&path);
                } else {
                    self.convert_message = Some(ConvertMessage::failure(String::from(
                        "先にモデルを読み込んでください",
                    )));
                }
                return;
            }
            // メッシュのみ or どちらもなし → モデルとして読み込み（下へ続行）
        }

        self.load_file_as_model(path);
    }

    /// モデルとしてファイルを読み込む（FBX選択ダイアログ不要時のパス）
    fn load_file_as_model(&mut self, path: PathBuf) {
        // スタンスのみ事前リセット（シェーダーは finish_load_with_gpu 成功時にリセット）
        self.normalize_pose = false;
        self.normalize_to_tstance = false;

        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();
        let format = detect_format(&ext);

        let result = match format {
            FileFormat::Fbx => self.try_load_fbx(&path),
            FileFormat::UnityPackage => self.try_load_unitypackage(&path),
            FileFormat::Pmx => self.try_load_pmx(&path),
            FileFormat::Pmd => self.try_load_pmd(&path),
            FileFormat::Obj => self.try_load_obj(&path),
            FileFormat::Stl => self.try_load_stl(&path),
            FileFormat::DirectX => self.try_load_x(&path),
            FileFormat::Zip | FileFormat::SevenZ => self.try_load_archive(&path),
            _ => self.try_load_vrm(&path),
        };

        match result {
            Ok(()) => {
                // アーカイブ系は一覧化完了（モデル選択はまだ）、それ以外はモデルロード完了
                match format {
                    FileFormat::UnityPackage => {
                        log::info!("Unitypackage indexed: {}", path.display());
                    }
                    FileFormat::Zip | FileFormat::SevenZ => {
                        log::info!("Archive indexed: {}", path.display());
                    }
                    _ => {
                        log::info!("Model loaded: {}", path.display());
                    }
                }
                self.convert_message = None;
                self.anim.state = None;
                self.anim.library.clear();
                self.anim.active_index = None;

                // FBXモデル読み込み後、同じファイルにアニメーションがあれば自動適用
                if format == FileFormat::Fbx && self.inspect_fbx(&path).has_anim {
                    self.try_load_fbx_animation(&path);
                }
            }
            Err(e) => {
                log::error!("Load failed: {e}");
                let user_msg = format!(
                    "ファイルを読み込めませんでした。\n\
                     ファイルが破損していないか、対応形式（VRM/FBX/PMX/PMD/ZIP/7z）であるか確認してください。\n\
                     詳細: {e}"
                );
                self.convert_message = Some(ConvertMessage::failure(user_msg));
            }
        }
    }

    /// FBX読み込み方法選択ダイアログの結果を実行
    pub fn execute_fbx_choice(&mut self, choice: PendingFbxChoice) {
        let PendingFbxChoice {
            path,
            load_model,
            load_animation,
            pkg_context,
            preloaded,
        } = choice;
        self.preloaded = preloaded;

        let mode = match (load_model, load_animation) {
            (true, true) => FbxLoadMode::Both,
            (true, false) => FbxLoadMode::ModelOnly,
            (false, true) => FbxLoadMode::AnimationOnly,
            (false, false) => return,
        };

        if let Some(pkg) = pkg_context {
            // unitypackage 経由: source_override を構築
            let source_override = if let Some(nested) = pkg.nested_archive_source {
                Some(nested)
            } else if let Some(ref snap) = pkg.archive_snapshot {
                Some(ReloadableSource::Snapshot {
                    original_path: pkg.source_path.clone(),
                    main_bytes: Arc::clone(snap),
                    aux_files: HashMap::new(),
                })
            } else {
                None
            };
            let pkg_idx_ref = pkg.pkg_index.as_deref();
            match self.load_fbx_from_assets(
                pkg.assets,
                pkg.fbx_index,
                &pkg.source_path,
                mode,
                source_override,
                pkg_idx_ref,
            ) {
                Ok(()) => {
                    log::info!("Model loaded from package: {}", pkg.source_path.display());
                    self.convert_message = None;
                }
                Err(e) => {
                    log::error!("Load failed: {e}");
                    self.convert_message = Some(ConvertMessage::failure(format!(
                        "ファイルを読み込めませんでした。\n詳細: {e}"
                    )));
                }
            }
        } else {
            // ファイル直接
            match mode {
                FbxLoadMode::Both | FbxLoadMode::ModelOnly => match self.try_load_fbx(&path) {
                    Ok(()) => {
                        log::info!("FBX model loaded: {}", path.display());
                        self.convert_message = None;
                        self.anim.state = None;
                        self.anim.library.clear();
                        self.anim.active_index = None;

                        if mode == FbxLoadMode::Both {
                            self.try_load_fbx_animation(&path);
                        }
                    }
                    Err(e) => {
                        log::error!("Load failed: {e}");
                        self.convert_message = Some(ConvertMessage::failure(format!(
                            "ファイルを読み込めませんでした。\n詳細: {e}"
                        )));
                    }
                },
                FbxLoadMode::AnimationOnly => {
                    self.try_load_fbx_animation(&path);
                }
            }
        }
        self.preloaded = None;
    }

    fn try_load_fbx(&mut self, path: &std::path::Path) -> anyhow::Result<()> {
        let data = self.read_or_preloaded(path)?;
        let ir = crate::fbx::extract::extract_ir_model_from_fbx_with_options(
            &data,
            Some(path),
            self.normalize_pose,
            self.normalize_to_tstance,
        )?;
        let source =
            if is_temp_path(path) || self.preloaded.as_ref().is_some_and(|pl| pl.path == path) {
                let aux = self.take_or_collect_aux(path);
                ReloadableSource::Snapshot {
                    original_path: path.to_path_buf(),
                    main_bytes: data,
                    aux_files: aux,
                }
            } else {
                ReloadableSource::File(path.to_path_buf())
            };
        self.finish_load(ir, source)
    }

    fn try_load_unitypackage(&mut self, path: &std::path::Path) -> anyhow::Result<()> {
        // ファイル消失前に一時パス判定を確定（canonicalize がファイル存在を前提とするため）
        let is_temp =
            is_temp_path(path) || self.preloaded.as_ref().is_some_and(|pl| pl.path == path);
        let archive_data: Arc<[u8]> = self.read_or_preloaded(path)?;

        // Phase 3: UnityPackageIndex を構築（Prefab テクスチャ解決用）
        let pkg_index = Arc::new(crate::unitypackage::build_unity_package_index(
            &archive_data,
        )?);
        // 既存コードとの互換性のため ExtractedAsset も構築（Arc 共有でコピー回避）
        let assets: Vec<crate::unitypackage::ExtractedAsset> = pkg_index
            .entries
            .iter()
            .map(|e| crate::unitypackage::ExtractedAsset {
                pathname: e.pathname.clone(),
                data: Arc::clone(&e.data),
            })
            .collect();

        // 一時ファイルの場合はアーカイブデータをスナップショット
        let snapshot = if is_temp {
            Some(Arc::clone(&archive_data))
        } else {
            None
        };

        // FBX と VRM を統合したモデルリストを構築
        let model_list = build_pkg_model_list(&assets);

        if model_list.is_empty() {
            anyhow::bail!(".unitypackage 内に VRM / FBX ファイルが見つかりません");
        }

        if model_list.len() == 1 {
            // モデルが1つだけ → プログレス表示後に遅延ロード
            let (idx, _, model_type) = model_list[0];
            self.pending.pkg_load = Some(PendingPkgModelLoad {
                assets,
                fbx_index: idx,
                model_type,
                source_path: path.to_path_buf(),
                shown: false,
                append: false,
                suppress_tex_match: false,
                archive_snapshot: snapshot,
                nested_archive_source: None,
                pkg_index: Some(pkg_index),
            });
        } else {
            // 複数 → 選択ダイアログを表示
            log::info!("Found {} models in .unitypackage:", model_list.len());
            for (_, name, mtype) in &model_list {
                log::info!("  {:?}: {}", mtype, name);
            }
            self.pending.unity_pkg = Some(PendingUnityPackage {
                assets,
                model_list,
                source_path: path.to_path_buf(),
                append: false,
                archive_snapshot: snapshot,
                nested_archive_source: None,
                pkg_index: Some(pkg_index),
            });
        }
        Ok(())
    }

    /// unitypackage をアペンドモードで読み込み
    fn try_load_unitypackage_for_append(&mut self, path: &std::path::Path) -> anyhow::Result<()> {
        // ファイル消失前に一時パス判定を確定（canonicalize がファイル存在を前提とするため）
        let is_temp =
            is_temp_path(path) || self.preloaded.as_ref().is_some_and(|pl| pl.path == path);
        let archive_data: Arc<[u8]> = self.read_or_preloaded(path)?;

        // Phase 3: UnityPackageIndex を構築
        let pkg_index = Arc::new(crate::unitypackage::build_unity_package_index(
            &archive_data,
        )?);
        let assets: Vec<crate::unitypackage::ExtractedAsset> = pkg_index
            .entries
            .iter()
            .map(|e| crate::unitypackage::ExtractedAsset {
                pathname: e.pathname.clone(),
                data: Arc::clone(&e.data),
            })
            .collect();

        // 一時ファイルの場合はアーカイブデータをスナップショット
        let snapshot = if is_temp {
            Some(Arc::clone(&archive_data))
        } else {
            None
        };

        let model_list = build_pkg_model_list(&assets);

        if model_list.is_empty() {
            anyhow::bail!(".unitypackage 内に VRM / FBX ファイルが見つかりません");
        }

        if model_list.len() == 1 {
            let (idx, _, model_type) = model_list[0];
            self.pending.pkg_load = Some(PendingPkgModelLoad {
                assets,
                fbx_index: idx,
                model_type,
                source_path: path.to_path_buf(),
                shown: false,
                append: true,
                suppress_tex_match: self.suppress_tex_match,
                archive_snapshot: snapshot,
                nested_archive_source: None,
                pkg_index: Some(pkg_index),
            });
        } else {
            self.pending.unity_pkg = Some(PendingUnityPackage {
                assets,
                model_list,
                source_path: path.to_path_buf(),
                append: true,
                archive_snapshot: snapshot,
                nested_archive_source: None,
                pkg_index: Some(pkg_index),
            });
        }
        Ok(())
    }

    /// アーカイブ（ZIP/7z）を読み込み
    fn try_load_archive(&mut self, path: &std::path::Path) -> anyhow::Result<()> {
        self.try_load_archive_impl(path, false)
    }

    /// アーカイブをアペンドモードで読み込み
    fn try_load_archive_for_append(&mut self, path: &std::path::Path) -> anyhow::Result<()> {
        self.try_load_archive_impl(path, true)
    }

    fn try_load_archive_impl(
        &mut self,
        path: &std::path::Path,
        append: bool,
    ) -> anyhow::Result<()> {
        let is_temp =
            is_temp_path(path) || self.preloaded.as_ref().is_some_and(|pl| pl.path == path);
        let archive_data: Arc<[u8]> = self.read_or_preloaded(path)?;

        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();
        let format = crate::archive::archive_format_from_ext(&ext)
            .ok_or_else(|| anyhow::anyhow!("未対応のアーカイブ形式: {ext}"))?;

        let contents = crate::archive::list_models(&archive_data, format)?;

        if contents.models.is_empty() {
            anyhow::bail!("アーカイブ内にモデルファイルが見つかりません");
        }

        if contents.models.len() == 1 {
            // モデルが1つだけ → 遅延ロード
            self.pending.archive_load = Some(PendingArchiveLoad {
                archive_data,
                format,
                contents,
                model_index: 0,
                source_path: path.to_path_buf(),
                shown: false,
                append,
                is_temp,
            });
        } else {
            // 複数 → 選択ダイアログ
            log::info!("Found {} models in archive:", contents.models.len());
            for (_, p, _, kind) in &contents.models {
                log::info!("  [{}] {}", kind.label(), p.display());
            }
            self.pending.archive = Some(super::pending::PendingArchive {
                archive_data,
                format,
                contents,
                source_path: path.to_path_buf(),
                append,
                is_temp,
            });
        }
        Ok(())
    }

    /// アーカイブからモデルを読み込み
    pub(super) fn load_model_from_archive(
        &mut self,
        pending: PendingArchiveLoad,
    ) -> anyhow::Result<()> {
        let model_path = pending.contents.models[pending.model_index].1.clone();
        let kind = pending.contents.models[pending.model_index].3;
        log::info!(
            "Load from archive: {:?} [{}] from {}",
            kind,
            model_path.display(),
            pending.source_path.display()
        );

        let bundle = crate::archive::extract_model_bundle(
            &pending.archive_data,
            pending.format,
            pending.contents,
            pending.model_index,
        )?;

        // UnityPackage: 二重展開 → 既存の unitypackage フローへ接続
        if kind == crate::archive::ArchiveModelKind::UnityPackage {
            return self.load_unitypackage_from_archive(
                bundle.model.data,
                pending.source_path,
                pending.is_temp,
                pending.archive_data,
                pending.append,
                model_path,
            );
        }

        let ir = self.build_ir_from_archive_bundle(&bundle, &pending.source_path)?;

        let source = ReloadableSource::Archive {
            original_path: pending.source_path,
            archive_bytes: if pending.is_temp {
                Some(pending.archive_data)
            } else {
                None
            },
            selected_entry_path: model_path.to_string_lossy().into_owned(),
            inner_kind: kind,
        };

        if pending.append {
            self.finish_append_with_source(ir, source, None);
            Ok(())
        } else {
            self.finish_load(ir, source)
        }
    }

    /// アーカイブ内 .unitypackage を展開し、既存の unitypackage 読み込みフローへ接続
    fn load_unitypackage_from_archive(
        &mut self,
        pkg_data: Vec<u8>,
        source_path: PathBuf,
        is_temp: bool,
        archive_data: Arc<[u8]>,
        append: bool,
        entry_path: PathBuf,
    ) -> anyhow::Result<()> {
        // UnityPackageIndex を構築（Prefab 解決に必要）
        let pkg_index = Arc::new(crate::unitypackage::build_unity_package_index(&pkg_data)?);
        // 既存コードとの互換性のため ExtractedAsset も構築（Arc 共有でコピー回避）
        let assets: Vec<crate::unitypackage::ExtractedAsset> = pkg_index
            .entries
            .iter()
            .map(|e| crate::unitypackage::ExtractedAsset {
                pathname: e.pathname.clone(),
                data: Arc::clone(&e.data),
            })
            .collect();

        // VRM / FBX を検出
        let model_list = build_pkg_model_list(&assets);
        if model_list.is_empty() {
            anyhow::bail!(".unitypackage 内に VRM / FBX ファイルが見つかりません");
        }

        // アーカイブスナップショット（一時ファイルの場合のみ保持）
        let archive_snapshot = if is_temp {
            Some(Arc::clone(&archive_data))
        } else {
            None
        };

        // リロード時に Archive 経由で二重展開するための情報
        let nested_archive_source = Some(ReloadableSource::Archive {
            original_path: source_path.clone(),
            archive_bytes: if is_temp { Some(archive_data) } else { None },
            selected_entry_path: entry_path.to_string_lossy().into_owned(),
            inner_kind: crate::archive::ArchiveModelKind::UnityPackage,
        });

        if model_list.len() == 1 {
            let (model_index, ref _name, model_type) = model_list[0];
            log::info!("Archive .unitypackage: 1 model detected");
            self.pending.pkg_load = Some(PendingPkgModelLoad {
                assets,
                fbx_index: model_index,
                model_type,
                source_path: source_path.clone(),
                shown: false,
                append,
                suppress_tex_match: false,
                archive_snapshot,
                nested_archive_source,
                pkg_index: Some(pkg_index),
            });
        } else {
            log::info!("Archive .unitypackage: found {} models:", model_list.len());
            for (_, name, mt) in &model_list {
                let label = match mt {
                    PkgModelType::Prefab => "Prefab",
                    PkgModelType::Vrm => "VRM",
                    PkgModelType::Fbx => "FBX",
                };
                log::info!("  [{}] {}", label, name);
            }
            self.pending.unity_pkg = Some(PendingUnityPackage {
                assets,
                model_list,
                source_path: source_path.clone(),
                append,
                archive_snapshot,
                nested_archive_source,
                pkg_index: Some(pkg_index),
            });
        }
        Ok(())
    }

    /// アーカイブバンドルから IrModel を構築
    fn build_ir_from_archive_bundle(
        &self,
        bundle: &crate::archive::ModelBundle,
        source_path: &Path,
    ) -> anyhow::Result<IrModel> {
        use crate::archive::ArchiveModelKind;
        match bundle.kind {
            ArchiveModelKind::Pmx => {
                let pmx_model = crate::pmx::reader::read_pmx_from_data(&bundle.model.data)?;
                let mut ir = crate::pmx::extract::pmx_to_ir_with_aux(
                    &pmx_model,
                    std::path::Path::new("."),
                    Some(&bundle.aux_files),
                )?;
                if self.normalize_pose {
                    ir.astance_result = crate::intermediate::pose::normalize_pose_to_tstance_full(
                        &mut ir.bones,
                        &mut ir.meshes,
                        &mut ir.morphs,
                        &mut ir.physics,
                        crate::convert::coord::gltf_pos_to_pmx,
                    );
                }
                Ok(ir)
            }
            ArchiveModelKind::Pmd => {
                let pmd_model = crate::pmd::reader::read_pmd_from_data(&bundle.model.data)?;
                let mut ir = crate::pmd::extract::pmd_to_ir_with_aux(
                    &pmd_model,
                    &bundle.model.path,
                    Some(&bundle.aux_files),
                )?;
                if self.normalize_pose {
                    ir.astance_result = crate::intermediate::pose::normalize_pose_to_tstance_full(
                        &mut ir.bones,
                        &mut ir.meshes,
                        &mut ir.morphs,
                        &mut ir.physics,
                        crate::convert::coord::gltf_pos_to_pmx,
                    );
                }
                Ok(ir)
            }
            ArchiveModelKind::Fbx => {
                let mut ir = crate::fbx::extract::extract_ir_model_from_fbx_with_options(
                    &bundle.model.data,
                    Some(source_path),
                    self.normalize_pose,
                    self.normalize_to_tstance,
                )?;
                let label = format!(
                    "archive({})",
                    source_path
                        .file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                );
                crate::unitypackage::embed_textures_into_ir_with_label(
                    &mut ir,
                    &bundle.textures,
                    &label,
                );
                Ok(ir)
            }
            ArchiveModelKind::Vrm | ArchiveModelKind::Glb => {
                let glb = vrm::loader::load_glb_from_data(&bundle.model.data)?;
                let version = vrm::detect::detect_version(&glb.document);
                let all_extensions = vrm::loader::get_raw_extensions(&glb.document);
                let ir = vrm::extract::extract_ir_model_with_options(
                    &glb.document,
                    &glb.buffers,
                    &glb.images,
                    &glb.vrm_extension,
                    &version,
                    &all_extensions,
                    self.normalize_pose,
                )?;
                Ok(ir)
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
                let ir = crate::obj::extract::load_obj_from_data(
                    &bundle.model.data,
                    name,
                    base_dir,
                    Some(&bundle.aux_files),
                )?;
                Ok(ir)
            }
            ArchiveModelKind::Stl => {
                let name = bundle
                    .model
                    .path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("Model");
                let ir = crate::stl::extract::load_stl_from_data(&bundle.model.data, name)?;
                Ok(ir)
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
                let ir = crate::directx::extract::load_x_from_data(
                    &bundle.model.data,
                    name,
                    base_dir,
                    Some(&bundle.aux_files),
                )?;
                Ok(ir)
            }
            ArchiveModelKind::UnityPackage => {
                // load_model_from_archive で先に分岐済み。ここに到達することはない
                anyhow::bail!("UnityPackage は build_ir_from_archive_bundle では処理できません")
            }
        }
    }

    /// ReloadableSource::Archive からIrModelを構築（reload/append共通）
    fn load_ir_from_archive_source(
        &self,
        original_path: &Path,
        archive_bytes: Option<&Arc<[u8]>>,
        selected_entry_path: &str,
        inner_kind: crate::archive::ArchiveModelKind,
    ) -> anyhow::Result<IrModel> {
        let owned;
        let data: &[u8] = if let Some(snap) = archive_bytes {
            snap
        } else {
            owned = std::fs::read(original_path)?;
            &owned
        };

        let ext = original_path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();
        let format = crate::archive::archive_format_from_ext(&ext)
            .ok_or_else(|| anyhow::anyhow!("未対応のアーカイブ形式: {ext}"))?;

        let contents = crate::archive::list_models(data, format)?;

        // selected_entry_path で同じモデルを再選択
        let model_index = contents
            .models
            .iter()
            .position(|(_, p, _, _)| p.to_string_lossy() == selected_entry_path)
            .ok_or_else(|| {
                anyhow::anyhow!("アーカイブ内に以前のモデルが見つかりません: {selected_entry_path}")
            })?;

        let bundle = crate::archive::extract_model_bundle(data, format, contents, model_index)?;
        let _ = inner_kind; // bundle.kind を使用
        self.build_ir_from_archive_bundle(&bundle, original_path)
    }

    /// アーカイブ(ZIP/7z)内の .unitypackage データを取り出す
    fn extract_pkg_from_archive(
        original_path: &Path,
        archive_bytes: Option<&Arc<[u8]>>,
        selected_entry_path: &str,
    ) -> anyhow::Result<Vec<u8>> {
        let owned;
        let data: &[u8] = if let Some(snap) = archive_bytes {
            snap
        } else {
            owned = std::fs::read(original_path)?;
            &owned
        };

        let ext = original_path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();
        let format = crate::archive::archive_format_from_ext(&ext)
            .ok_or_else(|| anyhow::anyhow!("未対応のアーカイブ形式: {ext}"))?;

        let contents = crate::archive::list_models(data, format)?;

        let model_index = contents
            .models
            .iter()
            .position(|(_, p, _, _)| p.to_string_lossy() == selected_entry_path)
            .ok_or_else(|| {
                anyhow::anyhow!("アーカイブ内に以前のモデルが見つかりません: {selected_entry_path}")
            })?;

        let bundle = crate::archive::extract_model_bundle(data, format, contents, model_index)?;
        Ok(bundle.model.data)
    }

    /// リロード時の unitypackage 同期アペンド（遅延処理を避け、テクスチャ復元も行う）
    fn reload_append_unitypackage(
        &mut self,
        source: &ReloadableSource,
        pkg_model_name: Option<&str>,
        pkg_model: Option<&crate::unitypackage::PkgModelLocator>,
        saved_pkg_tex_assignments: &HashMap<usize, String>,
    ) {
        // Arc 参照で済むケースではコピーを避け、所有権が必要なパスのみ Vec を確保
        use std::borrow::Cow;
        let archive_data: Cow<'_, [u8]> = match source {
            ReloadableSource::Snapshot { main_bytes, .. } => Cow::Borrowed(main_bytes),
            ReloadableSource::File(path) => match std::fs::read(path) {
                Ok(d) => Cow::Owned(d),
                Err(e) => {
                    log::error!("Unitypackage reload failed: {e}");
                    return;
                }
            },
            ReloadableSource::Archive {
                original_path,
                archive_bytes,
                selected_entry_path,
                inner_kind,
            } => {
                if *inner_kind == crate::archive::ArchiveModelKind::UnityPackage {
                    // アーカイブ内 .unitypackage: 二重展開
                    match Self::extract_pkg_from_archive(
                        original_path,
                        archive_bytes.as_ref(),
                        selected_entry_path,
                    ) {
                        Ok(data) => Cow::Owned(data),
                        Err(e) => {
                            log::error!("Archive unitypackage extraction failed: {e}");
                            return;
                        }
                    }
                } else if let Some(snap) = archive_bytes {
                    Cow::Borrowed(snap.as_ref())
                } else {
                    match std::fs::read(original_path) {
                        Ok(d) => Cow::Owned(d),
                        Err(e) => {
                            log::error!("Unitypackage reload failed: {e}");
                            return;
                        }
                    }
                }
            }
        };
        let path = source.display_path();
        let assets = match crate::unitypackage::extract_all_assets(&archive_data) {
            Ok(a) => a,
            Err(e) => {
                log::error!("Unitypackage extraction failed: {e}");
                return;
            }
        };

        // pkg_model (GUID/パス) → pkg_model_name (basename) → selected_fbx_name (basename) の優先順で照合
        let fbx_list = crate::unitypackage::find_fbx_list(&assets);
        let vrm_list = crate::unitypackage::find_vrm_list(&assets);

        // 1. GUID/パスベースの正確な照合
        let found_by_locator = pkg_model.and_then(|loc| {
            crate::unitypackage::find_asset_by_pathname(&assets, &loc.pathname)
                .map(|idx| (idx, loc.kind))
        });

        let (model_index, model_type) = if let Some(found) = found_by_locator {
            found
        } else {
            // 2. basename フォールバック
            let search_name = pkg_model_name.or(self.selected_fbx_name.as_deref());
            if let Some(prev_name) = search_name {
                if let Some((idx, _)) = fbx_list.iter().find(|(_, name)| name == prev_name) {
                    (*idx, PkgModelType::Fbx)
                } else if let Some((idx, _)) = vrm_list.iter().find(|(_, name)| name == prev_name) {
                    (*idx, PkgModelType::Vrm)
                } else if !fbx_list.is_empty() {
                    (fbx_list[0].0, PkgModelType::Fbx)
                } else if !vrm_list.is_empty() {
                    (vrm_list[0].0, PkgModelType::Vrm)
                } else {
                    log::error!("No models found in unitypackage");
                    return;
                }
            } else if !fbx_list.is_empty() {
                (fbx_list[0].0, PkgModelType::Fbx)
            } else if !vrm_list.is_empty() {
                (vrm_list[0].0, PkgModelType::Vrm)
            } else {
                log::error!("No models found in unitypackage");
                return;
            }
        };

        // マージ前の材質オフセットを記録
        let mat_offset = self
            .loaded
            .as_ref()
            .map(|l| l.ir.materials.len())
            .unwrap_or(0);

        // 同期的にアペンド
        // sourceがArchiveの場合はsource_overrideとして渡す
        let source_override = match source {
            ReloadableSource::Archive { .. } => Some(source.clone()),
            ReloadableSource::Snapshot { .. } => Some(source.clone()),
            _ => None,
        };
        self.append_from_pkg(assets, model_index, model_type, path, source_override);

        // 追加材質に対するpkgテクスチャ割り当てを復元
        if !saved_pkg_tex_assignments.is_empty() {
            // 割り当て対象を先に収集（借用解放のため）
            let assignments_to_restore: Vec<(usize, String, Vec<u8>)> = {
                let pkg_src = self.tex.pkg_textures.as_deref().unwrap_or(&[]);
                let name_to_data: HashMap<&str, &[u8]> = pkg_src
                    .iter()
                    .map(|(name, data)| (name.as_str(), data.as_slice()))
                    .collect();
                let mat_count = self
                    .loaded
                    .as_ref()
                    .map(|l| l.ir.materials.len())
                    .unwrap_or(0);
                saved_pkg_tex_assignments
                    .iter()
                    .filter(|(idx, _)| **idx >= mat_offset && **idx < mat_count)
                    .filter_map(|(idx, tex_name)| {
                        name_to_data
                            .get(tex_name.as_str())
                            .map(|data| (*idx, tex_name.clone(), data.to_vec()))
                    })
                    .collect()
            };
            for (mat_idx, tex_name, data) in &assignments_to_restore {
                if self.assign_texture_data_to_material(*mat_idx, tex_name, data) {
                    self.tex.pkg_assignments.insert(*mat_idx, tex_name.clone());
                } else {
                    // 復元失敗 → 不正な履歴を除去
                    self.tex.pkg_assignments.remove(mat_idx);
                }
            }
        }
    }

    /// 展開済みアセットから指定FBXをロード
    pub fn load_fbx_from_assets(
        &mut self,
        assets: Vec<crate::unitypackage::ExtractedAsset>,
        fbx_index: usize,
        source_path: &std::path::Path,
        mode: FbxLoadMode,
        source_override: Option<ReloadableSource>,
        pkg_index: Option<&UnityPackageIndex>,
    ) -> anyhow::Result<()> {
        // pkg_index が与えられた場合は prepare_pkg_fbx + embed_textures_with_prefab を使用
        let (fbx_data, fbx_name, textures_legacy, pkg_textures_new, _unmatched_precomputed) =
            if let Some(idx) = pkg_index {
                let prepared = crate::unitypackage::prepare_pkg_fbx(idx, fbx_index)?;
                let fbx_name = std::path::Path::new(prepared.model.pathname.as_ref())
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_string();
                let fbx_data = prepared.fbx_data.to_vec();
                // PackageTexture → (String, Vec<u8>) 変換（既存 pkg_textures 形式）
                let legacy_textures: Vec<(String, Vec<u8>)> = prepared
                    .textures
                    .iter()
                    .map(|t| (t.display_name.to_string(), t.data.to_vec()))
                    .collect();
                (
                    fbx_data,
                    fbx_name,
                    legacy_textures,
                    Some(prepared),
                    None::<Vec<usize>>,
                )
            } else {
                let (fbx_data, fbx_name, textures) =
                    crate::unitypackage::take_fbx_and_textures(assets, fbx_index)?;
                (fbx_data, fbx_name, textures, None, None)
            };

        log::info!(
            "FBX in unitypackage: {} textures: {}",
            fbx_name,
            textures_legacy.len()
        );
        self.selected_fbx_name = Some(fbx_name.clone());
        self.selected_pkg_model = pkg_textures_new.as_ref().map(|p| p.model.clone());

        let load_model = matches!(mode, FbxLoadMode::ModelOnly | FbxLoadMode::Both);
        let load_animation = matches!(mode, FbxLoadMode::AnimationOnly | FbxLoadMode::Both);

        if load_model {
            // unitypackage 経由: fbx_path=None で FBX 近傍テクスチャ検索を無効化
            // テクスチャは embed_textures_with_prefab / embed_textures_into_ir で埋め込む
            let mut ir = crate::fbx::extract::extract_ir_model_from_fbx_with_options(
                &fbx_data,
                None,
                self.normalize_pose,
                self.normalize_to_tstance,
            )?;

            // テクスチャ埋め込み: pkg_index 経由なら embed_textures_with_prefab を使用
            let unmatched = if let Some(ref prepared) = pkg_textures_new {
                let prefab_label = format!(
                    "prefab({})",
                    std::path::Path::new(&*prepared.model.pathname)
                        .file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                );
                crate::unitypackage::embed_textures_with_prefab(
                    &mut ir,
                    &prepared.textures,
                    &prepared.resolved,
                    &prefab_label,
                )
            } else {
                crate::unitypackage::embed_textures_into_ir(&mut ir, &textures_legacy)
            };

            // テクスチャをアプリ状態に保持
            if !textures_legacy.is_empty() {
                self.tex.pkg_textures = Some(textures_legacy);
                self.rebuild_pkg_thumb_cache();
            }

            // pkg_material_keys の構築（pkg_index がある場合のみ）
            let pkg_keys = if let Some(idx) = pkg_index {
                let fbx_guid = idx
                    .entries
                    .get(fbx_index)
                    .map(|e| e.guid.as_str())
                    .unwrap_or("");
                let instance_id = crate::unitypackage::BASE_INSTANCE_ID;
                let model_guid: std::sync::Arc<str> = fbx_guid.into();
                ir.materials
                    .iter()
                    .map(|mat| {
                        Some(crate::unitypackage::PkgMaterialKey {
                            instance_id,
                            model_guid: model_guid.clone(),
                            source_material: mat.source_material.clone(),
                            material_name: mat.name.as_str().into(),
                        })
                    })
                    .collect::<Vec<_>>()
            } else {
                Vec::new()
            };

            let source = source_override
                .unwrap_or_else(|| ReloadableSource::File(source_path.to_path_buf()));
            self.finish_load(ir, source)?;

            // finish_load 後に pkg_material_keys を設定
            if !pkg_keys.is_empty() {
                if let Some(ref mut loaded) = self.loaded {
                    loaded.pkg_material_keys = pkg_keys;
                }
            }

            // モデル読み込み時はアニメーションをクリア
            self.anim.state = None;
            self.anim.library.clear();
            self.anim.active_index = None;

            // 未割当材質がある場合、手動割当ダイアログを開く（リロード中は抑制）
            if !unmatched.is_empty() && self.tex.pkg_textures.is_some() && !self.suppress_tex_match
            {
                // 既存プレビューがあれば先に復元
                self.cancel_tex_match_preview();
                let count = unmatched.len();
                self.tex.pending_match = Some(PendingTexMatch {
                    mat_indices: unmatched,
                    selections: vec![None; count],
                    tex_filter: String::new(),
                    previewed: vec![None; count],
                    saved_binds: std::collections::HashMap::new(),
                    texture_views: Vec::new(),
                    failed_uploads: std::collections::HashSet::new(),
                });
            }
        }

        if load_animation {
            if let Ok(anims) = crate::fbx::animation::load_fbx_animation_from_data(&fbx_data) {
                for anim in anims {
                    let display_name = if anim.name == "animation" {
                        fbx_name.clone()
                    } else {
                        format!("{} ({})", fbx_name, anim.name)
                    };
                    let anim = std::sync::Arc::new(anim);
                    if let Some(ref loaded) = self.loaded {
                        let state = AnimationState::new(
                            std::sync::Arc::clone(&anim),
                            &loaded.ir,
                            &loaded.gpu_model,
                        );
                        self.anim
                            .library
                            .push((display_name, source_path.to_path_buf(), anim));
                        self.anim.active_index = Some(self.anim.library.len() - 1);
                        self.anim.state = Some(state);
                    }
                }
            }
        }

        Ok(())
    }

    /// 展開済みアセットから指定VRMをロード
    pub fn load_vrm_from_assets(
        &mut self,
        assets: Vec<crate::unitypackage::ExtractedAsset>,
        vrm_index: usize,
        source_path: &std::path::Path,
        source_override: Option<ReloadableSource>,
    ) -> anyhow::Result<()> {
        // assets 消費前に pathname を取得（reload 時の正確な再選択用）
        let vrm_pathname: Option<String> = assets.get(vrm_index).map(|a| a.pathname.clone());
        let (vrm_data, vrm_name) = crate::unitypackage::take_vrm(assets, vrm_index)?;
        log::info!(
            "VRM in unitypackage: {} ({}KB)",
            vrm_name,
            vrm_data.len() / 1024
        );
        self.selected_fbx_name = Some(vrm_name.clone());
        // VRM は Prefab テクスチャマッピング対象外だが、reload 時のモデル再選択には pathname が必要
        self.selected_pkg_model = vrm_pathname.map(|path| crate::unitypackage::PkgModelLocator {
            guid: "".into(),
            pathname: path.into(),
            kind: crate::unitypackage::PkgModelType::Vrm,
        });

        let glb = vrm::loader::load_glb_from_data(&vrm_data)?;
        let version = vrm::detect::detect_version(&glb.document);
        let all_extensions = vrm::loader::get_raw_extensions(&glb.document);

        let mut ir = vrm::extract::extract_ir_model_with_options(
            &glb.document,
            &glb.buffers,
            &glb.images,
            &glb.vrm_extension,
            &version,
            &all_extensions,
            self.normalize_pose,
        )?;

        let device = &self.render_state.device;
        let queue = &self.render_state.queue;
        let mat_count = ir.materials.len();
        let (smooth, clear, nmap, bloom) =
            Self::per_mat_or_default_display(&self.material_display, mat_count);
        let gpu_model = super::super::mesh::build_gpu_model(
            &ir,
            &glb.images,
            device,
            queue,
            &smooth,
            &clear,
            &nmap,
            &bloom,
        )?;

        Self::encode_ir_textures_as_png(&mut ir, &glb.images);
        let source =
            source_override.unwrap_or_else(|| ReloadableSource::File(source_path.to_path_buf()));
        self.finish_load_with_gpu(ir, gpu_model, source)
    }

    /// Prefab エントリから参照先 FBX を解決してロード（複数 FBX マージ対応）
    pub fn load_prefab_from_assets(
        &mut self,
        _assets: Vec<crate::unitypackage::ExtractedAsset>,
        prefab_index: usize,
        source_path: &std::path::Path,
        source_override: Option<ReloadableSource>,
        pkg_index: Option<Arc<UnityPackageIndex>>,
    ) -> anyhow::Result<()> {
        let pkg = pkg_index
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Prefab ロードには pkg_index が必要です"))?;

        // Prefab から全 FBX GUID とマテリアル解決結果を取得
        let resolve_result = crate::unitypackage::resolve_single_prefab(pkg, prefab_index)?;

        log::info!(
            "Prefab resolved: {} FBX detected",
            resolve_result.entries.len()
        );

        // Prefab ファイル名を保存（ファイル階層表示用）
        let prefab_filename = std::path::Path::new(&pkg.entries[prefab_index].pathname)
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();

        // テクスチャ収集
        let textures: Vec<crate::unitypackage::PackageTexture> = pkg
            .entries
            .iter()
            .filter(|e| {
                let lower = e.pathname.to_lowercase();
                [
                    ".png", ".jpg", ".jpeg", ".tga", ".bmp", ".psd", ".tif", ".tiff",
                ]
                .iter()
                .any(|ext| lower.ends_with(ext))
            })
            .map(|e| {
                let display_name = std::path::Path::new(&e.pathname)
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy();
                crate::unitypackage::PackageTexture {
                    guid: Arc::from(e.guid.as_str()),
                    display_name: Arc::from(display_name.as_ref()),
                    data: Arc::clone(&e.data),
                    pathname: Arc::from(e.pathname.as_str()),
                }
            })
            .collect();

        // レガシー形式のテクスチャリストも構築（pkg_textures 用）
        let legacy_textures: Vec<(String, Vec<u8>)> = textures
            .iter()
            .map(|t| (t.display_name.to_string(), t.data.to_vec()))
            .collect();

        let mut base_ir: Option<crate::intermediate::types::IrModel> = None;
        let mut all_pkg_keys: Vec<Option<crate::unitypackage::PkgMaterialKey>> = Vec::new();
        let mut all_unmatched: Vec<usize> = Vec::new();
        // FBX ごとの材質範囲を追跡（MaterialGroup 分割用）
        let mut fbx_ranges: Vec<(String, usize, usize)> = Vec::new(); // (name, mat_start, mat_count)

        for (i, fbx_entry_info) in resolve_result.entries.iter().enumerate() {
            let fbx_entry = &pkg.entries[fbx_entry_info.fbx_index];
            let fbx_data = fbx_entry.data.to_vec();
            let fbx_name = std::path::Path::new(&fbx_entry.pathname)
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();

            log::info!(
                "  FBX[{}]: {} (GUID={})",
                i,
                fbx_name,
                fbx_entry_info.fbx_guid
            );

            // unitypackage 経由: fbx_path=None で FBX 近傍テクスチャ検索を無効化
            let mut ir = crate::fbx::extract::extract_ir_model_from_fbx_with_options(
                &fbx_data,
                None,
                self.normalize_pose,
                self.normalize_to_tstance,
            )?;

            let prefab_label = format!("prefab({})", prefab_filename);
            let unmatched = crate::unitypackage::embed_textures_with_prefab(
                &mut ir,
                &textures,
                &fbx_entry_info.materials,
                &prefab_label,
            );

            // pkg_material_keys 構築
            let instance_id = crate::unitypackage::BASE_INSTANCE_ID;
            let model_guid: Arc<str> = fbx_entry_info.fbx_guid.as_str().into();
            let keys: Vec<_> = ir
                .materials
                .iter()
                .map(|mat| {
                    Some(crate::unitypackage::PkgMaterialKey {
                        instance_id,
                        model_guid: model_guid.clone(),
                        source_material: mat.source_material.clone(),
                        material_name: mat.name.as_str().into(),
                    })
                })
                .collect();

            if let Some(ref mut base) = base_ir {
                // 2つ目以降: merge
                let mat_offset = base.materials.len();
                let mat_count = ir.materials.len();
                base.merge(ir);
                fbx_ranges.push((fbx_name, mat_offset, mat_count));
                // unmatched のインデックスを offset
                all_unmatched.extend(unmatched.iter().map(|&idx| idx + mat_offset));
                all_pkg_keys.extend(keys);
            } else {
                // 最初の FBX: ベースモデル
                let mat_count = ir.materials.len();
                fbx_ranges.push((fbx_name.clone(), 0, mat_count));
                self.selected_fbx_name = Some(fbx_name);
                self.selected_pkg_model = Some(crate::unitypackage::PkgModelLocator {
                    guid: fbx_entry_info.fbx_guid.as_str().into(),
                    pathname: fbx_entry.pathname.as_str().into(),
                    kind: crate::unitypackage::PkgModelType::Fbx,
                });
                all_unmatched = unmatched;
                all_pkg_keys = keys;
                base_ir = Some(ir);
            }
        }

        let ir = base_ir.ok_or_else(|| anyhow::anyhow!("Prefab に有効な FBX が見つかりません"))?;

        // テクスチャをアプリ状態に保持
        if !legacy_textures.is_empty() {
            self.tex.pkg_textures = Some(legacy_textures);
            self.rebuild_pkg_thumb_cache();
        }

        let source =
            source_override.unwrap_or_else(|| ReloadableSource::File(source_path.to_path_buf()));
        self.finish_load(ir, source)?;

        // finish_load 後に Prefab 情報と per-FBX MaterialGroup を設定
        if let Some(ref mut loaded) = self.loaded {
            loaded.prefab_name = Some(prefab_filename);
            loaded.prefab_entry_path = Some(pkg.entries[prefab_index].pathname.clone());

            if !all_pkg_keys.is_empty() {
                loaded.pkg_material_keys = all_pkg_keys;
            }

            // 複数 FBX がある場合、単一 MaterialGroup を FBX 別に分割
            if fbx_ranges.len() > 1 {
                let mut new_groups = Vec::with_capacity(fbx_ranges.len());
                for (name, mat_start, mat_count) in &fbx_ranges {
                    let mat_range = *mat_start..*mat_start + *mat_count;
                    // draw_range: 材質インデックスが範囲内に含まれる draw を検索
                    let mut draw_start = usize::MAX;
                    let mut draw_end = 0usize;
                    for (di, draw) in loaded.gpu_model.draws.iter().enumerate() {
                        if mat_range.contains(&draw.material_index) {
                            draw_start = draw_start.min(di);
                            draw_end = draw_end.max(di + 1);
                        }
                    }
                    if draw_start == usize::MAX {
                        draw_start = draw_end;
                    }
                    new_groups.push(MaterialGroup {
                        name: name.clone(),
                        material_range: mat_range,
                        draw_range: draw_start..draw_end,
                    });
                }
                loaded.material_groups = new_groups;
            }
        }

        // モデル読み込み時はアニメーションをクリア
        self.anim.state = None;
        self.anim.library.clear();
        self.anim.active_index = None;

        // 未割当材質がある場合、手動割当ダイアログを開く
        if !all_unmatched.is_empty() && self.tex.pkg_textures.is_some() && !self.suppress_tex_match
        {
            self.cancel_tex_match_preview();
            let count = all_unmatched.len();
            self.tex.pending_match = Some(PendingTexMatch {
                mat_indices: all_unmatched,
                selections: vec![None; count],
                tex_filter: String::new(),
                previewed: vec![None; count],
                saved_binds: std::collections::HashMap::new(),
                texture_views: Vec::new(),
                failed_uploads: std::collections::HashSet::new(),
            });
        }

        Ok(())
    }

    /// 拡張子に基づいてアニメーションファイルを読み込む
    pub fn load_animation_file(&mut self, path: &std::path::Path) {
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();
        match ext.as_str() {
            "glb" | "gltf" => self.try_load_gltf_animation(path),
            "fbx" => self.try_load_fbx_animation(path),
            "anim" => self.try_load_unity_animation(path),
            _ => self.try_load_vrma(path),
        }
    }

    pub fn try_load_vrma(&mut self, path: &std::path::Path) {
        if self.loaded.is_none() {
            self.convert_message = Some(ConvertMessage::failure(
                "VRMAを読み込むには先にVRMモデルを読み込んでください".to_string(),
            ));
            return;
        }

        match vrm::animation::load_vrma(path) {
            Ok(anim) => {
                let anim = Arc::new(anim);
                let name = path
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_string();
                let loaded = self.loaded.as_ref().expect("loaded は is_some 分岐内");
                let state = AnimationState::new(Arc::clone(&anim), &loaded.ir, &loaded.gpu_model);
                log::info!("VRMALoad success: {}", path.display());

                // ライブラリに追加（重複パスは上書き）
                let path_buf = path.to_path_buf();
                if let Some(idx) = self
                    .anim
                    .library
                    .iter()
                    .position(|(_, p, _)| p == &path_buf)
                {
                    self.anim.library[idx] = (name.clone(), path_buf, anim);
                    self.anim.active_index = Some(idx);
                } else {
                    self.anim.library.push((name.clone(), path_buf, anim));
                    self.anim.active_index = Some(self.anim.library.len() - 1);
                }

                self.anim.state = Some(state);
                self.convert_message = Some(ConvertMessage::success(format!(
                    "VRMA 読み込み成功: {}",
                    name
                )));
            }
            Err(e) => {
                log::error!("VRMALoad failed: {e}");
                self.convert_message =
                    Some(ConvertMessage::failure(format!("VRMA 読み込み失敗: {e}")));
            }
        }
    }

    /// FBXファイルからアニメーションを読み込む
    pub fn try_load_fbx_animation(&mut self, path: &std::path::Path) {
        if self.loaded.is_none() {
            self.convert_message = Some(ConvertMessage::failure(
                "アニメーションを読み込むには先にモデルを読み込んでください".to_string(),
            ));
            return;
        }

        let anim_result = match self.read_or_preloaded(path) {
            Ok(data) => crate::fbx::animation::load_fbx_animation_from_data(&data),
            Err(_) => crate::fbx::animation::load_fbx_animation(path),
        };
        match anim_result {
            Ok(anims) if anims.is_empty() => {
                // 空配列 → no-op（成功メッセージも出さない）
                log::debug!("FBX animation: empty (skipped)");
            }
            Ok(anims) => {
                let loaded = self.loaded.as_ref().expect("loaded は is_some 分岐内");
                let path_buf = path.to_path_buf();
                let file_name = path
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_string();

                for anim in anims {
                    let display_name = if anim.name == "animation" {
                        file_name.clone()
                    } else {
                        format!("{} ({})", file_name, anim.name)
                    };
                    let anim = Arc::new(anim);
                    let state =
                        AnimationState::new(Arc::clone(&anim), &loaded.ir, &loaded.gpu_model);

                    // ライブラリに追加
                    self.anim
                        .library
                        .push((display_name.clone(), path_buf.clone(), anim));
                    self.anim.active_index = Some(self.anim.library.len() - 1);
                    self.anim.state = Some(state);
                }

                log::info!("FBX animation loaded: {}", path.display());
                self.convert_message = Some(ConvertMessage::success(format!(
                    "FBX アニメーション読み込み成功: {}",
                    file_name
                )));
            }
            Err(e) => {
                log::warn!("FBX animation load failed: {e}");
                self.convert_message = Some(ConvertMessage::failure(format!(
                    "FBX アニメーション読み込み失敗: {e}"
                )));
            }
        }
    }

    /// Unity .animファイルからアニメーションを読み込む
    pub fn try_load_unity_animation(&mut self, path: &std::path::Path) {
        if self.loaded.is_none() {
            self.convert_message = Some(ConvertMessage::failure(
                "アニメーションを読み込むには先にモデルを読み込んでください".to_string(),
            ));
            return;
        }

        match crate::unity::animation::load_unity_anim(path, self.anim.muscle_scale) {
            Ok(anim) => {
                let loaded = self.loaded.as_ref().expect("loaded は is_some 分岐内");
                let path_buf = path.to_path_buf();
                let file_name = path
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_string();
                let display_name = format!("{} ({})", file_name, anim.name);
                let anim = Arc::new(anim);
                let state = AnimationState::new(Arc::clone(&anim), &loaded.ir, &loaded.gpu_model);

                self.anim.library.push((display_name, path_buf, anim));
                self.anim.active_index = Some(self.anim.library.len() - 1);
                self.anim.state = Some(state);

                log::info!("Unity .animLoad success: {}", path.display());
                self.convert_message = Some(ConvertMessage::success(format!(
                    "Unity .anim 読み込み成功: {}",
                    file_name
                )));
            }
            Err(e) => {
                log::error!("Unity .animLoad failed: {e}");
                self.convert_message = Some(ConvertMessage::failure(format!(
                    "Unity .anim 読み込み失敗: {e}"
                )));
            }
        }
    }

    /// GLB/glTFファイルからアニメーションを読み込む
    pub fn try_load_gltf_animation(&mut self, path: &std::path::Path) {
        if self.loaded.is_none() {
            self.convert_message = Some(ConvertMessage::failure(
                "アニメーションを読み込むには先にモデルを読み込んでください".to_string(),
            ));
            return;
        }

        match vrm::animation::load_gltf_animation(path) {
            Ok(anims) => {
                let loaded = self.loaded.as_ref().expect("loaded は is_some 分岐内");
                let path_buf = path.to_path_buf();
                let file_name = path
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_string();

                for anim in anims {
                    let display_name = if anim.name == "animation" {
                        file_name.clone()
                    } else {
                        format!("{} ({})", file_name, anim.name)
                    };
                    let anim = Arc::new(anim);
                    let state =
                        AnimationState::new(Arc::clone(&anim), &loaded.ir, &loaded.gpu_model);

                    // ライブラリに追加
                    self.anim
                        .library
                        .push((display_name.clone(), path_buf.clone(), anim));
                    self.anim.active_index = Some(self.anim.library.len() - 1);
                    self.anim.state = Some(state);
                }

                log::info!("glTF animation loaded: {}", path.display());
                self.convert_message = Some(ConvertMessage::success(format!(
                    "アニメーション読み込み成功: {}",
                    file_name
                )));
            }
            Err(e) => {
                log::error!("glTF animation load failed: {e}");
                self.convert_message = Some(ConvertMessage::failure(format!(
                    "アニメーション読み込み失敗: {e}"
                )));
            }
        }
    }

    /// VRMAライブラリからインデックス指定で切り替え
    pub fn switch_vrma(&mut self, index: usize) {
        if let Some((_, _, ref anim)) = self.anim.library.get(index) {
            if let Some(ref loaded) = self.loaded {
                let state = AnimationState::new(Arc::clone(anim), &loaded.ir, &loaded.gpu_model);
                self.anim.state = Some(state);
                self.anim.active_index = Some(index);
            }
        }
    }

    fn try_load_pmx(&mut self, path: &std::path::Path) -> anyhow::Result<()> {
        let source = if is_temp_path(path)
            || self.preloaded.as_ref().is_some_and(|pl| pl.path == path)
        {
            let main_data: Arc<[u8]> = self.read_or_preloaded(path)?;
            let pmx_model = crate::pmx::reader::read_pmx_from_data(&main_data)?;
            let pmx_dir = path.parent().unwrap_or(Path::new("."));

            // 補助ファイル（テクスチャ）を収集: preloaded.aux_files を優先
            let mut aux = HashMap::new();
            let preloaded_aux = self
                .preloaded
                .as_ref()
                .filter(|pl| pl.path == path)
                .map(|pl| &pl.aux_files);
            for tex_path in &pmx_model.textures {
                let normalized = tex_path.replace('\\', "/");
                let key = PathBuf::from(&normalized);
                // preloaded aux_files からの取得を優先
                if let Some(data) = preloaded_aux.and_then(|a| a.get(&key)) {
                    aux.insert(key, Arc::clone(data));
                } else {
                    let full_path = pmx_dir.join(&normalized);
                    if let Ok(data) = std::fs::read(&full_path) {
                        aux.insert(key, Arc::from(data.into_boxed_slice()));
                    }
                }
            }

            let mut ir = crate::pmx::extract::pmx_to_ir_with_aux(&pmx_model, pmx_dir, Some(&aux))?;
            if self.normalize_pose {
                ir.astance_result = crate::intermediate::pose::normalize_pose_to_tstance_full(
                    &mut ir.bones,
                    &mut ir.meshes,
                    &mut ir.morphs,
                    &mut ir.physics,
                    crate::convert::coord::gltf_pos_to_pmx,
                );
            }

            let source = ReloadableSource::Snapshot {
                original_path: path.to_path_buf(),
                main_bytes: main_data,
                aux_files: aux,
            };
            return self.finish_load(ir, source);
        } else {
            ReloadableSource::File(path.to_path_buf())
        };

        let pmx_model = crate::pmx::reader::read_pmx(path)?;
        let pmx_dir = path.parent().unwrap_or(Path::new("."));
        let mut ir = crate::pmx::extract::pmx_to_ir(&pmx_model, pmx_dir)?;

        if self.normalize_pose {
            ir.astance_result = crate::intermediate::pose::normalize_pose_to_tstance_full(
                &mut ir.bones,
                &mut ir.meshes,
                &mut ir.morphs,
                &mut ir.physics,
                crate::convert::coord::gltf_pos_to_pmx,
            );
        }

        self.finish_load(ir, source)
    }

    fn try_load_pmd(&mut self, path: &std::path::Path) -> anyhow::Result<()> {
        let source =
            if is_temp_path(path) || self.preloaded.as_ref().is_some_and(|pl| pl.path == path) {
                let main_data: Arc<[u8]> = self.read_or_preloaded(path)?;
                let pmd_model = crate::pmd::reader::read_pmd_from_data(&main_data)?;
                let pmd_dir = path.parent().unwrap_or(Path::new("."));

                // 補助ファイル（テクスチャ + .txt）を収集: preloaded.aux_files を優先
                let mut aux = HashMap::new();
                let preloaded_aux = self
                    .preloaded
                    .as_ref()
                    .filter(|pl| pl.path == path)
                    .map(|pl| &pl.aux_files);
                // テクスチャ
                for mat in &pmd_model.materials {
                    if mat.texture_name.is_empty() {
                        continue;
                    }
                    let main_tex = mat.texture_name.split('*').next().unwrap_or("");
                    if main_tex.is_empty() {
                        continue;
                    }
                    let normalized = main_tex.replace('\\', "/");
                    let key = PathBuf::from(&normalized);
                    if aux.contains_key(&key) {
                        continue;
                    }
                    // preloaded aux_files からの取得を優先
                    if let Some(data) = preloaded_aux.and_then(|a| a.get(&key)) {
                        aux.insert(key, Arc::clone(data));
                    } else {
                        let full_path = pmd_dir.join(&normalized);
                        if let Ok(data) = std::fs::read(&full_path) {
                            aux.insert(key, Arc::from(data.into_boxed_slice()));
                        }
                    }
                }
                // .txt ファイル
                let txt_path = path.with_extension("txt");
                let txt_name = txt_path
                    .file_name()
                    .map(|f| PathBuf::from(f))
                    .unwrap_or_default();
                if let Some(data) = preloaded_aux.and_then(|a| a.get(&txt_name)) {
                    aux.insert(txt_name, Arc::clone(data));
                } else if let Ok(data) = std::fs::read(&txt_path) {
                    aux.insert(txt_name, Arc::from(data.into_boxed_slice()));
                }

                let mut ir = crate::pmd::extract::pmd_to_ir_with_aux(&pmd_model, path, Some(&aux))?;
                if self.normalize_pose {
                    ir.astance_result = crate::intermediate::pose::normalize_pose_to_tstance_full(
                        &mut ir.bones,
                        &mut ir.meshes,
                        &mut ir.morphs,
                        &mut ir.physics,
                        crate::convert::coord::gltf_pos_to_pmx,
                    );
                }

                let source = ReloadableSource::Snapshot {
                    original_path: path.to_path_buf(),
                    main_bytes: main_data,
                    aux_files: aux,
                };
                return self.finish_load(ir, source);
            } else {
                ReloadableSource::File(path.to_path_buf())
            };

        let pmd_model = crate::pmd::reader::read_pmd(path)?;
        let mut ir = crate::pmd::extract::pmd_to_ir(&pmd_model, path)?;

        if self.normalize_pose {
            ir.astance_result = crate::intermediate::pose::normalize_pose_to_tstance_full(
                &mut ir.bones,
                &mut ir.meshes,
                &mut ir.morphs,
                &mut ir.physics,
                crate::convert::coord::gltf_pos_to_pmx,
            );
        }

        self.finish_load(ir, source)
    }

    fn try_load_obj(&mut self, path: &std::path::Path) -> anyhow::Result<()> {
        let source =
            if is_temp_path(path) || self.preloaded.as_ref().is_some_and(|pl| pl.path == path) {
                let main_data: Arc<[u8]> = self.read_or_preloaded(path)?;
                let obj_dir = path.parent().unwrap_or(Path::new("."));
                let aux = self.take_or_collect_aux(path);
                let name = path.file_stem().and_then(|s| s.to_str()).unwrap_or("Model");
                let ir =
                    crate::obj::extract::load_obj_from_data(&main_data, name, obj_dir, Some(&aux))?;

                let source = ReloadableSource::Snapshot {
                    original_path: path.to_path_buf(),
                    main_bytes: main_data,
                    aux_files: aux,
                };
                return self.finish_load(ir, source);
            } else {
                ReloadableSource::File(path.to_path_buf())
            };

        let ir = crate::obj::extract::load_obj(path)?;
        self.finish_load(ir, source)
    }

    fn try_load_stl(&mut self, path: &std::path::Path) -> anyhow::Result<()> {
        let source =
            if is_temp_path(path) || self.preloaded.as_ref().is_some_and(|pl| pl.path == path) {
                let main_data: Arc<[u8]> = self.read_or_preloaded(path)?;
                let name = path.file_stem().and_then(|s| s.to_str()).unwrap_or("Model");
                let ir = crate::stl::extract::load_stl_from_data(&main_data, name)?;

                let source = ReloadableSource::Snapshot {
                    original_path: path.to_path_buf(),
                    main_bytes: main_data,
                    aux_files: HashMap::new(),
                };
                return self.finish_load(ir, source);
            } else {
                ReloadableSource::File(path.to_path_buf())
            };

        let ir = crate::stl::extract::load_stl(path)?;
        self.finish_load(ir, source)
    }

    fn try_load_x(&mut self, path: &std::path::Path) -> anyhow::Result<()> {
        let source =
            if is_temp_path(path) || self.preloaded.as_ref().is_some_and(|pl| pl.path == path) {
                let main_data: Arc<[u8]> = self.read_or_preloaded(path)?;
                let x_dir = path.parent().unwrap_or(Path::new("."));
                let aux = self.take_or_collect_aux(path);
                let name = path.file_stem().and_then(|s| s.to_str()).unwrap_or("Model");
                let ir =
                    crate::directx::extract::load_x_from_data(&main_data, name, x_dir, Some(&aux))?;

                let source = ReloadableSource::Snapshot {
                    original_path: path.to_path_buf(),
                    main_bytes: main_data,
                    aux_files: aux,
                };
                return self.finish_load(ir, source);
            } else {
                ReloadableSource::File(path.to_path_buf())
            };

        let ir = crate::directx::extract::load_x(path)?;
        self.finish_load(ir, source)
    }

    fn try_load_vrm(&mut self, path: &std::path::Path) -> anyhow::Result<()> {
        // .gltf は外部バッファ参照を持つためスナップショット化しない（.glb/.vrm のみ対象）
        let ext_lower = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();
        let source = if (is_temp_path(path)
            || self.preloaded.as_ref().is_some_and(|pl| pl.path == path))
            && ext_lower != "gltf"
        {
            let data: Arc<[u8]> = self.read_or_preloaded(path)?;
            let glb = vrm::loader::load_glb_from_data(&data)?;
            let version = vrm::detect::detect_version(&glb.document);
            let all_extensions = vrm::loader::get_raw_extensions(&glb.document);

            let mut ir = vrm::extract::extract_ir_model_with_options(
                &glb.document,
                &glb.buffers,
                &glb.images,
                &glb.vrm_extension,
                &version,
                &all_extensions,
                self.normalize_pose,
            )?;

            let device = &self.render_state.device;
            let queue = &self.render_state.queue;
            let mc = ir.materials.len();
            let (smooth, clear, nmap, bloom) =
                Self::per_mat_or_default_display(&self.material_display, mc);
            let gpu_model = super::super::mesh::build_gpu_model(
                &ir,
                &glb.images,
                device,
                queue,
                &smooth,
                &clear,
                &nmap,
                &bloom,
            )?;
            Self::encode_ir_textures_as_png(&mut ir, &glb.images);

            let source = ReloadableSource::Snapshot {
                original_path: path.to_path_buf(),
                main_bytes: data,
                aux_files: HashMap::new(),
            };
            return self.finish_load_with_gpu(ir, gpu_model, source);
        } else {
            ReloadableSource::File(path.to_path_buf())
        };

        let glb = vrm::loader::load_glb(path)?;
        let version = vrm::detect::detect_version(&glb.document);
        let all_extensions = vrm::loader::get_raw_extensions(&glb.document);

        let mut ir = vrm::extract::extract_ir_model_with_options(
            &glb.document,
            &glb.buffers,
            &glb.images,
            &glb.vrm_extension,
            &version,
            &all_extensions,
            self.normalize_pose,
        )?;

        let device = &self.render_state.device;
        let queue = &self.render_state.queue;
        let mc = ir.materials.len();
        let (smooth, clear, nmap, bloom) =
            Self::per_mat_or_default_display(&self.material_display, mc);
        let gpu_model = super::super::mesh::build_gpu_model(
            &ir,
            &glb.images,
            device,
            queue,
            &smooth,
            &clear,
            &nmap,
            &bloom,
        )?;

        // IrTexture を PNG エンコード済みに変換（convert_ir_to_pmx で統一的に使えるように）
        Self::encode_ir_textures_as_png(&mut ir, &glb.images);

        self.finish_load_with_gpu(ir, gpu_model, source)
    }

    /// リロード前の状態をスナップショットとして退避する。
    fn save_reload_snapshot(&mut self) -> ReloadSnapshot {
        let appended_models = self
            .loaded
            .as_ref()
            .map(|l| l.appended_models.clone())
            .unwrap_or_default();
        ReloadSnapshot {
            appended_models,
            camera: self.camera.clone(),
            morph_weights: std::mem::take(&mut self.morph_weights),
            material_visibility: std::mem::take(&mut self.material_visibility),
            material_display: std::mem::take(&mut self.material_display),
            material_filter: std::mem::take(&mut self.material_filter),
            pmx_output_path: std::mem::take(&mut self.export.pmx_output_path),
            export_visible_only: self.export.export_visible_only,
            tex_assignments: std::mem::take(&mut self.tex.assignments),
            pkg_tex_assignments: std::mem::take(&mut self.tex.pkg_assignments),
            pkg_textures: self.tex.pkg_textures.take(),
            vrma_library: std::mem::take(&mut self.anim.library),
            vrma_active_index: self.anim.active_index.take(),
            display: self.display.clone(),
        }
    }

    /// リロード失敗時にスナップショットから状態を復元する。
    /// 旧モデルがそのまま残るので、`save_reload_snapshot` で退避した全フィールドをそのまま書き戻す。
    fn restore_snapshot_on_failure(&mut self, snap: ReloadSnapshot) {
        self.camera = snap.camera;
        self.morph_weights = snap.morph_weights;
        self.morph_dirty = true;
        self.material_visibility = snap.material_visibility;
        self.material_display = snap.material_display;
        self.material_filter = snap.material_filter;
        self.export.pmx_output_path = snap.pmx_output_path;
        self.export.export_visible_only = snap.export_visible_only;
        if let Some(pkg) = snap.pkg_textures {
            self.tex.pkg_textures = Some(pkg);
        }
        self.tex.assignments = snap.tex_assignments;
        self.tex.pkg_assignments = snap.pkg_tex_assignments;
        self.anim.library = snap.vrma_library;
        self.anim.active_index = snap.vrma_active_index;
        self.display = snap.display;
        self.suppress_tex_match = false;
    }

    /// リロード成功後にスナップショットから状態を復元する。
    fn restore_snapshot_on_success(&mut self, snap: ReloadSnapshot) {
        // pkg_textures を復元
        if self.tex.pkg_textures.is_none() {
            self.tex.pkg_textures = snap.pkg_textures;
        }

        // カメラ復元（リロード時はカメラリセットしない）
        self.camera = snap.camera;
        self.pending.refit = false;

        // モーフ数が一致する場合のみ復元
        if snap.morph_weights.len() == self.morph_weights.len() {
            self.morph_weights = snap.morph_weights;
            self.morph_dirty = true;
        }
        // 材質数が一致する場合のみ復元
        if snap.material_visibility.len() == self.material_visibility.len() {
            self.material_visibility = snap.material_visibility;
        }
        if snap.material_display.len() == self.material_display.len() {
            self.material_display = snap.material_display;
        }
        // per-mat フラグが復元された場合、GPU モデルを再構築して反映
        if self.material_display.iter().any(|d| d.smooth_normals)
            || self.material_display.iter().any(|d| d.clear_normals)
            || self.material_display.iter().any(|d| !d.normal_map)
            || self.material_display.iter().any(|d| !d.bloom)
        {
            self.pending.rebuild = Some(PendingOverlay::WaitingOverlay);
        }
        self.material_filter = snap.material_filter;
        self.export.pmx_output_path = snap.pmx_output_path;
        self.export.export_visible_only = snap.export_visible_only;

        // テクスチャ割り当てを復元（ファイルパス分のみ。pkg分はreload_unitypackage内で処理済み）
        let saved_link = self.tex.link_same_name;
        self.tex.link_same_name = false;
        self.tex.assignments = HashMap::new();
        let current_mat_count = self
            .loaded
            .as_ref()
            .map(|l| l.ir.materials.len())
            .unwrap_or(0);
        for (mat_idx, tex_src) in &snap.tex_assignments {
            if *mat_idx < current_mat_count {
                self.assign_texture_source_to_material(*mat_idx, tex_src);
            }
        }
        self.tex.link_same_name = saved_link;

        // VRMAライブラリを復元し、アクティブなアニメーションを再構築
        if !snap.vrma_library.is_empty() {
            self.anim.library = snap.vrma_library;
            if let Some(idx) = snap.vrma_active_index {
                self.switch_vrma(idx);
            }
        }
        // 表示設定を復元（シェーダーオーバーライド・ライト・Bloom 等）
        self.display = snap.display;
        // リロード完了: テクスチャ選択ダイアログ抑制を解除
        self.suppress_tex_match = false;
    }

    /// 現在読み込み中のVRMを再読み込みする（オプション変更時）
    /// カメラ・モーフ・材質表示などの状態は保持する
    pub fn reload_current(&mut self) {
        if self.loaded.is_none() {
            return;
        }
        // リロード前にプレビューを復元（旧モデルの GPU リソースが有効な間に実行）
        self.cancel_tex_match_preview();
        // リロード中はテクスチャ選択ダイアログを抑制
        self.suppress_tex_match = true;
        let Some(loaded) = self.loaded.as_ref() else {
            return;
        };
        let source = loaded.source.clone();

        // スナップショットに状態を退避
        let snap = self.save_reload_snapshot();

        // unitypackage の場合は再展開せず FBX として再読み込み
        let ext = source.extension_lower();
        let result = match &source {
            ReloadableSource::Archive {
                inner_kind,
                original_path,
                archive_bytes,
                selected_entry_path,
            } if *inner_kind == crate::archive::ArchiveModelKind::UnityPackage => self
                .reload_archive_unitypackage(
                    original_path,
                    archive_bytes.as_ref(),
                    selected_entry_path,
                    &source,
                    &snap.pkg_textures,
                    &snap.pkg_tex_assignments,
                ),
            _ if ext == "unitypackage" => {
                self.reload_unitypackage(&source, &snap.pkg_textures, &snap.pkg_tex_assignments)
            }
            _ => self.reload_from_source(&source),
        };

        // リロード失敗時は状態変更をスキップして早期リターン
        if let Err(e) = result {
            log::error!("Reload failed: {e}");
            self.convert_message = Some(ConvertMessage::failure(format!("リロード失敗: {e}")));
            self.restore_snapshot_on_failure(snap);
            return;
        }

        // 追加モデルを再マージ（ベースモデルが正しく再ロードされた場合のみ）
        // 再ロード成功 = loaded の appended_models が空（新規 LoadedModel が作られた）
        if let Some(ref loaded) = self.loaded {
            if loaded.appended_models.is_empty() && !snap.appended_models.is_empty() {
                // リロード中フラグON（テクスチャ選択ダイアログ抑制）
                self.suppress_tex_match = true;
                for appended in &snap.appended_models {
                    match &appended.source {
                        ReloadableSource::Archive { inner_kind, .. }
                            if *inner_kind == crate::archive::ArchiveModelKind::UnityPackage =>
                        {
                            // アーカイブ内 unitypackage は同期的にアペンド
                            self.reload_append_unitypackage(
                                &appended.source,
                                appended.pkg_model_name.as_deref(),
                                appended.pkg_model.as_ref(),
                                &snap.pkg_tex_assignments,
                            );
                        }
                        _ if appended.source.extension_lower() == "unitypackage" => {
                            // 通常の unitypackage は同期的にアペンド（遅延処理を避ける）
                            self.reload_append_unitypackage(
                                &appended.source,
                                appended.pkg_model_name.as_deref(),
                                appended.pkg_model.as_ref(),
                                &snap.pkg_tex_assignments,
                            );
                        }
                        _ => {
                            self.append_model_from_source(
                                &appended.source,
                                appended.pkg_model_name.as_deref(),
                                appended.pkg_model.as_ref(),
                            );
                        }
                    }
                }
                self.suppress_tex_match = false;
                // リロード経由の再アペンドではテクスチャ選択ダイアログを抑制
                self.cancel_tex_match_preview();
                // アペンド失敗のエラーメッセージは保持、成功メッセージのみクリア
                if let Some(ref msg) = self.convert_message {
                    if matches!(
                        msg.result,
                        ConvertResult::Success(_) | ConvertResult::Warning(_)
                    ) {
                        self.convert_message = None;
                    }
                }
            }
        }

        self.restore_snapshot_on_success(snap);
    }

    /// ReloadableSource からモデルを再読み込み（load_file の UI 分岐を回避）
    fn reload_from_source(&mut self, source: &ReloadableSource) -> anyhow::Result<()> {
        let source_clone = source.clone();
        let result: anyhow::Result<()> = (|| {
            match &source_clone {
                ReloadableSource::File(path) => {
                    let ext = path
                        .extension()
                        .and_then(|e| e.to_str())
                        .unwrap_or("")
                        .to_lowercase();
                    match detect_format(&ext) {
                        FileFormat::Fbx => self.try_load_fbx(path),
                        FileFormat::Pmx => self.try_load_pmx(path),
                        FileFormat::Pmd => self.try_load_pmd(path),
                        FileFormat::Obj => self.try_load_obj(path),
                        FileFormat::Stl => self.try_load_stl(path),
                        FileFormat::DirectX => self.try_load_x(path),
                        _ => self.try_load_vrm(path),
                    }
                }
                ReloadableSource::Snapshot {
                    original_path,
                    main_bytes,
                    aux_files,
                } => {
                    let ext = original_path
                        .extension()
                        .and_then(|e| e.to_str())
                        .unwrap_or("")
                        .to_lowercase();
                    match detect_format(&ext) {
                        FileFormat::Fbx => {
                            // 外部テクスチャがある場合、ユニーク名の一時ディレクトリに復元（TempDir の Drop で自動削除）。
                            // 固定名だと BG ロード並行時にディレクトリが衝突するため、tempfile で毎回ユニーク名を生成する。
                            let temp_dir = if !aux_files.is_empty() {
                                let td = tempfile::Builder::new()
                                    .prefix("popone_fbx_reload_")
                                    .tempdir()?;
                                for (rel_path, data) in aux_files {
                                    let target = td.path().join(rel_path);
                                    if let Some(parent) = target.parent() {
                                        std::fs::create_dir_all(parent)?;
                                    }
                                    std::fs::write(&target, data.as_ref())?;
                                }
                                Some(td)
                            } else {
                                None
                            };
                            let fbx_path = temp_dir
                                .as_ref()
                                .map(|d| {
                                    d.path().join(original_path.file_name().unwrap_or_default())
                                })
                                .unwrap_or_else(|| original_path.clone());
                            let ir = crate::fbx::extract::extract_ir_model_from_fbx_with_options(
                                main_bytes,
                                Some(&fbx_path),
                                self.normalize_pose,
                                self.normalize_to_tstance,
                            )?;
                            // temp_dir はここでスコープ終了 → TempDir::drop が自動削除する
                            drop(temp_dir);
                            self.finish_load(ir, source_clone.clone())
                        }
                        FileFormat::Pmx => {
                            let pmx_model = crate::pmx::reader::read_pmx_from_data(main_bytes)?;
                            let pmx_dir = original_path.parent().unwrap_or(Path::new("."));
                            let mut ir = crate::pmx::extract::pmx_to_ir_with_aux(
                                &pmx_model,
                                pmx_dir,
                                Some(aux_files),
                            )?;
                            if self.normalize_pose {
                                ir.astance_result =
                                    crate::intermediate::pose::normalize_pose_to_tstance_full(
                                        &mut ir.bones,
                                        &mut ir.meshes,
                                        &mut ir.morphs,
                                        &mut ir.physics,
                                        crate::convert::coord::gltf_pos_to_pmx,
                                    );
                            }
                            self.finish_load(ir, source_clone.clone())
                        }
                        FileFormat::Pmd => {
                            let pmd_model = crate::pmd::reader::read_pmd_from_data(main_bytes)?;
                            let mut ir = crate::pmd::extract::pmd_to_ir_with_aux(
                                &pmd_model,
                                original_path,
                                Some(aux_files),
                            )?;
                            if self.normalize_pose {
                                ir.astance_result =
                                    crate::intermediate::pose::normalize_pose_to_tstance_full(
                                        &mut ir.bones,
                                        &mut ir.meshes,
                                        &mut ir.morphs,
                                        &mut ir.physics,
                                        crate::convert::coord::gltf_pos_to_pmx,
                                    );
                            }
                            self.finish_load(ir, source_clone.clone())
                        }
                        FileFormat::Obj => {
                            let obj_dir = original_path.parent().unwrap_or(Path::new("."));
                            let name = original_path
                                .file_stem()
                                .and_then(|s| s.to_str())
                                .unwrap_or("Model");
                            let ir = crate::obj::extract::load_obj_from_data(
                                main_bytes,
                                name,
                                obj_dir,
                                Some(aux_files),
                            )?;
                            self.finish_load(ir, source_clone.clone())
                        }
                        FileFormat::Stl => {
                            let name = original_path
                                .file_stem()
                                .and_then(|s| s.to_str())
                                .unwrap_or("Model");
                            let ir = crate::stl::extract::load_stl_from_data(main_bytes, name)?;
                            self.finish_load(ir, source_clone.clone())
                        }
                        FileFormat::DirectX => {
                            let x_dir = original_path.parent().unwrap_or(Path::new("."));
                            let name = original_path
                                .file_stem()
                                .and_then(|s| s.to_str())
                                .unwrap_or("Model");
                            let ir = crate::directx::extract::load_x_from_data(
                                main_bytes,
                                name,
                                x_dir,
                                Some(aux_files),
                            )?;
                            self.finish_load(ir, source_clone.clone())
                        }
                        _ => {
                            // VRM
                            let glb = vrm::loader::load_glb_from_data(main_bytes)?;
                            let version = vrm::detect::detect_version(&glb.document);
                            let all_extensions = vrm::loader::get_raw_extensions(&glb.document);
                            let mut ir = vrm::extract::extract_ir_model_with_options(
                                &glb.document,
                                &glb.buffers,
                                &glb.images,
                                &glb.vrm_extension,
                                &version,
                                &all_extensions,
                                self.normalize_pose,
                            )?;
                            let device = &self.render_state.device;
                            let queue = &self.render_state.queue;
                            let mc = ir.materials.len();
                            let (smooth, clear, nmap, bloom) =
                                Self::per_mat_or_default_display(&self.material_display, mc);
                            let gpu_model = super::super::mesh::build_gpu_model(
                                &ir,
                                &glb.images,
                                device,
                                queue,
                                &smooth,
                                &clear,
                                &nmap,
                                &bloom,
                            )?;
                            Self::encode_ir_textures_as_png(&mut ir, &glb.images);
                            self.finish_load_with_gpu(ir, gpu_model, source_clone.clone())
                        }
                    }
                }
                ReloadableSource::Archive {
                    original_path,
                    archive_bytes,
                    selected_entry_path,
                    inner_kind,
                } => {
                    if *inner_kind == crate::archive::ArchiveModelKind::UnityPackage {
                        // アーカイブ内 .unitypackage: 二重展開してリロード
                        // （reload_current 経由では saved_pkg_textures/assignments が渡されるため、
                        //   ここでは空デフォルトを使用）
                        return self.reload_archive_unitypackage(
                            original_path,
                            archive_bytes.as_ref(),
                            selected_entry_path,
                            &source_clone,
                            &None,
                            &HashMap::new(),
                        );
                    }
                    let ir = self.load_ir_from_archive_source(
                        original_path,
                        archive_bytes.as_ref(),
                        selected_entry_path,
                        *inner_kind,
                    )?;
                    self.finish_load(ir, source_clone.clone())
                }
            }
        })();
        if let Err(ref e) = result {
            log::error!("reload_from_source failed: {e}");
            self.convert_message = Some(ConvertMessage::failure(format!("リロード失敗: {e}")));
        }
        result
    }

    /// ReloadableSource から追加モデルを読み込み（リロード時用）
    fn append_model_from_source(
        &mut self,
        source: &ReloadableSource,
        pkg_model_name: Option<&str>,
        pkg_model: Option<&crate::unitypackage::PkgModelLocator>,
    ) {
        // アーカイブ内 .unitypackage は専用パスで処理
        if let ReloadableSource::Archive { inner_kind, .. } = source {
            if *inner_kind == crate::archive::ArchiveModelKind::UnityPackage {
                self.reload_append_unitypackage(source, pkg_model_name, pkg_model, &HashMap::new());
                return;
            }
        }

        let source_clone = source.clone();
        let ir_result: anyhow::Result<IrModel> = (|| -> anyhow::Result<IrModel> {
            match &source_clone {
                ReloadableSource::File(path) => {
                    let ext = path
                        .extension()
                        .and_then(|e| e.to_str())
                        .unwrap_or("")
                        .to_lowercase();
                    match detect_format(&ext) {
                        FileFormat::Fbx => {
                            let ir = crate::fbx::extract::extract_ir_model_from_fbx_with_options(
                                &std::fs::read(path)?,
                                Some(path),
                                self.normalize_pose,
                                self.normalize_to_tstance,
                            )?;
                            Ok(ir)
                        }
                        FileFormat::Pmx => {
                            let pmx_model = crate::pmx::reader::read_pmx(path)?;
                            let pmx_dir = path.parent().unwrap_or(Path::new("."));
                            let mut ir = crate::pmx::extract::pmx_to_ir(&pmx_model, pmx_dir)?;
                            if self.normalize_pose {
                                ir.astance_result =
                                    crate::intermediate::pose::normalize_pose_to_tstance_full(
                                        &mut ir.bones,
                                        &mut ir.meshes,
                                        &mut ir.morphs,
                                        &mut ir.physics,
                                        crate::convert::coord::gltf_pos_to_pmx,
                                    );
                            }
                            Ok(ir)
                        }
                        FileFormat::Pmd => {
                            let pmd_model = crate::pmd::reader::read_pmd(path)?;
                            let mut ir = crate::pmd::extract::pmd_to_ir(&pmd_model, path)?;
                            if self.normalize_pose {
                                ir.astance_result =
                                    crate::intermediate::pose::normalize_pose_to_tstance_full(
                                        &mut ir.bones,
                                        &mut ir.meshes,
                                        &mut ir.morphs,
                                        &mut ir.physics,
                                        crate::convert::coord::gltf_pos_to_pmx,
                                    );
                            }
                            Ok(ir)
                        }
                        FileFormat::Obj => Ok(crate::obj::extract::load_obj(path)?),
                        FileFormat::Stl => Ok(crate::stl::extract::load_stl(path)?),
                        FileFormat::DirectX => Ok(crate::directx::extract::load_x(path)?),
                        _ => self.load_vrm_as_ir(path),
                    }
                }
                ReloadableSource::Snapshot {
                    original_path,
                    main_bytes,
                    aux_files,
                } => {
                    let ext = original_path
                        .extension()
                        .and_then(|e| e.to_str())
                        .unwrap_or("")
                        .to_lowercase();
                    match detect_format(&ext) {
                        FileFormat::Fbx => {
                            // 固定名だと BG ロード並行時にディレクトリが衝突するため、tempfile で毎回ユニーク名を生成する。
                            let temp_dir = if !aux_files.is_empty() {
                                let td = tempfile::Builder::new()
                                    .prefix("popone_fbx_reload_")
                                    .tempdir()?;
                                for (rel_path, data) in aux_files {
                                    let target = td.path().join(rel_path);
                                    if let Some(parent) = target.parent() {
                                        std::fs::create_dir_all(parent)?;
                                    }
                                    std::fs::write(&target, data.as_ref())?;
                                }
                                Some(td)
                            } else {
                                None
                            };
                            let fbx_path = temp_dir
                                .as_ref()
                                .map(|d| {
                                    d.path().join(original_path.file_name().unwrap_or_default())
                                })
                                .unwrap_or_else(|| original_path.clone());
                            let ir = crate::fbx::extract::extract_ir_model_from_fbx_with_options(
                                main_bytes,
                                Some(&fbx_path),
                                self.normalize_pose,
                                self.normalize_to_tstance,
                            )?;
                            drop(temp_dir); // TempDir::drop で自動削除
                            Ok(ir)
                        }
                        FileFormat::Pmx => {
                            let pmx_model = crate::pmx::reader::read_pmx_from_data(main_bytes)?;
                            let pmx_dir = original_path.parent().unwrap_or(Path::new("."));
                            let mut ir = crate::pmx::extract::pmx_to_ir_with_aux(
                                &pmx_model,
                                pmx_dir,
                                Some(aux_files),
                            )?;
                            if self.normalize_pose {
                                ir.astance_result =
                                    crate::intermediate::pose::normalize_pose_to_tstance_full(
                                        &mut ir.bones,
                                        &mut ir.meshes,
                                        &mut ir.morphs,
                                        &mut ir.physics,
                                        crate::convert::coord::gltf_pos_to_pmx,
                                    );
                            }
                            Ok(ir)
                        }
                        FileFormat::Pmd => {
                            let pmd_model = crate::pmd::reader::read_pmd_from_data(main_bytes)?;
                            let mut ir = crate::pmd::extract::pmd_to_ir_with_aux(
                                &pmd_model,
                                original_path,
                                Some(aux_files),
                            )?;
                            if self.normalize_pose {
                                ir.astance_result =
                                    crate::intermediate::pose::normalize_pose_to_tstance_full(
                                        &mut ir.bones,
                                        &mut ir.meshes,
                                        &mut ir.morphs,
                                        &mut ir.physics,
                                        crate::convert::coord::gltf_pos_to_pmx,
                                    );
                            }
                            Ok(ir)
                        }
                        FileFormat::Obj => {
                            let obj_dir = original_path.parent().unwrap_or(Path::new("."));
                            let name = original_path
                                .file_stem()
                                .and_then(|s| s.to_str())
                                .unwrap_or("Model");
                            Ok(crate::obj::extract::load_obj_from_data(
                                main_bytes,
                                name,
                                obj_dir,
                                Some(aux_files),
                            )?)
                        }
                        FileFormat::Stl => {
                            let name = original_path
                                .file_stem()
                                .and_then(|s| s.to_str())
                                .unwrap_or("Model");
                            Ok(crate::stl::extract::load_stl_from_data(main_bytes, name)?)
                        }
                        FileFormat::DirectX => {
                            let x_dir = original_path.parent().unwrap_or(Path::new("."));
                            let name = original_path
                                .file_stem()
                                .and_then(|s| s.to_str())
                                .unwrap_or("Model");
                            Ok(crate::directx::extract::load_x_from_data(
                                main_bytes,
                                name,
                                x_dir,
                                Some(aux_files),
                            )?)
                        }
                        _ => {
                            let glb = vrm::loader::load_glb_from_data(main_bytes)?;
                            let version = vrm::detect::detect_version(&glb.document);
                            let all_extensions = vrm::loader::get_raw_extensions(&glb.document);
                            let mut ir = vrm::extract::extract_ir_model_with_options(
                                &glb.document,
                                &glb.buffers,
                                &glb.images,
                                &glb.vrm_extension,
                                &version,
                                &all_extensions,
                                self.normalize_pose,
                            )?;
                            Self::encode_ir_textures_as_png(&mut ir, &glb.images);
                            Ok(ir)
                        }
                    }
                }
                ReloadableSource::Archive {
                    original_path,
                    archive_bytes,
                    selected_entry_path,
                    inner_kind,
                } => {
                    // UnityPackage は早期に処理済み（ここに到達しない）
                    self.load_ir_from_archive_source(
                        original_path,
                        archive_bytes.as_ref(),
                        selected_entry_path,
                        *inner_kind,
                    )
                }
            }
        })();

        match ir_result {
            Ok(other_ir) => {
                if let Some(ref loaded) = self.loaded {
                    let host_fmt = loaded.ir.source_format;
                    let other_fmt = other_ir.source_format;
                    if host_fmt.is_vrm0() != other_fmt.is_vrm0() {
                        log::warn!(
                            "Appending model with different coordinate system: {} + {}",
                            host_fmt.label(),
                            other_fmt.label()
                        );
                        return;
                    }
                }
                self.finish_append_with_source(
                    other_ir,
                    source.clone(),
                    pkg_model_name.map(|s| s.to_string()),
                );
            }
            Err(e) => {
                log::error!("Additional model reload failed: {e}");
            }
        }
    }

    /// unitypackage 再読み込み（FBX/VRM再展開 + テクスチャ復元）
    fn reload_unitypackage(
        &mut self,
        source: &ReloadableSource,
        saved_pkg_textures: &Option<Vec<(String, Vec<u8>)>>,
        saved_pkg_tex_assignments: &HashMap<usize, String>,
    ) -> anyhow::Result<()> {
        use std::borrow::Cow;
        let (archive_data, snapshot): (Cow<'_, [u8]>, Option<Arc<[u8]>>) = match source {
            ReloadableSource::Snapshot { main_bytes, .. } => {
                (Cow::Borrowed(main_bytes), Some(Arc::clone(main_bytes)))
            }
            ReloadableSource::File(path) => {
                let data = std::fs::read(path)?;
                (Cow::Owned(data), None)
            }
            ReloadableSource::Archive {
                original_path,
                archive_bytes,
                ..
            } => {
                if let Some(snap) = archive_bytes {
                    (Cow::Borrowed(snap), Some(Arc::clone(snap)))
                } else {
                    let data = std::fs::read(original_path)?;
                    (Cow::Owned(data), None)
                }
            }
        };
        let path = source.display_path();

        // Prefab モデルの場合は Prefab パスで再読み込み
        if let Some(prefab_path) = self
            .loaded
            .as_ref()
            .and_then(|l| l.prefab_entry_path.clone())
        {
            return self.reload_as_prefab(
                &archive_data,
                snapshot,
                path,
                &prefab_path,
                source,
                saved_pkg_textures,
                saved_pkg_tex_assignments,
            );
        }

        let assets = crate::unitypackage::extract_all_assets(&archive_data)?;

        // 現在のモデルが VRM の場合は VRM として再読み込み
        let is_vrm = self.loaded.as_ref().is_some_and(|l| {
            !matches!(
                l.ir.source_format,
                crate::intermediate::types::SourceFormat::Fbx
            )
        });

        if is_vrm {
            let vrm_list = crate::unitypackage::find_vrm_list(&assets);
            if vrm_list.is_empty() {
                anyhow::bail!(".unitypackage 内に VRM ファイルが見つかりません");
            }
            // GUID/パスベース → basename フォールバック
            let vrm_idx = self
                .selected_pkg_model
                .as_ref()
                .and_then(|loc| crate::unitypackage::find_asset_by_pathname(&assets, &loc.pathname))
                .or_else(|| {
                    self.selected_fbx_name.as_ref().and_then(|prev_name| {
                        vrm_list
                            .iter()
                            .find(|(_, name)| name == prev_name)
                            .map(|(idx, _)| *idx)
                    })
                })
                .unwrap_or(vrm_list[0].0);
            let source_override = snapshot.map(|snap| ReloadableSource::Snapshot {
                original_path: path.to_path_buf(),
                main_bytes: snap,
                aux_files: HashMap::new(),
            });
            return self.load_vrm_from_assets(assets, vrm_idx, path, source_override);
        }

        // 初回ロードで Prefab 対応テクスチャマッピングが使われたか判定
        let use_prefab_mapping = self
            .loaded
            .as_ref()
            .is_some_and(|l| !l.pkg_material_keys.is_empty());

        let fbx_list = crate::unitypackage::find_fbx_list(&assets);
        if fbx_list.is_empty() {
            anyhow::bail!(".unitypackage 内に FBX ファイルが見つかりません");
        }

        // GUID/パスベース → basename フォールバック
        let fbx_idx = self
            .selected_pkg_model
            .as_ref()
            .and_then(|loc| crate::unitypackage::find_asset_by_pathname(&assets, &loc.pathname))
            .or_else(|| {
                self.selected_fbx_name.as_ref().and_then(|prev_name| {
                    fbx_list
                        .iter()
                        .find(|(_, name)| name == prev_name)
                        .map(|(idx, _)| *idx)
                })
            })
            .unwrap_or(fbx_list[0].0);

        if use_prefab_mapping {
            // Prefab 対応パス: UnityPackageIndex を構築し prepare_pkg_fbx で Prefab テクスチャ解決
            let pkg_index = std::sync::Arc::new(crate::unitypackage::build_unity_package_index(
                &archive_data,
            )?);
            // selected_pkg_model の GUID → pkg_index 内インデックス → パス照合 → フォールバック
            let pkg_fbx_idx = self
                .selected_pkg_model
                .as_ref()
                .and_then(|loc| pkg_index.by_guid.get(loc.guid.as_ref()).copied())
                .or_else(|| {
                    let fbx_pathname = &assets[fbx_idx].pathname;
                    pkg_index.by_path.get(fbx_pathname.as_str()).copied()
                })
                .unwrap_or(fbx_idx);

            let prepared = crate::unitypackage::prepare_pkg_fbx(&pkg_index, pkg_fbx_idx)?;
            let fbx_name = std::path::Path::new(prepared.model.pathname.as_ref())
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();
            log::info!(
                "Unitypackage reload (Prefab): {} textures: {}",
                fbx_name,
                prepared.textures.len()
            );

            // unitypackage 経由: fbx_path=None で FBX 近傍テクスチャ検索を無効化
            let mut ir = crate::fbx::extract::extract_ir_model_from_fbx_with_options(
                &prepared.fbx_data,
                None,
                self.normalize_pose,
                self.normalize_to_tstance,
            )?;

            // Prefab 対応テクスチャ埋め込み
            let prefab_label = format!(
                "prefab({})",
                std::path::Path::new(&*prepared.model.pathname)
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
            );
            crate::unitypackage::embed_textures_with_prefab(
                &mut ir,
                &prepared.textures,
                &prepared.resolved,
                &prefab_label,
            );

            // pkg_textures を legacy 形式で保持
            let legacy_textures: Vec<(String, Vec<u8>)> = prepared
                .textures
                .iter()
                .map(|t| (t.display_name.to_string(), t.data.to_vec()))
                .collect();
            if !legacy_textures.is_empty() {
                self.tex.pkg_textures = Some(legacy_textures);
                self.rebuild_pkg_thumb_cache();
            }

            // 手動割当の復元（GPU構築前にIrModelに適用）
            if !saved_pkg_tex_assignments.is_empty() {
                let pkg_src = self.tex.pkg_textures.as_deref().unwrap_or(&[]);
                let name_to_data: HashMap<&str, &[u8]> = pkg_src
                    .iter()
                    .map(|(name, data)| (name.as_str(), data.as_slice()))
                    .collect();
                let mut name_to_ir: HashMap<String, usize> = HashMap::new();
                for (mat_idx, tex_name) in saved_pkg_tex_assignments {
                    if *mat_idx >= ir.materials.len() {
                        continue;
                    }
                    let ir_idx = if let Some(&cached) = name_to_ir.get(tex_name) {
                        cached
                    } else if let Some(data) = name_to_data.get(tex_name.as_str()) {
                        let is_psd = super::super::texture::is_psd_filename(tex_name);
                        let basename = std::path::Path::new(tex_name)
                            .file_stem()
                            .unwrap_or_default()
                            .to_string_lossy()
                            .to_string();
                        let (ir_data, ir_filename, ir_mime) = if is_psd {
                            match Self::psd_to_png(data) {
                                Ok(png_data) => (
                                    png_data,
                                    format!("{}.png", basename),
                                    "image/png".to_string(),
                                ),
                                Err(e) => {
                                    log::warn!("PSD->PNG conversion failed (pkg restore): {e}");
                                    continue;
                                }
                            }
                        } else {
                            let ext = std::path::Path::new(tex_name)
                                .extension()
                                .and_then(|e| e.to_str())
                                .unwrap_or("")
                                .to_lowercase();
                            let mime = crate::intermediate::types::mime_for_ext(&ext).to_string();
                            (data.to_vec(), tex_name.clone(), mime)
                        };
                        let idx = ir.textures.len();
                        ir.textures.push(crate::intermediate::types::IrTexture {
                            filename: ir_filename,
                            data: TextureData::Encoded(ir_data),
                            mime_type: ir_mime,
                            source_path: format!("unitypackage: {}", tex_name),
                            mip_chain: None,
                        });
                        name_to_ir.insert(tex_name.clone(), idx);
                        idx
                    } else {
                        continue;
                    };
                    ir.materials[*mat_idx].texture_index = Some(ir_idx);
                    ir.materials[*mat_idx].apply_textured_defaults();
                    log::info!(
                        "Texture restored: mat[{}] '{}' <- '{}'",
                        mat_idx,
                        ir.materials[*mat_idx].name,
                        tex_name
                    );
                }
            }

            // pkg_material_keys を再構築
            let pkg_keys: Vec<Option<crate::unitypackage::PkgMaterialKey>> = {
                let fbx_guid: std::sync::Arc<str> = prepared.model.guid.as_ref().into();
                let instance_id = crate::unitypackage::BASE_INSTANCE_ID;
                ir.materials
                    .iter()
                    .map(|mat| {
                        Some(crate::unitypackage::PkgMaterialKey {
                            instance_id,
                            model_guid: fbx_guid.clone(),
                            source_material: mat.source_material.clone(),
                            material_name: mat.name.as_str().into(),
                        })
                    })
                    .collect()
            };

            let reload_source = match snapshot {
                Some(snap) => ReloadableSource::Snapshot {
                    original_path: path.to_path_buf(),
                    main_bytes: snap,
                    aux_files: HashMap::new(),
                },
                None => ReloadableSource::File(path.to_path_buf()),
            };
            let result = self.finish_load(ir, reload_source);
            // finish_load がクリアするので、その後に復元
            self.tex.pkg_assignments = saved_pkg_tex_assignments.clone();
            if let Some(ref mut loaded) = self.loaded {
                loaded.pkg_material_keys = pkg_keys;
            }
            return result;
        }

        // 通常パス: 単純名前マッチング
        let (fbx_data, fbx_name, textures) =
            crate::unitypackage::take_fbx_and_textures(assets, fbx_idx)?;
        log::info!(
            "Unitypackage reload: {} textures: {}",
            fbx_name,
            textures.len()
        );

        // unitypackage 経由: fbx_path=None で FBX 近傍テクスチャ検索を無効化
        let mut ir = crate::fbx::extract::extract_ir_model_from_fbx_with_options(
            &fbx_data,
            None,
            self.normalize_pose,
            self.normalize_to_tstance,
        )?;

        // テクスチャ埋め込み
        let tex_source = if !textures.is_empty() {
            &textures
        } else if let Some(ref pkg) = saved_pkg_textures {
            pkg.as_slice()
        } else {
            &[]
        };
        crate::unitypackage::embed_textures_into_ir(&mut ir, tex_source);

        // 手動割当の復元（GPU構築前にIrModelに適用）
        let pkg_src = if !textures.is_empty() {
            &textures
        } else {
            saved_pkg_textures.as_deref().unwrap_or(&[])
        };
        if !saved_pkg_tex_assignments.is_empty() && !pkg_src.is_empty() {
            let name_to_data: HashMap<&str, &[u8]> = pkg_src
                .iter()
                .map(|(name, data)| (name.as_str(), data.as_slice()))
                .collect();
            let mut name_to_ir: HashMap<String, usize> = HashMap::new();
            for (mat_idx, tex_name) in saved_pkg_tex_assignments {
                if *mat_idx >= ir.materials.len() {
                    continue;
                }
                let ir_idx = if let Some(&cached) = name_to_ir.get(tex_name) {
                    cached
                } else if let Some(data) = name_to_data.get(tex_name.as_str()) {
                    let is_psd = super::super::texture::is_psd_filename(tex_name);
                    let basename = std::path::Path::new(tex_name)
                        .file_stem()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .to_string();
                    let (ir_data, ir_filename, ir_mime) = if is_psd {
                        match Self::psd_to_png(data) {
                            Ok(png_data) => (
                                png_data,
                                format!("{}.png", basename),
                                "image/png".to_string(),
                            ),
                            Err(e) => {
                                log::warn!("PSD->PNG conversion failed (pkg restore): {e}");
                                continue;
                            }
                        }
                    } else {
                        let ext = std::path::Path::new(tex_name)
                            .extension()
                            .and_then(|e| e.to_str())
                            .unwrap_or("")
                            .to_lowercase();
                        let mime = crate::intermediate::types::mime_for_ext(&ext).to_string();
                        (data.to_vec(), tex_name.clone(), mime)
                    };
                    let idx = ir.textures.len();
                    ir.textures.push(crate::intermediate::types::IrTexture {
                        filename: ir_filename,
                        data: TextureData::Encoded(ir_data),
                        mime_type: ir_mime,
                        source_path: format!("unitypackage: {}", tex_name),
                        mip_chain: None,
                    });
                    name_to_ir.insert(tex_name.clone(), idx);
                    idx
                } else {
                    continue;
                };
                ir.materials[*mat_idx].texture_index = Some(ir_idx);
                ir.materials[*mat_idx].apply_textured_defaults();
                log::info!(
                    "Texture restored: mat[{}] '{}' <- '{}'",
                    mat_idx,
                    ir.materials[*mat_idx].name,
                    tex_name
                );
            }
        }

        if !textures.is_empty() {
            self.tex.pkg_textures = Some(textures);
            self.rebuild_pkg_thumb_cache();
        }

        let reload_source = match snapshot {
            Some(snap) => ReloadableSource::Snapshot {
                original_path: path.to_path_buf(),
                main_bytes: snap,
                aux_files: HashMap::new(),
            },
            None => ReloadableSource::File(path.to_path_buf()),
        };
        let result = self.finish_load(ir, reload_source);
        // finish_load がクリアするので、その後に復元
        self.tex.pkg_assignments = saved_pkg_tex_assignments.clone();
        result
    }

    /// Prefab モデルのリロード（pkg_index を再構築して load_prefab_from_assets を呼び直す）
    fn reload_as_prefab(
        &mut self,
        archive_data: &[u8],
        snapshot: Option<Arc<[u8]>>,
        path: &Path,
        prefab_entry_path: &str,
        archive_source: &ReloadableSource,
        saved_pkg_textures: &Option<Vec<(String, Vec<u8>)>>,
        saved_pkg_tex_assignments: &HashMap<usize, String>,
    ) -> anyhow::Result<()> {
        let pkg_index = Arc::new(crate::unitypackage::build_unity_package_index(
            archive_data,
        )?);
        let prefab_index = pkg_index
            .by_path
            .get(prefab_entry_path)
            .copied()
            .ok_or_else(|| {
                anyhow::anyhow!("Prefab エントリが見つかりません: {}", prefab_entry_path)
            })?;

        // リロード後も Archive ソースを維持（snapshot があれば Snapshot、なければ元の Archive を引き継ぐ）
        let source_override: Option<ReloadableSource> = if let Some(snap) = snapshot {
            Some(ReloadableSource::Snapshot {
                original_path: path.to_path_buf(),
                main_bytes: snap,
                aux_files: HashMap::new(),
            })
        } else {
            Some(archive_source.clone())
        };

        self.load_prefab_from_assets(
            Vec::new(),
            prefab_index,
            path,
            source_override,
            Some(pkg_index),
        )?;

        // pkg テクスチャが load_prefab_from_assets 内で設定されなかった場合に復元
        if self.tex.pkg_textures.is_none() {
            if let Some(ref saved) = saved_pkg_textures {
                self.tex.pkg_textures = Some(saved.clone());
                self.rebuild_pkg_thumb_cache();
            }
        }

        // 手動テクスチャ割当を GPU モデル構築後に復元
        // （借用チェッカー対策: データを先に収集してから適用）
        if !saved_pkg_tex_assignments.is_empty() {
            let assignments_to_restore: Vec<(usize, String, Vec<u8>)> = {
                let pkg_src = self.tex.pkg_textures.as_deref().unwrap_or(&[]);
                let name_to_data: HashMap<&str, &[u8]> = pkg_src
                    .iter()
                    .map(|(name, data)| (name.as_str(), data.as_slice()))
                    .collect();
                saved_pkg_tex_assignments
                    .iter()
                    .filter_map(|(idx, tex_name)| {
                        name_to_data
                            .get(tex_name.as_str())
                            .map(|data| (*idx, tex_name.clone(), data.to_vec()))
                    })
                    .collect()
            };
            for (mat_idx, tex_name, data) in &assignments_to_restore {
                if self.assign_texture_data_to_material(*mat_idx, tex_name, data) {
                    self.tex.pkg_assignments.insert(*mat_idx, tex_name.clone());
                }
            }
        }

        Ok(())
    }

    /// アーカイブ(ZIP/7z)内 .unitypackage のリロード
    fn reload_archive_unitypackage(
        &mut self,
        original_path: &Path,
        archive_bytes: Option<&Arc<[u8]>>,
        selected_entry_path: &str,
        archive_source: &ReloadableSource,
        saved_pkg_textures: &Option<Vec<(String, Vec<u8>)>>,
        saved_pkg_tex_assignments: &HashMap<usize, String>,
    ) -> anyhow::Result<()> {
        let owned;
        let data: &[u8] = if let Some(snap) = archive_bytes {
            snap
        } else {
            owned = std::fs::read(original_path)?;
            &owned
        };

        let ext = original_path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();
        let format = crate::archive::archive_format_from_ext(&ext)
            .ok_or_else(|| anyhow::anyhow!("未対応のアーカイブ形式: {ext}"))?;

        let contents = crate::archive::list_models(data, format)?;

        let model_index = contents
            .models
            .iter()
            .position(|(_, p, _, _)| p.to_string_lossy() == selected_entry_path)
            .ok_or_else(|| {
                anyhow::anyhow!("アーカイブ内に以前のモデルが見つかりません: {selected_entry_path}")
            })?;

        let bundle = crate::archive::extract_model_bundle(data, format, contents, model_index)?;

        let pkg_data = bundle.model.data;

        // Prefab モデルの場合は Prefab パスで再読み込み
        if let Some(prefab_path) = self
            .loaded
            .as_ref()
            .and_then(|l| l.prefab_entry_path.clone())
        {
            let snapshot_arc: Option<Arc<[u8]>> = archive_bytes.cloned();
            return self.reload_as_prefab(
                &pkg_data,
                snapshot_arc,
                original_path,
                &prefab_path,
                archive_source,
                saved_pkg_textures,
                saved_pkg_tex_assignments,
            );
        }

        let assets = crate::unitypackage::extract_all_assets(&pkg_data)?;

        let is_vrm = self.loaded.as_ref().is_some_and(|l| {
            !matches!(
                l.ir.source_format,
                crate::intermediate::types::SourceFormat::Fbx
            )
        });

        if is_vrm {
            let vrm_list = crate::unitypackage::find_vrm_list(&assets);
            if vrm_list.is_empty() {
                anyhow::bail!(".unitypackage 内に VRM ファイルが見つかりません");
            }
            // GUID/パスベース → basename フォールバック
            let vrm_idx = self
                .selected_pkg_model
                .as_ref()
                .and_then(|loc| crate::unitypackage::find_asset_by_pathname(&assets, &loc.pathname))
                .or_else(|| {
                    self.selected_fbx_name.as_ref().and_then(|prev_name| {
                        vrm_list
                            .iter()
                            .find(|(_, name)| name == prev_name)
                            .map(|(idx, _)| *idx)
                    })
                })
                .unwrap_or(vrm_list[0].0);
            return self.load_vrm_from_assets(
                assets,
                vrm_idx,
                original_path,
                Some(archive_source.clone()),
            );
        }

        let fbx_list = crate::unitypackage::find_fbx_list(&assets);
        if fbx_list.is_empty() {
            anyhow::bail!(".unitypackage 内に FBX ファイルが見つかりません");
        }

        // GUID/パスベース → basename フォールバック
        let fbx_idx = self
            .selected_pkg_model
            .as_ref()
            .and_then(|loc| crate::unitypackage::find_asset_by_pathname(&assets, &loc.pathname))
            .or_else(|| {
                self.selected_fbx_name.as_ref().and_then(|prev_name| {
                    fbx_list
                        .iter()
                        .find(|(_, name)| name == prev_name)
                        .map(|(idx, _)| *idx)
                })
            })
            .unwrap_or(fbx_list[0].0);

        let (fbx_data, fbx_name, textures) =
            crate::unitypackage::take_fbx_and_textures(assets, fbx_idx)?;
        log::info!(
            "Archive unitypackage reload: {} textures: {}",
            fbx_name,
            textures.len()
        );

        // unitypackage 経由: fbx_path=None で FBX 近傍テクスチャ検索を無効化
        let mut ir = crate::fbx::extract::extract_ir_model_from_fbx_with_options(
            &fbx_data,
            None,
            self.normalize_pose,
            self.normalize_to_tstance,
        )?;

        let tex_source = if !textures.is_empty() {
            &textures
        } else if let Some(ref pkg) = saved_pkg_textures {
            pkg.as_slice()
        } else {
            &[]
        };
        crate::unitypackage::embed_textures_into_ir(&mut ir, tex_source);

        let pkg_src = if !textures.is_empty() {
            &textures
        } else {
            saved_pkg_textures.as_deref().unwrap_or(&[])
        };
        if !saved_pkg_tex_assignments.is_empty() && !pkg_src.is_empty() {
            let name_to_data: HashMap<&str, &[u8]> = pkg_src
                .iter()
                .map(|(name, data)| (name.as_str(), data.as_slice()))
                .collect();
            let mut name_to_ir: HashMap<String, usize> = HashMap::new();
            for (mat_idx, tex_name) in saved_pkg_tex_assignments {
                if *mat_idx >= ir.materials.len() {
                    continue;
                }
                let ir_idx = if let Some(&cached) = name_to_ir.get(tex_name) {
                    cached
                } else if let Some(data) = name_to_data.get(tex_name.as_str()) {
                    let is_psd = super::super::texture::is_psd_filename(tex_name);
                    let basename = std::path::Path::new(tex_name)
                        .file_stem()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .to_string();
                    let (ir_data, ir_filename, ir_mime) = if is_psd {
                        match Self::psd_to_png(data) {
                            Ok(png_data) => (
                                png_data,
                                format!("{}.png", basename),
                                "image/png".to_string(),
                            ),
                            Err(e) => {
                                log::warn!("PSD->PNG conversion failed (pkg restore): {e}");
                                continue;
                            }
                        }
                    } else {
                        let ext = std::path::Path::new(tex_name)
                            .extension()
                            .and_then(|e| e.to_str())
                            .unwrap_or("")
                            .to_lowercase();
                        let mime = crate::intermediate::types::mime_for_ext(&ext).to_string();
                        (data.to_vec(), tex_name.clone(), mime)
                    };
                    let idx = ir.textures.len();
                    ir.textures.push(crate::intermediate::types::IrTexture {
                        filename: ir_filename,
                        data: TextureData::Encoded(ir_data),
                        mime_type: ir_mime,
                        source_path: format!("unitypackage: {}", tex_name),
                        mip_chain: None,
                    });
                    name_to_ir.insert(tex_name.clone(), idx);
                    idx
                } else {
                    continue;
                };
                ir.materials[*mat_idx].texture_index = Some(ir_idx);
                ir.materials[*mat_idx].apply_textured_defaults();
                log::info!(
                    "Texture restored: mat[{}] '{}' <- '{}'",
                    mat_idx,
                    ir.materials[*mat_idx].name,
                    tex_name
                );
            }
        }

        if !textures.is_empty() {
            self.tex.pkg_textures = Some(textures);
            self.rebuild_pkg_thumb_cache();
        }

        let result = self.finish_load(ir, archive_source.clone());
        self.tex.pkg_assignments = saved_pkg_tex_assignments.clone();
        result
    }

    pub(super) fn open_file_dialog(&mut self, ctx: &egui::Context) {
        // ダイアログが既にオープン中なら無視
        if self.pending.file_dialog.is_some() {
            return;
        }
        let initial_dir = self.last_model_dir.clone();
        let (tx, rx) = std::sync::mpsc::channel();
        let repaint = ctx.clone();
        std::thread::spawn(move || {
            let mut dialog = rfd::FileDialog::new()
                .set_title("3Dモデル / VRMAアニメーションを開く")
                .add_filter(
                    "対応形式",
                    &[
                        "vrm",
                        "fbx",
                        "pmx",
                        "pmd",
                        "obj",
                        "stl",
                        "x",
                        "unitypackage",
                        "vrma",
                        "zip",
                        "7z",
                    ],
                )
                .add_filter("VRM (.vrm)", &["vrm"])
                .add_filter("FBX (.fbx)", &["fbx"])
                .add_filter("PMX (.pmx)", &["pmx"])
                .add_filter("PMD (.pmd)", &["pmd"])
                .add_filter("OBJ (.obj)", &["obj"])
                .add_filter("STL (.stl)", &["stl"])
                .add_filter("DirectX text (.x)", &["x"])
                .add_filter("UnityPackage (.unitypackage)", &["unitypackage"])
                .add_filter("アーカイブ (.zip, .7z)", &["zip", "7z"])
                .add_filter("VRMA (.vrma)", &["vrma"]);
            if let Some(ref dir) = initial_dir {
                dialog = dialog.set_directory(dir);
            }
            let _ = tx.send(dialog.pick_file());
            repaint.request_repaint();
        });
        self.pending.file_dialog = Some((super::pending::FileDialogKind::Open, rx));
    }

    /// モデル追加読み込みダイアログ
    pub(super) fn open_append_dialog(&mut self, ctx: &egui::Context) {
        // ダイアログが既にオープン中なら無視
        if self.pending.file_dialog.is_some() {
            return;
        }
        let initial_dir = self.last_model_dir.clone();
        let (tx, rx) = std::sync::mpsc::channel();
        let repaint = ctx.clone();
        std::thread::spawn(move || {
            let mut dialog = rfd::FileDialog::new()
                .set_title("モデルを追加読み込み")
                .add_filter(
                    "3Dモデル",
                    &[
                        "vrm",
                        "fbx",
                        "pmx",
                        "pmd",
                        "obj",
                        "stl",
                        "x",
                        "unitypackage",
                        "zip",
                        "7z",
                    ],
                )
                .add_filter("VRM (.vrm)", &["vrm"])
                .add_filter("FBX (.fbx)", &["fbx"])
                .add_filter("PMX (.pmx)", &["pmx"])
                .add_filter("PMD (.pmd)", &["pmd"])
                .add_filter("UnityPackage (.unitypackage)", &["unitypackage"])
                .add_filter("アーカイブ (.zip, .7z)", &["zip", "7z"]);
            if let Some(ref dir) = initial_dir {
                dialog = dialog.set_directory(dir);
            }
            let _ = tx.send(dialog.pick_file());
            repaint.request_repaint();
        });
        self.pending.file_dialog = Some((super::pending::FileDialogKind::Append, rx));
    }

    /// モデルを既存モデルに追加（マージ）
    pub(super) fn append_model(&mut self, path: PathBuf) {
        log::info!("Append file: {}", path.display());
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();

        if ext == "unitypackage" {
            match self.try_load_unitypackage_for_append(&path) {
                Ok(()) => {}
                Err(e) => {
                    log::error!("Append load failed (pkg): {e}");
                    self.convert_message = Some(ConvertMessage::failure(format!(
                        "追加読み込みに失敗しました。\n詳細: {e}"
                    )));
                }
            }
            return;
        }
        if matches!(ext.as_str(), "zip" | "7z") {
            match self.try_load_archive_for_append(&path) {
                Ok(()) => {}
                Err(e) => {
                    log::error!("Append load failed (archive): {e}");
                    self.convert_message = Some(ConvertMessage::failure(format!(
                        "追加読み込みに失敗しました。\n詳細: {e}"
                    )));
                }
            }
            return;
        }

        let ir_result: anyhow::Result<IrModel> = (|| -> anyhow::Result<IrModel> {
            match ext.as_str() {
                "fbx" => {
                    let data = self.read_or_preloaded(&path)?;
                    Ok(crate::fbx::extract::extract_ir_model_from_fbx_with_options(
                        &data,
                        Some(&path),
                        self.normalize_pose,
                        self.normalize_to_tstance,
                    )?)
                }
                "pmx" => {
                    let pmx_model = if self.preloaded.as_ref().is_some_and(|pl| pl.path == path) {
                        let data = self.read_or_preloaded(&path)?;
                        crate::pmx::reader::read_pmx_from_data(&data)?
                    } else {
                        crate::pmx::reader::read_pmx(&path)?
                    };
                    let pmx_dir = path.parent().unwrap_or(std::path::Path::new("."));
                    let mut ir = crate::pmx::extract::pmx_to_ir(&pmx_model, pmx_dir)?;
                    if self.normalize_pose {
                        ir.astance_result =
                            crate::intermediate::pose::normalize_pose_to_tstance_full(
                                &mut ir.bones,
                                &mut ir.meshes,
                                &mut ir.morphs,
                                &mut ir.physics,
                                crate::convert::coord::gltf_pos_to_pmx,
                            );
                    }
                    Ok(ir)
                }
                "pmd" => {
                    let pmd_model = if self.preloaded.as_ref().is_some_and(|pl| pl.path == path) {
                        let data = self.read_or_preloaded(&path)?;
                        crate::pmd::reader::read_pmd_from_data(&data)?
                    } else {
                        crate::pmd::reader::read_pmd(&path)?
                    };
                    let mut ir = crate::pmd::extract::pmd_to_ir(&pmd_model, &path)?;
                    if self.normalize_pose {
                        ir.astance_result =
                            crate::intermediate::pose::normalize_pose_to_tstance_full(
                                &mut ir.bones,
                                &mut ir.meshes,
                                &mut ir.morphs,
                                &mut ir.physics,
                                crate::convert::coord::gltf_pos_to_pmx,
                            );
                    }
                    Ok(ir)
                }
                "obj" => {
                    if is_temp_path(&path)
                        || self.preloaded.as_ref().is_some_and(|pl| pl.path == path)
                    {
                        let data = self.read_or_preloaded(&path)?;
                        let obj_dir = path.parent().unwrap_or(Path::new("."));
                        let aux = self.take_or_collect_aux(&path);
                        let name = path.file_stem().and_then(|s| s.to_str()).unwrap_or("Model");
                        Ok(crate::obj::extract::load_obj_from_data(
                            &data,
                            name,
                            obj_dir,
                            Some(&aux),
                        )?)
                    } else {
                        Ok(crate::obj::extract::load_obj(&path)?)
                    }
                }
                "stl" => {
                    if is_temp_path(&path)
                        || self.preloaded.as_ref().is_some_and(|pl| pl.path == path)
                    {
                        let data = self.read_or_preloaded(&path)?;
                        let name = path.file_stem().and_then(|s| s.to_str()).unwrap_or("Model");
                        Ok(crate::stl::extract::load_stl_from_data(&data, name)?)
                    } else {
                        Ok(crate::stl::extract::load_stl(&path)?)
                    }
                }
                "x" => {
                    if is_temp_path(&path)
                        || self.preloaded.as_ref().is_some_and(|pl| pl.path == path)
                    {
                        let data = self.read_or_preloaded(&path)?;
                        let x_dir = path.parent().unwrap_or(Path::new("."));
                        let aux = self.take_or_collect_aux(&path);
                        let name = path.file_stem().and_then(|s| s.to_str()).unwrap_or("Model");
                        Ok(crate::directx::extract::load_x_from_data(
                            &data,
                            name,
                            x_dir,
                            Some(&aux),
                        )?)
                    } else {
                        Ok(crate::directx::extract::load_x(&path)?)
                    }
                }
                _ => self.load_vrm_as_ir(&path),
            }
        })();

        match ir_result {
            Ok(other_ir) => {
                if let Some(ref loaded) = self.loaded {
                    let host_fmt = loaded.ir.source_format;
                    let other_fmt = other_ir.source_format;
                    if host_fmt.is_vrm0() != other_fmt.is_vrm0() {
                        log::warn!(
                            "Appending model with different coordinate system: {} + {}",
                            host_fmt.label(),
                            other_fmt.label()
                        );
                        self.convert_message = Some(ConvertMessage::failure(format!(
                            "座標系が異なるモデルは追加できません。\nホスト: {}, 追加: {}",
                            host_fmt.label(),
                            other_fmt.label()
                        )));
                        return;
                    }
                }
                let source = if is_temp_path(&path) {
                    let main_data = match std::fs::read(&path) {
                        Ok(d) => d,
                        Err(_) => {
                            log::warn!("Temp file reload failed: {}", path.display());
                            self.finish_append_with_source(
                                other_ir,
                                ReloadableSource::File(path.clone()),
                                None,
                            );
                            return;
                        }
                    };
                    let mut aux = HashMap::new();
                    if ext == "fbx" {
                        if let Some(dir) = path.parent() {
                            collect_image_files_recursive(dir, dir, &mut aux);
                        }
                    } else if ext == "pmx" {
                        if let Ok(pmx_model) = crate::pmx::reader::read_pmx_from_data(&main_data) {
                            let pmx_dir = path.parent().unwrap_or(Path::new("."));
                            for tex_path in &pmx_model.textures {
                                let normalized = tex_path.replace('\\', "/");
                                let full = pmx_dir.join(&normalized);
                                if let Ok(data) = std::fs::read(&full) {
                                    aux.insert(
                                        PathBuf::from(&normalized),
                                        Arc::from(data.into_boxed_slice()),
                                    );
                                }
                            }
                        }
                    } else if ext == "pmd" {
                        let pmd_dir = path.parent().unwrap_or(Path::new("."));
                        if let Ok(pmd_model) = crate::pmd::reader::read_pmd_from_data(&main_data) {
                            for mat in &pmd_model.materials {
                                if mat.texture_name.is_empty() {
                                    continue;
                                }
                                let main_tex = mat.texture_name.split('*').next().unwrap_or("");
                                if main_tex.is_empty() {
                                    continue;
                                }
                                let normalized = main_tex.replace('\\', "/");
                                let key = PathBuf::from(&normalized);
                                if !aux.contains_key(&key) {
                                    if let Ok(data) = std::fs::read(pmd_dir.join(&normalized)) {
                                        aux.insert(key, Arc::from(data.into_boxed_slice()));
                                    }
                                }
                            }
                        }
                        let txt_path = path.with_extension("txt");
                        if let Ok(data) = std::fs::read(&txt_path) {
                            let txt_name = txt_path
                                .file_name()
                                .map(|f| PathBuf::from(f))
                                .unwrap_or_default();
                            aux.insert(txt_name, Arc::from(data.into_boxed_slice()));
                        }
                    } else if ext == "obj" || ext == "x" {
                        // OBJ/DirectX: 同ディレクトリの画像 + MTL を収集
                        if let Some(dir) = path.parent() {
                            collect_image_files_recursive(dir, dir, &mut aux);
                        }
                    }
                    // STL: aux 不要（テクスチャ・MTL なし）
                    ReloadableSource::Snapshot {
                        original_path: path.clone(),
                        main_bytes: main_data.into(),
                        aux_files: aux,
                    }
                } else {
                    ReloadableSource::File(path.clone())
                };
                self.finish_append_with_source(other_ir, source, None);
            }
            Err(e) => {
                log::error!("Append load failed: {e}");
                self.convert_message = Some(ConvertMessage::failure(format!(
                    "追加読み込みに失敗しました。\n詳細: {e}"
                )));
            }
        }
    }

    /// VRMファイルを IrModel として読み込む（追加用・GPU構築なし）
    fn load_vrm_as_ir(&mut self, path: &std::path::Path) -> anyhow::Result<IrModel> {
        let glb = if self.preloaded.as_ref().is_some_and(|pl| pl.path == path) {
            let data = self.read_or_preloaded(path)?;
            vrm::loader::load_glb_from_data(&data)?
        } else {
            vrm::loader::load_glb(path)?
        };
        let version = vrm::detect::detect_version(&glb.document);
        let all_extensions = vrm::loader::get_raw_extensions(&glb.document);

        let mut ir = vrm::extract::extract_ir_model_with_options(
            &glb.document,
            &glb.buffers,
            &glb.images,
            &glb.vrm_extension,
            &version,
            &all_extensions,
            self.normalize_pose,
        )?;

        Self::encode_ir_textures_as_png(&mut ir, &glb.images);
        Ok(ir)
    }

    /// unitypackage 内のモデルを既存モデルに追加（アペンド）
    pub(super) fn append_from_pkg(
        &mut self,
        assets: Vec<crate::unitypackage::ExtractedAsset>,
        model_index: usize,
        model_type: PkgModelType,
        source_path: &std::path::Path,
        source_override: Option<ReloadableSource>,
    ) {
        let normalize = self.normalize_pose;
        let normalize_tstance = self.normalize_to_tstance;
        let mut pkg_unmatched: Vec<usize> = Vec::new();
        let mut pkg_model_name: Option<String> = None;
        let mut pkg_textures_to_add: Vec<(String, Vec<u8>)> = Vec::new();
        // PkgModelLocator 構築用: assets 消費前に pathname を取得
        let asset_pathname: Option<String> = assets.get(model_index).map(|a| a.pathname.clone());
        let ir_result: anyhow::Result<IrModel> = (|| -> anyhow::Result<IrModel> {
            match model_type {
                PkgModelType::Fbx => {
                    let (fbx_data, fbx_name, textures) =
                        crate::unitypackage::take_fbx_and_textures(assets, model_index)?;
                    log::info!(
                        "Append (FBX in pkg): {} textures: {}",
                        fbx_name,
                        textures.len()
                    );
                    pkg_model_name = Some(fbx_name.clone());
                    // unitypackage 経由: fbx_path=None で FBX 近傍テクスチャ検索を無効化
                    let mut ir = crate::fbx::extract::extract_ir_model_from_fbx_with_options(
                        &fbx_data,
                        None,
                        normalize,
                        normalize_tstance,
                    )?;
                    let unmatched = crate::unitypackage::embed_textures_into_ir(&mut ir, &textures);
                    log::info!(
                        "Append (pkg): {} materials matched, unassigned: {}",
                        ir.materials.len() - unmatched.len(),
                        unmatched.len()
                    );
                    pkg_unmatched = unmatched;
                    pkg_textures_to_add = textures;
                    Ok(ir)
                }
                PkgModelType::Vrm => {
                    let (vrm_data, vrm_name) = crate::unitypackage::take_vrm(assets, model_index)?;
                    log::info!("Append (VRM in pkg): {}", vrm_name);
                    pkg_model_name = Some(vrm_name.clone());
                    let glb = vrm::loader::load_glb_from_data(&vrm_data)?;
                    let version = vrm::detect::detect_version(&glb.document);
                    let all_extensions = vrm::loader::get_raw_extensions(&glb.document);
                    let mut ir = vrm::extract::extract_ir_model_with_options(
                        &glb.document,
                        &glb.buffers,
                        &glb.images,
                        &glb.vrm_extension,
                        &version,
                        &all_extensions,
                        normalize,
                    )?;
                    Self::encode_ir_textures_as_png(&mut ir, &glb.images);
                    Ok(ir)
                }
                PkgModelType::Prefab => {
                    // Prefab のアペンドは非対応（通常ロードのみ）
                    anyhow::bail!("Prefab のアペンドモードは未対応です");
                }
            }
        })();

        match ir_result {
            Ok(other_ir) => {
                let mat_offset = self
                    .loaded
                    .as_ref()
                    .map(|l| l.ir.materials.len())
                    .unwrap_or(0);
                let appended_before = self
                    .loaded
                    .as_ref()
                    .map(|l| l.appended_models.len())
                    .unwrap_or(0);
                let tex_count_before = self
                    .loaded
                    .as_ref()
                    .map(|l| l.ir.textures.len())
                    .unwrap_or(0);
                // 安定キー構築（pathname ベース — GUID は ExtractedAsset 経路では利用不可）
                let pkg_locator = asset_pathname.map(|path| {
                    crate::unitypackage::PkgModelLocator {
                        guid: "".into(), // ExtractedAsset 経由では GUID なし
                        pathname: path.into(),
                        kind: model_type,
                    }
                });
                match source_override {
                    Some(source) => {
                        self.finish_append_ext(other_ir, source, false, pkg_model_name, pkg_locator)
                    }
                    None => self.finish_append_ext(
                        other_ir,
                        ReloadableSource::File(source_path.to_path_buf()),
                        false,
                        pkg_model_name,
                        pkg_locator,
                    ),
                }
                let appended_after = self
                    .loaded
                    .as_ref()
                    .map(|l| l.appended_models.len())
                    .unwrap_or(0);
                if appended_after > appended_before {
                    let pkg_stem = source_path
                        .file_stem()
                        .and_then(|s| s.to_str())
                        .unwrap_or("pkg");
                    let pkg_prefix = format!("{}_pkg{}", pkg_stem, appended_after);

                    if let Some(ref mut loaded) = self.loaded {
                        for tex in loaded.ir.textures[tex_count_before..].iter_mut() {
                            tex.filename = format!("{}_{}", pkg_prefix, tex.filename);
                        }
                    }

                    if !pkg_textures_to_add.is_empty() {
                        for (name, _) in &mut pkg_textures_to_add {
                            *name = format!("{}_{}", pkg_prefix, name);
                        }
                        if let Some(ref mut existing) = self.tex.pkg_textures {
                            existing.extend(pkg_textures_to_add);
                        } else {
                            self.tex.pkg_textures = Some(pkg_textures_to_add);
                        }
                        self.rebuild_pkg_thumb_cache();
                    }
                }
                if !pkg_unmatched.is_empty()
                    && self.tex.pkg_textures.is_some()
                    && !self.suppress_tex_match
                {
                    // 既存プレビューがあれば先に復元
                    self.cancel_tex_match_preview();
                    let global_unmatched: Vec<usize> =
                        pkg_unmatched.iter().map(|&i| i + mat_offset).collect();
                    let count = global_unmatched.len();
                    self.tex.pending_match = Some(PendingTexMatch {
                        mat_indices: global_unmatched,
                        selections: vec![None; count],
                        tex_filter: String::new(),
                        previewed: vec![None; count],
                        saved_binds: std::collections::HashMap::new(),
                        texture_views: Vec::new(),
                        failed_uploads: std::collections::HashSet::new(),
                    });
                }
            }
            Err(e) => {
                log::error!("Append load failed (pkg): {e}");
                self.convert_message = Some(ConvertMessage::failure(format!(
                    "追加読み込みに失敗しました。\n詳細: {e}"
                )));
            }
        }
    }

    #[expect(dead_code)]
    fn finish_append(&mut self, other_ir: IrModel, path: &Path) {
        self.finish_append_ext(
            other_ir,
            ReloadableSource::File(path.to_path_buf()),
            false,
            None,
            None,
        );
    }

    pub(super) fn finish_append_with_source(
        &mut self,
        other_ir: IrModel,
        source: ReloadableSource,
        pkg_model_name: Option<String>,
    ) {
        self.finish_append_ext(other_ir, source, false, pkg_model_name, None);
    }

    fn finish_append_ext(
        &mut self,
        mut other_ir: IrModel,
        source: ReloadableSource,
        silent: bool,
        pkg_model_name: Option<String>,
        #[allow(unused_variables)] pkg_locator: Option<crate::unitypackage::PkgModelLocator>,
    ) {
        let Some(ref mut loaded) = self.loaded else {
            return;
        };

        let added_name = other_ir.name.clone();
        let added_bones = other_ir.bones.len();
        let added_meshes = other_ir.meshes.len();
        let added_materials = other_ir.materials.len();

        let saved_bone_count = loaded.ir.bones.len();
        let saved_mesh_count = loaded.ir.meshes.len();
        let saved_material_count = loaded.ir.materials.len();
        let saved_texture_count = loaded.ir.textures.len();
        let saved_morph_count = loaded.ir.morphs.len();
        let saved_rigid_count = loaded.ir.physics.rigid_bodies.len();
        let saved_joint_count = loaded.ir.physics.joints.len();
        let saved_name = loaded.ir.name.clone();
        let saved_node_to_bone = loaded.ir.node_to_bone.clone();
        let saved_humanoid_count = loaded.ir.humanoid_bone_count;
        let saved_bone_meta: Vec<(Vec<usize>, Option<String>)> = loaded
            .ir
            .bones
            .iter()
            .map(|b| (b.children.clone(), b.vrm_bone_name.clone()))
            .collect();

        // other側にヒューマノイド情報がなければ original_name で再検出して補完
        let other_has_humanoid = other_ir.bones.iter().any(|b| b.vrm_bone_name.is_some());
        if !other_has_humanoid {
            let names: Vec<(usize, &str)> = other_ir
                .bones
                .iter()
                .enumerate()
                .map(|(i, b)| (i, b.original_name.as_str()))
                .collect();
            let mapping = crate::fbx::humanoid::detect_humanoid(&names);
            for (&idx, hb) in &mapping.mapping {
                other_ir.bones[idx].vrm_bone_name = Some(hb.as_vrm_name().to_string());
            }
            if !mapping.mapping.is_empty() {
                log::info!(
                    "Pre-merge humanoid completion: {} bones detected",
                    mapping.mapping.len()
                );
            }
        }

        let (merged_bones, new_bones) = loaded.ir.merge(other_ir);

        let device = &self.render_state.device;
        let queue = &self.render_state.queue;
        // merge後は材質数が変わるため material_display を resize
        let mc = loaded.ir.materials.len();
        self.material_display
            .resize_with(mc, MaterialDisplayState::default);
        let (smooth, clear, nmap, bloom) = Self::extract_per_mat_vecs(&self.material_display);
        match super::super::mesh::build_gpu_model_from_ir(
            &loaded.ir, device, queue, &smooth, &clear, &nmap, &bloom,
        ) {
            Ok(mut gpu_model) => {
                if let Some(ref renderer) = self.renderer {
                    renderer.prepare_mmd_resources(device, &mut gpu_model, &loaded.ir, &bloom);
                }
                if let Some(tex_id) = self.viewport_texture_id.take() {
                    let mut renderer = self.render_state.renderer.write();
                    renderer.free_texture(&tex_id);
                }
                let new_draw_count = gpu_model.draws.len();
                self.material_visibility.resize(new_draw_count, true);
                let new_morph_count = loaded.ir.morphs.len();
                self.morph_weights.resize(new_morph_count, 0.0);
                self.morph_dirty = self.morph_weights.iter().any(|&w| w != 0.0);
                loaded.mat_cache = Self::build_mat_cache(&loaded.ir, &gpu_model);
                loaded.stats_cache = CachedStats::new(&loaded.ir);
                let prev_draw_end: usize = loaded
                    .material_groups
                    .iter()
                    .map(|g| g.draw_range.end)
                    .max()
                    .unwrap_or(0);
                loaded.material_groups.push(MaterialGroup {
                    name: added_name.clone(),
                    material_range: saved_material_count..saved_material_count + added_materials,
                    draw_range: prev_draw_end..gpu_model.draws.len(),
                });
                loaded.gpu_model = gpu_model;
                let display_path = source.display_path().to_path_buf();
                loaded.appended_models.push(AppendedModel {
                    source,
                    pkg_model_name: pkg_model_name.clone(),
                    pkg_model: pkg_locator,
                });
                if let Some(dir) = display_path.parent() {
                    self.tex.last_dir = Some(dir.to_path_buf());
                }
                if let Some(ref mut renderer) = self.renderer {
                    renderer.invalidate_visualization_cache();
                    renderer.invalidate_normal_cache();
                    renderer.mark_sort_dirty();
                    // append 後のグリッドを更新（巨大モデル追加時にグリッドを拡大）
                    let (bbox_min, bbox_max) = loaded.gpu_model.bbox();
                    renderer.rebuild_grid(&self.render_state.device, bbox_min, bbox_max);
                }
                if let (Some(ref loaded), Some(ref old_anim)) = (&self.loaded, &self.anim.state) {
                    let mut new_state = AnimationState::new(
                        Arc::clone(&old_anim.animation),
                        &loaded.ir,
                        &loaded.gpu_model,
                    );
                    new_state.playing = old_anim.playing;
                    new_state.loop_mode = old_anim.loop_mode;
                    new_state.speed = old_anim.speed;
                    new_state.current_time = old_anim.current_time;
                    new_state.ab_start = old_anim.ab_start;
                    new_state.ab_end = old_anim.ab_end;
                    new_state.ping_pong_direction = old_anim.ping_pong_direction;
                    self.anim.state = Some(new_state);
                }
                log::info!(
                    "Append loaded: {} (bones:{} -> merged:{}/new:{}, meshes:{}, materials:{})",
                    added_name,
                    added_bones,
                    merged_bones,
                    new_bones,
                    added_meshes,
                    added_materials,
                );
                if !silent {
                    self.convert_message = Some(ConvertMessage::success(format!(
                        "追加読み込み完了: {}\nボーン:{} (統合:{} + 新規:{}), メッシュ:{}, 材質:{}",
                        added_name,
                        added_bones,
                        merged_bones,
                        new_bones,
                        added_meshes,
                        added_materials,
                    )));
                }
            }
            Err(e) => {
                log::error!("GPU rebuild failed (merge rollback): {e}");
                loaded.ir.bones.truncate(saved_bone_count);
                loaded.ir.meshes.truncate(saved_mesh_count);
                loaded.ir.materials.truncate(saved_material_count);
                loaded.ir.textures.truncate(saved_texture_count);
                loaded.ir.morphs.truncate(saved_morph_count);
                loaded.ir.physics.rigid_bodies.truncate(saved_rigid_count);
                loaded.ir.physics.joints.truncate(saved_joint_count);
                loaded.ir.name = saved_name;
                loaded.ir.node_to_bone = saved_node_to_bone;
                loaded.ir.humanoid_bone_count = saved_humanoid_count;
                for (i, bone) in loaded.ir.bones.iter_mut().enumerate() {
                    if i < saved_bone_meta.len() {
                        bone.children = saved_bone_meta[i].0.clone();
                        bone.vrm_bone_name = saved_bone_meta[i].1.clone();
                    }
                }
                self.convert_message = Some(ConvertMessage::failure(format!(
                    "追加読み込み後のGPU再構築に失敗しました。\n詳細: {e}"
                )));
            }
        }
        // loaded の借用スコープ外でシェーダー状態を正規化（ユーザー選択維持）
        self.normalize_shader_state();
    }

    /// ドラッグ＆ドロップ処理。(画像ホバー中, モデルホバー中) を返す
    pub(super) fn process_drag_and_drop(&mut self, ctx: &egui::Context) -> (bool, bool) {
        let (dropped_files, hover_ext, shift_held) = ctx.input(|i| {
            let hover_ext = i
                .raw
                .hovered_files
                .first()
                .and_then(|f| f.path.as_ref())
                .and_then(|p| p.extension())
                .and_then(|e| e.to_str())
                .map(|e| e.to_lowercase())
                .unwrap_or_default();
            self.drag_hovering = !i.raw.hovered_files.is_empty();
            let paths: Vec<PathBuf> = i
                .raw
                .dropped_files
                .iter()
                .filter_map(|f| f.path.clone())
                .collect();
            (paths, hover_ext, i.modifiers.shift)
        });

        let is_hover_image = IMAGE_EXTENSIONS.contains(&hover_ext.as_str());
        let is_hover_model = MODEL_EXTENSIONS.contains(&hover_ext.as_str());

        if !dropped_files.is_empty() {
            let mut image_files: Vec<PathBuf> = Vec::new();
            let mut model_file: Option<PathBuf> = None;
            for path in dropped_files {
                let ext = path
                    .extension()
                    .and_then(|e| e.to_str())
                    .unwrap_or("")
                    .to_lowercase();
                if IMAGE_EXTENSIONS.contains(&ext.as_str()) {
                    image_files.push(path);
                } else {
                    model_file = Some(path);
                }
            }

            let has_loaded_model = self.loaded.is_some();

            if let Some(model_path) = model_file {
                let append_ext = model_path
                    .extension()
                    .and_then(|e| e.to_str())
                    .unwrap_or("")
                    .to_lowercase();
                let is_appendable = matches!(
                    append_ext.as_str(),
                    "vrm"
                        | "fbx"
                        | "pmx"
                        | "pmd"
                        | "obj"
                        | "stl"
                        | "x"
                        | "unitypackage"
                        | "zip"
                        | "7z"
                );

                // temp/非temp を統一して PendingLoadDispatch に投入
                let preloaded = if is_temp_path(&model_path) {
                    match std::fs::read(&model_path) {
                        Ok(bytes) => {
                            let mut aux = HashMap::new();
                            if let Some(dir) = model_path.parent() {
                                collect_image_files_recursive(dir, dir, &mut aux);
                            }
                            Some(PreloadedData {
                                path: model_path.clone(),
                                main_bytes: bytes.into(),
                                aux_files: aux,
                            })
                        }
                        Err(e) => {
                            log::error!("Temp file prefetch failed: {e}");
                            None
                        }
                    }
                } else {
                    None
                };
                let append = shift_held && has_loaded_model && is_appendable;
                self.pending
                    .bg_state
                    .submit_dispatch(super::pending::PendingLoadDispatch {
                        path: model_path,
                        append,
                        overlay: super::pending::PendingOverlay::WaitingOverlay,
                        preloaded,
                    });
            }

            if !image_files.is_empty() && has_loaded_model {
                if image_files.len() == 1 {
                    let path = image_files.into_iter().next().expect("image_files は非空");
                    self.open_texture_preview(path);
                } else {
                    self.auto_assign_textures(image_files);
                }
            }
        }

        (is_hover_image, is_hover_model)
    }

    /// キーボードショートカット処理
    pub(super) fn process_keyboard_shortcuts(&mut self, ctx: &egui::Context) {
        use super::super::gpu::{DrawMode, LightMode};
        let wants_kb = ctx.wants_keyboard_input();
        ctx.input(|i| {
            if i.modifiers.ctrl && i.key_pressed(egui::Key::O) {
                self.open_file_dialog(ctx);
            }
            if !wants_kb {
                if !i.modifiers.ctrl && i.key_pressed(egui::Key::R) {
                    if let Some(ref loaded) = self.loaded {
                        let (bbox_min, bbox_max) = loaded.gpu_model.bbox();
                        self.camera.reset_to_bbox_with_margin(
                            bbox_min,
                            bbox_max,
                            self.last_viewport_width,
                            self.last_viewport_height,
                        );
                    }
                }
                if !i.modifiers.ctrl && i.key_pressed(egui::Key::F) {
                    if let Some(ref loaded) = self.loaded {
                        let (bbox_min, bbox_max) = loaded.gpu_model.bbox();
                        self.camera.fit_to_bbox_with_margin(
                            bbox_min,
                            bbox_max,
                            self.last_viewport_width,
                            self.last_viewport_height,
                        );
                    }
                }
                if !i.modifiers.ctrl && i.key_pressed(egui::Key::G) {
                    self.display.show_grid = !self.display.show_grid;
                }
                if !i.modifiers.ctrl && i.key_pressed(egui::Key::B) {
                    self.display.show_bones = !self.display.show_bones;
                }
                if !i.modifiers.ctrl && i.key_pressed(egui::Key::P) {
                    self.display.show_spring_bones = !self.display.show_spring_bones;
                }
                if !i.modifiers.ctrl && i.key_pressed(egui::Key::W) {
                    self.display.draw_mode = match self.display.draw_mode {
                        DrawMode::Solid => DrawMode::Wireframe,
                        DrawMode::Wireframe => DrawMode::SolidWireframe,
                        DrawMode::SolidWireframe => DrawMode::Solid,
                    };
                }
                if !i.modifiers.ctrl && i.key_pressed(egui::Key::N) {
                    self.display.show_normals = !self.display.show_normals;
                }
                if !i.modifiers.ctrl && i.key_pressed(egui::Key::L) {
                    self.display.light_mode = match self.display.light_mode {
                        LightMode::CameraFollow => LightMode::Fixed,
                        LightMode::Fixed => LightMode::CameraFollow,
                    };
                }
                {
                    let deg15 = 15.0_f32.to_radians();
                    if !i.modifiers.ctrl && i.key_pressed(egui::Key::Num0) {
                        self.camera.yaw = 0.0;
                        self.camera.pitch = 0.0;
                    }
                    if !i.modifiers.ctrl && i.key_pressed(egui::Key::Num1) {
                        self.camera.yaw = std::f32::consts::FRAC_PI_2;
                        self.camera.pitch = 0.0;
                    }
                    if !i.modifiers.ctrl && i.key_pressed(egui::Key::Num2) {
                        self.camera.pitch -= deg15;
                    }
                    if !i.modifiers.ctrl && i.key_pressed(egui::Key::Num3) {
                        self.camera.yaw = -std::f32::consts::FRAC_PI_2;
                        self.camera.pitch = 0.0;
                    }
                    if !i.modifiers.ctrl && i.key_pressed(egui::Key::Num4) {
                        self.camera.yaw += deg15;
                    }
                    if !i.modifiers.ctrl && i.key_pressed(egui::Key::Num5) {
                        self.camera.perspective = !self.camera.perspective;
                    }
                    if !i.modifiers.ctrl && i.key_pressed(egui::Key::Num6) {
                        self.camera.yaw -= deg15;
                    }
                    if !i.modifiers.ctrl && i.key_pressed(egui::Key::Num7) {
                        self.camera.yaw = 0.0;
                        self.camera.pitch = std::f32::consts::FRAC_PI_2 - 0.01;
                    }
                    if !i.modifiers.ctrl && i.key_pressed(egui::Key::Num8) {
                        self.camera.pitch += deg15;
                    }
                    if !i.modifiers.ctrl && i.key_pressed(egui::Key::Num9) {
                        self.camera.yaw = std::f32::consts::PI;
                        self.camera.pitch = 0.0;
                    }
                    if !i.modifiers.ctrl && i.key_pressed(egui::Key::Period) {
                        if let Some(ref loaded) = self.loaded {
                            let (bbox_min, bbox_max) = loaded.gpu_model.bbox();
                            self.camera.fit_to_bbox_with_margin(
                                bbox_min,
                                bbox_max,
                                self.last_viewport_width,
                                self.last_viewport_height,
                            );
                        }
                    }
                }
                if i.key_pressed(egui::Key::Space) {
                    if let Some(ref mut anim) = self.anim.state {
                        anim.playing = !anim.playing;
                    }
                }
                if i.key_pressed(egui::Key::ArrowLeft) {
                    if let Some(ref mut anim) = self.anim.state {
                        if !anim.playing {
                            anim.step_frame(false);
                        }
                    }
                }
                if i.key_pressed(egui::Key::ArrowRight) {
                    if let Some(ref mut anim) = self.anim.state {
                        if !anim.playing {
                            anim.step_frame(true);
                        }
                    }
                }
            }
        });
    }
}
