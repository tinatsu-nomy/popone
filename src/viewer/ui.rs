use std::collections::HashSet;
use std::path::Path;
use std::sync::Arc;

use eframe::egui;
use egui::epaint::{Color32, Mesh, Vertex};
use rust_i18n::t;

use super::app::uv_edit::{
    material_has_uv1, mesh_global_offsets_of, morph_uv_entry_count, read_displayed_uv,
    write_displayed_uv, UvDragMode, UvGizmoAction, UvRectBehavior,
};
use super::app::{ConvertMessage, DisplaySettings, PendingOverlay, SidePanelTab, ViewerApp};
use super::export_filter::build_filtered_ir;
use super::gpu::{DrawMode, LightMode, ShaderSelection};
use crate::intermediate::types::CullMode;

/// Dark theme panel background color (#1D1D1D).
const DARK_PANEL_BG: egui::Color32 = egui::Color32::from_rgb(0x1D, 0x1D, 0x1D);
/// Dark theme border color (#333333).
const DARK_BORDER_COLOR: egui::Color32 = egui::Color32::from_rgb(0x33, 0x33, 0x33);

// Material-row icons (migrated in v0.5.3 from [S][C][N][B][edit] to emoji).
// `NotoEmoji-Regular` included in egui's `FontDefinitions::default()` works as
// a fallback, so codepoints absent from Noto Sans JP/SC are still expected to
// render.
/// Normal smoothing (formerly [S]) - sparkle.
const ICON_SMOOTH: &str = "✨";
/// Clear custom normals (formerly [C]) - trash.
const ICON_CLEAR_NORMAL: &str = "🗑";
/// Normal map (formerly [N]) - map.
const ICON_NORMAL_MAP: &str = "🗺";
/// Emissive (formerly [B]) - bulb.
const ICON_EMISSIVE: &str = "💡";
/// Material edit drawer (formerly [edit]) - pencil.
const ICON_EDIT: &str = "✏";

/// Texture-assignment request from the material panel.
enum TexAssignRequest {
    /// Selected via file dialog.
    FileDialog(usize),
    /// Selected from `pkg_textures` (material index, in-pkg texture index).
    PkgTexture(usize, usize),
}

pub fn show_side_panel(ctx: &egui::Context, app: &mut ViewerApp) {
    // Texture-assignment request (handled outside the panel to avoid borrow conflicts).
    let mut tex_assign_request: Option<TexAssignRequest> = None;

    // v0.5.3: sync IR texture thumbnails for the leading button on each
    // material row. Early-returns on length match, so it costs nothing when
    // already synced.
    app.sync_ir_thumb_cache();

    let dark_panel = DARK_PANEL_BG;
    let dark_border = egui::Stroke::new(1.0, DARK_BORDER_COLOR);
    let panel_frame = egui::Frame::new()
        .fill(dark_panel)
        .stroke(dark_border)
        .inner_margin(egui::Margin::same(4));

    // The Display tab's toggle controls resize behavior.
    // - OFF (default): completely fixed at 280 px.
    // - ON          : user can drag in the 280..=600 px range.
    //
    // egui 0.31's `SidePanel` persists "min_rect after content drawing" per
    // `id` as `PanelState` and uses it as the width on the next frame.
    // Consequently, when ON has a wider `width_range`, the panel grows every
    // frame whenever a child widget's `min_rect` exceeds the panel width
    // (= auto-resize problem).
    //
    // Mitigation: forcibly overwrite `PanelState` with our app-managed
    // `target_w` immediately before `show`, and right after `show` import
    // the new width "only if the `__resize` ID was dragged". This blocks
    // content-driven width changes and reflects only user drags into
    // `panel_width`.
    let panel_resizable = app.display.panel_resizable;
    // Use the persisted `panel_width` for both ON / OFF.
    // The only ON / OFF difference is whether the user can change it via drag.
    //   - ON : `width_range(280..=600)`, draggable; the new width is captured post-show and saved.
    //   - OFF: `width_range(target_w..=target_w)`, locked to the current width (no drag).
    // On first launch, starts from the default `panel_width` of 280 px.
    let clamped = app.display.panel_width.clamp(280.0, 600.0);
    if (clamped - app.display.panel_width).abs() > f32::EPSILON {
        app.display.panel_width = clamped;
    }
    let target_w = clamped;
    let (min_w, max_w) = if panel_resizable {
        (280.0, 600.0)
    } else {
        (target_w, target_w)
    };

    let panel_id_obj = egui::Id::new("info_panel");

    // Squash whatever `PanelState.width` egui inflated last frame to `target_w`.
    ctx.data_mut(|d| {
        let new_rect = match d.get_persisted::<egui::containers::panel::PanelState>(panel_id_obj) {
            Some(mut s) => {
                s.rect.set_width(target_w);
                s.rect
            }
            None => egui::Rect::from_min_size(egui::Pos2::ZERO, egui::Vec2::new(target_w, 1.0)),
        };
        d.insert_persisted(
            panel_id_obj,
            egui::containers::panel::PanelState { rect: new_rect },
        );
    });

    egui::SidePanel::right("info_panel")
        .default_width(target_w)
        .width_range(min_w..=max_w)
        .resizable(panel_resizable)
        .frame(panel_frame)
        .show(ctx, |ui| {
            // Force all side-panel text white.
            ui.visuals_mut().widgets.noninteractive.fg_stroke =
                egui::Stroke::new(1.0, egui::Color32::WHITE);
            ui.visuals_mut().widgets.inactive.fg_stroke =
                egui::Stroke::new(1.0, egui::Color32::WHITE);
            ui.visuals_mut().override_text_color = Some(egui::Color32::WHITE);

            // Tab bar (v0 design: flat style, equal width, no gaps).
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 0.0;
                let panel_w = ui.available_width();
                let tabs: [(SidePanelTab, std::borrow::Cow<'static, str>); 4] = [
                    (SidePanelTab::Info, t!("viewer.tab.info")),
                    (SidePanelTab::Control, t!("viewer.tab.control")),
                    (SidePanelTab::Display, t!("viewer.tab.display")),
                    (SidePanelTab::Export, t!("viewer.tab.export")),
                ];
                // Tabs are evenly divided to track the panel width.
                // At the minimum panel width of 280 px, panel_w / 4 ~= 68 px,
                // which is the effective minimum tab size (= the existing UI
                // size). When the panel widens, tabs grow by 1/4 each so no
                // gap appears on the right.
                let tab_width = panel_w / tabs.len() as f32;
                for (tab, label) in tabs {
                    let is_active = app.side_panel_tab == tab;
                    let text = egui::RichText::new(label).size(11.0);
                    let text = if is_active {
                        text.color(egui::Color32::WHITE).strong()
                    } else {
                        text.color(egui::Color32::from_gray(0xD0))
                    };
                    let btn = egui::Button::new(text)
                        .fill(if is_active {
                            egui::Color32::from_rgb(0x4A, 0x90, 0xD9)
                        } else {
                            egui::Color32::from_rgb(0x2A, 0x2A, 0x2A)
                        })
                        .min_size(egui::vec2(tab_width, 20.0));
                    if ui.add(btn).clicked() {
                        app.side_panel_tab = tab;
                    }
                }
            });
            ui.separator();

            egui::ScrollArea::vertical().show(ui, |ui| match app.side_panel_tab {
                SidePanelTab::Info => show_tab_info(ui, app),
                SidePanelTab::Control => show_tab_control(ui, app),
                SidePanelTab::Display => show_tab_display(ui, app, &mut tex_assign_request),
                SidePanelTab::Export => show_tab_export(ui, app),
            });
        });

    // Update `panel_width` only when the user drags the resize handle.
    // egui SidePanel assigns `id.with("__resize")` to the resize handle
    // (egui 0.31 panel.rs:257). Using `dragged()` on its response
    // distinguishes content-driven width changes from user-driven ones.
    if panel_resizable {
        let resize_id = panel_id_obj.with("__resize");
        let dragged = ctx.read_response(resize_id).is_some_and(|r| r.dragged());
        if dragged {
            let stored_w = ctx.data_mut(|d| {
                d.get_persisted::<egui::containers::panel::PanelState>(panel_id_obj)
                    .map(|s| s.rect.width())
            });
            if let Some(w) = stored_w {
                app.display.panel_width = w.clamp(280.0, 600.0);
            }
        }
    }

    // Texture assignment (handled after the borrow is released).
    match tex_assign_request {
        // Skip if the dialog is already open.
        Some(TexAssignRequest::FileDialog(mat_idx)) if app.tex.pending_file_dialog.is_none() => {
            let mat_name = app
                .loaded
                .as_ref()
                .and_then(|l| l.mat_cache.names.get(mat_idx))
                .map(|s| s.to_string())
                .unwrap_or_else(|| "?".to_string());
            let file_name = app.loaded.as_ref().and_then(|l| {
                l.mat_cache
                    .source_tex_names
                    .get(mat_idx)
                    .and_then(|s| s.clone())
            });
            let initial_dir = app.tex.last_dir.clone();

            // Open the file dialog on a separate thread (does not block the UI).
            let dialog_title = t!("viewer.texture_picker.title", name = mat_name).into_owned();
            let (tx, rx) = std::sync::mpsc::channel();
            let repaint = ctx.clone();
            std::thread::spawn(move || {
                let mut dialog = rfd::FileDialog::new()
                    .set_title(dialog_title)
                    .add_filter("Image", &["png", "jpg", "jpeg", "tga", "bmp", "psd", "dds"]);
                if let Some(ref name) = file_name {
                    dialog = dialog.set_file_name(name);
                }
                if let Some(ref dir) = initial_dir {
                    dialog = dialog.set_directory(dir);
                }
                let _ = tx.send(dialog.pick_file());
                repaint.request_repaint();
            });
            app.tex.pending_file_dialog = Some((
                mat_idx,
                crate::intermediate::types::TextureSlot::BaseColor,
                rx,
            ));
        }
        Some(TexAssignRequest::FileDialog(_)) => {
            // `pending_file_dialog` is `Some` (already open) -> skip.
        }
        Some(TexAssignRequest::PkgTexture(mat_idx, tex_idx)) => {
            if let Some(ref pkg) = app.tex.pkg_textures {
                if let Some((ref tex_name, ref tex_data)) = pkg.get(tex_idx) {
                    let name = tex_name.clone();
                    let data = tex_data.clone();
                    if app.assign_texture_data_to_material(mat_idx, &name, &data) {
                        app.tex.pkg_assignments.insert(mat_idx, name.clone());
                        // Record same-name siblings into pkg-assignment history too (limited to the same MaterialGroup).
                        if app.tex.link_same_name {
                            if let Some(ref loaded) = app.loaded {
                                for sib in loaded.same_name_siblings(mat_idx) {
                                    app.tex.pkg_assignments.insert(sib, name.clone());
                                }
                            }
                        }
                    }
                }
            }
        }
        None => {}
    }

    // Poll the result of the async texture file dialog.
    if let Some((mat_idx, slot, ref rx)) = app.tex.pending_file_dialog {
        match rx.try_recv() {
            Ok(Some(path)) => {
                if let Some(dir) = path.parent() {
                    app.tex.last_dir = Some(dir.to_path_buf());
                }
                // Validate the material index in case the model has changed.
                // (When another model is loaded while the dialog is open, the index becomes stale.)
                let valid = app
                    .loaded
                    .as_ref()
                    .is_some_and(|l| mat_idx < l.ir.materials.len());
                if valid {
                    // Step 4-16b: branch the assignment path by slot.
                    if slot == crate::intermediate::types::TextureSlot::BaseColor {
                        app.assign_texture_to_material(mat_idx, &path);
                    } else {
                        // Non-BaseColor: read the file and assign via `assign_texture_core(slot)`.
                        // review_016: record the path in `slot_texture_paths` so it can be restored on reload.
                        if let Ok(data) = std::fs::read(&path) {
                            let ext = path
                                .extension()
                                .and_then(|e| e.to_str())
                                .unwrap_or("")
                                .to_lowercase();
                            let is_psd = ext == "psd";
                            let name = path.to_string_lossy().to_string();
                            if app.assign_texture_core(mat_idx, slot, &data, is_psd, &name) {
                                app.slot_texture_paths.insert((mat_idx, slot), path.clone());
                                // review_017 [P2-1]: also record same-name siblings into `slot_texture_paths`.
                                if app.tex.link_same_name {
                                    if let Some(ref loaded) = app.loaded {
                                        let siblings = loaded.same_name_siblings(mat_idx);
                                        for sib in siblings {
                                            app.slot_texture_paths
                                                .insert((sib, slot), path.clone());
                                        }
                                    }
                                }
                            }
                        }
                    }
                } else {
                    log::warn!(
                        "Texture dialog result discarded: material index {} out of range \
                         (model changed during dialog)",
                        mat_idx
                    );
                }
                app.tex.pending_file_dialog = None;
            }
            Ok(None) => {
                // User cancelled.
                app.tex.pending_file_dialog = None;
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => {
                // Dialog still open - do nothing.
            }
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                // Thread terminated abnormally.
                app.tex.pending_file_dialog = None;
            }
        }
    }

    // FBX load-mode selection dialog.
    show_fbx_choice_dialog(ctx, app);

    // OBJ/STL import-options dialog.
    show_import_options_dialog(ctx, app);

    // unitypackage model-selection dialog.
    show_fbx_select_dialog(ctx, app);

    // In-archive model-selection dialog.
    show_archive_select_dialog(ctx, app);
    show_archive_password_dialog(ctx, app);

    // unitypackage manual texture-assignment dialog + real-time preview.
    app.prepare_tex_match_views();
    show_tex_match_dialog(ctx, app);
    app.sync_tex_match_preview();

    // Texture-history overwrite confirmation dialog.
    show_confirm_save_tex_history(ctx, app);
}

/// Texture-history overwrite-save confirmation dialog.
fn show_confirm_save_tex_history(ctx: &egui::Context, app: &mut ViewerApp) {
    if !app.pending.confirm_save_tex_history {
        return;
    }
    let mut confirmed = false;
    let mut cancelled = false;
    egui::Window::new(t!("viewer.dialog.tex_history.title"))
        .collapsible(false)
        .resizable(false)
        .default_pos(ctx.screen_rect().center())
        .pivot(egui::Align2::CENTER_CENTER)
        .show(ctx, |ui| {
            ui.label(t!("viewer.dialog.tex_history.message1"));
            ui.label(t!("viewer.dialog.tex_history.message2"));
            ui.separator();
            ui.horizontal(|ui| {
                if ui.button(t!("viewer.dialog.tex_history.confirm")).clicked() {
                    confirmed = true;
                }
                if ui.button(t!("viewer.dialog.common.cancel")).clicked() {
                    cancelled = true;
                }
            });
        });
    if confirmed {
        app.pending.confirm_save_tex_history = false;
        app.do_save_texture_history();
    } else if cancelled {
        app.pending.confirm_save_tex_history = false;
    }
}

/// FBX load-mode selection dialog (when the file contains both model and animation).
fn show_fbx_choice_dialog(ctx: &egui::Context, app: &mut ViewerApp) {
    let Some(ref mut pending) = app.pending.fbx_choice else {
        return;
    };

    let file_name = pending
        .path
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();

    let mut confirmed = false;
    let mut cancelled = false;
    let mut open = true;

    egui::Window::new(t!("viewer.dialog.fbx_choice.title"))
        .open(&mut open)
        .collapsible(false)
        .resizable(false)
        .default_pos(ctx.screen_rect().center())
        .pivot(egui::Align2::CENTER_CENTER)
        .show(ctx, |ui| {
            ui.label(format!("\"{}\"", file_name));
            ui.label(t!("viewer.dialog.fbx_choice.message"));
            ui.separator();
            let no_model_loaded = app.loaded.is_none();
            if no_model_loaded {
                // On first load, the model is required (animation-only is not allowed).
                pending.load_model = true;
                ui.add_enabled(
                    false,
                    egui::Checkbox::new(
                        &mut pending.load_model,
                        t!("viewer.dialog.fbx_choice.load_model"),
                    ),
                )
                .on_disabled_hover_text(t!("viewer.dialog.fbx_choice.load_model_required"));
            } else {
                ui.checkbox(
                    &mut pending.load_model,
                    t!("viewer.dialog.fbx_choice.load_model"),
                );
            }
            ui.checkbox(
                &mut pending.load_animation,
                t!("viewer.dialog.fbx_choice.load_animation"),
            );
            ui.separator();
            ui.horizontal(|ui| {
                let can_ok = pending.load_model || pending.load_animation;
                if ui
                    .add_enabled(can_ok, egui::Button::new(t!("viewer.dialog.common.ok")))
                    .clicked()
                {
                    confirmed = true;
                }
                if ui.button(t!("viewer.dialog.common.cancel")).clicked() {
                    cancelled = true;
                }
            });
        });

    if confirmed {
        let choice = app
            .pending
            .fbx_choice
            .take()
            .expect("pending_fbx_choice confirmed Some");
        app.execute_fbx_choice(choice);
    } else if cancelled || !open || ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
        app.pending.fbx_choice = None;
        app.pending.multi_load = None;
    }
}

/// OBJ/STL import-options selection dialog.
fn show_import_options_dialog(ctx: &egui::Context, app: &mut ViewerApp) {
    use super::app::pending::ImportUnit;

    let Some(ref mut pending) = app.pending.import_options else {
        return;
    };

    let file_name = pending
        .path
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();
    let format_label = match pending.format {
        super::app::file_io::FileFormat::Obj => "OBJ",
        super::app::file_io::FileFormat::Stl => "STL",
        _ => "?",
    };

    let mut confirmed = false;
    let mut cancelled = false;
    let mut open = true;

    egui::Window::new(t!(
        "viewer.dialog.import_options.title",
        format = format_label
    ))
    .open(&mut open)
    .collapsible(false)
    .resizable(false)
    .default_pos(ctx.screen_rect().center())
    .pivot(egui::Align2::CENTER_CENTER)
    .show(ctx, |ui| {
        ui.label(format!("\"{}\"", file_name));
        ui.separator();

        ui.horizontal(|ui| {
            ui.label(t!("viewer.dialog.import_options.unit"));
            for unit in [
                ImportUnit::Mm,
                ImportUnit::Cm,
                ImportUnit::M,
                ImportUnit::Inch,
            ] {
                ui.radio_value(&mut pending.unit, unit, unit.label());
            }
        });
        ui.add_space(4.0);
        ui.checkbox(
            &mut pending.z_up,
            t!("viewer.dialog.import_options.z_up_convert"),
        );
        ui.separator();

        ui.horizontal(|ui| {
            if ui.button(t!("viewer.dialog.common.ok")).clicked() {
                confirmed = true;
            }
            if ui.button(t!("viewer.dialog.common.cancel")).clicked() {
                cancelled = true;
            }
        });
    });

    if confirmed {
        let opts = app
            .pending
            .import_options
            .take()
            .expect("pending.import_options confirmed Some");
        app.execute_import_with_options(opts);
    } else if cancelled || !open || ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
        app.pending.import_options = None;
    }
}

/// Selection dialog used when a unitypackage contains multiple models.
fn show_fbx_select_dialog(ctx: &egui::Context, app: &mut ViewerApp) {
    if app.pending.unity_pkg.is_none() {
        return;
    }

    let mut selected: Option<(usize, super::app::PkgModelType)> = None;
    let mut multi_selected = false;
    let mut cancelled = false;
    let mut open = true;

    egui::Window::new(t!("viewer.dialog.pkg_select.title"))
        .open(&mut open)
        .collapsible(false)
        .resizable(false)
        .default_pos(ctx.screen_rect().center())
        .pivot(egui::Align2::CENTER_CENTER)
        .show(ctx, |ui| {
            ui.label(t!("viewer.dialog.pkg_select.message1"));
            ui.label(t!("viewer.dialog.pkg_select.message2"));
            ui.separator();
            let Some(pending) = app.pending.unity_pkg.as_mut() else {
                return;
            };
            egui::ScrollArea::vertical()
                .max_height(300.0)
                .show(ui, |ui| {
                    for (i, (asset_idx, name, model_type)) in pending.model_list.iter().enumerate()
                    {
                        let type_label = match model_type {
                            super::app::PkgModelType::Prefab => "[Prefab]",
                            super::app::PkgModelType::Vrm => "[VRM]",
                            super::app::PkgModelType::Fbx => "[FBX]",
                        };
                        ui.horizontal(|ui| {
                            ui.checkbox(&mut pending.checked[i], "");
                            if ui.button(format!("{} {}", type_label, name)).clicked() {
                                selected = Some((*asset_idx, *model_type));
                            }
                        });
                    }
                });
            ui.separator();
            let checked_count = pending.checked.iter().filter(|&&c| c).count();
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(
                        checked_count >= 1,
                        egui::Button::new(t!(
                            "viewer.dialog.pkg_select.batch_load",
                            count = checked_count
                        )),
                    )
                    .clicked()
                {
                    multi_selected = true;
                }
                if ui.button(t!("viewer.dialog.common.cancel")).clicked() {
                    cancelled = true;
                }
            });
        });

    if let Some((idx, model_type)) = selected {
        // Single selection: same behavior as before.
        let pending = app
            .pending
            .unity_pkg
            .take()
            .expect("pending_unity_pkg confirmed Some");
        app.pending.pkg_load = Some(super::app::PendingPkgModelLoad {
            assets: std::sync::Arc::new(pending.assets),
            fbx_index: idx,
            model_type,
            source_path: pending.source_path,
            shown: false,
            append: pending.append,
            suppress_tex_match: false,
            archive_snapshot: pending.archive_snapshot,
            nested_archive_source: pending.nested_archive_source,
            pkg_index: pending.pkg_index,
            batch_progress: None,
            skip_anim_check: false,
        });
    } else if multi_selected {
        // Multi-selection: load the first normally; queue the rest into `PendingMultiLoad`.
        let pending = app
            .pending
            .unity_pkg
            .take()
            .expect("pending_unity_pkg confirmed Some");
        let checked_indices: Vec<usize> = pending
            .checked
            .iter()
            .enumerate()
            .filter_map(|(i, &c)| if c { Some(i) } else { None })
            .collect();

        if let Some((&first, rest)) = checked_indices.split_first() {
            let (first_asset_idx, _, first_model_type) = pending.model_list[first];

            // Wrap `assets` in `Arc` for sharing (clones bump only the refcount).
            let shared_assets = std::sync::Arc::new(pending.assets);

            if rest.is_empty() {
                // Only one item.
                app.pending.pkg_load = Some(super::app::PendingPkgModelLoad {
                    assets: shared_assets,
                    fbx_index: first_asset_idx,
                    model_type: first_model_type,
                    source_path: pending.source_path,
                    shown: false,
                    append: pending.append,
                    suppress_tex_match: false,
                    archive_snapshot: pending.archive_snapshot,
                    nested_archive_source: pending.nested_archive_source,
                    pkg_index: pending.pkg_index,
                    batch_progress: None,
                    skip_anim_check: false,
                });
            } else {
                // Multiple: share `assets` via `Arc::clone` only.
                let remaining: Vec<(usize, super::app::PkgModelType)> = rest
                    .iter()
                    .rev()
                    .map(|&i| {
                        let (idx, _, mt) = pending.model_list[i];
                        (idx, mt)
                    })
                    .collect();
                let total_count = 1 + remaining.len(); // first + rest
                app.pending.pkg_load = Some(super::app::PendingPkgModelLoad {
                    assets: std::sync::Arc::clone(&shared_assets),
                    fbx_index: first_asset_idx,
                    model_type: first_model_type,
                    source_path: pending.source_path.clone(),
                    shown: false,
                    append: pending.append,
                    suppress_tex_match: false,
                    archive_snapshot: pending.archive_snapshot.clone(),
                    nested_archive_source: pending.nested_archive_source.clone(),
                    pkg_index: pending.pkg_index.clone(),
                    batch_progress: Some((1, total_count)),
                    skip_anim_check: false,
                });
                app.pending.multi_load = Some(super::app::PendingMultiLoad {
                    assets: shared_assets,
                    remaining,
                    source_path: pending.source_path,
                    archive_snapshot: pending.archive_snapshot,
                    nested_archive_source: pending.nested_archive_source,
                    pkg_index: pending.pkg_index,
                    total_count,
                });
            }
        }
    } else if cancelled || !open || ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
        app.pending.unity_pkg = None;
    }
}

/// Selection dialog used when an archive contains multiple models.
fn show_archive_select_dialog(ctx: &egui::Context, app: &mut ViewerApp) {
    if app.pending.archive.is_none() {
        return;
    }

    let mut selected: Option<usize> = None;
    let mut cancelled = false;
    let mut open = true;

    egui::Window::new(t!("viewer.dialog.archive_select.title"))
        .open(&mut open)
        .collapsible(false)
        .resizable(false)
        .default_pos(ctx.screen_rect().center())
        .pivot(egui::Align2::CENTER_CENTER)
        .show(ctx, |ui| {
            ui.label(t!("viewer.dialog.archive_select.message1"));
            ui.label(t!("viewer.dialog.archive_select.message2"));
            ui.separator();
            // Re-borrow `pending` inside the closure (avoids cloning PathBuf / String).
            let Some(pending) = app.pending.archive.as_ref() else {
                return;
            };
            egui::ScrollArea::vertical()
                .max_height(300.0)
                .show(ui, |ui| {
                    for (i, (_, path, _name, kind)) in pending.contents.models.iter().enumerate() {
                        if ui
                            .button(format!("[{}] {}", kind.label(), path.display()))
                            .clicked()
                        {
                            selected = Some(i);
                        }
                    }
                });
            // Bundled text documents (readme etc.): open in a separate window
            // without leaving the dialog, so the readme can guide the choice.
            let clicked_text = {
                let m = app.text_viewer.lock().unwrap_or_else(|p| p.into_inner());
                if m.files.is_empty() {
                    None
                } else {
                    ui.separator();
                    ui.label(t!("viewer.dialog.archive_select.text_files"));
                    egui::ScrollArea::vertical()
                        .id_salt("archive_select_text_list")
                        .max_height(120.0)
                        .show(ui, |ui| m.list_ui(ui))
                        .inner
                }
            };
            if let Some(i) = clicked_text {
                let mut m = app.text_viewer.lock().unwrap_or_else(|p| p.into_inner());
                if let Some(vp_id) = m.open_doc(i) {
                    ui.ctx()
                        .send_viewport_cmd_to(vp_id, egui::ViewportCommand::Focus);
                }
            }
            ui.separator();
            if ui.button(t!("viewer.dialog.common.cancel")).clicked() {
                cancelled = true;
            }
        });

    if let Some(idx) = selected {
        let pending = app
            .pending
            .archive
            .take()
            .expect("pending_archive confirmed Some");
        app.pending.archive_load = Some(super::app::PendingArchiveLoad {
            archive_data: pending.archive_data,
            format: pending.format,
            contents: pending.contents,
            model_index: idx,
            source_path: pending.source_path,
            shown: false,
            append: pending.append,
            is_temp: pending.is_temp,
            password: pending.password,
        });
    } else if cancelled || !open || ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
        app.pending.archive = None;
    }
}

/// Password-input dialog for encrypted archives. The entered password is used
/// once for the retried load and never persisted (see `PendingArchivePassword`).
fn show_archive_password_dialog(ctx: &egui::Context, app: &mut ViewerApp) {
    let Some(pending) = app.pending.archive_password.as_mut() else {
        return;
    };
    // Already submitted: keep the dialog hidden while the retry is dispatched.
    if pending.submitted {
        return;
    }

    let mut submitted = false;
    let mut cancelled = false;
    let mut open = true;

    egui::Window::new(t!("viewer.dialog.archive_password.title"))
        .open(&mut open)
        .collapsible(false)
        .resizable(false)
        .default_pos(ctx.screen_rect().center())
        .pivot(egui::Align2::CENTER_CENTER)
        .show(ctx, |ui| {
            let file_name = pending
                .path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy();
            ui.label(t!(
                "viewer.dialog.archive_password.message",
                name = file_name
            ));
            if let Some(ref err) = pending.error {
                ui.colored_label(egui::Color32::from_rgb(0xE0, 0x60, 0x60), err);
            }
            let edit = egui::TextEdit::singleline(&mut pending.input)
                .password(true)
                .desired_width(240.0);
            let response = ui.add(edit);
            // Focus the field when the dialog opens so the user can type immediately.
            if !response.has_focus() && pending.input.is_empty() && pending.error.is_none() {
                response.request_focus();
            }
            let enter_pressed =
                response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            ui.separator();
            ui.horizontal(|ui| {
                let ok_enabled = !pending.input.is_empty();
                if ui
                    .add_enabled(
                        ok_enabled,
                        egui::Button::new(t!("viewer.dialog.archive_password.ok")),
                    )
                    .clicked()
                    || (enter_pressed && ok_enabled)
                {
                    submitted = true;
                }
                if ui.button(t!("viewer.dialog.common.cancel")).clicked() {
                    cancelled = true;
                }
            });
        });

    if submitted {
        pending.error = None;
        pending.submitted = true;
    } else if cancelled || !open || ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
        app.pending.archive_password = None;
    }
}

