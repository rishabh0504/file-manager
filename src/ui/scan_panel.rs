use crate::app::SortwiseApp;
use crate::ui::style::{muted, secondary_button};
use eframe::egui::{self, scroll_area::ScrollBarVisibility, Color32, Frame, Margin, RichText, Rounding, ScrollArea, Stroke};
use sortwise::types::{ActionMode, IoProfile};

/// Shared scan-screen scroll wrapper. `body` gets the height the two columns should fill:
/// the window below the title block (about 130 px), with a floor for short windows.
pub fn scan_scroll(ui: &mut egui::Ui, id: &'static str, body: impl FnOnce(&mut egui::Ui, f32)) {
    let body_h = ui.available_height().max(120.0);
    let col_h = (body_h - 130.0).max(260.0);
    ScrollArea::vertical()
        .max_height(body_h)
        .scroll_bar_visibility(ScrollBarVisibility::VisibleWhenNeeded)
        .auto_shrink([false, false])
        .id_salt(id)
        .show(ui, |ui| {
            ui.add_space(4.0);
            body(ui, col_h);
            ui.add_space(16.0);
        });
}

/// One-line intro shared by every scan screen. The page title already sits in the top bar,
/// so this only adds the icon, a short name and what the tool does.
pub fn scan_header(ui: &mut egui::Ui, icon: &str, title: &str, desc: &str) {
    let p = crate::ui::style::pal();
    ui.horizontal_wrapped(|ui| {
        ui.label(RichText::new(icon).size(16.0).color(p.accent));
        ui.label(RichText::new(title).strong().size(14.0).color(p.text));
        ui.label(RichText::new(desc).color(p.text_muted).size(12.5));
    });
    ui.add_space(12.0);
}

/// Width below which the folders and settings cards stack vertically.
const STACK_W: f32 = 720.0;

/// Folders on the left, launch card on the right. Below `STACK_W` they stack, settings last.
/// Both closures get the app, so they run one after the other (no double mutable borrow).
pub fn scan_columns(
    app: &mut SortwiseApp,
    ui: &mut egui::Ui,
    col_h: f32,
    left: impl FnOnce(&mut SortwiseApp, &mut egui::Ui),
    right: impl FnOnce(&mut SortwiseApp, &mut egui::Ui),
) {
    let total_width = ui.available_width();
    let gap = 16.0;
    if total_width < STACK_W {
        ui.allocate_ui(egui::vec2(total_width, 300.0), |ui| {
            ui.set_width(total_width);
            left(app, ui);
        });
        ui.add_space(gap);
        ui.allocate_ui(egui::vec2(total_width, 420.0), |ui| {
            ui.set_width(total_width);
            right(app, ui);
        });
        return;
    }
    // Cards add 32 px of frame padding outside their content width.
    let right_width = (total_width * 0.32).clamp(280.0, 360.0) - 32.0;
    let left_width = total_width - right_width - 32.0 - gap - 32.0;
    let col_h = col_h.min(540.0);

    ui.horizontal_top(|ui| {
        ui.allocate_ui(egui::vec2(left_width, col_h), |ui| {
            ui.set_width(left_width);
            left(app, ui);
        });
        ui.add_space(gap);
        ui.allocate_ui(egui::vec2(right_width, col_h), |ui| {
            ui.set_width(right_width);
            right(app, ui);
        });
    });
}

pub fn scan_panel(app: &mut SortwiseApp, ui: &mut egui::Ui) {
    scan_scroll(ui, "scan_panel_scroll", |ui, col_h| scan_form(app, ui, col_h));
}

