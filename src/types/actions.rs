use super::library::ActionMode;

#[derive(Debug, Clone)]
pub enum PendingAction {
    Trash {
        title: String,
        action: &'static str,
        plan: crate::action_plan::ActionPlan,
    },
    Apply {
        mode: ActionMode,
        plan: crate::action_plan::ActionPlan,
    },
}