/// unitypackage manual texture-assignment dialog.
fn show_tex_match_dialog(ctx: &egui::Context, app: &mut ViewerApp) {
    let Some(ref pending) = app.tex.pending_match else {
        return;
    };

    // Reference the `pkg_textures` file-name list and thumbnail IDs.
    let tex_names: Vec<&str> = app
        .tex
        .pkg_textures
        .as_ref()
        .map(|t| t.iter().map(|(name, _)| name.as_str()).collect())
        .unwrap_or_default();
    let thumb_ids = &app.tex.pkg_thumb_cache;
    if tex_names.is_empty() {
        app.cancel_tex_match_preview();
        return;
    }

    // Get material names / source names from `loaded` (avoids clones).
    let mat_info: Vec<(String, Option<String>)> = pending
        .mat_indices
        .iter()
        .map(|&i| {
            app.loaded
                .as_ref()
                .map(|l| {
                    (
                        l.ir.materials[i].name.clone(),
                        l.ir.materials[i].source_texture_name.clone(),
                    )
                })
                .unwrap_or_default()
        })
        .collect();
    let mat_count = mat_info.len();

    let mut apply = false;
    let mut cancelled = false;
    let mut new_selections = pending.selections.clone();
    let mut open = true;

    egui::Window::new(t!("viewer.dialog.tex_match.title"))
        .open(&mut open)
        .collapsible(true)
        .resizable(true)
        .default_width(450.0)
        .default_pos(egui::pos2(20.0, 60.0))
        .show(ctx, |ui| {
            ui.label(t!("viewer.dialog.tex_match.instruction"));
            ui.horizontal(|ui| {
                ui.label(t!(
                    "viewer.dialog.tex_match.pkg_tex_count",
                    count = tex_names.len()
                ));
                let link_resp = ui.checkbox(
                    &mut app.tex.link_same_name,
                    t!("viewer.dialog.tex_match.link_same_name"),
                );
                // On link toggle, fully restore the preview and resync.
                if link_resp.changed() {
                    if let (Some(ref mut pending), Some(ref mut loaded)) =
                        (&mut app.tex.pending_match, &mut app.loaded)
                    {
                        // Restore all `saved_binds`.
                        for (draw_idx, (orig_tex, orig_mmd)) in pending.saved_binds.drain() {
                            if draw_idx < loaded.gpu_model.draws.len() {
                                loaded.gpu_model.draws[draw_idx].texture_bind_group = orig_tex;
                                loaded.gpu_model.draws[draw_idx].mmd_texture_bind_group = orig_mmd;
                            }
                        }
                        // On ON: normalize `selections` within each same-name group
                        // (prefer `Some` to unify the group).
                        if app.tex.link_same_name {
                            let mat_count = pending.mat_indices.len();
                            let mut unified: std::collections::HashMap<String, Option<usize>> =
                                std::collections::HashMap::new();
                            for i in 0..mat_count {
                                let mi = pending.mat_indices[i];
                                let mat_name = loaded
                                    .ir
                                    .materials
                                    .get(mi)
                                    .map(|m| m.name.clone())
                                    .unwrap_or_default();
                                let entry = unified.entry(mat_name).or_insert(None);
                                // Prefer `Some` (None -> Some overwrites, Some -> Some keeps the first).
                                if entry.is_none() && pending.selections[i].is_some() {
                                    *entry = pending.selections[i];
                                }
                            }
                            for i in 0..mat_count {
                                let mi = pending.mat_indices[i];
                                let mat_name = loaded
                                    .ir
                                    .materials
                                    .get(mi)
                                    .map(|m| m.name.as_str())
                                    .unwrap_or_default();
                                if let Some(&group_sel) = unified.get(mat_name) {
                                    pending.selections[i] = group_sel;
                                }
                            }
                        }
                        // Reset all `previewed` -> reapplied during next frame's sync.
                        pending.previewed.iter_mut().for_each(|p| *p = None);
                    }
                }
            });
            ui.separator();

            let mut tex_filter = app
                .tex
                .pending_match
                .as_ref()
                .map(|p| p.tex_filter.clone())
                .unwrap_or_default();

            egui::ScrollArea::vertical()
                .max_height(400.0)
                .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysVisible)
                .show(ui, |ui| {
                    egui::Grid::new("tex_match_grid")
                        .num_columns(3)
                        .spacing([8.0, 4.0])
                        .striped(true)
                        .show(ui, |ui| {
                            ui.strong(t!("viewer.dialog.tex_match.col_material"));
                            ui.strong(t!("viewer.dialog.tex_match.col_source_tex"));
                            ui.strong(t!("viewer.dialog.tex_match.col_assigned_tex"));
                            ui.end_row();

                            for i in 0..mat_count {
                                // Highlight flag for this row (any cell hover triggers it).
                                let mut row_highlight = false;

                                let mat_label = ui.label(&mat_info[i].0);
                                if mat_label.contains_pointer() {
                                    row_highlight = true;
                                }
                                let src = mat_info[i].1.as_deref().unwrap_or("-");
                                let src_label = ui.label(egui::RichText::new(src).color(egui::Color32::GRAY));
                                if src_label.contains_pointer() {
                                    row_highlight = true;
                                }

                                ui.horizontal(|ui| {
                                    // Thumbnail of the currently selected texture.
                                    if let Some(sel_idx) = new_selections[i] {
                                        if let Some(Some(tex_id)) = thumb_ids.get(sel_idx) {
                                            ui.image(egui::load::SizedTexture::new(
                                                *tex_id,
                                                [32.0, 32.0],
                                            ));
                                        }
                                    }
                                    let none_label = t!("viewer.dialog.tex_match.none");
                                    let current_label = new_selections[i]
                                        .and_then(|idx| tex_names.get(idx))
                                        .copied()
                                        .unwrap_or(&none_label);
                                    let popup_id = ui.id().with(("tex_match_popup", i));
                                    let btn = ui.add_sized(
                                        [188.0, 20.0],
                                        egui::Button::new(
                                            egui::RichText::new(format!("⏷ {current_label}"))
                                                .color(ui.visuals().text_color()),
                                        )
                                        .frame(true),
                                    );
                                    if btn.contains_pointer() || btn.has_focus() {
                                        row_highlight = true;
                                    }
                                    // Highlight while the popup is open as well.
                                    if ui.memory(|m| m.is_popup_open(popup_id)) {
                                        row_highlight = true;
                                    }
                                    if btn.clicked() {
                                        ui.memory_mut(|m| m.toggle_popup(popup_id));
                                    }
                                    egui::popup_below_widget(
                                        ui,
                                        popup_id,
                                        &btn,
                                        egui::PopupCloseBehavior::CloseOnClickOutside,
                                        |ui| {
                                            ui.set_min_width(240.0);
                                            if ui.selectable_value(
                                                &mut new_selections[i],
                                                None,
                                                t!("viewer.dialog.tex_match.none"),
                                            ).clicked() {
                                                ui.memory_mut(|m| m.toggle_popup(popup_id));
                                                tex_filter.clear();
                                            }
                                            ui.separator();
                                            ui.add(
                                                egui::TextEdit::singleline(&mut tex_filter)
                                                    .desired_width(ui.available_width())
                                                    .hint_text(t!("viewer.dialog.tex_match.filter_hint")),
                                            );
                                            let tex_filter_lower = tex_filter.to_lowercase();
                                            egui::ScrollArea::vertical()
                                                .max_height(300.0)
                                                .show(ui, |ui| {
                                                    for (ti, name) in tex_names.iter().enumerate() {
                                                        if !tex_filter_lower.is_empty()
                                                            && !name
                                                                .to_lowercase()
                                                                .contains(&tex_filter_lower)
                                                        {
                                                            continue;
                                                        }
                                                        let clicked = ui.horizontal(|ui| {
                                                            if let Some(Some(tex_id)) = thumb_ids.get(ti) {
                                                                ui.image(egui::load::SizedTexture::new(
                                                                    *tex_id,
                                                                    [24.0, 24.0],
                                                                ));
                                                            }
                                                            ui.selectable_value(
                                                                &mut new_selections[i],
                                                                Some(ti),
                                                                *name,
                                                            ).clicked()
                                                        }).inner;
                                                        if clicked {
                                                            ui.memory_mut(|m| m.toggle_popup(popup_id));
                                                            tex_filter.clear();
                                                        }
                                                    }
                                                });
                                        },
                                    );
                                });
                                // Row hover -> highlight in the 3D view.
                                if row_highlight {
                                    if let (Some(ref pending), Some(ref loaded)) =
                                        (&app.tex.pending_match, &app.loaded)
                                    {
                                        let real_mat_idx = pending.mat_indices[i];
                                        for (di, d) in loaded.gpu_model.draws.iter().enumerate() {
                                            if d.material_index == real_mat_idx
                                                && app.material_visibility.get(di).copied().unwrap_or(true)
                                            {
                                                app.hovered_draw_indices.push(di);
                                            }
                                        }
                                    }
                                }
                                ui.end_row();
                            }
                        });
                });

            // Write back the filter value.
            if let Some(ref mut pending) = app.tex.pending_match {
                pending.tex_filter = tex_filter;
            }

            ui.separator();
            ui.horizontal(|ui| {
                let has_selection = new_selections.iter().any(|s| s.is_some());
                if ui
                    .add_enabled(
                        has_selection,
                        egui::Button::new(t!("viewer.dialog.tex_match.apply")),
                    )
                    .clicked()
                {
                    apply = true;
                }
                if ui.button(t!("viewer.dialog.tex_match.skip")).clicked() {
                    cancelled = true;
                }
            });
        });

    // Same-name linking: propagate a selection change to all same-name materials.
    if app.tex.link_same_name {
        if let Some(ref pending) = app.tex.pending_match {
            let prev = &pending.selections;
            for i in 0..mat_info.len() {
                if new_selections[i] != prev[i] {
                    // i-th selection changed -> apply to same-name materials.
                    let changed_name = &mat_info[i].0;
                    let new_val = new_selections[i];
                    for j in 0..mat_info.len() {
                        if j != i && mat_info[j].0 == *changed_name {
                            new_selections[j] = new_val;
                        }
                    }
                }
            }
        }
    }

    // Reflect the `selections` update.
    if let Some(ref mut pending) = app.tex.pending_match {
        pending.selections = new_selections;
    }

    if apply {
        let pending = app
            .tex
            .pending_match
            .take()
            .expect("pending_match confirmed Some by apply flag");
        // Restore the bind groups in preview (final assignment will overwrite them).
        if let Some(ref mut loaded) = app.loaded {
            for (draw_idx, (orig_tex, orig_mmd)) in pending.saved_binds.into_iter() {
                if draw_idx < loaded.gpu_model.draws.len() {
                    loaded.gpu_model.draws[draw_idx].texture_bind_group = orig_tex;
                    loaded.gpu_model.draws[draw_idx].mmd_texture_bind_group = orig_mmd;
                }
            }
        }
        // If a D&D preview coexists, reset it (restore would otherwise misalign the view).
        if let Some(ref mut preview) = app.tex.pending_preview {
            preview.previewed.iter_mut().for_each(|v| *v = false);
        }
        // Copy the assignment info first so we can release borrows.
        let assignments: Vec<(usize, String, Arc<[u8]>)> = pending
            .selections
            .iter()
            .enumerate()
            .filter_map(|(i, sel)| {
                sel.and_then(|tex_idx| {
                    app.tex
                        .pkg_textures
                        .as_ref()
                        .and_then(|pkg| pkg.get(tex_idx))
                        .map(|(name, data)| (pending.mat_indices[i], name.clone(), data.clone()))
                })
            })
            .collect();
        // The dialog already duplicates selections for same-name linking, but
        // when the same pkg texture is applied to a same-name material group
        // we must avoid pushing the `IrTexture` multiple times.
        // -> Deduplicate by (texture_name, material_name) and call
        //    `assign_texture_data_to_material` once per same-name material
        //    group (`link_same_name` handles the lateral propagation).
        let mut applied_pairs: std::collections::HashSet<(String, String)> =
            std::collections::HashSet::new();
        let mut count = 0usize;
        for (mat_idx, tex_name, tex_data) in &assignments {
            let mat_name = app
                .loaded
                .as_ref()
                .map(|l| l.ir.materials[*mat_idx].name.clone())
                .unwrap_or_default();
            if app.tex.link_same_name
                && applied_pairs.contains(&(tex_name.clone(), mat_name.clone()))
            {
                // Same-name texture x same-name material is already
                // propagated via `link_same_name`; siblings'
                // `pkg_assignments` were recorded on the first application.
                continue;
            }
            applied_pairs.insert((tex_name.clone(), mat_name.clone()));
            if !app.assign_texture_data_to_material(*mat_idx, tex_name, tex_data) {
                // Decode / upload failed - do not record in `pkg_assignments`.
                continue;
            }
            app.tex.pkg_assignments.insert(*mat_idx, tex_name.clone());
            // Also record sibling materials propagated via `link_same_name` into `pkg_assignments`.
            // Record same-name siblings into pkg-assignment history (limited to the same MaterialGroup).
            if app.tex.link_same_name {
                if let Some(ref loaded) = app.loaded {
                    for sib in loaded.same_name_siblings(*mat_idx) {
                        app.tex.pkg_assignments.insert(sib, tex_name.clone());
                    }
                }
            }
            count += 1;
        }
        if count > 0 {
            app.convert_message = Some(ConvertMessage::success(
                t!("viewer.dialog.tex_match.applied_msg", count = count).into_owned(),
            ));
        }
    } else if cancelled || !open {
        app.cancel_tex_match_preview();
    }
}

/// Material edit drawer (§A): a floating `egui::Window` opened from the
/// material row's edit button.
///
/// - Returns immediately if `editing_material_index` is `None`.
/// - Per plan TODO-8, pinned via `Id::new("material_editor_window")` to
///   prevent multiple instances.
/// - `default_width(360.0)` + resizable + collapsible.
/// - From Step 2 onward, all §E sections (basic / shade / outline / rim /
///   MatCap / UV anim / emissive / normal / other) are added in stages.
///
/// ## Dirty propagation in the edit path (borrow-checker workaround)
///
/// Inside the closure only set a local `dirty: bool` flag; outside the
/// closure call `app.mark_material_dirty(mat_idx)` and write to
/// `app.material_overrides`. This serializes the in-closure `&mut app` and
/// the out-of-closure `&mut app` in time. IR writes and the
/// `MaterialParamOverride` records are performed simultaneously inside the
/// closure, so the post-reload reapply (§A / A-stance support) keeps the
/// same values consistently.
/// M6 Step 6.5: visual badge displayed at the head of PMX-unsupported
/// sections. Detaches the plain text "(not supported in PMX)" from the
/// section title and emphasizes it with color.
fn pmx_unsupported_badge(ui: &mut egui::Ui) {
    let badge = egui::RichText::new(t!("viewer.pmx_unsupported.badge"))
        .small()
        .color(egui::Color32::from_rgb(230, 175, 90));
    ui.label(badge)
        .on_hover_text(t!("viewer.pmx_unsupported.hover"));
}

/// v0.5.2: per-material-section embedded texture-slot row widget.
///
/// Decomposes the legacy aggregated "Texture slots" section by placing the
/// widget at the head of each section (shade / outline / rim / MatCap /
/// UV anim / emissive / normal), so the texture and the parameters that use
/// it (color / scale / shift, etc.) can be viewed together.
///
/// Layout: `[image button] {label}: {filename or (unassigned)} [x]`.
///
/// - Assigned: clicking the `ImageButton` opens the file dialog (replace).
/// - Unassigned: an X-icon placeholder button. Click to assign.
/// - `x` is shown only when assigned and resets the slot.
///
/// Return value: `(assign_clicked, reset_clicked)` - the caller uses these
/// flags to decide whether to put `slot` into `pending_tex_request` /
/// `pending_tex_clear` (only flags are returned to avoid crossing borrow
/// boundaries).
fn texture_slot_widget(
    ui: &mut egui::Ui,
    label: &str,
    tex_idx_opt: Option<usize>,
    textures: &[crate::intermediate::types::IrTexture],
    ir_thumb_ids: &[Option<egui::TextureId>],
) -> (bool, bool) {
    const THUMB_DISPLAY_PX: f32 = 32.0;
    let thumb_size = egui::vec2(THUMB_DISPLAY_PX, THUMB_DISPLAY_PX);
    let tex_name = tex_idx_opt
        .and_then(|idx| textures.get(idx))
        .map(|t| t.filename.as_str());
    let thumb_id = tex_idx_opt.and_then(|idx| ir_thumb_ids.get(idx).copied().flatten());

    let mut assign_clicked = false;
    let mut reset_clicked = false;

    ui.horizontal(|ui| {
        let clicked = match thumb_id {
            Some(tid) => ui
                .add(
                    egui::ImageButton::new(
                        egui::Image::from_texture((tid, thumb_size)).fit_to_exact_size(thumb_size),
                    )
                    .frame(true),
                )
                .on_hover_text(tex_name.unwrap_or(""))
                .clicked(),
            None => {
                let resp = ui.allocate_response(thumb_size, egui::Sense::click());
                let rect = resp.rect;
                let visuals = ui.style().interact(&resp);
                ui.painter().rect(
                    rect,
                    2.0,
                    visuals.bg_fill,
                    visuals.bg_stroke,
                    egui::StrokeKind::Inside,
                );
                let pad = 6.0;
                let x_stroke = egui::Stroke::new(2.0, visuals.fg_stroke.color);
                ui.painter().line_segment(
                    [
                        rect.left_top() + egui::vec2(pad, pad),
                        rect.right_bottom() - egui::vec2(pad, pad),
                    ],
                    x_stroke,
                );
                ui.painter().line_segment(
                    [
                        rect.right_top() + egui::vec2(-pad, pad),
                        rect.left_bottom() + egui::vec2(pad, -pad),
                    ],
                    x_stroke,
                );
                resp.on_hover_text(t!("viewer.texture_picker.unassigned_hover"))
                    .clicked()
            }
        };
        if clicked {
            assign_clicked = true;
        }
        let unassigned = t!("viewer.texture_picker.unassigned_short");
        ui.label(format!(
            "{}: {}",
            label,
            tex_name.unwrap_or(unassigned.as_ref())
        ));
        if tex_idx_opt.is_some() && ui.small_button("×").clicked() {
            reset_clicked = true;
        }
    });
    (assign_clicked, reset_clicked)
}

/// One-line widget to edit KHR_texture_transform (offset / scale / rotation) (v0.5.4).
/// Designed not to render for slots whose `info` is `None`; the caller pre-checks.
/// rotation is entered / displayed in **degrees** but stored as radians in
/// `IrTextureInfo.rotation`.
/// Returns `true` if any value changed.
fn uv_transform_widget(
    ui: &mut egui::Ui,
    id_salt: &str,
    info: &mut crate::intermediate::types::IrTextureInfo,
) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        ui.add_space(16.0);
        ui.label("UV:");
        ui.label("off");
        if ui
            .add(
                egui::DragValue::new(&mut info.offset.x)
                    .speed(0.001)
                    .fixed_decimals(3)
                    .range(-10.0..=10.0),
            )
            .on_hover_text(format!("{} offset.x", id_salt))
            .changed()
        {
            changed = true;
        }
        if ui
            .add(
                egui::DragValue::new(&mut info.offset.y)
                    .speed(0.001)
                    .fixed_decimals(3)
                    .range(-10.0..=10.0),
            )
            .on_hover_text(format!("{} offset.y", id_salt))
            .changed()
        {
            changed = true;
        }
        ui.label("scale");
        if ui
            .add(
                egui::DragValue::new(&mut info.scale.x)
                    .speed(0.005)
                    .fixed_decimals(3)
                    .range(-100.0..=100.0),
            )
            .on_hover_text(format!("{} scale.x", id_salt))
            .changed()
        {
            changed = true;
        }
        if ui
            .add(
                egui::DragValue::new(&mut info.scale.y)
                    .speed(0.005)
                    .fixed_decimals(3)
                    .range(-100.0..=100.0),
            )
            .on_hover_text(format!("{} scale.y", id_salt))
            .changed()
        {
            changed = true;
        }
        ui.label("rot°");
        let mut deg = info.rotation.to_degrees();
        if ui
            .add(
                egui::DragValue::new(&mut deg)
                    .speed(0.5)
                    .fixed_decimals(1)
                    .range(-720.0..=720.0)
                    .suffix("°"),
            )
            .on_hover_text(format!("{} rotation (degrees)", id_salt))
            .changed()
        {
            info.rotation = deg.to_radians();
            changed = true;
        }
        if ui
            .small_button("⟲")
            .on_hover_text("UV 変形をリセット (offset=0, scale=1, rotation=0)")
            .clicked()
        {
            info.offset = glam::Vec2::ZERO;
            info.scale = glam::Vec2::ONE;
            info.rotation = 0.0;
            changed = true;
        }
    });
    changed
}

/// Read current values from `info` and set all three fields on
/// `pending_override`'s UV entry.
/// Helper to be called immediately after the UI widget returns `changed = true`.
fn record_uv_override(
    target: &mut Option<crate::viewer::app::material_edit::TextureUvOverride>,
    info: &crate::intermediate::types::IrTextureInfo,
) {
    *target = Some(crate::viewer::app::material_edit::TextureUvOverride {
        offset: Some(info.offset.to_array()),
        scale: Some(info.scale.to_array()),
        rotation: Some(info.rotation),
    });
}

