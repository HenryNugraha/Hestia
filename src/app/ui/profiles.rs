fn clamp_profile_switcher_name(
    ui: &Ui,
    name: &str,
    font_id: &egui::FontId,
    max_width: f32,
) -> String {
    let measure = |value: &str| {
        ui.painter()
            .layout_no_wrap(value.to_string(), font_id.clone(), Color32::WHITE)
            .size()
            .x
    };
    if measure(name) <= max_width {
        return name.to_string();
    }

    let chars = name.chars().collect::<Vec<_>>();
    let mut low = 0usize;
    let mut high = chars.len();
    while low < high {
        let mid = (low + high).div_ceil(2);
        let candidate = format!("{}…", chars[..mid].iter().collect::<String>());
        if measure(&candidate) <= max_width {
            low = mid;
        } else {
            high = mid - 1;
        }
    }
    format!("{}…", chars[..low].iter().collect::<String>())
}

fn profile_switcher_button(
    ui: &mut Ui,
    profile_label: &str,
    profile_name: &str,
    width: f32,
    pane_top: Option<f32>,
) -> egui::Response {
    let (id, slot_rect) = ui.allocate_space(Vec2::new(width.max(1.0), 64.0));
    let control_rect = profile_switcher_control_rect(slot_rect, pane_top);
    let response = ui.interact(control_rect, id, Sense::click());
    let hovered = response.hovered();
    ui.painter().rect(
        control_rect,
        egui::CornerRadius {
            nw: 10,
            ne: 10,
            sw: 0,
            se: 0,
        },
        if hovered {
            Color32::from_rgba_premultiplied(48, 52, 57, 242)
        } else {
            Color32::from_rgba_premultiplied(36, 39, 43, 238)
        },
        egui::Stroke::new(
            1.0,
            if hovered {
                Color32::from_rgb(86, 92, 100)
            } else {
                Color32::from_rgb(65, 70, 77)
            },
        ),
        egui::StrokeKind::Inside,
    );

    let icon_font = egui::FontId::new(17.0, FontFamily::Name(LUCIDE_FAMILY.into()));
    let label_font = egui::FontId::proportional(12.5);
    let name_font = egui::FontId::proportional(13.0);
    let icon_center = egui::pos2(control_rect.left() + 18.0, control_rect.center().y);
    ui.painter().text(
        icon_center,
        egui::Align2::CENTER_CENTER,
        icon_char(Icon::Users),
        icon_font.clone(),
        Color32::from_rgb(188, 195, 203),
    );

    let prefix = format!("{profile_label}:");
    let prefix_pos = egui::pos2(control_rect.left() + 34.0, control_rect.center().y);
    let prefix_galley = ui.painter().layout_no_wrap(
        prefix.clone(),
        label_font.clone(),
        Color32::from_rgb(167, 173, 181),
    );
    ui.painter().galley(
        egui::pos2(prefix_pos.x, prefix_pos.y - prefix_galley.size().y * 0.5),
        prefix_galley.clone(),
        Color32::from_rgb(167, 173, 181),
    );

    let chevron_x = control_rect.right() - 16.0;
    let name_x = prefix_pos.x + prefix_galley.size().x + 6.0;
    let available_name_width = (chevron_x - 14.0 - name_x).max(1.0);
    let visible_name =
        clamp_profile_switcher_name(ui, profile_name, &name_font, available_name_width);
    ui.painter().text(
        egui::pos2(name_x, control_rect.center().y),
        egui::Align2::LEFT_CENTER,
        visible_name,
        name_font,
        if hovered {
            Color32::from_rgb(244, 246, 248)
        } else {
            Color32::from_rgb(224, 228, 233)
        },
    );
    ui.painter().text(
        egui::pos2(chevron_x, control_rect.center().y),
        egui::Align2::CENTER_CENTER,
        icon_char(Icon::ChevronDown),
        egui::FontId::new(15.0, FontFamily::Name(LUCIDE_FAMILY.into())),
        Color32::from_rgb(171, 177, 185),
    );

    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}

