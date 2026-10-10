impl HestiaApp {
    fn start_folder_categories_batch(
        &mut self,
        category_ids: &[String],
        action: FolderBatchAction,
    ) -> bool {
        let Some(first_id) = category_ids.first() else {
            return false;
        };
        let Some(game_id) = self
            .state
            .categories
            .iter()
            .find(|category| category.id == *first_id)
            .map(|category| category.game_id.clone())
        else {
            return false;
        };
        if category_ids.iter().any(|category_id| {
            self.state
                .categories
                .iter()
                .find(|category| category.id == *category_id)
                .is_none_or(|category| category.game_id != game_id)
        }) {
            return false;
        }
        let category_ids = category_ids.to_vec();
        let selected: HashSet<String> = category_ids.iter().cloned().collect();
        let all_mod_ids = folder_member_ids(
            &self.categories_for_game(&game_id),
            &self.state.mods,
            &game_id,
            &selected,
        );
        self.start_folder_batch_targets(
            FolderBatchTargets {
                game_id,
                category_ids,
                mod_ids: all_mod_ids.clone(),
                all_mod_ids,
                scope: FolderContentsScope::All,
            },
            action,
            None,
        )
    }

    fn start_folder_category_batch(
        &mut self,
        category_id: &str,
        visible_mod_ids: &[String],
        scope: FolderContentsScope,
        action: FolderBatchAction,
    ) -> bool {
        let Some(game) = self.selected_game() else {
            return false;
        };
        let game_id = game.definition.id.clone();
        let categories = self.categories_for_game(&game_id);
        let category_ids = vec![category_id.to_owned()];
        let selected: HashSet<String> = category_ids.iter().cloned().collect();
        let all_mod_ids = folder_member_ids(&categories, &self.state.mods, &game_id, &selected);
        let visible: HashSet<&str> = visible_mod_ids.iter().map(String::as_str).collect();
        let mod_ids = all_mod_ids
            .iter()
            .filter(|id| scope == FolderContentsScope::All || visible.contains(id.as_str()))
            .cloned()
            .collect();
        self.start_folder_batch_targets(
            FolderBatchTargets {
                game_id,
                category_ids,
                mod_ids,
                all_mod_ids,
                scope,
            },
            action,
            None,
        )
    }

    fn start_folder_category_contents_batch(
        &mut self,
        category_id: &str,
        visible_mod_ids: &[String],
        action: FolderBatchAction,
    ) -> bool {
        let Some(game_id) = self
            .state
            .categories
            .iter()
            .find(|category| category.id == category_id)
            .map(|category| category.game_id.clone())
        else {
            return false;
        };
        let category_ids = vec![category_id.to_owned()];
        let selected: HashSet<String> = category_ids.iter().cloned().collect();
        let all_mod_ids = folder_member_ids(
            &self.categories_for_game(&game_id),
            &self.state.mods,
            &game_id,
            &selected,
        );
        let members: HashSet<&str> = all_mod_ids.iter().map(String::as_str).collect();
        let mod_ids = visible_mod_ids
            .iter()
            .filter(|id| members.contains(id.as_str()))
            .cloned()
            .collect();
        self.start_folder_batch_targets(
            FolderBatchTargets {
                game_id,
                category_ids,
                mod_ids,
                all_mod_ids,
                scope: FolderContentsScope::Visible,
            },
            action,
            None,
        )
    }

    fn render_folder_selection_toolbar(&mut self, ui: &mut Ui, how_expanded: f32) {
        let Some(mut targets) = self.folder_batch_targets() else {
            self.folder_delete_menu_requested = false;
            return;
        };
        if self.library_folder_contents_scope == FolderContentsScope::All
            && !self.folder_batch_scope_has_hidden_contents(&targets)
        {
            self.library_folder_contents_scope = FolderContentsScope::Visible;
            if let Some(fresh_targets) = self.folder_batch_targets() {
                targets = fresh_targets;
            }
        }
        let _ = how_expanded;
        let text = self.text();
        let busy = self.folder_batch_job.is_some();

        ui.add_space(-10.0);
        ui.spacing_mut().button_padding = egui::vec2(7.0, 5.0);
        let radius = egui::CornerRadius::same(5);
        ui.style_mut().visuals.widgets.inactive.corner_radius = radius;
        ui.style_mut().visuals.widgets.hovered.corner_radius = radius;
        ui.style_mut().visuals.widgets.active.corner_radius = radius;
        ui.style_mut().visuals.widgets.open.corner_radius = radius;
        ui.vertical(|ui| {
            self.render_folder_batch_common_toolbar(ui, &targets, busy);
            ui.add_space(2.0);
            ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                ui.add_space(6.0);
                let clear = ui
                    .add(
                        egui::Button::new(icon_rich(Icon::CircleX, 11.0, Color32::from_gray(170)))
                            .frame(false),
                    )
                    .on_hover_text(text.folder_clear_selection_tooltip())
                    .on_hover_cursor(egui::CursorIcon::PointingHand);
                if clear.hovered() {
                    ui.painter().circle_filled(
                        clear.rect.center(),
                        9.0,
                        Color32::from_rgba_premultiplied(90, 94, 102, 60),
                    );
                }
                if clear.clicked() {
                    self.clear_library_folder_selection();
                }
                ui.add_space(3.0);
                let summary = text.folder_selection_summary(targets.category_ids.len(), None);
                ui.add(
                    egui::Label::new(
                        RichText::new(&summary)
                            .size(12.0)
                            .color(Color32::from_gray(160)),
                    )
                    .truncate(),
                )
                .on_hover_text(summary);
            });
        });
    }

    fn render_folder_batch_common_toolbar(
        &mut self,
        ui: &mut Ui,
        targets: &FolderBatchTargets,
        busy: bool,
    ) {
        let text = self.text();
        let status_actions = [
            (FolderBatchAction::Enable, text.enable()),
            (FolderBatchAction::Disable, text.disable()),
            (FolderBatchAction::Restore, text.restore()),
            (FolderBatchAction::Archive, text.archive()),
        ];
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            for (action, _label) in status_actions {
                if !self.folder_batch_action_allowed(targets, action) {
                    continue;
                }
                self.render_folder_batch_action_button(ui, targets, action, busy, true);
            }

            let delete_actions = [
                FolderBatchAction::RemoveFolders,
                FolderBatchAction::DeleteContents,
                FolderBatchAction::DeleteFoldersAndContents,
            ];
            let delete_enabled = !busy
                && delete_actions
                    .into_iter()
                    .any(|action| self.folder_batch_action_allowed(targets, action));
            let delete_response = folder_batch_sized_action_button(
                ui,
                delete_enabled,
                90.0,
                egui::Button::new(folder_batch_delete_button_text(text.delete())),
            )
            .on_hover_cursor(egui::CursorIcon::PointingHand);
            let delete_targets = targets.clone();
            let delete_requested = std::mem::take(&mut self.folder_delete_menu_requested);
            show_folder_batch_delete_popup(ui, &delete_response, delete_requested, |ui| {
                ui.set_max_width(180.0);
                ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Extend);
                for action in delete_actions {
                    if self.render_folder_batch_delete_choice(ui, &delete_targets, action, busy) {
                        break;
                    }
                }
            });
        });
    }

    fn render_folder_batch_delete_choice(
        &mut self,
        ui: &mut Ui,
        targets: &FolderBatchTargets,
        action: FolderBatchAction,
        busy: bool,
    ) -> bool {
        let text = self.text();
        let enabled = !busy && self.folder_batch_action_allowed(targets, action);
        let hide_hidden_counts = self.state.static_prefs.unsafe_content_mode
            == UnsafeContentMode::HideNoCounter;
        if Self::render_folder_batch_delete_choice_button(
            ui,
            text,
            targets,
            action,
            enabled,
            busy,
            hide_hidden_counts,
        ) {
            self.start_folder_batch(action, None);
            ui.close();
            true
        } else {
            false
        }
    }

    fn render_folder_batch_delete_choice_button(
        ui: &mut Ui,
        text: TextCatalog,
        targets: &FolderBatchTargets,
        action: FolderBatchAction,
        enabled: bool,
        busy: bool,
        hide_hidden_counts: bool,
    ) -> bool {
        let label = match action {
            FolderBatchAction::RemoveFolders => text.folder_only_move_mods_outside(),
            FolderBatchAction::DeleteContents => text.folder_mods_inside_keep_folder(),
            FolderBatchAction::DeleteFoldersAndContents => text.folder_and_mods_inside(),
            _ => return false,
        };
        let mut response = ui.add_enabled(
            enabled,
            egui::Button::new(icon_text_sized(Icon::Trash2, label, 12.0, 12.0)),
        );
        let hidden_count = if targets.scope == FolderContentsScope::Visible {
            targets
                .all_mod_ids
                .len()
                .saturating_sub(targets.mod_ids.len())
        } else {
            0
        };
        if !enabled {
            let tooltip = if busy {
                text.folder_batch_busy_tooltip().to_owned()
            } else if action == FolderBatchAction::DeleteFoldersAndContents && hidden_count > 0 {
                if hide_hidden_counts {
                    text.folder_delete_requires_all_contents().to_owned()
                } else {
                    text.folder_and_mods_inside_hidden_tooltip(hidden_count)
                }
            } else if action == FolderBatchAction::DeleteContents && hidden_count > 0 {
                if hide_hidden_counts {
                    text.folder_batch_action_tooltip(action).to_owned()
                } else {
                    text.folder_mods_inside_keep_folder_hidden_tooltip(hidden_count)
                }
            } else {
                text.folder_batch_action_tooltip(action).to_owned()
            };
            response = response.on_disabled_hover_text(tooltip);
        } else if action == FolderBatchAction::DeleteContents && hidden_count > 0 {
            response = response.on_hover_text(if hide_hidden_counts {
                text.folder_batch_action_tooltip(action).to_owned()
            } else {
                text.folder_mods_inside_keep_folder_hidden_tooltip(hidden_count)
            });
        } else {
            response = response.on_hover_text(text.folder_batch_action_tooltip(action));
        }
        response.clicked()
    }

    /// Render the contextual scope row below the fixed-height selection toolbar.
    /// The library header owns the sibling placement so the row keeps its width when
    /// the search bar is expanded.
    fn render_folder_selection_scope_line(&mut self, ui: &mut Ui) {
        let Some(mut targets) = self.folder_batch_targets() else {
            return;
        };
        if self.library_folder_contents_scope == FolderContentsScope::All
            && !self.folder_batch_scope_has_hidden_contents(&targets)
        {
            self.library_folder_contents_scope = FolderContentsScope::Visible;
            if let Some(fresh_targets) = self.folder_batch_targets() {
                targets = fresh_targets;
            }
        }
        if self.folder_batch_scope_has_hidden_contents(&targets) {
            ui.horizontal(|ui| {
                ui.add_space(6.0);
                let _ = self.render_folder_batch_scope_control(ui, &targets);
            });
        }
    }

    fn render_folder_batch_scope_control(
        &mut self,
        ui: &mut Ui,
        targets: &FolderBatchTargets,
    ) -> bool {
        let has_hidden_contents = self.folder_batch_scope_has_hidden_contents(targets);
        if !has_hidden_contents {
            if self.library_folder_contents_scope == FolderContentsScope::All {
                self.library_folder_contents_scope = FolderContentsScope::Visible;
                return true;
            }
            return false;
        }

        let text = self.text();
        let current_scope = self.library_folder_contents_scope;
        let show_counts = self.folder_batch_show_counts(targets);
        let current_count = targets.mod_ids.len();
        let mut changed = false;
        ui.horizontal(|ui| {
            ui.spacing_mut().button_padding = egui::vec2(6.0, 2.0);
            ui.spacing_mut().interact_size.y = 20.0;
            ui.spacing_mut().item_spacing.x = 5.0;
            let summary = text.folder_batch_scope_summary(current_scope);
            let change_label = text.folder_batch_scope_change();
            let change_galley = ui.painter().layout_no_wrap(
                change_label.to_owned(),
                egui::TextStyle::Button.resolve(ui.style()),
                ui.visuals().text_color(),
            );
            let change_width = change_galley.size().x
                + ui.spacing().button_padding.x * 2.0
                + ui.spacing().item_spacing.x;
            let summary_width = (ui.available_width() - change_width).max(0.0);
            let summary_response = ui.add_sized(
                [summary_width, 18.0],
                egui::Label::new(
                    RichText::new(summary)
                        .size(11.0)
                        .color(Color32::from_gray(165)),
                )
                .truncate(),
            );
            summary_response.on_hover_text(summary);
            let change = ui
                .button(change_label)
                .on_hover_text(text.folder_batch_scope_tooltip())
                .on_hover_cursor(egui::CursorIcon::PointingHand);
            egui::Popup::menu(&change)
                .id(ui.id().with("folder_batch_scope_picker"))
                .width(248.0)
                .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
                .show(|ui| {
                    ui.label(
                        RichText::new(text.folder_batch_scope_summary(current_scope)).strong(),
                    );
                    if show_counts {
                        ui.label(text.folder_batch_scope_mod_count(current_count));
                    }
                    ui.separator();
                    for scope in [FolderContentsScope::Visible, FolderContentsScope::All] {
                        let response = ui
                            .selectable_label(
                                current_scope == scope,
                                text.folder_contents_scope(scope),
                            )
                            .on_hover_text(text.folder_batch_scope_tooltip());
                        if response.clicked() {
                            if self.library_folder_contents_scope != scope {
                                self.library_folder_contents_scope = scope;
                                changed = true;
                            }
                            ui.close();
                        }
                    }
                });
        });
        changed
    }

    fn render_folder_batch_action_button(
        &mut self,
        ui: &mut Ui,
        targets: &FolderBatchTargets,
        action: FolderBatchAction,
        busy: bool,
        compact: bool,
    ) {
        let text = self.text();
        let enabled = !busy && self.folder_batch_action_allowed(targets, action);
        let label = if compact {
            match action {
                FolderBatchAction::Update => text.update_button(),
                FolderBatchAction::Enable => text.enable(),
                FolderBatchAction::Disable => text.disable(),
                FolderBatchAction::Archive => text.archive(),
                FolderBatchAction::Restore => text.restore(),
                _ => text.folder_batch_action(action),
            }
        } else {
            text.folder_batch_action(action)
        };
        let icon = match action {
            FolderBatchAction::Enable => Icon::Check,
            FolderBatchAction::Disable => Icon::Ban,
            FolderBatchAction::Archive => Icon::Archive,
            FolderBatchAction::Restore => Icon::ArchiveRestore,
            FolderBatchAction::CheckUpdates => Icon::Search,
            FolderBatchAction::Update => Icon::RefreshCw,
            FolderBatchAction::MoveContents => Icon::FolderOpen,
            FolderBatchAction::RemoveFolders => Icon::Trash2,
            FolderBatchAction::DeleteContents | FolderBatchAction::DeleteFoldersAndContents => {
                Icon::Trash2
            }
        };
        let mut response = if action == FolderBatchAction::MoveContents && enabled {
            let menu = ui
                .menu_button(icon_text_sized(icon, label, 12.0, 12.0), |ui| {
                    ui.set_min_width(220.0);
                    ui.label(RichText::new(label).strong());
                    ui.separator();
                    if ui
                        .button(icon_text_sized(
                            Icon::FolderOpen,
                            text.uncategorized(),
                            12.0,
                            12.0,
                        ))
                        .on_hover_cursor(egui::CursorIcon::PointingHand)
                        .clicked()
                    {
                        self.start_folder_batch(action, None);
                        ui.close();
                    }
                    for category in self.categories_for_game(&targets.game_id) {
                        if targets.category_ids.contains(&category.id) {
                            continue;
                        }
                        if ui
                            .button(icon_text_sized(
                                Icon::FolderOpen,
                                &category.name,
                                12.0,
                                12.0,
                            ))
                            .on_hover_cursor(egui::CursorIcon::PointingHand)
                            .clicked()
                        {
                            self.start_folder_batch(action, Some(category.id.clone()));
                            ui.close();
                        }
                    }
                })
                .response;
            menu
        } else {
            let button = egui::Button::new(icon_text_sized(icon, label, 13.0, 13.0));
            if action == FolderBatchAction::Update {
                let button = button
                    .fill(Color32::from_rgb(180, 78, 35))
                    .stroke(egui::Stroke::new(1.0, Color32::from_rgb(203, 104, 59)));
                if compact {
                    folder_batch_sized_action_button(ui, enabled, 72.0, button)
                } else {
                    ui.add_enabled(enabled, button)
                }
            } else if compact {
                folder_batch_sized_action_button(ui, enabled, 72.0, button)
            } else {
                ui.add_enabled(enabled, button)
            }
        };
        response = response.on_hover_text(if busy {
            text.folder_batch_busy_tooltip().to_owned()
        } else {
            text.folder_batch_action_tooltip(action).to_owned()
        });
        if response.clicked() {
            if action != FolderBatchAction::MoveContents {
                self.start_folder_batch(action, None);
                ui.close();
            }
        }
    }

    fn render_folder_context_select_rows(&mut self, ui: &mut Ui) {
        let text = self.text();
        let all_selected = !self.library_visible_folder_ids.is_empty()
            && self
                .library_visible_folder_ids
                .iter()
                .all(|id| self.selected_library_folder_ids.contains(id));
        if ui
            .add_enabled(
                !all_selected,
                egui::Button::new(icon_text_sized(
                    Icon::ListChecks,
                    text.context_select_all(),
                    12.0,
                    12.0,
                )),
            )
            .on_hover_text(text.folder_select_all_tooltip())
            .on_hover_cursor(egui::CursorIcon::PointingHand)
            .clicked()
        {
            self.select_all_library_folders();
            ui.close();
        }
        if !self.selected_library_folder_ids.is_empty()
            && ui
                .button(icon_text_sized(
                    Icon::CircleX,
                    text.context_clear_selection(),
                    12.0,
                    12.0,
                ))
                .on_hover_text(text.folder_clear_selection_tooltip())
                .on_hover_cursor(egui::CursorIcon::PointingHand)
                .clicked()
        {
            self.clear_library_folder_selection();
            ui.close();
        }
    }

    fn render_folder_batch_status(&mut self, ui: &mut Ui) {
        let status_background = Color32::from_rgb(210, 189, 156);
        let status_ink = Color32::from_rgb(43, 38, 36);
        if let Some((done, total, show_counts)) = self.folder_batch_progress_state() {
            let text = self.text();
            let progress = if show_counts {
                text.folder_batch_progress(done, total)
            } else {
                text.folder_batch_working().to_owned()
            };
            egui::Frame::new()
                .fill(status_background)
                .corner_radius(egui::CornerRadius::same(4))
                .inner_margin(egui::Margin::symmetric(6, 3))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 4.0;
                        ui.spacing_mut().button_padding = egui::vec2(6.0, 2.0);
                        ui.spacing_mut().interact_size.y = 22.0;
                        let cancel_width = 24.0;
                        let label_width = (ui.available_width()
                            - cancel_width
                            - ui.spacing().item_spacing.x)
                            .max(0.0);
                        let response = ui.add_sized(
                            [label_width, 22.0],
                            egui::Label::new(RichText::new(&progress).size(11.0).color(status_ink))
                                .truncate(),
                        );
                        response.on_hover_text(progress);
                        let cancel = ui
                            .add_sized(
                                [cancel_width, 22.0],
                                egui::Button::new(icon_rich(Icon::CircleX, 12.0, status_ink))
                                    .frame(false),
                            )
                            .on_hover_text(text.cancel())
                            .on_hover_cursor(egui::CursorIcon::PointingHand);
                        if cancel.clicked() {
                            self.cancel_folder_batch();
                        }
                    });
                });
            // A running job owns the status row. Do not stack a stale report beneath it.
            return;
        }
        if let Some((completed, skipped, unsupported, changed_missing, failed, cancelled)) =
            self.folder_batch_report_summary()
        {
            let text = self.text();
            let show_counts = self.folder_batch_report().is_some_and(|report| {
                report.scope != FolderContentsScope::All
                    || self.state.static_prefs.unsafe_content_mode
                        != UnsafeContentMode::HideNoCounter
            });
            let skipped = skipped + unsupported + changed_missing;
            let summary = text.folder_batch_result_summary(
                completed,
                skipped,
                failed,
                cancelled,
                show_counts,
            );
            egui::Frame::new()
                .fill(status_background)
                .corner_radius(egui::CornerRadius::same(4))
                .inner_margin(egui::Margin::symmetric(6, 3))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 4.0;
                        ui.spacing_mut().button_padding = egui::vec2(6.0, 2.0);
                        ui.spacing_mut().interact_size.y = 22.0;
                        let close_width = 24.0;
                        let more_width = if show_counts {
                            let galley = ui.painter().layout_no_wrap(
                                text.folder_batch_report_details().to_owned(),
                                egui::TextStyle::Button.resolve(ui.style()),
                                status_ink,
                            );
                            galley.size().x
                                + 12.0
                                + ui.spacing().icon_spacing
                                + ui.spacing().button_padding.x * 2.0
                                + 2.0
                        } else {
                            0.0
                        };
                        let reserved = close_width
                            + more_width
                            + if show_counts {
                                ui.spacing().item_spacing.x
                            } else {
                                0.0
                            }
                            + ui.spacing().item_spacing.x;
                        let summary_width = (ui.available_width() - reserved).max(0.0);
                        let summary_response = ui.add_sized(
                            [summary_width, 22.0],
                            egui::Label::new(
                                RichText::new(&summary).size(11.0).color(status_ink),
                            )
                            .truncate(),
                        );
                        summary_response.on_hover_text(summary.clone());
                        if show_counts {
                            let mut details_job = icon_text_sized(
                                Icon::ListChecks,
                                text.folder_batch_report_details(),
                                12.0,
                                12.0,
                            );
                            for section in &mut details_job.sections {
                                section.format.color = status_ink;
                            }
                            let details = ui
                                .add_sized(
                                    [more_width, 22.0],
                                    egui::Button::new(details_job)
                                        .fill(status_background)
                                        .stroke(egui::Stroke::new(
                                            1.0,
                                            status_ink.gamma_multiply(0.35),
                                        )),
                                )
                                .on_hover_text(text.folder_batch_report_details())
                                .on_hover_cursor(egui::CursorIcon::PointingHand);
                            egui::Popup::menu(&details)
                                .id(ui.id().with("folder_batch_report_details"))
                                .width(300.0)
                                .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
                                .show(|ui| {
                                    let text = self.text();
                                    let queued_action = self.folder_batch_report().is_some_and(|report| {
                                        matches!(
                                            report.action,
                                            FolderBatchAction::CheckUpdates | FolderBatchAction::Update
                                        )
                                    });
                                    if let Some(items) = self.folder_batch_report_items() {
                                        ScrollArea::vertical()
                                            .id_salt("folder_batch_report_rows")
                                            .max_height(320.0)
                                            .show(ui, |ui| {
                                                for item in items {
                                                    let label = item.label;
                                                    let result_label = match item.result {
                                                        FolderBatchItemResult::Completed => {
                                                            if queued_action {
                                                                text.task_status_label(TaskStatus::Queued)
                                                                    .to_owned()
                                                            } else {
                                                                text.folder_batch_completed().to_owned()
                                                            }
                                                        }
                                                        FolderBatchItemResult::SkippedLocked => {
                                                            text.folder_batch_skipped_locked().to_owned()
                                                        }
                                                        FolderBatchItemResult::Unsupported => {
                                                            text.folder_batch_unsupported().to_owned()
                                                        }
                                                        FolderBatchItemResult::ChangedMissing => {
                                                            text.folder_batch_changed_missing().to_owned()
                                                        }
                                                        FolderBatchItemResult::Failed(error) => {
                                                            if self.state.static_prefs.unsafe_content_mode
                                                                != UnsafeContentMode::Show
                                                            {
                                                                text.folder_batch_failed().to_owned()
                                                            } else {
                                                                format!(
                                                                    "{}: {error}",
                                                                    text.folder_batch_failed()
                                                                )
                                                            }
                                                        }
                                                    };
                                                    ui.label(format!("{label}: {result_label}"));
                                                }
                                            });
                                    }
                                });
                        }
                        let close = ui
                            .add_sized(
                                [close_width, 22.0],
                                egui::Button::new(icon_rich(Icon::X, 12.0, status_ink))
                                    .frame(false),
                            )
                            .on_hover_text(text.close())
                            .on_hover_cursor(egui::CursorIcon::PointingHand);
                        if close.clicked() {
                            self.dismiss_folder_batch_report();
                        }
                    });
                });
        }
    }
}

