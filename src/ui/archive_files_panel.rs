use crate::app::{SmartFileManagerApp, AppView};
use crate::ui::style::{self, primary_button, secondary_button, muted};
use eframe::egui::{self, scroll_area::ScrollBarVisibility, ScrollArea, RichText};

pub fn archive_files_scan_panel(app: &mut SmartFileManagerApp, ui: &mut egui::Ui) {
    let body_h = ui.available_height().max(120.0);
    ScrollArea::vertical()
        .max_height(body_h)
        .scroll_bar_visibility(ScrollBarVisibility::AlwaysVisible)
        .auto_shrink([false, false])
        .id_salt("archive_files_scan_panel_main")
        .show(ui, |ui| {
            ui.vertical(|ui| {
                ui.set_max_width(700.0);
                
    ui.label(
        RichText::new("Find Archive Files")
            .strong()
            .size(26.0)
            .color(egui::Color32::from_rgb(240, 243, 255)),
    );
    ui.add_space(4.0);
    ui.label(muted("Find and collect archive files (.zip, .rar, .7z, etc.) recursively from specified folders."));
    ui.add_space(20.0);

    let border_stroke = egui::Stroke::new(1.0, egui::Color32::from_rgb(58, 62, 78));

    egui::SidePanel::right("archive_files_action_panel")
        .resizable(false)
        .exact_width(280.0)
        .frame(egui::Frame::none().inner_margin(egui::Margin { left: 16.0, right: 16.0, top: 0.0, bottom: 0.0 }))
        .show_inside(ui, |ui| {
            egui::Frame::none()
                .fill(style::SURFACE_1)
                .stroke(border_stroke)
                .rounding(egui::Rounding::same(12.0))
                .inner_margin(egui::Margin::same(24.0))
                .show(ui, |ui| {
                    ui.vertical_centered(|ui| {
                        ui.label(RichText::new("⚡ Action Panel").strong().size(16.0).color(style::TEXT_PRI));
                        ui.add_space(20.0);
                        ui.label(RichText::new(format!("{} folder(s) selected", app.roots.len())).size(15.0).color(style::TEXT_SEC));
                        ui.add_space(32.0);
                        
                        let can_start = !app.roots.is_empty();
                        ui.add_enabled_ui(can_start, |ui| {
                            let btn = egui::Button::new(RichText::new("🚀 Start Scan").strong().size(15.0))
                                .fill(if can_start { egui::Color32::from_rgb(20, 160, 90) } else { style::SURFACE_2 })
                                .rounding(6.0)
                                .min_size(egui::vec2(140.0, 38.0));
                            if ui.add(btn).clicked() {
                                app.start_archive_files_scan();
                            }
                        });
                        if !can_start {
                            ui.add_space(12.0);
                            ui.label(muted("Add folders first."));
                        }
                    });
                });
        });

    egui::CentralPanel::default()
        .frame(egui::Frame::none().inner_margin(egui::Margin { left: 0.0, right: 16.0, top: 0.0, bottom: 0.0 }))
        .show_inside(ui, |ui| {
            // Folders to Scan Card
            egui::Frame::none()
                .fill(style::SURFACE_1)
                .stroke(border_stroke)
                .rounding(egui::Rounding::same(12.0))
                .inner_margin(egui::Margin::same(20.0))
                .show(ui, |ui| {
                    ui.vertical(|ui| {
                        ui.horizontal(|ui| {
                            ui.label(RichText::new("Folders to Scan").strong().size(16.0).color(style::TEXT_PRI));
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                if !app.roots.is_empty() {
                                    if secondary_button(ui, "Clear All").clicked() {
                                        app.roots.clear();
                                    }
                                    ui.add_space(8.0);
                                    if primary_button(ui, "＋ Add Folder").clicked() {
                                        if let Some(mut folders) = rfd::FileDialog::new().pick_folders() {
                                            app.roots.append(&mut folders);
                                        }
                                    }
                                }
                            });
                        });
                        ui.add_space(16.0);
        
                        let mut remove_idx = None;
                        let mut swap_up_idx = None;
                        if app.roots.is_empty() {
                            ui.add_space(40.0);
                            ui.vertical_centered(|ui| {
                                ui.label(RichText::new("📂").size(72.0));
                                ui.add_space(16.0);
                                ui.label(RichText::new("No folders selected").strong().size(20.0).color(style::TEXT_PRI));
                                ui.add_space(8.0);
                                ui.label(RichText::new("Click below to select folders to scan for archive files.").size(14.0).color(style::TEXT_SEC));
                                ui.add_space(32.0);
                                let btn = egui::Button::new(RichText::new("＋ Add Folder").strong().size(15.0))
                                    .fill(egui::Color32::from_rgb(60, 100, 250))
                                    .rounding(6.0)
                                    .min_size(egui::vec2(140.0, 38.0));
                                if ui.add(btn).clicked() {
                                    if let Some(mut folders) = rfd::FileDialog::new().pick_folders() {
                                        app.roots.append(&mut folders);
                                    }
                                }
                            });
                            ui.add_space(40.0);
                        } else {
                            ScrollArea::vertical()
                                .max_height(ui.available_height() - 20.0)
                                .scroll_bar_visibility(ScrollBarVisibility::AlwaysVisible)
                                .auto_shrink([false, false])
                                .id_salt("archive_files_roots_list")
                                .show(ui, |ui| {
                                    for (i, p) in app.roots.iter().enumerate() {
                                        egui::Frame::none()
                                            .fill(style::SURFACE_2)
                                            .rounding(egui::Rounding::same(8.0))
                                            .inner_margin(egui::Margin::same(12.0))
                                            .show(ui, |ui| {
                                                ui.horizontal(|ui| {
                                                    ui.label(RichText::new("📁").size(16.0));
                                                    ui.add_space(6.0);
                                                    let slot = (ui.available_width() - 90.0).max(60.0);
                                                    ScrollArea::horizontal()
                                                        .max_width(slot)
                                                        .min_scrolled_height(20.0)
                                                        .scroll_bar_visibility(ScrollBarVisibility::AlwaysHidden)
                                                        .id_salt(("archive_files_root_path", i))
                                                        .show(ui, |ui| {
                                                            ui.label(style::path_monospace(p).size(14.0));
                                                        });
                                                    
                                                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                                        if ui.button(RichText::new("✖").color(style::TEXT_SEC)).on_hover_text("Remove").clicked() {
                                                            remove_idx = Some(i);
                                                        }
                                                        if i > 0 {
                                                            ui.add_space(6.0);
                                                            if ui.button(RichText::new("↑").color(style::TEXT_SEC)).on_hover_text("Move up").clicked() {
                                                                swap_up_idx = Some(i);
                                                            }
                                                        }
                                                    });
                                                });
                                            });
                                        if i < app.roots.len() - 1 {
                                            ui.add_space(8.0);
                                        }
                                    }
                                });
                        }
        
                        if let Some(i) = swap_up_idx {
                            app.roots.swap(i, i - 1);
                        }
                        if let Some(i) = remove_idx {
                            app.roots.remove(i);
                        }
                    });
                });
        });
            });
        });
}