fn scan_form(app: &mut SortwiseApp, ui: &mut egui::Ui, col_h: f32) {
    let is_compare = app.mode == crate::app::AppMode::CompareSourceDest;

    if is_compare {
        scan_header(
            ui,
            "\u{E0A0}",
            "Compare A to B",
            "Finds copies in B that also exist in A. Trash applies to B only.",
        );
    } else {
        scan_header(
            ui,
            "\u{E30C}",
            "Duplicate File Finder",
            "Select folders to analyze and find duplicate images, videos, audio, and archives across your storage.",
        );
    }

    scan_columns(
        app,
        ui,
        col_h,
        |app, ui| {
            if is_compare {
                // Two equal side-by-side cards for A and B
                ui.columns(2, |cols| {
                    cols[0].vertical(|ui| {
                        render_folder_box(
                            ui,
                            "\u{E24A} Source (A)",
                            "Reference. Never changed.",
                            &mut app.source_roots,
                            "+ Add Source",
                            crate::ui::style::pal().accent,
                            crate::ui::style::pal().success_soft,
                            "source_a_list",
                        );
                    });

                    cols[1].vertical(|ui| {
                        render_folder_box(
                            ui,
                            "\u{E1D6} Destination (B)",
                            "Copies found here can be trashed.",
                            &mut app.dest_roots,
                            "+ Add Destination",
                            crate::ui::style::pal().accent,
                            crate::ui::style::pal().danger_soft,
                            "dest_b_list",
                        );
                    });
                });
            } else {
                render_folder_box(
                    ui,
                    "\u{E24A} Folders to Scan",
                    "Add folders to scan and find duplicate files",
                    &mut app.roots,
                    "+ Add Folder",
                    crate::ui::style::pal().accent,
                    crate::ui::style::pal().accent_soft,
                    "standard_roots_list",
                );
            }
        },
        |app, ui| render_settings_panel(app, ui, is_compare),
    );
}