pub fn show_material_editor_window(ctx: &egui::Context, app: &mut ViewerApp) {
    use crate::intermediate::types::{MtoonParams, ShaderFamily};

    // v0.5.9: when the panel is absent, force `overlay_h` to 0.
    // If a stale value remains (early return / panel not open), the central
    // viewport applies an unintended FOV compensation and the model stays
    // unintentionally scaled down. Overwrite with the correct value after
    // `show()` when the panel is actually displayed.
    app.material_panel_height_px = 0.0;

    let Some(mat_idx) = app.editing_material_index else {
        return;
    };

    // v0.5.2: sync `ir_thumb_cache` to the model before showing the material
    // edit window. Even if `ir.textures.len()` changes via external paths
    // (model switch, BG load completion, etc.), checking here keeps the
    // texture slot thumbnails in sync.
    app.sync_ir_thumb_cache();

    // Get the material name and total count via immutable borrow first.
    let (mat_name, mat_count) = {
        let Some(loaded) = app.loaded.as_ref() else {
            app.editing_material_index = None;
            return;
        };
        if mat_idx >= loaded.ir.materials.len() {
            // Close when the material count shrinks (e.g. model reload).
            app.editing_material_index = None;
            return;
        }
        (
            loaded.ir.materials[mat_idx].name.clone(),
            loaded.ir.materials.len(),
        )
    };

    // M6 Step 6.3: dirty indicator - prepend `*` if there are edits or texture slot assignments.
    let has_param_override = app
        .material_overrides
        .get(&mat_idx)
        .is_some_and(|o| !o.is_empty());
    let has_slot_texture = app.slot_texture_paths.keys().any(|(mi, _)| *mi == mat_idx);
    let has_base_texture = app.tex.assignments.contains_key(&mat_idx);
    let is_dirty_mat = has_param_override || has_slot_texture || has_base_texture;

    let window_title = if is_dirty_mat {
        t!("viewer.material_edit.title_dirty", name = mat_name).into_owned()
    } else {
        t!("viewer.material_edit.title", name = mat_name).into_owned()
    };
    let mut is_open = true;
    let mut dirty = false;

    // Hold per-section edit deltas in a temporary buffer so we can apply them outside the closure.
    let mut pending_override = super::app::material_edit::MaterialParamOverride::new();

    // Step 4-16b: a texture-pick button click -> open the file dialog outside the closure.
    let mut pending_tex_request: Option<crate::intermediate::types::TextureSlot> = None;
    // Step 4-17: texture slot reset request -> clear the slot outside the closure.
    let mut pending_tex_clear: Option<crate::intermediate::types::TextureSlot> = None;
    // review_024 [P2]: MME category "Reset to estimated" -> clear `mme_kind` outside the closure.
    let mut pending_mme_reset = false;

    // v0.5.2: take a snapshot of `ir_thumb_cache` outside the closure.
    // Disjoint borrows are needed to reference `app.loaded.as_mut()` from the
    // closure in parallel; `TextureId` is `Copy`, so cloning is negligible.
    let ir_thumb_ids: Vec<Option<egui::TextureId>> = app.tex.ir_thumb_cache.clone();

    // v0.5.3: switched from a floating Window to a bottom-docked TopBottomPanel.
    // Pinned right above the shortcut hint bar; resizable and scrollable.
    // The caller (`app/mod.rs`) invokes this function after `status_bar` /
    // `shortcut_hints`, giving the panel stacking order
    // "bottom = status_bar / middle = shortcut_hints / top = this edit panel".
    let panel_frame = egui::Frame::new()
        .fill(DARK_PANEL_BG)
        .stroke(egui::Stroke::new(1.0, DARK_BORDER_COLOR))
        .inner_margin(egui::Margin::same(4));
    let panel_inner = egui::TopBottomPanel::bottom("material_editor_panel")
        .resizable(true)
        .min_height(120.0)
        .default_height(360.0)
        .frame(panel_frame)
        .show(ctx, |ui| {
            // Header row: title + right-edge [x] close button + UV edit button (v0.5.5).
            ui.horizontal(|ui| {
                ui.heading(&window_title);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui
                        .small_button("✕")
                        .on_hover_text(t!("viewer.material_edit.close_panel_tooltip"))
                        .clicked()
                    {
                        is_open = false;
                    }
                    ui.label(
                        egui::RichText::new(format!("mat_idx: {} / {}", mat_idx, mat_count))
                            .small(),
                    );
                    // v0.5.5: open the UV edit window (set the currently edited material active).
                    if ui
                        .small_button(t!("viewer.material_edit.open_uv_edit_button"))
                        .on_hover_text(t!("viewer.material_edit.open_uv_edit_tooltip"))
                        .clicked()
                    {
                        app.uv_edit.active_material = mat_idx;
                        app.uv_edit_window_open = true;
                    }
                });
            });
            ui.separator();
            // Make the entire panel scrollable.
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    let Some(loaded) = app.loaded.as_mut() else {
                        return;
                    };
                    let Some(mat) = loaded.ir.materials.get_mut(mat_idx) else {
                        return;
                    };

                    // ==================== Material name edit (v0.5.3) ====================
                    // The material name can be edited directly. The change is recorded in
                    // `MaterialParamOverride.name` and restored by `apply_to` after reload.
                    // `update_mat_cache` also tracks the UI-side cache.
                    ui.horizontal(|ui| {
                        ui.label(t!("viewer.material_edit.material_name_label"));
                        let resp = ui.add(
                            egui::TextEdit::singleline(&mut mat.name)
                                .desired_width(ui.available_width() - 4.0),
                        );
                        if resp.changed() {
                            pending_override.name = Some(mat.name.clone());
                            dirty = true;
                        }
                    });
                    ui.separator();

                    // ==================== MToon enable checkbox (§G / Step 2-10) ====================
                    //
                    // Toggle that explicitly switches the material's `shader_family`
                    // between `Mtoon` and `Other`. This makes the PMX-conversion
                    // `shader_family` axis decision (Step 2-9) match the UI 1:1.
                    // Unless the user toggles this checkbox, expanding / editing
                    // sections like shade or outline does not change `shader_family`,
                    // and PMX conversion goes through the existing non-MToon path
                    // (review_005 [P1] requirement).
                    //
                    // ON  -> `shader_family = Mtoon` + `mtoon = Some(default)` (preserves existing `mtoon`).
                    // OFF -> `shader_family = Other` + `mtoon = None` (loses MToon-section edits).
                    {
                        let mut mtoon_enabled = mat.shader_family == ShaderFamily::Mtoon;
                        ui.horizontal(|ui| {
                            if ui
                                .checkbox(
                                    &mut mtoon_enabled,
                                    t!("viewer.material_edit.mtoon_enable_checkbox"),
                                )
                                .changed()
                            {
                                if mtoon_enabled {
                                    mat.shader_family = ShaderFamily::Mtoon;
                                    if mat.mtoon.is_none() {
                                        mat.mtoon = Some(MtoonParams::default());
                                    }
                                    pending_override.enable_mtoon = Some(true);
                                } else {
                                    mat.shader_family = ShaderFamily::Other;
                                    mat.mtoon = None;
                                    pending_override.enable_mtoon = Some(false);
                                }
                                dirty = true;
                            }
                            ui.small(match mat.shader_family {
                                ShaderFamily::Mtoon => t!("viewer.material_edit.shader_kind.mtoon"),
                                ShaderFamily::Uts2 => t!("viewer.material_edit.shader_kind.uts2"),
                                ShaderFamily::LilToon => {
                                    t!("viewer.material_edit.shader_kind.liltoon")
                                }
                                ShaderFamily::Poiyomi => {
                                    t!("viewer.material_edit.shader_kind.poiyomi")
                                }
                                ShaderFamily::Other => t!("viewer.material_edit.shader_kind.other"),
                            });
                        });
                        ui.small(t!("viewer.material_edit.mtoon_toggle_warning"));
                    }

                    // ==================== Reset to defaults + preset (§H / §J / Step 5) ====================
                    ui.horizontal(|ui| {
                        // Reset to defaults.
                        // review_019 [P2-2]: also clear `tex.assignments` and
                        // `slot_texture_paths` to prevent texture revival after reload.
                        if mat_idx < app.pristine_materials.len()
                            && ui
                                .button(t!("viewer.material_edit.reset_defaults"))
                                .clicked()
                        {
                            *mat = app.pristine_materials[mat_idx].clone();
                            app.material_overrides.remove(&mat_idx);
                            app.tex.assignments.remove(&mat_idx);
                            app.tex.pkg_assignments.remove(&mat_idx); // review_020 [P2]
                            app.slot_texture_paths.retain(|&(idx, _), _| idx != mat_idx);
                            dirty = true;
                        }

                        // Preset ComboBox + apply button.
                        use super::app::material_presets::MaterialPreset;
                        // egui's ComboBox holds no external state, so compute the label every frame.
                        ui.label("|");
                        let preset_id = ui.id().with("preset_combo");
                        let mut selected_preset: Option<MaterialPreset> = None;
                        egui::ComboBox::from_id_salt(preset_id)
                            .selected_text(t!("viewer.material_edit.preset_select"))
                            .width(140.0)
                            .show_ui(ui, |ui| {
                                for p in MaterialPreset::ALL {
                                    if ui.selectable_label(false, p.label()).clicked() {
                                        selected_preset = Some(p);
                                    }
                                }
                            });
                        if let Some(preset) = selected_preset {
                            let patch = preset.to_override();
                            patch.apply_to(mat);
                            // review_019 [P2-1]: recompute override via `diff_from` (not `merge_from`).
                            // Prevents stale overrides not included in the preset (UV anim, etc.) from accumulating.
                            if mat_idx < app.pristine_materials.len() {
                                let new_override =
                                    super::app::material_edit::MaterialParamOverride::diff_from(
                                        &app.pristine_materials[mat_idx],
                                        mat,
                                    );
                                match new_override {
                                    Some(o) => {
                                        app.material_overrides.insert(mat_idx, o);
                                    }
                                    None => {
                                        app.material_overrides.remove(&mat_idx);
                                    }
                                }
                            }
                            dirty = true;
                            log::info!(
                                "Preset applied: mat[{}] '{}' <- {}",
                                mat_idx,
                                mat.name,
                                preset.label()
                            );
                        }

                        // M6 Step 6.4: material-parameter copy / paste.
                        ui.label("|");
                        if ui
                            .button(t!("viewer.material_edit.copy"))
                            .on_hover_text(t!("viewer.material_edit.copy_tooltip"))
                            .clicked()
                            && mat_idx < app.pristine_materials.len()
                        {
                            let diff = super::app::material_edit::MaterialParamOverride::diff_from(
                                &app.pristine_materials[mat_idx],
                                mat,
                            );
                            app.clipboard_material = diff;
                            log::info!("Material params copied: mat[{}] '{}'", mat_idx, mat.name,);
                        }
                        let can_paste = app.clipboard_material.is_some();
                        if ui
                            .add_enabled(
                                can_paste,
                                egui::Button::new(t!("viewer.material_edit.paste")),
                            )
                            .on_hover_text(t!("viewer.material_edit.paste_tooltip"))
                            .clicked()
                        {
                            if let Some(clip) = app.clipboard_material.clone() {
                                clip.apply_to(mat);
                                if mat_idx < app.pristine_materials.len() {
                                    let new_override =
                                        super::app::material_edit::MaterialParamOverride::diff_from(
                                            &app.pristine_materials[mat_idx],
                                            mat,
                                        );
                                    match new_override {
                                        Some(o) => {
                                            app.material_overrides.insert(mat_idx, o);
                                        }
                                        None => {
                                            app.material_overrides.remove(&mat_idx);
                                        }
                                    }
                                }
                                dirty = true;
                                log::info!(
                                    "Material params pasted: mat[{}] '{}'",
                                    mat_idx,
                                    mat.name,
                                );
                            }
                        }
                    });
                    ui.separator();

                    // ==================== §E-1 Basic section ====================
                    egui::CollapsingHeader::new(t!("viewer.material_edit.section.basic"))
                        .default_open(true)
                        .show(ui, |ui| {
                            // v0.5.2: BaseColor texture thumbnail + assign UI.
                            let (assign, reset) = texture_slot_widget(
                                ui,
                                "BaseColor",
                                mat.texture_index,
                                &loaded.ir.textures,
                                &ir_thumb_ids,
                            );
                            if assign {
                                pending_tex_request =
                                    Some(crate::intermediate::types::TextureSlot::BaseColor);
                            }
                            if reset {
                                pending_tex_clear =
                                    Some(crate::intermediate::types::TextureSlot::BaseColor);
                            }
                            // v0.5.4: BaseColor UV edit (shown only when a texture exists).
                            if let Some(ti) = mat.base_color_tex_info.as_mut() {
                                if uv_transform_widget(ui, "base_color", ti) {
                                    record_uv_override(&mut pending_override.base_color_uv, ti);
                                    dirty = true;
                                }
                            }
                            ui.horizontal(|ui| {
                                ui.label("diffuse:");
                                let mut rgb = [mat.diffuse.x, mat.diffuse.y, mat.diffuse.z];
                                if ui.color_edit_button_rgb(&mut rgb).changed() {
                                    mat.diffuse.x = rgb[0];
                                    mat.diffuse.y = rgb[1];
                                    mat.diffuse.z = rgb[2];
                                    pending_override.diffuse = Some(mat.diffuse);
                                    dirty = true;
                                }
                            });
                        });

                    // ==================== §E-2 Shade section ====================
                    //
                    // **Important (review_005 [P1])**: read via `mat.mtoon()`,
                    // and call `mat.mtoon_mut()` only at the moment the user
                    // actually changes a value to trigger the side effect.
                    // `mat.mtoon_mut()` immediately inserts
                    // `MtoonParams::default()` when `mtoon == None`, which
                    // previously caused "merely expanding the section" to
                    // promote a non-MToon material to MToon and affect the
                    // PMX conversion result (fixed together with the §G axis
                    // switch).
                    egui::CollapsingHeader::new(t!("viewer.material_edit.section.shade"))
                        .default_open(false)
                        .show(ui, |ui| {
                            // v0.5.2: Shade / ShadingShift texture thumbnails + assign UI.
                            {
                                let mp = mat.mtoon();
                                let shade_idx = mp.shade_texture.as_ref().map(|t| t.index);
                                let shift_idx = mp.shading_shift_texture.as_ref().map(|t| t.index);
                                let (a1, r1) = texture_slot_widget(
                                    ui,
                                    "shade texture",
                                    shade_idx,
                                    &loaded.ir.textures,
                                    &ir_thumb_ids,
                                );
                                if a1 {
                                    pending_tex_request = Some(
                                        crate::intermediate::types::TextureSlot::ShadeMultiply,
                                    );
                                }
                                if r1 {
                                    pending_tex_clear = Some(
                                        crate::intermediate::types::TextureSlot::ShadeMultiply,
                                    );
                                }
                                let (a2, r2) = texture_slot_widget(
                                    ui,
                                    "shading_shift texture",
                                    shift_idx,
                                    &loaded.ir.textures,
                                    &ir_thumb_ids,
                                );
                                if a2 {
                                    pending_tex_request =
                                        Some(crate::intermediate::types::TextureSlot::ShadingShift);
                                }
                                if r2 {
                                    pending_tex_clear =
                                        Some(crate::intermediate::types::TextureSlot::ShadingShift);
                                }
                            }
                            // v0.5.4: Shade / ShadingShift UV edit (only if mtoon exists and slot is assigned).
                            if let Some(mp) = mat.mtoon.as_mut() {
                                if let Some(ti) = mp.shade_texture.as_mut() {
                                    if uv_transform_widget(ui, "shade", ti) {
                                        record_uv_override(&mut pending_override.shade_uv, ti);
                                        dirty = true;
                                    }
                                }
                                if let Some(ti) = mp.shading_shift_texture.as_mut() {
                                    if uv_transform_widget(ui, "shading_shift", ti) {
                                        record_uv_override(
                                            &mut pending_override.shading_shift_uv,
                                            ti,
                                        );
                                        dirty = true;
                                    }
                                }
                            }

                            // Read: `mat.mtoon()` references defaults, so it has no side effects.
                            let (
                                mut shade_color_rgb,
                                mut shading_toony,
                                mut shading_shift,
                                mut gi_eq,
                            ) = {
                                let mp = mat.mtoon();
                                (
                                    mp.shade_color.unwrap_or(glam::Vec3::ZERO).to_array(),
                                    mp.shading_toony_factor,
                                    mp.shading_shift_factor,
                                    mp.gi_equalization_factor,
                                )
                            };

                            // Widgets (do not touch the IR here).
                            let mut shade_changed = false;
                            let mut toony_changed = false;
                            let mut shift_changed = false;
                            let mut gi_changed = false;

                            ui.horizontal(|ui| {
                                ui.label("shade_color:");
                                if ui.color_edit_button_rgb(&mut shade_color_rgb).changed() {
                                    shade_changed = true;
                                }
                            });
                            ui.horizontal(|ui| {
                                ui.label("shading_toony:");
                                if ui
                                    .add(
                                        egui::Slider::new(&mut shading_toony, 0.0..=1.0)
                                            .fixed_decimals(3),
                                    )
                                    .changed()
                                {
                                    toony_changed = true;
                                }
                            });
                            ui.horizontal(|ui| {
                                ui.label("shading_shift:");
                                if ui
                                    .add(
                                        egui::Slider::new(&mut shading_shift, -1.0..=1.0)
                                            .fixed_decimals(3),
                                    )
                                    .changed()
                                {
                                    shift_changed = true;
                                }
                            });
                            ui.horizontal(|ui| {
                                ui.label("gi_equalization:");
                                if ui
                                    .add(egui::Slider::new(&mut gi_eq, 0.0..=1.0).fixed_decimals(3))
                                    .changed()
                                {
                                    gi_changed = true;
                                }
                            });

                            // Call `mat.mtoon_mut()` only on a change to write the value.
                            // This prevents `mtoon` from being inserted just by expanding the section on a non-MToon material.
                            if shade_changed || toony_changed || shift_changed || gi_changed {
                                let mp = mat.mtoon_mut();
                                if shade_changed {
                                    let v = glam::Vec3::from_array(shade_color_rgb);
                                    mp.shade_color = Some(v);
                                    pending_override.shade_color = Some(v);
                                }
                                if toony_changed {
                                    mp.shading_toony_factor = shading_toony;
                                    pending_override.shading_toony_factor = Some(shading_toony);
                                }
                                if shift_changed {
                                    mp.shading_shift_factor = shading_shift;
                                    pending_override.shading_shift_factor = Some(shading_shift);
                                }
                                if gi_changed {
                                    mp.gi_equalization_factor = gi_eq;
                                    pending_override.gi_equalization_factor = Some(gi_eq);
                                }
                                dirty = true;
                            }

                            if !matches!(
                                mat.shader_family,
                                ShaderFamily::Mtoon
                                    | ShaderFamily::Uts2
                                    | ShaderFamily::LilToon
                                    | ShaderFamily::Poiyomi
                            ) {
                                ui.small(t!("viewer.material_edit.shade_mtoon_note"));
                            }
                        });

                    // ==================== §E-3 Outline section ====================
                    //
                    // - `edge_color` / `edge_size` are direct `IrMaterial` fields (non-MToon).
                    // - `outline_width_mode` / `outline_width_factor` /
                    //   `outline_lighting_mix` are `MtoonParams` fields, so
                    //   read via `mat.mtoon()` and call `mat.mtoon_mut()`
                    //   only when the value actually changes (containment
                    //   pattern from review_005 [P1]).
                    egui::CollapsingHeader::new(t!("viewer.material_edit.section.outline"))
                        .default_open(false)
                        .show(ui, |ui| {
                            use crate::intermediate::types::OutlineWidthMode;

                            // v0.5.2: OutlineWidth texture thumbnail + assign UI.
                            {
                                let outline_idx =
                                    mat.mtoon().outline_width_texture.as_ref().map(|t| t.index);
                                let (a, r) = texture_slot_widget(
                                    ui,
                                    "outline_width texture",
                                    outline_idx,
                                    &loaded.ir.textures,
                                    &ir_thumb_ids,
                                );
                                if a {
                                    pending_tex_request =
                                        Some(crate::intermediate::types::TextureSlot::OutlineWidth);
                                }
                                if r {
                                    pending_tex_clear =
                                        Some(crate::intermediate::types::TextureSlot::OutlineWidth);
                                }
                            }
                            // v0.5.4: OutlineWidth UV edit.
                            if let Some(mp) = mat.mtoon.as_mut() {
                                if let Some(ti) = mp.outline_width_texture.as_mut() {
                                    if uv_transform_widget(ui, "outline_width", ti) {
                                        record_uv_override(
                                            &mut pending_override.outline_width_uv,
                                            ti,
                                        );
                                        dirty = true;
                                    }
                                }
                            }

                            // edge_color: direct on `IrMaterial` (RGBA).
                            ui.horizontal(|ui| {
                                ui.label("edge_color:");
                                let mut rgba = mat.edge_color.to_array();
                                if ui.color_edit_button_rgba_unmultiplied(&mut rgba).changed() {
                                    mat.edge_color = glam::Vec4::from_array(rgba);
                                    pending_override.edge_color = Some(mat.edge_color);
                                    dirty = true;
                                }
                            });

                            // edge_size: direct on `IrMaterial` (for MMD edges; clamped to 1.0 on PMX export).
                            ui.horizontal(|ui| {
                                ui.label("edge_size:");
                                if ui
                                    .add(
                                        egui::Slider::new(&mut mat.edge_size, 0.0..=2.0)
                                            .fixed_decimals(3),
                                    )
                                    .changed()
                                {
                                    pending_override.edge_size = Some(mat.edge_size);
                                    dirty = true;
                                }
                            });

                            // MToon outline-related (read via `mat.mtoon()` - no side effects).
                            let (mut width_mode, mut width_factor, mut lighting_mix) = {
                                let mp = mat.mtoon();
                                (
                                    mp.outline_width_mode,
                                    mp.outline_width_factor,
                                    mp.outline_lighting_mix,
                                )
                            };
                            let mut width_mode_changed = false;
                            let mut width_factor_changed = false;
                            let mut lighting_mix_changed = false;

                            // outline_width_mode: ComboBox.
                            ui.horizontal(|ui| {
                                ui.label("width_mode:");
                                let label_of = |m: OutlineWidthMode| match m {
                                    OutlineWidthMode::None => "None",
                                    OutlineWidthMode::WorldCoordinates => "World",
                                    OutlineWidthMode::ScreenCoordinates => "Screen",
                                };
                                egui::ComboBox::from_id_salt("outline_width_mode_combo")
                                    .selected_text(label_of(width_mode))
                                    .show_ui(ui, |ui| {
                                        for m in [
                                            OutlineWidthMode::None,
                                            OutlineWidthMode::WorldCoordinates,
                                            OutlineWidthMode::ScreenCoordinates,
                                        ] {
                                            if ui
                                                .selectable_label(width_mode == m, label_of(m))
                                                .clicked()
                                                && width_mode != m
                                            {
                                                width_mode = m;
                                                width_mode_changed = true;
                                            }
                                        }
                                    });
                            });

                            // outline_width_factor: DragValue (world=meters, screen=ratio).
                            ui.horizontal(|ui| {
                                ui.label("width_factor:");
                                if ui
                                    .add(
                                        egui::DragValue::new(&mut width_factor)
                                            .speed(0.001)
                                            .range(0.0..=10.0),
                                    )
                                    .changed()
                                {
                                    width_factor_changed = true;
                                }
                            });

                            // outline_lighting_mix: Slider 0.0..=1.0.
                            ui.horizontal(|ui| {
                                ui.label("lighting_mix:");
                                if ui
                                    .add(
                                        egui::Slider::new(&mut lighting_mix, 0.0..=1.0)
                                            .fixed_decimals(3),
                                    )
                                    .changed()
                                {
                                    lighting_mix_changed = true;
                                }
                            });

                            // MToon: call `mtoon_mut()` only on change.
                            if width_mode_changed || width_factor_changed || lighting_mix_changed {
                                let mp = mat.mtoon_mut();
                                if width_mode_changed {
                                    mp.outline_width_mode = width_mode;
                                    pending_override.outline_width_mode = Some(width_mode);
                                }
                                if width_factor_changed {
                                    mp.outline_width_factor = width_factor;
                                    pending_override.outline_width_factor = Some(width_factor);
                                }
                                if lighting_mix_changed {
                                    mp.outline_lighting_mix = lighting_mix;
                                    pending_override.outline_lighting_mix = Some(lighting_mix);
                                }
                                dirty = true;
                            }
                        });

                    // ==================== §E-4 Rim section ====================
                    //
                    // All fields are `MtoonParams` fields, so read via
                    // `mat.mtoon()` and call `mat.mtoon_mut()` only on change.
                    egui::CollapsingHeader::new(t!("viewer.material_edit.section.rim"))
                        .default_open(false)
                        .show(ui, |ui| {
                            pmx_unsupported_badge(ui);
                            // v0.5.2: RimMultiply texture thumbnail + assign UI.
                            {
                                let rim_idx =
                                    mat.mtoon().rim_multiply_texture.as_ref().map(|t| t.index);
                                let (a, r) = texture_slot_widget(
                                    ui,
                                    "rim_multiply texture",
                                    rim_idx,
                                    &loaded.ir.textures,
                                    &ir_thumb_ids,
                                );
                                if a {
                                    pending_tex_request =
                                        Some(crate::intermediate::types::TextureSlot::RimMultiply);
                                }
                                if r {
                                    pending_tex_clear =
                                        Some(crate::intermediate::types::TextureSlot::RimMultiply);
                                }
                            }
                            // v0.5.4: RimMultiply UV edit.
                            if let Some(mp) = mat.mtoon.as_mut() {
                                if let Some(ti) = mp.rim_multiply_texture.as_mut() {
                                    if uv_transform_widget(ui, "rim_multiply", ti) {
                                        record_uv_override(
                                            &mut pending_override.rim_multiply_uv,
                                            ti,
                                        );
                                        dirty = true;
                                    }
                                }
                            }
                            let (mut rim_rgb, mut fresnel_power, mut rim_lift, mut rim_mix) = {
                                let mp = mat.mtoon();
                                (
                                    mp.parametric_rim_color.to_array(),
                                    mp.parametric_rim_fresnel_power,
                                    mp.parametric_rim_lift,
                                    mp.rim_lighting_mix,
                                )
                            };
                            let mut rim_color_changed = false;
                            let mut fresnel_changed = false;
                            let mut lift_changed = false;
                            let mut mix_changed = false;

                            ui.horizontal(|ui| {
                                ui.label("rim_color:");
                                if ui.color_edit_button_rgb(&mut rim_rgb).changed() {
                                    rim_color_changed = true;
                                }
                            });
                            ui.horizontal(|ui| {
                                ui.label("fresnel_power:");
                                if ui
                                    .add(
                                        egui::DragValue::new(&mut fresnel_power)
                                            .speed(0.05)
                                            .range(0.0..=100.0),
                                    )
                                    .changed()
                                {
                                    fresnel_changed = true;
                                }
                            });
                            ui.horizontal(|ui| {
                                ui.label("rim_lift:");
                                if ui
                                    .add(
                                        egui::Slider::new(&mut rim_lift, 0.0..=1.0)
                                            .fixed_decimals(3),
                                    )
                                    .changed()
                                {
                                    lift_changed = true;
                                }
                            });
                            ui.horizontal(|ui| {
                                ui.label("lighting_mix:");
                                if ui
                                    .add(
                                        egui::Slider::new(&mut rim_mix, 0.0..=1.0)
                                            .fixed_decimals(3),
                                    )
                                    .changed()
                                {
                                    mix_changed = true;
                                }
                            });

                            if rim_color_changed || fresnel_changed || lift_changed || mix_changed {
                                let mp = mat.mtoon_mut();
                                if rim_color_changed {
                                    let v = glam::Vec3::from_array(rim_rgb);
                                    mp.parametric_rim_color = v;
                                    pending_override.parametric_rim_color = Some(v);
                                }
                                if fresnel_changed {
                                    mp.parametric_rim_fresnel_power = fresnel_power;
                                    pending_override.parametric_rim_fresnel_power =
                                        Some(fresnel_power);
                                }
                                if lift_changed {
                                    mp.parametric_rim_lift = rim_lift;
                                    pending_override.parametric_rim_lift = Some(rim_lift);
                                }
                                if mix_changed {
                                    mp.rim_lighting_mix = rim_mix;
                                    pending_override.rim_lighting_mix = Some(rim_mix);
                                }
                                dirty = true;
                            }
                        });

                    // ==================== §E-5 MatCap section ====================
                    egui::CollapsingHeader::new("MatCap")
                        .default_open(false)
                        .show(ui, |ui| {
                            pmx_unsupported_badge(ui);
                            // v0.5.2: Matcap texture thumbnail + assign UI.
                            {
                                let matcap_idx =
                                    mat.mtoon().matcap_texture.as_ref().map(|t| t.index);
                                let (a, r) = texture_slot_widget(
                                    ui,
                                    "matcap texture",
                                    matcap_idx,
                                    &loaded.ir.textures,
                                    &ir_thumb_ids,
                                );
                                if a {
                                    pending_tex_request =
                                        Some(crate::intermediate::types::TextureSlot::Matcap);
                                }
                                if r {
                                    pending_tex_clear =
                                        Some(crate::intermediate::types::TextureSlot::Matcap);
                                }
                            }
                            // v0.5.4: Matcap UV edit.
                            if let Some(mp) = mat.mtoon.as_mut() {
                                if let Some(ti) = mp.matcap_texture.as_mut() {
                                    if uv_transform_widget(ui, "matcap", ti) {
                                        record_uv_override(&mut pending_override.matcap_uv, ti);
                                        dirty = true;
                                    }
                                }
                            }
                            let mut matcap_rgb = mat.mtoon().matcap_factor.to_array();
                            let mut matcap_changed = false;

                            ui.horizontal(|ui| {
                                ui.label("matcap_factor:");
                                if ui.color_edit_button_rgb(&mut matcap_rgb).changed() {
                                    matcap_changed = true;
                                }
                            });

                            if matcap_changed {
                                let v = glam::Vec3::from_array(matcap_rgb);
                                mat.mtoon_mut().matcap_factor = v;
                                pending_override.matcap_factor = Some(v);
                                dirty = true;
                            }
                        });

                    // ==================== §E-6 UV anim section ====================
                    egui::CollapsingHeader::new("UV anim")
                        .default_open(false)
                        .show(ui, |ui| {
                            pmx_unsupported_badge(ui);
                            // v0.5.2: UvAnimMask texture thumbnail + assign UI.
                            {
                                let mask_idx = mat
                                    .mtoon()
                                    .uv_animation_mask_texture
                                    .as_ref()
                                    .map(|t| t.index);
                                let (a, r) = texture_slot_widget(
                                    ui,
                                    "uv_animation_mask texture",
                                    mask_idx,
                                    &loaded.ir.textures,
                                    &ir_thumb_ids,
                                );
                                if a {
                                    pending_tex_request =
                                        Some(crate::intermediate::types::TextureSlot::UvAnimMask);
                                }
                                if r {
                                    pending_tex_clear =
                                        Some(crate::intermediate::types::TextureSlot::UvAnimMask);
                                }
                            }
                            // v0.5.4: UvAnimMask UV edit (can be combined with scroll / rotation dynamic animation).
                            if let Some(mp) = mat.mtoon.as_mut() {
                                if let Some(ti) = mp.uv_animation_mask_texture.as_mut() {
                                    if uv_transform_widget(ui, "uv_anim_mask", ti) {
                                        record_uv_override(
                                            &mut pending_override.uv_animation_mask_uv,
                                            ti,
                                        );
                                        dirty = true;
                                    }
                                }
                            }
                            let (mut scroll_x, mut scroll_y, mut rotation) = {
                                let mp = mat.mtoon();
                                (
                                    mp.uv_animation_scroll_x_speed,
                                    mp.uv_animation_scroll_y_speed,
                                    mp.uv_animation_rotation_speed,
                                )
                            };
                            let mut scroll_x_changed = false;
                            let mut scroll_y_changed = false;
                            let mut rotation_changed = false;

                            ui.horizontal(|ui| {
                                ui.label("scroll_x_speed:");
                                if ui
                                    .add(
                                        egui::DragValue::new(&mut scroll_x)
                                            .speed(0.01)
                                            .range(-100.0..=100.0),
                                    )
                                    .changed()
                                {
                                    scroll_x_changed = true;
                                }
                            });
                            ui.horizontal(|ui| {
                                ui.label("scroll_y_speed:");
                                if ui
                                    .add(
                                        egui::DragValue::new(&mut scroll_y)
                                            .speed(0.01)
                                            .range(-100.0..=100.0),
                                    )
                                    .changed()
                                {
                                    scroll_y_changed = true;
                                }
                            });
                            ui.horizontal(|ui| {
                                ui.label("rotation_speed:");
                                if ui
                                    .add(
                                        egui::DragValue::new(&mut rotation)
                                            .speed(0.01)
                                            .range(-100.0..=100.0),
                                    )
                                    .changed()
                                {
                                    rotation_changed = true;
                                }
                            });

                            if scroll_x_changed || scroll_y_changed || rotation_changed {
                                let mp = mat.mtoon_mut();
                                if scroll_x_changed {
                                    mp.uv_animation_scroll_x_speed = scroll_x;
                                    pending_override.uv_animation_scroll_x_speed = Some(scroll_x);
                                }
                                if scroll_y_changed {
                                    mp.uv_animation_scroll_y_speed = scroll_y;
                                    pending_override.uv_animation_scroll_y_speed = Some(scroll_y);
                                }
                                if rotation_changed {
                                    mp.uv_animation_rotation_speed = rotation;
                                    pending_override.uv_animation_rotation_speed = Some(rotation);
                                }
                                dirty = true;
                            }
                        });

                    // ==================== §E-7 Emissive / Normal section ====================
                    //
                    // Both are direct `IrMaterial` fields, so the MToon
                    // read-write separation isn't needed.
                    //
                    // **review_006 [P2]**: `emissive_factor` must keep HDR
                    // values (> 1.0) (VRM's
                    // `KHR_materials_emissive_strength` multiplies an
                    // intensity factor). However,
                    // `color_edit_button_rgb` internally clamps to a 0..1
                    // linear `Rgba`, so merely touching an existing HDR
                    // emissive once would weaken the emission.
                    //
                    // **Adopted design**: split the UI into color and
                    // intensity.
                    // - Color (0..1 `base_color`): pick intuitively with `ColorPicker`.
                    // - Intensity (0..100 multiplier): handle HDR range via `DragValue`.
                    // - Internally recompute `emissive_factor = base_color * intensity`.
                    egui::CollapsingHeader::new(t!("viewer.material_edit.section.emissive_normal"))
                        .default_open(false)
                        .show(ui, |ui| {
                            // v0.5.2: Emissive / Normal texture thumbnails + assign UI.
                            {
                                let emissive_idx = mat.emissive_texture.as_ref().map(|t| t.index);
                                let normal_idx = mat.normal_texture.as_ref().map(|t| t.index);
                                let (a1, r1) = texture_slot_widget(
                                    ui,
                                    "emissive texture",
                                    emissive_idx,
                                    &loaded.ir.textures,
                                    &ir_thumb_ids,
                                );
                                if a1 {
                                    pending_tex_request =
                                        Some(crate::intermediate::types::TextureSlot::Emissive);
                                }
                                if r1 {
                                    pending_tex_clear =
                                        Some(crate::intermediate::types::TextureSlot::Emissive);
                                }
                                let (a2, r2) = texture_slot_widget(
                                    ui,
                                    "normal texture",
                                    normal_idx,
                                    &loaded.ir.textures,
                                    &ir_thumb_ids,
                                );
                                if a2 {
                                    pending_tex_request =
                                        Some(crate::intermediate::types::TextureSlot::Normal);
                                }
                                if r2 {
                                    pending_tex_clear =
                                        Some(crate::intermediate::types::TextureSlot::Normal);
                                }
                            }
                            // v0.5.4: Emissive / Normal UV edit (slots directly on `IrMaterial`).
                            if let Some(ti) = mat.emissive_texture.as_mut() {
                                if uv_transform_widget(ui, "emissive", ti) {
                                    record_uv_override(&mut pending_override.emissive_uv, ti);
                                    dirty = true;
                                }
                            }
                            if let Some(ti) = mat.normal_texture.as_mut() {
                                if uv_transform_widget(ui, "normal", ti) {
                                    record_uv_override(&mut pending_override.normal_uv, ti);
                                    dirty = true;
                                }
                            }

                            // Decompose the current `emissive_factor` into (base_color, intensity).
                            let current = mat.emissive_factor;
                            let intensity = current.max_element().max(0.0);
                            let base_rgb_vec = if intensity > 1e-6 {
                                current / intensity
                            } else {
                                glam::Vec3::ZERO
                            };
                            let mut base_rgb = base_rgb_vec.to_array();
                            let mut intensity_edit = intensity;
                            let mut color_changed = false;
                            let mut intensity_changed = false;

                            ui.horizontal(|ui| {
                                ui.label("emissive:");
                                if ui.color_edit_button_rgb(&mut base_rgb).changed() {
                                    color_changed = true;
                                }
                                ui.label(t!("viewer.material_edit.intensity_label"));
                                if ui
                                    .add(
                                        egui::DragValue::new(&mut intensity_edit)
                                            .speed(0.05)
                                            .range(0.0..=100.0),
                                    )
                                    .changed()
                                {
                                    intensity_changed = true;
                                }
                            });
                            ui.small(t!("viewer.material_edit.intensity_note"));

                            if color_changed || intensity_changed {
                                // If only the color changes while intensity stays 0, the result becomes [0,0,0].
                                // So when "color changed AND intensity == 0", fall back to intensity = 1.0.
                                let effective_intensity = if color_changed && intensity_edit <= 1e-6
                                {
                                    1.0
                                } else {
                                    intensity_edit
                                };
                                let new_v = glam::Vec3::from_array(base_rgb) * effective_intensity;
                                mat.emissive_factor = new_v;
                                pending_override.emissive_factor = Some(new_v);
                                dirty = true;
                            }

                            // normal_texture_scale: f32 (default 1.0).
                            ui.horizontal(|ui| {
                                ui.label("normal_scale:");
                                if ui
                                    .add(
                                        egui::DragValue::new(&mut mat.normal_texture_scale)
                                            .speed(0.01)
                                            .range(0.0..=10.0),
                                    )
                                    .changed()
                                {
                                    pending_override.normal_texture_scale =
                                        Some(mat.normal_texture_scale);
                                    dirty = true;
                                }
                            });
                        });

                    // ==================== MMD textures (Sphere / Toon) ====================
                    //
                    // v0.5.2: Generic slots like BaseColor / Emissive / Normal
                    // are integrated directly into each parameter section
                    // (Basic / Shade / Outline / Rim / MatCap / UV anim /
                    // Emissive+Normal), so only MMD/PMX-specific Sphere /
                    // Toon remain here.
                    egui::CollapsingHeader::new(t!("viewer.material_edit.section.mmd_texture"))
                        .default_open(false)
                        .show(ui, |ui| {
                            use crate::intermediate::types::TextureSlot;
                            let (a1, r1) = texture_slot_widget(
                                ui,
                                "sphere texture",
                                mat.sphere_texture_index,
                                &loaded.ir.textures,
                                &ir_thumb_ids,
                            );
                            if a1 {
                                pending_tex_request = Some(TextureSlot::Sphere);
                            }
                            if r1 {
                                pending_tex_clear = Some(TextureSlot::Sphere);
                            }
                            let (a2, r2) = texture_slot_widget(
                                ui,
                                "toon texture",
                                mat.toon_texture_index,
                                &loaded.ir.textures,
                                &ir_thumb_ids,
                            );
                            if a2 {
                                pending_tex_request = Some(TextureSlot::Toon);
                            }
                            if r2 {
                                pending_tex_clear = Some(TextureSlot::Toon);
                            }
                        });

                    // ==================== §E-8 Other section ====================
                    //
                    // - `alpha_mode` / `alpha_cutoff` / `cull_mode` are direct on `IrMaterial`.
                    // - `render_queue_offset` is a `MtoonParams` field (read-write separation pattern).
                    egui::CollapsingHeader::new(t!("viewer.material_edit.section.other"))
                        .default_open(false)
                        .show(ui, |ui| {
                            use crate::intermediate::types::{AlphaMode, CullMode};

                            // alpha_mode: ComboBox (Opaque / Mask / BlendWithZWrite / Blend).
                            ui.horizontal(|ui| {
                                ui.label("alpha_mode:");
                                let label_of = |m: AlphaMode| match m {
                                    AlphaMode::Opaque => "Opaque",
                                    AlphaMode::Mask => "Mask",
                                    AlphaMode::BlendWithZWrite => "BlendWithZWrite",
                                    AlphaMode::Blend => "Blend",
                                };
                                egui::ComboBox::from_id_salt("alpha_mode_combo")
                                    .selected_text(label_of(mat.alpha_mode))
                                    .show_ui(ui, |ui| {
                                        for m in [
                                            AlphaMode::Opaque,
                                            AlphaMode::Mask,
                                            AlphaMode::BlendWithZWrite,
                                            AlphaMode::Blend,
                                        ] {
                                            if ui
                                                .selectable_label(mat.alpha_mode == m, label_of(m))
                                                .clicked()
                                                && mat.alpha_mode != m
                                            {
                                                mat.alpha_mode = m;
                                                pending_override.alpha_mode = Some(m);
                                                dirty = true;
                                            }
                                        }
                                    });
                            });

                            // alpha_cutoff: Slider 0.0..=1.0 (effective only in Mask mode).
                            ui.horizontal(|ui| {
                                ui.label("alpha_cutoff:");
                                if ui
                                    .add(
                                        egui::Slider::new(&mut mat.alpha_cutoff, 0.0..=1.0)
                                            .fixed_decimals(3),
                                    )
                                    .changed()
                                {
                                    pending_override.alpha_cutoff = Some(mat.alpha_cutoff);
                                    dirty = true;
                                }
                            });

                            // cull_mode: ComboBox (Back / None / Front).
                            ui.horizontal(|ui| {
                                ui.label("cull_mode:");
                                let label_of = |m: CullMode| match m {
                                    CullMode::Back => "Back (single-sided)",
                                    CullMode::None => "None (double-sided)",
                                    CullMode::Front => "Front",
                                };
                                egui::ComboBox::from_id_salt("cull_mode_combo")
                                    .selected_text(label_of(mat.cull_mode))
                                    .show_ui(ui, |ui| {
                                        for m in [CullMode::Back, CullMode::None, CullMode::Front] {
                                            if ui
                                                .selectable_label(mat.cull_mode == m, label_of(m))
                                                .clicked()
                                                && mat.cull_mode != m
                                            {
                                                mat.cull_mode = m;
                                                pending_override.cull_mode = Some(m);
                                                dirty = true;
                                            }
                                        }
                                    });
                            });

                            // render_queue_offset: `MtoonParams` field (for sorting within BLEND).
                            let mut rqo = mat.mtoon().render_queue_offset;
                            let mut rqo_changed = false;
                            ui.horizontal(|ui| {
                                ui.label("render_queue_offset:");
                                if ui
                                    .add(egui::DragValue::new(&mut rqo).speed(1).range(-9..=9))
                                    .changed()
                                {
                                    rqo_changed = true;
                                }
                            });
                            if rqo_changed {
                                mat.mtoon_mut().render_queue_offset = rqo;
                                pending_override.render_queue_offset = Some(rqo);
                                dirty = true;
                            }
                        });

                    // ==================== MME export preview (§K.3 / Step 6) ====================
                    egui::CollapsingHeader::new("MME export (ray-mmd)")
                        .default_open(false)
                        .show(ui, |ui| {
                            use crate::convert::mme::ray_mmd::{
                                guess_ray_mmd_kind, RayMmdMaterialKind,
                            };

                            let estimated = guess_ray_mmd_kind(mat);
                            let current_override = app
                                .material_overrides
                                .get(&mat_idx)
                                .and_then(|o| o.mme_kind);
                            let current = current_override.unwrap_or(estimated);

                            ui.horizontal(|ui| {
                                ui.label(t!("viewer.material_edit.category_label"));
                                egui::ComboBox::from_id_salt("mme_kind_combo")
                                    .selected_text(current.label())
                                    .show_ui(ui, |ui| {
                                        for kind in RayMmdMaterialKind::ALL {
                                            if ui
                                                .selectable_label(current == kind, kind.label())
                                                .clicked()
                                                && current != kind
                                            {
                                                pending_override.mme_kind = Some(kind);
                                                dirty = true;
                                            }
                                        }
                                    });
                                if current_override.is_some() && current != estimated {
                                    ui.small(t!("viewer.material_edit.manual_override_note"));
                                    if ui
                                        .small_button(t!("viewer.material_edit.reset_estimate"))
                                        .clicked()
                                    {
                                        // review_024 [P2]: `merge_from` only overwrites when value is `Some`,
                                        // so to clear `mme_kind` we set it to `None` outside the closure.
                                        pending_mme_reset = true;
                                    }
                                }
                            });

                            ui.small(t!(
                                "viewer.material_edit.estimated",
                                kind = estimated.label()
                            ));

                            // Show the ray-mmd root.
                            let root_label =
                                app.app_config.ray_mmd_root.as_deref().unwrap_or(".\\");
                            ui.small(format!("ray-mmd: {}", root_label));
                        });
                }); // ScrollArea::vertical().show
        });

    // v0.5.9: record the panel's pixel height
    // (egui logical pt * pixels_per_point); used for FOV compensation in the
    // central viewport. Keeps the model's on-screen pixel size constant
    // across panel open/close and resize.
    app.material_panel_height_px = panel_inner.response.rect.height() * ctx.pixels_per_point();

    if dirty {
        // Merge the edit delta into `material_overrides` (overwrites existing entries).
        // Even when reload (A-stance conversion etc.) rebuilds the IR, `apply_to()`
        // restores values automatically.
        // `merge_from` handles all 24 fields via macro in one shot, so this dirty path
        // stays a single line as new sections are added.
        let name_changed = pending_override.name.is_some();
        let entry = app.material_overrides.entry(mat_idx).or_default();
        entry.merge_from(&pending_override);
        app.mark_material_dirty(mat_idx);
        // v0.5.3: when the material name changes, also rebuild the UI cache so the
        // side panel's material list reflects it immediately.
        if name_changed {
            app.update_mat_cache();
        }
    }

    // review_024 [P2]: MME category "Reset to estimated".
    if pending_mme_reset {
        if let Some(entry) = app.material_overrides.get_mut(&mat_idx) {
            entry.mme_kind = None;
        }
    }

    // Step 4-17: texture-slot reset `x`.
    if let Some(slot) = pending_tex_clear {
        // review_017 [P2-2]: also remove from `slot_texture_paths` to prevent revival on reload.
        app.slot_texture_paths.remove(&(mat_idx, slot));
        if let Some(loaded) = app.loaded.as_mut() {
            if let Some(mat) = loaded.ir.materials.get_mut(mat_idx) {
                use crate::intermediate::types::TextureSlot;
                match slot {
                    TextureSlot::Emissive => mat.emissive_texture = None,
                    TextureSlot::Normal => mat.normal_texture = None,
                    TextureSlot::ShadeMultiply => {
                        if let Some(ref mut mp) = mat.mtoon {
                            mp.shade_texture = None;
                        }
                    }
                    TextureSlot::ShadingShift => {
                        if let Some(ref mut mp) = mat.mtoon {
                            mp.shading_shift_texture = None;
                        }
                    }
                    TextureSlot::RimMultiply => {
                        if let Some(ref mut mp) = mat.mtoon {
                            mp.rim_multiply_texture = None;
                        }
                    }
                    TextureSlot::OutlineWidth => {
                        if let Some(ref mut mp) = mat.mtoon {
                            mp.outline_width_texture = None;
                        }
                    }
                    TextureSlot::Matcap => {
                        if let Some(ref mut mp) = mat.mtoon {
                            mp.matcap_texture = None;
                        }
                    }
                    TextureSlot::UvAnimMask => {
                        if let Some(ref mut mp) = mat.mtoon {
                            mp.uv_animation_mask_texture = None;
                        }
                    }
                    TextureSlot::Sphere => {
                        mat.sphere_texture_index = None;
                    }
                    TextureSlot::Toon => {
                        mat.toon_texture_index = None;
                    }
                    TextureSlot::BaseColor => {}
                }
            }
        }
        app.mark_material_dirty(mat_idx);
    }

    // Step 4-16b: a texture-pick button click -> open the file dialog.
    // Handled outside the closure, so no `app` borrow conflict.
    if let Some(slot) = pending_tex_request {
        if app.tex.pending_file_dialog.is_none() {
            let (tx, rx) = std::sync::mpsc::channel();
            let repaint = ctx.clone();
            let dir = app.tex.last_dir.clone();
            let title = t!("viewer.material_edit.tex_picker_title").into_owned();
            std::thread::spawn(move || {
                let mut dialog = rfd::FileDialog::new()
                    .set_title(title)
                    .add_filter("Image", &["png", "jpg", "jpeg", "tga", "bmp", "psd", "dds"]);
                if let Some(ref d) = dir {
                    dialog = dialog.set_directory(d);
                }
                let _ = tx.send(dialog.pick_file());
                repaint.request_repaint();
            });
            app.tex.pending_file_dialog = Some((mat_idx, slot, rx));
        }
    }

    if !is_open {
        app.editing_material_index = None;
    }
}

/// Material-selection dialog at texture D&D (multi-select + real-time preview).
pub fn show_texture_drop_dialog(ctx: &egui::Context, app: &mut ViewerApp) {
    let Some(ref mut preview) = app.tex.pending_preview else {
        return;
    };
    let Some(ref loaded) = app.loaded else {
        app.tex.pending_preview = None;
        return;
    };

    let file_name = preview
        .path
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();

    let mut apply = false;
    let mut cancelled = false;

    egui::Window::new(t!("viewer.tex_drop.title"))
        .collapsible(true)
        .resizable(true)
        .default_pos(egui::pos2(20.0, 60.0))
        .show(ctx, |ui| {
            // Show thumbnail + file name side by side.
            ui.horizontal(|ui| {
                if let Some(tex_id) = preview.preview_tex_id {
                    let thumb_size = 64.0;
                    ui.image(egui::load::SizedTexture::new(
                        tex_id,
                        [thumb_size, thumb_size],
                    ));
                }
                ui.vertical(|ui| {
                    ui.label(t!("viewer.tex_drop.image_label", name = file_name));
                    ui.add_space(2.0);
                    ui.label(t!("viewer.tex_drop.instruction"));
                });
            });
            ui.separator();
            ui.horizontal(|ui| {
                if ui.small_button(t!("viewer.tex_drop.select_all")).clicked() {
                    preview.selection.iter_mut().for_each(|v| *v = true);
                }
                if ui
                    .small_button(t!("viewer.tex_drop.deselect_all"))
                    .clicked()
                {
                    preview.selection.iter_mut().for_each(|v| *v = false);
                }
                if ui.small_button(t!("viewer.tex_drop.unset_only")).clicked() {
                    for &(_draw_idx, mat_idx) in loaded.mat_cache.draw_indices.iter() {
                        if mat_idx < preview.selection.len() {
                            let has_tex = loaded
                                .mat_cache
                                .tex_indices
                                .get(mat_idx)
                                .and_then(|t| *t)
                                .is_some();
                            preview.selection[mat_idx] = !has_tex;
                        }
                    }
                }
            });
            ui.separator();
            egui::ScrollArea::vertical()
                .max_height(300.0)
                .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysVisible)
                .show(ui, |ui| {
                    for &(_draw_idx, mat_idx) in loaded.mat_cache.draw_indices.iter() {
                        if mat_idx >= preview.selection.len() {
                            continue;
                        }
                        let name = loaded
                            .mat_cache
                            .names
                            .get(mat_idx)
                            .map(|s| s.as_str())
                            .unwrap_or("?");
                        let has_tex = loaded
                            .mat_cache
                            .tex_indices
                            .get(mat_idx)
                            .and_then(|t| *t)
                            .is_some();
                        let indicator = if has_tex {
                            egui::RichText::new("\u{25A3}")
                                .color(egui::Color32::from_rgb(0x40, 0xC0, 0x40))
                        } else {
                            egui::RichText::new("\u{25A1}")
                                .color(egui::Color32::from_rgb(0xA0, 0x60, 0x60))
                        };
                        // FBX original texture file name.
                        let src_name = loaded
                            .mat_cache
                            .source_tex_names
                            .get(mat_idx)
                            .and_then(|s| s.as_deref())
                            .unwrap_or("");
                        let row = ui.horizontal(|ui| {
                            let ind_resp = ui.label(indicator);
                            let cb = ui.checkbox(&mut preview.selection[mat_idx], name);
                            if !src_name.is_empty() {
                                ui.label(
                                    egui::RichText::new(src_name)
                                        .small()
                                        .color(egui::Color32::from_gray(0x90)),
                                );
                            }
                            ind_resp.contains_pointer() || cb.contains_pointer()
                        });
                        // Material-row hover -> highlight in the 3D view.
                        if row.inner {
                            for (di, d) in loaded.gpu_model.draws.iter().enumerate() {
                                if d.material_index == mat_idx
                                    && app.material_visibility.get(di).copied().unwrap_or(true)
                                {
                                    app.hovered_draw_indices.push(di);
                                }
                            }
                        }
                    }
                });
            ui.add_space(8.0);
            let selected_count = preview.selection.iter().filter(|&&v| v).count();
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(
                        selected_count > 0,
                        egui::Button::new(t!("viewer.dialog.common.apply")),
                    )
                    .clicked()
                {
                    apply = true;
                }
                if ui.button(t!("viewer.dialog.common.cancel")).clicked() {
                    cancelled = true;
                }
            });
        });

    if apply {
        app.apply_tex_preview();
    } else if cancelled {
        app.cancel_tex_preview();
    }
}

