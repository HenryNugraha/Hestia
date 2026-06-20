impl HestiaApp {
    fn render_browse_left_pane(&mut self, ui: &mut Ui) {
        let text = self.text();
        let age_now = Local::now();
        egui::Frame::new()
            .fill(Color32::from_rgba_premultiplied(36, 38, 42, 242))
            .corner_radius(egui::CornerRadius::same(0))
            .inner_margin(egui::Margin::same(18))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.set_height(41.0);
                    let is_empty = self.browse_query.trim().is_empty();
                    let expanded = self.browse_search_expanded;
                    let how_expanded = ui.ctx().animate_bool_with_time(
                        ui.id().with("browse_search_anim"),
                        expanded,
                        0.2,
                    );

                    ui.scope(|ui| {
                        let icon_size = 41.0;
                        let full_width = 320.0;
                        let current_width = icon_size + (full_width - icon_size) * how_expanded;

                        let (rect, _area_resp) =
                            ui.allocate_exact_size(Vec2::new(current_width, 41.0), Sense::hover());

                        if how_expanded > 0.0 {
                            let bg_alpha = (how_expanded * 255.0) as u8;
                            ui.painter().rect(
                                rect,
                                egui::CornerRadius::same(6),
                                Color32::from_rgba_premultiplied(44, 47, 52, bg_alpha),
                                egui::Stroke::new(
                                    1.0,
                                    Color32::from_rgba_premultiplied(69, 74, 81, bg_alpha),
                                ),
                                egui::StrokeKind::Inside,
                            );
                        }

                        let icon_pos = rect.left_center() + egui::vec2(20.5, 0.0);
                        let icon_area =
                            egui::Rect::from_center_size(icon_pos, egui::Vec2::splat(28.0));
                        let icon_resp = ui.interact(
                            icon_area,
                            ui.id().with("browse_search_toggle"),
                            Sense::click(),
                        );

                        let icon_color = if expanded || !is_empty {
                            Color32::from_rgb(214, 104, 58)
                        } else if icon_resp.hovered() {
                            Color32::WHITE
                        } else {
                            Color32::from_gray(170)
                        };

                        ui.painter().text(
                            icon_pos,
                            egui::Align2::CENTER_CENTER,
                            icon_char(Icon::Search),
                            egui::FontId::new(18.0, FontFamily::Name(LUCIDE_FAMILY.into())),
                            icon_color,
                        );

                        if icon_resp.clicked() {
                            self.browse_search_expanded = !self.browse_search_expanded;
                        }
                        if icon_resp.hovered() {
                            icon_resp
                                .clone()
                                .on_hover_cursor(egui::CursorIcon::PointingHand);
                            if !expanded {
                                ui.painter().circle_filled(
                                    icon_pos,
                                    14.0,
                                    Color32::from_white_alpha(15),
                                );
                            }
                        }

                        if how_expanded > 0.2 {
                            let right_padding = if !is_empty { 64.0 } else { 32.0 };
                            let input_rect = egui::Rect::from_min_max(
                                rect.min + egui::vec2(icon_size, 0.0),
                                rect.max - egui::vec2(right_padding, 0.0),
                            );

                            let mut child_ui =
                                ui.new_child(egui::UiBuilder::new().max_rect(input_rect));
                            let edit_resp = child_ui.add(
                                TextEdit::singleline(&mut self.browse_query)
                                    .id_source(BROWSE_SEARCH_INPUT_ID)
                                    .hint_text(if how_expanded > 0.8 {
                                        text.browse_search_hint()
                                    } else {
                                        ""
                                    })
                                    .frame(false)
                                    .desired_width(input_rect.width()),
                            );
                            if self.browse_search_focus_pending {
                                edit_resp.request_focus();
                                self.browse_search_focus_pending = false;
                            }
                            if edit_resp.lost_focus()
                                && ui.input(|input| input.key_pressed(egui::Key::Enter))
                            {
                                self.restart_browse_query();
                            }
                        }

                        if how_expanded > 0.9 {
                            let mut next_x = rect.right() - 16.0;

                            let action_icon = if is_empty {
                                Icon::RotateCw
                            } else {
                                Icon::ArrowRightCircle
                            };
                            let action_pos = egui::pos2(next_x, rect.center().y);
                            let action_area =
                                egui::Rect::from_center_size(action_pos, egui::Vec2::splat(24.0));
                            let action_resp = ui.interact(
                                action_area,
                                ui.id().with("browse_search_submit"),
                                Sense::click(),
                            );
                            let action_color = if action_resp.hovered() {
                                Color32::WHITE
                            } else {
                                Color32::from_gray(170)
                            };

                            ui.painter().text(
                                action_pos,
                                egui::Align2::CENTER_CENTER,
                                icon_char(action_icon),
                                egui::FontId::new(16.0, FontFamily::Name(LUCIDE_FAMILY.into())),
                                action_color,
                            );
                            if action_resp.clicked() {
                                self.restart_browse_query();
                            }
                            action_resp
                                .clone()
                                .on_hover_cursor(egui::CursorIcon::PointingHand);

                            next_x -= 24.0;

                            if !is_empty {
                                let x_pos = egui::pos2(next_x, rect.center().y);
                                let x_area =
                                    egui::Rect::from_center_size(x_pos, egui::Vec2::splat(24.0));
                                let x_resp = ui.interact(
                                    x_area,
                                    ui.id().with("browse_search_clear"),
                                    Sense::click(),
                                );
                                let x_color = if x_resp.hovered() {
                                    Color32::from_gray(225)
                                } else {
                                    Color32::from_gray(120)
                                };
                                ui.painter().text(
                                    x_pos,
                                    egui::Align2::CENTER_CENTER,
                                    icon_char(Icon::X),
                                    egui::FontId::new(14.0, FontFamily::Name(LUCIDE_FAMILY.into())),
                                    x_color,
                                );
                                if x_resp.clicked() {
                                    self.browse_query.clear();
                                    self.restart_browse_query();
                                }
                                x_resp
                                    .clone()
                                    .on_hover_cursor(egui::CursorIcon::PointingHand);
                            }
                        }
                    });

                    // Floating Header Label: Disappears if expanded
                    let header_visibility = 1.0 - how_expanded;
                    if header_visibility > 0.01 {
                        ui.add_space(-4.0 * header_visibility);
                        let label_width = 160.0 * header_visibility;
                        let (label_rect, label_resp) =
                            ui.allocate_exact_size(egui::vec2(label_width, 41.0), Sense::click());

                        if label_resp.clicked() {
                            self.browse_search_expanded = true;
                        }
                        label_resp
                            .clone()
                            .on_hover_cursor(egui::CursorIcon::PointingHand);

                        let slide_left = 40.0 * (1.0 - header_visibility);
                        let text_color = Color32::from_rgba_premultiplied(
                            228,
                            231,
                            235,
                            (header_visibility * 255.0) as u8,
                        );
                        let title_text =
                            bold(text.browse_mods_title(), Some(18.0)).color(text_color);
                        let title_galley = egui::WidgetText::from(title_text).into_galley(
                            ui,
                            Some(egui::TextWrapMode::Extend),
                            f32::INFINITY,
                            egui::FontSelection::Default,
                        );
                        let extended_clip_rect = label_rect.expand2(egui::vec2(10.0, 0.0));
                        ui.painter().with_clip_rect(extended_clip_rect).galley(
                            egui::Align2::LEFT_CENTER
                                .align_size_within_rect(title_galley.size(), label_rect)
                                .min
                                + egui::vec2(-slide_left - 10.0, 0.0),
                            title_galley,
                            text_color,
                        );
                    }

                    ui.add_space(-1.0);
                    ui.scope(|ui| {
                        let radius = egui::CornerRadius::same(3);
                        ui.style_mut().visuals.widgets.inactive.corner_radius = radius;
                        ui.style_mut().visuals.widgets.hovered.corner_radius = radius;
                        ui.style_mut().visuals.widgets.active.corner_radius = radius;
                        ui.style_mut().visuals.widgets.open.corner_radius = radius;
                        let response = ui
                            .add(
                                egui::Button::new(icon_text_sized(
                                    Icon::Users,
                                    text.browse_characters(),
                                    12.0,
                                    12.0,
                                ))
                                .corner_radius(radius),
                            )
                            .on_hover_cursor(egui::CursorIcon::PointingHand);
                        egui::Popup::from_toggle_button_response(&response)
                            .kind(egui::PopupKind::Menu)
                            .layout(egui::Layout::top_down(egui::Align::Min))
                            .width(BROWSE_CHARACTER_PICKER_WIDTH)
                            .gap(0.0)
                            .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
                            .frame(egui::Frame::popup(ui.style()))
                            .show(|ui| self.render_browse_character_picker(ui));
                    });

                    ui.add_space(-1.0);
                    let mut sort_changed = false;
                    ui.scope(|ui| {
                        ui.add_space(5.0);
                        ui.spacing_mut().icon_width = 7.5;
                        ui.spacing_mut().icon_spacing = 2.0;

                        let visuals = ui.visuals_mut();
                        visuals.widgets.inactive.bg_fill =
                            Color32::from_rgba_unmultiplied(96, 96, 96, 182);
                        visuals.widgets.hovered.bg_fill = Color32::from_gray(182);
                        visuals.widgets.active.bg_fill = Color32::from_gray(96);
                        visuals.widgets.inactive.bg_stroke.color = Color32::from_gray(160);
                        visuals.selection.bg_fill = Color32::BLACK;

                        ui.vertical(|ui| {
                            ui.spacing_mut().item_spacing.y = -2.0;
                            ui.add_space(2.0);
                            if self.browse_state.selected_character_category.is_some() {
                                sort_changed |= ui
                                    .radio_value(
                                        &mut self.state.static_prefs.browse_sort,
                                        BrowseSort::Popular,
                                        RichText::new(text.browse_popular()).size(11.0),
                                    )
                                    .changed();
                                sort_changed |= ui
                                    .radio_value(
                                        &mut self.state.static_prefs.browse_sort,
                                        BrowseSort::RecentUpdated,
                                        RichText::new(text.browse_recent_updated()).size(11.0),
                                    )
                                    .changed();
                            } else if is_empty {
                                sort_changed |= ui
                                    .radio_value(
                                        &mut self.state.static_prefs.browse_sort,
                                        BrowseSort::Popular,
                                        RichText::new(text.browse_popular()).size(11.0),
                                    )
                                    .changed();
                                sort_changed |= ui
                                    .radio_value(
                                        &mut self.state.static_prefs.browse_sort,
                                        BrowseSort::RecentUpdated,
                                        RichText::new(text.browse_recent_updated()).size(11.0),
                                    )
                                    .changed();
                            } else {
                                sort_changed |= ui
                                    .radio_value(
                                        &mut self.state.static_prefs.search_sort,
                                        SearchSort::BestMatch,
                                        RichText::new(text.browse_best_match()).size(11.0),
                                    )
                                    .changed();
                                sort_changed |= ui
                                    .radio_value(
                                        &mut self.state.static_prefs.search_sort,
                                        SearchSort::RecentUpdated,
                                        RichText::new(text.browse_recent_updated()).size(11.0),
                                    )
                                    .changed();
                            }
                        });
                    });
                    if sort_changed {
                        self.save_state();
                        self.restart_browse_query();
                    }

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.vertical(|ui| {
                            ui.with_layout(egui::Layout::top_down(egui::Align::Max), |ui| {
                                let count_label = self
                                    .browse_state
                                    .total_count
                                    .map(|count| text.browse_mods_count(count))
                                    .unwrap_or_else(|| text.browse_loading().to_string());
                                static_label(
                                    ui,
                                    RichText::new(count_label)
                                        .size(13.0)
                                        .color(Color32::from_gray(160)),
                                );
                                let hidden_count = self.hidden_unsafe_browse_count();
                                if hidden_count > 0 {
                                    ui.add_space(-10.0);
                                    static_label(
                                        ui,
                                        RichText::new(text.browse_hidden_nsfw_count(hidden_count))
                                            .size(11.0)
                                            .color(Color32::from_rgb(168, 112, 112)),
                                    );
                                }
                            });
                        });
                    });
                });
            });

        let left_padding = 12.0;
        ui.add_space(8.0);
        if let Some(error) = self.browse_state.page_error.clone() {
            let error_label = if error == text.connection_timed_out() {
                error.as_str()
            } else {
                text.connection_failed()
            };
            ui.horizontal(|ui| {
                ui.add_space(left_padding);
                ui.vertical(|ui| {
                    static_label(
                        ui,
                        RichText::new(error_label)
                            .size(14.0)
                            .color(Color32::from_rgb(213, 139, 139)),
                    );
                    if ui
                        .add(
                            egui::Button::new(RichText::new(text.task_retry()).size(12.0))
                                .corner_radius(egui::CornerRadius::same(2)),
                        )
                        .clicked()
                    {
                        self.restart_browse_query();
                    }
                });
            });
            ui.add_space(8.0);
        }
        let scroll_rect = ui.available_rect_before_wrap();
        let scroll_navigation = vertical_scroll_navigation(ui, scroll_rect);
        ScrollArea::vertical()
            .id_salt("browse_main_mod_grid_scroll")
            .show(ui, |ui| {
            apply_vertical_scroll_navigation(ui, scroll_navigation, false);
            ui.spacing_mut().item_spacing.x = 8.0;
            if let Some(category) = self.browse_state.selected_character_category.clone() {
                ui.horizontal(|ui| {
                    ui.add_space(left_padding);
                    egui::Frame::new()
                        .fill(Color32::from_rgba_premultiplied(33, 35, 39, 242))
                        .corner_radius(egui::CornerRadius::same(6))
                        .stroke(egui::Stroke::new(1.0, Color32::from_rgb(70, 75, 82)))
                        .inner_margin(egui::Margin {
                            left: 8,
                            right: 8,
                            top: 6,
                            bottom: 6,
                        })
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                if let Some(url) = category.icon_url.as_ref() {
                                    let key = Self::browse_thumb_texture_key(
                                        url,
                                        ThumbnailProfile::Icon,
                                    );
                                    self.queue_browse_image_with_profile(
                                        url.clone(),
                                        None,
                                        false,
                                        ThumbnailProfile::Icon,
                                        2,
                                    );
                                    let (rect, _) =
                                        ui.allocate_exact_size(Vec2::splat(24.0), Sense::hover());
                                    if let Some(texture) = self.get_browse_thumb_texture(&key, 1) {
                                        paint_thumbnail_image(
                                            ui,
                                            rect,
                                            texture,
                                            ThumbnailFit::Contain,
                                            Color32::WHITE,
                                            egui::CornerRadius::same(4),
                                        );
                                    } else {
                                        ui.painter().rect_filled(
                                            rect,
                                            4.0,
                                            Color32::from_rgba_premultiplied(45, 48, 53, 242),
                                        );
                                    }
                                }
                                static_label(
                                    ui,
                                    RichText::new(&category.name)
                                        .size(13.0)
                                        .strong()
                                        .color(Color32::from_rgb(247, 222, 204)),
                                );
                                static_label(
                                    ui,
                                    RichText::new(
                                        text.browse_selected_character_mods_count(
                                            category.item_count,
                                        ),
                                    )
                                        .size(12.0)
                                        .color(Color32::from_gray(155)),
                                );
                                if ui
                                    .add_sized(
                                        [24.0, 24.0],
                                        egui::Button::new(icon_rich(
                                            Icon::X,
                                            12.0,
                                            Color32::from_gray(190),
                                        ))
                                        .corner_radius(egui::CornerRadius::same(3)),
                                    )
                                    .on_hover_text(text.browse_show_all_mods())
                                    .on_hover_cursor(egui::CursorIcon::PointingHand)
                                    .clicked()
                                {
                                    self.clear_browse_character_category();
                                }
                            });
                        });
                });
                ui.add_space(8.0);
            }
            let available = ui.available_width().max(BROWSE_PANEL_CARD_WIDTH + left_padding);
            let max_card_width = (available - left_padding).max(BROWSE_PANEL_CARD_WIDTH);
            let columns = ((max_card_width + 8.0) / (BROWSE_PANEL_CARD_WIDTH + 8.0))
                .floor()
                .max(1.0) as usize;
            let cards: Vec<BrowseCard> = self
                .browse_cards_for_display()
                .into_iter()
                .cloned()
                .collect();

            if cards.is_empty() && self.browse_state.loading_page {
                ui.add_space(12.0);
                ui.centered_and_justified(|ui| {
                    static_label(
                        ui,
                        RichText::new(text.browse_fetching_mods())
                            .size(16.0)
                            .color(Color32::from_gray(180)),
                    );
                });
            }

            // Viewport culling for browse cards
            let viewport = ui.clip_rect();
            let viewport_top = viewport.top();
            let viewport_bottom = viewport.bottom();
            let card_spacing = 8.0;
            let row_height = BROWSE_CARD_HEIGHT + card_spacing;
            let buffer_rows = 2; // Render 2 extra rows above/below for smooth scrolling

            for row in cards.chunks(columns) {
                // Calculate row position
                let row_top = ui.cursor().top();
                let row_bottom = row_top + row_height;
                
                // Check if row is visible (with buffer)
                let is_visible = row_bottom >= viewport_top - (buffer_rows as f32 * row_height)
                    && row_top <= viewport_bottom + (buffer_rows as f32 * row_height);
                
                if !is_visible {
                    // Just allocate space for invisible rows
                    ui.add_space(row_height);
                    continue;
                }

                ui.horizontal_top(|ui| {
                    ui.add_space(left_padding);
                    for card in row {
                        let selected = self.browse_state.selected_mod_id == Some(card.id);
                        let frame_fill = if selected {
                            Color32::from_rgba_premultiplied(43, 44, 50, 242)
                        } else {
                            Color32::from_rgba_premultiplied(33, 35, 39, 242)
                        };
                        let card_frame = egui::Frame::new()
                            .fill(frame_fill)
                            .corner_radius(egui::CornerRadius::same(8))
                            .stroke(egui::Stroke::new(
                                1.0,
                                if selected {
                                    Color32::from_rgb(108, 122, 160)
                                } else {
                                    Color32::from_rgb(60, 64, 70)
                                },
                            ))
                            .inner_margin(egui::Margin::same(0))
                            .show(ui, |ui| {
                                ui.set_width(BROWSE_PANEL_CARD_WIDTH);
                                ui.vertical(|ui| {
                                    let (rect, response) = ui.allocate_exact_size(
                                        Vec2::new(BROWSE_PANEL_CARD_WIDTH, BROWSE_THUMBNAIL_HEIGHT),
                                        Sense::click(),
                                    );
                                    ui.painter().rect_filled(
                                        rect,
                                        8.0,
                                        Color32::from_rgba_premultiplied(45, 48, 53, 242),
                                    );
                                    if let Some(url) = &card.thumbnail_url {
                                        let key = Self::browse_thumb_texture_key(url, ThumbnailProfile::Card);
                                        let clip = ui.clip_rect();
                                        let is_visible = rect.intersects(clip);
                                        let priority = if is_visible { 2 } else { 1 };

                                        if let Some(texture) = self.get_browse_thumb_texture(&key, priority) {
                                            paint_thumbnail_image(
                                                ui,
                                                rect,
                                                texture,
                                                ThumbnailFit::Cover,
                                                Color32::WHITE,
                                                egui::CornerRadius::same(8),
                                            );
                                        } else if let Some(placeholder) =
                                            self.mod_thumbnail_placeholder.as_ref()
                                        {
                                            paint_thumbnail_image(
                                                ui,
                                                rect,
                                                placeholder,
                                                ThumbnailFit::Contain,
                                                Color32::from_white_alpha(51),
                                                egui::CornerRadius::same(8),
                                            );
                                        }
                                    } else if let Some(placeholder) =
                                        self.mod_thumbnail_placeholder.as_ref()
                                    {
                                        paint_thumbnail_image(
                                            ui,
                                            rect,
                                            placeholder,
                                            ThumbnailFit::Contain,
                                            Color32::from_white_alpha(51),
                                            egui::CornerRadius::same(8),
                                        );
                                    }
                                    if card.unsafe_content && self.should_censor_unsafe() {
                                        paint_unsafe_overlay(
                                            ui,
                                            rect,
                                            egui::CornerRadius::same(8),
                                        );
                                    }
                                    if response.clicked() {
                                        if self.browse_state.selected_mod_id == Some(card.id) && self.browse_detail_open {
                                            self.browse_state.selected_mod_id = None;
                                            self.browse_detail_open = false;
                                        } else {
                                            self.open_browse_detail(card.id);
                                        }
                                    }
                                    egui::Frame::new()
                                        .inner_margin(egui::Margin {
                                            left: 8,
                                            right: 8,
                                            top: 4,
                                            bottom: 8,
                                        })
                                        .show(ui, |ui| {
                                            ui.add(
                                                egui::Label::new(
                                                    RichText::new(&card.name)
                                                        .size(15.0)
                                                        .strong()
                                                        .color(Color32::from_rgb(228, 231, 235)),
                                                )
                                                .selectable(false),
                                            ).on_hover_cursor(egui::CursorIcon::Default);
                                            ui.add_space(-4.0);
                                            ui.horizontal(|ui| {
                                                ui.vertical(|ui| {
                                                    ui.add_space(4.0);
                                                    let is_installed = self.is_browse_mod_installed(card);
                                                    let selected_game_ready = self.selected_game_is_installed_or_configured();
                                                    ui.spacing_mut().button_padding.y = 4.0;
                                                    let install_response = ui.add_enabled(
                                                        card.has_files && selected_game_ready,
                                                        egui::Button::new(
                                                            RichText::new(if is_installed { text.installed() } else { text.install() })
                                                                .size(15.0)
                                                                .color(if is_installed {
                                                                    Color32::from_rgb(214, 218, 224)
                                                                } else {
                                                                    Color32::from_rgb(247, 222, 204)
                                                                }),
                                                        )
                                                        .fill(if is_installed {
                                                            Color32::from_rgb(86, 92, 100)
                                                        } else {
                                                            Color32::from_rgb(180, 78, 35)
                                                        })
                                                        .corner_radius(egui::CornerRadius::same(3))
                                                        .min_size(Vec2::new(56.0, 4.0)),
                                                    );
                                                    if !selected_game_ready {
                                                        install_response
                                                            .clone()
                                                            .on_hover_text(text.game_not_installed())
                                                            .on_hover_cursor(egui::CursorIcon::NotAllowed);
                                                    } else if card.has_files {
                                                        install_response
                                                            .clone()
                                                            .on_hover_cursor(egui::CursorIcon::PointingHand);
                                                    }
                                                    if install_response.clicked() {
                                                        self.queue_install_for_browse_mod(card.id, false);
                                                    }
                                                });
                                                ui.add_space(24.0);
                                                ui.allocate_ui_with_layout(
                                                    ui.available_size(),
                                                    egui::Layout::right_to_left(egui::Align::Center),
                                                    |ui| {
                                                        ui.with_layout(egui::Layout::top_down(egui::Align::Max), |ui| {
                                                            ui.add(
                                                                egui::Label::new(
                                                                    RichText::new(&card.author_name)
                                                                        .size(11.0)
                                                                        .color(Color32::from_gray(168))
                                                                )
                                                                .truncate()
                                                                .selectable(false)
                                                            ).on_hover_cursor(egui::CursorIcon::Default);
                                                            ui.add_space(-6.0);
                                                            ui.horizontal(|ui| {
                                                                ui.add(egui::Label::new(RichText::new(relative_time_label_at(card.updated_at, age_now, false, text)).size(11.5).color(Color32::from_gray(145))).selectable(false)).on_hover_cursor(egui::CursorIcon::Default);
                                                                ui.add_space(-5.0);
                                                                ui.add(egui::Label::new(icon_rich(Icon::CalendarClock, 12.0, Color32::from_rgb(112, 164, 118))).selectable(false)).on_hover_cursor(egui::CursorIcon::Default);
                                                                ui.add_space(4.0);
                                                                let likes_response = ui.add(egui::Label::new(RichText::new(format_compact_count(card.like_count)).size(11.5).color(Color32::from_gray(178))).selectable(false)).on_hover_cursor(egui::CursorIcon::Default);
                                                                if card.like_count >= 1_000 {
                                                                    likes_response.on_hover_text(format_count_with_separators(card.like_count));
                                                                }
                                                                ui.add_space(-5.0);
                                                                ui.add(egui::Label::new(icon_rich(Icon::Heart, 12.0, Color32::from_rgb(214, 132, 154))).selectable(false)).on_hover_cursor(egui::CursorIcon::Default);
                                                            });
                                                        });
                                                    },
                                                );
                                             });
                                        });
                                });
                            });
                        let popup_id =
                            ui.id().with(("browse_card_context_menu_popup", card.id));
                        let open_context_menu = ui.ctx().input(|i| {
                            i.pointer.secondary_clicked()
                                && i.pointer
                                    .hover_pos()
                                    .is_some_and(|pos| card_frame.response.rect.contains(pos))
                        });
                        egui::Popup::new(
                            popup_id,
                            ui.ctx().clone(),
                            egui::PopupAnchor::PointerFixed,
                            card_frame.response.layer_id,
                        )
                        .kind(egui::PopupKind::Menu)
                        .layout(egui::Layout::top_down_justified(egui::Align::Min))
                        .width(156.0)
                        .gap(0.0)
                        .frame(
                            egui::Frame::menu(ui.style())
                                .fill({
                                    let fill = ui.style().visuals.window_fill();
                                    Color32::from_rgba_premultiplied(
                                        fill.r(),
                                        fill.g(),
                                        fill.b(),
                                        ((fill.a() as f32) * 0.9).round() as u8,
                                    )
                                })
                                .inner_margin(egui::Margin::same(12)),
                        )
                        .open_memory(open_context_menu.then_some(
                            egui::SetOpenCommand::Bool(true),
                        ))
                        .show(|ui| {
                            ui.set_min_width(156.0);
                            let radius = egui::CornerRadius::same(3);
                            ui.style_mut().visuals.widgets.inactive.corner_radius = radius;
                            ui.style_mut().visuals.widgets.hovered.corner_radius = radius;
                            ui.style_mut().visuals.widgets.active.corner_radius = radius;
                            ui.style_mut().visuals.widgets.open.corner_radius = radius;
                            ui.add_sized(
                                [ui.available_width(), 0.0],
                                egui::Label::new(
                                    RichText::new(&card.name)
                                        .size(12.5)
                                        .strong()
                                        .color(Color32::from_rgb(228, 231, 235)),
                                )
                                .halign(egui::Align::Min)
                                .wrap()
                                .selectable(false),
                            )
                            .on_hover_cursor(egui::CursorIcon::Default);
                            ui.add_space(-2.0);
                            ui.separator();
                            ui.add_space(-2.0);
                            let detail = self.browse_state.details.get(&card.id).cloned();
                            if detail.is_none() {
                                if !self.browse_state.loading_details.contains(&card.id) {
                                    self.request_browse_detail(card.id);
                                }
                                ui.ctx().output_mut(|o| o.cursor_icon = egui::CursorIcon::Progress);
                                ui.ctx().request_repaint();
                                static_label(
                                    ui,
                                    RichText::new(text.browse_loading())
                                        .size(12.0)
                                        .color(Color32::from_gray(170)),
                                );
                                return;
                            }

                            let detail = detail.expect("checked above");
                            let profile = detail.translated_profile.as_ref().unwrap_or(&detail.profile);
                            let install_disabled_reason =
                                gamebanana::install_block_reason(profile)
                                    .unwrap_or_default();
                            let install_blocked = !install_disabled_reason.is_empty();
                            let selected_game_ready = self.selected_game_is_installed_or_configured();

                            let install_response = ui.add_enabled(
                                !install_blocked && selected_game_ready,
                                egui::Button::new(icon_text_sized(
                                    Icon::PackagePlus,
                                    text.install(),
                                    13.0,
                                    13.0,
                                ))
                                .fill(Color32::from_rgb(180, 78, 35))
                                .stroke(egui::Stroke::new(
                                    1.0,
                                    Color32::from_rgb(180, 78, 35),
                                ))
                                .corner_radius(radius),
                            ).on_hover_cursor(egui::CursorIcon::PointingHand);
                            if !selected_game_ready {
                                install_response
                                    .clone()
                                    .on_hover_text_at_pointer(text.game_not_installed())
                                    .on_hover_cursor(egui::CursorIcon::NotAllowed);
                            } else {
                                install_response
                                    .clone()
                                    .on_hover_text_at_pointer(install_disabled_reason.clone());
                            }
                            if install_response.clicked() {
                                self.queue_install_for_browse_mod(card.id, false);
                                ui.close();
                            }

                            let install_disabled_response = ui.add_enabled(
                                !install_blocked && selected_game_ready,
                                egui::Button::new(icon_text_sized(
                                    Icon::PackagePlus,
                                    text.install_disabled(),
                                    13.0,
                                    13.0,
                                ))
                                .corner_radius(radius),
                            );
                            if !selected_game_ready {
                                install_disabled_response
                                    .clone()
                                    .on_hover_text_at_pointer(text.game_not_installed())
                                    .on_hover_cursor(egui::CursorIcon::NotAllowed);
                            } else if install_blocked {
                                install_disabled_response
                                    .clone()
                                    .on_hover_text_at_pointer(install_disabled_reason.clone());
                            } else {
                                install_disabled_response
                                    .clone()
                                    .on_hover_cursor(egui::CursorIcon::PointingHand);
                            }
                            if install_disabled_response.clicked() {
                                self.queue_install_for_browse_mod(card.id, true);
                                ui.close();
                            }

                            let browser_response = ui.button(icon_text_sized(
                                Icon::Globe,
                                text.open_in_browser(),
                                13.0,
                                13.0,
                            )).on_hover_cursor(egui::CursorIcon::PointingHand);
                            if browser_response.clicked() {
                                if let Err(err) = open_external_url(&gamebanana::browser_url(card.id)) {
                                    self.report_error(err, Some(text.could_not_open_browser()));
                                }
                                ui.close();
                            }
                        });
                        if egui::Popup::is_id_open(ui.ctx(), popup_id)
                            && self.browse_state.loading_details.contains(&card.id)
                        {
                            ui.ctx().output_mut(|o| o.cursor_icon = egui::CursorIcon::Progress);
                            ui.ctx().request_repaint();
                        }
                    }
                });
                ui.add_space(4.0);
                ui.add_space(6.0);
            }

            if self.browse_state.loading_page {
                ui.add_space(8.0);
                ui.centered_and_justified(|ui| {
                    static_label(
                        ui,
                        RichText::new(text.browse_loading_more())
                            .size(12.5)
                            .color(Color32::from_gray(165)),
                    );
                });
            }

            let sentinel = ui.allocate_response(Vec2::new(ui.available_width(), 400.0), Sense::hover());
            if sentinel.rect.intersects(ui.clip_rect())
                && !self.browse_state.loading_page
                && self.browse_state.page_error.is_none()
                && self.browse_state.has_more
            {
                self.request_browse_page(self.browse_state.next_page);
            }
            apply_vertical_scroll_navigation(ui, scroll_navigation, true);
        });
    }

    fn render_browse_character_picker(&mut self, ui: &mut Ui) {
        let text = self.text();
        ui.set_width(BROWSE_CHARACTER_PICKER_WIDTH);
        ui.set_min_height(BROWSE_CHARACTER_PICKER_HEIGHT);
        ui.add_space(4.0);

        if self.selected_game().is_none_or(|game| {
            gamebanana::character_super_category_id_for_hestia(&game.definition.id).is_none()
        }) {
            ui.allocate_ui_with_layout(
                Vec2::new(
                    BROWSE_CHARACTER_PICKER_WIDTH - 8.0,
                    BROWSE_CHARACTER_PICKER_HEIGHT - 8.0,
                ),
                egui::Layout::top_down(egui::Align::Min),
                |ui| {
                    static_label(
                        ui,
                        RichText::new(text.browse_no_character_list())
                            .size(12.5)
                            .color(Color32::from_gray(170)),
                    );
                },
            );
            return;
        }

        if self.browse_state.character_categories.is_empty()
            && !self.browse_state.character_categories_loading
        {
            self.request_browse_character_categories(false);
        }

        let category_count = self.browse_state.character_categories.len();
        let selected_category_name = self
            .browse_state
            .selected_character_category
            .as_ref()
            .map(|category| category.name.clone());
        let header_width = BROWSE_CHARACTER_PICKER_WIDTH - 8.0;
        let header_response = ui.allocate_response(
            Vec2::new(header_width, BROWSE_CHARACTER_PICKER_HEADER_HEIGHT),
            Sense::hover(),
        );
        let header_rect = header_response.rect;
        let title_font = egui::FontId::new(14.0, FontFamily::Name(BOLD_FONT_FAMILY.into()));
        let title_pos = egui::pos2(header_rect.center().x, header_rect.top() + 14.0);
        ui.painter().text(
            title_pos,
            egui::Align2::CENTER_CENTER,
            text.browse_characters(),
            title_font,
            Color32::from_rgb(228, 231, 235),
        );
        let underline_y = title_pos.y + 9.0;
        ui.painter().line_segment(
            [
                egui::pos2(title_pos.x - 34.0, underline_y),
                egui::pos2(title_pos.x + 34.0, underline_y),
            ],
            egui::Stroke::new(1.0, Color32::from_rgb(228, 231, 235)),
        );
        let header_icon_size = 24.0;
        let header_icon_gap = -4.0;
        let refresh_rect = egui::Rect::from_center_size(
            egui::pos2(title_pos.x + 54.0, header_rect.center().y),
            Vec2::splat(header_icon_size),
        );
        let refresh_response = ui
            .interact(
                refresh_rect,
                ui.id().with("browse_character_refresh"),
                Sense::click(),
            )
            .on_hover_text(text.browse_refresh_characters())
            .on_hover_cursor(egui::CursorIcon::PointingHand);
        ui.painter().text(
            refresh_rect.center(),
            egui::Align2::CENTER_CENTER,
            icon_char(Icon::RotateCw),
            egui::FontId::new(13.0, FontFamily::Name(LUCIDE_FAMILY.into())),
            if refresh_response.hovered() {
                Color32::WHITE
            } else {
                Color32::from_gray(185)
            },
        );
        if refresh_response.clicked() {
            self.request_browse_character_categories(true);
        }
        if self.browse_state.selected_character_category.is_some() {
            let clear_rect = egui::Rect::from_center_size(
                egui::pos2(
                    refresh_rect.right() + header_icon_gap + header_icon_size / 2.0,
                    header_rect.center().y,
                ),
                Vec2::splat(header_icon_size),
            );
            let clear_response = ui
                .interact(
                    clear_rect,
                    ui.id().with("browse_character_clear"),
                    Sense::click(),
                )
                .on_hover_text(text.browse_clear_filter())
                .on_hover_cursor(egui::CursorIcon::PointingHand);
            ui.painter().text(
                clear_rect.center(),
                egui::Align2::CENTER_CENTER,
                icon_char(Icon::CircleX),
                egui::FontId::new(13.0, FontFamily::Name(LUCIDE_FAMILY.into())),
                if clear_response.hovered() {
                    Color32::WHITE
                } else {
                    Color32::from_gray(185)
                },
            );
            if clear_response.clicked() {
                self.clear_browse_character_category();
                ui.close();
            }
        }
        ui.separator();
        let status = if self.browse_state.character_categories_loading {
            text.browse_loading().to_string()
        } else if let Some(name) = selected_category_name.as_deref() {
            text.browse_selected_character(&Self::take_label_chunk(name, 30, true))
        } else if category_count > 0 {
            text.browse_character_count(category_count)
        } else {
            text.browse_waiting().to_string()
        };
        ui.painter().text(
            egui::pos2(header_rect.center().x, header_rect.top() + 35.0),
            egui::Align2::CENTER_CENTER,
            status,
            egui::FontId::proportional(11.5),
            Color32::from_gray(150),
        );

        let grid_width = BROWSE_CHARACTER_PICKER_WIDTH - 8.0;
        let grid_height =
            BROWSE_CHARACTER_PICKER_HEIGHT - BROWSE_CHARACTER_PICKER_HEADER_HEIGHT - 18.0;
        if self.browse_state.character_categories_loading {
            ui.allocate_ui_with_layout(
                Vec2::new(grid_width, grid_height),
                egui::Layout::top_down(egui::Align::Min),
                |ui| {
                    ui.add_space(4.0);
                    static_label(
                        ui,
                        RichText::new(text.browse_loading())
                            .size(12.5)
                            .color(Color32::from_gray(170)),
                    );
                },
            );
            ui.ctx().request_repaint();
            return;
        }

        if self.browse_state.character_categories.is_empty() {
            ui.allocate_ui_with_layout(
                Vec2::new(grid_width, grid_height),
                egui::Layout::top_down(egui::Align::Min),
                |ui| {
                    ui.add_space(4.0);
                    static_label(
                        ui,
                        RichText::new(text.browse_no_characters_returned())
                            .size(12.5)
                            .color(Color32::from_gray(170)),
                    );
                },
            );
            return;
        }

        let categories = self.browse_state.character_categories.clone();
        let columns = 3;
        let scroll_rect = ui.available_rect_before_wrap();
        let scroll_navigation = vertical_scroll_navigation(ui, scroll_rect);
        ScrollArea::vertical()
            .max_height(grid_height)
            .min_scrolled_height(grid_height)
            .auto_shrink([false, false])
            .id_salt("browse_character_picker_scroll")
            .show(ui, |ui| {
                apply_vertical_scroll_navigation(ui, scroll_navigation, false);
                ui.spacing_mut().item_spacing = Vec2::new(6.0, 6.0);
                let selected_id = self
                    .browse_state
                    .selected_character_category
                    .as_ref()
                    .map(|category| category.id);
                for row in categories.chunks(columns) {
                    ui.horizontal_top(|ui| {
                        for category in row {
                            let selected = selected_id == Some(category.id);
                            let response =
                                self.render_browse_character_tile(ui, category, selected);
                            if response.clicked() {
                                self.select_browse_character_category(category.clone());
                                ui.close();
                            }
                        }
                    });
                }
                apply_vertical_scroll_navigation(ui, scroll_navigation, true);
            });
    }

    fn render_browse_character_tile(
        &mut self,
        ui: &mut Ui,
        category: &BrowseCharacterCategory,
        selected: bool,
    ) -> egui::Response {
        let (rect, response) = ui.allocate_exact_size(
            Vec2::new(BROWSE_CHARACTER_TILE_WIDTH, BROWSE_CHARACTER_TILE_HEIGHT),
            Sense::click(),
        );
        let hovered = response.hovered();
        let fill = if selected {
            Color32::from_rgba_premultiplied(74, 50, 39, 242)
        } else if hovered {
            Color32::from_rgba_premultiplied(49, 54, 61, 242)
        } else {
            Color32::from_rgba_premultiplied(34, 37, 42, 242)
        };
        ui.painter()
            .rect_filled(rect, egui::CornerRadius::same(3), fill);
        if selected || hovered {
            let accent_rect =
                egui::Rect::from_min_max(rect.min, egui::pos2(rect.right(), rect.top() + 2.0));
            ui.painter().rect_filled(
                accent_rect,
                egui::CornerRadius::same(3),
                if selected {
                    Color32::from_rgb(214, 104, 58)
                } else {
                    Color32::from_rgb(108, 122, 160)
                },
            );
        }
        ui.painter().rect_stroke(
            rect,
            egui::CornerRadius::same(3),
            egui::Stroke::new(
                1.0,
                if selected {
                    Color32::from_rgb(180, 78, 35)
                } else if hovered {
                    Color32::from_rgb(108, 122, 160)
                } else {
                    Color32::from_rgb(60, 64, 70)
                },
            ),
            egui::StrokeKind::Inside,
        );

        let mut child_ui = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(rect.shrink2(Vec2::new(6.0, 5.0)))
                .layout(egui::Layout::top_down(egui::Align::Center)),
        );
        let icon_rect = child_ui
            .allocate_exact_size(Vec2::splat(40.0), Sense::hover())
            .0;
        child_ui.painter().rect_filled(
            icon_rect,
            egui::CornerRadius::same(3),
            if selected {
                Color32::from_rgba_premultiplied(92, 61, 46, 242)
            } else if hovered {
                Color32::from_rgba_premultiplied(60, 65, 72, 242)
            } else {
                Color32::from_rgba_premultiplied(45, 48, 53, 242)
            },
        );
        child_ui.painter().rect_stroke(
            icon_rect,
            egui::CornerRadius::same(3),
            egui::Stroke::new(
                1.0,
                if selected {
                    Color32::from_rgb(180, 78, 35)
                } else {
                    Color32::from_rgb(68, 73, 80)
                },
            ),
            egui::StrokeKind::Inside,
        );
        if let Some(url) = category.icon_url.as_ref() {
            let key = Self::browse_thumb_texture_key(url, ThumbnailProfile::Icon);
            self.queue_browse_image_with_profile(
                url.clone(),
                None,
                false,
                ThumbnailProfile::Icon,
                3,
            );
            if let Some(texture) = self.get_browse_thumb_texture(&key, 2) {
                paint_thumbnail_image(
                    &mut child_ui,
                    icon_rect,
                    texture,
                    ThumbnailFit::Contain,
                    Color32::WHITE,
                    egui::CornerRadius::same(3),
                );
            }
        }

        child_ui.add_space(1.0);
        let (line_1, line_2) =
            Self::browse_character_label_lines(&category.name, category.item_count);
        let color = if selected {
            Color32::from_rgb(247, 222, 204)
        } else if hovered {
            Color32::from_rgb(236, 239, 244)
        } else {
            Color32::from_rgb(218, 222, 228)
        };
        static_label(&mut child_ui, RichText::new(line_1).size(11.5).color(color));
        if let Some(line_2) = line_2 {
            child_ui.add_space(-6.0);
            static_label(&mut child_ui, RichText::new(line_2).size(11.5).color(color));
        }

        response.on_hover_cursor(egui::CursorIcon::PointingHand)
    }

    fn browse_character_label_lines(name: &str, item_count: u64) -> (String, Option<String>) {
        let suffix = format!(" ({item_count})");
        let name = name.trim();
        let one_line_limit = BROWSE_CHARACTER_LABEL_CHARS_PER_LINE;
        if name.chars().count() + suffix.chars().count() <= one_line_limit {
            return (format!("{name}{suffix}"), None);
        }

        let first_limit = one_line_limit;
        let first = Self::take_label_chunk(name, first_limit, false);
        let remaining_owned = name.chars().skip(first.chars().count()).collect::<String>();
        let remaining = remaining_owned.trim_start();
        let second_name_limit = one_line_limit.saturating_sub(suffix.chars().count()).max(1);
        let second_name = Self::take_label_chunk(remaining, second_name_limit, true);
        (first, Some(format!("{second_name}{suffix}")))
    }

    fn take_label_chunk(value: &str, limit: usize, ellipsize: bool) -> String {
        if value.chars().count() <= limit {
            return value.to_string();
        }
        let mut end_byte = value.len();
        for (count, (idx, _)) in value.char_indices().enumerate() {
            if count == limit {
                end_byte = idx;
                break;
            }
        }
        let candidate = &value[..end_byte];
        let trimmed = candidate
            .rfind(char::is_whitespace)
            .filter(|idx| *idx >= limit / 2)
            .map(|idx| &candidate[..idx])
            .unwrap_or(candidate)
            .trim_end();
        if ellipsize {
            format!("{}…", trimmed.trim_end_matches('…'))
        } else {
            trimmed.to_string()
        }
    }

    fn render_browse_detail_window(&mut self, ctx: &egui::Context, pane_rect: egui::Rect) {
        let text = self.text();
        let age_now = Local::now();
        let Some(mod_id) = self.browse_state.selected_mod_id else {
            self.render_browse_screenshot_overlay(ctx);
            return;
        };
        if !self.browse_detail_open {
            self.render_browse_screenshot_overlay(ctx);
            return;
        }

        let details_rect = pane_rect.shrink2(egui::vec2(12.0, 12.0));
        let details_offset = egui::vec2(0.0, 32.0);
        let details_pos = details_rect.min + details_offset;
        let mut browse_detail_open = self.browse_detail_open;
        let response = egui::Window::new(icon_text_sized(Icon::PackageSearch, text.browse_mod_detail(), 14.0, 14.0)) // BROWSE view's mod detail GUI
            .id(egui::Id::new(BROWSE_DETAIL_WINDOW_ID))
            .default_pos(details_pos)
            .default_size(BROWSE_DETAIL_SIZE)
            .open(&mut browse_detail_open)
            .title_bar(true)
            .resizable(false)
            .collapsible(true)
            .movable(true)
            .constrain_to(details_rect)
            .frame(
                egui::Frame::window(&ctx.style()).inner_margin(egui::Margin::same(18)),
            )
            .show(ctx, |ui| {
                let card = self
                    .browse_state
                    .cards
                    .iter()
                    .find(|card| card.id == mod_id)
                    .cloned();
                ui.horizontal_wrapped(|ui| {
                    let title = self.browse_state
                        .details
                        .get(&mod_id)
                        .and_then(|detail| {
                            // If translation is active, use translated name
                            if detail.translation_lang.is_some() {
                                detail.translated_profile.as_ref().map(|p| p.name.clone())
                            } else {
                                None
                            }
                        })
                        .or_else(|| card.as_ref().map(|card| card.name.clone()))
                        .or_else(|| {
                            self.browse_state
                                .details
                                .get(&mod_id)
                                .map(|detail| detail.profile.name.clone())
                        })
                        .filter(|name| !name.trim().is_empty())
                        .unwrap_or_else(|| format!("Mod {mod_id}"));
                    ui.heading(title);
                    let id_response = ui.add(
                        egui::Label::new(
                            RichText::new(format!("ID: {}", mod_id))
                                .size(12.0)
                                .color(Color32::from_gray(158)),
                        )
                        .selectable(false)
                        .sense(Sense::click()),
                    );
                    id_response
                        .clone()
                        .on_hover_text(text.copy_gamebanana_id())
                        .on_hover_cursor(egui::CursorIcon::PointingHand);
                    if id_response.clicked() {
                        ui.ctx().copy_text(mod_id.to_string());
                        self.set_message_ok(text.gamebanana_id_copied());
                    }
                });
                
                if let Some(detail) = self.browse_state.details.get(&mod_id).cloned() {
                    // Use translated profile if available, otherwise use original
                    let profile = detail.translated_profile.as_ref().unwrap_or(&detail.profile);
                    
                    let browse_game_id = card
                        .as_ref()
                        .map(|card| card.game_id.clone())
                        .or_else(|| self.browse_state.active_game_id.clone())
                        .or_else(|| self.selected_game().map(|game| game.definition.id.clone()))
                        .unwrap_or_default();
                    let author_name = card
                        .as_ref()
                        .map(|card| card.author_name.clone())
                        .or_else(|| {
                            profile
                                .submitter
                                .as_ref()
                                .map(|submitter| submitter.name.clone())
                        })
                        .unwrap_or_else(|| text.unknown().to_string());
                    let like_count = card
                        .as_ref()
                        .map(|card| card.like_count)
                        .unwrap_or(profile.like_count);
                    let updated_at = card
                        .as_ref()
                        .map(|card| card.updated_at)
                        .unwrap_or_else(|| {
                            timestamp_to_utc(
                                profile
                                    .date_updated
                                    .unwrap_or(profile.date_modified),
                            )
                        });
                    let fallback_thumbnail_url = card
                        .as_ref()
                        .and_then(|card| card.thumbnail_url.clone())
                        .or_else(|| {
                            profile
                                .preview_media
                                .as_ref()
                                .and_then(|preview| preview.images.first())
                                .and_then(gamebanana::thumbnail_url)
                        });
                    let trashed_by_owner = gamebanana::trashed_by_owner(profile).cloned();
                    let withhold_notice = gamebanana::withheld_notice(profile).cloned();
                    let is_trashed_by_owner = trashed_by_owner.is_some();
                    let is_private = profile.is_private;
                    let is_withheld = withhold_notice.is_some();
                    let is_deleted = profile.is_deleted || profile.id == 0;
                    let is_installed = card
                        .as_ref()
                        .is_some_and(|card| self.is_browse_mod_installed(card));
                    let install_disabled_reason =
                        gamebanana::install_block_reason(profile).unwrap_or_default();
                    let install_blocked = !install_disabled_reason.is_empty();
                    let selected_game_ready = self.selected_game_is_installed_or_configured();
                    ui.add_space(-4.0);
                    ui.horizontal(|ui| {
                        let install_response = ui.add_enabled(
                            !install_blocked && selected_game_ready,
                            egui::Button::new(icon_text_sized(
                                Icon::PackagePlus,
                                if is_installed {
                                    text.installed()
                                } else {
                                    text.install()
                                },
                                13.0,
                                13.0,
                            ))
                                .fill(if is_installed {
                                    Color32::from_rgb(86, 92, 100)
                                } else {
                                    Color32::from_rgb(180, 78, 35)
                                })
                                .corner_radius(egui::CornerRadius::same(6)),
                        );
                        if !selected_game_ready {
                            install_response
                                .clone()
                                .on_hover_text_at_pointer(text.game_not_installed())
                                .on_hover_cursor(egui::CursorIcon::NotAllowed);
                        } else if install_blocked {
                            install_response
                                .clone()
                                .on_hover_text_at_pointer(install_disabled_reason.clone());
                        }
                        let install_response = if selected_game_ready && !install_blocked {
                            install_response.on_hover_cursor(egui::CursorIcon::PointingHand)
                        } else {
                            install_response
                        };
                        if install_response.clicked() {
                            self.queue_install_for_browse_mod(mod_id, false);
                        }
                        install_response.context_menu(|ui| {
                            if ui
                                .add_enabled(
                                    selected_game_ready && !install_blocked,
                                    egui::Button::new(icon_text_sized(
                                        Icon::PackagePlus,
                                        text.install(),
                                        13.0,
                                        13.0,
                                    )),
                                )
                                .clicked()
                            {
                                self.queue_install_for_browse_mod(mod_id, false);
                                ui.close();
                            }
                            if ui
                                .add_enabled(
                                    selected_game_ready && !install_blocked,
                                    egui::Button::new(icon_text_sized(
                                        Icon::PackagePlus,
                                        text.install_disabled(),
                                        13.0,
                                        13.0,
                                    )),
                                )
                                .clicked()
                            {
                                self.queue_install_for_browse_mod(mod_id, true);
                                ui.close();
                            }
                            if !selected_game_ready {
                                static_label(
                                    ui,
                                    RichText::new(text.game_not_installed())
                                        .size(12.0)
                                        .color(Color32::from_gray(170)),
                                );
                            } else if install_blocked {
                                static_label(
                                    ui,
                                    RichText::new(&install_disabled_reason)
                                        .size(12.0)
                                        .color(Color32::from_gray(170)),
                                );
                            }
                        });
                        ui.add_space(-2.0);
                        if ui.add(
                            egui::Button::new(icon_text_sized(Icon::Globe, text.open_in_browser(), 13.0, 13.0))
                                    .corner_radius(egui::CornerRadius::same(6)),
                            ).on_hover_cursor(egui::CursorIcon::PointingHand).clicked()
                        {
                            if let Err(err) = open_external_url(&gamebanana::browser_url(mod_id)) {
                                self.report_error(err, Some(text.could_not_open_browser()));
                            }
                        }
                        
                        // Translation button
                        ui.add_space(-2.0);
                        if let Some(detail) = self.browse_state.details.get(&mod_id) {
                            let translation_loading = detail.translation_loading;
                            let translation_active = detail.translation_lang.is_some();
                            let pulse = if translation_loading {
                                ui.ctx()
                                    .request_repaint_after(std::time::Duration::from_millis(80));
                                ((ui.input(|i| i.time) * 4.0).sin() as f32 * 0.5 + 0.5)
                                    .clamp(0.0, 1.0)
                            } else {
                                0.0
                            };
                            
                            let icon_color = if translation_loading {
                                Color32::from_rgb(
                                    245,
                                    (142.0 + 64.0 * pulse) as u8,
                                    (11.0 + 28.0 * pulse) as u8,
                                )
                            } else if translation_active {
                                Color32::from_rgb(34, 197, 94)
                            } else {
                                Color32::from_gray(160)
                            };
                            let icon_size = if translation_loading {
                                13.0 + 1.5 * pulse
                            } else {
                                13.0
                            };
                            
                            let translate_btn = ui.add(
                                egui::Button::new(icon_rich(Icon::Languages, icon_size, icon_color))
                                    .frame(false),
                            );

                            let translate_btn = if translation_loading {
                                translate_btn.on_hover_text(text.translation_in_progress())
                            } else {
                                translate_btn.on_hover_text(text.translate_shortcut())
                            };
                            let translate_btn =
                                translate_btn.on_hover_cursor(egui::CursorIcon::PointingHand);
                            if translate_btn.clicked() {
                                self.toggle_browse_translation(mod_id);
                            }
                            translate_btn.context_menu(|ui| {
                                if ui
                                    .add_enabled(
                                        !translation_loading,
                                        egui::Button::new(text.retranslate()),
                                    )
                                    .clicked()
                                {
                                    self.retranslate_browse_translation(mod_id);
                                    ui.close();
                                }
                            });
                        }
                        
                        ui.allocate_ui_with_layout(
                            ui.available_size(),
                            egui::Layout::right_to_left(egui::Align::Center),
                            |ui| {
                                ui.with_layout(egui::Layout::top_down(egui::Align::Max), |ui| {
                                    ui.horizontal(|ui| {
                                        let downloads_response = ui.add(egui::Label::new(RichText::new(format_compact_count(profile.download_count)).size(11.5).color(Color32::from_gray(178))).selectable(false)).on_hover_cursor(egui::CursorIcon::Default);
                                        if profile.download_count >= 1_000 {
                                            downloads_response.on_hover_text(format_count_with_separators(profile.download_count));
                                        }
                                        ui.add_space(-5.0);
                                        ui.add(egui::Label::new(icon_rich(Icon::Download, 12.0, Color32::from_rgb(131, 214, 247))).selectable(false)).on_hover_cursor(egui::CursorIcon::Default);
                                        ui.add_space(4.0);
                                        ui.add(egui::Label::new(RichText::new(relative_time_label_at(updated_at, age_now, false, text)).size(11.5).color(Color32::from_gray(145))).selectable(false)).on_hover_cursor(egui::CursorIcon::Default);
                                        ui.add_space(-5.0);
                                        ui.add(egui::Label::new(icon_rich(Icon::CalendarClock, 12.0, Color32::from_rgb(112, 164, 118))).selectable(false)).on_hover_cursor(egui::CursorIcon::Default);
                                        ui.add_space(4.0);
                                        let likes_response = ui.add(egui::Label::new(RichText::new(format_compact_count(like_count)).size(11.5).color(Color32::from_gray(178))).selectable(false)).on_hover_cursor(egui::CursorIcon::Default);
                                        if like_count >= 1_000 {
                                            likes_response.on_hover_text(format_count_with_separators(like_count));
                                        }
                                        ui.add_space(-5.0);
                                        ui.add(egui::Label::new(icon_rich(Icon::Heart, 12.0, Color32::from_rgb(214, 132, 154))).selectable(false)).on_hover_cursor(egui::CursorIcon::Default);
                                    });
                                    ui.add_space(-6.0);
                                    ui.add(
                                        egui::Label::new(
                                            RichText::new(author_name.clone())
                                                .size(11.0)
                                                .color(Color32::from_gray(168))
                                        ).truncate().selectable(false)
                                    ).on_hover_cursor(egui::CursorIcon::Default);
                                });
                            },
                        );
                    });

                    let scroll_id_salt = egui::Id::new("browse_detail_scroll");
                    let scroll_rect = ui.available_rect_before_wrap();
                    let scroll_navigation = vertical_scroll_navigation(ui, scroll_rect);
                    ScrollArea::vertical().id_salt(scroll_id_salt).show(ui, |ui| {
                        apply_vertical_scroll_navigation(ui, scroll_navigation, false);
                        if let Some(preview) = &profile.preview_media {
                            ui.add_space(10.0);
                            
                            ui.style_mut().spacing.scroll.floating = false;

                            let scroll_id = ui.make_persistent_id(format!("browse_preview_{}", mod_id));
                            let anim_id = scroll_id.with("anim");
                            let mut scroll_area = ScrollArea::horizontal()
                                .id_salt(scroll_id)
                                .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysVisible);
                            
                            // Handle smooth scroll animation
                            if let Some((start_time, start_val, target_val)) = ui.data(|d| d.get_temp::<(f64, f32, f32)>(anim_id)) {
                                let now = ui.input(|i| i.time);
                                let duration = 0.35; // 350ms animation
                                let t = ((now - start_time) / duration).clamp(0.0, 1.0) as f32;
                                
                                // Cubic ease-out: 1 - (1 - t)^3
                                let ease = 1.0 - (1.0 - t).powi(3);
                                let current_val = start_val + (target_val - start_val) * ease;
                                
                                scroll_area = scroll_area.horizontal_scroll_offset(current_val);
                                
                                if t < 1.0 {
                                    ui.ctx().request_repaint();
                                } else {
                                    ui.data_mut(|d| d.remove_temp::<(f64, f32, f32)>(anim_id));
                                }
                            } else if let Some(target_x) = ui.data_mut(|d| d.remove_temp::<f32>(scroll_id)) {
                                scroll_area = scroll_area.horizontal_scroll_offset(target_x);
                            }

                            let output = scroll_area.show(ui, |ui| {
                                let out = ui.horizontal(|ui| {
                                    let mut rects = Vec::with_capacity(preview.images.len());
                                    for (idx, image) in preview.images.iter().enumerate() {
                                        let full_url = gamebanana::full_image_url(image);
                                        let full_key = hash64_hex(full_url.as_bytes());
                                        
                                        let aspect_ratio = if let (Some(w), Some(h)) = (image.width_220, image.height_220) { if h > 0 { w as f32 / h as f32 } else { 1.777 } } else { 1.777 };
                                        let target_height = 220.0;
                                        let width = target_height * aspect_ratio;
                                        let (rect, response) = ui.allocate_exact_size(Vec2::new(width, target_height), Sense::click());

                                        let clip = ui.clip_rect();
                                        let is_visible = rect.intersects(clip);
                                        let distance_x = if is_visible { 0.0 } else if rect.center().x < clip.left() { clip.left() - rect.center().x } else { rect.center().x - clip.right() };
                                        let priority = if is_visible { 10 + (idx as u32 % 10) } else { 40 + (distance_x as u32 / 10) + (idx as u32 % 10) };
                                        let thumb_url = gamebanana::thumbnail_url(image);
                                        if let Some(url) = &thumb_url {
                                            self.queue_browse_image_with_profile(url.clone(), Some(self.browse_detail_generation), false, ThumbnailProfile::Rail, priority);
                                        } else {
                                            self.queue_browse_image_with_profile(full_url.clone(), Some(self.browse_detail_generation), false, ThumbnailProfile::Rail, priority);
                                        }

                                        let hq_key = Self::browse_thumb_texture_key(&full_url, ThumbnailProfile::Rail);
                                        let placeholder_key = thumb_url.as_ref().map(|url| Self::browse_thumb_texture_key(url, ThumbnailProfile::Rail));

                                        let texture = self.get_browse_thumb_texture(&hq_key, 2).cloned()
                                            .or_else(|| placeholder_key.as_ref().and_then(|key| self.get_browse_thumb_texture(key, 2).cloned()))
                                            .or_else(|| {
                                                if idx == 0 {
                                                    fallback_thumbnail_url.as_ref().and_then(|url| {
                                                        let card_key = Self::browse_thumb_texture_key(url, ThumbnailProfile::Card);
                                                        self.get_browse_thumb_texture(&card_key, 2).cloned()
                                                    })
                                                } else {
                                                    None
                                                }
                                            });

                                        if let Some(texture) = &texture {
                                            paint_thumbnail_image(
                                                ui,
                                                rect,
                                                texture,
                                                ThumbnailFit::Cover,
                                                Color32::WHITE,
                                                egui::CornerRadius::same(4),
                                            );
                                            if detail.unsafe_content && self.should_censor_unsafe() {
                                                paint_unsafe_overlay(
                                                    ui,
                                                    rect,
                                                    egui::CornerRadius::same(4),
                                                );
                                            }
                                        } else {
                                            ui.painter().rect_filled(rect, 4.0, Color32::from_white_alpha(12));
                                            ui.painter().text(
                                                rect.center(),
                                                egui::Align2::CENTER_CENTER,
                                                icon_char(Icon::Image),
                                                egui::FontId::new(
                                                    24.0,
                                                    FontFamily::Name(LUCIDE_FAMILY.into()),
                                                ),
                                                Color32::from_white_alpha(40),
                                            );
                                        }

                                        if response.clicked() {
                                            self.queue_overlay_full_texture(&full_key);
                                            self.browse_state.screenshot_overlay = Some(BrowseOverlayImage {
                                                texture_key: full_key,
                                                caption: image.caption.clone(),
                                            });
                                        }
                                        rects.push(rect);
                                    }
                                    rects
                                });
                                ui.add_space(-44.0);
                                out
                            });

                            // Overlay Navigation Buttons
                            let content_response = &output.inner.response;
                            let image_rects = &output.inner.inner;
                            let visible_rect = content_response.rect.intersect(ui.clip_rect());
                            
                            if ui.rect_contains_pointer(visible_rect) {
                                let current_offset = output.state.offset.x;
                                let content_width = output.content_size.x;
                                let view_width = visible_rect.width();
                                let max_offset = (content_width - view_width).max(0.0);

                                let button_size = Vec2::new(24.0, 64.0);
                                let button_y = visible_rect.center().y - button_size.y / 2.0;

                                // Left Button
                                if current_offset > 1.0 {
                                    let left_rect = egui::Rect::from_min_size(
                                        egui::pos2(visible_rect.min.x + 16.0, button_y),
                                        button_size
                                    );
                                    let response = ui.interact(left_rect, scroll_id.with("left"), Sense::click());
                                    let alpha = if response.hovered() { 240 } else { 102 };
                                    ui.painter().rect_filled(left_rect, 4.0, Color32::from_black_alpha(alpha));
                                    ui.painter().text(left_rect.center(), egui::Align2::CENTER_CENTER, icon_char(Icon::ChevronLeft), egui::FontId::new(20.0, FontFamily::Name(LUCIDE_FAMILY.into())), Color32::WHITE);
                                    
                                    if response.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                                        // Find previous image start position
                                        let target = image_rects.iter()
                                            .rev()
                                            .map(|r| r.min.x - visible_rect.min.x + current_offset)
                                            .find(|&off| off < current_offset - 5.0)
                                            .unwrap_or(0.0)
                                            .max(0.0);
                                        
                                        if target.is_finite() {
                                            let time = ui.input(|i| i.time);
                                            ui.data_mut(|d| d.insert_temp(anim_id, (time, current_offset, target)));
                                            ui.ctx().request_repaint();
                                        }
                                    }
                                }

                                // Right Button
                                if current_offset < max_offset - 1.0 {
                                    let right_rect = egui::Rect::from_min_size(
                                        egui::pos2(visible_rect.max.x - button_size.x - 16.0, button_y),
                                        button_size
                                    );
                                    let response = ui.interact(right_rect, scroll_id.with("right"), Sense::click());
                                    let alpha = if response.hovered() { 240 } else { 102 };
                                    ui.painter().rect_filled(right_rect, 4.0, Color32::from_black_alpha(alpha));
                                    ui.painter().text(right_rect.center(), egui::Align2::CENTER_CENTER, icon_char(Icon::ChevronRight), egui::FontId::new(20.0, FontFamily::Name(LUCIDE_FAMILY.into())), Color32::WHITE);

                                    if response.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                                        // Find next image start position
                                        let target = image_rects.iter()
                                            .map(|r| r.min.x - visible_rect.min.x + current_offset)
                                            .find(|&off| off > current_offset + 5.0)
                                            .unwrap_or(max_offset)
                                            .min(max_offset);

                                        if target.is_finite() {
                                            let time = ui.input(|i| i.time);
                                            ui.data_mut(|d| d.insert_temp(anim_id, (time, current_offset, target)));
                                            ui.ctx().request_repaint();
                                        }
                                    }
                                }
                            }
                        }
                        ui.add_space(8.0);

                        let youtube_url = profile.embedded_media.iter()
                            .find(|url| url.contains("youtube.com") || url.contains("youtu.be"));

                        if let Some(url) = youtube_url {
                            self.render_youtube_card(ui, url);
                            ui.add_space(8.0);
                        }

                        match &detail.updates {
                            BrowseUpdatesState::Loaded(entries) if !entries.is_empty() => {
                                egui::CollapsingHeader::new(
                                    bold(text.browse_updates(), Some(13.0)).underline().color(Color32::from_gray(220)),
                                )
                                .id_salt(("browse_updates_section", self.browse_detail_generation, mod_id))
                                .default_open(false)
                                .show(ui, |ui| {
                                    for (idx, entry) in entries.iter().enumerate() {
                                        if idx > 0 {
                                            ui.add_space(8.0);
                                            ui.separator();
                                            ui.add_space(8.0);
                                        }
                                        ui.horizontal_wrapped(|ui| {
                                            static_label(
                                                ui,
                                                RichText::new(&entry.name)
                                                    .strong()
                                                    .color(Color32::from_gray(228)),
                                            );
                                            if let Some(version) = &entry.version {
                                                static_label(
                                                    ui,
                                                    RichText::new(version)
                                                        .size(11.0)
                                                        .color(Color32::from_gray(160)),
                                                );
                                            }
                                            static_label(
                                                ui,
                                                RichText::new(format!("/ {}", relative_time_label_at(entry.updated_at, age_now, false, text)))
                                                    .size(11.0)
                                                    .color(Color32::from_gray(145)),
                                            );
                                        });
                                        if !entry.markdown.trim().is_empty() {
                                            self.queue_gif_previews_for_markdown(ui.ctx(), &entry.markdown, None);
                                            self.prewarm_markdown_images(&entry.markdown);
                                            self.render_markdown_with_inline_images(ui, &entry.markdown);
                                        }
                                    }
                                    ui.add_space(1.0);
                                });
                                ui.add_space(10.0);
                            }
                            BrowseUpdatesState::Failed(message) => {
                                egui::CollapsingHeader::new(
                                    RichText::new(text.browse_updates())
                                        .size(13.0)
                                        .strong()
                                        .color(Color32::from_gray(220)),
                                )
                                .id_salt(("browse_updates_section", self.browse_detail_generation, mod_id))
                                .default_open(false)
                                .show(ui, |ui| {
                                    static_label(
                                        ui,
                                        RichText::new(message)
                                            .size(12.0)
                                            .color(Color32::from_rgb(220, 120, 120)),
                                    );
                                });
                                ui.add_space(10.0);
                            }
                            _ => {}
                        }

                        if is_private || is_trashed_by_owner || is_withheld || is_deleted {
                            ui.add_space(12.0);
                            ui.group(|ui| {
                                ui.set_max_width(ui.available_width());
                                if is_private {
                                    ui.horizontal(|ui| {
                                        static_label(ui, icon_rich(Icon::Lock, 16.0, Color32::from_rgb(200, 100, 100)));
                                        static_label(
                                            ui,
                                            RichText::new(text.browse_private_mod())
                                                .strong()
                                                .color(Color32::from_rgb(220, 120, 120))
                                        );
                                    });
                                    static_label(ui, text.browse_automatic_install_disabled_authorized());
                                } else if is_withheld {
                                    ui.add_space(6.0);
                                    ui.horizontal(|ui| {
                                        static_label(
                                            ui,
                                            icon_rich(
                                                Icon::ShieldAlert,
                                                16.0,
                                                Color32::from_rgb(200, 100, 100),
                                            ),
                                        );
                                        ui.add_space(-4.0);
                                        ui.vertical(|ui| {
                                            ui.set_max_width(ui.available_width());
                                            static_label(
                                                ui,
                                                RichText::new(text.browse_withheld_mod())
                                                    .strong()
                                                    .color(Color32::from_rgb(220, 120, 120)),
                                            );
                                            if let Some(withholder) = withhold_notice
                                                .as_ref()
                                                .and_then(|n| n.withholder.as_ref())
                                            {
                                                static_label(
                                                    ui,
                                                    RichText::new(text.browse_withheld_by())
                                                        .size(12.0)
                                                        .color(Color32::from_gray(192)),
                                                );
                                                let response = ui
                                                    .add(
                                                        egui::Label::new(
                                                            RichText::new(&withholder.name)
                                                                .size(12.0)
                                                                .underline()
                                                                .color(Color32::from_rgb(
                                                                    220, 120, 120,
                                                                )),
                                                        )
                                                        .wrap()
                                                        .sense(Sense::click()),
                                                    )
                                                    .on_hover_cursor(
                                                        egui::CursorIcon::PointingHand,
                                                    );
                                                if response.clicked() {
                                                    if let Err(err) =
                                                        open_external_url(&withholder.profile_url)
                                                    {
                                                        self.report_error(
                                                            err,
                                                            Some(text.could_not_open_browser()),
                                                        );
                                                    }
                                                }
                                            }
                                        });
                                    });
                                    ui.add(
                                        egui::Label::new(
                                            RichText::new(
                                                text.browse_automatic_install_disabled_withheld(),
                                            ),
                                        )
                                        .wrap()
                                        .selectable(false),
                                    )
                                    .on_hover_cursor(egui::CursorIcon::Default);
                                    if let Some(notice) = withhold_notice.as_ref() {
                                        if let Some(notes) = notice.notes.as_deref() {
                                            let notes = gamebanana::sanitize_inline(notes);
                                            if !notes.trim().is_empty() {
                                                ui.add_space(4.0);
                                                ui.add(
                                                    egui::Label::new(
                                                        RichText::new(notes)
                                                            .size(12.0)
                                                            .color(Color32::from_gray(192)),
                                                    )
                                                    .wrap()
                                                    .selectable(false),
                                                )
                                                .on_hover_cursor(egui::CursorIcon::Default);
                                            }
                                        }
                                        if !notice.rules_violated.is_empty() {
                                            ui.add_space(6.0);
                                            for rule in &notice.rules_violated {
                                                let label = rule
                                                    .code
                                                    .as_deref()
                                                    .or(rule.name.as_deref())
                                                    .unwrap_or(text.browse_rule_violation());
                                                ui.add(
                                                    egui::Label::new(
                                                        RichText::new(format!("• {label}"))
                                                            .size(11.5)
                                                            .color(Color32::from_gray(176)),
                                                    )
                                                    .wrap()
                                                    .selectable(false),
                                                )
                                                .on_hover_cursor(egui::CursorIcon::Default);
                                            }
                                        }
                                    }
                                } else if is_deleted {
                                    ui.add_space(6.0);
                                    ui.horizontal(|ui| {
                                        static_label(
                                            ui,
                                            icon_rich(
                                                Icon::CircleX,
                                                16.0,
                                                Color32::from_rgb(200, 100, 100),
                                            ),
                                        );
                                        ui.add_space(-4.0);
                                        static_label(
                                            ui,
                                            RichText::new(text.browse_deleted_mod_no_longer_exists())
                                                .strong()
                                                .color(Color32::from_rgb(220, 120, 120)),
                                        );
                                    });
                                } else {
                                    ui.add_space(6.0);
                                    ui.allocate_ui_with_layout(
                                        egui::vec2(ui.available_width(), 34.0),
                                        egui::Layout::left_to_right(egui::Align::Center),
                                        |ui| {
                                            ui.with_layout(
                                                egui::Layout::centered_and_justified(egui::Direction::LeftToRight),
                                                |ui| {
                                                    ui.horizontal(|ui| {
                                                        ui.add_space(12.0);
                                                        static_label(ui, icon_rich(Icon::Trash2, 16.0, Color32::from_rgb(200, 100, 100)));
                                                        ui.add_space(-4.0);
                                                        if let Some(trasher) = trashed_by_owner.as_ref() {
                                                            static_label(
                                                                ui,
                                                                RichText::new(text.browse_deleted_by())
                                                                    .strong()
                                                                    .color(Color32::from_rgb(220, 120, 120))
                                                            );
                                                            ui.add_space(-6.0);
                                                            let response = ui.add(
                                                                egui::Label::new(
                                                                    RichText::new(&trasher.name)
                                                                        .strong()
                                                                        .underline()
                                                                        .color(Color32::from_rgb(220, 120, 120))
                                                                )
                                                                .sense(Sense::click())
                                                            ).on_hover_cursor(egui::CursorIcon::PointingHand);
                                                            if response.clicked() {
                                                                if let Err(err) = open_external_url(&trasher.profile_url) {
                                                                    self.report_error(err, Some(text.could_not_open_browser()));
                                                                }
                                                            }
                                                        } else {
                                                            static_label(
                                                                ui,
                                                                RichText::new(text.browse_deleted())
                                                                    .strong()
                                                                    .color(Color32::from_rgb(220, 120, 120))
                                                            );
                                                        }
                                                        ui.add_space(6.0);
                                                    });
                                                }
                                            );
                                        }
                                    );
                                    ui.add_space(6.0);
                                }
                            });
                        }

                        self.queue_gif_previews_for_markdown(ui.ctx(), &detail.markdown, None);
                        let markdown = rewrite_markdown_gif_images(&detail.markdown, None);
                        self.prewarm_markdown_images(&markdown);
                        self.render_markdown_with_inline_images(ui, &markdown);

                        let render_file_section_label = |ui: &mut Ui, label: &str, count: usize| {
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
                            let label_text = format!("{label} ({count})");
                            let galley = ui.painter().layout_no_wrap(label_text, egui::FontId::proportional(12.0), Color32::from_gray(200));
                            let text_rect = egui::Rect::from_center_size(rect.center(), galley.size());
                            ui.painter().rect_filled(text_rect.expand(6.0), 6.0, Color32::from_rgba_premultiplied(28, 30, 34, 230));
                            ui.painter().galley(text_rect.min, galley, Color32::WHITE);
                        };

                        if !install_blocked && !profile.files.is_empty() {
                            ui.add_space(12.0);
                            render_file_section_label(ui, text.browse_files(), profile.files.len());
                            for file in &profile.files {
                                self.render_browse_file_row(ui, &browse_game_id, mod_id, file);
                            }
                        }
                        if !install_blocked && !profile.archived_files.is_empty() {
                            ui.add_space(12.0);
                            render_file_section_label(ui, text.browse_archived_files(), profile.archived_files.len());
                            for file in &profile.archived_files {
                                self.render_browse_file_row(ui, &browse_game_id, mod_id, file);
                            }
                        }
                        apply_vertical_scroll_navigation(ui, scroll_navigation, true);
                    });
                } else {
                    ui.add_space(18.0);
                    ui.centered_and_justified(|ui| {
                    static_label(
                        ui,
                        RichText::new(text.browse_loading_details())
                            .size(15.0)
                            .color(Color32::from_gray(175)),
                    );
                    });
                }
            });

        self.browse_detail_open = browse_detail_open;
        if !self.browse_detail_open {
            self.browse_state.selected_mod_id = None;
        }
        if self.browse_detail_focus_requested {
            if let Some(inner) = response {
                ctx.move_to_top(inner.response.layer_id);
            }
            self.browse_detail_focus_requested = false;
        }
        self.render_browse_screenshot_overlay(ctx);
    }

    fn render_browse_file_row(
        &mut self,
        ui: &mut Ui,
        game_id: &str,
        mod_id: u64,
        file: &gamebanana::ModFile,
    ) {
        let text = self.text();
        egui::Frame::group(ui.style())
            .inner_margin(egui::Margin::same(10))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.vertical(|ui| {
                        static_label(ui, bold(&file.file_name, None));
                        ui.add_space(-4.0);
                        static_label(
                            ui,
                            RichText::new(text.browse_file_metadata(
                                format_file_size(file.file_size),
                                format_exact_local_timestamp(file.date_added),
                                file.download_count,
                            ))
                            .size(11.5)
                            .color(Color32::from_gray(155)),
                        );
                        if let Some(description) = &file.description {
                            static_label(
                                ui,
                                RichText::new(gamebanana::sanitize_inline(description))
                                    .size(12.0)
                                    .color(Color32::from_gray(186)),
                            );
                        }
                    });
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
                        let selected_game_ready = self.game_is_installed_or_configured(&game_id);
                        let install_response = ui.add_enabled(
                            selected_game_ready && file.download_url.is_some(),
                            egui::Button::new(text.install()),
                        );
                        if !selected_game_ready {
                            install_response
                                .clone()
                                .on_hover_text(text.game_not_installed())
                                .on_hover_cursor(egui::CursorIcon::NotAllowed);
                        } else if file.download_url.is_some() {
                            install_response
                                .clone()
                                .on_hover_cursor(egui::CursorIcon::PointingHand);
                        }
                        if install_response.clicked() {
                            self.queue_browse_download(
                                game_id.to_string(),
                                mod_id,
                                file.clone(),
                                vec![file.clone()],
                                None,
                                self.browse_state
                                    .details
                                    .get(&mod_id)
                                    .map(|detail| detail.unsafe_content)
                                    .unwrap_or(false),
                                None,
                                None,
                                false,
                                None,
                            );
                        }
                    });
                });
            });
    }

    fn render_browse_file_prompt(&mut self, ctx: &egui::Context, constrain_rect: egui::Rect) {
        let text = self.text();
        let Some(prompt_game_id) = self
            .browse_state
            .file_prompt
            .as_ref()
            .map(|prompt| prompt.game_id.clone())
        else {
            return;
        };
        let prompt_game_ready = self.game_is_installed_or_configured(&prompt_game_id);
        let Some(prompt) = self.browse_state.file_prompt.as_mut() else {
            return;
        };
        let mod_name = self
            .browse_state
            .cards
            .iter()
            .find(|c| c.id == prompt.mod_id)
            .map(|c| c.name.clone())
            .or_else(|| {
                self.browse_state
                    .details
                    .get(&prompt.mod_id)
                    .map(|d| d.profile.name.clone())
            })
            .unwrap_or_else(|| format!("Mod {}", prompt.mod_id));

        let mut open = true;
        let mut should_cancel = false;
        let mut should_confirm = false;
        egui::Window::new(icon_text_sized(
            Icon::Files,
            text.browse_choose_files(),
            14.0,
            14.0,
        ))
        .id(egui::Id::new(BROWSE_FILE_PICKER_WINDOW_ID))
        .default_pos(constrain_rect.min + egui::vec2(16.0, 16.0))
        .default_size(egui::vec2(420.0, 420.0))
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
            ui.horizontal(|ui| {
                static_label(
                    ui,
                    icon_rich(Icon::Info, 96.0, Color32::from_rgb(148, 192, 232)),
                );
                ui.vertical(|ui| {
                    static_label(ui, bold(&mod_name, Some(16.0)).underline());
                    ui.add_space(4.0);
                    static_label(
                        ui,
                        RichText::new(text.browse_multiple_files_prompt()).size(14.0),
                    );
                });
            });
            ui.add_space(-8.0);
            ui.vertical_centered(|ui| {
                egui::Frame::new()
                    .fill(Color32::from_rgba_unmultiplied(180, 180, 180, 48))
                    .corner_radius(12.0)
                    .inner_margin(egui::Margin::same(16))
                    .outer_margin(egui::Margin {
                        left: 0,
                        right: 0,
                        top: 0,
                        bottom: 4,
                    })
                    .show(ui, |ui| {
                        ui.set_width(388.0);
                        ScrollArea::vertical().max_height(300.0).show(ui, |ui| {
                            let mut style = (**ui.style()).clone();
                            style.visuals.widgets.active.bg_fill =
                                egui::Color32::from_rgba_unmultiplied(128, 128, 128, 128);
                            style.visuals.widgets.inactive.bg_fill =
                                egui::Color32::from_rgba_unmultiplied(128, 128, 128, 128);
                            style.visuals.widgets.hovered.bg_fill =
                                egui::Color32::from_rgba_unmultiplied(128, 128, 128, 128);
                            style.visuals.widgets.active.corner_radius =
                                egui::CornerRadius::same(2);
                            style.visuals.widgets.inactive.corner_radius =
                                egui::CornerRadius::same(2);
                            style.visuals.widgets.hovered.corner_radius =
                                egui::CornerRadius::same(2);
                            style.spacing.item_spacing.y = 8.0;
                            style.spacing.icon_spacing = 4.0;
                            ui.set_style(style);
                            ui.spacing_mut().item_spacing.y = 6.0;
                            for file_entry in &mut prompt.files {
                                let row_response = egui::Frame::group(ui.style())
                                    .inner_margin(egui::Margin::symmetric(10, 8))
                                    .show(ui, |ui| {
                                        ui.horizontal(|ui| {
                                            larger_checkbox(ui, file_entry.selected);
                                            ui.vertical(|ui| {
                                                static_label(
                                                    ui,
                                                    bold(&file_entry.file.file_name, None),
                                                );
                                                ui.add_space(-4.0);
                                                static_label(
                                                    ui,
                                                    RichText::new(text.browse_file_metadata(
                                                        format_file_size(file_entry.file.file_size),
                                                        format_exact_local_timestamp(
                                                            file_entry.file.date_added,
                                                        ),
                                                        file_entry.file.download_count,
                                                    ))
                                                    .size(11.5)
                                                    .color(Color32::from_gray(155)),
                                                );
                                                if let Some(description) =
                                                    &file_entry.file.description
                                                {
                                                    if !description.trim().is_empty() {
                                                        static_label(
                                                            ui,
                                                            RichText::new(
                                                                gamebanana::sanitize_inline(
                                                                    description,
                                                                ),
                                                            )
                                                            .size(12.0)
                                                            .color(Color32::from_gray(186)),
                                                        );
                                                    }
                                                }
                                            });
                                        });
                                    })
                                    .response;
                                if row_response
                                    .interact(Sense::click())
                                    .on_hover_cursor(egui::CursorIcon::PointingHand)
                                    .clicked()
                                {
                                    file_entry.selected = !file_entry.selected;
                                }
                            }
                        });
                    });
                ui.horizontal(|ui| {
                    let install_response = ui.add_enabled(
                        prompt_game_ready,
                        egui::Button::new(text.install()).fill(Color32::from_rgb(180, 78, 35)),
                    );
                    if !prompt_game_ready {
                        install_response
                            .clone()
                            .on_hover_text(text.game_not_installed())
                            .on_hover_cursor(egui::CursorIcon::NotAllowed);
                    }
                    if install_response.clicked() {
                        should_confirm = true;
                    }
                    if ui.button(text.cancel()).clicked() {
                        should_cancel = true;
                    }
                });
            });
        });
        if should_confirm {
            self.confirm_browse_file_prompt();
        } else if should_cancel || !open {
            self.browse_state.file_prompt = None;
        }
    }

    fn render_browse_screenshot_overlay(&mut self, ctx: &egui::Context) {
        let Some(overlay) = self.browse_state.screenshot_overlay.as_ref() else {
            return;
        };
        let current_key = overlay.texture_key.clone();
        let current_overlay_caption = overlay.caption.clone();

        let capacity = if self.current_view == ViewMode::Browse {
            self.browse_state
                .selected_mod_id
                .and_then(|mod_id| self.browse_state.details.get(&mod_id))
                .and_then(|detail| {
                    let profile = detail
                        .translated_profile
                        .as_ref()
                        .unwrap_or(&detail.profile);
                    profile.preview_media.as_ref().map(|p| p.images.len())
                })
                .unwrap_or(0)
        } else {
            self.my_mod_overlay_images.len()
        };
        let mut images: Vec<(String, Option<String>)> = Vec::with_capacity(capacity);
        if self.current_view == ViewMode::Browse {
            if let Some(mod_id) = self.browse_state.selected_mod_id {
                if let Some(detail) = self.browse_state.details.get(&mod_id) {
                    let profile = detail
                        .translated_profile
                        .as_ref()
                        .unwrap_or(&detail.profile);
                    if let Some(preview) = &profile.preview_media {
                        for image in &preview.images {
                            let full_url = gamebanana::full_image_url(image);
                            let key = hash64_hex(full_url.as_bytes());
                            images.push((key, image.caption.clone()));
                        }
                    }
                }
            }
        } else {
            for item in &self.my_mod_overlay_images {
                images.push((item.texture_key.clone(), item.caption.clone()));
            }
        }

        let current_index = images.iter().position(|(k, _)| *k == current_key);
        let total_images = images.len();

        enum NavAction {
            Close,
            Next,
            Prev,
        }
        let mut action = None;

        let screen_rect = ctx.viewport_rect();
        egui::Area::new(egui::Id::new("browse_screenshot_overlay_area"))
            .order(egui::Order::Foreground)
            .fixed_pos(screen_rect.min)
            .interactable(true)
            .show(ctx, |ui| {
                ui.set_min_size(screen_rect.size());
                let rect = ui.available_rect_before_wrap();

                let bg_response = ui.allocate_rect(rect, Sense::click());
                ui.painter()
                    .rect_filled(rect, 0.0, Color32::from_rgba_premultiplied(0, 0, 0, 240));

                if bg_response.clicked() {
                    action = Some(NavAction::Close);
                }

                ui.input(|i| {
                    if i.key_pressed(egui::Key::Escape)
                        || i.key_pressed(egui::Key::Space)
                        || i.key_pressed(egui::Key::Enter)
                    {
                        action = Some(NavAction::Close);
                    }
                    if i.key_pressed(egui::Key::A)
                        || i.key_pressed(egui::Key::W)
                        || i.key_pressed(egui::Key::ArrowLeft)
                        || i.key_pressed(egui::Key::ArrowUp)
                    {
                        action = Some(NavAction::Prev);
                    }
                    if i.key_pressed(egui::Key::S)
                        || i.key_pressed(egui::Key::D)
                        || i.key_pressed(egui::Key::ArrowRight)
                        || i.key_pressed(egui::Key::ArrowDown)
                    {
                        action = Some(NavAction::Next);
                    }
                    if i.raw_scroll_delta.y > 0.0 {
                        action = Some(NavAction::Prev);
                    } else if i.raw_scroll_delta.y < 0.0 {
                        action = Some(NavAction::Next);
                    }
                });

                self.queue_overlay_full_texture(&current_key);
                let texture = if let Some(texture) = self.get_browse_full_texture(&current_key, 3) {
                    Some(texture)
                } else if let Some(texture) = self.get_mod_full_texture(&current_key, 3) {
                    Some(texture)
                } else {
                    None
                };

                if let Some(texture) = texture {
                    let size = texture.size_vec2();
                    let scale = (rect.width() / size.x).min(rect.height() / size.y);
                    let target_size = size * scale;
                    let image_rect = egui::Rect::from_center_size(rect.center(), target_size);

                    ui.painter().image(
                        texture.id(),
                        image_rect,
                        egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                        Color32::WHITE,
                    );

                    let caption = current_index
                        .and_then(|i| images[i].1.as_ref())
                        .or(current_overlay_caption.as_ref());
                    if let Some(caption) = caption {
                        if !caption.trim().is_empty() {
                            let galley = ui.painter().layout(
                                caption.to_string(),
                                egui::FontId::proportional(18.0),
                                Color32::WHITE,
                                rect.width() - 40.0,
                            );

                            let caption_bg_height = galley.size().y + 24.0;
                            let caption_rect = egui::Rect::from_min_max(
                                egui::pos2(rect.min.x, rect.max.y - caption_bg_height),
                                rect.max,
                            );

                            ui.painter().rect_filled(
                                caption_rect,
                                0.0,
                                Color32::from_black_alpha(64),
                            );
                            ui.painter().galley(
                                caption_rect.center() - galley.size() / 2.0,
                                galley,
                                Color32::WHITE,
                            );
                        }
                    }
                } else {
                    ui.put(rect, egui::Spinner::new().size(48.0));
                }

                if total_images > 1 {
                    let button_size = Vec2::new(64.0, 128.0);
                    let center_y = rect.center().y - (button_size.y / 2.0);

                    if current_index.is_some_and(|i| i > 0) {
                        let left_rect = egui::Rect::from_min_size(
                            egui::pos2(rect.min.x + 32.0, center_y),
                            button_size,
                        );
                        let resp = ui
                            .interact(left_rect, ui.id().with("nav_l"), Sense::click())
                            .on_hover_cursor(egui::CursorIcon::PointingHand);
                        let alpha = if resp.hovered() { 240 } else { 102 };
                        ui.painter()
                            .rect_filled(left_rect, 12.0, Color32::from_black_alpha(alpha));
                        ui.painter().text(
                            left_rect.center(),
                            egui::Align2::CENTER_CENTER,
                            icon_char(Icon::ChevronLeft),
                            egui::FontId::new(48.0, FontFamily::Name(LUCIDE_FAMILY.into())),
                            Color32::WHITE,
                        );
                        if resp.clicked() {
                            action = Some(NavAction::Prev);
                        }
                    }

                    if current_index.is_some_and(|i| i < total_images - 1) {
                        let right_rect = egui::Rect::from_min_size(
                            egui::pos2(rect.max.x - 32.0 - button_size.x, center_y),
                            button_size,
                        );
                        let resp = ui
                            .interact(right_rect, ui.id().with("nav_r"), Sense::click())
                            .on_hover_cursor(egui::CursorIcon::PointingHand);
                        let alpha = if resp.hovered() { 240 } else { 102 };
                        ui.painter().rect_filled(
                            right_rect,
                            12.0,
                            Color32::from_black_alpha(alpha),
                        );
                        ui.painter().text(
                            right_rect.center(),
                            egui::Align2::CENTER_CENTER,
                            icon_char(Icon::ChevronRight),
                            egui::FontId::new(48.0, FontFamily::Name(LUCIDE_FAMILY.into())),
                            Color32::WHITE,
                        );
                        if resp.clicked() {
                            action = Some(NavAction::Next);
                        }
                    }
                }
            });

        match action {
            Some(NavAction::Close) => {
                self.browse_state.screenshot_overlay = None;
                self.my_mod_overlay_images.clear();
            }
            Some(NavAction::Next) => {
                if let Some(i) = current_index {
                    if i + 1 < images.len() {
                        let next_key = images[i + 1].0.clone();
                        let next_caption = images[i + 1].1.clone();
                        self.queue_overlay_full_texture(&next_key);
                        if i + 2 < images.len() {
                            self.queue_overlay_full_texture(&images[i + 2].0);
                        }
                        self.browse_state.screenshot_overlay = Some(BrowseOverlayImage {
                            texture_key: next_key,
                            caption: next_caption,
                        });
                    }
                }
            }
            Some(NavAction::Prev) => {
                if let Some(i) = current_index {
                    if i > 0 {
                        let prev_key = images[i - 1].0.clone();
                        let prev_caption = images[i - 1].1.clone();
                        self.queue_overlay_full_texture(&prev_key);
                        if i > 1 {
                            self.queue_overlay_full_texture(&images[i - 2].0);
                        }
                        self.browse_state.screenshot_overlay = Some(BrowseOverlayImage {
                            texture_key: prev_key,
                            caption: prev_caption,
                        });
                    }
                }
            }
            None => {}
        }
    }
}
