use std::collections::HashMap;
use std::path::{Path, PathBuf};
use eframe::egui::{self, RichText, ScrollArea};
use crate::app::SortwiseApp;
use crate::ui::style::{self, folder_chip, overlap_heat_row, panel_card, secondary_button};
use sortwise::types::{DuplicateGroup, ScanResult};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct FolderOverlapEntry {
    pub shared_groups: usize,
    pub shared_bytes: u64,
    pub groups_in_a: usize,
    pub groups_in_b: usize,
    pub overlap_pct: f32,
    pub total_files_in_a: usize,
    pub total_files_in_b: usize,
    pub total_dupes_in_a: usize,
    pub total_dupes_in_b: usize,
}

fn count_physical_files(dir: &Path) -> usize {
    std::fs::read_dir(dir)
        .map(|rd| {
            rd.filter_map(|e| e.ok())
                .filter(|e| e.path().is_file())
                .count()
        })
        .unwrap_or(0)
}

pub fn compute_folder_overlap(
    result: &ScanResult
) -> HashMap<(PathBuf, PathBuf), FolderOverlapEntry> {
    let mut overlap_map: HashMap<(PathBuf, PathBuf), FolderOverlapEntry> = HashMap::new();
    
    // First, count total groups each folder is present in
    let mut folder_group_counts: HashMap<PathBuf, usize> = HashMap::new();
    let mut folder_dupes_counts: HashMap<PathBuf, usize> = HashMap::new();
    
    for g in &result.duplicate_groups {
        let mut folders_in_group = std::collections::HashSet::new();
        for f in &g.files {
            if let Some(parent) = f.path.parent() {
                folders_in_group.insert(parent.to_path_buf());
                *folder_dupes_counts.entry(parent.to_path_buf()).or_insert(0) += 1;
            }
        }
        for folder in folders_in_group {
            *folder_group_counts.entry(folder).or_insert(0) += 1;
        }
    }

    // Now, calculate overlap pairs
    for g in &result.duplicate_groups {
        let mut folders: Vec<PathBuf> = g.files.iter()
            .filter_map(|f| f.path.parent())
            .map(|p| p.to_path_buf())
            .collect();
        folders.sort();
        folders.dedup();

        for i in 0..folders.len() {
            for j in (i + 1)..folders.len() {
                let key = (folders[i].clone(), folders[j].clone());
                let dupes_a = *folder_dupes_counts.get(&folders[i]).unwrap_or(&0);
                let dupes_b = *folder_dupes_counts.get(&folders[j]).unwrap_or(&0);
                let files_a = count_physical_files(&folders[i]);
                let files_b = count_physical_files(&folders[j]);
                
                let entry = overlap_map.entry(key).or_insert(FolderOverlapEntry {
                    shared_groups: 0,
                    shared_bytes: 0,
                    groups_in_a: *folder_group_counts.get(&folders[i]).unwrap_or(&0),
                    groups_in_b: *folder_group_counts.get(&folders[j]).unwrap_or(&0),
                    overlap_pct: 0.0,
                    total_files_in_a: files_a,
                    total_files_in_b: files_b,
                    total_dupes_in_a: dupes_a,
                    total_dupes_in_b: dupes_b,
                });
                entry.shared_groups += 1;
                entry.shared_bytes += g.total_wasted_bytes;
            }
        }
    }

    // Compute percentage values
    for (_, entry) in overlap_map.iter_mut() {
        let min_groups = entry.groups_in_a.min(entry.groups_in_b) as f32;
        if min_groups > 0.0 {
            entry.overlap_pct = (entry.shared_groups as f32 / min_groups) * 100.0;
        }
    }

    overlap_map
}

