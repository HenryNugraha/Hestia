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

fn profile_selector_menu_frame(style: &egui::Style) -> egui::Frame {
    let menu_radius = style.visuals.menu_corner_radius;
    egui::Frame::popup(style)
        .inner_margin(egui::Margin {
            left: 10,
            right: 10,
            top: 8,
            bottom: 2,
        })
        .corner_radius(egui::CornerRadius {
            nw: 0,
            ne: 0,
            sw: menu_radius.sw,
            se: menu_radius.se,
        })
}

const PROFILE_SELECTOR_ROW_HEIGHT: f32 = 34.0;
const PROFILE_SELECTOR_FOOTER_GAP: f32 = 1.0;
const PROFILE_SELECTOR_VISIBLE_ROWS: usize = 7;
const PROFILE_SELECTOR_DOT_RADIUS: f32 = 4.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ProfileStorageTooltipState {
    Active,
    Queued,
    Running,
    Compressed,
    Failed,
    Unavailable,
}

fn profile_selector_list_max_height(row_gap: f32) -> f32 {
    PROFILE_SELECTOR_ROW_HEIGHT * PROFILE_SELECTOR_VISIBLE_ROWS as f32
        + row_gap * PROFILE_SELECTOR_VISIBLE_ROWS.saturating_sub(1) as f32
}

fn profile_storage_tooltip_state(
    active: bool,
    transient: Option<ProfileCompressionUiState>,
    loose_exists: bool,
    archive_exists: bool,
    archive_part_exists: bool,
) -> ProfileStorageTooltipState {
    if active {
        return ProfileStorageTooltipState::Active;
    }
    if let Some(transient) = transient {
        return match transient {
            ProfileCompressionUiState::Queued => ProfileStorageTooltipState::Queued,
            ProfileCompressionUiState::Running => ProfileStorageTooltipState::Running,
            ProfileCompressionUiState::Failed => ProfileStorageTooltipState::Failed,
        };
    }
    if archive_part_exists {
        ProfileStorageTooltipState::Running
    } else if loose_exists {
        ProfileStorageTooltipState::Queued
    } else if archive_exists {
        ProfileStorageTooltipState::Compressed
    } else {
        ProfileStorageTooltipState::Unavailable
    }
}

fn paint_profile_selector_dot(painter: &egui::Painter, center: egui::Pos2, active: bool) {
    if active {
        painter.circle_filled(
            center,
            PROFILE_SELECTOR_DOT_RADIUS,
            Color32::from_rgb(112, 164, 118),
        );
    } else {
        painter.circle_stroke(
            center,
            PROFILE_SELECTOR_DOT_RADIUS,
            egui::Stroke::new(1.25, Color32::from_rgb(145, 151, 159)),
        );
    }
}

fn profile_storage_tooltip_text(
    text: TextCatalog,
    state: ProfileStorageTooltipState,
    archive_size: Option<u64>,
) -> String {
    let state_label = match state {
        ProfileStorageTooltipState::Active => text.profile_compression_active(),
        ProfileStorageTooltipState::Queued => text.profile_compression_queued(),
        ProfileStorageTooltipState::Running => text.profile_compression_running(),
        ProfileStorageTooltipState::Compressed => text.profile_compression_complete(),
        ProfileStorageTooltipState::Failed => text.profile_compression_failed(),
        ProfileStorageTooltipState::Unavailable => text.profile_compression_unavailable(),
    };
    let size_label = if archive_size.is_some()
        && !matches!(
            state,
            ProfileStorageTooltipState::Compressed | ProfileStorageTooltipState::Unavailable
        ) {
        text.profile_previous_archive_size()
    } else {
        text.profile_archive_size()
    };
    let size = archive_size
        .map(format_file_size)
        .unwrap_or_else(|| text.profile_no_archive_yet().to_string());
    format!(
        "{}: {state_label}\n{size_label}: {size}",
        text.profile_status_label()
    )
}