pub fn archive_files_progress_panel(app: &mut SmartFileManagerApp, ui: &mut egui::Ui, ctx: &egui::Context) {
    let count = *app.archive_files_progress_count.lock().unwrap();
    let current_path = app.archive_files_progress_path.lock().unwrap().clone();

    ui.vertical_centered(|ui| {
        ui.add_space(40.0);
        ui.add(egui::Spinner::new().size(40.0));
        ui.add_space(16.0);
        ui.label(
            RichText::new("Scanning for Archive Files...")
                .strong()
                .size(16.0)
                .color(style::TEXT_PRI),
        );
        ui.add_space(8.0);
        ui.label(muted(format!("Found {} archives so far", count)));
        if !current_path.as_os_str().is_empty() {
            ui.add_space(12.0);
            let display_path = if current_path.to_string_lossy().len() > 60 {
                let s = current_path.to_string_lossy();
                format!("...{}", &s[s.len() - 57..])
            } else {
                current_path.to_string_lossy().into_owned()
            };
            ui.label(RichText::new(display_path).font(egui::FontId::monospace(11.0)).color(style::TEXT_SEC));
        }

        ui.add_space(24.0);
        if ui.button(RichText::new("🛑 Cancel Scan").strong().color(egui::Color32::from_rgb(255, 200, 200))).clicked() {
            app.token.cancel();
        }
    });
    ctx.request_repaint_after(std::time::Duration::from_millis(100));
}