pub fn folder_intel_section(ui: &mut egui::Ui, app: &SortwiseApp, result: &ScanResult, group_idx: usize) {
    let Some(g) = result.duplicate_groups.get(group_idx) else { return; };
    let mut folders: Vec<PathBuf> = g.files.iter()
        .filter_map(|f| f.path.parent())
        .map(|p| p.to_path_buf())
        .collect();
    folders.sort();
    folders.dedup();

    if folders.len() < 2 {
        ui.add_space(4.0);
        ui.label(RichText::new("ℹ  All copies in this group are in the same folder. Safe to clean.").size(11.5).color(crate::ui::style::pal().text_muted));
        return;
    }

    ui.add_space(4.0);
    ui.horizontal(|ui| {
        ui.label(RichText::new("Folder Overlap Intelligence:").size(11.0).strong().color(crate::ui::style::pal().text_muted));
        ui.label(RichText::new(format!("Spans {} folders.", folders.len())).size(11.0).color(crate::ui::style::pal().text_muted));
    });
    ui.add_space(2.0);

    for (i, j) in (0..folders.len()).flat_map(|i| ((i + 1)..folders.len()).map(move |j| (i, j))) {
        let key = (folders[i].clone(), folders[j].clone());
        if let Some(entry) = app.folder_overlap.get(&key) {
            let name_a = folders[i].file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| "/".into());
            let name_b = folders[j].file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| "/".into());
            overlap_heat_row(ui, &name_a, &name_b, entry.overlap_pct, entry.shared_groups);
            ui.add_space(1.0);
        }
    }
}

