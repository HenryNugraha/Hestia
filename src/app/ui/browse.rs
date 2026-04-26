impl HestiaApp {
    fn render_browse_left_pane(&mut self, ui: &mut Ui) {
        egui::Frame::new()
            .fill(Color32::from_rgba_premultiplied(36, 38, 42, 242))
            .corner_radius(egui::CornerRadius::same(0))
            .inner_margin(egui::Margin::same(18))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.set_height(41.0);
                    let is_empty = self.browse_query.trim().is_empty();
                    let expanded = self.browse_search_expanded;
                    let how_expanded = ui.ctx().animate_bool_with_time(ui.id().with("browse_search_anim"), expanded, 0.2);

                    ui.scope(|ui| {
                        let icon_size = 41.0;
                        let full_width = 320.0;
                        let current_width = icon_size + (full_width - icon_size) * how_expanded;

                        let (rect, _area_resp) = ui.allocate_exact_size(Vec2::new(current_width, 41.0), Sense::hover());
                        
                        if how_expanded > 0.0 {
                            let bg_alpha = (how_expanded * 255.0) as u8;
                            ui.painter().rect(
                                rect,
                                egui::CornerRadius::same(6),
                                Color32::from_rgba_premultiplied(44, 47, 52, bg_alpha),
                                egui::Stroke::new(1.0, Color32::from_rgba_premultiplied(69, 74, 81, bg_alpha)),
                                egui::StrokeKind::Inside,
                            );
                        }

                        let icon_pos = rect.left_center() + egui::vec2(20.5, 0.0);
                        let icon_area = egui::Rect::from_center_size(icon_pos, egui::Vec2::splat(28.0));
                        let icon_resp = ui.interact(icon_area, ui.id().with("browse_search_toggle"), Sense::click());

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
                            icon_resp.clone().on_hover_cursor(egui::CursorIcon::PointingHand);
                            if !expanded {
                                ui.painter().circle_filled(icon_pos, 14.0, Color32::from_white_alpha(15));
                            }
                        }

                        if how_expanded > 0.2 {
                            let right_padding = if !is_empty { 64.0 } else { 32.0 };
                            let input_rect = egui::Rect::from_min_max(
                                rect.min + egui::vec2(icon_size, 0.0),
                                rect.max - egui::vec2(right_padding, 0.0)
                            );
                            
                            let mut child_ui = ui.new_child(egui::UiBuilder::new().max_rect(input_rect));
                            let edit_resp = child_ui.add(
                                TextEdit::singleline(&mut self.browse_query)
                                    .id_source(BROWSE_SEARCH_INPUT_ID)
                                    .hint_text(if how_expanded > 0.8 { "Discover mods on GameBanana..." } else { "" })
                                    .frame(false)
                                    .desired_width(input_rect.width())
                            );
                            if self.browse_search_focus_pending {
                                edit_resp.request_focus();
                                self.browse_search_focus_pending = false;
                            }
                            if edit_resp.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter)) {
                                self.restart_browse_query();
                            }
                        }

                        if how_expanded > 0.9 {
                            let mut next_x = rect.right() - 16.0;
                            
                            let action_icon = if is_empty { Icon::RotateCw } else { Icon::CircleArrowRight };
                            let action_pos = egui::pos2(next_x, rect.center().y);
                            let action_area = egui::Rect::from_center_size(action_pos, egui::Vec2::splat(24.0));
                            let action_resp = ui.interact(action_area, ui.id().with("browse_search_submit"), Sense::click());
                            let action_color = if action_resp.hovered() { Color32::WHITE } else { Color32::from_gray(170) };
                            
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
                            action_resp.clone().on_hover_cursor(egui::CursorIcon::PointingHand);
                            
                            next_x -= 24.0;

                            if !is_empty {
                                let x_pos = egui::pos2(next_x, rect.center().y);
                                let x_area = egui::Rect::from_center_size(x_pos, egui::Vec2::splat(24.0));
                                let x_resp = ui.interact(x_area, ui.id().with("browse_search_clear"), Sense::click());
                                let x_color = if x_resp.hovered() { Color32::from_gray(225) } else { Color32::from_gray(120) };
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
                                x_resp.clone().on_hover_cursor(egui::CursorIcon::PointingHand);
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
                        label_resp.clone().on_hover_cursor(egui::CursorIcon::PointingHand);

                        let slide_left = 40.0 * (1.0 - header_visibility);
                        let text_pos = label_rect.left_center() - egui::vec2(slide_left, 0.0);

                        ui.painter().with_clip_rect(label_rect).text(
                            text_pos,
                            egui::Align2::LEFT_CENTER,
                            "GameBanana Mods",
                            egui::FontId::proportional(18.0),
                            Color32::from_rgba_premultiplied(228, 231, 235, (header_visibility * 255.0) as u8),
                        );
                    }

                    ui.add_space(-1.0);
                    let mut sort_changed = false;
                    ui.scope(|ui| {
                        ui.add_space(5.0);
                        ui.spacing_mut().icon_width = 7.5;
                        ui.spacing_mut().icon_spacing = 2.0;
                        
                        let visuals = ui.visuals_mut();
                        visuals.widgets.inactive.bg_fill = Color32::from_rgba_unmultiplied(96, 96, 96, 182);
                        visuals.widgets.hovered.bg_fill = Color32::from_gray(182);
                        visuals.widgets.active.bg_fill = Color32::from_gray(96);
                        visuals.widgets.inactive.bg_stroke.color = Color32::from_gray(160);
                        visuals.selection.bg_fill = Color32::BLACK;

                        ui.vertical(|ui| {
                            ui.spacing_mut().item_spacing.y = -2.0;
                            ui.add_space(2.0);
                            if is_empty {
                                sort_changed |= ui.radio_value(&mut self.state.browse_sort, BrowseSort::Popular, RichText::new("Popular").size(11.0)).changed();
                                sort_changed |= ui.radio_value(&mut self.state.browse_sort, BrowseSort::RecentUpdated, RichText::new("Recent Updated").size(11.0)).changed();
                            } else {
                                sort_changed |= ui.radio_value(&mut self.state.search_sort, SearchSort::BestMatch, RichText::new("Best Match").size(11.0)).changed();
                                sort_changed |= ui.radio_value(&mut self.state.search_sort, SearchSort::RecentUpdated, RichText::new("Recent Updated").size(11.0)).changed();
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
                                    .map(|count| format!("{count} mods"))
                                    .unwrap_or_else(|| "Loading…".to_string());
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
                                        RichText::new(format!("{hidden_count} hidden for NSFW"))
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
        ScrollArea::vertical().show(ui, |ui| {
            ui.spacing_mut().item_spacing.x = 8.0;
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
                        RichText::new("Fetching mods from GameBanana…")
                            .size(16.0)
                            .color(Color32::from_gray(180)),
                    );
                });
            }

            for row in cards.chunks(columns) {
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
                                            self.mod_thumbnail_placeholder.as_ref(),
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
                                                            RichText::new(if is_installed { "Installed" } else { "Install" })
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
                                                            .on_hover_text("Game is not installed or configured.")
                                                            .on_hover_cursor(egui::CursorIcon::NotAllowed);
                                                    } else if card.has_files {
                                                        install_response
                                                            .clone()
                                                            .on_hover_cursor(egui::CursorIcon::PointingHand);
                                                    }
                                                    if install_response.clicked() {
                                                        self.queue_install_for_browse_mod(card.id);
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
                                                                    RichText::new(card.author_name.clone())
                                                                        .size(11.0)
                                                                        .color(Color32::from_gray(168))
                                                                )
                                                                .truncate()
                                                                .selectable(false)
                                                            ).on_hover_cursor(egui::CursorIcon::Default);
                                                            ui.add_space(-6.0);
                                                            ui.horizontal(|ui| {
                                                                ui.add(egui::Label::new(RichText::new(relative_time_label(card.updated_at, false)).size(11.5).color(Color32::from_gray(145))).selectable(false)).on_hover_cursor(egui::CursorIcon::Default);
                                                                ui.add_space(-5.0);
                                                                ui.add(egui::Label::new(icon_rich(Icon::CalendarClock, 12.0, Color32::from_rgb(112, 164, 118))).selectable(false)).on_hover_cursor(egui::CursorIcon::Default);
                                                                ui.add_space(8.0);
                                                                ui.add(egui::Label::new(RichText::new(card.like_count.to_string()).size(11.5).color(Color32::from_gray(178))).selectable(false)).on_hover_cursor(egui::CursorIcon::Default);
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
                                    RichText::new("Loading…")
                                        .size(12.0)
                                        .color(Color32::from_gray(170)),
                                );
                                return;
                            }

                            let detail = detail.expect("checked above");
                            let install_disabled_reason =
                                gamebanana::install_block_reason(&detail.profile)
                                    .unwrap_or_default();
                            let install_blocked = !install_disabled_reason.is_empty();
                            let selected_game_ready = self.selected_game_is_installed_or_configured();

                            let install_response = ui.add_enabled(
                                !install_blocked && selected_game_ready,
                                egui::Button::new(icon_text_sized(
                                    Icon::PackagePlus,
                                    "Install",
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
                                    .on_hover_text_at_pointer("Game is not installed or configured.")
                                    .on_hover_cursor(egui::CursorIcon::NotAllowed);
                            } else {
                                install_response.clone().on_hover_text_at_pointer(install_disabled_reason);
                            }
                            if install_response.clicked() {
                                self.queue_install_for_browse_mod(card.id);
                                ui.close();
                            }

                            let browser_response = ui.button(icon_text_sized(
                                Icon::Globe,
                                "Open in Browser",
                                13.0,
                                13.0,
                            )).on_hover_cursor(egui::CursorIcon::PointingHand);
                            if browser_response.clicked() {
                                if let Err(err) = open_external_url(&gamebanana::browser_url(card.id)) {
                                    self.report_error(err, Some("Could not open browser"));
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
                        RichText::new("Loading more…")
                            .size(12.5)
                            .color(Color32::from_gray(165)),
                    );
                });
            }

            let sentinel = ui.allocate_response(Vec2::new(ui.available_width(), 24.0), Sense::hover());
            if sentinel.rect.intersects(ui.clip_rect())
                && !self.browse_state.loading_page
                && self.browse_state.has_more
            {
                self.request_browse_page(self.browse_state.next_page);
            }
        });
    }

    fn render_browse_detail_window(&mut self, ctx: &egui::Context, pane_rect: egui::Rect) {
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
        let response = egui::Window::new("Mod Detail") // BROWSE view's mod detail GUI
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
                    let title = card
                        .as_ref()
                        .map(|card| card.name.clone())
                        .or_else(|| {
                            self.browse_state
                                .details
                                .get(&mod_id)
                                .map(|detail| detail.profile.name.clone())
                        })
                        .filter(|name| !name.trim().is_empty())
                        .unwrap_or_else(|| format!("Mod {mod_id}"));
                    ui.heading(title);
                    ui.label(
                        RichText::new(format!("ID: {}", mod_id))
                            .size(12.0)
                            .color(Color32::from_gray(158)),
                    );
                });
                if let Some(detail) = self.browse_state.details.get(&mod_id).cloned() {
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
                            detail
                                .profile
                                .submitter
                                .as_ref()
                                .map(|submitter| submitter.name.clone())
                        })
                        .unwrap_or_else(|| "Unknown".to_string());
                    let like_count = card
                        .as_ref()
                        .map(|card| card.like_count)
                        .unwrap_or(detail.profile.like_count);
                    let updated_at = card
                        .as_ref()
                        .map(|card| card.updated_at)
                        .unwrap_or_else(|| {
                            timestamp_to_utc(
                                detail
                                    .profile
                                    .date_updated
                                    .unwrap_or(detail.profile.date_modified),
                            )
                        });
                    let fallback_thumbnail_url = card
                        .as_ref()
                        .and_then(|card| card.thumbnail_url.clone())
                        .or_else(|| {
                            detail
                                .profile
                                .preview_media
                                .as_ref()
                                .and_then(|preview| preview.images.first())
                                .and_then(gamebanana::thumbnail_url)
                        });
                    let trashed_by_owner = gamebanana::trashed_by_owner(&detail.profile).cloned();
                    let withhold_notice = gamebanana::withheld_notice(&detail.profile).cloned();
                    let is_trashed_by_owner = trashed_by_owner.is_some();
                    let is_private = detail.profile.is_private;
                    let is_withheld = withhold_notice.is_some();
                    let is_deleted = detail.profile.is_deleted || detail.profile.id == 0;
                    let is_installed = card
                        .as_ref()
                        .is_some_and(|card| self.is_browse_mod_installed(card));
                    let install_disabled_reason =
                        gamebanana::install_block_reason(&detail.profile).unwrap_or_default();
                    let install_blocked = !install_disabled_reason.is_empty();
                    let selected_game_ready = self.selected_game_is_installed_or_configured();
                    ui.add_space(-4.0);
                    ui.horizontal(|ui| {
                        let install_response = ui.add_enabled(
                            !install_blocked && selected_game_ready,
                            egui::Button::new(icon_text_sized(
                                Icon::PackagePlus,
                                if is_installed { "Installed" } else { "Install" },
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
                                .on_hover_text_at_pointer("Game is not installed or configured.")
                                .on_hover_cursor(egui::CursorIcon::NotAllowed);
                        } else if install_blocked {
                            install_response
                                .clone()
                                .on_hover_text_at_pointer(install_disabled_reason);
                        }
                        let install_response = if selected_game_ready && !install_blocked {
                            install_response.on_hover_cursor(egui::CursorIcon::PointingHand)
                        } else {
                            install_response
                        };
                        if install_response.clicked() {
                            self.queue_install_for_browse_mod(mod_id);
                        }
                        ui.add_space(-2.0);
                        if ui.add(
                                egui::Button::new(icon_text_sized(Icon::Globe, "Open in Browser", 13.0, 13.0))
                                    .corner_radius(egui::CornerRadius::same(6)),
                            ).on_hover_cursor(egui::CursorIcon::PointingHand).clicked()
                        {
                            if let Err(err) = open_external_url(&gamebanana::browser_url(mod_id)) {
                                self.report_error(err, Some("Could not open browser"));
                            }
                        }
                            ui.allocate_ui_with_layout(
                            ui.available_size(),
                            egui::Layout::right_to_left(egui::Align::Center),
                            |ui| {
                                ui.with_layout(egui::Layout::top_down(egui::Align::Max), |ui| {
                                    ui.horizontal(|ui| {
                                        ui.add(egui::Label::new(RichText::new(detail.profile.download_count.to_string()).size(11.5).color(Color32::from_gray(178))).selectable(false)).on_hover_cursor(egui::CursorIcon::Default);
                                        ui.add_space(-5.0);
                                        ui.add(egui::Label::new(icon_rich(Icon::Download, 12.0, Color32::from_rgb(131, 214, 247))).selectable(false)).on_hover_cursor(egui::CursorIcon::Default);
                                        ui.add_space(8.0);
                                        ui.add(egui::Label::new(RichText::new(relative_time_label(updated_at, false)).size(11.5).color(Color32::from_gray(145))).selectable(false)).on_hover_cursor(egui::CursorIcon::Default);
                                        ui.add_space(-5.0);
                                        ui.add(egui::Label::new(icon_rich(Icon::CalendarClock, 12.0, Color32::from_rgb(112, 164, 118))).selectable(false)).on_hover_cursor(egui::CursorIcon::Default);
                                        ui.add_space(8.0);
                                        ui.add(egui::Label::new(RichText::new(like_count.to_string()).size(11.5).color(Color32::from_gray(178))).selectable(false)).on_hover_cursor(egui::CursorIcon::Default);
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

                    ScrollArea::vertical().id_salt("browse_detail_scroll").show(ui, |ui| {
                        if let Some(preview) = &detail.profile.preview_media {
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
                                    let mut rects = Vec::new();
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
                                                    self.mod_thumbnail_placeholder.as_ref(),
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

                        let youtube_url = detail.profile.embedded_media.iter()
                            .find(|url| url.contains("youtube.com") || url.contains("youtu.be"));

                        if let Some(url) = youtube_url {
                            self.render_youtube_card(ui, url);
                            ui.add_space(8.0);
                        }

                        match &detail.updates {
                            BrowseUpdatesState::Loaded(entries) if !entries.is_empty() => {
                                egui::CollapsingHeader::new(
                                    bold("Updates").size(13.0).underline().color(Color32::from_gray(220)),
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
                                                RichText::new(format!("/ {}", mod_age_label(entry.updated_at)))
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
                                    RichText::new("Updates")
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
                                            RichText::new("This mod is private.")
                                                .strong()
                                                .color(Color32::from_rgb(220, 120, 120))
                                        );
                                    });
                                    static_label(ui, "Automatic installation is disabled. You may be able to view or download it directly on GameBanana if you are authorized.");
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
                                                RichText::new("This mod has been withheld")
                                                    .strong()
                                                    .color(Color32::from_rgb(220, 120, 120)),
                                            );
                                            if let Some(withholder) = withhold_notice
                                                .as_ref()
                                                .and_then(|n| n.withholder.as_ref())
                                            {
                                                static_label(
                                                    ui,
                                                    RichText::new("Withheld by")
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
                                                            Some("Could not open browser"),
                                                        );
                                                    }
                                                }
                                            }
                                        });
                                    });
                                    ui.add(
                                        egui::Label::new(
                                            RichText::new(
                                                "Automatic installation is disabled until the withhold is resolved.",
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
                                                    .unwrap_or("Rule violation");
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
                                            RichText::new("This mod no longer exists.")
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
                                                                RichText::new("This mod has been deleted by")
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
                                                                    self.report_error(err, Some("Could not open browser"));
                                                                }
                                                            }
                                                        } else {
                                                            static_label(
                                                                ui,
                                                                RichText::new("This mod has been deleted")
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

                        if !install_blocked && !detail.profile.files.is_empty() {
                            ui.add_space(12.0);
                            render_file_section_label(ui, "Files", detail.profile.files.len());
                            for file in &detail.profile.files {
                                self.render_browse_file_row(ui, &browse_game_id, mod_id, file);
                            }
                        }
                        if !install_blocked && !detail.profile.archived_files.is_empty() {
                            ui.add_space(12.0);
                            render_file_section_label(ui, "Archived Files", detail.profile.archived_files.len());
                            for file in &detail.profile.archived_files {
                                self.render_browse_file_row(ui, &browse_game_id, mod_id, file);
                            }
                        }
                    });
                } else {
                    ui.add_space(18.0);
                    ui.centered_and_justified(|ui| {
                    static_label(
                        ui,
                        RichText::new("Loading mod details…")
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
        egui::Frame::group(ui.style())
            .inner_margin(egui::Margin::same(10))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.vertical(|ui| {
                        static_label(ui, bold(&file.file_name));
                        ui.add_space(-4.0);
                        static_label(
                            ui,
                            RichText::new(format!(
                                "{} • {} • {} downloads",
                                format_file_size(file.file_size),
                                format_exact_local_timestamp(file.date_added),
                                file.download_count
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
                            egui::Button::new("Install"),
                        );
                        if !selected_game_ready {
                            install_response
                                .clone()
                                .on_hover_text("Game is not installed or configured.")
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
                                self
                                    .browse_state
                                    .details
                                    .get(&mod_id)
                                    .map(|detail| detail.unsafe_content)
                                    .unwrap_or(false),
                                None,
                                None,
                                None,
                            );
                        }
                    });
                });
            });
    }

    fn render_browse_file_prompt(&mut self, ctx: &egui::Context, constrain_rect: egui::Rect) {
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
            .or_else(|| self.browse_state.details.get(&prompt.mod_id).map(|d| d.profile.name.clone()))
            .unwrap_or_else(|| format!("Mod {}", prompt.mod_id));

        let mut open = true;
        let mut should_cancel = false;
        let mut should_confirm = false;
        egui::Window::new("Choose Files")
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
                    static_label(ui, icon_rich(Icon::Info, 96.0, Color32::from_rgb(148, 192, 232)));
                    ui.vertical(|ui| {
                        static_label(ui, bold(&mod_name).underline().size(16.0));
                        ui.add_space(4.0);
                        static_label(
                            ui,
                            RichText::new(concat!("This mod has multiple files available.\n",
                                                "Select file(s) to download and install:"
                            ))
                                .size(14.0),
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
                            ScrollArea::vertical()
                                .max_height(300.0)
                                .show(ui, |ui| {
                                let mut style = (**ui.style()).clone();
                                style.visuals.widgets.active.bg_fill = egui::Color32::from_rgba_unmultiplied(128, 128, 128, 128);
                                style.visuals.widgets.inactive.bg_fill = egui::Color32::from_rgba_unmultiplied(128, 128, 128, 128);
                                style.visuals.widgets.hovered.bg_fill = egui::Color32::from_rgba_unmultiplied(128, 128, 128, 128);
                                style.visuals.widgets.active.corner_radius = egui::CornerRadius::same(2);
                                style.visuals.widgets.inactive.corner_radius = egui::CornerRadius::same(2);
                                style.visuals.widgets.hovered.corner_radius = egui::CornerRadius::same(2);
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
                                                    static_label(ui, bold(&file_entry.file.file_name));
                                                    ui.add_space(-4.0);
                                                    static_label(
                                                        ui,
                                                        RichText::new(format!(
                                                            "{} • {} • {} downloads",
                                                            format_file_size(file_entry.file.file_size),
                                                            format_exact_local_timestamp(file_entry.file.date_added),
                                                            file_entry.file.download_count
                                                        ))
                                                        .size(11.5)
                                                        .color(Color32::from_gray(155)),
                                                    );
                                                    if let Some(description) = &file_entry.file.description {
                                                        if !description.trim().is_empty() {
                                                            static_label(ui, RichText::new(gamebanana::sanitize_inline(description)).size(12.0).color(Color32::from_gray(186)));
                                                        }
                                                    }
                                                });
                                            });
                                        }).response;
                                    if row_response.interact(Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                                        file_entry.selected = !file_entry.selected;
                                    }
                                }
                            });
                        });
                        ui.horizontal(|ui| {
                            let install_response = ui.add_enabled(
                                prompt_game_ready,
                                egui::Button::new("Install")
                                    .fill(Color32::from_rgb(180, 78, 35))
                                );
                            if !prompt_game_ready {
                                install_response
                                    .clone()
                                    .on_hover_text("Game is not installed or configured.")
                                    .on_hover_cursor(egui::CursorIcon::NotAllowed);
                            }
                            if install_response.clicked() {
                                should_confirm = true;
                            }
                            if ui.button("Cancel").clicked() {
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
        
        let mut images: Vec<(String, Option<String>)> = Vec::new();
        if self.current_view == ViewMode::Browse {
            if let Some(mod_id) = self.browse_state.selected_mod_id {
                if let Some(detail) = self.browse_state.details.get(&mod_id) {
                    if let Some(preview) = &detail.profile.preview_media {
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

        enum NavAction { Close, Next, Prev }
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
                ui.painter().rect_filled(rect, 0.0, Color32::from_rgba_premultiplied(0, 0, 0, 240));

                if bg_response.clicked() {
                    action = Some(NavAction::Close);
                }

                ui.input(|i| {
                    if i.key_pressed(egui::Key::Escape) || i.key_pressed(egui::Key::Space) || i.key_pressed(egui::Key::Enter) {
                        action = Some(NavAction::Close);
                    }
                    if i.key_pressed(egui::Key::A) || i.key_pressed(egui::Key::W) || i.key_pressed(egui::Key::ArrowLeft) || i.key_pressed(egui::Key::ArrowUp) {
                        action = Some(NavAction::Prev);
                    }
                    if i.key_pressed(egui::Key::S) || i.key_pressed(egui::Key::D) || i.key_pressed(egui::Key::ArrowRight) || i.key_pressed(egui::Key::ArrowDown) {
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
                        Color32::WHITE
                    );

                    if let Some(caption) = current_index.and_then(|i| images[i].1.as_ref()) {
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

                            ui.painter().rect_filled(caption_rect, 0.0, Color32::from_black_alpha(64));
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
                        let left_rect = egui::Rect::from_min_size(egui::pos2(rect.min.x + 32.0, center_y), button_size);
                        let resp = ui.interact(left_rect, ui.id().with("nav_l"), Sense::click())
                            .on_hover_cursor(egui::CursorIcon::PointingHand);
                        let alpha = if resp.hovered() { 240 } else { 102 };
                        ui.painter().rect_filled(left_rect, 12.0, Color32::from_black_alpha(alpha));
                        ui.painter().text(left_rect.center(), egui::Align2::CENTER_CENTER, icon_char(Icon::ChevronLeft), egui::FontId::new(48.0, FontFamily::Name(LUCIDE_FAMILY.into())), Color32::WHITE);
                        if resp.clicked() { action = Some(NavAction::Prev); }
                    }
                    
                    if current_index.is_some_and(|i| i < total_images - 1) {
                        let right_rect = egui::Rect::from_min_size(egui::pos2(rect.max.x - 32.0 - button_size.x, center_y), button_size);
                        let resp = ui.interact(right_rect, ui.id().with("nav_r"), Sense::click())
                            .on_hover_cursor(egui::CursorIcon::PointingHand);
                        let alpha = if resp.hovered() { 240 } else { 102 };
                        ui.painter().rect_filled(right_rect, 12.0, Color32::from_black_alpha(alpha));
                        ui.painter().text(right_rect.center(), egui::Align2::CENTER_CENTER, icon_char(Icon::ChevronRight), egui::FontId::new(48.0, FontFamily::Name(LUCIDE_FAMILY.into())), Color32::WHITE);
                        if resp.clicked() { action = Some(NavAction::Next); }
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
                        self.queue_overlay_full_texture(&next_key);
                        if i + 2 < images.len() {
                            self.queue_overlay_full_texture(&images[i + 2].0);
                        }
                        self.browse_state.screenshot_overlay = Some(BrowseOverlayImage { texture_key: next_key });
                    }
                }
            }
            Some(NavAction::Prev) => {
                if let Some(i) = current_index {
                    if i > 0 {
                        let prev_key = images[i - 1].0.clone();
                        self.queue_overlay_full_texture(&prev_key);
                        if i > 1 {
                            self.queue_overlay_full_texture(&images[i - 2].0);
                        }
                        self.browse_state.screenshot_overlay = Some(BrowseOverlayImage { texture_key: prev_key });
                    }
                }
            }
            None => {}
        }
    }

}
