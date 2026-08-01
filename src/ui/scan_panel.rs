use crate::app::SmartFileManagerApp;
use crate::ui::style::{self, primary_button, secondary_button, muted};
use eframe::egui::{self, scroll_area::ScrollBarVisibility, ScrollArea, RichText};
use smart_file_manager::types::{ActionMode, IoProfile};

pub fn scan_panel(app: &mut SmartFileManagerApp, ui: &mut egui::Ui) {
    let body_h = ui.available_height().max(120.0);
    ScrollArea::vertical()
        .max_height(body_h)
        .scroll_bar_visibility(ScrollBarVisibility::AlwaysVisible)
        .auto_shrink([false, false])
        .id_salt("scan_panel_main")
        .show(ui, |ui| {
            ui.vertical(|ui| {
                ui.set_max_width(700.0);
                scan_form(app, ui);
            });
        });
}

fn scan_form(app: &mut SmartFileManagerApp, ui: &mut egui::Ui) {
    ui.label(
        RichText::new("Find Duplicate Files")
            .strong()
            .size(26.0)
            .color(egui::Color32::from_rgb(240, 243, 255)),
    );
    ui.add_space(4.0);
    ui.label(muted("Select folders and refine filters to search for duplicates."));
    ui.add_space(16.0);

    // Getting Started Info
    ui.horizontal(|ui| {
        ui.label(RichText::new("1.").strong().color(style::TEXT_PRI));
        ui.label(muted("Add folders."));
        ui.add_space(12.0);
        ui.label(RichText::new("2.").strong().color(style::TEXT_PRI));
        ui.label(muted("Set keep order."));
        ui.add_space(12.0);
        ui.label(RichText::new("3.").strong().color(style::TEXT_PRI));
        ui.label(muted("Review & apply."));
    });
    ui.add_space(20.0);

    let border_stroke = egui::Stroke::new(1.0, egui::Color32::from_rgb(58, 62, 78));

    egui::SidePanel::right("scan_settings_panel")
        .resizable(false)
        .exact_width(340.0)
        .frame(egui::Frame::none().inner_margin(egui::Margin { left: 16.0, right: 16.0, top: 0.0, bottom: 0.0 }))
        .show_inside(ui, |ui| {
            ui.vertical(|ui| {
                // Basic Settings Card
                egui::Frame::none()
                    .fill(style::SURFACE_1)
                    .stroke(border_stroke)
                    .rounding(egui::Rounding::same(12.0))
                    .inner_margin(egui::Margin::same(16.0))
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.horizontal(|ui| {
                            ui.label(RichText::new("⚙ Basic Settings").strong().size(14.0).color(style::TEXT_PRI));
                        });
                        ui.add_space(16.0);
        
                        ui.vertical(|ui| {
                            ui.checkbox(&mut app.skip_system_dirs, "Skip system/dev folders");
                            ui.add_space(6.0);
                            ui.checkbox(&mut app.skip_hidden, "Skip hidden files");
                        });
                        ui.add_space(16.0);
        
                        ui.vertical(|ui| {
                            ui.horizontal(|ui| {
                                ui.label(RichText::new("Min Size:").color(style::TEXT_SEC));
                                ui.add(egui::DragValue::new(&mut app.min_file_size).speed(1024.0).suffix(" B"));
                            });
                                
                                ui.add_space(12.0);
                                
                                ui.label(RichText::new("Action Mode:").color(style::TEXT_SEC));
                                egui::ComboBox::new(egui::Id::new("action_mode_scan"), "")
                                    .width(280.0)
                                    .selected_text(match app.action_mode {
                                        ActionMode::DryRun => "Dry run (report)",
                                        ActionMode::Delete => "Delete (move to Trash)",
                                        ActionMode::HardLink => "Replace with hard links",
                                    })
                                    .show_ui(ui, |ui| {
                                        ui.selectable_value(&mut app.action_mode, ActionMode::DryRun, "Dry run (report)");
                                        ui.selectable_value(&mut app.action_mode, ActionMode::Delete, "Delete (move to Trash)");
                                        ui.selectable_value(&mut app.action_mode, ActionMode::HardLink, "Replace with hard links");
                                    });
                            });
                    });

                ui.add_space(16.0);

                // Advanced Filters Card
                egui::Frame::none()
                    .fill(style::SURFACE_1)
                    .stroke(border_stroke)
                    .rounding(egui::Rounding::same(12.0))
                    .inner_margin(egui::Margin::same(16.0))
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        egui::CollapsingHeader::new(RichText::new("Advanced Filters & Perf").strong().size(14.0).color(style::TEXT_PRI))
                            .default_open(false)
                            .show(ui, |ui| {
                                ui.add_space(10.0);
                                ui.vertical(|ui| {
                                    ui.checkbox(&mut app.use_hash_cache, "Use fast hash cache");
                                    ui.add_space(6.0);
                                    ui.checkbox(&mut app.skip_logs_and_locks, "Skip log & lock files");
                                });
                                ui.add_space(12.0);
            
                                ui.vertical(|ui| {
                                    ui.label(RichText::new("I/O Profile:").color(style::TEXT_SEC));
                                    egui::ComboBox::new(egui::Id::new("io_profile_scan"), "")
                                        .width(280.0)
                                        .selected_text(match app.io_profile {
                                            IoProfile::Auto => "Auto detect speed",
                                            IoProfile::HddSafe => "HDD-safe (sequential)",
                                            IoProfile::SsdFast => "SSD / fast parallel",
                                        })
                                        .show_ui(ui, |ui| {
                                            ui.selectable_value(&mut app.io_profile, IoProfile::Auto, "Auto detect speed");
                                            ui.selectable_value(&mut app.io_profile, IoProfile::HddSafe, "HDD-safe (sequential)");
                                            ui.selectable_value(&mut app.io_profile, IoProfile::SsdFast, "SSD / fast parallel");
                                        });
                                });
                                ui.add_space(16.0);
            
                                ui.vertical(|ui| {
                                    ui.label(muted("Preferred paths"));
                                    ui.add(
                                        egui::TextEdit::multiline(&mut app.preferred_dirs_text)
                                            .desired_rows(2)
                                            .desired_width(280.0),
                                    );
                                    ui.add_space(8.0);
                                    ui.label(muted("Glob excludes (e.g. *.tmp)"));
                                    ui.add(
                                        egui::TextEdit::multiline(&mut app.exclude_globs_text)
                                            .desired_rows(2)
                                            .desired_width(280.0),
                                    );
                                    ui.add_space(8.0);
                                    ui.label(muted("Subdirs to exclude"));
                                    ui.add(
                                        egui::TextEdit::multiline(&mut app.excluded_subdirs_text)
                                            .desired_rows(2)
                                            .desired_width(280.0),
                                    );
                                });
                            });
                    });

                ui.add_space(32.0);

                let can_start = !app.roots.is_empty();
                ui.vertical_centered(|ui| {
                    ui.add_enabled_ui(can_start, |ui| {
                        let btn = egui::Button::new(RichText::new("🚀 Start Scan").strong().size(15.0))
                            .fill(if can_start { egui::Color32::from_rgb(20, 160, 90) } else { style::SURFACE_2 })
                            .rounding(6.0)
                            .min_size(egui::vec2(140.0, 38.0));
                        if ui.add(btn).clicked() {
                            app.start_scan();
                        }
                    });
                    if !can_start {
                        ui.add_space(8.0);
                        ui.label(muted("Add at least one folder to scan."));
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
                                ui.label(RichText::new("Click below to select folders to scan for duplicates.").size(14.0).color(style::TEXT_SEC));
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
                                .id_salt("roots_list")
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
                                                        .id_salt(("root_path", i))
                                                        .show(ui, |ui| {
                                                            ui.label(crate::ui::style::path_monospace(p).size(14.0));
                                                        });
                                                    
                                                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                                        if ui.button(RichText::new("✖").color(style::TEXT_SEC)).on_hover_text("Remove").clicked() {
                                                            remove_idx = Some(i);
                                                        }
                                                        if i > 0 {
                                                            ui.add_space(6.0);
                                                            if ui.button(RichText::new("↑").color(style::TEXT_SEC)).on_hover_text("Move up (highest keep preference)").clicked() {
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
}