fn profile_switcher_control_rect(slot_rect: egui::Rect, pane_top: Option<f32>) -> egui::Rect {
    const CONTROL_HEIGHT: f32 = 42.0;
    let default_bottom = slot_rect.center().y + CONTROL_HEIGHT * 0.5;
    let bottom = pane_top
        .filter(|value| value.is_finite())
        .unwrap_or(default_bottom);
    egui::Rect::from_min_max(
        egui::pos2(slot_rect.left(), bottom - CONTROL_HEIGHT),
        egui::pos2(slot_rect.right(), bottom),
    )
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ProfileTimelineStage {
    Archiving,
    Extracting,
    Activating,
}

const PROFILE_TIMELINE_CIRCLE_X: f32 = 20.0;

fn profile_progress_footer_rects(
    footer_rect: egui::Rect,
    cancel_width: f32,
) -> (egui::Rect, egui::Rect) {
    const CANCEL_HEIGHT: f32 = 30.0;
    const FOOTER_GAP: f32 = 16.0;

    let cancel_rect = egui::Rect::from_min_size(
        egui::pos2(footer_rect.right() - cancel_width, footer_rect.top()),
        Vec2::new(cancel_width, CANCEL_HEIGHT),
    );
    let note_rect = egui::Rect::from_min_max(
        footer_rect.min,
        egui::pos2(cancel_rect.left() - FOOTER_GAP, footer_rect.bottom()),
    );
    (note_rect, cancel_rect)
}

fn profile_timeline_stages(
    kind: ProfileOperationKind,
    prepares_before_activating: bool,
) -> &'static [ProfileTimelineStage] {
    const ACTIVATE_ONLY: &[ProfileTimelineStage] = &[ProfileTimelineStage::Activating];
    const PREPARE_FIRST: &[ProfileTimelineStage] = &[
        ProfileTimelineStage::Extracting,
        ProfileTimelineStage::Activating,
    ];

    match kind {
        ProfileOperationKind::Create => ACTIVATE_ONLY,
        ProfileOperationKind::Duplicate | ProfileOperationKind::Switch
            if prepares_before_activating =>
        {
            PREPARE_FIRST
        }
        _ => ACTIVATE_ONLY,
    }
}

fn profile_timeline_active_stage(stage: &str) -> Option<ProfileTimelineStage> {
    if stage.contains("Archiving") || stage.contains("Current profile archived") {
        Some(ProfileTimelineStage::Archiving)
    } else if stage.contains("Preparing selected")
        || stage.contains("Extracting")
        || stage.contains("Selected profile prepared")
    {
        Some(ProfileTimelineStage::Extracting)
    } else if stage.contains("Committing")
        || stage.contains("committed")
        || stage.contains("Activating")
    {
        Some(ProfileTimelineStage::Activating)
    } else {
        None
    }
}

fn profile_timeline_stage_progress(
    timeline_stage: ProfileTimelineStage,
    progress: u8,
    worker_stage: &str,
) -> f32 {
    let progress = progress as f32;
    match timeline_stage {
        ProfileTimelineStage::Archiving => {
            if worker_stage.contains("Current profile archived") {
                1.0
            } else {
                ((progress - 10.0) / 35.0).clamp(0.0, 1.0)
            }
        }
        ProfileTimelineStage::Extracting => {
            if worker_stage.contains("Selected profile prepared") {
                1.0
            } else {
                ((progress - 10.0) / 50.0).clamp(0.0, 1.0)
            }
        }
        ProfileTimelineStage::Activating => {
            if worker_stage.contains("committed") {
                1.0
            } else {
                ((progress - 70.0) / 25.0).clamp(0.0, 1.0)
            }
        }
    }
}

fn profile_progress_text(text: &str) -> &str {
    text.strip_suffix('…').unwrap_or(text)
}

impl HestiaApp {
    fn selected_game_profile_catalog(&self) -> Option<&ProfileCatalog> {
        let game_id = &self.selected_game()?.definition.id;
        self.state.profiles_by_game.get(game_id)
    }

    fn active_profile_name(&self) -> String {
        let Some(catalog) = self.selected_game_profile_catalog() else {
            return self.text().default_profile().to_string();
        };
        catalog
            .active_profile_id
            .and_then(|active_id| {
                catalog
                    .profiles
                    .iter()
                    .find(|profile| profile.id == active_id)
            })
            .map(|profile| self.text().profile_display_name(&profile.display_name))
            .unwrap_or_else(|| self.text().default_profile().to_string())
    }

    fn next_profile_name(&self, base: &str) -> String {
        let Some(catalog) = self.selected_game_profile_catalog() else {
            return base.to_string();
        };
        if !catalog
            .profiles
            .iter()
            .any(|profile| TextCatalog::profile_names_equal(&profile.display_name, base))
        {
            return base.to_string();
        }
        for suffix in 2.. {
            let candidate = format!("{base} {suffix}");
            if !catalog
                .profiles
                .iter()
                .any(|profile| TextCatalog::profile_names_equal(&profile.display_name, &candidate))
            {
                return candidate;
            }
        }
        unreachable!()
    }

    fn start_profile_name_prompt(
        &mut self,
        kind: ProfileOperationKind,
        target_id: Option<Uuid>,
        suggested_name: String,
    ) {
        self.profile_name_prompt = Some(kind);
        self.profile_name_target_id = target_id;
        self.profile_name_draft = suggested_name;
    }

