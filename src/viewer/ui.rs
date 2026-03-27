use std::collections::HashSet;
use std::path::Path;

use eframe::egui;
use egui::epaint::{Color32, Mesh, Vertex};

use super::app::{ConvertMessage, DisplaySettings, PendingOverlay, SidePanelTab, ViewerApp};
use super::export_filter::build_filtered_ir;
use super::gpu::{DrawMode, LightMode};
use crate::intermediate::types::CullMode;

/// 材質パネルからのテクスチャ割り当てリクエスト
enum TexAssignRequest {
    /// ファイルダイアログから選択
    FileDialog(usize),
    /// pkg_textures から選択（材質Index, pkg内テクスチャIndex）
    PkgTexture(usize, usize),
}

pub fn show_side_panel(ctx: &egui::Context, app: &mut ViewerApp) {
    // テクスチャ割り当てリクエスト（借用制約回避のためパネル外で処理）
    let mut tex_assign_request: Option<TexAssignRequest> = None;

    egui::SidePanel::right("info_panel")
        .default_width(300.0)
        .width_range(200.0..=500.0)
        .show(ctx, |ui| {
            // タブバー
            ui.horizontal(|ui| {
                ui.selectable_value(&mut app.side_panel_tab, SidePanelTab::Info, "情報");
                ui.selectable_value(&mut app.side_panel_tab, SidePanelTab::Control, "操作");
                ui.selectable_value(&mut app.side_panel_tab, SidePanelTab::Display, "表示");
                ui.selectable_value(&mut app.side_panel_tab, SidePanelTab::Export, "出力");
            });
            ui.separator();

            egui::ScrollArea::vertical().show(ui, |ui| match app.side_panel_tab {
                SidePanelTab::Info => show_tab_info(ui, app),
                SidePanelTab::Control => show_tab_control(ui, app),
                SidePanelTab::Display => show_tab_display(ui, app, &mut tex_assign_request),
                SidePanelTab::Export => show_tab_export(ui, app),
            });
        });

    // テクスチャ割り当て（借用解放後に処理）
    match tex_assign_request {
        Some(TexAssignRequest::FileDialog(mat_idx)) => {
            let mat_name = app
                .loaded
                .as_ref()
                .and_then(|l| l.mat_cache.names.get(mat_idx))
                .map(|s| s.as_str())
                .unwrap_or("?");
            let mut dialog = rfd::FileDialog::new()
                .set_title(format!("テクスチャ画像を選択 - {}", mat_name))
                .add_filter("Image", &["png", "jpg", "jpeg", "tga", "bmp", "psd"]);
            if let Some(ref loaded) = app.loaded {
                if let Some(src_name) = loaded
                    .mat_cache
                    .source_tex_names
                    .get(mat_idx)
                    .and_then(|s| s.as_deref())
                {
                    dialog = dialog.set_file_name(src_name);
                }
            }
            if let Some(ref dir) = app.tex.last_dir {
                dialog = dialog.set_directory(dir);
            }
            if let Some(path) = dialog.pick_file() {
                if let Some(dir) = path.parent() {
                    app.tex.last_dir = Some(dir.to_path_buf());
                }
                app.assign_texture_to_material(mat_idx, &path);
            }
        }
        Some(TexAssignRequest::PkgTexture(mat_idx, tex_idx)) => {
            if let Some(ref pkg) = app.tex.pkg_textures {
                if let Some((ref tex_name, ref tex_data)) = pkg.get(tex_idx) {
                    let name = tex_name.clone();
                    let data = tex_data.clone();
                    app.assign_texture_data_to_material(mat_idx, &name, &data);
                    app.tex.pkg_assignments.insert(mat_idx, name.clone());
                    // 同名連動分もpkg割り当て履歴に記録
                    if app.tex.link_same_name {
                        if let Some(ref loaded) = app.loaded {
                            let target_name = loaded.ir.materials[mat_idx].name.clone();
                            for (i, m) in loaded.ir.materials.iter().enumerate() {
                                if i != mat_idx && m.name == target_name {
                                    app.tex.pkg_assignments.insert(i, name.clone());
                                }
                            }
                        }
                    }
                }
            }
        }
        None => {}
    }

    // FBX読み込み方法選択ダイアログ
    show_fbx_choice_dialog(ctx, app);

    // unitypackage モデル選択ダイアログ
    show_fbx_select_dialog(ctx, app);

    // アーカイブ内モデル選択ダイアログ
    show_archive_select_dialog(ctx, app);

    // unitypackage テクスチャ手動割当ダイアログ
    show_tex_match_dialog(ctx, app);
}

/// FBX読み込み方法選択ダイアログ（モデル+アニメーション両方含む場合）
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

    egui::Window::new("FBX読み込み")
        .open(&mut open)
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .show(ctx, |ui| {
            ui.label(format!("\"{}\"", file_name));
            ui.label("モデルとアニメーションの両方が含まれています。");
            ui.separator();
            ui.checkbox(&mut pending.load_model, "モデルを読み込む");
            ui.checkbox(&mut pending.load_animation, "アニメーションを読み込む");
            ui.separator();
            ui.horizontal(|ui| {
                let can_ok = pending.load_model || pending.load_animation;
                if ui.add_enabled(can_ok, egui::Button::new("OK")).clicked() {
                    confirmed = true;
                }
                if ui.button("キャンセル").clicked() {
                    cancelled = true;
                }
            });
        });

    if confirmed {
        let choice = app
            .pending
            .fbx_choice
            .take()
            .expect("pending_fbx_choice は Some 確認済み");
        app.execute_fbx_choice(choice);
    } else if cancelled || !open {
        app.pending.fbx_choice = None;
    }
}

/// unitypackage内に複数モデルがある場合の選択ダイアログ
fn show_fbx_select_dialog(ctx: &egui::Context, app: &mut ViewerApp) {
    if app.pending.unity_pkg.is_none() {
        return;
    }

    let mut selected: Option<(usize, super::app::PkgModelType)> = None;
    let mut cancelled = false;
    let mut open = true;

    egui::Window::new("モデル選択")
        .open(&mut open)
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .show(ctx, |ui| {
            ui.label(".unitypackage 内に複数のモデルが見つかりました。");
            ui.label("読み込むファイルを選択してください。");
            ui.separator();
            // クロージャ内で pending を再借用（名前の String clone を回避）
            let pending = app.pending.unity_pkg.as_ref().unwrap();
            egui::ScrollArea::vertical()
                .max_height(300.0)
                .show(ui, |ui| {
                    for (asset_idx, name, model_type) in &pending.model_list {
                        let type_label = match model_type {
                            super::app::PkgModelType::Vrm => "[VRM]",
                            super::app::PkgModelType::Fbx => "[FBX]",
                        };
                        if ui.button(format!("{} {}", type_label, name)).clicked() {
                            selected = Some((*asset_idx, *model_type));
                        }
                    }
                });
            ui.separator();
            if ui.button("キャンセル").clicked() {
                cancelled = true;
            }
        });

    if let Some((idx, model_type)) = selected {
        let pending = app
            .pending
            .unity_pkg
            .take()
            .expect("pending_unity_pkg は Some 確認済み");
        app.pending.pkg_load = Some(super::app::PendingPkgModelLoad {
            assets: pending.assets,
            fbx_index: idx,
            model_type,
            source_path: pending.source_path,
            shown: false,
            append: pending.append,
            suppress_tex_match: false,
            archive_snapshot: pending.archive_snapshot,
            nested_archive_source: pending.nested_archive_source,
        });
    } else if cancelled || !open {
        app.pending.unity_pkg = None;
    }
}