/// Execute PMX conversion.
pub fn execute_conversion(app: &mut ViewerApp, ctx: &egui::Context) {
    if app.loaded.is_none() {
        return;
    }
    let output_path = std::path::PathBuf::from(&app.export.pmx_output_path);
    let log_path = output_path.with_extension("log");

    // Record cumulative bytes-written before conversion (drain-safe cumulative offset).
    let log_offset_before = app
        .log_buffer
        .lock()
        .ok()
        .map(|lb| lb.total_written)
        .unwrap_or(0);

    // If normals are modified, write them back to `IrModel` to build the conversion IR.
    let normals_modified = app.material_display.iter().any(|d| d.smooth_normals)
        || app.material_display.iter().any(|d| d.clear_normals);
    // Save original normals (for restore).
    let saved_normals: Option<Vec<Vec<glam::Vec3>>> = if normals_modified {
        app.loaded.as_ref().map(|loaded| {
            loaded
                .ir
                .meshes
                .iter()
                .map(|m| m.vertices.iter().map(|v| v.normal).collect())
                .collect()
        })
    } else {
        None
    };
    if normals_modified {
        if let Some(ref mut loaded) = app.loaded {
            loaded.gpu_model.write_normals_back(&mut loaded.ir);
        }
    }
    let loaded = app
        .loaded
        .as_ref()
        .expect("loaded already verified by has_model check");

    // Filter visible materials.
    let convert_ir = if app.export.export_visible_only {
        let visible_mat_indices: HashSet<usize> = loaded
            .mat_cache
            .draw_indices
            .iter()
            .filter(|(draw_idx, _)| {
                app.material_visibility
                    .get(*draw_idx)
                    .copied()
                    .unwrap_or(true)
            })
            .map(|(_, mat_idx)| *mat_idx)
            .collect();

        log::info!(
            "Exporting visible materials only: {}/{} materials",
            visible_mat_indices.len(),
            &loaded.ir.materials.len()
        );
        build_filtered_ir(&loaded.ir, &visible_mat_indices)
    } else {
        loaded.ir.clone_for_export()
    };

    // For PMX/PMD formats `no_physics` / `raw_structure` are disabled (UI is greyed out too).
    let is_pmx_pmd = convert_ir.source_format.is_pmx_pmd();
    let options = crate::pmx::build::PmxBuildOptions {
        align_rigid_rotation: app.display.align_rigid_rotation,
        no_physics: if is_pmx_pmd {
            false
        } else {
            app.export.no_physics
        },
        raw_structure: if is_pmx_pmd {
            false
        } else {
            app.export.raw_structure
        },
        scale: app.export.scale,
    };

    // Capture A/T-stance info up front (BG thread cannot access `loaded`).
    let primary_astance_result = app
        .loaded
        .as_ref()
        .map(|l| l.primary_astance_result)
        .unwrap_or_default();
    let stance_label = if is_pmx_pmd { "T-stance" } else { "A-stance" };
    let stance_label_owned = stance_label.to_string();
    let output_log = app.export.output_log;
    let log_buffer = Arc::clone(&app.log_buffer);

    // Capture MME-output data (move to the BG thread).
    let output_mme = app.export.output_mme;
    let ray_mmd_root = if output_mme {
        Some(
            app.app_config
                .ray_mmd_root
                .as_ref()
                .map(std::path::PathBuf::from)
                .unwrap_or_else(|| std::path::PathBuf::from(".")),
        )
    } else {
        None
    };
    // Per-material manual category override.
    let mme_kinds: std::collections::HashMap<
        usize,
        crate::convert::mme::ray_mmd::RayMmdMaterialKind,
    > = if output_mme {
        app.material_overrides
            .iter()
            .filter_map(|(idx, ov)| ov.mme_kind.map(|k| (*idx, k)))
            .collect()
    } else {
        std::collections::HashMap::new()
    };

    // Restore normals immediately (the BG thread uses a cloned IR, so the original IR can be restored at once).
    if let Some(saved) = saved_normals {
        if let Some(ref mut loaded) = app.loaded {
            for (mi, mesh) in loaded.ir.meshes.iter_mut().enumerate() {
                if let Some(normals) = saved.get(mi) {
                    for (vi, v) in mesh.vertices_mut().iter_mut().enumerate() {
                        if let Some(&n) = normals.get(vi) {
                            v.normal = n;
                        }
                    }
                }
            }
        }
    }

    // Run PMX conversion on a BG thread.
    let cancel = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let cancel_clone = std::sync::Arc::clone(&cancel);
    let (tx, rx) = std::sync::mpsc::channel();
    let repaint = ctx.clone();

    std::thread::spawn(move || {
        // Cooperative cancel: check the flag between steps.
        let result = crate::convert_ir_to_pmx_with_cancel(
            &convert_ir,
            &output_path,
            &options,
            &cancel_clone,
        );

        // If the error came from a cancel, exit silently (the UI already showed the message).
        if cancel_clone.load(std::sync::atomic::Ordering::Relaxed) {
            return;
        }

        if output_log {
            let debug_logs = read_log_buffer_from_offset(&log_buffer, log_offset_before);
            write_convert_log(
                &log_path,
                &convert_ir,
                result.as_ref(),
                debug_logs.as_deref(),
            );
        }

        let bg_result = match result {
            Ok(stats) => {
                use crate::intermediate::types::AStanceResult;
                let mut msg = t!(
                    "viewer.toast.convert.complete",
                    path = stats.output_path,
                    bones = stats.bones,
                    vertices = stats.vertices,
                    materials = stats.materials,
                    morphs = stats.morphs,
                )
                .into_owned();
                if output_log {
                    msg += &t!(
                        "viewer.toast.convert.log_appended",
                        path = log_path.display().to_string(),
                    );
                }

                // MME output.
                let mut mme_warning = false;
                if output_mme {
                    if let Some(ref root) = ray_mmd_root {
                        let mme_dir = output_path.with_extension("").with_file_name(format!(
                            "{}_mme",
                            output_path
                                .file_stem()
                                .unwrap_or_default()
                                .to_string_lossy()
                        ));
                        match emit_mme_files(&convert_ir, &mme_dir, root, &mme_kinds) {
                            Ok(result) => {
                                msg += &t!(
                                    "viewer.toast.convert.mme_summary",
                                    count = result.count,
                                    dir = mme_dir.display().to_string(),
                                );
                                if let Some(ref warn) = result.include_warning {
                                    msg += &t!("viewer.toast.convert.mme_warn_prefix", warn = warn);
                                    mme_warning = true;
                                }
                            }
                            Err(e) => {
                                msg +=
                                    &t!("viewer.toast.convert.mme_failed", err = format!("{e}"),);
                                mme_warning = true;
                            }
                        }
                    }
                }

                let has_warning = match primary_astance_result {
                    AStanceResult::NotFound => {
                        msg += &t!(
                            "viewer.toast.convert.stance_not_found",
                            label = stance_label_owned,
                        );
                        true
                    }
                    AStanceResult::AlreadyAStance => {
                        msg += &t!(
                            "viewer.toast.convert.stance_already",
                            label = stance_label_owned,
                        );
                        false
                    }
                    _ => false,
                };
                super::app::pending::ConvertBgResult {
                    result: Ok(msg),
                    log_written: output_log,
                    has_warning: has_warning || mme_warning,
                    output_dir: output_path.parent().map(|d| d.to_path_buf()),
                }
            }
            Err(e) => super::app::pending::ConvertBgResult {
                result: Err(t!("viewer.toast.convert.failed", err = format!("{e}")).into_owned()),
                log_written: output_log,
                has_warning: false,
                output_dir: None,
            },
        };

        let _ = tx.send(bg_result);
        repaint.request_repaint();
    });

    app.pending.convert_bg = Some(super::app::pending::PendingConvertBg { rx, cancel });
}

/// MME (.fx) output result.
struct MmeEmitResult {
    count: usize,
    /// Holds a warning message when the `#include`-target fxsub is not found.
    include_warning: Option<String>,
}

/// Emit the MME (.fx) files.
/// Called from the BG thread after PMX conversion succeeds.
fn emit_mme_files(
    ir: &crate::intermediate::types::IrModel,
    mme_dir: &std::path::Path,
    ray_mmd_root: &std::path::Path,
    mme_kinds: &std::collections::HashMap<usize, crate::convert::mme::ray_mmd::RayMmdMaterialKind>,
) -> anyhow::Result<MmeEmitResult> {
    use crate::convert::mme::ray_mmd;

    std::fs::create_dir_all(mme_dir)?;

    // Relative `#include` path.
    let include_path = ray_mmd::resolve_include_path(ray_mmd_root, mme_dir);

    // Verify the `#include` target exists (.fx files are still emitted; warn if missing).
    let fxsub_abs = mme_dir.join(&include_path);
    let include_warning = if !fxsub_abs.exists() {
        Some(format!(
            "#include target not found: {}\nCheck the ray-mmd root.",
            fxsub_abs.display()
        ))
    } else {
        None
    };

    // Emit auxiliary textures.
    let support_textures = ray_mmd::export_mme_support_textures(ir, mme_dir)?;

    // Generate .fx files.
    let mut used_names = std::collections::HashSet::new();
    let mut fx_manifest: Vec<(usize, String, ray_mmd::RayMmdMaterialKind)> = Vec::new();

    for (mat_idx, mat) in ir.materials.iter().enumerate() {
        let kind = mme_kinds
            .get(&mat_idx)
            .copied()
            .unwrap_or_else(|| ray_mmd::guess_ray_mmd_kind(mat));
        let fx_name = ray_mmd::make_fx_filename(&mat.name, &mut used_names);
        let fx_content = ray_mmd::generate_fx(mat, kind, &include_path, &support_textures);

        std::fs::write(mme_dir.join(&fx_name), fx_content)?;
        fx_manifest.push((mat_idx, fx_name, kind));
    }

    // README.txt
    ray_mmd::write_mme_readme(mme_dir, &fx_manifest)?;

    log::info!(
        "MME output: {} .fx files written to {}",
        fx_manifest.len(),
        mme_dir.display()
    );

    Ok(MmeEmitResult {
        count: fx_manifest.len(),
        include_warning,
    })
}

/// Format a number with comma separators (e.g. 34059 -> "34,059").
fn format_number(n: usize) -> String {
    let s = n.to_string();
    let len = s.len();
    let mut result = String::with_capacity(len + (len.saturating_sub(1)) / 3);
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (len - i).is_multiple_of(3) {
            result.push(',');
        }
        result.push(c);
    }
    result
}

/// Show meta info in collapsible Grids per section.
/// Info tab: model info + meta info.
fn show_tab_info(ui: &mut egui::Ui, app: &mut ViewerApp) {
    let Some(ref loaded) = app.loaded else {
        return;
    };
    let ir = &loaded.ir;

    ui.heading(
        egui::RichText::new(t!("viewer.section.model_info")).color(egui::Color32::from_gray(0xD0)),
    );
    ui.separator();
    // Name (single row).
    egui::Grid::new("model_info_name")
        .num_columns(2)
        .show(ui, |ui| {
            ui.label(t!("viewer.model_info.name"));
            ui.label(&ir.name);
            ui.end_row();

            ui.label(t!("viewer.model_info.format"));
            ui.label(ir.source_format.label());
            ui.end_row();
        });
    // Compact 4-column display (label+value x 2) for numeric info.
    egui::Grid::new("model_info_stats")
        .num_columns(4)
        .spacing([4.0, 2.0])
        .show(ui, |ui| {
            ui.label(t!("viewer.model_info.bones"));
            ui.label(format_number(ir.bones.len()));
            ui.label(t!("viewer.model_info.vertices"));
            ui.label(format_number(ir.total_vertices()));
            ui.end_row();

            ui.label(t!("viewer.model_info.faces"));
            ui.label(format_number(ir.total_faces()));
            ui.label(t!("viewer.model_info.materials"));
            ui.label(format_number(ir.materials.len()));
            ui.end_row();

            ui.label(t!("viewer.model_info.textures"));
            ui.label(format_number(ir.textures.len()));
            ui.label(t!("viewer.model_info.morphs"));
            ui.label(format_number(ir.morphs.len()));
            ui.end_row();
        });
    if let Some(ref rig) = ir.rig_type {
        egui::Grid::new("model_info_rig")
            .num_columns(4)
            .spacing([4.0, 2.0])
            .show(ui, |ui| {
                ui.label(t!("viewer.model_info.rig"));
                ui.label(rig);
                ui.label("Humanoid");
                if ir.humanoid_bone_count > 0 {
                    ui.label(t!(
                        "viewer.model_info.humanoid_count",
                        count = ir.humanoid_bone_count,
                    ));
                } else {
                    ui.colored_label(
                        egui::Color32::GRAY,
                        t!("viewer.model_info.humanoid_unsupported"),
                    );
                }
                ui.end_row();
            });
    }

    ui.add_space(12.0);

    // Meta info / comment.
    if !ir.comment.is_empty() {
        if ir.source_format.is_pmx_pmd() {
            // PMX/PMD: show the free-form comment as is.
            ui.heading(
                egui::RichText::new(t!("viewer.section.comment"))
                    .color(egui::Color32::from_gray(0xD0)),
            );
            ui.separator();
            egui::ScrollArea::vertical()
                .max_height(200.0)
                .show(ui, |ui| {
                    ui.label(&ir.comment);
                });
        } else {
            show_meta_info(ui, &ir.comment);
        }
    }
}

/// Control tab: animation controls + expression-morph sliders.
fn show_tab_control(ui: &mut egui::Ui, app: &mut ViewerApp) {
    show_animation_controls(ui, app);

    ui.add_space(8.0);

    let Some(ref loaded) = app.loaded else {
        return;
    };
    let ir = &loaded.ir;

    if ir.morphs.is_empty() {
        return;
    }

    // Collect expression names being controlled by the animation.
    let anim_expr_morphs: std::collections::HashSet<usize> = if let Some(ref anim) = app.anim.state
    {
        ir.morphs
            .iter()
            .enumerate()
            .filter_map(|(i, morph)| {
                if anim
                    .animation
                    .expression_channels
                    .contains_key(&morph.name_en)
                {
                    Some(i)
                } else {
                    None
                }
            })
            .collect()
    } else {
        std::collections::HashSet::new()
    };

    ui.heading(
        egui::RichText::new(t!("viewer.section.expression_morphs"))
            .color(egui::Color32::from_gray(0xD0)),
    );
    ui.separator();
    ui.horizontal(|ui| {
        ui.label(t!("viewer.morph.filter_label"));
        ui.text_edit_singleline(&mut app.morph_filter);
        if !app.morph_filter.is_empty() && ui.small_button("✕").clicked() {
            app.morph_filter.clear();
        }
    });
    if ui.small_button(t!("viewer.morph.reset_all")).clicked() {
        // Morphs being edited by the UV editor keep their stash values, so skip them (v0.5.6).
        let uv_locked_morph = app.uv_edit.active_morph;
        for (i, w) in app.morph_weights.iter_mut().enumerate() {
            if !anim_expr_morphs.contains(&i) && Some(i) != uv_locked_morph {
                *w = 0.0;
            }
        }
        app.morph_dirty = true;
    }
    ui.separator();
    let filter_lower = app.morph_filter.to_lowercase();
    for (i, morph) in ir.morphs.iter().enumerate() {
        // Skip morphs that don't match the filter.
        if !filter_lower.is_empty()
            && !morph.name.to_lowercase().contains(&filter_lower)
            && !morph.name_en.to_lowercase().contains(&filter_lower)
        {
            continue;
        }
        if i < app.morph_weights.len() {
            let is_anim_controlled = anim_expr_morphs.contains(&i);
            // v0.5.6: while this morph is being edited in the UV editor, its weight is fixed at 1.0,
            // so disable sliders to avoid mishandling (the original value is restored at edit end).
            let is_uv_morph_locked = app.uv_edit.active_morph == Some(i);
            let enabled = !is_anim_controlled && !is_uv_morph_locked;
            ui.horizontal(|ui| {
                ui.add_enabled_ui(enabled, |ui| {
                    if ui.small_button("0").clicked() {
                        app.morph_weights[i] = 0.0;
                        app.morph_dirty = true;
                    }
                    if ui
                        .add(
                            egui::Slider::new(&mut app.morph_weights[i], 0.0..=1.0)
                                .show_value(false),
                        )
                        .changed()
                    {
                        app.morph_dirty = true;
                    }
                    if ui.small_button("1").clicked() {
                        app.morph_weights[i] = 1.0;
                        app.morph_dirty = true;
                    }
                    if ui
                        .add(
                            egui::DragValue::new(&mut app.morph_weights[i])
                                .range(0.0..=1.0)
                                .speed(0.01)
                                .fixed_decimals(2),
                        )
                        .changed()
                    {
                        app.morph_dirty = true;
                    }
                });
                ui.label(&morph.name);
                if is_anim_controlled {
                    ui.weak("(VRMA)");
                }
                if is_uv_morph_locked {
                    ui.weak(t!("viewer.morph.uv_editing"));
                }
            });
        }
    }
}

