use crate::app::{SmartFileManagerApp, AppView};
use crate::ui::style::{self, primary_button, secondary_button, danger_button, muted};
use eframe::egui::{self, scroll_area::ScrollBarVisibility, ScrollArea, RichText};

pub fn empty_folders_scan_panel(app: &mut SmartFileManagerApp, ui: &mut egui::Ui) {
    let body_h = ui.available_height().max(120.0);
    ScrollArea::vertical()
        .max_height(body_h)
        .scroll_bar_visibility(ScrollBarVisibility::AlwaysVisible)
        .auto_shrink([false, false])
        .id_salt("empty_folders_scan_panel_main")
        .show(ui, |ui| {
            ui.vertical(|ui| {
                ui.set_max_width(700.0);
                
    ui.label(
        RichText::new("Find Empty Folders")
            .strong()
            .size(26.0)
            .color(egui::Color32::from_rgb(240, 243, 255)),
    );
    ui.add_space(4.0);
    ui.label(muted("Find and remove empty directories and subdirectories recursively."));
    ui.add_space(20.0);

    let border_stroke = egui::Stroke::new(1.0, egui::Color32::from_rgb(58, 62, 78));

    egui::SidePanel::right("empty_folders_action_panel")
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
                                app.start_empty_folders_scan();
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
                                ui.label(RichText::new("Click below to select folders to scan for empty directories.").size(14.0).color(style::TEXT_SEC));
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
                                .id_salt("empty_folders_roots_list")
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
                                                        .id_salt(("empty_folders_root_path", i))
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

pub fn empty_folders_progress_panel(app: &mut SmartFileManagerApp, ui: &mut egui::Ui, ctx: &egui::Context) {
    let count = *app.empty_folders_progress_count.lock().unwrap();
    let current_path = app.empty_folders_progress_path.lock().unwrap().clone();

    ui.vertical_centered(|ui| {
        ui.add_space(40.0);
        ui.add(egui::Spinner::new().size(40.0));
        ui.add_space(16.0);
        ui.label(
            RichText::new("Scanning for Empty Folders...")
                .strong()
                .size(16.0)
                .color(style::TEXT_PRI),
        );
        ui.add_space(8.0);
        ui.label(muted(format!("Found {} empty folders so far", count)));
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
        if danger_button(ui, "🛑 Cancel Scan").clicked() {
            app.token.cancel();
        }
    });
    ctx.request_repaint_after(std::time::Duration::from_millis(100));
}

pub fn empty_folders_results_panel(app: &mut SmartFileManagerApp, ui: &mut egui::Ui) {
    ui.horizontal(|ui| {
        ui.vertical(|ui| {
            ui.label(
                RichText::new("Empty Folders Found")
                    .strong()
                    .size(22.0)
                    .color(egui::Color32::from_rgb(240, 243, 255)),
            );
            ui.add_space(2.0);
            ui.label(muted(format!("Found {} empty folders eligible for deletion.", app.empty_folders_results.len())));
        });
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if secondary_button(ui, "Back").clicked() {
                app.view = AppView::Scan;
            }
            if !app.empty_folders_results.is_empty() {
                ui.add_space(8.0);
                let selected_count = app.empty_folders_selected.len();
                let delete_label = if selected_count == 0 {
                    "🗑 Delete Selected".to_string()
                } else {
                    format!("🗑 Delete Selected ({})", selected_count)
                };
                
                if danger_button(ui, &delete_label).clicked() && selected_count > 0 {
                    app.delete_selected_empty_folders();
                }
            }
        });
    });

    ui.add_space(14.0);

    let border_stroke = egui::Stroke::new(1.0, egui::Color32::from_rgb(58, 62, 78));
    egui::Frame::none()
        .fill(style::SURFACE_1)
        .stroke(border_stroke)
        .rounding(egui::Rounding::same(12.0))
        .inner_margin(egui::Margin::same(16.0))
        .show(ui, |ui| {
            ScrollArea::vertical()
                .auto_shrink([false, false])
                .scroll_bar_visibility(ScrollBarVisibility::AlwaysVisible)
                .show(ui, |ui| {
                    if app.empty_folders_results.is_empty() {
                        ui.centered_and_justified(|ui| {
                            ui.label(RichText::new("No empty folders found in the scanned directories.")
                                .color(style::TEXT_TER)
                                .size(14.0));
                        });
                    } else {
                        ui.horizontal(|ui| {
                            if ui.button("Select All").clicked() {
                                app.empty_folders_selected = app.empty_folders_results.iter().cloned().collect();
                            }
                            if ui.button("Deselect All").clicked() {
                                app.empty_folders_selected.clear();
                            }
                        });
                        ui.add_space(8.0);
                        
                        for (i, p) in app.empty_folders_results.clone().iter().enumerate() {
                            ui.horizontal(|ui| {
                                let mut is_selected = app.empty_folders_selected.contains(p);
                                if ui.checkbox(&mut is_selected, "").changed() {
                                    if is_selected {
                                        app.empty_folders_selected.insert(p.clone());
                                    } else {
                                        app.empty_folders_selected.remove(p);
                                    }
                                }
                                
                                ui.label(
                                    RichText::new(format!("{}.", i + 1))
                                        .strong()
                                        .color(egui::Color32::from_rgb(110, 140, 240)),
                                );
                                ui.label(style::path_monospace(p));
                                
                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                    if style::secondary_button(ui, "📂 Open").clicked() {
                                        crate::ui::preview::open_in_default_app(p);
                                    }
                                });
                            });
                            if i < app.empty_folders_results.len() - 1 {
                                ui.add_space(4.0);
                                ui.separator();
                                ui.add_space(4.0);
                            }
                        }
                    }
                });
        });
}