pub fn folder_twins_modal(_ui: &mut egui::Ui, ctx: &egui::Context, app: &mut SortwiseApp, result: &ScanResult) {
    if !app.dedupe_twins_modal_open {
        return;
    }


    let mut close_clicked = false;
    let closed = crate::ui::modal::show(
        ctx,
        "folder_twins",
        "Folder twins overlap",
        crate::ui::modal::ModalSize::Wide,
        |ui| {
            if let Some((folder_a, folder_b, selected_idx)) = app.twins_compare_pair.clone() {
                let name_a = folder_a.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
                let name_b = folder_b.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();

                ui.horizontal(|ui| {
                    if secondary_button(ui, "← Back").clicked() {
                        app.twins_compare_pair = None;
                    }
                    ui.add_space(10.0);
                    ui.label(RichText::new(format!("\u{E24A} A: {}  ↔  \u{E24A} B: {}", name_a, name_b))
                        .strong().size(13.0).color(crate::ui::style::pal().text));
                });
                ui.add_space(6.0);

                // Collect all duplicate groups that have a copy in BOTH A and B
                let overlap_groups: Vec<(usize, &DuplicateGroup)> = result.duplicate_groups.iter().enumerate()
                    .filter(|(_, g)| {
                        let has_a = g.files.iter().any(|f| f.path.parent() == Some(&folder_a));
                        let has_b = g.files.iter().any(|f| f.path.parent() == Some(&folder_b));
                        has_a && has_b
                    })
                    .collect();

                if overlap_groups.is_empty() {
                    ui.centered_and_justified(|ui| {
                        ui.label(RichText::new("No matching duplicates between these folders.").size(13.0).color(crate::ui::style::pal().text_muted));
                    });
                    app.twins_compare_pair = None;
                } else {
                    let current_idx = selected_idx.min(overlap_groups.len().saturating_sub(1));

                    // ── Master-Detail: left list + right preview ──
                    // Must read available_height BEFORE entering ui.horizontal() — inside it returns a tiny value
                    let panel_height = ui.available_height() - 8.0;
                    ui.horizontal(|ui| {

                        // ── LEFT: list panel ──────────────────────
                        ui.allocate_ui(egui::vec2(220.0, panel_height), |ui| {
                            ui.vertical(|ui| {
                                ui.label(RichText::new(format!("{} matched pairs", overlap_groups.len()))
                                    .size(11.0).color(crate::ui::style::pal().text_muted));
                                ui.add_space(4.0);
                                ScrollArea::vertical()
                                    .id_salt("compare_list")
                                    .auto_shrink([false, false])
                                    .show(ui, |ui| {
                                        ui.set_width(ui.available_width());
                                        for (i, (_gi, g)) in overlap_groups.iter().enumerate() {
                                            let f_a = g.files.iter().find(|f| f.path.parent() == Some(&folder_a));
                                            let f_b = g.files.iter().find(|f| f.path.parent() == Some(&folder_b));

                                            let fname_a = f_a.map(|f| f.path.file_name().unwrap_or_default().to_string_lossy().to_string()).unwrap_or_else(|| "—".into());
                                            let fname_b = f_b.map(|f| f.path.file_name().unwrap_or_default().to_string_lossy().to_string()).unwrap_or_else(|| "—".into());
                                            let size_a = f_a.map(|f| crate::ui::preview::format_bytes(f.size)).unwrap_or_default();
                                            let size_b = f_b.map(|f| crate::ui::preview::format_bytes(f.size)).unwrap_or_default();

                                            let is_selected = i == current_idx;

                                            // Allocate row with click sense at allocation time (correct ScrollArea pattern)
                                            let row_width = ui.available_width();
                                            let (row_rect, row_resp) = ui.allocate_exact_size(
                                                egui::vec2(row_width, 60.0),
                                                egui::Sense::click(),
                                            );

                                            if row_resp.hovered() {
                                                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                                            }
                                            if row_resp.clicked() {
                                                app.twins_compare_pair = Some((folder_a.clone(), folder_b.clone(), i));
                                            }

                                            // Paint background
                                            let bg = if is_selected {
                                                crate::ui::style::pal().accent_soft
                                            } else if row_resp.hovered() {
                                                crate::ui::style::pal().surface_raised
                                            } else {
                                                egui::Color32::TRANSPARENT
                                            };
                                            ui.painter().rect_filled(row_rect, 6.0, bg);

                                            // Left-side accent bar for selected row
                                            if is_selected {
                                                let bar = egui::Rect::from_min_size(
                                                    row_rect.min,
                                                    egui::vec2(3.0, row_rect.height()),
                                                );
                                                ui.painter().rect_filled(bar, 2.0, crate::ui::style::pal().accent);
                                            }

                                            let inner = row_rect.shrink2(egui::vec2(10.0, 6.0));
                                            let font_a  = egui::FontId::proportional(10.5);
                                            let font_b  = egui::FontId::proportional(10.5);
                                            let font_sz = egui::FontId::proportional(9.0);

                                            // Truncate helper
                                            let trunc = |s: &str, n: usize| -> String {
                                                if s.len() > n { format!("{}…", &s[..n]) } else { s.to_string() }
                                            };

                                            // Line 1: A filename (blue)
                                            ui.painter().text(
                                                egui::pos2(inner.min.x, inner.min.y),
                                                egui::Align2::LEFT_TOP,
                                                format!("A  {}", trunc(&fname_a, 22)),
                                                font_a,
                                                crate::ui::style::pal().accent_on_soft,
                                            );
                                            // Line 2: B filename (green)
                                            ui.painter().text(
                                                egui::pos2(inner.min.x, inner.min.y + 18.0),
                                                egui::Align2::LEFT_TOP,
                                                format!("B  {}", trunc(&fname_b, 22)),
                                                font_b,
                                                crate::ui::style::pal().success,
                                            );
                                            // Line 3: sizes (muted)
                                            ui.painter().text(
                                                egui::pos2(inner.min.x, inner.min.y + 36.0),
                                                egui::Align2::LEFT_TOP,
                                                format!("{}  ·  {}", size_a, size_b),
                                                font_sz,
                                                crate::ui::style::pal().text_muted,
                                            );

                                            ui.add_space(3.0);
                                        }
                                    });
                            });
                        });

                        ui.separator();

                        // ── RIGHT: preview panel ───────────────────
                        ui.vertical(|ui| {
                            let (_gi, g) = overlap_groups[current_idx];
                            let file_a = g.files.iter().find(|f| f.path.parent() == Some(&folder_a));
                            let file_b = g.files.iter().find(|f| f.path.parent() == Some(&folder_b));

                            let mut trash_a: Option<PathBuf> = None;
                            let mut trash_b: Option<PathBuf> = None;

                            if let Some(dec) = app.inline_video.as_ref() {
                                let mut latest = None;
                                while let Some(f) = dec.pop_frame() {
                                    latest = Some(f);
                                }
                                if let Some(img) = latest {
                                    match app.preview_texture.as_mut() {
                                        None => {
                                            app.preview_texture = Some(
                                                ctx.load_texture("sfm_inline_video", img, eframe::egui::TextureOptions::LINEAR),
                                            );
                                        }
                                        Some(t) => t.set(img, eframe::egui::TextureOptions::LINEAR),
                                    }
                                }
                                ctx.request_repaint_after(std::time::Duration::from_millis(32));
                            }

                            ScrollArea::vertical()
                                .id_salt("right_preview_scroll")
                                .auto_shrink([false, false])
                                .show(ui, |ui| {
                                    ui.columns(2, |columns| {
                                        // Column A preview
                                        columns[0].vertical(|ui| {
                                            ui.horizontal(|ui| {
                                                ui.label(RichText::new(format!("\u{E24A} A: {}", name_a))
                                                    .strong().size(11.0).color(crate::ui::style::pal().accent_on_soft));
                                            });
                                            ui.separator();
                                    if let Some(f_a) = file_a {
                                        let fname_a = f_a.path.file_name().unwrap_or_default().to_string_lossy();
                                        ui.horizontal(|ui| {
                                            ui.label(RichText::new(fname_a.as_ref()).size(11.5).color(crate::ui::style::pal().text));
                                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                                if ui.small_button("\u{E4A6} Trash A").on_hover_text(f_a.path.display().to_string()).clicked() {
                                                    trash_a = Some(f_a.path.clone());
                                                }
                                            });
                                        });
                                        ui.label(style::muted(crate::ui::preview::format_bytes(f_a.size).to_string()));
                                        ui.add_space(6.0);
                                        ui.add_space(6.0);
                                        let is_video = crate::ui::preview::is_video_path(&f_a.path);
                                        let mut played_here = false;
                                        if is_video && app.inline_video.is_some() && app.preview_path.as_deref() == Some(f_a.path.as_path()) {
                                            played_here = true;
                                            if let Some(tex) = &app.preview_texture {
                                                let tw = tex.size()[0] as f32;
                                                let th = tex.size()[1] as f32;
                                                let scale = (ui.available_width() / tw).min(350.0 / th);
                                                let fitted_size = egui::vec2(tw * scale, th * scale);
                                                ui.vertical_centered(|ui| {
                                                    ui.add(egui::Image::from_texture(eframe::egui::load::SizedTexture::from_handle(tex))
                                                        .fit_to_exact_size(fitted_size)
                                                        .rounding(egui::Rounding::same(8.0)));
                                                });
                                            }
                                        }
                                        if !played_here {
                                            crate::ui::explorer_panel::draw_thumb_for_path(app, ui, egui::vec2(ui.available_width(), 350.0), &f_a.path);
                                            // Show error if it failed to load
                                            if is_video && app.preview_path.as_deref() == Some(f_a.path.as_path()) && app.inline_video.is_none() {
                                                if let Some(err) = &app.preview_scroll_text {
                                                    ui.add_space(4.0);
                                                    ui.label(RichText::new(err).color(crate::ui::style::pal().danger).size(11.0));
                                                }
                                            }
                                        }
                                        ui.add_space(8.0);
                                        ui.horizontal_wrapped(|ui| {
                                            if is_video {
                                                if played_here {
                                                    if style::secondary_button(ui, "\u{E46C} Stop").clicked() {
                                                        app.inline_video = None;
                                                        app.preview_path = None;
                                                    }
                                                } else if style::secondary_button(ui, "\u{E3D0} Play").clicked() {
                                                    app.preview_path = None;
                                                    crate::ui::results_panel::update_preview(ctx, app, &f_a.path, 0);
                                                }
                                            }
                                            if style::secondary_button(ui, "Reveal").clicked() {
                                                crate::ui::preview::reveal_in_folder(&f_a.path);
                                            }
                                            if style::secondary_button(ui, "Open App").clicked() {
                                                crate::ui::preview::open_in_default_app(&f_a.path);
                                            }
                                        });
                                    } else {
                                        ui.label(style::muted("— already deleted —"));
                                    }
                                });

                                // Column B preview
                                columns[1].vertical(|ui| {
                                    ui.horizontal(|ui| {
                                        ui.label(RichText::new(format!("\u{E24A} B: {}", name_b))
                                            .strong().size(11.0).color(crate::ui::style::pal().success_on_soft));
                                    });
                                    ui.separator();
                                    if let Some(f_b) = file_b {
                                        let fname_b = f_b.path.file_name().unwrap_or_default().to_string_lossy();
                                        ui.horizontal(|ui| {
                                            ui.label(RichText::new(fname_b.as_ref()).size(11.5).color(crate::ui::style::pal().text));
                                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                                if ui.small_button("\u{E4A6} Trash B").on_hover_text(f_b.path.display().to_string()).clicked() {
                                                    trash_b = Some(f_b.path.clone());
                                                }
                                            });
                                        });
                                        ui.label(style::muted(crate::ui::preview::format_bytes(f_b.size).to_string()));
                                        ui.add_space(6.0);
                                        ui.add_space(6.0);
                                        let is_video = crate::ui::preview::is_video_path(&f_b.path);
                                        let mut played_here = false;
                                        if is_video && app.inline_video.is_some() && app.preview_path.as_deref() == Some(f_b.path.as_path()) {
                                            played_here = true;
                                            if let Some(tex) = &app.preview_texture {
                                                let tw = tex.size()[0] as f32;
                                                let th = tex.size()[1] as f32;
                                                let scale = (ui.available_width() / tw).min(350.0 / th);
                                                let fitted_size = egui::vec2(tw * scale, th * scale);
                                                ui.vertical_centered(|ui| {
                                                    ui.add(egui::Image::from_texture(eframe::egui::load::SizedTexture::from_handle(tex))
                                                        .fit_to_exact_size(fitted_size)
                                                        .rounding(egui::Rounding::same(8.0)));
                                                });
                                            }
                                        }
                                        if !played_here {
                                            crate::ui::explorer_panel::draw_thumb_for_path(app, ui, egui::vec2(ui.available_width(), 350.0), &f_b.path);
                                            // Show error if it failed to load
                                            if is_video && app.preview_path.as_deref() == Some(f_b.path.as_path()) && app.inline_video.is_none() {
                                                if let Some(err) = &app.preview_scroll_text {
                                                    ui.add_space(4.0);
                                                    ui.label(RichText::new(err).color(crate::ui::style::pal().danger).size(11.0));
                                                }
                                            }
                                        }
                                        ui.add_space(8.0);
                                        ui.horizontal_wrapped(|ui| {
                                            if is_video {
                                                if played_here {
                                                    if style::secondary_button(ui, "\u{E46C} Stop").clicked() {
                                                        app.inline_video = None;
                                                        app.preview_path = None;
                                                    }
                                                } else if style::secondary_button(ui, "\u{E3D0} Play").clicked() {
                                                    app.preview_path = None;
                                                    crate::ui::results_panel::update_preview(ctx, app, &f_b.path, 0);
                                                }
                                            }
                                            if style::secondary_button(ui, "Reveal").clicked() {
                                                crate::ui::preview::reveal_in_folder(&f_b.path);
                                            }
                                            if style::secondary_button(ui, "Open App").clicked() {
                                                crate::ui::preview::open_in_default_app(&f_b.path);
                                            }
                                        });
                                    } else {
                                        ui.label(style::muted("— already deleted —"));
                                    }
                                });
                            });
                        });

                            // Process trash outside columns borrow
                            for path in trash_a.into_iter().chain(trash_b) {
                                let yes = rfd::MessageDialog::new()
                                    .set_level(rfd::MessageLevel::Warning)
                                    .set_title("Trash duplicate file")
                                    .set_description(format!("Move\n{}\nto Trash?", path.display()))
                                    .set_buttons(rfd::MessageButtons::YesNo)
                                    .show();
                                if yes == rfd::MessageDialogResult::Yes {
                                    if let Ok(()) = sortwise::resolver::trash_single_path(&path) {
                                        let mut removed = std::collections::HashSet::new();
                                        removed.insert(path.clone());
                                        crate::ui::results_panel::prune_paths_from_results(app, &removed);
                                        app.clear_preview_cache();
                                    }
                                }
                            }
                        });
                    });
                }
            } else {
                ui.label(RichText::new("Pairs of folders containing duplicate files. Percentage shows shared duplicate groups relative to the smaller folder's duplicate set. Use the (duplicate/total files) stats to identify exact mirror folders.").size(11.5).color(crate::ui::style::pal().text_muted));
                ui.add_space(8.0);
                
                let mut pairs: Vec<((PathBuf, PathBuf), FolderOverlapEntry)> = app.folder_overlap.iter()
                    .map(|(k, v)| (k.clone(), v.clone()))
                    .collect();
                
                // Sort by overlap percentage descending, then total wasted bytes descending
                pairs.sort_by(|a, b| {
                    b.1.overlap_pct.partial_cmp(&a.1.overlap_pct).unwrap_or(std::cmp::Ordering::Equal)
                        .then_with(|| b.1.shared_bytes.cmp(&a.1.shared_bytes))
                });

                let mut all_a = !pairs.is_empty() && pairs.iter().all(|(k, _)| app.twins_trash_selections.get(k).map(|s| s.0).unwrap_or(false));
                let mut all_b = !pairs.is_empty() && pairs.iter().all(|(k, _)| app.twins_trash_selections.get(k).map(|s| s.1).unwrap_or(false));

                ui.horizontal(|ui| {
                    if ui.checkbox(&mut all_a, "Select All A").changed() {
                        for (k, _) in &pairs {
                            let mut sel = app.twins_trash_selections.get(k).copied().unwrap_or((false, false));
                            sel.0 = all_a;
                            app.twins_trash_selections.insert(k.clone(), sel);
                        }
                    }
                    if ui.checkbox(&mut all_b, "Select All B").changed() {
                        for (k, _) in &pairs {
                            let mut sel = app.twins_trash_selections.get(k).copied().unwrap_or((false, false));
                            sel.1 = all_b;
                            app.twins_trash_selections.insert(k.clone(), sel);
                        }
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let selected_count = app.twins_trash_selections.values().filter(|(a, b)| *a || *b).count();
                        let btn = egui::Button::new(RichText::new(format!("\u{E4A6} Delete Selected ({})", selected_count)).color(crate::ui::style::pal().danger));
                        if ui.add_enabled(selected_count > 0, btn).clicked() {
                            trash_selected_twins(app, result);
                        }
                    });
                });
                ui.add_space(8.0);

                if pairs.is_empty() {
                    ui.centered_and_justified(|ui| {
                        ui.label(RichText::new("No overlapping folders found in duplicate groups.").size(13.0).color(crate::ui::style::pal().text_muted));
                    });
                } else {
                    ScrollArea::vertical()
                        .max_height(320.0)
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            for ((folder_a, folder_b), entry) in pairs {
                                panel_card(ui, |ui| {
                                    ui.vertical(|ui| {
                                        ui.horizontal(|ui| {
                                            let name_a = folder_a.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| "/".into());
                                            let name_b = folder_b.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| "/".into());
                                            
                                            let chip_text_a = format!("{} - ({}/{})", name_a, entry.total_dupes_in_a, entry.total_files_in_a);
                                            let chip_text_b = format!("{} - ({}/{})", name_b, entry.total_dupes_in_b, entry.total_files_in_b);

                                            if folder_chip(ui, &chip_text_a).interact(egui::Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand).on_hover_text(folder_a.display().to_string()).clicked() {
                                                crate::ui::preview::reveal_in_folder(&folder_a);
                                            }
                                            ui.label(RichText::new("↔").strong().color(crate::ui::style::pal().text_muted));
                                            if folder_chip(ui, &chip_text_b).interact(egui::Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand).on_hover_text(folder_b.display().to_string()).clicked() {
                                                crate::ui::preview::reveal_in_folder(&folder_b);
                                            }
                                            
                                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                                let bg = if entry.overlap_pct >= 80.0 {
                                                    crate::ui::style::pal().danger_soft
                                                } else if entry.overlap_pct >= 40.0 {
                                                    crate::ui::style::pal().warning_soft
                                                } else {
                                                    crate::ui::style::pal().success_soft
                                                };
                                                let fg = if entry.overlap_pct >= 80.0 {
                                                    crate::ui::style::pal().danger_on_soft
                                                } else if entry.overlap_pct >= 40.0 {
                                                    crate::ui::style::pal().warning_on_soft
                                                } else {
                                                    crate::ui::style::pal().success_on_soft
                                                };
                                                style::pill(ui, &format!("{:.1}% overlap", entry.overlap_pct), fg, bg);
                                            });
                                        });
                                        ui.add_space(4.0);
                                        
                                        ui.label(RichText::new(format!("A: {}", folder_a.display())).size(10.5).color(crate::ui::style::pal().text_muted));
                                        ui.label(RichText::new(format!("B: {}", folder_b.display())).size(10.5).color(crate::ui::style::pal().text_muted));
                                        ui.add_space(4.0);

                                        ui.horizontal(|ui| {
                                            ui.label(RichText::new(format!(
                                                "Shared duplicates: {} groups · Wasted: {}",
                                                entry.shared_groups,
                                                crate::ui::preview::format_bytes(entry.shared_bytes)
                                            )).size(11.5).color(crate::ui::style::pal().text_muted));
                                        });
                                        ui.add_space(4.0);

                                        ui.horizontal(|ui| {
                                            let name_a = folder_a.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| "/".into());
                                            let name_b = folder_b.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| "/".into());
                                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                                if ui.small_button("Compare").on_hover_text(format!("Compare duplicates in {} and {}", name_a, name_b)).clicked() {
                                                    app.twins_compare_pair = Some((folder_a.clone(), folder_b.clone(), 0));
                                                }
                                                let key = (folder_a.clone(), folder_b.clone());
                                                let mut selections = app.twins_trash_selections.get(&key).copied().unwrap_or((false, false));
                                                if ui.checkbox(&mut selections.1, "Trash B").changed() {
                                                    app.twins_trash_selections.insert(key.clone(), selections);
                                                }
                                                if ui.checkbox(&mut selections.0, "Trash A").changed() {
                                                    app.twins_trash_selections.insert(key, selections);
                                                }
                                            });
                                        });
                                    });
                                });
                                ui.add_space(6.0);
                            }
                        });
                }

            }
        },
        // Footer runs right to left: Done (primary).
        |ui| {
            if crate::ui::style::primary_button(ui, "Done").clicked() {
                close_clicked = true;
            }
        },
    );

    app.dedupe_twins_modal_open = !(closed || close_clicked);
}