/// アーカイブ内に複数モデルがある場合の選択ダイアログ
fn show_archive_select_dialog(ctx: &egui::Context, app: &mut ViewerApp) {
    if app.pending.archive.is_none() {
        return;
    }

    let mut selected: Option<usize> = None;
    let mut cancelled = false;
    let mut open = true;

    egui::Window::new("アーカイブ内モデル選択")
        .open(&mut open)
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .show(ctx, |ui| {
            ui.label("アーカイブ内に複数のモデルが見つかりました。");
            ui.label("読み込むファイルを選択してください。");
            ui.separator();
            // クロージャ内で pending を再借用（PathBuf/String clone を回避）
            let pending = app.pending.archive.as_ref().unwrap();
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
            ui.separator();
            if ui.button("キャンセル").clicked() {
                cancelled = true;
            }
        });

    if let Some(idx) = selected {
        let pending = app
            .pending
            .archive
            .take()
            .expect("pending_archive は Some 確認済み");
        app.pending.archive_load = Some(super::app::PendingArchiveLoad {
            archive_data: pending.archive_data,
            format: pending.format,
            contents: pending.contents,
            model_index: idx,
            source_path: pending.source_path,
            shown: false,
            append: pending.append,
            is_temp: pending.is_temp,
        });
    } else if cancelled || !open {
        app.pending.archive = None;
    }
}