/// Display tab: display settings + material display.
fn show_tab_display(
    ui: &mut egui::Ui,
    app: &mut ViewerApp,
    tex_assign_request: &mut Option<TexAssignRequest>,
) {
    // Display settings.
    ui.heading(
        egui::RichText::new(t!("viewer.section.display_settings"))
            .color(egui::Color32::from_gray(0xD0)),
    );
    ui.separator();

    if ui.small_button(t!("viewer.display.light_reset")).clicked() {
        let d = DisplaySettings::default();
        app.display.light_intensity = d.light_intensity;
        app.display.light_color = d.light_color;
        app.display.ambient_intensity = d.ambient_intensity;
        app.display.ambient_sky_color = d.ambient_sky_color;
        app.display.ambient_ground_color = d.ambient_ground_color;
        app.display.bg_brightness = d.bg_brightness;
        // Bloom has a dedicated reset button, so do not touch it here.
    }
    // Align Light / Ambient / Ground color-button positions in a Grid.
    egui::Grid::new("light_color_grid")
        .num_columns(2)
        .show(ui, |ui| {
            // Disable Light for Unlit / Normal because lighting has no effect.
            let shader_sel = app.display.shader_selection();
            let light_enabled =
                !matches!(shader_sel, ShaderSelection::Unlit | ShaderSelection::Normal);
            ui.add_enabled(
                light_enabled,
                egui::Slider::new(&mut app.display.light_intensity, 0.0..=2.0)
                    .text(t!("viewer.display.light_label")),
            );
            ui.add_enabled_ui(light_enabled, |ui| {
                color_wheel_button_rgb(ui, "light_color", &mut app.display.light_color);
            });
            ui.end_row();

            // Disable ambient under MMD / Unlit / Normal.
            let amb_enabled = light_enabled && !app.display.use_mmd_path;
            ui.add_enabled(
                amb_enabled,
                egui::Slider::new(&mut app.display.ambient_intensity, 0.0..=1.0)
                    .text(t!("viewer.display.ambient_label")),
            );
            ui.add_enabled_ui(amb_enabled, |ui| {
                color_wheel_button_rgb(ui, "ambient_sky", &mut app.display.ambient_sky_color);
            });
            ui.end_row();

            ui.add_enabled_ui(amb_enabled, |ui| {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label("Ground");
                });
            });
            ui.add_enabled_ui(amb_enabled, |ui| {
                color_wheel_button_rgb(ui, "ambient_ground", &mut app.display.ambient_ground_color);
            });
            ui.end_row();
        });
    ui.add(
        egui::Slider::new(&mut app.display.bg_brightness, 0.0..=1.0)
            .text(t!("viewer.display.background_label")),
    );
    ui.checkbox(&mut app.display.show_grid, t!("viewer.display.show_grid"));

    let has_bones = app.loaded.as_ref().is_some_and(|l| !l.ir.bones.is_empty());
    let has_spring = app
        .loaded
        .as_ref()
        .is_some_and(|l| !l.ir.physics.rigid_bodies.is_empty());
    ui.add_enabled_ui(has_bones, |ui| {
        ui.checkbox(&mut app.display.show_bones, t!("viewer.display.show_bones"))
            .on_disabled_hover_text(t!("viewer.display.bones_missing"));
        if app.display.show_bones {
            ui.add(
                egui::Slider::new(&mut app.display.bone_opacity, 0.05..=1.0)
                    .text(t!("viewer.display.bone_opacity_label")),
            );
        }
    });
    ui.add_enabled_ui(has_spring, |ui| {
        ui.checkbox(
            &mut app.display.show_spring_bones,
            t!("viewer.display.show_spring_bones"),
        )
        .on_disabled_hover_text(t!("viewer.display.spring_missing"));
        if app.display.show_spring_bones {
            ui.add(
                egui::Slider::new(&mut app.display.spring_bone_opacity, 0.05..=1.0)
                    .text(t!("viewer.display.spring_opacity_label")),
            );
        }
    });
    // Joint display (PMX/PMD only).
    let is_pmx_pmd_joints = app
        .loaded
        .as_ref()
        .is_some_and(|l| l.ir.source_format.is_pmx_pmd());
    let has_joints = app
        .loaded
        .as_ref()
        .is_some_and(|l| !l.ir.physics.joints.is_empty());
    if is_pmx_pmd_joints && has_joints {
        ui.checkbox(
            &mut app.display.show_joints,
            t!("viewer.display.show_joints"),
        );
        if app.display.show_joints {
            ui.add(
                egui::Slider::new(&mut app.display.joint_opacity, 0.05..=1.0)
                    .text(t!("viewer.display.joint_opacity_label")),
            );
        }
    }
    // Wireframe.
    let supports_wire = app
        .renderer
        .as_ref()
        .map(|r| r.supports_wireframe())
        .unwrap_or(false);
    if supports_wire {
        ui.horizontal(|ui| {
            ui.label(t!("viewer.display.draw_mode_label"));
            ui.selectable_value(&mut app.display.draw_mode, DrawMode::Solid, "Solid");
            ui.selectable_value(&mut app.display.draw_mode, DrawMode::Wireframe, "Wire");
            ui.selectable_value(&mut app.display.draw_mode, DrawMode::SolidWireframe, "S+W");
        });
    }
    // Light mode.
    ui.horizontal(|ui| {
        ui.label(t!("viewer.display.light_mode_label"));
        ui.selectable_value(
            &mut app.display.light_mode,
            LightMode::CameraFollow,
            t!("viewer.display.light_mode_camera_follow"),
        );
        ui.selectable_value(
            &mut app.display.light_mode,
            LightMode::Fixed,
            t!("viewer.display.light_mode_fixed"),
        );
    });
    // Decide based on whether there is a draw with built MMD resources.
    let has_mmd_capability = app.loaded.as_ref().is_some_and(|l| {
        l.gpu_model
            .draws
            .iter()
            .any(|d| d.mmd_material_bind_group.is_some())
    });
    ui.separator();

    // Shader mode selection (up-arrow ComboBox down-arrow).
    let mut sel = app.display.shader_selection();
    let shader_choices: Vec<ShaderSelection> = {
        let mut v = vec![
            ShaderSelection::Auto,
            ShaderSelection::Mtoon,
            ShaderSelection::Unlit,
            ShaderSelection::GgxPreview,
            ShaderSelection::Normal,
        ];
        if has_mmd_capability {
            v.push(ShaderSelection::Mmd);
        }
        v
    };
    let shader_label = |s: ShaderSelection| -> std::borrow::Cow<'static, str> {
        match s {
            ShaderSelection::Auto => std::borrow::Cow::Borrowed("Auto"),
            ShaderSelection::Mtoon => std::borrow::Cow::Borrowed("MToon/Lambert"),
            ShaderSelection::Unlit => std::borrow::Cow::Borrowed("Unlit"),
            ShaderSelection::GgxPreview => std::borrow::Cow::Borrowed("GGX Preview"),
            ShaderSelection::Normal => t!("viewer.display.shader_normal"),
            ShaderSelection::Mmd => std::borrow::Cow::Borrowed("MMD"),
        }
    };
    let len = shader_choices.len();
    // Compute a fixed width based on the longest choice.
    let combo_min_width = {
        let max_w = shader_choices
            .iter()
            .map(|&c| {
                ui.fonts(|f| {
                    f.layout_no_wrap(
                        shader_label(c).to_string(),
                        egui::FontId::default(),
                        egui::Color32::WHITE,
                    )
                    .size()
                    .x
                })
            })
            .fold(0.0f32, f32::max);
        max_w + ui.spacing().button_padding.x * 2.0 + 8.0
    };
    ui.horizontal(|ui| {
        ui.label(t!("viewer.display.shader_label"));
        if ui.small_button("\u{25b2}").clicked() {
            if let Some(idx) = shader_choices.iter().position(|&s| s == sel) {
                sel = shader_choices[(idx + len - 1) % len];
            }
        }
        ui.scope(|ui| {
            ui.spacing_mut().combo_width = combo_min_width;
            egui::ComboBox::from_id_salt("shader_mode")
                .selected_text(shader_label(sel))
                .icon(|_, _, _, _, _| {})
                .show_ui(ui, |ui| {
                    for &choice in &shader_choices {
                        ui.selectable_value(&mut sel, choice, shader_label(choice));
                    }
                });
        });
        if ui.small_button("\u{25bc}").clicked() {
            if let Some(idx) = shader_choices.iter().position(|&s| s == sel) {
                sel = shader_choices[(idx + 1) % len];
            }
        }
    });
    if sel != app.display.shader_selection() {
        app.display.set_shader_selection(sel);
        app.normalize_shader_state();
    }

    // Decide enable based on whether any Standard draw has an MToon outline.
    let has_outline_draws = app.loaded.as_ref().is_some_and(|l| {
        l.gpu_model
            .draws
            .iter()
            .any(|d| d.render_style == super::mesh::RenderStyle::Standard && d.has_outline)
    });
    let outline_available =
        has_outline_draws && matches!(sel, ShaderSelection::Auto | ShaderSelection::Mtoon);
    ui.add_enabled(
        outline_available,
        egui::Checkbox::new(
            &mut app.display.outline_enabled,
            t!("viewer.display.outline_enable"),
        ),
    );

    // MMD sub-options (when explicitly selecting Mmd, or when MMD path is enabled under Auto).
    let show_mmd_options =
        sel == ShaderSelection::Mmd || (sel == ShaderSelection::Auto && app.display.use_mmd_path);
    if show_mmd_options {
        ui.checkbox(
            &mut app.display.mmd_edge_enabled,
            t!("viewer.display.edge_enable"),
        );
        if app.display.mmd_edge_enabled {
            ui.add(
                egui::Slider::new(&mut app.display.mmd_edge_thickness, 0.1..=3.0)
                    .text(t!("viewer.display.edge_thickness_label")),
            );
        }
    }

    ui.separator();
    ui.checkbox(&mut app.display.msaa, t!("viewer.display.msaa_label"));
    let white_fallback_resp = ui
        .checkbox(
            &mut app.display.white_texture_fallback,
            t!("viewer.display.white_fallback_label"),
        )
        .on_hover_text(t!("viewer.display.white_fallback_hover"));
    if white_fallback_resp.changed() {
        super::texture::set_white_texture_fallback_dynamic(
            app.display.white_texture_fallback,
            &app.render_state.queue,
        );
    }
    ui.horizontal(|ui| {
        ui.checkbox(&mut app.display.bloom_enabled, "Bloom");
        if app.display.bloom_enabled
            && ui
                .small_button(t!("viewer.display.bloom_default"))
                .clicked()
        {
            let d = DisplaySettings::default();
            app.display.bloom_intensity = d.bloom_intensity;
            app.display.bloom_threshold = d.bloom_threshold;
            app.display.bloom_radius = d.bloom_radius;
        }
    });
    if app.display.bloom_enabled {
        ui.add(
            egui::Slider::new(&mut app.display.bloom_intensity, 0.0..=4.0)
                .text(t!("viewer.display.bloom_intensity_label")),
        );
        ui.add(
            egui::Slider::new(&mut app.display.bloom_threshold, 0.0..=1.0)
                .max_decimals(2)
                .text(t!("viewer.display.bloom_threshold_label")),
        );
        ui.add(
            egui::Slider::new(&mut app.display.bloom_radius, 3..=6)
                .text(t!("viewer.display.bloom_radius_label")),
        );
    }
    ui.checkbox(
        &mut app.display.show_normals,
        t!("viewer.display.show_normals"),
    );
    if app.display.show_normals {
        ui.add(
            egui::Slider::new(&mut app.display.normal_length, 0.1..=3.0)
                .text(t!("viewer.display.normal_length_label")),
        );
    }
    let has_mmd_normals = app.loaded.as_ref().is_some_and(|l| {
        l.gpu_model
            .draws
            .iter()
            .any(|d| d.render_style == super::mesh::RenderStyle::Mmd)
    });
    // Bulk toggle for per-material normal flags (label + [on]/[off] buttons).
    ui.add_enabled_ui(!has_mmd_normals, |ui| {
        // Normal smoothing bulk.
        ui.horizontal(|ui| {
            ui.label(t!("viewer.display.smooth_normals_label"));
            let on_resp = ui.small_button("on");
            let off_resp = ui.small_button("off");
            let mut new_val: Option<bool> = None;
            if on_resp.clicked() {
                new_val = Some(true);
            }
            if off_resp.clicked() {
                new_val = Some(false);
            }
            if let Some(v) = new_val {
                if let Some(ref loaded) = app.loaded {
                    let ir_mats = &loaded.ir.materials;
                    for (i, d) in app.material_display.iter_mut().enumerate() {
                        // Materials with a normal map are forced to false on "on" (preserves existing behavior).
                        if v && ir_mats.get(i).is_some_and(|m| m.normal_texture.is_some()) {
                            d.smooth_normals = false;
                        } else {
                            d.smooth_normals = v;
                        }
                    }
                    app.pending.rebuild = Some(PendingOverlay::WaitingOverlay);
                }
            }
            if has_mmd_normals {
                on_resp.on_disabled_hover_text(t!("viewer.display.pmx_normals_locked"));
                off_resp.on_disabled_hover_text(t!("viewer.display.pmx_normals_locked"));
            }
        });
        // Clear custom normals bulk.
        ui.horizontal(|ui| {
            ui.label(t!("viewer.display.clear_normals_label"));
            let on_resp = ui.small_button("on");
            let off_resp = ui.small_button("off");
            let mut new_val: Option<bool> = None;
            if on_resp.clicked() {
                new_val = Some(true);
            }
            if off_resp.clicked() {
                new_val = Some(false);
            }
            if let Some(v) = new_val {
                if let Some(ref loaded) = app.loaded {
                    let ir_mats = &loaded.ir.materials;
                    for (i, d) in app.material_display.iter_mut().enumerate() {
                        if v && ir_mats.get(i).is_some_and(|m| m.normal_texture.is_some()) {
                            d.clear_normals = false;
                        } else {
                            d.clear_normals = v;
                        }
                    }
                    app.pending.rebuild = Some(PendingOverlay::WaitingOverlay);
                }
            }
            if has_mmd_normals {
                on_resp.on_disabled_hover_text(t!("viewer.display.pmx_normals_locked"));
                off_resp.on_disabled_hover_text(t!("viewer.display.pmx_normals_locked"));
            }
        });
    });

    ui.add_space(12.0);

    // Material display.
    // Compute the texture-history key up front (avoid borrow conflicts).
    let tex_history_key = app.texture_history_key();
    let tex_history_has_entry = tex_history_key.as_ref().is_some_and(|k| {
        app.texture_history.history.contains_key(k)
            || app.texture_history.param_overrides.contains_key(k)
    });
    let has_file_assignments = app
        .tex
        .assignments
        .values()
        .any(|s| matches!(s, super::app::helpers::TextureSource::File(_)));
    // v0.5.0: allow saving with parameter edits alone, even without texture assignments (§I minimum persistence).
    let has_param_edits = !app.material_overrides.is_empty();

    let Some(ref loaded) = app.loaded else { return };
    if loaded.gpu_model.draws.is_empty() {
        return;
    }

    let draw_info = &loaded.mat_cache.draw_indices;
    let mat_tex_info = &loaded.mat_cache.tex_indices;
    let mat_names = &loaded.mat_cache.names;
    let mat_src_tex = &loaded.mat_cache.source_tex_names;
    let num_draws = draw_info.len();

    ui.heading(
        egui::RichText::new(t!("viewer.section.material_display"))
            .color(egui::Color32::from_gray(0xD0)),
    );
    ui.separator();
    let small = egui::TextStyle::Small;
    ui.horizontal(|ui| {
        if ui
            .small_button(t!("viewer.material_list.show_all"))
            .clicked()
        {
            app.material_visibility.iter_mut().for_each(|v| *v = true);
        }
        if ui
            .small_button(t!("viewer.material_list.hide_all"))
            .clicked()
        {
            app.material_visibility.iter_mut().for_each(|v| *v = false);
        }
        ui.checkbox(
            &mut app.tex.link_same_name,
            t!("viewer.material_list.link_same_name"),
        )
        .on_hover_text(t!("viewer.material_list.link_same_name_hover"));
    });
    // Row 2: texture reset + history buttons (small font).
    let mut do_save_history = false;
    let mut do_recall_history = false;
    ui.horizontal(|ui| {
        if !app.tex.assignments.is_empty()
            && ui
                .button(
                    egui::RichText::new(t!("viewer.material_list.tex_reset"))
                        .text_style(small.clone()),
                )
                .clicked()
        {
            app.tex.assignments.clear();
            app.tex.pkg_assignments.clear();
            app.pending.reload = Some(PendingOverlay::WaitingOverlay);
        }
        if tex_history_key.is_some() {
            if (has_file_assignments || has_param_edits)
                && ui
                    .button(
                        egui::RichText::new(t!("viewer.material_list.history_save"))
                            .text_style(small.clone()),
                    )
                    .clicked()
            {
                // If history exists, set the confirm flag; otherwise save immediately.
                if tex_history_has_entry {
                    app.pending.confirm_save_tex_history = true;
                } else {
                    do_save_history = true;
                }
            }
            if tex_history_has_entry
                && ui
                    .button(
                        egui::RichText::new(t!("viewer.material_list.history_recall"))
                            .text_style(small.clone()),
                    )
                    .clicked()
            {
                do_recall_history = true;
            }
        }
    });
    // Filter (useful when there are many materials).
    if num_draws > 10 {
        ui.horizontal(|ui| {
            ui.label(t!("viewer.material_list.search_label"));
            ui.add(
                egui::TextEdit::singleline(&mut app.material_filter)
                    .desired_width(ui.available_width())
                    .hint_text(t!("viewer.material_list.material_filter_hint")),
            );
        });
    }
    let filter_lower = app.material_filter.to_lowercase();
    let thumb_ids = &app.tex.pkg_thumb_cache;
    // Pre-extract per-material normal-map presence (to avoid borrow conflicts).
    let mat_has_normal_map: Vec<bool> = app
        .loaded
        .as_ref()
        .map(|l| {
            l.ir.materials
                .iter()
                .map(|m| m.normal_texture.is_some())
                .collect()
        })
        .unwrap_or_default();
    // Pre-extract per-material emissive presence.
    let mat_has_emissive: Vec<bool> = app
        .loaded
        .as_ref()
        .map(|l| {
            l.ir.materials
                .iter()
                .map(|m| {
                    m.emissive_factor != glam::Vec3::ZERO
                        || m.emissive_texture.is_some()
                        || (m.specular == glam::Vec3::ZERO && m.specular_power >= 100.0)
                })
                .collect()
        })
        .unwrap_or_default();
    // Lightly extract group info (only names and draw_range; avoid cloning the whole `MaterialGroup`).
    let (group_names, group_draw_ranges): (Vec<String>, Vec<std::ops::Range<usize>>) = app
        .loaded
        .as_ref()
        .map(|l| {
            l.material_groups
                .iter()
                .map(|g| (g.name.clone(), g.draw_range.clone()))
                .unzip()
        })
        .unwrap_or_default();
    let has_groups = !group_names.is_empty();

    if has_groups {
        // DrawCall index -> group index.
        let mut draw_to_group: Vec<usize> = vec![0; num_draws];
        for (gi, dr) in group_draw_ranges.iter().enumerate() {
            for item in draw_to_group
                .iter_mut()
                .take(dr.end.min(num_draws))
                .skip(dr.start)
            {
                *item = gi;
            }
        }
        // Also clone `draw_info` (used inside the `CollapsingHeader` closure).
        let draw_info_owned = draw_info.to_vec();
        // Release the `loaded` borrow.
        let _ = draw_info;
        let _ = mat_tex_info;
        let _ = mat_names;
        let _ = mat_src_tex;

        for (gi, gname) in group_names.iter().enumerate() {
            let group_draws: Vec<(usize, usize)> = draw_info_owned
                .iter()
                .filter(|&&(i, _)| i < num_draws && draw_to_group[i] == gi)
                .copied()
                .collect();
            if group_draws.is_empty() {
                continue;
            }
            // Collect unique `mat_idx` within the group (for S / C bulk).
            let group_mat_idxs: Vec<usize> = {
                let mut set = std::collections::BTreeSet::new();
                for &(_, mat_idx) in &group_draws {
                    set.insert(mat_idx);
                }
                set.into_iter().collect()
            };
            let group_draw_idxs: Vec<usize> = group_draws.iter().map(|&(i, _)| i).collect();

            let id = ui.id().with(("mat_group", gi));
            let mut state = egui::collapsing_header::CollapsingState::load_with_default_open(
                ui.ctx(),
                id,
                true,
            );
            // Header row: collapse [S][C][N][B][ ] group name.
            let header_res = ui.horizontal(|ui| {
                // Collapse toggle.
                state.show_toggle_button(ui, egui::collapsing_header::paint_default_icon);
                // [S] Smooth normals (group bulk) - always enabled (compatible with normal map).
                {
                    let all_on = !group_mat_idxs.is_empty()
                        && group_mat_idxs.iter().all(|&mi| {
                            app.material_display
                                .get(mi)
                                .is_some_and(|d| d.smooth_normals)
                        });
                    let resp = ui.add_enabled(
                        !group_mat_idxs.is_empty(),
                        egui::SelectableLabel::new(all_on, ICON_SMOOTH),
                    );
                    if resp.clicked() && !group_mat_idxs.is_empty() {
                        let new_val = !all_on;
                        for &mi in &group_mat_idxs {
                            if let Some(d) = app.material_display.get_mut(mi) {
                                d.smooth_normals = new_val;
                            }
                        }
                        app.pending.rebuild = Some(PendingOverlay::WaitingOverlay);
                    }
                    resp.on_hover_text(t!("viewer.material_list.smooth_normals_group_hover"));
                }
                // [C] Clear custom normals (group bulk).
                {
                    let all_on = !group_mat_idxs.is_empty()
                        && group_mat_idxs.iter().all(|&mi| {
                            app.material_display
                                .get(mi)
                                .is_some_and(|d| d.clear_normals)
                        });
                    let resp = ui.add_enabled(
                        !group_mat_idxs.is_empty(),
                        egui::SelectableLabel::new(all_on, ICON_CLEAR_NORMAL),
                    );
                    if resp.clicked() && !group_mat_idxs.is_empty() {
                        let new_val = !all_on;
                        for &mi in &group_mat_idxs {
                            if let Some(d) = app.material_display.get_mut(mi) {
                                d.clear_normals = new_val;
                            }
                        }
                        app.pending.rebuild = Some(PendingOverlay::WaitingOverlay);
                    }
                    resp.on_hover_text(t!("viewer.material_list.clear_normals_group_hover"));
                }
                // [N] Normal map ON/OFF (group bulk).
                {
                    let eligible: Vec<usize> = group_mat_idxs
                        .iter()
                        .copied()
                        .filter(|&mi| mat_has_normal_map.get(mi).copied().unwrap_or(false))
                        .collect();
                    let all_on = !eligible.is_empty()
                        && eligible
                            .iter()
                            .all(|&mi| app.material_display.get(mi).is_none_or(|d| d.normal_map));
                    let resp = ui.add_enabled(
                        !eligible.is_empty(),
                        egui::SelectableLabel::new(all_on, ICON_NORMAL_MAP),
                    );
                    if resp.clicked() && !eligible.is_empty() {
                        let new_val = !all_on;
                        for &mi in &eligible {
                            if let Some(d) = app.material_display.get_mut(mi) {
                                d.normal_map = new_val;
                            }
                        }
                        app.pending.rebuild = Some(PendingOverlay::WaitingOverlay);
                    }
                    resp.on_hover_text(t!("viewer.material_list.normal_map_group_hover"));
                }
                // [B] Emissive ON/OFF (group bulk).
                {
                    let eligible: Vec<usize> = group_mat_idxs
                        .iter()
                        .copied()
                        .filter(|&mi| mat_has_emissive.get(mi).copied().unwrap_or(false))
                        .collect();
                    let all_on = !eligible.is_empty()
                        && eligible
                            .iter()
                            .all(|&mi| app.material_display.get(mi).is_none_or(|d| d.emissive));
                    let resp = ui.add_enabled(
                        !eligible.is_empty(),
                        egui::SelectableLabel::new(all_on, ICON_EMISSIVE),
                    );
                    if resp.clicked() && !eligible.is_empty() {
                        let new_val = !all_on;
                        for &mi in &eligible {
                            if let Some(d) = app.material_display.get_mut(mi) {
                                d.emissive = new_val;
                            }
                        }
                        app.pending.rebuild = Some(PendingOverlay::WaitingOverlay);
                    }
                    resp.on_hover_text(t!("viewer.material_list.emissive_group_hover"));
                }
                // [ ] Visible / Hidden (group bulk).
                {
                    let all_visible = group_draw_idxs
                        .iter()
                        .all(|&di| app.material_visibility.get(di).copied().unwrap_or(true));
                    let mut checked = all_visible;
                    if ui.checkbox(&mut checked, "").clicked() {
                        for &di in &group_draw_idxs {
                            if di < app.material_visibility.len() {
                                app.material_visibility[di] = checked;
                            }
                        }
                    }
                }
                // Group name.
                if ui
                    .selectable_label(
                        false,
                        egui::RichText::new(gname)
                            .color(egui::Color32::from_rgb(0x60, 0xA0, 0xFF))
                            .strong(),
                    )
                    .clicked()
                {
                    state.toggle(ui);
                }
            });
            // Header-row hover -> highlight all draws in the group.
            if header_res.response.contains_pointer() {
                for &di in &group_draw_idxs {
                    if app.material_visibility.get(di).copied().unwrap_or(true) {
                        app.hovered_draw_indices.push(di);
                    }
                }
            }
            state.show_body_indented(&header_res.response, ui, |ui| {
                let Some(loaded) = app.loaded.as_ref() else { return };
                let mat_tex_info = &loaded.mat_cache.tex_indices;
                let mat_names = &loaded.mat_cache.names;
                let mat_src_tex = &loaded.mat_cache.source_tex_names;
                // v0.5.3: cache reference for thumbnail display on the leading button of each material row.
                let loaded_ir_thumb_ids: &[Option<egui::TextureId>] =
                    &app.tex.ir_thumb_cache;
                for &(i, mat_idx) in &group_draws {
                    if i >= app.material_visibility.len() { continue; }
                    let name = mat_names.get(mat_idx)
                        .map(|s: &String| s.as_str())
                        .unwrap_or("?");
                    if !filter_lower.is_empty()
                        && !name.to_lowercase().contains(&filter_lower)
                    {
                        continue;
                    }
                    let row_resp = ui.horizontal(|ui| {
                let mut row_highlight = false;
                // Texture-state indicator (v0.5.3: with thumbnail).
                // - When the assigned IR texture's thumbnail is available, use an `ImageButton` (18 px).
                // - has_tex=true but no thumbnail -> fallback to filled green square.
                // - has_tex=false -> empty red-brown square placeholder.
                {
                    let tex_idx_opt = mat_tex_info.get(mat_idx).and_then(|t| *t);
                    let has_tex = tex_idx_opt.is_some();
                    let thumb_id = tex_idx_opt
                        .and_then(|idx| loaded_ir_thumb_ids.get(idx).copied().flatten());
                    let src_name = mat_src_tex.get(mat_idx)
                        .and_then(|s: &Option<String>| s.as_deref());
                    let tooltip: String = match (has_tex, src_name) {
                        (true, Some(s)) => {
                            t!("viewer.material_list.tex_set_with_src", src = s).into_owned()
                        }
                        (true, None) => t!("viewer.material_list.tex_set").into_owned(),
                        (false, Some(s)) => {
                            t!("viewer.material_list.tex_unset_with_src", src = s).into_owned()
                        }
                        (false, None) => t!("viewer.material_list.tex_unset").into_owned(),
                    };
                    // v0.5.3: account for `ImageButton` framing by capping the image at 14 px,
                    // so the row's visual size matches the emoji icon columns.
                    const THUMB_PX: f32 = 14.0;
                    let thumb_size = egui::vec2(THUMB_PX, THUMB_PX);
                    let resp = if let Some(tid) = thumb_id {
                        // v0.5.3: compact preset (padding=1, stroke=0.5).
                        // Applied only inside the `scope` so other buttons aren't affected.
                        ui.scope(|ui| {
                            let style = ui.style_mut();
                            style.spacing.button_padding = egui::vec2(1.0, 1.0);
                            for w in [
                                &mut style.visuals.widgets.inactive,
                                &mut style.visuals.widgets.hovered,
                                &mut style.visuals.widgets.active,
                            ] {
                                w.bg_stroke.width = 0.5;
                            }
                            ui.add(
                                egui::ImageButton::new(
                                    egui::Image::from_texture((tid, thumb_size))
                                        .fit_to_exact_size(thumb_size),
                                )
                                .frame(true),
                            )
                            .on_hover_text(&tooltip)
                        })
                        .inner
                    } else {
                        let indicator = if has_tex {
                            egui::RichText::new("\u{25A3}")
                                .color(egui::Color32::from_rgb(0x40, 0xC0, 0x40))
                                .size(16.0)
                        } else {
                            egui::RichText::new("\u{25A1}")
                                .color(egui::Color32::from_rgb(0xA0, 0x60, 0x60))
                                .size(16.0)
                        };
                        ui.add(egui::Label::new(indicator).sense(egui::Sense::click()))
                            .on_hover_text(&tooltip)
                    };
                    if resp.contains_pointer() {
                        row_highlight = true;
                    }
                    let has_pkg = app.tex.pkg_textures.is_some();
                    let popup_id = ui.id().with(("pkg_tex_popup", mat_idx));
                    // Highlight while the popup is open as well.
                    if ui.memory(|m| m.is_popup_open(popup_id)) {
                        row_highlight = true;
                    }
                    if resp.clicked() {
                        if has_pkg {
                            ui.memory_mut(|m| m.toggle_popup(popup_id));
                        } else {
                            *tex_assign_request = Some(TexAssignRequest::FileDialog(mat_idx));
                        }
                    }
                    if has_pkg {
                        egui::popup_below_widget(ui, popup_id, &resp, egui::PopupCloseBehavior::CloseOnClickOutside, |ui| {
                            ui.set_min_width(280.0);
                            // Place "Select from file" at the top.
                            if ui.button(t!("viewer.material_list.pkg_select_file")).clicked() {
                                *tex_assign_request = Some(TexAssignRequest::FileDialog(mat_idx));
                                ui.memory_mut(|m| m.toggle_popup(popup_id));
                                app.tex.pkg_popup_filter.clear();
                            }
                            ui.separator();
                            ui.add(
                                egui::TextEdit::singleline(&mut app.tex.pkg_popup_filter)
                                    .desired_width(ui.available_width())
                                    .hint_text(t!("viewer.material_list.pkg_filter_hint")),
                            );
                            let filter_lower = app.tex.pkg_popup_filter.to_lowercase();
                            egui::ScrollArea::vertical().max_height(400.0)
                                .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysVisible)
                                .show(ui, |ui| {
                                if let Some(ref pkg) = app.tex.pkg_textures {
                                    for (ti, (tname, _)) in pkg.iter().enumerate() {
                                        if !filter_lower.is_empty()
                                            && !tname.to_lowercase().contains(&filter_lower)
                                        {
                                            continue;
                                        }
                                        let clicked = ui.horizontal(|ui| {
                                            if let Some(Some(tex_id)) = thumb_ids.get(ti) {
                                                ui.image(egui::load::SizedTexture::new(*tex_id, [32.0, 32.0]));
                                            }
                                            ui.button(tname).clicked()
                                        }).inner;
                                        if clicked {
                                            *tex_assign_request = Some(TexAssignRequest::PkgTexture(mat_idx, ti));
                                            ui.memory_mut(|m| m.toggle_popup(popup_id));
                                            app.tex.pkg_popup_filter.clear();
                                        }
                                    }
                                }
                            });
                        });
                    }
                }
                let assigned_name = app.tex.assignments.get(&mat_idx)
                    .map(|ts| {
                        let name = ts.display_name();
                        std::path::Path::new(&name)
                            .file_name()
                            .map(|f| f.to_string_lossy().into_owned())
                            .unwrap_or(name)
                    });
                let display_tex = assigned_name.as_deref()
                    .or_else(|| {
                        mat_src_tex.get(mat_idx)
                            .and_then(|s| s.as_deref())
                    });
                // Per-material normal toggles (S = smooth, C = clear custom, N = normal map, B = emissive).
                let has_nmap = mat_has_normal_map.get(mat_idx).copied().unwrap_or(false);
                // [S][C] are always enabled (compatible with normal map: smoothing the TBN base normal improves quality).
                if let Some(d) = app.material_display.get(mat_idx) {
                    let old = d.smooth_normals;
                    let resp = ui.add_enabled(
                        true,
                        egui::SelectableLabel::new(old, ICON_SMOOTH),
                    );
                    if resp.clicked() {
                        app.material_display[mat_idx].smooth_normals = !old;
                        app.pending.rebuild = Some(PendingOverlay::WaitingOverlay);
                    }
                    if resp.hovered() { row_highlight = true; }
                    resp.on_hover_text(t!("viewer.material_list.smooth_normals_hover"));
                }
                if let Some(d) = app.material_display.get(mat_idx) {
                    let old = d.clear_normals;
                    let resp = ui.add_enabled(
                        true,
                        egui::SelectableLabel::new(old, ICON_CLEAR_NORMAL),
                    );
                    if resp.clicked() {
                        app.material_display[mat_idx].clear_normals = !old;
                        app.pending.rebuild = Some(PendingOverlay::WaitingOverlay);
                    }
                    if resp.hovered() { row_highlight = true; }
                    resp.on_hover_text(t!("viewer.material_list.clear_normals_hover"));
                }
                // [N] Normal map ON/OFF.
                if let Some(d) = app.material_display.get(mat_idx) {
                    let old = d.normal_map;
                    let resp = ui.add_enabled(
                        has_nmap,
                        egui::SelectableLabel::new(old, ICON_NORMAL_MAP),
                    );
                    if resp.clicked() && has_nmap {
                        app.material_display[mat_idx].normal_map = !old;
                        app.pending.rebuild = Some(PendingOverlay::WaitingOverlay);
                    }
                    if resp.hovered() { row_highlight = true; }
                    resp.on_hover_text(t!("viewer.material_list.normal_map_hover"));
                }
                // [B] Emissive ON/OFF.
                if let Some(d) = app.material_display.get(mat_idx) {
                    let old = d.emissive;
                    let has_emissive = mat_has_emissive.get(mat_idx).copied().unwrap_or(false);
                    let resp = ui.add_enabled(
                        has_emissive,
                        egui::SelectableLabel::new(old, ICON_EMISSIVE),
                    );
                    if resp.clicked() && has_emissive {
                        app.material_display[mat_idx].emissive = !old;
                        app.pending.rebuild = Some(PendingOverlay::WaitingOverlay);
                    }
                    if resp.hovered() { row_highlight = true; }
                    resp.on_hover_text(t!("viewer.material_list.emissive_hover"));
                }

                // Material-edit panel open / close (§A). Treated as a column
                // separate from the existing icon columns; clicking toggles
                // the bottom-docked edit panel.
                // Note: the old version's ✎ (U+270E) couldn't be rendered
                // because the codepoint isn't in egui's NotoEmoji fallback.
                // `ICON_EDIT` (✏ U+270F) is in the fallback.
                {
                    let is_editing = app.editing_material_index == Some(mat_idx);
                    let resp = ui.selectable_label(is_editing, ICON_EDIT);
                    if resp.clicked() {
                        app.editing_material_index = if is_editing {
                            None
                        } else {
                            Some(mat_idx)
                        };
                    }
                    if resp.hovered() { row_highlight = true; }
                    resp.on_hover_text(t!("viewer.material_list.edit_panel_hover"));
                }

                let cb = if let Some(tex_name) = display_tex {
                    ui.checkbox(
                        &mut app.material_visibility[i],
                        format!("{} [{}]", name, tex_name),
                    )
                } else {
                    ui.checkbox(&mut app.material_visibility[i], name)
                };
                // Show the referenced texture file names as a tooltip on material-name hover.
                if let Some(ref loaded) = app.loaded {
                    if let Some(mat) = loaded.ir.materials.get(mat_idx) {
                        let textures = &loaded.ir.textures;
                        let mut lines = Vec::new();
                        if let Some(idx) = mat.texture_index {
                            if let Some(tex) = textures.get(idx) {
                                lines.push(
                                    t!("viewer.material_list.tex_slot_color", name = tex.filename)
                                        .into_owned(),
                                );
                            }
                        }
                        if let Some(idx) = mat.sphere_texture_index {
                            if let Some(tex) = textures.get(idx) {
                                lines.push(
                                    t!("viewer.material_list.tex_slot_sphere", name = tex.filename)
                                        .into_owned(),
                                );
                            }
                        }
                        if let Some(idx) = mat.toon_texture_index {
                            if let Some(tex) = textures.get(idx) {
                                lines.push(
                                    t!("viewer.material_list.tex_slot_toon", name = tex.filename)
                                        .into_owned(),
                                );
                            }
                        }
                        if let Some(ref info) = mat.normal_texture {
                            if let Some(tex) = textures.get(info.index) {
                                lines.push(
                                    t!("viewer.material_list.tex_slot_normal", name = tex.filename)
                                        .into_owned(),
                                );
                            }
                        }
                        if let Some(ref info) = mat.emissive_texture {
                            if let Some(tex) = textures.get(info.index) {
                                lines.push(
                                    t!(
                                        "viewer.material_list.tex_slot_emissive",
                                        name = tex.filename
                                    )
                                    .into_owned(),
                                );
                            }
                        }
                        if !lines.is_empty() {
                            cb.clone().on_hover_text(lines.join("\n"));
                        }
                    }
                }
                if cb.contains_pointer() { row_highlight = true; }
                row_highlight
                    });
                    // Row-hover detection -> highlight all draws of the same material (excluding hidden).
                    if row_resp.inner {
                        if let Some(ref loaded) = app.loaded {
                            for (di, d) in loaded.gpu_model.draws.iter().enumerate() {
                                if d.material_index == mat_idx
                                    && app.material_visibility.get(di).copied().unwrap_or(true)
                                {
                                    app.hovered_draw_indices.push(di);
                                }
                            }
                        }
                    }
                }
            });
        }
    }

    // -- File structure --
    show_file_tree(ui, app);

    // Deferred texture-history execution (after the `loaded` borrow is released).
    if do_save_history {
        app.do_save_texture_history();
    }
    if do_recall_history {
        app.do_recall_texture_history();
    }
}

/// File-structure tree: hierarchical display of the load chain (opened file -> intermediate -> final model).
fn show_file_tree(ui: &mut egui::Ui, app: &ViewerApp) {
    let Some(ref loaded) = app.loaded else { return };

    ui.add_space(12.0);
    ui.heading(
        egui::RichText::new(t!("viewer.section.file_structure"))
            .color(egui::Color32::from_gray(0xD0)),
    );
    ui.separator();

    let dir_color = egui::Color32::from_rgb(0xE0, 0xC0, 0x60);
    let file_color = egui::Color32::from_gray(0xC0);
    let tex_color = egui::Color32::from_rgb(0x80, 0xD0, 0x80);
    let anim_color = egui::Color32::from_rgb(0x80, 0xB0, 0xE0);
    let path_color = egui::Color32::from_gray(0x80);

    // -- Build the load chain --
    // Level 0: opened file (source).
    // Level 1: intermediate file (archive entry / Prefab).
    // Level 2: final model file (FBX group / single model).

    let source_path = loaded.source.display_path();
    let source_name = source_path
        .file_name()
        .map(|f| f.to_string_lossy().to_string())
        .unwrap_or_else(|| source_path.to_string_lossy().to_string());
    let source_full = source_path.to_string_lossy().to_string();

    // In-archive entry name (when going through ZIP / 7z).
    let archive_entry = if let super::app::ReloadableSource::Archive {
        selected_entry_path,
        ..
    } = &loaded.source
    {
        Some(selected_entry_path.clone())
    } else {
        None
    };

    // If there are multiple groups or a Prefab, show the group name as the final model file.
    let groups = &loaded.material_groups;
    let has_prefab = loaded.prefab_name.is_some();
    let has_multi_groups = groups.len() > 1;

    // -- Tree drawing --
    // Level 0: source file.
    egui::CollapsingHeader::new(egui::RichText::new(&source_name).color(dir_color).strong())
        .id_salt(ui.id().with("file_chain_root"))
        .default_open(true)
        .show(ui, |ui| {
            // Show path.
            ui.label(egui::RichText::new(&source_full).color(path_color).small());

            // Level 1: in-archive entry.
            if let Some(ref entry) = archive_entry {
                let entry_name = std::path::Path::new(entry)
                    .file_name()
                    .map(|f| f.to_string_lossy().to_string())
                    .unwrap_or_else(|| entry.clone());
                // The in-archive entry further holds a Prefab.
                if has_prefab {
                    egui::CollapsingHeader::new(egui::RichText::new(&entry_name).color(file_color))
                        .id_salt(ui.id().with("file_chain_archive_entry"))
                        .default_open(true)
                        .show(ui, |ui| {
                            show_prefab_subtree(ui, loaded, dir_color, file_color, tex_color);
                        });
                } else {
                    ui.label(egui::RichText::new(&entry_name).color(file_color));
                    // Textures.
                    show_texture_subtree(ui, loaded, groups, dir_color, tex_color);
                }
            } else if has_prefab {
                // Level 1: Prefab (directly under unitypackage).
                show_prefab_subtree(ui, loaded, dir_color, file_color, tex_color);
            } else if has_multi_groups {
                // Multiple groups (append, etc.): show textures per group.
                show_texture_subtree(ui, loaded, groups, dir_color, tex_color);
            } else {
                // Single model: show only textures.
                show_texture_subtree(ui, loaded, groups, dir_color, tex_color);
            }
        });

    // -- Appended models --
    for (ai, appended) in loaded.appended_models.iter().enumerate() {
        let ap = appended.source.display_path();
        let aname = ap
            .file_name()
            .map(|f| f.to_string_lossy().to_string())
            .unwrap_or_else(|| ap.to_string_lossy().to_string());
        egui::CollapsingHeader::new(
            egui::RichText::new(format!("+ {}", aname))
                .color(dir_color)
                .strong(),
        )
        .id_salt(ui.id().with(("file_chain_append", ai)))
        .default_open(false)
        .show(ui, |ui| {
            ui.label(
                egui::RichText::new(ap.to_string_lossy().to_string())
                    .color(path_color)
                    .small(),
            );
        });
    }

    // -- Animations --
    if !app.anim.library.is_empty() {
        let header = t!(
            "viewer.file_tree.animations",
            count = app.anim.library.len()
        );
        egui::CollapsingHeader::new(
            egui::RichText::new(header.as_ref())
                .color(anim_color)
                .strong(),
        )
        .id_salt(ui.id().with("file_chain_anim"))
        .default_open(false)
        .show(ui, |ui| {
            for (name, path, _) in &app.anim.library {
                ui.label(egui::RichText::new(name).color(file_color))
                    .on_hover_text(path.to_string_lossy().to_string());
            }
        });
    }

    // -- Package textures --
    if let Some(ref pkg) = app.tex.pkg_textures {
        if !pkg.is_empty() {
            let header = t!("viewer.file_tree.pkg_textures", count = pkg.len());
            egui::CollapsingHeader::new(
                egui::RichText::new(header.as_ref())
                    .color(dir_color)
                    .strong(),
            )
            .id_salt(ui.id().with("file_chain_pkg"))
            .default_open(false)
            .show(ui, |ui| {
                for (name, _) in pkg {
                    ui.label(egui::RichText::new(name).color(tex_color));
                }
            });
        }
    }
}

/// Prefab subtree: Prefab name -> FBX group (with textures).
fn show_prefab_subtree(
    ui: &mut egui::Ui,
    loaded: &super::app::LoadedModel,
    _dir_color: egui::Color32,
    file_color: egui::Color32,
    tex_color: egui::Color32,
) {
    let prefab_name = loaded.prefab_name.as_deref().unwrap_or("Prefab");
    let groups = &loaded.material_groups;

    egui::CollapsingHeader::new(egui::RichText::new(prefab_name).color(file_color))
        .id_salt(ui.id().with("file_chain_prefab"))
        .default_open(true)
        .show(ui, |ui| {
            for (gi, group) in groups.iter().enumerate() {
                // Collect per-group textures.
                let mut tex_indices = Vec::new();
                for mat_idx in group.material_range.clone() {
                    if let Some(mat) = loaded.ir.materials.get(mat_idx) {
                        collect_material_tex_indices(mat, &mut tex_indices);
                    }
                }
                tex_indices.sort();
                tex_indices.dedup();

                if tex_indices.is_empty() {
                    ui.label(egui::RichText::new(&group.name).color(file_color));
                } else {
                    egui::CollapsingHeader::new(
                        egui::RichText::new(format!("{} (tex: {})", group.name, tex_indices.len()))
                            .color(file_color),
                    )
                    .id_salt(ui.id().with(("file_chain_prefab_fbx", gi)))
                    .default_open(false)
                    .show(ui, |ui| {
                        for &ti in &tex_indices {
                            if let Some(tex) = loaded.ir.textures.get(ti) {
                                ui.label(egui::RichText::new(&tex.filename).color(tex_color));
                            }
                        }
                    });
                }
            }
        });
}

/// Texture subtree: shows textures per group or all textures.
fn show_texture_subtree(
    ui: &mut egui::Ui,
    loaded: &super::app::LoadedModel,
    groups: &[super::app::MaterialGroup],
    dir_color: egui::Color32,
    tex_color: egui::Color32,
) {
    let tex_count = loaded.ir.textures.len();
    if tex_count == 0 {
        return;
    }

    if groups.len() > 1 {
        // Multiple groups: show per group.
        for (gi, group) in groups.iter().enumerate() {
            let mut tex_indices = Vec::new();
            for mat_idx in group.material_range.clone() {
                if let Some(mat) = loaded.ir.materials.get(mat_idx) {
                    collect_material_tex_indices(mat, &mut tex_indices);
                }
            }
            tex_indices.sort();
            tex_indices.dedup();
            if tex_indices.is_empty() {
                continue;
            }
            let header = t!(
                "viewer.file_tree.textures_group",
                group = group.name,
                count = tex_indices.len(),
            );
            egui::CollapsingHeader::new(
                egui::RichText::new(header.as_ref())
                    .color(dir_color)
                    .strong(),
            )
            .id_salt(ui.id().with(("file_chain_tex_group", gi)))
            .default_open(false)
            .show(ui, |ui| {
                for &ti in &tex_indices {
                    if let Some(tex) = loaded.ir.textures.get(ti) {
                        ui.label(egui::RichText::new(&tex.filename).color(tex_color));
                    }
                }
            });
        }
    } else {
        // Single group: flat display.
        let header = t!("viewer.file_tree.textures_all", count = tex_count);
        egui::CollapsingHeader::new(
            egui::RichText::new(header.as_ref())
                .color(dir_color)
                .strong(),
        )
        .id_salt(ui.id().with("file_chain_tex_all"))
        .default_open(false)
        .show(ui, |ui| {
            for tex in &loaded.ir.textures {
                ui.label(egui::RichText::new(&tex.filename).color(tex_color));
            }
        });
    }
}

/// Collect all texture indices referenced by a material.
fn collect_material_tex_indices(
    mat: &crate::intermediate::types::IrMaterial,
    out: &mut Vec<usize>,
) {
    if let Some(idx) = mat.texture_index {
        if !out.contains(&idx) {
            out.push(idx);
        }
    }
    if let Some(ref info) = mat.base_color_tex_info {
        if !out.contains(&info.index) {
            out.push(info.index);
        }
    }
    if let Some(ref info) = mat.normal_texture {
        if !out.contains(&info.index) {
            out.push(info.index);
        }
    }
    if let Some(ref info) = mat.emissive_texture {
        if !out.contains(&info.index) {
            out.push(info.index);
        }
    }
    if let Some(idx) = mat.sphere_texture_index {
        if !out.contains(&idx) {
            out.push(idx);
        }
    }
    if let Some(idx) = mat.toon_texture_index {
        if !out.contains(&idx) {
            out.push(idx);
        }
    }
    // MToon additional textures.
    if let Some(ref mtoon) = mat.mtoon {
        for info in [
            &mtoon.shade_texture,
            &mtoon.shading_shift_texture,
            &mtoon.matcap_texture,
            &mtoon.rim_multiply_texture,
            &mtoon.outline_width_texture,
            &mtoon.uv_animation_mask_texture,
        ]
        .into_iter()
        .flatten()
        {
            if !out.contains(&info.index) {
                out.push(info.index);
            }
        }
    }
}