fn show_folder_batch_delete_popup(
    ui: &mut Ui,
    response: &egui::Response,
    keyboard_request: bool,
    contents: impl FnOnce(&mut Ui),
) {
    let popup = egui::Popup::menu(response)
        .id(ui.id().with("folder_batch_delete_menu"))
        .width(180.0)
        .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
        .frame(menu_popup_frame(ui));
    let popup = if keyboard_request {
        popup.open_memory(egui::SetOpenCommand::Bool(true))
    } else {
        popup
    };
    popup.show(contents);
}

fn folder_batch_delete_button_text(label: &str) -> egui::text::LayoutJob {
    let mut job = icon_text_sized(Icon::Trash2, label, 12.0, 12.0);
    let chevron = icon_char(Icon::ChevronDown).to_string();
    job.append(
        "  ",
        0.0,
        TextFormat {
            font_id: egui::FontId::proportional(12.0),
            color: Color32::from_rgb(225, 229, 233),
            ..Default::default()
        },
    );
    job.append(
        &chevron,
        0.0,
        TextFormat {
            font_id: egui::FontId::new(12.0, FontFamily::Name(LUCIDE_FAMILY.into())),
            color: Color32::from_rgb(225, 229, 233),
            ..Default::default()
        },
    );
    job
}

fn folder_batch_sized_action_button(
    ui: &mut Ui,
    enabled: bool,
    width: f32,
    button: egui::Button<'_>,
) -> egui::Response {
    ui.allocate_ui_with_layout(
        egui::vec2(width, 28.0),
        egui::Layout::centered_and_justified(egui::Direction::LeftToRight),
        |ui| ui.add_enabled(enabled, button),
    )
    .inner
}

