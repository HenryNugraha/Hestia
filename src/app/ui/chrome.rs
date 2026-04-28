impl HestiaApp {
    fn render_top_bar(&mut self, ctx: &egui::Context) {
        let current_time = ctx.input(|i| i.time);
        
        for toast in &mut self.toasts {
            if toast.created_at == 0.0 {
                toast.created_at = current_time;
            }
        }
        self.toasts
            .retain(|toast| current_time - toast.created_at <= TOAST_DURATION);
        
        egui::TopBottomPanel::top("top_bar")
            .frame(
                egui::Frame::new()
                    .fill(Color32::from_rgba_premultiplied(24, 26, 29, 242))
                    .inner_margin(egui::Margin::same(8))
                    .outer_margin(egui::Margin {
                        left: WINDOW_INSET,
                        right: WINDOW_INSET,
                        top: 0,
                        bottom: 0,
                    }),
            )
            .show(ctx, |ui| {
            let titlebar_height = TITLEBAR_GAME_ICON_SIZE + 20.0;
            let (titlebar_rect, titlebar_drag) = ui.allocate_exact_size(
                Vec2::new(ui.available_width(), titlebar_height),
                Sense::click_and_drag(),
            );
            if titlebar_drag.drag_started() {
                ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
            }
            if titlebar_drag.double_clicked() {
                let maximized = ctx.input(|input| input.viewport().maximized.unwrap_or(false));
                ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(!maximized));
            }

            let inner_rect = titlebar_rect.shrink2(Vec2::new(8.0, 6.0));
            let mut titlebar_ui = ui.new_child(
                egui::UiBuilder::new()
                    .max_rect(inner_rect)
                    .layout(egui::Layout::left_to_right(egui::Align::Min)),
            );
            titlebar_ui.horizontal_top(|ui| {
                let title_job = {
                    let mut job = LayoutJob::default();
                    job.append(
                        "Hestia ",
                        0.0,
                        TextFormat {
                            font_id: egui::FontId::proportional(32.0),
                            color: Color32::from_rgb(210, 189, 156),
                            ..Default::default()
                        },
                    );
                    job.append(
                        env!("CARGO_PKG_VERSION"),
                        0.0,
                        TextFormat {
                            font_id: egui::FontId::proportional(10.0),
                            color: Color32::from_gray(150),
                            line_height: Some(16.0),
                            ..Default::default()
                        }
                    );
                    job.append(
                        "\nXXMI Mod Manager",
                        0.0,
                        TextFormat {
                            font_id: egui::FontId::proportional(12.0),
                            color: Color32::from_gray(190),
                            ..Default::default()
                        },
                    );
                    job
                };
                self.render_game_switcher(ui);
                ui.add_space(8.0);
                let divider_size = Vec2::new(1.0, TITLEBAR_GAME_ICON_SIZE + 24.0);
                let (divider_rect, _) = ui.allocate_exact_size(divider_size, Sense::hover());
                ui.painter().line_segment(
                    [divider_rect.center_top(), divider_rect.center_bottom()],
                    egui::Stroke::new(1.0, Color32::from_gray(110)),
                );
                ui.vertical(|ui| {
                    ui.add(egui::Label::new(title_job).selectable(false));
                    ui.add_space(-6.0);
                    let row_height = 64.0;
                    ui.allocate_ui_with_layout(
                        Vec2::new(ui.available_width(), row_height),
                        egui::Layout::left_to_right(egui::Align::Min),
                        |ui| {
                            ui.add_space(-8.0);
                            ui.spacing_mut().item_spacing.x = 1.0;

                            let buttons = [
                                (Icon::Play, "Play"),
                                (Icon::PackagePlus, "Install\nZip/Rar"),
                                (Icon::FolderPlus, "Install\nFolder"),
                                (Icon::RefreshCw, "Reload"),
                            ];
                            let max_lines =
                                buttons.iter().map(|(_, l)| l.lines().count()).max().unwrap_or(1);

                            let selected_game_ready = self.selected_game_is_installed_or_configured();
                            let play_modded_ready = self.selected_game_can_launch_modded();
                            let play_vanilla_ready = self.selected_game_can_launch_vanilla();
                            let play_ready = play_modded_ready || play_vanilla_ready;
                            let tooltip = "Game is not installed or configured.";
                            ui.add_enabled_ui(play_ready, |ui| {
                                let response = titlebar_action_button(
                                    ui,
                                    buttons[0].0,
                                    buttons[0].1,
                                    max_lines,
                                );
                                let launch_modded_by_default = play_modded_ready;
                                response.clone().on_hover_text(if launch_modded_by_default {
                                    "Launch the game with mods via XXMI"
                                } else {
                                    "Launch the game without mods"
                                });
                                if !play_ready {
                                    response
                                        .clone()
                                        .on_hover_text(tooltip)
                                        .on_hover_cursor(egui::CursorIcon::NotAllowed);
                                }
                                if play_ready {
                                    response.context_menu(|ui| {
                                        if ui
                                            .add_enabled(
                                                play_modded_ready,
                                                    egui::Button::new(icon_text_sized(
                                                        Icon::Play,
                                                        "Play with mods",
                                                        14.0,
                                                        13.0,
                                                    )),
                                            )
                                            .clicked()
                                        {
                                            self.launch_selected_game(ui.ctx(), true);
                                            ui.close();
                                        }
                                        if ui
                                            .add_enabled(
                                                play_vanilla_ready,
                                                    egui::Button::new(icon_text_sized(
                                                        Icon::Play,
                                                        "Play without mods",
                                                        14.0,
                                                        13.0,
                                                    )),
                                            )
                                            .clicked()
                                        {
                                            self.launch_selected_game(ui.ctx(), false);
                                            ui.close();
                                        }
                                    });
                                }
                                if response.clicked() {
                                    self.launch_selected_game(ui.ctx(), launch_modded_by_default);
                                }
                            });
                            ui.add_enabled_ui(selected_game_ready, |ui| {
                                let response = titlebar_action_button(
                                    ui,
                                    buttons[1].0,
                                    buttons[1].1,
                                    max_lines,
                                );
                                response
                                    .clone()
                                    .on_hover_text("Install a mod from a zip/rar/7z archive");
                                if !selected_game_ready {
                                    response
                                        .clone()
                                        .on_hover_text(tooltip)
                                        .on_hover_cursor(egui::CursorIcon::NotAllowed);
                                }
                                if response.clicked() {
                                    if let Some(paths) = FileDialog::new()
                                        .add_filter("Archives", &["zip", "rar", "7z"])
                                        .pick_files()
                                    {
                                        let sources = paths
                                            .into_iter()
                                            .map(ImportSource::Archive)
                                            .collect::<Vec<_>>();
                                        self.enqueue_install_sources(sources);
                                    }
                                }
                            });
                            ui.add_enabled_ui(selected_game_ready, |ui| {
                                let response = titlebar_action_button(
                                    ui,
                                    buttons[2].0,
                                    buttons[2].1,
                                    max_lines,
                                );
                                response
                                    .clone()
                                    .on_hover_text("Install a mod from an already extracted folder");
                                if !selected_game_ready {
                                    response
                                        .clone()
                                        .on_hover_text(tooltip)
                                        .on_hover_cursor(egui::CursorIcon::NotAllowed);
                                }
                                if response.clicked() {
                                    if let Some(path) = FileDialog::new().pick_folder() {
                                        self.enqueue_install_sources(vec![ImportSource::Folder(path)]);
                                    }
                                }
                            });
                            ui.add_enabled_ui(selected_game_ready, |ui| {
                                let now = ui.input(|i| i.time);
                                let is_reload_busy = self.startup_scan_loading
                                    || self.refresh_inflight
                                    || (self.current_view == ViewMode::Browse
                                        && self.browse_state.loading_page);
                                if is_reload_busy && !self.reload_was_busy {
                                    self.reload_spin_until = now + 0.7;
                                }
                                self.reload_was_busy = is_reload_busy;
                                let is_reload_rotating =
                                    is_reload_busy || now < self.reload_spin_until;
                                if is_reload_rotating {
                                    ui.ctx().request_repaint();
                                }
                                ui.add_enabled_ui(!is_reload_rotating, |ui| {
                                    let response = titlebar_action_button_with_spinner(
                                        ui,
                                        buttons[3].0,
                                        buttons[3].1,
                                        max_lines,
                                        is_reload_rotating,
                                    );
                                    let reload_tooltip = match self.current_view {
                                        ViewMode::Library => {
                                            "Rescan installed mods and check for updates on GameBanana (Ctrl+R)"
                                        }
                                        ViewMode::Browse => "Reload the current list (Ctrl+R)",
                                    };
                                    response.clone().on_hover_text(reload_tooltip);
                                    if !selected_game_ready {
                                        response
                                            .clone()
                                            .on_hover_text(tooltip)
                                            .on_hover_cursor(egui::CursorIcon::NotAllowed);
                                    }
                                    if response.clicked() {
                                        self.reload_spin_until = now + 0.7;
                                        self.reload_was_busy = true;
                                        if self.current_view == ViewMode::Browse {
                                            self.restart_browse_query();
                                        } else {
                                            self.refresh_with_toast();
                                        }
                                    }
                                });
                            });

                            let remaining_width = ui.available_width().max(0.0);
                            let pinned_tools = self.selected_game_pinned_tools();
                            let mut save_titlebar_order = false;
                            let mut dragged_titlebar_preview: Option<(String, egui::Rect)> = None;
                            let mut pending_tool_launch: Option<String> = None;
                            let mut pending_tool_launch_options: Option<String> = None;
                            let mut pending_tool_open: Option<String> = None;
                            let mut pending_tool_remove: Option<String> = None;
                            let mut pending_tool_pin_changes: Vec<(String, bool)> = Vec::new();
                            ui.allocate_ui_with_layout(
                                Vec2::new(remaining_width, row_height),
                                egui::Layout::right_to_left(egui::Align::Min),
                                |ui| {
                                    ui.spacing_mut().item_spacing.x = 4.0;
                                    for tool in pinned_tools.iter().rev() {
                                        let is_available = tool.path.is_file();
                                        let allow_hover_cursor = self.dragging_titlebar_tool_id.is_none();
                                        let is_dragging_this = self
                                            .dragging_titlebar_tool_id
                                            .as_ref()
                                            .is_some_and(|dragging_id| dragging_id == &tool.id);
                                        self.ensure_tool_icon_texture(ui.ctx(), tool);
                                        ui.add_enabled_ui(selected_game_ready && is_available, |ui| {
                                            let response = titlebar_tool_button(
                                                ui,
                                                self.tool_icon_texture(&tool.id),
                                                &tool.path,
                                                &tool.label,
                                                is_available,
                                                allow_hover_cursor,
                                            );
                                            response.context_menu(|ui| {
                                                if ui
                                                    .add_enabled(
                                                        is_available,
                                                        egui::Button::new(icon_text_sized(
                                                            Icon::Play,
                                                            "Launch",
                                                            14.0,
                                                            13.0,
                                                        )),
                                                    )
                                                    .clicked()
                                                {
                                                    pending_tool_launch = Some(tool.id.clone());
                                                    ui.close();
                                                }
                                                if ui
                                                    .button(icon_text_sized(
                                                        Icon::SquareTerminal,
                                                        "Set launch options",
                                                        14.0,
                                                        13.0,
                                                    ))
                                                    .clicked()
                                                {
                                                    pending_tool_launch_options =
                                                        Some(tool.id.clone());
                                                    ui.close();
                                                }
                                                if ui
                                                    .button(icon_text_sized(
                                                        Icon::FolderOpen,
                                                        "Open Folder",
                                                        14.0,
                                                        13.0,
                                                    ))
                                                    .clicked()
                                                {
                                                    pending_tool_open = Some(tool.id.clone());
                                                    ui.close();
                                                }
                                                if ui
                                                    .button(icon_text_sized(
                                                        Icon::PinOff,
                                                        "Unpin from Titlebar",
                                                        14.0,
                                                        13.0,
                                                    ))
                                                    .clicked()
                                                {
                                                    pending_tool_pin_changes
                                                        .push((tool.id.clone(), false));
                                                    ui.close();
                                                }
                                                if ui
                                                    .button(icon_text_sized(
                                                        Icon::Trash2,
                                                        "Remove",
                                                        14.0,
                                                        13.0,
                                                    ))
                                                    .clicked()
                                                {
                                                    pending_tool_remove = Some(tool.id.clone());
                                                    ui.close();
                                                }
                                            });
                                            let this_index = pinned_tools
                                                .iter()
                                                .position(|candidate| candidate.id == tool.id);
                                            let pointer_pos = ui.ctx().pointer_latest_pos();
                                            let insert_before = pointer_pos
                                                .is_some_and(|pos| pos.x < response.rect.center().x);
                                            let insertion_slot = this_index.map(|index| {
                                                if insert_before {
                                                    index
                                                } else {
                                                    index.saturating_add(1)
                                                }
                                            });
                                            if self.dragging_titlebar_tool_target_index == insertion_slot
                                                && self
                                                    .dragging_titlebar_tool_id
                                                    .as_ref()
                                                    .is_some_and(|dragging_id| dragging_id != &tool.id)
                                            {
                                                let line_x = if insert_before {
                                                    response.rect.left() - 2.0
                                                } else {
                                                    response.rect.right() + 2.0
                                                };
                                                let top = response.rect.top() + 4.0;
                                                let bottom = response.rect.bottom() - 4.0;
                                                let dash = 4.0;
                                                let gap = 3.0;
                                                let mut y = top;
                                                while y < bottom {
                                                    let y2 = (y + dash).min(bottom);
                                                    ui.painter().line_segment(
                                                        [egui::pos2(line_x, y), egui::pos2(line_x, y2)],
                                                        egui::Stroke::new(
                                                            1.25,
                                                            Color32::from_rgba_premultiplied(232, 153, 118, 210),
                                                        ),
                                                    );
                                                    y += dash + gap;
                                                }
                                            }
                                            if !selected_game_ready {
                                                response
                                                    .clone()
                                                    .on_hover_text(tooltip)
                                                    .on_hover_cursor(egui::CursorIcon::NotAllowed);
                                            }
                                            if response.drag_started() {
                                                self.dragging_titlebar_tool_id = Some(tool.id.clone());
                                                self.dragging_titlebar_tool_target_index = this_index;
                                            }
                                            let pointer_over_tool =
                                                pointer_pos.is_some_and(|pos| response.rect.contains(pos));
                                            if self.dragging_titlebar_tool_id.is_some()
                                                && ui.input(|input| input.pointer.primary_down())
                                                && self
                                                    .dragging_titlebar_tool_id
                                                    .as_ref()
                                                    .is_some_and(|dragging_id| dragging_id != &tool.id)
                                                && pointer_over_tool
                                            {
                                                if let Some(slot_index) = insertion_slot {
                                                    self.dragging_titlebar_tool_target_index = Some(slot_index);
                                                    ui.ctx().request_repaint();
                                                }
                                            }
                                            if response.drag_stopped()
                                                && self
                                                    .dragging_titlebar_tool_id
                                                    .as_ref()
                                                    .is_some_and(|dragging_id| dragging_id == &tool.id)
                                            {
                                                if let (Some(dragging_id), Some(target_index)) = (
                                                    self.dragging_titlebar_tool_id.clone(),
                                                    self.dragging_titlebar_tool_target_index,
                                                ) {
                                                    if self.move_tool_titlebar_order_to_slot(&dragging_id, target_index) {
                                                        save_titlebar_order = true;
                                                    }
                                                }
                                                self.dragging_titlebar_tool_id = None;
                                                self.dragging_titlebar_tool_target_index = None;
                                            }
                                            if response.clicked() && !response.dragged() {
                                                self.launch_tool(ui.ctx(), &tool.id);
                                            }
                                            if is_dragging_this {
                                                dragged_titlebar_preview =
                                                    Some((tool.id.clone(), response.rect));
                                            }
                                        });
                                    }
                                },
                            );
                            if let Some((tool_id, source_rect)) = dragged_titlebar_preview {
                                if let Some(pointer_pos) = ui.ctx().pointer_latest_pos() {
                                    let ghost_rect = egui::Rect::from_center_size(
                                        pointer_pos + egui::vec2(6.0, 8.0),
                                        source_rect.size(),
                                    );
                                    let painter = ui.ctx().layer_painter(egui::LayerId::new(
                                        egui::Order::Tooltip,
                                        egui::Id::new("dragging_titlebar_tool_ghost"),
                                    ));
                                    painter.rect(
                                        ghost_rect,
                                        egui::CornerRadius::same(12),
                                        Color32::from_rgba_premultiplied(44, 47, 52, 220),
                                        egui::Stroke::new(2.0, Color32::from_rgb(214, 104, 58)),
                                        egui::StrokeKind::Inside,
                                    );
                                    if let Some(texture) = self.tool_icon_texture(&tool_id) {
                                        let icon_rect = egui::Rect::from_center_size(
                                            ghost_rect.center(),
                                            egui::Vec2::splat(24.0),
                                        );
                                        painter.image(
                                            texture.id(),
                                            icon_rect,
                                            egui::Rect::from_min_max(
                                                egui::pos2(0.0, 0.0),
                                                egui::pos2(1.0, 1.0),
                                            ),
                                            Color32::WHITE,
                                        );
                                    }
                                }
                            }
                            if save_titlebar_order {
                                self.save_state();
                            }
                            if let Some(tool_id) = pending_tool_launch {
                                self.launch_tool(ui.ctx(), &tool_id);
                            }
                            if let Some(tool_id) = pending_tool_launch_options {
                                self.open_tool_launch_options_prompt(&tool_id);
                            }
                            if let Some(tool_id) = pending_tool_open {
                                self.open_tool_location(&tool_id);
                            }
                            for (tool_id, value) in pending_tool_pin_changes {
                                self.toggle_tool_titlebar_pin(&tool_id, value);
                            }
                            if let Some(tool_id) = pending_tool_remove {
                                self.remove_tool(&tool_id);
                            }
                        },
                    );
                });
                let controls_size = Vec2::new(104.0, 24.0);
                let controls_rect = egui::Rect::from_min_size(
                    egui::pos2(inner_rect.max.x - controls_size.x - 2.0, inner_rect.min.y - 2.0),
                    controls_size,
                );
                let mut controls_ui = ui.new_child(
                    egui::UiBuilder::new()
                        .max_rect(controls_rect)
                        .layout(egui::Layout::right_to_left(egui::Align::Min)),
                );
                controls_ui.spacing_mut().item_spacing.x = 2.0;
                controls_ui.horizontal(|ui| {
                    if titlebar_control_button(ui, Icon::X, "Close").clicked()
                    {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                    let maximized = ctx.input(|input| input.viewport().maximized.unwrap_or(false));
                    if titlebar_control_button(
                        ui,
                        if maximized {
                            Icon::SquareStack
                        } else {
                            Icon::Square
                        },
                        if maximized { "Restore" } else { "Maximize" },
                    )
                    .clicked()
                    {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(!maximized));
                    }
                    if titlebar_control_button(ui, Icon::Minus, "Minimize").clicked() {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(true));
                    }
                });
                if self.app_update_verified_path.is_some() {
                    let restart_size = Vec2::new(132.0, 24.0);
                    let restart_rect = egui::Rect::from_min_size(
                        egui::pos2(
                            controls_rect.min.x - restart_size.x - 8.0,
                            controls_rect.min.y - 3.0,
                        ),
                        restart_size,
                    );
                    let mut restart_ui = ui.new_child(
                        egui::UiBuilder::new()
                            .max_rect(restart_rect)
                            .layout(egui::Layout::right_to_left(egui::Align::Center)),
                    );
                    let response = restart_ui.add(
                        egui::Label::new(
                            RichText::new("Restart to Update")
                                .color(Color32::from_rgb(210, 189, 156))
                                .underline(),
                        )
                        .selectable(false)
                        .sense(Sense::click()),
                    );
                    response.clone().on_hover_cursor(egui::CursorIcon::PointingHand);
                    if response.clicked() {
                        self.restart_to_update();
                    }
                }
                if self.dragging_titlebar_tool_id.is_some()
                    && ctx.input(|input| input.pointer.primary_down())
                {
                    ctx.output_mut(|output| output.cursor_icon = egui::CursorIcon::Grabbing);
                }
            });
            if !self.toasts.is_empty() {
                let panel_rect = ui.available_rect_before_wrap();
                let center_x = panel_rect.center().x;
                let mut next_y = panel_rect.top() + TOAST_OFFSET;
                for (index, toast) in self.toasts.iter().enumerate() {
                    let elapsed = current_time - toast.created_at;
                    let fade_start = (TOAST_DURATION - 1.0).max(0.0);
                    let alpha = if elapsed > fade_start {
                        let fade_time = elapsed - fade_start;
                        let fade_duration = (TOAST_DURATION - fade_start).max(0.001);
                        ((1.0 - (fade_time / fade_duration)) * 255.0) as u8
                    } else {
                        255
                    };
                    let bg_color = if toast.is_error {
                        Color32::from_rgba_unmultiplied(78, 32, 36, alpha)
                    } else {
                        Color32::from_rgba_unmultiplied(36, 38, 42, alpha)
                    };
                    let text_color = if toast.is_error {
                        Color32::from_rgba_unmultiplied(255, 214, 214, alpha)
                    } else {
                        Color32::from_rgba_unmultiplied(232, 232, 232, alpha)
                    };
                    let stroke_color = if toast.is_error {
                        Color32::from_rgba_unmultiplied(150, 72, 78, alpha)
                    } else {
                        Color32::from_rgba_unmultiplied(92, 94, 100, alpha)
                    };

                    let response = egui::Area::new(egui::Id::new(("toast", index)))
                        .anchor(egui::Align2::CENTER_TOP, egui::vec2(0.0, next_y))
                        .fixed_pos(egui::pos2(center_x, panel_rect.top()))
                        .show(ctx, |ui| {
                            ui.set_max_width(TOAST_MAX_WIDTH);
                            let frame = egui::Frame::new()
                                .fill(bg_color)
                                .stroke(egui::Stroke::new(1.0, stroke_color))
                                .corner_radius(egui::CornerRadius::same(10))
                                .inner_margin(egui::Margin::symmetric(14, 8));
                            frame
                                .show(ui, |ui| {
                                    ui.label(
                                        RichText::new(&toast.message)
                                            .color(text_color)
                                            .size(13.0),
                                    );
                                })
                                .response
                        });
                    next_y += response.response.rect.height() + TOAST_SPACING;
                }
                ctx.request_repaint();
            }
            
            window_drag_strip(ui, ctx, 4.0);
        });
    }

    fn render_nav_rail(&mut self, ctx: &egui::Context) {
        egui::SidePanel::left("nav_rail")
            .resizable(false)
            .exact_width(78.0)
            .frame(
                egui::Frame::new()
                    .fill(Color32::from_rgba_premultiplied(24, 26, 29, 242))
                    .inner_margin(egui::Margin::same(10))
                    .outer_margin(egui::Margin {
                        left: WINDOW_INSET,
                        right: 0,
                        top: 0,
                        bottom: WINDOW_INSET,
                    }),
            )
            .show(ctx, |ui| {
                ui.vertical_centered(|ui| {
                    ui.add_space(8.0);
                    let old_view = self.current_view;
                    mode_icon_button(
                        ui,
                        &mut self.current_view,
                        ViewMode::Library,
                        Icon::LibraryBig,
                        "My Mods",
                    );
                    ui.add_space(8.0);
                    mode_icon_button(
                        ui,
                        &mut self.current_view,
                        ViewMode::Browse,
                        Icon::Compass,
                        "Browse",
                    );
                    if self.current_view != old_view {
                        self.mod_detail_editing = false;
                    }
                    let bottom_height = 348.0;
                    let spacer = (ui.available_height() - bottom_height).max(8.0);
                    ui.add_space(spacer);
                    if action_icon_button(ui, Icon::FileCog, "Tools", self.state.show_tools, Some("Tools (Ctrl+T)")) {
                        self.toggle_tools_window();
                    }
                    ui.add_space(8.0);
                    if action_icon_button(ui, Icon::ListChecks, "Tasks", self.state.show_tasks, Some("Tasks (Ctrl+J)")) {
                        self.toggle_tasks_window();
                    }
                    ui.add_space(8.0);
                    if action_icon_button(ui, Icon::FileCog, "Log", self.state.show_log, Some("Log (Ctrl+L)")) {
                        self.toggle_log_window();
                    }
                    ui.add_space(8.0);
                    if action_icon_button(ui, Icon::Settings2, "Settings", self.settings_open, Some("Settings (F10)")) {
                        self.settings_open = !self.settings_open;
                    }
                    ui.add_space(16.0);
                });
            });
    }

    fn render_game_switcher(&mut self, ui: &mut Ui) {
        let enabled_games = self.enabled_games();
        if let Some(texture) = self.app_icon_texture.as_ref() {
            self.game_icon_textures
                .entry(APP_ICON_ID.to_string())
                .or_insert_with(|| texture.clone());
        }
        let selected_game = self.selected_game().cloned();
        let selected_game_id = selected_game
            .as_ref()
            .map(|game| game.definition.id.as_str())
            .filter(|_| !enabled_games.is_empty())
            .filter(|game_id| {
                enabled_games
                    .iter()
                    .any(|game| game.definition.id == *game_id)
            })
            .or_else(|| enabled_games.first().map(|game| game.definition.id.as_str()));
        if let Some(game_id) = selected_game_id {
            if !self.game_icon_textures.contains_key(game_id) {
                self.request_icon_texture(game_id);
            }
        }
        let switcher_id = if enabled_games.is_empty() {
            Some(APP_ICON_ID)
        } else {
            selected_game_id
        };
        let response = game_switcher_button(ui, &self.game_icon_textures, switcher_id);

        egui::Popup::menu(&response)
            .id(ui.id().with("game_selector_popup"))
            .width(860.0)
            .show(|ui| {
                ui.add_space(12.0);
                ui.horizontal(|ui| {
                    ui.add_space(16.0);
                    ui.vertical(|ui| {
                        ui.spacing_mut().item_spacing = egui::vec2(18.0, 18.0);
                        if enabled_games.is_empty() {
                            static_label(ui, RichText::new("No games detected or enabled").strong());
                            ui.add_space(-16.0);
                            static_label(
                                ui,
                                RichText::new("See Settings → Game & Path")
                                    .color(Color32::from_gray(150)),
                            );
                        } else {
                            for row in enabled_games.chunks(3) {
                                ui.horizontal_top(|ui| {
                                    for game in row {
                                        if !self.game_icon_textures.contains_key(&game.definition.id) {
                                            self.request_icon_texture(&game.definition.id);
                                        }
                                        let selected = self.selected_game().is_some_and(|current| {
                                            current.definition.id == game.definition.id
                                        });
                                        let response = game_grid_card(
                                            ui,
                                            &self.game_icon_textures,
                                            &game.definition.id,
                                            &game.definition.name,
                                            selected,
                                        );
                                        if response.clicked() {
                                            if let Some(index) = self.state.games.iter().position(|item| {
                                                item.definition.id == game.definition.id
                                            }) {
                                                self.set_selected_game(index, ui.ctx());
                                            }
                                            ui.close();
                                        }
                                    }
                                });
                            }
                        }
                    });
                    ui.add_space(16.0);
                });
                ui.add_space(12.0);
            });
    }

}