    fn clear_profile_name_prompt(&mut self) {
        self.profile_name_prompt = None;
        self.profile_name_target_id = None;
        self.profile_name_draft.clear();
    }

    fn render_profile_titlebar_action(&mut self, ui: &mut Ui, width: f32) {
        let text = self.text();
        let profile_catalog_ready = self.selected_game_profile_catalog().is_some_and(|catalog| {
            catalog.active_profile_id.is_some() && !catalog.profiles.is_empty()
        });
        if self.selected_game().is_some()
            && !profile_catalog_ready
            && self.profile_operation_inflight.is_none()
        {
            if let Err(error) = self.ensure_selected_game_default_profile() {
                self.report_error_message(
                    format!("failed to initialize profiles: {error:#}"),
                    Some(text.profile_operation_failed()),
                );
            }
        }

        let active_name = self.active_profile_name();
        let response = profile_switcher_button(
            ui,
            text.profile_label(),
            &active_name,
            width,
            self.last_right_pane_rect.map(|rect| rect.top()),
        );
        response
            .clone()
            .on_hover_text(format!(
                "{}: {}\n{}",
                text.profile_label(),
                active_name,
                text.switch_profile()
            ))
            .on_hover_cursor(egui::CursorIcon::PointingHand);

        let menu_radius = ui.style().visuals.menu_corner_radius;
        let menu_frame = egui::Frame::popup(ui.style()).corner_radius(egui::CornerRadius {
            nw: 0,
            ne: 0,
            sw: menu_radius.sw,
            se: menu_radius.se,
        });
        egui::Popup::menu(&response)
            .id(ui.id().with("profile_selector_popup"))
            .width(response.rect.width())
            .frame(menu_frame)
            .show(|ui| {
                self.render_profile_selector_popup(ui);
            });
    }