fn profile_selector_action_row(
    ui: &mut egui::Ui,
    icon: Icon,
    label: &str,
    enabled: bool,
) -> egui::Response {
    let row_width = ui.available_width().max(1.0);
    let sense = if enabled {
        Sense::click()
    } else {
        Sense::hover()
    };
    let (rect, response) =
        ui.allocate_exact_size(Vec2::new(row_width, PROFILE_SELECTOR_ROW_HEIGHT), sense);
    if enabled && response.hovered() {
        ui.painter().rect_filled(
            rect,
            egui::CornerRadius::same(7),
            Color32::from_rgba_premultiplied(44, 47, 52, 205),
        );
    }
    let icon_color = if enabled {
        Color32::from_rgb(171, 177, 185)
    } else {
        Color32::from_rgb(112, 117, 124)
    };
    let text_color = if enabled {
        Color32::from_rgb(218, 222, 227)
    } else {
        Color32::from_rgb(137, 142, 150)
    };
    ui.painter().text(
        egui::pos2(rect.left() + 15.0, rect.center().y),
        egui::Align2::CENTER_CENTER,
        icon_char(icon),
        egui::FontId::new(14.0, FontFamily::Name(LUCIDE_FAMILY.into())),
        icon_color,
    );
    ui.painter().text(
        egui::pos2(rect.left() + 31.0, rect.center().y),
        egui::Align2::LEFT_CENTER,
        label,
        egui::FontId::proportional(13.0),
        text_color,
    );

    if enabled {
        response.on_hover_cursor(egui::CursorIcon::PointingHand)
    } else {
        response.on_hover_cursor(egui::CursorIcon::NotAllowed)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ProfileTimelineStage {
    Archiving,
    Extracting,
    Switching,
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

fn profile_name_action_footer<R>(
    ui: &mut egui::Ui,
    add_contents: impl FnOnce(&mut egui::Ui) -> R,
) -> egui::InnerResponse<R> {
    let size = Vec2::new(ui.available_width(), ui.spacing().interact_size.y);
    ui.allocate_ui_with_layout(
        size,
        egui::Layout::right_to_left(egui::Align::Center),
        add_contents,
    )
}

fn profile_timeline_stages(
    kind: ProfileOperationKind,
    prepares_before_activating: bool,
) -> &'static [ProfileTimelineStage] {
    const ACTIVATE_ONLY: &[ProfileTimelineStage] = &[ProfileTimelineStage::Switching];
    const PREPARE_FIRST: &[ProfileTimelineStage] = &[
        ProfileTimelineStage::Extracting,
        ProfileTimelineStage::Switching,
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
        || stage.contains("Switching")
    {
        Some(ProfileTimelineStage::Switching)
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
        ProfileTimelineStage::Switching => {
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

        egui::Popup::menu(&response)
            .id(ui.id().with("profile_selector_popup"))
            .width(response.rect.width())
            .frame(profile_selector_menu_frame(ui.style()))
            .show(|ui| {
                self.render_profile_selector_popup(ui);
            });
    }

    fn render_profile_selector_popup(&mut self, ui: &mut Ui) {
        let text = self.text();
        let Some(game) = self.selected_game().cloned() else {
            static_label(ui, RichText::new(text.profile_select_game()));
            return;
        };
        let game_id = game.definition.id.clone();
        let profile_roots =
            profiles::profile_roots(&game, self.state.static_prefs.use_default_mods_path).ok();

        let catalog = self
            .state
            .profiles_by_game
            .get(&game_id)
            .cloned()
            .unwrap_or_default();
        let blocked = self.profile_operations_blocked();

        let profile_count = catalog.profiles.len();
        let active_profile_id = catalog.active_profile_id;
        let profile_row_gap = ui.spacing().item_spacing.y;
        let profile_list_max_height = profile_selector_list_max_height(profile_row_gap);
        ui.spacing_mut().item_spacing.y = PROFILE_SELECTOR_FOOTER_GAP;
        ScrollArea::vertical()
            .id_salt(("profile_selector_list", &game_id))
            .max_height(profile_list_max_height)
            .auto_shrink([false, true])
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = profile_row_gap;
                for profile in catalog.profiles {
                    let active = active_profile_id == Some(profile.id);
                    let profile_name = text.profile_display_name(&profile.display_name);
                    let row_width = ui.available_width().max(1.0);
                    let (row_rect, _) = ui.allocate_exact_size(
                        Vec2::new(row_width, PROFILE_SELECTOR_ROW_HEIGHT),
                        Sense::hover(),
                    );
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

                    paint_profile_selector_dot(
                        ui.painter(),
                        egui::pos2(row_rect.left() + 15.0, row_rect.center().y),
                        active,
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

                    if select.hovered() {
                        let transient = self
                            .profile_compression_states
                            .get(&(game_id.clone(), profile.id))
                            .copied();
                        let (loose_exists, archive_exists, archive_part_exists, archive_size) =
                            profile_roots
                                .as_ref()
                                .map_or((false, false, false, None), |roots| {
                                    let archive_path = roots.archive_path(profile.id);
                                    let archive_metadata = fs::metadata(&archive_path)
                                        .ok()
                                        .filter(|entry| entry.is_file());
                                    (
                                        roots.profile_path(profile.id).is_dir(),
                                        archive_metadata.is_some(),
                                        roots.archive_part_path(profile.id).is_file(),
                                        archive_metadata.map(|entry| entry.len()),
                                    )
                                });
                        let storage_state = profile_storage_tooltip_state(
                            active,
                            transient,
                            loose_exists,
                            archive_exists,
                            archive_part_exists,
                        );
                        let storage_details =
                            profile_storage_tooltip_text(text, storage_state, archive_size);
                        let mut tooltip = if visible_name != profile_name {
                            format!("{profile_name}\n\n{storage_details}")
                        } else {
                            storage_details
                        };
                        if blocked && !active {
                            tooltip.push_str("\n\n");
                            tooltip.push_str(text.profile_finish_current_operation_first());
                        }
                        select.clone().on_hover_text(tooltip);
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
                        select.clone().on_hover_cursor(egui::CursorIcon::NotAllowed);
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
                                egui::Button::new(icon_text_sized(
                                    Icon::Pencil,
                                    text.rename(),
                                    14.0,
                                    13.0,
                                )),
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
                                .on_disabled_hover_text(
                                    text.profile_finish_current_operation_first(),
                                )
                                .on_hover_cursor(egui::CursorIcon::NotAllowed);
                        }

                        let delete = ui
                            .add_enabled(
                                !blocked && !active && profile_count > 1,
                                egui::Button::new(icon_text_sized(
                                    Icon::Trash2,
                                    text.delete(),
                                    14.0,
                                    13.0,
                                )),
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
                                .on_disabled_hover_text(
                                    text.profile_finish_current_operation_first(),
                                )
                                .on_hover_cursor(egui::CursorIcon::NotAllowed);
                        }
                    });
                }
            });

        ui.spacing_mut().item_spacing.y = PROFILE_SELECTOR_FOOTER_GAP;
        ui.separator();
        let create =
            profile_selector_action_row(ui, Icon::Plus, text.create_empty_profile(), !blocked);
        if create.clicked() {
            let name = self.next_profile_name(text.new_profile());
            self.start_profile_name_prompt(ProfileOperationKind::Create, None, name);
            ui.close();
        } else if blocked {
            create.on_hover_text(text.profile_finish_current_operation_first());
        }
        let duplicate =
            profile_selector_action_row(ui, Icon::Copy, text.duplicate_current_profile(), !blocked);
        if duplicate.clicked() {
            let name = self.next_profile_name(&self.active_profile_name());
            self.start_profile_name_prompt(ProfileOperationKind::Duplicate, None, name);
            ui.close();
        } else if blocked {
            duplicate.on_hover_text(text.profile_finish_current_operation_first());
        }
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
        let description = match kind {
            ProfileOperationKind::Rename => text.profile_rename_description(),
            ProfileOperationKind::Duplicate => text.profile_duplicate_description(),
            _ => text.profile_new_description(),
        };
        let icon = match kind {
            ProfileOperationKind::Rename => Icon::Pencil,
            ProfileOperationKind::Duplicate => Icon::Copy,
            _ => Icon::Users,
        };
        let icon_color = match kind {
            ProfileOperationKind::Rename => Color32::from_rgb(166, 172, 181),
            ProfileOperationKind::Duplicate => Color32::from_rgb(148, 192, 232),
            _ => Color32::from_rgb(214, 104, 58),
        };
        let mut submit = false;
        let mut cancel = false;
        let constrain_rect = self
            .last_right_pane_rect
            .unwrap_or_else(|| ctx.viewport_rect());
        egui::Window::new(icon_text_sized(icon, title, 14.0, 14.0))
            .id(egui::Id::new("profile_name_dialog"))
            .default_pos(constrain_rect.min + egui::vec2(16.0, 16.0))
            .order(egui::Order::Foreground)
            .resizable(false)
            .collapsible(false)
            .default_width(440.0)
            .constrain_to(constrain_rect)
            .frame(
                egui::Frame::window(&ctx.style_of(ctx.theme()))
                    .inner_margin(egui::Margin::same(16))
                    .stroke(egui::Stroke::new(1.0, icon_color.gamma_multiply(0.72))),
            )
            .show(ctx, |ui| {
                ui.set_width(440.0);
                ui.horizontal(|ui| {
                    ui.add_space(6.0);
                    ui.add(egui::Label::new(icon_rich(icon, 64.0, icon_color)).selectable(false))
                        .on_hover_cursor(egui::CursorIcon::Default);
                    ui.add_space(12.0);
                    ui.vertical(|ui| {
                        ui.set_width(346.0);
                        static_label(ui, RichText::new(description).size(15.0).strong());
                        ui.add_space(9.0);
                        let edit = ui.add(
                            TextEdit::singleline(&mut self.profile_name_draft)
                                .desired_width(f32::INFINITY)
                                .hint_text(
                                    RichText::new(text.profile_name())
                                        .color(Color32::from_rgb(145, 151, 160)),
                                ),
                        );
                        if edit.gained_focus() {
                            edit.request_focus();
                        }
                        submit = edit.lost_focus()
                            && ui.input(|input| input.key_pressed(egui::Key::Enter));
                        cancel = ui.input(|input| input.key_pressed(egui::Key::Escape));
                    });
                    ui.add_space(4.0);
                });
                ui.add_space(5.0);
                ui.separator();
                ui.add_space(2.0);
                profile_name_action_footer(ui, |ui| {
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
                            egui::Button::new(action_label).fill(Color32::from_rgb(180, 78, 35)),
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
        let constrain_rect = self
            .last_right_pane_rect
            .unwrap_or_else(|| ctx.viewport_rect());
        let warn_color = Color32::from_rgb(214, 96, 34);
        egui::Window::new(icon_text_sized(
            Icon::Trash2,
            text.delete_profile(),
            14.0,
            14.0,
        ))
        .id(egui::Id::new("profile_delete_dialog"))
        .default_pos(constrain_rect.min + egui::vec2(16.0, 16.0))
        .order(egui::Order::Foreground)
        .resizable(false)
        .collapsible(false)
        .default_width(440.0)
        .constrain_to(constrain_rect)
        .frame(
            egui::Frame::window(&ctx.style_of(ctx.theme()))
                .inner_margin(egui::Margin::same(16))
                .stroke(egui::Stroke::new(1.0, warn_color)),
        )
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.add_space(6.0);
                ui.add(
                    egui::Label::new(icon_rich(Icon::Trash2, 64.0, warn_color)).selectable(false),
                )
                .on_hover_cursor(egui::CursorIcon::Default);
                ui.add_space(10.0);
                ui.vertical(|ui| {
                    ui.set_width(340.0);
                    static_label(
                        ui,
                        RichText::new(text.profile_delete_confirmation(&profile_name))
                            .size(17.0)
                            .strong(),
                    );
                    ui.add_space(4.0);
                    static_label(
                        ui,
                        RichText::new(text.profile_delete_confirmation_details())
                            .size(13.0)
                            .color(Color32::from_rgb(170, 175, 183)),
                    );
                    ui.add_space(14.0);
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button(text.cancel()).clicked() {
                            cancel = true;
                        }
                        if ui
                            .add(
                                egui::Button::new(icon_text_sized(
                                    Icon::Trash2,
                                    text.delete_profile(),
                                    14.0,
                                    13.0,
                                ))
                                .fill(Color32::from_rgb(180, 78, 35)),
                            )
                            .clicked()
                        {
                            delete = true;
                        }
                    });
                });
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
        let commit_started = stage.contains("Activating")
            || stage.contains("Switching")
            || stage.contains("committed");
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
                                    ProfileTimelineStage::Switching => Icon::Check,
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
                            ProfileTimelineStage::Switching => text.switching_profile(),
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
                        .on_hover_cursor(egui::CursorIcon::PointingHand)
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
    fn profile_selector_menu_uses_deliberate_inner_padding_and_square_top_corners() {
        let style = egui::Style::default();
        let frame = profile_selector_menu_frame(&style);

        assert_eq!(
            frame.inner_margin,
            egui::Margin {
                left: 10,
                right: 10,
                top: 8,
                bottom: 2,
            }
        );
        assert_eq!(frame.corner_radius.nw, 0);
        assert_eq!(frame.corner_radius.ne, 0);
        assert_eq!(frame.corner_radius.sw, style.visuals.menu_corner_radius.sw);
        assert_eq!(frame.corner_radius.se, style.visuals.menu_corner_radius.se);
    }

    #[test]
    fn profile_selector_action_row_fills_the_menu_width_and_matches_profile_height() {
        let ctx = egui::Context::default();
        let mut fonts = FontDefinitions::default();
        fonts.font_data.insert(
            LUCIDE_FAMILY.to_string(),
            FontData::from_static(LUCIDE_FONT_BYTES).into(),
        );
        fonts.families.insert(
            FontFamily::Name(LUCIDE_FAMILY.into()),
            vec![LUCIDE_FAMILY.to_string()],
        );
        ctx.set_fonts(fonts);
        let _ = ctx.run_ui(egui::RawInput::default(), |ui| {
            ui.set_width(240.0);
            ui.spacing_mut().item_spacing.y = PROFILE_SELECTOR_FOOTER_GAP;
            let create = profile_selector_action_row(ui, Icon::Plus, "New profile", true);
            let duplicate = profile_selector_action_row(ui, Icon::Copy, "Duplicate profile", true);

            assert_eq!(create.rect.width(), 240.0);
            assert_eq!(create.rect.height(), PROFILE_SELECTOR_ROW_HEIGHT);
            assert_eq!(duplicate.rect.width(), 240.0);
            assert_eq!(
                duplicate.rect.top() - create.rect.bottom(),
                PROFILE_SELECTOR_FOOTER_GAP
            );
        });
    }

    #[test]
    fn profile_selector_list_caps_at_seven_complete_rows() {
        assert_eq!(
            profile_selector_list_max_height(3.0),
            PROFILE_SELECTOR_ROW_HEIGHT * 7.0 + 18.0
        );
    }

    #[test]
    fn profile_selector_footer_stays_below_the_scrolling_list() {
        let ctx = egui::Context::default();
        let scroll_rect = std::cell::Cell::new(None);
        let footer_rect = std::cell::Cell::new(None);
        let max_height = std::cell::Cell::new(0.0);
        let _ = ctx.run_ui(egui::RawInput::default(), |ui| {
            ui.set_width(240.0);
            ui.set_height(600.0);
            let row_gap = ui.spacing().item_spacing.y;
            max_height.set(profile_selector_list_max_height(row_gap));
            ui.spacing_mut().item_spacing.y = PROFILE_SELECTOR_FOOTER_GAP;
            let scroll = ScrollArea::vertical()
                .max_height(max_height.get())
                .show(ui, |ui| {
                    ui.spacing_mut().item_spacing.y = row_gap;
                    for _ in 0..10 {
                        ui.allocate_exact_size(
                            Vec2::new(ui.available_width(), PROFILE_SELECTOR_ROW_HEIGHT),
                            Sense::hover(),
                        );
                    }
                });
            scroll_rect.set(Some(scroll.inner_rect));

            ui.spacing_mut().item_spacing.y = PROFILE_SELECTOR_FOOTER_GAP;
            ui.separator();
            let (_, footer) = ui.allocate_exact_size(
                Vec2::new(ui.available_width(), PROFILE_SELECTOR_ROW_HEIGHT * 2.0),
                Sense::hover(),
            );
            footer_rect.set(Some(footer.rect));
        });

        let scroll_rect = scroll_rect.get().expect("scroll area");
        let footer_rect = footer_rect.get().expect("footer");
        assert!(scroll_rect.height() <= max_height.get() + 0.1);
        assert!(footer_rect.top() >= scroll_rect.bottom());
    }

    #[test]
    fn profile_storage_tooltip_tracks_compression_through_completion() {
        assert_eq!(
            profile_storage_tooltip_state(
                false,
                Some(ProfileCompressionUiState::Queued),
                true,
                false,
                false,
            ),
            ProfileStorageTooltipState::Queued
        );
        assert_eq!(
            profile_storage_tooltip_state(
                false,
                Some(ProfileCompressionUiState::Running),
                true,
                false,
                false,
            ),
            ProfileStorageTooltipState::Running
        );
        assert_eq!(
            profile_storage_tooltip_state(false, None, false, true, false),
            ProfileStorageTooltipState::Compressed
        );
        assert_eq!(
            profile_storage_tooltip_state(
                true,
                Some(ProfileCompressionUiState::Running),
                true,
                true,
                true
            ),
            ProfileStorageTooltipState::Active
        );
        assert_eq!(
            profile_storage_tooltip_state(false, None, true, true, false),
            ProfileStorageTooltipState::Queued
        );
        assert_eq!(
            profile_storage_tooltip_state(
                false,
                Some(ProfileCompressionUiState::Failed),
                true,
                true,
                false,
            ),
            ProfileStorageTooltipState::Failed
        );

        let text = TextCatalog::new(AppLanguage::English);
        assert_eq!(
            profile_storage_tooltip_text(text, ProfileStorageTooltipState::Running, None),
            "Status: Compressing\nArchive size: No archive yet"
        );
        assert_eq!(
            profile_storage_tooltip_text(text, ProfileStorageTooltipState::Compressed, Some(1024)),
            "Status: Compressed\nArchive size: 1.0 KB"
        );
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
    fn profile_name_action_footer_does_not_consume_remaining_window_height() {
        let ctx = egui::Context::default();
        let _ = ctx.run_ui(egui::RawInput::default(), |ui| {
            ui.set_width(440.0);
            ui.set_height(300.0);
            ui.add_space(100.0);
            let top = ui.cursor().top();
            let expected_height = ui.spacing().interact_size.y;
            let response = profile_name_action_footer(ui, |ui| ui.button("Cancel"));

            assert_eq!(response.response.rect.top(), top);
            assert_eq!(response.response.rect.height(), expected_height);
        });
    }

    #[test]
    fn switching_timeline_matches_the_safe_worker_order() {
        assert_eq!(
            profile_timeline_stages(ProfileOperationKind::Switch, true),
            &[
                ProfileTimelineStage::Extracting,
                ProfileTimelineStage::Switching,
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
            Some(ProfileTimelineStage::Switching)
        );
        assert_eq!(
            profile_timeline_active_stage("Profile switch committed"),
            Some(ProfileTimelineStage::Switching)
        );
        assert_eq!(
            profile_timeline_stage_progress(
                ProfileTimelineStage::Switching,
                70,
                "Switching profile",
            ),
            0.0
        );
        assert_eq!(
            profile_timeline_stage_progress(
                ProfileTimelineStage::Switching,
                95,
                "Switching profile",
            ),
            1.0
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
