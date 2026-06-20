impl HestiaApp {
    fn render_whats_new_window(&mut self, ctx: &egui::Context) {
        if !self.state.show_whats_new {
            return;
        }
        let mut whats_new_open = self.state.show_whats_new;
        let force_default_pos = self.whats_new_force_default_pos;
        let text = self.text();
        let window_frame = egui::Frame::window(&ctx.style()).inner_margin(egui::Margin::same(16));
        let mut window = egui::Window::new(icon_text_sized(Icon::Bell, text.whats_new(), 14.0, 14.0))
            .id(egui::Id::new((
                "whats_new_window",
                self.whats_new_window_nonce,
            )))
            .open(&mut whats_new_open)
            .title_bar(true)
            .resizable(false)
            .collapsible(true)
            .frame(window_frame);

        if let Some(rect) = self.last_right_pane_rect {
            let inset_rect = rect.shrink2(egui::vec2(12.0, 12.0));
            let window_offset = egui::vec2(4.0, 4.0);
            let window_size = egui::vec2(460.0, 220.0);
            window = window
                .movable(true)
                .fixed_size(window_size)
                .constrain_to(inset_rect);
            if force_default_pos {
                let top_right = egui::pos2(inset_rect.max.x, inset_rect.min.y);
                window =
                    window.fixed_pos(top_right - egui::vec2(window_size.x, 0.0) - window_offset);
            }
        } else {
            window = window.default_width(460.0).default_height(220.0);
        }

        window.show(ctx, |ui| {
            ui.horizontal(|ui| {
                if feedback_survey().is_some() {
                    let version_response = ui
                        .add(
                            egui::Label::new(bold(format!("Hestia {APP_VERSION}"), Some(16.0)).underline())
                                .selectable(false)
                                .sense(Sense::click()),
                        )
                        .on_hover_text(text.whats_new_feedback_survey_tooltip())
                        .on_hover_cursor(egui::CursorIcon::PointingHand);
                    if version_response.clicked() {
                        self.open_feedback_survey_window();
                    }
                } else {
                    static_label(
                        ui,
                        bold(format!("Hestia {APP_VERSION}"), Some(16.0)).underline(),
                    );
                }
                ui.add_space(-4.0);
                ui.vertical(|ui| {
                    ui.add_space(5.0);
                    let whats_new_date = WHATS_NEW_DATE.get(self.state.static_prefs.language);
                    static_label(
                        ui,
                        RichText::new(whats_new_date)
                            .italics()
                            .size(11.0)
                            .small(),
                    );
                });
            });
            for highlight in WHATS_NEW_HIGHLIGHTS {
                let highlight = highlight.get(self.state.static_prefs.language);
                ui.horizontal_top(|ui| {
                    ui.allocate_ui_with_layout(
                        egui::vec2(12.0, 18.0),
                        egui::Layout::top_down(egui::Align::LEFT),
                        |ui| {
                            static_label(ui, "•");
                        },
                    );
                    ui.add(egui::Label::new(highlight).wrap().selectable(false))
                        .on_hover_cursor(egui::CursorIcon::Default);
                });
            }
        });

        if force_default_pos {
            self.whats_new_force_default_pos = false;
        }
        self.state.show_whats_new = whats_new_open;
    }

    fn render_feedback_survey_window(&mut self, ctx: &egui::Context) {
        if !self.state.show_feedback_survey {
            return;
        }

        let Some(survey) = feedback_survey() else {
            self.state.show_feedback_survey = false;
            return;
        };
        let text = self.text();
        let mut survey_open = self.state.show_feedback_survey;
        let force_default_pos = self.feedback_survey_force_default_pos;
        let window_frame = egui::Frame::window(&ctx.style()).inner_margin(egui::Margin::same(16));
        let survey_title = survey.title.get(self.state.static_prefs.language);
        let mut window =
            egui::Window::new(icon_text_sized(Icon::ClipboardList, survey_title, 14.0, 14.0))
                .id(egui::Id::new((
                    "feedback_survey_window",
                    self.feedback_survey_window_nonce,
                )))
                .open(&mut survey_open)
                .title_bar(true)
                .resizable(false)
                .collapsible(true)
                .frame(window_frame);

        let window_size = egui::vec2(420.0, 420.0);

        window = window.fixed_size(window_size);

        if let Some(rect) = self.last_right_pane_rect {
            let inset_rect = rect.shrink2(egui::vec2(12.0, 12.0));
            let window_offset = egui::vec2(4.0, 4.0);
            window = window.movable(true).constrain_to(inset_rect);
            if force_default_pos {
                let top_right = egui::pos2(inset_rect.max.x, inset_rect.min.y);
                window =
                    window.fixed_pos(top_right - egui::vec2(window_size.x, 0.0) - window_offset);
            }
        } else {
            window = window.default_size(window_size);
        }

        enum SurveyAction {
            Submit,
            MaybeLater,
            SkipVersion,
            NeverShow,
        }

        let mut action = None;
        window.show(ctx, |ui| {
            let content_width = (window_size.x - 40.0).max(80.0);
            ui.set_max_width(content_width);
            let content_height = (window_size.y - 160.0).max(48.0);
            ui.allocate_ui_with_layout(
                egui::vec2(content_width.min(ui.available_width()), content_height),
                egui::Layout::top_down(egui::Align::LEFT),
                |ui| {
                    ScrollArea::vertical()
                        .max_height(content_height)
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            for question in survey.questions {
                                ui.indent(("feedback_survey_question", question.id), |ui| {
                                    ui.label(question.prompt.get(self.state.static_prefs.language));
                                    ui.horizontal_wrapped(|ui| {
                                        let selected = self
                                            .feedback_survey_answers
                                            .entry(question.id.to_string())
                                            .or_insert(0);
                                        for answer in question.answers {
                                            ui.radio_value(
                                                selected,
                                                answer.id,
                                                answer.label.get(self.state.static_prefs.language),
                                            )
                                            .on_hover_cursor(egui::CursorIcon::PointingHand);
                                        }
                                    });
                                });
                                ui.add_space(6.0);
                            }

                            let message_label = survey.message_label.get(self.state.static_prefs.language);
                            if !message_label.trim().is_empty() {
                                ui.label(message_label);
                                ui.add(
                                    TextEdit::multiline(&mut self.feedback_survey_message)
                                        .desired_rows(5)
                                        .hint_text(
                                            RichText::new(text.feedback_survey_optional())
                                                .color(Color32::from_gray(120)),
                                        ),
                                );
                            }
                        });
                },
            );

            ui.add_space(2.0);

            let has_answer = survey.questions.iter().any(|question| {
                self.feedback_survey_answers
                    .get(question.id)
                    .is_some_and(|answer| *answer != 0)
            });
            let has_message = self.feedback_survey_message.trim().chars().count() >= 4;
            let complete = has_answer || has_message;

            ui.horizontal(|ui| {
                let submit_label =
                    text.feedback_survey_submit_label(self.feedback_survey_submitting);
                let submit_button = egui::Button::new(submit_label)
                    .fill(Color32::from_rgb(180, 78, 35))
                    .stroke(egui::Stroke::new(1.0, Color32::from_rgb(203, 104, 59)));
                let submit_enabled = complete && !self.feedback_survey_submitting;
                let submit_response = ui.add_enabled(submit_enabled, submit_button).on_hover_cursor(
                    if submit_enabled {
                        egui::CursorIcon::PointingHand
                    } else {
                        egui::CursorIcon::NotAllowed
                    },
                );
                if submit_response.clicked() {
                    action = Some(SurveyAction::Submit);
                }

                ui.menu_button(text.feedback_survey_dismiss(), |ui| {
                    if ui
                        .button(text.feedback_survey_remind_later())
                        .on_hover_cursor(egui::CursorIcon::PointingHand)
                        .clicked()
                    {
                        action = Some(SurveyAction::MaybeLater);
                        ui.close();
                    }
                    if ui
                        .button(text.feedback_survey_skip_version())
                        .on_hover_cursor(egui::CursorIcon::PointingHand)
                        .clicked()
                    {
                        action = Some(SurveyAction::SkipVersion);
                        ui.close();
                    }
                    if ui
                        .button(text.feedback_survey_never_ask_again())
                        .on_hover_cursor(egui::CursorIcon::PointingHand)
                        .clicked()
                    {
                        action = Some(SurveyAction::NeverShow);
                        ui.close();
                    }
                })
                .response
                .on_hover_cursor(egui::CursorIcon::PointingHand);

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let privacy_response = ui
                        .add(
                            egui::Label::new(
                                RichText::new(text.feedback_survey_privacy_details())
                                    .size(10.0)
                                    .color(Color32::from_gray(170)),
                            )
                            .selectable(false)
                            .sense(Sense::click()),
                        )
                        .on_hover_cursor(egui::CursorIcon::PointingHand);
                    if privacy_response.clicked() {
                        self.feedback_survey_privacy_expanded =
                            !self.feedback_survey_privacy_expanded;
                    }
                });
            });

            if self.feedback_survey_privacy_expanded {
                ui.add_space(6.0);
                ui.separator();
                static_label(
                    ui,
                    RichText::new(text.feedback_survey_privacy_copy())
                        .size(11.0)
                        .color(Color32::from_gray(180)),
                );
                ui.add_space(-4.0);
                let mut payload_preview = self.feedback_survey_payload_json();
                ui.add(
                    TextEdit::multiline(&mut payload_preview)
                        .desired_rows(6)
                        .code_editor()
                        .interactive(true),
                );
                ui.add_space(-4.0);
                static_label(
                    ui,
                    RichText::new(
                        text.feedback_survey_privacy_payload(FEEDBACK_SURVEY_SERVER_URL),
                    )
                        .size(11.0)
                        .color(Color32::from_gray(180)),
                );
                ui.vertical(|ui| {
                    ui.label(
                        RichText::new(text.feedback_survey_results_header())
                            .size(11.0)
                            .color(Color32::from_gray(180)),
                    );
                    ui.add_space(-10.0);
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new(text.feedback_survey_results_ongoing())
                                .size(11.0)
                                .color(Color32::from_gray(180)),
                        );
                        ui.add_space(-10.0);
                        ui.hyperlink_to(
                            RichText::new("hestia-survey.hnawc.com/ongoing")
                                .size(11.0),
                            "https://hestia-survey.hnawc.com/ongoing",
                        );
                    });
                    ui.add_space(-14.0);
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new(text.feedback_survey_results_previous())
                                .size(11.0)
                                .color(Color32::from_gray(180)),
                        );
                        ui.add_space(-10.0);
                        ui.hyperlink_to(
                            RichText::new("hestia-survey.hnawc.com/previous")
                                .size(11.0),
                            "https://hestia-survey.hnawc.com/previous",
                        );
                    });
                });
            }
        });

        if force_default_pos {
            self.feedback_survey_force_default_pos = false;
        }

        match action {
            Some(SurveyAction::Submit) => {
                self.submit_feedback_survey(self.feedback_survey_payload_json());
            }
            Some(SurveyAction::MaybeLater) => {
                self.state.defer_feedback_survey(survey);
                self.save_state();
            }
            Some(SurveyAction::SkipVersion) => {
                self.state.skip_feedback_survey(survey);
                self.feedback_survey_answers.clear();
                self.feedback_survey_message.clear();
                self.feedback_survey_privacy_expanded = false;
                self.save_state();
            }
            Some(SurveyAction::NeverShow) => {
                self.state.disable_feedback_surveys();
                self.feedback_survey_answers.clear();
                self.feedback_survey_message.clear();
                self.feedback_survey_privacy_expanded = false;
                self.save_state();
            }
            None => {
                if self.state.show_feedback_survey && !survey_open {
                    self.state.defer_feedback_survey(survey);
                    self.save_state();
                } else {
                    self.state.show_feedback_survey = survey_open;
                }
            }
        }
    }

    fn feedback_survey_payload_json(&self) -> String {
        let Some(survey) = feedback_survey() else {
            return "{}".to_string();
        };
        let mut answers = serde_json::Map::new();
        for question in survey.questions {
            if let Some(answer) = self.feedback_survey_answers.get(question.id) {
                if *answer != 0 {
                    answers.insert(
                        question.id.to_string(),
                        serde_json::Value::from(*answer),
                    );
                }
            }
        }

        let message = self.feedback_survey_message.trim();
        #[derive(serde::Serialize)]
        struct FeedbackSurveyPayload<'a> {
            client: String,
            version: &'a str,
            answers: serde_json::Map<String, serde_json::Value>,
            #[serde(skip_serializing_if = "Option::is_none")]
            message: Option<&'a str>,
        }

        let payload = FeedbackSurveyPayload {
            client: self.feedback_survey_client_hash(),
            version: survey.version,
            answers,
            message: (!message.is_empty()).then_some(message),
        };

        serde_json::to_string(&payload).unwrap_or_else(|_| "{}".to_string())
    }

    fn feedback_survey_client_hash(&self) -> String {
        use sha2::{Digest, Sha256};

        const SURVEY_CLIENT_HASH_SALT: &[u8] = b"hestia-feedback-survey-client-v1";
        let Some(client_id) = self.state.feedback_survey.client_id else {
            return String::new();
        };

        let mut hasher = Sha256::new();
        hasher.update(SURVEY_CLIENT_HASH_SALT);
        hasher.update(client_id.as_bytes());
        hasher
            .finalize()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect()
    }

    fn render_log_panel(&mut self, ctx: &egui::Context) {
        if !self.state.show_log {
            return;
        }
        let mut log_open = self.state.show_log;
        let stick_to_bottom = self.log_scroll_to_bottom;
        let just_opened = self.log_scroll_to_bottom;
        let force_default_pos = self.log_force_default_pos;
        let text = self.text();
        let log_frame = egui::Frame::window(&ctx.style()).inner_margin(egui::Margin::same(12));
        let mut window = egui::Window::new(icon_text_sized(Icon::FileCog, text.log(), 14.0, 14.0))
            .id(egui::Id::new(("log_window", self.log_window_nonce)))
            .open(&mut log_open)
            .title_bar(true)
            .frame(log_frame);

        if let Some(rect) = self.last_right_pane_rect {
            let inset_rect = rect.shrink2(egui::vec2(12.0, 12.0));
            let log_offset = egui::vec2(4.0, 4.0);
            let log_size = egui::vec2(460.0, 420.0);
            window = window
                .movable(true)
                .resizable(true)
                .constrain_to(inset_rect)
                .collapsible(true);
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
                            static_label(ui, bold(date.clone(), None).underline());
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
                        self.set_message_ok(text.log_copied());
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
        let text = self.text();
        let tasks_frame = egui::Frame::window(&ctx.style()).inner_margin(egui::Margin::same(12));
        let mut window = egui::Window::new(icon_text_sized(
            Icon::ListChecks,
            text.tasks_window(),
            14.0,
            14.0,
        ))
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
                window = window.fixed_pos(top_right - egui::vec2(tasks_size.x, 0.0) - tasks_offset);
            }
        } else if just_opened {
            window = window.default_width(460.0).default_height(420.0);
        }

        window.show(ctx, |ui| match self.state.tasks_layout {
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
                    !self
                        .browse_download_queue
                        .iter()
                        .any(|j| j.task_id == task.id)
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
                        [
                            egui::pos2(rect.left(), line_y),
                            egui::pos2(rect.right(), line_y),
                        ],
                        egui::Stroke::new(1.0, line_color),
                    );
                    let galley = ui.painter().layout_no_wrap(
                        label.to_string(),
                        egui::FontId::proportional(12.0),
                        Color32::from_gray(200),
                    );
                    let text_rect = egui::Rect::from_center_size(rect.center(), galley.size());
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
                let completed_height = (available_height - ongoing_height - section_gap).max(120.0);

                let ongoing_label = if ongoing.is_empty() {
                    text.tasks_ongoing().to_string()
                } else {
                    text.tasks_ongoing_count(ongoing.len())
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
                                    static_label(
                                        ui,
                                        RichText::new(text.no_active_tasks())
                                            .color(Color32::from_gray(140)),
                                    );
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
                    text.tasks_completed().to_string()
                } else {
                    text.tasks_completed_count(completed.len())
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
                                    static_label(
                                        ui,
                                        RichText::new(text.no_completed_tasks())
                                            .color(Color32::from_gray(140)),
                                    );
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
                                TaskStatus::Queued | TaskStatus::Installing | TaskStatus::Canceling
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
                            TaskStatus::Completed | TaskStatus::Failed | TaskStatus::Canceled
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
                    text.tasks_downloads_count(download_count)
                } else {
                    text.tasks_downloads().to_string()
                };
                let installs_label = if install_count > 0 {
                    text.tasks_installs_count(install_count)
                } else {
                    text.tasks_installs().to_string()
                };
                let completed_label = if completed_count > 0 {
                    text.tasks_completed_count(completed_count)
                } else {
                    text.tasks_completed().to_string()
                };
                let failed_label = if failed_count > 0 {
                    text.tasks_failed_count(failed_count)
                } else {
                    text.tasks_failed().to_string()
                };
                ui.horizontal(|ui| {
                    ui.selectable_value(&mut self.tasks_tab, TasksTab::Downloads, downloads_label);
                    ui.selectable_value(&mut self.tasks_tab, TasksTab::Installs, installs_label);
                    ui.selectable_value(&mut self.tasks_tab, TasksTab::Completed, completed_label);
                    ui.selectable_value(&mut self.tasks_tab, TasksTab::Failed, failed_label);
                });
                ui.add_space(6.0);

                ScrollArea::vertical()
                    .auto_shrink([false, true])
                    .show(ui, |ui| {
                        let items = match self.tasks_tab {
                            TasksTab::Downloads => self.sorted_tasks(|task| {
                                self.browse_download_queue
                                    .iter()
                                    .any(|j| j.task_id == task.id)
                                    || self.browse_download_inflight.contains_key(&task.id)
                            }),
                            TasksTab::Installs => self.sorted_tasks(|task| {
                                self.install_queue.iter().any(|j| j.id == task.id)
                                    || self.install_inflight.contains_key(&task.id)
                            }),
                            TasksTab::Completed => self.sorted_tasks(|task| {
                                !self
                                    .browse_download_queue
                                    .iter()
                                    .any(|j| j.task_id == task.id)
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
                            static_label(
                                ui,
                                RichText::new(text.no_tasks()).color(Color32::from_gray(140)),
                            );
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
                    self.state.tasks_order == TasksOrder::OldestFirst && self.tasks_scroll_to_edge;
                let scroll_to_top =
                    self.state.tasks_order == TasksOrder::NewestFirst && self.tasks_scroll_to_edge;
                ScrollArea::vertical()
                    .auto_shrink([false, true])
                    .stick_to_bottom(stick_to_bottom)
                    .show(ui, |ui| {
                        let items = self.sorted_tasks(|_| true);
                        if items.is_empty() {
                            static_label(
                                ui,
                                RichText::new(text.no_tasks()).color(Color32::from_gray(140)),
                            );
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
        let text = self.text();
        let tools_frame = egui::Frame::window(&ctx.style()).inner_margin(egui::Margin::same(12));
        let mut window = egui::Window::new(icon_text_sized(
            Icon::AppWindow,
            text.tools(),
            14.0,
            14.0,
        ))
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
                window = window.fixed_pos(top_right - egui::vec2(tools_size.x, 0.0) - tools_offset);
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
            let Some(_game) = self.selected_game().filter(|game| game.enabled).cloned() else {
                static_label(
                    ui,
                    RichText::new(text.no_game_selected()).color(Color32::from_gray(160)),
                );
                return;
            };

            let tools = self.selected_game_tools();
            for tool in &tools {
                self.ensure_tool_icon_texture(ctx, tool);
            }

            ScrollArea::vertical()
                .auto_shrink([false, true])
                .show(ui, |ui| {
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
                            let mut label_ui =
                                ui.new_child(egui::UiBuilder::new().max_rect(label_rect).layout(
                                    egui::Layout::centered_and_justified(egui::Direction::TopDown),
                                ));
                            let label_response = label_ui
                                .add(
                                    egui::Label::new(RichText::new(&tool.label).size(14.0).color(
                                        if is_missing && !tool.auto_detected {
                                            Color32::from_rgb(212, 122, 122)
                                        } else {
                                            Color32::from_rgb(228, 231, 235)
                                        },
                                    ))
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
                                                text.launch(),
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
                                            text.set_launch_options(),
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
                                            text.open_folder(),
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
                                                text.unpin_from_titlebar()
                                            } else {
                                                text.pin_to_titlebar()
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
                                            text.remove(),
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
                                            text.launch(),
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
                                        text.set_launch_options(),
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
                                        text.open_folder(),
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
                                            text.unpin_from_titlebar()
                                        } else {
                                            text.pin_to_titlebar()
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
                                        text.remove(),
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
                            let this_index =
                                tools.iter().position(|candidate| candidate.id == tool.id);
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
                                    if self
                                        .move_tool_window_order_to_slot(&dragging_id, target_index)
                                    {
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
                        let (add_rect, add_response) =
                            ui.allocate_exact_size(add_size, Sense::click());
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
                                [
                                    egui::pos2(x, outline.bottom()),
                                    egui::pos2(x2, outline.bottom()),
                                ],
                                stroke,
                            );
                            x += dash + gap;
                        }
                        let mut y = outline.top();
                        while y < outline.bottom() {
                            let y2 = (y + dash).min(outline.bottom());
                            ui.painter().line_segment(
                                [
                                    egui::pos2(outline.left(), y),
                                    egui::pos2(outline.left(), y2),
                                ],
                                stroke,
                            );
                            ui.painter().line_segment(
                                [
                                    egui::pos2(outline.right(), y),
                                    egui::pos2(outline.right(), y2),
                                ],
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
                            text.add_tool(),
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
                                        Some((
                                            next.left() - 3.0,
                                            next.top() + 8.0,
                                            next.bottom() - 8.0,
                                        ))
                                    }
                                } else {
                                    Some((
                                        add_rect.left() - 3.0,
                                        add_rect.top() + 8.0,
                                        add_rect.bottom() - 8.0,
                                    ))
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
                let ghost_rect =
                    egui::Rect::from_center_size(pointer_pos + egui::vec2(8.0, 10.0), size);
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
        let text = self.text();
        let constrain_rect = self
            .last_right_pane_rect
            .unwrap_or_else(|| ctx.viewport_rect());

        egui::Window::new(icon_text_sized(
            Icon::Terminal,
            text.tool_launch_options(),
            14.0,
            14.0,
        ))
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
                            RichText::new(text.tool_launch_options_hint())
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
                        egui::Button::new(text.save()).fill(Color32::from_rgb(180, 78, 35)),
                    )
                    .clicked()
                {
                    should_save = true;
                }
                if ui.button(text.cancel()).clicked() {
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

    fn finish_settings_category_drag(&mut self, game_id: &str) -> bool {
        let moved = if let Some(target_index) = self.settings_dragging_category_target_index {
            let moving_ids = self.settings_dragging_category_ids.clone();
            self.move_category_ids_to_slot(game_id, &moving_ids, target_index)
        } else {
            false
        };
        self.settings_dragging_category_ids.clear();
        self.settings_dragging_category_target_index = None;
        moved
    }

    fn update_settings_category_drag_target(
        &mut self,
        ui: &mut Ui,
        pointer_pos: Option<egui::Pos2>,
        category_row_rects: &[egui::Rect],
    ) {
        if self.settings_dragging_category_ids.is_empty()
            || !ui.input(|input| input.pointer.primary_down())
            || category_row_rects.is_empty()
        {
            return;
        }
        let Some(pointer_pos) = pointer_pos else {
            return;
        };
        let left = category_row_rects
            .iter()
            .map(|rect| rect.left())
            .fold(f32::INFINITY, f32::min);
        let right = category_row_rects
            .iter()
            .map(|rect| rect.right())
            .fold(f32::NEG_INFINITY, f32::max);
        let top = category_row_rects[0].top();
        let bottom = category_row_rects[category_row_rects.len() - 1].bottom();
        if pointer_pos.x >= left && pointer_pos.x <= right {
            if pointer_pos.y <= top {
                self.settings_dragging_category_target_index = Some(0);
                ui.ctx().request_repaint();
            } else if pointer_pos.y >= bottom {
                self.settings_dragging_category_target_index = Some(category_row_rects.len());
                ui.ctx().request_repaint();
            }
        }
    }

    fn paint_settings_category_drop_indicator(
        &self,
        ui: &mut Ui,
        category_row_rects: &[egui::Rect],
    ) {
        if self.settings_dragging_category_ids.is_empty()
            || !ui.input(|input| input.pointer.primary_down())
            || category_row_rects.is_empty()
        {
            return;
        }
        let Some(target_index) = self.settings_dragging_category_target_index else {
            return;
        };
        let clamped_index = target_index.min(category_row_rects.len());
        let y = if clamped_index == 0 {
            category_row_rects[0].top() + 1.0
        } else if clamped_index >= category_row_rects.len() {
            category_row_rects[category_row_rects.len() - 1].bottom() - 1.0
        } else {
            (category_row_rects[clamped_index - 1].bottom()
                + category_row_rects[clamped_index].top())
                * 0.5
        };
        let left = category_row_rects
            .iter()
            .map(|rect| rect.left())
            .fold(f32::INFINITY, f32::min);
        let right = category_row_rects
            .iter()
            .map(|rect| rect.right())
            .fold(f32::NEG_INFINITY, f32::max);
        ui.painter().line_segment(
            [egui::pos2(left, y), egui::pos2(right, y)],
            egui::Stroke::new(2.0, Color32::from_rgb(120, 170, 220)),
        );
    }

    fn render_category_sort_menu(&mut self, ui: &mut Ui, game_id: &str) {
        let text = self.text();
        let current_mode = self.category_sort_mode_for_game(game_id);
        let selected_label = text.library_category_sort_label(current_mode);
        ui.scope(|ui| {
            let radius = egui::CornerRadius::same(6);
            ui.style_mut().visuals.widgets.inactive.corner_radius = radius;
            ui.style_mut().visuals.widgets.hovered.corner_radius = radius;
            ui.style_mut().visuals.widgets.active.corner_radius = radius;
            ui.style_mut().visuals.widgets.open.corner_radius = radius;
            ui.menu_button(
                icon_text_sized(Icon::ArrowDownNarrowWide, selected_label, 13.0, 13.0),
                |ui| {
                    let options = [
                        ModCategorySortMode::ByNameAsc,
                        ModCategorySortMode::ByModCountAsc,
                        ModCategorySortMode::ByModCountDesc,
                    ];
                    for mode in options {
                        let active = current_mode == mode;
                        ui.horizontal(|ui| {
                            let icon = if active {
                                icon_rich(Icon::Check, 12.0, Color32::from_rgb(110, 194, 132))
                            } else {
                                RichText::new("")
                            };
                            ui.add_sized([18.0, 20.0], egui::Label::new(icon).selectable(false));
                            if ui
                                .button(text.library_category_sort_label(mode))
                                .on_hover_cursor(egui::CursorIcon::PointingHand)
                                .clicked()
                            {
                                self.set_category_sort_mode_for_game(game_id, mode);
                                ui.close();
                            }
                        });
                    }
                },
            )
            .response
            .on_hover_cursor(egui::CursorIcon::PointingHand);
        });
    }

    fn clamp_settings_category_name(text: &str) -> String {
        const MAX_CHARS: usize = 45;
        const PREFIX_CHARS: usize = 42;
        if text.chars().count() <= MAX_CHARS {
            return text.to_string();
        }
        let mut clamped: String = text.chars().take(PREFIX_CHARS).collect();
        clamped.truncate(clamped.trim_end().len());
        clamped.push_str("...");
        clamped
    }

    fn uncategorized_mod_count_for_game(
        &self,
        game_id: &str,
        category_ids: &HashSet<String>,
    ) -> usize {
        self.state
            .mods
            .iter()
            .filter(|mod_entry| {
                mod_entry.game_id == game_id
                    && mod_entry
                        .metadata
                        .user
                        .category_id
                        .as_ref()
                        .is_none_or(|category_id| !category_ids.contains(category_id))
            })
            .count()
    }

    fn render_categories_settings_tab(&mut self, ui: &mut Ui, should_save: &mut bool) {
        let text = self.text();
        let Some((game_id, game_name)) = self
            .selected_game()
            .map(|game| (game.definition.id.clone(), game.definition.name.clone()))
        else {
            static_label(ui, text.category_select_game());
            return;
        };

        static_label(ui, bold(text.category_browse(), Some(16.0)).underline());
        ui.indent("setting_categories_browse", |ui| {
            let game_has_categories = self
                .state
                .categories
                .iter()
                .any(|category| category.game_id == game_id);
            let preference_was_missing = !self
                .state
                .create_downloaded_mod_category_by_game
                .contains_key(&game_id);
            let enabled = self
                .state
                .create_downloaded_mod_category_by_game
                .entry(game_id.clone())
                .or_insert(!game_has_categories);
            let response = ui.checkbox(enabled, text.auto_create_gamebanana_categories());
            response
                .clone()
                .on_hover_text(text.applies_to_game(&game_name));
            if preference_was_missing || response.changed() {
                *should_save = true;
            }
            ui.add_space(1.0);
        });
        ui.add_space(24.0);

        let current_category_ids: HashSet<String> = self
            .state
            .categories
            .iter()
            .filter(|category| category.game_id == game_id)
            .map(|category| category.id.clone())
            .collect();
        self.selected_category_ids
            .retain(|category_id| current_category_ids.contains(category_id));
        let uncategorized_count =
            self.uncategorized_mod_count_for_game(&game_id, &current_category_ids);

        static_label(ui, bold(text.categories(), Some(16.0)).underline());
        let categories = self.categories_for_game(&game_id);
        let category_controls_width = (ui.available_width() - 48.0).max(0.0);
        ui.allocate_ui_with_layout(
            egui::vec2(category_controls_width, 24.0),
            egui::Layout::left_to_right(egui::Align::Center),
            |ui| {
                let has_selected_categories = !self.selected_category_ids.is_empty();
                let all_selected = !categories.is_empty()
                    && categories
                        .iter()
                        .all(|category| self.selected_category_ids.contains(&category.id));
                let select_all_response = ui.add_enabled(
                    !categories.is_empty(),
                    egui::Button::new(icon_rich(
                        Icon::SquaresIntersect,
                        13.0,
                        Color32::from_gray(220),
                    ))
                    .corner_radius(egui::CornerRadius::same(6)),
                );
                if select_all_response
                    .on_hover_text(if all_selected {
                        text.unselect_all_categories()
                    } else {
                        text.select_all_categories()
                    })
                    .on_hover_cursor(egui::CursorIcon::PointingHand)
                    .clicked()
                {
                    if all_selected {
                        for category in &categories {
                            self.selected_category_ids.remove(&category.id);
                        }
                    } else {
                        self.selected_category_ids
                            .extend(categories.iter().map(|category| category.id.clone()));
                    }
                }
                ui.add_space(-6.0);
                self.render_category_sort_menu(ui, &game_id);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui
                        .button(icon_text_sized(Icon::Plus, text.new_category(), 13.0, 13.0))
                        .on_hover_text(text.new_category_tooltip())
                        .on_hover_cursor(egui::CursorIcon::PointingHand)
                        .clicked()
                    {
                        self.create_category_for_game(
                            &game_id,
                            CategoryRenameSurface::Settings,
                        );
                    }
                    ui.add_space(-6.0);
                    if has_selected_categories
                        && ui
                            .button(icon_text_sized(Icon::Trash2, text.delete(), 13.0, 13.0))
                            .on_hover_cursor(egui::CursorIcon::PointingHand)
                            .clicked()
                    {
                        let selected_ids: Vec<String> =
                            self.selected_category_ids.iter().cloned().collect();
                        self.delete_categories(&selected_ids);
                    }
                });
            },
        );
        let pointer_pos = ui.ctx().pointer_latest_pos();
        let mut row_rects = Vec::new();
        ui.add_space(-4.0);
        egui::Frame::group(ui.style())
            .inner_margin(egui::Margin::same(4))
            .show(ui, |ui| {
                ui.set_min_height(180.0);
                ui.allocate_ui_with_layout(
                    egui::vec2(ui.available_width() - 48.0, 320.0),
                    egui::Layout::top_down(egui::Align::LEFT),
                    |ui| {
                        ui.style_mut().spacing.scroll.floating_allocated_width = 6.0;
                        ui.spacing_mut().item_spacing.x = 3.0;
                        ui.spacing_mut().item_spacing.y = 1.0;
                        egui::ScrollArea::vertical()
                            .max_height(320.0)
                            .auto_shrink([false, false])
                            .show(ui, |ui| {
                                ui.set_width(ui.available_width());
                                let category_drag_active =
                                    !self.settings_dragging_category_ids.is_empty();
                                let render_uncategorized_row = |ui: &mut Ui| {
                                    let (row_rect, _) = ui.allocate_exact_size(
                                        egui::vec2(ui.available_width(), 21.0),
                                        Sense::hover(),
                                    );
                                    let row_hovered = !category_drag_active
                                        && pointer_pos.is_some_and(|pos| row_rect.contains(pos));
                                    if row_hovered {
                                        ui.painter().rect_filled(
                                            row_rect.expand2(egui::vec2(2.0, 0.0)),
                                            egui::CornerRadius::same(3),
                                            Color32::from_rgba_unmultiplied(255, 255, 255, 38),
                                        );
                                    }
                                    let mut row_ui = ui.new_child(
                                        egui::UiBuilder::new().max_rect(row_rect).layout(
                                            egui::Layout::left_to_right(egui::Align::Center),
                                        ),
                                    );
                                    row_ui.spacing_mut().item_spacing.x = 3.0;
                                    row_ui.scope(|ui| {
                                        ui.spacing_mut().item_spacing.x = 3.0;
                                        ui.add_sized(
                                            [18.0, 20.0],
                                            egui::Label::new("").selectable(false),
                                        );
                                        let mut placeholder_checked = false;
                                        ui.add_visible(
                                            false,
                                            egui::Checkbox::new(&mut placeholder_checked, ""),
                                        );
                                        ui.allocate_ui_with_layout(
                                            egui::vec2(178.0, 20.0),
                                            egui::Layout::left_to_right(egui::Align::Center),
                                            |ui| {
                                                ui.add(
                                                    egui::Label::new(
                                                        RichText::new(text.uncategorized())
                                                            .color(Color32::from_gray(185)),
                                                    )
                                                    .selectable(false),
                                                );
                                            },
                                        );
                                        ui.add_space(-8.0);
                                        ui.add_sized(
                                            [34.0, 20.0],
                                            egui::Label::new(
                                                RichText::new(format!("({uncategorized_count})"))
                                                    .size(12.0)
                                                    .color(Color32::from_gray(145)),
                                            )
                                            .selectable(false),
                                        );
                                        ui.allocate_response(
                                            egui::vec2(18.0, 20.0),
                                            Sense::hover(),
                                        );
                                        ui.allocate_response(
                                            egui::vec2(18.0, 20.0),
                                            Sense::hover(),
                                        );
                                    });
                                };

                                if self.state.static_prefs.library_uncategorized_first {
                                    render_uncategorized_row(ui);
                                }
                                for (index, category) in categories.iter().enumerate() {
                                    let (row_rect, row_response) = ui.allocate_exact_size(
                                        egui::vec2(ui.available_width(), 21.0),
                                        Sense::hover(),
                                    );
                                    let selected =
                                        self.selected_category_ids.contains(&category.id);
                                    let row_hovered = !category_drag_active
                                        && pointer_pos.is_some_and(|pos| row_rect.contains(pos));
                                    let row_highlighted = row_hovered || selected;
                                    if row_highlighted {
                                        ui.painter().rect_filled(
                                            row_rect.expand2(egui::vec2(2.0, 0.0)),
                                            egui::CornerRadius::same(3),
                                            Color32::from_rgba_unmultiplied(255, 255, 255, 38),
                                        );
                                    }
                                    let mut row_ui = ui.new_child(
                                        egui::UiBuilder::new().max_rect(row_rect).layout(
                                            egui::Layout::left_to_right(egui::Align::Center),
                                        ),
                                    );
                                    row_ui.spacing_mut().item_spacing.x = 3.0;
                                    row_ui.scope(|ui| {
                                        ui.spacing_mut().item_spacing.x = 3.0;
                                        let drag_response = ui
                                            .add_sized(
                                                [18.0, 20.0],
                                                egui::Label::new(icon_rich(
                                                    Icon::GripVertical,
                                                    12.0,
                                                    Color32::from_gray(165),
                                                ))
                                                .selectable(false)
                                                .sense(Sense::click_and_drag()),
                                            )
                                            .on_hover_cursor(egui::CursorIcon::Grab);
                                        if drag_response.drag_started() {
                                            if self.selected_category_ids.contains(&category.id) {
                                                self.settings_dragging_category_ids = categories
                                                    .iter()
                                                    .filter(|category| {
                                                        self.selected_category_ids
                                                            .contains(&category.id)
                                                    })
                                                    .map(|category| category.id.clone())
                                                    .collect();
                                            } else {
                                                self.settings_dragging_category_ids =
                                                    vec![category.id.clone()];
                                            }
                                            self.settings_dragging_category_target_index =
                                                Some(index);
                                        }

                                        let mut selected = selected;
                                        if ui.checkbox(&mut selected, "").changed() {
                                            if selected {
                                                self.selected_category_ids
                                                    .insert(category.id.clone());
                                            } else {
                                                self.selected_category_ids.remove(&category.id);
                                            }
                                        }

                                        if self.category_rename_matches(
                                            &category.id,
                                            CategoryRenameSurface::Settings,
                                        )
                                        {
                                            let input = ui.add(
                                                TextEdit::singleline(
                                                    &mut self.category_rename_name,
                                                )
                                                .desired_width(178.0)
                                                .margin(egui::Margin::same(4)),
                                            );
                                            self.request_category_rename_focus(
                                                ui.ctx(),
                                                &input,
                                                &category.id,
                                            );
                                            let save_rename = ui.input_mut(|i| {
                                                i.consume_key(
                                                    egui::Modifiers::NONE,
                                                    egui::Key::Enter,
                                                )
                                            });
                                            let cancel_rename = ui.input_mut(|i| {
                                                i.consume_key(
                                                    egui::Modifiers::NONE,
                                                    egui::Key::Escape,
                                                )
                                            });
                                            if save_rename {
                                                let draft = self.category_rename_name.clone();
                                                self.rename_category(&category.id, &draft);
                                            }
                                            if cancel_rename {
                                                self.clear_category_rename();
                                            }
                                            if ui
                                                .add(
                                                    egui::Button::new(icon_rich(
                                                        Icon::Check,
                                                        13.0,
                                                        Color32::from_rgb(110, 194, 132),
                                                    ))
                                                    .frame(false),
                                                )
                                                .on_hover_cursor(egui::CursorIcon::PointingHand)
                                                .clicked()
                                            {
                                                let draft = self.category_rename_name.clone();
                                                self.rename_category(&category.id, &draft);
                                            }
                                        } else {
                                            let action_idle_alpha =
                                                if row_hovered { 77 } else { 0 };
                                            let label_response = ui
                                                .allocate_ui_with_layout(
                                                    egui::vec2(178.0, 20.0),
                                                    egui::Layout::left_to_right(
                                                        egui::Align::Center,
                                                    ),
                                                    |ui| {
                                                        ui.add(
                                                            egui::Label::new(
                                                                Self::clamp_settings_category_name(
                                                                    &category.name,
                                                                ),
                                                            )
                                                            .selectable(false),
                                                        )
                                                    },
                                                )
                                                .inner;
                                            label_response.on_hover_text(&category.name);
                                            ui.add_space(-8.0);
                                            ui.add_sized(
                                                [34.0, 20.0],
                                                egui::Label::new({
                                                    let count = self.category_member_count(
                                                        &game_id,
                                                        &category.id,
                                                    );
                                                    RichText::new(format!("({count})"))
                                                        .size(12.0)
                                                        .color(Color32::from_gray(145))
                                                })
                                                .selectable(false),
                                            );
                                            let edit_response = ui.allocate_response(
                                                egui::vec2(18.0, 20.0),
                                                Sense::click(),
                                            );
                                            let edit_alpha = if edit_response.hovered() {
                                                230
                                            } else {
                                                action_idle_alpha
                                            };
                                            ui.painter().text(
                                                edit_response.rect.center(),
                                                egui::Align2::CENTER_CENTER,
                                                icon_char(Icon::Pencil),
                                                egui::FontId::new(
                                                    12.0,
                                                    FontFamily::Name(LUCIDE_FAMILY.into()),
                                                ),
                                                Color32::from_rgba_unmultiplied(
                                                    175, 175, 175, edit_alpha,
                                                ),
                                            );
                                            if edit_response
                                                .on_hover_cursor(egui::CursorIcon::PointingHand)
                                                .clicked()
                                            {
                                                self.start_category_rename(
                                                    category.id.clone(),
                                                    category.name.clone(),
                                                    CategoryRenameSurface::Settings,
                                                );
                                            }
                                            let delete_response = ui.allocate_response(
                                                egui::vec2(18.0, 20.0),
                                                Sense::click(),
                                            );
                                            let delete_alpha = if delete_response.hovered() {
                                                230
                                            } else {
                                                action_idle_alpha
                                            };
                                            ui.painter().text(
                                                delete_response.rect.center(),
                                                egui::Align2::CENTER_CENTER,
                                                icon_char(Icon::Trash2),
                                                egui::FontId::new(
                                                    12.0,
                                                    FontFamily::Name(LUCIDE_FAMILY.into()),
                                                ),
                                                Color32::from_rgba_unmultiplied(
                                                    190,
                                                    120,
                                                    120,
                                                    delete_alpha,
                                                ),
                                            );
                                            if delete_response
                                                .on_hover_cursor(egui::CursorIcon::PointingHand)
                                                .clicked()
                                            {
                                                self.delete_category(&category.id);
                                            }
                                        }
                                    });
                                    row_rects.push(row_response.rect);
                                    if !self.settings_dragging_category_ids.is_empty()
                                        && pointer_pos
                                            .is_some_and(|pos| row_response.rect.contains(pos))
                                    {
                                        let insert_after = pointer_pos.is_some_and(|pos| {
                                            pos.y > row_response.rect.center().y
                                        });
                                        self.settings_dragging_category_target_index =
                                            Some(if insert_after {
                                                index.saturating_add(1)
                                            } else {
                                                index
                                            });
                                        ui.ctx().request_repaint();
                                    }
                                }
                                if !self.state.static_prefs.library_uncategorized_first {
                                    render_uncategorized_row(ui);
                                }
                                self.update_settings_category_drag_target(
                                    ui,
                                    pointer_pos,
                                    &row_rects,
                                );
                                self.paint_settings_category_drop_indicator(ui, &row_rects);
                            });
                    },
                );
            });

        if !self.settings_dragging_category_ids.is_empty()
            && !ui.ctx().input(|input| input.pointer.primary_down())
        {
            self.finish_settings_category_drag(&game_id);
        }
        if !self.settings_dragging_category_ids.is_empty()
            && ui.ctx().input(|input| input.pointer.primary_down())
        {
            ui.ctx()
                .output_mut(|output| output.cursor_icon = egui::CursorIcon::Grabbing);
        }
        ui.add_space(24.0);
    }

    fn render_settings_window(&mut self, ctx: &egui::Context) {
        if !self.settings_open {
            return;
        }
        let mut should_save = false;
        let mut update_check_targets_changed = false;
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

        let text = self.text();
        let mut settings_open = self.settings_open;
        let settings_frame = egui::Frame::window(&ctx.style()).inner_margin(egui::Margin::same(16));
        let mut window =
            egui::Window::new(icon_text_sized(Icon::Settings2, text.settings(), 14.0, 14.0))
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
                    bold(text.settings_tab_general().to_uppercase(), None),
                ).on_hover_cursor(egui::CursorIcon::PointingHand);
                ui.selectable_value(
                    &mut self.settings_tab,
                    SettingsTab::Categories,
                    bold(text.settings_tab_category().to_uppercase(), None),
                ).on_hover_cursor(egui::CursorIcon::PointingHand);
                ui.selectable_value(
                    &mut self.settings_tab,
                    SettingsTab::Advanced,
                    bold(text.settings_tab_advanced().to_uppercase(), None),
                ).on_hover_cursor(egui::CursorIcon::PointingHand);
                ui.selectable_value(
                    &mut self.settings_tab,
                    SettingsTab::Path,
                    bold(text.settings_tab_game_path().to_uppercase(), None),
                ).on_hover_cursor(egui::CursorIcon::PointingHand);
                ui.selectable_value(
                    &mut self.settings_tab,
                    SettingsTab::About,
                    bold(text.settings_tab_about().to_uppercase(), None),
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

                        let setting_block = |ui: &mut Ui, label: &str, add_control: &mut dyn FnMut(&mut Ui)| {
                            static_label(ui, label);
                            ui.add_space(-4.0);
                            add_control(ui);
                        };

                        static_label(ui, bold(text.behavior(), Some(16.0)).underline());
                        ui.indent("setting_general_behavior", |ui| {
                            let launch_behavior = self.state.static_prefs.launch_behavior;
                            let tool_launch_behavior = self.state.static_prefs.tool_launch_behavior;
                            let after_install = self.state.static_prefs.after_install_behavior;
                            let meta_vis = self.state.static_prefs.metadata_visibility;
                            let left_column_width = ui.available_width() * 0.32;
                            ui.horizontal_top(|ui| {
                                ui.vertical(|ui| {
                                    ui.set_width(left_column_width);
                                    setting_block(
                                        ui,
                                        text.when_launching_game(),
                                        &mut |ui| {
                                            egui::ComboBox::from_id_salt("launch_behavior")
                                                .selected_text(text.launch_behavior(self.state.static_prefs.launch_behavior))
                                                .show_ui(ui, |ui| {
                                                    ui.selectable_value(&mut self.state.static_prefs.launch_behavior, LaunchBehavior::DoNothing, text.launch_behavior(LaunchBehavior::DoNothing));
                                                    ui.selectable_value(&mut self.state.static_prefs.launch_behavior, LaunchBehavior::Minimize, text.launch_behavior(LaunchBehavior::Minimize));
                                                    ui.selectable_value(&mut self.state.static_prefs.launch_behavior, LaunchBehavior::Exit, text.launch_behavior(LaunchBehavior::Exit));
                                                });
                                        },
                                    );
                                    ui.add_space(8.0);
                                    setting_block(
                                        ui,
                                        text.after_installing_mod(),
                                        &mut |ui| {
                                            egui::ComboBox::from_id_salt("after_install_behavior")
                                                .selected_text(text.after_install_behavior(self.state.static_prefs.after_install_behavior))
                                                .show_ui(ui, |ui| {
                                                    ui.selectable_value(&mut self.state.static_prefs.after_install_behavior, AfterInstallBehavior::DoNothing, text.after_install_behavior(AfterInstallBehavior::DoNothing));
                                                    ui.selectable_value(&mut self.state.static_prefs.after_install_behavior, AfterInstallBehavior::AddToSelection, text.after_install_behavior(AfterInstallBehavior::AddToSelection));
                                                    ui.selectable_value(&mut self.state.static_prefs.after_install_behavior, AfterInstallBehavior::OpenModDetail, text.after_install_behavior(AfterInstallBehavior::OpenModDetail));
                                                });
                                        },
                                    );
                                });
                                ui.vertical(|ui| {
                                    setting_block(
                                        ui,
                                        text.when_launching_tool(),
                                        &mut |ui| {
                                            egui::ComboBox::from_id_salt("tool_launch_behavior")
                                                .selected_text(text.launch_behavior(self.state.static_prefs.tool_launch_behavior))
                                                .show_ui(ui, |ui| {
                                                    ui.selectable_value(&mut self.state.static_prefs.tool_launch_behavior, LaunchBehavior::DoNothing, text.launch_behavior(LaunchBehavior::DoNothing));
                                                    ui.selectable_value(&mut self.state.static_prefs.tool_launch_behavior, LaunchBehavior::Minimize, text.launch_behavior(LaunchBehavior::Minimize));
                                                    ui.selectable_value(&mut self.state.static_prefs.tool_launch_behavior, LaunchBehavior::Exit, text.launch_behavior(LaunchBehavior::Exit));
                                                });
                                        },
                                    );
                                    ui.add_space(8.0);
                                    setting_block(ui, text.mod_detail_metadata(), &mut |ui| {
                                        egui::ComboBox::from_id_salt("metadata_visibility")
                                            .selected_text(text.metadata_visibility(self.state.static_prefs.metadata_visibility))
                                            .show_ui(ui, |ui| {
                                                ui.selectable_value(&mut self.state.static_prefs.metadata_visibility, MetadataVisibility::Never, text.metadata_visibility(MetadataVisibility::Never));
                                                ui.selectable_value(&mut self.state.static_prefs.metadata_visibility, MetadataVisibility::OnlyIfNoDescription, text.metadata_visibility(MetadataVisibility::OnlyIfNoDescription));
                                                ui.selectable_value(&mut self.state.static_prefs.metadata_visibility, MetadataVisibility::Always, text.metadata_visibility(MetadataVisibility::Always));
                                            });
                                    });
                                });
                            });
                            if self.state.static_prefs.launch_behavior != launch_behavior {
                                should_save = true;
                            }
                            if self.state.static_prefs.tool_launch_behavior != tool_launch_behavior {
                                should_save = true;
                            }
                            if self.state.static_prefs.after_install_behavior != after_install {
                                should_save = true;
                            }
                            if self.state.static_prefs.metadata_visibility != meta_vis {
                                should_save = true;
                            }
                            ui.add_space(1.0);
                        });
                        ui.add_space(24.0);

                        static_label(ui, bold(text.installed_mods_list(), Some(16.0)).underline());
                        ui.indent("setting_general_installed_mods_list", |ui| {
                            let group_mode = self.state.static_prefs.library_group_mode;
                            let category_display_mode = self.state.static_prefs.library_category_display_mode;
                            let mut show_disabled = !self.state.static_prefs.hide_disabled;
                            let mut show_archived = !self.state.static_prefs.hide_archived;
                            let left_column_width = ui.available_width() * 0.32;
                            ui.horizontal_top(|ui| {
                                ui.vertical(|ui| {
                                    ui.set_width(left_column_width);
                                    setting_block(ui, text.group_list_by(), &mut |ui| {
                                        egui::ComboBox::from_id_salt("library_group_mode")
                                            .selected_text(text.library_group_mode(self.state.static_prefs.library_group_mode))
                                            .show_ui(ui, |ui| {
                                                ui.selectable_value(&mut self.state.static_prefs.library_group_mode, LibraryGroupMode::Category, text.library_group_mode(LibraryGroupMode::Category));
                                                ui.selectable_value(&mut self.state.static_prefs.library_group_mode, LibraryGroupMode::Status, text.library_group_mode(LibraryGroupMode::Status));
                                                ui.selectable_value(&mut self.state.static_prefs.library_group_mode, LibraryGroupMode::None, text.library_group_mode(LibraryGroupMode::None));
                                            });
                                    });
                                    ui.add_space(8.0);
                                    setting_block(ui, text.category_layout(), &mut |ui| {
                                        ui.add_enabled_ui(
                                            matches!(self.state.static_prefs.library_group_mode, LibraryGroupMode::Category),
                                            |ui| {
                                                egui::ComboBox::from_id_salt("library_category_display_mode")
                                                    .selected_text(text.library_category_display_mode(self.state.static_prefs.library_category_display_mode))
                                                    .show_ui(ui, |ui| {
                                                        ui.selectable_value(
                                                            &mut self.state.static_prefs.library_category_display_mode,
                                                            LibraryCategoryDisplayMode::GroupedSections,
                                                            text.library_category_display_mode(LibraryCategoryDisplayMode::GroupedSections),
                                                        );
                                                        ui.selectable_value(
                                                            &mut self.state.static_prefs.library_category_display_mode,
                                                            LibraryCategoryDisplayMode::Folders,
                                                            text.library_category_display_mode(LibraryCategoryDisplayMode::Folders),
                                                        );
                                                    });
                                            },
                                        );
                                    });
                                });
                                ui.vertical(|ui| {
                                    let checkbox_changed = match self.state.static_prefs.library_group_mode {
                                        LibraryGroupMode::Status => {
                                            let response = ui.checkbox(
                                                &mut self.state.static_prefs.library_sort_category_first,
                                                text.sort_by_category_first(),
                                            );
                                            response
                                                .clone()
                                                .on_hover_text(text.sort_by_category_first_tooltip());
                                            response.changed()
                                        }
                                        LibraryGroupMode::Category | LibraryGroupMode::None => {
                                            let response = ui.checkbox(
                                                &mut self.state.static_prefs.library_sort_status_first,
                                                text.sort_by_status_first(),
                                            );
                                            response
                                                .clone()
                                                .on_hover_text(text.sort_by_status_first_tooltip());
                                            response.changed()
                                        }
                                    };
                                    if checkbox_changed {
                                        should_save = true;
                                    }
                                    let show_card_detail_response = if matches!(self.state.static_prefs.library_group_mode, LibraryGroupMode::Category) {
                                        ui.checkbox(
                                            &mut self.state.static_prefs.library_category_group_show_status,
                                            text.show_mod_status_on_card(),
                                        )
                                    } else {
                                        let response = ui.checkbox(
                                            &mut self.state.static_prefs.library_status_group_show_category,
                                            text.show_category_on_card(),
                                        );
                                        response
                                            .clone()
                                            .on_hover_text(text.show_category_on_card_tooltip());
                                        response
                                    };
                                    if show_card_detail_response.changed() {
                                        should_save = true;
                                    }
                                    if ui
                                        .checkbox(&mut show_disabled, text.show_disabled_mods())
                                        .changed()
                                    {
                                        self.state.static_prefs.hide_disabled = !show_disabled;
                                        should_save = true;
                                    }
                                    if ui
                                        .checkbox(&mut show_archived, text.show_archived_mods())
                                        .changed()
                                    {
                                        self.state.static_prefs.hide_archived = !show_archived;
                                        should_save = true;
                                    }
                                    if matches!(
                                        self.state.static_prefs.library_category_display_mode,
                                        LibraryCategoryDisplayMode::GroupedSections
                                    ) && matches!(self.state.static_prefs.library_group_mode, LibraryGroupMode::Category)
                                        && ui
                                            .checkbox(
                                                &mut self.state.static_prefs.library_uncategorized_first,
                                                text.show_uncategorized_mods_first(),
                                            )
                                            .changed()
                                    {
                                        should_save = true;
                                    }
                                });
                            });
                            if self.state.static_prefs.library_group_mode != group_mode {
                                should_save = true;
                            }
                            if self.state.static_prefs.library_category_display_mode != category_display_mode {
                                should_save = true;
                            }
                            ui.add_space(1.0);
                        });
                        ui.add_space(24.0);

                        static_label(ui, bold(text.operational(), Some(16.0)).underline());
                        ui.indent("setting_general_operational", |ui| {
                            static_label(ui, text.mods_to_check_for_updates());
                            ui.add_space(-4.0);
                            let update_check_statuses = self.state.static_prefs.update_check_statuses;
                            ui.horizontal(|ui| {
                                ui.checkbox(
                                    &mut self.state.static_prefs.update_check_statuses.active,
                                    text.status_target_active(),
                                );
                                ui.checkbox(
                                    &mut self.state.static_prefs.update_check_statuses.disabled,
                                    text.status_target_disabled(),
                                );
                                ui.checkbox(
                                    &mut self.state.static_prefs.update_check_statuses.archived,
                                    text.status_target_archived(),
                                );
                            });
                            if self.state.static_prefs.update_check_statuses.active != update_check_statuses.active
                                || self.state.static_prefs.update_check_statuses.disabled != update_check_statuses.disabled
                                || self.state.static_prefs.update_check_statuses.archived != update_check_statuses.archived
                            {
                                should_save = true;
                                update_check_targets_changed = true;
                            }
                            ui.add_space(8.0);
                            static_label(ui, text.automatically_update_mods());
                            ui.add_space(-4.0);
                            let auto_update_statuses = self.state.static_prefs.auto_update_statuses;
                            ui.horizontal(|ui| {
                                ui.checkbox(
                                    &mut self.state.static_prefs.auto_update_statuses.active,
                                    text.status_target_active(),
                                );
                                ui.checkbox(
                                    &mut self.state.static_prefs.auto_update_statuses.disabled,
                                    text.status_target_disabled(),
                                );
                                ui.checkbox(
                                    &mut self.state.static_prefs.auto_update_statuses.archived,
                                    text.status_target_archived(),
                                );
                            });
                            if self.state.static_prefs.auto_update_statuses.active != auto_update_statuses.active
                                || self.state.static_prefs.auto_update_statuses.disabled != auto_update_statuses.disabled
                                || self.state.static_prefs.auto_update_statuses.archived != auto_update_statuses.archived
                            {
                                should_save = true;
                            }
                            ui.add_space(8.0);
                            static_label(ui, text.also_update_modified_mods());
                            ui.add_space(-4.0);
                            let modified_update_behavior = self.state.static_prefs.modified_update_behavior;
                            egui::ComboBox::from_id_salt("modified_update_behavior")
                                .selected_text(text.modified_update_behavior(modified_update_behavior))
                                .show_ui(ui, |ui| {
                                    ui.selectable_value(
                                        &mut self.state.static_prefs.modified_update_behavior,
                                        ModifiedUpdateBehavior::Yes,
                                        text.modified_update_behavior(ModifiedUpdateBehavior::Yes),
                                    );
                                    ui.selectable_value(
                                        &mut self.state.static_prefs.modified_update_behavior,
                                        ModifiedUpdateBehavior::ShowButton,
                                        text.modified_update_behavior(ModifiedUpdateBehavior::ShowButton),
                                    );
                                    ui.selectable_value(
                                        &mut self.state.static_prefs.modified_update_behavior,
                                        ModifiedUpdateBehavior::HideButton,
                                        text.modified_update_behavior(ModifiedUpdateBehavior::HideButton),
                                    );
                                });
                            if self.state.static_prefs.modified_update_behavior != modified_update_behavior {
                                should_save = true;
                            }
                            ui.add_space(8.0);
                            static_label(ui, text.when_installing_existing_mod());
                            ui.add_space(-4.0);
                            let import_resolution = self.state.static_prefs.import_resolution;
                            let always_replace_on_update = self.state.static_prefs.always_replace_on_update;
                            ui.horizontal(|ui| {
                                egui::ComboBox::from_id_salt("import_resolution")
                                    .selected_text(text.import_resolution(import_resolution))
                                    .show_ui(ui, |ui| {
                                        ui.selectable_value(&mut self.state.static_prefs.import_resolution, ImportResolution::Ask, text.import_resolution(ImportResolution::Ask));
                                        ui.selectable_value(&mut self.state.static_prefs.import_resolution, ImportResolution::Replace, text.import_resolution(ImportResolution::Replace));
                                        ui.selectable_value(&mut self.state.static_prefs.import_resolution, ImportResolution::Merge, text.import_resolution(ImportResolution::Merge));
                                        ui.selectable_value(&mut self.state.static_prefs.import_resolution, ImportResolution::KeepBoth, text.import_resolution(ImportResolution::KeepBoth));
                                    });
                                ui.checkbox(
                                    &mut self.state.static_prefs.always_replace_on_update,
                                    text.always_replace_on_updating_mods(),
                                );
                            });
                            if self.state.static_prefs.import_resolution != import_resolution
                                || self.state.static_prefs.always_replace_on_update != always_replace_on_update
                            {
                                should_save = true;
                            }
                            ui.add_space(8.0);
                            static_label(ui, text.when_deleting_mod());
                            ui.add_space(-4.0);
                            let delete_behavior = self.state.static_prefs.delete_behavior;
                            egui::ComboBox::from_id_salt("delete_behavior")
                                .selected_text(text.delete_behavior(delete_behavior))
                                .show_ui(ui, |ui| {
                                    ui.selectable_value(&mut self.state.static_prefs.delete_behavior, DeleteBehavior::RecycleBin, text.delete_behavior(DeleteBehavior::RecycleBin));
                                    ui.selectable_value(&mut self.state.static_prefs.delete_behavior, DeleteBehavior::Permanent, text.delete_behavior(DeleteBehavior::Permanent));
                                });
                            if self.state.static_prefs.delete_behavior != delete_behavior { should_save = true; }
                            ui.add_space(1.0);
                        });
                        ui.add_space(24.0);

                        static_label(ui, bold(text.tasks(), Some(16.0)).underline());
                        ui.indent("setting_general_tasks", |ui| {
                            let tasks_layout = self.state.tasks_layout;
                            let tasks_order = self.state.tasks_order;
                            let left_column_width = ui.available_width() * 0.32;
                            ui.horizontal_top(|ui| {
                                ui.vertical(|ui| {
                                    ui.set_width(left_column_width);
                                    setting_block(ui, text.tasks_layout(), &mut |ui| {
                                        egui::ComboBox::from_id_salt("tasks_layout")
                                            .selected_text(text.tasks_layout_value(self.state.tasks_layout))
                                            .show_ui(ui, |ui| {
                                                ui.selectable_value(&mut self.state.tasks_layout, TasksLayout::Sections, text.tasks_layout_value(TasksLayout::Sections));
                                                ui.selectable_value(&mut self.state.tasks_layout, TasksLayout::Tabbed, text.tasks_layout_value(TasksLayout::Tabbed));
                                                ui.selectable_value(&mut self.state.tasks_layout, TasksLayout::SingleList, text.tasks_layout_value(TasksLayout::SingleList));
                                            });
                                    });
                                    ui.add_space(8.0);
                                    setting_block(ui, text.clear_completed_tasks(), &mut |ui| {
                                        if ui
                                            .button(icon_text_sized(
                                                Icon::Trash2,
                                                text.clear_tasks(),
                                                14.5,
                                                13.0,
                                            ))
                                            .on_hover_cursor(egui::CursorIcon::PointingHand)
                                            .clicked()
                                        {
                                            self.clear_completed_tasks();
                                        }
                                    });
                                });
                                ui.vertical(|ui| {
                                    setting_block(ui, text.task_order(), &mut |ui| {
                                        egui::ComboBox::from_id_salt("tasks_order")
                                            .selected_text(text.tasks_order_value(self.state.tasks_order))
                                            .show_ui(ui, |ui| {
                                                ui.selectable_value(&mut self.state.tasks_order, TasksOrder::OldestFirst, text.tasks_order_value(TasksOrder::OldestFirst));
                                                ui.selectable_value(&mut self.state.tasks_order, TasksOrder::NewestFirst, text.tasks_order_value(TasksOrder::NewestFirst));
                                            });
                                    });
                                });
                            });
                            if self.state.tasks_layout != tasks_layout {
                                should_save = true;
                            }
                            if self.state.tasks_order != tasks_order {
                                should_save = true;
                            }
                            ui.add_space(1.0);
                        });
                        ui.add_space(24.0);
                    }
                    SettingsTab::Categories => {
                        self.render_categories_settings_tab(ui, &mut should_save);
                    }
                    SettingsTab::Path => {
                    ui.indent("path_scan_tools", |ui| {
                            static_label(
                                ui,
                                RichText::new(text.path_scan_title())
                                    .size(14.0)
                                    .color(Color32::from_gray(220)),
                            );
                            ui.add_space(-10.0);
                            static_label(
                                ui,
                                RichText::new(text.path_scan_description())
                                    .size(12.0)
                                    .color(Color32::from_gray(165)),
                            );
                            ui.add_space(-4.0);
                            ui.scope(|ui| {
                                let radius = egui::CornerRadius::same(3);
                                ui.style_mut().visuals.widgets.inactive.corner_radius = radius;
                                ui.style_mut().visuals.widgets.hovered.corner_radius = radius;
                                ui.style_mut().visuals.widgets.active.corner_radius = radius;
                                let scanning = self.startup_path_scan.is_some();
                                let scan_button = egui::Button::new(icon_text_sized(
                                    Icon::Search,
                                    text.path_scan_button(scanning),
                                    14.0,
                                    13.0,
                                ))
                                .fill(Color32::from_rgb(180, 78, 35))
                                .stroke(egui::Stroke::new(1.0, Color32::from_rgb(203, 104, 59)));
                                if ui
                                    .add_enabled(!scanning, scan_button)
                                    .on_hover_text(text.path_scan_button_tooltip())
                                    .on_hover_cursor(if scanning {
                                        egui::CursorIcon::NotAllowed
                                    } else {
                                        egui::CursorIcon::PointingHand
                                    })
                                    .clicked()
                                {
                                    self.start_manual_path_scan();
                                }
                                ui.add_space(1.0);
                            });
                    });
                    ui.add_space(16.0);
                    static_label(ui, bold(text.path_xxmi_section(), Some(16.0)).underline());
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
                                .static_prefs
                                .modded_launcher_path_override
                                .as_ref()
                                .map_or(true, |path| !path.is_file());
                            ui.horizontal(|ui| {
                                static_label(
                                    ui,
                                    RichText::new(text.path_xxmi_launcher())
                                        .small()
                                        .color(Color32::from_gray(165)),
                                );
                                if invalid {
                                    static_label(ui, icon_rich(Icon::AlertTriangle, 13.0, warn_color));
                                    ui.add_space(-8.0);
                                    static_label(
                                        ui,
                                        RichText::new(text.path_not_found())
                                            .small()
                                            .color(warn_color),
                                    );
                                }
                            });
                            ui.add_space(-8.0);
                            ui.horizontal(|ui| {
                                let browse_width = 28.0;
                                let input_width = 320.0;
                                let input_id = egui::Id::new("launcher_path_input");

                                let current_launcher_value = self
                                    .state
                                    .static_prefs
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
                                        .cursor_at_end(false)
                                        .desired_width(input_width),
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
                                    self.state.static_prefs.modded_launcher_path_override =
                                        non_empty(launcher_value).map(PathBuf::from);
                                    if let Some(path) = self.state.static_prefs.modded_launcher_path_override.clone() {
                                        for game in &mut self.state.games {
                                            game.modded_exe_path_override = Some(path.clone());
                                            game.mods_path_override = default_mods_path_from_launcher(
                                                &path,
                                                &game.definition.xxmi_code,
                                            );
                                        }
                                    }
                                    ui.data_mut(|d| {
                                        d.remove::<String>(input_id);
                                        for game in &self.state.games {
                                            d.remove::<String>(egui::Id::new((
                                                "settings_mods_path",
                                                game.definition.id.as_str(),
                                            )));
                                        }
                                    });
                                    should_save = true;
                                } else if !launcher_dirty && action_clicked {
                                    if let Some(path) = FileDialog::new()
                                        .add_filter(text.file_filter_executable(), &["exe"])
                                        .pick_file()
                                    {
                                        self.state.static_prefs.modded_launcher_path_override = Some(path.clone());
                                        for game in &mut self.state.games {
                                            game.modded_exe_path_override = Some(path.clone());
                                            game.mods_path_override = default_mods_path_from_launcher(
                                                &path,
                                                &game.definition.xxmi_code,
                                            );
                                        }
                                        ui.data_mut(|d| {
                                            d.remove::<String>(input_id);
                                            for game in &self.state.games {
                                                d.remove::<String>(egui::Id::new((
                                                    "settings_mods_path",
                                                    game.definition.id.as_str(),
                                                )));
                                            }
                                        });
                                        should_save = true;
                                    }
                                }
                            });
                            ui.add_space(-4.0);
                            ui.horizontal(|ui| {
                                ui.add_space(4.0);
                                let was_using_default_mods_path = self.state.static_prefs.use_default_mods_path;
                                if ui.checkbox(&mut self.state.static_prefs.use_default_mods_path, text.path_use_default_xxmi_mod_path()).changed() {
                                    if was_using_default_mods_path && !self.state.static_prefs.use_default_mods_path {
                                        for game in self.state.games.iter_mut().filter(|game| game.enabled) {
                                            if game.mods_path_override.is_none() {
                                                game.mods_path_override = game.mods_path(was_using_default_mods_path);
                                            }
                                            ui.data_mut(|data| {
                                                data.remove::<String>(egui::Id::new((
                                                    "settings_mods_path",
                                                    game.definition.id.as_str(),
                                                )));
                                            });
                                        }
                                    }
                                    should_save = true;
                                }
                            });
                        });
                    });
                    ui.add_space(24.0);
                    ui.label(bold(text.path_game_section(), Some(16.0)).underline())
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
                                                        RichText::new(text.path_game_exe_file())
                                                            .small()
                                                            .color(Color32::from_gray(165)),
                                                    ).on_hover_cursor(egui::CursorIcon::Default);
                                                    if vanilla_invalid {
                                                        ui.label(icon_rich(Icon::AlertTriangle, 13.0, warn_color))
                                                        .on_hover_cursor(egui::CursorIcon::Default);
                                                        ui.add_space(-8.0);
                                                        ui.label(
                                                            RichText::new(text.path_not_found())
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
                                                            .cursor_at_end(false)
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
                                                            .add_filter(text.file_filter_executable(), &["exe"])
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

                                            if game.enabled && !self.state.static_prefs.use_default_mods_path {
                                                let mods_invalid = game
                                                    .mods_path(self.state.static_prefs.use_default_mods_path)
                                                    .map(|path| !path.is_dir())
                                                    .unwrap_or(true);
                                                ui.horizontal(|ui| {
                                                    ui.label(
                                                        RichText::new(text.path_game_mods_folder(&game.definition.xxmi_code))
                                                            .small()
                                                            .color(Color32::from_gray(165)),
                                                    ).on_hover_cursor(egui::CursorIcon::Default);
                                                    if mods_invalid {
                                                        ui.label(icon_rich(Icon::AlertTriangle, 13.0, warn_color))
                                                        .on_hover_cursor(egui::CursorIcon::Default);
                                                        ui.add_space(-8.0);
                                                        ui.label(
                                                            RichText::new(text.path_not_found())
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
                                                        .mods_path(self.state.static_prefs.use_default_mods_path)
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
                                                            .cursor_at_end(false)
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
                        let radius = egui::CornerRadius::same(3);
                        ui.style_mut().visuals.widgets.inactive.corner_radius = radius;
                        ui.style_mut().visuals.widgets.hovered.corner_radius = radius;
                        ui.style_mut().visuals.widgets.active.corner_radius = radius;
                        ui.style_mut().visuals.widgets.open.corner_radius = radius;

                        static_label(ui, bold(text.appearance(), Some(16.0)).underline());
                        ui.indent("setting_general_interface", |ui| {
                            static_label(ui, text.language());
                            ui.add_space(-4.0);
                            let previous_language = self.state.static_prefs.language;
                            let always_translate_mod_details =
                                self.state.static_prefs.always_translate_mod_details;
                            ui.horizontal(|ui| {
                                egui::ComboBox::from_id_salt("app_language")
                                    .selected_text(self.state.static_prefs.language.native_label())
                                    .show_ui(ui, |ui| {
                                        for language in AppLanguage::ALL {
                                            ui.selectable_value(
                                                &mut self.state.static_prefs.language,
                                                language,
                                                language.native_label(),
                                            )
                                            .on_hover_text(language.label());
                                        }
                                    });
                                let auto_translate_response = ui.checkbox(
                                    &mut self.state.static_prefs.always_translate_mod_details,
                                    text.always_translate_mod_details(),
                                );
                                auto_translate_response
                                    .clone()
                                    .on_hover_text(text.always_translate_mod_details_tooltip());
                            });
                            if self.state.static_prefs.language != previous_language {
                                should_save = true;
                            }
                            if self.state.static_prefs.always_translate_mod_details
                                != always_translate_mod_details
                            {
                                should_save = true;
                            }
                            ui.add_space(8.0);

                            if classic_font_style_available() {
                                static_label(ui, text.font_style());
                                ui.add_space(-4.0);
                                let previous_font_style = self.state.static_prefs.font_style;
                                ui.horizontal(|ui| {
                                    ui.radio_value(
                                        &mut self.state.static_prefs.font_style,
                                        AppFontStyle::Classic,
                                        text.font_classic(),
                                    )
                                    .on_hover_text(text.font_classic_tooltip());
                                    ui.add_space(12.0);
                                    ui.radio_value(
                                        &mut self.state.static_prefs.font_style,
                                        AppFontStyle::Modern,
                                        text.font_modern(),
                                    )
                                    .on_hover_text(text.font_modern_tooltip());
                                });
                                if self.state.static_prefs.font_style != previous_font_style {
                                    install_app_fonts(ctx, self.state.static_prefs.font_style);
                                    should_save = true;
                                }
                                ui.add_space(8.0);
                            }
                            ui.add_space(1.0);
                        });
                        ui.add_space(24.0);

                        static_label(ui, bold(text.content_restriction(), Some(16.0)).underline());
                        ui.indent("setting_advanced_nsfw", |ui| {
                            static_label(ui, text.hide_unsafe_contents());
                            ui.add_space(-4.0);
                            let unsafe_mode = self.state.static_prefs.unsafe_content_mode;
                            egui::ComboBox::from_id_salt("unsafe_content_mode")
                                .selected_text(text.unsafe_content_mode(unsafe_mode))
                                .show_ui(ui, |ui| {
                                    ui.selectable_value(&mut self.state.static_prefs.unsafe_content_mode, UnsafeContentMode::HideNoCounter, text.unsafe_content_mode(UnsafeContentMode::HideNoCounter));
                                    ui.selectable_value(&mut self.state.static_prefs.unsafe_content_mode, UnsafeContentMode::HideShowCounter, text.unsafe_content_mode(UnsafeContentMode::HideShowCounter));
                                    ui.selectable_value(&mut self.state.static_prefs.unsafe_content_mode, UnsafeContentMode::Censor, text.unsafe_content_mode(UnsafeContentMode::Censor));
                                    ui.selectable_value(&mut self.state.static_prefs.unsafe_content_mode, UnsafeContentMode::Show, text.unsafe_content_mode(UnsafeContentMode::Show));
                                });
                            if self.state.static_prefs.unsafe_content_mode != unsafe_mode { should_save = true; }
                            ui.add_space(1.0);
                        });
                        ui.add_space(24.0);

                        static_label(ui, bold(text.cache_and_archive(), Some(16.0)).underline());
                        ui.indent("setting_advanced_cache", |ui| {
                            self.refresh_usage_counters_if_needed(ui.input(|i| i.time));
                            static_label(ui, text.cache_size());
                            ui.add_space(-4.0);
                            let previous_tier = self.state.static_prefs.cache_size_tier;
                            egui::ComboBox::from_id_salt("cache_size_tier")
                                .selected_text(self.state.static_prefs.cache_size_tier.label())
                                .show_ui(ui, |ui| {
                                    ui.selectable_value(&mut self.state.static_prefs.cache_size_tier, CacheSizeTier::Gb2, "2 GB");
                                    ui.selectable_value(&mut self.state.static_prefs.cache_size_tier, CacheSizeTier::Gb4, "4 GB");
                                    ui.selectable_value(&mut self.state.static_prefs.cache_size_tier, CacheSizeTier::Gb8, "8 GB");
                                    ui.selectable_value(&mut self.state.static_prefs.cache_size_tier, CacheSizeTier::Gb16, "16 GB");
                                });
                            if self.state.static_prefs.cache_size_tier != previous_tier {
                                self.cache_limit_bytes
                                    .store(self.state.static_prefs.cache_size_tier.bytes(), Ordering::Relaxed);
                                let _ = persistence::evict_lru_if_needed(
                                    &self.portable,
                                    self.state.static_prefs.cache_size_tier.bytes(),
                                );
                                self.mark_usage_counters_dirty();
                                should_save = true;
                            }
                            ui.add_space(8.0);
                            let usage_gb =
                                self.usage_cache_bytes as f64 / (1024.0 * 1024.0 * 1024.0);
                            static_label(ui, text.current_usage(usage_gb));
                            ui.add_space(-4.0);
                            if ui
                                .button(icon_text_sized(Icon::Trash2, text.clear_cache(), 14.5, 13.0))
                                .on_hover_cursor(egui::CursorIcon::PointingHand)
                                .clicked()
                            {
                                match self.clear_cache() {
                                    Ok(()) => self.set_message_ok(text.cache_cleared()),
                                    Err(err) => self.report_error(err, Some(text.clear_cache_failed())),
                                }
                            }
                            ui.add_space(8.0);
                            let archive_gb =
                                self.usage_archive_bytes as f64 / (1024.0 * 1024.0 * 1024.0);
                            static_label(ui, text.archive_usage(archive_gb));
                            ui.add_space(-4.0);
                            if ui
                                .button(icon_text_sized(Icon::Trash2, text.delete_archived_mods(), 14.5, 13.0))
                                .on_hover_cursor(egui::CursorIcon::PointingHand)
                                .clicked()
                            {
                                match self.clear_archives() {
                                    Ok(count) => {
                                        if count > 0 {
                                            let action = text.archive_delete_action(self.state.static_prefs.delete_behavior);
                                            self.log_action(action, &text.archived_mods_count(count));
                                            self.set_message_ok(text.archives_cleared(count));
                                        } else {
                                            self.set_message_ok(text.no_archives_to_clear());
                                        }
                                        self.refresh();
                                    }
                                    Err(err) => self.report_error(err, Some(text.clear_archives_failed())),
                                }
                            }
                            ui.add_space(1.0);
                        });
                        ui.add_space(24.0);
                    }
                    SettingsTab::About => {
                        static_label(ui, bold(APP_NAME, Some(16.0)).underline());
                        ui.indent("setting_about_app", |ui| {
                            ui.add_space(-2.0);
                            static_label(ui, text.about_by(APP_AUTHORS));
                            ui.add_space(-8.0);
                            ui.hyperlink("https://github.com/HenryNugraha/Hestia");
                            ui.add_space(2.0);
                            ui.horizontal(|ui| {
                                static_label(ui, text.about_version());
                                ui.add_space(-6.0);
                                let version_response = ui
                                    .add(
                                        egui::Label::new(
                                            RichText::new(APP_VERSION)
                                                .color(Color32::from_rgb(210, 189, 156)),
                                        )
                                        .selectable(false)
                                        .sense(Sense::click()),
                                    )
                                    .on_hover_text(text.about_version_tooltip())
                                    .on_hover_cursor(egui::CursorIcon::PointingHand);
                                if version_response.clicked() {
                                    self.toggle_whats_new_window();
                                }
                            });
                            ui.add_space(-6.0);
                            let now = ui.input(|i| i.time);
                            let label = self.app_update_button_label(now);
                            let enabled = self.app_update_button_enabled(now);
                            let update_icon = if self.app_update_verified_path.is_some() {
                                Icon::RotateCw
                            } else if self.app_update_button_state == AppUpdateButtonState::Checking
                                || now < self.app_update_button_spin_until
                            {
                                Icon::Loader2
                            } else {
                                Icon::RefreshCw
                            };
                            ui.horizontal(|ui| {
                                ui.add_space(-2.0);
                                let response = ui.add_enabled(
                                    enabled,
                                    egui::Button::new(icon_text_sized(
                                        update_icon,
                                        label,
                                        11.5,
                                        11.5,
                                    ))
                                    .corner_radius(egui::CornerRadius::same(3)),
                                );
                                if response
                                    .on_hover_cursor(egui::CursorIcon::PointingHand)
                                    .clicked()
                                {
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
                                    &mut self.state.static_prefs.automatically_check_for_update,
                                    text.automatically_check_for_update(),
                                )
                                .changed()
                            {
                                should_save = true;
                            }
                            ui.add_space(1.0);
                        });
                        ui.add_space(24.0);

                        static_label(ui, bold(text.attribution(), Some(16.0)).underline());
                        ui.indent("setting_about_attributions", |ui| {
                            static_label(ui, text.attribution_gamebanana());
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
        if update_check_targets_changed {
            let game_id = self.selected_game().map(|game| game.definition.id.clone());
            self.queue_update_check_for_linked_mods(game_id.as_deref());
        }
    }
}