    fn render_profile_selector_popup(&mut self, ui: &mut Ui) {
        let text = self.text();
        let Some(game_id) = self.selected_game().map(|game| game.definition.id.clone()) else {
            static_label(ui, RichText::new(text.profile_select_game()));
            return;
        };

        let catalog = self
            .state
            .profiles_by_game
            .get(&game_id)
            .cloned()
            .unwrap_or_default();
        let blocked = self.profile_operations_blocked();

        let profile_count = catalog.profiles.len();
        for profile in catalog.profiles {
            let active = catalog.active_profile_id == Some(profile.id);
            let profile_name = text.profile_display_name(&profile.display_name);
            let row_width = ui.available_width().max(1.0);
            let (row_rect, _) = ui.allocate_exact_size(Vec2::new(row_width, 34.0), Sense::hover());
            let menu_rect = egui::Rect::from_min_max(
                egui::pos2(row_rect.right() - 32.0, row_rect.top() + 4.0),
                egui::pos2(row_rect.right() - 4.0, row_rect.bottom() - 4.0),
            );
            let select_rect = egui::Rect::from_min_max(
                row_rect.min,
                egui::pos2(menu_rect.left(), row_rect.bottom()),
            );
            let select = ui.interact(
                select_rect,
                ui.id().with(("profile_row", profile.id)),
                if active || blocked {
                    Sense::hover()
                } else {
                    Sense::click()
                },
            );
            let row_hovered = ui
                .ctx()
                .pointer_latest_pos()
                .is_some_and(|position| row_rect.contains(position));
            if active || row_hovered {
                ui.painter().rect_filled(
                    row_rect,
                    egui::CornerRadius::same(7),
                    if active {
                        Color32::from_rgba_premultiplied(61, 66, 73, 220)
                    } else {
                        Color32::from_rgba_premultiplied(44, 47, 52, 205)
                    },
                );
            }

            ui.painter().text(
                egui::pos2(row_rect.left() + 15.0, row_rect.center().y),
                egui::Align2::CENTER_CENTER,
                if active { "●" } else { "○" },
                egui::FontId::proportional(13.0),
                if active {
                    Color32::from_rgb(112, 164, 118)
                } else {
                    Color32::from_rgb(145, 151, 159)
                },
            );
            let name_font = egui::FontId::proportional(13.0);
            let name_width = (select_rect.right() - row_rect.left() - 42.0).max(1.0);
            let visible_name =
                clamp_profile_switcher_name(ui, &profile_name, &name_font, name_width);
            ui.painter().text(
                egui::pos2(row_rect.left() + 31.0, row_rect.center().y),
                egui::Align2::LEFT_CENTER,
                &visible_name,
                name_font,
                if active {
                    Color32::from_rgb(239, 242, 245)
                } else {
                    Color32::from_rgb(218, 222, 227)
                },
            );

            if visible_name != profile_name {
                select.clone().on_hover_text(&profile_name);
            }
            if select.clicked() {
                if let Err(error) = self.request_switch_profile(profile.id) {
                    self.report_error_message(
                        format!("failed to switch profile: {error:#}"),
                        Some(text.profile_operation_failed()),
                    );
                }
                egui::Popup::close_all(ui.ctx());
            } else if !active && !blocked {
                select
                    .clone()
                    .on_hover_cursor(egui::CursorIcon::PointingHand);
            } else if blocked && !active {
                select
                    .clone()
                    .on_hover_text(text.profile_finish_current_operation_first())
                    .on_hover_cursor(egui::CursorIcon::NotAllowed);
            }

            let mut menu_ui = ui.new_child(
                egui::UiBuilder::new()
                    .max_rect(menu_rect)
                    .layout(egui::Layout::top_down(egui::Align::Center)),
            );
            menu_ui.style_mut().spacing.button_padding = egui::vec2(5.0, 3.0);
            menu_ui.menu_button("", |ui| {
                let rename = ui
                    .add_enabled(
                        !blocked,
                        egui::Button::new(icon_text_sized(Icon::Pencil, text.rename(), 14.0, 13.0)),
                    )
                    .on_hover_text(text.rename());
                if rename.clicked() {
                    self.start_profile_name_prompt(
                        ProfileOperationKind::Rename,
                        Some(profile.id),
                        profile_name.clone(),
                    );
                    egui::Popup::close_all(ui.ctx());
                } else if blocked {
                    rename
                        .on_disabled_hover_text(text.profile_finish_current_operation_first())
                        .on_hover_cursor(egui::CursorIcon::NotAllowed);
                }

                let delete = ui
                    .add_enabled(
                        !blocked && !active && profile_count > 1,
                        egui::Button::new(icon_text_sized(Icon::Trash2, text.delete(), 14.0, 13.0)),
                    )
                    .on_hover_text(text.delete());
                if delete.clicked() {
                    self.pending_profile_delete_id = Some(profile.id);
                    egui::Popup::close_all(ui.ctx());
                } else if active {
                    delete
                        .on_disabled_hover_text(text.profile_switch_before_delete())
                        .on_hover_cursor(egui::CursorIcon::NotAllowed);
                } else if profile_count <= 1 {
                    delete
                        .on_disabled_hover_text(text.profile_at_least_one_required())
                        .on_hover_cursor(egui::CursorIcon::NotAllowed);
                } else if blocked {
                    delete
                        .on_disabled_hover_text(text.profile_finish_current_operation_first())
                        .on_hover_cursor(egui::CursorIcon::NotAllowed);
                }
            });
        }

        ui.separator();
        ui.add_enabled_ui(!blocked, |ui| {
            if ui
                .button(icon_text_sized(
                    Icon::Plus,
                    text.create_empty_profile(),
                    14.0,
                    13.0,
                ))
                .clicked()
            {
                let name = self.next_profile_name(text.new_profile());
                self.start_profile_name_prompt(ProfileOperationKind::Create, None, name);
                ui.close();
            }
            if ui
                .button(icon_text_sized(
                    Icon::Copy,
                    text.duplicate_current_profile(),
                    14.0,
                    13.0,
                ))
                .clicked()
            {
                let name = self.next_profile_name(&self.active_profile_name());
                self.start_profile_name_prompt(ProfileOperationKind::Duplicate, None, name);
                ui.close();
            }
        });
        if blocked {
            static_label(
                ui,
                RichText::new(text.profile_finish_current_operation_first())
                    .size(12.0)
                    .color(Color32::from_gray(145)),
            );
        }
    }

    fn render_profile_dialogs(&mut self, ctx: &egui::Context) {
        if self.profile_operation_locks_app() {
            self.render_profile_progress_dialog(ctx);
            return;
        }
        self.render_profile_name_dialog(ctx);
        self.render_profile_delete_dialog(ctx);
        self.render_profile_progress_dialog(ctx);
    }

