#[derive(Clone, Copy, PartialEq, Eq)]
enum FrameworkDialogChoice {
    First,
    Second,
    OpenPage,
    Cancel,
}

struct FrameworkDialog<'a> {
    id: egui::Id,
    title: &'a str,
    name: &'a str,
    body: String,
    detail_lines: Vec<String>,
    first_button: &'a str,
    second_button: &'a str,
    page_button: Option<&'a str>,
    cancel_button: Option<&'a str>,
}

impl HestiaApp {
    fn render_framework_dialogs(&mut self, ctx: &egui::Context) {
        self.render_framework_question(ctx);
        self.render_framework_undecided_dialog(ctx);
    }

    fn show_framework_dialog(
        &self,
        ctx: &egui::Context,
        dialog: FrameworkDialog<'_>,
    ) -> Option<FrameworkDialogChoice> {
        let constrain_rect = self
            .last_right_pane_rect
            .unwrap_or_else(|| ctx.viewport_rect());
        let mut choice = None;
        egui::Window::new(icon_text_sized(Icon::Info, dialog.title, 14.0, 14.0))
            .id(dialog.id)
            .default_pos(constrain_rect.min + egui::vec2(16.0, 16.0))
            .order(egui::Order::Foreground)
            .resizable(false)
            .collapsible(false)
            .constrain_to(constrain_rect)
            .frame(
                egui::Frame::window(&ctx.style_of(ctx.theme()))
                    .inner_margin(egui::Margin::same(16))
                    .stroke(egui::Stroke::new(1.0, Color32::from_rgb(82, 134, 186))),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    static_label(
                        ui,
                        icon_rich(Icon::Info, 96.0, Color32::from_rgb(148, 192, 232)),
                    );
                    ui.vertical(|ui| {
                        ui.set_max_width(380.0);
                        static_label(ui, bold(dialog.name, Some(16.0)).underline());
                        ui.add_space(4.0);
                        ui.add(
                            egui::Label::new(RichText::new(dialog.body).size(14.0))
                                .selectable(false)
                                .wrap(),
                        )
                        .on_hover_cursor(egui::CursorIcon::Default);
                        if !dialog.detail_lines.is_empty() {
                            ui.add_space(4.0);
                        }
                        for line in dialog.detail_lines {
                            ui.add(
                                egui::Label::new(
                                    RichText::new(line)
                                        .size(12.0)
                                        .color(Color32::from_gray(175)),
                                )
                                .selectable(false)
                                .wrap(),
                            )
                            .on_hover_cursor(egui::CursorIcon::Default);
                        }
                        ui.add_space(8.0);
                        ui.horizontal(|ui| {
                            if ui.button(dialog.first_button).clicked() {
                                choice = Some(FrameworkDialogChoice::First);
                            }
                            if ui.button(dialog.second_button).clicked() {
                                choice = Some(FrameworkDialogChoice::Second);
                            }
                            if let Some(page_button) = dialog.page_button
                                && ui
                                    .button(icon_text_sized(Icon::Globe, page_button, 12.0, 12.0))
                                    .clicked()
                            {
                                choice = Some(FrameworkDialogChoice::OpenPage);
                            }
                            if let Some(cancel_button) = dialog.cancel_button
                                && ui.button(cancel_button).clicked()
                            {
                                choice = Some(FrameworkDialogChoice::Cancel);
                            }
                        });
                    });
                });
            });
        choice
    }

    fn framework_copy_lines(
        &self,
        framework_mod_id: &str,
        arriving_file_name: Option<&str>,
    ) -> Vec<String> {
        let text = self.text();
        let yours = self
            .state
            .mods
            .iter()
            .find(|entry| entry.id == framework_mod_id)
            .and_then(mod_framework_record)
            .and_then(FrameworkRecord::installed_copy)
            .map(|(_, copy)| text.framework_your_copy(&copy.file_name));
        let that_copy = arriving_file_name.map(|file_name| text.framework_that_copy(file_name));
        yours.into_iter().chain(that_copy).collect()
    }

    fn render_framework_question(&mut self, ctx: &egui::Context) {
        let Some(question) = self.frameworks.questions.front().cloned() else {
            return;
        };
        let text = self.text();
        match question {
            FrameworkQuestion::IsFramework { job_id, name } => {
                let choice = self.show_framework_dialog(
                    ctx,
                    FrameworkDialog {
                        id: egui::Id::new(("framework_question", job_id)),
                        title: text.framework_question_title(),
                        name: &name,
                        body: text.framework_question_body(&name),
                        detail_lines: Vec::new(),
                        first_button: text.framework_question_yes(),
                        second_button: text.framework_question_no(),
                        page_button: None,
                        cancel_button: None,
                    },
                );
                match choice {
                    Some(FrameworkDialogChoice::First) => {
                        self.answer_framework_question(job_id, FrameworkVerdict::Framework);
                    }
                    Some(FrameworkDialogChoice::Second) => {
                        self.answer_framework_question(job_id, FrameworkVerdict::Regular);
                    }
                    _ => {}
                }
            }
            FrameworkQuestion::UseArrivingCopy { job_id, name } => {
                let detail_lines = self
                    .frameworks
                    .copy_questions
                    .get(&job_id)
                    .map(|question| {
                        self.framework_copy_lines(
                            &question.existing_mod_id,
                            question
                                .source
                                .file
                                .as_ref()
                                .map(|file| file.file_name.as_str()),
                        )
                    })
                    .unwrap_or_default();
                let choice = self.show_framework_dialog(
                    ctx,
                    FrameworkDialog {
                        id: egui::Id::new(("framework_copy_question", job_id)),
                        title: text.framework_copy_question_title(),
                        name: &name,
                        body: text.framework_copy_question_body(&name),
                        detail_lines,
                        first_button: text.framework_use_that_copy(),
                        second_button: text.framework_keep_mine(),
                        page_button: None,
                        cancel_button: None,
                    },
                );
                match choice {
                    Some(FrameworkDialogChoice::First) => {
                        self.answer_framework_copy_question(job_id, true);
                    }
                    Some(FrameworkDialogChoice::Second) => {
                        self.answer_framework_copy_question(job_id, false);
                    }
                    _ => {}
                }
            }
        }
    }

    fn render_framework_undecided_dialog(&mut self, ctx: &egui::Context) {
        let Some(mod_id) = self.frameworks.undecided_dialog.clone() else {
            return;
        };
        let Some((name, page_id, is_tool, copy)) = self.framework_undecided_copy(&mod_id) else {
            self.frameworks.undecided_dialog = None;
            return;
        };
        let text = self.text();
        let detail_lines = self.framework_copy_lines(&mod_id, Some(&copy.file_name));
        let choice = self.show_framework_dialog(
            ctx,
            FrameworkDialog {
                id: egui::Id::new(("framework_undecided_copy", &mod_id)),
                title: text.framework_copy_question_title(),
                name: &name,
                body: text.framework_copy_question_body(&name),
                detail_lines,
                first_button: text.framework_use_that_copy(),
                second_button: text.framework_keep_mine(),
                page_button: Some(text.gamebanana_page()),
                cancel_button: Some(text.cancel()),
            },
        );
        match choice {
            Some(FrameworkDialogChoice::First) => {
                self.answer_framework_undecided_copy(&mod_id, true);
            }
            Some(FrameworkDialogChoice::Second) => {
                self.answer_framework_undecided_copy(&mod_id, false);
            }
            Some(FrameworkDialogChoice::OpenPage) => {
                if let Err(err) =
                    open_external_url(&gamebanana::browser_url_typed(page_id, is_tool))
                {
                    self.report_error(err, Some(text.app_could_not_open_browser()));
                }
            }
            Some(FrameworkDialogChoice::Cancel) => self.frameworks.undecided_dialog = None,
            None => {}
        }
    }

    fn render_page_menu_buttons(&mut self, ui: &mut Ui, page_id: u64, is_tool: bool) {
        let text = self.text();
        if ui
            .button(icon_text_sized(
                Icon::Globe,
                text.gamebanana_page(),
                13.0,
                13.0,
            ))
            .on_hover_cursor(egui::CursorIcon::PointingHand)
            .clicked()
        {
            if let Err(err) = open_external_url(&gamebanana::browser_url_typed(page_id, is_tool)) {
                self.report_error(err, Some(text.app_could_not_open_browser()));
            }
            ui.close();
        }
        if ui
            .button(icon_text_sized(
                Icon::Compass,
                "Browse in Hestia",
                13.0,
                13.0,
            ))
            .on_hover_cursor(egui::CursorIcon::PointingHand)
            .clicked()
        {
            self.open_page_in_browse(ui.ctx(), page_id, is_tool);
            ui.close();
        }
        if ui
            .button(icon_text_sized(
                Icon::Copy,
                text.copy_gamebanana_id(),
                13.0,
                13.0,
            ))
            .on_hover_cursor(egui::CursorIcon::PointingHand)
            .clicked()
        {
            ui.ctx().copy_text(page_id.to_string());
            self.set_message_ok(text.gamebanana_id_copied());
            ui.close();
        }
    }

    fn render_framework_card_notice(
        &mut self,
        ui: &mut Ui,
        mod_id: &str,
        notice: FrameworkCardNotice,
    ) {
        let text = self.text();
        match notice {
            FrameworkCardNotice::UndecidedCopy => {
                ui.spacing_mut().button_padding.y = 4.0;
                let response = ui
                    .add(
                        egui::Button::new(
                            RichText::new(text.new_files()).color(Color32::from_rgb(224, 240, 255)),
                        )
                        .fill(Color32::from_rgb(55, 95, 135))
                        .corner_radius(egui::CornerRadius::same(3))
                        .min_size(Vec2::new(64.0, 4.0)),
                    )
                    .on_hover_text(text.framework_undecided_tooltip())
                    .on_hover_cursor(egui::CursorIcon::PointingHand);
                if response.clicked() {
                    self.open_framework_undecided_dialog(mod_id);
                }
            }
            FrameworkCardNotice::Held => {
                ui.add(
                    egui::Label::new(
                        RichText::new(text.ignoring_always())
                            .size(11.0)
                            .color(Color32::from_rgb(181, 153, 196)),
                    )
                    .selectable(false),
                )
                .on_hover_text(mod_update_state_tooltip(
                    ModUpdateState::IgnoringUpdateAlways,
                ))
                .on_hover_cursor(egui::CursorIcon::Default);
            }
        }
    }

    fn render_framework_detail_notice(&self, ui: &mut Ui, notice: FrameworkCardNotice) {
        let text = self.text();
        let (label, color, tooltip) = match notice {
            FrameworkCardNotice::UndecidedCopy => (
                text.new_files(),
                Color32::from_rgb(120, 160, 200),
                text.framework_undecided_tooltip(),
            ),
            FrameworkCardNotice::Held => (
                text.ignoring_always(),
                Color32::from_rgb(181, 153, 196),
                mod_update_state_tooltip(ModUpdateState::IgnoringUpdateAlways),
            ),
        };
        static_label(ui, RichText::new(label).size(12.0).color(color)).on_hover_text(tooltip);
    }

    fn render_framework_undecided_button(&mut self, ui: &mut Ui, mod_id: &str) {
        let text = self.text();
        let response = ui
            .add(
                egui::Button::new(icon_text_sized(Icon::Files, text.new_files(), 12.0, 12.0))
                    .fill(Color32::from_rgb(55, 95, 135))
                    .corner_radius(egui::CornerRadius::same(6)),
            )
            .on_hover_text(text.framework_undecided_tooltip())
            .on_hover_cursor(egui::CursorIcon::PointingHand);
        if response.clicked() {
            self.open_framework_undecided_dialog(mod_id);
        }
    }

    fn render_framework_source_group(&mut self, ui: &mut Ui, mod_id: &str) {
        let text = self.text();
        let Some(entry) = self.state.mods.iter().find(|entry| entry.id == mod_id) else {
            return;
        };
        let page = framework_page_for_actions(entry);
        let mut held = entry
            .source
            .as_ref()
            .is_some_and(|source| source.ignore_update_always);
        if let Some((page_id, is_tool)) = page {
            let id_response = ui
                .add(
                    egui::Label::new(
                        RichText::new(format!("GameBanana ID: {page_id}"))
                            .size(13.0)
                            .strong(),
                    )
                    .selectable(false)
                    .sense(Sense::click()),
                )
                .on_hover_cursor(egui::CursorIcon::PointingHand)
                .on_hover_text(text.copy_gamebanana_id());
            if id_response.clicked() {
                ui.ctx().copy_text(page_id.to_string());
                self.set_message_ok(text.gamebanana_id_copied());
            }
            ui.add_space(2.0);
            if ui
                .button(icon_text_sized(
                    Icon::Globe,
                    text.gamebanana_page(),
                    12.0,
                    12.0,
                ))
                .on_hover_cursor(egui::CursorIcon::PointingHand)
                .clicked()
            {
                self.open_page_in_browse(ui.ctx(), page_id, is_tool);
            }
            ui.add_space(8.0);
        }
        static_label(
            ui,
            RichText::new(text.update_preferences())
                .size(12.0)
                .color(Color32::from_gray(170)),
        );
        ui.add_space(-6.0);
        if ui
            .checkbox(&mut held, text.ignore_update_always())
            .on_hover_text(text.ignore_update_always_tooltip())
            .changed()
        {
            self.set_framework_held(mod_id, held);
        }
    }
}
