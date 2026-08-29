use crate::app::SmartFileManagerApp;
use crate::ui::style::{self, muted, secondary_button};
use eframe::egui::{self, scroll_area::ScrollBarVisibility, Color32, Frame, Margin, RichText, Rounding, ScrollArea, Stroke};
use smart_file_manager::types::{ActionMode, IoProfile};

pub fn scan_panel(app: &mut SmartFileManagerApp, ui: &mut egui::Ui) {
    let body_h = ui.available_height().max(120.0);
    ScrollArea::vertical()
        .max_height(body_h)
        .scroll_bar_visibility(ScrollBarVisibility::AlwaysVisible)
        .auto_shrink([false, false])
        .id_salt("scan_panel_scroll")
        .show(ui, |ui| {
            ui.add_space(4.0);
            scan_form(app, ui);
            ui.add_space(16.0);
        });
}

fn scan_form(app: &mut SmartFileManagerApp, ui: &mut egui::Ui) {
    let is_compare = app.mode == crate::app::AppMode::CompareSourceDest;

    // Header Title & Description
    ui.horizontal(|ui| {
        let (icon, title_text) = if is_compare {
            ("⚡", "A -> B Target Comparison (Source -> Destination)")
        } else {
            ("🔍", "Duplicate File Finder")
        };

        ui.label(RichText::new(icon).size(24.0));
        ui.add_space(4.0);
        ui.label(
            RichText::new(title_text)
                .strong()
                .size(22.0)
                .color(Color32::from_rgb(240, 244, 255)),
        );
    });
    ui.add_space(4.0);

    let desc = if is_compare {
        "Compare reference files in Source (A) against Destination (B). Only duplicates in B will be flagged for removal; files in A remain untouched."
    } else {
        "Select folders to analyze and find duplicate images, videos, audio, and archives across your storage."
    };
    ui.label(RichText::new(desc).color(style::TEXT_SEC).size(13.0));
    ui.add_space(16.0);

    // Main 2-column or 3-column Layout (Left: Folder Dropzones, Right: Settings Sidebar)
    let total_width = ui.available_width();
    let settings_width = 300.0f32.min(total_width * 0.35);
    let folders_width = (total_width - settings_width - 16.0).max(280.0);

    ui.horizontal_top(|ui| {
        // Left Column: Folder Selection Area
        ui.allocate_ui(egui::vec2(folders_width, ui.available_height()), |ui| {
            ui.set_width(folders_width);
            if is_compare {
                // Two equal side-by-side cards for A and B
                ui.columns(2, |cols| {
                    cols[0].vertical(|ui| {
                        render_folder_box(
                            ui,
                            "📁 Source Folders (A — Reference)",
                            "Reference files to keep safely intact",
                            &mut app.source_roots,
                            "＋ Add Source",
                            Color32::from_rgb(34, 140, 80),
                            Color32::from_rgb(22, 50, 36),
                            "source_a_list",
                        );
                    });

                    cols[1].vertical(|ui| {
                        render_folder_box(
                            ui,
                            "🎯 Destination Folders (B — Target)",
                            "Find duplicate copies here to clean",
                            &mut app.dest_roots,
                            "＋ Add Destination",
                            Color32::from_rgb(210, 85, 55),
                            Color32::from_rgb(58, 28, 24),
                            "dest_b_list",
                        );
                    });
                });
            } else {
                render_folder_box(
                    ui,
                    "📁 Folders to Scan",
                    "Add folders to scan and find duplicate files",
                    &mut app.roots,
                    "＋ Add Folder",
                    style::ACCENT,
                    Color32::from_rgb(28, 38, 64),
                    "standard_roots_list",
                );
            }
        });

        ui.add_space(12.0);

        // Right Column: Settings & Launch Card
        ui.allocate_ui(egui::vec2(settings_width, ui.available_height()), |ui| {
            ui.set_width(settings_width);
            render_settings_panel(app, ui, is_compare);
        });
    });
}