/// unitypackage テクスチャ手動割当ダイアログ
fn show_tex_match_dialog(ctx: &egui::Context, app: &mut ViewerApp) {
    let Some(ref pending) = app.tex.pending_match else {
        return;
    };

    // pkg_textures のファイル名一覧とサムネイルIDを参照
    let tex_names: Vec<&str> = app
        .tex
        .pkg_textures
        .as_ref()
        .map(|t| t.iter().map(|(name, _)| name.as_str()).collect())
        .unwrap_or_default();
    let thumb_ids = &app.tex.pkg_thumb_cache;
    if tex_names.is_empty() {
        app.tex.pending_match = None;
        return;
    }

    // loaded から材質名・ソース名を取得（clone 回避）
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

    egui::Window::new("テクスチャ手動割当")
        .open(&mut open)
        .collapsible(false)
        .resizable(true)
        .default_width(450.0)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .show(ctx, |ui| {
            ui.label("自動割当できなかった材質にテクスチャを割り当ててください。");
            ui.horizontal(|ui| {
                ui.label(format!("パッケージ内テクスチャ: {}個", tex_names.len()));
                ui.checkbox(&mut app.tex.link_same_name, "同名連動");
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
                            ui.strong("材質名");
                            ui.strong("元テクスチャ");
                            ui.strong("割当テクスチャ");
                            ui.end_row();

                            for i in 0..mat_count {
                                ui.label(&mat_info[i].0);
                                let src = mat_info[i].1.as_deref().unwrap_or("-");
                                ui.label(egui::RichText::new(src).color(egui::Color32::GRAY));

                                ui.horizontal(|ui| {
                                    // 選択中テクスチャのサムネイル
                                    if let Some(sel_idx) = new_selections[i] {
                                        if let Some(Some(tex_id)) = thumb_ids.get(sel_idx) {
                                            ui.image(egui::load::SizedTexture::new(
                                                *tex_id,
                                                [32.0, 32.0],
                                            ));
                                        }
                                    }
                                    let current_label = new_selections[i]
                                        .and_then(|idx| tex_names.get(idx))
                                        .copied()
                                        .unwrap_or("(なし)");
                                    let popup_id = ui.id().with(("tex_match_popup", i));
                                    let btn = ui.add_sized(
                                        [188.0, 20.0],
                                        egui::Button::new(
                                            egui::RichText::new(format!("⏷ {current_label}"))
                                                .color(ui.visuals().text_color()),
                                        )
                                        .frame(true),
                                    );
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
                                                "(なし)",
                                            ).clicked() {
                                                ui.memory_mut(|m| m.toggle_popup(popup_id));
                                                tex_filter.clear();
                                            }
                                            ui.separator();
                                            ui.add(
                                                egui::TextEdit::singleline(&mut tex_filter)
                                                    .desired_width(ui.available_width())
                                                    .hint_text("テクスチャ名で絞り込み…"),
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
                                ui.end_row();
                            }
                        });
                });

            // フィルタ値を書き戻し
            if let Some(ref mut pending) = app.tex.pending_match {
                pending.tex_filter = tex_filter;
            }

            ui.separator();
            ui.horizontal(|ui| {
                let has_selection = new_selections.iter().any(|s| s.is_some());
                if ui
                    .add_enabled(has_selection, egui::Button::new("適用"))
                    .clicked()
                {
                    apply = true;
                }
                if ui.button("スキップ").clicked() {
                    cancelled = true;
                }
            });
        });

    // 同名連動: 選択が変わった材質と同名の材質にも同じ選択を反映
    if app.tex.link_same_name {
        if let Some(ref pending) = app.tex.pending_match {
            let prev = &pending.selections;
            for i in 0..mat_info.len() {
                if new_selections[i] != prev[i] {
                    // i番目の選択が変更された → 同名材質にも適用
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

    // selections の更新を反映
    if let Some(ref mut pending) = app.tex.pending_match {
        pending.selections = new_selections;
    }

    if apply {
        let pending = app
            .tex
            .pending_match
            .take()
            .expect("pending_match は apply フラグで Some 確認済み");
        // 割り当て情報を先にコピーして借用を解放
        let assignments: Vec<(usize, String, Vec<u8>)> = pending
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
        let count = assignments.len();
        for (mat_idx, tex_name, tex_data) in &assignments {
            app.assign_texture_data_to_material(*mat_idx, tex_name, tex_data);
            app.tex.pkg_assignments.insert(*mat_idx, tex_name.clone());
        }
        if count > 0 {
            app.convert_message = Some(ConvertMessage::success(format!(
                "テクスチャ手動割当: {}材質に適用",
                count
            )));
        }
    } else if cancelled || !open {
        app.tex.pending_match = None;
    }
}

/// テクスチャD&D時の材質選択ダイアログ（複数選択＋リアルタイムプレビュー）
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

    egui::Window::new("テクスチャ割り当て")
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .show(ctx, |ui| {
            // サムネイル + ファイル名を横並び表示
            ui.horizontal(|ui| {
                if let Some(tex_id) = preview.preview_tex_id {
                    let thumb_size = 64.0;
                    ui.image(egui::load::SizedTexture::new(
                        tex_id,
                        [thumb_size, thumb_size],
                    ));
                }
                ui.vertical(|ui| {
                    ui.label(format!("画像: {}", file_name));
                    ui.add_space(2.0);
                    ui.label("チェックでプレビュー、適用で確定");
                });
            });
            ui.separator();
            ui.horizontal(|ui| {
                if ui.small_button("全選択").clicked() {
                    preview.selection.iter_mut().for_each(|v| *v = true);
                }
                if ui.small_button("全解除").clicked() {
                    preview.selection.iter_mut().for_each(|v| *v = false);
                }
                if ui.small_button("未設定のみ").clicked() {
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
                        // FBX 元テクスチャファイル名
                        let src_name = loaded
                            .mat_cache
                            .source_tex_names
                            .get(mat_idx)
                            .and_then(|s| s.as_deref())
                            .unwrap_or("");
                        ui.horizontal(|ui| {
                            ui.label(indicator);
                            ui.checkbox(&mut preview.selection[mat_idx], name);
                            if !src_name.is_empty() {
                                ui.label(
                                    egui::RichText::new(src_name)
                                        .small()
                                        .color(egui::Color32::from_gray(0x90)),
                                );
                            }
                        });
                    }
                });
            ui.add_space(8.0);
            let selected_count = preview.selection.iter().filter(|&&v| v).count();
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(selected_count > 0, egui::Button::new("適用"))
                    .clicked()
                {
                    apply = true;
                }
                if ui.button("キャンセル").clicked() {
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

/// PMX変換を実行
pub fn execute_conversion(app: &mut ViewerApp) {
    if app.loaded.is_none() {
        return;
    }
    let output_path = std::path::PathBuf::from(&app.export.pmx_output_path);
    let log_path = output_path.with_extension("log");

    // 変換前のビューアログファイルサイズを記録
    let viewer_log_path = Some(app.log_path.clone());
    let log_offset_before = viewer_log_path
        .as_ref()
        .and_then(|p| std::fs::metadata(p).ok())
        .map(|m| m.len())
        .unwrap_or(0);

    // 法線が変更されている場合、IrModel に書き戻してから変換
    let normals_modified = app.display.smooth_normals || app.display.clear_custom_normals;
    if normals_modified {
        if let Some(ref mut loaded) = app.loaded {
            loaded.gpu_model.write_normals_back(&mut loaded.ir);
        }
    }
    let loaded = app
        .loaded
        .as_ref()
        .expect("loaded は has_model チェック済み");

    // 可視材質フィルタリング
    let filtered_ir;
    let ir_ref = if app.export.export_visible_only {
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
            "表示材質のみ出力: {}/{} 材質を出力",
            visible_mat_indices.len(),
            loaded.ir.materials.len()
        );
        filtered_ir = build_filtered_ir(&loaded.ir, &visible_mat_indices);
        &filtered_ir
    } else {
        &loaded.ir
    };

    // PMX/PMD 形式では no_physics/raw_structure は無効（UI もグレーアウト）
    let is_pmx_pmd = ir_ref.source_format.is_pmx_pmd();
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
    };
    let result = crate::convert_ir_to_pmx(ir_ref, &output_path, &options);

    if app.export.output_log {
        let debug_logs = viewer_log_path
            .as_ref()
            .and_then(|p| read_log_from_offset(p, log_offset_before));

        write_convert_log(&log_path, ir_ref, result.as_ref(), debug_logs.as_deref());
    }

    match result {
        Ok(stats) => {
            let mut msg = format!(
                "変換完了: {}\nボーン: {} / 頂点: {} / 材質: {} / モーフ: {}",
                stats.output_path, stats.bones, stats.vertices, stats.materials, stats.morphs,
            );
            if app.export.output_log {
                msg += &format!("\nログ: {}", log_path.display());
            }
            // A/Tスタンス変換の結果に応じて警告を付加（primary_astance_result を参照）
            use crate::intermediate::types::AStanceResult;
            let stance_label = if ir_ref.source_format.is_pmx_pmd() {
                "Tスタンス"
            } else {
                "Aスタンス"
            };
            let primary_result = app
                .loaded
                .as_ref()
                .map(|l| l.primary_astance_result)
                .unwrap_or_default();
            let has_warning = match primary_result {
                AStanceResult::NotFound => {
                    msg += &format!(
                        "\n⚠ {}変換: 腕ボーンが見つからず変換できませんでした",
                        stance_label
                    );
                    true
                }
                AStanceResult::AlreadyAStance => {
                    msg += &format!("\n※ 既に{}に近いためスキップしました", stance_label);
                    false
                }
                _ => false,
            };
            if has_warning {
                app.convert_message = Some(ConvertMessage::warning(msg));
            } else {
                app.convert_message = Some(ConvertMessage::success(msg));
            }
        }
        Err(e) => {
            app.convert_message = Some(ConvertMessage::failure(format!(
                "変換失敗: {e}\n出力先のパスやディスク容量を確認してください。"
            )));
        }
    }
}

/// メタ情報をセクションごとに折り畳み可能な Grid で表示
/// 情報タブ: モデル情報 + メタ情報
fn show_tab_info(ui: &mut egui::Ui, app: &mut ViewerApp) {
    let Some(ref loaded) = app.loaded else {
        ui.label("VRM ファイルを読み込んでください (Ctrl+O)");
        return;
    };
    let ir = &loaded.ir;

    ui.heading(egui::RichText::new("モデル情報").color(egui::Color32::from_gray(0x20)));
    ui.separator();
    egui::Grid::new("model_info").show(ui, |ui| {
        ui.label("名前");
        ui.label(&ir.name);
        ui.end_row();

        ui.label("ボーン数");
        ui.label(ir.bones.len().to_string());
        ui.end_row();

        ui.label("頂点数");
        ui.label(ir.total_vertices().to_string());
        ui.end_row();

        ui.label("面数");
        ui.label(ir.total_faces().to_string());
        ui.end_row();

        ui.label("材質数");
        ui.label(ir.materials.len().to_string());
        ui.end_row();

        ui.label("テクスチャ数");
        ui.label(ir.textures.len().to_string());
        ui.end_row();

        ui.label("モーフ数");
        ui.label(ir.morphs.len().to_string());
        ui.end_row();

        ui.label("形式");
        ui.label(ir.source_format.label());
        ui.end_row();

        if let Some(ref rig) = ir.rig_type {
            ui.label("リグ");
            ui.label(rig);
            ui.end_row();

            ui.label("Humanoid");
            if ir.humanoid_bone_count > 0 {
                ui.label(format!("{}本マッピング済", ir.humanoid_bone_count));
            } else {
                ui.colored_label(egui::Color32::GRAY, "非対応");
            }
            ui.end_row();
        }
    });

    ui.add_space(12.0);

    // メタ情報 / コメント
    if !ir.comment.is_empty() {
        if ir.source_format.is_pmx_pmd() {
            // PMX/PMD: 自由形式コメントをそのまま表示
            ui.heading(egui::RichText::new("コメント").color(egui::Color32::from_gray(0x20)));
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

/// 操作タブ: アニメーション制御 + 表情モーフスライダ
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

    // アニメーションが制御中の表情名を収集
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

    ui.heading(egui::RichText::new("表情モーフ").color(egui::Color32::from_gray(0x20)));
    ui.separator();
    if ui.small_button("全リセット").clicked() {
        for (i, w) in app.morph_weights.iter_mut().enumerate() {
            if !anim_expr_morphs.contains(&i) {
                *w = 0.0;
            }
        }
        app.morph_dirty = true;
    }
    ui.separator();
    for (i, morph) in ir.morphs.iter().enumerate() {
        if i < app.morph_weights.len() {
            let is_anim_controlled = anim_expr_morphs.contains(&i);
            ui.horizontal(|ui| {
                ui.add_enabled_ui(!is_anim_controlled, |ui| {
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
            });
        }
    }
}

/// 表示タブ: 表示設定 + 材質表示
fn show_tab_display(
    ui: &mut egui::Ui,
    app: &mut ViewerApp,
    tex_assign_request: &mut Option<TexAssignRequest>,
) {
    // 表示設定
    ui.heading(egui::RichText::new("表示設定").color(egui::Color32::from_gray(0x20)));
    ui.separator();

    if ui.small_button("ライト初期値").clicked() {
        let d = DisplaySettings::default();
        app.display.light_intensity = d.light_intensity;
        app.display.light_color = d.light_color;
        app.display.ambient_intensity = d.ambient_intensity;
        app.display.ambient_sky_color = d.ambient_sky_color;
        app.display.ambient_ground_color = d.ambient_ground_color;
        app.display.bg_brightness = d.bg_brightness;
    }
    // ライト・環境光・Ground のカラーボタン位置を Grid で揃える
    egui::Grid::new("light_color_grid")
        .num_columns(2)
        .show(ui, |ui| {
            ui.add(egui::Slider::new(&mut app.display.light_intensity, 0.0..=2.0).text("ライト"));
            color_wheel_button_rgb(ui, "light_color", &mut app.display.light_color);
            ui.end_row();

            ui.add(egui::Slider::new(&mut app.display.ambient_intensity, 0.0..=1.0).text("環境光"));
            color_wheel_button_rgb(ui, "ambient_sky", &mut app.display.ambient_sky_color);
            ui.end_row();

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label("Ground");
            });
            color_wheel_button_rgb(ui, "ambient_ground", &mut app.display.ambient_ground_color);
            ui.end_row();
        });
    ui.add(egui::Slider::new(&mut app.display.bg_brightness, 0.0..=1.0).text("背景"));
    ui.checkbox(&mut app.display.show_grid, "グリッド表示 (G)");

    let has_bones = app.loaded.as_ref().is_some_and(|l| !l.ir.bones.is_empty());
    let has_spring = app
        .loaded
        .as_ref()
        .is_some_and(|l| !l.ir.physics.rigid_bodies.is_empty());
    ui.add_enabled_ui(has_bones, |ui| {
        ui.checkbox(&mut app.display.show_bones, "ボーン表示 (B)")
            .on_disabled_hover_text("モデルにボーンがありません");
        if app.display.show_bones {
            ui.add(egui::Slider::new(&mut app.display.bone_opacity, 0.05..=1.0).text("ボーン濃度"));
        }
    });
    ui.add_enabled_ui(has_spring, |ui| {
        ui.checkbox(&mut app.display.show_spring_bones, "物理表示 (P)")
            .on_disabled_hover_text("モデルに物理設定がありません");
        if app.display.show_spring_bones {
            ui.add(
                egui::Slider::new(&mut app.display.spring_bone_opacity, 0.05..=1.0)
                    .text("物理濃度"),
            );
        }
    });
    // ジョイント表示（PMX/PMDのみ）
    let is_pmx_pmd_joints = app
        .loaded
        .as_ref()
        .is_some_and(|l| l.ir.source_format.is_pmx_pmd());
    let has_joints = app
        .loaded
        .as_ref()
        .is_some_and(|l| !l.ir.physics.joints.is_empty());
    if is_pmx_pmd_joints && has_joints {
        ui.checkbox(&mut app.display.show_joints, "ジョイント表示");
        if app.display.show_joints {
            ui.add(
                egui::Slider::new(&mut app.display.joint_opacity, 0.05..=1.0)
                    .text("ジョイント濃度"),
            );
        }
    }
    // ワイヤーフレーム
    let supports_wire = app
        .renderer
        .as_ref()
        .map(|r| r.supports_wireframe())
        .unwrap_or(false);
    if supports_wire {
        ui.horizontal(|ui| {
            ui.label("描画 (W):");
            ui.selectable_value(&mut app.display.draw_mode, DrawMode::Solid, "Solid");
            ui.selectable_value(&mut app.display.draw_mode, DrawMode::Wireframe, "Wire");
            ui.selectable_value(&mut app.display.draw_mode, DrawMode::SolidWireframe, "S+W");
        });
    }
    // ライトモード
    ui.horizontal(|ui| {
        ui.label("ライト:");
        ui.selectable_value(
            &mut app.display.light_mode,
            LightMode::CameraFollow,
            "カメラ追従",
        );
        ui.selectable_value(&mut app.display.light_mode, LightMode::Fixed, "固定 (L)");
    });
    // MMD レンダリング UI（MMD リソース構築済みの draw があるかで判定 — mmd_mode ON/OFF に依存しない）
    let has_mmd_capability = app.loaded.as_ref().is_some_and(|l| {
        l.gpu_model
            .draws
            .iter()
            .any(|d| d.mmd_material_bind_group.is_some())
    });
    ui.separator();
    // MToon アウトラインを持つ Standard draw があるかで有効判定
    let has_outline_draws = app.loaded.as_ref().is_some_and(|l| {
        l.gpu_model
            .draws
            .iter()
            .any(|d| d.render_style == super::mesh::RenderStyle::Standard && d.has_outline)
    });
    ui.add_enabled(
        has_outline_draws,
        egui::Checkbox::new(&mut app.display.outline_enabled, "アウトライン描画"),
    );

    if has_mmd_capability {
        ui.checkbox(&mut app.display.mmd_mode, "MMD レンダリング");
        if app.display.mmd_mode {
            ui.checkbox(&mut app.display.mmd_edge_enabled, "エッジ描画");
            if app.display.mmd_edge_enabled {
                ui.add(
                    egui::Slider::new(&mut app.display.mmd_edge_thickness, 0.1..=3.0)
                        .text("エッジ太さ"),
                );
            }
        }
    }

    ui.separator();
    ui.checkbox(&mut app.display.msaa, "MSAA (アンチエイリアス)");
    ui.checkbox(&mut app.display.show_normal_map, "法線マップ表示");
    ui.checkbox(&mut app.display.show_normals, "法線表示 (N)");
    if app.display.show_normals {
        ui.add(egui::Slider::new(&mut app.display.normal_length, 0.1..=3.0).text("法線長さ"));
    }
    let has_mmd_normals = app.loaded.as_ref().is_some_and(|l| {
        l.gpu_model
            .draws
            .iter()
            .any(|d| d.render_style == super::mesh::RenderStyle::Mmd)
    });
    let has_normal_map = app
        .loaded
        .as_ref()
        .is_some_and(|l| l.ir.materials.iter().any(|m| m.normal_texture.is_some()));
    ui.add_enabled_ui(!has_mmd_normals, |ui| {
        let smooth_disabled = has_normal_map;
        let old_smooth = app.display.smooth_normals;
        let resp = ui.add_enabled(
            !smooth_disabled,
            egui::Checkbox::new(&mut app.display.smooth_normals, "法線平滑化"),
        );
        if smooth_disabled {
            resp.on_disabled_hover_text(
                "法線マップ付き材質があるため無効（UV seam 境界で tangent が不正確になります）",
            );
        } else {
            resp.on_disabled_hover_text("PMX/PMD の法線は変更できません");
        }
        // smooth_normals が強制無効化された場合、値もリセット
        if smooth_disabled && app.display.smooth_normals {
            app.display.smooth_normals = false;
        }
        if app.display.smooth_normals != old_smooth && app.loaded.is_some() {
            app.pending.rebuild = Some(PendingOverlay::WaitingOverlay);
        }
        let old_clear = app.display.clear_custom_normals;
        let clear_resp = ui.add_enabled(
            !smooth_disabled,
            egui::Checkbox::new(&mut app.display.clear_custom_normals, "カスタム法線クリア"),
        );
        if smooth_disabled {
            clear_resp.on_disabled_hover_text(
                "法線マップ付き材質があるため無効（UV seam 境界で tangent が不正確になります）",
            );
            app.display.clear_custom_normals = false;
        } else {
            clear_resp.on_disabled_hover_text("PMX/PMD の法線は変更できません");
        }
        if app.display.clear_custom_normals != old_clear && app.loaded.is_some() {
            app.pending.rebuild = Some(PendingOverlay::WaitingOverlay);
        }
    });

    ui.add_space(12.0);

    // 材質表示
    let Some(ref loaded) = app.loaded else { return };
    if loaded.gpu_model.draws.is_empty() {
        return;
    }

    let draw_info = &loaded.mat_cache.draw_indices;
    let mat_tex_info = &loaded.mat_cache.tex_indices;
    let mat_names = &loaded.mat_cache.names;
    let mat_src_tex = &loaded.mat_cache.source_tex_names;
    let num_draws = draw_info.len();

    ui.heading(egui::RichText::new("材質表示").color(egui::Color32::from_gray(0x20)));
    ui.separator();
    ui.horizontal(|ui| {
        if ui.small_button("全表示").clicked() {
            app.material_visibility.iter_mut().for_each(|v| *v = true);
        }
        if ui.small_button("全非表示").clicked() {
            app.material_visibility.iter_mut().for_each(|v| *v = false);
        }
        ui.checkbox(&mut app.tex.link_same_name, "同名連動")
            .on_hover_text("同じ名前の材質にテクスチャを同時に割り当て");
        if !app.tex.assignments.is_empty() && ui.small_button("テクスチャリセット").clicked()
        {
            app.tex.assignments.clear();
            app.tex.pkg_assignments.clear();
            app.pending.reload = Some(PendingOverlay::WaitingOverlay);
        }
    });
    // フィルター（材質数が多い場合に便利）
    if num_draws > 10 {
        ui.horizontal(|ui| {
            ui.label("検索:");
            ui.add(
                egui::TextEdit::singleline(&mut app.material_filter)
                    .desired_width(ui.available_width())
                    .hint_text("材質名で絞り込み…"),
            );
        });
    }
    let filter_lower = app.material_filter.to_lowercase();
    let thumb_ids = &app.tex.pkg_thumb_cache;
    // グループ情報を軽量抽出（名前と draw_range のみ。MaterialGroup 全体の clone を回避）
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
    let has_groups = group_names.len() > 1;

    if has_groups {
        // DrawCall Index → グループIndex
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
        // draw_info もクローン（CollapsingHeader クロージャ内で使うため）
        let draw_info_owned = draw_info.to_vec();
        // loaded の借用を解放
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
            let id = ui.id().with(("mat_group", gi));
            egui::CollapsingHeader::new(
                egui::RichText::new(gname)
                    .color(egui::Color32::from_rgb(0x60, 0xA0, 0xFF))
                    .strong()
            )
            .id_salt(id)
            .default_open(true)
            .show(ui, |ui| {
                let loaded = app.loaded.as_ref().unwrap();
                let mat_tex_info = &loaded.mat_cache.tex_indices;
                let mat_names = &loaded.mat_cache.names;
                let mat_src_tex = &loaded.mat_cache.source_tex_names;
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
                    ui.horizontal(|ui| {
                // テクスチャ状態インジケータ
                {
                    let has_tex = mat_tex_info.get(mat_idx)
                        .and_then(|t| *t)
                        .is_some();
                    let indicator = if has_tex {
                        egui::RichText::new("\u{25A3}")
                            .color(egui::Color32::from_rgb(0x40, 0xC0, 0x40))
                            .size(16.0)
                    } else {
                        egui::RichText::new("\u{25A1}")
                            .color(egui::Color32::from_rgb(0xA0, 0x60, 0x60))
                            .size(16.0)
                    };
                    let src_name = mat_src_tex.get(mat_idx)
                        .and_then(|s: &Option<String>| s.as_deref());
                    let tooltip = match (has_tex, src_name) {
                        (true, Some(s)) => format!("テクスチャ設定済 ({})\nクリックで変更", s),
                        (true, None) => "テクスチャ設定済\nクリックで変更".to_string(),
                        (false, Some(s)) => format!("テクスチャ未設定 ({})\nクリックで割り当て", s),
                        (false, None) => "テクスチャ未設定\nクリックで割り当て".to_string(),
                    };
                    let resp = ui.add(egui::Label::new(indicator).sense(egui::Sense::click()))
                        .on_hover_text(&tooltip);
                    let has_pkg = app.tex.pkg_textures.is_some();
                    let popup_id = ui.id().with(("pkg_tex_popup", mat_idx));
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
                            // 「ファイルから選択」を先頭に配置
                            if ui.button("ファイルから選択...").clicked() {
                                *tex_assign_request = Some(TexAssignRequest::FileDialog(mat_idx));
                                ui.memory_mut(|m| m.toggle_popup(popup_id));
                                app.tex.pkg_popup_filter.clear();
                            }
                            ui.separator();
                            ui.add(
                                egui::TextEdit::singleline(&mut app.tex.pkg_popup_filter)
                                    .desired_width(ui.available_width())
                                    .hint_text("テクスチャ名で絞り込み…"),
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
                if let Some(tex_name) = display_tex {
                    ui.checkbox(
                        &mut app.material_visibility[i],
                        format!("{} [{}]", name, tex_name),
                    );
                } else {
                    ui.checkbox(&mut app.material_visibility[i], name);
                }
                    });
                }
            });
        }
    } else {
        for &(i, mat_idx) in draw_info.iter() {
            if i >= app.material_visibility.len() {
                continue;
            }
            let loaded = app.loaded.as_ref().unwrap();
            let mat_tex_info = &loaded.mat_cache.tex_indices;
            let mat_names = &loaded.mat_cache.names;
            let mat_src_tex = &loaded.mat_cache.source_tex_names;
            let name = mat_names
                .get(mat_idx)
                .map(|s: &String| s.as_str())
                .unwrap_or("?");
            if !filter_lower.is_empty() && !name.to_lowercase().contains(&filter_lower) {
                continue;
            }
            ui.horizontal(|ui| {
                // テクスチャ状態インジケータ
                {
                    let has_tex = mat_tex_info.get(mat_idx).and_then(|t| *t).is_some();
                    let indicator = if has_tex {
                        egui::RichText::new("\u{25A3}")
                            .color(egui::Color32::from_rgb(0x40, 0xC0, 0x40))
                            .size(16.0)
                    } else {
                        egui::RichText::new("\u{25A1}")
                            .color(egui::Color32::from_rgb(0xA0, 0x60, 0x60))
                            .size(16.0)
                    };
                    let src_name = mat_src_tex
                        .get(mat_idx)
                        .and_then(|s: &Option<String>| s.as_deref());
                    let tooltip = match (has_tex, src_name) {
                        (true, Some(s)) => format!("テクスチャ設定済 ({})\nクリックで変更", s),
                        (true, None) => "テクスチャ設定済\nクリックで変更".to_string(),
                        (false, Some(s)) => format!("テクスチャ未設定 ({})\nクリックで割り当て", s),
                        (false, None) => "テクスチャ未設定\nクリックで割り当て".to_string(),
                    };
                    let resp = ui
                        .add(egui::Label::new(indicator).sense(egui::Sense::click()))
                        .on_hover_text(&tooltip);
                    let has_pkg = app.tex.pkg_textures.is_some();
                    let popup_id = ui.id().with(("pkg_tex_popup", mat_idx));
                    if resp.clicked() {
                        if has_pkg {
                            ui.memory_mut(|m| m.toggle_popup(popup_id));
                        } else {
                            *tex_assign_request = Some(TexAssignRequest::FileDialog(mat_idx));
                        }
                    }
                    if has_pkg {
                        egui::popup_below_widget(
                            ui,
                            popup_id,
                            &resp,
                            egui::PopupCloseBehavior::CloseOnClickOutside,
                            |ui| {
                                ui.set_min_width(280.0);
                                if ui.button("ファイルから選択...").clicked() {
                                    *tex_assign_request =
                                        Some(TexAssignRequest::FileDialog(mat_idx));
                                    ui.memory_mut(|m| m.toggle_popup(popup_id));
                                    app.tex.pkg_popup_filter.clear();
                                }
                                ui.separator();
                                ui.add(
                                    egui::TextEdit::singleline(&mut app.tex.pkg_popup_filter)
                                        .desired_width(ui.available_width())
                                        .hint_text("テクスチャ名で絞り込み…"),
                                );
                                let filter_lower = app.tex.pkg_popup_filter.to_lowercase();
                                egui::ScrollArea::vertical()
                                    .max_height(400.0)
                                    .scroll_bar_visibility(
                                        egui::scroll_area::ScrollBarVisibility::AlwaysVisible,
                                    )
                                    .show(ui, |ui| {
                                        if let Some(ref pkg) = app.tex.pkg_textures {
                                            for (ti, (tname, _)) in pkg.iter().enumerate() {
                                                if !filter_lower.is_empty()
                                                    && !tname.to_lowercase().contains(&filter_lower)
                                                {
                                                    continue;
                                                }
                                                let clicked = ui
                                                    .horizontal(|ui| {
                                                        if let Some(Some(tex_id)) =
                                                            thumb_ids.get(ti)
                                                        {
                                                            ui.image(
                                                                egui::load::SizedTexture::new(
                                                                    *tex_id,
                                                                    [32.0, 32.0],
                                                                ),
                                                            );
                                                        }
                                                        ui.button(tname).clicked()
                                                    })
                                                    .inner;
                                                if clicked {
                                                    *tex_assign_request = Some(
                                                        TexAssignRequest::PkgTexture(mat_idx, ti),
                                                    );
                                                    ui.memory_mut(|m| m.toggle_popup(popup_id));
                                                    app.tex.pkg_popup_filter.clear();
                                                }
                                            }
                                        }
                                    });
                            },
                        );
                    }
                }
                let assigned_name = app.tex.assignments.get(&mat_idx).map(|ts| {
                    let name = ts.display_name();
                    std::path::Path::new(&name)
                        .file_name()
                        .map(|f| f.to_string_lossy().into_owned())
                        .unwrap_or(name)
                });
                let display_tex = assigned_name
                    .as_deref()
                    .or_else(|| mat_src_tex.get(mat_idx).and_then(|s| s.as_deref()));
                if let Some(tex_name) = display_tex {
                    ui.checkbox(
                        &mut app.material_visibility[i],
                        format!("{} [{}]", name, tex_name),
                    );
                } else {
                    ui.checkbox(&mut app.material_visibility[i], name);
                }
            });
        }
    }
}

/// 出力タブ: PMX変換 + UVマップ出力
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
    let is_processing = app.pending.load.is_some()
        || app.pending.append.is_some()
        || app.pending.convert.is_some()
        || app.pending.rebuild.is_some()
        || app.pending.reload.is_some()
        || app.pending.pkg_load.is_some();

    ui.heading(egui::RichText::new("PMX 変換").color(egui::Color32::from_gray(0x20)));
    ui.separator();

    // PMX/PMD ロード時は PMX 変換関連をグレーアウト
    ui.add_enabled_ui(has_model && !is_processing && !is_pmx_pmd, |ui| {
        if ui
            .button("PMX 変換")
            .on_disabled_hover_text(if is_pmx_pmd {
                "PMX/PMD ファイルからの変換は不要です"
            } else if is_processing {
                "処理中です..."
            } else {
                "モデルを読み込んでください"
            })
            .clicked()
        {
            let default_path = if app.export.pmx_output_path.is_empty() {
                std::path::PathBuf::from("output.pmx")
            } else {
                std::path::PathBuf::from(&app.export.pmx_output_path)
            };
            let mut dialog = rfd::FileDialog::new()
                .set_title("PMX出力先を選択")
                .add_filter("PMX", &["pmx"]);
            if let Some(dir) = default_path.parent() {
                dialog = dialog.set_directory(dir);
            }
            if let Some(name) = default_path.file_name() {
                dialog = dialog.set_file_name(name.to_string_lossy());
            }
            if let Some(path) = dialog.save_file() {
                app.export.pmx_output_path = path.to_string_lossy().into_owned();
                app.pending.convert = Some(PendingOverlay::WaitingOverlay);
            }
        }
    });
    // PMX/PMD 時: T→Aスタンスはグレーアウト、代わりにA→Tスタンスを表示
    let is_fbx = app
        .loaded
        .as_ref()
        .is_some_and(|l| l.ir.source_format == crate::intermediate::types::SourceFormat::Fbx);
    if is_pmx_pmd {
        if ui
            .checkbox(&mut app.normalize_pose, "Tスタンス変換")
            .changed()
        {
            app.pending.reload = Some(PendingOverlay::WaitingOverlay);
        }
    } else {
        ui.add_enabled_ui(has_humanoid, |ui| {
            if ui
                .checkbox(&mut app.normalize_pose, "Aスタンス変換")
                .on_disabled_hover_text("ヒューマノイドボーンがありません")
                .changed()
            {
                // Aスタンス変換ONならTスタンス変換OFFに
                if app.normalize_pose {
                    app.normalize_to_tstance = false;
                }
                app.pending.reload = Some(PendingOverlay::WaitingOverlay);
            }
        });
        // FBX 時: A→Tスタンス変換チェックボックス追加
        if is_fbx {
            ui.add_enabled_ui(has_humanoid, |ui| {
                if ui
                    .checkbox(&mut app.normalize_to_tstance, "Tスタンス変換")
                    .on_disabled_hover_text("ヒューマノイドボーンがありません")
                    .changed()
                {
                    // Tスタンス変換ONならAスタンス変換OFFに
                    if app.normalize_to_tstance {
                        app.normalize_pose = false;
                    }
                    app.pending.reload = Some(PendingOverlay::WaitingOverlay);
                }
            });
        }
    }
    ui.add_enabled_ui(has_physics && !is_pmx_pmd, |ui| {
        ui.checkbox(
            &mut app.display.align_rigid_rotation,
            "剛体回転をボーン方向に揃える",
        )
        .on_disabled_hover_text("物理設定がないか、PMX/PMD形式です");
    });
    ui.add_enabled_ui(has_physics && !is_pmx_pmd, |ui| {
        ui.checkbox(&mut app.export.no_physics, "物理なしで出力")
            .on_hover_text("剛体・ジョイントを出力しません")
            .on_disabled_hover_text("物理設定がないか、PMX/PMD形式です");
    });
    ui.add_enabled_ui(has_model && !is_pmx_pmd, |ui| {
        ui.checkbox(&mut app.export.raw_structure, "元のボーン構造で出力")
            .on_hover_text("標準ボーン（IK・捩り等）の挿入をスキップします")
            .on_disabled_hover_text("PMX/PMD形式では使用できません");
    });
    ui.add_enabled_ui(has_model && !is_pmx_pmd, |ui| {
        ui.checkbox(&mut app.export.export_visible_only, "表示材質のみ出力")
            .on_hover_text("表示タブで非表示にした材質を出力から除外します");
    });
    ui.add_enabled_ui(!is_pmx_pmd, |ui| {
        ui.checkbox(&mut app.export.output_log, "ログ出力")
            .on_disabled_hover_text("PMX/PMD形式ではログ出力はできません");
    });

    ui.add_space(12.0);

    // UVマップ出力
    ui.heading(egui::RichText::new("UVマップ出力").color(egui::Color32::from_gray(0x20)));
    ui.separator();
    ui.add_enabled_ui(has_model && !is_processing, |ui| {
        if ui.button("UVマップ出力").clicked() {
            let default_path = if app.export.pmx_output_path.is_empty() {
                std::path::PathBuf::from("uvmap.psd")
            } else {
                std::path::PathBuf::from(&app.export.pmx_output_path).with_extension("psd")
            };
            let mut dialog = rfd::FileDialog::new()
                .set_title("UVマップ出力先を選択")
                .add_filter("PSD", &["psd"]);
            if let Some(dir) = default_path.parent() {
                dialog = dialog.set_directory(dir);
            }
            if let Some(name) = default_path.file_name() {
                dialog = dialog.set_file_name(name.to_string_lossy());
            }
            if let Some(path) = dialog.save_file() {
                let Some(loaded) = app.loaded.as_ref() else {
                    app.convert_message = Some(ConvertMessage::failure(
                        "モデルが読み込まれていません".to_string(),
                    ));
                    return;
                };
                let uv_groups: Vec<(String, std::ops::Range<usize>)> = loaded
                    .material_groups
                    .iter()
                    .map(|g| (g.name.clone(), g.material_range.clone()))
                    .collect();
                match crate::convert::uvmap::export_uv_map_grouped(
                    &loaded.ir,
                    &path,
                    app.export.uv_map_size,
                    &uv_groups,
                ) {
                    Ok(()) => {
                        app.convert_message = Some(ConvertMessage::success(format!(
                            "UVマップ出力完了: {}",
                            path.display()
                        )));
                    }
                    Err(e) => {
                        app.convert_message =
                            Some(ConvertMessage::failure(format!("UVマップ出力失敗: {e}")));
                    }
                }
            }
        }
    });
    ui.horizontal(|ui| {
        ui.label("解像度:");
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

fn show_meta_info(ui: &mut egui::Ui, comment: &str) {
    // comment 形式: "[Section]" 行でセクション区切り、"  label: value" 行がフィールド
    // まずセクション単位にパースする
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
        egui::CollapsingHeader::new(&sec.title)
            .id_salt(id)
            .default_open(true)
            .show(ui, |ui| {
                egui::Grid::new(format!("meta_grid_{i}"))
                    .num_columns(2)
                    .spacing([8.0, 4.0])
                    .show(ui, |ui| {
                        for (label, value) in &sec.fields {
                            ui.label(label.as_str());
                            ui.label(value.as_str());
                            ui.end_row();
                        }
                    });
            });
    }
}

/// ビューアログファイルから指定オフセット以降を読み取る
fn read_log_from_offset(path: &Path, offset: u64) -> Option<String> {
    use std::io::{Read, Seek, SeekFrom};
    let mut file = std::fs::File::open(path).ok()?;
    file.seek(SeekFrom::Start(offset)).ok()?;
    let mut buf = String::new();
    file.read_to_string(&mut buf).ok()?;
    if buf.is_empty() {
        None
    } else {
        Some(buf)
    }
}

/// 変換ログをファイルに書き出す
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
            log::warn!("ログファイル作成失敗: {e}");
            return;
        }
    };

    let _ = writeln!(file, "[vrm-viewer] PMX変換ログ");
    let _ = writeln!(
        file,
        "日時: {}",
        chrono::Local::now().format("%Y-%m-%d %H:%M:%S")
    );
    let _ = writeln!(file, "ソース形式: {}", ir.source_format.label());
    let _ = writeln!(file);

    // 入力モデル情報
    let _ = writeln!(file, "=== 入力 VRM ===");
    let _ = writeln!(file, "モデル名: {}", ir.name);
    let _ = writeln!(file, "ボーン数: {}", ir.bones.len());
    let _ = writeln!(file, "頂点数: {}", ir.total_vertices());
    let _ = writeln!(file, "面数: {}", ir.total_faces());
    let _ = writeln!(file, "材質数: {}", ir.materials.len());
    let _ = writeln!(file, "テクスチャ数: {}", ir.textures.len());
    let _ = writeln!(file, "モーフ数: {}", ir.morphs.len());
    let _ = writeln!(file, "剛体数: {}", ir.physics.rigid_bodies.len());
    let _ = writeln!(file, "ジョイント数: {}", ir.physics.joints.len());

    // ボーン一覧
    let _ = writeln!(file);
    let _ = writeln!(file, "--- ボーン一覧 ---");
    for (i, bone) in ir.bones.iter().enumerate() {
        let vrm_name = bone.vrm_bone_name.as_deref().unwrap_or("-");
        let _ = writeln!(file, "  [{:3}] {} (vrm: {})", i, bone.name, vrm_name);
    }

    // モーフ一覧
    let _ = writeln!(file);
    let _ = writeln!(file, "--- モーフ一覧 ---");
    for morph in &ir.morphs {
        let _ = writeln!(file, "  [panel{}] {}", morph.panel, morph.name);
    }

    // 材質一覧
    let _ = writeln!(file);
    let _ = writeln!(file, "--- 材質一覧 ---");
    for (i, mat) in ir.materials.iter().enumerate() {
        let _ = writeln!(
            file,
            "  [{:2}] {} (tex:{:?} double:{} mtoon:{})",
            i,
            mat.name,
            mat.texture_index,
            mat.cull_mode != CullMode::Back,
            mat.is_mtoon(),
        );
    }

    // メタ情報
    if !ir.comment.is_empty() {
        let _ = writeln!(file);
        let _ = writeln!(file, "=== メタ情報 ===");
        let _ = writeln!(file, "{}", ir.comment.replace("\r\n", "\n"));
    }

    // 変換結果
    let _ = writeln!(file);
    let _ = writeln!(file, "=== 変換結果 ===");
    match result {
        Ok(stats) => {
            let _ = writeln!(file, "出力: {}", stats.output_path);
            let _ = writeln!(file, "テクスチャ: {}", stats.tex_dir);
            let _ = writeln!(file, "PMXボーン: {}", stats.bones);
            let _ = writeln!(file, "PMX頂点: {}", stats.vertices);
            let _ = writeln!(file, "PMX面: {}", stats.faces);
            let _ = writeln!(file, "PMX材質: {}", stats.materials);
            let _ = writeln!(file, "PMXテクスチャ: {}", stats.textures);
            let _ = writeln!(file, "PMXモーフ: {}", stats.morphs);
        }
        Err(e) => {
            let _ = writeln!(file, "変換失敗: {e}");
        }
    }

    // デバッグログ追記
    if let Some(logs) = debug_logs {
        let _ = writeln!(file);
        let _ = writeln!(file, "=== デバッグログ ===");
        let _ = write!(file, "{}", logs);
    }
}

/// アニメーション再生コントロールUI
fn show_animation_controls(ui: &mut egui::Ui, app: &mut ViewerApp) {
    use super::animation::LoopMode;

    ui.heading(egui::RichText::new("アニメーション").color(egui::Color32::from_gray(0x20)));
    ui.separator();

    // VRMAライブラリ
    if !app.anim.library.is_empty() {
        ui.label(format!(
            "アニメーションリスト ({}件):",
            app.anim.library.len()
        ));
        let mut switch_to: Option<usize> = None;
        let mut remove_idx: Option<usize> = None;
        for (i, (name, _, _)) in app.anim.library.iter().enumerate() {
            ui.horizontal(|ui| {
                let is_active = app.anim.active_index == Some(i);
                let label = if is_active {
                    format!("▶ {}", name)
                } else {
                    format!("   {}", name)
                };
                if ui.selectable_label(is_active, label).clicked() && !is_active {
                    switch_to = Some(i);
                }
                if ui.small_button("×").clicked() {
                    remove_idx = Some(i);
                }
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
        ui.label(format!(
            "{} ({:.1}秒)",
            anim.animation.name, anim.animation.duration
        ));

        ui.horizontal(|ui| {
            if ui
                .button("⏮")
                .on_hover_text("前のアニメーション / 先頭に戻す")
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
                .on_hover_text("コマ戻し");
            if step_back.clicked() {
                anim.step_frame(false);
            }
            if ui.button("◀").on_hover_text("逆再生").clicked() {
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
                .on_hover_text("コマ送り");
            if step_fwd.clicked() {
                anim.step_frame(true);
            }
            let has_next = app.anim.library.len() > 1;
            let next_btn = ui
                .add_enabled(has_next, egui::Button::new("⏭"))
                .on_hover_text("次のアニメーション");
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
            ui.label("速度");
            ui.add(
                egui::DragValue::new(&mut anim.speed)
                    .range(-3.0..=3.0)
                    .speed(0.05)
                    .fixed_decimals(1)
                    .suffix("x"),
            );
        });

        // Unity .anim の Muscle スケール調整（is_additive の場合のみ）
        if anim.animation.is_additive {
            ui.horizontal(|ui| {
                ui.label("Muscle倍率");
                let old_scale = app.anim.muscle_scale;
                let response = ui.add(
                    egui::DragValue::new(&mut app.anim.muscle_scale)
                        .range(0.01..=2.0)
                        .speed(0.01)
                        .fixed_decimals(2),
                );
                // DragValue確定時（ドラッグ解放 or Enter）のみ再読み込み
                if (app.anim.muscle_scale - old_scale).abs() > 1e-6
                    && (response.drag_stopped() || response.lost_focus())
                {
                    muscle_scale_changed = true;
                }
            });
        }

        ui.horizontal(|ui| {
            ui.label("ループ");
            egui::ComboBox::from_id_salt("loop_mode")
                .selected_text(match anim.loop_mode {
                    LoopMode::None => "なし",
                    LoopMode::Normal => "ループ",
                    LoopMode::AB => "A-B",
                    LoopMode::PingPong => "ピンポン",
                })
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut anim.loop_mode, LoopMode::None, "なし");
                    ui.selectable_value(&mut anim.loop_mode, LoopMode::Normal, "ループ");
                    ui.selectable_value(&mut anim.loop_mode, LoopMode::AB, "A-B");
                    ui.selectable_value(&mut anim.loop_mode, LoopMode::PingPong, "ピンポン");
                });
        });

        if anim.loop_mode == LoopMode::AB || anim.loop_mode == LoopMode::PingPong {
            ui.horizontal(|ui| {
                if ui
                    .small_button("𝄆")
                    .on_hover_text("リピート開始点を設定")
                    .clicked()
                {
                    anim.ab_start = Some(anim.current_time);
                }
                if ui
                    .small_button("𝄇")
                    .on_hover_text("リピート終了点を設定")
                    .clicked()
                {
                    anim.ab_end = Some(anim.current_time);
                }
                if ui.small_button("クリア").clicked() {
                    anim.ab_start = None;
                    anim.ab_end = None;
                }
            });
        }

        ui.label(format!(
            "ボーン: {}ch / 表情: {}ch",
            anim.animation.bone_channels.len(),
            anim.animation.expression_channels.len(),
        ));

        if ui.small_button("アニメーション解除").clicked() {
            app.anim.state = None;
            app.anim.active_index = None;
            app.morph_dirty = true;
        }
    } else {
        ui.label("アニメーションファイルをドロップして読み込み");
        if app.loaded.is_some() && ui.small_button("アニメーションを開く...").clicked() {
            let paths = rfd::FileDialog::new()
                .set_title("アニメーションを開く（複数選択可）")
                .add_filter("アニメーション", &["vrma", "glb", "gltf", "fbx"])
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

    // Muscle倍率変更 → .anim 再読み込み
    if muscle_scale_changed {
        if let Some(idx) = app.anim.active_index {
            let path = app.anim.library[idx].1.clone();
            if path
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("anim"))
            {
                // 現在の再生位置・状態を保存
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
        && ui.small_button("アニメーションを追加...").clicked()
    {
        let paths = rfd::FileDialog::new()
            .set_title("アニメーションを追加（複数選択可）")
            .add_filter("アニメーション", &["vrma", "glb", "gltf", "fbx"])
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

// ─── HSV カラーホイールウィジェット ───

/// Hue リング + SV 四角のカラーホイールをポップアップで表示するボタン。
/// `rgb` は linear [f32; 3]。
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

/// HSV ホイール本体: Hue リング + SV 四角
fn hsv_wheel_picker(ui: &mut egui::Ui, rgb: &mut [f32; 3]) {
    let hsv = rgb_to_hsv(*rgb);
    let mut h = hsv[0];
    let mut s = hsv[1];
    let mut v = hsv[2];

    let wheel_radius = 90.0_f32;
    let ring_width = 16.0_f32;
    let inner_radius = wheel_radius - ring_width;
    // SV 四角: 内接円に内接する正方形
    let sq_half = inner_radius * 0.65;
    let total_size = egui::vec2(wheel_radius * 2.0 + 8.0, wheel_radius * 2.0 + 8.0);

    let (rect, _) = ui.allocate_exact_size(total_size, egui::Sense::hover());
    let center = rect.center();
    let painter = ui.painter_at(rect);

    // ── Hue リング描画（三角形メッシュ） ──
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

    // Hue インジケータ（リング上の丸）
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

    // ── SV 四角描画 ──
    let sq_rect = egui::Rect::from_center_size(center, egui::vec2(sq_half * 2.0, sq_half * 2.0));
    // 4頂点のグラデーション: 左上(白), 右上(hue), 左下(黒), 右下(黒)
    // 実際にはSV空間: x=S(0→1), y=V(1→0)
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

    // SV インジケータ
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

    // ── インタラクション ──
    // Hue リングドラッグ
    let ring_id = ui.id().with("hue_ring");
    let ring_response = ui.interact(rect, ring_id, egui::Sense::click_and_drag());
    if ring_response.dragged() || ring_response.clicked() {
        if let Some(pos) = ring_response.interact_pointer_pos() {
            let dx = pos.x - center.x;
            let dy = -(pos.y - center.y);
            let dist = (dx * dx + dy * dy).sqrt();
            // リング上、またはドラッグ中なら hue を更新
            if dist >= inner_radius * 0.8 || ui.ctx().is_being_dragged(ring_id) {
                h = dy.atan2(dx) / std::f32::consts::TAU;
                if h < 0.0 {
                    h += 1.0;
                }
            }
        }
    }

    // SV 四角ドラッグ
    let sv_id = ui.id().with("sv_square");
    let sv_response = ui.interact(sq_rect, sv_id, egui::Sense::click_and_drag());
    if sv_response.dragged() || sv_response.clicked() {
        if let Some(pos) = sv_response.interact_pointer_pos() {
            s = ((pos.x - sq_rect.left()) / sq_rect.width()).clamp(0.0, 1.0);
            v = 1.0 - ((pos.y - sq_rect.top()) / sq_rect.height()).clamp(0.0, 1.0);
        }
    }

    // 値を書き戻し
    let new_rgb = hsv_to_rgb(h, s, v);
    *rgb = new_rgb;

    // 現在色プレビュー
    let preview_color = Color32::from_rgb(
        linear_to_srgb_u8(rgb[0]),
        linear_to_srgb_u8(rgb[1]),
        linear_to_srgb_u8(rgb[2]),
    );
    let preview_size = egui::vec2(total_size.x, 14.0);
    let (preview_rect, _) = ui.allocate_exact_size(preview_size, egui::Sense::hover());
    ui.painter().rect_filled(preview_rect, 2.0, preview_color);
}

// ─── 色空間変換ヘルパー ───

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
