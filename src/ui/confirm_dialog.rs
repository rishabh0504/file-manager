//! Confirm dialog for staged bulk actions (Implementation-Plan Phase 1, U1–U3).
//!
//! Shows what the action will touch before anything changes. Cancel and Esc
//! discard the plan; only Confirm runs it.

use crate::app::{PendingAction, SortwiseApp};
use crate::ui::preview::format_bytes;
use crate::ui::modal;
use crate::ui::style::{danger_button, muted, primary_button, secondary_button};
use eframe::egui::{self, RichText, ScrollArea};
use sortwise::action_plan::ActionPlan;
use sortwise::types::ActionMode;

pub fn confirm_pending_action(app: &mut SortwiseApp, ctx: &egui::Context) {
    let Some(pending) = app.pending_action.as_ref() else {
        return;
    };
    let (title, plan, intro, confirm_label) = match pending {
        PendingAction::Trash { title, plan, .. } => (
            title.clone(),
            plan,
            "These files will be moved to the Trash. You can restore them from the Trash until it is emptied.",
            "Move to Trash",
        ),
        PendingAction::Apply { mode: ActionMode::Delete, plan } => (
            "Apply: delete duplicates".to_string(),
            plan,
            "These duplicate copies will be moved to the Trash. The keep file in each group is not touched.",
            "Move to Trash",
        ),
        PendingAction::Apply { mode: ActionMode::HardLink, plan } => (
            "Apply: replace with hard links".to_string(),
            plan,
            "Each duplicate will become a hard link to its group's keep file. No data is lost.",
            "Replace with hard links",
        ),
        PendingAction::Apply { mode: ActionMode::DryRun, plan } => (
            "Dry run".to_string(),
            plan,
            "Dry run: nothing will change.",
            "Close",
        ),
    };
    let summary = summary_line(plan);
    let sample = plan.sample.clone();
    let more = plan.paths.len().saturating_sub(sample.len());

    let mut decision: Option<bool> = None;
    let destructive = confirm_label == "Move to Trash";
    let closed = modal::show(
        ctx,
        "confirm_action",
        &title,
        modal::ModalSize::Narrow,
        |ui| {
            ui.label(RichText::new(summary).strong().size(15.0).color(crate::ui::style::pal().text));
            ui.add_space(4.0);
            ui.label(muted(intro));
            ui.add_space(10.0);

            ScrollArea::vertical().max_height(180.0).id_salt("confirm_sample").show(ui, |ui| {
                for p in &sample {
                    ui.label(RichText::new(p.display().to_string()).monospace().size(11.0).color(crate::ui::style::pal().text_muted));
                }
                if more > 0 {
                    ui.label(muted(format!("…and {more} more")));
                }
            });
        },
        // Footer runs right to left: primary first, then Cancel on its left.
        |ui| {
            if plan.is_empty() {
                ui.add_enabled(false, egui::Button::new(confirm_label));
            } else if destructive {
                if danger_button(ui, confirm_label).clicked() {
                    decision = Some(true);
                }
            } else if primary_button(ui, confirm_label).clicked() {
                decision = Some(true);
            }
            if secondary_button(ui, "Cancel").clicked() {
                decision = Some(false);
            }
        },
    );
    if closed && decision.is_none() {
        decision = Some(false);
    }

    match decision {
        Some(true) => {
            if let Some(pending) = app.pending_action.take() {
                if matches!(pending, PendingAction::Apply { mode: ActionMode::DryRun, .. }) {
                    return;
                }
                crate::ui::results_panel::execute_pending_action(app, pending);
            }
        }
        Some(false) => app.pending_action = None,
        None => {}
    }
}

fn summary_line(plan: &ActionPlan) -> String {
    if plan.is_empty() {
        return "Nothing to change.".to_string();
    }
    if plan.group_count == 0 {
        return format!("{} item(s) · {}", plan.paths.len(), format_bytes(plan.total_bytes));
    }
    format!(
        "{} file(s) in {} group(s) · {}",
        plan.paths.len(),
        plan.group_count,
        format_bytes(plan.total_bytes)
    )
}