fn render_folder_box(
    ui: &mut egui::Ui,
    title: &str,
    subtitle: &str,
    roots: &mut Vec<std::path::PathBuf>,
    add_label: &str,
    accent_btn_color: Color32,
    badge_bg: Color32,
    id_salt: &str,
) {
    let border_stroke = Stroke::new(1.0, Color32::from_rgb(46, 50, 66));
    Frame::none()
        .fill(style::SURFACE_1)
        .stroke(border_stroke)
        .rounding(Rounding::same(10.0))
        .inner_margin(Margin::same(14.0))
        .show(ui, |ui| {
            ui.vertical(|ui| {
                // Header
                ui.horizontal(|ui| {
                    ui.vertical(|ui| {
                        ui.label(RichText::new(title).strong().size(14.5).color(style::TEXT_PRI));
                        ui.add_space(1.0);
                        ui.label(RichText::new(subtitle).size(11.5).color(style::TEXT_TER));
                    });

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if !roots.is_empty() && secondary_button(ui, "Clear").clicked() {
                            roots.clear();
                        }
                    });
                });
                ui.add_space(10.0);

                // Add button bar
                ui.horizontal(|ui| {
                    let btn = egui::Button::new(
                        RichText::new(add_label)
                            .strong()
                            .size(12.5)
                            .color(Color32::from_rgb(248, 250, 255)),
                    )
                    .fill(accent_btn_color)
                    .rounding(6.0)
                    .min_size(egui::vec2(ui.available_width(), 30.0));

                    if ui.add(btn).clicked() {
                        if let Some(mut folders) = rfd::FileDialog::new().pick_folders() {
                            roots.append(&mut folders);
                        }
                    }
                });

                ui.add_space(10.0);

                // Folder entries or empty state
                let mut remove_idx = None;
                let mut swap_up_idx = None;

                if roots.is_empty() {
                    Frame::none()
                        .fill(badge_bg)
                        .stroke(Stroke::new(1.0, Color32::from_rgb(50, 56, 74)))
                        .rounding(Rounding::same(8.0))
                        .inner_margin(Margin::symmetric(14.0, 28.0))
                        .show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            ui.vertical_centered(|ui| {
                                ui.label(RichText::new("📂").size(32.0));
                                ui.add_space(6.0);
                                ui.label(
                                    RichText::new("No folders selected")
                                        .strong()
                                        .size(13.5)
                                        .color(style::TEXT_PRI),
                                );
                                ui.add_space(2.0);
                                ui.label(muted("Click above to select directories"));
                            });
                        });
                } else {
                    let list_h = 240.0;
                    ScrollArea::vertical()
                        .max_height(list_h)
                        .scroll_bar_visibility(ScrollBarVisibility::AlwaysVisible)
                        .auto_shrink([false, false])
                        .id_salt(id_salt)
                        .show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            for (i, p) in roots.iter().enumerate() {
                                Frame::none()
                                    .fill(style::SURFACE_2)
                                    .stroke(Stroke::new(1.0, Color32::from_rgb(44, 48, 62)))
                                    .rounding(Rounding::same(6.0))
                                    .inner_margin(Margin::symmetric(10.0, 8.0))
                                    .show(ui, |ui| {
                                        ui.horizontal(|ui| {
                                            let folder_name = p
                                                .file_name()
                                                .map(|n| n.to_string_lossy().to_string())
                                                .unwrap_or_else(|| p.display().to_string());

                                            ui.vertical(|ui| {
                                                ui.label(
                                                    RichText::new(&folder_name)
                                                        .strong()
                                                        .size(12.5)
                                                        .color(Color32::from_rgb(230, 235, 250)),
                                                );
                                                ui.label(
                                                    RichText::new(p.display().to_string())
                                                        .size(10.5)
                                                        .color(style::TEXT_TER),
                                                );
                                            });

                                            ui.with_layout(
                                                egui::Layout::right_to_left(egui::Align::Center),
                                                |ui| {
                                                    if ui
                                                        .button(
                                                            RichText::new("✖")
                                                                .size(11.0)
                                                                .color(style::TEXT_SEC),
                                                        )
                                                        .on_hover_text("Remove folder")
                                                        .clicked()
                                                    {
                                                        remove_idx = Some(i);
                                                    }
                                                    if i > 0 {
                                                        ui.add_space(2.0);
                                                        if ui
                                                            .button(
                                                                RichText::new("↑")
                                                                    .size(11.0)
                                                                    .color(style::TEXT_SEC),
                                                            )
                                                            .on_hover_text("Move up")
                                                            .clicked()
                                                        {
                                                            swap_up_idx = Some(i);
                                                        }
                                                    }
                                                },
                                            );
                                        });
                                    });
                                if i < roots.len() - 1 {
                                    ui.add_space(4.0);
                                }
                            }
                        });
                }

                if let Some(i) = swap_up_idx {
                    roots.swap(i, i - 1);
                }
                if let Some(i) = remove_idx {
                    roots.remove(i);
                }
            });
        });
}