    fn render_profile_name_dialog(&mut self, ctx: &egui::Context) {
        let Some(kind) = self.profile_name_prompt else {
            return;
        };
        let text = self.text();
        let title = match kind {
            ProfileOperationKind::Rename => text.rename_profile(),
            ProfileOperationKind::Duplicate => text.duplicate_current_profile(),
            _ => text.new_profile(),
        };
        let mut submit = false;
        let mut cancel = false;
        let constrain_rect = self
            .last_right_pane_rect
            .unwrap_or_else(|| ctx.viewport_rect());
        egui::Window::new(title)
            .id(egui::Id::new("profile_name_dialog"))
            .default_pos(constrain_rect.min + egui::vec2(16.0, 16.0))
            .order(egui::Order::Foreground)
            .resizable(false)
            .collapsible(false)
            .default_width(380.0)
            .constrain_to(constrain_rect)
            .show(ctx, |ui| {
                static_label(ui, RichText::new(text.profile_name()));
                let edit = ui.add(
                    TextEdit::singleline(&mut self.profile_name_draft).desired_width(f32::INFINITY),
                );
                if edit.gained_focus() {
                    edit.request_focus();
                }
                submit = edit.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter));
                cancel = ui.input(|input| input.key_pressed(egui::Key::Escape));
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui.button(text.cancel()).clicked() {
                        cancel = true;
                    }
                    let action_label = match kind {
                        ProfileOperationKind::Rename => text.rename_profile(),
                        ProfileOperationKind::Duplicate => text.duplicate_current_profile(),
                        _ => text.create_empty_profile(),
                    };
                    if ui
                        .add_enabled(
                            !self.profile_name_draft.trim().is_empty(),
                            egui::Button::new(action_label),
                        )
                        .clicked()
                    {
                        submit = true;
                    }
                });
            });

        if cancel {
            self.clear_profile_name_prompt();
        } else if submit {
            let name = self.profile_name_draft.trim().to_string();
            let result = match kind {
                ProfileOperationKind::Create => self.request_create_empty_profile(name),
                ProfileOperationKind::Duplicate => self.request_duplicate_current_profile(name),
                ProfileOperationKind::Rename => self
                    .profile_name_target_id
                    .ok_or_else(|| anyhow!("profile rename target is missing"))
                    .and_then(|profile_id| self.request_rename_profile(profile_id, name)),
                _ => Err(anyhow!("unsupported profile name operation")),
            };
            match result {
                Ok(()) => self.clear_profile_name_prompt(),
                Err(error) => self.report_error_message(
                    format!("profile operation failed: {error:#}"),
                    Some(text.profile_operation_failed()),
                ),
            }
        }
    }

    fn render_profile_delete_dialog(&mut self, ctx: &egui::Context) {
        let Some(profile_id) = self.pending_profile_delete_id else {
            return;
        };
        let text = self.text();
        let profile_name = self
            .selected_game_profile_catalog()
            .and_then(|catalog| {
                catalog
                    .profiles
                    .iter()
                    .find(|profile| profile.id == profile_id)
            })
            .map(|profile| text.profile_display_name(&profile.display_name));
        let Some(profile_name) = profile_name else {
            self.pending_profile_delete_id = None;
            return;
        };
        let mut delete = false;
        let mut cancel = false;
        egui::Window::new(text.delete_profile())
            .id(egui::Id::new("profile_delete_dialog"))
            .order(egui::Order::Foreground)
            .resizable(false)
            .collapsible(false)
            .default_width(440.0)
            .show(ctx, |ui| {
                static_label(
                    ui,
                    RichText::new(text.profile_delete_confirmation(&profile_name))
                        .size(16.0)
                        .strong(),
                );
                static_label(
                    ui,
                    RichText::new(text.profile_delete_confirmation_details())
                        .color(Color32::from_gray(170)),
                );
                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    if ui.button(text.cancel()).clicked() {
                        cancel = true;
                    }
                    if ui
                        .button(icon_text_sized(
                            Icon::Trash2,
                            text.delete_profile(),
                            14.0,
                            13.0,
                        ))
                        .clicked()
                    {
                        delete = true;
                    }
                });
            });
        if cancel {
            self.pending_profile_delete_id = None;
        } else if delete {
            match self.request_delete_profile(profile_id) {
                Ok(()) => self.pending_profile_delete_id = None,
                Err(error) => self.report_error_message(
                    format!("failed to delete profile: {error:#}"),
                    Some(text.profile_operation_failed()),
                ),
            }
        }
    }

    fn render_profile_progress_dialog(&mut self, ctx: &egui::Context) {
        let Some(inflight) = self.profile_operation_inflight.clone() else {
            return;
        };
        if inflight.kind == ProfileOperationKind::Recover {
            return;
        }
        let text = self.text();
        let kind = inflight.kind;
        let (progress, stage) = self
            .profile_operation_progress()
            .unwrap_or_else(|| (0, String::new()));
        let label = if stage.contains("Archiving") {
            text.archiving_current_profile()
        } else if stage.contains("Extracting") {
            text.extracting_selected_profile()
        } else if stage.contains("Committing") || stage.contains("Activating") {
            text.activating_selected_profile()
        } else {
            match kind {
                ProfileOperationKind::Create => text.creating_profile(),
                ProfileOperationKind::Duplicate => text.duplicating_profile(),
                ProfileOperationKind::Delete => text.deleting_profile(),
                _ => text.switching_profile(),
            }
        };
        let commit_started = stage.contains("Activating") || stage.contains("committed");
        let cancel_requested = inflight.cancel.load(Ordering::Relaxed);
        let app_locked = self.profile_operation_locks_app();
        let cancel = if app_locked {
            let title = match kind {
                ProfileOperationKind::Create => text.creating_profile(),
                ProfileOperationKind::Duplicate => text.duplicating_profile(),
                _ => text.switching_profile(),
            };
            let source_name = inflight
                .source_display_name
                .as_deref()
                .map(|name| text.profile_display_name(name))
                .unwrap_or_else(|| self.active_profile_name());
            let target_name = inflight
                .target_display_name
                .as_deref()
                .map(|name| text.profile_display_name(name))
                .unwrap_or_else(|| self.active_profile_name());
            let timeline_stages =
                profile_timeline_stages(kind, inflight.prepares_before_activating);
            let active_stage = profile_timeline_active_stage(&stage).unwrap_or(timeline_stages[0]);
            let active_index = timeline_stages
                .iter()
                .position(|candidate| *candidate == active_stage)
                .unwrap_or(0);
            let modal_frame = egui::Frame::popup(&ctx.style_of(ctx.theme()))
                .inner_margin(egui::Margin::symmetric(28, 24))
                .corner_radius(egui::CornerRadius::same(7))
                .fill(Color32::from_rgb(29, 31, 35))
                .stroke(egui::Stroke::new(1.0, Color32::from_rgb(72, 77, 85)));
            let response = egui::Modal::new(egui::Id::new("profile_progress_modal"))
                .backdrop_color(Color32::from_black_alpha(200))
                .frame(modal_frame)
                .show(ctx, |ui| {
                    const MODAL_WIDTH: f32 = 500.0;
                    const TIMELINE_ROW_HEIGHT: f32 = 76.0;
                    const TIMELINE_LABEL_X: f32 = 58.0;
                    const ACCENT: Color32 = Color32::from_rgb(214, 104, 58);

                    ui.set_width(MODAL_WIDTH);
                    static_label(
                        ui,
                        RichText::new(profile_progress_text(title))
                            .size(20.0)
                            .strong()
                            .color(Color32::from_rgb(240, 242, 245)),
                    );
                    ui.add_space(5.0);
                    let route_font = egui::FontId::proportional(13.0);
                    let profile_name_width = (MODAL_WIDTH - 56.0) * 0.5;
                    let source_name = clamp_profile_switcher_name(
                        ui,
                        &source_name,
                        &route_font,
                        profile_name_width,
                    );
                    let target_name = clamp_profile_switcher_name(
                        ui,
                        &target_name,
                        &route_font,
                        profile_name_width,
                    );
                    static_label(
                        ui,
                        RichText::new(format!("{source_name}  →  {target_name}"))
                            .size(13.0)
                            .color(Color32::from_rgb(163, 169, 178)),
                    );
                    ui.add_space(22.0);

                    let timeline_height = TIMELINE_ROW_HEIGHT * timeline_stages.len() as f32 - 12.0;
                    let (timeline_rect, _) = ui.allocate_exact_size(
                        Vec2::new(MODAL_WIDTH, timeline_height),
                        Sense::hover(),
                    );
                    let painter = ui.painter().clone();
                    let first_center =
                        timeline_rect.min + egui::vec2(PROFILE_TIMELINE_CIRCLE_X, 18.0);

                    for index in 0..timeline_stages.len().saturating_sub(1) {
                        let start_y = first_center.y + index as f32 * TIMELINE_ROW_HEIGHT + 15.0;
                        let end_y =
                            first_center.y + (index + 1) as f32 * TIMELINE_ROW_HEIGHT - 15.0;
                        painter.line_segment(
                            [
                                egui::pos2(first_center.x, start_y),
                                egui::pos2(first_center.x, end_y),
                            ],
                            egui::Stroke::new(
                                1.0,
                                if index < active_index {
                                    ACCENT.gamma_multiply(0.7)
                                } else {
                                    Color32::from_rgb(74, 79, 87)
                                },
                            ),
                        );
                    }

                    for (index, timeline_stage) in timeline_stages.iter().enumerate() {
                        let center = egui::pos2(
                            first_center.x,
                            first_center.y + index as f32 * TIMELINE_ROW_HEIGHT,
                        );
                        let completed = index < active_index;
                        let active = index == active_index;
                        if completed {
                            painter.circle_filled(center, 14.0, ACCENT.gamma_multiply(0.72));
                            painter.text(
                                center,
                                egui::Align2::CENTER_CENTER,
                                icon_char(Icon::Check),
                                egui::FontId::new(14.0, FontFamily::Name(LUCIDE_FAMILY.into())),
                                Color32::WHITE,
                            );
                        } else {
                            painter.circle_stroke(
                                center,
                                14.0,
                                egui::Stroke::new(
                                    if active { 1.8 } else { 1.2 },
                                    if active {
                                        ACCENT
                                    } else {
                                        Color32::from_rgb(103, 109, 118)
                                    },
                                ),
                            );
                            if active {
                                let icon = match timeline_stage {
                                    ProfileTimelineStage::Archiving => Icon::Archive,
                                    ProfileTimelineStage::Extracting => Icon::PackageOpen,
                                    ProfileTimelineStage::Activating => Icon::Check,
                                };
                                painter.text(
                                    center,
                                    egui::Align2::CENTER_CENTER,
                                    icon_char(icon),
                                    egui::FontId::new(14.0, FontFamily::Name(LUCIDE_FAMILY.into())),
                                    ACCENT,
                                );
                            }
                        }

                        let row_label = match timeline_stage {
                            ProfileTimelineStage::Archiving => text.archiving_current_profile(),
                            ProfileTimelineStage::Extracting => text.extracting_selected_profile(),
                            ProfileTimelineStage::Activating => text.activating_selected_profile(),
                        };
                        let label_color = if active {
                            Color32::from_rgb(238, 240, 243)
                        } else if completed {
                            Color32::from_rgb(187, 192, 199)
                        } else {
                            Color32::from_rgb(148, 154, 163)
                        };
                        let label_pos =
                            egui::pos2(timeline_rect.left() + TIMELINE_LABEL_X, center.y - 1.0);
                        let label_font = egui::FontId::proportional(14.0);
                        let row_label = profile_progress_text(row_label);
                        let label_width = painter
                            .layout_no_wrap(row_label.to_string(), label_font.clone(), label_color)
                            .size()
                            .x;
                        painter.text(
                            label_pos,
                            egui::Align2::LEFT_CENTER,
                            row_label,
                            label_font,
                            label_color,
                        );

                        if active {
                            let spinner_rect = egui::Rect::from_center_size(
                                egui::pos2(label_pos.x + label_width + 14.0, label_pos.y),
                                Vec2::splat(14.0),
                            );
                            let mut spinner_ui =
                                ui.new_child(egui::UiBuilder::new().max_rect(spinner_rect).layout(
                                    egui::Layout::centered_and_justified(
                                        egui::Direction::LeftToRight,
                                    ),
                                ));
                            spinner_ui.add(egui::Spinner::new().size(13.0));
                            let stage_progress =
                                profile_timeline_stage_progress(*timeline_stage, progress, &stage);
                            painter.text(
                                egui::pos2(timeline_rect.right(), center.y - 1.0),
                                egui::Align2::RIGHT_CENTER,
                                format!("{:.0}%", stage_progress * 100.0),
                                egui::FontId::proportional(13.0),
                                Color32::from_rgb(183, 188, 195),
                            );
                            let track = egui::Rect::from_min_max(
                                egui::pos2(label_pos.x, center.y + 19.0),
                                egui::pos2(timeline_rect.right(), center.y + 23.0),
                            );
                            painter.rect_filled(
                                track,
                                egui::CornerRadius::same(2),
                                Color32::from_rgb(65, 69, 76),
                            );
                            let fill = egui::Rect::from_min_max(
                                track.min,
                                egui::pos2(
                                    track.left() + track.width() * stage_progress,
                                    track.bottom(),
                                ),
                            );
                            painter.rect_filled(fill, egui::CornerRadius::same(2), ACCENT);
                        }
                    }

                    ui.add_space(13.0);
                    ui.separator();
                    ui.add_space(3.0);
                    let cancel_label = if cancel_requested {
                        text.task_canceling()
                    } else {
                        text.cancel()
                    };
                    let cancel_font = egui::TextStyle::Button.resolve(ui.style());
                    let cancel_width = (ui
                        .painter()
                        .layout_no_wrap(cancel_label.to_string(), cancel_font, Color32::PLACEHOLDER)
                        .size()
                        .x
                        + ui.spacing().button_padding.x * 2.0
                        + 4.0)
                        .max(72.0);
                    let (footer_rect, _) =
                        ui.allocate_exact_size(Vec2::new(MODAL_WIDTH, 38.0), Sense::hover());
                    let (note_rect, cancel_rect) =
                        profile_progress_footer_rects(footer_rect, cancel_width);
                    let mut note_ui = ui.new_child(
                        egui::UiBuilder::new()
                            .max_rect(note_rect)
                            .layout(egui::Layout::top_down(egui::Align::Min)),
                    );
                    note_ui
                        .add(
                            egui::Label::new(
                                RichText::new(text.inactive_profiles_compressed_note())
                                    .size(12.0)
                                    .color(Color32::from_rgb(143, 149, 158)),
                            )
                            .wrap()
                            .halign(egui::Align::LEFT)
                            .selectable(false),
                        )
                        .on_hover_cursor(egui::CursorIcon::Default);

                    let mut cancel_ui =
                        ui.new_child(egui::UiBuilder::new().max_rect(cancel_rect).layout(
                            egui::Layout::centered_and_justified(egui::Direction::LeftToRight),
                        ));
                    cancel_ui
                        .add_enabled_ui(!commit_started && !cancel_requested, |ui| {
                            ui.add_sized(cancel_rect.size(), egui::Button::new(cancel_label))
                        })
                        .inner
                        .clicked()
                });
            response
                .backdrop_response
                .on_hover_cursor(egui::CursorIcon::Wait);
            response.inner
        } else {
            let mut cancel = false;
            let mut render_contents = |ui: &mut egui::Ui| {
                ui.set_width(420.0);
                static_label(ui, RichText::new(label).size(15.0));
                ui.add_space(4.0);
                ui.add(
                    egui::ProgressBar::new(progress as f32 / 100.0)
                        .show_percentage()
                        .animate(true),
                );
                if ui
                    .add_enabled(
                        !commit_started && !cancel_requested,
                        egui::Button::new(if cancel_requested {
                            text.task_canceling()
                        } else {
                            text.cancel()
                        }),
                    )
                    .clicked()
                {
                    cancel = true;
                }
            };
            let constrain_rect = self
                .last_right_pane_rect
                .unwrap_or_else(|| ctx.viewport_rect());
            egui::Window::new(text.profiles())
                .id(egui::Id::new("profile_progress_dialog"))
                .default_pos(constrain_rect.min + egui::vec2(16.0, 16.0))
                .order(egui::Order::Foreground)
                .resizable(false)
                .collapsible(false)
                .default_width(420.0)
                .constrain_to(constrain_rect)
                .show(ctx, &mut render_contents);
            cancel
        };
        if cancel {
            self.cancel_profile_operation();
            ctx.request_repaint();
        }
        ctx.request_repaint_after(Duration::from_millis(100));
    }
}