/// Export tab: PMX conversion + UV map export.
fn show_tab_export(ui: &mut egui::Ui, app: &mut ViewerApp) {
    let has_humanoid = app
        .loaded
        .as_ref()
        .is_some_and(|l| l.ir.humanoid_bone_count > 0);
    let has_physics = app
        .loaded
        .as_ref()
        .is_some_and(|l| !l.ir.physics.rigid_bodies.is_empty());
    let has_model = app.loaded.is_some();
    let is_pmx_pmd = app
        .loaded
        .as_ref()
        .is_some_and(|l| l.ir.source_format.is_pmx_pmd());
    let is_processing = app.pending.bg_state.is_active()
        || app.pending.convert.is_some()
        || app.pending.rebuild.is_some()
        || app.pending.reload.is_some()
        || app.pending.pkg_load.is_some()
        || app.export.pending_mkdir.is_some();

    ui.heading(
        egui::RichText::new(t!("viewer.section.pmx_export")).color(egui::Color32::from_gray(0xD0)),
    );
    ui.separator();

    // Output directory (where `converted_modelXX` is created).
    ui.horizontal(|ui| {
        ui.label(t!("viewer.export_tab.output_dir_label"));
        let dir_label = app
            .export
            .output_base_dir
            .as_ref()
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_else(|| t!("viewer.export_tab.output_dir_default").into_owned());
        ui.label(
            egui::RichText::new(&dir_label)
                .small()
                .color(egui::Color32::from_gray(0x60)),
        );
    });
    ui.horizontal(|ui| {
        // Do not relaunch while the dialog is open.
        let dialog_active = app.export.pending_folder_dialog.is_some();
        if ui
            .add_enabled(
                !dialog_active,
                egui::Button::new(t!("viewer.export_tab.folder_select_button")).small(),
            )
            .clicked()
        {
            let start_dir = app
                .export
                .output_base_dir
                .clone()
                .or_else(|| {
                    app.loaded
                        .as_ref()
                        .and_then(|l| l.source.display_path().parent().map(|p| p.to_path_buf()))
                })
                .unwrap_or_default();
            // Open the folder-select dialog on a separate thread (does not block the UI).
            let dialog_title = t!("viewer.export_tab.output_dir_dialog_title").into_owned();
            let (tx, rx) = std::sync::mpsc::channel();
            let repaint = ui.ctx().clone();
            std::thread::spawn(move || {
                let mut dialog = rfd::FileDialog::new().set_title(dialog_title);
                if start_dir.exists() {
                    dialog = dialog.set_directory(&start_dir);
                }
                let _ = tx.send(dialog.pick_folder());
                repaint.request_repaint();
            });
            app.export.pending_folder_dialog = Some(rx);
        }
        if app.export.output_base_dir.is_some()
            && ui
                .small_button(t!("viewer.export_tab.reset_button"))
                .clicked()
        {
            app.export.output_base_dir = None;
        }
    });

    // Model-name edit (reflected in both the title bar and the PMX output filename).
    // Greyed out when no model is loaded.
    ui.horizontal(|ui| {
        ui.label(t!("viewer.export_tab.model_name_label"));
        ui.add_enabled_ui(has_model && !is_processing, |ui| {
            let response = ui.add(
                egui::TextEdit::singleline(&mut app.export.model_display_name)
                    .desired_width(f32::INFINITY)
                    .hint_text(t!("viewer.export_tab.model_name_hint")),
            );
            if response.changed() {
                // Changes from `TextEdit` are user input, so reflect them as is without sanitization.
                // Apply to the title bar and PMX output filename immediately.
                app.refresh_derived_from_display_name();
            }
        });
    });
    ui.separator();

    // Grey out PMX-conversion controls while a PMX/PMD model is loaded.
    ui.add_enabled_ui(has_model && !is_processing && !is_pmx_pmd, |ui| {
        let convert_disabled_hint = if is_pmx_pmd {
            t!("viewer.export_tab.convert_disabled_pmx_pmd")
        } else if is_processing {
            t!("viewer.export_tab.convert_disabled_processing")
        } else {
            t!("viewer.export_tab.convert_disabled_no_model")
        };
        if ui
            .button(t!("viewer.export_tab.pmx_convert_button"))
            .on_disabled_hover_text(convert_disabled_hint)
            .clicked()
        {
            // Renumber `converted_modelXX` per conversion (avoids overwriting).
            if let Some(ref loaded) = app.loaded {
                let source_path = loaded.source.display_path();
                let base_dir =
                    app.export.output_base_dir.as_deref().unwrap_or_else(|| {
                        source_path.parent().unwrap_or(std::path::Path::new("."))
                    });
                let converted_dir = crate::next_converted_dir(base_dir);
                // Prefer the user-editable `model_display_name`; fall back only if unset.
                let pmx_stem = if !app.export.model_display_name.is_empty() {
                    app.export.model_display_name.clone()
                } else {
                    crate::sanitize_filename(&loaded.ir.name).unwrap_or_else(|| {
                        source_path
                            .file_stem()
                            .unwrap_or_default()
                            .to_string_lossy()
                            .into_owned()
                    })
                };
                let output_path = converted_dir.join(format!("{}.pmx", pmx_stem));
                app.export.pmx_output_path = output_path.to_string_lossy().into_owned();
                // Create the output directory on a BG thread (network-drive friendly).
                if let Some(dir) = output_path.parent() {
                    let dir = dir.to_path_buf();
                    let (tx, rx) = std::sync::mpsc::channel();
                    let repaint = ui.ctx().clone();
                    std::thread::spawn(move || {
                        let result = std::fs::create_dir_all(&dir);
                        let _ = tx.send(result.map_err(|e| format!("{e}")));
                        repaint.request_repaint();
                    });
                    app.export.pending_mkdir = Some(super::app::pending::PendingMkdir { rx });
                } else {
                    // No parent directory: start the conversion directly.
                    app.pending.convert = Some(PendingOverlay::WaitingOverlay);
                }
            }
        }
    });
    // For PMX/PMD: grey out T->A stance and show A->T stance instead.
    let is_fbx = app
        .loaded
        .as_ref()
        .is_some_and(|l| l.ir.source_format == crate::intermediate::types::SourceFormat::Fbx);
    if is_pmx_pmd {
        if ui
            .checkbox(
                &mut app.normalize_pose,
                t!("viewer.export_tab.pose_t_stance"),
            )
            .changed()
        {
            app.pending.reload = Some(PendingOverlay::WaitingOverlay);
        }
    } else {
        ui.add_enabled_ui(has_humanoid, |ui| {
            if ui
                .checkbox(
                    &mut app.normalize_pose,
                    t!("viewer.export_tab.pose_a_stance"),
                )
                .on_disabled_hover_text(t!("viewer.export_tab.pose_disabled_no_humanoid"))
                .changed()
            {
                // If A-stance conversion is ON, force T-stance conversion OFF.
                if app.normalize_pose {
                    app.normalize_to_tstance = false;
                }
                app.pending.reload = Some(PendingOverlay::WaitingOverlay);
            }
        });
        // For FBX: add an A->T stance conversion checkbox.
        if is_fbx {
            ui.add_enabled_ui(has_humanoid, |ui| {
                if ui
                    .checkbox(
                        &mut app.normalize_to_tstance,
                        t!("viewer.export_tab.pose_t_stance"),
                    )
                    .on_disabled_hover_text(t!("viewer.export_tab.pose_disabled_no_humanoid"))
                    .changed()
                {
                    // If T-stance conversion is ON, force A-stance conversion OFF.
                    if app.normalize_to_tstance {
                        app.normalize_pose = false;
                    }
                    app.pending.reload = Some(PendingOverlay::WaitingOverlay);
                }
            });
        }
    }
    // 2-column option grid.
    egui::Grid::new("export_options")
        .num_columns(2)
        .spacing([8.0, 2.0])
        .show(ui, |ui| {
            ui.add_enabled(
                has_physics && !is_pmx_pmd,
                egui::Checkbox::new(
                    &mut app.display.align_rigid_rotation,
                    t!("viewer.export_tab.align_rigid"),
                ),
            )
            .on_disabled_hover_text(t!("viewer.export_tab.physics_disabled"));
            ui.add_enabled(
                has_physics && !is_pmx_pmd,
                egui::Checkbox::new(
                    &mut app.export.no_physics,
                    t!("viewer.export_tab.no_physics"),
                ),
            )
            .on_disabled_hover_text(t!("viewer.export_tab.physics_disabled"));
            ui.end_row();

            ui.add_enabled(
                has_model && !is_pmx_pmd,
                egui::Checkbox::new(
                    &mut app.export.raw_structure,
                    t!("viewer.export_tab.raw_structure"),
                ),
            )
            .on_disabled_hover_text(t!("viewer.export_tab.raw_structure_disabled"));
            ui.add_enabled(
                has_model && !is_pmx_pmd,
                egui::Checkbox::new(
                    &mut app.export.export_visible_only,
                    t!("viewer.export_tab.visible_only"),
                ),
            );
            ui.end_row();

            ui.add_enabled(
                !is_pmx_pmd,
                egui::Checkbox::new(
                    &mut app.export.output_log,
                    t!("viewer.export_tab.output_log"),
                ),
            )
            .on_disabled_hover_text(t!("viewer.export_tab.output_log_disabled"));
            ui.label(t!("viewer.export_tab.scale_label"));
            ui.end_row();

            ui.add(
                egui::DragValue::new(&mut app.export.scale)
                    .speed(0.01)
                    .range(0.01..=100.0)
                    .suffix("x"),
            );
            if ui.small_button("1x").clicked() {
                app.export.scale = 1.0;
            }
            ui.end_row();
        });

    // MME (ray-mmd) - sub-menu of PMX conversion (§K.5 / Step 6).
    ui.add_enabled(
        has_model,
        egui::Checkbox::new(
            &mut app.export.output_mme,
            t!("viewer.export_tab.mme_output"),
        ),
    )
    .on_disabled_hover_text(t!("viewer.export_tab.mme_disabled_no_model"));
    if app.export.output_mme {
        ui.indent("mme_settings", |ui| {
            ui.horizontal(|ui| {
                ui.label(t!("viewer.export_tab.ray_mmd_root_label"));
                let dir_label = app
                    .app_config
                    .ray_mmd_root
                    .clone()
                    .unwrap_or_else(|| ".\\".to_string());
                ui.label(
                    egui::RichText::new(&dir_label)
                        .small()
                        .color(egui::Color32::from_gray(0x60)),
                );
            });
            ui.horizontal(|ui| {
                let dialog_active = app.export.pending_ray_mmd_dialog.is_some();
                if ui
                    .add_enabled(
                        !dialog_active,
                        egui::Button::new(t!("viewer.export_tab.folder_select_button")).small(),
                    )
                    .clicked()
                {
                    let start_dir = app
                        .app_config
                        .ray_mmd_root
                        .as_ref()
                        .map(std::path::PathBuf::from)
                        .unwrap_or_else(|| std::path::PathBuf::from("."));
                    let dialog_title = t!("viewer.export_tab.ray_mmd_dialog_title").into_owned();
                    let (tx, rx) = std::sync::mpsc::channel();
                    let repaint = ui.ctx().clone();
                    std::thread::spawn(move || {
                        let mut dialog = rfd::FileDialog::new().set_title(dialog_title);
                        if start_dir.exists() {
                            dialog = dialog.set_directory(&start_dir);
                        }
                        let _ = tx.send(dialog.pick_folder());
                        repaint.request_repaint();
                    });
                    app.export.pending_ray_mmd_dialog = Some(rx);
                }
                if app.app_config.ray_mmd_root.is_some()
                    && ui
                        .small_button(t!("viewer.export_tab.reset_button"))
                        .clicked()
                {
                    app.app_config.ray_mmd_root = None;
                }
            });
        });
    }

    ui.add_space(12.0);

    // UV map export.
    ui.heading(
        egui::RichText::new(t!("viewer.section.uvmap_export"))
            .color(egui::Color32::from_gray(0xD0)),
    );
    ui.separator();
    ui.add_enabled_ui(has_model && !is_processing, |ui| {
        // Do not relaunch while the dialog is open.
        let uv_dialog_active = app.export.pending_uv_dialog.is_some();
        if ui
            .add_enabled(
                !uv_dialog_active,
                egui::Button::new(t!("viewer.export_tab.uvmap_button")),
            )
            .clicked()
        {
            // Default directory: the directory where the model was loaded (source-file's parent).
            // For archives, `display_path` points to the archive itself, so
            // the directory containing the archive is automatically used.
            let default_dir = app.loaded.as_ref().map(|l| {
                l.source
                    .display_path()
                    .parent()
                    .unwrap_or(std::path::Path::new("."))
                    .to_path_buf()
            });
            // Default filename: `model_display_name` if set; otherwise "uvmap".
            let file_name = Some(if app.export.model_display_name.is_empty() {
                "uvmap.psd".to_string()
            } else {
                format!("{}.psd", app.export.model_display_name)
            });
            // Capture material-group info to be used after the dialog returns.
            let uv_groups: Vec<(String, std::ops::Range<usize>)> =
                if let Some(ref loaded) = app.loaded {
                    loaded
                        .material_groups
                        .iter()
                        .map(|g| (g.name.clone(), g.material_range.clone()))
                        .collect()
                } else {
                    Vec::new()
                };
            let uv_map_size = app.export.uv_map_size;
            // Open the save dialog on a separate thread (does not block the UI).
            let dialog_title = t!("viewer.export_tab.uvmap_dialog_title").into_owned();
            let (tx, rx) = std::sync::mpsc::channel();
            let repaint = ui.ctx().clone();
            std::thread::spawn(move || {
                let mut dialog = rfd::FileDialog::new()
                    .set_title(dialog_title)
                    .add_filter("PSD", &["psd"]);
                if let Some(dir) = default_dir.as_deref() {
                    dialog = dialog.set_directory(dir);
                }
                if let Some(ref name) = file_name {
                    dialog = dialog.set_file_name(name);
                }
                let _ = tx.send(dialog.save_file());
                repaint.request_repaint();
            });
            app.export.pending_uv_dialog = Some(super::app::pending::PendingUvExport {
                rx,
                uv_map_size,
                uv_groups,
            });
        }
    });
    ui.horizontal(|ui| {
        ui.label(t!("viewer.export_tab.resolution_label"));
        egui::ComboBox::from_id_salt("uv_size")
            .selected_text(format!("{}", app.export.uv_map_size))
            .width(70.0)
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut app.export.uv_map_size, 1024, "1024");
                ui.selectable_value(&mut app.export.uv_map_size, 2048, "2048");
                ui.selectable_value(&mut app.export.uv_map_size, 4096, "4096");
                ui.selectable_value(&mut app.export.uv_map_size, 8192, "8192");
            });
    });
}

/// Badge variants for permission values.
enum MetaBadge {
    /// Allow (green badge).
    Allow,
    /// Warn (yellow badge).
    Warn,
    /// Deny (red badge).
    Deny,
    /// Neutral (grey badge).
    Neutral,
}

impl MetaBadge {
    fn colors(&self) -> (egui::Color32, egui::Color32) {
        match self {
            MetaBadge::Allow => (
                egui::Color32::from_rgb(0x20, 0x60, 0x20),
                egui::Color32::from_rgb(0x80, 0xFF, 0x80),
            ),
            MetaBadge::Warn => (
                egui::Color32::from_rgb(0x60, 0x50, 0x10),
                egui::Color32::from_rgb(0xFF, 0xE0, 0x60),
            ),
            MetaBadge::Deny => (
                egui::Color32::from_rgb(0x60, 0x18, 0x18),
                egui::Color32::from_rgb(0xFF, 0x80, 0x80),
            ),
            MetaBadge::Neutral => (
                egui::Color32::from_rgb(0x40, 0x40, 0x40),
                egui::Color32::from_gray(0xA0),
            ),
        }
    }

    fn rich_text(&self, text: &str) -> egui::RichText {
        let (bg, fg) = self.colors();
        egui::RichText::new(format!(" {text} "))
            .background_color(bg)
            .color(fg)
    }
}

/// Format a VRM meta value into a colored badge + tooltip.
/// Returns (display RichText, optional tooltip text).
fn format_meta_value(value: &str) -> (egui::RichText, Option<std::borrow::Cow<'static, str>>) {
    match value {
        // VRM 1.0 bool fields
        "true" => (
            MetaBadge::Allow.rich_text("allow"),
            Some(t!("viewer.meta.value.bool_true")),
        ),
        "false" => (
            MetaBadge::Deny.rich_text("disallow"),
            Some(t!("viewer.meta.value.bool_false")),
        ),
        // VRM 0.0 usage values
        "Allow" => (
            MetaBadge::Allow.rich_text("Allow"),
            Some(t!("viewer.meta.value.allow")),
        ),
        "Disallow" => (
            MetaBadge::Deny.rich_text("Disallow"),
            Some(t!("viewer.meta.value.disallow")),
        ),
        // VRM 0.0 / 1.0 avatar permission
        "OnlyAuthor" | "onlyAuthor" => (
            MetaBadge::Warn.rich_text("OnlyAuthor"),
            Some(t!("viewer.meta.value.only_author")),
        ),
        "Everyone" | "everyone" => (
            MetaBadge::Allow.rich_text("Everyone"),
            Some(t!("viewer.meta.value.everyone")),
        ),
        "ExplicitlyLicensedPerson" | "onlySeparatelyLicensedPerson" => (
            MetaBadge::Warn.rich_text("SeparatelyLicensed"),
            Some(t!("viewer.meta.value.explicitly_licensed")),
        ),
        // VRM 1.0 commercial usage
        "personalNonProfit" => (
            MetaBadge::Deny.rich_text("personalNonProfit"),
            Some(t!("viewer.meta.value.personal_non_profit")),
        ),
        "personalProfit" => (
            MetaBadge::Warn.rich_text("personalProfit"),
            Some(t!("viewer.meta.value.personal_profit")),
        ),
        "corporation" => (
            MetaBadge::Allow.rich_text("corporation"),
            Some(t!("viewer.meta.value.corporation")),
        ),
        // VRM 1.0 credit notation
        "required" => (
            MetaBadge::Warn.rich_text("required"),
            Some(t!("viewer.meta.value.required")),
        ),
        "unnecessary" => (
            MetaBadge::Neutral.rich_text("unnecessary"),
            Some(t!("viewer.meta.value.unnecessary")),
        ),
        // VRM 1.0 modification
        "prohibited" => (
            MetaBadge::Deny.rich_text("prohibited"),
            Some(t!("viewer.meta.value.prohibited")),
        ),
        "allowModification" => (
            MetaBadge::Allow.rich_text("allowModification"),
            Some(t!("viewer.meta.value.allow_modification")),
        ),
        "allowModificationRedistribution" => (
            MetaBadge::Allow.rich_text("allowModificationRedistribution"),
            Some(t!("viewer.meta.value.allow_modification_redistribution")),
        ),
        // VRM 0.0 license
        "Redistribution_Prohibited" => (
            MetaBadge::Deny.rich_text("Redistribution_Prohibited"),
            Some(t!("viewer.meta.value.redistribution_prohibited")),
        ),
        "CC0" => (
            MetaBadge::Allow.rich_text("CC0"),
            Some(t!("viewer.meta.value.cc0")),
        ),
        "CC_BY" => (
            MetaBadge::Allow.rich_text("CC_BY"),
            Some(t!("viewer.meta.value.cc_by")),
        ),
        "CC_BY_NC" => (
            MetaBadge::Warn.rich_text("CC_BY_NC"),
            Some(t!("viewer.meta.value.cc_by_nc")),
        ),
        "CC_BY_SA" => (
            MetaBadge::Allow.rich_text("CC_BY_SA"),
            Some(t!("viewer.meta.value.cc_by_sa")),
        ),
        "CC_BY_NC_SA" => (
            MetaBadge::Warn.rich_text("CC_BY_NC_SA"),
            Some(t!("viewer.meta.value.cc_by_nc_sa")),
        ),
        "CC_BY_ND" => (
            MetaBadge::Warn.rich_text("CC_BY_ND"),
            Some(t!("viewer.meta.value.cc_by_nd")),
        ),
        "CC_BY_NC_ND" => (
            MetaBadge::Deny.rich_text("CC_BY_NC_ND"),
            Some(t!("viewer.meta.value.cc_by_nc_nd")),
        ),
        "Other" => (
            MetaBadge::Neutral.rich_text("Other"),
            Some(t!("viewer.meta.value.other")),
        ),
        _ => (egui::RichText::new(value), None),
    }
}

/// Translate an English section title into the active locale.
fn meta_section_ja(title: &str) -> std::borrow::Cow<'static, str> {
    match title {
        "Model Info" => t!("viewer.meta.section.model_info"),
        "Author" => t!("viewer.meta.section.author"),
        "Permissions" => t!("viewer.meta.section.permissions"),
        "License" => t!("viewer.meta.section.license"),
        _ => std::borrow::Cow::Owned(title.to_string()),
    }
}

/// Translate an English field label into the active locale.
fn meta_label_ja(label: &str) -> std::borrow::Cow<'static, str> {
    match label {
        // Model Info
        "model name" => t!("viewer.meta.label.model_name"),
        "version" => t!("viewer.meta.label.version"),
        // Author
        "author" => t!("viewer.meta.label.author"),
        "contact information" => t!("viewer.meta.label.contact_information"),
        "reference" => t!("viewer.meta.label.reference"),
        "copyright information" => t!("viewer.meta.label.copyright_information"),
        "third party licenses" => t!("viewer.meta.label.third_party_licenses"),
        // VRM 0.0 Permissions
        "allowed user" => t!("viewer.meta.label.allowed_user"),
        "violent ussage" => t!("viewer.meta.label.violent_ussage"),
        "sexual ussage" => t!("viewer.meta.label.sexual_ussage"),
        "commercial ussage" | "commercial usage" => t!("viewer.meta.label.commercial_ussage"),
        "other permission" => t!("viewer.meta.label.other_permission"),
        // VRM 1.0 Permissions
        "avatar permission" => t!("viewer.meta.label.avatar_permission"),
        "violent usage" => t!("viewer.meta.label.violent_usage"),
        "sexual usage" => t!("viewer.meta.label.sexual_usage"),
        "political/religious" => t!("viewer.meta.label.political_religious"),
        "antisocial/hate" => t!("viewer.meta.label.antisocial_hate"),
        "credit notation" => t!("viewer.meta.label.credit_notation"),
        "redistribution" => t!("viewer.meta.label.redistribution"),
        "modification" => t!("viewer.meta.label.modification"),
        // License
        "license" => t!("viewer.meta.label.license"),
        "other license" => t!("viewer.meta.label.other_license"),
        _ => std::borrow::Cow::Owned(label.to_string()),
    }
}

/// Tooltip for permission / license labels (left column).
fn meta_label_tooltip(label: &str) -> Option<std::borrow::Cow<'static, str>> {
    match label {
        // VRM 0.0 Permissions
        "allowed user" => Some(t!("viewer.meta.label_tooltip.allowed_user")),
        "violent ussage" => Some(t!("viewer.meta.label_tooltip.violent_ussage")),
        "sexual ussage" => Some(t!("viewer.meta.label_tooltip.sexual_ussage")),
        "commercial ussage" | "commercial usage" => {
            Some(t!("viewer.meta.label_tooltip.commercial_ussage"))
        }
        "other permission" => Some(t!("viewer.meta.label_tooltip.other_permission")),
        // License
        "license" => Some(t!("viewer.meta.label_tooltip.license")),
        "other license" => Some(t!("viewer.meta.label_tooltip.other_license")),
        // VRM 1.0 Permissions
        "avatar permission" => Some(t!("viewer.meta.label_tooltip.avatar_permission")),
        "violent usage" => Some(t!("viewer.meta.label_tooltip.violent_usage")),
        "sexual usage" => Some(t!("viewer.meta.label_tooltip.sexual_usage")),
        "political/religious" => Some(t!("viewer.meta.label_tooltip.political_religious")),
        "antisocial/hate" => Some(t!("viewer.meta.label_tooltip.antisocial_hate")),
        "credit notation" => Some(t!("viewer.meta.label_tooltip.credit_notation")),
        "redistribution" => Some(t!("viewer.meta.label_tooltip.redistribution")),
        "modification" => Some(t!("viewer.meta.label_tooltip.modification")),
        _ => None,
    }
}

fn show_meta_info(ui: &mut egui::Ui, comment: &str) {
    // Comment format: a "[Section]" line is a section separator, "  label: value" lines are fields.
    // Parse per section first.
    struct Section {
        title: String,
        fields: Vec<(String, String)>,
    }

    let mut sections: Vec<Section> = Vec::new();
    for line in comment.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if line.starts_with('[') && line.ends_with(']') {
            sections.push(Section {
                title: line[1..line.len() - 1].to_string(),
                fields: Vec::new(),
            });
        } else if let Some(pos) = line.find(':') {
            let label = line[..pos].trim().to_string();
            let value = line[pos + 1..].trim().to_string();
            if !value.is_empty() {
                if let Some(sec) = sections.last_mut() {
                    sec.fields.push((label, value));
                }
            }
        }
    }

    for (i, sec) in sections.iter().enumerate() {
        if sec.fields.is_empty() {
            continue;
        }
        let id = egui::Id::new(format!("meta_section_{i}"));
        let title_ja = meta_section_ja(&sec.title);
        egui::CollapsingHeader::new(title_ja)
            .id_salt(id)
            .default_open(true)
            .show(ui, |ui| {
                egui::Grid::new(format!("meta_grid_{i}"))
                    .num_columns(2)
                    .spacing([8.0, 4.0])
                    .show(ui, |ui| {
                        for (label, value) in &sec.fields {
                            let label_ja = meta_label_ja(label);
                            let label_resp = ui.label(label_ja);
                            if let Some(tip) = meta_label_tooltip(label) {
                                label_resp.on_hover_text(tip);
                            }
                            let (rich, tooltip) = format_meta_value(value);
                            let resp = ui.label(rich);
                            if let Some(tip) = tooltip {
                                resp.on_hover_text(tip);
                            }
                            ui.end_row();
                        }
                    });
            });
    }
}

/// Read from the in-memory log buffer starting at the cumulative offset (drain-safe).
fn read_log_buffer_from_offset(buffer: &crate::SharedLogBuffer, offset: usize) -> Option<String> {
    let lb = buffer.lock().ok()?;
    lb.read_from_offset(offset)
}

/// Write the conversion log to a file.
fn write_convert_log(
    log_path: &Path,
    ir: &crate::intermediate::types::IrModel,
    result: Result<&crate::ConvertStats, &crate::error::PoponeError>,
    debug_logs: Option<&str>,
) {
    use std::io::Write;

    let mut file = match std::fs::File::create(log_path) {
        Ok(f) => f,
        Err(e) => {
            log::warn!("Failed to create log file: {e}");
            return;
        }
    };

    let _ = writeln!(file, "[vrm-viewer] PMX conversion log");
    let _ = writeln!(
        file,
        "Date: {}",
        chrono::Local::now().format("%Y-%m-%d %H:%M:%S")
    );
    let _ = writeln!(file, "Source format: {}", ir.source_format.label());
    let _ = writeln!(file);

    // Input model info
    let _ = writeln!(file, "=== Input model ===");
    let _ = writeln!(file, "Model name: {}", ir.name);
    let _ = writeln!(file, "Bones: {}", ir.bones.len());
    let _ = writeln!(file, "Vertices: {}", ir.total_vertices());
    let _ = writeln!(file, "Faces: {}", ir.total_faces());
    let _ = writeln!(file, "Materials: {}", ir.materials.len());
    let _ = writeln!(file, "Textures: {}", ir.textures.len());
    let _ = writeln!(file, "Morphs: {}", ir.morphs.len());
    let _ = writeln!(file, "Rigid bodies: {}", ir.physics.rigid_bodies.len());
    let _ = writeln!(file, "Joints: {}", ir.physics.joints.len());

    // Bone list
    let _ = writeln!(file);
    let _ = writeln!(file, "--- Bone list ---");
    for (i, bone) in ir.bones.iter().enumerate() {
        let vrm_name = bone.vrm_bone_name.as_deref().unwrap_or("-");
        let _ = writeln!(file, "  [{:3}] {} (vrm: {})", i, bone.name, vrm_name);
    }

    // Morph list
    let _ = writeln!(file);
    let _ = writeln!(file, "--- Morph list ---");
    for morph in &ir.morphs {
        let _ = writeln!(file, "  [panel{}] {}", morph.panel, morph.name);
    }

    // Material list
    let _ = writeln!(file);
    let _ = writeln!(file, "--- Material list ---");
    for (i, mat) in ir.materials.iter().enumerate() {
        let _ = writeln!(
            file,
            "  [{:2}] {} (tex:{:?} double:{} shader:{})",
            i,
            mat.name,
            mat.texture_index,
            mat.cull_mode != CullMode::Back,
            mat.shader_family,
        );
    }

    // Meta info
    if !ir.comment.is_empty() {
        let _ = writeln!(file);
        let _ = writeln!(file, "=== Meta info ===");
        let _ = writeln!(file, "{}", ir.comment.replace("\r\n", "\n"));
    }

    // Conversion result
    let _ = writeln!(file);
    let _ = writeln!(file, "=== Conversion result ===");
    match result {
        Ok(stats) => {
            let _ = writeln!(file, "Output: {}", stats.output_path);
            let _ = writeln!(file, "Texture dir: {}", stats.tex_dir);
            let _ = writeln!(file, "PMX bones: {}", stats.bones);
            let _ = writeln!(file, "PMX vertices: {}", stats.vertices);
            let _ = writeln!(file, "PMX faces: {}", stats.faces);
            let _ = writeln!(file, "PMX materials: {}", stats.materials);
            let _ = writeln!(file, "PMX textures: {}", stats.textures);
            let _ = writeln!(file, "PMX morphs: {}", stats.morphs);
        }
        Err(e) => {
            let _ = writeln!(file, "Conversion failed: {e}");
        }
    }

    // Debug log appendix
    if let Some(logs) = debug_logs {
        let _ = writeln!(file);
        let _ = writeln!(file, "=== Debug log ===");
        let _ = write!(file, "{}", logs);
    }
}

/// Animation playback controls UI.
fn show_animation_controls(ui: &mut egui::Ui, app: &mut ViewerApp) {
    use super::animation::LoopMode;

    ui.heading(
        egui::RichText::new(t!("viewer.section.animation")).color(egui::Color32::from_gray(0xD0)),
    );
    ui.separator();

    // VRMA library.
    if !app.anim.library.is_empty() {
        ui.label(t!(
            "viewer.animation.library_count",
            count = app.anim.library.len()
        ));
        let mut switch_to: Option<usize> = None;
        let mut remove_idx: Option<usize> = None;
        for (i, (name, _, _)) in app.anim.library.iter().enumerate() {
            ui.horizontal(|ui| {
                let is_active = app.anim.active_index == Some(i);
                // [play][x] filename (clicking play switches).
                let play_icon = if is_active {
                    egui::RichText::new("▶").color(egui::Color32::from_rgb(0x4A, 0x90, 0xD9))
                } else {
                    egui::RichText::new("▶").color(egui::Color32::from_gray(0x60))
                };
                if ui.small_button(play_icon).clicked() && !is_active {
                    switch_to = Some(i);
                }
                if ui.small_button("×").clicked() {
                    remove_idx = Some(i);
                }
                ui.label(name.as_str());
            });
        }
        if let Some(idx) = switch_to {
            app.switch_vrma(idx);
        }
        if let Some(idx) = remove_idx {
            let was_active = app.anim.active_index == Some(idx);
            app.anim.library.remove(idx);
            if was_active {
                if app.anim.library.is_empty() {
                    // Pose reset (same processing as animation unbind).
                    if let Some(ref anim) = app.anim.state {
                        if let Some(ref mut loaded) = app.loaded {
                            for (i, morph) in loaded.ir.morphs.iter().enumerate() {
                                if anim
                                    .animation
                                    .expression_channels
                                    .contains_key(&morph.name_en)
                                {
                                    if let Some(w) = app.morph_weights.get_mut(i) {
                                        *w = 0.0;
                                    }
                                }
                            }
                            loaded.gpu_model.invalidate_morph_cache();
                        }
                    }
                    app.anim.state = None;
                    app.anim.active_index = None;
                    app.morph_dirty = true;
                } else {
                    let new_idx = idx.min(app.anim.library.len() - 1);
                    app.switch_vrma(new_idx);
                }
            } else if let Some(ref mut ai) = app.anim.active_index {
                if *ai > idx {
                    *ai -= 1;
                }
            }
        }
        ui.separator();
    }

    let mut switch_anim: Option<bool> = None;
    let mut muscle_scale_changed = false;

    if let Some(ref mut anim) = app.anim.state {
        ui.label(t!(
            "viewer.animation.name_duration",
            name = anim.animation.name,
            duration = format!("{:.1}", anim.animation.duration)
        ));

        ui.horizontal(|ui| {
            if ui
                .button("⏮")
                .on_hover_text(t!("viewer.animation.prev_or_restart_tooltip"))
                .clicked()
            {
                let (lo, _) = anim.effective_range();
                if anim.current_time - lo < 0.5 && app.anim.library.len() > 1 {
                    switch_anim = Some(false);
                } else {
                    anim.current_time = lo;
                    anim.ping_pong_direction = 1.0;
                }
            }
            let step_back = ui
                .add_enabled(
                    !anim.playing,
                    egui::Button::new("|◀").min_size(egui::vec2(24.0, 0.0)),
                )
                .on_hover_text(t!("viewer.animation.step_back_tooltip"));
            if step_back.clicked() {
                anim.step_frame(false);
            }
            if ui
                .button("◀")
                .on_hover_text(t!("viewer.animation.play_reverse_tooltip"))
                .clicked()
            {
                anim.speed = -anim.speed.abs();
                anim.playing = true;
            }
            let play_label = if anim.playing { "⏸" } else { "▶" };
            if ui.button(play_label).clicked() {
                if !anim.playing && anim.speed < 0.0 {
                    anim.speed = anim.speed.abs();
                }
                anim.playing = !anim.playing;
            }
            let step_fwd = ui
                .add_enabled(
                    !anim.playing,
                    egui::Button::new("▶|").min_size(egui::vec2(24.0, 0.0)),
                )
                .on_hover_text(t!("viewer.animation.step_fwd_tooltip"));
            if step_fwd.clicked() {
                anim.step_frame(true);
            }
            let has_next = app.anim.library.len() > 1;
            let next_btn = ui
                .add_enabled(has_next, egui::Button::new("⏭"))
                .on_hover_text(t!("viewer.animation.next_tooltip"));
            if next_btn.clicked() {
                switch_anim = Some(true);
            }
        });

        let duration = anim.animation.duration;
        if duration > 0.0 {
            ui.horizontal(|ui| {
                ui.label(format!("{:.2}s", anim.current_time));
                ui.add(egui::Slider::new(&mut anim.current_time, 0.0..=duration).show_value(false));
            });
            if (anim.loop_mode == LoopMode::AB || anim.loop_mode == LoopMode::PingPong)
                && (anim.ab_start.is_some() || anim.ab_end.is_some())
            {
                let a_str = anim
                    .ab_start
                    .map_or("-".to_string(), |t| format!("{:.2}s", t));
                let b_str = anim
                    .ab_end
                    .map_or("-".to_string(), |t| format!("{:.2}s", t));
                ui.label(format!("A:{} B:{}", a_str, b_str));
            }
        }

        ui.horizontal(|ui| {
            ui.label(t!("viewer.animation.speed_label"));
            ui.add(
                egui::DragValue::new(&mut anim.speed)
                    .range(-3.0..=3.0)
                    .speed(0.05)
                    .fixed_decimals(1)
                    .suffix("x"),
            );
        });

        // Adjust Unity .anim muscle scale (only when `is_additive`).
        if anim.animation.is_additive {
            ui.horizontal(|ui| {
                ui.label(t!("viewer.animation.muscle_scale_label"));
                let old_scale = app.anim.muscle_scale;
                let response = ui.add(
                    egui::DragValue::new(&mut app.anim.muscle_scale)
                        .range(0.01..=2.0)
                        .speed(0.01)
                        .fixed_decimals(2),
                );
                // Reload only when DragValue is committed (drag release or Enter).
                if (app.anim.muscle_scale - old_scale).abs() > 1e-6
                    && (response.drag_stopped() || response.lost_focus())
                {
                    muscle_scale_changed = true;
                }
            });
        }

        ui.horizontal(|ui| {
            ui.label(t!("viewer.animation.loop_label"));
            let loop_label_none = t!("viewer.animation.loop_mode.none");
            let loop_label_normal = t!("viewer.animation.loop_mode.normal");
            let loop_label_ping_pong = t!("viewer.animation.loop_mode.ping_pong");
            let selected_label: std::borrow::Cow<'static, str> = match anim.loop_mode {
                LoopMode::None => loop_label_none.clone(),
                LoopMode::Normal => loop_label_normal.clone(),
                LoopMode::AB => std::borrow::Cow::Borrowed("A-B"),
                LoopMode::PingPong => loop_label_ping_pong.clone(),
            };
            egui::ComboBox::from_id_salt("loop_mode")
                .selected_text(selected_label)
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut anim.loop_mode, LoopMode::None, loop_label_none);
                    ui.selectable_value(&mut anim.loop_mode, LoopMode::Normal, loop_label_normal);
                    ui.selectable_value(&mut anim.loop_mode, LoopMode::AB, "A-B");
                    ui.selectable_value(
                        &mut anim.loop_mode,
                        LoopMode::PingPong,
                        loop_label_ping_pong,
                    );
                });
        });

        if anim.loop_mode == LoopMode::AB || anim.loop_mode == LoopMode::PingPong {
            ui.horizontal(|ui| {
                if ui
                    .small_button("𝄆")
                    .on_hover_text(t!("viewer.animation.ab_start_tooltip"))
                    .clicked()
                {
                    anim.ab_start = Some(anim.current_time);
                }
                if ui
                    .small_button("𝄇")
                    .on_hover_text(t!("viewer.animation.ab_end_tooltip"))
                    .clicked()
                {
                    anim.ab_end = Some(anim.current_time);
                }
                if ui.small_button(t!("viewer.animation.ab_clear")).clicked() {
                    anim.ab_start = None;
                    anim.ab_end = None;
                }
            });
        }

        ui.label(t!(
            "viewer.animation.bone_expr_stats",
            bones = anim.animation.bone_channels.len(),
            expressions = anim.animation.expression_channels.len()
        ));

        if ui.small_button(t!("viewer.animation.unbind")).clicked() {
            // Reset morph weights controlled by the animation to 0.
            if let Some(ref mut loaded) = app.loaded {
                for (i, morph) in loaded.ir.morphs.iter().enumerate() {
                    if anim
                        .animation
                        .expression_channels
                        .contains_key(&morph.name_en)
                    {
                        if let Some(w) = app.morph_weights.get_mut(i) {
                            *w = 0.0;
                        }
                    }
                }
                // Invalidate the cache to reset vertices deformed by bone animation.
                loaded.gpu_model.invalidate_morph_cache();
            }
            app.anim.state = None;
            app.anim.active_index = None;
            app.morph_dirty = true;
        }
    } else {
        ui.label(t!("viewer.animation.drop_to_load"));
        if app.loaded.is_some()
            && ui
                .small_button(t!("viewer.animation.open_button"))
                .clicked()
        {
            let paths = rfd::FileDialog::new()
                .set_title(t!("viewer.animation.open_dialog_title"))
                .add_filter(
                    t!("viewer.animation.filter_label"),
                    &["vrma", "glb", "gltf", "fbx"],
                )
                .add_filter("VRMA (.vrma)", &["vrma"])
                .add_filter("GLB (.glb)", &["glb"])
                .add_filter("glTF (.gltf)", &["gltf"])
                .add_filter("FBX (.fbx)", &["fbx"])
                .pick_files()
                .unwrap_or_default();
            for path in &paths {
                app.load_animation_file(path);
            }
        }
    }

    if let Some(is_next) = switch_anim {
        if let Some(active) = app.anim.active_index {
            let len = app.anim.library.len();
            if len > 1 {
                let new_idx = if is_next {
                    (active + 1) % len
                } else {
                    (active + len - 1) % len
                };
                app.switch_vrma(new_idx);
            }
        }
    }

    // Muscle-scale change -> reload .anim.
    if muscle_scale_changed {
        if let Some(idx) = app.anim.active_index {
            let path = app.anim.library[idx].1.clone();
            if path
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("anim"))
            {
                // Save the current playback position / state.
                let (cur_time, was_playing) = app
                    .anim
                    .state
                    .as_ref()
                    .map(|s| (s.current_time, s.playing))
                    .unwrap_or((0.0, false));
                if let Ok(new_anim) =
                    crate::unity::animation::load_unity_anim(&path, app.anim.muscle_scale)
                {
                    let new_anim = std::sync::Arc::new(new_anim);
                    if let Some(ref loaded) = app.loaded {
                        let mut state = super::animation::AnimationState::new(
                            std::sync::Arc::clone(&new_anim),
                            &loaded.ir,
                            &loaded.gpu_model,
                        );
                        state.current_time = cur_time;
                        state.playing = was_playing;
                        app.anim.library[idx].2 = new_anim;
                        app.anim.state = Some(state);
                    }
                }
            }
        }
    }

    if app.loaded.is_some()
        && app.anim.state.is_some()
        && ui
            .small_button(t!("viewer.animation.append_button"))
            .clicked()
    {
        let paths = rfd::FileDialog::new()
            .set_title(t!("viewer.animation.append_dialog_title"))
            .add_filter(
                t!("viewer.animation.filter_label"),
                &["vrma", "glb", "gltf", "fbx"],
            )
            .add_filter("VRMA (.vrma)", &["vrma"])
            .add_filter("GLB (.glb)", &["glb"])
            .add_filter("glTF (.gltf)", &["gltf"])
            .add_filter("FBX (.fbx)", &["fbx"])
            .pick_files()
            .unwrap_or_default();
        for path in &paths {
            let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
            match ext.to_lowercase().as_str() {
                "glb" | "gltf" => app.try_load_gltf_animation(path),
                "fbx" => app.try_load_fbx_animation(path),
                "anim" => app.try_load_unity_animation(path),
                _ => app.try_load_vrma(path),
            }
        }
    }
}