pub(crate) fn render_folder_box(
    ui: &mut egui::Ui,
    title: &str,
    subtitle: &str,
    roots: &mut Vec<std::path::PathBuf>,
    add_label: &str,
    accent_btn_color: Color32,
    badge_bg: Color32,
    id_salt: &str,
) {
    let p = crate::ui::style::pal();
    let _ = badge_bg;
    Frame::none()
        .fill(p.surface)
        .stroke(Stroke::new(1.0, p.border))
        .rounding(Rounding::same(crate::ui::style::R_MD))
        .inner_margin(Margin::same(16.0))
        .show(ui, |ui| {
            ui.set_min_height(ui.available_height());
            ui.vertical(|ui| {
                // Header: title + count on the left, Clear and Add on the right (own row when narrow).
                let stacked = ui.available_width() < 520.0;
                ui.horizontal(|ui| {
                    ui.vertical(|ui| {
                        ui.spacing_mut().item_spacing.y = 2.0;
                        ui.horizontal(|ui| {
                            ui.label(crate::ui::style::section_title(title));
                            if !roots.is_empty() {
                                crate::ui::style::pill(ui, &roots.len().to_string(), p.text_muted, p.surface_raised);
                            }
                        });
                        ui.label(RichText::new(subtitle).size(12.0).color(p.text_muted));
                    });

                    if stacked {
                        return;
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let btn = egui::Button::new(
                            RichText::new(add_label).strong().color(p.on_accent),
                        )
                        .fill(accent_btn_color)
                        .stroke(Stroke::NONE)
                        .rounding(Rounding::same(crate::ui::style::R_SM))
                        .min_size(egui::vec2(0.0, crate::ui::style::H_CONTROL));
                        if ui.add(btn).clicked() {
                            if let Some(mut folders) = rfd::FileDialog::new().pick_folders() {
                                roots.append(&mut folders);
                            }
                        }
                        if !roots.is_empty() && secondary_button(ui, "Clear").clicked() {
                            roots.clear();
                        }
                    });
                });
                if stacked {
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        let btn = egui::Button::new(RichText::new(add_label).strong().color(p.on_accent))
                            .fill(accent_btn_color)
                            .stroke(Stroke::NONE)
                            .rounding(Rounding::same(crate::ui::style::R_SM))
                            .min_size(egui::vec2(0.0, crate::ui::style::H_CONTROL));
                        if ui.add(btn).clicked() {
                            if let Some(mut folders) = rfd::FileDialog::new().pick_folders() {
                                roots.append(&mut folders);
                            }
                        }
                        if !roots.is_empty() && secondary_button(ui, "Clear").clicked() {
                            roots.clear();
                        }
                    });
                }
                ui.add_space(12.0);

                // Folder entries or empty state
                let mut remove_idx = None;
                let mut swap_up_idx = None;

                if roots.is_empty() {
                    Frame::none()
                        .fill(p.surface_raised)
                        .stroke(Stroke::new(1.0, p.border))
                        .rounding(Rounding::same(crate::ui::style::R_MD))
                        .inner_margin(Margin::symmetric(14.0, 32.0))
                        .show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            ui.vertical_centered(|ui| {
                                ui.label(RichText::new("\u{E256}").size(28.0).color(p.text_muted));
                                ui.add_space(6.0);
                                ui.label(RichText::new("No folders yet").strong().size(13.5).color(p.text));
                                ui.add_space(2.0);
                                ui.label(muted(format!("Use “{}” to choose folders", add_label.trim_start_matches("+ "))));
                            });
                        });
                } else {
                    ScrollArea::vertical()
                        .max_height(ui.available_height().max(160.0))
                        .scroll_bar_visibility(ScrollBarVisibility::VisibleWhenNeeded)
                        .auto_shrink([false, true])
                        .id_salt(id_salt)
                        .show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            ui.spacing_mut().item_spacing.y = 0.0;
                            for (i, path) in roots.iter().enumerate() {
                                let folder_name = path
                                    .file_name()
                                    .map(|n| n.to_string_lossy().to_string())
                                    .unwrap_or_else(|| path.display().to_string());
                                let row = Frame::none()
                                    .inner_margin(Margin::symmetric(8.0, 7.0))
                                    .rounding(Rounding::same(crate::ui::style::R_SM))
                                    .show(ui, |ui| {
                                        ui.horizontal(|ui| {
                                            ui.label(RichText::new(crate::ui::icons::Icons::FOLDER).size(16.0).color(p.accent));
                                            ui.add_space(2.0);
                                            ui.vertical(|ui| {
                                                ui.spacing_mut().item_spacing.y = 1.0;
                                                // Leave room for the move-up and remove buttons; long paths truncate.
                                                ui.set_max_width(ui.available_width() - 76.0);
                                                ui.label(RichText::new(&folder_name).strong().size(13.0).color(p.text));
                                                ui.add(
                                                    egui::Label::new(
                                                        RichText::new(path.display().to_string()).size(11.5).color(p.text_muted),
                                                    )
                                                    .truncate(),
                                                );
                                            });
                                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                                if crate::ui::style::icon_button(ui, crate::ui::icons::Icons::CLOSE, false)
                                                    .on_hover_text("Remove folder")
                                                    .clicked()
                                                {
                                                    remove_idx = Some(i);
                                                }
                                                if i > 0
                                                    && crate::ui::style::icon_button(ui, crate::ui::icons::Icons::NAV_UP, false)
                                                        .on_hover_text("Move up")
                                                        .clicked()
                                                {
                                                    swap_up_idx = Some(i);
                                                }
                                            });
                                        });
                                    });
                                if i + 1 < roots.len() {
                                    let r = row.response.rect;
                                    ui.painter().hline(r.x_range(), r.bottom(), Stroke::new(1.0, p.border));
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

fn render_settings_panel(app: &mut SortwiseApp, ui: &mut egui::Ui, is_compare: bool) {
    let can_start = if is_compare {
        !app.source_roots.is_empty() && !app.dest_roots.is_empty()
    } else {
        !app.roots.is_empty()
    };
    let btn_text = if is_compare { "Start A → B comparison" } else { "Find duplicates" };
    let warn = if is_compare {
        "Select at least 1 Source and 1 Destination folder."
    } else {
        "Select at least 1 folder to scan."
    };

    let clicked = launch_card(
        ui,
        "Settings",
        |ui| {
                ui.checkbox(&mut app.skip_system_dirs, "Skip system/tool folders")
                    .on_hover_text("Excludes .git, node_modules, $RECYCLE.BIN, cache, and build folders.");
                ui.add_space(4.0);
                ui.checkbox(&mut app.skip_hidden, "Skip hidden dotfiles (e.g. .photos)")
                    .on_hover_text("When checked, files starting with '.' are skipped.");
                ui.add_space(10.0);

                ui.horizontal(|ui| {
                    ui.label(RichText::new("Minimum size").size(12.5).color(crate::ui::style::pal().text_muted));
                    // Unit is UI-only state; the engine always receives bytes (U6).
                    let unit_id = ui.id().with("min_size_unit");
                    let mut unit = ui.data_mut(|d| *d.get_temp_mut_or(unit_id, 1024u64));
                    let mut value = app.min_file_size as f64 / unit as f64;
                    let changed = ui
                        .add_sized(
                            [72.0, 26.0],
                            egui::DragValue::new(&mut value)
                                .speed(0.1)
                                .range(0.0..=1.0e9)
                                .max_decimals(2)
                                .custom_formatter(|v, _| format!("{v:.2}").trim_end_matches('0').trim_end_matches('.').to_string()),
                        )
                        .changed();
                    egui::ComboBox::from_id_salt(unit_id.with("combo"))
                        .width(64.0)
                        .selected_text(unit_label(unit))
                        .show_ui(ui, |ui| {
                            for u in [1u64, 1024, 1024 * 1024, 1024 * 1024 * 1024] {
                                ui.selectable_value(&mut unit, u, unit_label(u));
                            }
                        });
                    // Only the typed value writes back; changing the unit keeps the byte count.
                    if changed {
                        app.min_file_size = (value.max(0.0) * unit as f64).round() as u64;
                    }
                    ui.data_mut(|d| d.insert_temp(unit_id, unit));
                });
                ui.add_space(10.0);

                ui.label(RichText::new("When duplicates are found").size(12.5).color(crate::ui::style::pal().text_muted));
                ui.add_space(2.0);
                egui::ComboBox::new(egui::Id::new("action_mode_scan_box"), "")
                    .width(ui.available_width())
                    .selected_text(match app.action_mode {
                        ActionMode::DryRun => "Report only",
                        ActionMode::Delete => "Trash duplicates",
                        ActionMode::HardLink => "Replace with hard links",
                    })
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut app.action_mode, ActionMode::DryRun, "Report only");
                        ui.selectable_value(&mut app.action_mode, ActionMode::Delete, "Trash duplicates");
                        ui.selectable_value(&mut app.action_mode, ActionMode::HardLink, "Replace with hard links");
                    });

                ui.add_space(14.0);

                egui::CollapsingHeader::new(
                    RichText::new("Advanced Filters")
                        .size(12.5)
                        .color(crate::ui::style::pal().text_muted),
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

        },
        btn_text,
        can_start,
        warn,
        "Scans photos, videos, audio and archives.",
    );
    if clicked {
        app.start_scan();
    }
}

