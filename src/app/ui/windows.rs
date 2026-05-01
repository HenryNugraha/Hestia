impl HestiaApp {
    fn render_log_panel(&mut self, ctx: &egui::Context) {
        if !self.state.show_log {
            return;
        }
        let mut log_open = self.state.show_log;
        let stick_to_bottom = self.log_scroll_to_bottom;
        let just_opened = self.log_scroll_to_bottom;
        let force_default_pos = self.log_force_default_pos;
        let log_frame = egui::Frame::window(&ctx.style()).inner_margin(egui::Margin::same(12));
        let mut window = egui::Window::new("Log")
            .id(egui::Id::new(("log_window", self.log_window_nonce)))
            .open(&mut log_open)
            .title_bar(true)
            .frame(log_frame);

        if let Some(rect) = self.last_right_pane_rect {
            let inset_rect = rect.shrink2(egui::vec2(12.0, 12.0));
            let log_offset = egui::vec2(4.0, 4.0);
            let log_size = egui::vec2(460.0, 420.0);
            window = window.movable(true).resizable(true).constrain_to(inset_rect).collapsible(true);
            if just_opened {
                window = window.default_size(log_size);
            }
            if force_default_pos {
                window = window.fixed_pos(inset_rect.max - log_size - log_offset);
            }
        } else if just_opened {
            window = window.default_width(460.0).default_height(420.0);
        }

        let log_response = window.show(ctx, |ui| {
            let use_24h = system_uses_24h_time();
            ScrollArea::vertical()
                .auto_shrink([false, false])
                .stick_to_bottom(stick_to_bottom)
                .show(ui, |ui| {
                let mut last_date: Option<String> = None;
                for entry in self.state.operations.iter().rev() {
                    let (date, time) = format_log_timestamp(entry.timestamp, use_24h);
                    if last_date.as_deref() != Some(date.as_str()) {
                        if last_date.is_some() {
                            ui.add_space(12.0);
                        }
                        static_label(ui, bold(date.clone()).underline());
                        last_date = Some(date);
                        ui.add_space(-4.0);
                    }
                    let summary = sanitize_log_subject(&entry.summary);
                    static_label(ui, format!("[{}] {}", time, summary));
                }
            });
        });

        if let Some(inner) = log_response {
            let copy_size = egui::vec2(26.0, 26.0);
            let window_rect = inner.response.rect;
            let copy_min =
                window_rect.right_bottom() - egui::vec2(copy_size.x + 36.0, copy_size.y + 24.0);
            egui::Area::new(egui::Id::new("log_copy_button"))
                .order(egui::Order::Foreground)
                .fixed_pos(copy_min)
                .show(ctx, |ui| {
                    let response = ui
                        .add(egui::Button::new(icon_rich(
                            Icon::Copy,
                            14.0,
                            Color32::from_gray(210),
                        )))
                        .on_hover_cursor(egui::CursorIcon::PointingHand);
                    if response.clicked() {
                        ctx.copy_text(build_log_text(&self.state.operations));
                        self.set_message_ok("Log copied");
                    }
                });
        }

        if stick_to_bottom {
            self.log_scroll_to_bottom = false;
        }
        if force_default_pos {
            self.log_force_default_pos = false;
        }
        if log_open != self.state.show_log {
            self.state.show_log = log_open;
            self.save_state();
        }
    }

    fn render_tasks_window(&mut self, ctx: &egui::Context) {
        if !self.state.show_tasks {
            return;
        }
        let mut tasks_open = self.state.show_tasks;
        let just_opened = self.tasks_force_default_pos;
        let force_default_pos = self.tasks_force_default_pos;
        let tasks_frame = egui::Frame::window(&ctx.style()).inner_margin(egui::Margin::same(12));
        let mut window = egui::Window::new("Tasks")
            .id(egui::Id::new(("tasks_window", self.tasks_window_nonce)))
            .open(&mut tasks_open)
            .title_bar(true)
            .frame(tasks_frame);

        if let Some(rect) = self.last_right_pane_rect {
            let inset_rect = rect.shrink2(egui::vec2(12.0, 12.0));
            let tasks_offset = egui::vec2(4.0, 4.0);
            let tasks_size = egui::vec2(460.0, 420.0);
            window = window
                .movable(true)
                .resizable(true)
                .constrain_to(inset_rect)
                .collapsible(true);
            if just_opened {
                window = window.default_size(tasks_size);
            }
            if force_default_pos {
                let top_right = egui::pos2(inset_rect.max.x, inset_rect.min.y);
                window = window.fixed_pos(
                    top_right - egui::vec2(tasks_size.x, 0.0) - tasks_offset,
                );
            }
        } else if just_opened {
            window = window.default_width(460.0).default_height(420.0);
        }

        window.show(ctx, |ui| {
            match self.state.tasks_layout {
                TasksLayout::Sections => {
                    let ongoing = self.sorted_tasks(|task| {
                        matches!(
                            task.status,
                            TaskStatus::Queued
                                | TaskStatus::Installing
                                | TaskStatus::Downloading
                                | TaskStatus::Canceling
                        )
                    });
                    let completed = self.sorted_tasks(|task| {
                        !self.browse_download_queue.iter().any(|j| j.task_id == task.id)
                            && !self.browse_download_inflight.contains_key(&task.id)
                            && !self.install_queue.iter().any(|j| j.id == task.id)
                            && !self.install_inflight.contains_key(&task.id)
                    });

                    let render_section_label = |ui: &mut Ui, label: &str| {
                        let section_height = 20.0;
                        let (rect, _) = ui.allocate_exact_size(
                            Vec2::new(ui.available_width(), section_height),
                            Sense::hover(),
                        );
                        let line_y = rect.center().y;
                        let line_color = Color32::from_gray(70);
                        ui.painter().line_segment(
                            [egui::pos2(rect.left(), line_y), egui::pos2(rect.right(), line_y)],
                            egui::Stroke::new(1.0, line_color),
                        );
                        let galley = ui.painter().layout_no_wrap(
                            label.to_string(),
                            egui::FontId::proportional(12.0),
                            Color32::from_gray(200),
                        );
                        let text_rect =
                            egui::Rect::from_center_size(rect.center(), galley.size());
                        ui.painter().rect_filled(
                            text_rect.expand(6.0),
                            6.0,
                            Color32::from_rgba_premultiplied(28, 30, 34, 230),
                        );
                        ui.painter().galley(text_rect.min, galley, Color32::WHITE);
                    };

                    let available_height = ui.available_height().max(1.0);
                    let section_gap = 10.0;
                    let ongoing_height = (available_height * 0.55).max(120.0);
                    let completed_height =
                        (available_height - ongoing_height - section_gap).max(120.0);

                    let ongoing_label = if ongoing.is_empty() {
                        "Ongoing".to_string()
                    } else {
                        format!("Ongoing ({})", ongoing.len())
                    };
                    render_section_label(ui, &ongoing_label);
                    ui.allocate_ui_with_layout(
                        Vec2::new(ui.available_width(), ongoing_height),
                        egui::Layout::top_down(egui::Align::Min),
                        |ui| {
                            ScrollArea::vertical()
                                .id_salt("tasks_ongoing")
                                .auto_shrink([false, true])
                                .show(ui, |ui| {
                                if ongoing.is_empty() {
                                    static_label(ui, RichText::new("No active tasks").color(Color32::from_gray(140)));
                                } else {
                                    for task in ongoing {
                                        ui.push_id(task.id, |ui| {
                                            self.render_task_row(ui, &task);
                                        });
                                        ui.add_space(8.0);
                                    }
                                }
                            });
                        },
                    );
                    ui.add_space(section_gap);
                    let completed_label = if completed.is_empty() {
                        "Completed".to_string()
                    } else {
                        format!("Completed ({})", completed.len())
                    };
                    render_section_label(ui, &completed_label);
                    ui.allocate_ui_with_layout(
                        Vec2::new(ui.available_width(), completed_height),
                        egui::Layout::top_down(egui::Align::Min),
                        |ui| {
                            ScrollArea::vertical()
                                .id_salt("tasks_completed")
                                .auto_shrink([false, true])
                                .show(ui, |ui| {
                                if completed.is_empty() {
                                    static_label(ui, RichText::new("No completed tasks").color(Color32::from_gray(140)));
                                } else {
                                    for task in completed {
                                        ui.push_id(task.id, |ui| {
                                            self.render_task_row(ui, &task);
                                        });
                                        ui.add_space(8.0);
                                    }
                                }
                            });
                        },
                    );
                }
                TasksLayout::Tabbed => {
                    let download_count =
                        self.browse_download_queue.len() + self.browse_download_inflight.len();
                    let install_count = self
                        .state
                        .tasks
                        .iter()
                        .filter(|task| {
                            task.kind == TaskKind::Install
                                && matches!(
                                    task.status,
                                    TaskStatus::Queued
                                        | TaskStatus::Installing
                                        | TaskStatus::Canceling
                                )
                        })
                        .count();
                    let completed_count = self
                        .state
                        .tasks
                        .iter()
                        .filter(|task| {
                            matches!(
                                task.status,
                                TaskStatus::Completed
                                    | TaskStatus::Failed
                                    | TaskStatus::Canceled
                            )
                        })
                        .count();
                    let failed_count = self
                        .state
                        .tasks
                        .iter()
                        .filter(|task| task.status == TaskStatus::Failed)
                        .count();
                    let downloads_label = if download_count > 0 {
                        format!("Downloads ({download_count})")
                    } else {
                        "Downloads".to_string()
                    };
                    let installs_label = if install_count > 0 {
                        format!("Installs ({install_count})")
                    } else {
                        "Installs".to_string()
                    };
                    let completed_label = if completed_count > 0 {
                        format!("Completed ({completed_count})")
                    } else {
                        "Completed".to_string()
                    };
                    let failed_label = if failed_count > 0 {
                        format!("Failed ({failed_count})")
                    } else {
                        "Failed".to_string()
                    };
                    ui.horizontal(|ui| {
                        ui.selectable_value(
                            &mut self.tasks_tab,
                            TasksTab::Downloads,
                            downloads_label,
                        );
                        ui.selectable_value(
                            &mut self.tasks_tab,
                            TasksTab::Installs,
                            installs_label,
                        );
                        ui.selectable_value(
                            &mut self.tasks_tab,
                            TasksTab::Completed,
                            completed_label,
                        );
                        ui.selectable_value(
                            &mut self.tasks_tab,
                            TasksTab::Failed,
                            failed_label,
                        );
                    });
                    ui.add_space(6.0);

                    ScrollArea::vertical().auto_shrink([false, true]).show(ui, |ui| {
                        let items = match self.tasks_tab {
                            TasksTab::Downloads => self.sorted_tasks(|task| {
                                self.browse_download_queue.iter().any(|j| j.task_id == task.id)
                                    || self.browse_download_inflight.contains_key(&task.id)
                            }),
                            TasksTab::Installs => self.sorted_tasks(|task| {
                                self.install_queue.iter().any(|j| j.id == task.id)
                                    || self.install_inflight.contains_key(&task.id)
                            }),
                            TasksTab::Completed => self.sorted_tasks(|task| {
                                !self.browse_download_queue.iter().any(|j| j.task_id == task.id)
                                    && !self.browse_download_inflight.contains_key(&task.id)
                                    && !self.install_queue.iter().any(|j| j.id == task.id)
                                    && !self.install_inflight.contains_key(&task.id)
                                    && task.status != TaskStatus::Failed
                            }),
                            TasksTab::Failed => {
                                self.sorted_tasks(|task| task.status == TaskStatus::Failed)
                            }
                        };
                        if items.is_empty() {
                            static_label(ui, RichText::new("No tasks").color(Color32::from_gray(140)));
                        } else {
                            for task in items {
                                ui.push_id(task.id, |ui| {
                                    self.render_task_row(ui, &task);
                                });
                                ui.add_space(8.0);
                            }
                        }
                    });
                }
                TasksLayout::SingleList => {
                    let stick_to_bottom =
                        self.state.tasks_order == TasksOrder::OldestFirst
                            && self.tasks_scroll_to_edge;
                    let scroll_to_top =
                        self.state.tasks_order == TasksOrder::NewestFirst
                            && self.tasks_scroll_to_edge;
                    ScrollArea::vertical()
                        .auto_shrink([false, true])
                        .stick_to_bottom(stick_to_bottom)
                        .show(ui, |ui| {
                        let items = self.sorted_tasks(|_| true);
                        if items.is_empty() {
                            static_label(ui, RichText::new("No tasks").color(Color32::from_gray(140)));
                        } else {
                            for task in items {
                                ui.push_id(task.id, |ui| {
                                    self.render_task_row(ui, &task);
                                });
                                ui.add_space(8.0);
                            }
                            if scroll_to_top {
                                ui.scroll_to_cursor(Some(egui::Align::Min));
                            }
                        }
                    });
                    if self.tasks_scroll_to_edge {
                        self.tasks_scroll_to_edge = false;
                    }
                }
            }
        });

        if force_default_pos {
            self.tasks_force_default_pos = false;
        }
        if tasks_open != self.state.show_tasks {
            self.state.show_tasks = tasks_open;
            self.save_state();
        }
    }

    fn render_tools_window(&mut self, ctx: &egui::Context) {
        if !self.state.show_tools {
            return;
        }
        let mut tools_open = self.state.show_tools;
        let just_opened = self.tools_force_default_pos;
        let force_default_pos = self.tools_force_default_pos;
        let tools_frame = egui::Frame::window(&ctx.style()).inner_margin(egui::Margin::same(12));
        let mut window = egui::Window::new("Tools")
            .id(egui::Id::new(("tools_window", self.tools_window_nonce)))
            .open(&mut tools_open)
            .title_bar(true)
            .frame(tools_frame);

        if let Some(rect) = self.last_right_pane_rect {
            let inset_rect = rect.shrink2(egui::vec2(12.0, 12.0));
            let tools_offset = egui::vec2(20.0, 4.0);
            let tools_size = egui::vec2(560.0, 460.0);
            window = window
                .movable(true)
                .resizable(true)
                .constrain_to(inset_rect)
                .collapsible(true);
            if just_opened {
                window = window.default_size(tools_size);
            }
            if force_default_pos {
                let top_right = egui::pos2(inset_rect.max.x, inset_rect.min.y);
                window = window.fixed_pos(
                    top_right - egui::vec2(tools_size.x, 0.0) - tools_offset,
                );
            }
        } else if just_opened {
            window = window.default_width(560.0).default_height(460.0);
        }

        let mut pending_remove: Option<String> = None;
        let mut pending_launch: Option<String> = None;
        let mut pending_open: Option<String> = None;
        let mut pin_changes: Vec<(String, bool)> = Vec::new();
        let mut pending_add = false;
        let mut pending_launch_options: Option<String> = None;
        let mut save_after_drag = false;
        let mut dragged_window_preview: Option<(String, String, bool, egui::Rect)> = None;
        let mut tool_card_rects: Vec<egui::Rect> = Vec::new();
        let mut add_card_rect: Option<egui::Rect> = None;

        window.show(ctx, |ui| {
            let Some(_game) = self.selected_game().cloned() else {
                static_label(ui, RichText::new("No game selected").color(Color32::from_gray(160)));
                return;
            };

            let tools = self.selected_game_tools();
            for tool in &tools {
                self.ensure_tool_icon_texture(ctx, tool);
            }

            ScrollArea::vertical().auto_shrink([false, true]).show(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing = egui::vec2(12.0, 12.0);

                    for tool in &tools {
                        let is_missing = !tool.path.is_file();
                        let allow_hover_cursor = self.dragging_window_tool_id.is_none();
                        let is_dragging_this = self
                            .dragging_window_tool_id
                            .as_ref()
                            .is_some_and(|dragging_id| dragging_id == &tool.id);
                        let card_size = Vec2::new(168.0, 204.0);
                        let (rect, response) =
                            ui.allocate_exact_size(card_size, Sense::click_and_drag());
                        tool_card_rects.push(rect);
                        let response = if allow_hover_cursor {
                            response.on_hover_cursor(egui::CursorIcon::PointingHand)
                        } else {
                            response
                        };
                        let fill = if is_dragging_this {
                            Color32::from_rgba_premultiplied(31, 33, 37, 110)
                        } else if response.hovered() {
                            Color32::from_rgba_premultiplied(44, 47, 52, 242)
                        } else {
                            Color32::from_rgba_premultiplied(31, 33, 37, 242)
                        };
                        let stroke = if is_dragging_this {
                            Color32::from_rgb(214, 104, 58)
                        } else if response.hovered() {
                            Color32::from_rgb(92, 98, 107)
                        } else {
                            Color32::from_rgb(69, 74, 81)
                        };
                        ui.painter().rect(
                            rect.shrink(1.0),
                            egui::CornerRadius::same(16),
                            fill,
                            egui::Stroke::new(1.0, stroke),
                            egui::StrokeKind::Inside,
                        );
                        let icon_size = 92.0;
                        let icon_rect = egui::Rect::from_center_size(
                            egui::pos2(rect.center().x, rect.top() + 72.0),
                            Vec2::splat(icon_size),
                        );
                        let mut icon_ui = ui.new_child(
                            egui::UiBuilder::new()
                                .max_rect(icon_rect)
                                .layout(egui::Layout::top_down(egui::Align::Center)),
                        );
                        let icon_response = paint_tool_icon(
                            &mut icon_ui,
                            &self.tool_icon_textures,
                            &tool.id,
                            &tool.path,
                            icon_size,
                            if is_missing && !tool.auto_detected {
                                Color32::from_gray(130)
                            } else {
                                Color32::WHITE
                            },
                            Sense::hover(),
                        );
                        icon_response.on_hover_text(tool.path.display().to_string());

                        let label_rect = egui::Rect::from_center_size(
                            egui::pos2(rect.center().x, rect.bottom() - 34.0),
                            egui::vec2(rect.width() - 22.0, 34.0),
                        );
                        let mut label_ui = ui.new_child(
                            egui::UiBuilder::new()
                                .max_rect(label_rect)
                                .layout(
                                    egui::Layout::centered_and_justified(egui::Direction::TopDown),
                                ),
                        );
                        let label_response = label_ui
                            .add(
                            egui::Label::new(
                                RichText::new(&tool.label)
                                    .size(14.0)
                                    .color(if is_missing && !tool.auto_detected {
                                        Color32::from_rgb(212, 122, 122)
                                    } else {
                                        Color32::from_rgb(228, 231, 235)
                                    }),
                            )
                            .truncate(),
                        )
                            .on_hover_cursor(egui::CursorIcon::Default);
                        if label_response.hovered() {
                            response.clone().on_hover_text_at_pointer(&tool.label);
                        }

                        let menu_rect = egui::Rect::from_min_size(
                            egui::pos2(rect.right() - 34.0, rect.top() + 10.0),
                            Vec2::new(24.0, 24.0),
                        );
                        let mut menu_ui = ui.new_child(
                            egui::UiBuilder::new()
                                .max_rect(menu_rect)
                                .layout(egui::Layout::top_down(egui::Align::Center)),
                        );
                        menu_ui.style_mut().spacing.button_padding = egui::vec2(4.0, 2.0);
                        let menu_response = menu_ui.menu_button(
                            RichText::new(icon_char(Icon::EllipsisVertical).to_string())
                                .family(FontFamily::Name(LUCIDE_FAMILY.into()))
                                .size(18.0)
                                .color(Color32::from_rgb(220, 224, 229)),
                            |ui| {
                                if ui
                                    .add_enabled(
                                        !is_missing,
                                        egui::Button::new(icon_text_sized(
                                            Icon::Play,
                                            "Launch",
                                            14.0,
                                            13.0,
                                        )),
                                    )
                                    .clicked()
                                {
                                    pending_launch = Some(tool.id.clone());
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
                                    pending_launch_options = Some(tool.id.clone());
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
                                    pending_open = Some(tool.id.clone());
                                    ui.close();
                                }
                                if ui
                                    .button(icon_text_sized(
                                        if tool.show_in_titlebar {
                                            Icon::PinOff
                                        } else {
                                            Icon::Pin
                                        },
                                        if tool.show_in_titlebar {
                                            "Unpin from Titlebar"
                                        } else {
                                            "Pin to Titlebar"
                                        },
                                        14.0,
                                        13.0,
                                    ))
                                    .clicked()
                                {
                                    pin_changes.push((tool.id.clone(), !tool.show_in_titlebar));
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
                                    pending_remove = Some(tool.id.clone());
                                    ui.close();
                                }
                            },
                        );
                        response.context_menu(|ui| {
                            if ui
                                .add_enabled(
                                    !is_missing,
                                    egui::Button::new(icon_text_sized(
                                        Icon::Play,
                                        "Launch",
                                        14.0,
                                        13.0,
                                    )),
                                )
                                .clicked()
                            {
                                pending_launch = Some(tool.id.clone());
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
                                pending_launch_options = Some(tool.id.clone());
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
                                pending_open = Some(tool.id.clone());
                                ui.close();
                            }
                            if ui
                                .button(icon_text_sized(
                                    if tool.show_in_titlebar {
                                        Icon::PinOff
                                    } else {
                                        Icon::Pin
                                    },
                                    if tool.show_in_titlebar {
                                        "Unpin from Titlebar"
                                    } else {
                                        "Pin to Titlebar"
                                    },
                                    14.0,
                                    13.0,
                                ))
                                .clicked()
                            {
                                pin_changes.push((tool.id.clone(), !tool.show_in_titlebar));
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
                                pending_remove = Some(tool.id.clone());
                                ui.close();
                            }
                        });
                        let pointer_over_menu = ui
                            .ctx()
                            .pointer_latest_pos()
                            .is_some_and(|pos| menu_rect.contains(pos));
                        if response.drag_started() {
                            self.dragging_window_tool_id = Some(tool.id.clone());
                            self.dragging_window_tool_target_index =
                                tools.iter().position(|candidate| candidate.id == tool.id);
                        }
                        let pointer_over_card = ui
                            .ctx()
                            .pointer_latest_pos()
                            .is_some_and(|pos| rect.contains(pos));
                        let insert_before = ui
                            .ctx()
                            .pointer_latest_pos()
                            .is_some_and(|pos| pos.x < rect.center().x);
                        let this_index = tools.iter().position(|candidate| candidate.id == tool.id);
                        let insertion_slot = this_index.map(|index| {
                            if insert_before {
                                index
                            } else {
                                index.saturating_add(1)
                            }
                        });
                        if self.dragging_window_tool_id.is_some()
                            && ui.input(|input| input.pointer.primary_down())
                            && self
                                .dragging_window_tool_id
                                .as_ref()
                                .is_some_and(|dragging_id| dragging_id != &tool.id)
                            && pointer_over_card
                        {
                            if let Some(slot_index) = insertion_slot {
                                self.dragging_window_tool_target_index = Some(slot_index);
                                ui.ctx().request_repaint();
                            }
                        }
                        if response.drag_stopped()
                            && self
                                .dragging_window_tool_id
                                .as_ref()
                                .is_some_and(|dragging_id| dragging_id == &tool.id)
                        {
                            if let (Some(dragging_id), Some(target_index)) = (
                                self.dragging_window_tool_id.clone(),
                                self.dragging_window_tool_target_index,
                            ) {
                                if self.move_tool_window_order_to_slot(&dragging_id, target_index) {
                                    save_after_drag = true;
                                }
                            }
                            self.dragging_window_tool_id = None;
                            self.dragging_window_tool_target_index = None;
                        }
                        if response.clicked()
                            && !response.dragged()
                            && !menu_response.response.hovered()
                            && !pointer_over_menu
                        {
                            pending_launch = Some(tool.id.clone());
                        }
                        if is_dragging_this {
                            dragged_window_preview = Some((
                                tool.id.clone(),
                                tool.label.clone(),
                                is_missing && !tool.auto_detected,
                                rect,
                            ));
                        }
                    }

                    let add_size = Vec2::new(168.0, 204.0);
                    let (add_rect, add_response) = ui.allocate_exact_size(add_size, Sense::click());
                    add_card_rect = Some(add_rect);
                    let add_response = if self.dragging_window_tool_id.is_none() {
                        add_response.on_hover_cursor(egui::CursorIcon::PointingHand)
                    } else {
                        add_response
                    };
                    let add_fill = if add_response.hovered() {
                        Color32::from_rgba_premultiplied(44, 47, 52, 242)
                    } else {
                        Color32::from_rgba_premultiplied(31, 33, 37, 242)
                    };
                    ui.painter().rect(
                        add_rect.shrink(1.0),
                        egui::CornerRadius::same(16),
                        add_fill,
                        egui::Stroke::new(1.0, Color32::from_rgb(86, 92, 100)),
                        egui::StrokeKind::Inside,
                    );
                    let dash = 10.0;
                    let gap = 6.0;
                    let outline = add_rect.shrink2(egui::vec2(18.0, 18.0));
                    let stroke = egui::Stroke::new(1.0, Color32::from_rgb(104, 110, 118));
                    let mut x = outline.left();
                    while x < outline.right() {
                        let x2 = (x + dash).min(outline.right());
                        ui.painter().line_segment(
                            [egui::pos2(x, outline.top()), egui::pos2(x2, outline.top())],
                            stroke,
                        );
                        ui.painter().line_segment(
                            [egui::pos2(x, outline.bottom()), egui::pos2(x2, outline.bottom())],
                            stroke,
                        );
                        x += dash + gap;
                    }
                    let mut y = outline.top();
                    while y < outline.bottom() {
                        let y2 = (y + dash).min(outline.bottom());
                        ui.painter().line_segment(
                            [egui::pos2(outline.left(), y), egui::pos2(outline.left(), y2)],
                            stroke,
                        );
                        ui.painter().line_segment(
                            [egui::pos2(outline.right(), y), egui::pos2(outline.right(), y2)],
                            stroke,
                        );
                        y += dash + gap;
                    }
                    ui.painter().text(
                        egui::pos2(add_rect.center().x, add_rect.center().y - 18.0),
                        egui::Align2::CENTER_CENTER,
                        icon_char(Icon::Plus),
                        egui::FontId::new(36.0, FontFamily::Name(LUCIDE_FAMILY.into())),
                        Color32::from_rgb(214, 218, 223),
                    );
                    ui.painter().text(
                        egui::pos2(add_rect.center().x, add_rect.center().y + 22.0),
                        egui::Align2::CENTER_CENTER,
                        "Add Tool",
                        egui::FontId::proportional(14.0),
                        Color32::from_rgb(214, 218, 223),
                    );
                    if add_response.clicked() {
                        pending_add = true;
                    }
                    if add_response.contains_pointer()
                        && self.dragging_window_tool_id.is_some()
                        && ui.input(|input| input.pointer.primary_down())
                    {
                        self.dragging_window_tool_target_index = Some(tools.len());
                        ui.ctx().request_repaint();
                    }
                    if let (Some(target_index), Some(add_rect)) =
                        (self.dragging_window_tool_target_index, add_card_rect)
                    {
                        let line = if !tool_card_rects.is_empty()
                            && self.dragging_window_tool_id.is_some()
                            && target_index <= tool_card_rects.len()
                        {
                            if target_index == 0 {
                                Some((
                                    tool_card_rects[0].left() - 3.0,
                                    tool_card_rects[0].top() + 8.0,
                                    tool_card_rects[0].bottom() - 8.0,
                                ))
                            } else if target_index < tool_card_rects.len() {
                                let previous = tool_card_rects[target_index - 1];
                                let next = tool_card_rects[target_index];
                                if (previous.center().y - next.center().y).abs() < 4.0 {
                                    Some((
                                        (previous.right() + next.left()) * 0.5,
                                        previous.top().max(next.top()) + 8.0,
                                        previous.bottom().min(next.bottom()) - 8.0,
                                    ))
                                } else {
                                    Some((next.left() - 3.0, next.top() + 8.0, next.bottom() - 8.0))
                                }
                            } else {
                                Some((add_rect.left() - 3.0, add_rect.top() + 8.0, add_rect.bottom() - 8.0))
                            }
                        } else {
                            None
                        };
                        if let Some((line_x, top, bottom)) = line {
                            let dash = 5.0;
                            let gap = 4.0;
                            let mut y = top;
                            while y < bottom {
                                let y2 = (y + dash).min(bottom);
                                ui.painter().line_segment(
                                    [egui::pos2(line_x, y), egui::pos2(line_x, y2)],
                                    egui::Stroke::new(
                                        1.0,
                                        Color32::from_rgba_premultiplied(232, 153, 118, 160),
                                    ),
                                );
                                y += dash + gap;
                            }
                        }
                    }
                });
            });
        });

        if let Some((_, label, is_missing, source_rect)) = dragged_window_preview {
            if let Some(pointer_pos) = ctx.pointer_latest_pos() {
                let size = source_rect.size();
                let ghost_rect = egui::Rect::from_center_size(
                    pointer_pos + egui::vec2(8.0, 10.0),
                    size,
                );
                let painter = ctx.layer_painter(egui::LayerId::new(
                    egui::Order::Tooltip,
                    egui::Id::new("dragging_window_tool_ghost"),
                ));
                painter.rect(
                    ghost_rect,
                    egui::CornerRadius::same(16),
                    Color32::from_rgba_premultiplied(44, 47, 52, 220),
                    egui::Stroke::new(2.0, Color32::from_rgb(214, 104, 58)),
                    egui::StrokeKind::Inside,
                );
                painter.text(
                    egui::pos2(ghost_rect.center().x, ghost_rect.bottom() - 34.0),
                    egui::Align2::CENTER_CENTER,
                    label,
                    egui::FontId::proportional(14.0),
                    if is_missing {
                        Color32::from_rgb(212, 122, 122)
                    } else {
                        Color32::from_rgb(228, 231, 235)
                    },
                );
            }
        }
        if self.dragging_window_tool_id.is_some() && ctx.input(|input| input.pointer.primary_down())
        {
            ctx.output_mut(|output| output.cursor_icon = egui::CursorIcon::Grabbing);
        }

        if pending_add {
            self.prompt_add_tool_for_selected_game();
        }
        if save_after_drag {
            self.save_state();
        }
        for (tool_id, pinned) in pin_changes {
            self.toggle_tool_titlebar_pin(&tool_id, pinned);
        }
        if let Some(tool_id) = pending_open {
            self.open_tool_location(&tool_id);
        }
        if let Some(tool_id) = pending_launch {
            self.launch_tool(ctx, &tool_id);
        }
        if let Some(tool_id) = pending_launch_options {
            self.open_tool_launch_options_prompt(&tool_id);
        }
        if let Some(tool_id) = pending_remove {
            self.remove_tool(&tool_id);
        }
        if force_default_pos {
            self.tools_force_default_pos = false;
        }
        if tools_open != self.state.show_tools {
            self.state.show_tools = tools_open;
            self.save_state();
        }
    }

    fn render_tool_launch_options_prompt(&mut self, ctx: &egui::Context) {
        let Some(prompt) = self.tool_launch_options_prompt.as_ref() else {
            return;
        };
        let Some(tool) = self
            .state
            .tools
            .iter()
            .find(|tool| tool.id == prompt.tool_id)
            .cloned()
        else {
            self.tool_launch_options_prompt = None;
            return;
        };

        let directory = tool
            .path
            .parent()
            .map(|path| path.display().to_string())
            .unwrap_or_default();
        let file_name = tool
            .path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_else(|| tool.label.as_str())
            .to_string();

        let mut open = true;
        let mut should_save = false;
        let mut should_cancel = false;
        let mut draft_args = prompt.launch_args.clone();
        let constrain_rect = self.last_right_pane_rect.unwrap_or_else(|| ctx.available_rect());

        egui::Window::new("Set Launch Options")
            .id(egui::Id::new(("tool_launch_options", tool.id.clone())))
            .default_pos(constrain_rect.min + egui::vec2(16.0, 16.0))
            .default_size(egui::vec2(360.0, 172.0))
            .order(egui::Order::Foreground)
            .resizable(false)
            .collapsible(false)
            .constrain_to(constrain_rect)
            .open(&mut open)
            .frame(
                egui::Frame::window(&ctx.style())
                    .inner_margin(egui::Margin::same(16))
                    .stroke(egui::Stroke::new(1.0, Color32::from_rgb(82, 134, 186))),
            )
            .show(ctx, |ui| {
                static_label(
                    ui,
                    RichText::new(&directory)
                        .family(FontFamily::Monospace)
                        .size(13.0)
                        .color(Color32::from_gray(180)),
                );
                ui.horizontal(|ui| {
                    let spacing = ui.spacing().item_spacing.x;
                    let available_width = ui.available_width();
                    let file_width = (available_width - 196.0 - spacing).clamp(84.0, 168.0);
                    let file_response = ui.add_sized(
                        [file_width, 22.0],
                        egui::Label::new(
                            RichText::new(&file_name)
                                .family(FontFamily::Monospace)
                                .size(14.0)
                                .color(Color32::from_rgb(226, 230, 234)),
                        )
                        .truncate()
                        .selectable(false),
                    );
                    file_response
                        .clone()
                        .on_hover_cursor(egui::CursorIcon::Default);
                    let input = ui.add(
                        TextEdit::singleline(&mut draft_args)
                            .id_salt(("tool_launch_options_input", tool.id.clone()))
                            .desired_width(ui.available_width())
                            .hint_text(
                                RichText::new("Launch options (ie, -option value -flag)")
                                    .color(Color32::from_gray(130)),
                            ),
                    );
                    if input.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                        should_save = true;
                    }
                });
                ui.add_space(14.0);
                ui.horizontal(|ui| {
                    if ui
                        .add(
                            egui::Button::new("Save")
                                .fill(Color32::from_rgb(180, 78, 35)),
                        )
                        .clicked()
                    {
                        should_save = true;
                    }
                    if ui.button("Cancel").clicked() {
                        should_cancel = true;
                    }
                });
            });

        if should_save {
            if let Some(prompt) = self.tool_launch_options_prompt.as_mut() {
                prompt.launch_args = draft_args;
            }
            self.save_tool_launch_options_prompt();
        } else if should_cancel || !open {
            self.tool_launch_options_prompt = None;
        } else if let Some(prompt) = self.tool_launch_options_prompt.as_mut() {
            prompt.launch_args = draft_args;
        }
    }

    fn render_settings_window(&mut self, ctx: &egui::Context) {
        if !self.settings_open {
            return;
        }
        let mut should_save = false;
        let game_ids: Vec<String> = self
            .state
            .games
            .iter()
            .map(|game| game.definition.id.clone())
            .collect();
        for game_id in game_ids {
            if !self.game_icon_textures.contains_key(&game_id) {
                self.request_icon_texture(&game_id);
            }
        }

        let mut settings_open = self.settings_open;
        let settings_frame = egui::Frame::window(&ctx.style()).inner_margin(egui::Margin::same(16));
        let mut window = egui::Window::new("Settings")
            .open(&mut settings_open)
            .title_bar(true)
            .frame(settings_frame);

        if let Some(rect) = self.last_right_pane_rect {
            let inset_rect = rect.shrink2(egui::vec2(12.0, 12.0));
            let settings_offset = egui::vec2(32.0, 0.0);
            window = window
                .default_pos(inset_rect.min + settings_offset)
                .default_size(egui::vec2(400.0, 600.0))
                .movable(true)
                .resizable(true)
                .constrain_to(inset_rect)
                .collapsible(true);
        } else {
            window = window.default_width(400.0).default_height(400.0);
        }

        let settings_response = window.show(ctx, |ui| {
            ui.horizontal(|ui| {
                let radius = egui::CornerRadius::same(3);
                ui.style_mut().visuals.widgets.inactive.corner_radius = radius;
                ui.style_mut().visuals.widgets.hovered.corner_radius = radius;
                ui.style_mut().visuals.widgets.active.corner_radius = radius;
                ui.style_mut().visuals.widgets.open.corner_radius = radius;
                ui.selectable_value(
                    &mut self.settings_tab,
                    SettingsTab::General,
                    bold("General".to_uppercase()),
                ).on_hover_cursor(egui::CursorIcon::PointingHand);
                ui.selectable_value(
                    &mut self.settings_tab,
                    SettingsTab::Advanced,
                    bold("Advanced".to_uppercase()),
                ).on_hover_cursor(egui::CursorIcon::PointingHand);
                ui.selectable_value(
                    &mut self.settings_tab,
                    SettingsTab::Path,
                    bold("Game & Path".to_uppercase()),
                ).on_hover_cursor(egui::CursorIcon::PointingHand);
                ui.selectable_value(
                    &mut self.settings_tab,
                    SettingsTab::About,
                    bold("About".to_uppercase()),
                ).on_hover_cursor(egui::CursorIcon::PointingHand);
            });

            ui.add_space(-4.0);
            ui.separator();
            ui.add_space(8.0);

            ScrollArea::vertical().auto_shrink([false, true]).show(ui, |ui| {
                match self.settings_tab {
                    SettingsTab::General => {
                        let radius = egui::CornerRadius::same(3);
                        ui.style_mut().visuals.widgets.inactive.corner_radius = radius;
                        ui.style_mut().visuals.widgets.hovered.corner_radius = radius;
                        ui.style_mut().visuals.widgets.active.corner_radius = radius;
                        ui.style_mut().visuals.widgets.open.corner_radius = radius;

                        static_label(ui, bold("Interface").underline().size(16.0));
                        ui.indent("setting_general_interface", |ui| {
                            static_label(ui, "When launching a game:");
                            ui.add_space(-4.0);
                            let launch_behavior = self.state.launch_behavior;
                            egui::ComboBox::from_id_salt("launch_behavior")
                                .selected_text(match launch_behavior {
                                    LaunchBehavior::DoNothing => "Do Nothing",
                                    LaunchBehavior::Minimize => "Minimize Hestia",
                                    LaunchBehavior::Exit => "Exit Hestia",
                                })
                                .show_ui(ui, |ui| {
                                    ui.selectable_value(&mut self.state.launch_behavior, LaunchBehavior::DoNothing, "Do Nothing");
                                    ui.selectable_value(&mut self.state.launch_behavior, LaunchBehavior::Minimize, "Minimize Hestia");
                                    ui.selectable_value(&mut self.state.launch_behavior, LaunchBehavior::Exit, "Exit Hestia");
                            });
                            if self.state.launch_behavior != launch_behavior { should_save = true; }
                            ui.add_space(8.0);
                            static_label(ui, "When launching a tool:");
                            ui.add_space(-4.0);
                            let tool_launch_behavior = self.state.tool_launch_behavior;
                            egui::ComboBox::from_id_salt("tool_launch_behavior")
                                .selected_text(match tool_launch_behavior {
                                    LaunchBehavior::DoNothing => "Do Nothing",
                                    LaunchBehavior::Minimize => "Minimize Hestia",
                                    LaunchBehavior::Exit => "Exit Hestia",
                                })
                                .show_ui(ui, |ui| {
                                    ui.selectable_value(&mut self.state.tool_launch_behavior, LaunchBehavior::DoNothing, "Do Nothing");
                                    ui.selectable_value(&mut self.state.tool_launch_behavior, LaunchBehavior::Minimize, "Minimize Hestia");
                                    ui.selectable_value(&mut self.state.tool_launch_behavior, LaunchBehavior::Exit, "Exit Hestia");
                                });
                            if self.state.tool_launch_behavior != tool_launch_behavior { should_save = true; }
                            ui.add_space(8.0);
                            static_label(ui, "After installing a mod:");
                            ui.add_space(-4.0);
                            let after_install = self.state.after_install_behavior;
                            egui::ComboBox::from_id_salt("after_install_behavior")
                                .selected_text(match after_install {
                                    AfterInstallBehavior::DoNothing => "Do Nothing",
                                    AfterInstallBehavior::AddToSelection => "Add to Selection",
                                    AfterInstallBehavior::OpenModDetail => "Open Mod Detail",
                                })
                                .show_ui(ui, |ui| {
                                    ui.selectable_value(&mut self.state.after_install_behavior, AfterInstallBehavior::DoNothing, "Do Nothing");
                                    ui.selectable_value(&mut self.state.after_install_behavior, AfterInstallBehavior::AddToSelection, "Add to Selection");
                                    ui.selectable_value(&mut self.state.after_install_behavior, AfterInstallBehavior::OpenModDetail, "Open Mod Detail");
                            });
                            if self.state.after_install_behavior != after_install { should_save = true; }
                            ui.add_space(8.0);
                            static_label(ui, "Group list by:");
                            ui.add_space(-4.0);
                            let group_mode = self.state.library_group_mode;
                            ui.horizontal(|ui| {
                                egui::ComboBox::from_id_salt("library_group_mode")
                                    .selected_text(match group_mode {
                                        LibraryGroupMode::Category => "Category",
                                        LibraryGroupMode::Status => "Status",
                                        LibraryGroupMode::None => "None",
                                    })
                                    .show_ui(ui, |ui| {
                                        ui.selectable_value(&mut self.state.library_group_mode, LibraryGroupMode::Category, "Category");
                                        ui.selectable_value(&mut self.state.library_group_mode, LibraryGroupMode::Status, "Status");
                                        ui.selectable_value(&mut self.state.library_group_mode, LibraryGroupMode::None, "None");
                                    });
                                ui.vertical(|ui| {
                                    if matches!(self.state.library_group_mode, LibraryGroupMode::Category) {
                                        ui.add_space(-10.0);
                                        if ui
                                            .checkbox(
                                                &mut self.state.library_uncategorized_first,
                                                "Show uncategorized mods first",
                                            )
                                            .changed()
                                        {
                                            should_save = true;
                                        }
                                        ui.add_space(-4.0);
                                    }
                                    let checkbox_changed = match self.state.library_group_mode {
                                        LibraryGroupMode::Status => {
                                            let response = ui.checkbox(
                                                &mut self.state.library_sort_category_first,
                                                "Sort by category first",
                                            );
                                            response
                                                .clone()
                                                .on_hover_text("Sorts by category order (not necessarily alphabetical).");
                                            response.changed()
                                        }
                                        LibraryGroupMode::Category | LibraryGroupMode::None => {
                                            let response = ui.checkbox(
                                                &mut self.state.library_sort_status_first,
                                                "Sort by status first",
                                            );
                                            response
                                                .clone()
                                                .on_hover_text("Sorts Active mods first, then Disabled, then Archived.");
                                            response.changed()
                                        }
                                    };
                                    if checkbox_changed {
                                        should_save = true;
                                    }
                                });
                            });
                            if self.state.library_group_mode != group_mode { should_save = true; }
                            ui.add_space(8.0);
                            let show_card_detail_response = if matches!(self.state.library_group_mode, LibraryGroupMode::Category) {
                                ui.checkbox(
                                    &mut self.state.library_category_group_show_status,
                                    "Show mod status on card",
                                )
                            } else {
                                let response = ui.checkbox(
                                    &mut self.state.library_status_group_show_category,
                                    "Show category on card",
                                );
                                response
                                    .clone()
                                    .on_hover_text("Mod state is still shown by the colored status dot.");
                                response
                            };
                            if show_card_detail_response.changed() {
                                should_save = true;
                            }
                            let mut show_disabled = !self.state.hide_disabled;
                            if ui.checkbox(&mut show_disabled, "Show disabled mods").changed() {
                                self.state.hide_disabled = !show_disabled;
                                should_save = true;
                            }
                            let mut show_archived = !self.state.hide_archived;
                            if ui.checkbox(&mut show_archived, "Show archived mods").changed() {
                                self.state.hide_archived = !show_archived;
                                should_save = true;
                            }
                            ui.add_space(8.0);
                            static_label(ui, "Metadata:");
                            ui.add_space(-4.0);
                            let meta_vis = self.state.metadata_visibility;
                            egui::ComboBox::from_id_salt("metadata_visibility")
                                .selected_text(match meta_vis {
                                    MetadataVisibility::Never => "Never show",
                                    MetadataVisibility::OnlyIfNoDescription => "Show if no description",
                                    MetadataVisibility::Always => "Always show",
                                })
                                .show_ui(ui, |ui| {
                                    ui.selectable_value(&mut self.state.metadata_visibility, MetadataVisibility::Never, "Never show");
                                    ui.selectable_value(&mut self.state.metadata_visibility, MetadataVisibility::OnlyIfNoDescription, "Show if no description");
                                    ui.selectable_value(&mut self.state.metadata_visibility, MetadataVisibility::Always, "Always show");
                                });
                            if self.state.metadata_visibility != meta_vis { should_save = true; }
                            ui.add_space(1.0);
                        });
                        ui.add_space(24.0);

                        static_label(ui, bold("Operational").underline().size(16.0));
                        ui.indent("setting_general_operational", |ui| {
                            static_label(ui, "Mods to check for updates:");
                            ui.add_space(-4.0);
                            let update_check_statuses = self.state.update_check_statuses;
                            ui.horizontal(|ui| {
                                ui.checkbox(&mut self.state.update_check_statuses.active, "Active");
                                ui.checkbox(&mut self.state.update_check_statuses.disabled, "Disabled");
                                ui.checkbox(&mut self.state.update_check_statuses.archived, "Archived");
                            });
                            if self.state.update_check_statuses.active != update_check_statuses.active
                                || self.state.update_check_statuses.disabled != update_check_statuses.disabled
                                || self.state.update_check_statuses.archived != update_check_statuses.archived
                            {
                                should_save = true;
                            }
                            ui.add_space(8.0);
                            static_label(ui, "Automatically update mods:");
                            ui.add_space(-4.0);
                            let auto_update_statuses = self.state.auto_update_statuses;
                            ui.horizontal(|ui| {
                                ui.checkbox(&mut self.state.auto_update_statuses.active, "Active");
                                ui.checkbox(&mut self.state.auto_update_statuses.disabled, "Disabled");
                                ui.checkbox(&mut self.state.auto_update_statuses.archived, "Archived");
                            });
                            if self.state.auto_update_statuses.active != auto_update_statuses.active
                                || self.state.auto_update_statuses.disabled != auto_update_statuses.disabled
                                || self.state.auto_update_statuses.archived != auto_update_statuses.archived
                            {
                                should_save = true;
                            }
                            ui.add_space(8.0);
                            static_label(ui, "Also update mods that have been modified:");
                            ui.add_space(-4.0);
                            let modified_update_behavior = self.state.modified_update_behavior;
                            egui::ComboBox::from_id_salt("modified_update_behavior")
                                .selected_text(match modified_update_behavior {
                                    ModifiedUpdateBehavior::Yes => "Yes",
                                    ModifiedUpdateBehavior::ShowButton => "No, but show Update button",
                                    ModifiedUpdateBehavior::HideButton => "No, and hide Update button",
                                })
                                .show_ui(ui, |ui| {
                                    ui.selectable_value(
                                        &mut self.state.modified_update_behavior,
                                        ModifiedUpdateBehavior::Yes,
                                        "Yes",
                                    );
                                    ui.selectable_value(
                                        &mut self.state.modified_update_behavior,
                                        ModifiedUpdateBehavior::ShowButton,
                                        "No, but show Update button",
                                    );
                                    ui.selectable_value(
                                        &mut self.state.modified_update_behavior,
                                        ModifiedUpdateBehavior::HideButton,
                                        "No, and hide Update button",
                                    );
                                });
                            if self.state.modified_update_behavior != modified_update_behavior {
                                should_save = true;
                            }
                            ui.add_space(8.0);
                            static_label(ui, "When installing an already exist mod:");
                            ui.add_space(-4.0);
                            let import_resolution = self.state.import_resolution;
                            let always_replace_on_update = self.state.always_replace_on_update;
                            ui.horizontal(|ui| {
                                egui::ComboBox::from_id_salt("import_resolution")
                                    .selected_text(match import_resolution {
                                        ImportResolution::Ask => "Always Ask",
                                        ImportResolution::Replace => "Always Replace",
                                        ImportResolution::Merge => "Always Merge",
                                        ImportResolution::KeepBoth => "Always Keep Both",
                                    })
                                    .show_ui(ui, |ui| {
                                        ui.selectable_value(&mut self.state.import_resolution, ImportResolution::Ask, "Always Ask");
                                        ui.selectable_value(&mut self.state.import_resolution, ImportResolution::Replace, "Always Replace");
                                        ui.selectable_value(&mut self.state.import_resolution, ImportResolution::Merge, "Always Merge");
                                        ui.selectable_value(&mut self.state.import_resolution, ImportResolution::KeepBoth, "Always Keep Both");
                                    });
                                ui.checkbox(&mut self.state.always_replace_on_update, "Always replace on updating mods");
                            });
                            if self.state.import_resolution != import_resolution
                                || self.state.always_replace_on_update != always_replace_on_update
                            {
                                should_save = true;
                            }
                            ui.add_space(8.0);
                            static_label(ui, "When deleting a mod:");
                            ui.add_space(-4.0);
                            let delete_behavior = self.state.delete_behavior;
                            egui::ComboBox::from_id_salt("delete_behavior")
                                .selected_text(match delete_behavior {
                                    DeleteBehavior::RecycleBin => "Move to Recycle Bin",
                                    DeleteBehavior::Permanent => "Delete Permanently",
                                })
                                .show_ui(ui, |ui| {
                                    ui.selectable_value(&mut self.state.delete_behavior, DeleteBehavior::RecycleBin, "Move to Recycle Bin");
                                    ui.selectable_value(&mut self.state.delete_behavior, DeleteBehavior::Permanent, "Delete Permanently");
                                });
                            if self.state.delete_behavior != delete_behavior { should_save = true; }
                            ui.add_space(1.0);
                        });
                        ui.add_space(24.0);

                        static_label(ui, bold("Tasks").underline().size(16.0));
                        ui.indent("setting_general_tasks", |ui| {
                            static_label(ui, "Tasks layout:");
                            ui.add_space(-4.0);
                            let tasks_layout = self.state.tasks_layout;
                            egui::ComboBox::from_id_salt("tasks_layout")
                                .selected_text(match tasks_layout {
                                    TasksLayout::Sections => "Sections",
                                    TasksLayout::Tabbed => "Tabbed",
                                    TasksLayout::SingleList => "Single List",
                                })
                                .show_ui(ui, |ui| {
                                    ui.selectable_value(&mut self.state.tasks_layout, TasksLayout::Sections, "Sections");
                                    ui.selectable_value(&mut self.state.tasks_layout, TasksLayout::Tabbed, "Tabbed");
                                    ui.selectable_value(&mut self.state.tasks_layout, TasksLayout::SingleList, "Single List");
                                });
                            if self.state.tasks_layout != tasks_layout { should_save = true; }
                            ui.add_space(8.0);
                            static_label(ui, "Task order:");
                            ui.add_space(-4.0);
                            let tasks_order = self.state.tasks_order;
                            egui::ComboBox::from_id_salt("tasks_order")
                                .selected_text(match tasks_order {
                                    TasksOrder::OldestFirst => "Oldest → Newest",
                                    TasksOrder::NewestFirst => "Newest → Oldest",
                                })
                                .show_ui(ui, |ui| {
                                    ui.selectable_value(&mut self.state.tasks_order, TasksOrder::OldestFirst, "Oldest → Newest");
                                    ui.selectable_value(&mut self.state.tasks_order, TasksOrder::NewestFirst, "Newest → Oldest");
                                });
                            if self.state.tasks_order != tasks_order { should_save = true; }
                            ui.add_space(8.0);
                            static_label(ui, "Clear completed tasks:");
                            ui.add_space(-4.0);
                            // if ui.button("Clear").clicked() {
                            //     self.clear_completed_tasks();
                            // }
                            if ui
                                .button(icon_text_sized(Icon::Trash2, "Clear Tasks", 14.5, 13.0))
                                .on_hover_cursor(egui::CursorIcon::PointingHand)
                                .clicked()
                            {
                                self.clear_completed_tasks();
                            }
                            ui.add_space(1.0);
                        });
                        ui.add_space(24.0);
                    }
                    SettingsTab::Path => {
                    static_label(ui, bold("XXMI").underline().size(16.0));
                    ui.group(|ui| {
                        let warn_color = Color32::from_rgb(124, 45, 58);
                        let err_stroke = egui::Stroke::new(1.0, warn_color);
                        ui.scope(|ui| {
                            let radius = egui::CornerRadius::same(3);
                            ui.style_mut().visuals.widgets.inactive.corner_radius = radius;
                            ui.style_mut().visuals.widgets.hovered.corner_radius = radius;
                            ui.style_mut().visuals.widgets.active.corner_radius = radius;
                            ui.style_mut().visuals.widgets.open.corner_radius = radius;

                            let invalid = self
                                .state
                                .modded_launcher_path_override
                                .as_ref()
                                .map_or(true, |path| !path.is_file());
                            ui.horizontal(|ui| {
                                static_label(
                                    ui,
                                    RichText::new("XXMI Launcher:")
                                        .small()
                                        .color(Color32::from_gray(165)),
                                );
                                if invalid {
                                    static_label(ui, icon_rich(Icon::TriangleAlert, 13.0, warn_color));
                                    ui.add_space(-8.0);
                                    static_label(
                                        ui,
                                        RichText::new("Path not found")
                                            .small()
                                            .color(warn_color),
                                    );
                                }
                            });
                            ui.add_space(-8.0);
                            ui.horizontal(|ui| {
                                let browse_width = 28.0;
                                let input_width = 320.0;
                                let input_id = ui.make_persistent_id("launcher_path_input");

                                let current_launcher_value = self
                                    .state
                                    .modded_launcher_path_override
                                    .as_ref()
                                    .map(|path| path.to_string_lossy().to_string())
                                    .unwrap_or_default();
                                let mut launcher_value = ui
                                    .data_mut(|d| d.get_temp::<String>(input_id))
                                    .unwrap_or_else(|| current_launcher_value.clone());
                                let launcher_dirty = launcher_value != current_launcher_value;
                                let resp = ui.add(
                                    TextEdit::singleline(&mut launcher_value)
                                        .id(input_id)
                                        .cursor_at_end(true)
                                        .horizontal_align(egui::Align::RIGHT)
                                        .desired_width(input_width)
                                );
                                ui.data_mut(|d| d.insert_temp(input_id, launcher_value.clone()));
                                if invalid {
                                    ui.painter().rect_stroke(
                                        resp.rect.expand(1.0),
                                        4.0,
                                        err_stroke,
                                        egui::StrokeKind::Inside,
                                    );
                                }
                                ui.add_space(-8.0);
                                let action_clicked = ui.scope(|ui| {
                                    ui.spacing_mut().button_padding = Vec2::new(2.0, 1.0);
                                    ui.add_sized(
                                        [browse_width, resp.rect.height()],
                                        egui::Button::new(
                                            if launcher_dirty {
                                                icon_rich(
                                                    Icon::Check,
                                                    16.0,
                                                    Color32::from_rgb(110, 194, 132),
                                                )
                                            } else {
                                                RichText::new(icon_char(Icon::Ellipsis).to_string())
                                                    .family(FontFamily::Name(LUCIDE_FAMILY.into()))
                                                    .size(16.0)
                                            },
                                        ),
                                    )
                                    .clicked()
                                })
                                .inner;
                                let submit_with_enter = resp.lost_focus()
                                    && ui.input(|i| i.key_pressed(egui::Key::Enter))
                                    && launcher_dirty;
                                if launcher_dirty && (action_clicked || submit_with_enter) {
                                    self.state.modded_launcher_path_override =
                                        non_empty(launcher_value).map(PathBuf::from);
                                    ui.data_mut(|d| d.remove::<String>(input_id));
                                    should_save = true;
                                } else if !launcher_dirty && action_clicked {
                                    if let Some(path) = FileDialog::new()
                                        .add_filter("Executable", &["exe"])
                                        .pick_file()
                                    {
                                        self.state.modded_launcher_path_override = Some(path);
                                        ui.data_mut(|d| d.remove::<String>(input_id));
                                        should_save = true;
                                    }
                                }
                            });
                            ui.add_space(-4.0);
                            ui.horizontal(|ui| {
                                ui.add_space(4.0);
                                if ui.checkbox(&mut self.state.use_default_mods_path, "Use default XXMI mod path for games").changed() {
                                    should_save = true;
                                }
                            });
                        });
                    });
                    ui.add_space(24.0);
                    ui.label(bold("Game").underline().size(16.0))
                    .on_hover_cursor(egui::CursorIcon::Default);
                    let mut selected_game_was_disabled = false;
                    let mut enabled_game_ids = Vec::new();
                    for (index, game) in self.state.games.iter_mut().enumerate() {
                        ui.group(|ui| {
                                ui.set_min_width(360.0);
                                ui.horizontal_top(|ui| {
                                    let tint = if game.enabled {
                                        Color32::WHITE
                                    } else {
                                        Color32::from_white_alpha(64)
                                    };
                                    let gameicon = paint_game_icon(
                                        ui,
                                        &self.game_icon_textures,
                                        &game.definition.id,
                                        128.0,
                                        tint,
                                        Sense::click(),
                                    )
                                    .on_hover_cursor(egui::CursorIcon::PointingHand);
                                    let before = game.enabled;
                                    if gameicon.clicked() {
                                        game.enabled = !game.enabled;
                                        should_save = true;
                                        if !before && game.enabled {
                                            enabled_game_ids.push(game.definition.id.clone());
                                        }
                                        if before && !game.enabled && index == self.selected_game {
                                            selected_game_was_disabled = true;
                                        }
                                    }
                                    ui.add_space(-4.0);
                                    ui.vertical(|ui| {
                                        ui.horizontal(|ui| {
                                            ui.add(
                                                egui::Label::new(
                                                    RichText::new(&game.definition.name)
                                                        .size(18.0),
                                                )
                                                .selectable(false),
                                            ).on_hover_cursor(egui::CursorIcon::PointingHand);
                                            ui.add(
                                                egui::Label::new(
                                                    RichText::new(&game.definition.xxmi_code)
                                                        .small()
                                                        .color(Color32::from_gray(145)),
                                                )
                                                .selectable(false),
                                            ).on_hover_cursor(egui::CursorIcon::PointingHand);
                                            let before = game.enabled;
                                            if toggle_switch(ui, &mut game.enabled).on_hover_cursor(egui::CursorIcon::PointingHand).changed() {
                                                should_save = true;
                                                if !before && game.enabled {
                                                    enabled_game_ids.push(game.definition.id.clone());
                                                }
                                                if before && !game.enabled && index == self.selected_game {
                                                    selected_game_was_disabled = true;
                                                }
                                            }
                                        });
                                        let browse_width = 28.0;
                                        let input_width = 200.0;
                                        let warn_color = Color32::from_rgb(124, 45, 58);
                                        let err_stroke = egui::Stroke::new(1.0, warn_color);
                                        ui.scope(|ui| {
                                            let radius = egui::CornerRadius::same(3);
                                            ui.style_mut().visuals.widgets.inactive.corner_radius = radius;
                                            ui.style_mut().visuals.widgets.hovered.corner_radius = radius;
                                            ui.style_mut().visuals.widgets.active.corner_radius = radius;
                                            ui.style_mut().visuals.widgets.open.corner_radius = radius;
                                            ui.spacing_mut().item_spacing.y = 3.0;

                                            // Vanilla game exe
                                            if game.enabled {
                                                let vanilla_invalid = game
                                                    .vanilla_exe_path_override
                                                    .as_ref()
                                                    .map(|path| !path.is_file())
                                                    .unwrap_or(true);
                                                ui.horizontal(|ui| {
                                                    ui.label(
                                                        RichText::new("Game EXE file:")
                                                            .small()
                                                            .color(Color32::from_gray(165)),
                                                    ).on_hover_cursor(egui::CursorIcon::Default);
                                                    if vanilla_invalid {
                                                        ui.label(icon_rich(Icon::TriangleAlert, 13.0, warn_color))
                                                        .on_hover_cursor(egui::CursorIcon::Default);
                                                        ui.add_space(-8.0);
                                                        ui.label(
                                                            RichText::new("Path not found")
                                                                .small()
                                                                .color(warn_color),
                                                        ).on_hover_cursor(egui::CursorIcon::Default);
                                                    }
                                                });
                                                ui.horizontal(|ui| {
                                                    let input_id = egui::Id::new((
                                                        "settings_vanilla_path",
                                                        game.definition.id.as_str(),
                                                    ));
                                                    let current_vanilla_value = game
                                                        .vanilla_exe_path_override
                                                        .as_ref()
                                                        .map(|path| path.to_string_lossy().to_string())
                                                        .unwrap_or_default();
                                                    let mut vanilla_value = ui
                                                        .data_mut(|d| d.get_temp::<String>(input_id))
                                                        .unwrap_or_else(|| current_vanilla_value.clone());
                                                    let vanilla_dirty = vanilla_value != current_vanilla_value;
                                                    let invalid = vanilla_value.trim().is_empty() || !Path::new(&vanilla_value).is_file();
                                                    let resp = ui.add(
                                                        TextEdit::singleline(&mut vanilla_value)
                                                            .id(input_id)
                                                            .horizontal_align(egui::Align::RIGHT)
                                                            .cursor_at_end(true)
                                                            .desired_width(input_width),
                                                    );
                                                    ui.data_mut(|d| d.insert_temp(input_id, vanilla_value.clone()));
                                                    if invalid {
                                                        ui.painter().rect_stroke(
                                                            resp.rect.expand(1.0),
                                                            4.0,
                                                            err_stroke,
                                                            egui::StrokeKind::Inside,
                                                        );
                                                    }
                                                    ui.add_space(-8.0);
                                                    let action_clicked = ui.scope(|ui| {
                                                        ui.spacing_mut().button_padding = Vec2::new(6.0, 2.0);
                                                        ui.add_sized(
                                                            [browse_width, resp.rect.height()],
                                                            egui::Button::new(
                                                                if vanilla_dirty {
                                                                    icon_rich(
                                                                        Icon::Check,
                                                                        16.0,
                                                                        Color32::from_rgb(
                                                                            110, 194, 132,
                                                                        ),
                                                                    )
                                                                } else {
                                                                    RichText::new(
                                                                        icon_char(Icon::Ellipsis)
                                                                            .to_string(),
                                                                    )
                                                                    .family(FontFamily::Name(
                                                                        LUCIDE_FAMILY.into(),
                                                                    ))
                                                                    .size(16.0)
                                                                },
                                                            ),
                                                        )
                                                        .clicked()
                                                    })
                                                    .inner;
                                                    let submit_with_enter = resp.lost_focus()
                                                        && ui.input(|i| i.key_pressed(egui::Key::Enter))
                                                        && vanilla_dirty;
                                                    if vanilla_dirty && (action_clicked || submit_with_enter) {
                                                        game.vanilla_exe_path_override =
                                                            non_empty(vanilla_value).map(PathBuf::from);
                                                        ui.data_mut(|d| d.remove::<String>(input_id));
                                                        should_save = true;
                                                    } else if !vanilla_dirty && action_clicked {
                                                        if let Some(path) = FileDialog::new()
                                                            .add_filter("Executable", &["exe"])
                                                            .pick_file()
                                                        {
                                                            game.vanilla_exe_path_override = Some(path);
                                                            ui.data_mut(|d| d.remove::<String>(input_id));
                                                            should_save = true;
                                                        }
                                                    }
                                                });
                                                ui.add_space(3.0);
                                            }

                                            if game.enabled && !self.state.use_default_mods_path {
                                                let mods_invalid = game
                                                    .mods_path(self.state.use_default_mods_path)
                                                    .map(|path| !path.is_dir())
                                                    .unwrap_or(true);
                                                ui.horizontal(|ui| {
                                                    ui.label(
                                                        RichText::new(format!("{} Mods Folder:", game.definition.xxmi_code))
                                                            .small()
                                                            .color(Color32::from_gray(165)),
                                                    ).on_hover_cursor(egui::CursorIcon::Default);
                                                    if mods_invalid {
                                                        ui.label(icon_rich(Icon::TriangleAlert, 13.0, warn_color))
                                                        .on_hover_cursor(egui::CursorIcon::Default);
                                                        ui.add_space(-8.0);
                                                        ui.label(
                                                            RichText::new("Path not found")
                                                                .small()
                                                                .color(warn_color),
                                                        ).on_hover_cursor(egui::CursorIcon::Default);
                                                    }
                                                });
                                                ui.horizontal(|ui| {
                                                    let input_id = egui::Id::new((
                                                        "settings_mods_path",
                                                        game.definition.id.as_str(),
                                                    ));
                                                    let current_path_value = game
                                                        .mods_path(self.state.use_default_mods_path)
                                                        .map(|path| path.to_string_lossy().to_string())
                                                        .unwrap_or_default();
                                                    let mut path_value = ui
                                                        .data_mut(|d| d.get_temp::<String>(input_id))
                                                        .unwrap_or_else(|| current_path_value.clone());
                                                    let path_dirty = path_value != current_path_value;
                                                    let invalid = path_value.trim().is_empty() || !Path::new(&path_value).is_dir();
                                                    let resp = ui.add(
                                                        TextEdit::singleline(&mut path_value)
                                                            .id(input_id)
                                                            .horizontal_align(egui::Align::RIGHT)
                                                            .cursor_at_end(true)
                                                            .desired_width(input_width),
                                                    );
                                                    ui.data_mut(|d| d.insert_temp(input_id, path_value.clone()));
                                                    if invalid {
                                                        ui.painter().rect_stroke(
                                                            resp.rect.expand(1.0),
                                                            4.0,
                                                            err_stroke,
                                                            egui::StrokeKind::Inside,
                                                        );
                                                    }
                                                    ui.add_space(-8.0);
                                                    let action_clicked = ui.scope(|ui| {
                                                        ui.spacing_mut().button_padding = Vec2::new(6.0, 2.0);
                                                        ui.add_sized(
                                                            [browse_width, resp.rect.height()],
                                                            egui::Button::new(
                                                                if path_dirty {
                                                                    icon_rich(
                                                                        Icon::Check,
                                                                        16.0,
                                                                        Color32::from_rgb(
                                                                            110, 194, 132,
                                                                        ),
                                                                    )
                                                                } else {
                                                                    RichText::new(
                                                                        icon_char(Icon::Ellipsis)
                                                                            .to_string(),
                                                                    )
                                                                    .family(FontFamily::Name(
                                                                        LUCIDE_FAMILY.into(),
                                                                    ))
                                                                    .size(16.0)
                                                                },
                                                            ),
                                                        )
                                                        .clicked()
                                                    })
                                                    .inner;
                                                    let submit_with_enter = resp.lost_focus()
                                                        && ui.input(|i| i.key_pressed(egui::Key::Enter))
                                                        && path_dirty;
                                                    if path_dirty && (action_clicked || submit_with_enter) {
                                                        game.mods_path_override =
                                                            non_empty(path_value).map(PathBuf::from);
                                                        ui.data_mut(|d| d.remove::<String>(input_id));
                                                        should_save = true;
                                                    } else if !path_dirty && action_clicked {
                                                        if let Some(path) = FileDialog::new().pick_folder() {
                                                            game.mods_path_override = Some(path);
                                                            ui.data_mut(|d| d.remove::<String>(input_id));
                                                            should_save = true;
                                                        }
                                                    }
                                                });
                                            }
                                        });
                                    });
                                });
                            });
                        ui.add_space(6.0);
                    }
                    if selected_game_was_disabled {
                        self.ensure_selected_game_enabled(ctx);
                    }
                    for game_id in enabled_game_ids {
                        if self.auto_detect_enabled_game_paths(&game_id) {
                            ui.data_mut(|data| {
                                data.remove::<String>(
                                    egui::Id::new(("settings_vanilla_path", game_id.as_str())),
                                );
                                data.remove::<String>(
                                    egui::Id::new(("settings_mods_path", game_id.as_str())),
                                );
                            });
                        }
                        self.queue_game_refresh(game_id);
                    }
                    }
                    SettingsTab::Advanced => {
                        static_label(ui, bold("Content Restriction").underline().size(16.0));
                        ui.indent("setting_advanced_nsfw", |ui| {
                            static_label(ui, "Hide Unsafe Contents:");
                            ui.add_space(-4.0);
                            let unsafe_mode = self.state.unsafe_content_mode;
                            egui::ComboBox::from_id_salt("unsafe_content_mode")
                                .selected_text(match unsafe_mode {
                                    UnsafeContentMode::HideNoCounter => "Hide NSFW mods, hide counter",
                                    UnsafeContentMode::HideShowCounter => "Hide NSFW mods, show counter",
                                    UnsafeContentMode::Censor => "Show with images censored",
                                    UnsafeContentMode::Show => "Show unrestricted",
                                })
                                .show_ui(ui, |ui| {
                                    ui.selectable_value(&mut self.state.unsafe_content_mode, UnsafeContentMode::HideNoCounter, "Hide NSFW mods, hide counter");
                                    ui.selectable_value(&mut self.state.unsafe_content_mode, UnsafeContentMode::HideShowCounter, "Hide NSFW mods, show counter");
                                    ui.selectable_value(&mut self.state.unsafe_content_mode, UnsafeContentMode::Censor, "Show with images censored");
                                    ui.selectable_value(&mut self.state.unsafe_content_mode, UnsafeContentMode::Show, "Show unrestricted");
                                });
                            if self.state.unsafe_content_mode != unsafe_mode { should_save = true; }
                            ui.add_space(1.0);
                        });
                        ui.add_space(24.0);

                        static_label(ui, bold("Cache and Archive").underline().size(16.0));
                        ui.indent("setting_advanced_cache", |ui| {
                            self.refresh_usage_counters_if_needed(ui.input(|i| i.time));
                            static_label(ui, "Cache size:");
                            ui.add_space(-4.0);
                            let previous_tier = self.state.cache_size_tier;
                            egui::ComboBox::from_id_salt("cache_size_tier")
                                .selected_text(self.state.cache_size_tier.label())
                                .show_ui(ui, |ui| {
                                    ui.selectable_value(&mut self.state.cache_size_tier, CacheSizeTier::Gb2, "2 GB");
                                    ui.selectable_value(&mut self.state.cache_size_tier, CacheSizeTier::Gb4, "4 GB");
                                    ui.selectable_value(&mut self.state.cache_size_tier, CacheSizeTier::Gb8, "8 GB");
                                    ui.selectable_value(&mut self.state.cache_size_tier, CacheSizeTier::Gb16, "16 GB");
                                });
                            if self.state.cache_size_tier != previous_tier {
                                self.cache_limit_bytes
                                    .store(self.state.cache_size_tier.bytes(), Ordering::Relaxed);
                                let _ = persistence::evict_lru_if_needed(
                                    &self.portable,
                                    self.state.cache_size_tier.bytes(),
                                );
                                self.mark_usage_counters_dirty();
                                should_save = true;
                            }
                            ui.add_space(8.0);
                            let usage_gb =
                                self.usage_cache_bytes as f64 / (1024.0 * 1024.0 * 1024.0);
                            let usage_text = format!("Current Usage: {:.2} GB", usage_gb);
                            static_label(ui, usage_text);
                            ui.add_space(-4.0);
                            if ui
                                .button(icon_text_sized(Icon::Trash2, "Clear Cache", 14.5, 13.0))
                                .on_hover_cursor(egui::CursorIcon::PointingHand)
                                .clicked()
                            {
                                match self.clear_cache() {
                                    Ok(()) => self.set_message_ok("Cache cleared"),
                                    Err(err) => self.report_error(err, Some("Could not clear cache")),
                                }
                            }
                            ui.add_space(8.0);
                            let archive_gb =
                                self.usage_archive_bytes as f64 / (1024.0 * 1024.0 * 1024.0);
                            static_label(ui, format!("Archive Usage: {:.2} GB", archive_gb));
                            ui.add_space(-4.0);
                            if ui
                                .button(icon_text_sized(Icon::Trash2, "Delete Archived Mods", 14.5, 13.0))
                                .on_hover_cursor(egui::CursorIcon::PointingHand)
                                .clicked()
                            {
                                match self.clear_archives() {
                                    Ok(count) => {
                                        if count > 0 {
                                            let action = match self.state.delete_behavior {
                                                DeleteBehavior::RecycleBin => "Recycled",
                                                DeleteBehavior::Permanent => "Deleted",
                                            };
                                            self.log_action(action, &format!("{count} archived mods"));
                                            self.set_message_ok(format!("Archives cleared: {}", count));
                                        } else {
                                            self.set_message_ok("No archives to clear");
                                        }
                                        self.refresh();
                                    }
                                    Err(err) => self.report_error(err, Some("Could not clear archives")),
                                }
                            }
                            ui.add_space(1.0);
                        });
                        ui.add_space(24.0);
                    }
                    SettingsTab::About => {
                        static_label(ui, bold(APP_NAME).underline().size(16.0));
                        ui.indent("setting_about_app", |ui| {
                            ui.add_space(-2.0);
                            static_label(ui, format!("by {APP_AUTHORS}"));
                            ui.add_space(-8.0);
                            ui.hyperlink("https://github.com/HenryNugraha/Hestia");
                            ui.add_space(2.0);
                            static_label(ui, format!("Version: {APP_VERSION}"));
                            ui.add_space(-6.0);
                            let now = ui.input(|i| i.time);
                            let label = self.app_update_button_label(now);
                            let enabled = self.app_update_button_enabled(now);
                            ui.horizontal(|ui|{
                                ui.add_space(-2.0);
                                let response = ui.add_enabled(
                                    enabled,
                                    egui::Button::new(icon_text_sized(
                                        if label == "Checking..." {
                                            Icon::LoaderCircle
                                        } else if label == "Restart to Update" {
                                            Icon::RotateCw
                                        } else {
                                        Icon::RefreshCw
                                    },
                                    label,
                                        11.5,
                                        11.5,
                                    ))
                                    .corner_radius(egui::CornerRadius::same(3)),
                                );
                                if response.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                                    if self.app_update_verified_path.is_some() {
                                        self.restart_to_update();
                                    } else {
                                        self.request_app_update_check(now);
                                    }
                                }
                            });
                            ui.add_space(-6.0);
                                if ui
                                    .checkbox(
                                        &mut self.state.automatically_check_for_update,
                                        "Automatically check for update",
                                    )
                                    .changed()
                                {
                                    should_save = true;
                                }
                            ui.add_space(1.0);
                        });
                        ui.add_space(24.0);

                        static_label(ui, bold("Attribution").underline().size(16.0));
                        ui.indent("setting_about_attributions", |ui| {
                            static_label(ui, "Data source: GameBanana, API used with permission. GameBanana mod metadata, media, and browse data are sourced from GameBanana.");
                            ui.add_space(1.0);
                        });
                        ui.add_space(24.0);
                    }
                }
            });
        });
        let _ = settings_response;
        self.settings_open = settings_open;
        if should_save {
            self.save_state();
        }
    }

}