// --- HSV color wheel widget ---

/// A button that pops up a Hue ring + SV square color wheel.
/// `rgb` is linear `[f32; 3]`.
fn color_wheel_button_rgb(ui: &mut egui::Ui, label: &str, rgb: &mut [f32; 3]) {
    let popup_id = ui.make_persistent_id(label);
    let open = ui.memory(|mem| mem.is_popup_open(popup_id));

    let color32 = Color32::from_rgb(
        linear_to_srgb_u8(rgb[0]),
        linear_to_srgb_u8(rgb[1]),
        linear_to_srgb_u8(rgb[2]),
    );
    let size = egui::vec2(18.0, 18.0);
    let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click());
    if ui.is_rect_visible(rect) {
        let visuals = ui.style().interact(&response);
        let rounding = visuals.corner_radius;
        ui.painter()
            .rect_filled(rect.expand(1.0), rounding, visuals.bg_fill);
        ui.painter().rect_filled(rect, rounding, color32);
        if open {
            ui.painter().rect_stroke(
                rect,
                rounding,
                egui::Stroke::new(1.0, Color32::WHITE),
                egui::StrokeKind::Outside,
            );
        }
    }

    if response.clicked() {
        ui.memory_mut(|mem| mem.toggle_popup(popup_id));
    }

    if ui.memory(|mem| mem.is_popup_open(popup_id)) {
        let area_response = egui::Area::new(popup_id)
            .kind(egui::UiKind::Picker)
            .order(egui::Order::Foreground)
            .fixed_pos(response.rect.max)
            .show(ui.ctx(), |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    hsv_wheel_picker(ui, rgb);
                });
            })
            .response;

        if !response.clicked()
            && (ui.input(|i| i.key_pressed(egui::Key::Escape)) || area_response.clicked_elsewhere())
        {
            ui.memory_mut(|mem| mem.close_popup());
        }
    }
}

/// HSV wheel body: Hue ring + SV square.
fn hsv_wheel_picker(ui: &mut egui::Ui, rgb: &mut [f32; 3]) {
    let hsv = rgb_to_hsv(*rgb);
    let mut h = hsv[0];
    let mut s = hsv[1];
    let mut v = hsv[2];

    let wheel_radius = 90.0_f32;
    let ring_width = 16.0_f32;
    let inner_radius = wheel_radius - ring_width;
    // SV square: inscribed in the inner circle.
    let sq_half = inner_radius * 0.65;
    let total_size = egui::vec2(wheel_radius * 2.0 + 8.0, wheel_radius * 2.0 + 8.0);

    let (rect, _) = ui.allocate_exact_size(total_size, egui::Sense::hover());
    let center = rect.center();
    let painter = ui.painter_at(rect);

    // -- Hue ring drawing (triangle mesh) --
    let segments = 64;
    let mut hue_mesh = Mesh::default();
    for i in 0..segments {
        let a0 = std::f32::consts::TAU * (i as f32 / segments as f32);
        let a1 = std::f32::consts::TAU * ((i + 1) as f32 / segments as f32);
        let hue0 = i as f32 / segments as f32;
        let hue1 = (i + 1) as f32 / segments as f32;
        let c0 = hsv_to_color32(hue0, 1.0, 1.0);
        let c1 = hsv_to_color32(hue1, 1.0, 1.0);

        let outer0 = center + egui::vec2(a0.cos(), -a0.sin()) * wheel_radius;
        let inner0 = center + egui::vec2(a0.cos(), -a0.sin()) * inner_radius;
        let outer1 = center + egui::vec2(a1.cos(), -a1.sin()) * wheel_radius;
        let inner1 = center + egui::vec2(a1.cos(), -a1.sin()) * inner_radius;

        let base = hue_mesh.vertices.len() as u32;
        hue_mesh.vertices.push(Vertex {
            pos: outer0,
            uv: egui::Pos2::ZERO,
            color: c0,
        });
        hue_mesh.vertices.push(Vertex {
            pos: inner0,
            uv: egui::Pos2::ZERO,
            color: c0,
        });
        hue_mesh.vertices.push(Vertex {
            pos: outer1,
            uv: egui::Pos2::ZERO,
            color: c1,
        });
        hue_mesh.vertices.push(Vertex {
            pos: inner1,
            uv: egui::Pos2::ZERO,
            color: c1,
        });
        hue_mesh.indices.extend_from_slice(&[
            base,
            base + 1,
            base + 2,
            base + 1,
            base + 3,
            base + 2,
        ]);
    }
    painter.add(egui::Shape::mesh(hue_mesh));

    // Hue indicator (circle on the ring).
    let hue_angle = h * std::f32::consts::TAU;
    let hue_mid_r = (wheel_radius + inner_radius) * 0.5;
    let hue_pos = center + egui::vec2(hue_angle.cos(), -hue_angle.sin()) * hue_mid_r;
    painter.circle_stroke(
        hue_pos,
        ring_width * 0.4,
        egui::Stroke::new(2.0, Color32::WHITE),
    );
    painter.circle_stroke(
        hue_pos,
        ring_width * 0.4 + 1.0,
        egui::Stroke::new(1.0, Color32::BLACK),
    );

    // -- SV square drawing --
    let sq_rect = egui::Rect::from_center_size(center, egui::vec2(sq_half * 2.0, sq_half * 2.0));
    // 4-vertex gradient: top-left (white), top-right (hue), bottom-left (black), bottom-right (black).
    // SV space: x = S (0 -> 1), y = V (1 -> 0).
    let mut sv_mesh = Mesh::default();
    let sv_steps = 32_u32;
    for yi in 0..sv_steps {
        for xi in 0..sv_steps {
            let x0 = xi as f32 / sv_steps as f32;
            let x1 = (xi + 1) as f32 / sv_steps as f32;
            let y0 = yi as f32 / sv_steps as f32;
            let y1 = (yi + 1) as f32 / sv_steps as f32;

            let p00 = egui::pos2(
                sq_rect.left() + x0 * sq_rect.width(),
                sq_rect.top() + y0 * sq_rect.height(),
            );
            let p10 = egui::pos2(
                sq_rect.left() + x1 * sq_rect.width(),
                sq_rect.top() + y0 * sq_rect.height(),
            );
            let p01 = egui::pos2(
                sq_rect.left() + x0 * sq_rect.width(),
                sq_rect.top() + y1 * sq_rect.height(),
            );
            let p11 = egui::pos2(
                sq_rect.left() + x1 * sq_rect.width(),
                sq_rect.top() + y1 * sq_rect.height(),
            );

            let c00 = hsv_to_color32(h, x0, 1.0 - y0);
            let c10 = hsv_to_color32(h, x1, 1.0 - y0);
            let c01 = hsv_to_color32(h, x0, 1.0 - y1);
            let c11 = hsv_to_color32(h, x1, 1.0 - y1);

            let base = sv_mesh.vertices.len() as u32;
            sv_mesh.vertices.push(Vertex {
                pos: p00,
                uv: egui::Pos2::ZERO,
                color: c00,
            });
            sv_mesh.vertices.push(Vertex {
                pos: p10,
                uv: egui::Pos2::ZERO,
                color: c10,
            });
            sv_mesh.vertices.push(Vertex {
                pos: p01,
                uv: egui::Pos2::ZERO,
                color: c01,
            });
            sv_mesh.vertices.push(Vertex {
                pos: p11,
                uv: egui::Pos2::ZERO,
                color: c11,
            });
            sv_mesh.indices.extend_from_slice(&[
                base,
                base + 1,
                base + 2,
                base + 1,
                base + 3,
                base + 2,
            ]);
        }
    }
    painter.add(egui::Shape::mesh(sv_mesh));
    painter.rect_stroke(
        sq_rect,
        0.0,
        egui::Stroke::new(1.0, Color32::from_gray(80)),
        egui::StrokeKind::Outside,
    );

    // SV indicator.
    let sv_pos = egui::pos2(
        sq_rect.left() + s * sq_rect.width(),
        sq_rect.top() + (1.0 - v) * sq_rect.height(),
    );
    let indicator_color = if v > 0.5 {
        Color32::BLACK
    } else {
        Color32::WHITE
    };
    painter.circle_stroke(sv_pos, 4.0, egui::Stroke::new(2.0, indicator_color));

    // -- Interaction --
    // Hue ring drag.
    let ring_id = ui.id().with("hue_ring");
    let ring_response = ui.interact(rect, ring_id, egui::Sense::click_and_drag());
    if ring_response.dragged() || ring_response.clicked() {
        if let Some(pos) = ring_response.interact_pointer_pos() {
            let dx = pos.x - center.x;
            let dy = -(pos.y - center.y);
            let dist = (dx * dx + dy * dy).sqrt();
            // Update hue if on the ring or while dragging.
            if dist >= inner_radius * 0.8 || ui.ctx().is_being_dragged(ring_id) {
                h = dy.atan2(dx) / std::f32::consts::TAU;
                if h < 0.0 {
                    h += 1.0;
                }
            }
        }
    }

    // SV square drag.
    let sv_id = ui.id().with("sv_square");
    let sv_response = ui.interact(sq_rect, sv_id, egui::Sense::click_and_drag());
    if sv_response.dragged() || sv_response.clicked() {
        if let Some(pos) = sv_response.interact_pointer_pos() {
            s = ((pos.x - sq_rect.left()) / sq_rect.width()).clamp(0.0, 1.0);
            v = 1.0 - ((pos.y - sq_rect.top()) / sq_rect.height()).clamp(0.0, 1.0);
        }
    }

    // Write back values.
    let new_rgb = hsv_to_rgb(h, s, v);
    *rgb = new_rgb;

    // Current-color preview.
    let preview_color = Color32::from_rgb(
        linear_to_srgb_u8(rgb[0]),
        linear_to_srgb_u8(rgb[1]),
        linear_to_srgb_u8(rgb[2]),
    );
    let preview_size = egui::vec2(total_size.x, 14.0);
    let (preview_rect, _) = ui.allocate_exact_size(preview_size, egui::Sense::hover());
    ui.painter().rect_filled(preview_rect, 2.0, preview_color);
}

// --- Color-space conversion helpers ---

fn linear_to_srgb_u8(c: f32) -> u8 {
    let s = if c <= 0.0031308 {
        c * 12.92
    } else {
        1.055 * c.powf(1.0 / 2.4) - 0.055
    };
    (s.clamp(0.0, 1.0) * 255.0 + 0.5) as u8
}

/// linear RGB → HSV (h: 0..1, s: 0..1, v: 0..1)
fn rgb_to_hsv(rgb: [f32; 3]) -> [f32; 3] {
    // linear → sRGB for perceptual HSV
    let r = if rgb[0] <= 0.0031308 {
        rgb[0] * 12.92
    } else {
        1.055 * rgb[0].powf(1.0 / 2.4) - 0.055
    };
    let g = if rgb[1] <= 0.0031308 {
        rgb[1] * 12.92
    } else {
        1.055 * rgb[1].powf(1.0 / 2.4) - 0.055
    };
    let b = if rgb[2] <= 0.0031308 {
        rgb[2] * 12.92
    } else {
        1.055 * rgb[2].powf(1.0 / 2.4) - 0.055
    };
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let delta = max - min;
    let v = max;
    let s = if max > 0.0 { delta / max } else { 0.0 };
    let h = if delta < 1e-6 {
        0.0
    } else if max == r {
        ((g - b) / delta).rem_euclid(6.0) / 6.0
    } else if max == g {
        ((b - r) / delta + 2.0) / 6.0
    } else {
        ((r - g) / delta + 4.0) / 6.0
    };
    [h, s, v]
}

/// HSV → linear RGB
fn hsv_to_rgb(h: f32, s: f32, v: f32) -> [f32; 3] {
    // HSV → sRGB
    let h6 = (h * 6.0).rem_euclid(6.0);
    let f = h6 - h6.floor();
    let p = v * (1.0 - s);
    let q = v * (1.0 - s * f);
    let t = v * (1.0 - s * (1.0 - f));
    let (r, g, b) = match h6 as u32 {
        0 => (v, t, p),
        1 => (q, v, p),
        2 => (p, v, t),
        3 => (p, q, v),
        4 => (t, p, v),
        _ => (v, p, q),
    };
    // sRGB → linear
    let to_lin = |c: f32| {
        if c <= 0.04045 {
            c / 12.92
        } else {
            ((c + 0.055) / 1.055).powf(2.4)
        }
    };
    [to_lin(r), to_lin(g), to_lin(b)]
}

/// HSV → Color32 (sRGB, for painting)
fn hsv_to_color32(h: f32, s: f32, v: f32) -> Color32 {
    let h6 = (h * 6.0).rem_euclid(6.0);
    let f = h6 - h6.floor();
    let p = v * (1.0 - s);
    let q = v * (1.0 - s * f);
    let t = v * (1.0 - s * (1.0 - f));
    let (r, g, b) = match h6 as u32 {
        0 => (v, t, p),
        1 => (q, v, p),
        2 => (p, v, t),
        3 => (p, q, v),
        4 => (t, p, v),
        _ => (v, p, q),
    };
    Color32::from_rgb(
        (r * 255.0 + 0.5) as u8,
        (g * 255.0 + 0.5) as u8,
        (b * 255.0 + 0.5) as u8,
    )
}

// ===========================================================================
// UV edit tab (v0.5.5 Phase 1: per-vertex UV editing).
// ===========================================================================

/// Convert a UV coordinate (0..1) to screen coordinates inside the canvas rect.
///
/// Maps UV Y=0 to the **top** and UV Y=1 to the **bottom** (to match the
/// PSD output in `convert/uvmap.rs`, which writes directly to image Y as
/// `y = v * dim`). This makes the UV editor view and `.psd` UV map view
/// match exactly, letting the user reference both simultaneously.
///
/// In Phase 2-3 we added view transforms (`view_offset`, `view_zoom`):
/// the UV coordinate placed at the canvas's top-left is `view_offset`;
/// the zoom factor is `view_zoom`.
///
/// The scale is a fixed X/Y common value (`UV_BASE_PX_PER_UNIT *
/// view_zoom` px/UV); UV [0, 1] becomes a
/// `UV_BASE_PX_PER_UNIT * UV_BASE_PX_PER_UNIT` px square at
/// `view_zoom = 1.0`. It does not depend on the canvas aspect ratio, so
/// even when the window is wide / tall the UV's visual aspect (1:1) is
/// preserved, and the window behaves like "a viewport into UV space"
/// (standard DCC-tool behavior).
fn uv_to_canvas(
    uv: [f32; 2],
    rect: egui::Rect,
    view_offset: [f32; 2],
    view_zoom: f32,
) -> egui::Pos2 {
    let s = view_zoom * UV_BASE_PX_PER_UNIT;
    egui::pos2(
        rect.min.x + (uv[0] - view_offset[0]) * s,
        rect.min.y + (uv[1] - view_offset[1]) * s,
    )
}

/// Convert in-canvas screen coordinates (px) to UV coordinates (0..1). Inverse of `uv_to_canvas`.
fn canvas_to_uv(
    p: egui::Pos2,
    rect: egui::Rect,
    view_offset: [f32; 2],
    view_zoom: f32,
) -> [f32; 2] {
    let s = view_zoom * UV_BASE_PX_PER_UNIT;
    [
        (p.x - rect.min.x) / s + view_offset[0],
        (p.y - rect.min.y) / s + view_offset[1],
    ]
}

/// Reference scale of the UV edit canvas (px / UV unit, when `view_zoom = 1.0`).
/// Shared between X and Y, so UV [0, 1] is always drawn as a square (1:1 aspect).
/// 256 was chosen so it roughly fits the initial canvas height (~250 px).
const UV_BASE_PX_PER_UNIT: f32 = 256.0;

/// Shift-snap: round `val` to a multiple of `step`.
fn snap_to(val: f32, step: f32) -> f32 {
    (val / step).round() * step
}

/// UV edit window (v0.5.5 Phase 1 / Phase 3 A-3). Opened from a header button on the material edit panel.
///
/// When `app.uv_edit.detached == false`, opens as a floating `egui::Window`
/// inside the main window. When `true`, switches to an OS-native standalone
/// window via `ctx.show_viewport_immediate`. Both use fixed IDs
/// (`Id::new("uv_edit_window")` /
/// `ViewportId::from_hash_of("uv_edit_viewport")`) to prevent multiple
/// instances. Drawn only when `app.uv_edit_window_open` is `true`.
pub fn show_uv_edit_window(ctx: &egui::Context, app: &mut ViewerApp) {
    if !app.uv_edit_window_open {
        return;
    }
    // Auto-close when no model is loaded (matches the material edit panel).
    if app.loaded.is_none() {
        app.uv_edit_window_open = false;
        return;
    }
    // Reflect the active-material name in the title.
    let title = {
        let mat_count = app
            .loaded
            .as_ref()
            .map(|l| l.mat_cache.names.len())
            .unwrap_or(0);
        if app.uv_edit.active_material >= mat_count {
            app.uv_edit.active_material = 0;
        }
        let name = app
            .loaded
            .as_ref()
            .and_then(|l| l.mat_cache.names.get(app.uv_edit.active_material))
            .cloned()
            .unwrap_or_default();
        t!("viewer.uv_edit.title", name = name).into_owned()
    };

    if app.uv_edit.detached {
        // Phase 3 / A-3: render into an OS-native standalone window.
        // `show_viewport_immediate` invokes the content closure on the same
        // thread as main, so `&mut ViewerApp` can be passed as is (no
        // Arc<Mutex> needed unlike the `deferred` variant).
        let viewport_id = egui::ViewportId::from_hash_of("uv_edit_viewport");
        let builder = egui::ViewportBuilder::default()
            .with_title(&title)
            .with_inner_size([480.0, 560.0])
            .with_min_inner_size([320.0, 440.0]);
        ctx.show_viewport_immediate(viewport_id, builder, |vctx, _class| {
            egui::CentralPanel::default().show(vctx, |ui| {
                show_uv_edit_body(ui, app);
            });
            if vctx.input(|i| i.viewport().close_requested()) {
                // When the user closes the window via the x button, reset the visibility flag to false.
                // Keep the `detached` flag as a user preference (so the next "UV edit" also uses standalone).
                app.uv_edit_window_open = false;
            }
        });
    } else {
        let mut is_open = true;
        // v0.5.7: do not set any min / max so the user can resize freely.
        // The auto-grow loop is blocked by pre-reserving footer height when
        // allocating the canvas inside `show_uv_edit_body`.
        // `default_height` is enlarged to 520 so the canvas height starts at
        // ~250 px and the UV area doesn't look small (the header takes
        // ~200 px).
        egui::Window::new(title)
            .id(egui::Id::new("uv_edit_window"))
            .default_width(320.0)
            .default_height(520.0)
            .resizable(true)
            .collapsible(true)
            .open(&mut is_open)
            .show(ctx, |ui| {
                show_uv_edit_body(ui, app);
            });
        app.uv_edit_window_open = is_open;
    }
}