#[cfg(test)]
mod profile_switcher_geometry_tests {
    use super::*;

    #[test]
    fn profile_switcher_bottom_touches_workspace_pane_top() {
        let slot = egui::Rect::from_min_size(egui::pos2(20.0, 40.0), egui::vec2(260.0, 64.0));
        let control = profile_switcher_control_rect(slot, Some(112.0));

        assert_eq!(control.left(), slot.left());
        assert_eq!(control.right(), slot.right());
        assert_eq!(control.bottom(), 112.0);
        assert_eq!(control.height(), 42.0);
    }

    #[test]
    fn profile_progress_footer_aligns_with_the_separator_edges() {
        let footer = egui::Rect::from_min_size(egui::pos2(45.0, 320.0), Vec2::new(500.0, 38.0));
        let (note, cancel) = profile_progress_footer_rects(footer, 104.0);

        assert_eq!(note.left(), footer.left());
        assert_eq!(cancel.right(), footer.right());
        assert_eq!(cancel.top(), footer.top());
        assert_eq!(cancel.width(), 104.0);
        assert!(note.right() < cancel.left());
    }

    #[test]
    fn switching_timeline_matches_the_safe_worker_order() {
        assert_eq!(
            profile_timeline_stages(ProfileOperationKind::Switch, true),
            &[
                ProfileTimelineStage::Extracting,
                ProfileTimelineStage::Activating,
            ]
        );
    }

    #[test]
    fn profile_worker_stages_map_to_the_expected_timeline_rows() {
        assert_eq!(
            profile_timeline_active_stage("Preparing selected profile"),
            Some(ProfileTimelineStage::Extracting)
        );
        assert_eq!(
            profile_timeline_active_stage("Committing profile switch"),
            Some(ProfileTimelineStage::Activating)
        );
    }

    #[test]
    fn positioned_timeline_spinner_does_not_move_the_parent_cursor() {
        let ctx = egui::Context::default();
        let _ = ctx.run_ui(egui::RawInput::default(), |ui| {
            let cursor_before = ui.cursor();
            let spinner_rect = egui::Rect::from_center_size(
                cursor_before.center() + egui::vec2(120.0, 40.0),
                Vec2::splat(14.0),
            );
            let mut spinner_ui =
                ui.new_child(egui::UiBuilder::new().max_rect(spinner_rect).layout(
                    egui::Layout::centered_and_justified(egui::Direction::LeftToRight),
                ));
            spinner_ui.add(egui::Spinner::new().size(13.0));

            assert_eq!(ui.cursor(), cursor_before);
        });
    }
}
