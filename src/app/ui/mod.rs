impl HestiaApp {
    fn right_pane_bound_window_open(&self) -> bool {
        let profile_progress_open = self
            .profile_operation_inflight
            .as_ref()
            .is_some_and(|inflight| {
                inflight.kind != ProfileOperationKind::Recover
                    && !self.profile_operation_locks_app()
            });
        let import_review_open = self.pending_conflicts.is_empty()
            && self
                .pending_imports
                .front()
                .is_some_and(|pending| pending.inspection.candidates.len() > 1);

        self.settings_open
            || self.state.show_tasks
            || self.state.show_tools
            || self.state.show_log
            || self.state.show_whats_new
            || self.state.show_feedback_survey
            || self.tool_launch_options_prompt.is_some()
            || self.browse_state.file_prompt.is_some()
            || !self.pending_conflicts.is_empty()
            || import_review_open
            || self.profile_name_prompt.is_some()
            || self.pending_profile_delete_id.is_some()
            || profile_progress_open
    }

    fn render_right_pane_window_scrim(&self, ctx: &egui::Context) {
        if !self.right_pane_bound_window_open() {
            return;
        }
        let Some(rect) = self.last_right_pane_rect else {
            return;
        };

        egui::Area::new(egui::Id::new("right_pane_window_scrim"))
            .order(egui::Order::Middle)
            .fixed_pos(rect.min)
            .interactable(true)
            .show(ctx, |ui| {
                let (scrim_rect, response) =
                    ui.allocate_exact_size(rect.size(), egui::Sense::click_and_drag());
                ui.painter().rect_filled(
                    scrim_rect,
                    0.0,
                    Color32::from_black_alpha(84),
                );
                response.on_hover_cursor(egui::CursorIcon::Default);
            });
    }
}

include!("widgets.rs");
include!("chrome.rs");
include!("profiles.rs");
include!("windows.rs");
include!("library.rs");
include!("browse.rs");
include!("dialogs.rs");
