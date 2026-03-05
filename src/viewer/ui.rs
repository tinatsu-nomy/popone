use std::path::Path;

use eframe::egui;

use super::app::ViewerApp;

pub fn show_side_panel(ctx: &egui::Context, app: &mut ViewerApp) {
    egui::SidePanel::right("info_panel")
        .default_width(300.0)
        .show(ctx, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                let Some(ref loaded) = app.loaded else {
                    ui.label("VRM ファイルを読み込んでください");
                    return;
                };

                let ir = &loaded.ir;

                // モデル情報
                ui.heading("モデル情報");
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

                    ui.label("VRM");
                    ui.label(if ir.is_vrm0 { "0.0" } else { "1.0" });
                    ui.end_row();
                });

                ui.add_space(12.0);

                // メタ情報
                if !ir.comment.is_empty() {
                    show_meta_info(ui, &ir.comment);
                    ui.add_space(8.0);
                }

                // 表情モーフスライダ
                if !ir.morphs.is_empty() {
                    ui.collapsing("表情モーフ", |ui| {
                        for (i, morph) in ir.morphs.iter().enumerate() {
                            if i < app.morph_weights.len() {
                                ui.add(
                                    egui::Slider::new(&mut app.morph_weights[i], 0.0..=1.0)
                                        .text(&morph.name),
                                );
                            }
                        }
                    });
                    ui.add_space(8.0);
                }

                // 表示設定
                ui.collapsing("表示設定", |ui| {
                    ui.add(
                        egui::Slider::new(&mut app.light_intensity, 0.0..=2.0)
                            .text("ライト"),
                    );
                    ui.add(
                        egui::Slider::new(&mut app.ambient_intensity, 0.0..=1.0)
                            .text("環境光"),
                    );
                    ui.add(
                        egui::Slider::new(&mut app.bg_brightness, 0.0..=1.0)
                            .text("背景"),
                    );
                });
                ui.add_space(8.0);

                // PMX 変換
                ui.separator();
                ui.add_space(4.0);
                ui.label("出力先:");
                ui.horizontal(|ui| {
                    ui.add(
                        egui::TextEdit::singleline(&mut app.pmx_output_path)
                            .desired_width(ui.available_width() - 40.0),
                    );
                    if ui.button("...").clicked() {
                        let default_path = std::path::PathBuf::from(&app.pmx_output_path);
                        let mut dialog = rfd::FileDialog::new()
                            .add_filter("PMX", &["pmx"]);
                        if let Some(dir) = default_path.parent() {
                            dialog = dialog.set_directory(dir);
                        }
                        if let Some(name) = default_path.file_name() {
                            dialog = dialog.set_file_name(name.to_string_lossy());
                        }
                        if let Some(path) = dialog.save_file() {
                            app.pmx_output_path = path.to_string_lossy().to_string();
                        }
                    }
                });
                ui.add_space(2.0);
                ui.horizontal(|ui| {
                    ui.checkbox(&mut app.output_log, "ログ出力");
                    if ui.button("PMX 変換").clicked() {
                        let input_path = loaded.file_path.clone();
                        let output_path = std::path::PathBuf::from(&app.pmx_output_path);
                        let log_path = output_path.with_extension("log");

                        let result =
                            crate::convert_vrm_to_pmx(&input_path, &output_path, false);

                        if app.output_log {
                            write_convert_log(
                                &log_path,
                                &loaded.ir,
                                result.as_ref(),
                            );
                        }

                        match result {
                            Ok(stats) => {
                                let mut msg = format!(
                                    "変換完了: {}\nボーン: {} / 頂点: {} / 材質: {} / モーフ: {}",
                                    stats.output_path,
                                    stats.bones,
                                    stats.vertices,
                                    stats.materials,
                                    stats.morphs,
                                );
                                if app.output_log {
                                    msg += &format!("\nログ: {}", log_path.display());
                                }
                                app.convert_message = Some(msg);
                            }
                            Err(e) => {
                                app.convert_message = Some(format!("変換失敗: {e}"));
                            }
                        }
                    }
                });

                if let Some(ref msg) = app.convert_message {
                    ui.add_space(4.0);
                    ui.label(msg);
                }
            });
        });
}

/// メタ情報をセクションごとに折り畳み可能な Grid で表示
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
            .default_open(i == 0) // 最初のセクションだけ開く
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

/// 変換ログをファイルに書き出す
fn write_convert_log(
    log_path: &Path,
    ir: &crate::intermediate::types::IrModel,
    result: Result<&crate::ConvertStats, &anyhow::Error>,
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
    let _ = writeln!(file, "日時: {}", chrono::Local::now().format("%Y-%m-%d %H:%M:%S"));
    let _ = writeln!(file, "VRMバージョン: {}", if ir.is_vrm0 { "0.0" } else { "1.0" });
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
            i, mat.name, mat.texture_index, mat.is_double_sided, mat.is_mtoon,
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
}