fn trash_selected_twins(
    app: &mut SortwiseApp,
    result: &ScanResult,
) {
    let mut to_trash = std::collections::HashSet::new();

    for ((folder_a, folder_b), (trash_a, trash_b)) in &app.twins_trash_selections {
        if !trash_a && !trash_b { continue; }

        for g in &result.duplicate_groups {
            let has_a = g.files.iter().any(|f| f.path.parent() == Some(folder_a));
            let has_b = g.files.iter().any(|f| f.path.parent() == Some(folder_b));
            
            if has_a && has_b {
                for f in &g.files {
                    let p = f.path.parent();
                    if *trash_a && p == Some(folder_a) {
                        to_trash.insert(f.path.clone());
                    }
                    if *trash_b && p == Some(folder_b) {
                        to_trash.insert(f.path.clone());
                    }
                }
            }
        }
    }

    if to_trash.is_empty() {
        app.apply_message = Some("No duplicates selected to delete.".into());
        return;
    }

    use rfd::{MessageDialog, MessageLevel, MessageButtons, MessageDialogResult};
    use sortwise::resolver::trash_batch;

    let yes = MessageDialog::new()
        .set_level(MessageLevel::Warning)
        .set_title("Delete Overlapping Duplicates")
        .set_description(format!("Move {} selected duplicate files to Trash?", to_trash.len()))
        .set_buttons(MessageButtons::YesNo)
        .show();

    if yes == MessageDialogResult::Yes {
        let paths_vec: Vec<PathBuf> = to_trash.into_iter().collect();

        match trash_batch(&paths_vec) {
            Ok(trashed) => {
                let paths_removed: std::collections::HashSet<PathBuf> = trashed.into_iter().collect();
                crate::ui::results_panel::prune_paths_from_results(app, &paths_removed);
                app.clear_preview_cache();
                app.twins_trash_selections.clear();
                app.apply_message = Some(format!("Removed {} duplicate file(s) in one batch.", paths_removed.len()));
                app.apply_errors = None;
            }
            Err(e) => {
                app.apply_errors = Some(format!("Batch trash error: {}", e));
            }
        }
    }
}