/// Launch card shared by every scan screen: title, body, and the primary button pinned to the
/// bottom with its one-line hint. Returns true when the button was clicked.
pub fn launch_card(
    ui: &mut egui::Ui,
    title: &str,
    body: impl FnOnce(&mut egui::Ui),
    btn_text: &str,
    can_start: bool,
    warn: &str,
    hint: &str,
) -> bool {
    let mut clicked = false;
    Frame::none()
        .fill(crate::ui::style::pal().surface)
        .stroke(Stroke::new(1.0, crate::ui::style::pal().border))
        .rounding(Rounding::same(crate::ui::style::R_MD))
        .inner_margin(Margin::same(16.0))
        .show(ui, |ui| {
            ui.set_min_height(ui.available_height());
            ui.vertical(|ui| {
                ui.label(crate::ui::style::section_title(title));
                ui.add_space(12.0);
                body(ui);
                ui.add_space(18.0);

                // Bottom-up: the hint is laid out first, so the button and hint stay at the card bottom.
                ui.with_layout(egui::Layout::bottom_up(egui::Align::Min), |ui| {
                    if can_start {
                        ui.label(muted(hint));
                    } else {
                        ui.label(RichText::new(warn).size(11.0).color(crate::ui::style::pal().warning));
                    }
                    ui.add_space(6.0);

                    let p = crate::ui::style::pal();
                    let btn = egui::Button::new(
                        RichText::new(btn_text)
                            .strong()
                            .size(14.0)
                            .color(if can_start { p.on_accent } else { p.text_muted }),
                    )
                    .fill(if can_start { p.accent } else { p.surface_raised })
                    .stroke(if can_start { Stroke::NONE } else { Stroke::new(1.0, p.border) })
                    .rounding(Rounding::same(crate::ui::style::R_SM))
                    .min_size(egui::vec2(ui.available_width(), 36.0));
                    if ui.add_enabled(can_start, btn).clicked() {
                        clicked = true;
                    }

                    ui.add_space(12.0);
                });
            });
        });
    clicked
}

fn unit_label(bytes: u64) -> &'static str {
    match bytes {
        1 => "B",
        1024 => "KB",
        u if u == 1024 * 1024 => "MB",
        _ => "GB",
    }
}