fn show_uv_edit_body(ui: &mut egui::Ui, app: &mut ViewerApp) {
    ui.small(t!("viewer.uv_edit.instruction"));

    if app.loaded.is_none() {
        ui.add_space(8.0);
        ui.label(t!("viewer.uv_edit.no_model"));
        return;
    }

    // Get the material list (clone to avoid borrow conflicts).
    let mat_names: Vec<String> = app
        .loaded
        .as_ref()
        .map(|l| l.mat_cache.names.clone())
        .unwrap_or_default();
    let mat_count = mat_names.len();
    if mat_count == 0 {
        ui.label(t!("viewer.uv_edit.no_material"));
        return;
    }

    // Normalize `active_material` (reset to 0 if it exceeds the material count).
    if app.uv_edit.active_material >= mat_count {
        app.uv_edit.active_material = 0;
    }
    let active_mat = app.uv_edit.active_material;

    // Material selection ComboBox.
    egui::ComboBox::from_id_salt("uv_edit_material_combo")
        .width(ui.available_width() - 4.0)
        .selected_text(format!("[{}] {}", active_mat, mat_names[active_mat]))
        .show_ui(ui, |ui| {
            for (i, name) in mat_names.iter().enumerate() {
                ui.selectable_value(
                    &mut app.uv_edit.active_material,
                    i,
                    format!("[{}] {}", i, name),
                );
            }
        });

    // Phase 3 A-1: UV-set selection ComboBox.
    // If no mesh of the active material has UV1, disable the UV1 choice and,
    // if `active_uv_set == 1`, revert to UV0 (safe side: avoid stale selection).
    let has_uv1 = app
        .loaded
        .as_ref()
        .map(|l| material_has_uv1(&l.ir, active_mat))
        .unwrap_or(false);
    if !has_uv1 && app.uv_edit.active_uv_set == 1 {
        app.uv_edit.active_uv_set = 0;
    }
    ui.horizontal(|ui| {
        ui.label("UV set:");
        let current_label = if app.uv_edit.active_uv_set == 0 {
            "UV0"
        } else {
            "UV1"
        };
        let mut new_set = app.uv_edit.active_uv_set;
        egui::ComboBox::from_id_salt("uv_edit_uvset_combo")
            .selected_text(current_label)
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut new_set, 0u8, "UV0");
                ui.add_enabled_ui(has_uv1, |ui| {
                    ui.selectable_value(&mut new_set, 1u8, "UV1");
                });
            });
        if new_set != app.uv_edit.active_uv_set {
            // On set switch, cancel any in-progress drag (avoid mishandling in a different UV space).
            app.uv_edit.active_uv_set = new_set;
            app.uv_edit.dragging = false;
            app.uv_edit.drag_mode = UvDragMode::None;
            app.uv_edit.drag_start_uvs.clear();
            app.uv_edit.drag_press_uv = None;
            app.uv_edit.drag_pivot = None;
        }
        if !has_uv1 {
            ui.small(t!("viewer.uv_edit.no_uv1_label"))
                .on_hover_text(t!("viewer.uv_edit.no_uv1_tooltip"));
        }
    });

    // Phase 3 A-2: UV-morph selection ComboBox.
    // List only UV morphs whose channel matches `active_uv_set`.
    // While a morph is selected, the following constraints apply:
    //   - drag state / selected belong to a separate space; clear them on switch.
    //   - `active_uv_set` is forcibly synced to the morph's channel (avoid UV-set mismatch).
    let uv_morph_list: Vec<(usize, String, u8)> = app
        .loaded
        .as_ref()
        .map(|l| {
            l.ir.morphs
                .iter()
                .enumerate()
                .filter_map(|(i, m)| {
                    if let crate::intermediate::types::IrMorphKind::Uv { channel, .. } = &m.kind {
                        Some((i, m.name.clone(), *channel))
                    } else {
                        None
                    }
                })
                .collect()
        })
        .unwrap_or_default();
    // If the current `active_morph` is not in the list (e.g. after the IR changed), reset to `None`.
    // Restore weights via the helper at the same time.
    if let Some(cur) = app.uv_edit.active_morph {
        if !uv_morph_list.iter().any(|(i, _, _)| *i == cur)
            && app
                .uv_edit
                .switch_active_morph(None, &mut app.morph_weights)
        {
            app.morph_dirty = true;
        }
    }
    if !uv_morph_list.is_empty() {
        ui.horizontal(|ui| {
            ui.label(t!("viewer.uv_edit.target_label"));
            let current_label: String = match app.uv_edit.active_morph {
                None => t!("viewer.uv_edit.base_uv").into_owned(),
                Some(idx) => uv_morph_list
                    .iter()
                    .find(|(i, _, _)| *i == idx)
                    .map(|(_, name, ch)| {
                        t!("viewer.uv_edit.morph_label", name = name, ch = ch).into_owned()
                    })
                    .unwrap_or_else(|| t!("viewer.uv_edit.morph_unknown").into_owned()),
            };
            let mut new_morph = app.uv_edit.active_morph;
            egui::ComboBox::from_id_salt("uv_edit_morph_combo")
                .selected_text(current_label)
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut new_morph, None, t!("viewer.uv_edit.base_uv"));
                    for (idx, name, ch) in &uv_morph_list {
                        ui.selectable_value(
                            &mut new_morph,
                            Some(*idx),
                            format!("{} (UV{})", name, ch),
                        );
                    }
                });
            // On switch: stash / restore weights and update `active_morph` via the helper in one go.
            // If the return value is `true` (mode actually switched), clear selection / drag state / undo too.
            if app
                .uv_edit
                .switch_active_morph(new_morph, &mut app.morph_weights)
            {
                app.morph_dirty = true;
                app.uv_edit.selected.clear();
                app.uv_edit.dragging = false;
                app.uv_edit.drag_mode = UvDragMode::None;
                app.uv_edit.drag_start_uvs.clear();
                app.uv_edit.drag_press_uv = None;
                app.uv_edit.drag_pivot = None;
                app.uv_edit.gizmo_action = None;
                app.uv_edit.undo_stack.clear();
                app.uv_edit.redo_stack.clear();
                app.uv_edit.pristine_uvs.clear();
                // On morph selection, force `active_uv_set` to the channel.
                if let Some(idx) = new_morph {
                    if let Some((_, _, ch)) = uv_morph_list.iter().find(|(i, _, _)| *i == idx) {
                        app.uv_edit.active_uv_set = *ch;
                    }
                }
            }
            // Behavior note for morph editing (weight is locked to 1.0 while editing; restored on end).
            if app.uv_edit.active_morph.is_some() {
                ui.small(t!("viewer.uv_edit.morph_edit_weight_lock_hint"));
            }
        });
    }
    // The morph branch above may update `active_uv_set`, so re-read `active_chan` with the latest value.
    let active_chan = app.uv_edit.active_uv_set;
    let active_morph = app.uv_edit.active_morph;
    // Phase 3 A-2: UV-morph entries are `(global_vi, [f32; 4])`, so
    // pre-compute per-mesh global vertex offsets and pass them to
    // `read_displayed_uv` etc. Base UV editing (`active_morph == None`)
    // doesn't really use them, but always prepare them to keep the call
    // sites' types uniform.
    let global_offsets: Vec<usize> = app
        .loaded
        .as_ref()
        .map(|l| mesh_global_offsets_of(&l.ir))
        .unwrap_or_default();

    // Status (count only the current UV set).
    let override_count = app
        .uv_edit
        .overrides
        .keys()
        .filter(|(_, _, c)| *c == active_chan)
        .count();
    let selected_count = app
        .uv_edit
        .selected
        .iter()
        .filter(|(_, _, c)| *c == active_chan)
        .count();
    ui.small(t!(
        "viewer.uv_edit.vertex_stats",
        edited = override_count,
        selected = selected_count
    ));

    let mut select_all_trigger = false;
    ui.horizontal(|ui| {
        if ui
            .small_button(t!("viewer.uv_edit.select_all"))
            .on_hover_text(t!("viewer.uv_edit.select_all_tooltip"))
            .clicked()
        {
            select_all_trigger = true;
        }
        if ui
            .small_button(t!("viewer.uv_edit.clear_selection"))
            .clicked()
        {
            app.uv_edit.selected.clear();
        }
        if ui
            .small_button(t!("viewer.uv_edit.clear_all_edits"))
            .on_hover_text(t!("viewer.uv_edit.clear_all_edits_tooltip"))
            .clicked()
        {
            // review_result_03 [P2]: also drop undo / redo so Ctrl+Z / Ctrl+Y immediately after clear
            // does not revive edits. Matches the "Clear all" UI label.
            // review_result_06 [P2]: also drop `pristine_uvs`. If it remains, the next drag's
            // `record_pristine` reuses the old pristine via `or_insert`, and post-clear new edits
            // are judged "back to old A" by undo and never leave `overrides`.
            app.uv_edit.overrides.clear();
            app.uv_edit.selected.clear();
            app.uv_edit.undo_stack.clear();
            app.uv_edit.redo_stack.clear();
            app.uv_edit.pristine_uvs.clear();
        }
        if ui
            .small_button(t!("viewer.uv_edit.view_reset"))
            .on_hover_text(t!("viewer.uv_edit.view_reset_tooltip"))
            .clicked()
        {
            app.uv_edit.reset_view();
        }
        // Phase 3 / A-3: toggle to switch to a standalone window.
        // The moment the button is pressed, `detached` flips, and the next
        // frame's `show_uv_edit_window` switches the render target
        // (`egui::Window` vs `show_viewport_immediate`).
        let (label, hover) = if app.uv_edit.detached {
            (
                t!("viewer.uv_edit.dock_button"),
                t!("viewer.uv_edit.dock_tooltip"),
            )
        } else {
            (
                t!("viewer.uv_edit.detach_button"),
                t!("viewer.uv_edit.detach_tooltip"),
            )
        };
        if ui.small_button(label).on_hover_text(hover).clicked() {
            app.uv_edit.detached = !app.uv_edit.detached;
        }
    });
    ui.small(t!(
        "viewer.uv_edit.zoom_status",
        zoom = format!("{:.2}", app.uv_edit.view_zoom)
    ));
    ui.small(t!("viewer.uv_edit.hint_modifiers"));
    ui.small(t!("viewer.uv_edit.hint_select_modes"));
    ui.small(t!("viewer.uv_edit.hint_handles"));

    // Phase 2-5: Undo / Redo button row and keyboard shortcuts (Ctrl+Z / Ctrl+Y / Ctrl+Shift+Z).
    let can_undo = !app.uv_edit.undo_stack.is_empty();
    let can_redo = !app.uv_edit.redo_stack.is_empty();
    let mut undo_trigger = false;
    let mut redo_trigger = false;
    ui.horizontal(|ui| {
        if ui
            .add_enabled(
                can_undo,
                egui::Button::new(t!("viewer.uv_edit.undo")).small(),
            )
            .on_hover_text("Ctrl+Z")
            .clicked()
        {
            undo_trigger = true;
        }
        if ui
            .add_enabled(
                can_redo,
                egui::Button::new(t!("viewer.uv_edit.redo")).small(),
            )
            .on_hover_text("Ctrl+Y / Ctrl+Shift+Z")
            .clicked()
        {
            redo_trigger = true;
        }
        ui.small(format!(
            "undo: {} / redo: {}",
            app.uv_edit.undo_stack.len(),
            app.uv_edit.redo_stack.len()
        ));
    });
    // Keyboard shortcuts: only handled when no widget (e.g. `TextEdit`) is requesting keyboard input.
    if !ui.ctx().wants_keyboard_input() {
        let (key_undo, key_redo, key_select_all) = ui.input(|i| {
            let z = i.key_pressed(egui::Key::Z) && i.modifiers.command;
            let y = i.key_pressed(egui::Key::Y) && i.modifiers.command;
            let a = i.key_pressed(egui::Key::A) && i.modifiers.command;
            let shift = i.modifiers.shift;
            (z && !shift, (z && shift) || y, a)
        });
        if key_undo {
            undo_trigger = true;
        }
        if key_redo {
            redo_trigger = true;
        }
        if key_select_all {
            select_all_trigger = true;
        }
    }
    if select_all_trigger {
        // Add all vertices of every mesh belonging to the active material into
        // `selected` on the current UV set (preserving existing selection).
        // Skip meshes without UV1 in UV1 mode.
        if let Some(loaded) = app.loaded.as_ref() {
            for (mi, mesh) in loaded.ir.meshes.iter().enumerate() {
                if mesh.material_index != active_mat {
                    continue;
                }
                if active_chan == 1 && mesh.uvs1.len() != mesh.vertices.len() {
                    continue;
                }
                for vi in 0..mesh.vertices.len() {
                    app.uv_edit
                        .selected
                        .insert((mi as u32, vi as u32, active_chan));
                }
            }
        }
    }
    if undo_trigger && can_undo {
        let queue = app.render_state.queue.clone();
        if let Some(loaded) = app.loaded.as_mut() {
            if app.uv_edit.apply_undo(&mut loaded.ir) {
                loaded.gpu_model.sync_uvs_from_ir(&loaded.ir, &queue);
            }
        }
    }
    if redo_trigger && can_redo {
        let queue = app.render_state.queue.clone();
        if let Some(loaded) = app.loaded.as_mut() {
            if app.uv_edit.apply_redo(&mut loaded.ir) {
                loaded.gpu_model.sync_uvs_from_ir(&loaded.ir, &queue);
            }
        }
    }

    ui.separator();

    // Resolve the active material's BaseColor texture (PMX/PMD falls back to `texture_index`).
    let bg_tex_idx: Option<usize> = app
        .loaded
        .as_ref()
        .and_then(|l| l.ir.materials.get(active_mat))
        .and_then(|mat| {
            mat.base_color_tex_info
                .as_ref()
                .map(|t| t.index)
                .or(mat.texture_index)
        });

    // Cache-diff detection: when mismatched, free the old egui `TextureId` and register the new `TextureView`.
    let cached_idx = app.uv_edit_bg_tex.as_ref().map(|(i, _)| *i);
    if cached_idx != bg_tex_idx {
        if let Some((_, old_id)) = app.uv_edit_bg_tex.take() {
            let mut renderer = app.render_state.renderer.write();
            renderer.free_texture(&old_id);
        }
        if let Some(idx) = bg_tex_idx {
            let new_id: Option<egui::TextureId> = app.loaded.as_ref().and_then(|loaded| {
                loaded.gpu_model.gpu_texture_views.get(idx).map(|view| {
                    let mut renderer = app.render_state.renderer.write();
                    renderer.register_native_texture(
                        &app.render_state.device,
                        view,
                        eframe::wgpu::FilterMode::Linear,
                    )
                })
            });
            if let Some(id) = new_id {
                app.uv_edit_bg_tex = Some((idx, id));
            }
        }
    }

    // Canvas drawing: rectangle that tracks the Window (no forced square).
    //   - canvas_w = avail.x - 4 (4 px margin on the right).
    //   - canvas_h = avail.y - 32 (footer reservation: leaves room for the
    //     `add_space(4)` + `small()` at the function tail).
    //     Without this, the Window auto-grows in a loop and vertical resize is disabled.
    //   - UV [0, 1] stretches across the whole canvas as
    //     `rect.width() x rect.height()` (see `uv_to_canvas`). When the
    //     Window is tall / wide, the UV display stretches in the same aspect.
    const UV_FOOTER_RESERVE_PX: f32 = 32.0;
    let avail = ui.available_size();
    let canvas_w = (avail.x - 4.0).max(160.0);
    let canvas_h = (avail.y - UV_FOOTER_RESERVE_PX).max(160.0);
    let (rect, response) = ui.allocate_exact_size(
        egui::vec2(canvas_w, canvas_h),
        egui::Sense::click_and_drag(),
    );
    let painter = ui.painter_at(rect);

    // Wheel: zoom centered on the cursor position (Phase 2-3).
    let scroll_y = ui.input(|i| i.raw_scroll_delta.y);
    if response.hovered() && scroll_y != 0.0 {
        if let Some(cursor) = ui.input(|i| i.pointer.hover_pos()) {
            let voff_cur = app.uv_edit.view_offset;
            let vzoom_cur = app.uv_edit.view_zoom;
            let pre_uv = canvas_to_uv(cursor, rect, voff_cur, vzoom_cur);
            let factor = (scroll_y * 0.002).exp();
            let new_zoom = (vzoom_cur * factor).clamp(0.1, 32.0);
            app.uv_edit.view_zoom = new_zoom;
            let post_uv = canvas_to_uv(cursor, rect, voff_cur, new_zoom);
            app.uv_edit.view_offset[0] += pre_uv[0] - post_uv[0];
            app.uv_edit.view_offset[1] += pre_uv[1] - post_uv[1];
        }
    }

    // Middle-button drag: pan (Phase 2-3, supports fixed scale).
    let (mid_down, ptr_delta) = ui.input(|i| (i.pointer.middle_down(), i.pointer.delta()));
    if response.hovered() && mid_down && (ptr_delta.x.abs() + ptr_delta.y.abs()) > 0.0 {
        let s = app.uv_edit.view_zoom * UV_BASE_PX_PER_UNIT;
        app.uv_edit.view_offset[0] -= ptr_delta.x / s;
        app.uv_edit.view_offset[1] -= ptr_delta.y / s;
    }

    // View state used by drawing / picking / dragging below (this frame's settled values).
    let voff = app.uv_edit.view_offset;
    let vzoom = app.uv_edit.view_zoom;

    // Compute where the UV [0, 1] region ends up on the canvas.
    let uv01_tl = uv_to_canvas([0.0, 0.0], rect, voff, vzoom);
    let uv01_br = uv_to_canvas([1.0, 1.0], rect, voff, vzoom);
    let uv01_rect = egui::Rect::from_two_pos(uv01_tl, uv01_br);

    // Background: paint the whole canvas dark (also visually indicates outside UV [0, 1]).
    painter.rect_filled(rect, 0.0, Color32::from_rgb(0x0A, 0x0A, 0x0A));
    // Draw the BaseColor texture 1:1 over the UV [0, 1] rect, if present.
    if let Some((_, tex_id)) = app.uv_edit_bg_tex {
        painter.image(
            tex_id,
            uv01_rect,
            egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
            Color32::WHITE,
        );
    } else {
        painter.rect_filled(uv01_rect, 0.0, Color32::from_rgb(0x10, 0x10, 0x10));
    }
    painter.rect_stroke(
        uv01_rect,
        0.0,
        egui::Stroke::new(1.0, Color32::from_gray(0x60)),
        egui::StrokeKind::Inside,
    );
    // Canvas outer frame.
    painter.rect_stroke(
        rect,
        0.0,
        egui::Stroke::new(1.0, Color32::from_gray(0x40)),
        egui::StrokeKind::Inside,
    );

    // Drawing: UV wires + vertex dots for the selected material's meshes.
    let wire_stroke = egui::Stroke::new(0.5, Color32::from_rgb(0x80, 0x80, 0x80));
    let vert_default = Color32::from_rgb(0xE0, 0xE0, 0xE0);
    let vert_edited = Color32::from_rgb(0x66, 0xBB, 0xFF);
    let vert_selected = Color32::from_rgb(0xFF, 0xE0, 0x40);

    if let Some(loaded) = app.loaded.as_ref() {
        let ir = &loaded.ir;
        for (mi, mesh) in ir.meshes.iter().enumerate() {
            if mesh.material_index != active_mat {
                continue;
            }
            // In UV1 mode, do not draw meshes without UV1 at all (matches pick / drag behavior).
            if active_chan == 1 && mesh.uvs1.len() != mesh.vertices.len() {
                continue;
            }
            let vcount = mesh.vertices.len();
            let mi_u32 = mi as u32;
            for tri in mesh.indices.as_ref().chunks_exact(3) {
                let Some(a) = read_displayed_uv(
                    ir,
                    mi_u32,
                    tri[0],
                    active_chan,
                    active_morph,
                    &global_offsets,
                ) else {
                    continue;
                };
                let Some(b) = read_displayed_uv(
                    ir,
                    mi_u32,
                    tri[1],
                    active_chan,
                    active_morph,
                    &global_offsets,
                ) else {
                    continue;
                };
                let Some(c) = read_displayed_uv(
                    ir,
                    mi_u32,
                    tri[2],
                    active_chan,
                    active_morph,
                    &global_offsets,
                ) else {
                    continue;
                };
                let pa = uv_to_canvas(a, rect, voff, vzoom);
                let pb = uv_to_canvas(b, rect, voff, vzoom);
                let pc = uv_to_canvas(c, rect, voff, vzoom);
                painter.line_segment([pa, pb], wire_stroke);
                painter.line_segment([pb, pc], wire_stroke);
                painter.line_segment([pc, pa], wire_stroke);
            }
            for vi in 0..vcount {
                let vi_u32 = vi as u32;
                let Some(uv) = read_displayed_uv(
                    ir,
                    mi_u32,
                    vi_u32,
                    active_chan,
                    active_morph,
                    &global_offsets,
                ) else {
                    continue;
                };
                let key = (mi_u32, vi_u32, active_chan);
                let color = if app.uv_edit.selected.contains(&key) {
                    vert_selected
                } else if app.uv_edit.overrides.contains_key(&key) {
                    vert_edited
                } else {
                    vert_default
                };
                let p = uv_to_canvas(uv, rect, voff, vzoom);
                painter.circle_filled(p, 2.5, color);
            }
        }
    }

    // Phase 3 A-5: draw the selection bbox and 2D gizmo handles.
    // The bbox has zero area with one or fewer selected vertices, so show only when >= 2.
    // Also disable when the bbox is extremely narrow (UV-space < 1e-5) -
    // rotation / scale becomes numerically unstable.
    let selection_bbox_uv: Option<[f32; 4]> = if app.uv_edit.selected.len() >= 2 {
        let mut u_min = f32::INFINITY;
        let mut u_max = f32::NEG_INFINITY;
        let mut v_min = f32::INFINITY;
        let mut v_max = f32::NEG_INFINITY;
        let mut any = false;
        if let Some(loaded) = app.loaded.as_ref() {
            for &(mi, vi, chan) in app.uv_edit.selected.iter() {
                if chan != active_chan {
                    continue;
                }
                if let Some(uv) =
                    read_displayed_uv(&loaded.ir, mi, vi, chan, active_morph, &global_offsets)
                {
                    u_min = u_min.min(uv[0]);
                    u_max = u_max.max(uv[0]);
                    v_min = v_min.min(uv[1]);
                    v_max = v_max.max(uv[1]);
                    any = true;
                }
            }
        }
        if any && (u_max - u_min).abs() > 1e-5 && (v_max - v_min).abs() > 1e-5 {
            Some([u_min, v_min, u_max, v_max])
        } else {
            None
        }
    } else {
        None
    };

    // If a bbox exists, draw gizmos on the canvas. Handle position / pick threshold numbers live only here.
    const GIZMO_HANDLE_RADIUS: f32 = 5.0;
    const GIZMO_PICK_RADIUS_SQ: f32 = 100.0; // 10 px
    const GIZMO_ROTATE_OFFSET: f32 = 24.0; // pixel offset above the bbox top edge
    let gizmo_handle_pos: Option<([egui::Pos2; 4], egui::Pos2)> = selection_bbox_uv.map(|bb| {
        // Canvas coordinates of the 4 corners; array order [min/min, max/min, min/max, max/max].
        let p_mm = uv_to_canvas([bb[0], bb[1]], rect, voff, vzoom);
        let p_xm = uv_to_canvas([bb[2], bb[1]], rect, voff, vzoom);
        let p_mx = uv_to_canvas([bb[0], bb[3]], rect, voff, vzoom);
        let p_xx = uv_to_canvas([bb[2], bb[3]], rect, voff, vzoom);
        // Rotate handle: place at GIZMO_ROTATE_OFFSET outside the bbox top-edge midpoint (toward smaller canvas y).
        let top_mid = egui::pos2((p_mm.x + p_xm.x) * 0.5, (p_mm.y + p_xm.y) * 0.5);
        let rotate = egui::pos2(top_mid.x, top_mid.y - GIZMO_ROTATE_OFFSET);
        ([p_mm, p_xm, p_mx, p_xx], rotate)
    });
    if let (Some(bb), Some((corners, rot_handle))) = (selection_bbox_uv, gizmo_handle_pos) {
        let bb_rect = egui::Rect::from_two_pos(corners[0], corners[3]).intersect(rect); // clip to canvas
        let gizmo_stroke = egui::Stroke::new(
            1.0,
            Color32::from_rgba_premultiplied(0xFF, 0xA8, 0x40, 0xE0),
        );
        painter.rect_stroke(bb_rect, 0.0, gizmo_stroke, egui::StrokeKind::Inside);
        // 4 corner scale handles (filled squares).
        let handle_fill = Color32::from_rgba_premultiplied(0xFF, 0xA8, 0x40, 0xFF);
        for &c in &corners {
            let hr = egui::Rect::from_center_size(
                c,
                egui::vec2(GIZMO_HANDLE_RADIUS * 2.0, GIZMO_HANDLE_RADIUS * 2.0),
            );
            painter.rect_filled(hr, 1.0, handle_fill);
            painter.rect_stroke(
                hr,
                1.0,
                egui::Stroke::new(1.0, Color32::BLACK),
                egui::StrokeKind::Inside,
            );
        }
        // Rotate handle (filled circle + line connecting to the bbox top edge).
        let top_mid = egui::pos2((corners[0].x + corners[1].x) * 0.5, corners[0].y);
        painter.line_segment(
            [top_mid, rot_handle],
            egui::Stroke::new(1.0, Color32::from_gray(0x70)),
        );
        painter.circle_filled(
            rot_handle,
            GIZMO_HANDLE_RADIUS,
            Color32::from_rgba_premultiplied(0x66, 0xCC, 0xFF, 0xFF),
        );
        painter.circle_stroke(
            rot_handle,
            GIZMO_HANDLE_RADIUS,
            egui::Stroke::new(1.0, Color32::BLACK),
        );
        let _ = bb; // currently used only at pick time; not referenced in drawing
    }

    // Click: select the nearest vertex (within 12 px only).
    if response.clicked() {
        if let Some(click_pos) = response.interact_pointer_pos() {
            let mut best: Option<((u32, u32, u8), f32)> = None;
            if let Some(loaded) = app.loaded.as_ref() {
                let ir = &loaded.ir;
                for (mi, mesh) in ir.meshes.iter().enumerate() {
                    if mesh.material_index != active_mat {
                        continue;
                    }
                    if active_chan == 1 && mesh.uvs1.len() != mesh.vertices.len() {
                        continue;
                    }
                    let mi_u32 = mi as u32;
                    for vi in 0..mesh.vertices.len() {
                        let vi_u32 = vi as u32;
                        let Some(uv) = read_displayed_uv(
                            ir,
                            mi_u32,
                            vi_u32,
                            active_chan,
                            active_morph,
                            &global_offsets,
                        ) else {
                            continue;
                        };
                        let p = uv_to_canvas(uv, rect, voff, vzoom);
                        let d = (p - click_pos).length_sq();
                        if best.is_none_or(|(_, bd)| d < bd) {
                            best = Some(((mi_u32, vi_u32, active_chan), d));
                        }
                    }
                }
            }
            app.uv_edit.selected.clear();
            if let Some((key, d2)) = best {
                if d2 < 144.0 {
                    app.uv_edit.selected.insert(key);
                }
            }
        }
    }

    // Drag start: mode decision (Move or Rect, Phase 2-2).
    //
    // Decision rules:
    //   - Press within 12 px of an already-selected vertex -> Move (translate the existing selection).
    //   - Press within 12 px of any vertex -> Move (single-select that vertex and translate).
    //   - Otherwise -> Rect (rectangle selection; existing selection is cleared).
    if response.drag_started() {
        app.uv_edit.drag_start_uvs.clear();
        app.uv_edit.drag_press_uv = None;
        app.uv_edit.drag_mode = UvDragMode::None;
        app.uv_edit.gizmo_action = None;

        if let Some(press_pos) = response.interact_pointer_pos() {
            app.uv_edit.drag_press_uv = Some(canvas_to_uv(press_pos, rect, voff, vzoom));

            // Phase 3 A-5: gizmo handle hit-test (highest priority).
            // On hit, set `drag_mode = Move` + `gizmo_action = Some(...)`;
            // subsequent `dragged()` Move branches reference `gizmo_action`
            // to force `XformMode`.
            let gizmo_hit: Option<(UvGizmoAction, [f32; 2])> =
                if let (Some(bb), Some((corners, rot))) = (selection_bbox_uv, gizmo_handle_pos) {
                    // Compute squared distance to each handle and pick the smallest within threshold.
                    // Simple comparison suffices because the rotate handle never overlaps the 4 corners.
                    let d_rot = (rot - press_pos).length_sq();
                    let d_corners = [
                        (corners[0] - press_pos).length_sq(),
                        (corners[1] - press_pos).length_sq(),
                        (corners[2] - press_pos).length_sq(),
                        (corners[3] - press_pos).length_sq(),
                    ];
                    let (best_idx, best_d) = d_corners.iter().enumerate().fold(
                        (usize::MAX, f32::INFINITY),
                        |(bi, bd), (i, d)| {
                            if *d < bd {
                                (i, *d)
                            } else {
                                (bi, bd)
                            }
                        },
                    );
                    if d_rot < GIZMO_PICK_RADIUS_SQ && d_rot <= best_d {
                        // Rotate handle: pivot = bbox center.
                        Some((
                            UvGizmoAction::Rotate,
                            [(bb[0] + bb[2]) * 0.5, (bb[1] + bb[3]) * 0.5],
                        ))
                    } else if best_d < GIZMO_PICK_RADIUS_SQ {
                        // Corner handle: sign is 2 bits (u axis, v axis). Array order [mm, xm, mx, xx].
                        let (sign_u, sign_v): (i8, i8) = match best_idx {
                            0 => (-1, -1),
                            1 => (1, -1),
                            2 => (-1, 1),
                            3 => (1, 1),
                            _ => (0, 0),
                        };
                        // Scale pivot is "the opposite corner of the grabbed corner".
                        let pivot_u = if sign_u > 0 { bb[0] } else { bb[2] };
                        let pivot_v = if sign_v > 0 { bb[1] } else { bb[3] };
                        Some((
                            UvGizmoAction::ScaleCorner { sign_u, sign_v },
                            [pivot_u, pivot_v],
                        ))
                    } else {
                        None
                    }
                } else {
                    None
                };

            let mut nearest_sel_sq = f32::INFINITY;
            let mut nearest_any_sq = f32::INFINITY;
            let mut best_any: Option<(u32, u32, u8)> = None;
            if let Some(loaded) = app.loaded.as_ref() {
                let ir = &loaded.ir;
                for (mi, mesh) in ir.meshes.iter().enumerate() {
                    if mesh.material_index != active_mat {
                        continue;
                    }
                    if active_chan == 1 && mesh.uvs1.len() != mesh.vertices.len() {
                        continue;
                    }
                    let mi_u32 = mi as u32;
                    for vi in 0..mesh.vertices.len() {
                        let vi_u32 = vi as u32;
                        let Some(uv) = read_displayed_uv(
                            ir,
                            mi_u32,
                            vi_u32,
                            active_chan,
                            active_morph,
                            &global_offsets,
                        ) else {
                            continue;
                        };
                        let p = uv_to_canvas(uv, rect, voff, vzoom);
                        let d2 = (p - press_pos).length_sq();
                        let key = (mi_u32, vi_u32, active_chan);
                        if app.uv_edit.selected.contains(&key) && d2 < nearest_sel_sq {
                            nearest_sel_sq = d2;
                        }
                        if d2 < nearest_any_sq {
                            nearest_any_sq = d2;
                            best_any = Some(key);
                        }
                    }
                }
            }

            let mode = if gizmo_hit.is_some() {
                // Phase 3 A-5: press on a gizmo handle -> Move (transform type forced by `gizmo_action`).
                // Keep the selected vertices as is (the transform is applied to all vertices inside the bbox).
                UvDragMode::Move
            } else if nearest_sel_sq < 144.0 {
                UvDragMode::Move
            } else if nearest_any_sq < 144.0 {
                // Single-select the new vertex and Move.
                app.uv_edit.selected.clear();
                if let Some(k) = best_any {
                    app.uv_edit.selected.insert(k);
                }
                UvDragMode::Move
            } else {
                // Far from any vertex -> rectangle selection. Phase 3 A-4: Shift = add / Ctrl = subtract / no-modifier = replace.
                let (shift_down, ctrl_down) =
                    ui.input(|i| (i.modifiers.shift, i.modifiers.command));
                let behavior = if shift_down {
                    UvRectBehavior::Add
                } else if ctrl_down {
                    UvRectBehavior::Subtract
                } else {
                    UvRectBehavior::Replace
                };
                app.uv_edit.rect_behavior = behavior;
                if matches!(behavior, UvRectBehavior::Replace) {
                    app.uv_edit.selected.clear();
                    app.uv_edit.rect_initial_selected.clear();
                } else {
                    // Add/Subtract: save the initial `selected` at the start.
                    app.uv_edit.rect_initial_selected = app.uv_edit.selected.clone();
                }
                UvDragMode::Rect
            };
            app.uv_edit.drag_mode = mode;

            // For Move mode, record the starting UV (origin for the accumulating method).
            // Phase 2-4: also save the selection-bbox center as `pivot` (basis for rotation / scale).
            // Phase 3 A-1: limit to selected vertices matching `active_chan` to
            // avoid cross-channel selection interference.
            if matches!(mode, UvDragMode::Move) {
                let selected: Vec<(u32, u32, u8)> = app
                    .uv_edit
                    .selected
                    .iter()
                    .copied()
                    .filter(|(_, _, c)| *c == active_chan)
                    .collect();
                let mut u_min = f32::INFINITY;
                let mut u_max = f32::NEG_INFINITY;
                let mut v_min = f32::INFINITY;
                let mut v_max = f32::NEG_INFINITY;
                let mut any = false;
                if let Some(loaded) = app.loaded.as_ref() {
                    let ir = &loaded.ir;
                    for (mi, vi, chan) in &selected {
                        if let Some(arr) =
                            read_displayed_uv(ir, *mi, *vi, *chan, active_morph, &global_offsets)
                        {
                            app.uv_edit.drag_start_uvs.insert((*mi, *vi, *chan), arr);
                            // review_result_05 [P2]: record the UV at the first-drag moment as pristine
                            // (`or_insert` semantics, so it is not overwritten on subsequent calls).
                            app.uv_edit.record_pristine((*mi, *vi, *chan), arr);
                            u_min = u_min.min(arr[0]);
                            u_max = u_max.max(arr[0]);
                            v_min = v_min.min(arr[1]);
                            v_max = v_max.max(arr[1]);
                            any = true;
                        }
                    }
                }
                app.uv_edit.drag_pivot = if any {
                    Some([(u_min + u_max) * 0.5, (v_min + v_max) * 0.5])
                } else {
                    None
                };

                // Phase 3 A-5: if there's a gizmo hit, set the action and overwrite `pivot` with the gizmo's pivot.
                // ScaleCorner pivot = opposite corner; Rotate pivot = bbox center (matches existing value).
                if let Some((action, pivot)) = gizmo_hit {
                    app.uv_edit.gizmo_action = Some(action);
                    app.uv_edit.drag_pivot = Some(pivot);
                }
            }
        }
    }

    // While dragging: handle by mode.
    if response.dragged() {
        match app.uv_edit.drag_mode {
            UvDragMode::Move => {
                // Phase 2-4: switch transform mode by modifier key.
                //   - no modifier: translate (existing).
                //   - Alt        : rotate around pivot (angle delta = angle from pivot of cursor minus press).
                //   - Ctrl       : scale around pivot (factor = distance from pivot of cursor / press).
                // All use `start_uv + transform`, so over-accumulation proportional to frame count does not occur.
                if !app.uv_edit.drag_start_uvs.is_empty() {
                    if let (Some(press_uv), Some(cursor_pos)) =
                        (app.uv_edit.drag_press_uv, response.interact_pointer_pos())
                    {
                        let cursor_uv = canvas_to_uv(cursor_pos, rect, voff, vzoom);
                        let (shift_down, alt_down, ctrl_down) =
                            ui.input(|i| (i.modifiers.shift, i.modifiers.alt, i.modifiers.command));
                        let pivot = app.uv_edit.drag_pivot;
                        let snap_step = 1.0 / 16.0;

                        #[derive(Clone, Copy, PartialEq, Eq)]
                        enum XformMode {
                            Translate,
                            Rotate,
                            Scale,
                        }
                        // Phase 3 A-5: when `gizmo_action` is `Some`, prefer the handle-derived mode
                        // and fix the mode before reading modifier keys. When `None`, fall back to
                        // Ctrl = Scale, Alt = Rotate, no modifier = Translate.
                        let xform = match app.uv_edit.gizmo_action {
                            Some(UvGizmoAction::ScaleCorner { .. }) if pivot.is_some() => {
                                XformMode::Scale
                            }
                            Some(UvGizmoAction::Rotate) if pivot.is_some() => XformMode::Rotate,
                            _ => {
                                if ctrl_down && pivot.is_some() {
                                    XformMode::Scale
                                } else if alt_down && pivot.is_some() {
                                    XformMode::Rotate
                                } else {
                                    XformMode::Translate
                                }
                            }
                        };

                        // Pre-compute transform parameters per mode.
                        let scale_factor: f32 = if let (XformMode::Scale, Some(pv)) = (xform, pivot)
                        {
                            let pd = ((press_uv[0] - pv[0]).powi(2)
                                + (press_uv[1] - pv[1]).powi(2))
                            .sqrt();
                            let cd = ((cursor_uv[0] - pv[0]).powi(2)
                                + (cursor_uv[1] - pv[1]).powi(2))
                            .sqrt();
                            if pd > 1e-6 {
                                cd / pd
                            } else {
                                1.0
                            }
                        } else {
                            1.0
                        };
                        let (sin_a, cos_a): (f32, f32) =
                            if let (XformMode::Rotate, Some(pv)) = (xform, pivot) {
                                let pa = (press_uv[1] - pv[1]).atan2(press_uv[0] - pv[0]);
                                let ca = (cursor_uv[1] - pv[1]).atan2(cursor_uv[0] - pv[0]);
                                (ca - pa).sin_cos()
                            } else {
                                (0.0, 1.0)
                            };

                        let starts: Vec<((u32, u32, u8), [f32; 2])> = app
                            .uv_edit
                            .drag_start_uvs
                            .iter()
                            .map(|(&k, &v)| (k, v))
                            .collect();
                        let mut new_entries: Vec<((u32, u32, u8), [f32; 2])> =
                            Vec::with_capacity(starts.len());
                        if let Some(loaded) = app.loaded.as_mut() {
                            for ((mi, vi, chan), start_uv) in starts {
                                let (raw_u, raw_v) = match (xform, pivot) {
                                    (XformMode::Scale, Some(pv)) => (
                                        pv[0] + (start_uv[0] - pv[0]) * scale_factor,
                                        pv[1] + (start_uv[1] - pv[1]) * scale_factor,
                                    ),
                                    (XformMode::Rotate, Some(pv)) => {
                                        let du = start_uv[0] - pv[0];
                                        let dv = start_uv[1] - pv[1];
                                        (
                                            pv[0] + du * cos_a - dv * sin_a,
                                            pv[1] + du * sin_a + dv * cos_a,
                                        )
                                    }
                                    _ => {
                                        // Translate (fallback).
                                        let dx = cursor_uv[0] - press_uv[0];
                                        let dy = cursor_uv[1] - press_uv[1];
                                        (start_uv[0] + dx, start_uv[1] + dy)
                                    }
                                };
                                // Shift snap applies only in translate mode.
                                let (nu, nv) =
                                    if shift_down && matches!(xform, XformMode::Translate) {
                                        (snap_to(raw_u, snap_step), snap_to(raw_v, snap_step))
                                    } else {
                                        (raw_u, raw_v)
                                    };
                                // Phase 3 A-1/A-2: write to UV0 / UV1 or the morph offset based on `chan`.
                                if write_displayed_uv(
                                    &mut loaded.ir,
                                    mi,
                                    vi,
                                    [nu, nv],
                                    chan,
                                    active_morph,
                                    &global_offsets,
                                ) {
                                    new_entries.push(((mi, vi, chan), [nu, nv]));
                                }
                            }
                        }
                        for (k, uv) in new_entries {
                            app.uv_edit.overrides.insert(k, uv);
                        }
                        app.uv_edit.dragging = true;
                    }
                }
            }
            UvDragMode::Rect => {
                // Compute vertices inside the rect and recompute `selected` per `behavior` (Replace / Add / Subtract).
                if let (Some(press_uv), Some(cursor_pos)) =
                    (app.uv_edit.drag_press_uv, response.interact_pointer_pos())
                {
                    let cursor_uv = canvas_to_uv(cursor_pos, rect, voff, vzoom);
                    let u_lo = press_uv[0].min(cursor_uv[0]);
                    let u_hi = press_uv[0].max(cursor_uv[0]);
                    let v_lo = press_uv[1].min(cursor_uv[1]);
                    let v_hi = press_uv[1].max(cursor_uv[1]);
                    // Collect vertices inside the rect (only those on `active_chan`).
                    let mut inside: std::collections::HashSet<(u32, u32, u8)> =
                        std::collections::HashSet::new();
                    if let Some(loaded) = app.loaded.as_ref() {
                        let ir = &loaded.ir;
                        for (mi, mesh) in ir.meshes.iter().enumerate() {
                            if mesh.material_index != active_mat {
                                continue;
                            }
                            if active_chan == 1 && mesh.uvs1.len() != mesh.vertices.len() {
                                continue;
                            }
                            let mi_u32 = mi as u32;
                            for vi in 0..mesh.vertices.len() {
                                let vi_u32 = vi as u32;
                                let Some(uv) = read_displayed_uv(
                                    ir,
                                    mi_u32,
                                    vi_u32,
                                    active_chan,
                                    active_morph,
                                    &global_offsets,
                                ) else {
                                    continue;
                                };
                                if uv[0] >= u_lo && uv[0] <= u_hi && uv[1] >= v_lo && uv[1] <= v_hi
                                {
                                    inside.insert((mi_u32, vi_u32, active_chan));
                                }
                            }
                        }
                    }
                    // Rebuild `selected` per `behavior` (Phase 3 A-4).
                    // Add/Subtract preserves the initial selection, so
                    // selections on other UV sets are carried over (no
                    // cross-channel interference: `inside` is only
                    // `active_chan`).
                    app.uv_edit.selected = match app.uv_edit.rect_behavior {
                        UvRectBehavior::Replace => inside,
                        UvRectBehavior::Add => {
                            let mut s = app.uv_edit.rect_initial_selected.clone();
                            s.extend(inside);
                            s
                        }
                        UvRectBehavior::Subtract => {
                            let mut s = app.uv_edit.rect_initial_selected.clone();
                            for k in &inside {
                                s.remove(k);
                            }
                            s
                        }
                    };
                    // Visual feedback for the selection rect (translucent fill + outline, added after vertex drawing).
                    let p0 = uv_to_canvas(press_uv, rect, voff, vzoom);
                    let p1 = uv_to_canvas(cursor_uv, rect, voff, vzoom);
                    let sel_rect = egui::Rect::from_two_pos(p0, p1);
                    painter.rect_filled(
                        sel_rect,
                        0.0,
                        Color32::from_rgba_premultiplied(0x40, 0x70, 0xA0, 0x40),
                    );
                    painter.rect_stroke(
                        sel_rect,
                        0.0,
                        egui::Stroke::new(1.0, Color32::from_rgb(0x66, 0xBB, 0xFF)),
                        egui::StrokeKind::Inside,
                    );
                }
            }
            UvDragMode::None => {}
        }
    }

    // Drag end: mode-specific post-processing + common cleanup.
    if response.drag_stopped() {
        if matches!(app.uv_edit.drag_mode, UvDragMode::Move) && app.uv_edit.dragging {
            // Phase 2-5: record an undo entry (before clearing `drag_start_uvs`).
            let before = app.uv_edit.drag_start_uvs.clone();
            let mut after: std::collections::HashMap<(u32, u32, u8), [f32; 2]> =
                std::collections::HashMap::with_capacity(before.len());
            if let Some(loaded) = app.loaded.as_ref() {
                let ir = &loaded.ir;
                for &(mi, vi, chan) in before.keys() {
                    if let Some(uv) =
                        read_displayed_uv(ir, mi, vi, chan, active_morph, &global_offsets)
                    {
                        after.insert((mi, vi, chan), uv);
                    }
                }
            }
            app.uv_edit.push_undo(before, after);

            let queue = app.render_state.queue.clone();
            if let Some(loaded) = app.loaded.as_mut() {
                loaded.gpu_model.sync_uvs_from_ir(&loaded.ir, &queue);
            }
        }
        app.uv_edit.dragging = false;
        app.uv_edit.drag_mode = UvDragMode::None;
        app.uv_edit.drag_start_uvs.clear();
        app.uv_edit.drag_press_uv = None;
        app.uv_edit.drag_pivot = None;
        app.uv_edit.rect_behavior = UvRectBehavior::Replace;
        app.uv_edit.rect_initial_selected.clear();
        app.uv_edit.gizmo_action = None;
    }

    // Phase 2-4: while dragging in Move mode, show the pivot as a cross marker (visual feedback).
    if app.uv_edit.dragging && matches!(app.uv_edit.drag_mode, UvDragMode::Move) {
        if let Some(pivot) = app.uv_edit.drag_pivot {
            let p = uv_to_canvas(pivot, rect, voff, vzoom);
            let size = 8.0;
            let stroke = egui::Stroke::new(1.5, Color32::from_rgb(0xFF, 0x80, 0x40));
            painter.line_segment(
                [egui::pos2(p.x - size, p.y), egui::pos2(p.x + size, p.y)],
                stroke,
            );
            painter.line_segment(
                [egui::pos2(p.x, p.y - size), egui::pos2(p.x, p.y + size)],
                stroke,
            );
            painter.circle_stroke(p, 3.5, stroke);
        }
    }

    ui.add_space(4.0);
    if let Some(morph_idx) = active_morph {
        let n = app
            .loaded
            .as_ref()
            .map(|l| morph_uv_entry_count(&l.ir, morph_idx))
            .unwrap_or(0);
        ui.small(t!("viewer.uv_edit.morph_edit_mode_hint", count = n));
    } else {
        ui.small(t!("viewer.uv_edit.base_uv_mode_hint"));
    }
}