fn render_settings_panel(app: &mut SmartFileManagerApp, ui: &mut egui::Ui, is_compare: bool) {
    let border_stroke = Stroke::new(1.0, Color32::from_rgb(46, 50, 66));

    Frame::none()
        .fill(style::SURFACE_1)
        .stroke(border_stroke)
        .rounding(Rounding::same(10.0))
        .inner_margin(Margin::same(14.0))
        .show(ui, |ui| {
            ui.vertical(|ui| {
                ui.label(RichText::new("⚙ Scan Settings").strong().size(14.0).color(style::TEXT_PRI));
                ui.add_space(10.0);

                ui.checkbox(&mut app.skip_system_dirs, "Skip system/tool folders")
                    .on_hover_text("Excludes .git, node_modules, $RECYCLE.BIN, cache, and build folders.");
                ui.add_space(4.0);
                ui.checkbox(&mut app.skip_hidden, "Skip hidden dotfiles (e.g. .photos)")
                    .on_hover_text("When checked, files starting with '.' are skipped.");
                ui.add_space(10.0);

                ui.horizontal(|ui| {
                    ui.label(RichText::new("Min Size:").size(12.5).color(style::TEXT_SEC));
                    ui.add(egui::DragValue::new(&mut app.min_file_size).speed(1024.0).suffix(" B"));
                });
                ui.add_space(10.0);

                ui.label(RichText::new("Action Mode:").size(12.5).color(style::TEXT_SEC));
                ui.add_space(2.0);
                egui::ComboBox::new(egui::Id::new("action_mode_scan_box"), "")
                    .width(ui.available_width())
                    .selected_text(match app.action_mode {
                        ActionMode::DryRun => "Dry run (report only)",
                        ActionMode::Delete => "Delete (move to Trash)",
                        ActionMode::HardLink => "Replace with hard links",
                    })
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut app.action_mode, ActionMode::DryRun, "Dry run (report only)");
                        ui.selectable_value(&mut app.action_mode, ActionMode::Delete, "Delete (move to Trash)");
                        ui.selectable_value(&mut app.action_mode, ActionMode::HardLink, "Replace with hard links");
                    });

                ui.add_space(14.0);

                egui::CollapsingHeader::new(
                    RichText::new("Advanced Filters")
                        .size(12.5)
                        .color(style::TEXT_SEC),
                )
                .default_open(false)
                .show(ui, |ui| {
                    ui.add_space(6.0);
                    ui.checkbox(&mut app.use_hash_cache, "Use fast hash cache");
                    ui.add_space(4.0);
                    ui.checkbox(&mut app.skip_logs_and_locks, "Skip logs/locks");
                    ui.add_space(8.0);

                    ui.label(muted("I/O Speed:"));
                    egui::ComboBox::new(egui::Id::new("io_profile_box"), "")
                        .width(ui.available_width())
                        .selected_text(match app.io_profile {
                            IoProfile::Auto => "Auto-detect",
                            IoProfile::HddSafe => "HDD Safe (sequential)",
                            IoProfile::SsdFast => "SSD Fast (parallel)",
                        })
                        .show_ui(ui, |ui| {
                            ui.selectable_value(&mut app.io_profile, IoProfile::Auto, "Auto-detect");
                            ui.selectable_value(&mut app.io_profile, IoProfile::HddSafe, "HDD Safe (sequential)");
                            ui.selectable_value(&mut app.io_profile, IoProfile::SsdFast, "SSD Fast (parallel)");
                        });
                });

                ui.add_space(18.0);
                ui.separator();
                ui.add_space(14.0);

                // Start button
                let can_start = if is_compare {
                    !app.source_roots.is_empty() && !app.dest_roots.is_empty()
                } else {
                    !app.roots.is_empty()
                };

                let btn_text = if is_compare {
                    "🚀 Start A ➔ B Comparison"
                } else {
                    "🚀 Start Duplicate Scan"
                };

                let btn = egui::Button::new(RichText::new(btn_text).strong().size(14.0))
                    .fill(if can_start {
                        Color32::from_rgb(24, 150, 85)
                    } else {
                        Color32::from_rgb(40, 42, 54)
                    })
                    .rounding(8.0)
                    .min_size(egui::vec2(ui.available_width(), 38.0));

                if ui.add_enabled(can_start, btn).clicked() {
                    app.start_scan();
                }

                ui.add_space(6.0);
                if !can_start {
                    let warn = if is_compare {
                        "Select at least 1 Source and 1 Destination folder."
                    } else {
                        "Select at least 1 folder to scan."
                    };
                    ui.label(RichText::new(warn).size(11.0).color(style::WARNING));
                } else {
                    ui.label(muted("Scans media files (photos, videos, audio, zip)."));
                }
            });
        });
}