#[cfg(test)]
mod folder_batch_layout_tests {
    use super::*;

    fn popup_pointer_click_input(pos: egui::Pos2) -> egui::RawInput {
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(320.0, 220.0),
            )),
            events: vec![
                egui::Event::PointerMoved(pos),
                egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: egui::Modifiers::NONE,
                },
                egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed: false,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
            ..Default::default()
        }
    }

    fn run_delete_popup_frame(
        context: &egui::Context,
        input: egui::RawInput,
        enabled: bool,
        keyboard_request: bool,
    ) -> (egui::Rect, bool) {
        let mut button_rect = egui::Rect::NOTHING;
        let mut popup_open = false;
        context.run_ui(input, |ui| {
            let response = folder_batch_sized_action_button(
                ui,
                enabled,
                90.0,
                egui::Button::new("Delete"),
            );
            button_rect = response.rect;
            show_folder_batch_delete_popup(ui, &response, keyboard_request, |ui| {
                popup_open = true;
                ui.label("Delete choice");
            });
        })
        .drop_without_applying_deltas();
        (button_rect, popup_open)
    }

    #[test]
    fn delete_popup_toggles_outside_closes_keyboard_opens_and_disabled_stays_closed() {
        let context = egui::Context::default();
        let (button_rect, popup_open) = run_delete_popup_frame(
            &context,
            egui::RawInput::default(),
            true,
            false,
        );
        assert!(!popup_open);

        let (_, popup_open) = run_delete_popup_frame(
            &context,
            popup_pointer_click_input(button_rect.center()),
            true,
            false,
        );
        assert!(popup_open);
        let (_, popup_open) = run_delete_popup_frame(
            &context,
            egui::RawInput::default(),
            true,
            false,
        );
        assert!(popup_open);

        let (_, popup_open) = run_delete_popup_frame(
            &context,
            popup_pointer_click_input(button_rect.center()),
            true,
            false,
        );
        assert!(!popup_open);
        let (_, popup_open) = run_delete_popup_frame(
            &context,
            egui::RawInput::default(),
            true,
            false,
        );
        assert!(!popup_open);

        let (_, popup_open) = run_delete_popup_frame(
            &context,
            popup_pointer_click_input(button_rect.center()),
            true,
            false,
        );
        assert!(popup_open);
        let (_, popup_open) = run_delete_popup_frame(
            &context,
            popup_pointer_click_input(egui::pos2(280.0, 180.0)),
            true,
            false,
        );
        assert!(popup_open);
        let (_, popup_open) = run_delete_popup_frame(
            &context,
            egui::RawInput::default(),
            true,
            false,
        );
        assert!(!popup_open);

        let (_, popup_open) = run_delete_popup_frame(
            &context,
            egui::RawInput::default(),
            true,
            true,
        );
        assert!(popup_open);

        let disabled_context = egui::Context::default();
        let (disabled_rect, popup_open) = run_delete_popup_frame(
            &disabled_context,
            egui::RawInput::default(),
            false,
            false,
        );
        assert!(!popup_open);
        let (_, popup_open) = run_delete_popup_frame(
            &disabled_context,
            popup_pointer_click_input(disabled_rect.center()),
            false,
            false,
        );
        assert!(!popup_open);
    }

    #[test]
    fn action_tiles_wrap_at_outer_row_and_preserve_disabled_state() {
        let context = egui::Context::default();
        let mut input = egui::RawInput::default();
        input.screen_rect = Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(180.0, 180.0),
        ));
        let mut responses = Vec::new();

        context.run_ui(input, |ui| {
            ui.horizontal_wrapped(|ui| {
                responses.push(folder_batch_sized_action_button(
                    ui,
                    true,
                    72.0,
                    egui::Button::new("Enable"),
                ));
                responses.push(folder_batch_sized_action_button(
                    ui,
                    false,
                    72.0,
                    egui::Button::new("Disable"),
                ));
                responses.push(folder_batch_sized_action_button(
                    ui,
                    true,
                    112.0,
                    egui::Button::new("Move contents"),
                ));
                responses.push(folder_batch_sized_action_button(
                    ui,
                    true,
                    120.0,
                    egui::Button::new("Delete contents"),
                ));
            });
        })
        .drop_without_applying_deltas();

        assert_eq!(responses.len(), 4);
        assert_eq!(responses[0].rect.min.y, responses[1].rect.min.y);
        assert!(responses[2].rect.min.y > responses[1].rect.min.y);
        assert!(responses[2].rect.min.x <= responses[0].rect.min.x + 1.0);
        assert!(responses[3].rect.min.y > responses[2].rect.min.y);
        assert!(responses[3].rect.min.x <= responses[0].rect.min.x + 1.0);
        assert!(responses
            .iter()
            .all(|response| response.rect.right() <= 180.0));
        assert!(responses[0].enabled());
        assert!(!responses[1].enabled());
    }
}