pub fn archive_files_results_panel(app: &mut SmartFileManagerApp, ui: &mut egui::Ui) {
    ui.horizontal(|ui| {
        ui.vertical(|ui| {
            ui.label(
                RichText::new("Archive Files Found")
                    .strong()
                    .size(22.0)
                    .color(egui::Color32::from_rgb(240, 243, 255)),
            );
            ui.add_space(2.0);
            ui.label(muted(format!("Found {} archive files.", app.archive_files_results.len())));
        });
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if secondary_button(ui, "Back").clicked() {
                app.view = AppView::Scan;
            }
        });
    });

    ui.add_space(14.0);

    // Target Folder Picker Card
    let border_stroke = egui::Stroke::new(1.0, egui::Color32::from_rgb(58, 62, 78));
    egui::Frame::none()
        .fill(style::SURFACE_1)
        .stroke(border_stroke)
        .rounding(egui::Rounding::same(10.0))
        .inner_margin(egui::Margin::same(12.0))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new("Move Target Directory:").strong().color(style::TEXT_PRI));
                if let Some(target) = &app.archive_files_move_target {
                    ScrollArea::horizontal()
                        .max_width(ui.available_width() - 140.0)
                        .scroll_bar_visibility(ScrollBarVisibility::AlwaysHidden)
                        .id_salt("archive_move_target_display")
                        .show(ui, |ui| {
                            ui.label(style::path_monospace(target));
                        });
                } else {
                    ui.label(RichText::new("No folder selected").italics().color(style::TEXT_TER));
                }

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if primary_button(ui, "📁 Choose...").clicked() {
                        if let Some(folder) = rfd::FileDialog::new().pick_folder() {
                            app.archive_files_move_target = Some(folder);
                        }
                    }
                });
            });
        });

    ui.add_space(10.0);

    // Bulk Actions & Header Info
    ui.horizontal(|ui| {
        let total_count = app.archive_files_results.len();
        let selected_count = app.archive_files_selected.len();
        
        let mut all_checked = selected_count == total_count && total_count > 0;
        if ui.checkbox(&mut all_checked, "Select All").clicked() {
            if all_checked {
                app.archive_files_selected = app.archive_files_results.iter().cloned().collect();
            } else {
                app.archive_files_selected.clear();
            }
        }

        ui.label(muted(format!("({} of {} selected)", selected_count, total_count)));

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if app.archive_files_moving {
                let (done, total, fname) = app.archive_files_move_progress.lock()
                    .map(|g| g.clone())
                    .unwrap_or((0, 0, String::new()));
                ui.vertical(|ui| {
                    ui.add_space(2.0);
                    ui.horizontal(|ui| {
                        ui.add(egui::Spinner::new().size(14.0));
                        ui.add_space(6.0);
                        ui.label(RichText::new(format!("Moving {}/{}", done + 1, total.max(1)))
                            .color(style::TEXT_SEC).size(12.0));
                    });
                    if !fname.is_empty() {
                        let truncated = if fname.len() > 40 {
                            format!("…{}", &fname[fname.len().saturating_sub(37)..])
                        } else {
                            fname.clone()
                        };
                        ui.label(muted(truncated));
                    }
                    let fraction = if total > 0 { done as f32 / total as f32 } else { 0.0 };
                    ui.add(egui::ProgressBar::new(fraction)
                        .desired_width(180.0)
                        .animate(true));
                });
            } else {
                let can_move = app.archive_files_move_target.is_some() && !app.archive_files_selected.is_empty();
                ui.add_enabled_ui(can_move, |ui| {
                    if primary_button(ui, "📦 Move Selected Archives").clicked() {
                        app.execute_move_archive_files();
                    }
                });
            }
        });
    });

    ui.add_space(10.0);

    // Results Scroll Area
    egui::Frame::none()
        .fill(style::SURFACE_1)
        .stroke(border_stroke)
        .rounding(egui::Rounding::same(12.0))
        .inner_margin(egui::Margin::same(16.0))
        .show(ui, |ui| {
            let list_h = ui.available_height().max(100.0);
            ScrollArea::vertical()
                .auto_shrink([false, false])
                .max_height(list_h)
                .scroll_bar_visibility(ScrollBarVisibility::AlwaysVisible)
                .id_salt("archive_files_results_list")
                .show(ui, |ui| {
                    if app.archive_files_results.is_empty() {
                        ui.centered_and_justified(|ui| {
                            ui.label(RichText::new("No archive files found in the scanned directories.")
                                .color(style::TEXT_TER)
                                .size(14.0));
                        });
                    } else {
                        for (i, p) in app.archive_files_results.iter().enumerate() {
                            ui.horizontal(|ui| {
                                let mut checked = app.archive_files_selected.contains(p);
                                if ui.checkbox(&mut checked, "").clicked() {
                                    if checked {
                                        app.archive_files_selected.insert(p.clone());
                                    } else {
                                        app.archive_files_selected.remove(p);
                                    }
                                }

                                ui.label(
                                    RichText::new(format!("{}.", i + 1))
                                        .strong()
                                        .color(egui::Color32::from_rgb(110, 140, 240)),
                                );
                                ui.label(style::path_monospace(p));
                            });
                            if i < app.archive_files_results.len() - 1 {
                                ui.add_space(4.0);
                                ui.separator();
                                ui.add_space(4.0);
                            }
                        }
                    }
                });
        });
}
